# Appendix A — Dependency policy and allowed crates

## 1. Policy

"Pure Rust" means: **no C/C++/assembly compiled as part of the Ferrite build**, except in the explicit allowlist below. Linking against **operating-system APIs** (graphics drivers, audio servers, windowing, udev) is allowed because it's unavoidable and doesn't vendor foreign code.

Enforcement: `cargo deny` with a `[bans]` section listing crates that pull `cc`/`cmake`/`bindgen`-built native code; CI fails on any new one. Licences allowed: MIT, Apache-2.0, BSD-2/3, Zlib, ISC, Unicode, MPL-2.0, LGPL-2.1+ (only for ported emulators, see audio), GPL-2.0+/GPL-3.0+ (project is GPL-3.0-or-later). Any new dependency requires a line in this file with its purpose.

Before adding a crate, verify its current maintenance state and that the version is not a yanked release. Versions are pinned in the workspace `Cargo.toml` `[workspace.dependencies]`.

## 2. Allowed crates (initial list)

| Area | Crate | Purpose / notes |
|---|---|---|
| core | `bitflags`, `thiserror`, `smallvec`, `arrayvec`, `indexmap`, `foldhash` (or `ahash` with fixed seed), `bytes`, `serde`, `ron` (compat `levelfixes.ron`), `postcard`, `xxhash-rust`, `crc32fast`, `tracing`, `tracing-subscriber`, `libm` | general |
| cli | `clap`, `anyhow` | `ferrite-cli` / tool binaries |
| hashing | `md-5`, `sha2` | MD5 for compat DB/fingerprints, SHA-256 for module identity |
| vfs | `zip` 8.x (default features off; `deflate` + `bzip2` + `lzma` — pure Rust via `libbz2-rs-sys` / `lzma-rust2`; never enable `bzip2-rs`/`xz` which pull C), `sevenz-rust2` (7z) | archives |
| images | `png`, `gif` (encode screenshots / in-game GIF capture), `zune-jpeg`, hand-rolled TGA/BMP/PCX (M2); `image` (default-features off; golden tests + later decode), `image-webp`, `ddsfile`, `texture2ddecoder` | decoding / capture |
| fonts | `ab_glyph` or `fontdue` | TrueType extension |
| render | `wgpu`, `naga` (glsl-in, wgsl-out, validation), `glam` (render-only math), `bytemuck` | GPU |
| platform | `winit`, `pollster` (block on wgpu futures in native shells), `raw-window-handle`, `directories`, `arboard`, `rfd`, `gilrs` (feature `gamepad`), `midir` (optional hardware MIDI) | native shell |
| audio | `lewton` (Vorbis), `claxon` (FLAC), `cpal` (native device I/O; OS `-sys` only), `symphonia`, `rustysynth`; later `rtrb`, `rubato`, `xmrs` + `xmrsplayer`, `assert_no_alloc` | sound & music |
| scripting | `wasmparser`, `wasm-encoder`, `wasmi`, `wasmtime` (native only; Cranelift is pure Rust) | WASM host |
| net | `matchbox_socket` (WebRTC, web + native); `snow` (Noise Protocol Framework; feature `noise`, pure-Rust resolver) | transport / encryption |
| web | `wasm-bindgen`, `wasm-bindgen-futures`, `js-sys`, `web-sys`, `console_error_panic_hook`, `wasm-bindgen-test` | browser (`ferrite-web` feature `browser`) |
| parallel | `rayon` (native only paths) | loading, scene build |
| config | `toml`, `toml_edit` (preserve user comments), `rust-ini` (gzdoom.ini import) | |
| guest SDK | `talc` or `dlmalloc`, `libm` | WASM guest |
| dev/test | `proptest`, `insta`, `criterion`, `image-compare`, `pretty_assertions`, `cargo-fuzz`/`libfuzzer-sys` (fuzz only; libFuzzer is C++ but limited to the fuzz workspace) | testing |

## 3. Native-code exceptions (allowlist)

| Crate | Why allowed | Scope |
|---|---|---|
| `ring` (or other crypto with asm) | DTLS for native WebRTC via `matchbox_socket`/`webrtc-rs` | optional feature `net-webrtc-native`; UDP transport needs no crypto |
| `libfuzzer-sys` | fuzzing harness | `fuzz/` crate only, never shipped |
| OS binding `-sys` crates without vendored sources (`alsa-sys`, `libudev-sys`, `coreaudio-sys`, `windows-sys`, …) | OS APIs | transitively via cpal/gilrs/winit |

## 4. Ported (not linked) third-party algorithms

Code translated into Rust from other projects must be listed in `THIRD_PARTY.md` with licence and origin:

| Component | Origin | Licence |
|---|---|---|
| SFMT-607 | Saito & Matsumoto reference implementation | BSD-3 |
| Node builder | ZDBSP / GZDoom `nodebuild*.cpp` | GPL-2.0+/GPL-3.0+ |
| OPL3 emulator | Nuked OPL3 | LGPL-2.1+ |
| DMX OPL driver behaviour | GZDoom/ZMusic, Chocolate Doom | GPL |
| Behaviour ports (sim, ACS, UI) | GZDoom, ZDoom, Doom, Heretic/Hexen, Boom/MBF/PrBoom+ | GPL family |
| xlat tables, compatibility data, IWADINFO, language strings | GZDoom `wadsrc/static` | GPL |
