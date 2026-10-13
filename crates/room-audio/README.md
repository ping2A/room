# room-audio

Mixer, SFX, positional audio, reverb, and output backends.

## Public API (M5 bootstrap)

- `Mixer` / `SampleBank` — soft stereo mix with Doom-style distance attenuation
- `mix_music` / `render_with_music` — blend SoftFont (or other) interleaved music PCM via `music_gain`
- `SoundEvent` — start/stop commands from the game shell
