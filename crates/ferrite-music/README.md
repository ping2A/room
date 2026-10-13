# ferrite-music

MUS/MIDI sequencer, SoundFont, OPL3, trackers, and streams.

## Public API (M5 bootstrap)

- `MusicPlayer` — load MUS→MIDI via `ferrite-formats`, play/pause/stop, volume; `MidiDevice::{SoftFont,Opl}`; SoftFont or OPL PCM via `render_interleaved`
- `SoftFont` / `SoftFontSequencer` — SF2/SF3 via `rustysynth` (offline `render_midi`, streaming `render` / `render_interleaved`)
- `Opl3Synth` / `OplMidiDriver` — Nuked OPL3 + Doom `GENMIDI` parse + DMX-style 9-voice MIDI allocation (OPL3 dual-voice / full frequency curve still incomplete)
