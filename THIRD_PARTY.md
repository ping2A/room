# Third-party origins

Ferrite is licensed under GPL-3.0-or-later. Code and tables derived from other projects:

| Component | Origin | Licence | Location |
|-----------|--------|---------|----------|
| SFMT-607 | Saito & Matsumoto / Hiroshima University; GZDoom `src/common/thirdparty/sfmt/` (scalar C path) | BSD-3-Clause | `crates/ferrite-core/src/rng/sfmt.rs` |
| Vanilla `rndtable` | id Software Doom `m_random.c` (via GPL-relicensed Doom source) | GPL-2.0-or-later | `crates/ferrite-core/src/rng/vanilla.rs` |
| Named RNG seeding (`NameCRC` + global seed → `InitByArray`) | GZDoom `src/common/engine/m_random.cpp` (`FRandom::Init`) | BSD-3-Clause (file header) | `crates/ferrite-core/src/rng/mod.rs` |
| CRC-32 of RNG names | zlib/miniz `crc32` as used by GZDoom `CalcCRC32` | zlib | `crc32fast` crate |
| MUS→MIDI converter | Chocolate Doom `src/mus2mid.c` (Ben Ryves / Simon Howard); algorithm port | GPL-2.0-or-later | `crates/ferrite-formats/src/mus.rs` |
| Doom linedef XLAT (specials 0–141) + Boom generalized ranges | GZDoom `wadsrc/static/xlat/base.txt` + `defines.i` (behavioural tables) | GPL-3.0-or-later | `crates/ferrite-map/src/xlat.rs`, `boom_gen.rs` |
| OPL3 emulator | `nuked-opl3` crate (pure-Rust port of Nuked-OPL3 by Nuke.YKT) | LGPL-2.1+ | `crates/ferrite-music` via workspace dep |
| `GENMIDI` layout | Doom / DMX OPL driver (Chocolate Doom / ZMusic references) | GPL family | `crates/ferrite-music/src/opl.rs` |
| TimGM6mb SoundFont | Timidity++ GM bank (fetched by `scripts/fetch-soundfont.sh`) | Public domain / free | `soundfonts/TimGM6mb.sf2` (not committed) |
| DeHackEd vanilla index tables (InfoNames, OrgSprNames, SoundMap, ActionList, WeaponNames, AmmoNames) | GZDoom `wadsrc/static/dehsupp.txt` | GPL-3.0-or-later | `wadsrc/ferrite/dehacked/` |
| Doom SBARINFO fixture | GZDoom `wadsrc/static/sbarinfo/doom.txt` | GPL-3.0-or-later | `testdata/sbarinfo/doom.txt` |
| TERRAIN fixture | GZDoom `wadsrc/static/terrain.txt` | GPL-3.0-or-later | `testdata/terrain/gzdoom_terrain.txt` |
| DECALDEF fixture | GZDoom `wadsrc/static/decaldef.txt` | GPL-3.0-or-later | `testdata/decaldef/gzdoom_decaldef.txt` |
| LOCKDEFS fixture | GZDoom `wadsrc/static/lockdefs.txt` | GPL-3.0-or-later | `testdata/lockdefs/gzdoom_lockdefs.txt` |
| MENUDEF fixture | GZDoom `wadsrc/static/menudef.txt` | GPL-3.0-or-later | `testdata/menudef/gzdoom_menudef.txt` |

See `docs/appendix-a-dependencies.md` for crate policy.
