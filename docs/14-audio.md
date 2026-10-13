# 14 — Audio (`room-audio`, `room-music`)

GZDoom references: `src/common/audio/sound/` (`s_sound.cpp`, `oalsound.cpp`),
`src/sound/s_doomsound.cpp`, `s_advsound.cpp` (SNDINFO), `s_sndseq.cpp` (sound sequences),
`s_environment.cpp` (reverbs), ZMusic library (`ZMusic` repo: MIDI devices, MUS/HMI/XMI readers, OPL, streams).

## 1. Architecture

```
room-sim ──SoundEvent──► room-game ──AudioCommand──► [lock-free ring] ──► mixer thread (room-audio)
                     SimView (positions each frame) ──listener/source updates──►        │
room-music (decoders/synths) ──► music stream ─────────────────────────────────► mixer ──► device (cpal / WebAudio)
```

- `room-audio` is device-agnostic: `trait AudioOutput { fn start(&mut self, render: Box<dyn FnMut(&mut [f32]) + Send>) }` implemented by `room-app` (cpal) and `room-web` (AudioWorklet).
- The mixer runs in the device callback (native) at the device rate (resample all sources with a high-quality polyphase/sinc resampler; `rubato` crate or hand-written windowed-sinc; linear for `snd_resampler 0`).
- Commands are passed through an SPSC ring (`rtrb`); the mixer never allocates or locks. Decoded sample buffers are `Arc<[f32]>` shared immutable data, pre-decoded on the main thread (on demand with a cache; precache level sounds at load).

## 2. Sim interface

```rust
pub enum SoundEvent {
    Start { origin: SoundOrigin, channel: i32, sound: SoundId, volume: f32, attenuation: f32,
            pitch: f32, flags: SoundFlags /* CHANF_* */, start_time: f32 },
    Stop { origin: SoundOrigin, channel: i32 },
    StopAll { origin: SoundOrigin },
    SetVolume { origin, channel, volume }, SetPitch { origin, channel, pitch },
    Relink { from: SoundOrigin, to: SoundOrigin },        // actor destroyed → sound continues at a fixed point (S_RelinkSound)
    MusicChange { name: Name, order: i32, looping: bool, force: bool },
    Announcer / LocalSound { player: u8, sound, … },      // 2D sounds only heard by one player (local player filtering in room-game)
}
pub enum SoundOrigin { None, Actor(ActorId), Sector(SectorId, SectorSoundPart), Polyobj(PolyId), Point(DVec3), Listener }
```

- The sim never knows whether audio exists. Events are drained each tic (`World::drain_sound_events`).
- Per frame, `room-game` resolves origin positions from the interpolated `SimView` and sends `UpdateSources` with positions/velocities and the listener transform (+ underwater flag, current reverb environment).
- **Sound sequences** (SNDSEQ, `DSeqNode`) run inside the sim as thinkers (they are saved by GZDoom) and emit `SoundEvent`s. Port `s_sndseq.cpp` into `room-sim::sndseq`.
- Ambient sounds (`$ambient`, things 14001–14064 / `AmbientSound` actors) are sim actors. Their random intervals use the sim RNG `pr_ambient`, as in GZDoom, so save/restore stays faithful.

## 3. SFX semantics (port `s_sound.cpp`)

- **Resolution**: logical name → `SoundId` via `SoundRegistry`: aliases (`$alias`), random lists (`$random`, choice made at play time with a **non-sim** RNG), player sounds (`*pain100` → class/gender/skin table with fallbacks: `$playersounddup`, `$playercompat`, gender defaults), `$singular`, `$limit` (max instances within distance), `$volume`, `$attenuation`, `$rolloff` (linear/log/custom with `SNDCURVE`), `$pitchshift`/`$pitchshiftrange`.
- **Channels**: `CHAN_AUTO (0)`, `CHAN_WEAPON (1)`, `CHAN_VOICE (2)`, `CHAN_ITEM (3)`, `CHAN_BODY (4)`, `5..7`, plus any integer channel (GZDoom allows arbitrary channels with `CHANF_OVERLAP`); a new sound on an occupied channel of the same origin replaces it unless `CHANF_OVERLAP`. Flags include `CHANF_LISTENERZ, MAYBE_LOCAL, UI, NOPAUSE, LOOP, NOSTOP, OVERLAP, LOCAL, TRANSIENT`; port the full `EChanFlag` list from GZDoom with identical bit values (modules and DECORATE pass them numerically).
- **Attenuation**: Doom model (`S_CLOSE_DIST` = 160 → full volume, `S_CLIPPING_DIST` = 1200 → silent, linear between, scaled by `attenuation`; `ATTN_NONE (0)`, `ATTN_NORM (1)`, `ATTN_IDLE (1.001)`, `ATTN_STATIC (3)`); rolloff override per sound. Positions relative to listener use portal-aware offsets (`Vec3To` across linked portals; GZDoom picks the closest via portal groups — port `CalcPosVel`/`GetSoundPosition`).
- **Panning**: stereo panning by angle (GZDoom: `snd_panning` behaviour), optional HRTF (C: `snd_hrtf`) — implement equal-power panning + optional simple HRTF later.
- **Pitch**: random pitch variation when `snd_pitched` (Doom-style random shifts for non-singular sounds), `A_SoundPitch`, time-freeze powerup pitch drop.
- **Voice management**: `snd_channels` (default 128); priority by distance & recency; virtualize inaudible sounds (keep position/time so looping sounds resume when audible).
- **Sector sounds**: `SectorSoundPart` (floor, ceiling, interior) with positions at the nearest point of the sector to the listener (GZDoom `CalcSectorSoundOrg`).
- **Underwater filter** (`snd_waterlp`): one-pole low-pass when listener is in a swimmable 3D floor / Boom deep-water fake sector.
- **Reverb**: environments from REVERBS (EAX 2.0-like parameter sets: Generic, PaddedCell, Room, Bathroom, …, user-defined). Implement an FDN reverb (8-line feedback delay network with HF damping), parameter mapping from EAX fields (DecayTime, DecayHFRatio, Reflections, ReflectionsDelay, Reverb gain, ReverbDelay, Diffusion, Density, RoomHF…). Zones: sectors get environments from `SoundEnvironment` things (zone flood fill by sound-blocking lines `ML_ZONEBOUNDARY`, as GZDoom `P_InitZones`); the listener's zone selects the active environment with crossfade. `snd_reverb` toggles.
- **Pause**: sounds without `CHANF_NOPAUSE` pause with the game; menu sounds are UI sounds (`CHANF_UI`).
- **Save/restore**: GZDoom saves playing looping sounds (`S_SerializeSounds`). Room saves: active looping/sequence sounds with origin and offset, current music (name, order, position). Non-looping one-shots are not saved.

## 4. Music (`room-music`)

### 4.1 Selection logic

- Map music from MAPINFO `music` (with optional `:order` for modules / track), `$map`/`$musicalias` (SNDINFO), `MUSINFO` changers (`MusicChanger` things, `changemus` console command, ACS `SetMusic`/`LocalSetMusic`), intermission/finale/title music from MAPINFO/GAMEINFO, `D_`-prefixed lump name conventions (Doom), `MUS_` (Heretic), Hexen `$map` numbers.
- `snd_musicvolume`, per-song `$musicvolume`, `relative_volume` metadata, looping (`loop` flag; non-looping for finales/intermissions as GZDoom).
- Device selection CVar `snd_mididevice` mapping: `-5` = FluidSynth-equivalent (SoundFont), `-2` = OPL, `-3` = Timidity, `-4` = GUS, `-6`/`-7` = ADL/OPN MIDI, etc. Room supports `sf2` (default), `opl`; others map to `sf2` with a console note.

### 4.2 MIDI pipeline

```
MUS ─► MusToMidi events ┐
SMF (0/1/2) ────────────┼─► MidiSequencer (tempo map, loop points: CC 111 (RPG Maker), `loopStart/loopEnd` markers (EMIDI/XMI loops), Final Fantasy markers) ─► MidiSynth trait
HMI/HMP/XMI readers ────┘
MidiSynth impls: SoundFontSynth (rustysynth — pure Rust SF2/SF3), Opl3Synth (DMX-faithful instrument handling with GENMIDI)
```

- **SoundFont**: `rustysynth` (pure Rust) with chorus/reverb. Ship a small, freely licensed GM SoundFont next to `room.pk3` (GZDoom ships `gzdoom.sf2` for the same reason); check its licence before bundling and record it in `THIRD_PARTY.md`. Search order: `soundfonts/` dirs, config path list, `fluid_patchset` CVar equivalent `snd_soundfont`.
- **OPL3**: pure-Rust port of the **Nuked OPL3** emulator (LGPL-2.1+; compatible with GPL-3 project licence) — or of DOSBox's `dbopl` (GPL-2+). Driver logic emulates DMX's OPL driver (GENMIDI patches, voice allocation, double-voice instruments, the "OPL2 vs OPL3 mode" `opl_numchips`, `opl_core` selection). Also plays raw OPL streams (IMF, RAW, DRO).
- Timidity/GUS (`DMXGUS`), WildMidi, ADLMIDI, OPNMIDI: **C priority**; would require large ports.
- Hardware MIDI out (`midir` crate; native only): S.

### 4.3 Modules and streams

- Tracker modules: pure-Rust player (`xmrs` + `xmrsplayer` family for MOD/S3M/XM/IT) — verify feature coverage of IT (NNA, envelopes, filters). Fallback: write a minimal IT/XM player if crate coverage is insufficient (ADR). Module `order` selection from MAPINFO.
- Streams: `symphonia` (OGG Vorbis, FLAC, MP3, WAV, AIFF), `ogg`+`opus` decoder if a pure-Rust Opus decoder is available; loop points from tags (`LOOP_START`, `LOOP_END`, `LOOPSTART`, `LOOPLENGTH`, `loop_start`… GZDoom accepts several; port the list from ZMusic `SoundStream` handling).
- GME (chiptune formats): C priority.

### 4.4 Threading

Music synthesis runs on the mixer thread for low-cost synths, or on a dedicated "music decode" thread producing PCM into a ring buffer for expensive ones (SoundFont with chorus/reverb, IT modules) — native. On web, the music generator runs inside the AudioWorklet's processing callback (single-threaded budget) or on the main thread feeding a ring when the worklet can't host the synth (see `18-platform-web.md §Audio`).

## 5. Other

- `snd_output` choices: default device / device by name (cpal enumeration), sample rate (`snd_samplerate`, default device rate), buffer size (`snd_buffersize`).
- PC speaker sounds (Doom `DP*` lumps): synthesize square waves at 140 Hz tone steps when `snd_pcspeaker` is on or digital sound is missing.
- `S_StartSound` from UI (menu sounds) goes directly to audio without the sim.
- `snd_listmididevices`, `snd_status`, `playsound`, `stopsound`, `snd_reset` console commands.

## 6. Tests

- Decoder tests per format (tiny fixtures generated by tools, committed).
- MUS→MIDI conversion golden (event list snapshot).
- OPL emulator test vectors (compare with reference output of the C emulator captured once → committed WAV fixtures, bit-exact for Nuked port).
- Attenuation/panning unit tests vs GZDoom formulas.
- Mixer real-time safety test: run render callback under `assert_no_alloc` in debug builds.
