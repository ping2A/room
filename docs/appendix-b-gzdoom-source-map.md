# Appendix B — GZDoom source → Room crate map

Paths refer to `https://github.com/ZDoom/gzdoom` (master at project start; pin a commit hash in `docs/decisions/0001-reference-commit.md` and port against that commit). Use this map to find the reference implementation for any feature.

| GZDoom path | Topic | Room crate / module |
|---|---|---|
| `src/common/filesystem/` | resource files, lump lookup | `room-vfs` |
| `src/d_iwad.cpp`, `wadsrc/static/iwadinfo.txt` | IWAD detection | `room-vfs::iwad`, `room-app::iwad_paths` |
| `src/d_main.cpp` | startup sequence, load order, main loop | `room-game::startup`, `room-game::engine` |
| `src/common/engine/sc_man*.cpp` | text scanner | `room-sc` |
| `src/gamedata/g_mapinfo.cpp`, `umapinfo.cpp` | MAPINFO/UMAPINFO | `room-defs::mapinfo` |
| `src/gamedata/textures/`, `src/common/textures/` | texture manager, TEXTURES, ANIMDEFS, image formats | `room-defs::textures`, `room-formats::image` |
| `src/gamedata/d_dehacked.cpp` | DeHackEd/BEX/MBF21 | `room-dehacked` |
| `src/scripting/decorate/`, `src/scripting/thingdef*.cpp` | DECORATE | `room-decorate` |
| `src/gamedata/info.cpp`, `src/playsim/actor.h` | class/actor info | `room-defs::classes`, `room-sim::actor` |
| `src/maploader/` | map loading, UDMF, slopes, polyobjects, compat fixes | `room-map` |
| `src/common/utility/nodebuilder/` (`nodebuild*.cpp`) | node builder | `room-nodebuild` |
| `src/playsim/p_tick.cpp`, `dthinker.cpp` | tic loop, thinkers | `room-sim::tick`, `room-sim::thinker` |
| `src/playsim/p_mobj.cpp` | actor movement, spawning, states | `room-sim::actor`, `::physics` |
| `src/playsim/p_map.cpp`, `p_maputl.cpp`, `p_secnodes.cpp`, `p_blockmap.h` | collision, traversal, blockmap | `room-sim::physics`, `::blockmap` |
| `src/playsim/p_sight.cpp`, `p_trace.cpp`, `p_linetracedata.h` | sight, traces | `room-sim::sight`, `::trace` |
| `src/playsim/p_interaction.cpp` | damage, death | `room-sim::damage` |
| `src/playsim/p_enemy.cpp`, `p_actionfunctions.cpp` | AI, action functions | `room-sim::ai`, `::actions` |
| `src/playsim/p_user.cpp`, `d_player.h` | player | `room-sim::player` |
| `src/playsim/p_spec.cpp`, `p_lnspec.cpp`, `actionspecials.h`, `p_switch.cpp`, `p_teleport.cpp` | specials | `room-sim::specials` |
| `src/playsim/mapthinkers/` (`a_doors`, `a_floor`, `a_ceiling`, `a_plats`, `a_pillar`, `a_lights`, `a_lighttransfer`, `a_scroll`, `a_pusher`, `a_quake`, `a_lightning`, `a_decalfx`, `dsectoreffect`) | movers, light thinkers | `room-sim::movers`, `::lights`, `::effects` |
| `src/playsim/p_3dfloors.cpp`, `p_3dmidtex.cpp` | 3D floors | `room-map::ffloor`, `room-sim::ffloor` |
| `src/playsim/portal.cpp`, `portal.h` | portals | `room-map::portal`, `room-sim::portal` |
| `src/playsim/po_man.cpp` | polyobjects | `room-sim::polyobj` |
| `src/playsim/p_acs.cpp` | ACS | `room-acs` |
| `src/playsim/p_conversation.cpp`, `src/gamedata/p_usdf.cpp` | Strife dialogue | `room-sim::conversation`, `room-defs::dialogue` |
| `src/playsim/a_morph.cpp`, `zscript/actors/morph.zs` | morphing | `room-sim::morph` |
| `src/playsim/bots/` | bots | (stretch) |
| `src/m_random.cpp` | RNG | `room-core::rng` |
| `src/g_game.cpp`, `src/g_level.cpp`, `src/g_levellocals.h` | game flow, levels, hubs, demos, saves | `room-game` |
| `src/serializer*.cpp`, `src/p_saveg.cpp` | save format reference | `room-game::save` (own format) |
| `src/d_net.cpp`, `src/d_protocol.cpp` | netcode | `room-net` |
| `src/common/console/` | console, CVars, binds | `room-console`, `room-input` |
| `src/common/menu/`, `wadsrc/static/zscript/engine/ui/menu/` | menus | `room-ui::menu` |
| `src/g_statusbar/`, `wadsrc/static/zscript/ui/statusbar/` | status bars, SBARINFO, alt HUD | `room-ui::sbar` |
| `src/am_map.cpp` | automap | `room-ui::automap` |
| `src/intermission/`, `src/wi_stuff.cpp`, `src/f_wipe.cpp` | intermissions, finales, wipes | `room-ui::intermission` |
| `src/common/fonts/`, `src/common/2d/` | fonts, 2D drawing | `room-ui::font`, `::draw` |
| `src/rendering/hwrenderer/`, `src/common/rendering/hwrenderer/` | hardware renderer | `room-render` |
| `src/rendering/swrenderer/` | software renderer | `room-render-soft` (stretch) |
| `wadsrc/static/shaders/` | GLSL shaders | `room-render/shaders/*.wgsl` (ported) |
| `src/r_data/gldefs.cpp`, `models.cpp`, `src/common/models/` | GLDEFS, models, voxels | `room-defs::gldefs`, `room-render::models` |
| `src/common/audio/sound/`, `src/sound/` | SFX, SNDINFO, sequences, reverb | `room-audio`, `room-defs::sndinfo`, `room-sim::sndseq` |
| ZMusic repo (`ZDoom/ZMusic`) | music: MIDI readers, devices, OPL, streams | `room-music` |
| `wadsrc/static/xlat/` | line special translation | `room.pk3/xlat/` + `room-defs::xlat` |
| `wadsrc/static/mapinfo/`, `language.*`, `menudef.txt`, `sndinfo.txt`, `compatibility.txt` | engine data | `room.pk3` |
| `wadsrc/static/zscript/actors/**` | built-in actor behaviour (reference) | `room-sim::builtins` + `room.pk3/actors/**` (DECORATE) |
| `wadsrc/static/zscript/level_compatibility.zs` | per-map fixes | `room.pk3/compat/levelfixes.ron` |
| `specs/udmf*.txt` | UDMF spec | `room-map::udmf` |
