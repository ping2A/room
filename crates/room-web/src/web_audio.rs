//! Web Audio output for the software mixer + music.
//!
//! Prefers an AudioWorklet playback node (PCM fed from the main thread).
//! Falls back to deprecated `ScriptProcessorNode` when worklets are unavailable.

use room_audio::{ENGINE_SAMPLE_RATE, Mixer};
use room_music::{MidiDevice, MusicPlayer, SoftFont};
use std::cell::RefCell;
use std::rc::{Rc, Weak};
use wasm_bindgen::JsCast;
use wasm_bindgen::JsValue;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;
use web_sys::{
    AudioContext, AudioContextOptions, AudioContextState, AudioProcessingEvent, AudioWorkletNode,
    Blob, BlobPropertyBag, GainNode, MessageEvent, ScriptProcessorNode, Url,
};

const WORKLET_JS: &str = include_str!("../mixer-worklet.js");
/// Max frames rendered per worklet message (≈23 ms @ 44.1 kHz).
const FEED_FRAMES: usize = 1024;
/// Initial audio queued ahead of the device (seconds). Feeding is driven by
/// the worklet's ~10 ms reports, so this only has to cover message jitter.
const FEED_TARGET_START_SEC: f64 = 0.04;
/// Adaptive target bounds: grow on underruns, decay back while stable.
const FEED_TARGET_MIN_SEC: f64 = 0.025;
const FEED_TARGET_MAX_SEC: f64 = 0.16;
/// Grow step per observed underrun burst.
const FEED_TARGET_GROW_SEC: f64 = 0.012;
/// Shrink step after [`FEED_STABLE_MS`] without underruns.
const FEED_TARGET_SHRINK_SEC: f64 = 0.004;
const FEED_STABLE_MS: f64 = 4_000.0;
/// Never render more than this in one call (tab returning from background).
const FEED_MAX_SEC: f64 = 0.25;

thread_local! {
    /// GM SoundFont parsed before audio started (applied in [`WebAudio::start`]).
    static PENDING_SOFTFONT: RefCell<Option<SoftFont>> = const { RefCell::new(None) };
    /// Music player of the live context, so a late SoundFont can be swapped in.
    static ACTIVE_MUSIC: RefCell<Weak<RefCell<MusicPlayer>>> = const { RefCell::new(Weak::new()) };
    /// Global mute (`roomSetMuted`), applied to every graph including later ones.
    static MUTED: RefCell<bool> = const { RefCell::new(false) };
    static MASTER_GAINS: RefCell<Vec<GainNode>> = const { RefCell::new(Vec::new()) };
}

/// Mute / unmute all game audio (SFX + music).
pub fn set_muted(muted: bool) {
    MUTED.with(|m| *m.borrow_mut() = muted);
    MASTER_GAINS.with(|g| {
        for node in g.borrow().iter() {
            node.gain().set_value(if muted { 0.0 } else { 1.0 });
        }
    });
}

/// Current [`set_muted`] state.
pub fn muted() -> bool {
    MUTED.with(|m| *m.borrow())
}

fn track_master_gain(gain: &GainNode) {
    MASTER_GAINS.with(|g| {
        let mut g = g.borrow_mut();
        // Play tests rebuild the graph; old contexts are dead weight.
        if g.len() >= 4 {
            g.remove(0);
        }
        g.push(gain.clone());
    });
}

fn now_ms() -> f64 {
    web_sys::window()
        .and_then(|w| w.performance())
        .map_or(0.0, |p| p.now())
}

/// `?midi=opl` keeps the GENMIDI FM synth even when a SoundFont is available.
fn force_opl() -> bool {
    crate::browser::room_url_param("midi").is_some_and(|v| v.eq_ignore_ascii_case("opl"))
}

/// Same policy as the desktop `snd_mididevice auto`: SoundFont first, then OPL.
pub fn preferred_midi_device(music: &MusicPlayer) -> MidiDevice {
    if music.has_softfont() && !force_opl() {
        MidiDevice::SoftFont
    } else {
        MidiDevice::Opl
    }
}

/// Install a GM SoundFont (SF2/SF3 bytes) for map music.
pub fn set_soundfont(bytes: &[u8]) -> Result<(), String> {
    let font = SoftFont::load(bytes).map_err(|e| e.to_string())?;
    let live = ACTIVE_MUSIC.with(|m| m.borrow().upgrade());
    match live {
        Some(music) => {
            let mut music = music
                .try_borrow_mut()
                .map_err(|_| "music player busy".to_owned())?;
            music.set_softfont(font);
            let device = preferred_midi_device(&music);
            music.set_mididevice(device);
        }
        None => PENDING_SOFTFONT.with(|p| *p.borrow_mut() = Some(font)),
    }
    Ok(())
}

/// Estimate of PCM queued in the worklet ring + adaptive latency target.
struct FeedClock {
    /// Frames buffered as of `at_ms`.
    buffered: f64,
    /// Timestamp of `buffered`.
    at_ms: f64,
    /// Seconds kept queued ahead of the device.
    target_sec: f64,
    /// Worklet underrun count at the last report.
    underruns: u32,
    /// Last time the target grew / shrank.
    adjusted_ms: f64,
}

impl Default for FeedClock {
    fn default() -> Self {
        Self {
            buffered: 0.0,
            at_ms: 0.0,
            target_sec: FEED_TARGET_START_SEC,
            underruns: 0,
            adjusted_ms: 0.0,
        }
    }
}

impl FeedClock {
    /// Fold one worklet report into the estimate / target.
    fn report(&mut self, buffered: f64, underruns: Option<u32>, now: f64) {
        self.buffered = buffered.max(0.0);
        self.at_ms = now;
        if let Some(u) = underruns {
            if u > self.underruns {
                self.target_sec = (self.target_sec + FEED_TARGET_GROW_SEC).min(FEED_TARGET_MAX_SEC);
                self.adjusted_ms = now;
            }
            self.underruns = u;
        }
        if now - self.adjusted_ms > FEED_STABLE_MS {
            self.target_sec = (self.target_sec - FEED_TARGET_SHRINK_SEC).max(FEED_TARGET_MIN_SEC);
            self.adjusted_ms = now;
        }
    }
}

/// Snapshot for the debug overlay (`roomAudioStats`).
pub struct AudioStats {
    /// Queued-ahead target (ms).
    pub target_ms: f64,
    /// Estimated queued audio (ms).
    pub buffered_ms: f64,
    /// Device starvations since start.
    pub underruns: u32,
    /// `"worklet"`, `"script"` or `"pending"`.
    pub backend: &'static str,
    /// Voices currently mixing.
    pub voices: usize,
}
/// ScriptProcessor buffer size (power of two).
const SP_FRAMES: u32 = 2048;

enum Backend {
    /// Waiting for AudioWorklet `addModule` (silent until ready / SP fallback).
    Pending,
    ScriptProcessor {
        #[allow(dead_code)]
        node: ScriptProcessorNode,
    },
    Worklet {
        _node: AudioWorkletNode,
        port: web_sys::MessagePort,
    },
}

/// Browser audio graph: `AudioContext` + mixer/music.
pub struct WebAudio {
    /// Context (kept for `resume`).
    ctx: AudioContext,
    /// Master gain (0..=1.5 via [`Self::set_master_gain`]).
    #[allow(dead_code)]
    gain: GainNode,
    /// Shared SFX mixer.
    pub mixer: Rc<RefCell<Mixer>>,
    /// Shared music player (OPL / SoftFont).
    pub music: Rc<RefCell<MusicPlayer>>,
    /// Main-thread renderer for the worklet ring.
    feeder: Feeder,
}

/// Renders mixer + music into the worklet ring; shared by rAF and worklet reports.
#[derive(Clone)]
struct Feeder {
    ctx: AudioContext,
    /// Active output path.
    backend: Rc<RefCell<Backend>>,
    /// Worklet fill estimate (updated from its `buffered` reports).
    clock: Rc<RefCell<FeedClock>>,
    /// Scratch for main-thread / ScriptProcessor render (avoids per-callback alloc).
    scratch: Rc<RefCell<Scratch>>,
    mixer: Rc<RefCell<Mixer>>,
    music: Rc<RefCell<MusicPlayer>>,
}

struct Scratch {
    interleaved: Vec<f32>,
    music: Vec<f32>,
    left: Vec<f32>,
    right: Vec<f32>,
}

impl Scratch {
    fn with_frames(frames: usize) -> Self {
        Self {
            interleaved: vec![0.0; frames * 2],
            music: vec![0.0; frames * 2],
            left: vec![0.0; frames],
            right: vec![0.0; frames],
        }
    }

    fn ensure(&mut self, frames: usize) {
        let need = frames * 2;
        if self.interleaved.len() < need {
            self.interleaved.resize(need, 0.0);
            self.music.resize(need, 0.0);
        }
        if self.left.len() < frames {
            self.left.resize(frames, 0.0);
            self.right.resize(frames, 0.0);
        }
    }
}

impl WebAudio {
    /// Create an `AudioContext` and start the best available output path.
    pub fn start() -> Result<Self, JsValue> {
        let opts = AudioContextOptions::new();
        opts.set_sample_rate(ENGINE_SAMPLE_RATE as f32);
        let ctx = AudioContext::new_with_context_options(&opts).or_else(|_| AudioContext::new())?;
        let rate = ctx.sample_rate().round().max(1.0) as u32;
        let mixer = Rc::new(RefCell::new(Mixer::with_sample_rate(rate)));
        let mut music_player = MusicPlayer::new();
        music_player.set_sample_rate(rate);
        if let Some(font) = PENDING_SOFTFONT.with(|p| p.borrow_mut().take()) {
            music_player.set_softfont(font);
        }
        let music = Rc::new(RefCell::new(music_player));
        ACTIVE_MUSIC.with(|m| *m.borrow_mut() = Rc::downgrade(&music));
        let scratch = Rc::new(RefCell::new(Scratch::with_frames(SP_FRAMES as usize)));
        let gain = ctx.create_gain()?;
        gain.gain().set_value(if muted() { 0.0 } else { 1.0 });
        track_master_gain(&gain);
        let _ = gain.connect_with_audio_node(&ctx.destination());

        // Prefer AudioWorklet first (avoids ScriptProcessor deprecation warning).
        // Fall back to ScriptProcessor only if the worklet cannot start.
        let backend = Rc::new(RefCell::new(Backend::Pending));
        let clock = Rc::new(RefCell::new(FeedClock::default()));
        let feeder = Feeder {
            ctx: ctx.clone(),
            backend: Rc::clone(&backend),
            clock: Rc::clone(&clock),
            scratch: Rc::clone(&scratch),
            mixer: Rc::clone(&mixer),
            music: Rc::clone(&music),
        };
        {
            let ctx_w = ctx.clone();
            let gain_w = gain.clone();
            let backend_w = Rc::clone(&backend);
            let clock_w = Rc::clone(&clock);
            let mixer_w = Rc::clone(&mixer);
            let music_w = Rc::clone(&music);
            let scratch_w = Rc::clone(&scratch);
            let feeder_w = feeder.clone();
            wasm_bindgen_futures::spawn_local(async move {
                match install_worklet(&ctx_w, &gain_w).await {
                    Ok((node, port)) => {
                        let clock_msg = Rc::clone(&clock_w);
                        let onmsg = Closure::wrap(Box::new(move |ev: MessageEvent| {
                            let data = ev.data();
                            let get = |k: &str| {
                                js_sys::Reflect::get(&data, &JsValue::from_str(k))
                                    .ok()
                                    .and_then(|v| v.as_f64())
                            };
                            let Some(frames) = get("buffered") else {
                                return;
                            };
                            let underruns = get("underruns").map(|u| u.max(0.0) as u32);
                            if let Ok(mut c) = clock_msg.try_borrow_mut() {
                                c.report(frames, underruns, now_ms());
                            }
                            // Audio-clock driven refill (rAF alone stalls on slow frames).
                            feeder_w.feed();
                        }) as Box<dyn FnMut(_)>);
                        port.set_onmessage(Some(onmsg.as_ref().unchecked_ref()));
                        onmsg.forget();
                        *backend_w.borrow_mut() = Backend::Worklet { _node: node, port };
                        web_sys::console::log_1(&"room-web: audio worklet playback active".into());
                    }
                    Err(e) => {
                        web_sys::console::warn_1(
                            &format!(
                                "room-web: audio worklet unavailable, using ScriptProcessor ({e:?})"
                            )
                            .into(),
                        );
                        match start_script_processor(&ctx_w, &gain_w, mixer_w, music_w, scratch_w) {
                            Ok(sp) => *backend_w.borrow_mut() = sp,
                            Err(sp_err) => {
                                web_sys::console::error_1(
                                    &format!("room-web: audio output failed ({sp_err:?})").into(),
                                );
                            }
                        }
                    }
                }
            });
        }

        let _ = ctx.resume();
        web_sys::console::log_1(&format!("room-web: audio {rate} Hz").into());
        Ok(Self {
            ctx,
            gain,
            mixer,
            music,
            feeder,
        })
    }

    /// Latency / underrun numbers for the debug overlay.
    #[must_use]
    pub fn stats(&self) -> AudioStats {
        let rate = f64::from(self.sample_rate()).max(1.0);
        let (target_ms, buffered_ms, underruns) =
            self.feeder.clock.try_borrow().map_or((0.0, 0.0, 0), |c| {
                (
                    c.target_sec * 1000.0,
                    c.buffered * 1000.0 / rate,
                    c.underruns,
                )
            });
        let backend = match self.feeder.backend.try_borrow().as_deref() {
            Ok(Backend::Worklet { .. }) => "worklet",
            Ok(Backend::ScriptProcessor { .. }) => "script",
            _ => "pending",
        };
        AudioStats {
            target_ms,
            buffered_ms,
            underruns,
            backend,
            voices: self.mixer.try_borrow().map_or(0, |m| m.voices.len()),
        }
    }

    /// Resume after a user gesture / tab focus (autoplay + iOS quirks).
    pub fn resume(&self) {
        if self.ctx.state() == AudioContextState::Running {
            return;
        }
        let _ = self.ctx.resume();
    }

    /// Safari sometimes needs suspend→resume after returning from background.
    pub fn kick(&self) {
        if self.ctx.state() == AudioContextState::Running {
            return;
        }
        let ctx = self.ctx.clone();
        wasm_bindgen_futures::spawn_local(async move {
            if let Ok(p) = ctx.suspend() {
                let _ = JsFuture::from(p).await;
            }
            if let Ok(p) = ctx.resume() {
                let _ = JsFuture::from(p).await;
            }
        });
    }

    /// True when the context is not running (silent until resumed).
    #[must_use]
    pub fn needs_resume(&self) -> bool {
        self.ctx.state() != AudioContextState::Running
    }

    /// Device / mixer sample rate.
    #[must_use]
    pub fn sample_rate(&self) -> u32 {
        self.ctx.sample_rate().round().max(1.0) as u32
    }

    /// Master output gain (clamped).
    #[allow(dead_code)]
    pub fn set_master_gain(&self, gain: f32) {
        self.gain.gain().set_value(gain.clamp(0.0, 1.5));
    }

    /// Render mixer+music into the worklet ring (no-op for ScriptProcessor).
    pub fn feed_output(&self) {
        self.feeder.feed();
    }
}

impl Feeder {
    /// Top the worklet ring up to the adaptive target.
    fn feed(&self) {
        let port = {
            let Ok(backend) = self.backend.try_borrow() else {
                return;
            };
            let Backend::Worklet { port, .. } = &*backend else {
                return;
            };
            port.clone()
        };
        if self.ctx.state() != AudioContextState::Running {
            return;
        }
        // Render exactly what the device consumed since the last report so
        // music tempo and SFX length match the desktop callback path.
        let rate = self.ctx.sample_rate().round().max(1.0) as f64;
        let now = now_ms();
        let mut todo = {
            let Ok(mut c) = self.clock.try_borrow_mut() else {
                return;
            };
            let played = (now - c.at_ms).max(0.0) * rate / 1000.0;
            c.buffered = (c.buffered - played).max(0.0);
            c.at_ms = now;
            let want = (c.target_sec * rate - c.buffered).min(FEED_MAX_SEC * rate);
            if want < 128.0 {
                return;
            }
            let frames = (want / 128.0).ceil() * 128.0;
            c.buffered += frames;
            frames as usize
        };
        let Ok(mut scratch) = self.scratch.try_borrow_mut() else {
            return;
        };
        while todo > 0 {
            let frames = todo.min(FEED_FRAMES);
            todo -= frames;
            scratch.ensure(frames);
            let Scratch {
                interleaved,
                music: music_buf,
                ..
            } = &mut *scratch;
            let interleaved = &mut interleaved[..frames * 2];
            let music_buf = &mut music_buf[..frames * 2];
            interleaved.fill(0.0);
            music_buf.fill(0.0);
            if let Ok(mut m) = self.music.try_borrow_mut() {
                m.render_interleaved(music_buf);
            }
            if let Ok(mut mix) = self.mixer.try_borrow_mut() {
                mix.render_with_music(interleaved, music_buf);
            }
            let pcm = js_sys::Float32Array::new_with_length((frames * 2) as u32);
            pcm.copy_from(interleaved);
            let msg = js_sys::Object::new();
            let _ = js_sys::Reflect::set(&msg, &JsValue::from_str("pcm"), &pcm);
            let _ = port.post_message(&msg);
        }
    }
}

fn start_script_processor(
    ctx: &AudioContext,
    gain: &GainNode,
    mixer: Rc<RefCell<Mixer>>,
    music: Rc<RefCell<MusicPlayer>>,
    scratch: Rc<RefCell<Scratch>>,
) -> Result<Backend, JsValue> {
    let node = ctx
        .create_script_processor_with_buffer_size_and_number_of_input_channels_and_number_of_output_channels(
            SP_FRAMES, 0, 2,
        )?;
    let cb = Closure::wrap(Box::new(move |ev: AudioProcessingEvent| {
        let Ok(out) = ev.output_buffer() else {
            return;
        };
        let frames = out.length() as usize;
        if frames == 0 {
            return;
        }
        let Ok(mut scratch) = scratch.try_borrow_mut() else {
            return;
        };
        scratch.ensure(frames);
        let Scratch {
            interleaved,
            music: music_buf,
            left,
            right,
        } = &mut *scratch;
        let interleaved = &mut interleaved[..frames * 2];
        let music_buf = &mut music_buf[..frames * 2];
        let left = &mut left[..frames];
        let right = &mut right[..frames];
        interleaved.fill(0.0);
        music_buf.fill(0.0);
        if let Ok(mut m) = music.try_borrow_mut() {
            m.render_interleaved(music_buf);
        }
        if let Ok(mut mix) = mixer.try_borrow_mut() {
            mix.render_with_music(interleaved, music_buf);
        }
        for i in 0..frames {
            left[i] = interleaved[i * 2];
            right[i] = interleaved[i * 2 + 1];
        }
        let _ = out.copy_to_channel(left, 0);
        let _ = out.copy_to_channel(right, 1);
    }) as Box<dyn FnMut(_)>);
    node.set_onaudioprocess(Some(cb.as_ref().unchecked_ref()));
    let _ = node.connect_with_audio_node(gain);
    cb.forget();
    Ok(Backend::ScriptProcessor { node })
}

async fn install_worklet(
    ctx: &AudioContext,
    gain: &GainNode,
) -> Result<(AudioWorkletNode, web_sys::MessagePort), JsValue> {
    let worklet = ctx.audio_worklet()?;
    // Prefer same-origin file (CSP-safe); fall back to blob: for ad-hoc hosts.
    // Content hash in the URL so a rebuilt worklet is never served from cache.
    let hash = WORKLET_JS.bytes().fold(0x811c_9dc5_u32, |h, b| {
        (h ^ u32::from(b)).wrapping_mul(0x0100_0193)
    });
    let url = format!("mixer-worklet.js?v={hash:08x}");
    if let Err(e) = add_worklet_module(&worklet, &url).await {
        web_sys::console::warn_1(
            &format!("room-web: mixer-worklet.js addModule failed ({e:?}), trying blob").into(),
        );
        let parts = js_sys::Array::new();
        parts.push(&JsValue::from_str(WORKLET_JS));
        let props = BlobPropertyBag::new();
        props.set_type("application/javascript");
        let blob = Blob::new_with_str_sequence_and_options(&parts, &props)?;
        let url = Url::create_object_url_with_blob(&blob)?;
        let result = add_worklet_module(&worklet, &url).await;
        let _ = Url::revoke_object_url(&url);
        result?;
    }

    let opts = web_sys::AudioWorkletNodeOptions::new();
    opts.set_number_of_inputs(0);
    opts.set_number_of_outputs(1);
    opts.set_output_channel_count(&js_sys::Array::of1(&JsValue::from(2)));
    let processor_opts = js_sys::Object::new();
    let _ = js_sys::Reflect::set(
        &processor_opts,
        &JsValue::from_str("bufferSec"),
        &JsValue::from(0.45),
    );
    opts.set_processor_options(Some(&processor_opts));
    let node = AudioWorkletNode::new_with_options(ctx, "room-mixer-playback", &opts)?;
    let port = node.port()?;
    let _ = node.connect_with_audio_node(gain);
    Ok((node, port))
}

async fn add_worklet_module(worklet: &web_sys::AudioWorklet, url: &str) -> Result<(), JsValue> {
    let add = worklet.add_module(url)?;
    JsFuture::from(add).await?;
    Ok(())
}
