//! Web Audio output for the software mixer + music (ScriptProcessor fallback).

use ferrite_audio::{ENGINE_SAMPLE_RATE, Mixer};
use ferrite_music::MusicPlayer;
use std::cell::RefCell;
use std::rc::Rc;
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;
use web_sys::{AudioContext, AudioContextOptions, AudioProcessingEvent};

/// Browser audio graph: `AudioContext` + mixer/music drained from the rAF / tic loop.
pub struct WebAudio {
    /// Context (kept for `resume`).
    ctx: AudioContext,
    /// Shared SFX mixer (filled on the main thread, rendered in `onaudioprocess`).
    pub mixer: Rc<RefCell<Mixer>>,
    /// Shared music player (OPL / SoftFont), rendered into the same callback.
    pub music: Rc<RefCell<MusicPlayer>>,
    /// Keeps the processor node alive.
    _node: web_sys::ScriptProcessorNode,
}

impl WebAudio {
    /// Create (or resume) an `AudioContext` at the engine sample rate when possible.
    pub fn start() -> Result<Self, JsValue> {
        let opts = AudioContextOptions::new();
        opts.set_sample_rate(ENGINE_SAMPLE_RATE as f32);
        let ctx = AudioContext::new_with_context_options(&opts).or_else(|_| AudioContext::new())?;
        let rate = ctx.sample_rate().round().max(1.0) as u32;
        let mixer = Rc::new(RefCell::new(Mixer::with_sample_rate(rate)));
        let mut music_player = MusicPlayer::new();
        music_player.set_sample_rate(rate);
        let music = Rc::new(RefCell::new(music_player));
        let node = ctx
            .create_script_processor_with_buffer_size_and_number_of_input_channels_and_number_of_output_channels(
                2048, 0, 2,
            )?;
        let mixer_cb = Rc::clone(&mixer);
        let music_cb = Rc::clone(&music);
        let cb = Closure::wrap(Box::new(move |ev: AudioProcessingEvent| {
            let Ok(out) = ev.output_buffer() else {
                return;
            };
            let frames = out.length() as usize;
            if frames == 0 {
                return;
            }
            let mut interleaved = vec![0.0_f32; frames * 2];
            let mut music_buf = vec![0.0_f32; frames * 2];
            if let Ok(mut m) = music_cb.try_borrow_mut() {
                m.render_interleaved(&mut music_buf);
            }
            if let Ok(mut mix) = mixer_cb.try_borrow_mut() {
                mix.render_with_music(&mut interleaved, &music_buf);
            }
            let mut left = vec![0.0_f32; frames];
            let mut right = vec![0.0_f32; frames];
            for i in 0..frames {
                left[i] = interleaved[i * 2];
                right[i] = interleaved[i * 2 + 1];
            }
            let _ = out.copy_to_channel(&left, 0);
            let _ = out.copy_to_channel(&right, 1);
        }) as Box<dyn FnMut(_)>);
        node.set_onaudioprocess(Some(cb.as_ref().unchecked_ref()));
        let _ = node.connect_with_audio_node(&ctx.destination());
        cb.forget();
        let _ = ctx.resume();
        web_sys::console::log_1(&format!("ferrite-web: audio {rate} Hz").into());
        Ok(Self {
            ctx,
            mixer,
            music,
            _node: node,
        })
    }

    /// Resume after a user gesture (required by autoplay policy).
    pub fn resume(&self) {
        let _ = self.ctx.resume();
    }

    /// Device / mixer sample rate.
    #[must_use]
    pub fn sample_rate(&self) -> u32 {
        self.ctx.sample_rate().round().max(1.0) as u32
    }
}
