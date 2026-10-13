# Appendix C — Glossary

| Term | Meaning |
|---|---|
| **ABI (scripting)** | The import/export contract between the engine and WASM modules (`12 §5`). Versioned by major number. |
| **ACS** | Action Code Script — Hexen/ZDoom bytecode scripting language for maps (`10`). |
| **Activation (SPAC)** | How a line special is triggered: cross, use, impact, push, etc. |
| **Actor** | Game object with position, state machine and class (monsters, items, projectiles, decorations, players' bodies). |
| **Arbitrator** | The netplay peer that owns game settings (usually the host). |
| **BAM** | Binary Angle Measurement, `u32` angle where 2³² = 360°. Used in vanilla paths. |
| **Blockmap** | 128×128 grid accelerating collision queries. |
| **Container** | A mounted resource file (WAD, PK3, PK7, directory). |
| **DECORATE** | Text language for declaring actor classes (`11 §1`). |
| **Definitions** | Immutable, compiled registries built from all loaded content (`01 §3`). |
| **DeHackEd / BEX / MBF21** | Patch formats modifying vanilla actor tables (`11 §2`). |
| **Fixed** | 16.16 fixed-point number (`i32`). |
| **GL nodes** | BSP whose subsectors are closed convex polygons (needed by hardware renderers). |
| **Hub** | Group of maps (cluster with `hub` flag) where map state is preserved when travelling between them. |
| **IWAD / PWAD** | Main game data file / patch (mod) WAD. |
| **Linked portal** | Portal that connects geometry for collision, sight and sound, not just rendering. |
| **Lump** | One named resource in a container. |
| **Module** | A WebAssembly mod binary with manifest (`12`). |
| **Namespace** | Category of lumps (sprites, flats, textures, …) derived from markers or directories. |
| **Play / UI scope** | Deterministic world-mutating code vs. presentation code (`12 §3`). |
| **PSprite** | Player sprite layer: weapon, flash, overlays drawn in screen space. |
| **SimView** | Read-only snapshot of the world consumed by renderer, audio and UI. |
| **Special** | Numbered action with five integer arguments (`09`). |
| **State** | One frame of an actor's state machine: sprite, frame, duration, action, next. |
| **Statnum** | Thinker list index controlling update order (`07 §3`). |
| **Thinker** | Anything that updates every tic. |
| **Tic** | One simulation step (1/35 s). |
| **TicCmd** | Per-player input for one tic (`07 §7.1`). |
| **TID** | Thing ID used by ACS/specials to address actors. |
| **UDMF** | Universal Doom Map Format, a text map format (`06 §2.1`). |
| **XLAT** | Translation tables turning Doom-format line types into ZDoom specials (`09 §3`). |
| **ZScript** | GZDoom's scripting language; not supported by Ferrite (`11 §3`). |
