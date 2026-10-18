# Third-party origins

Room is licensed under GPL-3.0-or-later. Code and tables derived from other projects:

| Component | Origin | Licence | Location |
|-----------|--------|---------|----------|
| SFMT-607 | Saito & Matsumoto / Hiroshima University; GZDoom `src/common/thirdparty/sfmt/` (scalar C path) | BSD-3-Clause | `crates/room-core/src/rng/sfmt.rs` |
| Vanilla `rndtable` | id Software Doom `m_random.c` (via GPL-relicensed Doom source) | GPL-2.0-or-later | `crates/room-core/src/rng/vanilla.rs` |
| Named RNG seeding (`NameCRC` + global seed → `InitByArray`) | GZDoom `src/common/engine/m_random.cpp` (`FRandom::Init`) | BSD-3-Clause (file header) | `crates/room-core/src/rng/mod.rs` |
| CRC-32 of RNG names | zlib/miniz `crc32` as used by GZDoom `CalcCRC32` | zlib | `crc32fast` crate |
| MUS→MIDI converter | Chocolate Doom `src/mus2mid.c` (Ben Ryves / Simon Howard); algorithm port | GPL-2.0-or-later | `crates/room-formats/src/mus.rs` |
| Doom linedef XLAT (specials 0–141) + Boom generalized ranges | GZDoom `wadsrc/static/xlat/base.txt` + `defines.i` (behavioural tables) | GPL-3.0-or-later | `crates/room-map/src/xlat.rs`, `boom_gen.rs` |
| OPL3 emulator | `nuked-opl3` crate (pure-Rust port of Nuked-OPL3 by Nuke.YKT) | LGPL-2.1+ | `crates/room-music` via workspace dep |
| `GENMIDI` layout | Doom / DMX OPL driver (Chocolate Doom / ZMusic references) | GPL family | `crates/room-music/src/opl.rs` |
| TimGM6mb SoundFont | Timidity++ GM bank (fetched by `scripts/fetch-soundfont.sh`) | Public domain / free | `soundfonts/TimGM6mb.sf2` (not committed) |
| DeHackEd vanilla index tables (InfoNames, OrgSprNames, SoundMap, ActionList, WeaponNames, AmmoNames) | GZDoom `wadsrc/static/dehsupp.txt` | GPL-3.0-or-later | `wadsrc/room/dehacked/` |
| Doom actor state sequences (frames, tics, action order incl. XDeath / Raise / Heal) | id Software Doom `info.c` state table, cross-checked with GZDoom `wadsrc/static/zscript/actors/doom/` (behavioural tables) | GPL-2.0-or-later / GPL-3.0-or-later | `wadsrc/room/actors/doom.txt`, `testdata/actors/doom_basic.txt` |
| Doom SBARINFO fixture | GZDoom `wadsrc/static/sbarinfo/doom.txt` | GPL-3.0-or-later | `testdata/sbarinfo/doom.txt` |
| TERRAIN fixture | GZDoom `wadsrc/static/terrain.txt` | GPL-3.0-or-later | `testdata/terrain/gzdoom_terrain.txt` |
| DECALDEF fixture | GZDoom `wadsrc/static/decaldef.txt` | GPL-3.0-or-later | `testdata/decaldef/gzdoom_decaldef.txt` |
| LOCKDEFS fixture | GZDoom `wadsrc/static/lockdefs.txt` | GPL-3.0-or-later | `testdata/lockdefs/gzdoom_lockdefs.txt` |
| Build formats (GRP, ART, PALETTE.DAT, MAP v7/8) and unit conversion (x/16, −y/16, −z/256) | Ken Silverman BUILDINF.TXT; Raze map-loading unit conventions (behavioural facts) | GPL-2.0-or-later / GPL-3.0-or-later | `crates/room-build/` |
| Duke Nukem 3D behaviour tables: sector lotags / SE lotags, switch and hurt-floor tile names, `pskyoff` sky layouts, rotation view layouts, action tile/delay rules, weapon HUD positions, status-bar layout (`coolgaugetext`), enemy attack kinds, `LOOKUP.DAT` / VOC formats | 3D Realms Duke Nukem 3D source release (`sector.c`, `premap.c`, `game.c`, `player.c`, `actors.c`) and Raze (behavioural facts) | GPL-2.0-or-later / GPL-3.0-or-later | `crates/room-build/src/{convert,duke,wad,lookup,sprites,condata}.rs`, `crates/room-ui/src/statusbar.rs`, `crates/room-formats/src/sound.rs` |
| Duke moving sectors, water, bombs: SE0/1/7/10/11/15 and sector lotags 9/23/25 semantics (speeds, distances, pivots, start direction), pipebomb/tripbomb pickups and radii, jetpack | Duke Nukem 3D source release (`sector.c`, `actors.c`, `player.c`) and Raze (behavioural facts) | GPL-2.0-or-later / GPL-3.0-or-later | `crates/room-build/src/{polys,convert,duke}.rs` |
| Polyobject rules (rotate/door speeds /8, perpetual angle 255, busy/blocked door handling), `PowerFlight` / `FRICTION_FLY`, `A_CheckProximity` flags | GZDoom `src/playsim/po_man.cpp`, `p_user.cpp`, `p_mobj.cpp`, `p_actionfunctions.cpp` (behavioural facts) | GPL-3.0-or-later | `crates/room-sim/src/{polyobj,think_player,physics,action}.rs` |
| MENUDEF fixture | GZDoom `wadsrc/static/menudef.txt` | GPL-3.0-or-later | `testdata/menudef/gzdoom_menudef.txt` |

See `docs/appendix-a-dependencies.md` for crate policy.
