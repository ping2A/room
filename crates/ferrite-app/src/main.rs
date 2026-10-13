//! Native desktop map viewer (M4 free-fly shell).

#![cfg_attr(not(test), deny(clippy::unwrap_used))]

mod audio_out;
mod crash;
mod launcher;
mod midi_device;
mod netplay;
mod screen_capture;
mod soundfont;

use audio_out::AudioOut;
use clap::Parser;
use ferrite_console::{Console, HostCommand};
use ferrite_decorate::parse_decorate;
use ferrite_defs::{DefinitionsBuilder, DynLightKind, GlDefs, ModelDefs};
use ferrite_game::{
    GameOptions, GameState, Headless, Skill, SoundSystem, intermission_music_candidates,
    keyconf_alias_lines, keyconf_default_binds, mount_embedded_engine,
    mount_optional_engine_extras, play_first_music_lump, play_map_music, play_music_lump,
    title_music_candidates,
};
use ferrite_input::{AttackLatch, InputSample, Key, build_ticcmd, device_button_is_primary};
use ferrite_map::{MapGeometry, load_map};
use ferrite_nodebuild::{apply_nodes, ensure_nodes};
use ferrite_render::{
    Camera, Canvas, CanvasGpu, CubemapFaces, DecalStore, DynLightInstance, GpuContext, LightMode,
    MaterialCatalog, MaterialMaps, ModelActorPose, ModelCatalog, OffscreenTarget, PARTICLE_TEX,
    ParticleStore, SceneLists, SpriteInstance, UnlitPass, append_actor_models, append_decals,
    append_particles, apply_glow_fullbright, build_scene, build_scene_with_sky_ex_sized,
    clip_scene_to_mirror_line, mirror_clip_plane, read_rgba, read_texture_rgba, render_offscreen,
    scroll_sky_u, sprite_lump_candidates_rot, sprite_rotation_digit,
};
use ferrite_sim::{
    ActorId, Buttons, ClassRegistry, FLAG_CORPSE, FLAG_COUNTKILL, FLAG_DONTDRAW, FLAG_FLOAT,
    FLAG_MISSILE, FLAG_NOGRAVITY, FLAG_SPAWNCEILING, RenderStyle, ShotKind, StateTable, TicCmd,
    actor_light_level, check_sight,
};
use ferrite_testkit::{MapBuilder, combat_arena, rect};
use ferrite_ui::{
    AutomapView, CanvasSink, DoomStatusBar, DrawCmd, FontRegistry, HudState, IntermissionStats,
    MenuCommand, MenuSfx, MenuStack, MessageQueue, PatchCache, Rect, Rgba, StatusBar, UiDrawList,
    blit_draw_list_full, draw_automap, draw_finale, draw_intermission, draw_title, draw_weapon,
};
use ferrite_vfs::{Namespace, Vfs, VfsBuilder};
use glam::Vec3;
use midi_device::{MidiBackend, load_genmidi, resolve_midi_device};
use screen_capture::GifRecorder;
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
#[command(name = "ferrite", about = "Ferrite map viewer / play shell (M4–M5)")]
struct Args {
    /// IWAD / PWAD / directory to mount (optional; uses built-in test room).
    #[arg(long)]
    iwad: Option<PathBuf>,
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
    /// Optional DECORATE file (defaults to testdata/actors/doom_basic.txt when --play).
    #[arg(long)]
    decorate: Option<PathBuf>,
    /// GM SoundFont (`.sf2`/`.sf3`) for MIDI/MUS music. Also searches `soundfonts/`,
    /// `$FERRITE_SOUNDFONT`, and common bank names.
    #[arg(long)]
    soundfont: Option<PathBuf>,
    /// MIDI backend: `auto` (SF2 then OPL), `sf2`, or `opl` (IWAD `GENMIDI`).
    #[arg(long, value_enum, default_value_t = MidiBackend::Auto)]
    mididevice: MidiBackend,
    /// List discovered IWADs and exit (launcher bootstrap).
    #[arg(long, default_value_t = false)]
    launcher: bool,
    /// Import a gzdoom.ini / ferrite.ini before starting.
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
}

fn main() {
    crash::install();
    let mut args = Args::parse();
    if args.host.is_some() && args.join.is_some() {
        eprintln!("ferrite: use only one of --host / --join");
        std::process::exit(2);
    }
    if args.host.is_some() || args.join.is_some() {
        args.play = true;
        args.title = false;
        if args.iwad.is_none() {
            let doom1 = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/doom/doom1.wad");
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
            eprintln!("ferrite: found gzdoom.ini candidate {}", p.display());
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
                eprintln!("ferrite: auto IWAD {}", c.path.display());
                args.iwad = Some(c.path.clone());
            }
        }
    }
    let (map, vfs) = match load_viewer_map(&args) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("ferrite: {e}");
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
            eprintln!("ferrite: event loop: {e}");
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
            eprintln!("ferrite: audio output started");
            Some(a)
        }
        Err(e) => {
            eprintln!("ferrite: audio: {e}");
            None
        }
    };
    let audio_rate = audio
        .as_ref()
        .map(|a| a.sample_rate)
        .unwrap_or(ferrite_audio::ENGINE_SAMPLE_RATE);
    let iwad_label = args
        .iwad
        .as_ref()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "builtin".into());
    let fingerprint =
        netplay::session_fingerprint(&iwad_label, &args.map, skill.0);
    let mut net: Option<netplay::NetPlay> = None;
    let mut local_seat: u8 = 0;
    let mut coop_players: u8 = 1;
    let mut rng_seed: u32 = 1993;
    if let Some(bind) = args.host.as_deref() {
        let addr = match netplay::parse_host_bind(bind) {
            Ok(a) => a,
            Err(e) => {
                eprintln!("ferrite: --host: {e}");
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
                eprintln!("ferrite: host failed: {e}");
                std::process::exit(1);
            }
        }
    } else if let Some(join) = args.join.as_deref() {
        let addr = match netplay::parse_join_addr(join) {
            Ok(a) => a,
            Err(e) => {
                eprintln!("ferrite: --join: {e}");
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
                        "ferrite: warning: host map {map_name} != local --map {}",
                        args.map
                    );
                }
                local_seat = n.local.0;
                coop_players = n.peer_count;
                rng_seed = n.rng_seed;
                net = Some(n);
            }
            Err(e) => {
                eprintln!("ferrite: join failed: {e}");
                std::process::exit(1);
            }
        }
    }
    let (play, decaldef) = if args.play {
        let (mut game, decaldef) = start_play_session(
            &map,
            args.decorate.as_deref(),
            vfs.as_ref(),
            skill,
            audio_rate,
            rng_seed,
            coop_players,
        );
        // Default: jump straight into `--map` so WASD/playsim work without a menu dig.
        if !args.title {
            if let Err(e) = game.begin_pending_map() {
                eprintln!("ferrite: start map: {e}");
                std::process::exit(1);
            }
            let label = if args.combat_demo {
                "combat arena"
            } else {
                args.map.as_str()
            };
            eprintln!(
                "ferrite: entered {label} (skill {}, coop={coop_players}, seat={local_seat})",
                skill.0
            );
            if args.give_all {
                for seat in 0..coop_players {
                    let n = game.cheat_give(seat as usize, "all");
                    eprintln!("ferrite: give all → seat {seat}: {n} items");
                }
            }
            if net.is_some() {
                eprintln!(
                    "ferrite: netplay — WASD/mouse control seat {local_seat}; lockstep with {coop_players} peers"
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
                    "ferrite: combat demo — WASD move, mouse look, Ctrl/F/LMB fire, 1–7 weapons, E use, L toggle look"
                );
            } else {
                eprintln!("ferrite: use --title for the main menu");
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
        .map(|g| MenuStack::from_menudef_or_doom(g.menudef()));
    let (skyboxes, initial_sky, initial_cube) = load_sky_defs(vfs.as_ref(), &args.map);
    let gldefs = load_gldefs(vfs.as_ref());
    let modeldefs = load_modeldefs(vfs.as_ref());
    let camera_tex_sizes = load_camera_tex_sizes(vfs.as_ref());
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
        decaldef,
        sky_texture: initial_sky,
        sky_cubemap: initial_cube,
        skyboxes,
        gldefs,
        modeldefs,
        models: ModelCatalog::new(),
        camera_tex_sizes,
        camera_tex_frame: 0,
        window: None,
        surface: None,
        config: None,
        gpu: None,
        pass: None,
        depth: None,
        stencil_aux: None,
        frame_color: None,
        canvas_gpu: None,
        camera: Camera::default(),
        pending_screenshot: false,
        gif_recorder: None,
        keys: Keys::default(),
        yaw: 0.0,
        pitch: 0.0,
        mouse_look: false,
        ignore_warp_motion: false,
        menu_cursor: (160.0, 100.0),
        last_frame: std::time::Instant::now(),
        frames: 0,
        fps_window: std::time::Instant::now(),
        fps_count: 0,
        automap: false,
        hud: HudState::doom_default(),
        play,
        net,
        local_seat,
        tic_accum: 0.0,
        cam_prev: None,
        cam_curr: None,
        messages: MessageQueue::new(),
        last_player_hp: 100,
        mouse_yaw_accum: 0,
        mouse_pitch_accum: 0,
        console: {
            let mut con = Console::with_defaults();
            // Load user ferrite.ini, optional --import-ini, then first gzdoom.ini.
            let cfg = dirs_ferrite_ini();
            if cfg.is_file() {
                if let Ok(r) = ferrite_console::load_ini_file(&mut con.cvars, &cfg) {
                    eprintln!("ferrite: loaded {} ({} cvars)", cfg.display(), r.set);
                }
            }
            if let Some(path) = &args.import_ini {
                match ferrite_console::load_ini_file(&mut con.cvars, path) {
                    Ok(r) => eprintln!(
                        "ferrite: imported {} ({} cvars, {} binds)",
                        path.display(),
                        r.set,
                        r.binds.len()
                    ),
                    Err(e) => eprintln!("ferrite: import-ini: {e}"),
                }
            } else if let Some(gz) = launcher::discover_gzdoom_ini().into_iter().next() {
                if let Ok(r) = ferrite_console::load_ini_file(&mut con.cvars, &gz) {
                    eprintln!("ferrite: imported {} ({} cvars)", gz.display(), r.set);
                }
            }
            con
        },
        console_open: false,
        console_line: String::new(),
        keyconf_binds: std::collections::BTreeMap::new(),
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
        eprintln!("ferrite: {e}");
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
            "ferrite: no music synth (pass --soundfont, or --iwad with GENMIDI, or --mididevice opl)"
        );
        return;
    };
    let Ok(mut shared) = audio.shared.lock() else {
        return;
    };
    if let Some((path, font)) = sf {
        shared.music.set_softfont(font);
        if device == ferrite_music::MidiDevice::SoftFont {
            eprintln!("ferrite: soundfont {}", path.display());
        }
    }
    if let Some(bank) = genmidi {
        shared.music.set_genmidi(bank);
        if device == ferrite_music::MidiDevice::Opl {
            eprintln!("ferrite: GENMIDI → OPL MIDI device");
        }
    }
    shared.music.set_mididevice(device);
    eprintln!("ferrite: mididevice {backend} → {device:?}");
}

fn dirs_ferrite_ini() -> PathBuf {
    #[cfg(target_os = "macos")]
    {
        if let Some(home) = std::env::var_os("HOME") {
            return PathBuf::from(home)
                .join("Library")
                .join("Application Support")
                .join("ferrite")
                .join("ferrite.ini");
        }
    }
    #[cfg(target_os = "windows")]
    {
        if let Some(app) = std::env::var_os("APPDATA") {
            return PathBuf::from(app).join("ferrite").join("ferrite.ini");
        }
    }
    if let Some(xdg) = std::env::var_os("XDG_CONFIG_HOME") {
        return PathBuf::from(xdg).join("ferrite").join("ferrite.ini");
    }
    if let Some(home) = std::env::var_os("HOME") {
        return PathBuf::from(home)
            .join(".config")
            .join("ferrite")
            .join("ferrite.ini");
    }
    PathBuf::from("ferrite.ini")
}

fn default_freedoom2() -> Option<PathBuf> {
    let candidates = [
        PathBuf::from("testdata/freedoom/freedoom2.wad"),
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/freedoom/freedoom2.wad"),
    ];
    candidates.into_iter().find(|p| p.is_file())
}

/// Prefer the requested map; if missing, fall back Doom1 `E1M1` or Doom2 `MAP01`.
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
                eprintln!("ferrite: embedded pk3: {e}");
                VfsBuilder::new()
            }
        };
        let builder = mount_optional_engine_extras(builder, &engine_extra_search_roots());
        let vfs = builder
            .mount_path(&path)
            .map_err(|e| e.to_string())?
            .build()
            .map_err(|e| e.to_string())?;
        let mut map = combat_arena()
            .build_udmf("MAP01")
            .map_err(|e| e.to_string())?;
        apply_nodes(&mut map);
        eprintln!("ferrite: combat arena + {}", path.display());
        return Ok((map, Some(vfs)));
    }
    if let Some(path) = &args.iwad {
        let builder = match mount_embedded_engine(VfsBuilder::new()) {
            Ok(b) => b,
            Err(e) => {
                eprintln!("ferrite: embedded pk3: {e}");
                VfsBuilder::new()
            }
        };
        let builder = mount_optional_engine_extras(builder, &engine_extra_search_roots());
        let vfs = builder
            .mount_path(path)
            .map_err(|e| e.to_string())?
            .build()
            .map_err(|e| e.to_string())?;
        // Doom 1 / Freedoom Phase 1 use ExMy; Doom II uses MAPxx.
        let map_name = resolve_start_map(&vfs, &args.map);
        if map_name != args.map {
            eprintln!(
                "ferrite: map {} not in IWAD — starting {}",
                args.map, map_name
            );
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
    shift: bool,
    /// Turn left (← / Q when not in a menu).
    left: bool,
    /// Turn right (→).
    right: bool,
    /// Attack held (Ctrl / mouse left in play mode).
    attack: AttackLatch,
    /// Use (E).
    use_btn: bool,
    /// Weapon slots 1..=7 edge (cleared after sample).
    weapon: u8,
    /// Next/prev weapon edges (cleared after sample).
    next_weapon: bool,
    prev_weapon: bool,
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
    /// DECALDEF registry for fader / pic lookup (`--play`).
    decaldef: Option<ferrite_defs::DecalInfo>,
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
    /// ANIMDEFS `cameratexture` sizes (uppercase name → w×h).
    camera_tex_sizes: std::collections::BTreeMap<String, (u32, u32)>,
    /// Frame counter for throttling camera-texture refreshes.
    camera_tex_frame: u32,
    window: Option<Arc<Window>>,
    surface: Option<wgpu::Surface<'static>>,
    config: Option<wgpu::SurfaceConfiguration>,
    gpu: Option<GpuContext>,
    pass: Option<UnlitPass>,
    depth: Option<(wgpu::Texture, wgpu::TextureView)>,
    /// Dummy R8 MRT target (Metal stencil / unlit MRT pass layout).
    stencil_aux: Option<(wgpu::Texture, wgpu::TextureView)>,
    /// Colour buffer with `COPY_SRC` (surface itself often cannot be read back).
    frame_color: Option<(wgpu::Texture, wgpu::TextureView)>,
    canvas_gpu: Option<CanvasGpu>,
    camera: Camera,
    /// Capture PNG of the next presented frame.
    pending_screenshot: bool,
    /// When set, append scaled frames for GIF encode on stop.
    gif_recorder: Option<GifRecorder>,
    keys: Keys,
    yaw: f32,
    pitch: f32,
    mouse_look: bool,
    /// Skip the next `DeviceEvent::MouseMotion` (macOS cursor recenter warp).
    ignore_warp_motion: bool,
    /// Cursor in virtual 320×200 UI space (menus).
    menu_cursor: (f32, f32),
    last_frame: std::time::Instant,
    frames: u64,
    fps_window: std::time::Instant,
    fps_count: u32,
    /// Tab toggles classic automap overlay.
    automap: bool,
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
    /// On-screen messages from the sim.
    messages: MessageQueue,
    /// Previous local player health (to announce invisible attackers).
    last_player_hp: i32,
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
    /// Native audio output (mixer + music on the device thread).
    audio: Option<AudioOut>,
}

fn load_camera_tex_sizes(vfs: Option<&Vfs>) -> std::collections::BTreeMap<String, (u32, u32)> {
    let mut out = std::collections::BTreeMap::new();
    let Some(vfs) = vfs else {
        return out;
    };
    let Ok(defs) = DefinitionsBuilder::new(vfs).build() else {
        return out;
    };
    for cam in &defs.animdefs.camera_textures {
        let w = cam.width.clamp(1, 256) as u32;
        let h = cam.height.clamp(1, 256) as u32;
        out.insert(cam.name.clone(), (w, h));
    }
    out
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

fn start_play_session(
    map: &MapGeometry,
    decorate: Option<&std::path::Path>,
    vfs: Option<&Vfs>,
    skill: Skill,
    audio_rate: u32,
    rng_seed: u32,
    coop_players: u8,
) -> (Headless, Option<ferrite_defs::DecalInfo>) {
    let mut game = Headless::new(GameOptions {
        skill,
        rng_seed,
        record_demo: false,
        coop_players: coop_players.max(1),
        ..GameOptions::default()
    });
    let mut decaldef = None;
    if let Some(vfs) = vfs {
        if let Ok(defs) = DefinitionsBuilder::new(vfs).build() {
            game.set_complevel(defs.complevel);
            game.set_mapinfo(Arc::new(defs.mapinfo));
            game.set_terrain(&defs.terrain);
            game.set_sndseq(&defs.sndseq);
            game.set_lockdefs(&defs.lockdefs, Some("Doom"));
            game.set_decaldef(&defs.decaldef);
            decaldef = Some(defs.decaldef.clone());
            game.set_keyconf(&defs.keyconf);
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
                game.set_acs_libraries(libs);
            }
        }
    }
    if let Some(path) = decorate {
        let src = match std::fs::read_to_string(path) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("ferrite: decorate {path:?}: {e}");
                std::process::exit(1);
            }
        };
        let ir = match parse_decorate(&src) {
            Ok(i) => i,
            Err(e) => {
                eprintln!("ferrite: decorate parse: {e}");
                std::process::exit(1);
            }
        };
        let classes = match ClassRegistry::from_decorate(&ir) {
            Ok(c) => Arc::new(c),
            Err(e) => {
                eprintln!("ferrite: class registry: {e}");
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
        eprintln!("ferrite: loaded embedded engine data (ferrite.pk3)");
    } else {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let ir = match parse_decorate(src) {
            Ok(i) => i,
            Err(e) => {
                eprintln!("ferrite: decorate parse: {e}");
                std::process::exit(1);
            }
        };
        let classes = match ClassRegistry::from_decorate(&ir) {
            Ok(c) => Arc::new(c),
            Err(e) => {
                eprintln!("ferrite: class registry: {e}");
                std::process::exit(1);
            }
        };
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        game.set_defs(classes, states);
    }
    if let Some(vfs) = vfs {
        let mut sounds = SoundSystem::with_sample_rate(audio_rate);
        match sounds.load_sndinfo_from_vfs(vfs) {
            Ok(()) => {
                game.set_sounds(sounds);
                eprintln!("ferrite: SNDINFO loaded ({audio_rate} Hz)");
            }
            Err(e) => eprintln!("ferrite: SNDINFO: {e}"),
        }
    }
    game.set_pending_map(map.clone());
    eprintln!(
        "ferrite: play mode — WASD move, ←→/Q turn, mouse look, E use, Ctrl/LMB attack, Tab automap, Esc pause, F6–F9 cheats, F12 screenshot, F11 GIF"
    );
    (game, decaldef)
}

fn draw_console_overlay(list: &mut UiDrawList, console: &Console, line: &str) {
    use ferrite_ui::{Rect, Rgba, Vec2};
    list.fill(
        Rect::new(0.0, 0.0, 320.0, 72.0),
        Rgba::new(0.0, 0.0, 0.0, 0.72),
    );
    let mut y = 4.0;
    for msg in console
        .scrollback
        .iter()
        .rev()
        .take(4)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
    {
        list.text(
            "SmallFont",
            Vec2::new(4.0, y),
            msg.as_str(),
            Rgba::new(0.85, 0.85, 0.85, 1.0),
            1.0,
        );
        y += 12.0;
    }
    list.text(
        "SmallFont",
        Vec2::new(4.0, 58.0),
        format!("]{line}_"),
        Rgba::new(1.0, 1.0, 0.55, 1.0),
        1.0,
    );
}

fn try_load_engine_data() -> Option<ferrite_game::EngineData> {
    match ferrite_game::load_embedded_engine_data() {
        Ok(data) => return Some(data),
        Err(e) => eprintln!("ferrite: embedded engine data: {e}"),
    }
    let candidates = [
        std::path::PathBuf::from("wadsrc/ferrite"),
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../wadsrc/ferrite"),
        std::path::PathBuf::from("target/ferrite.pk3"),
    ];
    for path in candidates {
        if !path.exists() {
            continue;
        }
        let vfs = ferrite_vfs::VfsBuilder::new()
            .mount_path(&path)
            .ok()?
            .build()
            .ok()?;
        match ferrite_game::load_engine_data(&vfs) {
            Ok(data) => return Some(data),
            Err(e) => eprintln!("ferrite: engine data {}: {e}", path.display()),
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
            .with_title("Ferrite map viewer")
            .with_inner_size(LogicalSize::new(1280.0, 720.0));
        #[cfg(target_os = "macos")]
        let attrs = {
            use winit::platform::macos::WindowAttributesExtMacOS;
            attrs.with_accepts_first_mouse(true)
        };
        let window = match event_loop.create_window(attrs) {
            Ok(w) => Arc::new(w),
            Err(e) => {
                eprintln!("ferrite: window: {e}");
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
                eprintln!("ferrite: surface: {e}");
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
                    eprintln!("ferrite: adapter: {e}");
                    event_loop.exit();
                    return;
                }
            };
        let gpu = match pollster::block_on(GpuContext::from_adapter(instance, adapter)) {
            Ok(g) => g,
            Err(e) => {
                eprintln!("ferrite: gpu: {e}");
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
            eprintln!("ferrite: no surface formats");
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
        self.depth = Some(depth);
        self.stencil_aux = Some(stencil_aux);
        self.frame_color = Some(frame_color);
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
                    KeyCode::ShiftLeft | KeyCode::ShiftRight if !menu_open => {
                        self.keys.shift = down
                    }
                    KeyCode::ControlLeft | KeyCode::ControlRight | KeyCode::KeyF if !menu_open => {
                        self.keys.attack.set_key(down);
                    }
                    KeyCode::KeyE if !menu_open => self.keys.use_btn = down,
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
                    KeyCode::BracketRight if down && !menu_open => self.keys.next_weapon = true,
                    KeyCode::BracketLeft if down && !menu_open => self.keys.prev_weapon = true,
                    KeyCode::Tab if down && !menu_open => self.automap = !self.automap,
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
                        if down {
                            if let Some(name) = keycode_keyconf_name(code) {
                                if let Some(cmd) = self.keyconf_binds.get(name).cloned() {
                                    self.apply_console(&cmd);
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
                // Viewer: hold RMB to look. Play: look stays on (RMB must not
                // touch attack or release grab — that was breaking LMB on macOS).
                if self.play.is_none() {
                    self.set_play_look(state == ElementState::Pressed);
                }
            }
            WindowEvent::Focused(focused) => {
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
            WindowEvent::RedrawRequested => self.frame(),
            _ => {}
        }
    }

    fn device_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _device_id: winit::event::DeviceId,
        event: DeviceEvent,
    ) {
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
                        let rad_per_count = (ferrite_input::MOUSE_DEG_PER_COUNT as f32)
                            * (std::f32::consts::PI / 180.0);
                        let invert_y = matches!(
                            self.console.cvars.get("invertmouse"),
                            Some(ferrite_console::CvarValue::Bool(true))
                        );
                        let sens = match self.console.cvars.get("m_sensitivity") {
                            Some(ferrite_console::CvarValue::Float(f)) => {
                                (*f).clamp(0.1, 5.0) as f32
                            }
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
        // Solid white for ACS SpawnParticle billboards.
        let white = vec![255u8; 8 * 8 * 4];
        let _ = self
            .materials
            .upsert_rgba(&gpu.device, &gpu.queue, PARTICLE_TEX, 8, 8, &white);
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
                        eprintln!("ferrite: bound albedo {name}");
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
        eprintln!("ferrite: preloaded {ok} map+sprite materials ({sprites_ok} monster frames)");
    }

    fn sync_forward(&mut self) {
        let cp = self.pitch.cos();
        self.camera.forward = Vec3::new(self.yaw.cos() * cp, self.yaw.sin() * cp, self.pitch.sin())
            .normalize_or_zero();
        if self.camera.forward.length_squared() < 1e-6 {
            self.camera.forward = Vec3::X;
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
                w.set_title(&format!("Ferrite {mode} — {fps} fps"));
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
        let map_ref = self
            .play
            .as_ref()
            .and_then(|g| g.world())
            .map(|w| w.map.as_ref());
        if in_level {
            append_decals(&mut scene, &self.decals, &self.camera, map_ref);
            append_particles(&mut scene, &self.particles, &self.camera);
            if let (Some(vfs), poses) = (self.vfs.as_ref(), self.collect_model_poses()) {
                if !poses.is_empty() && !self.modeldefs.by_actor.is_empty() {
                    append_actor_models(&mut scene, &mut self.models, vfs, &self.modeldefs, &poses);
                }
            }
        }
        self.refresh_mirror_textures(&scene.mirrors);
        self.refresh_portal_textures(&scene.linked_portals);
        self.refresh_plane_reflect_textures(&scene.plane_reflects);
        let want_shot = self.pending_screenshot;
        let want_gif = self.gif_recorder.is_some();
        let capture_rgba = {
        let (
            Some(surface),
            Some(gpu),
            Some(pass),
            Some(depth),
            Some(stencil_aux),
            Some(frame_color),
            Some(canvas_gpu),
            materials,
            vfs,
        ) = (
            &self.surface,
            &self.gpu,
            &mut self.pass,
            &self.depth,
            &self.stencil_aux,
            &self.frame_color,
            &mut self.canvas_gpu,
            &mut self.materials,
            self.vfs.as_ref(),
        )
        else {
            return;
        };
        pass.set_fog_density(fog_density);
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
                let has_titlepic = self.vfs.as_ref().is_some_and(|v| {
                    v.find("TITLEPIC", Namespace::Global).is_some()
                        || v.find_any_ns("TITLEPIC").is_some()
                });
                draw_title(&mut ui, 320.0, 200.0, has_titlepic);
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
                draw_finale(&mut ui, map_name, 320.0, 200.0);
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
                    // Classic follow-mode zoom (~64 mu across half-width ≈ Doom default feel).
                    let view = AutomapView {
                        center_x: f64::from(self.camera.eye.x),
                        center_y: f64::from(self.camera.eye.y),
                        scale: 0.35,
                        screen_x: 0.0,
                        screen_y: 0.0,
                        width: 320.0,
                        height: 168.0,
                        angle_deg: self.yaw.to_degrees(),
                        allmap,
                    };
                    draw_automap(&self.map, &view, &mut ui);
                } else {
                    if let Some(vfs) = self.vfs.as_ref() {
                        if !self.hud.weapon_patch.is_empty() {
                            let _ = self.patches.ensure(vfs, &self.hud.weapon_patch);
                        }
                        if !self.hud.flash_patch.is_empty() {
                            let _ = self.patches.ensure(vfs, &self.hud.flash_patch);
                        }
                    }
                    let soft = self.patches.get(&self.hud.weapon_patch);
                    draw_weapon(
                        &mut ui,
                        &self.hud.weapon_patch,
                        self.hud.fullscreen,
                        soft,
                        self.hud.bob_x,
                        self.hud.bob_y,
                    );
                    if !self.hud.flash_patch.is_empty() {
                        let soft_flash = self.patches.get(&self.hud.flash_patch);
                        draw_weapon(
                            &mut ui,
                            &self.hud.flash_patch,
                            self.hud.fullscreen,
                            soft_flash,
                            self.hud.bob_x,
                            self.hud.bob_y,
                        );
                    }
                }
                DoomStatusBar.draw(&self.hud, &mut ui);
                self.messages.draw(&mut ui, 8.0, 8.0);
                if let Some(menu) = &self.menu {
                    let tic = self
                        .play
                        .as_ref()
                        .and_then(|g| g.world())
                        .map(|w| w.level.maptime)
                        .unwrap_or(self.frames as u32);
                    menu.draw(&mut ui, tic, Some(&self.fonts));
                }
                if let Some(fade) = self
                    .play
                    .as_ref()
                    .and_then(|g| g.world())
                    .map(|w| w.screen_fade)
                {
                    if fade.a > 0.001 {
                        ui.fill(
                            Rect::new(0.0, 0.0, 320.0, 200.0),
                            Rgba {
                                r: fade.r,
                                g: fade.g,
                                b: fade.b,
                                a: fade.a,
                            },
                        );
                    }
                }
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

        let Ok(frame) = surface.get_current_texture() else {
            return;
        };
        // Draw into a COPY_SRC colour buffer, then blit to the swapchain.
        let color_view = &frame_color.1;
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
                        view: color_view,
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
            }
        }
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
        let (cap_w, cap_h) = self
            .config
            .as_ref()
            .map(|c| (c.width, c.height))
            .unwrap_or((1, 1));
        encoder.copy_texture_to_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &frame_color.0,
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
            match read_texture_rgba(gpu, &frame_color.0, cap_w, cap_h) {
                Ok(rgba) => Some((rgba, cap_w, cap_h)),
                Err(e) => {
                    eprintln!("ferrite: capture failed: {e}");
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
                }
                if let Some(rec) = self.gif_recorder.as_mut() {
                    let n = rec.frames.len();
                    if rec.push_rgba(&rgba, w, h)
                        && n + 1 >= screen_capture::GIF_MAX_FRAMES
                    {
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

    /// Drive Headless at 35 Hz from keyboard / mouse samples.
    fn tick_play(&mut self, dt: f32) {
        const TIC: f32 = 1.0 / 35.0;
        self.tic_accum += dt;
        while self.tic_accum >= TIC {
            self.tic_accum -= TIC;
            let sample = self.sample_input();
            let bits = build_ticcmd(&sample);
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
                        eprintln!("ferrite: netplay: {e}");
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
            let mut entered_intermission = false;
            if let Some(game) = self.play.as_mut() {
                let prev_state = game.state;
                game.tick(&cmds);
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
                            if let Some(p) = game.world().and_then(|w| w.players.get(seat).and_then(|p| p.as_ref())) {
                                if let Some(mo) = p.mo {
                                    if let Some(a) = game.world().and_then(|w| w.actors.get(mo)) {
                                        shared.mixer.listener = a.pos;
                                    }
                                }
                            }
                            let paused = game.world().is_some_and(|w| w.paused);
                            shared.music.set_paused(paused);
                            if let Some(song) = music_req.as_deref() {
                                if let Err(e) = play_music_lump(&mut shared.music, vfs, song) {
                                    eprintln!("ferrite: SetMusic {song}: {e}");
                                }
                            }
                        }
                    }
                }
                if let Some(world) = game.world_mut() {
                    msgs = world.drain_messages();
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
                        .map(|e| (e.color, e.fullbright, e.lifetime, e.size, e.x, e.y, e.z))
                        .collect();
                    let (extralight, infrared) = world
                        .players
                        .first()
                        .and_then(|p| p.as_ref())
                        .map(|p| (p.extralight, p.infrared))
                        .unwrap_or((0, false));
                    // Doom: extralight<<4; infrared roughly fills to mid-bright.
                    let light_boost =
                        (extralight.clamp(0, 2) * 16 + i32::from(infrared) * 64) as i16;
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
                                let fov = if p.camera.is_some() && a.camera_fov > 0.0 {
                                    Some(a.camera_fov)
                                } else {
                                    None
                                };
                                cam = Some((
                                    a.pos.x(),
                                    a.pos.y(),
                                    z,
                                    a.angle.radians(),
                                    a.pitch.radians(),
                                    fov,
                                ));
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
            let frac = (self.tic_accum / TIC).clamp(0.0, 1.0) as f64;
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
                self.messages.push(msg, 70);
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
                        ferrite_render::DecalIngestAnim {
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
                self.decals.ingest_one(tex, x, y, z, line, anim);
            }
            self.decals.tick();
            self.particles.ingest(new_particles);
            self.particles.tick();
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
                    let msg = format!(
                        "HIT {}→{} by {who}",
                        self.last_player_hp, h.health
                    );
                    eprintln!("ferrite: {msg}");
                    for line in detail {
                        eprintln!("ferrite:   {line}");
                    }
                    self.messages.push(msg, 70);
                }
                self.last_player_hp = h.health;
                self.hud.health = h.health;
                self.hud.armor = h.armor;
                self.hud.ammo = h.ammo;
                self.hud.ammo_max = h.ammo_max;
                self.hud.keys = h.keys;
                self.hud.face = h.face;
                self.hud.weapon_patch = h.weapon_patch;
                self.hud.flash_patch = h.flash_patch;
                self.hud.bob_x = h.bob_x;
                self.hud.bob_y = h.bob_y;
            }
            self.messages.tick();
            self.sync_forward();
        }
        // Smooth eye XYZ between tics so stair steps don't snap every 1/35s.
        // Yaw/pitch are locked to the sim after each tic; mouse still previews mid-tic.
        let frac = (self.tic_accum / TIC).clamp(0.0, 1.0);
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
                    if def.kind == ferrite_defs::DynLightKind::Spot && def.spot_angle > 0.0 {
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
            if a.destroyed
                || Some(id) == local_mo
                || ((a.flags | class_flags) & FLAG_DONTDRAW) != 0
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

        let sid = ferrite_nodebuild::point_in_sector(&world.map, px, py)
            .or_else(|| ferrite_nodebuild::point_in_sector_vanilla(&world.map, px, py));
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
                let infl = if !hit.inflictor_class.is_empty()
                    && hit.inflictor_class != hit.source_class
                {
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
                if let Some(line) =
                    self.describe_hit_actor(world, mo, id, px, py, yaw, half_fov)
                {
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
                if let Some(line) =
                    self.describe_hit_actor(world, mo, id, px, py, yaw, half_fov)
                {
                    detail.push(format!("hunter {line}"));
                }
                if who == "unknown"
                    || who.starts_with("sector ")
                {
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
                if let Some(line) =
                    self.describe_hit_actor(world, mo, id, px, py, yaw, half_fov)
                {
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
        world: &ferrite_sim::World,
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
                || self
                    .vfs
                    .as_ref()
                    .is_some_and(|v| {
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
            if (flags & FLAG_MISSILE) != 0 { "Y" } else { "N" },
            if (flags & FLAG_CORPSE) != 0 || a.health <= 0 {
                "Y"
            } else {
                "N"
            },
            if patch_ok { "Y" } else { "N" },
            if gpu_ok { "Y" } else { "N" },
        ))
    }

    /// Live actor billboards for play mode (`None` in free-fly viewer).
    fn collect_live_sprites(&mut self) -> Option<Vec<SpriteInstance>> {
        let game = self.play.as_ref()?;
        let world = game.world()?;
        let local_mo = world
            .players
            .get(usize::from(self.local_seat))
            .and_then(|p| p.as_ref())
            .and_then(|p| p.mo);
        let view_x = f64::from(self.camera.eye.x);
        let view_y = f64::from(self.camera.eye.y);
        let mut out = Vec::new();
        let actors: Vec<_> = world
            .thinkers
            .default
            .iter()
            .filter_map(|&id| {
                let a = world.actors.get(id)?;
                let class_flags = world.classes.get(a.class).map(|c| c.flags).unwrap_or(0);
                let flags = a.flags | class_flags;
                if a.destroyed
                    || Some(id) == local_mo
                    || (flags & FLAG_DONTDRAW) != 0
                {
                    return None;
                }
                let is_other_player = world
                    .players
                    .iter()
                    .flatten()
                    .any(|p| p.mo == Some(id) && Some(id) != local_mo);
                let bright = a
                    .state
                    .and_then(|sid| world.states.get(sid))
                    .is_some_and(|st| st.bright);
                let is_monster = (flags & FLAG_COUNTKILL) != 0;
                // Imp/caco balls etc. — keep readable against dark flats.
                let is_missile = a.sprite.starts_with("BAL")
                    || a.sprite.starts_with("PLAS")
                    || a.sprite.starts_with("MISL")
                    || a.sprite.starts_with("ROKE");
                let mut light = (actor_light_level(world, a) as f32 / 255.0).clamp(0.2, 1.0);
                // COUNTKILL always fullbright: DoomSoftware + dark computer-room
                // sectors made zombies unreadable ("invisible killer after the door").
                if is_monster || is_missile || bright || is_other_player {
                    light = 1.0;
                }
                let alpha = a.alpha.clamp(0.0, 1.0);
                let fuzz = matches!(a.render_style, RenderStyle::Fuzzy | RenderStyle::OptFuzzy);
                #[allow(clippy::cast_possible_truncation)]
                let alpha = alpha as f32;
                Some((
                    a.pos.x(),
                    a.pos.y(),
                    a.pos.z(),
                    a.angle.degrees(),
                    a.radius,
                    a.height,
                    a.scale_x,
                    a.scale_y,
                    light,
                    alpha,
                    fuzz,
                    a.sprite.clone(),
                    a.frame,
                    flags,
                ))
            })
            .collect();
        for (
            x,
            y,
            z,
            angle_deg,
            radius,
            height,
            scale_x,
            scale_y,
            light,
            alpha,
            fuzz,
            sprite,
            frame,
            flags,
        ) in actors
        {
            // Prefer patch pixel size (1 map unit per texel). Floor items / monsters
            // must keep Doom topoffset math — do not inflate height independently.
            let mut w = (radius * 2.0 * scale_x.abs()).clamp(1.0, 256.0);
            let mut h = (height * scale_y.abs()).clamp(1.0, 256.0);
            let mut texture = String::new();
            let mut flip_u = false;
            let mut anchor_u = 0.5_f32;
            // Doom: billboard top = actor.z + topoffset; bottom = top - spriteheight.
            let mut z_bottom = z;
            let grounded = (flags & (FLAG_NOGRAVITY | FLAG_SPAWNCEILING | FLAG_FLOAT)) == 0;
            if !sprite.is_empty() {
                let rot = sprite_rotation_digit(x, y, angle_deg, view_x, view_y);
                let candidates = sprite_lump_candidates_rot(&sprite, frame, rot);
                if let Some(vfs) = self.vfs.as_ref() {
                    for (name, flip) in &candidates {
                        if let Some(patch) = self.patches.ensure(vfs, name) {
                            let pw = (f64::from(patch.width) * scale_x.abs()).clamp(1.0, 256.0);
                            let ph = (f64::from(patch.height) * scale_y.abs()).clamp(1.0, 256.0);
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
                if texture.is_empty() {
                    if let Some((name, flip)) = candidates.first() {
                        texture = name.clone();
                        flip_u = *flip;
                    }
                }
            } else if grounded {
                z_bottom = z + 1.0;
            }
            #[allow(clippy::cast_possible_truncation)]
            out.push(SpriteInstance {
                pos: Vec3::new(x as f32, y as f32, z_bottom as f32),
                width: w as f32,
                height: h as f32,
                light,
                alpha,
                fuzz,
                texture,
                flip_u,
                anchor_u,
            });
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
    fn refresh_portal_textures(&mut self, portals: &[ferrite_render::LinkedPortalSurface]) {
        if portals.is_empty() || self.camera_tex_frame % 4 != 2 {
            return;
        }
        let cam = self.camera;
        self.refresh_portal_views(cam, portals, 0);
    }

    /// Recursively fill portal canvases up to [`ferrite_render::MAX_PORTAL_RECURSION`].
    fn refresh_portal_views(
        &mut self,
        cam: ferrite_render::Camera,
        portals: &[ferrite_render::LinkedPortalSurface],
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
            if ferrite_render::portal_view_strips_portals(depth) {
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
            let target = OffscreenTarget::new(gpu, 128, 128);
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
    fn refresh_plane_reflect_textures(&mut self, planes: &[ferrite_render::PlaneReflectSurface]) {
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
            let target = OffscreenTarget::new(gpu, 128, 128);
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
    fn refresh_mirror_textures(&mut self, mirrors: &[ferrite_render::MirrorSurface]) {
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
            let target = OffscreenTarget::new(gpu, 128, 128);
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
        let binds: Vec<(String, ferrite_sim::CameraTextureBind)> = world
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
            let target = OffscreenTarget::new(gpu, w, h);
            render_offscreen(gpu, pass, &target, &cam, &scene);
            let Ok(rgba) = read_rgba(gpu, &target) else {
                continue;
            };
            let _ = self
                .materials
                .upsert_rgba(&gpu.device, &gpu.queue, &name, w, h, &rgba);
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
                    self.set_play_look(true);
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
                                    let defs = game.menudef().cloned();
                                    self.menu =
                                        Some(MenuStack::from_menudef_or_doom(defs.as_ref()));
                                    self.start_title_music();
                                }
                            }
                        }
                        Err(e) => {
                            self.messages.push(format!("next map {name}: {e}"), 70);
                            game.leave_intermission();
                            let defs = game.menudef().cloned();
                            self.menu = Some(MenuStack::from_menudef_or_doom(defs.as_ref()));
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
                let defs = game.menudef().cloned();
                self.reset_impact_fx();
                self.menu = Some(MenuStack::from_menudef_or_doom(defs.as_ref()));
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
            self.set_play_look(true);
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
                self.play_advance();
            }
            MenuCommand::Resume => self.toggle_pause(),
            MenuCommand::Quit => {
                // Confirm sheet pushed inside MenuStack::activate.
            }
            MenuCommand::QuitConfirmed => event_loop.exit(),
            MenuCommand::EndGame | MenuCommand::OpenMenu(_) | MenuCommand::SelectEpisode(_) => {
                // Confirm / submenus handled inside MenuStack::activate.
            }
            MenuCommand::EndGameConfirmed => {
                if let Some(game) = self.play.as_mut() {
                    game.return_to_title();
                }
                self.keys.attack.clear();
                self.reset_impact_fx();
                let defs = self.play.as_ref().and_then(|g| g.menudef().cloned());
                self.menu = Some(MenuStack::from_menudef_or_doom(defs.as_ref()));
                self.set_play_look(false);
                self.messages.push("Title", 35);
                self.start_title_music();
            }
            MenuCommand::LoadGame => {
                self.messages
                    .push("Load Game — not wired yet (use console `load`)", 70);
            }
            MenuCommand::SaveGame => {
                self.messages
                    .push("Save Game — not wired yet (use console `save`)", 70);
            }
            MenuCommand::ReadThis => {
                self.messages.push("Read This — help screen TBD", 70);
            }
            MenuCommand::Other(action) => {
                if let Some(cvar) = action.strip_prefix("option ") {
                    let name = cvar.trim();
                    if !name.is_empty() {
                        let _ = self.console.exec(&format!("toggle {name}"));
                        self.messages.push(format!("toggled {name}"), 35);
                    }
                } else if let Some(cmd) = action.strip_prefix("cmd ") {
                    let line = cmd.trim();
                    if !line.is_empty() {
                        self.apply_console(line);
                    }
                }
            }
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
            eprintln!("ferrite: GIF recording started");
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
                        eprintln!("ferrite: {msg}");
                    }
                    Err(e) => {
                        self.messages.push(format!("GIF write failed: {e}"), 70);
                        eprintln!("ferrite: GIF write failed: {e}");
                    }
                },
                Err(e) => {
                    self.messages.push(format!("GIF dir failed: {e}"), 70);
                    eprintln!("ferrite: GIF dir failed: {e}");
                }
            },
            None => {
                self.messages.push("GIF: no frames captured", 70);
                eprintln!("ferrite: GIF: no frames captured");
            }
        }
    }

    fn save_screenshot_rgba(&mut self, rgba: &[u8], width: u32, height: u32) {
        match screen_capture::ensure_screenshots_dir() {
            Ok(dir) => match screen_capture::write_png(&dir, rgba, width, height) {
                Ok(path) => {
                    let msg = format!("Screenshot: {}", path.display());
                    self.messages.push(msg.clone(), 105);
                    eprintln!("ferrite: {msg}");
                }
                Err(e) => {
                    self.messages.push(format!("Screenshot failed: {e}"), 70);
                    eprintln!("ferrite: screenshot failed: {e}");
                }
            },
            Err(e) => {
                self.messages.push(format!("Screenshot dir failed: {e}"), 70);
                eprintln!("ferrite: screenshot dir failed: {e}");
            }
        }
    }

    fn apply_console(&mut self, line: &str) {
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
            }
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
        match play_map_music(&mut shared.music, vfs, &name, None) {
            Ok(lump) => self.messages.push(format!("Music {lump}"), 35),
            Err(e) => eprintln!("ferrite: music {name}: {e}"),
        }
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
            Err(e) => eprintln!("ferrite: {label} music: {e}"),
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
            Some(ferrite_console::CvarValue::Bool(true))
        );
        if self.keys.shift || always_run {
            keys.push(Key::Speed);
        }
        if self.keys.attack.active() {
            keys.push(Key::Attack);
        }
        self.keys.attack.tick();
        if self.keys.use_btn {
            keys.push(Key::Use);
        }
        if self.keys.space {
            keys.push(Key::Jump);
        }
        match self.keys.weapon {
            1 => keys.push(Key::Weapon1),
            2 => keys.push(Key::Weapon2),
            3 => keys.push(Key::Weapon3),
            4 => keys.push(Key::Weapon4),
            5 => keys.push(Key::Weapon5),
            6 => keys.push(Key::Weapon6),
            7 => keys.push(Key::Weapon7),
            _ => {}
        }
        self.keys.weapon = 0;
        if self.keys.next_weapon {
            keys.push(Key::NextWeapon);
            self.keys.next_weapon = false;
        }
        if self.keys.prev_weapon {
            keys.push(Key::PrevWeapon);
            self.keys.prev_weapon = false;
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
        format: ferrite_render::DEPTH_STENCIL_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let view = tex.create_view(&wgpu::TextureViewDescriptor::default());
    (tex, view)
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
        format: wgpu::TextureFormat::R8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let view = tex.create_view(&wgpu::TextureViewDescriptor::default());
    (tex, view)
}
