//! Native cpal output for the software mixer (M5 bootstrap).
//!
//! Mixer + music live behind a mutex shared with the device callback.
//! A lock-free command ring (`rtrb`) is the longer-term path per docs/14-audio.md.

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, Stream, StreamConfig};
use room_audio::{ENGINE_SAMPLE_RATE, Mixer};
use room_music::MusicPlayer;
use std::sync::{Arc, Mutex};

/// Shared audio graph mutated on the main thread and rendered on the device thread.
pub struct AudioShared {
    /// SFX mixer.
    pub mixer: Mixer,
    /// Map / UI music.
    pub music: MusicPlayer,
}

/// Handle keeping the output stream alive.
pub struct AudioOut {
    /// Shared mixer/music state.
    pub shared: Arc<Mutex<AudioShared>>,
    /// Device stream sample rate (Hz).
    pub sample_rate: u32,
    /// Device stream (dropped to stop).
    _stream: Stream,
}

impl AudioOut {
    /// Open the default output device and start streaming stereo f32.
    pub fn start() -> Result<Self, String> {
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or_else(|| "no default audio output device".to_owned())?;
        let supported = device.default_output_config().map_err(|e| e.to_string())?;
        let sample_format = supported.sample_format();
        let mut stream_config: StreamConfig = supported.into();
        // Prefer stereo for the mixer path.
        if stream_config.channels == 0 {
            stream_config.channels = 2;
        }
        // Prefer 44.1 kHz when the device accepts it (matches SoftFont / engine rate).
        if let Ok(configs) = device.supported_output_configs() {
            for range in configs {
                if range.channels() == stream_config.channels
                    && range.min_sample_rate().0 <= ENGINE_SAMPLE_RATE
                    && range.max_sample_rate().0 >= ENGINE_SAMPLE_RATE
                {
                    stream_config.sample_rate = cpal::SampleRate(ENGINE_SAMPLE_RATE);
                    break;
                }
            }
        }
        let sample_rate = stream_config.sample_rate.0.max(1);

        let shared = Arc::new(Mutex::new(AudioShared {
            mixer: Mixer::with_sample_rate(sample_rate),
            music: {
                let mut m = MusicPlayer::new();
                m.set_sample_rate(sample_rate);
                m
            },
        }));
        let shared_cb = Arc::clone(&shared);
        let channels = stream_config.channels as usize;

        let stream = match sample_format {
            SampleFormat::F32 => build_stream::<f32>(&device, &stream_config, shared_cb, channels)?,
            SampleFormat::I16 => build_stream::<i16>(&device, &stream_config, shared_cb, channels)?,
            SampleFormat::U16 => build_stream::<u16>(&device, &stream_config, shared_cb, channels)?,
            other => return Err(format!("unsupported sample format {other:?}")),
        };
        stream.play().map_err(|e| e.to_string())?;
        eprintln!("room: audio output {sample_rate} Hz");
        Ok(Self {
            shared,
            sample_rate,
            _stream: stream,
        })
    }
}

fn build_stream<T>(
    device: &cpal::Device,
    config: &StreamConfig,
    shared: Arc<Mutex<AudioShared>>,
    channels: usize,
) -> Result<Stream, String>
where
    T: cpal::SizedSample + cpal::FromSample<f32>,
{
    let err_fn = |e| eprintln!("room audio: {e}");
    device
        .build_output_stream(
            config,
            move |data: &mut [T], _| {
                let frames = data.len() / channels.max(1);
                // Thread-local scratch avoids per-callback heap traffic.
                thread_local! {
                    static BUS: std::cell::RefCell<(Vec<f32>, Vec<f32>)> =
                        const { std::cell::RefCell::new((Vec::new(), Vec::new())) };
                }
                BUS.with(|cell| {
                    let (interleaved, music_buf) = &mut *cell.borrow_mut();
                    interleaved.resize(frames * 2, 0.0);
                    music_buf.resize(frames * 2, 0.0);
                    if let Ok(mut audio) = shared.lock() {
                        audio.music.render_interleaved(music_buf);
                        audio.mixer.render_with_music(interleaved, music_buf);
                    }
                    for (frame, chunk) in data.chunks_mut(channels).enumerate() {
                        let l = interleaved.get(frame * 2).copied().unwrap_or(0.0);
                        let r = interleaved.get(frame * 2 + 1).copied().unwrap_or(l);
                        for (ch, sample) in chunk.iter_mut().enumerate() {
                            let v = if ch == 0 {
                                l
                            } else if ch == 1 {
                                r
                            } else {
                                (l + r) * 0.5
                            };
                            *sample = T::from_sample(v);
                        }
                    }
                });
            },
            err_fn,
            None,
        )
        .map_err(|e| e.to_string())
}
