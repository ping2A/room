//! Native desktop map viewer (M4 free-fly shell).

#![cfg_attr(not(test), deny(clippy::unwrap_used))]

mod audio_out;
mod crash;
mod face_import;
mod launcher;
mod midi_device;
mod netplay;
mod screen_capture;
mod shader_cvars;
mod soundfont;

use audio_out::AudioOut;
use clap::Parser;
use glam::Vec3;
use midi_device::{MidiBackend, load_genmidi, resolve_midi_device};
use room_console::{Console, HostCommand};
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
    AttackLatch, InputSample, Key, ModButtonState, WeaponImpulseState,
    build_ticcmd_with_mods_and_impulses, default_bind_pairs, device_button_is_primary,
};
use room_map::{MapGeometry, load_map};
use room_nodebuild::{apply_nodes, ensure_nodes};
use room_render::{
    Camera, Canvas, CanvasGpu, CubemapFaces, DecalStore, DynLightInstance, GpuContext, LightMode,
    MAX_VOXEL_WALL_VERTS_PER_FRAME, MaterialCatalog, MaterialMaps, ModZonePass, ModelActorPose,
    ModelCatalog, OffscreenTarget, PARTICLE_TEX, PARTICLE_TEX_ADD, ParticleStore, PostChain,
    PostChainInputs, RenderSettings, SceneLists, SpriteInstance, SpriteOrient, TextureAnimator,
    UnlitPass, VoxelActorPose, VoxelCatalog, ZoneDraw, ZoneMeshes, append_actor_models,
    append_decals, append_particles_ex, apply_glow_fullbright, build_scene,
    build_scene_with_sky_ex_sized, build_zone_meshes, clip_scene_to_mirror_line, mirror_clip_plane,
    particle_tint_tex_name, read_rgba, read_texture_rgba, render_offscreen, scroll_sky_u,
    soft_disc_rgba, sprite_lump_candidates_rot, sprite_offset_delta, sprite_rotation_digit,
    voxel_spin_degrees, voxel_view_angles,
};
use room_sim::mod_hud::layer as mod_hud_layer;
use room_sim::{
    ActorId, Buttons, ClassRegistry, FLAG_CORPSE, FLAG_COUNTKILL, FLAG_DONTDRAW, FLAG_DROPPED,
    FLAG_FLOAT, FLAG_MISSILE, FLAG_NOGRAVITY, FLAG_SPAWNCEILING, RenderStyle, ShotKind, StateTable,
    TicCmd, actor_light_level, check_sight, sprite_pose,
};
use room_testkit::{MapBuilder, combat_arena, rect};
use room_ui::{
    AutomapView, CanvasSink, DoomStatusBar, DrawCmd, DukeStatusBar, FontRegistry, HudState,
    IntermissionStats, MenuCommand, MenuSfx, MenuStack, MessageQueue, ModHudMapCtx, PatchCache,
    StatusBar, UiDrawList, anim_pic, blit_draw_list_full, draw_automap, draw_crosshair,
    draw_finale, draw_intermission, draw_mod_hud_layer, draw_mod_hud_modals,
    draw_mod_zones_automap, draw_outside_zone_tint, draw_stcfn_string, draw_title, draw_weapon_lit,
    draw_weapon_tinted, finale_anm_prefix, overlay_bob, overlay_layer_bob, parse_titledef,
    player_map_poses, splash_ms, title_screen_pic,
};
use room_vfs::{Namespace, Vfs, VfsBuilder};
use screen_capture::GifRecorder;
use shader_cvars::{
    cycle_cvar, graphics_menu, render_settings_from_cvars, reset_shaders_to_defaults,
    toggle_float_cvar,
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

#[derive(Debug, Parser)]
#[command(name = "room", about = "Room map viewer / play shell (M4–M5)")]
struct Args {
    /// IWAD / PWAD / directory to mount (optional; uses built-in test room).
    #[arg(long)]
    iwad: Option<PathBuf>,
    /// PWAD / PK3 / directory mounted after `--iwad` (repeatable; e.g. WASM mods).
    #[arg(long)]
    file: Vec<PathBuf>,
    /// Map lump name (e.g. MAP01).
    #[arg(long, default_value = "MAP01")]
    map: String,
    /// Run Headless playsim and drive the player with WASD (M5 bootstrap).
    #[arg(long, default_value_t = false)]
    play: bool,
    /// Keep the title / main menu instead of entering `--map` immediately (with `--play`).
    #[arg(long, default_value_t = false)]
    title: bool,
    /// Built-in combat arena (monsters + weapons/ammo). Implies `--play`, skips title.
    /// Uses `--iwad` for sprites/textures (defaults to `testdata/freedoom/freedoom2.wad`).
    #[arg(long, default_value_t = false)]
    combat_demo: bool,
    /// Doom skill 1..=5 (Hurt Me Plenty = 3).
    #[arg(long, default_value_t = 3)]
    skill: u8,
    /// Start with all weapons, ammo, keys, and armor (`give all`).
    #[arg(long, default_value_t = false)]
    give_all: bool,
    /// Start with god mode on (`iddqd` / console `god`). Handy with `--combat-demo`.
    #[arg(long, default_value_t = false)]
    god: bool,
    /// Save a screenshot after this many frames, then quit (smoke checks).
    #[arg(long)]
    shot_after: Option<u64>,
    /// Optional DECORATE file (defaults to testdata/actors/doom_basic.txt when --play).
    #[arg(long)]
    decorate: Option<PathBuf>,
    /// GM SoundFont (`.sf2`/`.sf3`) for MIDI/MUS music. Also searches `soundfonts/`,
    /// `$ROOM_SOUNDFONT`, and common bank names.
    #[arg(long)]
    soundfont: Option<PathBuf>,
    /// MIDI backend: `auto` (SF2 then OPL), `sf2`, or `opl` (IWAD `GENMIDI`).
    #[arg(long, value_enum, default_value_t = MidiBackend::Auto)]
    mididevice: MidiBackend,
    /// List discovered IWADs and exit (launcher bootstrap).
    #[arg(long, default_value_t = false)]
    launcher: bool,
    /// Import a gzdoom.ini / room.ini before starting.
    #[arg(long)]
    import_ini: Option<PathBuf>,
    /// Host coop (`--host 5029`). Implies `--play`. Waits for `--players` joiners.
    #[arg(long)]
    host: Option<String>,
    /// Join coop host (`--join 127.0.0.1:5029`). Implies `--play`.
    #[arg(long)]
    join: Option<String>,
    /// Coop player count when hosting (2..=4 UDP/TCP; 2..=8 with `--transport tcp-server`).
    #[arg(long, default_value_t = 2)]
    players: u8,
    /// Net transport: `udp` (default), `tcp` (P2P), or `tcp-server` (host packet-server relay).
    #[arg(long, value_enum, default_value_t = netplay::NetTransportKind::Udp)]
    transport: netplay::NetTransportKind,
    /// Encrypt lockstep payloads with Noise_XX (after lobby; UDP/TCP/tcp-server).
    #[arg(long, default_value_t = false)]
    noise: bool,
    /// Local display name for netplay / session (also `name` cvar when `--import-ini`).
    #[arg(long, default_value = "")]
    name: String,
    /// Set a CVar at startup (`--set gl_bloom=1`, repeatable). Applied after
    /// `room.ini` / `--import-ini` so launcher menus can override graphics.
    #[arg(long = "set", value_name = "NAME=VALUE")]
    set_cvars: Vec<String>,
}

fn main() {
    crash::install();
    let mut args = Args::parse();
    if args.host.is_some() && args.join.is_some() {
        eprintln!("room: use only one of --host / --join");
        std::process::exit(2);
    }
    if args.host.is_some() || args.join.is_some() {
        args.play = true;
        args.title = false;
        if args.iwad.is_none() {
            let doom1 =
                PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/doom/doom1.wad");
            if doom1.is_file() {
                args.iwad = Some(doom1);
                if args.map == "MAP01" {
                    args.map = "E1M1".into();
                }
            }
        }
    }
    if args.launcher {
        let cands = launcher::discover_iwads();
        launcher::print_launcher_help(&cands);
        for p in launcher::discover_gzdoom_ini() {
            eprintln!("room: found gzdoom.ini candidate {}", p.display());
        }
        std::process::exit(0);
    }
    if args.combat_demo {
        args.play = true;
        args.title = false;
        // Instant arsenal so the fight is the focus; ground pickups still exist.
        if !args.give_all {
            args.give_all = true;
        }
        if args.iwad.is_none() {
            let cands = launcher::discover_iwads();
            if let Some(c) = launcher::prefer_doom2(&cands) {
                eprintln!("room: auto IWAD {}", c.path.display());
                args.iwad = Some(c.path.clone());
            }
        }
    }
    let (map, mut vfs) = match load_viewer_map(&args) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("room: {e}");
            let cands = launcher::discover_iwads();
            if !cands.is_empty() {
                launcher::print_launcher_help(&cands);
            }
            std::process::exit(1);
        }
    };
    let event_loop = match EventLoop::new() {
        Ok(e) => e,
        Err(e) => {
            eprintln!("room: event loop: {e}");
            std::process::exit(1);
        }
    };
    // Raw mouse buttons while look-grabbed (needed on every OS; macOS no-ops but safe).
    event_loop.listen_device_events(DeviceEvents::Always);
    event_loop.set_control_flow(ControlFlow::Poll);
    let skill = Skill::new(args.skill.clamp(1, 5));
    // Open audio before the play session so SFX resample to the device rate.
    let audio = match AudioOut::start() {
        Ok(a) => {
            eprintln!("room: audio output started");
            Some(a)
        }
        Err(e) => {
            eprintln!("room: audio: {e}");
            None
        }
    };
    let audio_rate = audio
        .as_ref()
        .map(|a| a.sample_rate)
        .unwrap_or(room_audio::ENGINE_SAMPLE_RATE);
    let iwad_label = args
        .iwad
        .as_ref()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "builtin".into());
    let fingerprint = netplay::session_fingerprint(&iwad_label, &args.map, skill.0);
    let mut net: Option<netplay::NetPlay> = None;
    let mut local_seat: u8 = 0;
    let mut coop_players: u8 = 1;
    let mut rng_seed: u32 = 1993;
    if let Some(bind) = args.host.as_deref() {
        let addr = match netplay::parse_host_bind(bind) {
            Ok(a) => a,
            Err(e) => {
                eprintln!("room: --host: {e}");
                std::process::exit(2);
            }
        };
        coop_players = match args.transport {
            netplay::NetTransportKind::TcpServer => args.players.clamp(2, 8),
            _ => args.players.clamp(2, 4),
        };
        rng_seed = 0xDEAD_1993;
        match netplay::NetPlay::host(netplay::HostOpts {
            bind_addr: addr,
            max_players: coop_players,
            fingerprint,
            rng_seed,
            map: &args.map,
            skill: skill.0,
            kind: args.transport,
            noise: args.noise,
        }) {
            Ok(n) => {
                local_seat = 0;
                coop_players = n.peer_count;
                rng_seed = n.rng_seed;
                net = Some(n);
            }
            Err(e) => {
                eprintln!("room: host failed: {e}");
                std::process::exit(1);
            }
        }
    } else if let Some(join) = args.join.as_deref() {
        let addr = match netplay::parse_join_addr(join) {
            Ok(a) => a,
            Err(e) => {
                eprintln!("room: --join: {e}");
                std::process::exit(2);
            }
        };
        match netplay::NetPlay::join(
            addr,
            fingerprint,
            "player",
            args.transport,
            args.noise,
            args.players.clamp(2, 8),
        ) {
            Ok((n, map_name)) => {
                if !map_name.eq_ignore_ascii_case(&args.map) {
                    eprintln!(
                        "room: warning: host map {map_name} != local --map {}",
                        args.map
                    );
                }
                local_seat = n.local.0;
                coop_players = n.peer_count;
                rng_seed = n.rng_seed;
                net = Some(n);
            }
            Err(e) => {
                eprintln!("room: join failed: {e}");
                std::process::exit(1);
            }
        }
    }
    let (play, decaldef) = if args.play {
        let (mut game, decaldef) = start_play_session(
            &map,
            args.decorate.as_deref(),
            vfs.as_mut(),
            skill,
            audio_rate,
            rng_seed,
            coop_players,
        );
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
                eprintln!("room: start map: {e}");
                std::process::exit(1);
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
        (Some(game), decaldef)
    } else {
        (None, None)
    };
    let fonts = vfs
        .as_ref()
        .map(FontRegistry::load_doom_fonts)
        .unwrap_or_default();
    let patches = vfs.as_ref().map(PatchCache::from_vfs).unwrap_or_default();
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
        post_ping: None,
        post_ldr: None,
        post_chain: None,
        render_settings: RenderSettings::default(),
        ephemeral_shaders: args.combat_demo,
        canvas_gpu: None,
        camera: Camera::default(),
        pending_screenshot: false,
        shot_after: args.shot_after,
        quit: false,
        gif_recorder: None,
        keys: Keys::default(),
        yaw: 0.0,
        pitch: 0.0,
        roll: 0.0,
        weapon_bob_prev: (0.0, 0.0),
        mouse_look: false,
        ignore_warp_motion: false,
        unfocused_since: None,
        menu_cursor: (160.0, 100.0),
        last_frame: std::time::Instant::now(),
        frames: 0,
        title_anim_origin: 0,
        title_skip_ms: 0,
        demo_slot: "quick".into(),
        fps_window: std::time::Instant::now(),
        fps_count: 0,
        automap: false,
        automap_scale: 0.35,
        hud: HudState::doom_default(),
        play,
        net,
        local_seat,
        tic_accum: 0.0,
        cam_prev: None,
        cam_curr: None,
        sprite_prev: std::collections::HashMap::new(),
        sprite_curr: std::collections::HashMap::new(),
        messages: MessageQueue::new(),
        hud_message_log: std::collections::HashMap::new(),
        language,
        last_player_hp: 100,
        fixed_colormap: 0,
        mouse_yaw_accum: 0,
        mouse_pitch_accum: 0,
        console: {
            let mut con = Console::with_defaults();
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
        mod_buttons: ModButtonState::default(),
        weapon_impulses: WeaponImpulseState::default(),
        audio,
    };
    if let Some(audio) = &app.audio {
        configure_music(
            audio,
            args.mididevice,
            args.soundfont.as_deref(),
            app.vfs.as_ref(),
        );
    }
    if let Some(conf) = app.play.as_ref().and_then(|g| g.keyconf()) {
        for line in keyconf_alias_lines(conf) {
            let _ = app.console.exec(&line);
        }
        for (key, cmd) in keyconf_default_binds(conf) {
            app.keyconf_binds.insert(key.to_ascii_lowercase(), cmd);
        }
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
    }
    if let Err(e) = event_loop.run_app(&mut app) {
        eprintln!("room: {e}");
        std::process::exit(1);
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

fn dirs_room_ini() -> PathBuf {
    #[cfg(target_os = "macos")]
    {
        if let Some(home) = std::env::var_os("HOME") {
            return PathBuf::from(home)
                .join("Library")
                .join("Application Support")
                .join("room")
                .join("room.ini");
        }
    }
    #[cfg(target_os = "windows")]
    {
        if let Some(app) = std::env::var_os("APPDATA") {
            return PathBuf::from(app).join("room").join("room.ini");
        }
    }
    if let Some(xdg) = std::env::var_os("XDG_CONFIG_HOME") {
        return PathBuf::from(xdg).join("room").join("room.ini");
    }
    if let Some(home) = std::env::var_os("HOME") {
        return PathBuf::from(home)
            .join(".config")
            .join("room")
            .join("room.ini");
    }
    PathBuf::from("room.ini")
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
    let ini = dirs_room_ini();
    match ini.parent() {
        Some(dir) => dir.join("saves").join(format!("{slot}.fsave")),
        None => PathBuf::from(format!("{slot}.fsave")),
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

fn dirs_room_demo(slot: &str) -> PathBuf {
    let ini = dirs_room_ini();
    match ini.parent() {
        Some(dir) => dir.join("demos").join(format!("{slot}.fdem")),
        None => PathBuf::from(format!("{slot}.fdem")),
    }
}

fn default_freedoom2() -> Option<PathBuf> {
    let candidates = [
        PathBuf::from("testdata/freedoom/freedoom2.wad"),
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/freedoom/freedoom2.wad"),
    ];
    candidates.into_iter().find(|p| p.is_file())
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
    let mut roots = vec![
        PathBuf::from("."),
        PathBuf::from("testdata/m10"),
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/m10"),
    ];
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            roots.push(dir.to_path_buf());
        }
    }
    roots
}

fn mount_pwads(mut builder: VfsBuilder, args: &Args) -> Result<VfsBuilder, String> {
    for path in &args.file {
        builder = builder
            .mount_path(path)
            .map_err(|e| format!("{}: {e}", path.display()))?;
        eprintln!("room: mounted {}", path.display());
    }
    Ok(builder)
}

fn load_viewer_map(args: &Args) -> Result<(MapGeometry, Option<Vfs>), String> {
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
    shift: bool,
    /// Turn left (← / Q when not in a menu).
    left: bool,
    /// Turn right (→).
    right: bool,
    /// Attack held (Ctrl / mouse left in play mode).
    attack: AttackLatch,
    /// Alt attack held (mouse right in play mode; `docs/31` MA4).
    alt_attack: bool,
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
    /// Current MAPINFO `sky1` texture (empty = default bind).
    sky_texture: String,
    /// Active GLDEFS cubemap (when complete); else cylinder uses [`Self::sky_texture`].
    sky_cubemap: Option<CubemapFaces>,
    /// GLDEFS skyboxes by lowercase name.
    skyboxes: std::collections::BTreeMap<String, CubemapFaces>,
    /// GLDEFS dynamic lights + object-frame bindings (M10).
    gldefs: GlDefs,
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
    window: Option<Arc<Window>>,
    camera: Camera,
    /// Capture PNG of the next presented frame.
    pending_screenshot: bool,
    /// `--shot-after`: frame to capture on; quit once saved.
    shot_after: Option<u64>,
    quit: bool,
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
    frames: u64,
    /// `frames` when the current title/splash started.
    title_anim_origin: u64,
    /// Extra title elapsed so Enter / click can skip `RADLOGO`.
    title_skip_ms: u64,
    /// Slot name for the in-progress Room demo (`record` / `stop`).
    demo_slot: String,
    fps_window: std::time::Instant,
    fps_count: u32,
    /// Tab toggles classic automap overlay.
    automap: bool,
    /// Automap zoom: screen pixels per map unit (`=`/`-` / mouse wheel).
    automap_scale: f32,
    /// Status bar numbers (viewer stub).
    hud: HudState,
    /// Optional Headless playsim session (`--play`).
    play: Option<Headless>,
    /// Optional UDP lockstep coop session (`--host` / `--join`).
    net: Option<netplay::NetPlay>,
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
    /// Held `+mod1` … `+mod4` bits (`docs/28` BE4).
    mod_buttons: ModButtonState,
    /// KEYCONF `+reload` / `+zoom` / `+user1`–`4` latch (`docs/33` BD4).
    weapon_impulses: WeaponImpulseState,
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

fn install_basic_decorate(game: &mut Headless) {
    let src = include_str!("../../../testdata/actors/doom_basic.txt");
    let ir = match parse_decorate(src) {
        Ok(i) => i,
        Err(e) => {
            eprintln!("room: decorate parse: {e}");
            std::process::exit(1);
        }
    };
    let classes = match ClassRegistry::from_decorate(&ir) {
        Ok(c) => Arc::new(c),
        Err(e) => {
            eprintln!("room: class registry: {e}");
            std::process::exit(1);
        }
    };
    let states = Arc::new(StateTable::from_decorate(&ir, &classes));
    game.set_defs(classes, states);
}

fn start_play_session(
    map: &MapGeometry,
    decorate: Option<&std::path::Path>,
    vfs: Option<&mut Vfs>,
    skill: Skill,
    audio_rate: u32,
    rng_seed: u32,
    coop_players: u8,
) -> (Headless, Option<room_defs::DecalInfo>) {
    let mut game = Headless::new(GameOptions {
        skill,
        rng_seed,
        record_demo: false,
        coop_players: coop_players.max(1),
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
            zscript_handlers = defs.zscript_event_handlers();
            game.set_mapinfo(Arc::new(defs.mapinfo));
            game.set_terrain(&defs.terrain);
            game.set_sndseq(&defs.sndseq);
            game.set_lockdefs(&defs.lockdefs, Some("Doom"));
            game.set_decaldef(&defs.decaldef);
            decaldef = Some(defs.decaldef.clone());
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
        game.set_zscript(zs.program, zscript_handlers);
        if let Some(path) = decorate {
            let src = match std::fs::read_to_string(path) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("room: decorate {path:?}: {e}");
                    std::process::exit(1);
                }
            };
            let ir = match parse_decorate(&src) {
                Ok(i) => i,
                Err(e) => {
                    eprintln!("room: decorate parse: {e}");
                    std::process::exit(1);
                }
            };
            let classes = match ClassRegistry::from_decorate(&ir) {
                Ok(c) => Arc::new(c),
                Err(e) => {
                    eprintln!("room: class registry: {e}");
                    std::process::exit(1);
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
            install_basic_decorate(&mut game);
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
                eprintln!("room: decorate {path:?}: {e}");
                std::process::exit(1);
            }
        };
        let ir = match parse_decorate(&src) {
            Ok(i) => i,
            Err(e) => {
                eprintln!("room: decorate parse: {e}");
                std::process::exit(1);
            }
        };
        let classes = match ClassRegistry::from_decorate(&ir) {
            Ok(c) => Arc::new(c),
            Err(e) => {
                eprintln!("room: class registry: {e}");
                std::process::exit(1);
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
        install_basic_decorate(&mut game);
    }
    game.set_pending_map(map.clone());
    eprintln!(
        "room: play mode — WASD move, ←→/Q turn, mouse look, wheel weapon, E use, Ctrl/LMB attack, Tab automap (=/- / wheel zoom), Esc pause, F6–F9 cheats, F12 screenshot, F11 GIF"
    );
    (game, decaldef)
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
    let candidates = [
        std::path::PathBuf::from("wadsrc/room"),
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../wadsrc/room"),
        std::path::PathBuf::from("target/room.pk3"),
    ];
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

impl ApplicationHandler for ViewerApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let attrs = Window::default_attributes()
            .with_title("Room map viewer")
            .with_inner_size(LogicalSize::new(1280.0, 720.0));
        #[cfg(target_os = "macos")]
        let attrs = {
            use winit::platform::macos::WindowAttributesExtMacOS;
            attrs.with_accepts_first_mouse(true)
        };
        let window = match event_loop.create_window(attrs) {
            Ok(w) => Arc::new(w),
            Err(e) => {
                eprintln!("room: window: {e}");
                event_loop.exit();
                return;
            }
        };

        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..Default::default()
        });
        let surface = match instance.create_surface(window.clone()) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("room: surface: {e}");
                event_loop.exit();
                return;
            }
        };
        let adapter =
            match pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })) {
                Ok(a) => a,
                Err(e) => {
                    eprintln!("room: adapter: {e}");
                    event_loop.exit();
                    return;
                }
            };
        let gpu = match pollster::block_on(GpuContext::from_adapter(instance, adapter)) {
            Ok(g) => g,
            Err(e) => {
                eprintln!("room: gpu: {e}");
                event_loop.exit();
                return;
            }
        };

        let size = window.inner_size();
        let caps = surface.get_capabilities(&gpu.adapter);
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
        surface.configure(&gpu.device, &config);
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
        let depth = create_depth(&gpu.device, config.width, config.height);
        let stencil_aux = create_stencil_aux(&gpu.device, config.width, config.height);
        let frame_color = create_frame_color(&gpu.device, format, config.width, config.height);
        let post_ping = create_frame_color(&gpu.device, format, config.width, config.height);
        let post_ldr = create_frame_color(&gpu.device, format, config.width, config.height);
        let post_chain = PostChain::new(&gpu, format, format);
        let canvas_gpu = CanvasGpu::new(&gpu, format, config.width, config.height);
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

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                if let (Some(surface), Some(config), Some(gpu)) =
                    (&self.surface, &mut self.config, &self.gpu)
                {
                    config.width = size.width.max(1);
                    config.height = size.height.max(1);
                    surface.configure(&gpu.device, config);
                    self.depth = Some(create_depth(&gpu.device, config.width, config.height));
                    self.stencil_aux =
                        Some(create_stencil_aux(&gpu.device, config.width, config.height));
                    self.frame_color = Some(create_frame_color(
                        &gpu.device,
                        config.format,
                        config.width,
                        config.height,
                    ));
                    self.post_ping = Some(create_frame_color(
                        &gpu.device,
                        config.format,
                        config.width,
                        config.height,
                    ));
                    self.post_ldr = Some(create_frame_color(
                        &gpu.device,
                        config.format,
                        config.width,
                        config.height,
                    ));
                    self.canvas_gpu = Some(CanvasGpu::new(
                        gpu,
                        config.format,
                        config.width,
                        config.height,
                    ));
                    self.camera.aspect = f64::from(config.width) / f64::from(config.height);
                }
            }
            WindowEvent::KeyboardInput {
                event:
                    KeyEvent {
                        physical_key: PhysicalKey::Code(code),
                        state,
                        text,
                        ..
                    },
                ..
            } => {
                let down = state == ElementState::Pressed;
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
                let menu_open = self.menu.is_some();
                match code {
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
                    KeyCode::KeyL if down && self.play.is_some() && !menu_open => {
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
                    KeyCode::F5 if down => {
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
                            event_loop.exit();
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
            } if self.play.is_some() && self.menu.is_none() => {
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
                if self.play.is_none() {
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
            WindowEvent::RedrawRequested => {
                self.frame();
                if self.quit {
                    event_loop.exit();
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
                if self.play.is_some()
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

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(w) = &self.window {
            w.request_redraw();
        }
    }
}

impl ViewerApp {
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

    fn frame(&mut self) {
        let dt = self.last_frame.elapsed().as_secs_f32().min(0.05);
        self.last_frame = std::time::Instant::now();
        if self
            .play
            .as_ref()
            .is_some_and(|g| g.state == GameState::Level)
        {
            self.tick_play(dt);
            // Re-apply after tics (and between tics) so switch LEDs stay flipped.
            self.sync_switch_textures_from_world();
        } else if self.play.is_none() {
            self.integrate(dt);
        }

        self.frames = self.frames.wrapping_add(1);
        self.fps_count = self.fps_count.wrapping_add(1);
        if self.fps_window.elapsed().as_secs() >= 1 {
            let fps = self.fps_count;
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
        // Mirror canvases need the current scene's mirror list — updated after build.
        let live_sprites = self.collect_live_sprites();
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
        let tex_sizes = self.materials.wall_texel_sizes();
        let mut scene: SceneLists = build_scene_with_sky_ex_sized(
            &self.map,
            &self.camera,
            live_sprites.as_deref(),
            sky,
            self.sky_cubemap.as_ref(),
            Some(&tex_sizes),
        );
        let tic = self
            .play
            .as_ref()
            .and_then(|g| g.world())
            .map_or(0, |w| w.level.maptime);
        self.tex_anim
            .apply(&mut scene, u32::try_from(tic).unwrap_or(0));
        scene.dyn_lights = self.collect_dyn_lights();
        // Dyn lights shade on the GPU (`LightUniforms`); keep CPU boost off to
        // avoid double-lighting once the forward path is active.
        {
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
            apply_glow_fullbright(&mut scene, &glow);
        }
        let mut fog_density = 0.0f32;
        if let Some(world) = self.play.as_ref().and_then(|g| g.world()) {
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
                if self.render_settings.actor_sprite_shadow {
                    let name = particle_tint_tex_name([16, 16, 16]);
                    if self.materials.get(&name).is_none() {
                        let rgba = soft_disc_rgba([16, 16, 16]);
                        let _ = self.materials.upsert_rgba(
                            &gpu.device,
                            &gpu.queue,
                            &name,
                            16,
                            16,
                            &rgba,
                        );
                    }
                }
            }
            append_particles_ex(
                &mut scene,
                &self.particles,
                &self.camera,
                self.render_settings.particle_fx,
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
        self.refresh_mirror_textures(&scene.mirrors);
        self.refresh_portal_textures(&scene.linked_portals);
        self.refresh_plane_reflect_textures(&scene.plane_reflects);
        if self.shot_after.is_some_and(|n| self.frames >= n) {
            self.pending_screenshot = true;
        }
        let want_shot = self.pending_screenshot;
        let want_gif = self.gif_recorder.is_some();
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
        let capture_rgba = {
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
            pass.inverse_colormap = inverse_colormap;
            pass.time += dt; // fuzz animation clock
            pass.prepare(gpu, &self.camera, &scene);

            let ui_only = matches!(
                self.play.as_ref().map(|g| g.state),
                Some(GameState::Title | GameState::Intermission | GameState::Finale) | None
            );

            let mut hud = Canvas::new();
            hud.clear([0.0, 0.0, 0.0, 0.0]);
            // Build UI draw list (status bar + optional automap) then blit to canvas.
            let mut ui = UiDrawList::new();
            match self.play.as_ref().map(|g| g.state) {
                Some(GameState::Title) => {
                    let elapsed_ms = self
                        .frames
                        .saturating_sub(self.title_anim_origin)
                        .saturating_mul(16)
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
                }
                Some(GameState::Finale) => {
                    let map_name = self
                        .play
                        .as_ref()
                        .and_then(|g| g.intermission.as_ref())
                        .map(|t| t.map_name.as_str())
                        .unwrap_or("");
                    let elapsed_ms = self.frames.saturating_mul(16);
                    let pic = self.vfs.as_ref().and_then(|v| {
                        let prefix = finale_anm_prefix(map_name)?;
                        let def = v.find_any_ns(&format!("{prefix}DEF"))?;
                        let body = v.read(def).ok()?;
                        let (n, fps) = parse_titledef(&String::from_utf8_lossy(&body))?;
                        Some(anim_pic(prefix, n, fps, elapsed_ms))
                    });
                    draw_finale(&mut ui, map_name, 320.0, 200.0, pic.as_deref());
                }
                _ => {
                    if self.automap {
                        let seat = usize::from(self.local_seat);
                        let allmap = self
                            .play
                            .as_ref()
                            .and_then(|g| g.world())
                            .and_then(|w| w.players.get(seat).and_then(|p| p.as_ref()))
                            .is_some_and(|p| p.allmap);
                        let view = AutomapView {
                            center_x: f64::from(self.camera.eye.x),
                            center_y: f64::from(self.camera.eye.y),
                            scale: self.automap_scale,
                            screen_x: 0.0,
                            screen_y: 0.0,
                            width: 320.0,
                            height: 168.0,
                            angle_deg: self.yaw.to_degrees(),
                            allmap,
                            // Play mode: only show linedefs the player has walked near.
                            fog: self.play.is_some(),
                        };
                        // Live world map carries `ML_MAPPED` exploration bits.
                        if let Some(world) = self.play.as_ref().and_then(|g| g.world()) {
                            draw_automap(&world.map, &view, &mut ui);
                            let maptime = world.level.maptime;
                            draw_mod_zones_automap(&world.mod_zones, maptime, 1000, &view, &mut ui);
                        } else {
                            draw_automap(&self.map, &view, &mut ui);
                        }
                    } else {
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
                        if !self.hud.overlay_patch.is_empty() {
                            if let Some(soft_overlay) = self.patches.get(&self.hud.overlay_patch) {
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
                            }
                        }
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
                        if self.hud.inverse_colormap() {
                            ui.invert_since(weapon_cmds);
                        }
                        draw_crosshair(&mut ui, self.hud.crosshair);
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
                        if !spectating {
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
            struct Sink<'a>(&'a mut Canvas);
            impl CanvasSink for Sink<'_> {
                fn fill_rect(&mut self, x: f32, y: f32, w: f32, h: f32, color: [f32; 4]) {
                    self.0.fill_rect(x, y, w, h, color);
                }
                fn text(&mut self, x: f32, y: f32, text: &str, color: [f32; 4]) {
                    self.0.text(x, y, text, color);
                }
                fn blit_rgba(&mut self, x: f32, y: f32, w: u32, h: u32, pixels: &[u8]) {
                    self.0.blit_rgba(x, y, w, h, pixels.to_vec());
                }
            }
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
            blit_draw_list_full(
                &ui,
                320.0 * scale,
                200.0 * scale,
                Some(&self.fonts),
                Some(&self.patches),
                &mut OffsetSink {
                    inner: Sink(&mut hud),
                    ox,
                    oy,
                },
            );
            // Pain and ACS FadeTo cover the whole window. The post chain
            // already mixes the palette tint when it is on.
            if let Some(world) = self.play.as_ref().and_then(|g| g.world()) {
                if !use_post {
                    if let Some(tint) = room_ui::screen_blend(self.hud.palette) {
                        hud.fill_rect(0.0, 0.0, fb_w, fb_h, [tint.r, tint.g, tint.b, tint.a]);
                    }
                }
                let fade = world.screen_fade;
                if fade.a > 0.001 {
                    hud.fill_rect(
                        0.0,
                        0.0,
                        fb_w,
                        fb_h,
                        [fade.r, fade.g, fade.b, fade.a.min(1.0)],
                    );
                }
            }

            let Ok(frame) = surface.get_current_texture() else {
                return;
            };
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
            let (cap_w, cap_h) = self
                .config
                .as_ref()
                .map(|c| (c.width, c.height))
                .unwrap_or((1, 1));
            let present_tex = if use_post && !ui_only {
                let mut blend = [0.0_f32, 0.0, 0.0, 0.0];
                if let Some(tint) = room_ui::screen_blend(self.hud.palette) {
                    blend = [tint.r, tint.g, tint.b, tint.a];
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
                        width: cap_w,
                        height: cap_h,
                        time: pass.time,
                    },
                );
                &post_ldr
            } else {
                frame_color
            };
            let color_view = &present_tex.1;
            {
                canvas_gpu.rasterize(&hud, None);
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
                canvas_gpu.blit(gpu, &mut rp);
            }
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
        let tic = 1.0 / self.play.as_ref().map(GameRuntime::tic_rate).unwrap_or(35) as f32;
        self.tic_accum += dt;
        while self.tic_accum >= tic {
            self.tic_accum -= tic;
            let sample = self.sample_input();
            let bits = build_ticcmd_with_mods_and_impulses(
                &sample,
                self.mod_buttons,
                self.weapon_impulses,
            );
            let cmd = TicCmd {
                buttons: Buttons(bits.buttons),
                pitch: bits.pitch,
                yaw: bits.yaw,
                roll: bits.roll,
                forward_move: bits.forward_move,
                side_move: bits.side_move,
                up_move: bits.up_move,
                weapon_select: bits.weapon_select,
            };
            let cmds: Vec<TicCmd> = if let Some(net) = self.net.as_mut() {
                match net.advance_tic(netplay::tic_to_net(cmd)) {
                    Ok(c) => c,
                    Err(e) => {
                        eprintln!("room: netplay: {e}");
                        continue;
                    }
                }
            } else {
                vec![cmd]
            };
            let mut msgs = Vec::new();
            let mut cam = None;
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
                for line in game.drain_script_log() {
                    eprintln!("room: mod {line}");
                }
                entered_intermission =
                    prev_state == GameState::Level && game.state == GameState::Intermission;
                if let Some(vfs) = self.vfs.as_ref() {
                    let events = game.drain_audio(vfs);
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
                    for line in world.drain_debug() {
                        eprintln!("room: {line}");
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
                                            seed ^ 0xB100_D,
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
                    let light_boost =
                        (extralight.clamp(0, 2) * 16 + i32::from(colormap == 1) * 64) as i16;
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
                        if let Some(vid) = view_id {
                            if let Some(a) = world.actors.get(vid) {
                                let z = if p.camera.is_some() {
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
                                let (mut ex, mut ey, ez) = a.camera_eye(z);
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
                    self.cam_prev = self.cam_curr.or(Some(next));
                    self.cam_curr = Some(next);
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
            if let Some(h) = hud_snap {
                if h.health < self.last_player_hp {
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
                self.hud.weapon_class = h.weapon_class.clone();
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
            self.messages.tick();
            self.sync_forward();
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

    /// GLDEFS object-frame lights for actors currently in a lit attack pose.
    fn collect_dyn_lights(&self) -> Vec<DynLightInstance> {
        let mut out = Vec::new();
        let Some(world) = self.play.as_ref().and_then(|g| g.world()) else {
            return out;
        };
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
            let frame_ch = char::from(b'A' + a.frame.min(25));
            let frame_s = frame_ch.to_string();
            for assoc in &self.gldefs.object_lights {
                if !assoc.class.eq_ignore_ascii_case(&class.name) {
                    continue;
                }
                if !assoc.frame.is_empty() && !assoc.frame.eq_ignore_ascii_case(&frame_s) {
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
        let actors: Vec<_> = world.thinkers.default.iter().copied().collect();
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

    /// Actors that have a MODELDEF entry (drawn as meshes when files resolve).
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
            if !self
                .modeldefs
                .by_actor
                .contains_key(&class.name.to_ascii_lowercase())
            {
                continue;
            }
            let light = (actor_light_level(world, a) as f32 / 255.0).clamp(0.2, 1.0);
            #[allow(clippy::cast_possible_truncation)]
            out.push(ModelActorPose {
                class: class.name.clone(),
                origin: Vec3::new(a.pos.x() as f32, a.pos.y() as f32, a.pos.z() as f32),
                light,
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
        self.sprite_curr = next;
    }

    /// Live actor billboards for play mode (`None` in free-fly viewer).
    fn collect_live_sprites(&mut self) -> Option<Vec<SpriteInstance>> {
        self.fill_voxel_poses();
        let game = self.play.as_ref()?;
        let world = game.world()?;
        let local_mo = world
            .players
            .get(usize::from(self.local_seat))
            .and_then(|p| p.as_ref())
            .and_then(|p| p.mo);
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
                let alpha = alpha as f32;
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
                    a.scale_x,
                    a.scale_y,
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
        ) in actors
        {
            if self.actor_draws_voxel(id) {
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
                let rot = sprite_rotation_digit(x, y, angle_deg, view_x, view_y);
                let mut candidates = sprite_lump_candidates_rot(&sprite, frame, rot);
                if let Some(vfs) = self.vfs.as_ref() {
                    // Indexed lookup first: mixed-letter pairs (`BOS2A2C8`).
                    if let Some(hit) = self.patches.sprite_frame_lump(vfs, &sprite, frame, rot) {
                        candidates.insert(0, hit);
                    }
                    for (name, flip) in &candidates {
                        if let Some(patch) = self.patches.ensure(vfs, name) {
                            let pw = (f64::from(patch.width) * scale_x.abs()).clamp(0.25, 4096.0);
                            let ph = (f64::from(patch.height) * scale_y.abs()).clamp(0.25, 4096.0);
                            let top = f64::from(patch.top_offset) * scale_y.abs();
                            let left = f64::from(patch.left_offset) * scale_x.abs();
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
            let orient = if flags & room_sim::FLAG_FLATSPRITE != 0 {
                z_bottom = z;
                SpriteOrient::Flat { yaw }
            } else if flags & room_sim::FLAG_WALLSPRITE != 0 {
                SpriteOrient::Wall { yaw }
            } else {
                SpriteOrient::Billboard
            };
            if std::env::var("ROOM_DEBUG_SPRITE")
                .is_ok_and(|v| v == "*" || sprite.eq_ignore_ascii_case(&v))
            {
                eprintln!(
                    "room: dbgsprite {sprite}{frame} tex={texture} w={w:.1} h={h:.1} pos=({x:.0},{y:.0},{z_bottom:.0}) alpha={alpha} add={additive} light={light}"
                );
            }
            out.push(SpriteInstance {
                pos: Vec3::new(x as f32, y as f32, z_bottom as f32)
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
            });
            if self.render_settings.actor_sprite_shadow && (flags & FLAG_COUNTKILL) != 0 && !fuzz {
                let shadow_tex = particle_tint_tex_name([16, 16, 16]);
                out.push(SpriteInstance {
                    pos: Vec3::new(x as f32, y as f32, z as f32 + 0.5),
                    width: (w as f32 * 0.9).max(8.0),
                    height: (w as f32 * 0.35).max(4.0),
                    light: 0.15,
                    alpha: 0.35,
                    fuzz: false,
                    texture: shadow_tex,
                    flip_u: false,
                    anchor_u: 0.5,
                    orient: SpriteOrient::Flat {
                        yaw: angle_deg.to_radians() as f32,
                    },
                    additive: false,
                });
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

    /// Render displaced views into `__PORTAL*` materials (throttled).
    fn refresh_portal_textures(&mut self, portals: &[room_render::LinkedPortalSurface]) {
        if portals.is_empty() || self.camera_tex_frame % 4 != 2 {
            return;
        }
        let cam = self.camera;
        self.refresh_portal_views(cam, portals, 0);
    }

    /// Recursively fill portal canvases up to [`room_render::MAX_PORTAL_RECURSION`].
    fn refresh_portal_views(
        &mut self,
        cam: room_render::Camera,
        portals: &[room_render::LinkedPortalSurface],
        depth: u32,
    ) {
        if portals.is_empty() || self.gpu.is_none() || self.pass.is_none() {
            return;
        }
        for p in portals {
            let view = cam.shifted_by(p.dx, p.dy);
            let mut scene = build_scene(&self.map, &view, None);
            scene
                .wall_ranges
                .retain(|r| !r.name.starts_with("__MIRROR"));
            scene.mirrors.clear();
            if room_render::portal_view_strips_portals(depth) {
                scene
                    .wall_ranges
                    .retain(|r| !r.name.starts_with("__PORTAL"));
                scene.linked_portals.clear();
            } else {
                let nested = scene.linked_portals.clone();
                self.refresh_portal_views(view, &nested, depth + 1);
            }
            let Some(gpu) = self.gpu.as_ref() else {
                return;
            };
            let Some(pass) = self.pass.as_mut() else {
                return;
            };
            let target = OffscreenTarget::with_format(gpu, 128, 128, pass.format());
            render_offscreen(gpu, pass, &target, &view, &scene);
            let Ok(rgba) = read_rgba(gpu, &target) else {
                continue;
            };
            let _ =
                self.materials
                    .upsert_rgba(&gpu.device, &gpu.queue, &p.tex_name, 128, 128, &rgba);
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

    /// Render reflected views into `__MIRROR*` materials (throttled with cameras).
    fn refresh_mirror_textures(&mut self, mirrors: &[room_render::MirrorSurface]) {
        if mirrors.is_empty() || self.camera_tex_frame % 4 != 1 {
            return;
        }
        let Some(gpu) = self.gpu.as_ref() else {
            return;
        };
        let Some(pass) = self.pass.as_mut() else {
            return;
        };
        let cam = self.camera;
        for m in mirrors {
            let reflected = cam.reflect_across_line(m.ax, m.ay, m.bx, m.by);
            let mut scene = build_scene(&self.map, &reflected, None);
            // Avoid drawing mirror surfaces in the reflected view (stencil portal pass later).
            scene
                .wall_ranges
                .retain(|r| !r.name.starts_with("__MIRROR"));
            scene.mirrors.clear();
            clip_scene_to_mirror_line(
                &mut scene,
                m.ax,
                m.ay,
                m.bx,
                m.by,
                f64::from(reflected.eye.x),
                f64::from(reflected.eye.y),
            );
            let plane = mirror_clip_plane(
                m.ax,
                m.ay,
                m.bx,
                m.by,
                f64::from(reflected.eye.x),
                f64::from(reflected.eye.y),
            );
            pass.set_clip_plane(plane);
            let target = OffscreenTarget::with_format(gpu, 128, 128, pass.format());
            render_offscreen(gpu, pass, &target, &reflected, &scene);
            pass.clear_clip_plane();
            let Ok(rgba) = read_rgba(gpu, &target) else {
                continue;
            };
            let _ =
                self.materials
                    .upsert_rgba(&gpu.device, &gpu.queue, &m.tex_name, 128, 128, &rgba);
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
            let mut stack = MenuStack::pause();
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
            let mut stack = MenuStack::pause();
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
            MenuCommand::QuitConfirmed => event_loop.exit(),
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
                if let Some(rest) = action.strip_prefix("optioncycle ") {
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
                HostCommand::Save(slot) => self.save_play_save(slot.as_deref()),
                HostCommand::Load(slot) => self.load_play_save(slot.as_deref()),
                HostCommand::Record(slot) => self.begin_play_demo(slot.as_deref()),
                HostCommand::StopDemo => self.stop_play_demo(),
                HostCommand::PlayDemo(slot) => self.play_play_demo(slot.as_deref()),
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
                if let Some(p) = h.player {
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
        match game.start_custom_map(next_map, skill) {
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

    fn save_play_save(&mut self, slot: Option<&str>) {
        let slot = save_slot_name(slot);
        let Some(game) = self.play.as_mut() else {
            self.messages.push("save: not in play mode", 70);
            return;
        };
        match game.save_game() {
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
        let Some(game) = self.play.as_mut() else {
            self.messages.push("record: not in play mode", 70);
            return;
        };
        game.begin_demo_record();
        self.messages
            .push(format!("Recording {}", self.demo_slot), 70);
    }

    fn stop_play_demo(&mut self) {
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

    fn mouse_look_enabled(&self) -> bool {
        !matches!(
            self.console.cvars.get("mouse_look"),
            Some(room_console::CvarValue::Bool(false))
        )
    }

    fn title_elapsed_ms(&self) -> u64 {
        self.frames
            .saturating_sub(self.title_anim_origin)
            .saturating_mul(16)
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
        self.title_anim_origin = self.frames;
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
        if self.keys.w {
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
        if self.keys.attack.active() {
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
