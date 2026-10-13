# Appendix B — GZDoom source → Ferrite crate map

Paths refer to `https://github.com/ZDoom/gzdoom` (master at project start; pin a commit hash in `docs/decisions/0001-reference-commit.md` and port against that commit). Use this map to find the reference implementation for any feature.

| GZDoom path | Topic | Ferrite crate / module |
|---|---|---|
| `src/common/filesystem/` | resource files, lump lookup | `ferrite-vfs` |
| `src/d_iwad.cpp`, `wadsrc/static/iwadinfo.txt` | IWAD detection | `ferrite-vfs::iwad`, `ferrite-app::iwad_paths` |
| `src/d_main.cpp` | startup sequence, load order, main loop | `ferrite-game::startup`, `ferrite-game::engine` |
| `src/common/engine/sc_man*.cpp` | text scanner | `ferrite-sc` |
| `src/gamedata/g_mapinfo.cpp`, `umapinfo.cpp` | MAPINFO/UMAPINFO | `ferrite-defs::mapinfo` |
| `src/gamedata/textures/`, `src/common/textures/` | texture manager, TEXTURES, ANIMDEFS, image formats | `ferrite-defs::textures`, `ferrite-formats::image` |
| `src/gamedata/d_dehacked.cpp` | DeHackEd/BEX/MBF21 | `ferrite-dehacked` |
| `src/scripting/decorate/`, `src/scripting/thingdef*.cpp` | DECORATE | `ferrite-decorate` |
| `src/gamedata/info.cpp`, `src/playsim/actor.h` | class/actor info | `ferrite-defs::classes`, `ferrite-sim::actor` |
| `src/maploader/` | map loading, UDMF, slopes, polyobjects, compat fixes | `ferrite-map` |
| `src/common/utility/nodebuilder/` (`nodebuild*.cpp`) | node builder | `ferrite-nodebuild` |
| `src/playsim/p_tick.cpp`, `dthinker.cpp` | tic loop, thinkers | `ferrite-sim::tick`, `ferrite-sim::thinker` |
| `src/playsim/p_mobj.cpp` | actor movement, spawning, states | `ferrite-sim::actor`, `::physics` |
| `src/playsim/p_map.cpp`, `p_maputl.cpp`, `p_secnodes.cpp`, `p_blockmap.h` | collision, traversal, blockmap | `ferrite-sim::physics`, `::blockmap` |
| `src/playsim/p_sight.cpp`, `p_trace.cpp`, `p_linetracedata.h` | sight, traces | `ferrite-sim::sight`, `::trace` |
| `src/playsim/p_interaction.cpp` | damage, death | `ferrite-sim::damage` |
| `src/playsim/p_enemy.cpp`, `p_actionfunctions.cpp` | AI, action functions | `ferrite-sim::ai`, `::actions` |
| `src/playsim/p_user.cpp`, `d_player.h` | player | `ferrite-sim::player` |
| `src/playsim/p_spec.cpp`, `p_lnspec.cpp`, `actionspecials.h`, `p_switch.cpp`, `p_teleport.cpp` | specials | `ferrite-sim::specials` |
| `src/playsim/mapthinkers/` (`a_doors`, `a_floor`, `a_ceiling`, `a_plats`, `a_pillar`, `a_lights`, `a_lighttransfer`, `a_scroll`, `a_pusher`, `a_quake`, `a_lightning`, `a_decalfx`, `dsectoreffect`) | movers, light thinkers | `ferrite-sim::movers`, `::lights`, `::effects` |
| `src/playsim/p_3dfloors.cpp`, `p_3dmidtex.cpp` | 3D floors | `ferrite-map::ffloor`, `ferrite-sim::ffloor` |
| `src/playsim/portal.cpp`, `portal.h` | portals | `ferrite-map::portal`, `ferrite-sim::portal` |
| `src/playsim/po_man.cpp` | polyobjects | `ferrite-sim::polyobj` |
| `src/playsim/p_acs.cpp` | ACS | `ferrite-acs` |
| `src/playsim/p_conversation.cpp`, `src/gamedata/p_usdf.cpp` | Strife dialogue | `ferrite-sim::conversation`, `ferrite-defs::dialogue` |
| `src/playsim/a_morph.cpp`, `zscript/actors/morph.zs` | morphing | `ferrite-sim::morph` |
| `src/playsim/bots/` | bots | (stretch) |
| `src/m_random.cpp` | RNG | `ferrite-core::rng` |
| `src/g_game.cpp`, `src/g_level.cpp`, `src/g_levellocals.h` | game flow, levels, hubs, demos, saves | `ferrite-game` |
| `src/serializer*.cpp`, `src/p_saveg.cpp` | save format reference | `ferrite-game::save` (own format) |
| `src/d_net.cpp`, `src/d_protocol.cpp` | netcode | `ferrite-net` |
| `src/common/console/` | console, CVars, binds | `ferrite-console`, `ferrite-input` |
| `src/common/menu/`, `wadsrc/static/zscript/engine/ui/menu/` | menus | `ferrite-ui::menu` |
| `src/g_statusbar/`, `wadsrc/static/zscript/ui/statusbar/` | status bars, SBARINFO, alt HUD | `ferrite-ui::sbar` |
| `src/am_map.cpp` | automap | `ferrite-ui::automap` |
| `src/intermission/`, `src/wi_stuff.cpp`, `src/f_wipe.cpp` | intermissions, finales, wipes | `ferrite-ui::intermission` |
| `src/common/fonts/`, `src/common/2d/` | fonts, 2D drawing | `ferrite-ui::font`, `::draw` |
| `src/rendering/hwrenderer/`, `src/common/rendering/hwrenderer/` | hardware renderer | `ferrite-render` |
| `src/rendering/swrenderer/` | software renderer | `ferrite-render-soft` (stretch) |
| `wadsrc/static/shaders/` | GLSL shaders | `ferrite-render/shaders/*.wgsl` (ported) |
| `src/r_data/gldefs.cpp`, `models.cpp`, `src/common/models/` | GLDEFS, models, voxels | `ferrite-defs::gldefs`, `ferrite-render::models` |
| `src/common/audio/sound/`, `src/sound/` | SFX, SNDINFO, sequences, reverb | `ferrite-audio`, `ferrite-defs::sndinfo`, `ferrite-sim::sndseq` |
| ZMusic repo (`ZDoom/ZMusic`) | music: MIDI readers, devices, OPL, streams | `ferrite-music` |
| `wadsrc/static/xlat/` | line special translation | `ferrite.pk3/xlat/` + `ferrite-defs::xlat` |
| `wadsrc/static/mapinfo/`, `language.*`, `menudef.txt`, `sndinfo.txt`, `compatibility.txt` | engine data | `ferrite.pk3` |
| `wadsrc/static/zscript/actors/**` | built-in actor behaviour (reference) | `ferrite-sim::builtins` + `ferrite.pk3/actors/**` (DECORATE) |
| `wadsrc/static/zscript/level_compatibility.zs` | per-map fixes | `ferrite.pk3/compat/levelfixes.ron` |
| `specs/udmf*.txt` | UDMF spec | `ferrite-map::udmf` |
