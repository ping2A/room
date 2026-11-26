//! The app itself: window, game loop, rendering, menus and netplay glue.
//! Moved out of the old `main.rs` in docs/41 M0; the `room` binary and the
//! mobile shells both start it through [`crate::run`].

use crate::config::{AppConfig, StartError};
use crate::lifecycle::ResumeStep;

mod launcher_input;
mod play_overlay;
mod touch_input;

/// Report a start-up failure on stderr (`room: …`, as the binary always
/// printed it) and return it from the enclosing function, so the caller
/// decides what to do: the desktop binary exits with `exit_code`, a phone
/// shows the message.
macro_rules! fail {
    ($code:expr, $($fmt:tt)+) => {{
        let message = format!($($fmt)+);
        eprintln!("room: {message}");
        return Err(StartError {
            message,
            exit_code: $code,
        });
    }};
}
use crate::platform;
use crate::{
    audio_out, gamepad, launcher, midi_device, netplay, screen_capture, shader_cvars, soundfont,
};
use audio_out::AudioOut;
use glam::Vec3;
use midi_device::{MidiBackend, load_genmidi, resolve_midi_device};
use room_audio::{SampleBank, SoundEvent, SoundFlags, SoundOrigin};
use room_console::{Console, HostCommand};
use room_core::SoundId;
use room_decorate::parse_decorate;
use room_defs::{
    DefinitionsBuilder, DynLightKind, GlDefs, Language, ModelDefs, VoxelDefs, parse_language,
    view_model_weapon_scale,
};
use room_game::{
    DemoPlayer, DemoRecorder, GameOptions, GameRuntime, GameState, Headless, Skill, SoundSystem,
    intermission_music_candidates, keyconf_alias_lines, keyconf_default_binds,
    mount_embedded_engine, mount_optional_engine_extras, play_first_music_lump, play_map_music,
    play_music_lump, read_save_info, title_music_candidates,
};
use room_input::{
    AttackLatch, InputSample, Key, MOUSE_DEG_PER_COUNT, ModButtonState, WeaponImpulseState,
    build_ticcmd_with_mods_and_impulses, default_bind_pairs, device_button_is_primary,
};
use room_map::{MapGeometry, load_map};
use room_nodebuild::{apply_nodes, ensure_nodes};
use room_render::{
    Camera, Canvas, CanvasCmd, CanvasGpu, CubemapFaces, DecalStore, DynLightInstance, GpuContext,
    LightMode, MAX_VOXEL_WALL_VERTS_PER_FRAME, MaterialCatalog, MaterialMaps, ModZonePass,
    ModelActorPose, ModelCatalog, OffscreenTarget, PARTICLE_TEX, PARTICLE_TEX_ADD, ParticleStore,
    PostChain, PostChainInputs, RenderSettings, SceneLists, SpriteInstance, SpriteOrient,
    TextureAnimator, UnlitPass, VoxelActorPose, VoxelCatalog, ZoneDraw, ZoneMeshes,
    append_actor_models, append_decals, append_particles_animated, apply_glow_fullbright,
    build_scene, build_scene_with_sky_ex_shared, build_zone_meshes, particle_tint_tex_name,
    read_rgba, read_texture_rgba, render_offscreen, scroll_sky_u, soft_disc_rgba,
    sprite_lump_candidates_rot, sprite_offset_delta, sprite_rotation_digit, voxel_spin_degrees,
    voxel_view_angles,
};
use room_sim::mod_hud::layer as mod_hud_layer;
use room_sim::{
    ActorId, Buttons, ClassRegistry, FLAG_CORPSE, FLAG_DONTDRAW, FLAG_DROPPED, FLAG_FLOAT,
    FLAG_MISSILE, FLAG_NOGRAVITY, FLAG_SPAWNCEILING, RenderStyle, ShotKind, StateTable, TicCmd,
    actor_light_level, check_sight, sprite_pose,
};
use room_testkit::{MapBuilder, combat_arena, rect};
use room_ui::{
    AutomapView, CanvasSink, DoomStatusBar, DrawCmd, DukeStatusBar, FontRegistry, HudState,
    IntermissionStats, ListMenu, MenuCommand, MenuItem, MenuSfx, MenuStack, MessageQueue,
    ModHudMapCtx, PatchCache, StatusBar, UiDrawList, Vec2, anim_pic, blit_draw_list_full,
    draw_automap, draw_crosshair, draw_finale, draw_intermission, draw_mod_hud_layer,
    draw_mod_hud_modals, draw_mod_zones_automap, draw_outside_zone_tint, draw_stcfn_string,
    draw_title, draw_weapon_lit, draw_weapon_tinted, finale_anm_prefix, overlay_bob,
    overlay_layer_bob, parse_titledef, player_map_poses, splash_ms, title_screen_pic,
};
use room_vfs::{Namespace, Vfs, VfsBuilder};
use screen_capture::GifRecorder;
use shader_cvars::{
    cycle_cvar, graphics_menu, next_rendermode, render_settings_from_cvars,
    reset_shaders_to_defaults, software_rendermode, swap_cvar, toggle_float_cvar,
};
use soundfont::load_first_soundfont;
use std::path::PathBuf;
use std::sync::Arc;
use winit::application::ApplicationHandler;
use winit::dpi::{LogicalSize, PhysicalPosition};
use winit::event::{DeviceEvent, ElementState, KeyEvent, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, DeviceEvents, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowId};

const DUKE_RTS_SOUND_BASE: u32 = 0x1_0000;

/// Run the app until its window closes. `event_loop`: one the caller already
/// built (phones must build theirs from the OS activity); `None` builds one.
pub(crate) fn run_with(
    args: AppConfig,
    event_loop: Option<EventLoop<()>>,
) -> Result<(), StartError> {
    // Display options a game started from the launcher keeps.
    let base = AppConfig {
        set_cvars: args.set_cvars.clone(),
        shot_after: args.shot_after,
        bench_frames: args.bench_frames,
        mididevice: args.mididevice,
        soundfont: args.soundfont.clone(),
        ..AppConfig::default()
    };
    crate::startlog::step(format_args!(
        "building the app (launcher screen: {}, game: {})",
        args.launcher_screen,
        args.iwad
            .as_ref()
            .map_or_else(|| "none".to_owned(), |p| p.display().to_string())
    ));
    let Some(app) = build_app(args, None)? else {
        return Ok(());
    };
    crate::startlog::step("app built; starting the event loop");
    let event_loop = match event_loop.map_or_else(EventLoop::new, Ok) {
        Ok(e) => e,
        Err(e) => {
            fail!(1, "event loop: {e}");
        }
    };
    // Raw mouse buttons while look-grabbed (needed on every OS; macOS no-ops but safe).
    event_loop.listen_device_events(DeviceEvents::Always);
    event_loop.set_control_flow(ControlFlow::Poll);
    let mut shell = Shell {
        app,
        base,
        loading_shown_at: None,
        lobby: None,
        ready_net: None,
        lan_search: None,
    };
    crate::startlog::step("event loop running (waiting for the window)");
    if let Err(e) = event_loop.run_app(&mut shell) {
        fail!(1, "{e}");
    }
    print_endoom(&shell.app);
    Ok(())
}

/// Load everything for one run (game data, sound, netplay) and build the
/// app, without a window yet. `None`: nothing to run (`--launcher` listing).
/// A finished co-op lobby, handed to [`build_app`].
pub(crate) struct NetReady {
    pub(crate) net: netplay::NetPlay,
    /// Start map from the host's welcome (joiners).
    pub(crate) welcome_map: Option<String>,
}

/// Host or join as `args` says (`--host` / `--join`), waiting for the other
/// players. Blocks: the desktop command line calls it from [`build_app`],
/// the launcher on a thread of its own. Errors are `(exit code, message)`.
pub(crate) fn run_lobby(args: &AppConfig) -> Result<NetReady, (i32, String)> {
    let fingerprint = netplay::config_fingerprint(args);
    let skill = args.skill.clamp(1, 5);
    if let Some(bind) = args.host.as_deref() {
        let addr = netplay::parse_host_bind(bind).map_err(|e| (2, format!("--host: {e}")))?;
        let max_players = match args.transport {
            netplay::NetTransportKind::TcpServer => args.players.clamp(2, 8),
            _ => args.players.clamp(2, 4),
        };
        let net = netplay::NetPlay::host(netplay::HostOpts {
            bind_addr: addr,
            max_players,
            fingerprint,
            rng_seed: 0xDEAD_1993,
            map: &args.map,
            skill,
            kind: args.transport,
            noise: args.noise,
            deathmatch: args.deathmatch,
            bots: args.bots,
        })
        .map_err(|e| (1, format!("host failed: {e}")))?;
        return Ok(NetReady {
            net,
            welcome_map: None,
        });
    }
    let Some(join) = args.join.as_deref() else {
        return Err((2, "no --host or --join".into()));
    };
    let addr = netplay::parse_join_addr(join).map_err(|e| (2, format!("--join: {e}")))?;
    let (net, map_name) = netplay::NetPlay::join(
        addr,
        fingerprint,
        "player",
        args.transport,
        args.noise,
        args.players.clamp(2, 8),
    )
    .map_err(|e| (1, format!("join failed: {e}")))?;
    Ok(NetReady {
        net,
        welcome_map: Some(map_name),
    })
}

fn build_app(
    mut args: AppConfig,
    pre_net: Option<NetReady>,
) -> Result<Option<ViewerApp>, StartError> {
    let platform = platform::current();
    crate::startlog::step(format_args!("build: platform {}", platform.name()));
    room_vfs::mapped::set_map_large_files(
        platform.map_large_files() || std::env::var_os("ROOM_MMAP").is_some(),
    );
    if let Err(e) = args.apply_implications(platform.dev_root().as_deref(), |p| p.is_file()) {
        fail!(e.exit_code, "{e}");
    }
    if args.launcher {
        let cands = launcher::discover_iwads();
        launcher::print_launcher_help(&cands);
        for p in launcher::discover_gzdoom_ini() {
            eprintln!("room: found gzdoom.ini candidate {}", p.display());
        }
        return Ok(None);
    }
    if args.combat_demo && args.iwad.is_none() {
        let cands = launcher::discover_iwads();
        if let Some(c) = launcher::prefer_doom2(&cands) {
            eprintln!("room: auto IWAD {}", c.path.display());
            args.iwad = Some(c.path.clone());
        }
    }
    let bench = args.bench_frames.is_some();
    crate::bench::log_stage(bench, "start");
    crate::startlog::step("build: loading the map and game files");
    let (map, mut vfs) = match load_viewer_map(&args) {
        Ok(v) => v,
        Err(e) => {
            crate::startlog::step(format_args!("build: game files failed: {e}"));
            eprintln!("room: {e}");
            let cands = launcher::discover_iwads();
            if !cands.is_empty() {
                launcher::print_launcher_help(&cands);
            }
            return Err(StartError {
                message: e,
                exit_code: 1,
            });
        }
    };
    crate::bench::log_stage(bench, "mounting game files");
    let skill = Skill::new(args.skill.clamp(1, 5));
    // Open audio before the play session so SFX resample to the device rate.
    crate::startlog::step("build: opening audio output");
    let audio = match AudioOut::start() {
        Ok(a) => {
            eprintln!("room: audio output started");
            crate::startlog::step(format_args!("build: audio open at {} Hz", a.sample_rate));
            Some(a)
        }
        Err(e) => {
            eprintln!("room: audio: {e}");
            crate::startlog::step(format_args!("build: no audio: {e}"));
            None
        }
    };
    let audio_rate = audio
        .as_ref()
        .map(|a| a.sample_rate)
        .unwrap_or(room_audio::ENGINE_SAMPLE_RATE);
    let mut net: Option<netplay::NetPlay> = None;
    let mut local_seat: u8 = 0;
    let mut coop_players: u8 = 1;
    let mut rng_seed: u32 = 1993;
    let lobby = match pre_net {
        Some(ready) => Some(ready),
        None if args.host.is_some() || args.join.is_some() => match run_lobby(&args) {
            Ok(ready) => Some(ready),
            Err((code, message)) => {
                fail!(code, "{message}");
            }
        },
        None => None,
    };
    if let Some(ready) = lobby {
        if let Some(map_name) = &ready.welcome_map {
            if !map_name.eq_ignore_ascii_case(&args.map) {
                eprintln!(
                    "room: warning: host map {map_name} != local --map {}",
                    args.map
                );
            }
        }
        local_seat = ready.net.local.0;
        coop_players = ready.net.peer_count;
        rng_seed = ready.net.rng_seed;
        net = Some(ready.net);
    }
    // Rules: the host's in netplay, else the command line.
    let (deathmatch, bots) = net
        .as_ref()
        .map_or((args.deathmatch, args.bots), |n| (n.deathmatch, n.bots));
    let (play, decaldef, script_sbar, sbarinfo_bar, mod_cvars) = if args.play && !args.native_duke {
        let (mut game, decaldef, script_sbar, sbarinfo_bar, mod_cvars) = start_play_session(
            &map,
            args.decorate.as_deref(),
            vfs.as_mut(),
            skill,
            audio_rate,
            rng_seed,
            coop_players,
            (deathmatch, bots),
        )?;
        if let Some(net) = net.as_ref() {
            for (i, n) in net
                .seat_names
                .iter()
                .enumerate()
                .take(usize::from(coop_players))
            {
                if !n.is_empty() {
                    game.set_player_display_name(i, n);
                }
            }
        }
        // Default: jump straight into `--map` so WASD/playsim work without a menu dig.
        if !args.title {
            if let Err(e) = game.begin_pending_map() {
                fail!(1, "start map: {e}");
            }
            let label = if args.combat_demo {
                "combat arena"
            } else {
                args.map.as_str()
            };
            eprintln!(
                "room: entered {label} (skill {}, coop={coop_players}, seat={local_seat})",
                skill.0
            );
            // Debug: `ROOM_START_POS="x,y[,angle]"` moves seat 0 (screenshot repros).
            if let Ok(spec) = std::env::var("ROOM_START_POS") {
                let v: Vec<f64> = spec
                    .split(',')
                    .filter_map(|s| s.trim().parse().ok())
                    .collect();
                if let (Some(&x), Some(&y), Some(w)) = (v.first(), v.get(1), game.world_mut()) {
                    if let Some(mo) = w.players[0].as_ref().and_then(|p| p.mo) {
                        let z = room_sim::sector_at_xy(w, x, y)
                            .and_then(|i| w.sectors.get(i))
                            .map_or(0.0, |s| s.floor_z);
                        if let Some(a) = w.actors.get_mut(mo) {
                            a.pos = room_core::DVec3::new(x, y, z);
                            if let Some(&ang) = v.get(2) {
                                a.angle = room_core::Angle::from_degrees(ang);
                            }
                        }
                        eprintln!("room: ROOM_START_POS → ({x}, {y}, {z})");
                    }
                }
            }
            if args.give_all {
                for seat in 0..coop_players {
                    let n = game.cheat_give(seat as usize, "all");
                    eprintln!("room: give all → seat {seat}: {n} items");
                }
            }
            if args.god {
                for seat in 0..coop_players {
                    if game.cheat_god(seat as usize) {
                        eprintln!("room: god on → seat {seat}");
                    }
                }
            }
            if net.is_some() {
                eprintln!(
                    "room: netplay — WASD/mouse control seat {local_seat}; lockstep with {coop_players} peers"
                );
            } else if args.combat_demo {
                // Snap chaingun ready so the first LMB fires without a raise delay.
                if let Some(world) = game.world_mut() {
                    if let Some(cg) = world.classes.find_name("Chaingun") {
                        if let Some(p) = world.players[0].as_mut() {
                            p.ready_weapon = Some(cg);
                            p.pending_weapon = None;
                            p.pspr_state = None;
                            p.pspr_tics = 0;
                            p.pspr_y = 32.0; // WEAPON_TOP
                        }
                    }
                }
                eprintln!(
                    "room: combat demo — WASD move, mouse look, Ctrl/F/LMB fire, 1–7 weapons, E use, L toggle look{}",
                    if args.god { " (god)" } else { "" }
                );
            } else {
                eprintln!("room: use --title for the main menu");
            }
        }
        (Some(game), decaldef, script_sbar, sbarinfo_bar, mod_cvars)
    } else {
        (None, None, None, None, Vec::new())
    };
    let (duke_session, duke_renderer, duke_sound_bank, duke_campaign_content, duke_start_state) =
        if args.native_duke {
            let Some(path) = args.iwad.as_ref() else {
                fail!(2, "native Duke needs an input GRP path");
            };
            let bytes = match std::fs::read(path) {
                Ok(bytes) => bytes,
                Err(e) => {
                    fail!(1, "read Duke GRP {}: {e}", path.display());
                }
            };
            let mut addon_bytes: Vec<Vec<u8>> = Vec::with_capacity(args.grp.len());
            for addon in &args.grp {
                match std::fs::read(addon) {
                    Ok(bytes) => addon_bytes.push(bytes),
                    Err(error) => {
                        fail!(1, "read Duke add-on GRP {}: {error}", addon.display());
                    }
                }
            }
            let mut grp_bytes = Vec::with_capacity(addon_bytes.len() + 1);
            grp_bytes.push(bytes);
            grp_bytes.extend(addon_bytes);
            let grp_layers: Vec<&[u8]> = grp_bytes.iter().map(Vec::as_slice).collect();
            let con_bytes = match args.con.as_ref().map(|con| (con, std::fs::read(con))) {
                None => None,
                Some((_, Ok(bytes))) => Some(bytes),
                Some((con, Err(error))) => {
                    fail!(1, "read Duke CON {}: {error}", con.display());
                }
            };
            let con_override = con_bytes.as_deref();
            let mut con_includes = args.con.as_ref().map(|con| DukeConIncludes {
                directory: con
                    .parent()
                    .unwrap_or_else(|| std::path::Path::new("."))
                    .to_path_buf(),
            });
            // A `--con` from a mod folder (one with DEF files, like Brutal
            // Duke's) is EDuke32 CON; a lone replacement GAME.CON stays Atomic.
            let con_dialect =
                duke_con_dialect(args.con.as_ref().and_then(|con| con.parent()).map(|dir| {
                    if dir.as_os_str().is_empty() {
                        std::path::Path::new(".")
                    } else {
                        dir
                    }
                }));
            let selected_skill = args.skill.clamp(1, 5);
            let skill = selected_skill - 1;
            let tables_dat = path.parent().and_then(|parent| {
                ["TABLES.DAT", "tables.dat"]
                    .into_iter()
                    .find_map(|name| std::fs::read(parent.join(name)).ok())
            });
            let (mod_def_files, mod_tile_assets, mod_mhk_files, mod_music_assets) =
                match args.con.as_ref() {
                    Some(con) => match read_duke_mod_render_assets(
                        con.parent().unwrap_or_else(|| std::path::Path::new(".")),
                    ) {
                        Ok(assets) => assets,
                        Err(error) => {
                            fail!(1, "read Duke mod render assets: {error}");
                        }
                    },
                    None => (Vec::new(), Vec::new(), Vec::new(), Vec::new()),
                };
            let mod_def_refs: Vec<(&str, &[u8])> = mod_def_files
                .iter()
                .map(|(name, bytes)| (name.as_str(), bytes.as_slice()))
                .collect();
            let mod_tile_refs: Vec<(&str, &[u8])> = mod_tile_assets
                .iter()
                .map(|(name, bytes)| (name.as_str(), bytes.as_slice()))
                .collect();
            let mod_mhk_refs: Vec<(&str, &[u8])> = mod_mhk_files
                .iter()
                .map(|(name, bytes)| (name.as_str(), bytes.as_slice()))
                .collect();
            let renderer =
                match room_duke::DukeSoftRenderer::from_grp_layers_with_mod_assets_and_mhks(
                    &grp_layers,
                    tables_dat.as_deref(),
                    &mod_def_refs,
                    &mod_tile_refs,
                    &mod_mhk_refs,
                ) {
                    Ok(renderer) => renderer,
                    Err(e) => {
                        fail!(1, "Duke renderer: {e}");
                    }
                };
            let map_file = std::path::Path::new(&args.map);
            let mut standalone_map_bytes = None;
            let session_result = if map_file.is_file() {
                match std::fs::read(map_file) {
                    Ok(map_bytes) => {
                        standalone_map_bytes = Some(map_bytes.clone());
                        room_duke::session_from_grp_map_bytes_with_con_and_includes_dialect(
                            &grp_layers,
                            &map_bytes,
                            skill,
                            con_override,
                            con_includes.as_mut().map(|resolver| {
                                resolver as &mut dyn room_duke::con::IncludeResolver
                            }),
                            con_dialect,
                        )
                    }
                    Err(error) => Err(format!("read user map {}: {error}", map_file.display())),
                }
            } else {
                room_duke::session_from_grp_layers_with_con_and_includes_dialect(
                    &grp_layers,
                    &args.map,
                    skill,
                    con_override,
                    con_includes
                        .as_mut()
                        .map(|resolver| resolver as &mut dyn room_duke::con::IncludeResolver),
                    con_dialect,
                )
            };
            let mut session = match session_result {
                Ok(session) => session,
                Err(e) => {
                    fail!(1, "Duke start: {e}");
                }
            };
            let selected_map_name = if map_file.is_file() {
                map_file
                    .file_name()
                    .and_then(std::ffi::OsStr::to_str)
                    .unwrap_or(&args.map)
                    .to_owned()
            } else {
                let map_name = args.map.trim().to_ascii_uppercase();
                let base = map_name.strip_suffix(".MAP").unwrap_or(&map_name);
                format!("{base}.MAP")
            };
            let selected_map_bytes = standalone_map_bytes.or_else(|| {
                let name = selected_map_name.to_ascii_uppercase();
                room_build::grp::GrpStack::parse_layers(&grp_layers)
                    .ok()?
                    .get(&name)
                    .map(<[u8]>::to_vec)
            });
            if let Some(map_bytes) = selected_map_bytes.as_deref() {
                match renderer.load_map_hack_for_map(&selected_map_name, map_bytes) {
                    Ok(Some(name)) => eprintln!("room: loaded Duke map hack {name}"),
                    Ok(None) => {}
                    Err(error) => eprintln!("room: Duke map hack skipped: {error}"),
                }
            }
            if net.is_some() {
                if let Err(error) = session.validate_network_content() {
                    fail!(1, "Duke network content: {error}");
                }
                if let Err(error) = session.configure_multiplayer_mode(
                    usize::from(coop_players),
                    args.duke_game_mode,
                    args.duke_friendly_fire,
                ) {
                    fail!(1, "Duke multiplayer setup: {error}");
                }
            }
            if net.is_some() {
                let mode = match args.duke_game_mode {
                    0 => "DukeMatch",
                    1 => "co-op",
                    _ => "no-spawn",
                };
                eprintln!(
                    "room: Duke network settings — mode={mode}, friendly_fire={}",
                    args.duke_friendly_fire
                );
            }
            if let Some((volume, level)) = duke_campaign_coordinates(&args.map) {
                session.campaign = room_duke::DukeCampaignProgress::new(volume, level);
            }
            if args.give_all {
                session.player.gotweapon.fill(true);
                for weapon in 0..session.player.ammo.len() {
                    session.player.ammo[weapon] =
                        room_duke::weapons::max_ammo_amount(weapon as i16);
                }
                session.player.extra = 100;
                eprintln!("room: Duke give all → weapons + full ammo");
            }
            let sound_bank = duke_sound_bank(
                &grp_layers,
                session.script.as_ref(),
                audio_rate,
                args.con.as_ref().and_then(|con| con.parent()),
            );
            if !args.grp.is_empty() {
                eprintln!("room: mounted {} Duke add-on GRP(s)", args.grp.len());
            }
            eprintln!(
                "room: entered native Duke {} (difficulty {selected_skill}, internal skill {skill}); WASD move, mouse/Q/arrows turn, 1–0 weapons, Space jump, Ctrl/F/LMB fire, E tagged switches/doors (partial)",
                args.map,
            );
            let campaign_content = Some(DukeCampaignContent {
                def_music: read_duke_def_music(&grp_bytes, &mod_def_files),
                mod_music_assets,
                grp_layers: grp_bytes,
                con_override: con_bytes,
                con_directory: args.con.as_ref().map(|con| {
                    con.parent()
                        .unwrap_or_else(|| std::path::Path::new("."))
                        .to_path_buf()
                }),
                sample_rate: audio_rate,
            });
            let start_state = session.save_fsave().ok();
            (
                Some(session),
                Some(renderer),
                sound_bank,
                campaign_content,
                start_state,
            )
        } else {
            (None, None, SampleBank::new(), None, None)
        };
    let mut fonts = vfs
        .as_ref()
        .map(FontRegistry::load_doom_fonts)
        .unwrap_or_default();
    let mut patches = vfs.as_ref().map(PatchCache::from_vfs).unwrap_or_default();
    if let Some(renderer) = duke_renderer.as_ref() {
        install_duke_ui_art(renderer, &mut fonts, &mut patches);
    }
    let menu = play
        .as_ref()
        .filter(|g| g.state == GameState::Title)
        .map(title_menu);
    let (skyboxes, initial_sky, initial_cube) = load_sky_defs(vfs.as_ref(), &args.map);
    let gldefs = load_gldefs(vfs.as_ref());
    let modeldefs = load_modeldefs(vfs.as_ref());
    let voxeldefs = load_voxeldefs(vfs.as_ref());
    let (camera_tex_sizes, tex_anim) = load_animdefs(vfs.as_ref());
    let mut materials = MaterialCatalog::new();
    {
        let mut bm = std::collections::BTreeMap::new();
        for (k, v) in &gldefs.brightmaps {
            bm.insert(k.clone(), v.map.clone());
        }
        for (k, v) in &gldefs.materials {
            if let Some(map) = &v.brightmap {
                bm.insert(k.clone(), map.clone());
            }
        }
        materials.set_brightmaps(bm);
        let mut pbr = std::collections::BTreeMap::new();
        for (k, v) in &gldefs.materials {
            let maps = MaterialMaps {
                normal: v.normal.clone(),
                specular: v.specular.clone(),
                metallic: v.metallic.clone(),
                roughness: v.roughness.clone(),
                ao: v.ao.clone(),
                shader: v.shader.clone(),
            };
            if maps.normal.is_some()
                || maps.specular.is_some()
                || maps.metallic.is_some()
                || maps.roughness.is_some()
                || maps.ao.is_some()
                || maps.shader.is_some()
            {
                pbr.insert(k.clone(), maps);
            }
        }
        materials.set_material_maps(pbr);
    }
    crate::bench::log_stage(bench, "game data (DECORATE, ZScript, sounds, materials)");
    let language = load_language(vfs.as_ref());
    let mut app = ViewerApp {
        map_base: map.clone(),
        map,
        vfs,
        fonts,
        patches,
        menu,
        materials,
        decals: DecalStore::default(),
        particles: ParticleStore::default(),
        view_kick: (0.0, 0.0),
        hit_flash: std::collections::BTreeMap::new(),
        heat_haze_tics: 0,
        decaldef,
        script_sbar,
        sbarinfo_bar,
        althud_bar: None,
        sky_texture: initial_sky,
        sky_cubemap: initial_cube,
        skyboxes,
        gldefs,
        modeldefs,
        models: ModelCatalog::new(),
        voxeldefs,
        voxels: VoxelCatalog::new(),
        voxel_poses: Vec::new(),
        voxel_pose_actors: std::collections::BTreeSet::new(),
        model_pose_actors: std::collections::BTreeSet::new(),
        camera_tex_sizes,
        tex_anim,
        camera_tex_frame: 0,
        window: None,
        surface: None,
        config: None,
        gpu: None,
        pass: None,
        zone_pass: None,
        depth: None,
        stencil_aux: None,
        frame_color: None,
        scene_full: None,
        upscaler: None,
        render_scale: 1.0,
        dyn_scale: None,
        pacer: crate::frame_pacer::FramePacer::new(),
        frame_rate_hint: None,
        thermal: crate::frame_pacer::Thermal::Nominal,
        thermal_override: std::env::var("ROOM_TEST_THERMAL")
            .ok()
            .and_then(|v| crate::frame_pacer::Thermal::parse(&v)),
        thermal_checked: None,
        refresh_millihertz: None,
        portal_renderer: room_render::PortalRenderer::new(),
        post_ping: None,
        post_ldr: None,
        post_chain: None,
        hw_post: None,
        post_requests: std::collections::BTreeMap::new(),
        post_requests_dirty: false,
        mod_cvars: mod_cvars
            .iter()
            .map(|(name, _)| (name.clone(), String::new()))
            .collect(),
        render_settings: RenderSettings::default(),
        ephemeral_shaders: args.combat_demo,
        canvas_gpu: None,
        hud_sprites: None,
        scene_sprites: None,
        camera: Camera::default(),
        pending_screenshot: false,
        shot_after: args.shot_after,
        bench_frames: args.bench_frames,
        frame_stats: crate::bench::FrameStats::default(),
        quit: false,
        from_launcher: args.from_launcher,
        back_to_launcher: None,
        bench_result: None,
        gif_recorder: None,
        keys: Keys::default(),
        yaw: 0.0,
        pitch: 0.0,
        roll: 0.0,
        weapon_bob_prev: (0.0, 0.0),
        mouse_look: args.native_duke,
        ignore_warp_motion: false,
        unfocused_since: None,
        menu_cursor: (160.0, 100.0),
        last_frame: std::time::Instant::now(),
        lifecycle: crate::lifecycle::Lifecycle::default(),
        background_test_every: std::env::var("ROOM_TEST_BACKGROUND")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .filter(|&n| n > 0),
        background_test_last: 0,
        away_test: std::env::var("ROOM_TEST_AWAY")
            .ok()
            .and_then(|v| crate::netplay::parse_away_test(&v)),
        net_away: None,
        away_test_until: None,
        frames: 0,
        elapsed_ms: 0,
        title_anim_origin: 0,
        finale_anim_origin: 0,
        duke_finale_sound_frame: None,
        title_skip_ms: 0,
        demo_slot: "quick".into(),
        fps_window: std::time::Instant::now(),
        perf: crate::perf::FrameTimer::default(),
        precached_map: None,
        precached_weapons: std::collections::HashSet::new(),
        precached_weapon_name: String::new(),
        perf_panel: None,
        fps_count: 0,
        duke_fps: 0,
        automap: false,
        duke_view_key_down: false,
        duke_brightness_key_down: false,
        duke_help_page: 0,
        duke_help_key_down: false,
        duke_cheat_phase: 0,
        duke_cheat_buffer: String::new(),
        automap_scale: 0.35,
        duke_automap_seen: Vec::new(),
        duke_show_all_map: false,
        duke_coords_on: false,
        duke_rate_on: false,
        duke_debug_on: false,
        hud: HudState::doom_default(),
        play,
        duke_session,
        duke_start_state,
        duke_renderer,
        soft_world: None,
        duke_sound_bank,
        duke_campaign_content,
        duke_demo_recorder: None,
        duke_atomic_demo_recorder: None,
        duke_demo_playback: None,
        duke_atomic_demo_playback: None,
        duke_demo_map: duke_map_coordinates(&args.map),
        duke_music_selection: {
            let (volume, level) = duke_map_coordinates(&args.map);
            usize::from(volume) * 11 + usize::from(level)
        },
        duke_music_quote_tics: 0,
        duke_bonus_panel: None,
        duke_menu: None,
        duke_music_key_down: false,
        duke_rts_key_down: None,
        duke_environment_music_active: false,
        duke_last_save_slot: None,
        net,
        rollback_frames: std::collections::VecDeque::new(),
        rollback_world: None,
        rollback_swapped: false,
        rollback_sounds: std::collections::VecDeque::new(),
        local_seat,
        tic_accum: 0.0,
        cam_prev: None,
        cam_curr: None,
        sprite_prev: std::collections::HashMap::new(),
        sprite_extra_prev: std::collections::HashMap::new(),
        sprite_extra_curr: std::collections::HashMap::new(),
        sprite_curr: std::collections::HashMap::new(),
        messages: MessageQueue::new(),
        hud_message_log: std::collections::HashMap::new(),
        language,
        last_player_hp: 100,
        fixed_colormap: 0,
        mouse_yaw_accum: 0,
        duke_turn_residual: 0.0,
        duke_turn_held: 0,
        duke_horiz_residual: 0.0,
        mouse_pitch_accum: 0,
        console: {
            let mut con = Console::with_defaults();
            let fps = platform::current().default_max_fps();
            if fps > 0 {
                let _ = con.cvars.set("vid_maxfps", &fps.to_string());
            }
            for (name, value) in platform::current().default_cvars() {
                if let Err(e) = con.cvars.set(name, value) {
                    eprintln!("room: platform default {name}: {e}");
                }
            }
            // Mod settings (CVARINFO): console CVars, saved in room.ini like
            // any other, so `set colorworld_mode 2` / `--set` reach the mod.
            for (name, default) in &mod_cvars {
                if con.cvars.get(name).is_none() {
                    con.cvars.register(
                        name,
                        room_console::CvarType::String,
                        room_console::CvarValue::String(default.clone()),
                        room_console::CvarFlags::ARCHIVE,
                    );
                }
            }
            // Load user room.ini, optional --import-ini, then first gzdoom.ini.
            let cfg = dirs_room_ini();
            if cfg.is_file() {
                if let Ok(r) = room_console::load_ini_file(&mut con.cvars, &cfg) {
                    eprintln!("room: loaded {} ({} cvars)", cfg.display(), r.set);
                }
            }
            if let Some(path) = &args.import_ini {
                match room_console::load_ini_file(&mut con.cvars, path) {
                    Ok(r) => eprintln!(
                        "room: imported {} ({} cvars, {} binds)",
                        path.display(),
                        r.set,
                        r.binds.len()
                    ),
                    Err(e) => eprintln!("room: import-ini: {e}"),
                }
            } else if let Some(gz) = launcher::discover_gzdoom_ini().into_iter().next() {
                if let Ok(r) = room_console::load_ini_file(&mut con.cvars, &gz) {
                    eprintln!("room: imported {} ({} cvars)", gz.display(), r.set);
                }
            }
            // Combat demo: ignore archived shader prefs; start faithful / all FX off.
            // `--set NAME=VALUE` below can still enable effects for this run.
            if args.combat_demo {
                reset_shaders_to_defaults(&mut con.cvars);
                eprintln!("room: combat-demo — shader CVars reset (not saved on writeconfig)");
            }
            for spec in &args.set_cvars {
                let Some((name, value)) = spec.split_once('=') else {
                    eprintln!("room: --set expects NAME=VALUE, got `{spec}`");
                    continue;
                };
                let name = name.trim();
                let value = value.trim();
                if name.eq_ignore_ascii_case("r_quality") {
                    if let Ok(level) = value.parse::<i32>() {
                        con.cvars.apply_quality_preset(level);
                        eprintln!("room: r_quality={level}");
                        continue;
                    }
                }
                match con.cvars.set(name, value) {
                    Ok(()) => eprintln!("room: set {name}={value}"),
                    Err(e) => eprintln!("room: --set {name}: {e}"),
                }
            }
            for (key, cmd) in default_bind_pairs() {
                con.aliases.insert(format!("bind_{key}"), cmd.to_string());
            }
            con
        },
        console_open: false,
        console_line: String::new(),
        keyconf_binds: std::collections::BTreeMap::new(),
        double_binds: room_input::DoubleBindings::default(),
        double_tap: room_input::DoubleTapDetector::default(),
        start_instant: std::time::Instant::now(),
        pad_backend: gamepad::PadBackend::new(),
        pad: None,
        touch: room_input::touch::TouchState::new(),
        touch_layout: None,
        touch_overlay: crate::touch_overlay::TouchOverlay::default(),
        radar_cache: crate::radar::RadarCache::default(),
        touch_keys: Vec::new(),
        touch_analog: None,
        launcher: None,
        launcher_frame: Vec::new(),
        start_request: None,
        find_lan_request: false,
        touch_shown: false,
        touch_script: std::env::var("ROOM_TEST_TOUCH")
            .map(|v| touch_input::parse_test_touches(&v))
            .unwrap_or_default(),
        pad_prev_buttons: 0,
        pad_pressed_since_tic: 0,
        mod_buttons: ModButtonState::default(),
        weapon_impulses: WeaponImpulseState::default(),
        inv_cmd: 0,
        show_scores: false,
        net_save_cmd: None,
        audio,
    };
    if let Some(audio) = &app.audio {
        if app.duke_session.is_some() {
            if let Ok(mut shared) = audio.shared.lock() {
                shared.mixer.bank.merge_missing(&app.duke_sound_bank);
            }
        }
    }
    if let Some(audio) = &app.audio {
        if app.duke_session.is_some() {
            configure_duke_music(
                audio,
                args.mididevice,
                args.soundfont.as_deref(),
                app.duke_campaign_content.as_ref(),
            );
        } else {
            configure_music(
                audio,
                args.mididevice,
                args.soundfont.as_deref(),
                app.vfs.as_ref(),
            );
        }
    }
    if let Some(conf) = app.play.as_ref().and_then(|g| g.keyconf()) {
        for line in keyconf_alias_lines(conf) {
            let _ = app.console.exec(&line);
        }
        for (key, cmd) in keyconf_default_binds(conf) {
            app.keyconf_binds.insert(key.to_ascii_lowercase(), cmd);
        }
    }
    // UZDoom's inventory keys, where Room has no other use for them.
    for (key, cmd) in [
        ("pgup", "invprev"),
        ("pgdn", "invnext"),
        ("end", "invuse"),
        ("\\", "+showscores"),
    ] {
        app.keyconf_binds
            .entry(key.to_owned())
            .or_insert_with(|| cmd.to_owned());
    }
    if app.net.is_none() {
        let local_name = if !args.name.is_empty() {
            args.name.clone()
        } else {
            app.console
                .cvars
                .get("name")
                .and_then(|v| {
                    if let room_console::CvarValue::String(s) = v {
                        (!s.is_empty()).then(|| s.clone())
                    } else {
                        None
                    }
                })
                .unwrap_or_default()
        };
        if !local_name.is_empty() {
            if let Some(game) = app.play.as_mut() {
                game.set_player_display_name(usize::from(app.local_seat), &local_name);
            }
        }
    }
    if app
        .play
        .as_ref()
        .is_some_and(|g| g.state == GameState::Title)
    {
        app.start_title_music();
    } else if app
        .play
        .as_ref()
        .is_some_and(|g| g.state == GameState::Level)
    {
        app.start_level_music();
        app.set_play_look(true);
    } else if app.duke_session.is_some() {
        app.start_duke_level_music();
        app.set_play_look(true);
    }
    if args.launcher_screen {
        crate::startlog::step("build: scanning the game library");
        let library = crate::library::scan(&platform.library_dirs());
        crate::startlog::step(format_args!(
            "build: library has {} game(s), {} mod(s), {} unreadable file(s)",
            library.games.len(),
            library.mods.len(),
            library.problems.len()
        ));
        app.launcher = Some(crate::launcher_screen::LauncherScreen::new(
            library,
            platform.import_hint(),
        ));
    }
    // The console's values (room.ini, `--set`) of the mods' CVARINFO
    // settings, before the first tic.
    app.sync_mod_cvars();
    crate::startlog::step("build: done");
    Ok(Some(app))
}

/// The event loop's handler: runs the current app, and swaps in a new one
/// when the launcher starts a game (docs/41 M4).
struct Shell {
    app: ViewerApp,
    /// `--set`, `--shot-after`, music options: carried into launched games.
    base: AppConfig,
    /// Frame count when the launcher's "Loading …" notice was set: the game
    /// is built (which blocks) only after a later frame showed it.
    loading_shown_at: Option<u64>,
    /// A co-op lobby started from the launcher, running on its own thread
    /// so the screen stays live (docs/41 M7).
    lobby: Option<PendingLobby>,
    /// A finished lobby waiting for its game to be built.
    ready_net: Option<NetReady>,
    /// A local-network search started from the launcher.
    lan_search: Option<std::thread::JoinHandle<Vec<crate::lan::LanGame>>>,
}

/// A launcher lobby in progress.
struct PendingLobby {
    handle: std::thread::JoinHandle<Result<NetReady, (i32, String)>>,
    config: AppConfig,
    /// Shown while waiting ("at 192.168.1.20:5029" / "Joining …").
    hosting_at: Option<String>,
}

impl Shell {
    /// The game asked to go back to the launcher: build it and hand it
    /// the window, as a launcher start does the other way round.
    fn return_to_launcher(&mut self) {
        let Some(message) = self.app.back_to_launcher.take() else {
            return;
        };
        let config = AppConfig {
            launcher_screen: true,
            set_cvars: self.base.set_cvars.clone(),
            mididevice: self.base.mididevice,
            soundfont: self.base.soundfont.clone(),
            ..AppConfig::default()
        };
        // One audio stream at a time.
        self.app.audio = None;
        match build_app(config, None) {
            Ok(Some(mut next)) => {
                next.adopt_display(&mut self.app);
                if let Some(launcher) = next.launcher.as_mut() {
                    launcher.notice = message;
                }
                let old = std::mem::replace(&mut self.app, next);
                drop(old);
                crate::startlog::step("launcher: back on screen");
            }
            Ok(None) => {}
            Err(e) => eprintln!("room: back to the launcher: {}", e.message),
        }
    }

    fn start_requested_game(&mut self) {
        self.return_to_launcher();
        if self.app.start_request.is_none() {
            return;
        }
        // Loading blocks this thread (Brutal Doom takes seconds on a phone):
        // first let one frame show the launcher's "Loading …" notice.
        let notice = self
            .app
            .launcher
            .as_ref()
            .is_some_and(|l| l.notice.is_some());
        if notice && self.app.window.is_some() && self.app.lifecycle.is_running() {
            match self.loading_shown_at {
                None => {
                    self.loading_shown_at = Some(self.app.frames);
                    return;
                }
                Some(at) if self.app.frames <= at => return,
                Some(_) => {}
            }
        }
        self.loading_shown_at = None;
        let Some(mut config) = self.app.start_request.take() else {
            return;
        };
        // Co-op from the launcher: wait for the other players on a thread,
        // then come back here with the lobby done (`ready_net`).
        let co_op = config.host.is_some() || config.join.is_some();
        if co_op && self.ready_net.is_none() && self.app.launcher.is_some() {
            self.start_lobby(config);
            return;
        }
        config.set_cvars.extend(self.base.set_cvars.iter().cloned());
        config.shot_after = config.shot_after.or(self.base.shot_after);
        config.bench_frames = config.bench_frames.or(self.base.bench_frames);
        config.mididevice = self.base.mididevice;
        config.soundfont = config.soundfont.or_else(|| self.base.soundfont.clone());
        crate::startlog::step(format_args!(
            "launcher: starting {}",
            config
                .iwad
                .as_ref()
                .map_or_else(|| "native Duke".to_owned(), |p| p.display().to_string())
        ));
        // One audio stream at a time: the launcher's goes before the game's opens.
        self.app.audio = None;
        match build_app(config, self.ready_net.take()) {
            Ok(Some(mut next)) => {
                crate::startlog::step("launcher: game built; handing over the window");
                next.adopt_display(&mut self.app);
                crate::startlog::step("launcher: game on screen");
                let old = std::mem::replace(&mut self.app, next);
                drop(old);
            }
            Ok(None) => {}
            Err(e) => {
                // Stay in the launcher and show why.
                if let Some(launcher) = self.app.launcher.as_mut() {
                    launcher.notice = None;
                    launcher.error = Some(e.message);
                }
            }
        }
    }
}

impl Shell {
    /// Start a launcher co-op lobby on its own thread and show the waiting page.
    fn start_lobby(&mut self, config: AppConfig) {
        let hosting_at = config.host.as_ref().map(|port| {
            crate::lan::local_ip()
                .map_or_else(|| format!("port {port}"), |ip| format!("{ip}:{port}"))
        });
        let text = match (&hosting_at, &config.join) {
            (Some(at), _) => format!("Waiting for players at {at}"),
            (None, Some(addr)) => format!("Joining {addr} …"),
            (None, None) => String::new(),
        };
        crate::startlog::step(format_args!("lobby: {text}"));
        if let Some(launcher) = self.app.launcher.as_mut() {
            launcher.show_waiting(text);
        }
        let args = config.clone();
        let handle = std::thread::Builder::new()
            .name("room-lobby".into())
            .spawn(move || run_lobby(&args));
        match handle {
            Ok(handle) => {
                self.lobby = Some(PendingLobby {
                    handle,
                    config,
                    hosting_at,
                });
            }
            Err(e) => {
                if let Some(launcher) = self.app.launcher.as_mut() {
                    launcher.lobby_ended(Some(format!("lobby thread: {e}")));
                }
            }
        }
    }

    /// Follow the lobby and network-search threads (once per loop turn).
    fn poll_launcher_jobs(&mut self) {
        if std::mem::take(&mut self.app.find_lan_request) && self.lan_search.is_none() {
            let targets = [crate::lan::broadcast_target()];
            self.lan_search = std::thread::Builder::new()
                .name("room-lan".into())
                .spawn(move || {
                    crate::lan::discover(&targets, std::time::Duration::from_millis(1500))
                        .unwrap_or_default()
                })
                .ok();
        }
        if self.lan_search.as_ref().is_some_and(|h| h.is_finished()) {
            if let Some(handle) = self.lan_search.take() {
                let games = handle.join().unwrap_or_default();
                crate::startlog::step(format_args!("lan: {} game(s) found", games.len()));
                if let Some(launcher) = self.app.launcher.as_mut() {
                    launcher.lan_searching = false;
                    launcher.lan_games = Some(games);
                }
            }
        }
        let Some(lobby) = self.lobby.as_ref() else {
            return;
        };
        if !lobby.handle.is_finished() {
            if let (Some(at), Some((seats, max))) = (&lobby.hosting_at, netplay::lobby_progress()) {
                if let Some(launcher) = self.app.launcher.as_mut() {
                    launcher.waiting = Some(format!("Waiting for players at {at} ({seats}/{max})"));
                }
            }
            return;
        }
        let Some(lobby) = self.lobby.take() else {
            return;
        };
        let result = lobby
            .handle
            .join()
            .unwrap_or_else(|_| Err((1, "lobby thread stopped".into())));
        match result {
            Ok(ready) => {
                crate::startlog::step(format_args!(
                    "lobby: ready, seat {} of {}",
                    ready.net.local.0 + 1,
                    ready.net.peer_count
                ));
                self.ready_net = Some(ready);
                if let Some(launcher) = self.app.launcher.as_mut() {
                    launcher.notice = Some("Loading the co-op game …".into());
                }
                self.app.start_request = Some(lobby.config);
            }
            Err((_, message)) => {
                crate::startlog::step(format_args!("lobby: ended: {message}"));
                let cancelled = message.ends_with(netplay::LOBBY_CANCELLED);
                if let Some(launcher) = self.app.launcher.as_mut() {
                    launcher.lobby_ended((!cancelled).then_some(message));
                }
            }
        }
    }
}

impl ApplicationHandler for Shell {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        self.app.resumed(event_loop);
    }

    fn suspended(&mut self, event_loop: &ActiveEventLoop) {
        self.app.suspended(event_loop);
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        self.app.window_event(event_loop, id, event);
        self.start_requested_game();
    }

    fn device_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        id: winit::event::DeviceId,
        event: DeviceEvent,
    ) {
        self.app.device_event(event_loop, id, event);
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        self.app.about_to_wait(event_loop);
        self.poll_launcher_jobs();
        self.start_requested_game();
    }
}

/// `showendoom`: print the IWAD/PWAD ENDOOM (ENDTEXT, ENDSTRF) quit screen
/// to the terminal in ANSI colour after the window closes.
/// CON dialect for a `--con` override: EDuke32 when its folder is a mod with
/// DEF files (as the browser does for mounted mod files), else Atomic.
fn duke_con_dialect(con_directory: Option<&std::path::Path>) -> room_duke::ConDialect {
    fn has_def(dir: &std::path::Path, depth: u8) -> bool {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return false;
        };
        entries.flatten().any(|entry| {
            let path = entry.path();
            if path.is_dir() {
                depth > 0 && has_def(&path, depth - 1)
            } else {
                path.extension()
                    .and_then(std::ffi::OsStr::to_str)
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("def"))
            }
        })
    }
    match con_directory {
        Some(dir) if has_def(dir, 2) => room_duke::ConDialect::EDuke32,
        _ => room_duke::ConDialect::Atomic,
    }
}

fn read_duke_mod_render_assets(
    root: &std::path::Path,
) -> Result<
    (
        Vec<(String, Vec<u8>)>,
        Vec<(String, Vec<u8>)>,
        Vec<(String, Vec<u8>)>,
        Vec<(String, Vec<u8>)>,
    ),
    String,
> {
    fn walk(
        root: &std::path::Path,
        current: &std::path::Path,
        defs: &mut Vec<(String, Vec<u8>)>,
        images: &mut Vec<(String, Vec<u8>)>,
        mhks: &mut Vec<(String, Vec<u8>)>,
        music: &mut Vec<(String, Vec<u8>)>,
    ) -> Result<(), String> {
        let entries = std::fs::read_dir(current)
            .map_err(|error| format!("{}: {error}", current.display()))?;
        let mut entries = entries
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| format!("{}: {error}", current.display()))?;
        entries.sort_by_key(std::fs::DirEntry::file_name);
        for entry in entries {
            let path = entry.path();
            let kind = entry
                .file_type()
                .map_err(|error| format!("{}: {error}", path.display()))?;
            if kind.is_dir() {
                walk(root, &path, defs, images, mhks, music)?;
                continue;
            }
            if !kind.is_file() {
                continue;
            }
            let extension = path
                .extension()
                .and_then(std::ffi::OsStr::to_str)
                .map(str::to_ascii_lowercase);
            let target = match extension.as_deref() {
                Some("def") => Some(&mut *defs),
                // Everything a DEF references by path travels with the tile
                // assets: images, `voxel` KVX models, `basepalette` .pal and
                // `blendtable` .raw files (same set as the web bundle).
                Some("png" | "bmp" | "jpg" | "jpeg" | "kvx" | "pal" | "raw") => Some(&mut *images),
                Some("mhk") => Some(&mut *mhks),
                Some("ogg" | "wav" | "flac" | "mp3" | "mid" | "midi" | "mus") => Some(&mut *music),
                _ => None,
            };
            let Some(target) = target else {
                continue;
            };
            let relative = path
                .strip_prefix(root)
                .map_err(|error| error.to_string())?
                .to_string_lossy()
                .replace('\\', "/");
            let bytes =
                std::fs::read(&path).map_err(|error| format!("{}: {error}", path.display()))?;
            target.push((relative, bytes));
        }
        Ok(())
    }

    let mut defs = Vec::new();
    let mut images = Vec::new();
    let mut mhks = Vec::new();
    let mut music = Vec::new();
    walk(root, root, &mut defs, &mut images, &mut mhks, &mut music)?;
    Ok((defs, images, mhks, music))
}

#[derive(Clone)]
struct DukeDefMusicTrack {
    source_def: String,
    id: String,
    file: String,
}

fn read_duke_def_music(
    grp_layers: &[Vec<u8>],
    loose_defs: &[(String, Vec<u8>)],
) -> Vec<DukeDefMusicTrack> {
    fn append_document(source_name: &str, bytes: &[u8], tracks: &mut Vec<DukeDefMusicTrack>) {
        let Ok(contents) = std::str::from_utf8(bytes) else {
            return;
        };
        let Ok(document) = room_build::def::DefDocument::parse(contents) else {
            eprintln!("room: skip invalid Duke DEF music source {source_name}");
            return;
        };
        match document.music_tracks() {
            Ok(def_tracks) => {
                tracks.extend(def_tracks.into_iter().map(|track| DukeDefMusicTrack {
                    source_def: source_name.to_owned(),
                    id: track.id,
                    file: track.file,
                }))
            }
            Err(error) => eprintln!("room: skip Duke DEF music in {source_name}: {error}"),
        }
    }

    let mut tracks = Vec::new();
    if let Ok(grp) = room_build::grp::GrpStack::parse_layers(
        &grp_layers.iter().map(Vec::as_slice).collect::<Vec<_>>(),
    ) {
        let mut seen = std::collections::BTreeSet::new();
        let names: Vec<String> = grp
            .names()
            .filter(|name| name.to_ascii_uppercase().ends_with(".DEF"))
            .filter_map(|name| {
                let owned = name.to_owned();
                seen.insert(owned.clone()).then_some(owned)
            })
            .collect();
        for name in names {
            if let Some(bytes) = grp.get(&name) {
                append_document(&name, bytes, &mut tracks);
            }
        }
    }
    for (name, bytes) in loose_defs {
        append_document(name, bytes, &mut tracks);
    }
    tracks
}

fn print_endoom(app: &ViewerApp) {
    let on = matches!(
        app.console.cvars.get("showendoom"),
        Some(room_console::CvarValue::Bool(true)) | Some(room_console::CvarValue::Int(1..))
    );
    if !on {
        return;
    }
    let Some(vfs) = app.vfs.as_ref() else {
        return;
    };
    for name in room_ui::endoom::ENDOOM_LUMPS {
        if let Some(lump) = vfs.find_any_ns(name)
            && let Ok(data) = vfs.read(lump)
            && let Some(text) = room_ui::endoom::endoom_to_ansi(data.as_ref())
        {
            print!("{text}");
            return;
        }
    }
}

fn configure_music(
    audio: &AudioOut,
    backend: MidiBackend,
    soundfont: Option<&std::path::Path>,
    vfs: Option<&Vfs>,
) {
    let sf = load_first_soundfont(soundfont);
    let softfont_ok = sf.is_some();
    let genmidi = vfs.and_then(load_genmidi);
    let genmidi_ok = genmidi.is_some();
    let Some(device) = resolve_midi_device(backend, softfont_ok, genmidi_ok) else {
        eprintln!(
            "room: no music synth (pass --soundfont, or --iwad with GENMIDI, or --mididevice opl)"
        );
        return;
    };
    let Ok(mut shared) = audio.shared.lock() else {
        return;
    };
    if let Some((path, font)) = sf {
        shared.music.set_softfont(font);
        if device == room_music::MidiDevice::SoftFont {
            eprintln!("room: soundfont {}", path.display());
        }
    }
    if let Some(bank) = genmidi {
        shared.music.set_genmidi(bank);
        if device == room_music::MidiDevice::Opl {
            eprintln!("room: GENMIDI → OPL MIDI device");
        }
    }
    shared.music.set_mididevice(device);
    eprintln!("room: mididevice {backend} → {device:?}");
}

/// Configure the shared sequencer against the native Duke GRP instead of the
/// Doom VFS. Atomic MIDI files use the same synth backends; some GRP variants
/// also carry the OPL `GENMIDI` bank.
fn configure_duke_music(
    audio: &AudioOut,
    backend: MidiBackend,
    soundfont: Option<&std::path::Path>,
    content: Option<&DukeCampaignContent>,
) {
    let sf = load_first_soundfont(soundfont);
    let genmidi = content.and_then(|content| {
        let layers: Vec<&[u8]> = content.grp_layers.iter().map(Vec::as_slice).collect();
        let grp = room_build::grp::GrpStack::parse_layers(&layers).ok()?;
        let bytes = grp.get("GENMIDI")?;
        match room_music::GenMidiBank::parse(bytes) {
            Ok(bank) => Some(bank),
            Err(error) => {
                eprintln!("room: Duke GENMIDI: {error}");
                None
            }
        }
    });
    let Some(device) = resolve_midi_device(backend, sf.is_some(), genmidi.is_some()) else {
        eprintln!("room: no Duke music synth (pass --soundfont; OPL needs GRP GENMIDI)");
        return;
    };
    let Ok(mut shared) = audio.shared.lock() else {
        return;
    };
    if let Some((path, font)) = sf {
        shared.music.set_softfont(font);
        if device == room_music::MidiDevice::SoftFont {
            eprintln!("room: soundfont {}", path.display());
        }
    }
    if let Some(bank) = genmidi {
        shared.music.set_genmidi(bank);
        if device == room_music::MidiDevice::Opl {
            eprintln!("room: Duke GENMIDI → OPL MIDI device");
        }
    }
    shared.music.set_mididevice(device);
    eprintln!("room: Duke mididevice {backend} → {device:?}");
}

/// Resolve external includes relative to a user-supplied root CON file.
struct DukeConIncludes {
    directory: PathBuf,
}

/// Reusable native campaign assets retained for Atomic map-to-map transitions.
struct DukeCampaignContent {
    grp_layers: Vec<Vec<u8>>,
    con_override: Option<Vec<u8>>,
    con_directory: Option<PathBuf>,
    sample_rate: u32,
    def_music: Vec<DukeDefMusicTrack>,
    mod_music_assets: Vec<(String, Vec<u8>)>,
}

impl DukeCampaignContent {
    fn map_bytes(&self, map: &str) -> Option<Vec<u8>> {
        let name = map.trim().to_ascii_uppercase();
        let base = name.strip_suffix(".MAP").unwrap_or(&name);
        let name = format!("{base}.MAP");
        let layers: Vec<&[u8]> = self.grp_layers.iter().map(Vec::as_slice).collect();
        room_build::grp::GrpStack::parse_layers(&layers)
            .ok()?
            .get(&name)
            .map(<[u8]>::to_vec)
    }

    fn music_asset<'a>(
        &'a self,
        grp: &room_build::grp::GrpStack<'a>,
        track: &DukeDefMusicTrack,
    ) -> Option<&'a [u8]> {
        let file = normalize_duke_resource_path(&track.file);
        let source = normalize_duke_resource_path(&track.source_def);
        let mut candidates = vec![file.clone()];
        if let Some((parent, _)) = source.rsplit_once('/') {
            candidates.push(format!("{parent}/{file}"));
        }
        for candidate in &candidates {
            if let Some((_, bytes)) = self
                .mod_music_assets
                .iter()
                .find(|(name, _)| normalize_duke_resource_path(name) == *candidate)
            {
                return Some(bytes);
            }
        }
        let basename = file.rsplit('/').next().unwrap_or(&file);
        grp.get(&track.file).or_else(|| grp.get(basename))
    }

    fn load_level_at_skill(
        &self,
        volume: u8,
        level: u8,
        skill: u8,
    ) -> Result<room_duke::DukeSession, String> {
        let map = format!("E{}L{}", volume + 1, level + 1);
        let layers: Vec<&[u8]> = self.grp_layers.iter().map(Vec::as_slice).collect();
        let mut includes = self
            .con_directory
            .as_ref()
            .map(|directory| DukeConIncludes {
                directory: directory.clone(),
            });
        room_duke::session_from_grp_layers_with_con_and_includes_dialect(
            &layers,
            &map,
            skill,
            self.con_override.as_deref(),
            includes
                .as_mut()
                .map(|resolver| resolver as &mut dyn room_duke::con::IncludeResolver),
            duke_con_dialect(
                self.con_override
                    .as_ref()
                    .and(self.con_directory.as_deref()),
            ),
        )
    }

    fn sound_bank(
        &self,
        script: Option<&room_duke::con::ScriptImage>,
        sample_rate: u32,
    ) -> SampleBank {
        let layers: Vec<&[u8]> = self.grp_layers.iter().map(Vec::as_slice).collect();
        duke_sound_bank(&layers, script, sample_rate, self.con_directory.as_deref())
    }
}

fn normalize_duke_resource_path(path: &str) -> String {
    path.replace('\\', "/")
        .trim_start_matches("./")
        .to_ascii_lowercase()
}

impl room_duke::con::IncludeResolver for DukeConIncludes {
    fn read_include(&mut self, name: &str) -> Option<Vec<u8>> {
        let normalized = name.replace('\\', "/");
        let requested = std::path::Path::new(&normalized);
        let candidates = if requested.is_absolute() {
            vec![requested.to_path_buf()]
        } else {
            vec![
                self.directory.join(requested),
                self.directory.join(normalized.to_ascii_uppercase()),
                self.directory.join(normalized.to_ascii_lowercase()),
            ]
        };
        candidates
            .into_iter()
            .find_map(|path| std::fs::read(path).ok())
    }
}

/// Decode native Duke CON `definesound` assets into the shared mixer bank.
/// GRP content wins; an external mod directory supplies loose WAV/OGG/VOC.
fn duke_sound_bank(
    grp_layers: &[&[u8]],
    script: Option<&room_duke::con::ScriptImage>,
    sample_rate: u32,
    resource_directory: Option<&std::path::Path>,
) -> SampleBank {
    let mut bank = SampleBank::new();
    let Ok(grp) = room_build::grp::GrpStack::parse_layers(grp_layers) else {
        return bank;
    };
    if let Some(script) = script {
        for (id, definition) in script.sounds.iter().enumerate() {
            let Some(definition) = definition else {
                continue;
            };
            let decoded = grp
                .get(&definition.file)
                .and_then(|bytes| room_formats::decode_sound(bytes).ok())
                .or_else(|| {
                    let mut resolver = DukeConIncludes {
                        directory: resource_directory?.to_path_buf(),
                    };
                    let bytes = room_duke::con::IncludeResolver::read_include(
                        &mut resolver,
                        &definition.file,
                    )?;
                    room_formats::decode_sound(&bytes).ok()
                });
            let Some(decoded) = decoded else {
                continue;
            };
            insert_duke_sample(
                &mut bank,
                SoundId::from_raw(id as u32),
                decoded,
                sample_rate,
            );
        }
    }
    if let Some(bytes) = grp.get("DUKE.RTS")
        && let Ok(rts) = room_build::DukeRts::parse(bytes)
    {
        for index in 0..rts.len() {
            let Some(bytes) = rts.sound(index) else {
                continue;
            };
            let Ok(decoded) = room_formats::decode_sound(bytes) else {
                continue;
            };
            insert_duke_sample(
                &mut bank,
                SoundId::from_raw(DUKE_RTS_SOUND_BASE + index as u32),
                decoded,
                sample_rate,
            );
        }
    }
    bank
}

fn insert_duke_sample(
    bank: &mut SampleBank,
    id: SoundId,
    decoded: room_formats::DecodedSound,
    sample_rate: u32,
) {
    let channels = usize::from(decoded.channels.max(1));
    let mono: Vec<f32> = decoded
        .samples
        .chunks_exact(channels)
        .map(|frame| frame.iter().sum::<f32>() / channels as f32)
        .collect();
    bank.insert_resampled(id, &mono, decoded.rate.max(1), sample_rate.max(1));
}

fn dirs_room_ini() -> PathBuf {
    platform::current().config_file()
}

/// Slot name for a Room `.fsave` (`quick` when empty or punctured).
fn save_slot_name(raw: Option<&str>) -> String {
    let raw = raw.unwrap_or("quick");
    let slot: String = raw
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'))
        .take(32)
        .collect();
    if slot.is_empty() {
        "quick".into()
    } else {
        slot
    }
}

fn dirs_room_save(slot: &str) -> PathBuf {
    let file = format!("{slot}.fsave");
    match platform::current().saves_dir() {
        Some(dir) => dir.join(file),
        None => PathBuf::from(file),
    }
}

fn read_play_save_bytes(slot: &str) -> Result<(PathBuf, Vec<u8>), String> {
    let fsave = dirs_room_save(slot);
    if let Ok(b) = std::fs::read(&fsave) {
        return Ok((fsave, b));
    }
    let sav = fsave.with_extension("sav");
    std::fs::read(&sav)
        .map(|b| (sav, b))
        .map_err(|e| e.to_string())
}

/// Parse Atomic campaign coordinates from the currently selected Duke map.
/// Non-campaign names and standalone maps use episode/map zero metadata.
fn duke_map_coordinates(map: &str) -> (u8, u8) {
    duke_campaign_coordinates(map).unwrap_or((0, 0))
}

fn duke_campaign_coordinates(map: &str) -> Option<(u8, u8)> {
    let bytes = map.as_bytes();
    if !(4..=5).contains(&bytes.len())
        || !bytes[0].eq_ignore_ascii_case(&b'E')
        || !bytes[2].eq_ignore_ascii_case(&b'L')
        || !(b'1'..=b'4').contains(&bytes[1])
    {
        return None;
    }
    let level = map.get(3..)?.parse::<u8>().ok()?;
    (1..=11)
        .contains(&level)
        .then_some((bytes[1] - b'1', level - 1))
}

fn duke_audio_volume(cvars: &room_console::CvarStore, name: &str) -> f32 {
    match cvars.get(name) {
        Some(room_console::CvarValue::Int(value)) => (*value).clamp(0, 255) as f32 / 255.0,
        _ => 1.0,
    }
}

fn duke_reverb_zone_level(
    session: Option<&room_duke::DukeSession>,
    _listener: (i32, i32, i32, i16),
) -> i32 {
    session.map_or(0, |session| i32::from(session.duke_reverb_level))
}

fn duke_sound_visibility_geometry(
    session: Option<&room_duke::DukeSession>,
) -> Option<(
    Vec<room_build_engine::geom::SectorZ>,
    Vec<room_build_engine::geom::WallXY>,
)> {
    let world = session?.world.as_ref()?;
    let sectors = world
        .sectors
        .iter()
        .map(|sector| room_build_engine::geom::SectorZ {
            wallptr: sector.wallptr,
            wallnum: sector.wallnum,
            ceilingz: sector.ceilingz,
            floorz: sector.floorz,
            ceilingstat: sector.ceilingstat,
            floorstat: sector.floorstat,
            ceilingheinum: sector.ceilingheinum,
            floorheinum: sector.floorheinum,
        })
        .collect();
    let walls = world
        .walls
        .iter()
        .map(|wall| room_build_engine::geom::WallXY {
            x: wall.x,
            y: wall.y,
            point2: wall.point2,
            nextsector: wall.nextsector,
            cstat: wall.cstat,
        })
        .collect();
    Some((sectors, walls))
}

fn duke_sound_is_occluded(
    session: Option<&room_duke::DukeSession>,
    event: &room_duke::DukeSoundEvent,
    listener: (i32, i32, i32, i16),
    sectors: &[room_build_engine::geom::SectorZ],
    walls: &[room_build_engine::geom::WallXY],
) -> bool {
    if event.global || event.ambience {
        return false;
    }
    let Some(session) = session else {
        return false;
    };
    let listener_sector = session.player.cursectnum;
    let source_sector = event
        .source_id
        .and_then(|id| usize::try_from(id).ok())
        .and_then(|index| session.world.as_ref()?.sprites.get(index))
        .map_or(listener_sector, |sprite| sprite.sectnum);
    let eye_offset = 24_i32 << 8;
    !room_build_engine::geom::cansee(
        listener.0,
        listener.1,
        listener.2.wrapping_sub(eye_offset),
        listener_sector,
        event.position[0],
        event.position[1],
        event.position[2].wrapping_sub(eye_offset),
        source_sector,
        sectors,
        walls,
    )
}

/// Atomic tile bases for the menu (`menutext`) and message (`gametext`) fonts.
const DUKE_BIGALPHANUM: i16 = 2940;
const DUKE_STARTALPHANUM: i16 = 2822;
const DUKE_SPINNINGNUKEICON: i16 = 2813;

/// Register Duke's own ART fonts and menu cursor in the shared UI caches, so
/// the generic list menus and quotes render with Duke's look instead of
/// missing Doom `STCFN`/`DBIGFONT` lumps (drawn as grey boxes).
fn install_duke_ui_art(
    renderer: &room_duke::DukeSoftRenderer,
    fonts: &mut FontRegistry,
    patches: &mut PatchCache,
) {
    let palette = renderer.ui_palette_rgb();
    // Fon2 reserves index 0 for transparency: shift Duke indices up by one.
    let mut fon_palette = vec![[0u8, 0, 0]];
    fon_palette.extend_from_slice(&palette[..255]);
    let glyph = |pic: i16| {
        renderer
            .tile_indices(pic)
            .map(|(w, h, idx)| room_formats::FontGlyph {
                width: w,
                height: h,
                indices: idx
                    .into_iter()
                    .map(|i| {
                        if i == room_build::art::TRANSPARENT {
                            0
                        } else {
                            i + 1
                        }
                    })
                    .collect(),
            })
    };
    let make_font = |map: &dyn Fn(u8) -> Option<i16>, kerning: i16| {
        let glyphs: Vec<Option<room_formats::FontGlyph>> =
            (32u8..=126).map(|c| map(c).and_then(&glyph)).collect();
        let height = glyphs.iter().flatten().map(|g| g.height).max().unwrap_or(0);
        (height > 0).then(|| {
            room_ui::SoftFont::Fon2(room_formats::Fon2 {
                font_height: height,
                first_char: 32,
                last_char: 126,
                constant_width: false,
                kerning,
                palette: fon_palette.clone(),
                glyphs,
            })
        })
    };
    // Chocolate `menutext`: A–Z (lowercase folded), digits, a few symbols.
    let big = |c: u8| -> Option<i16> {
        let c = c.to_ascii_uppercase();
        match c {
            b'A'..=b'Z' => Some(DUKE_BIGALPHANUM + i16::from(c - b'A')),
            b'0'..=b'9' => Some(DUKE_BIGALPHANUM - 10 + i16::from(c - b'0')),
            b'-' => Some(DUKE_BIGALPHANUM - 11),
            b'.' => Some(3002),
            b',' => Some(3003),
            b'!' => Some(3004),
            b'?' => Some(3005),
            b';' | b':' => Some(3006),
            b'\'' => Some(3022),
            _ => None,
        }
    };
    // Chocolate `gametext`: STARTALPHANUM + (c - '!').
    let small = |c: u8| -> Option<i16> {
        (b'!'..=b'~')
            .contains(&c)
            .then(|| DUKE_STARTALPHANUM + i16::from(c - b'!'))
    };
    if let Some(font) = make_font(&big, 0) {
        fonts.insert(room_ui::FontId::BIG, font);
    }
    if let Some(font) = make_font(&small, 1) {
        fonts.insert(room_ui::FontId::SMALL, font);
    }
    // Dense menus draw `STCFNxxx` patches directly: alias them to `gametext`
    // glyphs. The list-menu cursor (`M_SKULL1/2`) becomes the spinning nuke.
    let mut art: Vec<(String, i16)> = vec![
        ("M_SKULL1".into(), DUKE_SPINNINGNUKEICON),
        ("M_SKULL2".into(), DUKE_SPINNINGNUKEICON + 3),
    ];
    for c in b'!'..=b'_' {
        if let Some(pic) = small(c) {
            art.push((format!("STCFN{c:03}"), pic));
        }
    }
    for (name, pic) in art {
        let name = name.as_str();
        let Some((w, h, idx)) = renderer.tile_indices(pic) else {
            continue;
        };
        let mut rgba = Vec::with_capacity(idx.len() * 4);
        let mut alpha = Vec::with_capacity(idx.len());
        for &i in &idx {
            let solid = i != room_build::art::TRANSPARENT;
            let [r, g, b] = palette[usize::from(i)];
            rgba.extend_from_slice(&[r, g, b, if solid { 255 } else { 0 }]);
            alpha.push(if solid { 255 } else { 0 });
        }
        patches.insert(
            name,
            room_ui::SoftPatch {
                width: w,
                height: h,
                left_offset: 0,
                top_offset: 0,
                indices: idx,
                alpha,
                rgba: Some(rgba),
            },
        );
    }
}

fn duke_audio_menu(cvars: &room_console::CvarStore) -> ListMenu {
    let is_on = |name: &str| !matches!(cvars.get(name), Some(room_console::CvarValue::Bool(false)));
    let autoaim = match cvars.get("duke_autoaim") {
        Some(room_console::CvarValue::Int(value)) if *value == 1 => "BULLET ONLY",
        _ => "FULL",
    };
    let volume = |name: &str| match cvars.get(name) {
        Some(room_console::CvarValue::Int(value)) => ((*value).clamp(0, 255) * 100) / 255,
        _ => 100,
    };
    ListMenu {
        title: "SETUP SOUNDS".into(),
        items: vec![
            MenuItem::text(
                format!("SOUND: {}", if is_on("duke_sound") { "ON" } else { "OFF" }),
                "duke_audio_sound",
            ),
            MenuItem::text(
                format!("MUSIC: {}", if is_on("duke_music") { "ON" } else { "OFF" }),
                "duke_audio_music",
            ),
            MenuItem::text(
                format!("SOUND VOLUME: {}%", volume("duke_sfx_volume")),
                "duke_audio_sfx_volume",
            ),
            MenuItem::text(
                format!("MUSIC VOLUME: {}%", volume("duke_music_volume")),
                "duke_audio_music_volume",
            ),
            MenuItem::text(
                format!(
                    "DUKE TALK: {}",
                    if is_on("duke_talk") { "ON" } else { "OFF" }
                ),
                "duke_audio_talk",
            ),
            MenuItem::text(
                format!(
                    "AMBIENCE: {}",
                    if is_on("duke_ambience") { "ON" } else { "OFF" }
                ),
                "duke_audio_ambience",
            ),
            MenuItem::text(
                format!(
                    "FLIP STEREO: {}",
                    if is_on("duke_reverse_stereo") {
                        "ON"
                    } else {
                        "OFF"
                    }
                ),
                "duke_audio_reverse_stereo",
            ),
            MenuItem::text(format!("AUTO-AIM: {autoaim}"), "duke_audio_autoaim"),
            MenuItem::text("BACK", "duke_audio_close"),
        ],
        selected: 0,
        scroll: 0,
        visible_rows: 0,
        origin: Vec2::new(84.0, 64.0),
        dim_background: true,
        line_spacing: 18.0,
        show_title: true,
    }
}

fn duke_save_menu(load: bool, selected_slot: usize) -> ListMenu {
    let items = (0..10)
        .map(|slot| {
            let name = format!("duke-{slot}");
            let exists = dirs_room_save(&name).is_file();
            if load && !exists {
                MenuItem::text(format!("EMPTY SLOT {}", slot + 1), "duke_slot_empty")
            } else {
                MenuItem::text(
                    format!("{} SLOT {}", if load { "LOAD" } else { "SAVE" }, slot + 1),
                    format!("duke_{}_slot {slot}", if load { "load" } else { "save" }),
                )
            }
        })
        .chain(std::iter::once(MenuItem::text("BACK", "duke_slot_close")))
        .collect();
    ListMenu {
        title: if load { "LOAD GAME" } else { "SAVE GAME" }.into(),
        items,
        selected: selected_slot.min(10),
        scroll: 0,
        visible_rows: 0,
        origin: Vec2::new(84.0, 44.0),
        dim_background: true,
        line_spacing: 14.0,
        show_title: true,
    }
}

fn duke_quickload_confirm_menu(slot: usize) -> ListMenu {
    ListMenu {
        title: "QUICK LOAD".into(),
        items: vec![
            MenuItem::text(
                "LOAD LAST SAVED GAME?",
                format!("duke_confirm_load_slot {slot}"),
            ),
            MenuItem::text("CANCEL", "duke_slot_close"),
        ],
        selected: 1,
        scroll: 0,
        visible_rows: 0,
        origin: Vec2::new(84.0, 74.0),
        dim_background: true,
        line_spacing: 18.0,
        show_title: true,
    }
}

fn duke_quit_confirm_menu() -> ListMenu {
    ListMenu {
        title: "ARE YOU SURE YOU WANT TO QUIT?".into(),
        items: vec![
            MenuItem::text("YES", "menu_quit_yes"),
            MenuItem::text("NO", "duke_quit_cancel"),
        ],
        selected: 1,
        scroll: 0,
        visible_rows: 0,
        origin: Vec2::new(80.0, 80.0),
        dim_background: true,
        line_spacing: 18.0,
        show_title: true,
    }
}

/// Carry Atomic's persistent player inventory/weapon state into the next map
/// while keeping the spawn pose and sprite handle created by the new map.
/// Draw a first native Duke automap directly from Build wall loops.
///
/// This keeps Duke's loaded native world as the source of truth instead of
/// displaying the unrelated Doom map geometry held by the desktop shell.
fn draw_duke_automap(
    world: &room_build_engine::world::BuildWorld,
    seen_sectors: &[bool],
    visible_sectors: &[bool],
    player_x: i32,
    player_y: i32,
    player_angle: i16,
    zoom: f32,
    list: &mut UiDrawList,
) {
    use room_ui::{Rect, Rgba, Vec2};

    let viewport = Rect::new(0.0, 0.0, 320.0, 168.0);
    list.fill(
        viewport,
        Rgba {
            r: 0.015,
            g: 0.02,
            b: 0.03,
            a: 0.88,
        },
    );
    let scale = 0.004 * (zoom / 0.35).clamp(0.06, 12.0);
    let point = |x: i32, y: i32| {
        Vec2::new(
            160.0 + (x.saturating_sub(player_x) as f32) * scale,
            84.0 - (y.saturating_sub(player_y) as f32) * scale,
        )
    };
    let solid = Rgba {
        r: 0.9,
        g: 0.74,
        b: 0.28,
        a: 1.0,
    };
    let portal = Rgba {
        r: 0.32,
        g: 0.72,
        b: 0.9,
        a: 0.85,
    };
    for (sector_index, sector) in world
        .sectors
        .iter()
        .take(world.numsectors.max(0) as usize)
        .enumerate()
    {
        if !seen_sectors.get(sector_index).copied().unwrap_or(false) {
            continue;
        }
        let first = usize::try_from(sector.wallptr).unwrap_or(usize::MAX);
        let count = usize::try_from(sector.wallnum).unwrap_or_default();
        let end_wall = first.saturating_add(count).min(world.walls.len());
        for wall in world.walls.get(first..end_wall).unwrap_or_default() {
            let Ok(next) = usize::try_from(wall.point2) else {
                continue;
            };
            let Some(end) = world.walls.get(next) else {
                continue;
            };
            list.cmds.push(DrawCmd::Line {
                a: point(wall.x, wall.y),
                b: point(end.x, end.y),
                color: if wall.nextsector >= 0 { portal } else { solid },
                thickness: 1.0,
            });
        }
    }

    let sprite_color = Rgba {
        r: 1.0,
        g: 0.35,
        b: 0.25,
        a: 1.0,
    };
    for sprite in world.sprites.iter().filter(|sprite| {
        sprite.xrepeat != 0
            && sprite.yrepeat != 0
            && sprite.cstat & 0x8000 == 0
            && usize::try_from(sprite.sectnum)
                .ok()
                .and_then(|sector| visible_sectors.get(sector))
                .copied()
                .unwrap_or(false)
    }) {
        let center = point(sprite.x, sprite.y);
        list.fill(
            Rect::new(center.x - 1.5, center.y - 1.5, 3.0, 3.0),
            sprite_color,
        );
    }

    let angle = f32::from(player_angle) * (std::f32::consts::TAU / 2048.0);
    let direction = Vec2::new(angle.cos(), -angle.sin());
    let side = Vec2::new(-direction.y, direction.x);
    let tip = Vec2::new(160.0 + direction.x * 9.0, 84.0 + direction.y * 9.0);
    let left = Vec2::new(
        160.0 - direction.x * 6.0 + side.x * 4.0,
        84.0 - direction.y * 6.0 + side.y * 4.0,
    );
    let right = Vec2::new(
        160.0 - direction.x * 6.0 - side.x * 4.0,
        84.0 - direction.y * 6.0 - side.y * 4.0,
    );
    let player_color = Rgba {
        r: 0.2,
        g: 1.0,
        b: 0.3,
        a: 1.0,
    };
    for (a, b) in [(tip, left), (left, right), (right, tip)] {
        list.cmds.push(DrawCmd::Line {
            a,
            b,
            color: player_color,
            thickness: 2.0,
        });
    }
}

/// Chocolate Build renderer's automap sector flood, scoped to the active view.
/// The ported cross-product gate only decides which portal sectors are marked;
/// wall clipping and the view-frustum rasterizer remain separate concerns.
fn duke_automap_visible_sectors(
    world: &room_build_engine::world::BuildWorld,
    start_sector: i16,
    player_x: i32,
    player_y: i32,
) -> Vec<bool> {
    let sector_count = world.numsectors.max(0) as usize;
    let mut visible = vec![false; sector_count];
    let Ok(start) = usize::try_from(start_sector) else {
        return visible;
    };
    if start >= sector_count {
        return visible;
    }
    let mut pending = vec![start];
    while let Some(sector_index) = pending.pop() {
        if std::mem::replace(&mut visible[sector_index], true) {
            continue;
        }
        let sector = world.sectors[sector_index];
        let first = usize::try_from(sector.wallptr).unwrap_or(usize::MAX);
        let count = usize::try_from(sector.wallnum).unwrap_or_default();
        let end = first.saturating_add(count).min(world.walls.len());
        for wall in world.walls.get(first..end).unwrap_or_default() {
            if wall.nextsector < 0 || wall.cstat & 32 != 0 {
                continue;
            }
            let Ok(next_sector) = usize::try_from(wall.nextsector) else {
                continue;
            };
            if next_sector >= sector_count || visible[next_sector] {
                continue;
            }
            let Ok(point2) = usize::try_from(wall.point2) else {
                continue;
            };
            let Some(end_wall) = world.walls.get(point2) else {
                continue;
            };
            let x1 = wall.x.wrapping_sub(player_x);
            let y1 = wall.y.wrapping_sub(player_y);
            let x2 = end_wall.x.wrapping_sub(player_x);
            let y2 = end_wall.y.wrapping_sub(player_y);
            let cross = x1.wrapping_mul(y2).wrapping_sub(x2.wrapping_mul(y1));
            // Chocolate's unsigned range check is equivalent to a signed
            // interval [-262144, 262143] while preserving 32-bit wraparound.
            let facing = (cross as u32).wrapping_add(262_144) < 524_288;
            if !facing {
                continue;
            }
            let wall_dx = x2.wrapping_sub(x1);
            let wall_dy = y2.wrapping_sub(y1);
            let cross_scaled = ((i64::from(cross) * i64::from(cross)) >> 5) as i32;
            let wall_length_squared = wall_dx
                .wrapping_mul(wall_dx)
                .wrapping_add(wall_dy.wrapping_mul(wall_dy));
            if cross_scaled <= wall_length_squared {
                pending.push(next_sector);
            }
        }
    }
    visible
}

fn dirs_room_demo(slot: &str) -> PathBuf {
    let ini = dirs_room_ini();
    match ini.parent() {
        Some(dir) => dir.join("demos").join(format!("{slot}.fdem")),
        None => PathBuf::from(format!("{slot}.fdem")),
    }
}

fn default_freedoom2() -> Option<PathBuf> {
    platform::current()
        .asset_roots()
        .into_iter()
        .map(|root| root.join("testdata/freedoom/freedoom2.wad"))
        .find(|p| p.is_file())
}

/// Prefer the requested map; if missing, fall back Doom1 `E1M1` or Doom2 `MAP01`.
fn load_language(vfs: Option<&Vfs>) -> Language {
    let Some(vfs) = vfs else {
        return Language::default();
    };
    let mut lang = Language::default();
    // GZDoom `FStringTable::LoadStrings`: every lump named LANGUAGE (any
    // extension: language.enu, LANGUAGE.RUS, …), in load order.
    let ids: Vec<_> = vfs
        .iter()
        .filter(|(_, info)| info.short_name.as_str().eq_ignore_ascii_case("LANGUAGE"))
        .map(|(id, _)| id)
        .collect();
    for id in ids {
        let Ok(bytes) = vfs.read(id) else {
            continue;
        };
        let Ok(text) = std::str::from_utf8(&bytes) else {
            continue;
        };
        let Ok(part) = parse_language(text) else {
            continue;
        };
        for (loc, table) in part.tables {
            let dest = lang.tables.entry(loc).or_default();
            for (k, v) in table {
                dest.insert(k, v);
            }
        }
    }
    lang
}

/// `$KEY` → LANGUAGE text. Unresolved keys stay off the HUD (the Doom font
/// draws them as broken gaps). `\c` color codes and `%o` are flattened.
/// Replace `$KEY` tokens found in LANGUAGE (GZDoom `GetTag` / `$` strings).
fn localize_tokens(lang: &Language, text: &str) -> String {
    if !text.contains('$') {
        return text.to_owned();
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(i) = rest.find('$') {
        out.push_str(&rest[..i]);
        let tail = &rest[i + 1..];
        let n = tail
            .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
            .unwrap_or(tail.len());
        let key = &tail[..n];
        match (n > 0)
            .then(|| lang.get("enu", &key.to_ascii_uppercase()))
            .flatten()
        {
            Some(v) => out.push_str(v),
            None => {
                out.push('$');
                out.push_str(key);
            }
        }
        rest = &tail[n..];
    }
    out.push_str(rest);
    out
}

fn hud_line(lang: &Language, raw: &str) -> Option<String> {
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }
    let text = if let Some(key) = raw.strip_prefix('$') {
        lang.get("enu", &key.to_ascii_uppercase())?.to_owned()
    } else {
        raw.to_owned()
    };
    let text = text.replace("%o", "You").replace("%k", "a monster");
    let mut out = String::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' && matches!(chars.peek(), Some('c' | 'C')) {
            chars.next();
            if chars.peek() == Some(&'[') {
                chars.next();
                for n in chars.by_ref() {
                    if n == ']' {
                        break;
                    }
                }
            } else {
                chars.next();
            }
            continue;
        }
        out.push(c);
    }
    let out = out.trim();
    if out.is_empty() {
        None
    } else {
        Some(out.to_owned())
    }
}

fn resolve_start_map(vfs: &Vfs, requested: &str) -> String {
    let req = requested.trim();
    let has =
        |name: &str| vfs.find(name, Namespace::Global).is_some() || vfs.find_any_ns(name).is_some();
    if !req.is_empty() && has(req) {
        return req.to_owned();
    }
    for cand in ["E1M1", "MAP01"] {
        if has(cand) {
            return (*cand).to_owned();
        }
    }
    if req.is_empty() {
        "E1M1".into()
    } else {
        req.to_owned()
    }
}

/// Roots searched for optional `lights.pk3` / `brightmaps.pk3` / … (cwd, exe dir, testdata).
fn engine_extra_search_roots() -> Vec<PathBuf> {
    platform::current().engine_extra_roots()
}

fn mount_pwads(mut builder: VfsBuilder, args: &AppConfig) -> Result<VfsBuilder, String> {
    for path in &args.file {
        builder = builder
            .mount_path(path)
            .map_err(|e| format!("{}: {e}", path.display()))?;
        eprintln!("room: mounted {}", path.display());
    }
    Ok(builder)
}

fn load_viewer_map(args: &AppConfig) -> Result<(MapGeometry, Option<Vfs>), String> {
    if args.native_duke {
        // Native Duke is rendered from BuildWorld; this inert map only supplies
        // the existing GPU shell's frame/canvas lifecycle, never game geometry.
        let mut map = MapBuilder::new()
            .room("native-duke", rect(0.0, 0.0, 512.0, 512.0), 0.0, 128.0)
            .player_start(256.0, 256.0, 0)
            .build_udmf("MAP01")
            .map_err(|e| e.to_string())?;
        apply_nodes(&mut map);
        return Ok((map, None));
    }
    if args.combat_demo {
        let iwad = args.iwad.clone().or_else(default_freedoom2);
        let Some(path) = iwad else {
            return Err(
                "combat demo needs an IWAD (pass --iwad or fetch testdata/freedoom/freedoom2.wad)"
                    .into(),
            );
        };
        let builder = match mount_embedded_engine(VfsBuilder::new()) {
            Ok(b) => b,
            Err(e) => {
                eprintln!("room: embedded pk3: {e}");
                VfsBuilder::new()
            }
        };
        let builder = mount_optional_engine_extras(builder, &engine_extra_search_roots());
        let vfs = mount_pwads(builder.mount_path(&path).map_err(|e| e.to_string())?, args)?
            .build()
            .map_err(|e| e.to_string())?;
        let mut map = combat_arena()
            .build_udmf("MAP01")
            .map_err(|e| e.to_string())?;
        apply_nodes(&mut map);
        eprintln!("room: combat arena + {}", path.display());
        return Ok((map, Some(vfs)));
    }
    if let Some(path) = &args.iwad {
        let builder = match mount_embedded_engine(VfsBuilder::new()) {
            Ok(b) => b,
            Err(e) => {
                eprintln!("room: embedded pk3: {e}");
                VfsBuilder::new()
            }
        };
        let builder = mount_optional_engine_extras(builder, &engine_extra_search_roots());
        let vfs = mount_pwads(room_build::mount::mount_path(builder, path)?, args)?
            .build()
            .map_err(|e| e.to_string())?;
        // Doom 1 / Freedoom Phase 1 use ExMy; Doom II uses MAPxx.
        let map_name = resolve_start_map(&vfs, &args.map);
        if map_name != args.map {
            eprintln!("room: map {} not in IWAD — starting {}", args.map, map_name);
        }
        let mut map = load_map(&vfs, &map_name).map_err(|e| e.to_string())?;
        ensure_nodes(&mut map);
        Ok((map, Some(vfs)))
    } else {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 512.0, 512.0), 0.0, 128.0)
            .player_start(256.0, 256.0, 0)
            .build_udmf("MAP01")
            .map_err(|e| e.to_string())?;
        apply_nodes(&mut map);
        Ok((map, None))
    }
}

/// Map a winit keycode to a KEYCONF `defaultbind` key name (lowercase).
fn keycode_keyconf_name(code: KeyCode) -> Option<&'static str> {
    Some(match code {
        KeyCode::KeyA => "a",
        KeyCode::KeyB => "b",
        KeyCode::KeyC => "c",
        KeyCode::KeyD => "d",
        KeyCode::KeyE => "e",
        KeyCode::KeyF => "f",
        KeyCode::KeyG => "g",
        KeyCode::KeyH => "h",
        KeyCode::KeyI => "i",
        KeyCode::KeyJ => "j",
        KeyCode::KeyK => "k",
        KeyCode::KeyL => "l",
        KeyCode::KeyM => "m",
        KeyCode::KeyN => "n",
        KeyCode::KeyO => "o",
        KeyCode::KeyP => "p",
        KeyCode::KeyQ => "q",
        KeyCode::KeyR => "r",
        KeyCode::KeyS => "s",
        KeyCode::KeyT => "t",
        KeyCode::KeyU => "u",
        KeyCode::KeyV => "v",
        KeyCode::KeyW => "w",
        KeyCode::KeyX => "x",
        KeyCode::KeyY => "y",
        KeyCode::KeyZ => "z",
        KeyCode::Digit0 | KeyCode::Numpad0 => "0",
        KeyCode::Digit1 | KeyCode::Numpad1 => "1",
        KeyCode::Digit2 | KeyCode::Numpad2 => "2",
        KeyCode::Digit3 | KeyCode::Numpad3 => "3",
        KeyCode::Digit4 | KeyCode::Numpad4 => "4",
        KeyCode::Digit5 | KeyCode::Numpad5 => "5",
        KeyCode::Digit6 | KeyCode::Numpad6 => "6",
        KeyCode::Digit7 | KeyCode::Numpad7 => "7",
        KeyCode::Digit8 | KeyCode::Numpad8 => "8",
        KeyCode::Digit9 | KeyCode::Numpad9 => "9",
        KeyCode::F1 => "f1",
        KeyCode::F2 => "f2",
        KeyCode::F3 => "f3",
        KeyCode::F4 => "f4",
        KeyCode::F5 => "f5",
        KeyCode::F6 => "f6",
        KeyCode::F7 => "f7",
        KeyCode::F8 => "f8",
        KeyCode::F9 => "f9",
        KeyCode::F10 => "f10",
        KeyCode::F11 => "f11",
        KeyCode::F12 => "f12",
        KeyCode::Space => "space",
        KeyCode::Tab => "tab",
        KeyCode::Enter | KeyCode::NumpadEnter => "enter",
        KeyCode::Escape => "escape",
        KeyCode::Backspace => "backspace",
        KeyCode::Delete => "del",
        KeyCode::Insert => "ins",
        KeyCode::Home => "home",
        KeyCode::End => "end",
        KeyCode::PageUp => "pgup",
        KeyCode::PageDown => "pgdn",
        KeyCode::ArrowUp => "up",
        KeyCode::ArrowDown => "down",
        KeyCode::ArrowLeft => "left",
        KeyCode::ArrowRight => "right",
        KeyCode::ShiftLeft | KeyCode::ShiftRight => "shift",
        KeyCode::ControlLeft | KeyCode::ControlRight => "ctrl",
        KeyCode::AltLeft | KeyCode::AltRight => "alt",
        KeyCode::CapsLock => "capslock",
        KeyCode::Pause => "pause",
        KeyCode::PrintScreen => "printscrn",
        KeyCode::ScrollLock => "scroll",
        KeyCode::NumLock => "numlock",
        KeyCode::Comma => ",",
        KeyCode::Period => ".",
        KeyCode::Slash => "/",
        KeyCode::Semicolon => ";",
        KeyCode::Quote => "'",
        KeyCode::BracketLeft => "[",
        KeyCode::BracketRight => "]",
        KeyCode::Backslash => "\\",
        KeyCode::Minus | KeyCode::NumpadSubtract => "-",
        KeyCode::Equal | KeyCode::NumpadAdd => "=",
        KeyCode::Backquote => "`",
        _ => return None,
    })
}

#[derive(Default)]
struct Keys {
    w: bool,
    a: bool,
    s: bool,
    d: bool,
    space: bool,
    c: bool,
    /// Duke quick kick (`/`).
    slash: bool,
    /// Duke inventory use (`'`).
    quote: bool,
    /// Duke turn around (Backspace; Brutal Duke reloads with it).
    backspace: bool,
    shift: bool,
    /// Turn left (← / Q when not in a menu).
    left: bool,
    /// Turn right (→).
    right: bool,
    /// Attack held (Ctrl / mouse left in play mode).
    attack: AttackLatch,
    /// Alt attack held (mouse right in play mode; `docs/31` MA4).
    alt_attack: bool,
    /// Physical Alt modifiers, used by Duke's RTS hotkeys.
    alt_left: bool,
    alt_right: bool,
    /// Use (E).
    use_btn: bool,
    /// Weapon slots 1..=9 edge (cleared after sample).
    weapon: u8,
    /// Queued weapon cycles (positive = next, negative = previous). One step
    /// is consumed per tic, so a multi-notch wheel still switches once per tic.
    weapon_cycle: i32,
    /// Fractional wheel notches not yet turned into a cycle.
    wheel_remainder: f32,
}

/// Native Chocolate `menues.c` pages (menus 0/50/100/110/990/501–503).
struct DukeMenuState {
    menu: room_duke::menu::DukeMenu,
    /// 120 Hz `totalclock` for the spinning cursor and logo pulse.
    clock: f32,
    /// Draw over the MENUSCREEN backdrop (title mode) instead of the game.
    backdrop: bool,
}

/// Chocolate `dobonus` between two campaign maps.
struct DukeBonusPanel {
    screen: room_duke::bonus::BonusScreen,
    transition: room_duke::DukeCampaignTransition,
}

struct ViewerApp {
    /// Immutable copy of the loaded map (sector lights / heights before playsim).
    map_base: MapGeometry,
    map: MapGeometry,
    vfs: Option<Vfs>,
    /// Doom SmallFont / BigFont when the IWAD is mounted.
    fonts: FontRegistry,
    /// Cached Doom picture lumps for HUD / weapon overlays.
    patches: PatchCache,
    /// Title / pause menu stack (`--play`).
    menu: Option<MenuStack>,
    materials: MaterialCatalog,
    /// Wall / bullet decals (renderer-owned).
    decals: DecalStore,
    /// ACS `SpawnParticle` billboards (renderer-owned).
    particles: ParticleStore,
    /// Render-only view kick (pitch/yaw radians); never written to the sim.
    view_kick: (f32, f32),
    /// Hit-flash remaining tics per actor (`r_hitflash` / E3).
    hit_flash: std::collections::BTreeMap<u32, u8>,
    /// Heat-haze pulse remaining tics (`r_heathaze` / D3 stand-in).
    heat_haze_tics: u8,
    /// DECALDEF registry for fader / pic lookup (`--play`).
    decaldef: Option<room_defs::DecalInfo>,
    /// `GameInfo.StatusBarClass` running in ZScript (else the native bar).
    script_sbar: Option<room_ui::ScriptStatusBar>,
    /// A mod's SBARINFO bar; drawn instead of the stock bar.
    sbarinfo_bar: Option<room_ui::SbarInfoBar>,
    /// `hud_althud` runner when no ZScript status bar is loaded (made on
    /// first use; `Err` once loading failed).
    althud_bar: Option<Result<room_ui::ScriptStatusBar, String>>,
    /// Current MAPINFO `sky1` texture (empty = default bind).
    sky_texture: String,
    /// Active GLDEFS cubemap (when complete); else cylinder uses [`Self::sky_texture`].
    sky_cubemap: Option<CubemapFaces>,
    /// GLDEFS skyboxes by lowercase name.
    skyboxes: std::collections::BTreeMap<String, CubemapFaces>,
    /// GLDEFS dynamic lights + object-frame bindings (M10).
    gldefs: GlDefs,
    /// GLDEFS `HardwareShader PostProcess` runners for `Enabled` shaders
    /// (built on first frame; docs/36 UZ6). `None` = not built yet.
    hw_post: Option<(wgpu::TextureFormat, Vec<HwPost>)>,
    /// What scripts asked of post shaders (`PPShader.SetEnabled` /
    /// `SetUniform*`), by lower-case shader name; kept across shader
    /// rebuilds (resize, format change) and maps.
    post_requests: std::collections::BTreeMap<String, PostRequest>,
    /// `post_requests` changed since it was applied to the shaders.
    post_requests_dirty: bool,
    /// Mod CVars (CVARINFO) and the value last handed to the game: console
    /// changes are passed on each frame.
    mod_cvars: Vec<(String, String)>,
    /// MODELDEF registry (M10).
    modeldefs: ModelDefs,
    /// Cached MODELDEF meshes.
    models: ModelCatalog,
    /// VOXELDEF registry.
    voxeldefs: VoxelDefs,
    /// Cached KVX meshes.
    voxels: VoxelCatalog,
    /// Voxel poses from the latest [`Self::collect_live_sprites`].
    voxel_poses: Vec<VoxelActorPose>,
    /// Actors that received a voxel pose this frame (sprite suppressed).
    voxel_pose_actors: std::collections::BTreeSet<(u32, u32)>,
    /// Actors whose current frame has a MODELDEF binding (no sprite drawn).
    model_pose_actors: std::collections::BTreeSet<(u32, u32)>,
    /// ANIMDEFS `cameratexture` sizes (uppercase name → w×h).
    camera_tex_sizes: std::collections::BTreeMap<String, (u32, u32)>,
    /// ANIMDEFS texture / flat animations.
    tex_anim: TextureAnimator,
    /// Frame counter for throttling camera-texture refreshes.
    camera_tex_frame: u32,
    /// GPU objects drop in declaration order, before [`Self::window`]. On
    /// Wayland, destroying the window first frees the `wl_surface` that EGL
    /// still uses and `wl_proxy_marshal` segfaults during process exit.
    surface: Option<wgpu::Surface<'static>>,
    config: Option<wgpu::SurfaceConfiguration>,
    gpu: Option<GpuContext>,
    pass: Option<UnlitPass>,
    zone_pass: Option<ModZonePass>,
    depth: Option<(wgpu::Texture, wgpu::TextureView)>,
    /// Dummy R8 MRT target (Metal stencil / unlit MRT pass layout).
    stencil_aux: Option<(wgpu::Texture, wgpu::TextureView)>,
    /// Colour buffer with `COPY_SRC` (surface itself often cannot be read back).
    frame_color: Option<(wgpu::Texture, wgpu::TextureView)>,
    /// Full-size target the scaled 3D view is stretched into (render scale
    /// below 1; docs/41 M6), and the pass that does it.
    scene_full: Option<(wgpu::Texture, wgpu::TextureView)>,
    upscaler: Option<room_render::Upscaler>,
    /// Render scale in use (`r_scale`, or the dynamic scale with `r_dynres`).
    render_scale: f32,
    dyn_scale: Option<crate::render_scale::DynamicScale>,
    /// `vid_maxfps` timing (docs/41 M6).
    pacer: crate::frame_pacer::FramePacer,
    /// Frame rate last passed to `Platform::set_frame_rate` (`None`: not yet,
    /// or a new surface needs it again).
    frame_rate_hint: Option<u32>,
    /// Last thermal state read from the platform.
    thermal: crate::frame_pacer::Thermal,
    /// `ROOM_TEST_THERMAL=serious|critical|…`: pretend the device is hot.
    thermal_override: Option<crate::frame_pacer::Thermal>,
    thermal_checked: Option<std::time::Instant>,
    /// The window's display refresh rate, read with the thermal state.
    refresh_millihertz: Option<u32>,
    /// Portal views drawn each frame (`room_render::portal_view`).
    portal_renderer: room_render::PortalRenderer,
    /// Ping buffer for SSAO when the post chain runs.
    post_ping: Option<(wgpu::Texture, wgpu::TextureView)>,
    /// LDR output of the post chain (HUD draws here when post is on).
    post_ldr: Option<(wgpu::Texture, wgpu::TextureView)>,
    /// Bloom / SSAO / tonemap chain (`docs/29`); created with the GPU.
    post_chain: Option<PostChain>,
    /// Live shader toggles (synced from console CVars each frame).
    render_settings: RenderSettings,
    /// `--combat-demo`: do not persist shader CVars via `writeconfig`.
    ephemeral_shaders: bool,
    canvas_gpu: Option<CanvasGpu>,
    /// The weapon and screen tints, drawn on the GPU (`hud_sprites.rs`).
    hud_sprites: Option<room_render::HudSprites>,
    /// The weapon drawn into the scene before GLDEFS post shaders.
    scene_sprites: Option<room_render::HudSprites>,
    window: Option<Arc<Window>>,
    camera: Camera,
    /// Capture PNG of the next presented frame.
    pending_screenshot: bool,
    /// `--shot-after`: frame to capture on; quit once saved.
    shot_after: Option<u64>,
    /// `--bench-frames`: frames to measure, and what was measured.
    bench_frames: Option<u64>,
    frame_stats: crate::bench::FrameStats,
    quit: bool,
    /// Started from the launcher: leaving goes back to it (phones never
    /// close the app from inside a game).
    from_launcher: bool,
    /// Set when leaving a game started from the launcher; the shell swaps
    /// the launcher back in, showing the message (a speed-test result).
    back_to_launcher: Option<Option<String>>,
    /// Speed-test summary for the launcher.
    bench_result: Option<String>,
    /// When set, append scaled frames for GIF encode on stop.
    gif_recorder: Option<GifRecorder>,
    keys: Keys,
    yaw: f32,
    pitch: f32,
    /// Camera bank in radians from `A_SetViewRoll`. 0 keeps the view upright.
    roll: f32,
    /// Gun offset from the previous tic (`PSPF_INTERPOLATE`).
    weapon_bob_prev: (f32, f32),
    mouse_look: bool,
    /// Skip the next `DeviceEvent::MouseMotion` (macOS cursor recenter warp).
    ignore_warp_motion: bool,
    /// When the window lost focus. Raw device events arrive system-wide, so
    /// they are ignored once focus has been gone longer than a click's blip.
    unfocused_since: Option<std::time::Instant>,
    /// Cursor in virtual 320×200 UI space (menus).
    menu_cursor: (f32, f32),
    last_frame: std::time::Instant,
    /// Running / in the background (phones; docs/41 §3.2).
    lifecycle: crate::lifecycle::Lifecycle,
    /// Test hook: `ROOM_TEST_BACKGROUND=N` sends the app to the background and
    /// back every N frames, as a phone does (docs/41 M2).
    background_test_every: Option<u64>,
    /// Test hook `ROOM_TEST_AWAY="frame:seconds"`: leave the screen at that
    /// frame and come back after that many seconds (seat away in netplay).
    away_test: Option<(u64, f32)>,
    /// Netplay while the app is off screen: the last background tic time.
    /// The seat keeps sending empty tic commands (the player stands still),
    /// so the other players are not held up.
    net_away: Option<std::time::Instant>,
    /// [`Self::away_test`]: when to come back.
    away_test_until: Option<std::time::Instant>,
    background_test_last: u64,
    frames: u64,
    /// Monotonic wall time used by title, ANM and finale frame selection.
    elapsed_ms: u64,
    /// Millisecond clock value when the current title/splash started.
    title_anim_origin: u64,
    /// Millisecond clock value when the current finale started.
    finale_anim_origin: u64,
    /// Last finale ANM frame whose sound cues were processed.
    duke_finale_sound_frame: Option<(String, u32)>,
    /// Extra title elapsed so Enter / click can skip `RADLOGO`.
    title_skip_ms: u64,
    /// Slot name for the in-progress Room demo (`record` / `stop`).
    demo_slot: String,
    fps_window: std::time::Instant,
    /// Frame timing for `vid_fps` and the slow-frame log (`perf.rs`).
    perf: crate::perf::FrameTimer,
    /// Map whose sprites [`ViewerApp::precache_level`] loaded.
    precached_map: Option<String>,
    /// Weapon classes whose HUD frames are loaded.
    precached_weapons: std::collections::HashSet<room_core::ClassId>,
    /// The weapon shown when [`ViewerApp::precache_hud`] last ran.
    precached_weapon_name: String,
    /// `vid_fps` panel commands and when they were built.
    perf_panel: Option<(std::time::Instant, Vec<room_render::CanvasCmd>)>,
    fps_count: u32,
    duke_fps: u32,
    /// Tab toggles classic automap overlay.
    automap: bool,
    /// F7 rising-edge latch for Atomic's native shoulder-view toggle.
    duke_view_key_down: bool,
    /// Rising-edge latch for Atomic's F11 brightness cycle.
    duke_brightness_key_down: bool,
    /// Chocolate `ud.show_help`: 0 hidden, 1 TEXTSTORY, 2 F1HELP.
    duke_help_page: u8,
    /// Prevent OS key-repeat from cycling several help pages in one press.
    duke_help_key_down: bool,
    /// Atomic single-player cheat parser phase: idle, after D, after DN.
    duke_cheat_phase: u8,
    duke_cheat_buffer: String,
    /// Automap zoom: screen pixels per map unit (`=`/`-` / mouse wheel).
    automap_scale: f32,
    /// Build sectors seen while the native Duke automap is open.
    duke_automap_seen: Vec<bool>,
    /// Atomic `ud.showallmap`, toggled by the native `dnshowmap` cheat.
    duke_show_all_map: bool,
    /// Atomic `ud.coords`, toggled by the native `dncoords` cheat.
    duke_coords_on: bool,
    /// Atomic `ud.tickrate & 1`, toggled by the native `dnrate` cheat.
    duke_rate_on: bool,
    /// Atomic `debug_on`; displays native playsim counts in the HUD.
    duke_debug_on: bool,
    /// Status bar numbers (viewer stub).
    hud: HudState,
    /// Optional Headless playsim session (`--play`).
    play: Option<Headless>,
    /// Optional native Build-based Duke simulation and soft renderer.
    duke_session: Option<room_duke::DukeSession>,
    duke_start_state: Option<Vec<u8>>,
    duke_renderer: Option<room_duke::DukeSoftRenderer>,
    /// Classic software renderer for the current map (`vid_rendermode` 0,
    /// `docs/40` SR3), keyed by map and `vid_softscale`; built on first use.
    soft_world: Option<((usize, u32), room_render_soft::sim::SoftWorldView)>,
    /// VOC samples indexed by native Duke CON sound id.
    duke_sound_bank: SampleBank,
    /// GRP/CON layers needed to load subsequent Atomic campaign maps.
    duke_campaign_content: Option<DukeCampaignContent>,
    /// Active native Duke `.fdem` recording (input plus post-tic state hashes).
    duke_demo_recorder: Option<room_duke::DukeDemoRecorder>,
    /// Matching Atomic `.DMO` recording for the same native input stream.
    duke_atomic_demo_recorder: Option<room_duke::DukeAtomicDemoRecorder>,
    /// Active native Duke `.fdem` playback.
    duke_demo_playback: Option<room_duke::DukeDemoRecorder>,
    /// Active Atomic revision 117 `.DMO` playback.
    duke_atomic_demo_playback: Option<room_duke::DukeAtomicDemoPlayer>,
    /// Campaign coordinates used in native `.fdem` metadata.
    duke_demo_map: (u8, u8),
    /// Atomic `music_select`, initialized from the campaign map and changed by Shift-F5.
    duke_music_selection: usize,
    /// Remaining Duke tics for the F5/Shift-F5 music hint quote.
    duke_music_quote_tics: u16,
    /// Frozen native intermission overlay shown after a solo campaign exit.
    duke_bonus_panel: Option<DukeBonusPanel>,
    duke_menu: Option<DukeMenuState>,
    /// Suppress OS key-repeat events while the music shortcut is held.
    duke_music_key_down: bool,
    /// Alt+F1..F10 edge latch for Atomic remote-ridicule sounds.
    duke_rts_key_down: Option<usize>,
    /// True while a native Duke pause/menu state has selected environment music.
    duke_environment_music_active: bool,
    /// Atomic `lastsavedpos` for F6/F9 quick-save/load shortcuts.
    duke_last_save_slot: Option<usize>,
    /// Optional UDP lockstep coop session (`--host` / `--join`).
    net: Option<netplay::NetPlay>,
    /// Rollback (`cl_rollback`): frames released by the lockstep, waiting to
    /// run on the confirmed game.
    rollback_frames: std::collections::VecDeque<Vec<TicCmd>>,
    /// Rollback: the speculative copy drawn this frame (swapped into the game
    /// only while drawing).
    rollback_world: Option<room_sim::World>,
    /// Rollback: `rollback_world` is in the game right now.
    rollback_swapped: bool,
    /// Rollback: own sounds already played from a copy, `(maptime, name)`,
    /// so the confirmed tic does not play them twice.
    rollback_sounds: std::collections::VecDeque<(u32, String)>,
    /// Local player seat when netplaying (also used for camera / HUD).
    local_seat: u8,
    /// Accumulator toward the next 1/35s tic.
    tic_accum: f32,
    /// Eye / look at the start of the current render interval (for tic lerp).
    cam_prev: Option<(f32, f32, f32, f32, f32)>,
    /// Eye / look after the last sim tic.
    cam_curr: Option<(f32, f32, f32, f32, f32)>,
    /// Sprite XYZ + yaw (degrees) at the previous tic, keyed by actor index.
    /// Keyed by the full handle (index + generation): arena slots are reused
    /// at once, so an index key made a fresh actor (a 2-tic fireball trail)
    /// interpolate from wherever the slot's previous owner was.
    sprite_prev: std::collections::HashMap<room_sim::ActorId, (f32, f32, f32, f32)>,
    /// Sprite XYZ + yaw after the last sim tic.
    sprite_curr: std::collections::HashMap<room_sim::ActorId, (f32, f32, f32, f32)>,
    /// Scale X/Y + alpha of `+INTERPOLATESCALE` / `+INTERPOLATEALPHA` actors
    /// at the previous and the last tic.
    sprite_extra_prev: std::collections::HashMap<room_sim::ActorId, (f32, f32, f32)>,
    sprite_extra_curr: std::collections::HashMap<room_sim::ActorId, (f32, f32, f32)>,
    /// On-screen messages from the sim.
    messages: MessageQueue,
    /// Last logged text per ACS HudMessage id (console log dedupe).
    hud_message_log: std::collections::HashMap<i32, String>,
    /// LANGUAGE strings (`$OB_ZOMBIESOLDIER` and pickup lines).
    language: Language,
    /// Previous local player health (to announce invisible attackers).
    last_player_hp: i32,
    /// Local player's Doom `fixedcolormap` (32 = invulnerability inverse).
    fixed_colormap: u8,
    /// Mouse yaw delta accumulated since last tic.
    mouse_yaw_accum: i32,
    /// Native Duke: sub-tic mouse yaw (Build angle units) not yet sent.
    duke_turn_residual: f64,
    /// Native Duke: Chocolate `turnheldtime` (120 Hz ticks).
    duke_turn_held: u32,
    /// Native Duke: sub-unit mouse pitch (horiz units) not yet sent.
    duke_horiz_residual: f64,
    /// Accumulated mouse pitch for the next play tic (`InputSample::mouse_pitch`).
    mouse_pitch_accum: i32,
    /// Console (cheats / CVars in play mode).
    console: Console,
    /// Console drop-down open.
    console_open: bool,
    /// Current console input line.
    console_line: String,
    /// KEYCONF `defaultbind` → console command (lowercase key name).
    keyconf_binds: std::collections::BTreeMap<String, String>,
    /// `doublebind` table and its tap timer.
    double_binds: room_input::DoubleBindings,
    double_tap: room_input::DoubleTapDetector,
    /// Process start, for input timing.
    start_instant: std::time::Instant,
    /// Game controller backend and this frame's / last frame's sample.
    pad_backend: gamepad::PadBackend,
    pad: Option<room_input::GamepadState>,
    /// On-screen touch controls (docs/41 M3): finger state, where the
    /// controls are, their sprites, and this frame's held keys / stick.
    touch: room_input::touch::TouchState,
    touch_layout: Option<room_input::touch::TouchLayout>,
    touch_overlay: crate::touch_overlay::TouchOverlay,
    /// Minimap radar image (phones, `touch_radar`).
    radar_cache: crate::radar::RadarCache,
    touch_keys: Vec<Key>,
    touch_analog: Option<(f32, f32)>,
    /// Launcher screen (docs/41 M4); `Some` while it is showing.
    launcher: Option<crate::launcher_screen::LauncherScreen>,
    /// The launcher's draw commands for this frame.
    launcher_frame: Vec<crate::launcher_screen::Draw>,
    /// A game picked in the launcher, started by [`Shell`] after this event.
    start_request: Option<AppConfig>,
    /// The launcher asked for a local-network game search (the shell runs it).
    find_lan_request: bool,
    /// Controls drawn this frame (`touch_controls`, touch screen or first touch).
    touch_shown: bool,
    /// Test hook `ROOM_TEST_TOUCH`: `(frame, id, phase, x, y)` still to play.
    touch_script: Vec<(u64, u64, room_input::touch::TouchPhase, f32, f32)>,
    pad_prev_buttons: u32,
    /// Controller buttons newly pressed since the last tic (edge-triggered binds).
    pad_pressed_since_tic: u32,
    /// Held `+mod1` … `+mod4` bits (`docs/28` BE4).
    mod_buttons: ModButtonState,
    /// KEYCONF `+reload` / `+zoom` / `+user1`–`4` latch (`docs/33` BD4).
    weapon_impulses: WeaponImpulseState,
    /// One-tic inventory-bar buttons from `invnext` / `invprev` / `invuse`.
    inv_cmd: u32,
    /// `+showscores` held.
    show_scores: bool,
    /// Host's net save (`false`) / load (`true`) to send with the next tic.
    net_save_cmd: Option<(bool, u8)>,
    /// Native audio output (mixer + music on the device thread).
    audio: Option<AudioOut>,
}

/// ANIMDEFS camera texture sizes and texture animations.
fn load_animdefs(
    vfs: Option<&Vfs>,
) -> (
    std::collections::BTreeMap<String, (u32, u32)>,
    TextureAnimator,
) {
    let mut out = std::collections::BTreeMap::new();
    let Some(vfs) = vfs else {
        return (out, TextureAnimator::default());
    };
    let Ok(defs) = DefinitionsBuilder::new(vfs).build() else {
        return (out, TextureAnimator::default());
    };
    for cam in &defs.animdefs.camera_textures {
        let w = cam.width.clamp(1, 256) as u32;
        let h = cam.height.clamp(1, 256) as u32;
        out.insert(cam.name.clone(), (w, h));
    }
    (out, TextureAnimator::new(&defs.animdefs))
}

/// Animate pulse / flicker radius from GLDEFS (`interval` in seconds).
/// Things for the second `iddt` (`AM_drawThings`): monsters red, items and
/// pickups yellow, the rest green, each shifted by its class `AutomapOffsets`.
fn automap_things(world: &room_sim::World) -> Vec<room_ui::AutomapThing> {
    let mut out = Vec::new();
    for &id in &world.thinkers.default {
        let Some(a) = world.actors.get(id) else {
            continue;
        };
        if a.destroyed {
            continue;
        }
        let Some(c) = world.classes.get(a.class) else {
            continue;
        };
        let (ox, oy) = (
            c.ext.stored_f64_at("automapoffsets", 0).unwrap_or(0.0),
            c.ext.stored_f64_at("automapoffsets", 1).unwrap_or(0.0),
        );
        let color = if a.flags & room_sim::FLAG_COUNTKILL != 0 && a.health > 0 {
            room_ui::Rgba::new(0.9, 0.2, 0.2, 1.0)
        } else if a.flags & room_sim::FLAG_COUNTITEM != 0 || c.is_inventory || c.is_weapon {
            room_ui::Rgba::new(0.9, 0.85, 0.2, 1.0)
        } else {
            room_ui::Rgba::new(0.3, 0.85, 0.3, 1.0)
        };
        #[allow(clippy::cast_possible_truncation)]
        out.push(room_ui::AutomapThing {
            x: a.pos.x() + ox,
            y: a.pos.y() + oy,
            angle_deg: a.angle.degrees() as f32,
            radius: a.radius.max(4.0) as f32,
            color,
        });
    }
    out
}

fn animated_light_radius(
    kind: DynLightKind,
    size: f32,
    secondary: f32,
    interval: f32,
    time_sec: f64,
) -> f32 {
    let sec = if secondary > 0.0 {
        secondary
    } else {
        size * 0.5
    };
    let period = if interval > 1e-4 {
        f64::from(interval)
    } else {
        0.5
    };
    match kind {
        DynLightKind::Point | DynLightKind::Spot | DynLightKind::Sector => size,
        DynLightKind::Pulse => {
            let phase = (time_sec / period) * std::f64::consts::TAU;
            let w = (phase.sin() * 0.5 + 0.5) as f32;
            size + (sec - size) * w
        }
        DynLightKind::Flicker | DynLightKind::FlickerRandom => {
            // Deterministic hash of time bucket (not sim RNG).
            let bucket = (time_sec / period.max(1e-4)).floor() as i64;
            let h = bucket.wrapping_mul(1103515245).wrapping_add(12345);
            let pick = (h & 1) == 0;
            if pick { size } else { sec }
        }
    }
}

fn load_voxeldefs(vfs: Option<&Vfs>) -> VoxelDefs {
    let Some(vfs) = vfs else {
        return VoxelDefs::default();
    };
    DefinitionsBuilder::new(vfs)
        .build()
        .map(|d| d.voxeldefs)
        .unwrap_or_default()
}

fn load_modeldefs(vfs: Option<&Vfs>) -> ModelDefs {
    let Some(vfs) = vfs else {
        return ModelDefs::default();
    };
    DefinitionsBuilder::new(vfs)
        .build()
        .map(|d| d.modeldefs)
        .unwrap_or_default()
}

/// The playsim's bone override in the renderer's form (UZ7).
fn bone_override(b: &room_sim::BoneOffset) -> room_render::BoneOverride {
    let comp = |c: &room_sim::BoneComponent| {
        (c.mode != 0 || c.prev_mode != 0).then(|| room_render::BoneComponent {
            mode: c.mode,
            prev_mode: c.prev_mode,
            switch_tic: c.switch_tic,
            interp_len: c.interp_len,
            prev: c.prev.map(|v| v as f32),
            cur: c.cur.map(|v| v as f32),
        })
    };
    room_render::BoneOverride {
        translation: comp(&b.translation),
        rotation: comp(&b.rotation),
        scaling: comp(&b.scaling),
    }
}

/// Compile every GLDEFS `HardwareShader PostProcess` marked `Enabled`
/// (UZDoom 5.0 auto uniforms + `LastInputTexture`; docs/36 UZ6).
/// One GLDEFS post shader and whether it runs (GLDEFS `Enabled`, then
/// `PPShader.SetEnabled`).
struct HwPost {
    name: String,
    enabled: bool,
    runner: room_render::HardwarePostRunner,
}

/// A script's requests for one post shader.
#[derive(Clone, Debug, Default, PartialEq)]
struct PostRequest {
    enabled: Option<bool>,
    uniforms: std::collections::BTreeMap<String, Vec<f32>>,
}

/// Fold `PPShader` calls into the per-shader requests; `true` if anything
/// changed.
fn record_post_ops(
    requests: &mut std::collections::BTreeMap<String, PostRequest>,
    ops: Vec<room_sim::zs_native_ops::PostShaderOp>,
) -> bool {
    use room_sim::zs_native_ops::PostShaderOp;
    let mut changed = false;
    for op in ops {
        match op {
            PostShaderOp::Enable { shader, on } => {
                let r = requests.entry(shader.to_ascii_lowercase()).or_default();
                changed |= r.enabled != Some(on);
                r.enabled = Some(on);
            }
            PostShaderOp::Uniform {
                shader,
                uniform,
                values,
            } => {
                let r = requests.entry(shader.to_ascii_lowercase()).or_default();
                let key = uniform.to_ascii_lowercase();
                if r.uniforms.get(&key) != Some(&values) {
                    r.uniforms.insert(key, values);
                    changed = true;
                }
            }
        }
    }
    changed
}

/// Apply the requests to the built shaders.
fn apply_post_requests(
    shaders: &mut [HwPost],
    requests: &std::collections::BTreeMap<String, PostRequest>,
) {
    for shader in shaders {
        let Some(r) = requests.get(&shader.name.to_ascii_lowercase()) else {
            continue;
        };
        if let Some(on) = r.enabled {
            shader.enabled = on;
        }
        for (name, values) in &r.uniforms {
            shader.runner.set_uniform(name, values);
        }
    }
}

/// Every GLDEFS post shader, enabled or not: a script may switch one on.
fn build_hw_post_runners(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    gldefs: &GlDefs,
    vfs: &Vfs,
) -> Vec<HwPost> {
    let mut out = Vec::new();
    for def in gldefs.hardware_shaders.values() {
        if def.kind != room_defs::HardwareShaderKind::PostProcess {
            continue;
        }
        // A path in a PK3; a lump name in a WAD (no folders there).
        let Some(src) = vfs
            .find_full(&def.shader)
            .or_else(|| {
                (!def.shader.contains('/'))
                    .then(|| vfs.find_any_ns(&def.shader))
                    .flatten()
            })
            .and_then(|id| vfs.read(id).ok())
            .map(|b| String::from_utf8_lossy(&b).into_owned())
        else {
            eprintln!(
                "room: post shader {}: lump {} not found",
                def.name, def.shader
            );
            continue;
        };
        match room_render::compile_post_glsl(def, &src) {
            Ok(compiled) => out.push(HwPost {
                name: def.name.clone(),
                enabled: def.enabled,
                runner: room_render::HardwarePostRunner::new(device, format, &compiled),
            }),
            Err(e) => eprintln!("room: post shader {}: {e}", def.name),
        }
    }
    out
}

fn load_gldefs(vfs: Option<&Vfs>) -> GlDefs {
    let defaults = GlDefs::doom_attack_defaults();
    let mut out = GlDefs::default();
    if let Some(vfs) = vfs {
        if let Ok(defs) = DefinitionsBuilder::new(vfs).build() {
            out = defs.gldefs;
        }
    }
    for (k, v) in defaults.lights {
        out.lights.entry(k).or_insert(v);
    }
    if out.object_lights.is_empty() {
        out.object_lights = defaults.object_lights;
    }
    out
}

fn fmt_actor_id(id: Option<ActorId>) -> String {
    id.map(|h| format!("#{}", h.index()))
        .unwrap_or_else(|| "-".into())
}

/// Title menu: MENUDEF `MainMenu` (or Doom's) with the MAPINFO skill names.
fn title_menu(game: &Headless) -> MenuStack {
    MenuStack::from_menudef_or_doom(game.menudef())
        .with_skills(game.mapinfo().map_or(&[], |m| m.skills.as_slice()))
        .with_episodes(game.mapinfo().map_or(&[], |m| m.episodes.as_slice()))
        .with_player_classes(game.menu_player_classes())
}

fn load_sky_defs(
    vfs: Option<&Vfs>,
    map_name: &str,
) -> (
    std::collections::BTreeMap<String, CubemapFaces>,
    String,
    Option<CubemapFaces>,
) {
    let mut skyboxes = std::collections::BTreeMap::new();
    let mut sky = String::new();
    let mut cubemap = None;
    let Some(vfs) = vfs else {
        return (skyboxes, sky, cubemap);
    };
    let Ok(defs) = DefinitionsBuilder::new(vfs).build() else {
        return (skyboxes, sky, cubemap);
    };
    for (name, box_) in &defs.gldefs.skyboxes {
        let faces = CubemapFaces::from_gldefs(&box_.faces);
        if !faces.primary().is_empty() {
            skyboxes.insert(name.clone(), faces);
        }
    }
    let key = map_name.to_ascii_uppercase();
    if let Some(m) = defs.mapinfo.maps.get(&key) {
        if let Some(box_name) = m.skybox.as_ref() {
            if let Some(faces) = skyboxes.get(&box_name.to_ascii_lowercase()) {
                if faces.is_complete() {
                    cubemap = Some(faces.clone());
                } else {
                    sky = faces.primary().to_owned();
                }
            }
        }
        if sky.is_empty() && cubemap.is_none() {
            if let Some(s) = m.sky1.as_ref() {
                sky = s.clone();
            }
        }
    }
    // Doom/Freedoom IWADs often omit MAPINFO; classic episode 1 sky is SKY1.
    if sky.is_empty() && cubemap.is_none() {
        sky = "SKY1".into();
    }
    (skyboxes, sky, cubemap)
}

fn install_basic_decorate(game: &mut Headless) -> Result<(), StartError> {
    let src = include_str!("../../../testdata/actors/doom_basic.txt");
    let ir = match parse_decorate(src) {
        Ok(i) => i,
        Err(e) => {
            fail!(1, "decorate parse: {e}");
        }
    };
    let classes = match ClassRegistry::from_decorate(&ir) {
        Ok(c) => Arc::new(c),
        Err(e) => {
            fail!(1, "class registry: {e}");
        }
    };
    let states = Arc::new(StateTable::from_decorate(&ir, &classes));
    game.set_defs(classes, states);
    Ok(())
}

fn start_play_session(
    map: &MapGeometry,
    decorate: Option<&std::path::Path>,
    vfs: Option<&mut Vfs>,
    skill: Skill,
    audio_rate: u32,
    rng_seed: u32,
    coop_players: u8,
    (deathmatch, bots): (bool, u8),
) -> Result<
    (
        Headless,
        Option<room_defs::DecalInfo>,
        Option<room_ui::ScriptStatusBar>,
        Option<room_ui::SbarInfoBar>,
        Vec<(String, String)>,
    ),
    StartError,
> {
    // CVARINFO settings (name, default) the mods declare.
    let mut mod_cvars: Vec<(String, String)> = Vec::new();
    let mut sbar_class: Option<String> = std::env::var("ROOM_STATUSBAR")
        .ok()
        .filter(|v| !v.is_empty());
    let mut script_sbar = None;
    let mut sbarinfo_bar = None;
    let mut game = Headless::new(GameOptions {
        skill,
        rng_seed,
        record_demo: false,
        // Bots take the seats after the players (every peer runs them).
        coop_players: coop_players.max(1).saturating_add(bots).min(8),
        deathmatch,
        fill_bots: bots > 0,
        bot_first_seat: coop_players.max(1),
        ..GameOptions::default()
    });
    let mut decaldef = None;
    if let Some(vfs) = vfs {
        let mut zscript_handlers: Vec<String> = Vec::new();
        if let Ok(defs) = DefinitionsBuilder::new(vfs).build() {
            if !defs.warnings.is_empty() {
                eprintln!("room: defs: {} warning(s)", defs.warnings.len());
                let debug = std::env::var_os("ROOM_ZSCRIPT_DEBUG").is_some_and(|v| v != "0");
                if debug {
                    for w in defs.warnings.iter().take(24) {
                        eprintln!("room: defs: {w}");
                    }
                    if defs.warnings.len() > 24 {
                        eprintln!("room: defs: … {} more warning(s)", defs.warnings.len() - 24);
                    }
                }
            }
            game.set_complevel(defs.complevel);
            // Mod CVars (CVARINFO) before the first map, as the web build
            // does: their defaults now, the console's values once it exists.
            let options =
                room_defs::mod_options::mod_options(&defs.cvars, &defs.menudef, &defs.language);
            for item in options.iter().flat_map(|g| &g.items) {
                game.set_mod_cvar(&item.cvar, &item.default);
                mod_cvars.push((item.cvar.clone(), item.default.clone()));
            }
            zscript_handlers = defs.zscript_event_handlers();
            if sbar_class.is_none() {
                sbar_class = defs.mapinfo.statusbar_class.clone();
            }
            game.set_mapinfo(Arc::new(defs.mapinfo));
            game.set_terrain(&defs.terrain);
            game.set_sndseq(&defs.sndseq);
            game.set_lockdefs(&defs.lockdefs, Some("Doom"));
            game.set_decaldef(&defs.decaldef);
            decaldef = Some(defs.decaldef.clone());
            // A mod's SBARINFO replaces the stock bar (UZDoom `DSBarInfo`).
            if let Some(info) = defs.sbarinfo.clone().filter(|i| !i.statusbars.is_empty()) {
                eprintln!(
                    "room: SBARINFO status bar ({})",
                    info.statusbars
                        .keys()
                        .cloned()
                        .collect::<Vec<_>>()
                        .join(", ")
                );
                sbarinfo_bar = Some(room_ui::SbarInfoBar::new(info, &defs.fonts));
            }
            game.set_keyconf(&defs.keyconf);
            if let Some(id) = vfs.find_any_ns("CONEVT") {
                if let Ok(bytes) = vfs.read(id) {
                    if let Ok(text) = std::str::from_utf8(&bytes) {
                        game.set_conevt(text);
                    }
                }
            }
            game.set_menudef(defs.menudef.clone());
            let mut libs = Vec::new();
            for name in &defs.loadacs.libraries {
                if let Some(id) = vfs
                    .find(name, Namespace::Acs)
                    .or_else(|| vfs.find_any_ns(name))
                {
                    if let Ok(bytes) = vfs.read(id) {
                        libs.push(bytes.to_vec());
                    }
                }
            }
            if !libs.is_empty() {
                eprintln!("room: LOADACS: {} librar(y/ies)", libs.len());
                game.set_acs_libraries(libs);
            }
            // Full pool too: `LOAD` chunks name more libraries (GZDoom `LoadModule`).
            game.set_acs_library_pool(
                room_game::acs_library_pool(vfs),
                defs.loadacs.libraries.clone(),
            );
        }
        // Compile ZScript from mounted archives (docs/34) — was missing from play path.
        let zs = room_game::load_zscript_from_vfs(vfs);
        room_game::log_zscript_report(&zs, &zscript_handlers);
        // `GameInfo.StatusBarClass`: a ZScript `BaseStatusBar` over the UZDoom library.
        if let Some(class) = sbar_class.as_deref() {
            match room_game::load_status_bar_program(vfs)
                .and_then(|prog| room_ui::ScriptStatusBar::new(prog, class))
            {
                Ok(bar) => {
                    eprintln!("room: ZScript status bar {}", bar.class());
                    script_sbar = Some(bar);
                }
                Err(e) => eprintln!("room: status bar {class}: {e} (native bar kept)"),
            }
        }
        game.set_zscript(zs.program, zscript_handlers);
        if let Some(path) = decorate {
            let src = match std::fs::read_to_string(path) {
                Ok(s) => s,
                Err(e) => {
                    fail!(1, "decorate {path:?}: {e}");
                }
            };
            let ir = match parse_decorate(&src) {
                Ok(i) => i,
                Err(e) => {
                    fail!(1, "decorate parse: {e}");
                }
            };
            let classes = match ClassRegistry::from_decorate(&ir) {
                Ok(c) => Arc::new(c),
                Err(e) => {
                    fail!(1, "class registry: {e}");
                }
            };
            let states = Arc::new(StateTable::from_decorate(&ir, &classes));
            game.set_defs(classes.clone(), states);
            eprintln!("room: DECORATE: {} classes (file)", classes.len());
        } else if let Some(data) = match room_game::load_embedded_engine_data_with(vfs) {
            Ok(d) => Some(d),
            Err(e) => {
                eprintln!("room: PWAD DECORATE failed: {e}");
                eprintln!("room: mod actors were not loaded — falling back to engine DECORATE");
                try_load_engine_data()
            }
        } {
            let n = data.classes.len();
            game.set_defs(data.classes, data.states);
            if let Some(db) = data.compat_db {
                game.set_compat_db(db);
            }
            if let Some(fixes) = data.level_fixes {
                game.set_level_fixes(fixes);
            }
            eprintln!("room: DECORATE: {n} classes (engine + mounted archives)");
        } else {
            install_basic_decorate(&mut game)?;
        }
        game.drop_weapons_without_sprites(vfs);
        let (mods, mod_errors) = room_game::discover_mods(vfs);
        for e in mod_errors {
            eprintln!("room: mod: {e}");
        }
        if !mods.is_empty() {
            match game.set_script_mods_with_vfs(mods, Some(vfs)) {
                Ok(names) => {
                    eprintln!("room: WASM mods: {}", names.join(", "));
                    if !game.generated_mod_wad().is_empty() {
                        eprintln!(
                            "room: mounted mod-generated.wad ({} bytes)",
                            game.generated_mod_wad().len()
                        );
                    }
                }
                Err(e) => eprintln!("room: {e}"),
            }
        }
        let (py_mods, py_errors) = room_game::discover_python_mods(vfs);
        for e in py_errors {
            eprintln!("room: python mod: {e}");
        }
        if !py_mods.is_empty() {
            match room_python_host::load_native_mods(&py_mods) {
                Ok((native, log)) => {
                    for line in log {
                        eprintln!("room: {line}");
                    }
                    let names: Vec<_> = native.iter().map(|m| m.name().to_owned()).collect();
                    game.set_native_mods(native);
                    eprintln!("room: Python mods: {}", names.join(", "));
                }
                Err(e) => eprintln!("room: python mod: {e}"),
            }
        }
        game.load_reverbs(vfs);
        let mut sounds = SoundSystem::with_sample_rate(audio_rate);
        match sounds.load_sndinfo_from_vfs(vfs) {
            Ok(()) => {
                game.set_sounds(sounds);
                eprintln!("room: SNDINFO loaded ({audio_rate} Hz)");
            }
            Err(e) => eprintln!("room: SNDINFO: {e}"),
        }
    } else if let Some(path) = decorate {
        let src = match std::fs::read_to_string(path) {
            Ok(s) => s,
            Err(e) => {
                fail!(1, "decorate {path:?}: {e}");
            }
        };
        let ir = match parse_decorate(&src) {
            Ok(i) => i,
            Err(e) => {
                fail!(1, "decorate parse: {e}");
            }
        };
        let classes = match ClassRegistry::from_decorate(&ir) {
            Ok(c) => Arc::new(c),
            Err(e) => {
                fail!(1, "class registry: {e}");
            }
        };
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        game.set_defs(classes, states);
    } else if let Some(data) = try_load_engine_data() {
        game.set_defs(data.classes, data.states);
        if let Some(db) = data.compat_db {
            game.set_compat_db(db);
        }
        if let Some(fixes) = data.level_fixes {
            game.set_level_fixes(fixes);
        }
        eprintln!("room: loaded embedded engine data (room.pk3)");
    } else {
        install_basic_decorate(&mut game)?;
    }
    game.set_pending_map(map.clone());
    eprintln!(
        "room: play mode — WASD move, ←→/Q turn, mouse look, wheel weapon, E use, Ctrl/LMB attack, Tab automap (=/- / wheel zoom), Esc pause, F6–F9 cheats, F12 screenshot, F11 GIF"
    );
    Ok((game, decaldef, script_sbar, sbarinfo_bar, mod_cvars))
}

fn draw_console_overlay(list: &mut UiDrawList, console: &Console, line: &str) {
    use room_ui::{Rect, Rgba};
    // SoftFont `list.text` is hard to read at HUD scale (fill-rects blur).
    // Match menus/HUD: Doom `STCFN###` patches at 1.5× in 320×200 space.
    const SCALE: f32 = 1.5;
    const LINE_H: f32 = 12.0; // ≈ (8+1)*1.5 tracking pitch
    const MAX_CHARS: usize = 34; // ~320 / ((8+1)*1.5)
    const HISTORY: usize = 6;
    let panel_h = 8.0 + LINE_H * (HISTORY as f32 + 1.0) + 6.0;
    list.fill(
        Rect::new(0.0, 0.0, 320.0, panel_h),
        Rgba::new(0.0, 0.0, 0.0, 0.82),
    );
    let trunc = |s: &str| -> String {
        let mut out = String::new();
        for ch in s.chars() {
            if out.chars().count() >= MAX_CHARS {
                out.push('…');
                break;
            }
            out.push(ch);
        }
        out
    };
    let mut y = 6.0;
    for msg in console
        .scrollback
        .iter()
        .rev()
        .take(HISTORY)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
    {
        draw_stcfn_string(list, 4.0, y, &trunc(msg.as_str()), SCALE);
        y += LINE_H;
    }
    let prompt = trunc(&format!("]{line}_"));
    draw_stcfn_string(list, 4.0, y + 2.0, &prompt, SCALE);
}

fn try_load_engine_data() -> Option<room_game::EngineData> {
    match room_game::load_embedded_engine_data() {
        Ok(data) => return Some(data),
        Err(e) => eprintln!("room: embedded engine data: {e}"),
    }
    let mut candidates: Vec<PathBuf> = platform::current()
        .asset_roots()
        .into_iter()
        .map(|root| root.join("wadsrc/room"))
        .collect();
    candidates.push(PathBuf::from("target/room.pk3"));
    for path in candidates {
        if !path.exists() {
            continue;
        }
        let vfs = room_vfs::VfsBuilder::new()
            .mount_path(&path)
            .ok()?
            .build()
            .ok()?;
        match room_game::load_engine_data(&vfs) {
            Ok(data) => return Some(data),
            Err(e) => eprintln!("room: engine data {}: {e}", path.display()),
        }
    }
    None
}

impl ViewerApp {
    /// Size the surface and every screen-sized render target to `size`
    /// (window resize, and a surface made again after the app returns).
    fn resize_targets(&mut self, size: winit::dpi::PhysicalSize<u32>) {
        if let (Some(surface), Some(config), Some(gpu)) =
            (&self.surface, &mut self.config, &self.gpu)
        {
            config.width = size.width.max(1);
            config.height = size.height.max(1);
            surface.configure(&gpu.device, config);
            self.canvas_gpu = Some(CanvasGpu::new(
                gpu,
                config.format,
                config.width,
                config.height,
            ));
            self.camera.aspect = f64::from(config.width) / f64::from(config.height);
        }
        self.rebuild_scene_targets();
    }

    /// (Re)create the 3D view's targets at the render scale (docs/41 M6):
    /// depth, stencil, scene colour and post-processing buffers at the
    /// scaled size, plus a full-size target the view is stretched into when
    /// the scale is below 1.
    fn rebuild_scene_targets(&mut self) {
        let (Some(config), Some(gpu)) = (&self.config, &self.gpu) else {
            return;
        };
        let (w, h) =
            crate::render_scale::scaled_size(config.width, config.height, self.render_scale);
        self.depth = Some(create_depth(&gpu.device, w, h));
        self.stencil_aux = Some(create_stencil_aux(&gpu.device, w, h));
        self.frame_color = Some(create_frame_color(&gpu.device, config.format, w, h));
        self.post_ping = Some(create_frame_color(&gpu.device, config.format, w, h));
        self.post_ldr = Some(create_frame_color(&gpu.device, config.format, w, h));
        if (w, h) == (config.width, config.height) {
            self.scene_full = None;
        } else {
            self.scene_full = Some(create_frame_color(
                &gpu.device,
                config.format,
                config.width,
                config.height,
            ));
            if self
                .upscaler
                .as_ref()
                .is_none_or(|u| u.format != config.format)
            {
                self.upscaler = Some(room_render::Upscaler::new(&gpu.device, config.format));
            }
        }
    }

    /// Follow `r_scale` / `r_dynres` after a frame that took `ms`.
    fn update_render_scale(&mut self, ms: f64) {
        let f = |v: Option<&room_console::CvarValue>, d: f32| match v {
            Some(room_console::CvarValue::Float(x)) => *x as f32,
            Some(room_console::CvarValue::Int(x)) => *x as f32,
            _ => d,
        };
        // A hot device lowers the ceiling and turns dynamic scaling on.
        let hot = crate::frame_pacer::limits(self.thermal);
        let ceiling = crate::render_scale::quantize(
            f(self.console.cvars.get("r_scale"), 1.0).min(hot.scale_ceiling),
        );
        let dynamic = hot.dynamic_scale
            || matches!(
                self.console.cvars.get("r_dynres"),
                Some(room_console::CvarValue::Bool(true)) | Some(room_console::CvarValue::Int(1))
            );
        let want = if dynamic {
            // Under a frame cap the budget is the cap's frame time.
            let cap = self.frame_cap();
            let mut fps = f(self.console.cvars.get("r_dynres_fps"), 60.0);
            if cap > 0 {
                fps = fps.min(cap as f32);
            }
            let dyn_scale = self
                .dyn_scale
                .get_or_insert_with(|| crate::render_scale::DynamicScale::new(ceiling));
            dyn_scale.frame(ms as f32, fps, ceiling);
            dyn_scale.scale()
        } else {
            self.dyn_scale = None;
            ceiling
        };
        if want != self.render_scale {
            self.render_scale = want;
            self.rebuild_scene_targets();
        }
    }

    /// Frames per second allowed now: `vid_maxfps` (0: no cap), lowered when
    /// the device is very hot.
    fn frame_cap(&self) -> u32 {
        let setting = match self.console.cvars.get("vid_maxfps") {
            Some(room_console::CvarValue::Int(n)) => (*n).clamp(0, 1000) as u32,
            Some(room_console::CvarValue::Float(x)) => x.clamp(0.0, 1000.0) as u32,
            _ => 0,
        };
        crate::frame_pacer::effective_cap(setting, crate::frame_pacer::limits(self.thermal).max_fps)
    }

    /// Read the thermal state and the display's refresh rate, at most once a
    /// second.
    fn poll_thermal(&mut self, window: &Window) {
        let now = std::time::Instant::now();
        if self
            .thermal_checked
            .is_some_and(|t| now.duration_since(t) < std::time::Duration::from_secs(1))
        {
            return;
        }
        self.thermal_checked = Some(now);
        self.refresh_millihertz = window
            .current_monitor()
            .and_then(|m| m.refresh_rate_millihertz());
        let thermal = self
            .thermal_override
            .unwrap_or_else(|| platform::current().thermal());
        if thermal != self.thermal {
            self.thermal = thermal;
            let hot = crate::frame_pacer::limits(thermal);
            eprintln!(
                "room: thermal {}: 3D resolution up to {}{}",
                thermal.name(),
                hot.scale_ceiling,
                hot.max_fps
                    .map_or_else(String::new, |n| format!(", at most {n} fps"))
            );
        }
    }

    /// Ask for the next frame now, or sleep until the frame cap allows it.
    fn schedule_frame(&mut self, event_loop: &ActiveEventLoop, window: &Window) {
        self.poll_thermal(window);
        let cap = self.frame_cap();
        if self.frame_rate_hint != Some(cap) {
            self.frame_rate_hint = Some(cap);
            platform::current().set_frame_rate(window, cap);
        }
        let timed = crate::frame_pacer::timer_needed(cap, self.refresh_millihertz);
        match self
            .pacer
            .poll(std::time::Instant::now(), if timed { cap } else { 0 })
        {
            None => {
                event_loop.set_control_flow(ControlFlow::Poll);
                window.request_redraw();
            }
            Some(deadline) => event_loop.set_control_flow(ControlFlow::WaitUntil(deadline)),
        }
    }

    /// Back from the background (phones): make a new surface for the same
    /// window and carry on. The game, GPU device and textures were kept.
    fn recreate_surface(&mut self, event_loop: &ActiveEventLoop) {
        let (Some(window), Some(gpu)) = (self.window.clone(), self.gpu.as_ref()) else {
            return;
        };
        let surface = match gpu.instance.create_surface(window.clone()) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("room: surface: {e}");
                event_loop.exit();
                return;
            }
        };
        self.surface = Some(surface);
        self.resize_targets(window.inner_size());
        // Don't count the time spent in the background as one long frame.
        self.last_frame = std::time::Instant::now();
        self.pacer.reset();
        self.frame_rate_hint = None;
        if let Some(audio) = &self.audio {
            audio.set_suspended(false);
        }
        let tic = self
            .play
            .as_ref()
            .and_then(|g| g.world())
            .map(|w| w.level.time);
        eprintln!(
            "room: resumed (back from the background {} time(s), game tic {tic:?})",
            self.lifecycle.returns
        );
        window.request_redraw();
    }
}

impl ViewerApp {
    /// Build the passes and screen-sized targets for a configured surface and
    /// take ownership of the window and GPU (first start, or a game started
    /// from the launcher taking over the launcher's window; docs/41 M4).
    fn install_display(
        &mut self,
        window: Arc<Window>,
        surface: wgpu::Surface<'static>,
        gpu: GpuContext,
        config: wgpu::SurfaceConfiguration,
    ) {
        let format = config.format;
        let mut pass = UnlitPass::new(&gpu, format);
        let zone_pass = ModZonePass::new(&gpu, format);
        // ~90° horizontal at 16:9 (Doom-like); `fov_y_deg` 90 is fish-eye wide.
        self.camera.fov_y_deg = 58.715;
        pass.light_mode = LightMode::DoomSoftware;
        // Must track UnlitPass default: values like 8–16 put every surface past
        // ~30mu on the DoomSoftware shade floor (monsters vanish into walls).
        pass.glob_vis = 512.0;
        self.bind_iwad_albedo(&gpu, &mut pass);
        self.preload_map_materials(&gpu);
        self.perf.loaded();
        crate::bench::log_stage(
            self.bench_frames.is_some(),
            "GPU set-up and texture preload",
        );
        let depth = create_depth(&gpu.device, config.width, config.height);
        let stencil_aux = create_stencil_aux(&gpu.device, config.width, config.height);
        let frame_color = create_frame_color(&gpu.device, format, config.width, config.height);
        let post_ping = create_frame_color(&gpu.device, format, config.width, config.height);
        let post_ldr = create_frame_color(&gpu.device, format, config.width, config.height);
        let post_chain = PostChain::new(&gpu, format, format);
        let canvas_gpu = CanvasGpu::new(&gpu, format, config.width, config.height);
        let hud_sprites = room_render::HudSprites::new(&gpu.device, format);
        self.camera.aspect = f64::from(config.width) / f64::from(config.height.max(1));
        if let Some(t) = self.map.things.iter().find(|t| t.type_id == 1) {
            #[allow(clippy::cast_possible_truncation)]
            {
                self.camera.eye = Vec3::new(f32::from(t.x), f32::from(t.y), 41.0);
                self.yaw = (f32::from(t.angle)).to_radians();
            }
            self.sync_forward();
        }
        self.window = Some(window);
        self.surface = Some(surface);
        self.config = Some(config);
        self.gpu = Some(gpu);
        self.pass = Some(pass);
        self.zone_pass = Some(zone_pass);
        self.depth = Some(depth);
        self.stencil_aux = Some(stencil_aux);
        self.frame_color = Some(frame_color);
        self.post_ping = Some(post_ping);
        self.post_ldr = Some(post_ldr);
        self.post_chain = Some(post_chain);
        self.canvas_gpu = Some(canvas_gpu);
        self.hud_sprites = Some(hud_sprites);
        if self.render_scale < 1.0 {
            self.rebuild_scene_targets();
        }
        // Startup called `set_play_look` before any window existed — apply it now.
        if self.mouse_look
            || self
                .play
                .as_ref()
                .is_some_and(|g| g.state == GameState::Level)
        {
            self.set_play_look(true);
        }
    }

    /// Take the window, surface and GPU device from `old` (the launcher) and
    /// set up this app's rendering on them. Nothing is created twice, so this
    /// also works on phones, which have exactly one window.
    fn adopt_display(&mut self, old: &mut ViewerApp) {
        // Only a launcher that is on screen hands over (a start is requested
        // by a tap or key press, so it is); otherwise `resumed` builds the
        // display for the new app as usual.
        if old.window.is_none()
            || old.surface.is_none()
            || old.gpu.is_none()
            || old.config.is_none()
        {
            return;
        }
        self.lifecycle = old.lifecycle;
        if let (Some(window), Some(surface), Some(gpu), Some(config)) = (
            old.window.take(),
            old.surface.take(),
            old.gpu.take(),
            old.config.take(),
        ) {
            self.install_display(window, surface, gpu, config);
        }
    }
}

impl ApplicationHandler for ViewerApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.net_away.take().is_some() {
            eprintln!("room: netplay: back in the game");
            self.last_frame = std::time::Instant::now();
        }
        let resume = self
            .lifecycle
            .resumed(self.window.is_some(), self.surface.is_some());
        crate::startlog::step(format_args!("resumed: {resume:?}"));
        match resume {
            ResumeStep::Nothing => return,
            ResumeStep::RecreateSurface => {
                self.recreate_surface(event_loop);
                return;
            }
            ResumeStep::CreateAll => {}
        }
        let attrs = Window::default_attributes()
            .with_title("Room map viewer")
            .with_inner_size(window_size());
        #[cfg(target_os = "macos")]
        let attrs = {
            use winit::platform::macos::WindowAttributesExtMacOS;
            attrs.with_accepts_first_mouse(true)
        };
        crate::startlog::step("window: creating");
        let window = match event_loop.create_window(attrs) {
            Ok(w) => Arc::new(w),
            Err(e) => {
                crate::startlog::step(format_args!("window: failed: {e}"));
                eprintln!("room: window: {e}");
                event_loop.exit();
                return;
            }
        };

        crate::startlog::step(format_args!(
            "window: created, {}x{} pixels, scale {}",
            window.inner_size().width,
            window.inner_size().height,
            window.scale_factor()
        ));
        crate::startlog::step("gpu: creating the wgpu instance");
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..Default::default()
        });
        crate::startlog::step("gpu: creating the surface");
        let surface = match instance.create_surface(window.clone()) {
            Ok(s) => s,
            Err(e) => {
                crate::startlog::step(format_args!("gpu: surface failed: {e}"));
                eprintln!("room: surface: {e}");
                event_loop.exit();
                return;
            }
        };
        crate::startlog::step("gpu: choosing an adapter");
        let adapter =
            match pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })) {
                Ok(a) => a,
                Err(e) => {
                    crate::startlog::step(format_args!("gpu: no adapter: {e}"));
                    eprintln!("room: adapter: {e}");
                    event_loop.exit();
                    return;
                }
            };
        {
            let info = adapter.get_info();
            eprintln!(
                "room: gpu: {} ({:?}, {:?}, driver {} {})",
                info.name, info.backend, info.device_type, info.driver, info.driver_info
            );
            crate::startlog::step(format_args!(
                "gpu: adapter {} ({:?}, driver {} {}); requesting the device",
                info.name, info.backend, info.driver, info.driver_info
            ));
        }
        let gpu = match pollster::block_on(GpuContext::from_adapter(instance, adapter)) {
            Ok(g) => g,
            Err(e) => {
                crate::startlog::step(format_args!("gpu: device failed: {e}"));
                eprintln!("room: gpu: {e}");
                event_loop.exit();
                return;
            }
        };
        crate::startlog::step(format_args!("gpu: device ready ({:?} tier)", gpu.tier));

        let size = window.inner_size();
        let caps = surface.get_capabilities(&gpu.adapter);
        crate::startlog::step(format_args!(
            "surface: formats {:?}, present modes {:?}, alpha {:?}, usages {:?}",
            caps.formats, caps.present_modes, caps.alpha_modes, caps.usages
        ));
        if !caps.usages.contains(wgpu::TextureUsages::COPY_DST) {
            crate::startlog::step(
                "surface: WARNING: the screen surface does not accept copies (COPY_DST); configuring it will fail",
            );
        }
        let Some(format) = caps
            .formats
            .iter()
            .copied()
            .find(wgpu::TextureFormat::is_srgb)
            .or_else(|| caps.formats.first().copied())
        else {
            eprintln!("room: no surface formats");
            event_loop.exit();
            return;
        };
        let alpha = caps
            .alpha_modes
            .first()
            .copied()
            .unwrap_or(wgpu::CompositeAlphaMode::Auto);
        let config = wgpu::SurfaceConfiguration {
            // COPY_DST: blit from the capturable frame colour buffer each present.
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_DST,
            format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: wgpu::PresentMode::AutoVsync,
            alpha_mode: alpha,
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        crate::startlog::step(format_args!(
            "surface: configuring {}x{} {:?}",
            config.width, config.height, config.format
        ));
        surface.configure(&gpu.device, &config);
        crate::startlog::step("display: building render passes and textures");
        self.install_display(window, surface, gpu, config);
        crate::startlog::step("display: ready; drawing");
    }

    /// The app left the screen (phones): drop the surface, which the OS is
    /// about to destroy, and pause sound. The game itself stays in memory.
    fn suspended(&mut self, _event_loop: &ActiveEventLoop) {
        crate::startlog::step("suspended (app left the screen)");
        if self.lifecycle.suspended() {
            self.surface = None;
            self.touch.release_all();
            self.touch_keys.clear();
            self.touch_analog = None;
            if let Some(audio) = &self.audio {
                audio.set_suspended(true);
            }
            eprintln!("room: suspended");
            if self.net.is_some() && self.duke_session.is_none() && self.play.is_some() {
                eprintln!("room: netplay: seat away (standing still until back)");
                self.net_away = Some(std::time::Instant::now());
            }
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => self.resize_targets(size),
            WindowEvent::KeyboardInput {
                event:
                    KeyEvent {
                        physical_key: PhysicalKey::Code(code),
                        state,
                        text,
                        repeat,
                        ..
                    },
                ..
            } => {
                let down = state == ElementState::Pressed;
                if self.launcher.is_some() {
                    if down {
                        self.launcher_key(code);
                    }
                    return;
                }
                match code {
                    KeyCode::AltLeft => self.keys.alt_left = down,
                    KeyCode::AltRight => self.keys.alt_right = down,
                    _ => {}
                }
                if down && code == KeyCode::Backquote {
                    self.console_open = !self.console_open;
                    if !self.console_open {
                        self.console_line.clear();
                    }
                    return;
                }
                if self.console_open {
                    if down {
                        match code {
                            KeyCode::Enter | KeyCode::NumpadEnter => {
                                let line = std::mem::take(&mut self.console_line);
                                if !line.is_empty() {
                                    self.apply_console(&line);
                                }
                            }
                            KeyCode::Escape => {
                                self.console_open = false;
                                self.console_line.clear();
                            }
                            KeyCode::Backspace => {
                                self.console_line.pop();
                            }
                            _ => {
                                if let Some(t) = text.as_deref() {
                                    if t != "`" && t != "~" {
                                        self.console_line.push_str(t);
                                    }
                                }
                            }
                        }
                    }
                    return;
                }
                if down && self.try_skip_title_splash() {
                    return;
                }
                // Native Duke menu navigation (Chocolate `probe`).
                if down && self.duke_menu.is_some() {
                    use room_duke::menu::MenuKey;
                    let key = match code {
                        KeyCode::ArrowUp | KeyCode::PageUp | KeyCode::Numpad8 | KeyCode::KeyW => {
                            Some(MenuKey::Up)
                        }
                        KeyCode::ArrowDown
                        | KeyCode::PageDown
                        | KeyCode::Numpad2
                        | KeyCode::KeyS => Some(MenuKey::Down),
                        KeyCode::ArrowLeft | KeyCode::Numpad4 => Some(MenuKey::Left),
                        KeyCode::ArrowRight | KeyCode::Numpad6 => Some(MenuKey::Right),
                        KeyCode::Enter | KeyCode::NumpadEnter | KeyCode::Space => {
                            Some(MenuKey::Enter)
                        }
                        KeyCode::Escape => Some(MenuKey::Escape),
                        KeyCode::KeyY => Some(MenuKey::Yes),
                        KeyCode::KeyN => Some(MenuKey::No),
                        _ => None,
                    };
                    if let Some(key) = key {
                        self.duke_menu_key(key, event_loop);
                    }
                    return;
                }
                // `dobonus`: any key skips the count-up, then ends the screen.
                if down
                    && !repeat
                    && let Some(panel) = self.duke_bonus_panel.as_mut()
                {
                    panel.screen.key();
                    let labels = panel.screen.take_sounds();
                    self.play_duke_bonus_sounds(&labels);
                    return;
                }
                let rts_index = match code {
                    KeyCode::F1 => Some(0),
                    KeyCode::F2 => Some(1),
                    KeyCode::F3 => Some(2),
                    KeyCode::F4 => Some(3),
                    KeyCode::F5 => Some(4),
                    KeyCode::F6 => Some(5),
                    KeyCode::F7 => Some(6),
                    KeyCode::F8 => Some(7),
                    KeyCode::F9 => Some(8),
                    KeyCode::F10 => Some(9),
                    _ => None,
                };
                if let Some(index) = rts_index
                    && self.duke_session.is_some()
                {
                    if !down && self.duke_rts_key_down == Some(index) {
                        self.duke_rts_key_down = None;
                        return;
                    }
                    if down && (self.keys.alt_left || self.keys.alt_right) && self.menu.is_none() {
                        if self.duke_rts_key_down != Some(index) {
                            self.duke_rts_key_down = Some(index);
                            if self.play_duke_rts_sound(index)
                                && let Some(net) = self.net.as_mut()
                                && let Err(error) = net.broadcast_duke_rts(index)
                            {
                                eprintln!("room: Duke RTS send: {error}");
                            }
                        }
                        return;
                    }
                }
                let help_advance = matches!(
                    code,
                    KeyCode::F1 | KeyCode::Space | KeyCode::Enter | KeyCode::NumpadEnter
                );
                if !down && help_advance {
                    self.duke_help_key_down = false;
                    if self.duke_help_page != 0 {
                        return;
                    }
                }
                if self.duke_help_page != 0 {
                    if down {
                        match code {
                            KeyCode::Escape => self.duke_help_page = 0,
                            _ if help_advance && !self.duke_help_key_down => {
                                self.duke_help_key_down = true;
                                self.duke_help_page = if self.duke_help_page == 1 { 2 } else { 0 };
                            }
                            _ => {}
                        }
                    }
                    return;
                }
                let menu_open = self.menu.is_some();
                // `doublebind`: a second press inside `m_doubleclicktime`.
                if down && !repeat && !menu_open && self.play.is_some() {
                    if let Some(name) = keycode_keyconf_name(code) {
                        let now = self.start_instant.elapsed().as_millis() as u64;
                        self.double_tap.threshold_ms =
                            match self.console.cvars.get("m_doubleclicktime") {
                                Some(room_console::CvarValue::Int(v)) => (*v).max(0) as u64,
                                _ => room_input::double_tap::DEFAULT_DOUBLE_TAP_MS,
                            };
                        if self.double_tap.press(name, now)
                            && let Some(cmd) = self.double_binds.get(name).map(str::to_owned)
                        {
                            self.apply_console(&cmd);
                        }
                    }
                }
                if down
                    && !menu_open
                    && self.duke_demo_recorder.is_none()
                    && self.duke_atomic_demo_recorder.is_none()
                    && self.duke_demo_playback.is_none()
                    && self
                        .duke_session
                        .as_ref()
                        .is_some_and(|s| s.player_count() == 1 && s.skill < 4)
                {
                    if self.duke_cheat_phase == 1 {
                        if code == KeyCode::KeyN {
                            self.duke_cheat_phase = 2;
                            self.duke_cheat_buffer.clear();
                            return;
                        }
                        if code != KeyCode::KeyD {
                            self.duke_cheat_phase = 0;
                        }
                    } else if self.duke_cheat_phase == 2 {
                        let ch = keycode_keyconf_name(code).filter(|name| name.len() == 1);
                        if let Some(ch) = ch {
                            self.duke_cheat_buffer.push_str(ch);
                            let cheats = [
                                "cornholio",
                                "kroz",
                                "stuff",
                                "weapons",
                                "items",
                                "inventory",
                                "keys",
                                "hyper",
                                "beta",
                                "todd",
                                "allen",
                                "time",
                                "rate",
                                "monsters",
                                "debug",
                                "showmap",
                                "view",
                                "coords",
                                "unlock",
                                "cashman",
                                "clip",
                            ];
                            let is_scotty_prefix = if self.duke_cheat_buffer.len() <= 6 {
                                "scotty".starts_with(&self.duke_cheat_buffer)
                            } else {
                                self.duke_cheat_buffer.starts_with("scotty")
                                    && self.duke_cheat_buffer.len() <= 9
                                    && self.duke_cheat_buffer[6..]
                                        .bytes()
                                        .all(|byte| byte.is_ascii_digit())
                            };
                            let scotty_complete = self.duke_cheat_buffer.len() == 9
                                && self.duke_cheat_buffer.starts_with("scotty")
                                && self.duke_cheat_buffer[6..]
                                    .bytes()
                                    .all(|byte| byte.is_ascii_digit());
                            let is_skill_prefix = if self.duke_cheat_buffer.len() <= 5 {
                                "skill".starts_with(&self.duke_cheat_buffer)
                            } else {
                                self.duke_cheat_buffer.len() == 6
                                    && self.duke_cheat_buffer.starts_with("skill")
                                    && matches!(self.duke_cheat_buffer.as_bytes()[5], b'1'..=b'5')
                            };
                            let skill_complete = self.duke_cheat_buffer.len() == 6
                                && self.duke_cheat_buffer.starts_with("skill")
                                && matches!(self.duke_cheat_buffer.as_bytes()[5], b'1'..=b'5');
                            let complete = cheats.contains(&self.duke_cheat_buffer.as_str())
                                || scotty_complete
                                || skill_complete;
                            let prefix = is_scotty_prefix
                                || is_skill_prefix
                                || cheats.iter().any(|candidate| {
                                    candidate.starts_with(&self.duke_cheat_buffer)
                                });
                            if complete {
                                let cheat = self.duke_cheat_buffer.clone();
                                self.duke_cheat_phase = 0;
                                self.duke_cheat_buffer.clear();
                                if scotty_complete {
                                    let bytes = cheat.as_bytes();
                                    let volume = bytes[6].checked_sub(b'1');
                                    let level =
                                        ((bytes[7] - b'0') * 10 + (bytes[8] - b'0')).checked_sub(1);
                                    if let (Some(volume), Some(level)) = (volume, level)
                                        && let Some(progress) =
                                            room_duke::DukeCampaignProgress::new(volume, level)
                                        && let Some(session) = self.duke_session.as_mut()
                                    {
                                        session.campaign = Some(progress);
                                        self.advance_duke_campaign(
                                            room_duke::DukeCampaignTransition::NextLevel {
                                                volume,
                                                level,
                                            },
                                            None,
                                        );
                                    }
                                    return;
                                }
                                if skill_complete {
                                    let skill = cheat.as_bytes()[5] - b'1';
                                    if let Some(campaign) = self
                                        .duke_session
                                        .as_ref()
                                        .and_then(|session| session.campaign)
                                    {
                                        self.advance_duke_campaign(
                                            room_duke::DukeCampaignTransition::NextLevel {
                                                volume: campaign.volume,
                                                level: campaign.level,
                                            },
                                            Some(skill),
                                        );
                                    }
                                    return;
                                }
                                if cheat == "showmap" {
                                    self.duke_show_all_map = !self.duke_show_all_map;
                                    if let Some(world) = self
                                        .duke_session
                                        .as_ref()
                                        .and_then(|session| session.world.as_ref())
                                    {
                                        self.duke_automap_seen = vec![
                                            self.duke_show_all_map;
                                            world.numsectors.max(0)
                                                as usize
                                        ];
                                    }
                                    self.messages.push(
                                        if self.duke_show_all_map {
                                            "Full map revealed"
                                        } else {
                                            "Map reveal disabled"
                                        },
                                        70,
                                    );
                                    return;
                                }
                                if cheat == "coords" {
                                    self.duke_coords_on = !self.duke_coords_on;
                                    self.messages.push(
                                        if self.duke_coords_on {
                                            "Coordinates ON"
                                        } else {
                                            "Coordinates OFF"
                                        },
                                        70,
                                    );
                                    return;
                                }
                                if cheat == "rate" {
                                    self.duke_rate_on = !self.duke_rate_on;
                                    self.messages.push(
                                        if self.duke_rate_on {
                                            "Rate display ON"
                                        } else {
                                            "Rate display OFF"
                                        },
                                        70,
                                    );
                                    return;
                                }
                                if cheat == "debug" {
                                    self.duke_debug_on = !self.duke_debug_on;
                                    self.messages.push(
                                        if self.duke_debug_on {
                                            "Duke debug ON"
                                        } else {
                                            "Duke debug OFF"
                                        },
                                        70,
                                    );
                                    return;
                                }
                                if let Some(session) = self.duke_session.as_mut() {
                                    if session.apply_player_cheat(&cheat).is_some() {
                                        self.messages.push(format!("Cheat: {cheat}"), 70);
                                    }
                                }
                                return;
                            }
                            if prefix {
                                return;
                            }
                        }
                        self.duke_cheat_phase = 0;
                        self.duke_cheat_buffer.clear();
                    } else if code == KeyCode::KeyD {
                        self.duke_cheat_phase = 1;
                    }
                }
                match code {
                    KeyCode::F11 if self.duke_session.is_some() => {
                        if down && !self.duke_brightness_key_down {
                            if let Some(renderer) = self.duke_renderer.as_ref() {
                                let row = renderer.brightness();
                                let next = if self.keys.shift {
                                    if row < 2 { 14 } else { row - 2 }
                                } else if row >= 14 {
                                    0
                                } else {
                                    row + 2
                                };
                                renderer.set_brightness(next);
                                self.messages.push(format!("Brightness {}", next * 2), 60);
                            }
                        }
                        self.duke_brightness_key_down = down;
                    }
                    KeyCode::F1 if down && !menu_open && self.duke_session.is_some() => {
                        self.duke_help_page = 1;
                        self.duke_help_key_down = true;
                        self.keys = Keys::default();
                        self.keys.attack.clear();
                    }
                    KeyCode::F7 if self.duke_session.is_some() => {
                        if down && !self.duke_view_key_down && !menu_open {
                            if let Some(session) = self.duke_session.as_mut() {
                                session.player.over_shoulder_on = !session.player.over_shoulder_on;
                                session.player.shoulder_camera_distance = 0;
                                self.messages.push(
                                    if session.player.over_shoulder_on {
                                        "Third-person view"
                                    } else {
                                        "First-person view"
                                    },
                                    45,
                                );
                            }
                        }
                        self.duke_view_key_down = down;
                    }
                    KeyCode::ArrowLeft
                        if down && menu_open && self.duke_audio_setting_row_selected() =>
                    {
                        if self.duke_audio_autoaim_row_selected() {
                            self.adjust_duke_autoaim(-1);
                        } else {
                            self.adjust_selected_duke_audio_volume(-4);
                        }
                    }
                    KeyCode::ArrowRight
                        if down && menu_open && self.duke_audio_setting_row_selected() =>
                    {
                        if self.duke_audio_autoaim_row_selected() {
                            self.adjust_duke_autoaim(1);
                        } else {
                            self.adjust_selected_duke_audio_volume(4);
                        }
                    }
                    KeyCode::ArrowUp | KeyCode::KeyW if down && menu_open => {
                        let sfx = self.menu.as_mut().and_then(|m| m.move_cursor(-1));
                        if let Some(sfx) = sfx {
                            self.play_menu_sfx(sfx);
                        }
                    }
                    KeyCode::ArrowDown | KeyCode::KeyS if down && menu_open => {
                        let sfx = self.menu.as_mut().and_then(|m| m.move_cursor(1));
                        if let Some(sfx) = sfx {
                            self.play_menu_sfx(sfx);
                        }
                    }
                    KeyCode::KeyW if !menu_open => self.keys.w = down,
                    KeyCode::KeyA if !menu_open => self.keys.a = down,
                    KeyCode::KeyS if !menu_open => self.keys.s = down,
                    KeyCode::KeyD if !menu_open => self.keys.d = down,
                    KeyCode::Space if !menu_open => self.keys.space = down,
                    KeyCode::KeyC if !menu_open => self.keys.c = down,
                    KeyCode::Slash if !menu_open => self.keys.slash = down,
                    KeyCode::Quote if !menu_open => self.keys.quote = down,
                    KeyCode::Backspace if !menu_open => self.keys.backspace = down,
                    KeyCode::ShiftLeft | KeyCode::ShiftRight if !menu_open => {
                        self.keys.shift = down
                    }
                    KeyCode::ControlLeft | KeyCode::ControlRight | KeyCode::KeyF if !menu_open => {
                        self.keys.attack.set_key(down);
                    }
                    KeyCode::KeyE if !menu_open => {
                        self.keys.use_btn = down;
                        eprintln!(
                            "room: VEHICLE KEY E down={} menu_open={} mapped_to=USE",
                            down, menu_open
                        );
                    }
                    KeyCode::KeyG if !menu_open => {
                        self.apply_console(if down { "+mod1" } else { "-mod1" });
                    }
                    KeyCode::KeyM if !menu_open => {
                        self.apply_console(if down { "+mod2" } else { "-mod2" });
                    }
                    KeyCode::KeyL
                        if down
                            && (self.play.is_some() || self.duke_session.is_some())
                            && !menu_open =>
                    {
                        self.set_play_look(!self.mouse_look);
                    }
                    KeyCode::KeyQ if !menu_open => self.keys.left = down,
                    KeyCode::Digit1 | KeyCode::Numpad1 if down && !menu_open => {
                        self.keys.weapon = 1;
                    }
                    KeyCode::Digit2 | KeyCode::Numpad2 if down && !menu_open => {
                        self.keys.weapon = 2;
                    }
                    KeyCode::Digit3 | KeyCode::Numpad3 if down && !menu_open => {
                        self.keys.weapon = 3;
                    }
                    KeyCode::Digit4 | KeyCode::Numpad4 if down && !menu_open => {
                        self.keys.weapon = 4;
                    }
                    KeyCode::Digit5 | KeyCode::Numpad5 if down && !menu_open => {
                        self.keys.weapon = 5;
                    }
                    KeyCode::Digit6 | KeyCode::Numpad6 if down && !menu_open => {
                        self.keys.weapon = 6;
                    }
                    KeyCode::Digit7 | KeyCode::Numpad7 if down && !menu_open => {
                        self.keys.weapon = 7;
                    }
                    KeyCode::Digit8 | KeyCode::Numpad8 if down && !menu_open => {
                        self.keys.weapon = 8;
                    }
                    KeyCode::Digit9 | KeyCode::Numpad9 if down && !menu_open => {
                        self.keys.weapon = 9;
                    }
                    KeyCode::Digit0 | KeyCode::Numpad0 if down && !menu_open => {
                        self.keys.weapon = 10;
                    }
                    KeyCode::BracketRight if down && !menu_open => {
                        self.keys.weapon_cycle = self.keys.weapon_cycle.saturating_add(1);
                    }
                    KeyCode::BracketLeft if down && !menu_open => {
                        self.keys.weapon_cycle = self.keys.weapon_cycle.saturating_sub(1);
                    }
                    KeyCode::Tab if down && !menu_open => self.automap = !self.automap,
                    KeyCode::Equal | KeyCode::NumpadAdd if down && self.automap && !menu_open => {
                        self.zoom_automap(1.25);
                    }
                    KeyCode::Minus | KeyCode::NumpadSubtract
                        if down && self.automap && !menu_open =>
                    {
                        self.zoom_automap(1.0 / 1.25);
                    }
                    KeyCode::KeyP if down && self.play.is_some() => self.toggle_pause(),
                    KeyCode::F2 if down && !menu_open && self.duke_session.is_some() => {
                        if self
                            .duke_session
                            .as_ref()
                            .is_some_and(|session| session.player.extra > 0)
                        {
                            self.menu = Some(MenuStack {
                                stack: vec![duke_save_menu(false, 0)],
                                ..MenuStack::default()
                            });
                            self.set_play_look(false);
                        } else {
                            self.messages.push("Cannot save while dead", 70);
                        }
                    }
                    KeyCode::F3 if down && !menu_open && self.duke_session.is_some() => {
                        self.menu = Some(MenuStack {
                            stack: vec![duke_save_menu(true, 0)],
                            ..MenuStack::default()
                        });
                        self.set_play_look(false);
                    }
                    KeyCode::F6 if down && !menu_open && self.duke_session.is_some() => {
                        if let Some(slot) = self.duke_last_save_slot {
                            if self
                                .duke_session
                                .as_ref()
                                .is_some_and(|session| session.player.extra > 0)
                            {
                                self.menu = Some(MenuStack {
                                    stack: vec![duke_save_menu(false, slot)],
                                    ..MenuStack::default()
                                });
                                self.set_play_look(false);
                            } else {
                                self.messages.push("Cannot save while dead", 70);
                            }
                        } else {
                            self.menu = Some(MenuStack {
                                stack: vec![duke_save_menu(false, 0)],
                                ..MenuStack::default()
                            });
                            self.set_play_look(false);
                        }
                    }
                    KeyCode::F8 if down && !menu_open && self.duke_session.is_some() => {
                        if let Some(session) = self.duke_session.as_mut() {
                            session.player.fta_enabled = !session.player.fta_enabled;
                            // Chocolate uses a forced FTA quote for this
                            // notification, so it remains visible even when
                            // ordinary first-time messages are disabled.
                            session.player.quote_id = 0;
                            session.player.quote_timer = 0;
                            self.messages.push(
                                if session.player.fta_enabled {
                                    "Messages ON"
                                } else {
                                    "Messages OFF"
                                },
                                70,
                            );
                        }
                    }
                    KeyCode::F4 if down && self.duke_session.is_some() => {
                        self.open_duke_audio_menu();
                    }
                    KeyCode::F5 if !down => self.duke_music_key_down = false,
                    KeyCode::F5
                        if down
                            && !self.duke_music_key_down
                            && self.duke_session.is_some()
                            && self.keys.shift =>
                    {
                        self.duke_music_key_down = true;
                        self.cycle_duke_music();
                    }
                    KeyCode::F5
                        if down && !self.duke_music_key_down && self.duke_session.is_some() =>
                    {
                        self.duke_music_key_down = true;
                        self.show_duke_music();
                    }
                    KeyCode::F5 if down && !self.duke_music_key_down => {
                        self.duke_music_key_down = true;
                        self.hud.fullscreen = !self.hud.fullscreen;
                        self.messages.push(
                            if self.hud.fullscreen {
                                "Fullscreen HUD"
                            } else {
                                "Status bar"
                            },
                            35,
                        );
                    }
                    KeyCode::F5 if down => {}
                    KeyCode::F9 if down && !menu_open && self.duke_session.is_some() => {
                        if let Some(slot) = self.duke_last_save_slot {
                            self.menu = Some(MenuStack {
                                stack: vec![duke_quickload_confirm_menu(slot)],
                                ..MenuStack::default()
                            });
                            self.set_play_look(false);
                        } else {
                            self.menu = Some(MenuStack {
                                stack: vec![duke_save_menu(true, 0)],
                                ..MenuStack::default()
                            });
                            self.set_play_look(false);
                        }
                    }
                    KeyCode::F10 if down && !menu_open && self.duke_session.is_some() => {
                        self.open_duke_quit_confirmation();
                    }
                    KeyCode::ArrowLeft if !menu_open => self.keys.left = down,
                    KeyCode::ArrowRight if !menu_open => self.keys.right = down,
                    KeyCode::Enter | KeyCode::NumpadEnter if down => {
                        if self.menu.is_some() {
                            self.menu_activate(event_loop);
                        } else {
                            self.play_advance();
                        }
                    }
                    KeyCode::F5 if down && self.play.is_some() => {
                        self.open_graphics_menu();
                    }
                    KeyCode::F6 if down && self.play.is_some() => {
                        self.apply_console("god");
                    }
                    KeyCode::F7 if down && self.play.is_some() => {
                        self.apply_console("noclip");
                    }
                    KeyCode::F8 if down && self.play.is_some() => {
                        self.apply_console("give all");
                    }
                    KeyCode::F9 if down && self.play.is_some() => {
                        self.apply_console("kill monsters");
                    }
                    KeyCode::F11 if down => {
                        self.toggle_gif_record();
                    }
                    KeyCode::F12 if down => {
                        self.request_screenshot();
                    }
                    KeyCode::Escape if down => {
                        if self.duke_session.is_some() {
                            if self.duke_menu.is_some() {
                                self.duke_menu_key(room_duke::menu::MenuKey::Escape, event_loop);
                            } else if self.menu.is_some() {
                                self.close_duke_menu();
                            } else {
                                self.open_duke_pause_menu();
                            }
                            return;
                        }
                        let in_level = self
                            .play
                            .as_ref()
                            .is_some_and(|g| g.state == GameState::Level);
                        if in_level {
                            self.toggle_pause();
                        } else if self.menu.as_ref().is_some_and(|m| m.stack.len() > 1) {
                            if let Some(sfx) = self.menu.as_mut().and_then(|m| m.backup().1) {
                                self.play_menu_sfx(sfx);
                            }
                        } else {
                            self.leave(event_loop);
                        }
                    }
                    _ => {
                        if let Some(name) = keycode_keyconf_name(code) {
                            if let Some(cmd) = self.keyconf_binds.get(name).cloned() {
                                if down {
                                    self.apply_console(&cmd);
                                } else if let Some(rest) = cmd.trim().strip_prefix('+') {
                                    // Release: `+reload` → `-reload` (BD4 hold semantics).
                                    self.apply_console(&format!("-{rest}"));
                                }
                            }
                        }
                    }
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                if self.menu.is_some() {
                    let (fb_w, fb_h) = self
                        .config
                        .as_ref()
                        .map(|c| (c.width as f32, c.height as f32))
                        .unwrap_or((1280.0, 720.0));
                    let scale = (fb_w / 320.0).min(fb_h / 200.0);
                    let ox = (fb_w - 320.0 * scale) * 0.5;
                    let oy = (fb_h - 200.0 * scale) * 0.5;
                    let vx = ((position.x as f32 - ox) / scale).clamp(0.0, 320.0);
                    let vy = ((position.y as f32 - oy) / scale).clamp(0.0, 200.0);
                    self.menu_cursor = (vx, vy);
                    if let Some(sfx) = self.menu.as_mut().and_then(|m| m.hover(vx, vy)) {
                        self.play_menu_sfx(sfx);
                    }
                } else if self.mouse_look {
                    // macOS (no Locked): pull cursor back before it hits the screen edge.
                    let size = self.window.as_ref().map(|w| w.inner_size());
                    if let Some(size) = size {
                        let m = 48.0;
                        let x = position.x;
                        let y = position.y;
                        if x < m
                            || y < m
                            || x > f64::from(size.width).max(m + 1.0) - m
                            || y > f64::from(size.height).max(m + 1.0) - m
                        {
                            self.recenter_cursor_for_look();
                        }
                    }
                }
            }
            WindowEvent::MouseInput {
                state,
                button: MouseButton::Left,
                ..
            } if state == ElementState::Pressed && self.try_skip_title_splash() => {}
            WindowEvent::MouseInput {
                state,
                button: MouseButton::Left,
                ..
            } if self.play.is_some() && self.menu.is_some() && state == ElementState::Pressed => {
                let (vx, vy) = self.menu_cursor;
                if self
                    .menu
                    .as_ref()
                    .and_then(|m| m.top())
                    .and_then(|t| t.hit_test(vx, vy))
                    .is_some()
                {
                    self.menu_activate(event_loop);
                }
            }
            WindowEvent::MouseInput {
                state,
                button: MouseButton::Left,
                ..
            } if (self.play.is_some() || self.duke_session.is_some()) && self.menu.is_none() => {
                self.keys
                    .attack
                    .set_window_left(state == ElementState::Pressed);
            }
            WindowEvent::MouseInput {
                state,
                button: MouseButton::Right,
                ..
            } => {
                // Viewer: hold RMB to look. Play: RMB = alt attack (`docs/31` MA4).
                if self.play.is_none() && self.duke_session.is_none() {
                    self.set_play_look(state == ElementState::Pressed);
                } else if self.menu.is_none() {
                    self.keys.alt_attack = state == ElementState::Pressed;
                }
            }
            WindowEvent::Focused(focused) => {
                self.unfocused_since = (!focused).then(std::time::Instant::now);
                if focused {
                    if self.mouse_look {
                        self.set_play_look(true);
                    }
                } else {
                    // Never clear mouse attack on focus loss — macOS Locked/hide can
                    // blip focus around a click and was wiping LMB before the tic sample.
                    self.keys.attack.set_key(false);
                }
            }
            WindowEvent::MouseWheel { delta, .. }
                if self.play.is_some() && !self.automap && self.menu.is_none() =>
            {
                // GZDoom: wheel up = next weapon, wheel down = previous.
                let steps = match delta {
                    winit::event::MouseScrollDelta::LineDelta(_, y) => y,
                    winit::event::MouseScrollDelta::PixelDelta(p) => p.y as f32 / 40.0,
                };
                self.keys.wheel_remainder += steps;
                #[allow(clippy::cast_possible_truncation)]
                let notches = self.keys.wheel_remainder.trunc() as i32;
                self.keys.wheel_remainder -= notches as f32;
                if notches != 0 {
                    self.keys.weapon_cycle = self.keys.weapon_cycle.saturating_add(notches);
                }
            }
            WindowEvent::MouseWheel { delta, .. } if self.automap && self.menu.is_none() => {
                let steps = match delta {
                    winit::event::MouseScrollDelta::LineDelta(_, y) => y,
                    winit::event::MouseScrollDelta::PixelDelta(p) => p.y as f32 / 40.0,
                };
                if steps > 0.0 {
                    self.zoom_automap(1.0 + 0.15 * steps);
                } else if steps < 0.0 {
                    self.zoom_automap(1.0 / (1.0 + 0.15 * (-steps)));
                }
            }
            WindowEvent::Touch(touch) if self.launcher.is_some() => self.launcher_touch(&touch),
            WindowEvent::Touch(touch) => self.touch_event(touch),
            WindowEvent::RedrawRequested => {
                if self.lifecycle.is_running() {
                    self.frame();
                }
                if self.quit {
                    self.quit = false;
                    self.leave(event_loop);
                }
            }
            _ => {}
        }
    }

    fn device_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _device_id: winit::event::DeviceId,
        event: DeviceEvent,
    ) {
        if self
            .unfocused_since
            .is_some_and(|t| t.elapsed() > std::time::Duration::from_millis(250))
        {
            self.keys.attack.set_device_left(false);
            return;
        }
        match event {
            DeviceEvent::MouseMotion { delta: (dx, dy) } if self.mouse_look => {
                if self.ignore_warp_motion {
                    self.ignore_warp_motion = false;
                } else {
                    #[allow(clippy::cast_possible_truncation)]
                    {
                        // Mouse right → look right: Doom BAM +angleturn is CCW, so
                        // positive screen dx must produce negative angleturn.
                        // Mid-tic preview must match `build_ticcmd` mouse scale.
                        let rad_per_count = (room_input::MOUSE_DEG_PER_COUNT as f32)
                            * (std::f32::consts::PI / 180.0);
                        let invert_y = matches!(
                            self.console.cvars.get("invertmouse"),
                            Some(room_console::CvarValue::Bool(true))
                        );
                        let sens = match self.console.cvars.get("m_sensitivity") {
                            Some(room_console::CvarValue::Float(f)) => (*f).clamp(0.1, 5.0) as f32,
                            _ => 1.0,
                        };
                        // Preview uses the same rounded counts as `build_ticcmd`
                        // so the crosshair never drifts ahead of thrust direction.
                        let yaw_add = ((-dx as f32) * sens).round() as i32;
                        let pitch_delta = if invert_y { dy } else { -dy };
                        let pitch_add = ((pitch_delta as f32) * sens).round() as i32;
                        self.yaw += yaw_add as f32 * rad_per_count;
                        self.pitch =
                            (self.pitch + pitch_add as f32 * rad_per_count).clamp(-1.5, 1.5);
                        self.mouse_yaw_accum = self
                            .mouse_yaw_accum
                            .saturating_add(yaw_add.clamp(-4096, 4096));
                        self.mouse_pitch_accum = self
                            .mouse_pitch_accum
                            .saturating_add(pitch_add.clamp(-4096, 4096));
                    }
                    self.sync_forward();
                }
            }
            // Raw buttons while grabbed (primary path on macOS Locked cursor).
            // Ids are OS-specific — never treat right as left.
            DeviceEvent::Button { button, state }
                if (self.play.is_some() || self.duke_session.is_some())
                    && self.menu.is_none()
                    && device_button_is_primary(button) =>
            {
                self.keys
                    .attack
                    .set_device_left(state == ElementState::Pressed);
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        self.poll_gamepad(event_loop);
        if self.launcher.is_some() {
            self.play_test_touches();
            self.prepare_launcher_frame();
        } else {
            self.poll_touch(event_loop);
        }
        self.update_audio_frame();
        if let Some(every) = self.background_test_every {
            if self.frames > 0
                && self.frames % every == 0
                && self.frames != self.background_test_last
                && self.surface.is_some()
            {
                self.background_test_last = self.frames;
                ApplicationHandler::suspended(self, event_loop);
                ApplicationHandler::resumed(self, event_loop);
            }
        }
        if let Some((frame, secs)) = self.away_test
            && self.frames >= frame
            && self.surface.is_some()
        {
            self.away_test = None;
            self.away_test_until =
                Some(std::time::Instant::now() + std::time::Duration::from_secs_f32(secs));
            ApplicationHandler::suspended(self, event_loop);
        }
        if let Some(until) = self.away_test_until
            && std::time::Instant::now() >= until
        {
            self.away_test_until = None;
            ApplicationHandler::resumed(self, event_loop);
        }
        if let Some(last) = self.net_away
            && !self.lifecycle.is_running()
        {
            // Off screen in netplay: run the game's tics with an empty
            // command, without drawing, at the tic rate.
            let now = std::time::Instant::now();
            let dt = now.duration_since(last).as_secs_f32().min(0.25);
            self.net_away = Some(now);
            self.tick_play(dt);
            let rate = self.play.as_ref().map_or(35, GameRuntime::tic_rate).max(1);
            let next = now + std::time::Duration::from_secs_f32(1.0 / rate as f32);
            let next = self.away_test_until.map_or(next, |u| next.min(u));
            event_loop.set_control_flow(ControlFlow::WaitUntil(next));
        }
        if let Some(w) = self.window.clone() {
            if self.lifecycle.is_running() {
                self.schedule_frame(event_loop, &w);
            }
        }
    }
}

impl ViewerApp {
    /// Native Duke vertical mouse-look gain (`m_pitch` cvar, default 1.25):
    /// Build's y-shearing view needs more horiz per degree than true pitch
    /// for the look to feel as quick as horizontal turning.
    fn duke_pitch_scale(&self) -> f64 {
        match self.console.cvars.get("m_pitch") {
            Some(room_console::CvarValue::Float(f)) => (*f).clamp(0.1, 10.0),
            _ => 1.25,
        }
    }

    /// Per-frame audio state: pause (`S_PauseSound`; menus over a running
    /// single-player game count) and the listener's sound environment.
    fn update_audio_frame(&mut self) {
        let Some(audio) = self.audio.as_ref() else {
            return;
        };
        let cvar_on = |name: &str| {
            !matches!(
                self.console.cvars.get(name),
                Some(room_console::CvarValue::Bool(false))
            )
        };
        let reverb_on = cvar_on("snd_reverb");
        let water = cvar_on("snd_waterreverb");
        let (paused, env, underwater) = match self.play.as_ref() {
            Some(game) if game.state == GameState::Level => {
                // `PowerTimeFreezer` pauses world sound too (`S_PauseSound`).
                let paused = game
                    .world()
                    .is_some_and(|w| w.paused || w.level.frozen_power);
                // `snd_reverb` off is UZDoom without an effect slot: no
                // reverb and no water filter, but the pitch still drops.
                let heard = game.listener_sound(self.local_seat, water && reverb_on);
                let env = if reverb_on { heard.env.cloned() } else { None };
                (paused, env, heard.underwater)
            }
            _ => (false, None, false),
        };
        if let Ok(mut shared) = audio.shared.lock() {
            shared.mixer.set_paused(paused);
            shared.mixer.set_environment(env.as_ref());
            shared.mixer.set_underwater(underwater, water && reverb_on);
        }
    }

    /// `joy_*` CVars → controller tuning.
    fn gamepad_config(&self) -> room_input::GamepadConfig {
        let f = |n: &str, d: f32| match self.console.cvars.get(n) {
            Some(room_console::CvarValue::Float(v)) => *v as f32,
            Some(room_console::CvarValue::Int(v)) => *v as f32,
            _ => d,
        };
        let d = room_input::GamepadConfig::default();
        room_input::GamepadConfig {
            deadzone: f("joy_deadzone", d.deadzone),
            curve: f("joy_curve", d.curve),
            look_sensitivity: f("joy_sensitivity", d.look_sensitivity),
            move_sensitivity: f("joy_speedmultiplier", d.move_sensitivity),
            invert_pitch: matches!(
                self.console.cvars.get("joy_invertpitch"),
                Some(room_console::CvarValue::Bool(true))
            ),
            trigger_threshold: d.trigger_threshold,
        }
    }

    /// Sample the controller; drive menus with D-pad / A / B / Start.
    fn poll_gamepad(&mut self, event_loop: &ActiveEventLoop) {
        use room_input::PadButton as B;
        let enabled = !matches!(
            self.console.cvars.get("use_joystick"),
            Some(room_console::CvarValue::Bool(false))
        );
        let (state, newly) = self.pad_backend.poll();
        if let Some(name) = newly {
            self.messages.push(format!("Controller: {name}"), 70);
        }
        let state = state.filter(|_| enabled);
        let buttons = state.map_or(0, |s| s.buttons);
        let pressed = buttons & !self.pad_prev_buttons;
        self.pad_prev_buttons = buttons;
        self.pad_pressed_since_tic |= pressed;
        self.pad = state;
        let Some(state) = state else {
            return;
        };
        let hit = |b: B| pressed & (1 << (b as u8)) != 0;
        if self.launcher.is_some() {
            use crate::launcher_screen::Nav;
            for (button, nav) in [
                (B::DPadUp, Nav::Up),
                (B::DPadDown, Nav::Down),
                (B::South, Nav::Enter),
                (B::Start, Nav::Enter),
                (B::East, Nav::Back),
                (B::Back, Nav::Back),
            ] {
                if hit(button) {
                    self.launcher_nav(nav);
                }
            }
            let _ = state;
            return;
        }
        if self.menu.is_some() {
            let up = hit(B::DPadUp);
            let down = hit(B::DPadDown);
            if up || down {
                let sfx = self
                    .menu
                    .as_mut()
                    .and_then(|m| m.move_cursor(if up { -1 } else { 1 }));
                if let Some(sfx) = sfx {
                    self.play_menu_sfx(sfx);
                }
            }
            if hit(B::South) || hit(B::Start) {
                self.menu_activate(event_loop);
            } else if hit(B::East) || hit(B::Back) {
                let in_level = self
                    .play
                    .as_ref()
                    .is_some_and(|g| g.state == GameState::Level);
                if self.menu.as_ref().is_some_and(|m| m.stack.len() > 1) {
                    if let Some(sfx) = self.menu.as_mut().and_then(|m| m.backup().1) {
                        self.play_menu_sfx(sfx);
                    }
                } else if in_level {
                    self.toggle_pause();
                }
            }
            let _ = state;
            return;
        }
        if hit(B::Start) {
            let in_level = self
                .play
                .as_ref()
                .is_some_and(|g| g.state == GameState::Level);
            if in_level {
                self.toggle_pause();
            } else {
                self.play_advance();
            }
        } else if hit(B::Back) && self.play.is_some() {
            self.automap = !self.automap;
        } else if hit(B::South)
            && self
                .play
                .as_ref()
                .is_some_and(|g| g.state != GameState::Level)
        {
            // Skip intermission / finale text like Enter.
            self.play_advance();
        }
    }

    /// Controller keys for this tic: held binds, with weapon cycling only on
    /// the press (a held shoulder must not spin through every weapon).
    fn gamepad_tic_keys(&mut self) -> Vec<Key> {
        let pressed = std::mem::take(&mut self.pad_pressed_since_tic);
        let Some(pad) = self.pad else {
            return Vec::new();
        };
        let cfg = self.gamepad_config();
        let binds = room_input::default_pad_binds();
        let mut keys = room_input::gamepad_keys(&pad, &binds, &cfg);
        keys.retain(|k| {
            let edge_only = matches!(k, Key::NextWeapon | Key::PrevWeapon);
            !edge_only
                || binds
                    .iter()
                    .any(|(b, bk)| bk == k && pressed & (1 << (*b as u8)) != 0)
        });
        keys
    }

    fn play_duke_rts_sound(&mut self, index: usize) -> bool {
        let sound_enabled = !matches!(
            self.console.cvars.get("duke_sound"),
            Some(room_console::CvarValue::Bool(false))
        );
        let lockout = matches!(
            self.console.cvars.get("duke_lockout"),
            Some(room_console::CvarValue::Bool(true))
        );
        if !sound_enabled || lockout {
            return false;
        }
        let Some(audio) = self.audio.as_ref() else {
            return false;
        };
        let Ok(mut shared) = audio.shared.lock() else {
            return false;
        };
        if shared.mixer.voices.iter().any(|voice| {
            let raw = voice.sound.raw();
            (DUKE_RTS_SOUND_BASE..DUKE_RTS_SOUND_BASE + 10).contains(&raw)
        }) {
            return false;
        }
        let sound = SoundId::from_raw(DUKE_RTS_SOUND_BASE + index as u32);
        if shared.mixer.bank.get(sound).is_none() {
            return false;
        }
        shared.mixer.apply(&[SoundEvent::Start {
            origin: SoundOrigin::None,
            channel: -100 - i32::try_from(index).unwrap_or(0),
            sound,
            check_sound: None,
            start_time: 0.0,
            volume: 1.0,
            attenuation: 1.0,
            pitch: 1.0,
            flags: SoundFlags::default(),
            source_id: None,
            no_restart: false,
        }]);
        true
    }

    fn zoom_automap(&mut self, factor: f32) {
        self.automap_scale = (self.automap_scale * factor).clamp(0.06, 4.0);
    }

    /// Drop hitscan chips / particles and restore sector lights from the loaded map.
    fn reset_impact_fx(&mut self) {
        self.decals.clear();
        self.particles.clear();
        self.map = self.map_base.clone();
        // `map_base` still has pre-flip SW1* names — re-apply runtime switch pics.
        self.sync_switch_textures_from_world();
    }

    /// Copy `World` switch mid/lower/upper overrides onto the render map.
    ///
    /// Called every frame before scene build so a `map_base` restore (impact FX
    /// reset) cannot leave SW1* on screen after the sim already flipped to SW2*.
    fn sync_switch_textures_from_world(&mut self) {
        let Some(world) = self.play.as_ref().and_then(|g| g.world()) else {
            return;
        };
        // Moving polyobjects and changed portals (`Line_SetPortalTarget`,
        // polyobject portals) reach the render copy of the map.
        room_map::portals::sync_dynamic_geometry(&mut self.map, &world.map);
        for (&i, name) in &world.switch_mids {
            if let Some(side) = self.map.sides.get_mut(i as usize) {
                side.middle = name.clone();
            }
        }
        for (&i, name) in &world.switch_lowers {
            if let Some(side) = self.map.sides.get_mut(i as usize) {
                side.lower = name.clone();
            }
        }
        for (&i, name) in &world.switch_uppers {
            if let Some(side) = self.map.sides.get_mut(i as usize) {
                side.upper = name.clone();
            }
        }
    }

    fn bind_iwad_albedo(&mut self, gpu: &GpuContext, pass: &mut UnlitPass) {
        let Some(vfs) = self.vfs.as_ref() else {
            return;
        };
        self.materials.load_playpal(vfs);
        self.materials.load_texturex(vfs);
        self.materials.ensure_sampler(&gpu.device);
        // Soft white discs for ACS SpawnParticle / additive FX billboards.
        let white = soft_disc_rgba([255, 255, 255]);
        let _ = self
            .materials
            .upsert_rgba(&gpu.device, &gpu.queue, PARTICLE_TEX, 16, 16, &white);
        let _ =
            self.materials
                .upsert_rgba(&gpu.device, &gpu.queue, PARTICLE_TEX_ADD, 16, 16, &white);
        // Prefer a midtexture from the first sidedef, else a common Doom wall.
        let candidates: Vec<String> = self
            .map
            .sides
            .iter()
            .flat_map(|s| [s.middle.clone(), s.upper.clone(), s.lower.clone()])
            .filter(|n| {
                let t = n.trim();
                !t.is_empty() && t != "-"
            })
            .collect();
        let mut names = candidates;
        names.extend(
            ["STARTAN2", "STONE2", "BIGBRIK1", "FLOOR0_1", "CEIL1_1"]
                .iter()
                .map(|s| (*s).to_owned()),
        );
        for name in &names {
            if let Some(id) = self
                .materials
                .get_or_upload(&gpu.device, &gpu.queue, vfs, name)
            {
                if let (Some(entry), Some(samp)) = (
                    self.materials.textures.get(id),
                    self.materials.sampler.as_ref(),
                ) {
                    if let Some(view) = entry.view.as_ref() {
                        pass.set_albedo(&gpu.device, view, samp);
                        eprintln!("room: bound albedo {name}");
                        return;
                    }
                }
            }
        }
    }

    /// Load the HUD's pictures before play (level start, and when the player
    /// switches to a weapon not loaded yet): the status bar's `ST*`
    /// pictures and every frame of the local player's weapons, decoded on
    /// several threads. Firing and reloading then never stop to decode a
    /// frame. World sprites load as they appear, spread over frames
    /// ([`room_render::MaterialCatalog::set_stream_budget`]).
    fn precache_hud(&mut self, status_bar: bool) {
        let started = std::time::Instant::now();
        let (Some(vfs), Some(game)) = (self.vfs.as_ref(), self.play.as_ref()) else {
            return;
        };
        let Some(world) = game.world() else {
            return;
        };
        let mut weapons: Vec<room_core::ClassId> = Vec::new();
        if let Some(mo) = world
            .players
            .get(usize::from(self.local_seat))
            .and_then(|p| p.as_ref())
            .and_then(|p| p.mo)
        {
            for item in world.inventory.iter(mo) {
                if world.classes.inherits_from(item.class, "Weapon") {
                    weapons.push(item.class);
                }
            }
        }
        let mut names: Vec<String> = Vec::new();
        let mut new_weapons = 0usize;
        for class in weapons {
            if !self.precached_weapons.insert(class) {
                continue;
            }
            new_weapons += 1;
            for (sprite, frame) in weapon_hud_frames(world, class) {
                match self.patches.sprite_frame_lump(vfs, &sprite, frame, 1) {
                    Some((name, _)) => names.push(name),
                    None => {
                        if let Some(letter) = b'A'.checked_add(frame).filter(|c| *c <= b']') {
                            names.push(format!("{sprite}{}0", char::from(letter)));
                        }
                    }
                }
            }
        }
        if status_bar {
            for (_, info) in vfs.iter() {
                let name = info.short_name.as_str().to_ascii_uppercase();
                if info.namespace != room_vfs::Namespace::Sprites
                    && (name.starts_with("ST") || name.starts_with("SBAR"))
                {
                    names.push(name);
                }
            }
        }
        if names.is_empty() {
            return;
        }
        let pictures = self.patches.preload(vfs, &names);
        eprintln!(
            "room: loaded {pictures} HUD pictures ({new_weapons} weapon(s)) in {:.0} ms",
            started.elapsed().as_secs_f64() * 1000.0
        );
    }

    /// Upload every wall/flat name referenced by the loaded map (avoids first-frame stalls).
    fn preload_map_materials(&mut self, gpu: &GpuContext) {
        let Some(vfs) = self.vfs.as_ref() else {
            return;
        };
        self.materials.ensure_sampler(&gpu.device);
        let mut names = std::collections::BTreeSet::new();
        for s in &self.map.sides {
            for n in [&s.middle, &s.upper, &s.lower] {
                let t = n.trim();
                if !t.is_empty() && t != "-" {
                    names.insert(t.to_ascii_uppercase());
                }
            }
        }
        for sec in &self.map.sectors {
            for n in [&sec.floor_pic, &sec.ceiling_pic] {
                let t = n.trim();
                if !t.is_empty() && t != "-" {
                    names.insert(t.to_ascii_uppercase());
                }
            }
        }
        // Eager-load common monster/projectile frames so the first sight of a
        // zombie never races a missing GPU texture (looks like an invisible killer).
        for base in ["POSS", "SPOS", "TROO", "BAL1", "PLAY", "PUFF"] {
            for frame in b'A'..=b'H' {
                let letter = frame as char;
                for rot in ['0', '1', '2', '3', '4', '5'] {
                    names.insert(format!("{base}{letter}{rot}"));
                }
                names.insert(format!("{base}{letter}2{letter}8"));
                names.insert(format!("{base}{letter}3{letter}7"));
                names.insert(format!("{base}{letter}4{letter}6"));
            }
        }
        let mut ok = 0u32;
        let mut sprites_ok = 0u32;
        for name in names {
            if self
                .materials
                .get_or_upload(&gpu.device, &gpu.queue, vfs, &name)
                .is_some()
            {
                ok += 1;
                if name.starts_with("POSS")
                    || name.starts_with("SPOS")
                    || name.starts_with("TROO")
                    || name.starts_with("BAL")
                {
                    sprites_ok += 1;
                }
            }
        }
        eprintln!("room: preloaded {ok} map+sprite materials ({sprites_ok} monster frames)");
    }

    fn sync_forward(&mut self) {
        // Render-only kick (I5): offset look without touching sim angles.
        let yaw = self.yaw + self.view_kick.1;
        let pitch = (self.pitch + self.view_kick.0).clamp(-1.5, 1.5);
        let cp = pitch.cos();
        self.camera.forward =
            Vec3::new(yaw.cos() * cp, yaw.sin() * cp, pitch.sin()).normalize_or_zero();
        if self.camera.forward.length_squared() < 1e-6 {
            self.camera.forward = Vec3::X;
        }
        self.camera.up = if self.roll.abs() > 1e-4 {
            room_render::Camera::rolled_up(self.camera.forward, self.roll)
        } else {
            Vec3::Z
        };
    }

    /// Upload soft tint discs used by blood / muzzle / impact particles.
    /// DECALDEF look for a placed decal: picture size × `x-scale`/`y-scale`,
    /// `translucent`, wall light (unless `fullbright`), and `shade "rr gg bb"`
    /// (the picture becomes an alpha mask filled with that colour — Brutal
    /// Doom's bullet holes are grey pictures shaded black, not white blobs).
    fn decal_look(&mut self, placed: &str, tex: &str, salt: u32, x: f64, y: f64) -> DecalLook {
        let mut look = DecalLook {
            texture: None,
            width: 8.0,
            height: 8.0,
            light: 1.0,
            alpha: 1.0,
        };
        let Some(vfs) = self.vfs.as_ref() else {
            return look;
        };
        let def = self
            .decaldef
            .as_ref()
            .and_then(|d| d.place_def(placed, salt))
            .cloned();
        let pal = *self.patches.palette();
        let (pw, ph, rgba) = match self.patches.ensure(vfs, tex) {
            Some(p) => {
                let rgba = p.rgba.clone().unwrap_or_else(|| {
                    let mut out = Vec::with_capacity(p.indices.len() * 4);
                    for (i, &ix) in p.indices.iter().enumerate() {
                        let c = pal[usize::from(ix)];
                        out.extend_from_slice(&[
                            c[0],
                            c[1],
                            c[2],
                            p.alpha.get(i).copied().unwrap_or(255),
                        ]);
                    }
                    out
                });
                (f32::from(p.width), f32::from(p.height), Some(rgba))
            }
            None => (8.0, 8.0, None),
        };
        let Some(def) = def else {
            return look;
        };
        #[allow(clippy::cast_possible_truncation)]
        {
            look.width = (pw * def.x_scale.unwrap_or(1.0) as f32).max(0.5);
            look.height = (ph * def.y_scale.unwrap_or(1.0) as f32).max(0.5);
            look.alpha = def.translucent.map_or(1.0, |t| t.clamp(0.0, 1.0) as f32);
        }
        if !def.fullbright {
            if let Some(world) = self.play.as_ref().and_then(|g| g.world()) {
                if let Some(sid) = room_nodebuild::point_in_sector(&world.map, x, y) {
                    if let Some(sec) = world.sectors.get(sid.raw() as usize) {
                        look.light = (f32::from(sec.light) / 255.0).clamp(0.1, 1.0);
                    }
                }
            }
        }
        if let (Some(shade), Some(rgba)) = (def.shade.as_deref(), rgba) {
            let parts: Vec<u8> = shade
                .split_whitespace()
                .filter_map(|p| u8::from_str_radix(p, 16).ok())
                .collect();
            if parts.len() == 3 {
                let name = format!(
                    "__DSHADE_{}_{:02X}{:02X}{:02X}",
                    tex.to_ascii_uppercase(),
                    parts[0],
                    parts[1],
                    parts[2]
                );
                if self.materials.get(&name).is_none() {
                    if let Some(gpu) = self.gpu.as_ref() {
                        // GZDoom shaded decals: alpha = picture luminance.
                        let mut out = Vec::with_capacity(rgba.len());
                        for px in rgba.chunks_exact(4) {
                            let lum = (u32::from(px[0]) * 54
                                + u32::from(px[1]) * 183
                                + u32::from(px[2]) * 19)
                                >> 8;
                            let a = (lum * u32::from(px[3]) / 255) as u8;
                            out.extend_from_slice(&[parts[0], parts[1], parts[2], a]);
                        }
                        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                        let _ = self.materials.upsert_rgba(
                            &gpu.device,
                            &gpu.queue,
                            &name,
                            pw as u32,
                            ph as u32,
                            &out,
                        );
                    }
                }
                if self.materials.get(&name).is_some() {
                    look.texture = Some(name);
                    look.light = 1.0;
                }
            }
        }
        look
    }

    fn ensure_particle_tint_textures(
        materials: &mut MaterialCatalog,
        particles: &ParticleStore,
        gpu: &GpuContext,
    ) {
        let mut needed: std::collections::BTreeSet<[u8; 3]> = std::collections::BTreeSet::new();
        for p in &particles.particles {
            let q = [
                ((p.color[0].clamp(0.0, 1.0) * 31.0).round() as u8).saturating_mul(8),
                ((p.color[1].clamp(0.0, 1.0) * 31.0).round() as u8).saturating_mul(8),
                ((p.color[2].clamp(0.0, 1.0) * 31.0).round() as u8).saturating_mul(8),
            ];
            if q[0] > 240 && q[1] > 240 && q[2] > 240 {
                continue;
            }
            needed.insert(q);
        }
        for rgb in needed {
            let name = particle_tint_tex_name(rgb);
            if materials.get(&name).is_some() {
                continue;
            }
            let rgba = soft_disc_rgba(rgb);
            let _ = materials.upsert_rgba(&gpu.device, &gpu.queue, &name, 16, 16, &rgba);
        }
    }

    /// Hand mod CVar (CVARINFO) values the console changed to the game, so
    /// mods see `set colorworld_mode 2` on their next tic.
    fn sync_mod_cvars(&mut self) {
        let Some(game) = self.play.as_mut() else {
            return;
        };
        for (name, last) in &mut self.mod_cvars {
            let now = self
                .console
                .cvars
                .get(name)
                .map(room_console::CvarValue::display)
                .unwrap_or_default();
            if now != *last {
                game.set_mod_cvar(name, &now);
                *last = now;
            }
        }
    }

    fn frame(&mut self) {
        self.perf.begin();
        self.sync_mod_cvars();
        let started = std::time::Instant::now();
        self.frame_inner();
        if self.frames <= 3 {
            crate::startlog::step(format_args!("frame {}: presented", self.frames));
        }
        // Rollback: the speculative copy was only borrowed for drawing.
        self.swap_rollback_world(false);
        self.perf.end();
        self.update_render_scale(started.elapsed().as_secs_f64() * 1000.0);
        if let Some(n) = self.bench_frames {
            self.bench_frame(n, started.elapsed().as_secs_f64() * 1000.0);
        }
    }

    /// Leave the game: back to the launcher when it started the game,
    /// otherwise close the app (desktop command line, `--shot-after`).
    fn leave(&mut self, event_loop: &ActiveEventLoop) {
        if self.from_launcher {
            crate::startlog::step("leaving the game for the launcher");
            self.back_to_launcher = Some(self.bench_result.take());
        } else {
            event_loop.exit();
        }
    }

    /// `--bench-frames`: skip a 10-frame warm-up, then measure `n` frames,
    /// print the summary and peak memory, and quit.
    fn bench_frame(&mut self, n: u64, ms: f64) {
        const WARM_UP: u64 = 10;
        if self.frames <= WARM_UP {
            return;
        }
        self.frame_stats.push(ms);
        if (self.frame_stats.len() as u64) < n {
            return;
        }
        let report = self.frame_stats.report();
        if let Some(report) = &report {
            eprintln!("room: bench: {report}");
        }
        let peak = crate::bench::peak_rss_bytes();
        match peak {
            Some(b) => eprintln!("room: bench: peak memory {:.1} MB", b as f64 / 1_048_576.0),
            None => eprintln!("room: bench: peak memory not available on this OS"),
        }
        self.bench_result = report.map(|r| crate::bench::short_summary(&r, peak));
        if let Some(now) = crate::bench::resident() {
            eprintln!("room: bench: at the end {now}");
        }
        self.bench_frames = None;
        self.quit = true;
    }

    /// Rollback: put the speculative copy into the game for drawing (`true`)
    /// or put the real world back (`false`).
    fn swap_rollback_world(&mut self, show: bool) {
        if show == self.rollback_swapped {
            return;
        }
        if show && self.rollback_world.is_none() {
            return;
        }
        if let Some(game) = self.play.as_mut() {
            game.swap_world(&mut self.rollback_world);
            self.rollback_swapped = show;
        }
    }

    fn frame_inner(&mut self) {
        // A new level: load every picture its actors can show first (one
        // long frame now instead of a stutter at each new sprite).
        let level = self
            .play
            .as_ref()
            .filter(|g| g.state == GameState::Level)
            .map(|g| g.map_name().to_owned());
        let in_level = level.is_some();
        if let Some(name) = level
            && self.precached_map.as_deref() != Some(name.as_str())
        {
            self.precached_map = Some(name);
            self.precache_hud(true);
            self.perf.loaded();
            // The time spent loading is not game time.
            self.last_frame = std::time::Instant::now();
        } else if in_level {
            // A weapon picked up: load its frames now, not while it fires.
            let weapon = self.hud.weapon_class.clone();
            if !weapon.is_empty() && self.precached_weapon_name != weapon {
                self.precached_weapon_name = weapon;
                self.precache_hud(false);
            }
        }
        // Spread sprite loading over frames during play (4 ms a frame);
        // load at once on menus and loading screens.
        self.materials
            .set_stream_budget(in_level.then(|| std::time::Duration::from_millis(4)));
        if let (Some(gpu), Some(vfs)) = (self.gpu.as_ref(), self.vfs.as_ref()) {
            self.materials.begin_frame(&gpu.device, &gpu.queue, vfs);
        }
        self.perf.mark("loads");
        let elapsed = self.last_frame.elapsed();
        let dt = elapsed.as_secs_f32().min(0.05);
        self.last_frame = std::time::Instant::now();
        self.elapsed_ms = self
            .elapsed_ms
            .saturating_add(u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX));
        if self.duke_session.is_some() {
            self.tick_play(dt);
        } else if self
            .play
            .as_ref()
            .is_some_and(|g| g.state == GameState::Level)
        {
            self.tick_play(dt);
            // Re-apply after tics (and between tics) so switch LEDs stay flipped.
            self.sync_switch_textures_from_world();
            // Rollback: draw this frame from the speculative copy.
            self.swap_rollback_world(true);
        } else if self.play.is_none() {
            self.integrate(dt);
        }
        self.perf.mark("sim");

        self.frames = self.frames.wrapping_add(1);
        if self.frames <= 3 {
            crate::startlog::step(format_args!("frame {}: drawing", self.frames));
        }
        self.fps_count = self.fps_count.wrapping_add(1);
        if self.fps_window.elapsed().as_secs() >= 1 {
            let fps = self.fps_count;
            self.duke_fps = fps;
            self.fps_count = 0;
            self.fps_window = std::time::Instant::now();
            if let Some(w) = &self.window {
                let mode = if self.play.is_some() {
                    "play"
                } else {
                    "viewer"
                };
                w.set_title(&format!("Room {mode} — {fps} fps"));
            }
        }
        self.refresh_camera_textures();
        // `vid_rendermode` 0 / 1: the software renderer draws the world into
        // the HUD canvas and the GPU world lists stay empty (`docs/40` §6).
        let software_view = software_rendermode(&self.console.cvars) && self.ensure_soft_world();
        // Mirror canvases need the current scene's mirror list — updated after build.
        let live_sprites = if software_view {
            None
        } else {
            self.collect_live_sprites()
        };
        let acs_sky = self
            .play
            .as_ref()
            .and_then(|g| g.world())
            .and_then(|w| w.sky1_override.clone());
        if let Some(s) = acs_sky.as_ref() {
            if !s.is_empty() {
                self.sky_texture = s.clone();
                self.sky_cubemap = None;
            }
        }
        let sky = if self.sky_texture.is_empty() {
            None
        } else {
            Some(self.sky_texture.as_str())
        };
        let tex_sizes = self.materials.wall_texel_sizes_shared();
        let mut scene: SceneLists = if software_view {
            SceneLists::default()
        } else {
            // The player's own view: leave out what solid walls hide.
            build_scene_with_sky_ex_shared(
                &self.map,
                &self.camera,
                live_sprites.as_deref(),
                sky,
                self.sky_cubemap.as_ref(),
                Some(std::sync::Arc::clone(&tex_sizes)),
                true,
            )
        };
        let tic = self
            .play
            .as_ref()
            .and_then(|g| g.world())
            .map_or(0, |w| w.level.maptime);
        self.tex_anim.apply(&mut scene, tic);
        scene.dyn_lights = self.collect_dyn_lights();
        // Dyn lights shade on the GPU (`LightUniforms`); keep CPU boost off to
        // avoid double-lighting once the forward path is active.
        let glow = {
            let mut glow = std::collections::BTreeSet::new();
            for n in self
                .gldefs
                .glow_flats
                .iter()
                .chain(self.gldefs.glow_walls.iter())
            {
                glow.insert(n.clone());
            }
            for g in &self.gldefs.glow_textures {
                if g.fullbright {
                    glow.insert(g.name.clone());
                }
            }
            glow
        };
        apply_glow_fullbright(&mut scene, &glow);
        let mut fog_density = 0.0f32;
        let mut thick_fog = (0.0f32, 0.0f32);
        let mut sky_fog = 0.0f32;
        if let Some(world) = self.play.as_ref().and_then(|g| g.world()) {
            thick_fog = world.thick_fog().unwrap_or((0.0, 0.0));
            sky_fog = world.sky_fog;
            // ACS `SetSkyScrollSpeed`: map-units/tic → UV turns (full circle ≈ 360°).
            let du = (world.sky_speed1 * f64::from(world.level.maptime) / 360.0) as f32;
            scroll_sky_u(&mut scene.sky_parts, du);
            fog_density = world.fog_density;
        }
        let in_level = self
            .play
            .as_ref()
            .is_some_and(|g| g.state == GameState::Level);
        if in_level {
            // `self.map` carries the live (interpolated) plane heights, so wall
            // decals on doors and lifts ride their wall part.
            self.decals.anchor_to_planes(&self.map);
            append_decals(&mut scene, &self.decals, &self.camera, Some(&self.map));
            if let Some(gpu) = self.gpu.as_ref() {
                Self::ensure_particle_tint_textures(&mut self.materials, &self.particles, gpu);
                for d in &self.decals.decals {
                    if !d.on_floor || !d.name.starts_with("__PARTICLE_T_") {
                        continue;
                    }
                    if self.materials.get(&d.name).is_some() {
                        continue;
                    }
                    // Recover RGB from `__PARTICLE_T_rrggbb`.
                    if let Ok(hex) =
                        u32::from_str_radix(d.name.trim_start_matches("__PARTICLE_T_"), 16)
                    {
                        let rgb = [
                            ((hex >> 16) & 0xff) as u8,
                            ((hex >> 8) & 0xff) as u8,
                            (hex & 0xff) as u8,
                        ];
                        let rgba = soft_disc_rgba(rgb);
                        let _ = self.materials.upsert_rgba(
                            &gpu.device,
                            &gpu.queue,
                            &d.name,
                            16,
                            16,
                            &rgba,
                        );
                    }
                }
            }
            let anim_tic = self
                .play
                .as_ref()
                .and_then(|g| g.world())
                .map_or(0, |w| w.level.maptime);
            append_particles_animated(
                &mut scene,
                &self.particles,
                &self.camera,
                self.render_settings.particle_fx,
                Some((&self.tex_anim, anim_tic)),
            );
            if let (Some(vfs), poses) = (self.vfs.as_ref(), self.collect_model_poses()) {
                if !poses.is_empty() && !self.modeldefs.by_actor.is_empty() {
                    append_actor_models(&mut scene, &mut self.models, vfs, &self.modeldefs, &poses);
                }
            }
            if let Some(vfs) = self.vfs.as_ref() {
                if !self.voxel_poses.is_empty() {
                    let uploads = self.voxels.append_poses(&mut scene, vfs, &self.voxel_poses);
                    if let Some(gpu) = &self.gpu {
                        for (name, rgba) in &uploads {
                            self.materials
                                .upsert_rgba(&gpu.device, &gpu.queue, name, 256, 1, rgba);
                            self.voxels.mark_palette_uploaded(name);
                        }
                    }
                }
            }
        }
        self.refresh_plane_reflect_textures(&scene.plane_reflects);
        if self.shot_after.is_some_and(|n| self.frames >= n) {
            self.pending_screenshot = true;
        }
        let want_shot = self.pending_screenshot;
        let want_dump = frame_dump().is_some_and(|(_, start, every)| {
            self.frames >= start && (self.frames - start) % every.max(1) == 0
        });
        let want_gif = self.gif_recorder.is_some() || want_dump;
        let inverse_colormap =
            self.play.is_some() && self.fixed_colormap == room_sim::INVERSE_COLORMAP;
        const TIC_SEC: f32 = 1.0 / 35.0;
        let ticfrac_milli = (self.tic_accum / TIC_SEC * 1000.0).clamp(0.0, 1000.0) as u32;
        let zone_meshes: Option<ZoneMeshes> = if in_level {
            self.play
                .as_ref()
                .and_then(|g| g.world())
                .and_then(|world| {
                    if world.mod_zones.is_empty() {
                        return None;
                    }
                    let draws: Vec<ZoneDraw> = world
                        .mod_zones
                        .iter()
                        .map(|z| {
                            let (cx, cy, r) = z.interp_current(world.level.maptime, ticfrac_milli);
                            ZoneDraw {
                                cx: cx as f32,
                                cy: cy as f32,
                                r: r as f32,
                                rgba: z.rgba,
                                flags: z.flags,
                            }
                        })
                        .collect();
                    Some(build_zone_meshes(&self.map, &draws))
                })
        } else {
            None
        };
        self.render_settings = render_settings_from_cvars(&self.console.cvars);
        let render_settings = self.render_settings.clone();
        let use_post = render_settings.needs_post();
        let duke_frame = if self.duke_bonus_panel.is_none() {
            let renderer = self.duke_renderer.as_ref();
            let mouse_look = self.mouse_look;
            let demo_playback = self.duke_demo_playback.is_none();
            let atomic_demo_playback = self.duke_atomic_demo_playback.is_none();
            let mouse_yaw_accum = self.mouse_yaw_accum;
            let mouse_pitch_accum = self.mouse_pitch_accum;
            let turn_residual = self.duke_turn_residual;
            let horiz_residual = self.duke_horiz_residual;
            let pitch_scale = self.duke_pitch_scale();
            let tic_accum = self.tic_accum;
            let dimensions = self
                .config
                .as_ref()
                .map_or((640, 400), |c| (c.width.max(1), c.height.max(1)));
            self.duke_session.as_mut().and_then(|session| {
                let renderer = renderer?;
                let world = match session.render_world_with_eduke_events(0) {
                    Ok(Some(world)) => world,
                    Ok(None) => return None,
                    Err(error) => {
                        eprintln!("[Duke CON] EVENT_ANIMATESPRITES failed: {error}");
                        session.render_world()?
                    }
                };
                for error in session.take_eduke_runtime_errors() {
                    eprintln!("[Duke CON] {error}");
                }
                renderer.set_overlay_quote(session.active_quote());
                let tic_duration = 1.0 / session.tic_rate() as f32;
                // Mouse look between simulation tics: show the not-yet-sent mouse
                // motion so the view follows the mouse at the display rate.
                let mut view_player = session.player.clone();
                if mouse_look
                    && demo_playback
                    && atomic_demo_playback
                    && session.player.dead_flag == 0
                {
                    let pending_yaw = -(mouse_yaw_accum as f64) * MOUSE_DEG_PER_COUNT * 2048.0
                        / 360.0
                        + turn_residual;
                    view_player.ang =
                        ((i32::from(view_player.ang) + pending_yaw.round() as i32) & 2047) as i16;
                    let pending_horiz = mouse_pitch_accum as f64
                        * MOUSE_DEG_PER_COUNT
                        * pitch_scale
                        * 160.0
                        * std::f64::consts::PI
                        / 180.0
                        + horiz_residual;
                    view_player.horiz = (f64::from(view_player.horiz) + pending_horiz.round())
                        .clamp(-99.0, 299.0) as i16;
                }
                // Chocolate `smoothratio`: draw the camera between the previous
                // and current tic positions (skipped across teleports).
                {
                    let frac = f64::from((tic_accum / tic_duration).clamp(0.0, 1.0));
                    let p = &session.player;
                    let (dx, dy, dz) = (p.posx - p.oposx, p.posy - p.oposy, p.posz - p.oposz);
                    if p.remote_camera.is_none()
                        && dx.abs() + dy.abs() < 2048
                        && dz.abs() < (64 << 8)
                        && (dx != 0 || dy != 0 || dz != 0)
                    {
                        let lerp =
                            |a: i32, d: i32| a - ((f64::from(d) * (1.0 - frac)).round() as i32);
                        view_player.posx = lerp(p.posx, dx);
                        view_player.posy = lerp(p.posy, dy);
                        view_player.posz = lerp(p.posz, dz);
                        let mut sect = p.cursectnum;
                        room_build_engine::geom::updatesector(
                            view_player.posx,
                            view_player.posy,
                            &mut sect,
                            &world.sector_walls(),
                            &world.wall_xy(),
                        );
                        if sect >= 0 {
                            view_player.cursectnum = sect;
                        } else {
                            view_player.posx = p.posx;
                            view_player.posy = p.posy;
                            view_player.posz = p.posz;
                        }
                    }
                }
                let (width, height) = dimensions;
                Some((
                    width,
                    height,
                    renderer.render_rgba_interpolated(
                        &world,
                        &view_player,
                        width,
                        height,
                        session.tic,
                        session.earthquake_time,
                        (tic_accum / tic_duration * 1000.0).clamp(0.0, 1000.0) as u32,
                    ),
                ))
            })
        } else {
            None
        };
        let soft_frame = if software_view {
            self.draw_soft_world()
        } else {
            None
        };
        if let (Some(gpu), Some(vfs), Some(frame_color)) = (
            self.gpu.as_ref(),
            self.vfs.as_ref(),
            self.frame_color.as_ref(),
        ) {
            let format = frame_color.0.format();
            if self.hw_post.as_ref().is_none_or(|(f, _)| *f != format) {
                let runners = build_hw_post_runners(&gpu.device, format, &self.gldefs, vfs);
                self.hw_post = Some((format, runners));
                self.post_requests_dirty = true;
            }
        }
        // `PPShader` calls from scripts (render-only, outside the checksum).
        if let Some(world) = self.play.as_mut().and_then(|g| g.world_mut()) {
            let ops = std::mem::take(&mut world.post_shader_ops);
            if !ops.is_empty() && record_post_ops(&mut self.post_requests, ops) {
                self.post_requests_dirty = true;
            }
        }
        if self.post_requests_dirty
            && let Some((_, shaders)) = self.hw_post.as_mut()
        {
            apply_post_requests(shaders, &self.post_requests);
            self.post_requests_dirty = false;
        }
        // Frame time the `vid_fps` graph measures against.
        let perf_target_ms = {
            let cap = self.frame_cap();
            let refresh = self
                .refresh_millihertz
                .map_or(60.0, |m| m as f32 / 1000.0)
                .max(30.0);
            1000.0
                / if cap > 0 {
                    (cap as f32).min(refresh)
                } else {
                    refresh
                }
        };
        self.perf.mark("scene");
        let capture_rgba = {
            // Model poses for portal views (taken before the GPU borrows).
            let portal_model_poses = if in_level
                && !scene.portal_views.is_empty()
                && !self.modeldefs.by_actor.is_empty()
            {
                self.collect_model_poses()
            } else {
                Vec::new()
            };
            // Minimap radar (touch screens): built before the GPU state is
            // borrowed below.
            let radar_sprite = self.radar_sprite();
            let (
                Some(surface),
                Some(gpu),
                Some(pass),
                Some(zone_pass),
                Some(depth),
                Some(stencil_aux),
                Some(frame_color),
                Some(post_ping),
                Some(post_ldr),
                Some(post_chain),
                Some(canvas_gpu),
                materials,
                vfs,
            ) = (
                &self.surface,
                &self.gpu,
                &mut self.pass,
                &mut self.zone_pass,
                &self.depth,
                &self.stencil_aux,
                &self.frame_color,
                &self.post_ping,
                &self.post_ldr,
                &mut self.post_chain,
                &mut self.canvas_gpu,
                &mut self.materials,
                self.vfs.as_ref(),
            )
            else {
                return;
            };
            // Linear scene when the post chain will tonemap / encode (SH0).
            pass.encode_srgb = !use_post;
            pass.set_fog_density(fog_density);
            pass.set_thick_fog(thick_fog.0, thick_fog.1);
            pass.set_sky_fog(sky_fog);
            pass.inverse_colormap = inverse_colormap;
            pass.time += dt; // fuzz animation clock
            // Portal views (line portals, stacked sectors, sky boxes, mirrors,
            // horizons): drawn first, sampled by the portal surfaces.
            if !scene.portal_views.is_empty()
                && let Some(vfs) = vfs
            {
                let size = (frame_color.0.width(), frame_color.0.height());
                let map = &self.map;
                let tex_anim = &self.tex_anim;
                let cubemap = self.sky_cubemap.as_ref();
                let dyn_lights = scene.dyn_lights.clone();
                let sky_du = self.play.as_ref().and_then(|g| g.world()).map_or(0.0, |w| {
                    (w.sky_speed1 * f64::from(w.level.maptime) / 360.0) as f32
                });
                let sky = (!self.sky_texture.is_empty()).then_some(self.sky_texture.as_str());
                // What the main view adds after the map (decals, particles,
                // models, voxels) belongs in portal views too; their
                // textures were uploaded for the main view above.
                let model_poses = &portal_model_poses;
                let decals = &self.decals;
                let particles = &self.particles;
                let particle_fx = self.render_settings.particle_fx;
                let models = &mut self.models;
                let modeldefs = &self.modeldefs;
                let voxels = &mut self.voxels;
                let voxel_poses = &self.voxel_poses;
                let mut build = |cam: &room_render::Camera, _depth: u32, _skybox: bool| {
                    // Portal eyes sit behind walls: no occlusion culling.
                    let mut s = build_scene_with_sky_ex_shared(
                        map,
                        cam,
                        live_sprites.as_deref(),
                        sky,
                        cubemap,
                        Some(std::sync::Arc::clone(&tex_sizes)),
                        false,
                    );
                    tex_anim.apply(&mut s, tic);
                    s.dyn_lights = dyn_lights.clone();
                    apply_glow_fullbright(&mut s, &glow);
                    scroll_sky_u(&mut s.sky_parts, sky_du);
                    if in_level {
                        append_decals(&mut s, decals, cam, Some(map));
                        append_particles_animated(
                            &mut s,
                            particles,
                            cam,
                            particle_fx,
                            Some((tex_anim, tic)),
                        );
                        if !model_poses.is_empty() {
                            append_actor_models(&mut s, models, vfs, modeldefs, model_poses);
                        }
                        if !voxel_poses.is_empty() {
                            let _ = voxels.append_poses(&mut s, vfs, voxel_poses);
                        }
                    }
                    s
                };
                self.portal_renderer.render(
                    gpu,
                    pass,
                    materials,
                    vfs,
                    map,
                    &self.camera,
                    &mut scene,
                    size,
                    &mut build,
                );
            }
            pass.prepare(gpu, &self.camera, &scene);

            let ui_only = matches!(
                self.play.as_ref().map(|g| g.state),
                Some(GameState::Title | GameState::Intermission | GameState::Finale) | None
            ) || self.duke_bonus_panel.is_some();

            let mut hud = Canvas::new();
            hud.clear([0.0, 0.0, 0.0, 0.0]);
            if let Some((width, height, rgba)) = soft_frame {
                hud.blit_rgba(0.0, 0.0, width, height, rgba);
            }
            if let Some((width, height, rgba)) = duke_frame {
                hud.blit_rgba(0.0, 0.0, width, height, rgba);
            }

            // Build UI draw list (status bar + optional automap) then blit to canvas.
            let mut ui = UiDrawList::new();
            // `ui` commands of the weapon and its layers (drawn on the GPU when
            // nothing in the canvas lies under them).
            let mut psprite_range: Option<(usize, usize)> = None;
            // `ui` commands of the ACS `HudMessage`s (their pictures can go to
            // the GPU too: Brutal Doom's screen blood is a full-window picture).
            let mut hud_msg_range: Option<(usize, usize)> = None;
            // SBARINFO bar: its commands, own screen placement, and where in
            // `ui` it goes (above the weapon, under messages and scores).
            let mut sbarinfo_out: Option<(usize, room_ui::SbarInfoOutput)> = None;
            match self.play.as_ref().map(|g| g.state) {
                Some(GameState::Title) => {
                    let elapsed_ms = self
                        .elapsed_ms
                        .saturating_sub(self.title_anim_origin)
                        .saturating_add(self.title_skip_ms);
                    let (pic, splash) = self.vfs.as_ref().map_or((None, false), |v| {
                        let read_def = |name: &str| {
                            v.find_any_ns(name)
                                .and_then(|id| v.read(id).ok())
                                .and_then(|b| parse_titledef(&String::from_utf8_lossy(&b)))
                        };
                        let splash_def = read_def("RADLDEF");
                        let logo_def = read_def("TITLEDEF");
                        if splash_def.is_some() || logo_def.is_some() {
                            let (name, splash) = title_screen_pic(splash_def, logo_def, elapsed_ms);
                            (Some(name), splash)
                        } else if v.find("TITLEPIC", Namespace::Global).is_some()
                            || v.find_any_ns("TITLEPIC").is_some()
                        {
                            (Some("TITLEPIC".into()), false)
                        } else {
                            (None, false)
                        }
                    });
                    draw_title(&mut ui, 320.0, 200.0, pic.as_deref());
                    if !splash {
                        if let Some(menu) = &self.menu {
                            let tic = self
                                .play
                                .as_ref()
                                .and_then(|g| g.world())
                                .map(|w| w.level.maptime)
                                .unwrap_or(self.frames as u32);
                            menu.draw(&mut ui, tic, Some(&self.fonts));
                        }
                    }
                }
                Some(GameState::Intermission) => {
                    let stats = self
                        .play
                        .as_ref()
                        .and_then(|g| g.intermission.as_ref())
                        .map(|t| IntermissionStats {
                            map_name: t.map_name.clone(),
                            kills: t.kills,
                            items: t.items,
                            secrets: t.secrets,
                            time_tics: t.time_tics,
                            par_tics: t.par_tics,
                        })
                        .unwrap_or_default();
                    draw_intermission(&mut ui, &stats, 320.0, 200.0);
                    // Netplay ready-vote: who has pressed use / fire.
                    if self.net.is_some() {
                        if let Some(game) = self.play.as_ref() {
                            let (ready, of) = game.intermission_votes();
                            if of > 0 {
                                let me = game.intermission_ready
                                    & room_sim::seat_bit(usize::from(self.local_seat))
                                    != 0;
                                let secs = game.intermission_auto_advance_in().div_ceil(35);
                                let line = if me {
                                    format!("READY  {ready}/{of}  {secs}")
                                } else {
                                    format!("PRESS USE WHEN READY  {ready}/{of}  {secs}")
                                };
                                let w = room_ui::measure_stcfn_string(&line, 1.0);
                                room_ui::draw_stcfn_string(
                                    &mut ui,
                                    160.0 - w / 2.0,
                                    188.0,
                                    &line,
                                    1.0,
                                );
                            }
                        }
                    }
                }
                Some(GameState::Finale) => {
                    let map_name = self
                        .play
                        .as_ref()
                        .and_then(|g| g.intermission.as_ref())
                        .map(|t| t.map_name.as_str())
                        .unwrap_or("");
                    let elapsed_ms = self.elapsed_ms.saturating_sub(self.finale_anim_origin);
                    let pic = self.vfs.as_ref().and_then(|v| {
                        let prefix = finale_anm_prefix(map_name)?;
                        let def = v.find_any_ns(&format!("{prefix}DEF"))?;
                        let body = v.read(def).ok()?;
                        let (n, fps) = parse_titledef(&String::from_utf8_lossy(&body))?;
                        Some(anim_pic(prefix, n, fps, elapsed_ms))
                    });
                    draw_finale(&mut ui, map_name, 320.0, 200.0, pic.as_deref());
                    if let (Some(prefix), Some(pic)) = (finale_anm_prefix(map_name), pic.as_deref())
                        && let Some(frame) = pic
                            .strip_prefix(prefix)
                            .and_then(|suffix| suffix.parse::<u32>().ok())
                    {
                        let map_key = map_name.to_owned();
                        let previous = self.duke_finale_sound_frame.clone();
                        let first_frame = previous
                            .as_ref()
                            .filter(|(previous_map, previous_frame)| {
                                previous_map == &map_key && *previous_frame <= frame
                            })
                            .map_or(1, |(_, previous_frame)| previous_frame.saturating_add(1));
                        let cues: Vec<_> = (first_frame..=frame)
                            .flat_map(|cue_frame| {
                                room_duke::campaign::finale_sound_labels(map_name, cue_frame)
                                    .iter()
                                    .copied()
                            })
                            .collect();
                        self.duke_finale_sound_frame = Some((map_key, frame));
                        if !cues.is_empty() {
                            if let Some(session) = self.duke_session.as_mut() {
                                for label in cues {
                                    session.queue_frontend_sound_label(label);
                                }
                            }
                        }
                    }
                }
                _ => {
                    if self.automap {
                        let seat = usize::from(self.local_seat);
                        // `PowerScanner` reveals the map while it runs.
                        let allmap = self.play.as_ref().and_then(|g| g.world()).is_some_and(|w| {
                            w.players
                                .get(seat)
                                .and_then(|p| p.as_ref())
                                .is_some_and(|p| {
                                    p.allmap
                                        || p.mo.is_some_and(|mo| {
                                            room_sim::powerups::reveals_map(w, mo)
                                        })
                                })
                        });
                        // The whole window, not the centred 320×200 box: on
                        // a wide phone that box left the 3D view showing on
                        // both sides of a small map. Stops at the status bar
                        // when it is shown.
                        let (fb_w, fb_h) = self
                            .config
                            .as_ref()
                            .map_or((320.0, 200.0), |c| (c.width as f32, c.height as f32));
                        let (map_x, map_y, map_w, map_h) =
                            play_overlay::automap_window_rect(fb_w, fb_h, !self.hud.fullscreen);
                        if !self.hud.fullscreen && self.duke_session.is_none() {
                            // Black under the status bar and beside it, so the
                            // 3D view does not show through at the bottom.
                            let (x, y, w, h) = play_overlay::window_rect(fb_w, fb_h);
                            ui.fill(
                                room_ui::Rect::new(x, y, w, h),
                                room_ui::Rgba::new(0.0, 0.0, 0.0, 1.0),
                            );
                        }
                        let view = AutomapView {
                            center_x: f64::from(self.camera.eye.x),
                            center_y: f64::from(self.camera.eye.y),
                            scale: self.automap_scale,
                            screen_x: map_x,
                            screen_y: map_y,
                            width: map_w,
                            height: map_h,
                            angle_deg: self.yaw.to_degrees(),
                            allmap,
                            // Play mode: only show linedefs the player has walked near.
                            fog: self.play.is_some(),
                            textured: matches!(
                                self.console.cvars.get("am_textured"),
                                Some(room_console::CvarValue::Bool(true))
                                    | Some(room_console::CvarValue::Int(1..))
                            ),
                        };
                        // Native Duke maps use Build wall loops and keep their
                        // own camera pose; the Doom `MapGeometry` automap is
                        // not the map loaded by DukeSession.
                        if let Some(session) = self.duke_session.as_ref() {
                            if let Some(world) = session.world.as_ref() {
                                let sector_count = world.numsectors.max(0) as usize;
                                self.duke_automap_seen.resize(sector_count, false);
                                let visible = duke_automap_visible_sectors(
                                    world,
                                    session.player.cursectnum,
                                    session.player.posx,
                                    session.player.posy,
                                );
                                for (seen, in_view) in self
                                    .duke_automap_seen
                                    .iter_mut()
                                    .zip(visible.iter().copied())
                                {
                                    *seen |= in_view;
                                }
                                draw_duke_automap(
                                    world,
                                    &self.duke_automap_seen,
                                    &visible,
                                    session.player.posx,
                                    session.player.posy,
                                    session.player.view_ang(),
                                    self.automap_scale,
                                    &mut ui,
                                );
                            }
                        // Live world map carries `ML_MAPPED` exploration bits.
                        } else if let Some(world) = self.play.as_ref().and_then(|g| g.world()) {
                            if let Some(vfs) = self.vfs.as_ref() {
                                for flat in room_ui::automap_flats(&world.map, &view) {
                                    let _ = self.patches.ensure_flat(vfs, &flat);
                                }
                            }
                            draw_automap(&world.map, &view, &mut ui);
                            if world
                                .players
                                .get(usize::from(self.local_seat))
                                .and_then(|p| p.as_ref())
                                .is_some_and(|p| p.automap_things)
                            {
                                let things = automap_things(world);
                                room_ui::draw_automap_things(&view, &things, &mut ui);
                            }
                            let maptime = world.level.maptime;
                            draw_mod_zones_automap(&world.mod_zones, maptime, 1000, &view, &mut ui);
                        } else {
                            if let Some(vfs) = self.vfs.as_ref() {
                                for flat in room_ui::automap_flats(&self.map, &view) {
                                    let _ = self.patches.ensure_flat(vfs, &flat);
                                }
                            }
                            draw_automap(&self.map, &view, &mut ui);
                        }
                    } else if self.duke_session.is_none() {
                        // Doom psprites/crosshair: the native Duke renderer
                        // draws its own weapon; a missing Doom weapon lump
                        // here showed as a grey 16x16 placeholder box.
                        if let Some(vfs) = self.vfs.as_ref() {
                            if !self.hud.weapon_patch.is_empty() {
                                let _ = self.patches.ensure(vfs, &self.hud.weapon_patch);
                            }
                            if !self.hud.flash_patch.is_empty() {
                                let _ = self.patches.ensure(vfs, &self.hud.flash_patch);
                            }
                            if !self.hud.overlay_patch.is_empty() {
                                let _ = self.patches.ensure(vfs, &self.hud.overlay_patch);
                            }
                            for layer in &self.hud.layers {
                                let _ = self.patches.ensure(vfs, &layer.patch);
                            }
                        }
                        let weapon_cmds = ui.cmds.len();
                        let soft = self.patches.get(&self.hud.weapon_patch);
                        let vm_scale = self
                            .modeldefs
                            .by_actor
                            .get(&self.hud.weapon_class.to_ascii_lowercase())
                            .map(|d| {
                                view_model_weapon_scale(
                                    self.camera.fov_y_deg as f32,
                                    d.view_model_fov,
                                )
                            })
                            .unwrap_or(1.0);
                        let frac = (self.tic_accum / TIC_SEC).clamp(0.0, 1.0);
                        let (bob_x, bob_y) = overlay_bob(
                            self.weapon_bob_prev,
                            (self.hud.bob_x, self.hud.bob_y),
                            frac,
                            self.hud.weapon_interpolate,
                        );
                        let (layer_bob_x, layer_bob_y) = overlay_bob(
                            self.weapon_bob_prev,
                            (self.hud.bob_x, self.hud.bob_y),
                            frac,
                            self.hud.overlay_interpolate,
                        );
                        // Extra `A_Overlay` layers (UZDoom draws them by id around the gun).
                        let draw_layers =
                            |ui: &mut room_ui::UiDrawList, pick: &dyn Fn(i32) -> bool| {
                                for layer in self.hud.layers.iter().filter(|l| pick(l.id)) {
                                    let Some(soft_layer) = self.patches.get(&layer.patch) else {
                                        continue;
                                    };
                                    let (lbx, lby) = overlay_bob(
                                        self.weapon_bob_prev,
                                        (self.hud.bob_x, self.hud.bob_y),
                                        frac,
                                        layer.interpolate,
                                    );
                                    let mut x = overlay_layer_bob(
                                        lbx,
                                        self.hud.bob_ofs_x,
                                        layer.add_bob,
                                        layer.add_weapon,
                                    ) + layer.ofs_x;
                                    if layer.mirror {
                                        x = -x;
                                    }
                                    let y = overlay_layer_bob(
                                        lby,
                                        self.hud.bob_ofs_y,
                                        layer.add_bob,
                                        layer.add_weapon,
                                    ) + layer.ofs_y;
                                    let first = ui.cmds.len();
                                    draw_weapon_tinted(
                                        ui,
                                        &layer.patch,
                                        self.hud.fullscreen,
                                        Some(soft_layer),
                                        x,
                                        y,
                                        self.hud.weapon_light,
                                        layer.bright,
                                        layer.fuzz,
                                        layer.alpha,
                                        layer.scale_x,
                                        layer.scale_y,
                                        layer.flip,
                                        layer.pivot_x,
                                        layer.pivot_y,
                                        layer.angle,
                                        layer.tint.0,
                                        layer.tint.1,
                                        layer.tint.2,
                                        false,
                                        layer.pivot_percent,
                                    );
                                    room_ui::set_patch_corners(
                                        ui,
                                        first,
                                        layer.vertex,
                                        layer.scale_x,
                                        layer.scale_y,
                                        layer.flip,
                                    );
                                }
                            };
                        draw_layers(&mut ui, &|id| id < 1);
                        let gun_first = ui.cmds.len();
                        draw_weapon_tinted(
                            &mut ui,
                            &self.hud.weapon_patch,
                            self.hud.fullscreen,
                            soft,
                            bob_x,
                            bob_y,
                            self.hud.weapon_light,
                            self.hud.weapon_bright,
                            self.hud.weapon_fuzz,
                            self.hud.weapon_alpha,
                            self.hud.weapon_scale_x * vm_scale,
                            self.hud.weapon_scale_y * vm_scale,
                            self.hud.weapon_flip,
                            self.hud.weapon_pivot_x,
                            self.hud.weapon_pivot_y,
                            self.hud.weapon_angle,
                            self.hud.weapon_tint_r,
                            self.hud.weapon_tint_g,
                            self.hud.weapon_tint_b,
                            self.hud.weapon_mirror,
                            self.hud.weapon_pivot_percent,
                        );
                        room_ui::set_patch_corners(
                            &mut ui,
                            gun_first,
                            self.hud.weapon_vertex,
                            self.hud.weapon_scale_x * vm_scale,
                            self.hud.weapon_scale_y * vm_scale,
                            self.hud.weapon_flip,
                        );
                        if !self.hud.overlay_patch.is_empty() {
                            if let Some(soft_overlay) = self.patches.get(&self.hud.overlay_patch) {
                                let ov_first = ui.cmds.len();
                                draw_weapon_tinted(
                                    &mut ui,
                                    &self.hud.overlay_patch,
                                    self.hud.fullscreen,
                                    Some(soft_overlay),
                                    overlay_layer_bob(
                                        layer_bob_x,
                                        self.hud.bob_ofs_x,
                                        self.hud.overlay_add_bob,
                                        self.hud.overlay_add_weapon,
                                    ) + self.hud.overlay_ofs_x,
                                    overlay_layer_bob(
                                        layer_bob_y,
                                        self.hud.bob_ofs_y,
                                        self.hud.overlay_add_bob,
                                        self.hud.overlay_add_weapon,
                                    ) + self.hud.overlay_ofs_y,
                                    self.hud.weapon_light,
                                    true,
                                    self.hud.overlay_fuzz,
                                    self.hud.overlay_alpha,
                                    self.hud.overlay_scale_x,
                                    self.hud.overlay_scale_y,
                                    self.hud.overlay_flip,
                                    self.hud.overlay_pivot_x,
                                    self.hud.overlay_pivot_y,
                                    self.hud.overlay_angle,
                                    self.hud.overlay_tint_r,
                                    self.hud.overlay_tint_g,
                                    self.hud.overlay_tint_b,
                                    self.hud.overlay_mirror,
                                    self.hud.overlay_pivot_percent,
                                );
                                room_ui::set_patch_corners(
                                    &mut ui,
                                    ov_first,
                                    self.hud.overlay_vertex,
                                    self.hud.overlay_scale_x,
                                    self.hud.overlay_scale_y,
                                    self.hud.overlay_flip,
                                );
                            }
                        }
                        draw_layers(&mut ui, &|id| id > 1 && id < 1000);
                        // A flash frame without a lump draws nothing (GZDoom).
                        if let Some(soft_flash) = self.patches.get(&self.hud.flash_patch) {
                            draw_weapon_lit(
                                &mut ui,
                                &self.hud.flash_patch,
                                self.hud.fullscreen,
                                Some(soft_flash),
                                bob_x,
                                bob_y,
                                self.hud.weapon_light,
                                self.hud.flash_bright,
                                self.hud.weapon_fuzz,
                                1.0,
                                1.0,
                                1.0,
                                false,
                                0.5,
                                0.5,
                                0.0,
                            );
                        }
                        draw_layers(&mut ui, &|id| id > 1000);
                        if self.hud.inverse_colormap() {
                            ui.invert_since(weapon_cmds);
                        }
                        psprite_range = Some((weapon_cmds, ui.cmds.len()));
                        draw_crosshair(&mut ui, self.hud.crosshair);
                        // `PowerTargeter` reticle (TRGTA0 centre, B0 / C0 sides).
                        let targeter = self.play.as_ref().and_then(|g| g.world()).and_then(|w| {
                            w.player_hud(usize::from(self.local_seat))
                                .and_then(|h| h.targeter)
                        });
                        if let (Some(t), Some(vfs)) = (targeter, self.vfs.as_ref()) {
                            let mut piece = |name: &str, x: f32, y: f32| {
                                if let Some(p) = self.patches.ensure(vfs, name) {
                                    let (w, h) = (f32::from(p.width), f32::from(p.height));
                                    let pos = room_ui::Vec2::new(
                                        x - f32::from(p.left_offset),
                                        y - f32::from(p.top_offset),
                                    );
                                    ui.patch(name, pos, room_ui::Vec2::new(w, h));
                                }
                            };
                            piece("TRGTA0", t.center.0, t.center.1);
                            if let Some(x) = t.left_x {
                                piece("TRGTB0", x, t.center.1);
                            }
                            if let Some(x) = t.right_x {
                                piece("TRGTC0", x, t.center.1);
                            }
                        }
                    }
                    let (mod_hud_seat, mod_hud_tic) = self
                        .play
                        .as_ref()
                        .and_then(|g| g.world())
                        .map(|w| (self.local_seat, w.level.maptime))
                        .unwrap_or((self.local_seat, self.frames as u32));
                    if let Some(world) = self.play.as_ref().and_then(|g| g.world()) {
                        let poses = player_map_poses(world);
                        let allmap = world
                            .players
                            .get(usize::from(self.local_seat))
                            .and_then(|p| p.as_ref())
                            .is_some_and(|p| p.allmap);
                        let map_hud_ctx = ModHudMapCtx {
                            map: Some(&world.map),
                            zones: &world.mod_zones,
                            player_pose: &poses,
                            hud: &world.mod_hud,
                            allmap,
                            ticfrac_milli: 1000,
                        };
                        room_ui::set_marker_cam(room_ui::MarkerCam {
                            x: f64::from(self.camera.eye.x),
                            y: f64::from(self.camera.eye.y),
                            yaw_deg: f64::from(self.yaw.to_degrees()),
                            scale: 1024.0,
                            z: f64::from(self.camera.eye.z),
                        });
                        draw_mod_hud_layer(
                            &world.mod_hud,
                            mod_hud_seat,
                            mod_hud_tic,
                            0.0,
                            &mut ui,
                            mod_hud_layer::UNDER_HUD,
                            Some(&map_hud_ctx),
                        );
                        let spectating = world
                            .players
                            .get(usize::from(self.local_seat))
                            .and_then(|p| p.as_ref())
                            .is_some_and(|p| {
                                p.player_state == room_sim::PlayerState::Dead && p.camera.is_some()
                            });
                        let seat = usize::from(self.local_seat);
                        // `hud_althud` replaces the full-screen HUD (UZDoom
                        // `DrawAltHUD`, `screenblocks` 11).
                        let althud = !spectating
                            && self.hud.fullscreen
                            && matches!(
                                self.console.cvars.get("hud_althud"),
                                Some(
                                    room_console::CvarValue::Bool(true)
                                        | room_console::CvarValue::Int(1..)
                                )
                            )
                            && world.status_bar_items(seat).is_some();
                        if althud && self.script_sbar.is_none() && self.althud_bar.is_none() {
                            self.althud_bar = Some(match self.vfs.as_ref() {
                                Some(vfs) => {
                                    room_game::load_status_bar_program(vfs).and_then(|prog| {
                                        room_ui::ScriptStatusBar::new(prog, "DoomStatusBar")
                                    })
                                }
                                None => Err("no VFS".to_owned()),
                            });
                            if let Some(Err(e)) = &self.althud_bar {
                                eprintln!("room: alt HUD unavailable: {e}");
                            }
                        }
                        let althud = althud
                            && (self.script_sbar.is_some()
                                || matches!(self.althud_bar, Some(Ok(_))));
                        let use_sbarinfo = !spectating && !althud && self.sbarinfo_bar.is_some();
                        let scripted = !spectating
                            && (self.script_sbar.is_some() || althud || use_sbarinfo)
                            && world.status_bar_items(seat).is_some();
                        if scripted {
                            let items = world.status_bar_items(seat).unwrap_or_default();
                            let snap = room_ui::SbarSnapshot {
                                health: self.hud.health,
                                keys: self.hud.keys,
                                slots: items.slots,
                                face: self.hud.face.clone(),
                                inventory: items.inventory,
                                icons: items.icons,
                                ready_weapon: items.ready_weapon,
                                ammo1: items.ammo1,
                                ammo2: items.ammo2,
                                frags: self.hud.frags,
                                inv_sel: items.inv_sel,
                                inv_bar: items.inv_bar,
                                inventory_tics: items.inventory_tics,
                                weapons: items
                                    .weapons
                                    .into_iter()
                                    .map(|(class, slot, ammo1, ammo2)| room_ui::SbarWeapon {
                                        class,
                                        slot,
                                        ammo1,
                                        ammo2,
                                    })
                                    .collect(),
                                spawn_sprites: items.spawn_sprites,
                                max_amounts: items.max_amounts,
                                power_tics: items.power_tics,
                            };
                            if let Some(vfs) = self.vfs.as_ref() {
                                for name in room_ui::ScriptStatusBar::althud_patches(&snap) {
                                    let _ = self.patches.ensure(vfs, &name);
                                }
                                if use_sbarinfo {
                                    if let Some(bar) = self.sbarinfo_bar.as_ref() {
                                        for name in bar.wanted_patches(&snap) {
                                            let _ = self.patches.ensure(vfs, &name);
                                        }
                                    }
                                }
                            }
                            let patches = &self.patches;
                            let dims = |name: &str| {
                                patches.get(name).map(|p| room_ui::PatchDims {
                                    w: f32::from(p.width),
                                    h: f32::from(p.height),
                                    left: f32::from(p.left_offset),
                                    top: f32::from(p.top_offset),
                                })
                            };
                            let first = ui.cmds.len();
                            if use_sbarinfo {
                                let (fb_w, fb_h) = self
                                    .config
                                    .as_ref()
                                    .map(|c| (c.width as f32, c.height as f32))
                                    .unwrap_or((1280.0, 720.0));
                                let multiplayer = world.players.iter().flatten().count() > 1;
                                let frame = room_ui::SbarInfoFrame {
                                    snap: &snap,
                                    fullscreen: self.hud.fullscreen,
                                    screen: (fb_w, fb_h),
                                    deathmatch: world.deathmatch,
                                    teamplay: false,
                                    multiplayer,
                                    player_class: &self.hud.player_class,
                                    hud_aspectscale: matches!(
                                        self.console.cvars.get("hud_aspectscale"),
                                        Some(room_console::CvarValue::Bool(true))
                                    ),
                                    dims: &dims,
                                };
                                if let Some(out) =
                                    self.sbarinfo_bar.as_ref().and_then(|b| b.draw(&frame))
                                {
                                    sbarinfo_out = Some((first, out));
                                } else if !self.hud.fullscreen {
                                    // No bar for this mode in the lump: the stock bar.
                                    DoomStatusBar.draw(&self.hud, &mut ui);
                                }
                            } else if althud {
                                // `DBaseStatusBar::DrawAltHUD`: 640 / scale wide,
                                // the height by the window's aspect ratio.
                                let scale = match self.console.cvars.get("hud_scalefactor") {
                                    Some(room_console::CvarValue::Float(f)) if *f > 0.0 => *f,
                                    _ => 1.0,
                                };
                                let w = (640.0 / scale).round();
                                let mut h = (w / self.camera.aspect.max(0.1)).round();
                                if matches!(
                                    self.console.cvars.get("hud_aspectscale"),
                                    Some(room_console::CvarValue::Bool(true))
                                ) {
                                    h = (h * 5.0 / 6.0).round();
                                }
                                let cvars: Vec<(&str, i64)> = room_ui::HUD_CVARS
                                    .iter()
                                    .filter_map(|(name, _)| match self.console.cvars.get(name) {
                                        Some(room_console::CvarValue::Int(v)) => {
                                            Some((*name, i64::from(*v)))
                                        }
                                        Some(room_console::CvarValue::Bool(b)) => {
                                            Some((*name, i64::from(*b)))
                                        }
                                        _ => None,
                                    })
                                    .collect();
                                let bar = match self.script_sbar.as_mut() {
                                    Some(bar) => Some(bar),
                                    None => self.althud_bar.as_mut().and_then(|r| r.as_mut().ok()),
                                };
                                if let Some(bar) = bar {
                                    for (name, v) in cvars {
                                        bar.set_cvar(name, v);
                                    }
                                    #[allow(clippy::cast_possible_truncation)]
                                    bar.draw_althud(&snap, (w as i32, h as i32), &dims, &mut ui);
                                    if let Some(e) = bar.last_error.take() {
                                        static LOGGED_ALT: std::sync::Once = std::sync::Once::new();
                                        LOGGED_ALT.call_once(|| eprintln!("room: alt HUD: {e}"));
                                    }
                                }
                            } else if let Some(bar) = self.script_sbar.as_mut() {
                                bar.draw(&snap, self.hud.fullscreen, 1.0, &dims, &mut ui);
                                if let Some(e) = bar.last_error.take() {
                                    static LOGGED: std::sync::Once = std::sync::Once::new();
                                    LOGGED.call_once(|| {
                                        eprintln!("room: status bar {}: {e}", bar.class());
                                    });
                                }
                            }
                            // Mod graphics the bar names beyond the stock set.
                            if let Some(vfs) = self.vfs.as_ref() {
                                for cmd in &ui.cmds[first..] {
                                    if let DrawCmd::Patch { name, .. } = cmd {
                                        if !self.patches.contains(name) {
                                            let _ = self.patches.ensure(vfs, name);
                                        }
                                    }
                                }
                            }
                        } else if !spectating {
                            if self
                                .vfs
                                .as_ref()
                                .is_some_and(|v| v.find_any_ns(DukeStatusBar::MARKER).is_some())
                            {
                                DukeStatusBar.draw(&self.hud, &mut ui);
                            } else {
                                DoomStatusBar.draw(&self.hud, &mut ui);
                            }
                        }
                    }
                    // Scoreboard: held, or on its own while dead in deathmatch.
                    if let Some(world) = self.play.as_ref().and_then(|g| g.world()) {
                        let seat = usize::from(self.local_seat);
                        let dead = world
                            .players
                            .get(seat)
                            .and_then(|p| p.as_ref())
                            .is_some_and(|p| p.player_state == room_sim::PlayerState::Dead);
                        if self.show_scores || (world.deathmatch && dead) {
                            let lines: Vec<room_ui::ScoreLine> = world
                                .scoreboard()
                                .into_iter()
                                .map(|r| room_ui::ScoreLine {
                                    seat: r.seat,
                                    name: r.name,
                                    frags: r.frags,
                                    deaths: r.deaths,
                                    kills: r.kills,
                                    items: r.items,
                                    secrets: r.secrets,
                                })
                                .collect();
                            let l = &world.level;
                            room_ui::draw_scoreboard(
                                &mut ui,
                                &lines,
                                world.deathmatch,
                                seat,
                                [l.total_kills, l.total_items, l.total_secrets],
                            );
                        }
                    }
                    // Pain tint stays off this letterboxed 320×200 layer. A fill
                    // here is a hard red rectangle on a widescreen view. The
                    // full-frame mix, when the post chain is on, is enough.
                    self.messages.draw(&mut ui, 8.0, 8.0);
                    if let Some(world) = self.play.as_ref().and_then(|g| g.world()) {
                        // ACS `HudMessage`s, `$KEY` tokens localized (GZDoom `GetTag`).
                        if !world.hud_messages.is_empty() {
                            let shown: Vec<room_sim::AcsHudMessage> = world
                                .hud_messages
                                .iter()
                                .map(|m| {
                                    let mut m = m.clone();
                                    m.text = localize_tokens(&self.language, &m.text);
                                    m
                                })
                                .collect();
                            let first = ui.cmds.len();
                            room_ui::draw_acs_hud_messages(
                                &mut ui,
                                &shown,
                                usize::from(self.local_seat),
                                !self.hud.fullscreen,
                                self.config.as_ref().map_or(16.0 / 9.0, |c| {
                                    c.width as f32 / c.height.max(1) as f32
                                }),
                                &|name| {
                                    self.patches
                                        .get(name)
                                        .map(|p| (f32::from(p.width), f32::from(p.height)))
                                },
                            );
                            hud_msg_range = Some((first, ui.cmds.len()));
                        }
                    }
                    if let Some(world) = self.play.as_ref().and_then(|g| g.world()) {
                        let poses = player_map_poses(world);
                        let allmap = world
                            .players
                            .get(usize::from(self.local_seat))
                            .and_then(|p| p.as_ref())
                            .is_some_and(|p| p.allmap);
                        let map_hud_ctx = ModHudMapCtx {
                            map: Some(&world.map),
                            zones: &world.mod_zones,
                            player_pose: &poses,
                            hud: &world.mod_hud,
                            allmap,
                            ticfrac_milli: 1000,
                        };
                        draw_mod_hud_layer(
                            &world.mod_hud,
                            mod_hud_seat,
                            mod_hud_tic,
                            0.0,
                            &mut ui,
                            mod_hud_layer::OVER_HUD,
                            Some(&map_hud_ctx),
                        );
                        let _ = draw_mod_hud_modals(
                            &world.mod_hud,
                            mod_hud_seat,
                            mod_hud_tic,
                            0.0,
                            &mut ui,
                            Some(&map_hud_ctx),
                        );
                        draw_outside_zone_tint(
                            &world.mod_zones,
                            mod_hud_tic,
                            1000,
                            f64::from(self.camera.eye.x),
                            f64::from(self.camera.eye.y),
                            &mut ui,
                        );
                    }
                    if let Some(menu) = &self.menu {
                        let tic = self
                            .play
                            .as_ref()
                            .and_then(|g| g.world())
                            .map(|w| w.level.maptime)
                            .unwrap_or(self.frames as u32);
                        menu.draw(&mut ui, tic, Some(&self.fonts));
                    }
                    // Screen fades and the pain tint are drawn in framebuffer
                    // pixels below. A 320×200 fill here is a hard rectangle on
                    // a widescreen window.
                }
            }
            if self.console_open {
                draw_console_overlay(&mut ui, &self.console, &self.console_line);
            }
            let duke_palette_state = match (&self.duke_session, &self.duke_renderer) {
                (Some(session), Some(renderer)) => session
                    .world
                    .as_ref()
                    .map(|world| (renderer, world, &session.player)),
                _ => None,
            };
            type DukePaletteState<'a> = Option<(
                &'a room_duke::DukeSoftRenderer,
                &'a room_build_engine::world::BuildWorld,
                &'a room_duke::Player,
            )>;
            struct Sink<'a> {
                canvas: &'a mut Canvas,
                duke_palette_state: DukePaletteState<'a>,
                remap_cache: std::collections::HashMap<[u8; 3], [u8; 3]>,
            }
            impl Sink<'_> {
                fn map_rgb(
                    state: DukePaletteState<'_>,
                    cache: &mut std::collections::HashMap<[u8; 3], [u8; 3]>,
                    rgb: [u8; 3],
                ) -> [u8; 3] {
                    if let Some(mapped) = cache.get(&rgb) {
                        return *mapped;
                    }
                    let mapped = state
                        .and_then(|(renderer, world, player)| {
                            renderer.remap_screen_rgb(world, player, rgb)
                        })
                        .unwrap_or(rgb);
                    cache.insert(rgb, mapped);
                    mapped
                }

                fn remap_rgb(&mut self, rgb: [u8; 3]) -> [u8; 3] {
                    Self::map_rgb(self.duke_palette_state, &mut self.remap_cache, rgb)
                }

                fn remap_ui_color(&mut self, color: [f32; 4]) -> [f32; 4] {
                    let to_byte = |channel: f32| (channel.clamp(0.0, 1.0) * 255.0).round() as u8;
                    let mapped =
                        self.remap_rgb([to_byte(color[0]), to_byte(color[1]), to_byte(color[2])]);
                    [
                        f32::from(mapped[0]) / 255.0,
                        f32::from(mapped[1]) / 255.0,
                        f32::from(mapped[2]) / 255.0,
                        color[3],
                    ]
                }

                fn remap_canvas_from(&mut self, first_command: usize) {
                    let Self {
                        canvas,
                        duke_palette_state,
                        remap_cache,
                    } = self;
                    for command in canvas.cmds.iter_mut().skip(first_command) {
                        match command {
                            CanvasCmd::Clear { color }
                            | CanvasCmd::FillRect { color, .. }
                            | CanvasCmd::Text { color, .. } => {
                                let to_byte =
                                    |channel: f32| (channel.clamp(0.0, 1.0) * 255.0).round() as u8;
                                let mapped = Self::map_rgb(
                                    *duke_palette_state,
                                    remap_cache,
                                    [to_byte(color[0]), to_byte(color[1]), to_byte(color[2])],
                                );
                                color[..3].copy_from_slice(&[
                                    f32::from(mapped[0]) / 255.0,
                                    f32::from(mapped[1]) / 255.0,
                                    f32::from(mapped[2]) / 255.0,
                                ]);
                            }
                            CanvasCmd::BlitRgba { pixels, .. } => {
                                for pixel in pixels.chunks_exact_mut(4) {
                                    if pixel[3] == 0 {
                                        continue;
                                    }
                                    let mapped = Self::map_rgb(
                                        *duke_palette_state,
                                        remap_cache,
                                        [pixel[0], pixel[1], pixel[2]],
                                    );
                                    pixel[..3].copy_from_slice(&mapped);
                                }
                            }
                        }
                    }
                }
            }
            impl CanvasSink for Sink<'_> {
                fn fill_rect(&mut self, x: f32, y: f32, w: f32, h: f32, color: [f32; 4]) {
                    let color = self.remap_ui_color(color);
                    self.canvas.fill_rect(x, y, w, h, color);
                }
                fn text(&mut self, x: f32, y: f32, text: &str, color: [f32; 4]) {
                    let color = self.remap_ui_color(color);
                    self.canvas.text(x, y, text, color);
                }
                fn blit_rgba(&mut self, x: f32, y: f32, w: u32, h: u32, pixels: &[u8]) {
                    if self.duke_palette_state.is_none() {
                        self.canvas.blit_rgba(x, y, w, h, pixels.to_vec());
                        return;
                    }
                    let mut mapped = pixels.to_vec();
                    for pixel in mapped.chunks_exact_mut(4) {
                        if pixel[3] == 0 {
                            continue;
                        }
                        let rgb = self.remap_rgb([pixel[0], pixel[1], pixel[2]]);
                        pixel[..3].copy_from_slice(&rgb);
                    }
                    self.canvas.blit_rgba(x, y, w, h, mapped);
                }
            }
            self.perf.mark("hud");
            let (fb_w, fb_h) = self
                .config
                .as_ref()
                .map(|c| (c.width as f32, c.height as f32))
                .unwrap_or((1280.0, 720.0));
            // Fit 320×200 virtual UI into the framebuffer while preserving aspect,
            // letterboxed to the centre (matches menu mouse hit-testing).
            let scale = (fb_w / 320.0).min(fb_h / 200.0);
            let ox = (fb_w - 320.0 * scale) * 0.5;
            let oy = (fb_h - 200.0 * scale) * 0.5;
            if let Some(vfs) = self.vfs.as_ref() {
                for cmd in &ui.cmds {
                    if let DrawCmd::Patch { name, .. } = cmd {
                        let _ = self.patches.ensure(vfs, name);
                    }
                }
            }
            struct OffsetSink<'a> {
                inner: Sink<'a>,
                ox: f32,
                oy: f32,
            }
            impl CanvasSink for OffsetSink<'_> {
                fn fill_rect(&mut self, x: f32, y: f32, w: f32, h: f32, color: [f32; 4]) {
                    self.inner.fill_rect(x + self.ox, y + self.oy, w, h, color);
                }
                fn text(&mut self, x: f32, y: f32, text: &str, color: [f32; 4]) {
                    self.inner.text(x + self.ox, y + self.oy, text, color);
                }
                fn blit_rgba(&mut self, x: f32, y: f32, w: u32, h: u32, pixels: &[u8]) {
                    self.inner.blit_rgba(x + self.ox, y + self.oy, w, h, pixels);
                }
            }
            // The weapon on the GPU: only when it is the first thing drawn
            // (nothing in the canvas under it) and every layer is a plain
            // picture; otherwise the CPU canvas draws it as before.
            let mut gpu_psprites: Vec<room_ui::PatchQuad> = Vec::new();
            if let Some((a, b)) = psprite_range
                && a == 0
                && b > a
                && b <= ui.cmds.len()
                && hud.cmds.len() == 1
                && duke_palette_state.is_none()
                && self.hud_sprites.is_some()
                && !cvar_bool(&self.console.cvars, "r_cpuhud")
                && let Some(quads) = room_ui::patch_quads(
                    &ui.cmds[a..b],
                    (ui.virtual_w, ui.virtual_h),
                    (320.0 * scale, 200.0 * scale),
                    (ox, oy),
                    &self.patches,
                )
                && quads.len() <= room_render::hud_sprites::MAX_QUADS
            {
                gpu_psprites = quads;
                ui.cmds.drain(a..b);
                if let Some((at, _)) = sbarinfo_out.as_mut() {
                    *at = if *at >= b {
                        *at - (b - a)
                    } else {
                        (*at).min(a)
                    };
                }
                if let Some((c, d)) = hud_msg_range.as_mut()
                    && *c >= b
                {
                    *c -= b - a;
                    *d -= b - a;
                }
            }
            // HudMessage pictures on the GPU, above the weapon. Scaling and
            // blending a full-window picture (Brutal Doom's blood on the
            // screen) on the CPU took 60-150 ms a frame on phones. Only when
            // the weapon itself is on the GPU (or there is none), so the
            // pictures stay above it; whatever the canvas draws (status bar,
            // text) lands above them.
            let mut gpu_overlays: Vec<room_ui::PatchQuad> = Vec::new();
            let weapon_on_gpu =
                !gpu_psprites.is_empty() || psprite_range.is_none_or(|(a, b)| a == b);
            if let Some((c, d)) = hud_msg_range
                && c < d
                && d <= ui.cmds.len()
                && weapon_on_gpu
                && hud.cmds.len() == 1
                && duke_palette_state.is_none()
                && self.hud_sprites.is_some()
                && !cvar_bool(&self.console.cvars, "r_cpuhud")
            {
                let mut moved = Vec::new();
                for i in c..d {
                    if !matches!(ui.cmds[i], DrawCmd::Patch { .. }) {
                        continue;
                    }
                    let Some(quads) = room_ui::patch_quads(
                        &ui.cmds[i..=i],
                        (ui.virtual_w, ui.virtual_h),
                        (320.0 * scale, 200.0 * scale),
                        (ox, oy),
                        &self.patches,
                    ) else {
                        continue;
                    };
                    // Small glyphs are cheap on the CPU; keep the quad budget
                    // for pictures that cover a real part of the window.
                    let big = quads
                        .iter()
                        .any(|q| u64::from(q.w) * u64::from(q.h) >= 64 * 64);
                    if !big
                        || gpu_psprites.len() + gpu_overlays.len() + quads.len()
                            > room_render::hud_sprites::MAX_QUADS
                    {
                        continue;
                    }
                    gpu_overlays.extend(quads);
                    moved.push(i);
                }
                for &i in moved.iter().rev() {
                    ui.cmds.remove(i);
                    if let Some((at, _)) = sbarinfo_out.as_mut()
                        && *at > i
                    {
                        *at -= 1;
                    }
                }
            }
            // The SBARINFO bar has its own placement: draw `ui` up to its
            // slot, the bar, then the rest of `ui`.
            let (ui_under, ui_over) = match &sbarinfo_out {
                Some((at, _)) => {
                    let at = (*at).min(ui.cmds.len());
                    let mut under = UiDrawList::new();
                    under.cmds = ui.cmds[..at].to_vec();
                    let mut over = UiDrawList::new();
                    over.cmds = ui.cmds[at..].to_vec();
                    (under, Some(over))
                }
                None => (std::mem::take(&mut ui), None),
            };
            let mut sink = OffsetSink {
                inner: Sink {
                    canvas: &mut hud,
                    duke_palette_state,
                    remap_cache: std::collections::HashMap::new(),
                },
                ox,
                oy,
            };
            blit_draw_list_full(
                &ui_under,
                320.0 * scale,
                200.0 * scale,
                Some(&self.fonts),
                Some(&self.patches),
                &mut sink,
            );
            if let Some((_, bar)) = &sbarinfo_out {
                if let Some(vfs) = self.vfs.as_ref() {
                    for cmd in &bar.list.cmds {
                        if let DrawCmd::Patch { name, .. } = cmd {
                            let _ = self.patches.ensure(vfs, name);
                        }
                    }
                }
                sink.ox = bar.origin.0;
                sink.oy = bar.origin.1;
                blit_draw_list_full(
                    &bar.list,
                    bar.size.0,
                    bar.size.1,
                    Some(&self.fonts),
                    Some(&self.patches),
                    &mut sink,
                );
                sink.ox = ox;
                sink.oy = oy;
            }
            if let Some(over) = &ui_over {
                blit_draw_list_full(
                    over,
                    320.0 * scale,
                    200.0 * scale,
                    Some(&self.fonts),
                    Some(&self.patches),
                    &mut sink,
                );
            }
            self.perf.mark("hud scale");
            let draw_list_end = hud.cmds.len();
            if self.duke_bonus_panel.is_none()
                && let Some(session) = &self.duke_session
            {
                // Health/ammo/inventory are drawn by `DukeSoftRenderer`'s
                // Atomic mini status bar inside the frame.
                // Quotes are drawn in-frame by the renderer (Duke `gametext`).
                let rts_playing = self.audio.as_ref().is_some_and(|audio| {
                    audio.shared.lock().is_ok_and(|shared| {
                        shared.mixer.voices.iter().any(|voice| {
                            let raw = voice.sound.raw();
                            (DUKE_RTS_SOUND_BASE..DUKE_RTS_SOUND_BASE + 10).contains(&raw)
                        })
                    })
                });
                if rts_playing {
                    hud.text(
                        (fb_w - 96.0).max(8.0),
                        20.0,
                        "RTS PLAYING",
                        [1.0, 0.82, 0.25, 1.0],
                    );
                }
                if self.duke_coords_on {
                    hud.text(
                        12.0,
                        40.0,
                        format!(
                            "X {} Y {} Z {} ANG {} SECT {}",
                            session.player.posx,
                            session.player.posy,
                            session.player.posz,
                            session.player.ang,
                            session.player.cursectnum
                        ),
                        [0.95, 0.95, 0.65, 1.0],
                    );
                }
                if self.duke_rate_on {
                    let map = session.campaign.map_or_else(
                        || "MAP".to_owned(),
                        |campaign| {
                            format!(
                                "E{}L{}",
                                campaign.volume.saturating_add(1),
                                campaign.level.saturating_add(1)
                            )
                        },
                    );
                    hud.text(
                        12.0,
                        if self.duke_coords_on { 60.0 } else { 40.0 },
                        format!("{map} {} fps", self.duke_fps),
                        [0.95, 0.95, 0.65, 1.0],
                    );
                }
                if self.duke_debug_on {
                    let count_stat = |status: i16| {
                        session
                            .lists
                            .sprite_stat
                            .iter()
                            .filter(|&&sprite_status| sprite_status == status)
                            .count()
                    };
                    let live_sprites = session
                        .lists
                        .sprite_stat
                        .iter()
                        .filter(|&&sprite_status| {
                            sprite_status != room_build_engine::world::MAXSTATUS as i16
                        })
                        .count();
                    let y = if self.duke_coords_on && self.duke_rate_on {
                        80.0
                    } else if self.duke_coords_on || self.duke_rate_on {
                        60.0
                    } else {
                        40.0
                    };
                    hud.text(
                        12.0,
                        y,
                        format!(
                            "DEBUG tic {} sprites {} actors {} proj {} fx {}",
                            session.tic,
                            live_sprites,
                            count_stat(room_duke::STAT_ACTOR),
                            count_stat(room_duke::STAT_PROJECTILE),
                            count_stat(room_duke::STAT_FX),
                        ),
                        [0.65, 1.0, 0.65, 1.0],
                    );
                    hud.text(
                        12.0,
                        y + 16.0,
                        format!(
                            "sectors {} walls {} effectors {}",
                            session.world.as_ref().map_or(0, |world| world.numsectors),
                            session.world.as_ref().map_or(0, |world| world.numwalls),
                            count_stat(room_duke::STAT_EFFECTOR),
                        ),
                        [0.65, 1.0, 0.65, 1.0],
                    );
                }
                if session.tip_timer() > 0
                    && let Some(renderer) = self.duke_renderer.as_ref()
                {
                    const TIP: i16 = 2576;
                    const TIP_Y: [i16; 21] = [
                        0, -8, -16, -32, -64, -84, -108, -108, -108, -108, -108, -108, -108, -108,
                        -108, -108, -96, -72, -64, -32, -16,
                    ];
                    let remaining = session.tip_timer().min(26);
                    let frame = (26 - remaining) >> 4;
                    let picnum = TIP + i16::from(frame);
                    let y_index = usize::from(remaining.min(20));
                    if let Some((width, height, pixels)) =
                        renderer.ui_tile_rgba(picnum, fb_w as u32, fb_h as u32)
                    {
                        let x = 170.0 * fb_w / 320.0;
                        // Chocolate `animatetip` anchors the tip at virtual
                        // y=240, then applies the tip pose and player horizon.
                        let horizon = (i32::from(session.player.horiz)
                            - i32::from(session.player.horizoff))
                            >> 4;
                        let y =
                            (240.0 + f32::from(TIP_Y[y_index] / 2) - horizon as f32) * fb_h / 240.0;
                        hud.blit_rgba(x, y, width, height, pixels);
                    }
                }
                if session.player.loogie_tics > 0
                    && let Some(renderer) = self.duke_renderer.as_ref()
                    && let Some((base_w, base_h, base_pixels)) =
                        renderer.ui_tile_rgba(1637, fb_w as u32, fb_h as u32)
                {
                    for index in 0..usize::from(session.player.num_loogies.min(6)) {
                        let phase = ((i32::from(session.player.loogie_tics)
                            + i32::try_from(index).unwrap_or(0))
                            << 6)
                            & 2047;
                        let wiggle = room_build_engine::helpers::sincos_q14(phase).0 >> 10;
                        let x_virtual = i32::from(session.player.loogie_x[index]) + wiggle;
                        let y_virtual = 200 + i32::from(session.player.loogie_y[index])
                            - i32::from(session.player.loogie_tics) * 4;
                        let zoom = (4096
                            + ((i32::from(session.player.loogie_tics)
                                + i32::try_from(index).unwrap_or(0))
                                << 9)
                            - (i32::try_from(index).unwrap_or(0) << 8))
                            .clamp(1, 65_536) as u32;
                        let width = (u64::from(base_w) * u64::from(zoom) / 65_536).max(1) as u32;
                        let height = (u64::from(base_h) * u64::from(zoom) / 65_536).max(1) as u32;
                        let mut pixels = Vec::with_capacity((width * height * 4) as usize);
                        for y in 0..height {
                            let source_y = y * base_h / height;
                            for x in 0..width {
                                let source_x = x * base_w / width;
                                let source = ((source_y * base_w + source_x) * 4) as usize;
                                pixels.extend_from_slice(&base_pixels[source..source + 4]);
                            }
                        }
                        let x = x_virtual as f32 * fb_w / 320.0 - width as f32 * 0.5;
                        let y = y_virtual as f32 * fb_h / 200.0 - height as f32 * 0.5;
                        hud.blit_rgba(x, y, width, height, pixels);
                    }
                }
                if session.player.palette_flash_tics > 0 {
                    let tint = session.player.palette_flash;
                    let alpha = 0.32 * f32::from(session.player.palette_flash_tics) / 32.0;
                    hud.fill_rect(
                        0.0,
                        0.0,
                        fb_w,
                        fb_h,
                        [
                            f32::from(tint[0]) / 64.0,
                            f32::from(tint[1]) / 64.0,
                            f32::from(tint[2]) / 64.0,
                            alpha,
                        ],
                    );
                }
            }
            if let Some(state) = &self.duke_menu
                && let Some(renderer) = self.duke_renderer.as_ref()
            {
                let (w, h) = (fb_w.max(1.0) as u32, fb_h.max(1.0) as u32);
                let rgba =
                    renderer.render_menu(w, h, &state.menu, state.clock as i64, state.backdrop);
                hud.blit_rgba(0.0, 0.0, w, h, rgba);
            }
            if let Some(panel) = &self.duke_bonus_panel
                && let Some(renderer) = self.duke_renderer.as_ref()
            {
                let (w, h) = (fb_w.max(1.0) as u32, fb_h.max(1.0) as u32);
                let rgba = renderer.render_bonus_screen(w, h, &panel.screen);
                hud.blit_rgba(0.0, 0.0, w, h, rgba);
            }
            if self.duke_help_page != 0
                && let Some(renderer) = self.duke_renderer.as_ref()
            {
                let tile = if self.duke_help_page == 1 { 3280 } else { 2445 };
                if let Some((width, height, pixels)) =
                    renderer.ui_tile_rgba(tile, fb_w as u32, fb_h as u32)
                {
                    hud.blit_rgba(0.0, 0.0, width, height, pixels);
                }
            }
            // Pain and ACS FadeTo cover the whole window. The post chain
            // already mixes the palette tint when it is on.
            let mut screen_tints: Vec<[f32; 4]> = Vec::new();
            if let Some(world) = self.play.as_ref().and_then(|g| g.world()) {
                // `Powerup.Color` / `Player.DamageScreenColor`.
                let extra = room_sim::powerups::screen_blend(world, usize::from(self.local_seat));
                let replaces_pain =
                    extra.is_some_and(|(_, r)| r) && (1..=8).contains(&self.hud.palette);
                // Accessibility `r_reduce_flashing`: damage / pickup / powerup
                // screen flashes drawn at a third of their strength.
                let flash = if self.render_settings.reduce_flashing {
                    0.35
                } else {
                    1.0
                };
                if !use_post && !replaces_pain {
                    if let Some(tint) = room_ui::screen_blend(self.hud.palette) {
                        screen_tints.push([tint.r, tint.g, tint.b, tint.a * flash]);
                    }
                }
                if let Some((rgba, _)) = extra {
                    if rgba[3] > 0.001 {
                        screen_tints.push([rgba[0], rgba[1], rgba[2], rgba[3] * flash]);
                    }
                }
                let fade = world.screen_fade;
                if fade.a > 0.001 {
                    screen_tints.push([fade.r, fade.g, fade.b, fade.a.min(1.0)]);
                }
            }
            // On the GPU over the 3D view and the weapon (one quad instead of
            // blending and uploading the whole canvas each frame of a flash),
            // under the status bar, as in GZDoom's hardware renderer. Duke's
            // palette remap needs them in the canvas.
            let gpu_tints = self.hud_sprites.is_some()
                && duke_palette_state.is_none()
                && !cvar_bool(&self.console.cvars, "r_cpuhud");
            let screen_tint = if gpu_tints {
                combine_tints(&screen_tints)
            } else {
                for c in &screen_tints {
                    hud.fill_rect(0.0, 0.0, fb_w, fb_h, *c);
                }
                None
            };
            if duke_palette_state.is_some() {
                Sink {
                    canvas: &mut hud,
                    duke_palette_state,
                    remap_cache: std::collections::HashMap::new(),
                }
                .remap_canvas_from(draw_list_end);
            }

            let mut launcher_backdrops = Vec::new();
            if self.launcher.is_some() {
                launcher_backdrops =
                    launcher_input::paint_launcher(&mut hud, &self.launcher_frame, (fb_w, fb_h));
            } else if self.touch_layout.is_some() && self.touch_shown {
                if let Some((x, y, size, rgba)) = radar_sprite {
                    hud.blit_rgba(x, y, size, size, rgba);
                }
                if let Some(layout) = self.touch_layout.as_ref() {
                    for sprite in self.touch_overlay.sprites(layout, &self.touch) {
                        hud.blit_rgba(sprite.x, sprite.y, sprite.w, sprite.h, sprite.rgba);
                    }
                }
            }
            // `vid_fps` (launcher: "Show frame rate", pause menu: "FPS
            // graph"; off by default): frame rate, frame times and a graph of the last
            // frames, top centre (phones have no window title to show it in).
            // Rebuilt 5 times a second so the HUD rows it covers are not
            // uploaded every frame.
            if self.launcher.is_none()
                && matches!(
                    self.console.cvars.get("vid_fps"),
                    Some(room_console::CvarValue::Bool(true))
                        | Some(room_console::CvarValue::Int(1))
                )
            {
                let now = std::time::Instant::now();
                let stale = self.perf_panel.as_ref().is_none_or(|(at, _)| {
                    now.duration_since(*at) >= std::time::Duration::from_millis(200)
                });
                if stale {
                    let scale = self.window.as_ref().map_or(1.0, |w| w.scale_factor()) as f32;
                    let screen_w = self.config.as_ref().map_or(0, |c| c.width) as f32;
                    let top = crate::platform::current().safe_area().top as f32;
                    let panel = perf_panel(
                        &self.perf,
                        perf_target_ms,
                        self.render_scale,
                        (screen_w, top, scale),
                    );
                    self.perf_panel = Some((now, panel));
                }
                if let Some((_, panel)) = &self.perf_panel {
                    hud.cmds.extend(panel.iter().cloned());
                }
            } else {
                self.perf_panel = None;
            }
            self.perf.mark("hud");
            let Ok(frame) = surface.get_current_texture() else {
                return;
            };
            self.perf.mark("wait");
            // Draw into a COPY_SRC colour buffer, then blit to the swapchain.
            let scene_view = &frame_color.1;
            let mut encoder = gpu
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("frame"),
                });
            {
                let clear = if ui_only {
                    wgpu::Color::BLACK
                } else {
                    wgpu::Color {
                        r: 0.05,
                        g: 0.05,
                        b: 0.08,
                        a: 1.0,
                    }
                };
                let mut rp = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("main"),
                    color_attachments: &[
                        Some(wgpu::RenderPassColorAttachment {
                            view: scene_view,
                            resolve_target: None,
                            ops: wgpu::Operations {
                                load: wgpu::LoadOp::Clear(clear),
                                store: wgpu::StoreOp::Store,
                            },
                        }),
                        Some(wgpu::RenderPassColorAttachment {
                            view: &stencil_aux.1,
                            resolve_target: None,
                            ops: wgpu::Operations {
                                load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                                store: wgpu::StoreOp::Discard,
                            },
                        }),
                    ],
                    depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                        view: &depth.1,
                        depth_ops: Some(wgpu::Operations {
                            load: wgpu::LoadOp::Clear(1.0),
                            store: wgpu::StoreOp::Store,
                        }),
                        stencil_ops: Some(wgpu::Operations {
                            load: wgpu::LoadOp::Clear(0),
                            store: wgpu::StoreOp::Store,
                        }),
                    }),
                    timestamp_writes: None,
                    occlusion_query_set: None,
                });
                // Title / intermission / finale are full-screen UI — do not keep drawing
                // the last level camera (letterbox sides were showing the 3D world).
                if !ui_only {
                    if let Some(vfs) = vfs {
                        materials.anisotropy = render_settings.anisotropy;
                        materials.set_hq_resize(render_settings.hq_resize);
                        pass.draw_materials(gpu, &mut rp, &scene, materials, vfs);
                    } else {
                        pass.draw(&mut rp, &scene);
                    }
                    if let Some(meshes) = zone_meshes.as_ref() {
                        if !meshes.wall_depth.is_empty()
                            || !meshes.wall_through.is_empty()
                            || !meshes.ring.is_empty()
                        {
                            zone_pass.prepare(gpu, &self.camera, pass.time);
                            zone_pass.upload(gpu, meshes);
                            zone_pass.draw(&mut rp, meshes);
                        }
                    }
                }
            }
            let color_screen_w = self.config.as_ref().map_or(1, |c| c.width);
            // GLDEFS custom post shaders: frame_color → post_ping → frame_color.
            if !ui_only
                && let Some((_, runners)) = self.hw_post.as_mut()
                && runners.iter().any(|h| h.enabled)
            {
                // As in GZDoom, the weapon is part of the scene the shaders
                // see (a colour mod recolours the hands too); the status bar
                // is not. Drawn here at the scene's resolution, not later.
                if !gpu_psprites.is_empty() {
                    let (fw, fh) = (frame_color.0.width(), frame_color.0.height());
                    let k = fw as f32 / color_screen_w.max(1) as f32;
                    let format = frame_color.0.format();
                    let sprites = self
                        .scene_sprites
                        .get_or_insert_with(|| room_render::HudSprites::new(&gpu.device, format));
                    let quads = sprite_quads(&gpu_psprites, k);
                    let patches = &self.patches;
                    sprites.prepare(gpu, (fw, fh), &quads, None, &mut |key: &str| {
                        patch_picture(patches, key)
                    });
                    {
                        let mut rp = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                            label: Some("weapon-in-scene"),
                            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                                view: &frame_color.1,
                                resolve_target: None,
                                ops: wgpu::Operations {
                                    load: wgpu::LoadOp::Load,
                                    store: wgpu::StoreOp::Store,
                                },
                            })],
                            depth_stencil_attachment: None,
                            timestamp_writes: None,
                            occlusion_query_set: None,
                        });
                        sprites.draw(&mut rp);
                    }
                    gpu_psprites.clear();
                }
                let game = self
                    .play
                    .as_ref()
                    .and_then(|g| g.world())
                    .map_or(0.0, |w| w.level.maptime as f32 / 35.0);
                let times = room_render::PostTimes {
                    delta: dt,
                    time: pass.time,
                    game,
                };
                let size = wgpu::Extent3d {
                    width: frame_color.0.width(),
                    height: frame_color.0.height(),
                    depth_or_array_layers: 1,
                };
                for runner in runners
                    .iter_mut()
                    .filter(|h| h.enabled)
                    .map(|h| &mut h.runner)
                {
                    runner.run_texture(
                        &gpu.device,
                        &gpu.queue,
                        &mut encoder,
                        &frame_color.0,
                        &post_ping.0,
                        times,
                        &[],
                    );
                    encoder.copy_texture_to_texture(
                        post_ping.0.as_image_copy(),
                        frame_color.0.as_image_copy(),
                        size,
                    );
                }
            }
            let (cap_w, cap_h) = self
                .config
                .as_ref()
                .map(|c| (c.width, c.height))
                .unwrap_or((1, 1));
            let present_tex = if use_post && !ui_only {
                let mut blend = [0.0_f32, 0.0, 0.0, 0.0];
                if let Some(tint) = room_ui::screen_blend(self.hud.palette) {
                    let flash = if render_settings.reduce_flashing {
                        0.35
                    } else {
                        1.0
                    };
                    blend = [tint.r, tint.g, tint.b, tint.a * flash];
                }
                if render_settings.damage_fx && self.hud.health > 0 && self.hud.health < 25 {
                    let low = ((25 - self.hud.health) as f32 / 25.0).clamp(0.0, 1.0);
                    blend[0] = blend[0].max(0.85);
                    blend[1] = blend[1].min(0.05);
                    blend[2] = blend[2].min(0.02);
                    blend[3] = blend[3].max(0.2 + low * 0.55);
                }
                if render_settings.screen_blood && self.hud.palette >= 4 {
                    blend[0] = blend[0].max(0.75);
                    blend[3] = blend[3].max(0.35);
                }
                let mut look = render_settings.clone();
                {
                    let water = self
                        .play
                        .as_ref()
                        .and_then(|g| g.world())
                        .and_then(|w| {
                            w.players
                                .get(usize::from(self.local_seat))
                                .and_then(|p| p.as_ref())
                                .and_then(|p| p.mo)
                                .and_then(|mo| w.actors.get(mo))
                                .map(|a| room_sim::actor_waterlevel(w, a))
                        })
                        .unwrap_or(0);
                    let liquids = render_settings.liquids;
                    if (render_settings.underwater_fx && water >= 2) || (liquids > 0 && water >= 1)
                    {
                        let amt = if water >= 2 { 1.0 } else { 0.45 };
                        look.chromatic = look.chromatic.max(0.35 * amt);
                        look.vignette = look.vignette.max(0.45 * amt);
                        if liquids >= 2 {
                            look.grain = look.grain.max(0.12 * amt);
                        }
                        blend[2] = blend[2].max(0.35 * amt);
                        blend[1] = blend[1].max(0.15 * amt);
                        blend[3] = blend[3].max(0.2 * amt);
                    }
                }
                if render_settings.powerup_fx {
                    match self.hud.palette {
                        // Light-amp / invuln-ish classic flashes → green phosphor edge.
                        1 if self.fixed_colormap == 1 => {
                            blend = [0.1, 0.85, 0.2, blend[3].max(0.25)];
                            look.grain = look.grain.max(0.2);
                            look.vignette = look.vignette.max(0.4);
                        }
                        12..=15 => {
                            // Radiation suit-ish green.
                            blend[1] = blend[1].max(0.35);
                            blend[3] = blend[3].max(0.15);
                        }
                        _ => {}
                    }
                }
                if render_settings.color_grade {
                    look.vignette = look.vignette.max(0.12);
                    look.grain = look.grain.max(0.05);
                    look.sharpen = look.sharpen.max(0.15);
                }
                if render_settings.menu_blur && self.menu.is_some() {
                    // G1 stand-in until dual-Kawase menu blur: soften via bloom + vignette.
                    look.bloom = true;
                    look.bloom_amount = look.bloom_amount.max(0.9);
                    look.vignette = look.vignette.max(0.55);
                    look.chromatic = look.chromatic.max(0.08);
                }
                if self.heat_haze_tics > 0 {
                    let pulse = f32::from(self.heat_haze_tics) / 12.0;
                    look.chromatic = look.chromatic.max(0.15 + pulse * 0.35);
                    look.grain = look.grain.max(0.08 * pulse);
                }
                if look.motion_blur {
                    // F12 stand-in: smear via bloom + mild CA until velocity buffer lands.
                    look.bloom = true;
                    look.bloom_amount = look.bloom_amount.max(0.7);
                    look.chromatic = look.chromatic.max(0.12);
                    look.sharpen = 0.0;
                }
                if look.dof {
                    // F11 stand-in: edge soften for menus/death cam until bokeh pass.
                    look.bloom = true;
                    look.vignette = look.vignette.max(0.35);
                    look.sharpen = 0.0;
                }
                if look.auto_exposure {
                    // F2 stand-in: nudge exposure from sector light until histogram compute.
                    let sector_l = self
                        .play
                        .as_ref()
                        .and_then(|g| g.world())
                        .and_then(|w| {
                            w.players
                                .get(usize::from(self.local_seat))
                                .and_then(|p| p.as_ref())
                                .and_then(|p| p.mo)
                                .and_then(|mo| w.actors.get(mo))
                                .map(|a| room_sim::actor_light_level(w, a) as f32 / 255.0)
                        })
                        .unwrap_or(0.5);
                    let target = (0.55 / sector_l.max(0.15)).clamp(0.7, 1.6);
                    look.exposure = look.exposure * 0.85 + target * 0.15;
                }
                if look.outlines {
                    // E2 stand-in: slight edge pop via sharpen + mild CA.
                    look.sharpen = look.sharpen.max(0.45);
                    look.chromatic = look.chromatic.max(0.06);
                }
                post_chain.apply_views(
                    gpu,
                    &mut encoder,
                    scene_view,
                    &depth.0,
                    &post_ping.1,
                    &post_ldr.1,
                    PostChainInputs {
                        settings: &look,
                        blend,
                        width: frame_color.0.width(),
                        height: frame_color.0.height(),
                        time: pass.time,
                        scene_encoded: false,
                    },
                );
                post_ldr
            } else {
                frame_color
            };
            // Render scale below 1: stretch the 3D view to the screen; the
            // HUD below is drawn on the full-size result.
            let present_tex = match (&self.scene_full, &self.upscaler) {
                (Some(full), Some(up)) => {
                    up.draw(&gpu.device, &mut encoder, &present_tex.1, &full.1);
                    full
                }
                _ => present_tex,
            };
            let color_view = &present_tex.1;
            let (color_target_w, color_target_h) = (present_tex.0.width(), present_tex.0.height());
            {
                self.perf.mark("draw");
                canvas_gpu.rasterize_frame(hud, None);
                self.perf.mark("hud raster");
                if let Some(sprites) = self.hud_sprites.as_mut() {
                    let mut quads: Vec<room_render::SpriteQuad> =
                        launcher_backdrops.iter().map(|b| b.quad.clone()).collect();
                    quads.extend(sprite_quads(&gpu_psprites, 1.0));
                    quads.extend(sprite_quads(&gpu_overlays, 1.0));
                    let patches = &self.patches;
                    let mut fetch = |key: &str| {
                        if let Some(b) = launcher_backdrops.iter().find(|b| b.quad.key == key) {
                            return Some((b.w, b.h, b.rgba.as_ref().clone()));
                        }
                        patch_picture(patches, key)
                    };
                    sprites.prepare(
                        gpu,
                        (color_target_w, color_target_h),
                        &quads,
                        screen_tint,
                        &mut fetch,
                    );
                }
                let mut rp = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("hud"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: color_view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Load,
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                });
                if let Some(sprites) = self.hud_sprites.as_ref() {
                    sprites.draw(&mut rp);
                }
                canvas_gpu.blit(gpu, &mut rp);
            }
            self.perf.mark("hud upload");
            encoder.copy_texture_to_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &present_tex.0,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::TexelCopyTextureInfo {
                    texture: &frame.texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::Extent3d {
                    width: cap_w,
                    height: cap_h,
                    depth_or_array_layers: 1,
                },
            );
            gpu.queue.submit(Some(encoder.finish()));
            self.perf.mark("submit");
            let captured = if want_shot || want_gif {
                match read_texture_rgba(gpu, &present_tex.0, cap_w, cap_h) {
                    Ok(rgba) => Some((rgba, cap_w, cap_h)),
                    Err(e) => {
                        eprintln!("room: capture failed: {e}");
                        Some((Vec::new(), 0, 0)) // signal error via empty
                    }
                }
            } else {
                None
            };
            frame.present();
            self.perf.mark("present");
            captured
        };
        if let Some((rgba, w, h)) = capture_rgba {
            if rgba.is_empty() {
                self.pending_screenshot = false;
                self.messages.push("capture failed", 70);
            } else {
                if want_shot {
                    self.pending_screenshot = false;
                    self.save_screenshot_rgba(&rgba, w, h);
                    self.quit |= self.shot_after.take().is_some();
                }
                if want_dump && let Some((dir, _, _)) = frame_dump() {
                    let path = dir.join(format!("frame_{:06}.png", self.frames));
                    let _ = std::fs::create_dir_all(&dir);
                    if let Err(e) = image_png(&path, &rgba, w, h) {
                        eprintln!("room: frame dump {}: {e}", path.display());
                    }
                }
                if let Some(rec) = self.gif_recorder.as_mut() {
                    let n = rec.frames.len();
                    if rec.push_rgba(&rgba, w, h) && n + 1 >= screen_capture::GIF_MAX_FRAMES {
                        self.finish_gif_record();
                    }
                }
            }
        }
    }

    fn integrate(&mut self, dt: f32) {
        let speed = if self.keys.shift { 640.0 } else { 320.0 };
        let forward =
            Vec3::new(self.camera.forward.x, self.camera.forward.y, 0.0).normalize_or_zero();
        let right = Vec3::new(-forward.y, forward.x, 0.0);
        let mut move_dir = Vec3::ZERO;
        if self.keys.w {
            move_dir += forward;
        }
        if self.keys.s {
            move_dir -= forward;
        }
        if self.keys.d {
            move_dir += right;
        }
        if self.keys.a {
            move_dir -= right;
        }
        if self.keys.space {
            move_dir += Vec3::Z;
        }
        if self.keys.c {
            move_dir -= Vec3::Z;
        }
        if move_dir.length_squared() > 0.0 {
            self.camera.eye += move_dir.normalize() * speed * dt;
        }
    }

    /// Drive Headless from keyboard / mouse samples at [`GameRuntime::tic_rate`].
    fn tick_play(&mut self, dt: f32) {
        if self.duke_session.is_some() {
            self.tick_duke(dt);
            return;
        }
        let tic = 1.0 / self.play.as_ref().map(GameRuntime::tic_rate).unwrap_or(35) as f32;
        self.tic_accum += dt;
        let rollback_window = match self.console.cvars.get("cl_rollback") {
            Some(room_console::CvarValue::Int(n)) => u32::try_from(*n).unwrap_or(0),
            _ => 0,
        };
        let rollback_ok =
            rollback_window > 0 && self.play.as_ref().is_some_and(Headless::can_speculate);
        if let Some(net) = self.net.as_mut() {
            net.set_rollback_window(if rollback_ok { rollback_window } else { 0 });
        }
        let rollback = self.net.as_ref().is_some_and(netplay::NetPlay::rollback);
        if !rollback {
            self.rollback_world = None;
        }
        let mut rollback_submitted = false;
        while self.tic_accum >= tic || !self.rollback_frames.is_empty() {
            let queued = self.rollback_frames.pop_front();
            let cmds: Vec<TicCmd> = if let Some(frame) = queued {
                frame
            } else {
                self.tic_accum -= tic;
                self.perf.add_tics(1);
                let sample = self.sample_input();
                let mut bits = build_ticcmd_with_mods_and_impulses(
                    &sample,
                    self.mod_buttons,
                    self.weapon_impulses,
                );
                if self.menu.is_none()
                    && let Some(pad) = self.pad
                {
                    room_input::apply_gamepad(&mut bits, &pad, &self.gamepad_config());
                }
                if self.menu.is_none()
                    && let Some((x, y)) = self.touch_analog
                {
                    // The on-screen stick replaces key movement: a full push
                    // runs, a half push walks, at any angle (as the web client).
                    bits.forward_move = touch_input::analog_units(y, 50.0 * 256.0);
                    bits.side_move = touch_input::analog_units(x, 40.0 * 256.0);
                }
                bits.buttons |= std::mem::take(&mut self.inv_cmd);
                if let Some((load, slot)) = self.net_save_cmd.take() {
                    bits.buttons |= if load {
                        Buttons::NETLOAD
                    } else {
                        Buttons::NETSAVE
                    };
                    bits.weapon_select = slot;
                }
                let cmd = if self.net_away.is_some() {
                    // Seat away (off screen): stand still, fire nothing.
                    TicCmd::default()
                } else {
                    TicCmd {
                        buttons: Buttons(bits.buttons),
                        pitch: bits.pitch,
                        yaw: bits.yaw,
                        roll: bits.roll,
                        forward_move: bits.forward_move,
                        side_move: bits.side_move,
                        up_move: bits.up_move,
                        weapon_select: bits.weapon_select,
                    }
                };
                if rollback && let Some(net) = self.net.as_mut() {
                    // Rollback: never wait; released frames run next, then the
                    // speculative copy is rebuilt after the loop.
                    let frames = net.rollback_tic(netplay::tic_to_net(cmd));
                    if let Some(tic) = net.take_desync() {
                        self.messages.push(
                            format!("Network desync at tic {tic}: players' games differ"),
                            175,
                        );
                    }
                    self.rollback_frames.extend(frames);
                    rollback_submitted = true;
                    continue;
                }
                if let Some(net) = self.net.as_mut() {
                    let play = &self.play;
                    let result = net.advance_doom_tic(netplay::tic_to_net(cmd), || {
                        play.as_ref().and_then(Headless::checksum)
                    });
                    if let Some(tic) = net.take_desync() {
                        self.messages.push(
                            format!("Network desync at tic {tic}: players' games differ"),
                            175,
                        );
                    }
                    match result {
                        Ok(c) => c,
                        Err(e) => {
                            eprintln!("room: netplay: {e}");
                            continue;
                        }
                    }
                } else {
                    vec![cmd]
                }
            };
            let mut msgs = Vec::new();
            let mut cam = None;
            // Set again below while a `VPSF_ORTHOGRAPHIC` view is active.
            self.camera.ortho = None;
            let mut hud_snap = None;
            let mut planes = Vec::new();
            let mut side_offs = Vec::new();
            let mut switch_tex = Vec::new();
            let mut floor_pics = Vec::new();
            let mut ceiling_pics = Vec::new();
            let mut floor_scroll = Vec::new();
            let mut ceil_scroll = Vec::new();
            let mut floor_scales = Vec::new();
            let mut ceiling_scales = Vec::new();
            let mut plane_angles = Vec::new();
            let mut side_scales = Vec::new();
            let mut plane_alphas = Vec::new();
            let mut plane_reflects = Vec::new();
            let mut line_alphas = Vec::new();
            let mut new_decals = Vec::new();
            let mut new_particles = Vec::new();
            let mut particle_level_frozen = false;
            let mut blood_sprays: Vec<(glam::Vec3, glam::Vec3, [u8; 3], i32, u64, Option<f32>)> =
                Vec::new();
            // Floor splat: (x, y, floor_z, rgb, radius, seed)
            let mut floor_splats: Vec<(f32, f32, f32, [u8; 3], f32, u64)> = Vec::new();
            // Wall splat: (x, y, z, line, rgb, seed)
            let mut wall_splats: Vec<(f32, f32, f32, u32, [u8; 3], u64)> = Vec::new();
            let mut muzzle_flashes: Vec<(glam::Vec3, glam::Vec3, u64)> = Vec::new();
            let mut tracers: Vec<(glam::Vec3, glam::Vec3, u64)> = Vec::new();
            let mut impacts: Vec<(glam::Vec3, glam::Vec3, u64)> = Vec::new();
            let mut explosions: Vec<(glam::Vec3, f32, u64)> = Vec::new();
            let mut corpse_pools: Vec<(glam::Vec3, [u8; 3], i32, bool, u64, Option<f32>)> =
                Vec::new();
            let mut trail_puffs: Vec<(glam::Vec3, glam::Vec3, u64)> = Vec::new();
            let mut casings: Vec<(glam::Vec3, glam::Vec3, u64)> = Vec::new();
            let mut weapon_smoke: Vec<(glam::Vec3, glam::Vec3, u64)> = Vec::new();
            let mut kick_impulse = (0.0_f32, 0.0_f32);
            let mut entered_intermission = false;
            let mut vote_passed = false;
            let mut net_save_req = None;
            let gore_level = match self.console.cvars.get("r_gore") {
                Some(room_console::CvarValue::Int(i)) => *i,
                _ => 1,
            };
            let blood_fx_on = match self.console.cvars.get("r_bloodfx") {
                Some(room_console::CvarValue::Bool(b)) => *b,
                Some(room_console::CvarValue::Int(i)) => *i != 0,
                _ => false,
            };
            let muzzle_fx_on = match self.console.cvars.get("r_muzzlefx") {
                Some(room_console::CvarValue::Bool(b)) => *b,
                Some(room_console::CvarValue::Int(i)) => *i != 0,
                _ => false,
            };
            let tracers_on = match self.console.cvars.get("r_tracers") {
                Some(room_console::CvarValue::Bool(b)) => *b,
                Some(room_console::CvarValue::Int(i)) => *i != 0,
                _ => false,
            };
            let impact_fx_on = match self.console.cvars.get("r_impactfx") {
                Some(room_console::CvarValue::Bool(b)) => *b,
                Some(room_console::CvarValue::Int(i)) => *i != 0,
                _ => false,
            };
            let view_kick_on = match self.console.cvars.get("r_viewkick") {
                Some(room_console::CvarValue::Bool(b)) => *b,
                Some(room_console::CvarValue::Int(i)) => *i != 0,
                _ => false,
            };
            // GZDoom `r_quakeintensity`: scales earthquake view jitter.
            let quake_scale = match self.console.cvars.get("r_quakeintensity") {
                Some(room_console::CvarValue::Float(f)) => (*f).clamp(0.0, 1.0),
                Some(room_console::CvarValue::Int(i)) => f64::from(*i).clamp(0.0, 1.0),
                _ => 1.0,
            };
            let view_kick_scale = match self.console.cvars.get("r_viewkick_scale") {
                Some(room_console::CvarValue::Float(f)) => *f as f32,
                Some(room_console::CvarValue::Int(i)) => *i as f32,
                _ => 1.0,
            };
            let explosion_fx_on = match self.console.cvars.get("r_explosionfx") {
                Some(room_console::CvarValue::Bool(b)) => *b,
                Some(room_console::CvarValue::Int(i)) => *i != 0,
                _ => false,
            };
            let hit_confirm_on = match self.console.cvars.get("r_hitconfirm") {
                Some(room_console::CvarValue::Bool(b)) => *b,
                Some(room_console::CvarValue::Int(i)) => *i != 0,
                _ => false,
            };
            let predict_on = matches!(
                self.console.cvars.get("cl_predict"),
                Some(room_console::CvarValue::Bool(true) | room_console::CvarValue::Int(1..))
            );
            // Unreleased local input for `cl_predict` (netplay only).
            let pending_local: Vec<TicCmd> = if predict_on {
                self.net
                    .as_ref()
                    .map(netplay::NetPlay::pending_local_cmds)
                    .unwrap_or_default()
            } else {
                Vec::new()
            };
            let hit_flash_on = match self.console.cvars.get("r_hitflash") {
                Some(room_console::CvarValue::Bool(b)) => *b,
                Some(room_console::CvarValue::Int(i)) => *i != 0,
                _ => false,
            };
            let corpse_pools_on = match self.console.cvars.get("r_corpsepools") {
                Some(room_console::CvarValue::Bool(b)) => *b,
                Some(room_console::CvarValue::Int(i)) => *i != 0,
                _ => false,
            };
            let gibs_on = match self.console.cvars.get("r_gibs") {
                Some(room_console::CvarValue::Bool(b)) => *b,
                Some(room_console::CvarValue::Int(i)) => *i != 0,
                _ => false,
            };
            let trails_on = match self.console.cvars.get("r_trails") {
                Some(room_console::CvarValue::Bool(b)) => *b,
                Some(room_console::CvarValue::Int(i)) => *i != 0,
                _ => false,
            };
            let casings_on = match self.console.cvars.get("r_casings") {
                Some(room_console::CvarValue::Bool(b)) => *b,
                Some(room_console::CvarValue::Int(i)) => *i != 0,
                _ => false,
            };
            let weapon_smoke_on = match self.console.cvars.get("r_weaponsmoke") {
                Some(room_console::CvarValue::Bool(b)) => *b,
                Some(room_console::CvarValue::Int(i)) => *i != 0,
                _ => false,
            };
            let heat_haze_on = match self.console.cvars.get("r_heathaze") {
                Some(room_console::CvarValue::Bool(b)) => *b,
                Some(room_console::CvarValue::Int(i)) => *i != 0,
                _ => false,
            };
            let dissolve_on = match self.console.cvars.get("r_dissolve") {
                Some(room_console::CvarValue::Bool(b)) => *b,
                Some(room_console::CvarValue::Int(i)) => *i != 0,
                _ => false,
            };
            let blood_drips_on = match self.console.cvars.get("r_blooddrips") {
                Some(room_console::CvarValue::Bool(b)) => *b,
                Some(room_console::CvarValue::Int(i)) => *i != 0,
                _ => false,
            };
            if let Some(game) = self.play.as_mut() {
                let autoaim = !matches!(
                    self.console.cvars.get("autoaim"),
                    Some(room_console::CvarValue::Bool(false))
                );
                if let Some(world) = game.world_mut() {
                    world.autoaim = autoaim;
                    world.fx_events_enabled = true;
                }
                let prev_state = game.state;
                game.tick(&cmds);
                if rollback && let Some(net) = self.net.as_mut() {
                    net.rollback_frame_applied(|| game.checksum());
                }
                for line in game.drain_script_log() {
                    eprintln!("room: mod {line}");
                }
                entered_intermission =
                    prev_state == GameState::Level && game.state == GameState::Intermission;
                if game.take_intermission_advance() && self.net.is_some() {
                    vote_passed = true;
                }
                net_save_req = game.take_net_save_request();
                if let Some(vfs) = self.vfs.as_ref() {
                    // Userinfo `gender` for `$playersound` (per local seat).
                    if let Some(room_console::CvarValue::String(g)) =
                        self.console.cvars.get("gender")
                    {
                        game.set_player_gender(self.local_seat, room_sim::parse_gender(g));
                    }
                    let events = if rollback {
                        game.drain_audio_for_predicted(
                            vfs,
                            self.local_seat,
                            &mut self.rollback_sounds,
                        )
                    } else {
                        game.drain_audio_for(vfs, self.local_seat)
                    };
                    let music_req = game.take_music_request();
                    if let Some(audio) = self.audio.as_ref() {
                        if let Ok(mut shared) = audio.shared.lock() {
                            if let Some(sounds) = game.sounds.as_ref() {
                                shared.mixer.bank.merge_missing(&sounds.bank);
                            }
                            shared.mixer.apply(&events);
                            let seat = usize::from(self.local_seat);
                            if let Some(p) = game
                                .world()
                                .and_then(|w| w.players.get(seat).and_then(|p| p.as_ref()))
                            {
                                if let Some(view) = p.camera.or(p.mo) {
                                    if let Some(a) = game.world().and_then(|w| w.actors.get(view)) {
                                        shared.mixer.set_listener(a.pos, a.camera_yaw().radians());
                                    }
                                }
                            }
                            let paused = game.world().is_some_and(|w| w.paused);
                            shared.music.set_paused(paused);
                            if let Some(song) = music_req.as_deref() {
                                if let Err(e) = play_music_lump(&mut shared.music, vfs, song) {
                                    eprintln!("room: SetMusic {song}: {e}");
                                }
                            }
                        }
                    }
                }
                if let Some(world) = game.world_mut() {
                    particle_level_frozen = world.level.frozen;
                    msgs = world.drain_messages();
                    // Diagnostics go to the console and the log; on screen
                    // only with `developer` on (as in GZDoom).
                    let developer = cvar_bool(&self.console.cvars, "developer");
                    let trace = developer || env_trace();
                    room_sim::set_trace(trace);
                    for line in world.drain_debug() {
                        // The terminal / log copy only when asked: on a phone
                        // each line costs a logcat write and a file write.
                        if trace {
                            eprintln!("room: {line}");
                        }
                        if developer {
                            self.messages.push(line.clone(), 105);
                        }
                        self.console.print(line);
                    }
                    if let Some(name) = world.take_slideshow_request() {
                        msgs.push(format!("Slideshow: {name}"));
                    }
                    if let Some((log, sound)) = world.take_subtitle_request() {
                        if sound.is_empty() {
                            msgs.push(format!("Subtitle #{log}"));
                        } else {
                            msgs.push(format!("Subtitle #{log} ({sound})"));
                        }
                    }
                    new_decals = world
                        .drain_decal_events()
                        .into_iter()
                        .map(|e| (e.name, e.x, e.y, e.z, e.line))
                        .collect();
                    new_particles = world
                        .drain_particle_events()
                        .into_iter()
                        .map(|e| room_render::particles::ParticleSpawn {
                            color: e.color,
                            fullbright: e.fullbright,
                            render_style: e.render_style,
                            lifetime: e.lifetime,
                            size: e.size,
                            pos: [e.x, e.y, e.z],
                            vel: [e.velocity.x(), e.velocity.y(), e.velocity.z()],
                            accel: [e.acceleration.x(), e.acceleration.y(), e.acceleration.z()],
                            alpha: e.alpha,
                            fade_step: e.fade_step,
                            flags: e.flags,
                            fade_out_step: e.fade_out_step,
                            size_step: e.size_step,
                            roll: e.roll,
                            texture: e.texture,
                        })
                        .collect();
                    let maptime = world.level.maptime;
                    if trails_on {
                        for (ti, id) in world.thinkers.default.iter().copied().enumerate() {
                            let Some(a) = world.actors.get(id) else {
                                continue;
                            };
                            if a.destroyed || (a.flags & FLAG_MISSILE) == 0 {
                                continue;
                            }
                            let speed = (a.vel.x() * a.vel.x()
                                + a.vel.y() * a.vel.y()
                                + a.vel.z() * a.vel.z())
                            .sqrt();
                            if speed < 1.0 {
                                continue;
                            }
                            #[allow(clippy::cast_possible_truncation)]
                            trail_puffs.push((
                                glam::Vec3::new(
                                    a.pos.x() as f32,
                                    a.pos.y() as f32,
                                    a.pos.z() as f32,
                                ),
                                glam::Vec3::new(
                                    a.vel.x() as f32,
                                    a.vel.y() as f32,
                                    a.vel.z() as f32,
                                ),
                                room_sim::FxEvent::render_seed(maptime, ti as u32),
                            ));
                        }
                    }
                    for (ei, ev) in world.drain_fx_events().into_iter().enumerate() {
                        let seed = room_sim::FxEvent::render_seed(maptime, ei as u32);
                        match ev {
                            room_sim::FxEvent::BloodHit {
                                victim,
                                pos,
                                dir,
                                damage,
                                color,
                            } => {
                                if hit_flash_on {
                                    self.hit_flash.insert(victim.index(), 5);
                                }
                                if gore_level >= 2 && blood_fx_on {
                                    let floor_z = room_sim::floor_at_xy(world, pos.x(), pos.y())
                                        .unwrap_or(pos.z());
                                    #[allow(clippy::cast_possible_truncation)]
                                    let floor_zf = floor_z as f32;
                                    #[allow(clippy::cast_possible_truncation)]
                                    let p = glam::Vec3::new(
                                        pos.x() as f32,
                                        pos.y() as f32,
                                        pos.z() as f32,
                                    );
                                    #[allow(clippy::cast_possible_truncation)]
                                    blood_sprays.push((
                                        p,
                                        glam::Vec3::new(
                                            dir.x() as f32,
                                            dir.y() as f32,
                                            dir.z() as f32,
                                        ),
                                        [color.r, color.g, color.b],
                                        damage,
                                        seed,
                                        Some(floor_zf),
                                    ));
                                    let radius = (8.0 + damage.clamp(1, 40) as f32 * 0.4).min(32.0);
                                    #[allow(clippy::cast_possible_truncation)]
                                    floor_splats.push((
                                        pos.x() as f32,
                                        pos.y() as f32,
                                        floor_zf,
                                        [color.r, color.g, color.b],
                                        radius,
                                        seed,
                                    ));
                                    // Secondary puddles along exit direction.
                                    for k in 1u32..=2 {
                                        let t = f64::from(k) * 10.0;
                                        let sx = pos.x() + dir.x() * t;
                                        let sy = pos.y() + dir.y() * t;
                                        if let Some(fz) = room_sim::floor_at_xy(world, sx, sy) {
                                            #[allow(clippy::cast_possible_truncation)]
                                            floor_splats.push((
                                                sx as f32,
                                                sy as f32,
                                                fz as f32,
                                                [color.r, color.g, color.b],
                                                radius * (1.0 - 0.2 * k as f32),
                                                seed ^ (u64::from(k) * 0x9E37),
                                            ));
                                        }
                                    }
                                    if let Some((line, wx, wy)) =
                                        nearest_blood_wall(&world.map, pos.x(), pos.y(), 72.0)
                                    {
                                        let wz = pos.z().clamp(floor_z + 8.0, floor_z + 56.0);
                                        #[allow(clippy::cast_possible_truncation)]
                                        wall_splats.push((
                                            wx as f32,
                                            wy as f32,
                                            wz as f32,
                                            line,
                                            [color.r, color.g, color.b],
                                            seed ^ 0xB_100D,
                                        ));
                                    }
                                }
                            }
                            room_sim::FxEvent::WeaponFired {
                                player,
                                origin,
                                forward,
                                ..
                            } => {
                                #[allow(clippy::cast_possible_truncation)]
                                let o = glam::Vec3::new(
                                    origin.x() as f32,
                                    origin.y() as f32,
                                    origin.z() as f32,
                                );
                                #[allow(clippy::cast_possible_truncation)]
                                let f = glam::Vec3::new(
                                    forward.x() as f32,
                                    forward.y() as f32,
                                    forward.z() as f32,
                                );
                                if muzzle_fx_on {
                                    muzzle_flashes.push((o, f, seed));
                                }
                                if casings_on {
                                    casings.push((o, f, seed ^ 0xC45E));
                                }
                                if weapon_smoke_on {
                                    weapon_smoke.push((o, f, seed ^ 0xA5A5_A5A5_5A5A_5A5A));
                                }
                                if view_kick_on && player == self.local_seat {
                                    let s = view_kick_scale.clamp(0.0, 4.0);
                                    kick_impulse.0 += 0.012 * s;
                                    kick_impulse.1 +=
                                        (((seed >> 8) as i64 % 7) - 3) as f32 * 0.0025 * s;
                                }
                            }
                            room_sim::FxEvent::HitscanFired {
                                origin,
                                end,
                                hit_actor,
                                shooter,
                                ..
                            } => {
                                if tracers_on {
                                    #[allow(clippy::cast_possible_truncation)]
                                    tracers.push((
                                        glam::Vec3::new(
                                            origin.x() as f32,
                                            origin.y() as f32,
                                            origin.z() as f32,
                                        ),
                                        glam::Vec3::new(
                                            end.x() as f32,
                                            end.y() as f32,
                                            end.z() as f32,
                                        ),
                                        seed,
                                    ));
                                }
                                if impact_fx_on && hit_actor.is_none() {
                                    #[allow(clippy::cast_possible_truncation)]
                                    let o = glam::Vec3::new(
                                        origin.x() as f32,
                                        origin.y() as f32,
                                        origin.z() as f32,
                                    );
                                    #[allow(clippy::cast_possible_truncation)]
                                    let e = glam::Vec3::new(
                                        end.x() as f32,
                                        end.y() as f32,
                                        end.z() as f32,
                                    );
                                    impacts.push((e, e - o, seed));
                                }
                                if hit_confirm_on && hit_actor.is_some() {
                                    let local_mo = world
                                        .players
                                        .get(usize::from(self.local_seat))
                                        .and_then(|p| p.as_ref())
                                        .and_then(|p| p.mo);
                                    if local_mo == Some(shooter) {
                                        self.hud.palette = self.hud.palette.max(2);
                                    }
                                }
                            }
                            room_sim::FxEvent::ActorDied {
                                pos,
                                color,
                                mass,
                                gibbed,
                                ..
                            } => {
                                let want_gore = gore_level >= 2
                                    && ((corpse_pools_on && !gibbed)
                                        || (gibs_on && gibbed)
                                        || blood_fx_on);
                                let floor_z = room_sim::floor_at_xy(world, pos.x(), pos.y())
                                    .unwrap_or(pos.z());
                                #[allow(clippy::cast_possible_truncation)]
                                let floor_zf = floor_z as f32;
                                if want_gore {
                                    let rgb =
                                        color.map(|c| [c.r, c.g, c.b]).unwrap_or([180, 16, 16]);
                                    #[allow(clippy::cast_possible_truncation)]
                                    corpse_pools.push((
                                        glam::Vec3::new(
                                            pos.x() as f32,
                                            pos.y() as f32,
                                            pos.z() as f32,
                                        ),
                                        rgb,
                                        mass,
                                        gibbed,
                                        seed,
                                        Some(floor_zf),
                                    ));
                                    let radius = if gibbed {
                                        18.0 + (mass as f32 * 0.04).min(20.0)
                                    } else {
                                        12.0 + (mass as f32 * 0.03).min(14.0)
                                    };
                                    #[allow(clippy::cast_possible_truncation)]
                                    floor_splats.push((
                                        pos.x() as f32,
                                        pos.y() as f32,
                                        floor_zf,
                                        rgb,
                                        radius,
                                        seed ^ 0xC0_FF_EE,
                                    ));
                                }
                                if dissolve_on {
                                    #[allow(clippy::cast_possible_truncation)]
                                    corpse_pools.push((
                                        glam::Vec3::new(
                                            pos.x() as f32,
                                            pos.y() as f32,
                                            pos.z() as f32,
                                        ),
                                        [40, 40, 40],
                                        mass.max(50),
                                        true,
                                        seed ^ 0xD155_015E_A11A,
                                        Some(floor_zf),
                                    ));
                                }
                            }
                            room_sim::FxEvent::Explosion { pos, radius, .. } => {
                                #[allow(clippy::cast_possible_truncation)]
                                let p =
                                    glam::Vec3::new(pos.x() as f32, pos.y() as f32, pos.z() as f32);
                                #[allow(clippy::cast_possible_truncation)]
                                let rad = radius as f32;
                                if explosion_fx_on {
                                    explosions.push((p, rad, seed));
                                }
                                let eye = self.camera.eye;
                                let dist = (eye - p).length().max(1.0);
                                if dist < rad * 2.5 {
                                    if view_kick_on {
                                        let s = view_kick_scale.clamp(0.0, 4.0);
                                        let falloff = (1.0 - dist / (rad * 2.5)).clamp(0.0, 1.0);
                                        kick_impulse.0 += 0.04 * falloff * s;
                                    }
                                    if heat_haze_on {
                                        self.heat_haze_tics = self.heat_haze_tics.max(12);
                                    }
                                }
                            }
                        }
                    }
                    let (extralight, colormap) = world
                        .players
                        .get(usize::from(self.local_seat))
                        .and_then(|p| p.as_ref())
                        .map(|p| (p.extralight, p.fixed_colormap()))
                        .unwrap_or((0, 0));
                    self.fixed_colormap = colormap;
                    // Doom: extralight<<4; infrared roughly fills to mid-bright.
                    // Accessibility `r_extralight` (0–4) adds 8 light levels per step.
                    let acc_light = self.render_settings.extra_light.clamp(0, 4) * 8;
                    let light_boost = (extralight.clamp(0, 2) * 16
                        + i32::from(colormap == 1) * 64
                        + acc_light) as i16;
                    planes = world
                        .sectors
                        .iter()
                        .map(|s| {
                            let (fz, pfz) = match s.fake_floor_z {
                                Some(z) => (z, z),
                                None => (s.floor_z, s.prev_floor_z),
                            };
                            let (cz, pcz) = match s.fake_ceiling_z {
                                Some(z) => (z, z),
                                None => (s.ceiling_z, s.prev_ceiling_z),
                            };
                            (
                                fz,
                                cz,
                                s.light.saturating_add(light_boost).min(255),
                                pfz,
                                pcz,
                            )
                        })
                        .collect();
                    side_offs = world
                        .side_offsets
                        .iter()
                        .map(|(&i, &(x, y))| (i, x, y))
                        .collect();
                    for (&i, name) in &world.switch_mids {
                        switch_tex.push((i, 0u8, name.clone()));
                    }
                    for (&i, name) in &world.switch_lowers {
                        switch_tex.push((i, 1u8, name.clone()));
                    }
                    for (&i, name) in &world.switch_uppers {
                        switch_tex.push((i, 2u8, name.clone()));
                    }
                    floor_pics = world
                        .sector_floor_pics
                        .iter()
                        .map(|(&i, name)| (i, name.clone()))
                        .collect();
                    ceiling_pics = world
                        .sector_ceiling_pics
                        .iter()
                        .map(|(&i, name)| (i, name.clone()))
                        .collect();
                    floor_scroll = world
                        .floor_scroll_offsets
                        .iter()
                        .map(|(&i, &(u, v))| (i, u, v))
                        .collect();
                    ceil_scroll = world
                        .ceiling_scroll_offsets
                        .iter()
                        .map(|(&i, &(u, v))| (i, u, v))
                        .collect();
                    floor_scales = world
                        .floor_scales
                        .iter()
                        .map(|(&i, &(u, v))| (i, u, v))
                        .collect();
                    ceiling_scales = world
                        .ceiling_scales
                        .iter()
                        .map(|(&i, &(u, v))| (i, u, v))
                        .collect();
                    plane_angles = world
                        .plane_angles
                        .iter()
                        .map(|(&i, &(f, c))| (i, f, c))
                        .collect();
                    side_scales = world
                        .side_scales
                        .iter()
                        .map(|(&i, &(sx, sy))| (i, sx, sy))
                        .collect();
                    plane_alphas = world
                        .sectors
                        .iter()
                        .enumerate()
                        .filter(|(_, s)| s.floor_alpha < 0.999 || s.ceiling_alpha < 0.999)
                        .map(|(i, s)| (i as u32, s.floor_alpha, s.ceiling_alpha))
                        .collect();
                    plane_reflects = world
                        .plane_reflection
                        .iter()
                        .map(|(&i, &(f, c))| (i, f, c))
                        .collect();
                    line_alphas = world.line_alpha.iter().map(|(&i, &a)| (i, a)).collect();
                    let seat = usize::from(self.local_seat);
                    if let Some(p) = world.players.get(seat).and_then(|p| p.as_ref()) {
                        // `ChangeCamera` view actor, else player mobj.
                        let view_id = p.camera.or(p.mo);
                        // `cl_predict`: the confirmed pawn moved by the input
                        // still in flight (view only; the world is untouched).
                        let predicted =
                            world
                                .predict_player_view(seat, &pending_local)
                                .and_then(|pv| {
                                    let mut a = world.actors.get(p.mo?)?.clone();
                                    a.pos = pv.pos;
                                    a.angle = pv.angle;
                                    a.pitch = pv.pitch;
                                    Some((a, pv.view_z))
                                });
                        if let Some(vid) = view_id {
                            let confirmed = world.actors.get(vid);
                            let a = predicted.as_ref().map(|(a, _)| a).or(confirmed);
                            if let Some(a) = a {
                                let z = if let Some((_, vz)) = predicted {
                                    vz
                                } else if p.camera.is_some() {
                                    let eye = if a.camera_height > 0.0 {
                                        a.camera_height
                                    } else if a.view_height > 0.0 {
                                        a.view_height
                                    } else {
                                        41.0
                                    };
                                    a.pos.z() + eye
                                } else {
                                    p.view_z
                                };
                                let base = if p.camera.is_some() && a.camera_fov > 0.0 {
                                    a.camera_fov
                                } else {
                                    58.715
                                };
                                let fov = if p.camera.is_some() {
                                    base
                                } else {
                                    room_sim::Player::zoomed_fov(base, p.weapon_zoom)
                                };
                                // `SetViewPos`: offset, floor / ceiling clamp
                                // unless out-of-bounds, orthographic flag.
                                let ((mut ex, mut ey, ez), ortho) = room_sim::view_eye(world, a, z);
                                #[allow(clippy::cast_possible_truncation)]
                                {
                                    self.camera.ortho = ortho.map(|d| d as f32);
                                }
                                // GZDoom `R_SetupFrame` quake jiggers: classic
                                // quakes push the eye along the view's forward
                                // and side axes by up to ±intensity units.
                                let intensity = world.quake_intensity_at(vid) * 0.5 * quake_scale;
                                if intensity > 0.0 {
                                    let t = u64::from(world.level.time);
                                    let r = |k: u64| {
                                        let mut h = t.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ k;
                                        h ^= h >> 33;
                                        h = h.wrapping_mul(0xFF51_AFD7_ED55_8CCD);
                                        h ^= h >> 33;
                                        (h as f64 / u64::MAX as f64) * 2.0 - 1.0
                                    };
                                    let yaw = a.camera_yaw().radians();
                                    let (fwd, side) = (r(1) * intensity, r(2) * intensity);
                                    ex += yaw.cos() * fwd - yaw.sin() * side;
                                    ey += yaw.sin() * fwd + yaw.cos() * side;
                                }
                                cam = Some((
                                    ex,
                                    ey,
                                    ez,
                                    a.camera_yaw().radians(),
                                    a.camera_pitch().radians(),
                                    Some(fov),
                                ));
                                #[allow(clippy::cast_possible_truncation)]
                                {
                                    self.roll = a.camera_roll().radians() as f32;
                                }
                            }
                        }
                    }
                    hud_snap = world.player_hud(usize::from(self.local_seat));
                }
            }
            if entered_intermission {
                self.start_intermission_music();
            }
            if vote_passed {
                self.play_advance_now();
            }
            if let Some(req) = net_save_req {
                let name = format!("net{}", req.slot);
                if req.load {
                    self.load_play_save_local(Some(&name));
                } else {
                    self.save_play_save_local(Some(&name));
                }
            }
            // Render lerp between previous and current plane Z.
            let frac = (self.tic_accum / tic).clamp(0.0, 1.0) as f64;
            for (i, (fz, cz, light, pfz, pcz)) in planes.into_iter().enumerate() {
                if let Some(sec) = self.map.sectors.get_mut(i) {
                    sec.floor_height = pfz + (fz - pfz) * frac;
                    sec.ceiling_height = pcz + (cz - pcz) * frac;
                    sec.light = light;
                }
            }
            for (i, x, y) in side_offs {
                if let Some(side) = self.map.sides.get_mut(i as usize) {
                    side.x_offset = x;
                    side.y_offset = y;
                }
            }
            for (i, part, name) in switch_tex {
                if let Some(side) = self.map.sides.get_mut(i as usize) {
                    match part {
                        0 => side.middle = name,
                        1 => side.lower = name,
                        _ => side.upper = name,
                    }
                }
            }
            for (i, u, v) in floor_scroll {
                if let Some(sec) = self.map.sectors.get_mut(i as usize) {
                    sec.floor_u_off = u;
                    sec.floor_v_off = v;
                }
            }
            for (i, u, v) in ceil_scroll {
                if let Some(sec) = self.map.sectors.get_mut(i as usize) {
                    sec.ceiling_u_off = u;
                    sec.ceiling_v_off = v;
                }
            }
            for (i, u, v) in floor_scales {
                if let Some(sec) = self.map.sectors.get_mut(i as usize) {
                    sec.floor_scale_u = u;
                    sec.floor_scale_v = v;
                }
            }
            for (i, u, v) in ceiling_scales {
                if let Some(sec) = self.map.sectors.get_mut(i as usize) {
                    sec.ceiling_scale_u = u;
                    sec.ceiling_scale_v = v;
                }
            }
            for (i, floor_a, ceil_a) in plane_angles {
                if let Some(sec) = self.map.sectors.get_mut(i as usize) {
                    sec.floor_angle = floor_a;
                    sec.ceiling_angle = ceil_a;
                }
            }
            for (i, sx, sy) in side_scales {
                if let Some(side) = self.map.sides.get_mut(i as usize) {
                    side.scale_x = sx;
                    side.scale_y = sy;
                }
            }
            for (i, fa, ca) in plane_alphas {
                if let Some(sec) = self.map.sectors.get_mut(i as usize) {
                    sec.floor_alpha = fa;
                    sec.ceiling_alpha = ca;
                }
            }
            for (i, fr, cr) in plane_reflects {
                if let Some(sec) = self.map.sectors.get_mut(i as usize) {
                    sec.floor_reflection = fr;
                    sec.ceiling_reflection = cr;
                }
            }
            for (i, a) in line_alphas {
                if let Some(line) = self.map.lines.get_mut(i as usize) {
                    line.mid_alpha = a;
                }
            }
            for (i, name) in floor_pics {
                if let Some(sec) = self.map.sectors.get_mut(i as usize) {
                    sec.floor_pic = name;
                }
            }
            for (i, name) in ceiling_pics {
                if let Some(sec) = self.map.sectors.get_mut(i as usize) {
                    sec.ceiling_pic = name;
                }
            }
            for msg in msgs {
                if let Some(text) = hud_line(&self.language, &msg) {
                    eprintln!("room: message: {text}");
                    self.messages.push(text, 70);
                }
            }
            // Log new / changed ACS HudMessages once (scripts redraw them every tic).
            if let Some(world) = self.play.as_ref().and_then(|g| g.world()) {
                for m in &world.hud_messages {
                    if m.tics > 0 {
                        continue;
                    }
                    let text =
                        room_ui::strip_color_codes(&localize_tokens(&self.language, &m.text));
                    let text = text.trim().replace('\n', " | ");
                    if text.is_empty() || self.hud_message_log.get(&m.id) == Some(&text) {
                        continue;
                    }
                    eprintln!("room: hudmessage {}: {text}", m.id);
                    self.console.print(format!("hudmessage {}: {text}", m.id));
                    self.hud_message_log.insert(m.id, text);
                }
            }
            for (name, x, y, z, line) in new_decals {
                let salt = {
                    #[allow(clippy::cast_possible_truncation)]
                    {
                        (x.to_bits() as u32)
                            .wrapping_mul(0x9E37_79B1)
                            .wrapping_add(y.to_bits() as u32)
                            .wrapping_add(z.to_bits() as u32)
                    }
                };
                // Group names (`BulletChip`) → concrete `pic` (CHIP1…). Missing CHIP*
                // lumps (not in Freedoom) fall back to an IWAD puff frame so we never
                // bind the previous actor sprite onto the wall.
                let tex = self
                    .decaldef
                    .as_ref()
                    .and_then(|d| d.place_texture(&name, salt))
                    .unwrap_or_else(|| name.clone());
                let tex = if self.vfs.as_ref().is_some_and(|v| {
                    v.find(&tex, Namespace::Global).is_some()
                        || v.find_any_ns(&tex).is_some()
                        || v.find(&tex, Namespace::Patches).is_some()
                }) {
                    tex
                } else if name.eq_ignore_ascii_case("BulletChip")
                    || name.to_ascii_lowercase().contains("chip")
                    || tex.to_ascii_lowercase().starts_with("chip")
                {
                    "PUFFB0".into()
                } else {
                    continue;
                };
                let anim = self
                    .decaldef
                    .as_ref()
                    .map(|d| {
                        let p = d.anim_params(&name);
                        room_render::DecalIngestAnim {
                            fade: p.fade,
                            stretch: p.stretch.map(|s| {
                                (
                                    s.start_tics,
                                    s.time_tics,
                                    s.goal_x.map(|v| v as f32),
                                    s.goal_y.map(|v| v as f32),
                                )
                            }),
                            slide: p
                                .slide
                                .map(|s| (s.start_tics, s.time_tics, s.dist_y as f32)),
                            color: p.color.map(|s| {
                                let [r, g, b] = s.goal_rgb;
                                #[allow(clippy::cast_possible_truncation)]
                                let lum = (0.2126 * r + 0.7152 * g + 0.0722 * b) as f32;
                                (s.start_tics, s.time_tics, lum)
                            }),
                        }
                    })
                    .unwrap_or_default();
                let look = self.decal_look(&name, &tex, salt, x, y);
                let tex = look.texture.clone().unwrap_or(tex);
                self.decals.ingest_one(tex, x, y, z, line, anim);
                if let Some(d) = self.decals.decals.last_mut() {
                    d.width = look.width;
                    d.height = look.height;
                    d.base_width = look.width;
                    d.base_height = look.height;
                    d.light = look.light;
                    d.base_light = look.light;
                    d.alpha = look.alpha;
                }
            }
            self.decals.tick();
            self.particles.ingest_ex(new_particles);
            for (pos, dir, rgb, damage, seed, floor_z) in blood_sprays {
                self.particles
                    .ingest_blood_spray(pos, dir, rgb, damage, seed, floor_z);
            }
            for (x, y, floor_z, rgb, radius, seed) in floor_splats {
                let q = [
                    ((f32::from(rgb[0]) / 255.0 * 31.0).round() as u8).saturating_mul(8),
                    ((f32::from(rgb[1]) / 255.0 * 31.0).round() as u8).saturating_mul(8),
                    ((f32::from(rgb[2]) / 255.0 * 31.0).round() as u8).saturating_mul(8),
                ];
                let name = particle_tint_tex_name(q);
                let lum = (0.2126 * f32::from(rgb[0])
                    + 0.7152 * f32::from(rgb[1])
                    + 0.0722 * f32::from(rgb[2]))
                    / 255.0;
                self.decals.ingest_floor_splat(
                    name,
                    f64::from(x),
                    f64::from(y),
                    f64::from(floor_z),
                    radius,
                    lum.clamp(0.25, 0.85),
                    seed,
                );
            }
            for (x, y, z, line, rgb, seed) in wall_splats {
                let q = [
                    ((f32::from(rgb[0]) / 255.0 * 31.0).round() as u8).saturating_mul(8),
                    ((f32::from(rgb[1]) / 255.0 * 31.0).round() as u8).saturating_mul(8),
                    ((f32::from(rgb[2]) / 255.0 * 31.0).round() as u8).saturating_mul(8),
                ];
                let name = particle_tint_tex_name(q);
                let drip = blood_drips_on && gore_level >= 2;
                self.decals.ingest_one(
                    name,
                    f64::from(x),
                    f64::from(y),
                    f64::from(z),
                    Some(line),
                    room_render::DecalIngestAnim {
                        fade: Some((35 * 12, 35 * 10)),
                        slide: if drip { Some((8, 35 * 8, -36.0)) } else { None },
                        stretch: Some((0, 20, Some(1.35), Some(1.6))),
                        color: None,
                    },
                );
                let _ = seed;
            }
            for (origin, forward, seed) in muzzle_flashes {
                self.particles.ingest_muzzle_flash(origin, forward, seed);
            }
            for (origin, forward, seed) in casings {
                self.particles.ingest_casing(origin, forward, seed);
            }
            for (origin, forward, seed) in weapon_smoke {
                self.particles.ingest_weapon_smoke(origin, forward, seed);
            }
            for (origin, end, seed) in tracers {
                self.particles.ingest_tracer(origin, end, seed);
            }
            for (pos, inward, seed) in impacts {
                self.particles.ingest_impact_sparks(pos, inward, seed);
            }
            for (pos, radius, seed) in explosions {
                self.particles.ingest_explosion(pos, radius, seed);
            }
            for (pos, rgb, mass, gibbed, seed, floor_z) in corpse_pools {
                self.particles
                    .ingest_corpse_pool(pos, rgb, mass, gibbed, seed, floor_z);
            }
            for (pos, vel, seed) in trail_puffs {
                self.particles.ingest_trail_puff(pos, vel, seed);
            }
            self.view_kick.0 = (self.view_kick.0 + kick_impulse.0) * 0.82;
            self.view_kick.1 = (self.view_kick.1 + kick_impulse.1) * 0.82;
            if self.view_kick.0.abs() < 1e-4 {
                self.view_kick.0 = 0.0;
            }
            if self.view_kick.1.abs() < 1e-4 {
                self.view_kick.1 = 0.0;
            }
            self.hit_flash.retain(|_, t| {
                *t = t.saturating_sub(1);
                *t > 0
            });
            self.heat_haze_tics = self.heat_haze_tics.saturating_sub(1);
            let soft_particles = match self.console.cvars.get("r_softparticles") {
                Some(room_console::CvarValue::Bool(b)) => *b,
                Some(room_console::CvarValue::Int(i)) => *i != 0,
                _ => false,
            };
            if soft_particles {
                for p in &mut self.particles.particles {
                    if p.style == room_render::ParticleStyle::Translucent {
                        p.style = room_render::ParticleStyle::Soft;
                    }
                }
            }
            self.particles.tick_with_freeze(particle_level_frozen);
            if let Some((x, y, vz, ang, pit, fov)) = cam {
                #[allow(clippy::cast_possible_truncation)]
                {
                    let next = (
                        x as f32,
                        y as f32,
                        vz as f32,
                        ang as f32,
                        (pit as f32).clamp(-1.5, 1.5),
                    );
                    // Advance the lerp window once per completed sim tic.
                    // (Rollback moves the eye from the speculative copy
                    // instead, after the loop.)
                    if rollback {
                        if let Some(fov) = fov {
                            self.camera.fov_y_deg = fov.clamp(1.0, 175.0);
                        }
                    } else {
                        self.cam_prev = self.cam_curr.or(Some(next));
                        self.cam_curr = Some(next);
                        // Through a portal this tic: no lerp across the map.
                        if self.local_crossed_portal() {
                            self.cam_prev = self.cam_curr;
                        }
                        // Always lock look to the sim after a tic. Mouse look still
                        // previews mid-tic via DeviceEvent, but must not drift away
                        // from `actor.angle` (rounded ticcmd counts vs float yaw) —
                        // otherwise W walks left/right of the crosshair.
                        self.yaw = next.3;
                        self.pitch = next.4;
                        self.note_sprite_poses();
                        if let Some(fov) = fov {
                            self.camera.fov_y_deg = fov.clamp(1.0, 175.0);
                        }
                    }
                }
            }
            // (Rollback draws the HUD from the speculative copy instead.)
            if let Some(h) = hud_snap
                && !rollback
            {
                self.apply_hud_snap(h);
            }
            self.messages.tick();
            self.sync_forward();
        }
        if rollback && rollback_submitted {
            self.rebuild_rollback_world();
        }
        // Smooth eye XYZ between tics so stair steps don't snap every tic.
        // Yaw/pitch are locked to the sim after each tic; mouse still previews mid-tic.
        let frac = (self.tic_accum / tic).clamp(0.0, 1.0);
        if let (Some(prev), Some(curr)) = (self.cam_prev, self.cam_curr) {
            let lerp = |a: f32, b: f32| a + (b - a) * frac;
            self.camera.eye = Vec3::new(
                lerp(prev.0, curr.0),
                lerp(prev.1, curr.1),
                lerp(prev.2, curr.2),
            );
        } else if let Some(curr) = self.cam_curr {
            self.camera.eye = Vec3::new(curr.0, curr.1, curr.2);
        }
    }

    /// EDuke32 runs EVENT_DISPLAYMENU each frame its menu is drawn over a
    /// game (`MENU_MAIN_INGAME`, 50); Room's menu pages are not mapped to
    /// EDuke32's sub-menu ids.
    fn run_duke_menu_event(&mut self) {
        if let Some(session) = self.duke_session.as_mut()
            && session.world.is_some()
        {
            let _ = session.run_eduke_menu_display(50);
        }
    }

    /// Drive the native Build playsim at EDuke32's selected timer rate.
    fn tick_duke(&mut self, dt: f32) {
        if let Some(state) = self.duke_menu.as_mut() {
            state.clock += dt * 120.0;
            self.run_duke_menu_event();
            return;
        }
        if self.menu.is_some() || self.duke_help_page != 0 {
            if self.menu.is_some() {
                self.run_duke_menu_event();
            }
            return;
        }
        self.tic_accum += dt;
        loop {
            let tic_duration = self
                .duke_session
                .as_ref()
                .map_or(1.0 / room_duke::DUKE_TIC_RATE as f32, |session| {
                    1.0 / session.tic_rate() as f32
                });
            if self.tic_accum < tic_duration {
                break;
            }
            self.tic_accum -= tic_duration;
            if self.duke_bonus_panel.is_some() {
                // Chocolate `totalclock` runs at 120 Hz: four clocks per tic.
                let (transition, labels) =
                    self.duke_bonus_panel
                        .as_mut()
                        .map_or((None, Vec::new()), |panel| {
                            panel.screen.tick(4);
                            (
                                panel.screen.done().then_some(panel.transition),
                                panel.screen.take_sounds(),
                            )
                        });
                self.play_duke_bonus_sounds(&labels);
                if let Some(transition) = transition {
                    self.duke_bonus_panel = None;
                    self.advance_duke_campaign(transition, None);
                }
                continue;
            }
            self.duke_music_quote_tics = self.duke_music_quote_tics.saturating_sub(1);
            let sample = self.sample_input();
            let requested_weapon = sample.keys.iter().find_map(|key| match key {
                Key::Weapon1 => room_duke::weapons::weapon_slot_from_hotkey(1),
                Key::Weapon2 => room_duke::weapons::weapon_slot_from_hotkey(2),
                Key::Weapon3 => room_duke::weapons::weapon_slot_from_hotkey(3),
                Key::Weapon4 => room_duke::weapons::weapon_slot_from_hotkey(4),
                Key::Weapon5 => room_duke::weapons::weapon_slot_from_hotkey(5),
                Key::Weapon6 => room_duke::weapons::weapon_slot_from_hotkey(6),
                Key::Weapon7 => room_duke::weapons::weapon_slot_from_hotkey(7),
                Key::Weapon8 => room_duke::weapons::weapon_slot_from_hotkey(8),
                Key::Weapon9 => room_duke::weapons::weapon_slot_from_hotkey(9),
                Key::Weapon0 => room_duke::weapons::weapon_slot_from_hotkey(10),
                _ => None,
            });
            let weapon_cycle = if sample.keys.contains(&Key::NextWeapon) {
                1
            } else if sample.keys.contains(&Key::PrevWeapon) {
                -1
            } else {
                0
            };
            let run = self.keys.shift
                || matches!(
                    self.console.cvars.get("cl_run"),
                    Some(room_console::CvarValue::Bool(true))
                );
            // Chocolate `getinput`: NORMALKEYMOVE (40), doubled when running;
            // strafe is positive to the left. Rotated to world space below.
            let keymove = if run { 80 } else { 40 };
            let vel = (i32::from(self.keys.w) - i32::from(self.keys.s)) * keymove;
            let svel = (i32::from(self.keys.a) - i32::from(self.keys.d)) * keymove;
            // Chocolate `getinput`: PREAMBLETURN (5) for the first
            // TURBOTURNTIME (15 of 120 Hz) ticks, then NORMALTURN (15), doubled
            // when running.
            let turn_dir = i16::from(self.keys.right) - i16::from(self.keys.left);
            if turn_dir == 0 {
                self.duke_turn_held = 0;
            } else {
                self.duke_turn_held = self.duke_turn_held.saturating_add(4);
            }
            let turn_amount = if self.duke_turn_held >= 15 {
                if run { 30 } else { 15 }
            } else {
                5
            };
            let key_turn = turn_dir * turn_amount;
            // Chocolate `processinput`: `ang += avel << 1`. Mouse yaw maps
            // 1:1 to view degrees; the sub-step remainder (and anything past
            // the i8 clamp) carries to the next tic so slow and fast moves
            // both stay exact.
            let yaw_units = -(sample.mouse_yaw as f64) * MOUSE_DEG_PER_COUNT * 2048.0 / 360.0
                + self.duke_turn_residual;
            let turn =
                (f64::from(key_turn) + (yaw_units / 2.0).trunc()).clamp(-127.0, 127.0) as i16;
            self.duke_turn_residual =
                (yaw_units - f64::from(turn - key_turn) * 2.0).clamp(-2048.0, 2048.0);
            let turn = turn as i8;
            // Chocolate mouse aiming adds `horz >> 1` to `horiz`; one horiz
            // unit is ~1/160 rad at the 320x200 reference projection, so a
            // mouse pitch of d degrees maps to horz = 2 * d * 160 * pi/180.
            let pitch_deg = sample.mouse_pitch as f64
                * room_input::MOUSE_DEG_PER_COUNT
                * self.duke_pitch_scale();
            let horiz_units =
                pitch_deg * 160.0 * std::f64::consts::PI / 180.0 + self.duke_horiz_residual;
            // `horiz += horz >> 1`: send even values so both directions move
            // by the same amount, keep the remainder for the next tic.
            let half = horiz_units.trunc().clamp(-63.0, 63.0);
            self.duke_horiz_residual = (horiz_units - half).clamp(-200.0, 200.0);
            let horz = (half as i8) * 2;
            let mut bits = 0_u32;
            // Chocolate's `ud.auto_aim = 2` is full; mode 1 is bullet-only.
            // Keep it as an archived console setting so the mode survives runs.
            let duke_autoaim = match self.console.cvars.get("duke_autoaim") {
                Some(room_console::CvarValue::Int(mode)) => (*mode).clamp(1, 2),
                _ => 2,
            };
            if duke_autoaim == 2 {
                bits |= room_duke::INPUT_AUTO_AIM;
            }
            if sample.keys.contains(&Key::Jump) {
                bits |= 1;
            }
            if self.keys.c {
                bits |= 2;
            }
            if sample.keys.contains(&Key::Use) {
                // Chocolate maps gamefunc_Open to sync bit 29.
                bits |= room_duke::INPUT_OPEN;
            }
            if sample.keys.contains(&Key::InvUse) {
                bits |= room_duke::INPUT_INVENTORY_USE;
            }
            // Inventory bar (touch <IT / IT>, pad d-pad left/right). The
            // session acts on the press edge, so holding steps once.
            if sample.keys.contains(&Key::InvPrev) {
                bits |= room_duke::INPUT_INVENTORY_LEFT;
            }
            if sample.keys.contains(&Key::InvNext) {
                bits |= room_duke::INPUT_INVENTORY_RIGHT;
            }
            // Chocolate gamefunc_TurnAround (sync bit 28), Backspace.
            if self.keys.backspace {
                bits |= room_duke::INPUT_TURNAROUND;
            }
            if run {
                bits |= 1 << 5;
            }
            if sample.keys.contains(&Key::Attack) {
                bits |= 1 << 2;
            }
            // Atomic transports direct weapon selection/cycling in
            // sync.bits. Desktop solo selection is handled by the existing
            // request path below; networked Duke must place it on the local
            // seat's command so every peer simulates the same action.
            if self.net.is_some() {
                let choice = requested_weapon
                    .and_then(|weapon| u32::try_from(weapon).ok())
                    .filter(|weapon| (1..=9).contains(weapon))
                    .map(|weapon| weapon.saturating_add(1))
                    .or(match weapon_cycle {
                        -1 => Some(11),
                        1 => Some(12),
                        _ => None,
                    })
                    .unwrap_or(0);
                bits |= (choice & 0x0f) << room_duke::weapons::INPUT_WEAPON_SHIFT;
            }
            let active_duke_sound_owners = self
                .audio
                .as_ref()
                .and_then(|audio| {
                    audio
                        .shared
                        .lock()
                        .ok()
                        .map(|shared| shared.mixer.duke_active_sound_owners())
                })
                .unwrap_or_default();
            let mut duke_campaign_transition = None;
            let mut duke_campaign_bonus_stats = None;
            let mut duke_restart_requested = false;
            let mut incoming_duke_rts = Vec::new();
            if let Some(session) = self.duke_session.as_mut() {
                session.set_active_sound_owners(&active_duke_sound_owners);
                // `duke_hitdebug 1`: echo what each player bullet hit.
                room_duke::shoot::set_hit_debug(!matches!(
                    self.console.cvars.get("duke_hitdebug"),
                    Some(room_console::CvarValue::Int(0))
                        | Some(room_console::CvarValue::Bool(false))
                ));
                // `duke_statusbar 0` switches to Atomic's full-screen mini HUD.
                if let Some(renderer) = self.duke_renderer.as_ref() {
                    renderer.set_full_status_bar(!matches!(
                        self.console.cvars.get("duke_statusbar"),
                        Some(room_console::CvarValue::Int(0))
                            | Some(room_console::CvarValue::Bool(false))
                    ));
                }
                if self.duke_demo_playback.is_none() && self.duke_atomic_demo_playback.is_none() {
                    // Desktop mouselook is Chocolate's mouse-aiming mode: no
                    // snap-to-centre dead zone around horiz 100.
                    session.player.aim_mode = u8::from(self.mouse_look);
                }
                let duke_input = room_duke::DukeInput::from_local(
                    vel,
                    svel,
                    session.player.ang,
                    turn,
                    horz,
                    bits,
                );
                if let Some(playback) = self.duke_demo_playback.as_mut() {
                    if let Err(error) = playback.playback_next(session) {
                        self.messages.push(format!("playdemo: {error}"), 70);
                        self.duke_demo_playback = None;
                    }
                } else if let Some(playback) = self.duke_atomic_demo_playback.as_mut() {
                    if let Err(error) = playback.playback_next(session) {
                        if matches!(error, room_duke::DukeDemoError::EndOfDemo) {
                            self.messages.push("Atomic Duke demo finished", 70);
                        } else {
                            self.messages.push(format!("playatomicdemo: {error}"), 70);
                        }
                        self.duke_atomic_demo_playback = None;
                    } else {
                        // Atomic demos can continue across campaign maps. The
                        // recording stores inputs, while level progression is
                        // still driven by the native NUKEBUTTON/exit logic.
                        duke_campaign_transition = session.take_campaign_exit_request();
                    }
                } else if let Some(net) = self.net.as_mut() {
                    let checksum = session.checksum().map(|value| (session.tic, value));
                    let frame = net.advance_duke_tic(duke_input, checksum);
                    if let Some(tic) = net.take_desync() {
                        self.messages.push(
                            format!("Network desync at tic {tic}: players' games differ"),
                            175,
                        );
                    }
                    let frame = match frame {
                        Ok(frame) => frame,
                        Err(error) => {
                            eprintln!("room: Duke netplay: {error}");
                            continue;
                        }
                    };
                    incoming_duke_rts.extend(net.drain_duke_rts());
                    session.tick_inputs(&frame);
                    duke_restart_requested = session.take_player_restart_request();
                    if self.duke_demo_recorder.is_none() {
                        duke_campaign_transition = session.take_campaign_exit_request();
                    }
                } else {
                    if let Some(weapon) = requested_weapon {
                        room_duke::weapons::select_weapon(&mut session.player, weapon);
                    }
                    if weapon_cycle > 0 {
                        room_duke::weapons::cycle_weapon(&mut session.player, 1);
                    } else if weapon_cycle < 0 {
                        room_duke::weapons::cycle_weapon(&mut session.player, -1);
                    }
                    session.tick(duke_input);
                    for line in room_duke::shoot::take_hit_log() {
                        eprintln!("room: {line}");
                        self.console.print(line);
                    }
                    duke_restart_requested = session.take_player_restart_request();
                    if self.duke_demo_recorder.is_none() {
                        let bonus_stats = session.bonus_screen();
                        duke_campaign_transition = session.take_campaign_exit_request();
                        if duke_campaign_transition.is_some() {
                            duke_campaign_bonus_stats = Some(bonus_stats);
                        }
                    }
                    if let Some(recorder) = self.duke_demo_recorder.as_mut()
                        && let Err(error) = recorder.record_tic_with_weapon_actions(
                            session,
                            duke_input,
                            requested_weapon,
                            weapon_cycle,
                        )
                    {
                        self.messages.push(format!("record: {error}"), 70);
                        self.duke_demo_recorder = None;
                        self.duke_atomic_demo_recorder = None;
                    }
                    if let Some(recorder) = self.duke_atomic_demo_recorder.as_mut()
                        && let Err(error) = recorder.record_tic_with_weapon_actions(
                            duke_input,
                            requested_weapon,
                            weapon_cycle,
                        )
                    {
                        self.messages.push(format!("record DMO: {error}"), 70);
                        self.duke_demo_recorder = None;
                        self.duke_atomic_demo_recorder = None;
                    }
                }
            }
            for (_, index) in incoming_duke_rts {
                self.play_duke_rts_sound(index);
            }
            let (sound_events, listener) = self
                .duke_session
                .as_mut()
                .map(|session| {
                    // Chocolate `xyzsound` samples the `opos` snapshot made
                    // at processinput entry, while actors emit later in tic.
                    (
                        session.drain_sound_events(),
                        (
                            session.player.oposx,
                            session.player.oposy,
                            session.player.oposz,
                            session.player.view_ang(),
                        ),
                    )
                })
                .unwrap_or_default();
            self.play_duke_sounds(&sound_events, listener);
            if duke_restart_requested {
                if let Some(slot) = self.duke_last_save_slot {
                    self.menu = Some(MenuStack {
                        stack: vec![duke_save_menu(true, slot)],
                        ..MenuStack::default()
                    });
                    self.set_play_look(false);
                } else if let (Some(session), Some(start_state)) =
                    (self.duke_session.as_mut(), self.duke_start_state.as_deref())
                {
                    match session.load_fsave(start_state) {
                        Ok(()) => {
                            self.duke_last_save_slot = None;
                            self.duke_automap_seen.clear();
                            self.duke_show_all_map = false;
                            self.messages.push("Restarted Duke level", 70);
                            self.start_duke_level_music();
                        }
                        Err(error) => self.messages.push(format!("Duke restart: {error}"), 70),
                    }
                } else {
                    self.messages.push(
                        "Duke restart unavailable: initial state could not be saved",
                        70,
                    );
                }
            }
            if let Some(transition) = duke_campaign_transition {
                if matches!(
                    transition,
                    room_duke::DukeCampaignTransition::NextLevel { .. }
                ) && self.net.is_none()
                    && self.duke_demo_playback.is_none()
                    && self.duke_atomic_demo_playback.is_none()
                    && let Some(screen) = duke_campaign_bonus_stats
                {
                    self.duke_bonus_panel = Some(DukeBonusPanel { screen, transition });
                } else {
                    self.advance_duke_campaign(transition, None);
                }
            }
        }
    }

    fn advance_duke_campaign(
        &mut self,
        transition: room_duke::DukeCampaignTransition,
        skill_override: Option<u8>,
    ) {
        match transition {
            room_duke::DukeCampaignTransition::EndGame => {
                self.messages.push("Duke campaign complete", 100);
            }
            room_duke::DukeCampaignTransition::NextLevel { volume, level } => {
                let Some(content) = self.duke_campaign_content.as_ref() else {
                    return;
                };
                let Some(previous) = self.duke_session.take() else {
                    return;
                };
                let campaign = previous.campaign;
                let skill = skill_override.unwrap_or(previous.skill);
                let actor_toggle = previous.actor_toggle;
                let atomic_globals = previous.atomic_globals.clone();
                let mut next = match content.load_level_at_skill(volume, level, skill) {
                    Ok(session) => session,
                    Err(error) => {
                        self.duke_session = Some(previous);
                        self.messages.push(format!("Duke next level: {error}"), 100);
                        return;
                    }
                };
                if skill_override.is_none() {
                    if let Err(error) = next.inherit_campaign_roster(&previous) {
                        self.duke_session = Some(previous);
                        self.messages
                            .push(format!("Duke next level roster: {error}"), 100);
                        return;
                    }
                }
                next.campaign = campaign;
                next.actor_toggle = actor_toggle;
                next.atomic_globals = atomic_globals;
                if let (Some(progress), Some(globals)) = (&next.campaign, &mut next.atomic_globals)
                {
                    globals.volume = Some(i32::from(progress.volume));
                    globals.level = Some(i32::from(progress.level));
                    globals.from_bonus = Some(i32::from(progress.from_bonus));
                }
                if let Some(world) = next.world.as_mut() {
                    room_duke::sync_player_sprite(&next.player, world.sprites.as_mut());
                }
                let map_name = format!("E{}L{}", volume + 1, level + 1);
                if let (Some(renderer), Some(map_bytes)) =
                    (self.duke_renderer.as_ref(), content.map_bytes(&map_name))
                {
                    match renderer.load_map_hack_for_map(&map_name, &map_bytes) {
                        Ok(Some(name)) => self.messages.push(format!("Duke map hack: {name}"), 70),
                        Ok(None) => {}
                        Err(error) => self
                            .messages
                            .push(format!("Duke map hack skipped: {error}"), 70),
                    }
                }
                self.duke_start_state = next.save_fsave().ok();
                self.duke_sound_bank =
                    content.sound_bank(next.script.as_ref(), content.sample_rate);
                if let Some(renderer) = self.duke_renderer.as_ref() {
                    renderer.invalidate_camera_feed();
                }
                self.duke_session = Some(next);
                self.duke_last_save_slot = None;
                self.duke_demo_map = (volume, level);
                self.duke_music_selection = usize::from(volume) * 11 + usize::from(level);
                self.start_duke_level_music();
                self.duke_automap_seen.clear();
                self.duke_show_all_map = false;
                self.cam_prev = None;
                self.cam_curr = None;
                self.messages
                    .push(format!("Duke: entered E{}L{}", volume + 1, level + 1), 100);
            }
        }
    }

    /// Play `dobonus` cues (CON labels) as centred global sounds.
    fn play_duke_bonus_sounds(&mut self, labels: &[&'static str]) {
        let Some(script) = self
            .duke_session
            .as_ref()
            .and_then(|session| session.script.as_ref())
        else {
            return;
        };
        let events: Vec<room_duke::DukeSoundEvent> = labels
            .iter()
            .filter_map(|label| script.labels.get(*label))
            .filter_map(|label| u16::try_from(label.value).ok())
            .map(|sound_id| room_duke::DukeSoundEvent {
                sound_id,
                position: [0; 3],
                source_id: None,
                play_once: false,
                loop_sound: false,
                voice_limit: 0,
                exclusive: false,
                ambience: false,
                priority: 255,
                global: true,
                no_attenuation: true,
                stop: false,
                stop_source_sound: false,
                pitch_offset: None,
            })
            .collect();
        if !events.is_empty() {
            self.play_duke_sounds(&events, (0, 0, 0, 0));
        }
    }

    fn play_duke_sounds(
        &mut self,
        sound_events: &[room_duke::DukeSoundEvent],
        listener: (i32, i32, i32, i16),
    ) {
        let duke_sound_enabled = !matches!(
            self.console.cvars.get("duke_sound"),
            Some(room_console::CvarValue::Bool(false))
        );
        let duke_lockout = matches!(
            self.console.cvars.get("duke_lockout"),
            Some(room_console::CvarValue::Bool(true))
        );
        let duke_talk_enabled = !matches!(
            self.console.cvars.get("duke_talk"),
            Some(room_console::CvarValue::Bool(false))
        );
        let duke_ambience_enabled = !matches!(
            self.console.cvars.get("duke_ambience"),
            Some(room_console::CvarValue::Bool(false))
        );
        let Some(audio) = self.audio.as_ref() else {
            return;
        };
        let visibility_geometry = duke_sound_visibility_geometry(self.duke_session.as_ref());
        let Ok(mut shared) = audio.shared.lock() else {
            return;
        };
        shared.mixer.duke_sfx_gain = duke_audio_volume(&self.console.cvars, "duke_sfx_volume");
        shared
            .mixer
            .set_reverb_level(duke_reverb_zone_level(self.duke_session.as_ref(), listener));
        shared.mixer.reverse_stereo = matches!(
            self.console.cvars.get("duke_reverse_stereo"),
            Some(room_console::CvarValue::Bool(true))
        );
        let to_audio_point = |position: [i32; 3]| {
            room_core::DVec3::new(
                f64::from(position[0]) / 16.0,
                f64::from(position[1]) / 16.0,
                f64::from(position[2]) / 256.0,
            )
        };
        let listener_pos = to_audio_point([listener.0, listener.1, listener.2]);
        let listener_angle =
            f64::from(listener.3.rem_euclid(2048)) * (std::f64::consts::TAU / 2048.0);
        shared.mixer.set_listener(listener_pos, listener_angle);
        for event in sound_events {
            let sound = SoundId::from_raw(u32::from(event.sound_id));
            if event.stop {
                shared.mixer.apply(&[SoundEvent::StopSound { sound }]);
                continue;
            }
            if event.stop_source_sound {
                shared.mixer.apply(&[SoundEvent::Stop {
                    origin: SoundOrigin::Point(to_audio_point(event.position)),
                    channel: i32::from(event.sound_id),
                }]);
                continue;
            }
            if let Some(cents) = event.pitch_offset {
                shared.mixer.apply(&[SoundEvent::AdjustSoundPitch {
                    sound,
                    source_id: event.source_id,
                    cents,
                }]);
                continue;
            }
            if self.duke_session.as_ref().is_some_and(|session| {
                session.player.time_before_exit > 0 && session.player.time_before_exit <= 26 * 3
            }) {
                // Chocolate `xyzsound` rejects new starts during the final
                // three seconds of `timebeforeexit`; stop events above still
                // pass so already-active voices can be cleaned up.
                continue;
            }
            let lockout_only = self
                .duke_session
                .as_ref()
                .and_then(|session| session.script.as_ref())
                .and_then(|script| script.sounds.get(usize::from(event.sound_id)))
                .and_then(Option::as_ref)
                .is_some_and(|definition| definition.m & 8 != 0);
            let talk_only = self
                .duke_session
                .as_ref()
                .and_then(|session| session.script.as_ref())
                .and_then(|script| script.sounds.get(usize::from(event.sound_id)))
                .and_then(Option::as_ref)
                .is_some_and(|definition| definition.m & 4 != 0);
            if !duke_sound_enabled
                || (duke_lockout && lockout_only)
                || (!duke_talk_enabled && talk_only)
                || (!duke_ambience_enabled && event.ambience)
            {
                continue;
            }
            let pitch = self.duke_session.as_ref().map_or(1.0, |session| {
                let underwater = session
                    .world
                    .as_ref()
                    .and_then(|world| world.sectors.get(session.player.cursectnum.max(0) as usize))
                    .is_some_and(|sector| sector.lotag == 2);
                room_duke::audio::pitch_scale(
                    session.script.as_ref(),
                    event.sound_id,
                    session.tic,
                    event.source_id,
                    underwater,
                    event.global,
                )
            });
            let distance_offset = if event.global || event.no_attenuation {
                0
            } else {
                self.duke_session
                    .as_ref()
                    .and_then(|session| session.script.as_ref())
                    .and_then(|script| script.sounds.get(usize::from(event.sound_id)))
                    .and_then(Option::as_ref)
                    .map_or(0, |definition| definition.vo / 16)
            };
            let line_of_sight = visibility_geometry.as_ref().is_none_or(|(sectors, walls)| {
                !duke_sound_is_occluded(self.duke_session.as_ref(), event, listener, sectors, walls)
            });
            let xyzsound_actor_voice = !event.global && !event.ambience && !event.play_once;
            if xyzsound_actor_voice {
                let dead_badguy = self
                    .duke_session
                    .as_ref()
                    .and_then(|session| {
                        let source = usize::try_from(event.source_id?).ok()?;
                        let sprite = session.world.as_ref()?.sprites.get(source)?;
                        Some(
                            sprite.extra <= 0
                                && room_duke::badguy_pic(
                                    sprite.picnum,
                                    session
                                        .script
                                        .as_ref()
                                        .map_or(&[], |script| script.actortype.as_slice()),
                                ),
                        )
                    })
                    .unwrap_or(false);
                shared
                    .mixer
                    .duke_xyzsound_stop_latest(sound, event.source_id, dead_badguy);
            }
            shared.mixer.apply(&[SoundEvent::Start {
                origin: if event.global {
                    SoundOrigin::None
                } else {
                    SoundOrigin::Point(to_audio_point(event.position))
                },
                channel: i32::from(event.sound_id),
                sound,
                check_sound: None,
                start_time: 0.0,
                volume: 1.0,
                attenuation: if line_of_sight { 1.0 } else { 1.03125 },
                pitch,
                flags: SoundFlags(
                    (if event.play_once {
                        SoundFlags::UNIQUE_SOUND
                    } else {
                        0
                    }) | (if event.loop_sound {
                        SoundFlags::LOOP
                    } else {
                        0
                    }) | match event.voice_limit {
                        1 => SoundFlags::DUKE_LIMIT_ONE,
                        2..=4 => SoundFlags::DUKE_LIMIT_FOUR,
                        _ => 0,
                    } | if event.exclusive {
                        SoundFlags::DUKE_EXCLUSIVE
                    } else {
                        0
                    } | if event.no_attenuation || event.global {
                        SoundFlags::ATTN_NONE
                    } else {
                        0
                    } | if event.ambience {
                        SoundFlags::DUKE_AMBIENCE
                    } else {
                        0
                    } | if xyzsound_actor_voice {
                        SoundFlags::OVERLAP
                    } else {
                        0
                    } | SoundFlags::with_duke_priority(i32::from(event.priority)).0
                        | SoundFlags::with_duke_distance_offset(distance_offset).0,
                ),
                source_id: event.source_id,
                no_restart: false,
            }]);
        }
    }

    /// GLDEFS object-frame lights for actors currently in a lit attack pose.
    fn collect_dyn_lights(&self) -> Vec<DynLightInstance> {
        let mut out = Vec::new();
        let Some(world) = self.play.as_ref().and_then(|g| g.world()) else {
            return out;
        };
        self.collect_actor_lights(world, &mut out);
        if self.gldefs.object_lights.is_empty() {
            return out;
        }
        for &id in &world.thinkers.default {
            let Some(a) = world.actors.get(id) else {
                continue;
            };
            if a.destroyed {
                continue;
            }
            let Some(class) = world.classes.get(a.class) else {
                continue;
            };
            for assoc in &self.gldefs.object_lights {
                if !assoc.class.eq_ignore_ascii_case(&class.name) {
                    continue;
                }
                if !assoc.matches(&a.sprite, a.frame) {
                    continue;
                }
                let Some(def) = self.gldefs.lights.get(&assoc.light.to_ascii_lowercase()) else {
                    continue;
                };
                let t = f64::from(world.level.maptime) / 35.0;
                let radius =
                    animated_light_radius(def.kind, def.size, def.secondary_size, def.interval, t);
                #[allow(clippy::cast_possible_truncation)]
                let (spot_dir, spot_outer_cos, spot_inner_cos) =
                    if def.kind == room_defs::DynLightKind::Spot && def.spot_angle > 0.0 {
                        let outer = def.spot_angle.to_radians().max(0.01);
                        let inner = if def.spot_inner > 0.0 {
                            def.spot_inner.to_radians().clamp(0.0, outer)
                        } else {
                            outer * 0.5
                        };
                        // Default aim: actor facing in XY, slightly down.
                        let yaw = a.angle.radians() as f32;
                        let dir = glam::Vec3::new(yaw.cos(), yaw.sin(), -0.15).normalize_or_zero();
                        (Some(dir), outer.cos(), inner.cos())
                    } else {
                        (None, 1.0, 1.0)
                    };
                out.push(DynLightInstance {
                    pos: glam::Vec3::new(
                        a.pos.x() as f32 + def.offset[0],
                        a.pos.y() as f32 + def.offset[1],
                        a.pos.z() as f32 + def.offset[2],
                    ),
                    color: def.color,
                    radius: radius.max(1.0),
                    subtractive: def.subtractive,
                    spot_dir,
                    spot_outer_cos,
                    spot_inner_cos,
                });
            }
        }
        out
    }

    /// Lights from `A_AttachLight` / `A_AttachLightDef`, and map-placed
    /// `DynamicLight` actors (`PointLight`, `SpotLight`, …: colour in args 0–2,
    /// radius in arg 3; spot cones from `DynamicLight.SpotInnerAngle` /
    /// `SpotOuterAngle`, default 10° / 25°).
    fn collect_actor_lights(&self, world: &room_sim::World, out: &mut Vec<DynLightInstance>) {
        let t = f64::from(world.level.maptime) / 35.0;
        for &id in &world.thinkers.default {
            let Some(a) = world.actors.get(id) else {
                continue;
            };
            if a.destroyed {
                continue;
            }
            #[allow(clippy::cast_possible_truncation)]
            let base = glam::Vec3::new(a.pos.x() as f32, a.pos.y() as f32, a.pos.z() as f32);
            #[allow(clippy::cast_possible_truncation)]
            let yaw = a.angle.radians() as f32;
            #[allow(clippy::cast_possible_truncation)]
            let pitch = a.pitch.radians() as f32;
            let facing = glam::Vec3::new(
                yaw.cos() * pitch.cos(),
                yaw.sin() * pitch.cos(),
                pitch.sin(),
            )
            .normalize_or_zero();
            for l in &a.ext.lights {
                if !l.def.is_empty() {
                    let Some(def) = self.gldefs.lights.get(&l.def.to_ascii_lowercase()) else {
                        continue;
                    };
                    let radius = animated_light_radius(
                        def.kind,
                        def.size,
                        def.secondary_size,
                        def.interval,
                        t,
                    );
                    out.push(DynLightInstance {
                        pos: base + glam::Vec3::from_array(def.offset),
                        color: def.color,
                        radius: radius.max(1.0),
                        subtractive: def.subtractive,
                        spot_dir: None,
                        spot_outer_cos: 1.0,
                        spot_inner_cos: 1.0,
                    });
                    continue;
                }
                let kind = match l.kind {
                    1 => DynLightKind::Pulse,
                    2 => DynLightKind::Flicker,
                    4 => DynLightKind::FlickerRandom,
                    3 => DynLightKind::Sector,
                    _ => DynLightKind::Point,
                };
                #[allow(clippy::cast_possible_truncation)]
                let radius = animated_light_radius(
                    kind,
                    l.radius.0 as f32,
                    l.radius.1 as f32,
                    l.param as f32,
                    t,
                );
                // Offset: forward, left, up in the actor's frame.
                #[allow(clippy::cast_possible_truncation)]
                let (fx, fy, fz) = (
                    l.offset.x() as f32,
                    l.offset.y() as f32,
                    l.offset.z() as f32,
                );
                let off = glam::Vec3::new(
                    fx * yaw.cos() - fy * yaw.sin(),
                    fx * yaw.sin() + fy * yaw.cos(),
                    fz,
                );
                let rgb = [
                    ((l.color >> 16) & 0xff) as f32 / 255.0,
                    ((l.color >> 8) & 0xff) as f32 / 255.0,
                    (l.color & 0xff) as f32 / 255.0,
                ];
                #[allow(clippy::cast_possible_truncation)]
                let k = l.intensity.max(0.0) as f32;
                let spot = l.flags & 64 != 0 && l.spot.1 > 0.0;
                #[allow(clippy::cast_possible_truncation)]
                let (inner, outer) = (l.spot.0.to_radians() as f32, l.spot.1.to_radians() as f32);
                out.push(DynLightInstance {
                    pos: base + off,
                    color: [rgb[0] * k, rgb[1] * k, rgb[2] * k],
                    radius: radius.max(1.0),
                    subtractive: l.flags & 1 != 0,
                    spot_dir: spot.then_some(facing),
                    spot_outer_cos: if spot { outer.cos() } else { 1.0 },
                    spot_inner_cos: if spot { inner.min(outer).cos() } else { 1.0 },
                });
            }
            // Map-placed `DynamicLight` things.
            let Some(class) = world.classes.get(a.class) else {
                continue;
            };
            if !world.classes.inherits_from(a.class, "DynamicLight") {
                continue;
            }
            if a.flags & room_sim::FLAG_DORMANT != 0 {
                continue;
            }
            let rgb = [
                (a.args[0].clamp(0, 255)) as f32 / 255.0,
                (a.args[1].clamp(0, 255)) as f32 / 255.0,
                (a.args[2].clamp(0, 255)) as f32 / 255.0,
            ];
            #[allow(clippy::cast_possible_truncation)]
            let radius = a.args[3].max(1) as f32;
            let spot = world.classes.inherits_from(a.class, "SpotLight")
                || class.name.to_ascii_lowercase().contains("spot");
            #[allow(clippy::cast_possible_truncation)]
            let inner = class
                .ext
                .stored_f64("dynamiclight.spotinnerangle")
                .unwrap_or(10.0) as f32;
            #[allow(clippy::cast_possible_truncation)]
            let outer = class
                .ext
                .stored_f64("dynamiclight.spotouterangle")
                .unwrap_or(25.0) as f32;
            out.push(DynLightInstance {
                pos: base,
                color: rgb,
                radius,
                subtractive: class.name.to_ascii_lowercase().contains("subtractive"),
                spot_dir: spot.then_some(facing),
                spot_outer_cos: if spot { outer.to_radians().cos() } else { 1.0 },
                spot_inner_cos: if spot {
                    inner.min(outer).to_radians().cos()
                } else {
                    1.0
                },
            });
        }
    }

    fn fill_voxel_poses(&mut self) {
        self.voxel_poses.clear();
        self.voxel_pose_actors.clear();
        let Some(world) = self.play.as_ref().and_then(|g| g.world()) else {
            return;
        };
        if self.voxeldefs.by_name.is_empty() {
            return;
        }
        let Some(vfs) = self.vfs.as_ref() else {
            return;
        };
        let local_mo = world
            .players
            .get(usize::from(self.local_seat))
            .and_then(|p| p.as_ref())
            .and_then(|p| p.mo);
        let tic = world.level.maptime as f32;
        let eye = self.camera.eye;
        let actors: Vec<_> = world.thinkers.default.to_vec();
        let mut cands = Vec::new();
        for id in actors {
            let Some(a) = world.actors.get(id) else {
                continue;
            };
            let class_flags = world.classes.get(a.class).map(|c| c.flags).unwrap_or(0);
            let flags = a.flags | class_flags;
            if a.destroyed || Some(id) == local_mo || (flags & FLAG_DONTDRAW) != 0 {
                continue;
            }
            let Some(def) = self.voxeldefs.for_sprite(&a.sprite, a.frame) else {
                continue;
            };
            let Some(cost) = self
                .voxels
                .wall_vert_cost(vfs, &def.file, def.override_palette)
            else {
                continue;
            };
            let dropped = (flags & FLAG_DROPPED) != 0;
            let spin_n = if dropped {
                def.dropped_spin
            } else {
                def.placed_spin
            };
            #[allow(clippy::cast_possible_truncation)]
            let yaw = a.angle.degrees() as f32;
            #[allow(clippy::cast_possible_truncation)]
            let (pitch, roll) = voxel_view_angles(
                yaw,
                a.pitch.degrees() as f32,
                a.roll.degrees() as f32,
                [a.vel.x() as f32, a.vel.y() as f32, a.vel.z() as f32],
                def.pitch_from_momentum,
                def.use_actor_pitch,
                def.use_actor_roll,
            );
            let light = (actor_light_level(world, a) as f32 / 255.0).clamp(0.2, 1.0);
            #[allow(clippy::cast_possible_truncation)]
            let origin = Vec3::new(a.pos.x() as f32, a.pos.y() as f32, a.pos.z() as f32);
            let dist2 = origin.distance_squared(eye);
            cands.push((
                dist2,
                id,
                cost,
                VoxelActorPose {
                    file: def.file.clone(),
                    override_palette: def.override_palette,
                    origin,
                    yaw_deg: yaw,
                    pitch_deg: pitch,
                    roll_deg: roll,
                    spin_deg: voxel_spin_degrees(spin_n, tic),
                    actor_scale: [a.scale_x as f32, a.scale_y as f32],
                    voxel_scale: def.scale,
                    angle_offset: def.angle_offset,
                    offset: def.offset,
                    light,
                },
            ));
        }
        cands.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
        let mut used = 0usize;
        for (_, id, cost, pose) in cands {
            if used.saturating_add(cost) > MAX_VOXEL_WALL_VERTS_PER_FRAME {
                continue;
            }
            used += cost;
            self.voxel_pose_actors.insert((id.index(), id.generation()));
            self.voxel_poses.push(pose);
        }
    }

    fn actor_draws_voxel(&self, id: ActorId) -> bool {
        self.voxel_pose_actors
            .contains(&(id.index(), id.generation()))
    }

    /// Actors whose current sprite frame a MODELDEF block binds: they are
    /// drawn as models instead of sprites (UZDoom `FindModelFrame`).
    fn fill_model_actors(&mut self) {
        self.model_pose_actors.clear();
        if self.modeldefs.blocks.is_empty() {
            return;
        }
        let Some(world) = self.play.as_ref().and_then(|g| g.world()) else {
            return;
        };
        for &id in &world.thinkers.default {
            let Some(a) = world.actors.get(id) else {
                continue;
            };
            let Some(class) = world.classes.get(a.class) else {
                continue;
            };
            let model_class = if a.ext.model.modeldef.is_empty() {
                class.name.as_str()
            } else {
                a.ext.model.modeldef.as_str()
            };
            let frame = char::from(b'A' + a.frame.min(25));
            if room_render::actor_has_model(&self.modeldefs, model_class, &a.sprite, frame) {
                self.model_pose_actors.insert((id.index(), id.generation()));
            }
        }
    }

    fn actor_draws_model(&self, id: ActorId) -> bool {
        self.model_pose_actors
            .contains(&(id.index(), id.generation()))
    }

    /// Actors drawn as MODELDEF models this frame.
    fn collect_model_poses(&self) -> Vec<ModelActorPose> {
        let Some(world) = self.play.as_ref().and_then(|g| g.world()) else {
            return Vec::new();
        };
        if self.modeldefs.by_actor.is_empty() {
            return Vec::new();
        }
        let local_mo = world
            .players
            .get(usize::from(self.local_seat))
            .and_then(|p| p.as_ref())
            .and_then(|p| p.mo);
        let mut out = Vec::new();
        for &id in &world.thinkers.default {
            let Some(a) = world.actors.get(id) else {
                continue;
            };
            let class_flags = world.classes.get(a.class).map(|c| c.flags).unwrap_or(0);
            if a.destroyed || Some(id) == local_mo || ((a.flags | class_flags) & FLAG_DONTDRAW) != 0
            {
                continue;
            }
            let Some(class) = world.classes.get(a.class) else {
                continue;
            };
            // `A_ChangeModel(modeldef: …)` swaps the MODELDEF entry.
            let model_class = if a.ext.model.modeldef.is_empty() {
                class.name.clone()
            } else {
                a.ext.model.modeldef.clone()
            };
            if !self.actor_draws_model(id) {
                continue;
            }
            let light = (actor_light_level(world, a) as f32 / 255.0).clamp(0.2, 1.0);
            let (sprite, frame) = (a.sprite.clone(), char::from(b'A' + a.frame.min(25)));
            let frac = f64::from((self.tic_accum * 35.0).clamp(0.0, 1.0));
            let block_flags = self
                .modeldefs
                .frame(&model_class, &sprite, frame)
                .map_or(0, |(d, _)| d.mdl_flags());
            let flags = a.ext.model.flags.unwrap_or(block_flags);
            let next = room_sim::model_frame_lerp(
                world,
                a,
                frac,
                flags & room_defs::mdl::INTERPOLATEDOUBLEDFRAMES != 0,
            )
            .map(|n| (n.sprite, char::from(b'A' + n.frame.min(25)), n.inter));
            let prev = self.sprite_prev.get(&id).copied();
            let pose = sprite_pose(
                a.pos.x(),
                a.pos.y(),
                a.pos.z(),
                a.angle.degrees(),
                prev.map_or(a.pos.x(), |p| f64::from(p.0)),
                prev.map_or(a.pos.y(), |p| f64::from(p.1)),
                prev.map_or(a.pos.z(), |p| f64::from(p.2)),
                prev.map_or(a.angle.degrees(), |p| f64::from(p.3)),
                frac,
                a.extra_flags.get_name("DONTINTERPOLATE"),
                a.extra_flags.get_name("INTERPOLATEANGLES"),
            );
            let m = &a.ext.model;
            let req = |anim: &(String, f64, i32, i32, i32, i32, i32), start: u32| {
                room_render::AnimRequest {
                    name: anim.0.clone(),
                    framerate: anim.1,
                    start_frame: anim.2,
                    loop_frame: anim.3,
                    end_frame: anim.4,
                    interpolate_tics: anim.5,
                    flags: anim.6,
                    start_tic: f64::from(start),
                }
            };
            #[allow(clippy::cast_possible_truncation)]
            out.push(ModelActorPose {
                class: model_class,
                origin: Vec3::new(pose.x as f32, pose.y as f32, pose.z as f32),
                light,
                angle_deg: (pose.angle_deg + a.ext.sprite_rotation) as f32,
                pitch_deg: a.pitch.degrees() as f32,
                roll_deg: a.roll.degrees() as f32,
                vel: Vec3::new(a.vel.x() as f32, a.vel.y() as f32, a.vel.z() as f32),
                scale: [a.scale_x as f32, a.scale_y as f32],
                next,
                flags: a.ext.model.flags,
                clock_tic: self.start_instant.elapsed().as_secs_f64() * 35.0,
                sprite,
                frame,
                tic: f64::from(world.level.maptime)
                    + f64::from((self.tic_accum * 35.0).clamp(0.0, 1.0)),
                animation: m
                    .animation
                    .as_ref()
                    .map(|anim| req(anim, m.animation_start)),
                prev_animation: m
                    .prev_animation
                    .as_ref()
                    .map(|(anim, start, switch)| (req(anim, *start), f64::from(*switch))),
                bones: m
                    .bones
                    .iter()
                    .map(|b| (b.index, b.name.clone(), bone_override(b)))
                    .collect(),
                animation_file: m.animation_file.as_ref().map(|(_, path, file)| {
                    if path.is_empty() {
                        file.clone()
                    } else {
                        room_render::join_model_path(path, file)
                    }
                }),
            });
        }
        out
    }

    /// Attribute a health drop: prefer `World::last_damage`, then dump sight/FOV/GPU/sector.
    fn format_hit_debug(&mut self, seat: usize, hp_drop: i32) -> (String, Vec<String>) {
        let mut detail = Vec::new();
        let Some(world) = self.play.as_ref().and_then(|g| g.world()) else {
            return ("unknown".into(), detail);
        };
        let mo = world
            .players
            .get(seat)
            .and_then(|p| p.as_ref())
            .and_then(|p| p.mo);
        let last = world.last_damage.clone();
        // Consume so the next HIT is not attributed to a stale hit.
        if let Some(g) = self.play.as_mut() {
            if let Some(w) = g.world_mut() {
                w.last_damage = None;
            }
        }
        let Some(world) = self.play.as_ref().and_then(|g| g.world()) else {
            return ("unknown".into(), detail);
        };
        let Some(mo) = mo else {
            return ("unknown".into(), detail);
        };
        let Some(player) = world.actors.get(mo) else {
            return ("unknown".into(), detail);
        };
        let px = player.pos.x();
        let py = player.pos.y();
        let pz = player.pos.z();
        let eye = self.camera.eye;
        let yaw = f64::from(self.yaw);
        let half_fov = (self.camera.fov_y_deg * self.camera.aspect * 0.5).to_radians();

        let sid = room_nodebuild::point_in_sector(&world.map, px, py)
            .or_else(|| room_nodebuild::point_in_sector_vanilla(&world.map, px, py));
        let sid_u = sid.map(|s| s.raw() as usize);
        let map_special = sid_u
            .and_then(|i| world.map.sectors.get(i))
            .map(|s| s.special)
            .unwrap_or(0);
        let runtime_special = sid_u
            .and_then(|i| world.sector_specials.get(&(i as u32)).copied())
            .unwrap_or(map_special);
        let (floor_z, ceil_z, dmg_amt, dmg_iv, leak) = sid_u
            .and_then(|i| world.sectors.get(i))
            .map(|s| {
                (
                    s.floor_z,
                    s.ceiling_z,
                    s.damage_amount,
                    s.damage_interval,
                    s.leakiness,
                )
            })
            .unwrap_or((0.0, 0.0, 0, 0, 0));
        let gap = ceil_z - floor_z;
        let crushers: Vec<_> = world
            .ceilings
            .iter()
            .filter(|c| sid.is_some_and(|s| c.sector == s) && c.crush)
            .map(|c| {
                format!(
                    "ceil crush dir={} speed={:.1} once={}",
                    c.direction, c.speed, c.once
                )
            })
            .collect();
        detail.push(format!(
            "player @({:.0},{:.0},{:.0}) velz={:.1} eye=({:.0},{:.0},{:.0}) yaw={:.0}° fov≈{:.0}°",
            px,
            py,
            pz,
            player.vel.z(),
            eye.x,
            eye.y,
            eye.z,
            yaw.to_degrees(),
            half_fov.to_degrees() * 2.0
        ));
        detail.push(format!(
            "sector {} map_special={map_special} runtime_special={runtime_special} dmgAmt={dmg_amt} dmgIv={dmg_iv} leak={leak} floor={floor_z:.0} ceil={ceil_z:.0} gap={gap:.0} player_h={:.0}",
            sid.map(|s| s.raw()).unwrap_or(u32::MAX),
            player.height
        ));
        if !crushers.is_empty() {
            detail.push(format!("crushers: {}", crushers.join("; ")));
        } else {
            detail.push("crushers: none".into());
        }

        let mut who = String::from("unknown");
        if let Some(ref hit) = last {
            let mismatch = if hit.dealt != hp_drop {
                format!(" (hud_drop={hp_drop}≠dealt={})", hit.dealt)
            } else {
                String::new()
            };
            who = if !hit.cause.is_empty() && hit.source_class.is_empty() {
                hit.cause.clone()
            } else if !hit.source_class.is_empty() {
                let infl =
                    if !hit.inflictor_class.is_empty() && hit.inflictor_class != hit.source_class {
                        format!(" via {}", hit.inflictor_class)
                    } else if hit.inflictor.is_some() && hit.inflictor != hit.source {
                        format!(" via {}", fmt_actor_id(hit.inflictor))
                    } else {
                        String::new()
                    };
                format!("{}{infl}", hit.source_class)
            } else if !hit.inflictor_class.is_empty() {
                hit.inflictor_class.clone()
            } else {
                format!(
                    "sector {} special={runtime_special}",
                    sid.map(|s| s.raw()).unwrap_or(u32::MAX)
                )
            };
            detail.push(format!(
                "last_damage tic={} raw={} dealt={} cause={} src={}/{} infl={}/{}{mismatch}",
                hit.tic,
                hit.raw,
                hit.dealt,
                if hit.cause.is_empty() {
                    "-"
                } else {
                    hit.cause.as_str()
                },
                fmt_actor_id(hit.source),
                hit.source_class,
                fmt_actor_id(hit.inflictor),
                hit.inflictor_class
            ));
            for &id in &[hit.source, hit.inflictor] {
                let Some(id) = id else { continue };
                if let Some(line) = self.describe_hit_actor(world, mo, id, px, py, yaw, half_fov) {
                    detail.push(line);
                }
            }
        } else {
            detail.push(format!(
                "last_damage: none (hud_drop={hp_drop}; guessing from target links)"
            ));
            if let Some(sid) = sid {
                who = format!("sector {} special={runtime_special}", sid.raw());
            }
        }

        // Recent shots (from → to) so invisible killers show the actual traces.
        let tic_now = world.level.time;
        let shots: Vec<_> = world
            .recent_shots
            .iter()
            .rev()
            .filter(|s| tic_now.saturating_sub(s.tic) <= 64)
            .take(16)
            .collect();
        if shots.is_empty() {
            detail.push("shots (last 64 tics): none".into());
        } else {
            detail.push(format!("shots (newest first, {} in window):", shots.len()));
            for s in shots {
                let kind = match s.kind {
                    ShotKind::Hitscan => "hitscan",
                    ShotKind::Missile => "missile",
                };
                let hit = if s.hit_class.is_empty() {
                    if s.hit.is_some() {
                        fmt_actor_id(s.hit)
                    } else {
                        "wall/miss".into()
                    }
                } else {
                    format!("{} {}", s.hit_class, fmt_actor_id(s.hit))
                };
                let dmg = if s.damage > 0 {
                    format!(" dmg={}", s.damage)
                } else {
                    String::new()
                };
                let missile = if s.missile_class.is_empty() {
                    String::new()
                } else {
                    format!(" {}", s.missile_class)
                };
                detail.push(format!(
                    "  @{tic} {kind}{missile} {}#{} ({:.0},{:.0},{:.0}) → ({:.0},{:.0},{:.0}) hit={hit}{dmg}",
                    s.source_class,
                    s.source.index(),
                    s.from[0],
                    s.from[1],
                    s.from[2],
                    s.to[0],
                    s.to[1],
                    s.to[2],
                    tic = s.tic,
                ));
            }
        }

        // Living attackers / missiles that still claim this player.
        let mut hunters = 0usize;
        let mut corpses = 0usize;
        for &id in &world.thinkers.default {
            let Some(a) = world.actors.get(id) else {
                continue;
            };
            if a.destroyed || a.target != Some(mo) {
                continue;
            }
            let class_flags = world.classes.get(a.class).map(|c| c.flags).unwrap_or(0);
            let flags = a.flags | class_flags;
            let corpse = (flags & FLAG_CORPSE) != 0 || a.health <= 0;
            if corpse {
                corpses += 1;
                if corpses <= 3 {
                    if let Some(line) =
                        self.describe_hit_actor(world, mo, id, px, py, yaw, half_fov)
                    {
                        detail.push(format!("corpse_target {line}"));
                    }
                }
                continue;
            }
            hunters += 1;
            if hunters <= 8 {
                if let Some(line) = self.describe_hit_actor(world, mo, id, px, py, yaw, half_fov) {
                    detail.push(format!("hunter {line}"));
                }
                if who == "unknown" || who.starts_with("sector ") {
                    if let Some(c) = world.classes.get(a.class) {
                        who = format!(
                            "{} sprite={}{}",
                            c.name,
                            a.sprite,
                            char::from(b'A' + a.frame.min(25))
                        );
                    }
                }
            }
        }
        detail.push(format!(
            "target_links: {hunters} alive, {corpses} corpse/dead (first few listed)"
        ));

        // Nearby missiles regardless of target (balls can die the same tic as impact).
        let mut missiles = 0usize;
        for &id in &world.thinkers.default {
            let Some(a) = world.actors.get(id) else {
                continue;
            };
            let class_flags = world.classes.get(a.class).map(|c| c.flags).unwrap_or(0);
            if a.destroyed || (a.flags | class_flags) & FLAG_MISSILE == 0 {
                continue;
            }
            let dx = a.pos.x() - px;
            let dy = a.pos.y() - py;
            let dist = (dx * dx + dy * dy).sqrt();
            if dist > 512.0 {
                continue;
            }
            missiles += 1;
            if missiles <= 6 {
                if let Some(line) = self.describe_hit_actor(world, mo, id, px, py, yaw, half_fov) {
                    detail.push(format!("missile {line}"));
                }
            }
        }
        if missiles == 0 {
            detail.push("missiles within 512mu: none".into());
        }

        (who, detail)
    }

    fn describe_hit_actor(
        &self,
        world: &room_sim::World,
        player_mo: ActorId,
        id: ActorId,
        px: f64,
        py: f64,
        yaw: f64,
        half_fov: f64,
    ) -> Option<String> {
        let a = world.actors.get(id)?;
        let class = world.classes.get(a.class)?;
        let class_flags = class.flags;
        let flags = a.flags | class_flags;
        let dx = a.pos.x() - px;
        let dy = a.pos.y() - py;
        let dist = (dx * dx + dy * dy).sqrt();
        let ang = dy.atan2(dx);
        let mut d_ang = ang - yaw;
        while d_ang > std::f64::consts::PI {
            d_ang -= std::f64::consts::TAU;
        }
        while d_ang < -std::f64::consts::PI {
            d_ang += std::f64::consts::TAU;
        }
        let in_fov = d_ang.abs() <= half_fov;
        let sight = check_sight(world, player_mo, id);
        let dontdraw = (flags & FLAG_DONTDRAW) != 0;
        let frame_ch = char::from(b'A' + a.frame.min(25));
        let rot = sprite_rotation_digit(
            a.pos.x(),
            a.pos.y(),
            a.angle.degrees(),
            f64::from(self.camera.eye.x),
            f64::from(self.camera.eye.y),
        );
        let candidates = sprite_lump_candidates_rot(&a.sprite, a.frame, rot);
        let mut lump = String::from("-");
        let mut patch_ok = false;
        let mut gpu_ok = false;
        if let Some((name, _)) = candidates.first() {
            lump = name.clone();
            patch_ok = self.patches.get(name).is_some()
                || self.vfs.as_ref().is_some_and(|v| {
                    v.find(name, Namespace::Sprites).is_some()
                        || v.find(name, Namespace::Global).is_some()
                        || v.find_any_ns(name).is_some()
                });
            gpu_ok = self.materials.get(name).is_some();
        }
        Some(format!(
            "{} id={} hp={} sprite={}{}{} @({:.0},{:.0},{:.0}) dist={:.0} dAng={:.0}° fov={} sight={} DONTDRAW={} MISSILE={} CORPSE={} lump={lump} patch={} gpu={}",
            class.name,
            fmt_actor_id(Some(id)),
            a.health,
            a.sprite,
            frame_ch,
            if a.destroyed { " DESTROYED" } else { "" },
            a.pos.x(),
            a.pos.y(),
            a.pos.z(),
            dist,
            d_ang.to_degrees(),
            if in_fov { "Y" } else { "N" },
            if sight { "Y" } else { "N" },
            if dontdraw { "Y" } else { "N" },
            if (flags & FLAG_MISSILE) != 0 {
                "Y"
            } else {
                "N"
            },
            if (flags & FLAG_CORPSE) != 0 || a.health <= 0 {
                "Y"
            } else {
                "N"
            },
            if patch_ok { "Y" } else { "N" },
            if gpu_ok { "Y" } else { "N" },
        ))
    }

    /// Shift sprite samples once per completed tic. `+DONTINTERPOLATE` is applied
    /// at draw time so a flag change does not need another snapshot.
    /// Copy one tic's status-bar / weapon-sprite numbers into the HUD.
    fn apply_hud_snap(&mut self, h: room_sim::PlayerHud) {
        if h.health < self.last_player_hp
            && (cvar_bool(&self.console.cvars, "developer") || env_trace())
        {
            let seat = usize::from(self.local_seat);
            let drop = self.last_player_hp - h.health;
            let (who, detail) = self.format_hit_debug(seat, drop);
            let msg = format!("HIT {}→{} by {who}", self.last_player_hp, h.health);
            eprintln!("room: {msg}");
            for line in detail {
                eprintln!("room:   {line}");
            }
        }
        self.last_player_hp = h.health;
        self.hud.health = h.health;
        self.hud.armor = h.armor;
        self.hud.ammo = h.ammo;
        self.hud.ammo_max = h.ammo_max;
        self.hud.ammo2 = h.ammo2;
        self.hud.ammo2_max = h.ammo2_max;
        self.hud.keys = h.keys;
        self.hud.face = h.face;
        self.hud.weapon_patch = h.weapon_patch;
        self.hud.layers = h
            .layers
            .iter()
            .map(|l| room_ui::HudLayer {
                id: l.id,
                patch: l.patch.clone(),
                ofs_x: l.ofs_x,
                ofs_y: l.ofs_y,
                alpha: l.alpha,
                scale_x: l.scale_x,
                scale_y: l.scale_y,
                flip: l.flip,
                mirror: l.mirror,
                angle: l.angle,
                pivot_x: l.pivot_x,
                pivot_y: l.pivot_y,
                pivot_percent: l.pivot_percent,
                add_bob: l.add_bob,
                add_weapon: l.add_weapon,
                interpolate: l.interpolate,
                fuzz: l.fuzz,
                bright: l.bright,
                tint: l.tint,
                vertex: l.vertex,
            })
            .collect();
        self.hud.flash_patch = h.flash_patch;
        self.hud.overlay_patch = h.overlay_patch;
        self.hud.overlay_ofs_x = h.overlay_ofs_x;
        self.hud.overlay_ofs_y = h.overlay_ofs_y;
        self.hud.overlay_alpha = h.overlay_alpha;
        self.hud.overlay_scale_x = h.overlay_scale_x;
        self.hud.overlay_scale_y = h.overlay_scale_y;
        self.hud.overlay_flip = h.overlay_flip;
        self.hud.overlay_mirror = h.overlay_mirror;
        self.hud.overlay_angle = h.overlay_angle;
        self.hud.overlay_pivot_x = h.overlay_pivot_x;
        self.hud.overlay_pivot_y = h.overlay_pivot_y;
        self.hud.overlay_tint_r = h.overlay_tint_r;
        self.hud.overlay_tint_g = h.overlay_tint_g;
        self.hud.overlay_tint_b = h.overlay_tint_b;
        self.hud.overlay_fuzz = h.overlay_fuzz;
        self.hud.overlay_pivot_percent = h.overlay_pivot_percent;
        self.hud.overlay_add_bob = h.overlay_add_bob;
        self.hud.overlay_add_weapon = h.overlay_add_weapon;
        self.hud.overlay_interpolate = h.overlay_interpolate;
        self.hud.bob_ofs_x = h.bob_ofs_x;
        self.hud.bob_ofs_y = h.bob_ofs_y;
        self.hud.weapon_light = h.weapon_light;
        self.hud.weapon_bright = h.weapon_bright;
        self.hud.flash_bright = h.flash_bright;
        self.hud.weapon_fuzz = h.weapon_fuzz;
        self.hud.weapon_alpha = h.weapon_alpha;
        self.hud.weapon_scale_x = h.weapon_scale_x;
        self.hud.weapon_scale_y = h.weapon_scale_y;
        self.hud.weapon_flip = h.weapon_flip;
        self.hud.weapon_mirror = h.weapon_mirror;
        self.hud.weapon_pivot_percent = h.weapon_pivot_percent;
        self.hud.weapon_pivot_x = h.weapon_pivot_x;
        self.hud.weapon_pivot_y = h.weapon_pivot_y;
        self.hud.weapon_angle = h.weapon_angle;
        self.hud.weapon_vertex = h.weapon_vertex;
        self.hud.inv_bar = h
            .inv_bar
            .iter()
            .map(|(icon, amount)| {
                let (w, hh) = self
                    .vfs
                    .as_ref()
                    .and_then(|v| self.patches.ensure(v, icon))
                    .map_or((0.0, 0.0), |p| (f32::from(p.width), f32::from(p.height)));
                room_ui::HudInvItem {
                    icon: icon.clone(),
                    amount: *amount,
                    w,
                    h: hh,
                }
            })
            .collect();
        self.hud.inv_sel = h.inv_sel;
        self.hud.inv_bar_open = h.inv_bar_open;
        self.hud.overlay_vertex = h.overlay_vertex;
        self.hud.weapon_class = h.weapon_class.clone();
        self.hud.player_class = h.player_class.clone();
        self.hud.armor_type = h.armor_type.clone();
        self.hud.crosshair = h.crosshair;
        self.hud.weapon_tint_r = h.weapon_tint_r;
        self.hud.weapon_tint_g = h.weapon_tint_g;
        self.hud.weapon_tint_b = h.weapon_tint_b;
        self.hud.weapon_interpolate = h.weapon_interpolate;
        self.weapon_bob_prev = (self.hud.bob_x, self.hud.bob_y);
        self.hud.bob_x = h.bob_x;
        self.hud.bob_y = h.bob_y;
        self.hud.arms = h.arms;
        self.hud.ammo_table = h.ammo_table;
        self.hud.palette = h.palette;
        self.hud.colormap = h.colormap;
        self.hud.deathmatch = h.deathmatch;
        self.hud.frags = h.frags;
        self.hud.face_back = h.face_back;
    }

    /// Rollback: run a copy of the confirmed game through the frames not
    /// confirmed yet (our real commands, guesses for the others), keep it
    /// to draw from, and move the eye and sprite poses to it. Rebuilt every
    /// local tic from the confirmed game, so a wrong guess lasts one tic.
    fn rebuild_rollback_world(&mut self) {
        let frames = self
            .net
            .as_ref()
            .map(netplay::NetPlay::speculative_frames)
            .unwrap_or_default();
        let seat_u8 = self.local_seat;
        let Some((spec, own_sounds)) = self
            .play
            .as_ref()
            .and_then(|g| g.speculate_with_own_sounds(&frames, seat_u8))
        else {
            self.rollback_world = None;
            return;
        };
        // Your own shots, jumps and pickups sound now, not a round trip
        // later; the confirmed tic skips them (`rollback_sounds`).
        if !own_sounds.is_empty() {
            let maptime = spec.level.maptime;
            for e in &own_sounds {
                self.rollback_sounds.push_back((maptime, e.name.clone()));
            }
            while self.rollback_sounds.len() > 64 {
                self.rollback_sounds.pop_front();
            }
            if let (Some(game), Some(vfs)) = (self.play.as_mut(), self.vfs.as_ref()) {
                let events = game.translate_sounds_for(vfs, &own_sounds, seat_u8);
                if let Some(audio) = self.audio.as_ref()
                    && let Ok(mut shared) = audio.shared.lock()
                {
                    shared.mixer.apply(&events);
                }
            }
        }
        self.rollback_world = Some(spec);
        self.swap_rollback_world(true);
        self.note_sprite_poses();
        let seat = usize::from(self.local_seat);
        // HUD and weapon sprite from the copy: your shot shows at once.
        if let Some(h) = self
            .play
            .as_ref()
            .and_then(|g| g.world())
            .and_then(|w| w.player_hud(seat))
        {
            self.apply_hud_snap(h);
        }
        let eye = self
            .play
            .as_ref()
            .and_then(|g| g.world())
            .and_then(|world| {
                let p = world.players.get(seat)?.as_ref()?;
                let a = world.actors.get(p.camera.or(p.mo)?)?;
                let z = if p.camera.is_some() {
                    a.pos.z()
                        + if a.camera_height > 0.0 {
                            a.camera_height
                        } else {
                            41.0
                        }
                } else {
                    p.view_z
                };
                let ((x, y, ez), _) = room_sim::view_eye(world, a, z);
                Some((
                    x,
                    y,
                    ez,
                    a.camera_yaw().radians(),
                    a.camera_pitch().radians(),
                ))
            });
        self.swap_rollback_world(false);
        if let Some((x, y, z, yaw, pitch)) = eye {
            #[allow(clippy::cast_possible_truncation)]
            let next = (
                x as f32,
                y as f32,
                z as f32,
                yaw as f32,
                (pitch as f32).clamp(-1.5, 1.5),
            );
            self.cam_prev = self.cam_curr.or(Some(next));
            self.cam_curr = Some(next);
            self.yaw = next.3;
            self.pitch = next.4;
        }
    }

    /// The local player's body went through a portal in the last tic.
    fn local_crossed_portal(&self) -> bool {
        let Some(w) = self.play.as_ref().and_then(|g| g.world()) else {
            return false;
        };
        let mo = w
            .players
            .get(usize::from(self.local_seat))
            .and_then(|p| p.as_ref())
            .and_then(|p| p.mo);
        w.portal_moves.iter().any(|m| Some(m.actor) == mo)
    }

    fn note_sprite_poses(&mut self) {
        let Some(game) = self.play.as_ref() else {
            return;
        };
        let Some(world) = game.world() else {
            return;
        };
        let mut next = std::collections::HashMap::new();
        for &id in &world.thinkers.default {
            let Some(a) = world.actors.get(id) else {
                continue;
            };
            #[allow(clippy::cast_possible_truncation)]
            next.insert(
                id,
                (
                    a.pos.x() as f32,
                    a.pos.y() as f32,
                    a.pos.z() as f32,
                    a.angle.degrees() as f32,
                ),
            );
        }
        self.sprite_prev = if self.sprite_curr.is_empty() {
            next.clone()
        } else {
            std::mem::take(&mut self.sprite_curr)
        };
        // Actors that went through a portal this tic snap instead of
        // sliding across the map.
        for m in &world.portal_moves {
            if let Some(&now) = next.get(&m.actor) {
                self.sprite_prev.insert(m.actor, now);
            }
        }
        self.sprite_curr = next;
        // `+INTERPOLATESCALE` / `+INTERPOLATEALPHA` (5.0): only flagged actors
        // keep a scale / alpha sample.
        let mut extra = std::collections::HashMap::new();
        for &id in &world.thinkers.default {
            let Some(a) = world.actors.get(id) else {
                continue;
            };
            if a.extra_flags.get_name("INTERPOLATESCALE")
                || a.extra_flags.get_name("INTERPOLATEALPHA")
            {
                #[allow(clippy::cast_possible_truncation)]
                extra.insert(id, (a.scale_x as f32, a.scale_y as f32, a.alpha as f32));
            }
        }
        self.sprite_extra_prev = std::mem::replace(&mut self.sprite_extra_curr, extra);
    }

    /// Live actor billboards for play mode (`None` in free-fly viewer).
    /// Software view for the current level (`vid_rendermode` 0): build it
    /// when the map or `vid_softscale` changed. `false` keeps the GPU path
    /// (Duke, no level, or a map without BSP nodes).
    fn ensure_soft_world(&mut self) -> bool {
        if self.duke_session.is_some() {
            return false;
        }
        let Some(world) = self
            .play
            .as_ref()
            .filter(|g| g.state == GameState::Level)
            .and_then(|g| g.world())
        else {
            return false;
        };
        let Some(vfs) = self.vfs.as_ref() else {
            return false;
        };
        let scale = match self.console.cvars.get("vid_softscale") {
            Some(room_console::CvarValue::Int(k)) => (*k).clamp(1, 4) as u32,
            _ => 2,
        };
        let key = (Arc::as_ptr(&world.map) as usize, scale);
        if self.soft_world.as_ref().is_some_and(|(k, _)| *k == key) {
            return true;
        }
        match room_render_soft::sim::SoftWorldView::new(vfs, &world.map, 320 * scale, 200 * scale) {
            Some(mut view) => {
                // SR7: ANIMDEFS camera textures.
                view.set_camera_canvases(
                    self.camera_tex_sizes
                        .iter()
                        .map(|(n, &(w, h))| (n.clone(), w, h)),
                );
                // SR8: VOXELDEF.
                view.set_voxeldefs(self.voxeldefs.clone());
                self.soft_world = Some((key, view));
                true
            }
            None => {
                if self.soft_world.take().is_some() || self.frames % 350 == 0 {
                    eprintln!("room: software renderer needs BSP nodes; using hardware");
                }
                false
            }
        }
    }

    /// Draw the level with the software renderer, scaled to the window.
    fn draw_soft_world(&mut self) -> Option<(u32, u32, Vec<u8>)> {
        let (width, height) = self
            .config
            .as_ref()
            .map_or((640, 400), |c| (c.width.max(1), c.height.max(1)));
        // SR8: dynamic lights for true colour (collected before borrowing
        // the view).
        let lights = if matches!(
            self.console.cvars.get("vid_rendermode"),
            Some(room_console::CvarValue::Int(1))
        ) {
            self.collect_dyn_lights()
        } else {
            Vec::new()
        };
        let vfs = self.vfs.as_ref()?;
        let game = self.play.as_ref()?;
        let world = game.world()?;
        let (_, view) = self.soft_world.as_mut()?;
        // SR5: as wide as the window (vanilla-shaped pixels), `fov`,
        // true colour for `vid_rendermode 1`, `r_visibility`.
        let float = |name: &str, default: f64| match self.console.cvars.get(name) {
            Some(room_console::CvarValue::Float(v)) => *v,
            Some(room_console::CvarValue::Int(v)) => f64::from(*v),
            _ => default,
        };
        let soft_h = 200
            * match self.console.cvars.get("vid_softscale") {
                Some(room_console::CvarValue::Int(k)) => (*k).clamp(1, 4) as u32,
                _ => 2,
            };
        let soft_w = ((f64::from(width) / f64::from(height)) * f64::from(soft_h) * 1.2)
            .round()
            .clamp(2.0, 8192.0) as u32;
        let truecolor = matches!(
            self.console.cvars.get("vid_rendermode"),
            Some(room_console::CvarValue::Int(1))
        );
        view.set_view(soft_w, soft_h, float("fov", 90.0), truecolor);
        view.view.renderer.visibility = float("r_visibility", 8.0) / 8.0;
        // SR9: vertical slices on the machine's threads.
        view.view.renderer.threads = match self.console.cvars.get("r_scene_multithreaded") {
            Some(room_console::CvarValue::Bool(false)) | Some(room_console::CvarValue::Int(0)) => 1,
            _ => std::thread::available_parallelism().map_or(1, usize::from),
        };
        let player = world
            .players
            .get(usize::from(self.local_seat))
            .and_then(|p| p.as_ref());
        let forward = self.camera.forward;
        let tic = 1.0 / game.tic_rate() as f32;
        let cam = room_render_soft::sim::SoftCamera {
            x: f64::from(self.camera.eye.x),
            y: f64::from(self.camera.eye.y),
            z: f64::from(self.camera.eye.z),
            angle_deg: f64::from(forward.y.atan2(forward.x).to_degrees()),
            extralight: player.map_or(0, |p| p.extralight),
            fixed_colormap: player.map_or(0, room_sim::Player::fixed_colormap),
            hide: player.and_then(|p| p.mo),
            sky: self.sky_texture.clone(),
            plane_lerp: Some(f64::from((self.tic_accum / tic).clamp(0.0, 1.0))),
            pitch_deg: f64::from(
                forward
                    .z
                    .atan2((forward.x * forward.x + forward.y * forward.y).sqrt())
                    .to_degrees(),
            ),
        };
        // SR8: particles, and dynamic lights in true colour.
        view.particles = room_render_soft::soft_particles(&self.particles);
        view.lights = if truecolor {
            room_render_soft::soft_lights(&lights)
        } else {
            Vec::new()
        };
        // SR6: the shell's decals on walls and floors.
        view.decals.clear();
        view.decals.extend(
            self.decals
                .decals
                .iter()
                .map(|d| room_render_soft::SoftDecal {
                    picture: d.name.clone(),
                    x: f64::from(d.pos.x),
                    y: f64::from(d.pos.y),
                    z: f64::from(d.pos.z),
                    width: f64::from(d.width),
                    height: f64::from(d.height),
                    line: d.line,
                    flat: d.on_floor,
                    light: None,
                    alpha: (d.alpha.clamp(0.0, 1.0) * 255.0).round() as u8,
                    ..room_render_soft::SoftDecal::default()
                }),
        );
        // The frame reuses every buffer but the one the canvas takes.
        let mut rgba = Vec::new();
        view.draw_fit(vfs, world, &self.map, &cam, width, height, &mut rgba);
        Some((width, height, rgba))
    }

    fn collect_live_sprites(&mut self) -> Option<Vec<SpriteInstance>> {
        self.fill_voxel_poses();
        self.fill_model_actors();
        let game = self.play.as_ref()?;
        let world = game.world()?;
        let local_mo = world
            .players
            .get(usize::from(self.local_seat))
            .and_then(|p| p.as_ref())
            .and_then(|p| p.mo);
        // Player pawns wear their seat's colour (vanilla translations).
        let pawn_colors: Vec<(room_sim::ActorId, u8)> = world
            .players
            .iter()
            .enumerate()
            .filter_map(|(seat, p)| {
                let mo = p.as_ref()?.mo?;
                Some((mo, room_render::player_colors::seat_color(seat)))
            })
            .filter(|(_, color)| *color != 0)
            .collect();
        let view_x = f64::from(self.camera.eye.x);
        let view_y = f64::from(self.camera.eye.y);
        let tic = 1.0 / self.play.as_ref().map(GameRuntime::tic_rate).unwrap_or(35) as f32;
        let frac = f64::from((self.tic_accum / tic).clamp(0.0, 1.0));
        let mut out = Vec::new();
        let actors: Vec<_> = world
            .thinkers
            .default
            .iter()
            .filter_map(|&id| {
                let a = world.actors.get(id)?;
                let class_flags = world.classes.get(a.class).map(|c| c.flags).unwrap_or(0);
                let flags = a.flags | class_flags;
                // RenderStyle None draws nothing in GZDoom (BD helpers / tokens).
                if a.destroyed
                    || Some(id) == local_mo
                    || (flags & FLAG_DONTDRAW) != 0
                    || matches!(a.render_style, RenderStyle::None)
                {
                    return None;
                }
                // `RenderRequired` / `RenderHidden`, `VisibleAngles` /
                // `VisiblePitch`, `+SPRITEANGLE`, `SpriteRotation`.
                let eye = room_core::DVec3::new(
                    f64::from(self.camera.eye.x),
                    f64::from(self.camera.eye.y),
                    f64::from(self.camera.eye.z),
                );
                let view = room_sim::sprite_view(world, a, eye, 0)?;
                let bright = a
                    .state
                    .and_then(|sid| world.states.get(sid))
                    .is_some_and(|st| st.bright);
                let light = (actor_light_level(world, a) as f32 / 255.0).clamp(0.2, 1.0);
                let alpha = a.alpha.clamp(0.0, 1.0);
                let fuzz = matches!(a.render_style, RenderStyle::Fuzzy | RenderStyle::OptFuzzy);
                // GZDoom STYLE_Add family: colour × alpha added to the scene.
                let additive = matches!(
                    a.render_style,
                    RenderStyle::Add | RenderStyle::AddStencil | RenderStyle::AddShaded
                );
                #[allow(clippy::cast_possible_truncation)]
                let mut alpha = alpha as f32;
                let (mut scale_x, mut scale_y) = (a.scale_x, a.scale_y);
                let blend_subtract = matches!(a.render_style, RenderStyle::Subtract);
                if let Some(&(psx, psy, pa)) = self.sprite_extra_prev.get(&id) {
                    let t = frac as f32;
                    if a.extra_flags.get_name("INTERPOLATESCALE") {
                        scale_x = f64::from(psx) + (a.scale_x - f64::from(psx)) * frac;
                        scale_y = f64::from(psy) + (a.scale_y - f64::from(psy)) * frac;
                    }
                    if a.extra_flags.get_name("INTERPOLATEALPHA") {
                        alpha = (pa + (alpha - pa) * t).clamp(0.0, 1.0);
                    }
                }
                let prev = self.sprite_prev.get(&id).copied().unwrap_or((
                    a.pos.x() as f32,
                    a.pos.y() as f32,
                    a.pos.z() as f32,
                    a.angle.degrees() as f32,
                ));
                let pose = sprite_pose(
                    a.pos.x(),
                    a.pos.y(),
                    a.pos.z(),
                    a.angle.degrees(),
                    f64::from(prev.0),
                    f64::from(prev.1),
                    f64::from(prev.2),
                    f64::from(prev.3),
                    frac,
                    a.extra_flags.get_name("DONTINTERPOLATE"),
                    a.extra_flags.get_name("INTERPOLATEANGLES"),
                );
                Some((
                    id,
                    pose.x,
                    pose.y,
                    pose.z,
                    pose.angle_deg,
                    a.radius,
                    a.height,
                    scale_x,
                    scale_y,
                    light,
                    bright,
                    alpha,
                    fuzz,
                    a.sprite.clone(),
                    a.frame,
                    a.sprite_ofs_x,
                    a.sprite_ofs_y,
                    flags,
                    additive,
                    view,
                    blend_subtract,
                    // `R_ShouldDrawSpriteShadow` (`r_actorspriteshadow` mode),
                    // with the floor the shadow snaps to.
                    room_sim::casts_sprite_shadow(
                        world,
                        id,
                        self.render_settings.actor_sprite_shadow,
                    )
                    .then(|| {
                        room_sim::floor_at_xy(world, a.pos.x(), a.pos.y())
                            .unwrap_or(a.pos.z())
                            .min(a.pos.z())
                    }),
                ))
            })
            .collect();
        for (
            id,
            x,
            y,
            z,
            angle_deg,
            radius,
            height,
            scale_x,
            scale_y,
            mut light,
            bright,
            alpha,
            fuzz,
            sprite,
            frame,
            sprite_ofs_x,
            sprite_ofs_y,
            flags,
            additive,
            view,
            blend_subtract,
            casts_shadow,
        ) in actors
        {
            // `+ROLLSPRITE` / `+SQUAREPIXELS` keep square pixels.
            let scale_x = scale_x * view.pixel_scale.0;
            let scale_y = scale_y * view.pixel_scale.1;
            if self.actor_draws_voxel(id) || self.actor_draws_model(id) {
                continue;
            }
            // Prefer patch pixel size (1 map unit per texel). Floor items / monsters
            // must keep Doom topoffset math — do not inflate height independently.
            let mut w = (radius * 2.0 * scale_x.abs()).clamp(0.25, 4096.0);
            let mut h = (height * scale_y.abs()).clamp(0.25, 4096.0);
            let mut texture = String::new();
            let mut flip_u = false;
            let mut anchor_u = 0.5_f32;
            // Doom: billboard top = actor.z + topoffset; bottom = top - spriteheight.
            let mut z_bottom = z;
            let grounded = (flags & (FLAG_NOGRAVITY | FLAG_SPAWNCEILING | FLAG_FLOAT)) == 0;
            if !sprite.is_empty() {
                let rot_angle = if view.fixed_rotation {
                    view.rotation_angle_deg
                } else {
                    angle_deg + view.rotation_offset_deg
                };
                let mut rot = sprite_rotation_digit(x, y, rot_angle, view_x, view_y);
                if view.sprite_flip {
                    // `GetSpriteFrame(…, flip)`: the mirrored rotation.
                    rot = room_render::mirror_rotation(rot).unwrap_or(rot);
                }
                let mut candidates = sprite_lump_candidates_rot(&sprite, frame, rot);
                if let Some(vfs) = self.vfs.as_ref() {
                    // Indexed lookup first: mixed-letter pairs (`BOS2A2C8`).
                    if let Some(hit) = self.patches.sprite_frame_lump(vfs, &sprite, frame, rot) {
                        candidates.insert(0, hit);
                    }
                    for (name, flip) in &candidates {
                        // Header only: the pixels are decoded once, for the GPU.
                        if let Some(patch) = self.patches.dims(vfs, name) {
                            let pw = (f64::from(patch.w) * scale_x.abs()).clamp(0.25, 4096.0);
                            let ph = (f64::from(patch.h) * scale_y.abs()).clamp(0.25, 4096.0);
                            let top = f64::from(patch.top) * scale_y.abs();
                            let left = f64::from(patch.left) * scale_x.abs();
                            w = pw;
                            h = ph;
                            // Doom software clips fragments below the floor; with a
                            // depth buffer those texels lose to the flat and the prop
                            // looks sunk. Keep the billboard bottom on/above feet.
                            const FLOOR_LIFT: f64 = 1.0;
                            let raw_bottom = z + top - ph;
                            z_bottom = if grounded {
                                raw_bottom.max(z + FLOOR_LIFT)
                            } else {
                                raw_bottom + 0.05
                            };
                            anchor_u = if pw > 1e-6 { (left / pw) as f32 } else { 0.5 };
                            texture = name.clone();
                            flip_u = *flip;
                            break;
                        }
                    }
                }
            }
            if texture.is_empty()
                && std::env::var("ROOM_DEBUG_SPRITE").is_ok_and(|v| sprite.eq_ignore_ascii_case(&v))
            {
                eprintln!("room: dbgsprite {sprite}{frame} NO TEXTURE");
            }
            // `TNT1` and frames without a lump draw nothing (GZDoom).
            if texture.is_empty() {
                continue;
            }
            // GZDoom never draws sprite `TNT1`, even when a mod ships a TNT1A0 lump.
            if sprite.eq_ignore_ascii_case("TNT1") {
                continue;
            }
            // UZDoom resolves disablefullbright against the actual sprite
            // texture selected for this frame, not only the actor's base name.
            if bright && !self.gldefs.disables_sprite_fullbright(&texture) {
                light = 1.0;
            }
            if self.hit_flash.contains_key(&id.index()) {
                light = 1.0;
            }
            #[allow(clippy::cast_possible_truncation)]
            let yaw = angle_deg.to_radians() as f32;
            // `+SPRITEFLIP` and `+XFLIP` each mirror the sprite.
            flip_u ^= view.sprite_flip ^ view.flip_x;
            let mut sprite_pos = Vec3::new(x as f32, y as f32, z_bottom as f32);
            let mut orient = if flags & room_sim::FLAG_FLATSPRITE != 0 {
                z_bottom = z;
                sprite_pos.z = z as f32;
                SpriteOrient::Flat { yaw }
            } else if flags & room_sim::FLAG_WALLSPRITE != 0 {
                SpriteOrient::Wall { yaw }
            } else {
                SpriteOrient::Billboard
            };
            if view.isometric
                && self.camera.ortho.is_some()
                && matches!(orient, SpriteOrient::Billboard)
            {
                // `+ISOMETRICSPRITES` under an orthographic camera: stretch,
                // tilt toward the camera about the sprite's foot, and step
                // back `radius·√2` along the view so stacked actors overlap.
                let fwd = Vec3::new(self.camera.forward.x, self.camera.forward.y, 0.0)
                    .normalize_or_zero();
                let pitch = (-self.camera.forward.z)
                    .clamp(-1.0, 1.0)
                    .asin()
                    .to_degrees();
                let yaw_deg =
                    f64::from(self.camera.forward.y.atan2(self.camera.forward.x)).to_degrees();
                let (scale_iso, theta) =
                    room_sim::isometric_sprite(radius, w, h, yaw_deg, f64::from(pitch));
                h *= scale_iso;
                #[allow(clippy::cast_possible_truncation)]
                let t = (theta as f32).to_radians();
                let up = (Vec3::Z * t.cos() + fwd * t.sin()).normalize_or_zero();
                let right = Vec3::new(-fwd.y, fwd.x, 0.0);
                #[allow(clippy::cast_possible_truncation)]
                let back = fwd * (radius as f32 * std::f32::consts::SQRT_2);
                sprite_pos -= back;
                // `Facing` centres the quad on `pos`: lift by half the height.
                sprite_pos += up * (h as f32 * 0.5);
                orient = SpriteOrient::Facing { right, up };
            }
            let texture = match pawn_colors.iter().find(|(mo, _)| *mo == id) {
                Some((_, color)) => {
                    room_render::player_colors::player_color_texture(&texture, *color)
                }
                None => texture,
            };
            if std::env::var("ROOM_DEBUG_SPRITE")
                .is_ok_and(|v| v == "*" || sprite.eq_ignore_ascii_case(&v))
            {
                eprintln!(
                    "room: dbgsprite {sprite}{frame} tex={texture} w={w:.1} h={h:.1} pos=({x:.0},{y:.0},{z_bottom:.0}) alpha={alpha} add={additive} light={light}"
                );
            }
            out.push(SpriteInstance {
                pos: sprite_pos
                    + sprite_offset_delta(
                        self.camera.forward,
                        sprite_ofs_x as f32,
                        sprite_ofs_y as f32,
                    ),
                width: w as f32,
                height: h as f32,
                light,
                alpha,
                fuzz,
                texture,
                flip_u,
                anchor_u,
                orient,
                additive,
                #[allow(clippy::cast_possible_truncation)]
                roll: view.roll_rad as f32,
                fx: room_render::SpriteFx {
                    flip_v: view.flip_y,
                    // UZDoom rolls about the actor's origin unless `+ROLLCENTER`.
                    roll_pivot: (!view.roll_center)
                        .then(|| Vec3::new(x as f32, y as f32, z as f32)),
                    no_dynlight: view.no_dynlight,
                    subtract: blend_subtract,
                },
            });
            if let (Some(floor), SpriteOrient::Billboard) = (casts_shadow, orient) {
                // UZDoom sprite shadow: the same frame as a black stencil at
                // `r_actorspriteshadowalpha`, its height ×0.15 about the
                // floor (`r.Scale(…, sprscale.Y * 0.15)` with `z = floorz`),
                // fading with height over `r_actorspriteshadowfadeheight`.
                let base = self.render_settings.actor_sprite_shadow_alpha;
                let fade = self.render_settings.actor_sprite_shadow_fade_height;
                #[allow(clippy::cast_possible_truncation)]
                let above = (z - floor).max(0.0) as f32;
                let shadow_alpha = if fade > 0.0 {
                    (base - above / fade).clamp(0.0, base)
                } else {
                    base
                } * alpha;
                if shadow_alpha > 0.0 {
                    #[allow(clippy::cast_possible_truncation)]
                    let (bottom, height) =
                        ((floor + (z_bottom - z) * 0.15) as f32, (h * 0.15) as f32);
                    // Always behind its sprite (UZDoom: `depth += EQUAL_EPSILON`).
                    let away = Vec3::new(
                        x as f32 - self.camera.eye.x,
                        y as f32 - self.camera.eye.y,
                        0.0,
                    )
                    .normalize_or_zero();
                    out.push(SpriteInstance {
                        pos: Vec3::new(x as f32, y as f32, bottom) + away,
                        width: w as f32,
                        height,
                        light: 0.0,
                        alpha: shadow_alpha,
                        fuzz: false,
                        texture: out.last().map(|s| s.texture.clone()).unwrap_or_default(),
                        flip_u,
                        anchor_u,
                        orient: SpriteOrient::Billboard,
                        additive: false,
                        roll: 0.0,
                        fx: room_render::SpriteFx {
                            flip_v: view.flip_y,
                            ..Default::default()
                        },
                    });
                }
            }
        }
        Some(out)
    }

    fn refresh_sky_texture(&mut self) {
        let map_name = self
            .play
            .as_ref()
            .map(|g| g.map_name().to_owned())
            .unwrap_or_else(|| self.map.name.clone());
        let (boxes, sky, cube) = load_sky_defs(self.vfs.as_ref(), &map_name);
        if !boxes.is_empty() {
            self.skyboxes = boxes;
        }
        self.sky_cubemap = cube;
        if !sky.is_empty() {
            self.sky_texture = sky;
        } else if self.sky_cubemap.is_some() {
            self.sky_texture.clear();
        }
        // ACS `ChangeSky` wins over MAPINFO/default for sky1.
        if let Some(game) = self.play.as_ref() {
            if let Some(world) = game.world() {
                if let Some(s) = world.sky1_override.as_ref() {
                    if !s.is_empty() {
                        self.sky_texture = s.clone();
                        self.sky_cubemap = None;
                    }
                }
            }
        }
    }

    /// Render Z-mirrored views into `__PREFL*` floor/ceiling materials.
    fn refresh_plane_reflect_textures(&mut self, planes: &[room_render::PlaneReflectSurface]) {
        if planes.is_empty() || self.camera_tex_frame % 4 != 2 {
            return;
        }
        let Some(gpu) = self.gpu.as_ref() else {
            return;
        };
        let Some(pass) = self.pass.as_mut() else {
            return;
        };
        let cam = self.camera;
        for p in planes {
            let reflected = cam.reflect_across_z(p.plane_z);
            let mut scene = build_scene(&self.map, &reflected, None);
            scene.flat_ranges.retain(|r| !r.name.starts_with("__PREFL"));
            scene
                .translucent_flat_ranges
                .retain(|r| !r.name.starts_with("__PREFL"));
            scene.plane_reflects.clear();
            let target = OffscreenTarget::with_format(gpu, 128, 128, pass.format());
            render_offscreen(gpu, pass, &target, &reflected, &scene);
            let Ok(rgba) = read_rgba(gpu, &target) else {
                continue;
            };
            let _ =
                self.materials
                    .upsert_rgba(&gpu.device, &gpu.queue, &p.tex_name, 128, 128, &rgba);
        }
    }

    /// Render ACS camera canvases into `MaterialCatalog` (throttled).
    fn refresh_camera_textures(&mut self) {
        self.camera_tex_frame = self.camera_tex_frame.wrapping_add(1);
        // Full GPU readback is expensive — refresh every 4th frame.
        if self.camera_tex_frame % 4 != 1 {
            return;
        }
        let Some(game) = self.play.as_ref() else {
            return;
        };
        let Some(world) = game.world() else {
            return;
        };
        if world.camera_textures.is_empty() {
            return;
        }
        let Some(gpu) = self.gpu.as_ref() else {
            return;
        };
        let Some(pass) = self.pass.as_mut() else {
            return;
        };
        let binds: Vec<(String, room_sim::CameraTextureBind)> = world
            .camera_textures
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        for (name, bind) in binds {
            let Some(actor) = world.actors.get(bind.camera) else {
                continue;
            };
            let (w, h) = self
                .camera_tex_sizes
                .get(&name)
                .copied()
                .unwrap_or((64, 64));
            let aspect = f64::from(w) / f64::from(h.max(1));
            let yaw = actor.angle.radians();
            let z = actor.pos.z() + 41.0;
            let cam =
                Camera::from_actor_view(actor.pos.x(), actor.pos.y(), z, yaw, bind.fov, aspect);
            let scene = build_scene(&self.map, &cam, None);
            let target = OffscreenTarget::with_format(gpu, w, h, pass.format());
            render_offscreen(gpu, pass, &target, &cam, &scene);
            let Ok(rgba) = read_rgba(gpu, &target) else {
                continue;
            };
            let _ = self
                .materials
                .upsert_rgba(&gpu.device, &gpu.queue, &name, w, h, &rgba);
        }
    }

    fn prepare_title_start_map(&mut self, name: &str) {
        let Some(vfs) = self.vfs.as_ref() else {
            return;
        };
        let mut next_map = match load_map(vfs, name) {
            Ok(m) => m,
            Err(e) => {
                self.messages.push(format!("start {name}: {e}"), 70);
                return;
            }
        };
        ensure_nodes(&mut next_map);
        self.map_base = next_map.clone();
        self.map = next_map.clone();
        if let Some(game) = self.play.as_mut() {
            game.set_pending_map(next_map);
        }
    }

    fn play_advance(&mut self) {
        // Netplay: leaving the intermission is a ready-vote carried by the
        // lockstep (use / fire), so every peer goes on the same tic.
        if self.net.is_some()
            && self
                .play
                .as_ref()
                .is_some_and(|g| g.state == GameState::Intermission)
        {
            return;
        }
        self.play_advance_now();
    }

    fn play_advance_now(&mut self) {
        let Some(game) = self.play.as_mut() else {
            return;
        };
        match game.state {
            GameState::Title => {
                if let Err(e) = game.begin_pending_map() {
                    self.messages.push(format!("start: {e}"), 70);
                } else {
                    self.reset_impact_fx();
                    self.refresh_sky_texture();
                    self.menu = None;
                    self.messages.push("Entering level", 35);
                    self.start_level_music();
                    self.set_play_look(self.mouse_look_enabled());
                }
            }
            GameState::Intermission => {
                let next = game.intermission.as_ref().and_then(|t| t.next_map.clone());
                if let (Some(name), Some(vfs)) = (next.as_deref(), self.vfs.as_ref()) {
                    match load_map(vfs, name) {
                        Ok(mut next_map) => {
                            ensure_nodes(&mut next_map);
                            self.map_base = next_map.clone();
                            self.map = next_map.clone();
                            match game.continue_to_map(next_map) {
                                Ok(()) => {
                                    self.reset_impact_fx();
                                    self.refresh_sky_texture();
                                    self.menu = None;
                                    self.messages.push(format!("Entering {name}"), 35);
                                    self.start_level_music();
                                }
                                Err(e) => {
                                    self.messages.push(format!("continue: {e}"), 70);
                                    game.leave_intermission();
                                    let title = title_menu(game);
                                    self.menu = Some(title);
                                    self.restart_title_anim();
                                    self.start_title_music();
                                }
                            }
                        }
                        Err(e) => {
                            self.messages.push(format!("next map {name}: {e}"), 70);
                            game.leave_intermission();
                            let title = title_menu(game);
                            self.menu = Some(title);
                            self.restart_title_anim();
                            self.start_title_music();
                        }
                    }
                } else {
                    game.enter_finale();
                    self.finale_anim_origin = self.elapsed_ms;
                    self.menu = None;
                    self.messages.push("Finale", 35);
                    // Keep intermission theme through the finale stub.
                }
            }
            GameState::Finale => {
                game.leave_finale();
                let title = title_menu(game);
                self.reset_impact_fx();
                self.restart_title_anim();
                self.menu = Some(title);
                self.messages.push("Title", 35);
                self.start_title_music();
            }
            GameState::Level => {}
        }
    }

    fn toggle_pause(&mut self) {
        let Some(game) = self.play.as_mut() else {
            return;
        };
        if game.state != GameState::Level {
            return;
        }
        let menudef = game.menudef().cloned();
        let Some(world) = game.world_mut() else {
            return;
        };
        if world.paused {
            world.paused = false;
            self.menu = None;
            self.messages.push("Resumed", 35);
            self.keys.attack.clear();
            self.set_play_look(self.mouse_look_enabled());
        } else {
            world.paused = true;
            let mut stack = MenuStack {
                stack: vec![play_overlay::pause_menu(
                    cvar_bool(&self.console.cvars, "vid_fps"),
                    self.from_launcher,
                )],
                ..MenuStack::default()
            };
            stack.menudef = menudef;
            self.menu = Some(stack);
            self.messages.push("Paused", 35);
            self.keys.attack.clear();
            self.set_play_look(false);
        }
    }

    /// Open the live Graphics / shader menu (F5). Pauses the level if needed.
    fn open_graphics_menu(&mut self) {
        let Some(game) = self.play.as_mut() else {
            return;
        };
        if game.state != GameState::Level {
            // Title / other: still allow browsing the CVar list.
            if let Some(stack) = self.menu.as_mut() {
                stack.push_menu(graphics_menu(&self.console.cvars));
            } else {
                let mut stack = MenuStack::doom_main();
                stack.push_menu(graphics_menu(&self.console.cvars));
                self.menu = Some(stack);
            }
            self.messages.push("Graphics", 35);
            self.set_play_look(false);
            return;
        }
        let menudef = game.menudef().cloned();
        let Some(world) = game.world_mut() else {
            return;
        };
        if !world.paused {
            world.paused = true;
            let mut stack = MenuStack {
                stack: vec![play_overlay::pause_menu(
                    cvar_bool(&self.console.cvars, "vid_fps"),
                    self.from_launcher,
                )],
                ..MenuStack::default()
            };
            stack.menudef = menudef;
            self.menu = Some(stack);
            self.keys.attack.clear();
            self.set_play_look(false);
        }
        if let Some(stack) = self.menu.as_mut() {
            if stack
                .top_title()
                .is_some_and(|t| t.eq_ignore_ascii_case("Graphics"))
            {
                stack.replace_top(graphics_menu(&self.console.cvars));
            } else {
                stack.push_menu(graphics_menu(&self.console.cvars));
            }
        }
        self.messages
            .push("Graphics — arrows / Enter, Esc back", 70);
    }

    /// Open Duke's native Sound Setup screen (F4).
    fn open_duke_audio_menu(&mut self) {
        if self.duke_session.is_none() {
            return;
        }
        let mut stack = MenuStack::default();
        stack.push_menu(duke_audio_menu(&self.console.cvars));
        self.menu = Some(stack);
        self.keys.attack.clear();
        self.set_play_look(false);
    }

    fn open_duke_pause_menu(&mut self) {
        // Chocolate menu 50 drawn with Duke art (`room_duke::menu`).
        let (volumes, skills) = self
            .duke_session
            .as_ref()
            .and_then(|session| session.script.as_ref())
            .map(|script| (script.volume_names.to_vec(), script.skill_names.to_vec()))
            .unwrap_or_default();
        self.menu = None;
        self.duke_menu = Some(DukeMenuState {
            menu: room_duke::menu::DukeMenu::new(true, volumes, skills),
            clock: 0.0,
            backdrop: false,
        });
        self.keys.attack.clear();
        self.set_play_look(false);
        self.duke_environment_music_active = true;
        self.start_duke_environment_music();
    }

    fn open_duke_quit_confirmation(&mut self) {
        self.menu = Some(MenuStack {
            stack: vec![duke_quit_confirm_menu()],
            ..MenuStack::default()
        });
        self.keys.attack.clear();
        self.set_play_look(false);
        self.duke_environment_music_active = true;
        self.start_duke_environment_music();
    }

    /// Feed one key to the native Duke menu and carry out its action.
    fn duke_menu_key(&mut self, key: room_duke::menu::MenuKey, event_loop: &ActiveEventLoop) {
        use room_duke::menu::MenuAction;
        let Some(state) = self.duke_menu.as_mut() else {
            return;
        };
        let action = state.menu.key(key);
        let labels = state.menu.take_sounds();
        if action == MenuAction::QuitToTitle {
            state.backdrop = true;
        }
        self.play_duke_bonus_sounds(&labels);
        match action {
            MenuAction::None | MenuAction::QuitToTitle => {}
            MenuAction::Resume => self.close_duke_menu(),
            MenuAction::QuitGame => self.leave(event_loop),
            MenuAction::OpenHelp => {
                self.close_duke_menu();
                self.duke_help_page = 1;
            }
            MenuAction::OpenSave | MenuAction::OpenLoad => {
                let load = action == MenuAction::OpenLoad;
                self.duke_menu = None;
                self.menu = Some(MenuStack {
                    stack: vec![duke_save_menu(load, self.duke_last_save_slot.unwrap_or(0))],
                    ..MenuStack::default()
                });
            }
            MenuAction::OpenOptions => {
                self.duke_menu = None;
                self.open_duke_audio_menu();
            }
            MenuAction::NewGame { volume, skill } => {
                if let Some(session) = self.duke_session.as_mut() {
                    session.campaign = room_duke::DukeCampaignProgress::new(volume, 0);
                }
                self.close_duke_menu();
                self.duke_bonus_panel = None;
                self.advance_duke_campaign(
                    room_duke::DukeCampaignTransition::NextLevel { volume, level: 0 },
                    Some(skill),
                );
            }
        }
    }

    fn close_duke_menu(&mut self) {
        self.menu = None;
        self.duke_menu = None;
        self.keys.attack.clear();
        self.set_play_look(true);
        if self.duke_environment_music_active {
            self.duke_environment_music_active = false;
            self.start_duke_level_music();
        }
    }

    /// Grab / release the cursor for mouselook.
    ///
    /// Safe to call before the window exists (`mouse_look` is remembered) and
    /// again from `resumed` / `Focused` — window-side hide/recenter always re-runs
    /// when a window is present so startup acquire cannot be skipped.
    fn set_play_look(&mut self, on: bool) {
        self.mouse_look = on;
        let Some(w) = self.window.clone() else {
            return;
        };
        if on {
            // Ensure key/mouse go to this window (macOS often opens unfocused).
            w.focus_window();
            self.recenter_cursor_for_look();
            #[cfg(target_os = "macos")]
            {
                // Locked (CGAssociate=false) eats left-click on macOS. Keep the
                // cursor associated, hide it, and edge-recenter so
                // WindowEvent::MouseInput / DeviceEvent::Button both fire.
                let _ = w.set_cursor_grab(winit::window::CursorGrabMode::None);
            }
            #[cfg(not(target_os = "macos"))]
            {
                let grab = winit::window::CursorGrabMode::Locked;
                if w.set_cursor_grab(grab).is_err() {
                    let _ = w.set_cursor_grab(winit::window::CursorGrabMode::Confined);
                }
            }
            w.set_cursor_visible(false);
        } else {
            let _ = w.set_cursor_grab(winit::window::CursorGrabMode::None);
            w.set_cursor_visible(true);
        }
    }

    /// Warp cursor to window center; ignore the following synthetic motion event.
    fn recenter_cursor_for_look(&mut self) {
        let Some(w) = self.window.as_ref() else {
            return;
        };
        let size = w.inner_size();
        if w.set_cursor_position(PhysicalPosition::new(size.width / 2, size.height / 2))
            .is_ok()
        {
            self.ignore_warp_motion = true;
        }
    }

    fn menu_activate(&mut self, event_loop: &ActiveEventLoop) {
        let Some((cmd, sfx)) = self.menu.as_mut().map(MenuStack::activate) else {
            return;
        };
        if let Some(sfx) = sfx {
            self.play_menu_sfx(sfx);
        }
        let Some(cmd) = cmd else {
            return;
        };
        match cmd {
            MenuCommand::StartSkill(level) => {
                if let Some(game) = self.play.as_mut() {
                    game.options.skill = Skill::new(level);
                }
                if let Some(start) = self
                    .menu
                    .as_ref()
                    .and_then(MenuStack::start_map)
                    .map(str::to_owned)
                {
                    self.prepare_title_start_map(&start);
                }
                self.play_advance();
            }
            MenuCommand::Resume => self.toggle_pause(),
            MenuCommand::Quit => {
                // Confirm sheet pushed inside MenuStack::activate.
            }
            MenuCommand::QuitConfirmed => self.leave(event_loop),
            MenuCommand::OpenMenu(name)
                if name.eq_ignore_ascii_case("GraphicsMenu")
                    || name.eq_ignore_ascii_case("Graphics") =>
            {
                if let Some(stack) = self.menu.as_mut() {
                    stack.push_menu(graphics_menu(&self.console.cvars));
                }
            }
            MenuCommand::EndGame | MenuCommand::OpenMenu(_) | MenuCommand::SelectEpisode(_) => {
                // Confirm / submenus handled inside MenuStack::activate.
            }
            MenuCommand::SelectPlayerClass(class) => {
                if let Some(game) = self.play.as_mut() {
                    game.set_player_class(Some(&class));
                }
            }
            MenuCommand::EndGameConfirmed => {
                if let Some(game) = self.play.as_mut() {
                    game.return_to_title();
                }
                self.keys.attack.clear();
                self.reset_impact_fx();
                self.restart_title_anim();
                let title = self
                    .play
                    .as_ref()
                    .map_or_else(MenuStack::doom_main, title_menu);
                self.menu = Some(title);
                self.set_play_look(false);
                self.messages.push("Title", 35);
                self.start_title_music();
            }
            MenuCommand::LoadGame => self.load_play_save(None),
            MenuCommand::SaveGame => self.save_play_save(None),
            MenuCommand::ReadThis => {
                self.messages.push("Read This — help screen TBD", 70);
            }
            MenuCommand::Other(action) => {
                if action == play_overlay::PAUSE_FPS {
                    let on = !cvar_bool(&self.console.cvars, "vid_fps");
                    let _ = self
                        .console
                        .cvars
                        .set("vid_fps", if on { "true" } else { "false" });
                    if let Some(stack) = self.menu.as_mut() {
                        stack.replace_top(play_overlay::pause_menu(on, self.from_launcher));
                    }
                } else if action == "duke_pause_resume" {
                    self.close_duke_menu();
                } else if action == "duke_pause_save" {
                    self.menu = Some(MenuStack {
                        stack: vec![duke_save_menu(false, self.duke_last_save_slot.unwrap_or(0))],
                        ..MenuStack::default()
                    });
                } else if action == "duke_pause_load" {
                    self.menu = Some(MenuStack {
                        stack: vec![duke_save_menu(true, self.duke_last_save_slot.unwrap_or(0))],
                        ..MenuStack::default()
                    });
                } else if action == "duke_pause_audio" {
                    self.open_duke_audio_menu();
                } else if action == "duke_pause_quit" {
                    self.open_duke_quit_confirmation();
                } else if action == "duke_slot_empty" {
                    self.messages.push("Empty save slot", 35);
                } else if let Some(rest) = action.strip_prefix("duke_save_slot ") {
                    if let Ok(slot) = rest.parse::<usize>()
                        && slot < 10
                    {
                        self.save_duke_slot(slot);
                    }
                } else if let Some(rest) = action.strip_prefix("duke_load_slot ") {
                    if let Ok(slot) = rest.parse::<usize>()
                        && slot < 10
                    {
                        self.load_duke_slot(slot);
                    }
                } else if let Some(rest) = action.strip_prefix("duke_confirm_load_slot ") {
                    if let Ok(slot) = rest.parse::<usize>()
                        && slot < 10
                    {
                        self.load_duke_slot(slot);
                    }
                } else if action == "duke_slot_close"
                    || action == "duke_audio_close"
                    || action == "duke_quit_cancel"
                {
                    self.close_duke_menu();
                } else if action == "duke_audio_sound" {
                    let enabled = !matches!(
                        self.console.cvars.get("duke_sound"),
                        Some(room_console::CvarValue::Bool(false))
                    );
                    let next = if enabled { "false" } else { "true" };
                    let _ = self.console.cvars.set("duke_sound", next);
                    if enabled {
                        if let Some(audio) = self.audio.as_ref() {
                            if let Ok(mut shared) = audio.shared.lock() {
                                shared.mixer.voices.clear();
                            }
                        }
                    }
                    self.refresh_duke_audio_menu();
                } else if action == "duke_audio_music" {
                    let enabled = !matches!(
                        self.console.cvars.get("duke_music"),
                        Some(room_console::CvarValue::Bool(false))
                    );
                    let _ = self
                        .console
                        .cvars
                        .set("duke_music", if enabled { "false" } else { "true" });
                    if enabled {
                        if let Some(audio) = self.audio.as_ref() {
                            if let Ok(mut shared) = audio.shared.lock() {
                                shared.music.stop();
                            }
                        }
                    } else {
                        if self.duke_environment_music_active {
                            self.start_duke_environment_music();
                        } else {
                            self.start_duke_level_music();
                        }
                    }
                    self.refresh_duke_audio_menu();
                } else if action == "duke_audio_talk" {
                    let enabled = !matches!(
                        self.console.cvars.get("duke_talk"),
                        Some(room_console::CvarValue::Bool(false))
                    );
                    let _ = self
                        .console
                        .cvars
                        .set("duke_talk", if enabled { "false" } else { "true" });
                    if enabled {
                        if let Some(audio) = self.audio.as_ref() {
                            if let Ok(mut shared) = audio.shared.lock() {
                                shared.mixer.voices.retain(|voice| {
                                    voice.flags.0 & SoundFlags::DUKE_EXCLUSIVE == 0
                                });
                            }
                        }
                    }
                    self.refresh_duke_audio_menu();
                } else if action == "duke_audio_ambience" {
                    let enabled = !matches!(
                        self.console.cvars.get("duke_ambience"),
                        Some(room_console::CvarValue::Bool(false))
                    );
                    let _ = self
                        .console
                        .cvars
                        .set("duke_ambience", if enabled { "false" } else { "true" });
                    if let Some(session) = self.duke_session.as_mut() {
                        session.reset_ambient_sound_markers();
                    }
                    if enabled {
                        if let Some(audio) = self.audio.as_ref() {
                            if let Ok(mut shared) = audio.shared.lock() {
                                shared
                                    .mixer
                                    .voices
                                    .retain(|voice| voice.flags.0 & SoundFlags::DUKE_AMBIENCE == 0);
                            }
                        }
                    }
                    self.refresh_duke_audio_menu();
                } else if action == "duke_audio_reverse_stereo" {
                    let enabled = !matches!(
                        self.console.cvars.get("duke_reverse_stereo"),
                        Some(room_console::CvarValue::Bool(false))
                    );
                    let _ = self.console.cvars.set(
                        "duke_reverse_stereo",
                        if enabled { "false" } else { "true" },
                    );
                    if let Some(audio) = self.audio.as_ref() {
                        if let Ok(mut shared) = audio.shared.lock() {
                            shared.mixer.reverse_stereo = !enabled;
                        }
                    }
                    self.refresh_duke_audio_menu();
                } else if action == "duke_audio_autoaim" {
                    self.adjust_duke_autoaim(1);
                } else if action == "duke_audio_sfx_volume" || action == "duke_audio_music_volume" {
                    let name = if action == "duke_audio_sfx_volume" {
                        "duke_sfx_volume"
                    } else {
                        "duke_music_volume"
                    };
                    self.adjust_duke_audio_volume(name, 4);
                    self.refresh_duke_audio_menu();
                } else if let Some(rest) = action.strip_prefix("optioncycle ") {
                    // `optioncycle <cvar> <lo> <hi>`
                    let parts: Vec<&str> = rest.split_whitespace().collect();
                    if parts.len() >= 3 {
                        let name = parts[0];
                        let lo: i32 = parts[1].parse().unwrap_or(0);
                        let hi: i32 = parts[2].parse().unwrap_or(lo);
                        let v = cycle_cvar(&mut self.console.cvars, name, lo, hi);
                        self.messages.push(format!("{name} = {v}"), 35);
                        self.refresh_graphics_menu();
                    }
                } else if action == "optionrendermode" {
                    let v = next_rendermode(&mut self.console.cvars);
                    self.messages.push(format!("vid_rendermode = {v}"), 35);
                    self.refresh_graphics_menu();
                } else if let Some(rest) = action.strip_prefix("optionswap ") {
                    // `optionswap <cvar> <a> <b>`
                    let parts: Vec<&str> = rest.split_whitespace().collect();
                    if parts.len() >= 3 {
                        let name = parts[0];
                        let a: i32 = parts[1].parse().unwrap_or(0);
                        let b: i32 = parts[2].parse().unwrap_or(a);
                        let v = swap_cvar(&mut self.console.cvars, name, a, b);
                        self.messages.push(format!("{name} = {v}"), 35);
                        self.refresh_graphics_menu();
                    }
                } else if let Some(rest) = action.strip_prefix("optiontogglefloat ") {
                    // `optiontogglefloat <cvar> <on_value>`
                    let parts: Vec<&str> = rest.split_whitespace().collect();
                    if parts.len() >= 2 {
                        let name = parts[0];
                        let on_v: f64 = parts[1].parse().unwrap_or(1.0);
                        let v = toggle_float_cvar(&mut self.console.cvars, name, on_v);
                        self.messages.push(format!("{name} = {v:.2}"), 35);
                        self.refresh_graphics_menu();
                    }
                } else if let Some(cvar) = action.strip_prefix("option ") {
                    let name = cvar.trim();
                    if !name.is_empty() {
                        // Bool toggle; also accept int 0/1 CVars via set flip.
                        match self.console.cvars.get(name) {
                            Some(room_console::CvarValue::Bool(_)) => {
                                let _ = self.console.exec(&format!("toggle {name}"));
                            }
                            Some(room_console::CvarValue::Int(i)) => {
                                let next = if *i != 0 { 0 } else { 1 };
                                let _ = self.console.cvars.set(name, &next.to_string());
                            }
                            _ => {
                                let _ = self.console.exec(&format!("toggle {name}"));
                            }
                        }
                        self.messages.push(format!("toggled {name}"), 35);
                        self.refresh_graphics_menu();
                        if name == "mouse_look"
                            && self
                                .play
                                .as_ref()
                                .is_some_and(|g| g.state == GameState::Level)
                            && self.menu.is_none()
                        {
                            self.set_play_look(self.mouse_look_enabled());
                        }
                    }
                } else if let Some(cmd) = action.strip_prefix("cmd ") {
                    let line = cmd.trim();
                    if line.eq_ignore_ascii_case("writeconfig")
                        || line.eq_ignore_ascii_case("saveconfig")
                    {
                        self.write_user_config();
                    } else if !line.is_empty() {
                        self.apply_console(line);
                    }
                }
            }
        }
    }

    /// Refresh live Graphics menu labels after a CVar change.
    fn refresh_graphics_menu(&mut self) {
        let Some(stack) = self.menu.as_mut() else {
            return;
        };
        if stack
            .top_title()
            .is_some_and(|t| t.eq_ignore_ascii_case("Graphics"))
        {
            let (sel, scroll) = stack
                .top()
                .map(|m| (m.selected, m.scroll))
                .unwrap_or((0, 0));
            let mut menu = graphics_menu(&self.console.cvars);
            menu.selected = sel.min(menu.items.len().saturating_sub(1));
            menu.scroll = scroll;
            menu.ensure_scroll_visible();
            stack.replace_top(menu);
        }
    }

    fn refresh_duke_audio_menu(&mut self) {
        let Some(stack) = self.menu.as_mut() else {
            return;
        };
        if stack
            .top_title()
            .is_some_and(|title| title.eq_ignore_ascii_case("SETUP SOUNDS"))
        {
            let (selected, scroll) = stack
                .top()
                .map(|menu| (menu.selected, menu.scroll))
                .unwrap_or((0, 0));
            let mut menu = duke_audio_menu(&self.console.cvars);
            menu.selected = selected.min(menu.items.len().saturating_sub(1));
            menu.scroll = scroll;
            menu.ensure_scroll_visible();
            stack.replace_top(menu);
        }
    }

    fn adjust_duke_audio_volume(&mut self, name: &str, delta: i32) {
        let current = match self.console.cvars.get(name) {
            Some(room_console::CvarValue::Int(value)) => *value,
            _ => 255,
        }
        .clamp(0, 255);
        let next = (current + delta).clamp(0, 255);
        let _ = self.console.cvars.set(name, &next.to_string());
        if let Some(audio) = self.audio.as_ref() {
            if let Ok(mut shared) = audio.shared.lock() {
                if name == "duke_sfx_volume" {
                    shared.mixer.duke_sfx_gain = next as f32 / 255.0;
                } else {
                    shared.mixer.music_gain = next as f32 / 255.0;
                }
            }
        }
    }

    fn adjust_selected_duke_audio_volume(&mut self, delta: i32) {
        let action = self
            .menu
            .as_ref()
            .filter(|menu| {
                menu.top_title()
                    .is_some_and(|title| title.eq_ignore_ascii_case("SETUP SOUNDS"))
            })
            .and_then(MenuStack::top)
            .and_then(|menu| menu.items.get(menu.selected))
            .map(|item| item.action.as_str());
        let name = match action {
            Some("duke_audio_sfx_volume") => "duke_sfx_volume",
            Some("duke_audio_music_volume") => "duke_music_volume",
            _ => return,
        };
        self.adjust_duke_audio_volume(name, delta);
        self.refresh_duke_audio_menu();
    }

    fn adjust_duke_autoaim(&mut self, delta: i32) {
        let current = match self.console.cvars.get("duke_autoaim") {
            Some(room_console::CvarValue::Int(value)) => (*value).clamp(1, 2),
            _ => 2,
        };
        let next = if delta < 0 {
            if current == 1 { 2 } else { current - 1 }
        } else if current == 2 {
            1
        } else {
            current + 1
        };
        let _ = self.console.cvars.set("duke_autoaim", &next.to_string());
        self.refresh_duke_audio_menu();
    }

    fn duke_audio_volume_row_selected(&self) -> bool {
        self.menu
            .as_ref()
            .filter(|menu| {
                menu.top_title()
                    .is_some_and(|title| title.eq_ignore_ascii_case("SETUP SOUNDS"))
            })
            .and_then(MenuStack::top)
            .and_then(|menu| menu.items.get(menu.selected))
            .is_some_and(|item| {
                item.action == "duke_audio_sfx_volume" || item.action == "duke_audio_music_volume"
            })
    }

    fn duke_audio_autoaim_row_selected(&self) -> bool {
        self.menu
            .as_ref()
            .filter(|menu| {
                menu.top_title()
                    .is_some_and(|title| title.eq_ignore_ascii_case("SETUP SOUNDS"))
            })
            .and_then(MenuStack::top)
            .and_then(|menu| menu.items.get(menu.selected))
            .is_some_and(|item| item.action == "duke_audio_autoaim")
    }

    fn duke_audio_setting_row_selected(&self) -> bool {
        self.duke_audio_volume_row_selected() || self.duke_audio_autoaim_row_selected()
    }

    /// Persist config; skips shader CVars when [`Self::ephemeral_shaders`].
    fn write_user_config(&mut self) {
        let path = dirs_room_ini();
        let exclude: &[&str] = if self.ephemeral_shaders {
            room_render::SHADER_CVAR_NAMES
        } else {
            &[]
        };
        match room_console::save_ini_file_excluding(&self.console.cvars, &path, exclude) {
            Ok(()) => {
                let note = if self.ephemeral_shaders {
                    " (shaders not saved — combat-demo)"
                } else {
                    ""
                };
                self.messages
                    .push(format!("wrote {}{note}", path.display()), 70);
            }
            Err(e) => self.messages.push(format!("writeconfig: {e}"), 70),
        }
    }

    fn play_menu_sfx(&mut self, sfx: MenuSfx) {
        let Some(vfs) = self.vfs.as_ref() else {
            return;
        };
        let Some(game) = self.play.as_mut() else {
            return;
        };
        let Some(sounds) = game.sounds.as_mut() else {
            return;
        };
        let Some(ev) = sounds.ui_event(vfs, sfx.logical()) else {
            return;
        };
        if let Some(audio) = self.audio.as_ref() {
            if let Ok(mut shared) = audio.shared.lock() {
                shared.mixer.bank.merge_missing(&sounds.bank);
                shared.mixer.apply(&[ev]);
            }
        }
    }

    fn request_screenshot(&mut self) {
        // Do not push a HUD message here — it would be burned into the PNG.
        self.pending_screenshot = true;
    }

    fn toggle_gif_record(&mut self) {
        if self.gif_recorder.is_some() {
            self.finish_gif_record();
        } else {
            self.gif_recorder = Some(GifRecorder::new());
            self.messages.push("GIF recording… (F11 to stop)", 70);
            eprintln!("room: GIF recording started");
        }
    }

    fn finish_gif_record(&mut self) {
        let Some(mut rec) = self.gif_recorder.take() else {
            return;
        };
        let n = rec.frames.len();
        match rec.finish() {
            Some(bytes) => match screen_capture::ensure_screenshots_dir() {
                Ok(dir) => match screen_capture::write_gif_bytes(&dir, &bytes) {
                    Ok(path) => {
                        let msg = format!("GIF saved ({n} frames): {}", path.display());
                        self.messages.push(msg.clone(), 105);
                        eprintln!("room: {msg}");
                    }
                    Err(e) => {
                        self.messages.push(format!("GIF write failed: {e}"), 70);
                        eprintln!("room: GIF write failed: {e}");
                    }
                },
                Err(e) => {
                    self.messages.push(format!("GIF dir failed: {e}"), 70);
                    eprintln!("room: GIF dir failed: {e}");
                }
            },
            None => {
                self.messages.push("GIF: no frames captured", 70);
                eprintln!("room: GIF: no frames captured");
            }
        }
    }

    fn save_screenshot_rgba(&mut self, rgba: &[u8], width: u32, height: u32) {
        match screen_capture::ensure_screenshots_dir() {
            Ok(dir) => match screen_capture::write_png(&dir, rgba, width, height) {
                Ok(path) => {
                    let msg = format!("Screenshot: {}", path.display());
                    self.messages.push(msg.clone(), 105);
                    eprintln!("room: {msg}");
                }
                Err(e) => {
                    self.messages.push(format!("Screenshot failed: {e}"), 70);
                    eprintln!("room: screenshot failed: {e}");
                }
            },
            Err(e) => {
                self.messages
                    .push(format!("Screenshot dir failed: {e}"), 70);
                eprintln!("room: screenshot dir failed: {e}");
            }
        }
    }

    fn apply_console(&mut self, line: &str) {
        let trimmed = line.trim();
        if self.mod_buttons.apply_cmd(trimmed) {
            return;
        }
        if self.weapon_impulses.apply_cmd(trimmed) {
            return;
        }
        // Inventory bar (UZDoom `invnext` / `invprev` / `invuse`): one tic each.
        match trimmed.to_ascii_lowercase().as_str() {
            "invnext" => {
                self.inv_cmd |= Buttons::INVNEXT;
                return;
            }
            "invprev" => {
                self.inv_cmd |= Buttons::INVPREV;
                return;
            }
            "invuse" => {
                self.inv_cmd |= Buttons::INVUSE;
                return;
            }
            "+showscores" => {
                self.show_scores = true;
                return;
            }
            "-showscores" => {
                self.show_scores = false;
                return;
            }
            _ => {}
        }
        let head = trimmed
            .split_whitespace()
            .next()
            .unwrap_or("")
            .to_ascii_lowercase();
        if head == "writeconfig" || head == "saveconfig" {
            self.write_user_config();
            return;
        }
        let Ok(result) = self.console.exec(line) else {
            return;
        };
        for msg in &result.output {
            self.messages.push(msg.clone(), 70);
        }
        for cmd in result.host {
            match cmd {
                HostCommand::God => {
                    if let Some(game) = self.play.as_mut() {
                        let _ = game.cheat_god(0);
                        // HUD text comes from `World::toggle_god` → drain_messages.
                    }
                }
                HostCommand::Noclip => {
                    if let Some(game) = self.play.as_mut() {
                        let _ = game.cheat_noclip(0);
                    }
                }
                HostCommand::Freeze => {
                    if let Some(game) = self.play.as_mut() {
                        let _ = game.cheat_freeze();
                    }
                }
                HostCommand::Iddt => {
                    if let Some(game) = self.play.as_mut() {
                        let _ = game.cheat_iddt(0);
                        // Opening the map makes the reveal obvious.
                        self.automap = true;
                    }
                }
                HostCommand::Idmypos => {
                    if let Some(game) = self.play.as_mut() {
                        let _ = game.cheat_idmypos(0);
                    }
                }
                HostCommand::Look => {
                    if let Some(game) = self.play.as_mut() {
                        if let Some(world) = game.world_mut() {
                            world.push_view_debug("look");
                            for line in world.drain_debug() {
                                eprintln!("room: {line}");
                                self.console.print(line);
                            }
                        }
                    }
                }
                HostCommand::Pause => self.toggle_pause(),
                HostCommand::Screenshot => self.request_screenshot(),
                HostCommand::GifToggle => self.toggle_gif_record(),
                HostCommand::Summon(class) => {
                    if let Some(game) = self.play.as_mut() {
                        if game.cheat_summon(0, &class) {
                            self.messages.push(format!("Summoned {class}"), 70);
                        } else {
                            self.messages.push(format!("summon: unknown '{class}'"), 70);
                        }
                    }
                }
                HostCommand::Give(what) => {
                    if let Some(game) = self.play.as_mut() {
                        let n = game.cheat_give(0, &what);
                        self.messages.push(format!("Gave {what} ({n})"), 70);
                    }
                }
                HostCommand::Map(name) => self.console_change_map(&name),
                HostCommand::ChangeMap {
                    map,
                    keep_inventory,
                } => {
                    let target =
                        map.or_else(|| self.play.as_ref().and_then(|g| g.console_next_map()));
                    match target {
                        Some(name) => self.console_change_map_ex(&name, keep_inventory),
                        None => self.messages.push("changemap: no next map", 70),
                    }
                }
                HostCommand::DoubleBind { key, command } => match command {
                    Some(cmd) => self.double_binds.bind(&key, &cmd),
                    None => {
                        let shown = self.double_binds.get(&key).unwrap_or("(none)").to_owned();
                        self.messages.push(format!("{key}: {shown}"), 70);
                    }
                },
                HostCommand::ChangeEpisode(n) => {
                    match self.play.as_ref().and_then(|g| g.episode_start_map(n)) {
                        Some(name) => self.console_change_map(&name),
                        None => self
                            .messages
                            .push(format!("changeepisode: no episode {n}"), 70),
                    }
                }
                HostCommand::Save(slot) => self.save_play_save(slot.as_deref()),
                HostCommand::Load(slot) => self.load_play_save(slot.as_deref()),
                HostCommand::Record(slot) => self.begin_play_demo(slot.as_deref()),
                HostCommand::StopDemo => self.stop_play_demo(),
                HostCommand::PlayDemo(slot) => self.play_play_demo(slot.as_deref()),
                HostCommand::PlayAtomicDemo(path) => self.play_atomic_duke_demo(&path),
                HostCommand::Kill(target) => {
                    let kind = target.as_deref();
                    if kind.is_none_or(|t| t.eq_ignore_ascii_case("monsters")) {
                        if let Some(game) = self.play.as_mut() {
                            let n = game.cheat_kill_monsters();
                            self.messages.push(format!("Killed {n} monsters"), 70);
                        }
                    } else if kind.is_some_and(|t| t.eq_ignore_ascii_case("self")) {
                        if let Some(game) = self.play.as_mut() {
                            if game.cheat_kill_self(0) {
                                self.messages.push("Killed self", 70);
                            }
                        }
                    } else if let Some(t) = kind {
                        self.messages
                            .push(format!("kill: unknown target '{t}'"), 70);
                    }
                }
                HostCommand::Puke { script, args } => {
                    if let Some(game) = self.play.as_mut() {
                        let ok = game.cheat_puke(script, args, false);
                        self.messages.push(
                            if ok != 0 {
                                format!("Puked script {script}")
                            } else {
                                format!("Puke {script}: not started")
                            },
                            70,
                        );
                    }
                }
                HostCommand::PukeName { name, always, args } => {
                    if let Some(game) = self.play.as_mut() {
                        let ok = game.cheat_puke_named(&name, args, always);
                        self.messages.push(
                            if ok != 0 {
                                format!("Puked {name}")
                            } else {
                                format!("PukeName {name}: not started")
                            },
                            70,
                        );
                    }
                }
            }
        }
    }

    fn load_duke_sav(&mut self, h: room_build::SavHeader) {
        let name = h.map_name();
        let label = if h.name.is_empty() {
            name.clone()
        } else {
            format!("{} → {name}", h.name)
        };
        if let Some(session) = self.duke_session.as_mut() {
            match session.restore_atomic_sav(&h) {
                Ok(()) => {
                    self.menu = None;
                    self.console_open = false;
                    self.messages
                        .push(format!("Restored Atomic Duke save {label}"), 70);
                    self.start_level_music();
                }
                Err(e) => self.messages.push(format!("load {label}: {e}"), 70),
            }
            return;
        }
        let Some(vfs) = self.vfs.as_ref() else {
            self.messages
                .push(format!("load {label}: no IWAD mounted"), 70);
            return;
        };
        let mut next_map = match load_map(vfs, &name) {
            Ok(m) => m,
            Err(e) => {
                self.messages.push(format!("load {label}: {e}"), 70);
                return;
            }
        };
        ensure_nodes(&mut next_map);
        self.map_base = next_map.clone();
        self.map = next_map.clone();
        let Some(game) = self.play.as_mut() else {
            self.messages
                .push(format!("load {label}: not in play mode"), 70);
            return;
        };
        if let Some(seed) = h.room_rng_seed() {
            game.options.rng_seed = seed;
        }
        match game.start_custom_map(next_map, Skill::new(h.room_skill())) {
            Ok(()) => {
                if let Some(t) = h.room_lock_tics() {
                    if let Some(w) = game.world_mut() {
                        w.level.time = t;
                    }
                }
                if let Some(t) = h.room_quake_tics() {
                    if let Some(w) = game.world_mut() {
                        if let Ok(n) = i32::try_from(t) {
                            w.start_duke_quake(n);
                        }
                    }
                }
                if let Some(r) = h.room_respawn_monsters() {
                    if let Some(w) = game.world_mut() {
                        w.respawn_monsters = r;
                    }
                }
                if let Some(r) = h.room_respawn_items() {
                    if let Some(w) = game.world_mut() {
                        w.respawn_items = r;
                    }
                }
                if let Some(r) = h.room_respawn_inventory() {
                    if let Some(w) = game.world_mut() {
                        w.respawn_inventory = r;
                    }
                }
                if h.room_monsters_off() == Some(true) {
                    if let Some(w) = game.world_mut() {
                        w.monsters_off = true;
                        let _ = w.kill_monsters();
                    }
                }
                if let Some(r) = h.room_from_bonus() {
                    if let Some(w) = game.world_mut() {
                        w.from_bonus = r;
                    }
                }
                if let Some(n) = h.room_secretlevel() {
                    if let Some(w) = game.world_mut() {
                        w.secretlevel = n;
                    }
                }
                if let Some(r) = h.room_eog() {
                    if let Some(w) = game.world_mut() {
                        w.eog = r;
                    }
                }
                if let Some(n) = h.room_global_random() {
                    if let Some(w) = game.world_mut() {
                        w.global_random = n;
                    }
                }
                if let Some(r) = h.room_coop() {
                    if let Some(w) = game.world_mut() {
                        w.coop = r;
                    }
                }
                if let Some(r) = h.room_ffire() {
                    if let Some(w) = game.world_mut() {
                        w.friendly_fire = r;
                    }
                }
                if let Some(r) = h.room_marker() {
                    if let Some(w) = game.world_mut() {
                        w.marker = r;
                    }
                }
                if let Some(n) = h.room_last_level() {
                    if let Some(w) = game.world_mut() {
                        w.last_level = n;
                    }
                }
                if let Some(n) = h.room_camsprite() {
                    if let Some(w) = game.world_mut() {
                        w.camsprite = n;
                        if n >= 0 {
                            w.set_demo_cameras(true);
                        }
                    }
                }
                if let Some(n) = h.room_pskybits() {
                    if let Some(w) = game.world_mut() {
                        w.pskybits = n;
                    }
                }
                if let Some(off) = h.room_pskyoff() {
                    if let Some(w) = game.world_mut() {
                        w.pskyoff = off;
                    }
                }
                if let Some(n) = h.room_parallaxyscale() {
                    if let Some(w) = game.world_mut() {
                        w.parallaxyscale = n;
                    }
                }
                if let Some(n) = h.room_connecthead() {
                    if let Some(w) = game.world_mut() {
                        w.connecthead = n;
                    }
                }
                if let Some(links) = h.room_connectpoint2() {
                    if let Some(w) = game.world_mut() {
                        w.connectpoint2 = links;
                    }
                }
                if let Some(n) = h.room_numplayersprites() {
                    if let Some(w) = game.world_mut() {
                        w.numplayersprites = n;
                    }
                }
                if let Some(table) = h.room_frags() {
                    if let Some(w) = game.world_mut() {
                        w.duke_frags = table;
                        if let Some(p) = w.players[0].as_mut() {
                            for (i, dest) in p.frags.iter_mut().enumerate() {
                                if let Some(row) = table.first() {
                                    if let Some(n) = row.get(i) {
                                        *dest = i32::from(*n);
                                    }
                                }
                            }
                        }
                    }
                }
                if let Some(n) = h.room_animatecnt() {
                    if let Some(w) = game.world_mut() {
                        w.animatecnt = n;
                    }
                }
                if let Some(v) = h.animategoal.as_ref() {
                    if let Some(w) = game.world_mut() {
                        w.animategoal = v.clone();
                    }
                }
                if let Some(v) = h.animatevel.as_ref() {
                    if let Some(w) = game.world_mut() {
                        w.animatevel = v.clone();
                    }
                }
                if let Some(v) = h.animateptr.as_ref() {
                    if let Some(w) = game.world_mut() {
                        w.animateptr = v.clone();
                    }
                }
                if let Some(n) = h.room_numanimwalls() {
                    if let Some(w) = game.world_mut() {
                        w.numanimwalls = n;
                    }
                }
                if let Some(n) = h.room_spriteqamount() {
                    if let Some(w) = game.world_mut() {
                        w.spriteqamount = n;
                    }
                }
                if let Some(n) = h.room_numclouds() {
                    if let Some(w) = game.world_mut() {
                        w.numclouds = n;
                    }
                }
                if let Some(n) = h.room_numcyclers() {
                    if let Some(w) = game.world_mut() {
                        w.numcyclers = n;
                    }
                }
                if let Some(bits) = h.room_gotpic() {
                    if let Some(w) = game.world_mut() {
                        w.apply_duke_gotpic(bits);
                    }
                }
                if let Some(n) = h.room_playerswhenstarted() {
                    if let Some(w) = game.world_mut() {
                        w.playerswhenstarted = n;
                        if n >= 2 {
                            let seats = u8::try_from(n).unwrap_or(4).clamp(2, 4);
                            let _ = w.spawn_coop_players(seats);
                        }
                    }
                }
                if let Some(g) = h.room_god() {
                    if let Some(w) = game.world_mut() {
                        if let Some(p) = w.players[0].as_mut() {
                            p.god = g;
                        }
                    }
                }
                if let Some(p) = h.player.as_ref() {
                    if let Some(w) = game.world_mut() {
                        let (x, y) = p.room_xy();
                        if let Some(n) = p.room_prev_pitch() {
                            w.set_player_prev_pitch(n);
                        }
                        if let Some(n) = p.room_footprint_shade() {
                            w.set_player_footprint_shade(n);
                        }
                        if let Some(n) = p.room_footprint_count() {
                            w.set_player_footprint_count(n);
                        }
                        if let Some(n) = p.room_footprint_pal() {
                            w.set_player_footprint_pal(n);
                        }
                        w.set_player_pose(
                            f64::from(x),
                            f64::from(y),
                            f64::from(p.room_z()),
                            f64::from(p.room_yaw()),
                            f64::from(p.room_pitch()),
                        );
                        if let Some(n) = p.room_weapon_ang_deg() {
                            w.add_player_yaw(f64::from(n));
                        }
                        if let Some(n) = p.room_rotscrnang() {
                            w.set_player_rotscrnang(n);
                        }
                        if let Some(n) = p.room_return_to_center() {
                            w.set_player_return_to_center(n);
                        }
                        if let Some(n) = p.room_tip_incs() {
                            w.set_player_tip_incs(n);
                        }
                        if let Some(on) = p.room_on_ground() {
                            w.set_player_on_ground(on);
                        }
                        if let Some(n) = p.room_transporter_hold_tics() {
                            w.set_player_transporter_hold(n);
                        }
                        if p.room_jumping() == Some(true) {
                            w.set_player_jumping(true);
                        }
                        if let Some(on) = p.room_jumping_toggle() {
                            w.set_player_jumping_toggle(on);
                        }
                        if let Some(hp) = p.room_health() {
                            w.set_player_health(hp);
                        }
                        if p.room_dead() == Some(true) {
                            w.set_player_dead(true);
                        }
                        if let Some(slot) = p.room_frag_ps() {
                            w.set_player_frag_ps(slot);
                        }
                        if let Some(n) = p.room_inven_icon() {
                            w.set_player_inven_icon(n);
                        }
                        if let Some(on) = p.room_toggle_key_flag() {
                            w.set_player_toggle_key_flag(on);
                        }
                        if p.room_refresh_inventory() == Some(true) {
                            w.set_player_invdisp_tics(35);
                        }
                        if let Some(n) = p.room_invdisp_tics() {
                            w.set_player_invdisp_tics(n);
                        }
                        if p.room_hbomb_on() == Some(true) {
                            w.set_player_hbomb_on(true);
                        }
                        if let Some(n) = p.room_hbomb_hold() {
                            w.set_player_hbomb_hold(n);
                        }
                        if let Some(n) = p.room_clip_ammo() {
                            w.set_player_clip_ammo(n);
                        }
                        if let Some(n) = p.room_shell_ammo() {
                            w.set_player_shell_ammo(n);
                        }
                        if let Some(n) = p.room_rocket_ammo() {
                            w.set_player_rocket_ammo(n);
                        }
                        if let Some(n) = p.room_cell_ammo() {
                            w.set_player_cell_ammo(n);
                        }
                        if let Some(n) = p.room_pipebomb_ammo() {
                            w.set_player_pipebomb_ammo(n);
                        }
                        if let Some(n) = p.room_devastator_ammo() {
                            w.set_player_devastator_ammo(n);
                        }
                        if let Some(n) = p.room_freeze_ammo() {
                            w.set_player_freeze_ammo(n);
                        }
                        if let Some(n) = p.room_tripbomb_ammo() {
                            w.set_player_tripbomb_ammo(n);
                        }
                        if let Some(n) = p.room_grow_ammo() {
                            w.set_player_grow_ammo(n);
                        }
                        if let Some(n) = p.room_armor() {
                            w.set_player_armor(n);
                        }
                        if let Some(n) = p.room_firstaid() {
                            w.set_player_firstaid(n);
                        }
                        if let Some(n) = p.room_steroids() {
                            w.set_player_steroids(n);
                        }
                        if let Some(n) = p.room_jetpack() {
                            w.set_player_jetpack(n);
                        }
                        if let Some(n) = p.room_scuba() {
                            w.set_player_scuba(n);
                        }
                        if let Some(n) = p.room_holoduke() {
                            w.set_player_holoduke(n);
                        }
                        if let Some(n) = p.room_boots() {
                            w.set_player_boots(n);
                        }
                        if let Some(n) = p.room_heat() {
                            w.set_player_heat(n);
                        }
                        if let Some(n) = p.room_got_access() {
                            w.set_player_got_access(n);
                        }
                        if p.room_holoduke_on() == Some(true) {
                            w.spawn_player_holoduke_decoy();
                        }
                        if let Some(on) = p.room_jetpack_on() {
                            w.set_player_jetpack_on(on);
                        }
                        if let Some(on) = p.room_scuba_on() {
                            w.set_player_scuba_on(on);
                        }
                        if let Some(n) = p.room_air_tics() {
                            w.set_player_air_tics(n);
                        }
                        if let Some(on) = p.room_heat_on() {
                            w.set_player_heat_on(on);
                        }
                        if let Some(n) = p.room_falling_tics() {
                            w.set_player_falling_tics(n);
                        }
                        if let Some(n) = p.room_hard_landing() {
                            w.set_player_hard_landing(n);
                        }
                        if p.room_knuckle_incs() == Some(true) {
                            w.set_player_knuckle_incs(true);
                        }
                        if let Some(n) = p.room_palookup() {
                            w.set_player_palookup(n);
                        }
                        if let Some(on) = p.room_walking_snd_toggle() {
                            w.set_player_walking_snd_toggle(on);
                        }
                        if let Some(n) = p.room_pycount() {
                            w.set_player_pycount(n);
                        }
                        if let Some(n) = p.room_pals_flash() {
                            w.set_player_pals_flash(n);
                        }
                        if let Some(n) = p.room_hurt_delay_tics() {
                            w.set_player_hurt_delay(n);
                        }
                        if let Some(i) = p.room_newowner() {
                            if let Some(sp) = h.sprites.as_ref().and_then(|ss| ss.get(i)) {
                                let (x, y) = sp.room_xy();
                                w.look_through_duke_sprite_at(f64::from(x), f64::from(y));
                            }
                        }
                        if let Some(got) = p.room_gotweapon() {
                            w.give_player_gotweapons(&got);
                        }
                        if let Some(slot) = p.room_last_weapon_slot() {
                            w.give_player_weapon_slot(slot);
                        }
                        if let Some(slot) = p.room_last_full_weapon_slot() {
                            w.give_player_weapon_slot(slot);
                        }
                        if let Some(slot) = p.room_weapon_slot() {
                            w.set_player_weapon_slot(slot);
                        }
                        if let Some(slot) = p.room_wantweapon_fire() {
                            w.set_player_wantweapon_fire(slot);
                        }
                        if let Some(n) = p.room_kickback_pic() {
                            w.set_player_kickback(n);
                        } else if p.room_show_empty_weapon() == Some(true) {
                            w.set_player_show_empty_weapon(true);
                        }
                        if p.room_rapid_fire_hold() == Some(true) {
                            w.set_player_rapid_fire_hold(true);
                        }
                        if let Some(n) = p.room_knee_incs() {
                            w.set_player_knee_incs(n);
                        }
                        if p.room_access_incs() == Some(true) {
                            if let Some(i) = p.room_access_wallnum() {
                                if let Some(wall) = h.walls.as_ref().and_then(|ws| ws.get(i)) {
                                    let (x, y) = wall.room_xy();
                                    w.use_duke_wall_at(f64::from(x), f64::from(y));
                                } else {
                                    w.set_player_access_incs(true);
                                }
                            } else if let Some(i) = p.room_access_spritenum() {
                                if let Some(sp) = h.sprites.as_ref().and_then(|ss| ss.get(i)) {
                                    let (x, y) = sp.room_xy();
                                    w.use_duke_sprite_at(f64::from(x), f64::from(y));
                                } else {
                                    w.set_player_access_incs(true);
                                }
                            } else {
                                w.set_player_access_incs(true);
                            }
                        }
                        if let (Some(tics), Some(q)) = (p.room_fta_tics(), p.room_ftq()) {
                            w.set_player_fta_quote(tics, q);
                        }
                        if let Some(y) = p.room_pspr_y() {
                            w.set_player_pspr_y(y);
                        }
                        if p.room_quick_kick() == Some(true) {
                            w.player_quick_kick();
                        }
                        if p.room_holster_weapon() == Some(true) {
                            w.set_player_holster(true);
                        }
                    }
                }
                if let Some(secs) = h.sectors.as_ref() {
                    if let Some(w) = game.world_mut() {
                        let zs: Vec<(f64, f64)> = secs
                            .iter()
                            .map(|s| (f64::from(s.room_floor_z()), f64::from(s.room_ceiling_z())))
                            .collect();
                        w.apply_duke_sector_zs(&zs);
                        let pics: Vec<(String, String)> = secs
                            .iter()
                            .map(|s| (s.room_floor_pic(), s.room_ceiling_pic()))
                            .collect();
                        w.apply_duke_sector_pics(&pics);
                        let lights: Vec<i16> = secs.iter().map(|s| s.room_light()).collect();
                        w.apply_duke_sector_lights(&lights);
                        let fogs: Vec<u8> = secs.iter().map(|s| s.room_fog_density()).collect();
                        w.apply_duke_sector_fogs(&fogs);
                    }
                }
                if let Some(sprites) = h.sprites.as_ref() {
                    if let Some(w) = game.world_mut() {
                        let xys: Vec<(f64, f64)> = sprites
                            .iter()
                            .filter(|s| s.extra > 0)
                            .map(|s| {
                                let (x, y) = s.room_xy();
                                (f64::from(x), f64::from(y))
                            })
                            .collect();
                        w.apply_duke_sprite_xy(&xys);
                        let hits: Vec<(f64, f64, i32)> = sprites
                            .iter()
                            .filter(|s| s.extra > 0)
                            .map(|s| {
                                let (x, y) = s.room_xy();
                                (f64::from(x), f64::from(y), i32::from(s.extra))
                            })
                            .collect();
                        w.apply_duke_sprite_health(&hits);
                        let looks: Vec<(f64, f64, i32)> = sprites
                            .iter()
                            .map(|s| {
                                let (x, y) = s.room_xy();
                                (f64::from(x), f64::from(y), i32::from(s.cstat))
                            })
                            .collect();
                        w.apply_duke_sprite_cstat(&looks);
                        let scales: Vec<(f64, f64, u8, u8, u8)> = sprites
                            .iter()
                            .map(|s| {
                                let (x, y) = s.room_xy();
                                (f64::from(x), f64::from(y), s.xrepeat, s.yrepeat, s.clipdist)
                            })
                            .collect();
                        w.apply_duke_sprite_scale(&scales);
                        let pals: Vec<(f64, f64, u8)> = sprites
                            .iter()
                            .map(|s| {
                                let (x, y) = s.room_xy();
                                (f64::from(x), f64::from(y), s.pal)
                            })
                            .collect();
                        w.apply_duke_sprite_pal(&pals);
                        let angs: Vec<(f64, f64, f64)> = sprites
                            .iter()
                            .map(|s| {
                                let (x, y) = s.room_xy();
                                (f64::from(x), f64::from(y), f64::from(s.room_yaw()))
                            })
                            .collect();
                        w.apply_duke_sprite_ang(&angs);
                        let zs: Vec<(f64, f64, f64)> = sprites
                            .iter()
                            .map(|s| {
                                let (x, y) = s.room_xy();
                                (f64::from(x), f64::from(y), f64::from(s.room_z()))
                            })
                            .collect();
                        w.apply_duke_sprite_z(&zs);
                    }
                }
                if let Some(walls) = h.walls.as_ref() {
                    if let Some(w) = game.world_mut() {
                        let hits: Vec<(f64, f64, i32)> = walls
                            .iter()
                            .filter_map(|wall| {
                                let hp = if wall.cstat & 1 == 0 {
                                    0
                                } else if wall.extra > 0 {
                                    i32::from(wall.extra)
                                } else {
                                    return None;
                                };
                                let (x, y) = wall.room_xy();
                                Some((f64::from(x), f64::from(y), hp))
                            })
                            .collect();
                        w.apply_duke_wall_health(&hits);
                        let pics: Vec<(f64, f64, String)> = walls
                            .iter()
                            .map(|wall| {
                                let (x, y) = wall.room_xy();
                                (f64::from(x), f64::from(y), wall.room_mid_pic())
                            })
                            .collect();
                        w.apply_duke_wall_pics(&pics);
                    }
                }
                if let Some(r) = h.room_auto_run() {
                    let _ = self.console.cvars.set("cl_run", if r { "1" } else { "0" });
                }
                if let Some(r) = h.room_crosshair() {
                    let _ = self
                        .console
                        .cvars
                        .set("cl_crosshair", if r { "1" } else { "0" });
                }
                self.reset_impact_fx();
                self.refresh_sky_texture();
                self.menu = None;
                self.set_play_look(self.mouse_look_enabled());
                self.start_level_music();
                self.messages
                    .push(format!("Load Game {label} (use .fsave to restore)"), 70);
            }
            Err(e) => self.messages.push(format!("load {label}: {e}"), 70),
        }
    }

    fn console_change_map(&mut self, name: &str) {
        self.console_change_map_ex(name, false);
    }

    /// `map` (fresh start) / `changemap` (`keep_inventory`: level exit travel).
    fn console_change_map_ex(&mut self, name: &str, keep_inventory: bool) {
        let Some(vfs) = self.vfs.as_ref() else {
            self.messages.push("map: no IWAD mounted", 70);
            return;
        };
        let mut next_map = match load_map(vfs, name) {
            Ok(m) => m,
            Err(e) => {
                self.messages.push(format!("map {name}: {e}"), 70);
                return;
            }
        };
        ensure_nodes(&mut next_map);
        self.map_base = next_map.clone();
        self.map = next_map.clone();
        let Some(game) = self.play.as_mut() else {
            self.messages.push("map: not in play mode", 70);
            return;
        };
        let skill = game.options.skill;
        let started = if keep_inventory {
            game.change_map(next_map, true)
        } else {
            game.start_custom_map(next_map, skill)
        };
        match started {
            Ok(()) => {
                self.reset_impact_fx();
                self.menu = None;
                self.console_open = false;
                self.messages.push(format!("Entering {name}"), 35);
                self.start_level_music();
            }
            Err(e) => self.messages.push(format!("map {name}: {e}"), 70),
        }
    }

    fn save_duke_slot(&mut self, slot: usize) {
        let Some(session) = self.duke_session.as_ref() else {
            self.messages.push("save: no native Duke session", 70);
            return;
        };
        let bytes = match session.save_fsave() {
            Ok(bytes) => bytes,
            Err(error) => {
                self.messages.push(format!("save: {error}"), 70);
                return;
            }
        };
        let atomic_bytes = session.save_atomic_sav();
        let path = dirs_room_save(&format!("duke-{slot}"));
        let atomic_path = path.with_extension("sav");
        if let Some(directory) = path.parent()
            && let Err(error) = std::fs::create_dir_all(directory)
        {
            self.messages.push(format!("save: {error}"), 70);
            return;
        }
        match std::fs::write(&path, bytes) {
            Ok(()) => {
                let atomic_notice = match atomic_bytes {
                    Ok(bytes) => match std::fs::write(&atomic_path, bytes) {
                        Ok(()) => None,
                        Err(error) => {
                            let _ = std::fs::remove_file(&atomic_path);
                            Some(format!("; Atomic companion save failed: {error}"))
                        }
                    },
                    Err(room_duke::DukeSaveError::MissingAtomicTemplate) => {
                        let _ = std::fs::remove_file(&atomic_path);
                        None
                    }
                    Err(error) => {
                        let _ = std::fs::remove_file(&atomic_path);
                        Some(format!("; Atomic companion save skipped: {error}"))
                    }
                };
                self.duke_last_save_slot = Some(slot);
                self.messages.push(
                    format!(
                        "Saved Duke slot {}{}",
                        slot + 1,
                        atomic_notice.unwrap_or_default()
                    ),
                    70,
                );
                self.close_duke_menu();
            }
            Err(error) => self.messages.push(format!("save: {error}"), 70),
        }
    }

    fn load_duke_slot(&mut self, slot: usize) {
        let path = dirs_room_save(&format!("duke-{slot}"));
        let bytes = match std::fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) => {
                self.messages.push(format!("load: {error}"), 70);
                return;
            }
        };
        let Some(session) = self.duke_session.as_mut() else {
            self.messages.push("load: no native Duke session", 70);
            return;
        };
        match session.load_fsave(&bytes) {
            Ok(()) => {
                let atomic_restore = std::fs::read(path.with_extension("sav")).ok().map(|bytes| {
                    room_build::SavHeader::parse(&bytes)
                        .map_err(|error| error.to_string())
                        .and_then(|save| session.restore_atomic_sav(&save))
                });
                self.duke_last_save_slot = Some(slot);
                if let Some(renderer) = self.duke_renderer.as_ref() {
                    renderer.invalidate_camera_feed();
                }
                let message = match atomic_restore {
                    Some(Err(error)) => format!(
                        "Loaded Duke slot {} (.fsave); Atomic companion restore failed: {error}",
                        slot + 1
                    ),
                    Some(Ok(())) => format!("Loaded Duke slot {} (.fsave + Atomic)", slot + 1),
                    None => format!("Loaded Duke slot {}", slot + 1),
                };
                self.messages.push(message, 70);
                self.duke_environment_music_active = false;
                self.menu = None;
                self.set_play_look(true);
                self.start_duke_level_music();
            }
            Err(error) => self.messages.push(format!("load: {error}"), 70),
        }
    }

    /// Netplay save / load go through the lockstep: only the host may ask,
    /// and every peer then saves (loads) its own `net<N>` copy on the same tic.
    /// Returns true when the request was handled here.
    fn route_net_save(&mut self, slot: Option<&str>, load: bool) -> bool {
        let Some(net) = self.net.as_ref() else {
            return false;
        };
        if net.local.0 != 0 {
            let what = if load { "load" } else { "save" };
            self.messages
                .push(format!("Only the host can {what} a net game"), 70);
            return true;
        }
        // `net3` or `3` → slot 3; anything else is slot 0.
        let n = slot
            .map(|s| s.trim().trim_start_matches("net"))
            .and_then(|s| s.parse::<u8>().ok())
            .unwrap_or(0);
        self.net_save_cmd = Some((load, n));
        true
    }

    fn save_play_save(&mut self, slot: Option<&str>) {
        if self.route_net_save(slot, false) {
            return;
        }
        self.save_play_save_local(slot);
    }

    fn save_play_save_local(&mut self, slot: Option<&str>) {
        let slot = save_slot_name(slot);
        let Some(game) = self.play.as_mut() else {
            self.messages.push("save: not in play mode", 70);
            return;
        };
        // Looping voices go into the save so the load can restart them.
        let loops = self
            .audio
            .as_ref()
            .and_then(|a| a.shared.lock().ok().map(|m| m.mixer.loops_for_save()))
            .unwrap_or_default();
        let saved = game
            .sounds
            .as_ref()
            .map(|s| s.save_voices(&loops))
            .unwrap_or_default();
        match game.save_game_with_sounds(&saved) {
            Ok(bytes) => {
                let path = dirs_room_save(&slot);
                if let Some(dir) = path.parent() {
                    if let Err(e) = std::fs::create_dir_all(dir) {
                        self.messages.push(format!("save: {e}"), 70);
                        return;
                    }
                }
                match std::fs::write(&path, bytes) {
                    Ok(()) => {
                        self.messages.push(format!("Saved {}", path.display()), 70);
                    }
                    Err(e) => self.messages.push(format!("save: {e}"), 70),
                }
            }
            Err(e) => self.messages.push(format!("save: {e}"), 70),
        }
    }

    fn load_play_save(&mut self, slot: Option<&str>) {
        if self.route_net_save(slot, true) {
            return;
        }
        self.load_play_save_local(slot);
    }

    fn load_play_save_local(&mut self, slot: Option<&str>) {
        let slot = save_slot_name(slot);
        let (path, bytes) = match read_play_save_bytes(&slot) {
            Ok(v) => v,
            Err(e) => {
                self.messages.push(format!("load {slot}: {e}"), 70);
                return;
            }
        };
        if let Ok(room_build::LoadedSave::Duke(h)) = room_build::identify_save(&bytes) {
            self.load_duke_sav(h);
            return;
        }
        let info = match read_save_info(&bytes) {
            Ok(i) => i,
            Err(e) => {
                self.messages
                    .push(format!("load {}: {e}", path.display()), 70);
                return;
            }
        };
        if self.play.is_none() {
            self.messages.push("load: not in play mode", 70);
            return;
        }
        let need_map = self
            .play
            .as_ref()
            .is_some_and(|g| g.state != GameState::Level || g.map_name() != info.map);
        if need_map {
            let Some(vfs) = self.vfs.as_ref() else {
                self.messages.push("load: no IWAD mounted", 70);
                return;
            };
            let mut next_map = match load_map(vfs, &info.map) {
                Ok(m) => m,
                Err(e) => {
                    self.messages.push(format!("load {}: {e}", info.map), 70);
                    return;
                }
            };
            ensure_nodes(&mut next_map);
            self.map_base = next_map.clone();
            self.map = next_map.clone();
            let skill = Skill::new(info.skill);
            if let Some(game) = self.play.as_mut() {
                if let Err(e) = game.start_custom_map(next_map, skill) {
                    self.messages.push(format!("load {}: {e}", info.map), 70);
                    return;
                }
            }
        }
        let Some(game) = self.play.as_mut() else {
            return;
        };
        match game.load_game(&bytes) {
            Ok(()) => {
                // ZDoom `S_RestoreSounds`: drop the old world's voices,
                // restart the saved loops.
                let restored = game.take_restored_sounds();
                let events = match (game.sounds.as_mut(), self.vfs.as_ref()) {
                    (Some(sounds), Some(vfs)) => sounds.restore_events(vfs, &restored),
                    _ => Vec::new(),
                };
                if let Some(audio) = self.audio.as_ref()
                    && let Ok(mut shared) = audio.shared.lock()
                {
                    shared.mixer.stop_world_sounds();
                    if let Some(sounds) = game.sounds.as_ref() {
                        shared.mixer.bank.merge_missing(&sounds.bank);
                    }
                    shared.mixer.apply(&events);
                }
                self.reset_impact_fx();
                self.refresh_sky_texture();
                self.menu = None;
                self.set_play_look(self.mouse_look_enabled());
                self.start_level_music();
                self.messages.push(format!("Loaded {}", info.map), 70);
            }
            Err(e) => self.messages.push(format!("load: {e}"), 70),
        }
    }

    fn begin_play_demo(&mut self, slot: Option<&str>) {
        self.demo_slot = save_slot_name(slot);
        if self.duke_session.is_some() {
            let Some(session) = self.duke_session.as_ref() else {
                return;
            };
            let recorder = match room_duke::DukeDemoRecorder::new(
                session,
                self.duke_demo_map.0,
                self.duke_demo_map.1,
                session.skill,
            ) {
                Ok(recorder) => recorder,
                Err(error) => {
                    self.messages.push(format!("record: {error}"), 70);
                    return;
                }
            };
            self.duke_demo_playback = None;
            self.duke_atomic_demo_playback = None;
            let atomic_recorder = match room_duke::DukeAtomicDemoRecorder::new_multiplayer(
                session,
                self.duke_demo_map.0,
                self.duke_demo_map.1,
                session.skill,
                session.player_count(),
            ) {
                Ok(recorder) => recorder,
                Err(error) => {
                    self.messages.push(format!("record DMO: {error}"), 70);
                    return;
                }
            };
            self.duke_demo_recorder = Some(recorder);
            self.duke_atomic_demo_recorder = Some(atomic_recorder);
            self.messages.push(
                format!("Recording Duke {} (.fdem + .dmo)", self.demo_slot),
                70,
            );
            return;
        }
        let Some(game) = self.play.as_mut() else {
            self.messages.push("record: not in play mode", 70);
            return;
        };
        game.begin_demo_record();
        self.messages
            .push(format!("Recording {}", self.demo_slot), 70);
    }

    fn stop_play_demo(&mut self) {
        if self.duke_demo_playback.take().is_some() {
            self.messages.push("Stopped Duke demo playback", 70);
            return;
        }
        if self.duke_atomic_demo_playback.take().is_some() {
            self.messages.push("Stopped Atomic Duke demo playback", 70);
            return;
        }
        if let Some(demo) = self.duke_demo_recorder.take() {
            let atomic_demo = self.duke_atomic_demo_recorder.take();
            let path = dirs_room_demo(&self.demo_slot);
            if let Some(dir) = path.parent()
                && let Err(error) = std::fs::create_dir_all(dir)
            {
                self.messages.push(format!("stop: {error}"), 70);
                return;
            }
            match demo.to_fdem().and_then(|bytes| {
                std::fs::write(&path, bytes)
                    .map_err(|error| room_duke::DukeDemoError::Invalid(error.to_string()))
            }) {
                Ok(()) => self.messages.push(
                    format!("Wrote {} ({} tics)", path.display(), demo.len()),
                    70,
                ),
                Err(error) => self.messages.push(format!("stop: {error}"), 70),
            }
            if let Some(atomic_demo) = atomic_demo {
                let atomic_path = path.with_extension("dmo");
                if let Some(dir) = atomic_path.parent()
                    && let Err(error) = std::fs::create_dir_all(dir)
                {
                    self.messages.push(format!("stop DMO: {error}"), 70);
                    return;
                }
                match atomic_demo.to_dmo().and_then(|bytes| {
                    std::fs::write(&atomic_path, bytes)
                        .map_err(|error| room_duke::DukeDemoError::Invalid(error.to_string()))
                }) {
                    Ok(()) => self.messages.push(
                        format!(
                            "Wrote {} ({} tics)",
                            atomic_path.display(),
                            atomic_demo.len()
                        ),
                        70,
                    ),
                    Err(error) => self.messages.push(format!("stop DMO: {error}"), 70),
                }
            }
            return;
        }
        let Some(game) = self.play.as_mut() else {
            self.messages.push("stop: not in play mode", 70);
            return;
        };
        let Some(demo) = game.take_demo() else {
            self.messages.push("stop: not recording", 70);
            return;
        };
        game.options.record_demo = false;
        let path = dirs_room_demo(&self.demo_slot);
        if let Some(dir) = path.parent() {
            if let Err(e) = std::fs::create_dir_all(dir) {
                self.messages.push(format!("stop: {e}"), 70);
                return;
            }
        }
        match std::fs::write(&path, demo.to_fdem()) {
            Ok(()) => self.messages.push(
                format!("Wrote {} ({} tics)", path.display(), demo.tics.len()),
                70,
            ),
            Err(e) => self.messages.push(format!("stop: {e}"), 70),
        }
    }

    fn play_play_demo(&mut self, slot: Option<&str>) {
        let slot = save_slot_name(slot);
        let path = dirs_room_demo(&slot);
        let bytes = match std::fs::read(&path) {
            Ok(b) => b,
            Err(e) => {
                self.messages
                    .push(format!("playdemo {}: {e}", path.display()), 70);
                return;
            }
        };
        if self.duke_session.is_some() {
            let recorder = match room_duke::DukeDemoRecorder::from_fdem(&bytes) {
                Ok(recorder) => recorder,
                Err(error) => {
                    self.messages.push(format!("playdemo: {error}"), 70);
                    return;
                }
            };
            if recorder.is_empty() {
                self.messages.push("playdemo: recording is empty", 70);
                return;
            }
            self.duke_demo_recorder = None;
            self.duke_atomic_demo_recorder = None;
            self.duke_atomic_demo_playback = None;
            self.duke_demo_playback = Some(recorder);
            self.messages.push(format!("Playing Duke demo {slot}"), 70);
            return;
        }
        let rec = match DemoRecorder::from_fdem(&bytes) {
            Ok(r) => r,
            Err(e) => {
                self.messages.push(format!("playdemo: {e}"), 70);
                return;
            }
        };
        let Some(vfs) = self.vfs.as_ref() else {
            self.messages.push("playdemo: no IWAD mounted", 70);
            return;
        };
        let mut next_map = match load_map(vfs, &rec.map) {
            Ok(m) => m,
            Err(e) => {
                self.messages.push(format!("playdemo {}: {e}", rec.map), 70);
                return;
            }
        };
        ensure_nodes(&mut next_map);
        self.map_base = next_map.clone();
        self.map = next_map.clone();
        let skill = Skill::new(rec.skill);
        let Some(game) = self.play.as_mut() else {
            self.messages.push("playdemo: not in play mode", 70);
            return;
        };
        game.options.record_demo = false;
        if let Err(e) = game.start_custom_map(next_map, skill) {
            self.messages.push(format!("playdemo {}: {e}", rec.map), 70);
            return;
        }
        let Some(world) = game.world_mut() else {
            self.messages.push("playdemo: no world", 70);
            return;
        };
        match (DemoPlayer { demo: &rec }).verify(world) {
            Ok(r) if r.first_mismatch.is_none() => {
                self.reset_impact_fx();
                self.refresh_sky_texture();
                self.menu = None;
                self.set_play_look(self.mouse_look_enabled());
                self.start_level_music();
                self.messages
                    .push(format!("Demo {} ({} tics)", rec.map, r.tics_ok), 70);
            }
            Ok(r) => self.messages.push(
                format!("demo desync at tic {}", r.first_mismatch.unwrap_or(0)),
                70,
            ),
            Err(e) => self.messages.push(format!("playdemo: {e}"), 70),
        }
    }

    fn play_atomic_duke_demo(&mut self, path: &str) {
        let demo_path = PathBuf::from(path);
        let bytes = match std::fs::read(&demo_path) {
            Ok(bytes) => bytes,
            Err(error) => {
                self.messages.push(
                    format!("playatomicdemo {}: {error}", demo_path.display()),
                    70,
                );
                return;
            }
        };
        let playback = match room_duke::DukeAtomicDemoPlayer::from_bytes(&bytes) {
            Ok(playback) => playback,
            Err(error) => {
                self.messages.push(format!("playatomicdemo: {error}"), 70);
                return;
            }
        };
        let Some(content) = self.duke_campaign_content.as_ref() else {
            self.messages
                .push("playatomicdemo: native Duke GRP is not loaded", 70);
            return;
        };
        let layers: Vec<&[u8]> = content.grp_layers.iter().map(Vec::as_slice).collect();
        let mut con_includes = content
            .con_directory
            .as_ref()
            .map(|directory| DukeConIncludes {
                directory: directory.clone(),
            });
        let mut session = match playback.load_session_with_con(
            &layers,
            content.con_override.as_deref(),
            con_includes
                .as_mut()
                .map(|resolver| resolver as &mut dyn room_duke::con::IncludeResolver),
        ) {
            Ok(session) => session,
            Err(error) => {
                self.messages.push(format!("playatomicdemo: {error}"), 70);
                return;
            }
        };
        session.campaign =
            room_duke::DukeCampaignProgress::new(playback.volume(), playback.level());
        let map_name = format!("E{}L{}", playback.volume() + 1, playback.level() + 1);
        if let (Some(renderer), Some(map_bytes)) =
            (self.duke_renderer.as_ref(), content.map_bytes(&map_name))
        {
            if let Err(error) = renderer.load_map_hack_for_map(&map_name, &map_bytes) {
                self.messages
                    .push(format!("Duke map hack skipped: {error}"), 70);
            }
        }
        self.duke_sound_bank = content.sound_bank(session.script.as_ref(), content.sample_rate);
        self.duke_demo_map = (playback.volume(), playback.level());
        self.duke_music_selection =
            usize::from(playback.volume()) * 11 + usize::from(playback.level());
        self.duke_demo_recorder = None;
        self.duke_atomic_demo_recorder = None;
        self.duke_demo_playback = None;
        self.duke_atomic_demo_playback = Some(playback);
        self.duke_start_state = session.save_fsave().ok();
        if let Some(renderer) = self.duke_renderer.as_ref() {
            renderer.invalidate_camera_feed();
        }
        self.duke_session = Some(session);
        self.duke_automap_seen.clear();
        self.duke_show_all_map = false;
        self.cam_prev = None;
        self.cam_curr = None;
        self.menu = None;
        self.set_play_look(self.mouse_look_enabled());
        self.start_duke_level_music();
        self.messages
            .push(format!("Playing Atomic Duke {}", demo_path.display()), 70);
    }

    fn start_level_music(&mut self) {
        let Some(vfs) = self.vfs.as_ref() else {
            return;
        };
        let Some(name) = self.play.as_ref().map(|g| g.map_name().to_owned()) else {
            return;
        };
        if name.is_empty() {
            return;
        }
        let Some(audio) = self.audio.as_ref() else {
            return;
        };
        let Ok(mut shared) = audio.shared.lock() else {
            return;
        };
        let mapinfo = self.play.as_ref().and_then(Headless::mapinfo);
        match play_map_music(&mut shared.music, vfs, &name, mapinfo) {
            Ok(lump) => self.messages.push(format!("Music {lump}"), 35),
            Err(e) => eprintln!("room: music {name}: {e}"),
        }
    }

    fn start_duke_level_music(&mut self) {
        if matches!(
            self.console.cvars.get("duke_music"),
            Some(room_console::CvarValue::Bool(false))
        ) {
            return;
        }
        let Some(content) = self.duke_campaign_content.as_ref() else {
            return;
        };
        let Some(session) = self.duke_session.as_ref() else {
            return;
        };
        let Some(script) = session.script.as_ref() else {
            return;
        };
        let layers: Vec<&[u8]> = content.grp_layers.iter().map(Vec::as_slice).collect();
        let Ok(grp) = room_build::grp::GrpStack::parse_layers(&layers) else {
            return;
        };
        let Some(audio) = self.audio.as_ref() else {
            return;
        };
        let Ok(mut shared) = audio.shared.lock() else {
            return;
        };
        shared.mixer.music_gain = duke_audio_volume(&self.console.cvars, "duke_music_volume");
        if let Some(map_id) = session
            .campaign
            .map(|position| format!("E{}L{}", position.volume + 1, position.level + 1))
            && let Some(track) = content
                .def_music
                .iter()
                .rev()
                .find(|track| track.id.eq_ignore_ascii_case(&map_id))
            && let Some(bytes) = content.music_asset(&grp, track)
        {
            let extension = track
                .file
                .rsplit_once('.')
                .map(|(_, extension)| extension.to_ascii_lowercase());
            let started = match extension.as_deref() {
                Some("mid" | "midi") => {
                    shared
                        .music
                        .play_midi(track.file.clone(), bytes.to_vec(), true);
                    true
                }
                Some("mus") => match shared.music.play_mus(track.file.clone(), bytes, true) {
                    Ok(()) => true,
                    Err(error) => {
                        eprintln!("room: decode Duke DEF music {}: {error}", track.file);
                        false
                    }
                },
                _ => match room_formats::decode_sound(bytes) {
                    Ok(sound) => match shared.music.play_pcm(track.file.clone(), sound, true) {
                        Ok(()) => true,
                        Err(error) => {
                            eprintln!("room: start Duke DEF music {}: {error}", track.file);
                            false
                        }
                    },
                    Err(error) => {
                        eprintln!("room: decode Duke DEF music {}: {error}", track.file);
                        false
                    }
                },
            };
            if started {
                self.messages.push(format!("Music {}", track.file), 35);
                return;
            }
        }
        let Some(name) = script
            .music_tracks
            .get(self.duke_music_selection)
            .filter(|name| !name.is_empty())
        else {
            return;
        };
        let Some(midi) = grp.get(name) else {
            eprintln!("room: Duke music {name} missing from GRP layers");
            return;
        };
        shared.music.play_midi(name, midi.to_vec(), true);
    }

    fn start_duke_environment_music(&mut self) {
        if matches!(
            self.console.cvars.get("duke_music"),
            Some(room_console::CvarValue::Bool(false))
        ) {
            return;
        }
        let Some(content) = self.duke_campaign_content.as_ref() else {
            return;
        };
        let Some(name) = self
            .duke_session
            .as_ref()
            .and_then(|session| session.script.as_ref())
            .and_then(|script| script.environment_music.first())
            .filter(|name| !name.is_empty())
        else {
            return;
        };
        let layers: Vec<&[u8]> = content.grp_layers.iter().map(Vec::as_slice).collect();
        let Ok(grp) = room_build::grp::GrpStack::parse_layers(&layers) else {
            return;
        };
        let Some(midi) = grp.get(name) else {
            eprintln!("room: Duke environment music {name} missing from GRP layers");
            return;
        };
        let Some(audio) = self.audio.as_ref() else {
            return;
        };
        let Ok(mut shared) = audio.shared.lock() else {
            return;
        };
        shared.mixer.music_gain = duke_audio_volume(&self.console.cvars, "duke_music_volume");
        shared.music.play_midi(name, midi.to_vec(), true);
    }

    fn show_duke_music(&mut self) {
        let Some(name) = self
            .duke_session
            .as_ref()
            .and_then(|session| session.script.as_ref())
            .and_then(|script| script.music_tracks.get(self.duke_music_selection))
            .filter(|name| !name.is_empty())
            .cloned()
        else {
            self.messages.push("No Duke music track", 70);
            return;
        };
        self.duke_music_quote_tics = 100;
        self.messages
            .push(format!("{name}. USE SHIFT-F5 TO CHANGE."), 100);
    }

    fn cycle_duke_music(&mut self) {
        if self.duke_music_quote_tics == 0 {
            return;
        }
        let Some(script) = self
            .duke_session
            .as_ref()
            .and_then(|session| session.script.as_ref())
        else {
            return;
        };
        if script.music_tracks.is_empty() {
            return;
        }
        self.duke_music_selection = (self.duke_music_selection + 1) % script.music_tracks.len();
        let name = script.music_tracks[self.duke_music_selection].clone();
        if name.is_empty() {
            self.messages.push("No Duke music track", 70);
            return;
        }
        self.duke_music_quote_tics = 100;
        self.messages.push(format!("PLAYING {name}"), 100);
        self.start_duke_level_music();
    }

    fn mouse_look_enabled(&self) -> bool {
        !matches!(
            self.console.cvars.get("mouse_look"),
            Some(room_console::CvarValue::Bool(false))
        )
    }

    fn title_elapsed_ms(&self) -> u64 {
        self.elapsed_ms
            .saturating_sub(self.title_anim_origin)
            .saturating_add(self.title_skip_ms)
    }

    fn title_anim_defs(&self) -> (Option<(u32, u16)>, Option<(u32, u16)>) {
        let Some(v) = self.vfs.as_ref() else {
            return (None, None);
        };
        let read_def = |name: &str| {
            v.find_any_ns(name)
                .and_then(|id| v.read(id).ok())
                .and_then(|b| parse_titledef(&String::from_utf8_lossy(&b)))
        };
        (read_def("RADLDEF"), read_def("TITLEDEF"))
    }

    fn title_in_splash(&self) -> bool {
        if !self
            .play
            .as_ref()
            .is_some_and(|g| g.state == GameState::Title)
        {
            return false;
        }
        let (splash, logo) = self.title_anim_defs();
        title_screen_pic(splash, logo, self.title_elapsed_ms()).1
    }

    fn try_skip_title_splash(&mut self) -> bool {
        if !self.title_in_splash() {
            return false;
        }
        let dur = splash_ms(self.title_anim_defs().0);
        if dur == 0 {
            return false;
        }
        self.title_skip_ms = dur;
        true
    }

    fn restart_title_anim(&mut self) {
        self.title_anim_origin = self.elapsed_ms;
        self.title_skip_ms = 0;
    }

    fn start_title_music(&mut self) {
        self.start_candidate_music(title_music_candidates(), "title");
    }

    fn start_intermission_music(&mut self) {
        self.start_candidate_music(intermission_music_candidates(), "intermission");
    }

    fn start_candidate_music(&mut self, candidates: &[&str], label: &str) {
        let Some(vfs) = self.vfs.as_ref() else {
            return;
        };
        let Some(audio) = self.audio.as_ref() else {
            return;
        };
        let Ok(mut shared) = audio.shared.lock() else {
            return;
        };
        match play_first_music_lump(&mut shared.music, vfs, candidates) {
            Ok(lump) => self.messages.push(format!("Music {lump}"), 35),
            Err(e) => eprintln!("room: {label} music: {e}"),
        }
    }

    fn sample_input(&mut self) -> InputSample {
        let mut keys = Vec::new();
        // Don't feed movement while a menu is up (title / pause).
        if self.menu.is_some() {
            self.mouse_yaw_accum = 0;
            self.mouse_pitch_accum = 0;
            return InputSample::default();
        }
        // `ROOM_AUTOWALK=1`: hold forward (measurements and recordings).
        static AUTOWALK: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
        let autowalk =
            *AUTOWALK.get_or_init(|| std::env::var_os("ROOM_AUTOWALK").is_some_and(|v| v != "0"));
        if self.keys.w || autowalk {
            keys.push(Key::Forward);
        }
        if self.keys.s {
            keys.push(Key::Back);
        }
        if self.keys.a {
            keys.push(Key::MoveLeft);
        }
        if self.keys.d {
            keys.push(Key::MoveRight);
        }
        if self.keys.left {
            keys.push(Key::Left);
        }
        if self.keys.right {
            keys.push(Key::Right);
        }
        let always_run = matches!(
            self.console.cvars.get("cl_run"),
            Some(room_console::CvarValue::Bool(true))
        );
        if self.keys.shift || always_run {
            keys.push(Key::Speed);
        }
        // `ROOM_AUTOFIRE=1`: hold fire (frame-time measurements of firing,
        // reloading and fights without a player).
        static AUTOFIRE: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
        let autofire =
            *AUTOFIRE.get_or_init(|| std::env::var_os("ROOM_AUTOFIRE").is_some_and(|v| v != "0"));
        if self.keys.attack.active() || autofire {
            keys.push(Key::Attack);
        }
        self.keys.attack.tick();
        if self.keys.alt_attack {
            keys.push(Key::AltAttack);
        }
        if self.keys.use_btn {
            keys.push(Key::Use);
        }
        if self.keys.space {
            keys.push(Key::Jump);
        }
        if self.keys.c {
            keys.push(Key::MoveDown);
        }
        if self.keys.slash {
            keys.push(Key::QuickKick);
        }
        if self.keys.quote {
            keys.push(Key::InvUse);
        }
        match self.keys.weapon {
            1 => keys.push(Key::Weapon1),
            2 => keys.push(Key::Weapon2),
            3 => keys.push(Key::Weapon3),
            4 => keys.push(Key::Weapon4),
            5 => keys.push(Key::Weapon5),
            6 => keys.push(Key::Weapon6),
            7 => keys.push(Key::Weapon7),
            8 => keys.push(Key::Weapon8),
            9 => keys.push(Key::Weapon9),
            10 => keys.push(Key::Weapon0),
            _ => {}
        }
        self.keys.weapon = 0;
        if self.keys.weapon_cycle > 0 {
            keys.push(Key::NextWeapon);
            self.keys.weapon_cycle -= 1;
        } else if self.keys.weapon_cycle < 0 {
            keys.push(Key::PrevWeapon);
            self.keys.weapon_cycle += 1;
        }
        for k in self.gamepad_tic_keys() {
            if !keys.contains(&k) {
                keys.push(k);
            }
        }
        for k in &self.touch_keys {
            if !keys.contains(k) {
                keys.push(*k);
            }
        }
        let mouse_yaw = self.mouse_yaw_accum;
        self.mouse_yaw_accum = 0;
        let mouse_pitch = self.mouse_pitch_accum;
        self.mouse_pitch_accum = 0;
        InputSample {
            keys,
            mouse_yaw,
            mouse_pitch,
        }
    }
}

fn create_frame_color(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    width: u32,
    height: u32,
) -> (wgpu::Texture, wgpu::TextureView) {
    let tex = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("frame-color"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::COPY_SRC
            | wgpu::TextureUsages::COPY_DST
            | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let view = tex.create_view(&wgpu::TextureViewDescriptor::default());
    (tex, view)
}

fn create_depth(
    device: &wgpu::Device,
    width: u32,
    height: u32,
) -> (wgpu::Texture, wgpu::TextureView) {
    let tex = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("depth"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: room_render::DEPTH_STENCIL_FORMAT,
        // TEXTURE_BINDING: SSAO samples depth when `gl_ssao` is on.
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let view = tex.create_view(&wgpu::TextureViewDescriptor::default());
    (tex, view)
}

/// Closest solid / near wall for blood splatter (`docs/29` J3).
///
/// Returns `(linedef index, projected x, projected y)` within `max_dist`.
fn nearest_blood_wall(map: &MapGeometry, x: f64, y: f64, max_dist: f64) -> Option<(u32, f64, f64)> {
    let max2 = max_dist * max_dist;
    let mut best: Option<(u32, f64, f64, f64)> = None;
    for (i, line) in map.lines.iter().enumerate() {
        // Prefer one-sided or blocking walls; allow close two-sided for thin partitions.
        let blocking = (line.flags & 1) != 0; // ML_BLOCKING
        let one_sided = line.back_sector.is_none();
        if !one_sided && !blocking {
            // Still consider if we'll be very close (≤ 28).
        }
        let Some(v1) = map.vertices.get(line.v1.raw() as usize) else {
            continue;
        };
        let Some(v2) = map.vertices.get(line.v2.raw() as usize) else {
            continue;
        };
        let ax = v1.pos.x();
        let ay = v1.pos.y();
        let bx = v2.pos.x();
        let by = v2.pos.y();
        let dx = bx - ax;
        let dy = by - ay;
        let len2 = dx * dx + dy * dy;
        if len2 < 1e-8 {
            continue;
        }
        let t = ((x - ax) * dx + (y - ay) * dy) / len2;
        let t = t.clamp(0.0, 1.0);
        let px = ax + t * dx;
        let py = ay + t * dy;
        let d2 = (x - px) * (x - px) + (y - py) * (y - py);
        if d2 > max2 {
            continue;
        }
        if !one_sided && !blocking && d2 > 28.0 * 28.0 {
            continue;
        }
        // Prefer solid walls when distances are similar.
        let score = d2 - if one_sided || blocking { 64.0 } else { 0.0 };
        if best.as_ref().is_none_or(|(_, _, _, s)| score < *s) {
            best = Some((i as u32, px, py, score));
        }
    }
    best.map(|(i, px, py, _)| (i, px, py))
}

fn create_stencil_aux(
    device: &wgpu::Device,
    width: u32,
    height: u32,
) -> (wgpu::Texture, wgpu::TextureView) {
    let tex = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("stencil-aux"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        // Must match unlit MRT slot 1 (docs/29 U16 G-buffer-lite).
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let view = tex.create_view(&wgpu::TextureViewDescriptor::default());
    (tex, view)
}

/// Size / light / opacity / texture chosen for one placed decal.
struct DecalLook {
    texture: Option<String>,
    width: f32,
    height: f32,
    light: f32,
    alpha: f32,
}

/// The `vid_fps` panel: "58 FPS  17.2 ms  worst 84 ms" (plus the 3D
/// resolution when lowered) over a bar graph of the last frames, with a line
/// at the target frame time. `screen` is (width px, safe top px, px per dp).
fn perf_panel(
    timer: &crate::perf::FrameTimer,
    target_ms: f32,
    render_scale: f32,
    (screen_w, safe_top, scale): (f32, f32, f32),
) -> Vec<room_render::CanvasCmd> {
    use room_render::CanvasCmd;
    let summary = timer.summary();
    let mut text = format!(
        "{} FPS  {:.1} MS  WORST {:.0}",
        summary.fps, summary.avg_ms, summary.worst_ms
    );
    if render_scale < 1.0 {
        text.push_str(&format!("  3D {:.0}%", render_scale * 100.0));
    }
    let k = (2.0 * scale).round().max(1.0) as u32;
    let (bw, bh, rgba) = crate::ui_font::badge(&text, k, [255, 240, 160, 255], [0, 0, 0, 0]);
    let bar_w = (2.0 * scale).round().max(1.0);
    let bars = 90usize;
    let graph_w = bar_w * bars as f32;
    let graph_h = (34.0 * scale).round();
    let pad = (6.0 * scale).round();
    let panel_w = graph_w.max(bw as f32) + 2.0 * pad;
    let panel_h = bh as f32 + graph_h + 2.0 * pad;
    let x = ((screen_w - panel_w) / 2.0).round().max(0.0);
    let y = safe_top + (6.0 * scale).round();
    let mut out = vec![CanvasCmd::FillRect {
        x,
        y,
        w: panel_w,
        h: panel_h,
        color: [0.0, 0.0, 0.0, 0.55],
    }];
    out.push(CanvasCmd::BlitRgba {
        x: (x + (panel_w - bw as f32) / 2.0).round(),
        y,
        w: bw,
        h: bh,
        pixels: rgba,
    });
    // Bars: full height is three target frames (50 ms at 60 fps).
    let gx = (x + (panel_w - graph_w) / 2.0).round();
    let base = y + bh as f32 + graph_h;
    let full = target_ms * 3.0;
    let history: Vec<f32> = timer.history().collect();
    let shown = &history[history.len().saturating_sub(bars)..];
    let start = bars - shown.len();
    for (i, &ms) in shown.iter().enumerate() {
        let h = ((ms / full).min(1.0) * graph_h).max(1.0).round();
        out.push(CanvasCmd::FillRect {
            x: gx + (start + i) as f32 * bar_w,
            y: base - h,
            w: bar_w,
            h,
            color: crate::perf::bar_color(ms, target_ms),
        });
    }
    // Target line.
    let line_y = (base - graph_h / 3.0).round();
    out.push(CanvasCmd::FillRect {
        x: gx,
        y: line_y,
        w: graph_w,
        h: scale.round().max(1.0),
        color: [1.0, 1.0, 1.0, 0.35],
    });
    out
}

/// A boolean CVar (`true`, or a non-zero number).
fn cvar_bool(cvars: &room_console::CvarStore, name: &str) -> bool {
    match cvars.get(name) {
        Some(room_console::CvarValue::Bool(b)) => *b,
        Some(room_console::CvarValue::Int(i)) => *i != 0,
        Some(room_console::CvarValue::Float(f)) => *f != 0.0,
        _ => false,
    }
}

/// Screen tints drawn one over another (straight alpha, display-referred),
/// as one tint: what drawing them in order over a picture gives.
fn combine_tints(tints: &[[f32; 4]]) -> Option<[f32; 4]> {
    let mut out: Option<[f32; 4]> = None;
    for t in tints {
        let a2 = t[3].clamp(0.0, 1.0);
        out = Some(match out {
            None => [t[0], t[1], t[2], a2],
            Some(o) => {
                let a1 = o[3];
                let a = a2 + a1 * (1.0 - a2);
                if a <= 0.0 {
                    [0.0; 4]
                } else {
                    let mix = |c1: f32, c2: f32| (c2 * a2 + c1 * a1 * (1.0 - a2)) / a;
                    [mix(o[0], t[0]), mix(o[1], t[1]), mix(o[2], t[2]), a]
                }
            }
        });
    }
    out
}

/// HUD frames `(sprite, frame index)` of weapon `class`: every state it and
/// its ancestors label, except the pickup's `Spawn` state.
fn weapon_hud_frames(
    world: &room_sim::World,
    class: room_core::ClassId,
) -> std::collections::BTreeSet<(String, u8)> {
    use std::collections::HashSet;
    let mut out = std::collections::BTreeSet::new();
    let mut cur = Some(class);
    let mut depth = 0;
    while let Some(c) = cur {
        if depth > 64 {
            break;
        }
        depth += 1;
        for (label, first) in world.states.labels_of(c) {
            if label == "spawn" {
                continue;
            }
            let mut seen: HashSet<room_core::StateId> = HashSet::new();
            let mut next = Some(first);
            while let Some(id) = next {
                if !seen.insert(id) || seen.len() > 1024 {
                    break;
                }
                let Some(state) = world.states.get(id) else {
                    break;
                };
                let sprite = state.sprite.trim().to_ascii_uppercase();
                if sprite.len() == 4 && sprite != "TNT1" && sprite != "----" && sprite != "####" {
                    out.insert((sprite, state.frame));
                }
                next = state.next;
            }
        }
        cur = world.classes.get(c).and_then(|d| d.parent);
    }
    out
}

/// Window size: `ROOM_WINDOW_SIZE=WxH` (logical px; e.g. `873x393` with
/// `WINIT_X11_SCALE_FACTOR=2.75` previews a phone's layout), else 1280×720.
fn window_size() -> LogicalSize<f64> {
    std::env::var("ROOM_WINDOW_SIZE")
        .ok()
        .and_then(|v| {
            let (w, h) = v.split_once(['x', 'X'])?;
            let (w, h): (f64, f64) = (w.trim().parse().ok()?, h.trim().parse().ok()?);
            (w >= 64.0 && h >= 64.0).then(|| LogicalSize::new(w, h))
        })
        .unwrap_or(LogicalSize::new(1280.0, 720.0))
}

/// GPU quads for the weapon's [`room_ui::PatchQuad`]s, scaled by `k` (1 on
/// the screen; the scene's resolution over the screen's when the weapon is
/// drawn into the scene).
/// `ROOM_SIM_TRACE=1`: print shots, specials, weapon states and hits to the
/// terminal without turning on `developer` (which also shows them on screen).
fn env_trace() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("ROOM_SIM_TRACE").is_some_and(|v| v != "0"))
}

fn sprite_quads(quads: &[room_ui::PatchQuad], k: f32) -> Vec<room_render::SpriteQuad> {
    quads
        .iter()
        .map(|q| {
            let (x, y, w, h) = if (k - 1.0).abs() < 1e-4 {
                (q.x, q.y, q.w, q.h)
            } else {
                let x0 = (q.x as f32 * k).floor();
                let y0 = (q.y as f32 * k).floor();
                let x1 = ((q.x as f32 + q.w as f32) * k).ceil();
                let y1 = ((q.y as f32 + q.h as f32) * k).ceil();
                (
                    x0 as i32,
                    y0 as i32,
                    (x1 - x0).max(1.0) as u32,
                    (y1 - y0).max(1.0) as u32,
                )
            };
            room_render::SpriteQuad {
                key: if q.inverse {
                    format!("{}\u{1}inverse", q.name)
                } else {
                    q.name.clone()
                },
                x,
                y,
                w,
                h,
                light: q.light,
                tint: q.tint,
                alpha: q.alpha,
                flip: q.flip,
                mirror: q.mirror,
                smooth: false,
            }
        })
        .collect()
}

/// RGBA of the HUD picture `key` (a name, `\u{1}inverse` for the inverse
/// palette) at its own size.
fn patch_picture(patches: &room_ui::PatchCache, key: &str) -> Option<(u32, u32, Vec<u8>)> {
    let (name, inverse) = match key.strip_suffix("\u{1}inverse") {
        Some(n) => (n, true),
        None => (key, false),
    };
    let patch = patches.get(name)?;
    let palette = if inverse {
        room_ui::inverse_palette(patches.palette())
    } else {
        *patches.palette()
    };
    let (w, h) = (u32::from(patch.width), u32::from(patch.height));
    Some((w, h, patch.scale_rgba(w, h, &palette)))
}

/// `ROOM_FRAME_DUMP=dir:start:every`: save every `every`-th frame from frame
/// `start` as `dir/frame_NNNNNN.png` (README GIFs, visual checks).
fn frame_dump() -> Option<(std::path::PathBuf, u64, u64)> {
    static DUMP: std::sync::OnceLock<Option<(std::path::PathBuf, u64, u64)>> =
        std::sync::OnceLock::new();
    DUMP.get_or_init(|| {
        let v = std::env::var("ROOM_FRAME_DUMP").ok()?;
        let mut parts = v.rsplitn(3, ':');
        let every = parts.next()?.parse().ok()?;
        let start = parts.next()?.parse().ok()?;
        let dir = parts.next()?;
        Some((std::path::PathBuf::from(dir), start, every))
    })
    .clone()
}

/// Write straight RGBA as a PNG.
fn image_png(path: &std::path::Path, rgba: &[u8], w: u32, h: u32) -> std::io::Result<()> {
    let mut png = Vec::new();
    room_render::encode_png(w, h, rgba, &mut png).map_err(std::io::Error::other)?;
    std::fs::write(path, png)
}
