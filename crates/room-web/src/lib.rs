//! Browser shell (wasm-bindgen + web-sys).
//!
//! Enable the `browser` feature for wasm32 builds.

#![cfg_attr(not(test), deny(clippy::unwrap_used))]

/// Library version string (always available; used by native CI too).
#[must_use]
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[cfg(feature = "browser")]
mod demo_map;

#[cfg(feature = "browser")]
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
mod editor_pick;

#[cfg(feature = "browser")]
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
mod feed;

#[cfg(all(feature = "browser", target_arch = "wasm32"))]
mod netplay;

#[cfg(all(feature = "browser", target_arch = "wasm32"))]
mod play;

#[cfg(all(feature = "browser", target_arch = "wasm32"))]
mod web_audio;

#[cfg(all(feature = "browser", target_arch = "wasm32"))]
mod input_hub;

#[cfg(all(feature = "browser", target_arch = "wasm32"))]
mod rtc_bridge;
#[cfg(all(feature = "browser", target_arch = "wasm32"))]
mod voice_hub;

#[cfg(all(feature = "browser", target_arch = "wasm32"))]
mod ws_net;

#[cfg(all(feature = "browser", target_arch = "wasm32"))]
mod ws_v2;

#[cfg(feature = "browser")]
pub use demo_map::load_embedded_demo_map;

#[cfg(all(feature = "browser", target_arch = "wasm32"))]
mod browser {
    mod editor;

    use crate::input_hub;
    use crate::load_embedded_demo_map;
    use crate::netplay::{NetJoinConfig, NetLobby, NetPlay, iwad_fingerprint};
    use crate::play::{PlaySession, PlayStartOpts};
    use glam::Vec3;
    use room_nodebuild::apply_nodes;
    use room_render::{
        Camera, CanvasGpu, GpuContext, LightMode, SceneLists, UnlitPass, build_scene_with_sky,
        build_scene_with_sky_ex_sized,
    };
    use std::cell::RefCell;
    use std::rc::Rc;
    use wasm_bindgen::JsCast;
    use wasm_bindgen::prelude::*;
    use web_sys::{HtmlCanvasElement, window};

    enum Mode {
        Demo {
            map: Box<room_map::MapGeometry>,
            start_ms: f64,
        },
        /// Waiting for WebSocket lobby peers (`?ws=`).
        Lobby {
            lobby: Box<NetLobby>,
            iwad: Vec<u8>,
            /// Map prepared after v2 welcome (before Ready / Start).
            prepared: Option<Box<PlaySession>>,
            demo: Box<room_map::MapGeometry>,
            start_ms: f64,
        },
        Play(Box<PlaySession>),
    }

    struct FrameState {
        surface: wgpu::Surface<'static>,
        gpu: GpuContext,
        config: wgpu::SurfaceConfiguration,
        pass: UnlitPass,
        depth: (wgpu::Texture, wgpu::TextureView),
        /// Dummy R8 MRT slot — `UnlitPass` pipelines expect two colour targets.
        stencil_aux: (wgpu::Texture, wgpu::TextureView),
        camera: Camera,
        mode: Mode,
        input_bound: bool,
        /// Weapon / HUD overlay (created when play starts).
        canvas: Option<CanvasGpu>,
    }

    /// WebGPU canvas preferred format is non-sRGB (`bgra8unorm`). Pipelines
    /// render to that; the shader applies linear→sRGB (`encode_srgb`).
    fn pick_web_surface_format(
        caps: &wgpu::SurfaceCapabilities,
    ) -> Result<wgpu::TextureFormat, String> {
        caps.formats
            .iter()
            .copied()
            .find(|f| {
                matches!(
                    f,
                    wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Rgba8Unorm
                )
            })
            .or_else(|| caps.formats.iter().copied().find(|f| !f.is_srgb()))
            .or_else(|| caps.formats.first().copied())
            .ok_or_else(|| "no surface formats".to_owned())
    }

    type Shared = Rc<RefCell<FrameState>>;
    type RafCallback = Rc<RefCell<Option<Closure<dyn FnMut(f64)>>>>;

    thread_local! {
        static APP: RefCell<Option<Shared>> = const { RefCell::new(None) };
        /// IWAD bytes received before GPU init finished.
        static PENDING_IWAD: RefCell<Option<(Vec<u8>, Option<String>)>> =
            const { RefCell::new(None) };
        /// PWADs (`roomAddPwad`) mounted by the next single-player start.
        static PWADS: RefCell<Vec<(String, Vec<u8>)>> = const { RefCell::new(Vec::new()) };
        /// Lobby pump timer id (`setInterval`) — rAF is throttled in background tabs.
        static LOBBY_INTERVAL: RefCell<Option<i32>> = const { RefCell::new(None) };
        /// Play lockstep pump — keeps TIC_CMD flowing when rAF is throttled.
        static PLAY_INTERVAL: RefCell<Option<i32>> = const { RefCell::new(None) };
        /// Download of the coop room's mod (`welcome.room.mod`).
        static NET_MOD: RefCell<NetModState> = const { RefCell::new(NetModState::Idle) };
    }

    enum NetModState {
        Idle,
        Fetching,
        /// Catalog name + verified bytes.
        Ready(String, Vec<u8>),
        Failed(String),
    }

    /// Start: locate `#room` and run a wgpu rAF loop (demo until IWAD loads).
    #[wasm_bindgen(start)]
    pub fn start() {
        console_error_panic_hook::set_once();
        wasm_bindgen_futures::spawn_local(async {
            match init_loop().await {
                Ok(()) => {}
                Err(e) => web_sys::console::error_1(&format!("room-web: {e}").into()),
            }
        });
    }

    async fn init_loop() -> Result<(), String> {
        let canvas = find_canvas()?;
        // Match backing-store size to CSS box so we never stretch 16:9 into a tall window.
        let (width, height) = fit_canvas_buffer(&canvas);
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::BROWSER_WEBGPU | wgpu::Backends::GL,
            ..Default::default()
        });
        let surface = instance
            .create_surface(wgpu::SurfaceTarget::Canvas(canvas.clone()))
            .map_err(|e| e.to_string())?;
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .map_err(|e| e.to_string())?;
        let gpu = GpuContext::from_adapter(instance, adapter)
            .await
            .map_err(|e| e.to_string())?;
        let caps = surface.get_capabilities(&gpu.adapter);
        let format = pick_web_surface_format(&caps)?;
        let alpha = caps
            .alpha_modes
            .first()
            .copied()
            .unwrap_or(wgpu::CompositeAlphaMode::Auto);
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width,
            height,
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode: alpha,
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&gpu.device, &config);
        let mut pass = UnlitPass::new(&gpu, format);
        apply_web_lighting(&mut pass);
        let depth = create_depth(&gpu.device, width, height);
        let stencil_aux = create_stencil_aux(&gpu.device, width, height);
        let mut map = load_embedded_demo_map()?;
        apply_nodes(&mut map);
        let mut camera = Camera {
            eye: Vec3::new(128.0, 64.0, 41.0),
            forward: Vec3::new(0.0, 1.0, 0.0),
            ..Camera::default()
        };
        camera.aspect = f64::from(width) / f64::from(height.max(1));
        camera.fov_y_deg = 58.715;
        let start_ms = now_ms();
        let state = Rc::new(RefCell::new(FrameState {
            surface,
            gpu,
            config,
            pass,
            depth,
            stencil_aux,
            camera,
            mode: Mode::Demo {
                map: Box::new(map),
                start_ms,
            },
            input_bound: false,
            canvas: None,
        }));
        web_sys::console::log_1(
            &format!(
                "room-web: format={format:?} encode_srgb={} {}×{}",
                state.borrow().pass.encode_srgb,
                width,
                height
            )
            .into(),
        );
        APP.with(|a| {
            *a.borrow_mut() = Some(state.clone());
        });
        // Bind keys immediately (not only when play starts) so the hub always
        // receives WASD; play mode drains the hub each frame.
        {
            let mut s = state.borrow_mut();
            if !s.input_bound {
                bind_input(state.clone());
                s.input_bound = true;
            }
        }
        set_status("Room web — demo map (load an IWAD to play Doom)");
        web_sys::console::log_1(
            &format!(
                "room-web {}: demo viewer; call roomStartIwad(bytes) to play",
                crate::version()
            )
            .into(),
        );
        // Apply IWAD that arrived before GPU init completed.
        let pending = PENDING_IWAD.with(|p| p.borrow_mut().take());
        if let Some((bytes, map)) = pending {
            if let Err(e) = begin_play(&state, &bytes, map.as_deref()) {
                web_sys::console::error_1(&e);
            }
        }
        if let Some(win) = window() {
            if let Ok(ev) = web_sys::Event::new("room-ready") {
                let _ = win.dispatch_event(&ev);
            }
        }
        request_frame(state);
        Ok(())
    }

    /// Mount a PWAD (e.g. a WASM mod) after the IWAD on single-player starts.
    #[wasm_bindgen(js_name = roomAddPwad)]
    pub fn room_add_pwad(name: &str, data: &[u8]) {
        PWADS.with(|p| p.borrow_mut().push((name.to_owned(), data.to_vec())));
    }

    /// Start (or restart) a Doom/Freedoom play session from IWAD bytes.
    #[wasm_bindgen(js_name = roomStartIwad)]
    pub fn room_start_iwad(data: &[u8]) -> Result<(), JsValue> {
        let map = room_url_param("map");
        let ready = APP.with(|app| app.borrow().clone());
        let Some(state) = ready else {
            PENDING_IWAD.with(|p| {
                *p.borrow_mut() = Some((data.to_vec(), map));
            });
            set_status("IWAD queued — waiting for GPU…");
            return Ok(());
        };
        begin_play(&state, data, map.as_deref())
    }

    fn begin_play(state: &Shared, data: &[u8], map: Option<&str>) -> Result<(), JsValue> {
        if let Some(ws) = room_url_param("ws") {
            return begin_net_lobby(state, data, map, &ws);
        }
        set_status("Loading IWAD…");
        let skill = room_url_param("skill")
            .and_then(|s| s.parse::<u8>().ok())
            .unwrap_or(3)
            .clamp(1, 5);
        let play = PlaySession::start_with(
            data,
            PlayStartOpts {
                map_name: map.map(str::to_owned),
                skill,
                pwads: PWADS.with(|p| p.borrow().clone()),
                ..PlayStartOpts::default()
            },
        )
        .map_err(|e| JsValue::from_str(&e))?;
        enter_play_mode(state, play)
    }

    fn begin_net_lobby(
        state: &Shared,
        data: &[u8],
        map: Option<&str>,
        ws_url: &str,
    ) -> Result<(), JsValue> {
        let players = room_url_param("players")
            .and_then(|s| s.parse::<u8>().ok())
            .unwrap_or(2)
            .clamp(2, 8);
        // index.html refuses to load the IWAD for coop until a callsign is set.
        let Some(name) = room_url_param("name")
            .map(|s| s.trim().to_owned())
            .filter(|s| !s.is_empty())
        else {
            return Err(JsValue::from_str("callsign required (?name=…)"));
        };
        let skill = room_url_param("skill")
            .and_then(|s| s.parse::<u8>().ok())
            .unwrap_or(3)
            .clamp(1, 5);
        let rng_seed = room_url_param("seed")
            .and_then(|s| s.parse::<u32>().ok())
            .unwrap_or(1);
        let map_name = map
            .map(str::to_owned)
            .or_else(|| room_url_param("map"))
            .unwrap_or_else(|| "E1M1".into());
        let ticket = room_url_param("ticket");
        let spectate = room_url_param("spectate").is_some_and(|v| v != "0");
        let cfg = NetJoinConfig {
            url: ws_url.to_owned(),
            players,
            name,
            fingerprint: iwad_fingerprint(data),
            map: map_name,
            skill,
            rng_seed,
            ticket,
            spectate,
            game_mod: None,
            time_limit: 0,
        };
        NET_MOD.with(|n| *n.borrow_mut() = NetModState::Idle);
        let lobby = NetLobby::begin(cfg).map_err(|e| JsValue::from_str(&e))?;
        set_status(&lobby.status());
        hide_boot_overlay();
        let mut s = state.borrow_mut();
        let (demo, start_ms) = match &s.mode {
            Mode::Demo { map, start_ms } => (map.clone(), *start_ms),
            Mode::Lobby { demo, start_ms, .. } => (demo.clone(), *start_ms),
            Mode::Play(_) => {
                let mut m = load_embedded_demo_map().map_err(|e| JsValue::from_str(&e))?;
                apply_nodes(&mut m);
                (Box::new(m), now_ms())
            }
        };
        s.mode = Mode::Lobby {
            lobby: Box::new(lobby),
            iwad: data.to_vec(),
            prepared: None,
            demo,
            start_ms,
        };
        drop(s);
        show_lobby_overlay();
        start_lobby_pump(state.clone());
        web_sys::console::log_1(
            &format!("room-web: coop lobby → {ws_url} ({players} players)").into(),
        );
        Ok(())
    }

    fn stop_lobby_pump() {
        LOBBY_INTERVAL.with(|slot| {
            if let Some(id) = slot.borrow_mut().take() {
                if let Some(win) = window() {
                    win.clear_interval_with_handle(id);
                }
            }
        });
    }

    fn stop_play_pump() {
        PLAY_INTERVAL.with(|slot| {
            if let Some(id) = slot.borrow_mut().take() {
                if let Some(win) = window() {
                    win.clear_interval_with_handle(id);
                }
            }
        });
    }

    /// Wall-clock lockstep pump when the tab is hidden (rAF is throttled to ~1 Hz).
    fn start_play_pump(state: Shared) {
        stop_play_pump();
        let Some(win) = window() else {
            return;
        };
        let state_tick = state;
        let cb = Closure::wrap(Box::new(move || {
            let hidden = window()
                .and_then(|w| w.document())
                .map(|d| d.hidden())
                .unwrap_or(false);
            if !hidden {
                return;
            }
            let Ok(mut s) = state_tick.try_borrow_mut() else {
                return;
            };
            let Mode::Play(play) = &mut s.mode else {
                stop_play_pump();
                return;
            };
            if !play.is_net() {
                return;
            }
            input_hub::drain_into(play);
            let _ = play.tick_play(1.0 / 35.0);
        }) as Box<dyn FnMut()>);
        match win
            .set_interval_with_callback_and_timeout_and_arguments_0(cb.as_ref().unchecked_ref(), 16)
        {
            Ok(id) => {
                PLAY_INTERVAL.with(|slot| *slot.borrow_mut() = Some(id));
                cb.forget();
            }
            Err(e) => web_sys::console::error_1(&e),
        }
    }

    /// Poll lobby on a wall-clock timer so background tabs still handshake.
    fn start_lobby_pump(state: Shared) {
        stop_lobby_pump();
        let Some(win) = window() else {
            return;
        };
        let state_tick = state;
        let cb = Closure::wrap(Box::new(move || {
            if !matches!(state_tick.borrow().mode, Mode::Lobby { .. }) {
                stop_lobby_pump();
                return;
            }
            if let Err(e) = promote_lobby_to_play(&state_tick) {
                web_sys::console::error_1(&e);
                stop_lobby_pump();
            } else if !matches!(state_tick.borrow().mode, Mode::Lobby { .. }) {
                stop_lobby_pump();
            }
        }) as Box<dyn FnMut()>);
        match win
            .set_interval_with_callback_and_timeout_and_arguments_0(cb.as_ref().unchecked_ref(), 50)
        {
            Ok(id) => {
                LOBBY_INTERVAL.with(|slot| *slot.borrow_mut() = Some(id));
                cb.forget();
            }
            Err(e) => {
                web_sys::console::error_1(&e);
            }
        }
    }

    fn enter_play_mode(state: &Shared, mut play: PlaySession) -> Result<(), JsValue> {
        {
            let mut s = state.borrow_mut();
            play.preload_materials(&s.gpu.device, &s.gpu.queue);
            play.last_ms = now_ms();
            play.apply_camera(&mut s.camera);
            apply_web_lighting(&mut s.pass);
            s.camera.fov_y_deg = 58.715;
            s.camera.aspect = f64::from(s.config.width) / f64::from(s.config.height.max(1));
            web_sys::console::log_1(
                &format!(
                    "room-web: {}×{} {:?} mode={:?} gain={} encode_srgb={} aspect={:.3} net={}",
                    s.config.width,
                    s.config.height,
                    s.config.format,
                    s.pass.light_mode,
                    s.pass.output_gain,
                    s.pass.encode_srgb,
                    s.camera.aspect,
                    play.is_net()
                )
                .into(),
            );
            let fmt = s.config.format;
            s.canvas = Some(CanvasGpu::new(
                &s.gpu,
                fmt,
                s.config.width.max(1),
                s.config.height.max(1),
            ));
            if !s.input_bound {
                bind_input(state.clone());
                s.input_bound = true;
            }
            web_sys::console::log_1(&play.status_line().into());
            s.mode = Mode::Play(Box::new(play));
            hide_boot_overlay();
            hide_status_overlay();
        }
        // Do not wipe held keys — WASD pressed during lobby must survive into play.
        focus_play_canvas();
        start_play_pump(state.clone());
        web_sys::console::log_1(&"room-web: play session started".into());
        Ok(())
    }

    fn promote_lobby_to_play(state: &Shared) -> Result<(), JsValue> {
        // Poll lobby / handshake first.
        let poll_result = {
            let mut s = state.borrow_mut();
            let Mode::Lobby { lobby, .. } = &mut s.mode else {
                return Ok(());
            };
            lobby.poll()
        };
        match poll_result {
            Ok(None) => {}
            Ok(Some(ready)) => {
                return finish_lobby_start(state, ready);
            }
            Err(e) => {
                set_status(&format!("Lobby error: {e}"));
                set_lobby_html(&format!(
                    "<h2>Lobby error</h2><p class=\"err\">{}</p>",
                    html_escape(&e)
                ));
                return Err(JsValue::from_str(&e));
            }
        }

        // After v2 welcome: prepare the map, then send Ready.
        prepare_lobby_map_if_needed(state)?;

        let panel = {
            let s = state.borrow();
            let Mode::Lobby { lobby, .. } = &s.mode else {
                return Ok(());
            };
            lobby.lobby_panel()
        };
        set_status(&{
            let s = state.borrow();
            match &s.mode {
                Mode::Lobby { lobby, .. } => lobby.status(),
                _ => String::new(),
            }
        });
        render_lobby_panel(&panel);
        Ok(())
    }

    fn prepare_lobby_map_if_needed(state: &Shared) -> Result<(), JsValue> {
        let needs = {
            let s = state.borrow();
            match &s.mode {
                Mode::Lobby {
                    lobby, prepared, ..
                } => lobby.needs_map_prepare() && prepared.is_none(),
                _ => false,
            }
        };
        if !needs {
            // If prepared but Ready not sent yet, send it now.
            let mut s = state.borrow_mut();
            let Mode::Lobby {
                lobby, prepared, ..
            } = &mut s.mode
            else {
                return Ok(());
            };
            if lobby.needs_map_prepare() {
                if let Some(play) = prepared.as_ref() {
                    let outline = play.map_outline_i16();
                    lobby.send_v2_ready_after_prepare(outline);
                }
            }
            return Ok(());
        }

        let Some(pwads) = coop_mod_pwads(state)? else {
            return Ok(());
        };
        set_status("Loading map…");
        set_lobby_html("<h2>Loading map…</h2><p class=\"muted\">Preparing level before Ready</p>");
        let (iwad, opts) = {
            let s = state.borrow();
            let Mode::Lobby { lobby, iwad, .. } = &s.mode else {
                return Ok(());
            };
            let Some((map, skill, rng_seed, players, seat)) = lobby.prepare_opts() else {
                return Ok(());
            };
            (
                iwad.clone(),
                PlayStartOpts {
                    map_name: Some(map),
                    skill,
                    rng_seed,
                    coop_players: players.min(4),
                    local_seat: seat,
                    pwads,
                    time_limit_secs: lobby.time_limit(),
                    ..PlayStartOpts::default()
                },
            )
        };
        let mut play = match PlaySession::start_with(&iwad, opts) {
            Ok(p) => p,
            Err(e) => {
                set_status(&format!("Failed to prepare map: {e}"));
                return Err(JsValue::from_str(&e));
            }
        };
        {
            let mut s = state.borrow_mut();
            play.preload_materials(&s.gpu.device, &s.gpu.queue);
            play.apply_camera(&mut s.camera);
            let Mode::Lobby {
                lobby, prepared, ..
            } = &mut s.mode
            else {
                return Ok(());
            };
            let outline = play.map_outline_i16();
            *prepared = Some(Box::new(play));
            lobby.send_v2_ready_after_prepare(outline);
        }
        web_sys::console::log_1(&"room-web: lobby map prepared".into());
        Ok(())
    }

    /// Named PWAD files (`name`, bytes).
    type NamedPwads = Vec<(String, Vec<u8>)>;

    /// PWADs for the room's mod: `Some` once ready (empty = vanilla room),
    /// `None` while the download runs.
    fn coop_mod_pwads(state: &Shared) -> Result<Option<NamedPwads>, JsValue> {
        let (game_mod, ws_url) = {
            let s = state.borrow();
            let Mode::Lobby { lobby, .. } = &s.mode else {
                return Ok(Some(Vec::new()));
            };
            (lobby.game_mod().cloned(), lobby.ws_url().to_owned())
        };
        let Some(m) = game_mod else {
            return Ok(Some(Vec::new()));
        };
        let start = NET_MOD.with(|n| {
            let mut n = n.borrow_mut();
            match &*n {
                NetModState::Ready(name, _) if *name == m.name => Ok(false),
                NetModState::Fetching => Ok(false),
                NetModState::Failed(e) => Err(e.clone()),
                _ => {
                    *n = NetModState::Fetching;
                    Ok(true)
                }
            }
        });
        let start = start.map_err(|e| {
            set_status(&format!("Mod download failed: {e}"));
            set_lobby_html(&format!(
                "<h2>Mod download failed</h2><p class=\"err\">{}</p>",
                html_escape(&e)
            ));
            JsValue::from_str(&e)
        })?;
        if start {
            set_status(&format!("Downloading mod {}…", m.title));
            set_lobby_html(&format!(
                "<h2>Downloading mod…</h2><p class=\"muted\">{} ({} KiB)</p>",
                html_escape(&m.title),
                m.size.div_ceil(1024)
            ));
            let url = mod_download_url(&ws_url, &m.name);
            let m = m.clone();
            wasm_bindgen_futures::spawn_local(async move {
                let result = fetch_bytes(&url).await.and_then(|bytes| {
                    let fp = format!("{:016x}", room_proto::content_fingerprint(&bytes));
                    if fp == m.fingerprint {
                        Ok(bytes)
                    } else {
                        Err(format!(
                            "{} has fingerprint {fp}, the room expects {}",
                            m.name, m.fingerprint
                        ))
                    }
                });
                NET_MOD.with(|n| {
                    *n.borrow_mut() = match result {
                        Ok(bytes) => NetModState::Ready(m.name.clone(), bytes),
                        Err(e) => NetModState::Failed(e),
                    };
                });
            });
        }
        Ok(NET_MOD.with(|n| match &*n.borrow() {
            NetModState::Ready(name, bytes) if *name == m.name => {
                Some(vec![(format!("{name}.wad"), bytes.clone())])
            }
            _ => None,
        }))
    }

    /// The downloaded coop mod, if any.
    fn ready_mod_pwads() -> Vec<(String, Vec<u8>)> {
        NET_MOD.with(|n| match &*n.borrow() {
            NetModState::Ready(name, bytes) => vec![(format!("{name}.wad"), bytes.clone())],
            _ => Vec::new(),
        })
    }

    /// `ws[s]://host/v2/…` → `http[s]://host/api/mods/{name}.wad` (the relay serves its mods).
    fn mod_download_url(ws_url: &str, name: &str) -> String {
        let (scheme, rest) = if let Some(r) = ws_url.strip_prefix("wss://") {
            ("https", r)
        } else if let Some(r) = ws_url.strip_prefix("ws://") {
            ("http", r)
        } else {
            return format!("/api/mods/{name}.wad");
        };
        let host = rest.split('/').next().unwrap_or(rest);
        format!("{scheme}://{host}/api/mods/{name}.wad")
    }

    async fn fetch_bytes(url: &str) -> Result<Vec<u8>, String> {
        let js_err = |e: JsValue| format!("{e:?}");
        let win = window().ok_or("no window")?;
        let resp: web_sys::Response = wasm_bindgen_futures::JsFuture::from(win.fetch_with_str(url))
            .await
            .map_err(js_err)?
            .dyn_into()
            .map_err(|_| "fetch did not return a Response".to_owned())?;
        if !resp.ok() {
            return Err(format!("HTTP {} for {url}", resp.status()));
        }
        let buf = wasm_bindgen_futures::JsFuture::from(resp.array_buffer().map_err(js_err)?)
            .await
            .map_err(js_err)?;
        Ok(js_sys::Uint8Array::new(&buf).to_vec())
    }

    fn finish_lobby_start(state: &Shared, ready: crate::netplay::NetReady) -> Result<(), JsValue> {
        set_status("Starting coop…");
        set_lobby_html("<h2>Starting…</h2><p class=\"muted\">All players ready</p>");
        let peer_count = ready.peer_count;
        let local = ready.local.0;
        let map = ready.map.clone();
        let skill = ready.skill;
        let rng_seed = ready.rng_seed;
        let time_limit_secs = ready.time_limit;
        let net = NetPlay::from_ready(ready);
        let (iwad, prepared) = {
            let mut s = state.borrow_mut();
            let Mode::Lobby { iwad, prepared, .. } = &mut s.mode else {
                return Err(JsValue::from_str("lobby ended unexpectedly"));
            };
            (iwad.clone(), prepared.take())
        };
        let mut play = if let Some(p) = prepared {
            *p
        } else {
            match PlaySession::start_with(
                &iwad,
                PlayStartOpts {
                    map_name: Some(map),
                    skill,
                    rng_seed,
                    coop_players: peer_count.min(4),
                    local_seat: local,
                    pwads: ready_mod_pwads(),
                    time_limit_secs,
                    ..PlayStartOpts::default()
                },
            ) {
                Ok(p) => p,
                Err(e) => {
                    set_status(&format!("Failed to start coop: {e}"));
                    return Err(JsValue::from_str(&e));
                }
            }
        };
        play.attach_net(net);
        stop_lobby_pump();
        hide_lobby_overlay();
        enter_play_mode(state, play)
    }

    fn render_lobby_panel(p: &crate::netplay::LobbyPanel) {
        let mut seats = String::new();
        for (seat, name, present) in &p.seats {
            let you = p.local_seat == Some(*seat);
            let cls = if *present { "seat on" } else { "seat" };
            let tag = if you { " (you)" } else { "" };
            let mark = if *present { "●" } else { "○" };
            seats.push_str(&format!(
                "<li class=\"{cls}\"><span class=\"dot\">{mark}</span> \
                 <span class=\"name\">{}{tag}</span> \
                 <span class=\"idx\">seat {seat}</span></li>",
                html_escape(name)
            ));
        }
        let phase = if p.connecting {
            "Connecting…"
        } else if p.started {
            "Starting…"
        } else if !p.ready_sent {
            "Loading map…"
        } else if p.spectator {
            "Spectating — waiting for the match"
        } else {
            "Waiting for players"
        };
        let role = if p.spectator {
            format!(
                "Watching as <strong>{}</strong>",
                html_escape(&p.local_name)
            )
        } else {
            format!("Playing as <strong>{}</strong>", html_escape(&p.local_name))
        };
        let html = format!(
            "<h2>{phase}</h2>\
             <p class=\"meta\"><span>{map}</span> · skill {skill} · {present}/{max} in lobby</p>\
             <p class=\"muted\">{role}</p>\
             <ul class=\"seats\">{seats}</ul>\
             <p class=\"hint\">Starts when the room is full and every seat has loaded the map.</p>",
            map = html_escape(&p.map),
            skill = p.skill,
            present = p.present,
            max = p.max_players,
        );
        set_lobby_html(&html);
        let show_invite = p.invite_url.is_some() && !p.connecting && !p.started && !p.spectator;
        update_lobby_invite(
            p.invite_url.as_deref().filter(|_| show_invite),
            p.room_id.as_deref(),
        );
    }

    /// Invite controls live outside `#lobby-body` so 50ms lobby polls do not destroy Copy.
    fn update_lobby_invite(url: Option<&str>, room_id: Option<&str>) {
        let Some(doc) = window().and_then(|w| w.document()) else {
            return;
        };
        let Some(wrap) = doc.get_element_by_id("lobby-invite") else {
            return;
        };
        let Ok(wrap) = wrap.dyn_into::<web_sys::HtmlElement>() else {
            return;
        };
        match url {
            Some(url) if !url.is_empty() => {
                wrap.set_hidden(false);
                if let Some(input) = doc.get_element_by_id("lobby-invite-url") {
                    if let Ok(input) = input.dyn_into::<web_sys::HtmlInputElement>() {
                        if input.value() != url {
                            input.set_value(url);
                        }
                        if let Some(room) = room_id {
                            let _ = input.set_attribute("title", &format!("Room {room}"));
                        }
                    }
                }
            }
            _ => {
                wrap.set_hidden(true);
            }
        }
    }

    fn html_escape(s: &str) -> String {
        s.chars()
            .map(|c| match c {
                '&' => "&amp;".into(),
                '<' => "&lt;".into(),
                '>' => "&gt;".into(),
                '"' => "&quot;".into(),
                _ => c.to_string(),
            })
            .collect()
    }

    fn set_lobby_html(html: &str) {
        let Some(win) = window() else {
            return;
        };
        let Some(doc) = win.document() else {
            return;
        };
        if let Some(el) = doc.get_element_by_id("lobby-body") {
            // Avoid clobbering the invite field selection every lobby poll.
            if el.inner_html() != html {
                el.set_inner_html(html);
            }
        }
        show_lobby_overlay();
    }

    fn show_lobby_overlay() {
        let Some(win) = window() else {
            return;
        };
        let Some(doc) = win.document() else {
            return;
        };
        // Free the cursor so invite copy / seat list stay usable.
        exit_pointer_lock();
        if let Some(el) = doc.get_element_by_id("lobby") {
            if let Ok(html) = el.dyn_into::<web_sys::HtmlElement>() {
                let _ = html.style().set_property("display", "flex");
            }
        }
    }

    fn exit_pointer_lock() {
        let Some(doc) = window().and_then(|w| w.document()) else {
            return;
        };
        if doc.pointer_lock_element().is_none() {
            return;
        }
        let Ok(func_val) = js_sys::Reflect::get(&doc, &JsValue::from_str("exitPointerLock")) else {
            return;
        };
        let Ok(func) = func_val.dyn_into::<js_sys::Function>() else {
            return;
        };
        let _ = func.apply(&doc, &js_sys::Array::new());
    }

    fn hide_lobby_overlay() {
        let Some(win) = window() else {
            return;
        };
        let Some(doc) = win.document() else {
            return;
        };
        if let Some(el) = doc.get_element_by_id("lobby") {
            if let Ok(html) = el.dyn_into::<web_sys::HtmlElement>() {
                let _ = html.style().set_property("display", "none");
            }
        }
        update_lobby_invite(None, None);
        // Invite <input> often keeps focus after the overlay is hidden; that
        // made every WASD keystroke look like "typing" and never reach the sim.
        focus_play_canvas();
    }

    /// True when keyboard events should go to a visible text field (not the game).
    fn is_visible_typing_target(el: &web_sys::Element) -> bool {
        let tag = el.tag_name();
        let editable = if tag == "INPUT" || tag == "TEXTAREA" {
            true
        } else {
            el.dyn_ref::<web_sys::HtmlElement>()
                .is_some_and(web_sys::HtmlElement::is_content_editable)
        };
        if !editable {
            return false;
        }
        let Some(html) = el.dyn_ref::<web_sys::HtmlElement>() else {
            return false;
        };
        // Hidden lobby invite / display:none nodes must not eat WASD.
        if html.hidden() {
            return false;
        }
        let style = win_computed_style(html);
        if style
            .as_ref()
            .is_some_and(|s| s.get_property_value("display").ok().as_deref() == Some("none"))
            || style.as_ref().is_some_and(|s| {
                s.get_property_value("visibility").ok().as_deref() == Some("hidden")
            })
        {
            return false;
        }
        // offsetParent is null for display:none (and the root <body> edge case).
        if html.offset_parent().is_none() && tag != "BODY" {
            // Also treat ancestors: lobby panel sets display:none on #lobby.
            if el.closest("#lobby").ok().flatten().is_some() {
                return false;
            }
        }
        if el.closest("#lobby").ok().flatten().is_some_and(|lobby| {
            lobby
                .dyn_ref::<web_sys::HtmlElement>()
                .is_some_and(|h| h.hidden() || win_display_none(h))
        }) {
            return false;
        }
        true
    }

    fn win_computed_style(el: &web_sys::HtmlElement) -> Option<web_sys::CssStyleDeclaration> {
        let win = window()?;
        win.get_computed_style(el).ok().flatten()
    }

    fn win_display_none(el: &web_sys::HtmlElement) -> bool {
        win_computed_style(el)
            .and_then(|s| s.get_property_value("display").ok())
            .as_deref()
            == Some("none")
    }

    fn focus_play_canvas() {
        let Some(doc) = window().and_then(|w| w.document()) else {
            return;
        };
        if let Some(active) = doc.active_element() {
            if let Some(html) = active.dyn_ref::<web_sys::HtmlElement>() {
                let _ = html.blur();
            }
        }
        if let Ok(canvas) = find_canvas() {
            let _ = canvas.focus();
        }
    }

    fn bind_input(state: Shared) {
        let Some(win) = window() else {
            return;
        };
        let Some(doc) = win.document() else {
            return;
        };
        let canvas = match find_canvas() {
            Ok(c) => c,
            Err(_) => return,
        };

        {
            // Input goes to `input_hub` — never borrow `FrameState` here (rAF holds it).
            let state_kd = state.clone();
            let kd = Closure::wrap(Box::new(move |ev: web_sys::KeyboardEvent| {
                let in_lobby = matches!(state_kd.borrow().mode, Mode::Lobby { .. });
                let in_play = matches!(state_kd.borrow().mode, Mode::Play(_));
                if let Some(doc) = window().and_then(|w| w.document()) {
                    if let Some(active) = doc.active_element() {
                        let tag = active.tag_name();
                        if in_lobby
                            && (tag == "BUTTON"
                                || tag == "A"
                                || tag == "INPUT"
                                || tag == "TEXTAREA")
                        {
                            // Lobby invite/copy needs focus; leave it alone.
                        } else if in_play
                            && !editor::active()
                            && active.id() != "room"
                            && !is_visible_typing_target(&active)
                        {
                            // Steal focus from voice chrome / leftover hidden lobby fields.
                            focus_play_canvas();
                        }
                    }
                }
                // Editor: the 3D view only takes keys while it has focus.
                let editor_elsewhere = editor::active()
                    && window()
                        .and_then(|w| w.document())
                        .and_then(|d| d.active_element())
                        .is_none_or(|el| el.id() != "room");
                if editor_elsewhere {
                    return;
                }
                let code = ev.code();
                let typing = window()
                    .and_then(|w| w.document())
                    .and_then(|d| d.active_element())
                    .is_some_and(|el| is_visible_typing_target(&el));
                if matches!(
                    code.as_str(),
                    "Space" | "Tab" | "ArrowUp" | "ArrowDown" | "ArrowLeft" | "ArrowRight"
                ) && !in_lobby
                    && !typing
                {
                    ev.prevent_default();
                }
                // Record WASD unless a *visible* text field has focus.
                if !typing {
                    input_hub::set_key(&code, true);
                }
            }) as Box<dyn FnMut(_)>);
            let _ = win.add_event_listener_with_callback("keydown", kd.as_ref().unchecked_ref());
            kd.forget();
        }
        {
            let ku = Closure::wrap(Box::new(move |ev: web_sys::KeyboardEvent| {
                input_hub::set_key(&ev.code(), false);
            }) as Box<dyn FnMut(_)>);
            let _ = win.add_event_listener_with_callback("keyup", ku.as_ref().unchecked_ref());
            ku.forget();
        }
        {
            let mm = Closure::wrap(Box::new(move |ev: web_sys::MouseEvent| {
                let locked = doc
                    .pointer_lock_element()
                    .is_some_and(|el| el.id() == "room");
                if !locked {
                    return;
                }
                input_hub::add_mouse_delta(ev.movement_x() as f64, ev.movement_y() as f64);
            }) as Box<dyn FnMut(_)>);
            let _ = win.add_event_listener_with_callback("mousemove", mm.as_ref().unchecked_ref());
            mm.forget();
        }
        {
            let wh = Closure::wrap(Box::new(move |ev: web_sys::WheelEvent| {
                let locked = window()
                    .and_then(|w| w.document())
                    .and_then(|d| d.pointer_lock_element())
                    .is_some_and(|el| el.id() == "room");
                if !locked {
                    return;
                }
                ev.prevent_default();
                // `delta_mode` 1 is lines; pixels need several counts per notch.
                let scale = if ev.delta_mode() == 1 {
                    1.0
                } else {
                    1.0 / 40.0
                };
                input_hub::add_weapon_wheel(ev.delta_y() * scale);
            }) as Box<dyn FnMut(_)>);
            let opts = web_sys::AddEventListenerOptions::new();
            opts.set_passive(false);
            let _ = win.add_event_listener_with_callback_and_add_event_listener_options(
                "wheel",
                wh.as_ref().unchecked_ref(),
                &opts,
            );
            wh.forget();
        }
        {
            let md = Closure::wrap(Box::new(move |ev: web_sys::MouseEvent| {
                input_hub::set_mouse_button(ev.button(), true);
            }) as Box<dyn FnMut(_)>);
            let _ =
                canvas.add_event_listener_with_callback("mousedown", md.as_ref().unchecked_ref());
            md.forget();
        }
        {
            let mu = Closure::wrap(Box::new(move |ev: web_sys::MouseEvent| {
                input_hub::set_mouse_button(ev.button(), false);
            }) as Box<dyn FnMut(_)>);
            let _ = win.add_event_listener_with_callback("mouseup", mu.as_ref().unchecked_ref());
            mu.forget();
        }
        {
            let state_click = state.clone();
            let click = Closure::wrap(Box::new(move |_ev: web_sys::MouseEvent| {
                // Pointer lock is play-only — lobby invite / waiting UI needs a free cursor.
                let in_play = matches!(state_click.borrow().mode, Mode::Play(_));
                if !in_play {
                    return;
                }
                if let Mode::Play(play) = &mut state_click.borrow_mut().mode {
                    play.ensure_audio();
                }
                // Editor fly camera: clicks pick, they don't grab the mouse.
                if editor::flying() {
                    return;
                }
                let Ok(canvas) = find_canvas() else {
                    return;
                };
                schedule_pointer_lock(&canvas);
            }) as Box<dyn FnMut(_)>);
            let _ =
                canvas.add_event_listener_with_callback("click", click.as_ref().unchecked_ref());
            click.forget();
        }
        // Autoplay policy: unlock on any first gesture, not only canvas click.
        bind_audio_unlock(state.clone());
        bind_audio_visibility(state);
    }

    fn bind_audio_unlock(state: Shared) {
        let Some(win) = window() else {
            return;
        };
        let Some(doc) = win.document() else {
            return;
        };
        let unlock = Closure::wrap(Box::new(move |_ev: web_sys::Event| {
            if let Ok(mut s) = state.try_borrow_mut() {
                if let Mode::Play(play) = &mut s.mode {
                    play.ensure_audio();
                }
            }
        }) as Box<dyn FnMut(_)>);
        let opts = web_sys::AddEventListenerOptions::new();
        opts.set_capture(true);
        // iOS Safari only resumes an AudioContext from `touchend` / `click`.
        for ev_name in ["pointerdown", "keydown", "touchstart", "touchend", "click"] {
            let _ = doc.add_event_listener_with_callback_and_add_event_listener_options(
                ev_name,
                unlock.as_ref().unchecked_ref(),
                &opts,
            );
        }
        unlock.forget();
    }

    fn bind_audio_visibility(state: Shared) {
        let Some(win) = window() else {
            return;
        };
        let Some(doc) = win.document() else {
            return;
        };
        let on_vis = Closure::wrap(Box::new(move |_ev: web_sys::Event| {
            let Some(win) = window() else {
                return;
            };
            let Some(doc) = win.document() else {
                return;
            };
            if doc.hidden() {
                return;
            }
            if let Ok(s) = state.try_borrow() {
                if let Mode::Play(play) = &s.mode {
                    if let Some(audio) = play.audio.as_ref() {
                        if audio.needs_resume() {
                            audio.kick();
                        } else {
                            audio.resume();
                        }
                    }
                }
            }
        }) as Box<dyn FnMut(_)>);
        let _ = doc
            .add_event_listener_with_callback("visibilitychange", on_vis.as_ref().unchecked_ref());
        on_vis.forget();
    }

    /// Request pointer lock after a short delay and swallow SecurityError
    /// (browsers reject lock immediately after Escape / getUserMedia unlock).
    fn schedule_pointer_lock(canvas: &HtmlCanvasElement) {
        let Some(win) = window() else {
            return;
        };
        let el: web_sys::HtmlElement = canvas.clone().unchecked_into();
        let cb = Closure::once(Box::new(move || {
            let Ok(func_val) = js_sys::Reflect::get(&el, &JsValue::from_str("requestPointerLock"))
            else {
                return;
            };
            let Ok(func) = func_val.dyn_into::<js_sys::Function>() else {
                return;
            };
            let Ok(ret) = func.apply(&el, &js_sys::Array::new()) else {
                return;
            };
            if let Ok(promise) = ret.dyn_into::<js_sys::Promise>() {
                let on_err = Closure::once(Box::new(move |_err: JsValue| {
                    // SecurityError is expected right after the user exits lock.
                }) as Box<dyn FnOnce(JsValue)>);
                let _ = promise.catch(&on_err);
                on_err.forget();
            }
        }) as Box<dyn FnOnce()>);
        // ~1 frame delay avoids "cannot be acquired immediately after exit".
        let _ = win
            .set_timeout_with_callback_and_timeout_and_arguments_0(cb.as_ref().unchecked_ref(), 50);
        cb.forget();
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

    fn request_frame(state: Shared) {
        let Some(win) = window() else {
            return;
        };
        let holder: RafCallback = Rc::new(RefCell::new(None));
        let holder2 = holder.clone();
        let state2 = state;
        let cb = Closure::new(move |_t: f64| {
            draw_frame(&state2);
            if let Some(w) = window() {
                if let Some(cb) = holder2.borrow().as_ref() {
                    let _ = w.request_animation_frame(cb.as_ref().unchecked_ref());
                }
            }
        });
        if win
            .request_animation_frame(cb.as_ref().unchecked_ref())
            .is_ok()
        {
            *holder.borrow_mut() = Some(cb);
        }
    }

    fn draw_frame(state: &Shared) {
        if matches!(state.borrow().mode, Mode::Lobby { .. }) {
            if let Err(e) = promote_lobby_to_play(state) {
                web_sys::console::error_1(&e);
            }
        }

        let mut s = state.borrow_mut();
        // Keep GPU buffer + CSS box + camera.aspect in lockstep (never mismatch).
        if let Ok(canvas_el) = find_canvas() {
            sync_surface_size(&canvas_el, &mut s);
        }
        // Re-assert web lighting each frame.
        apply_web_lighting(&mut s.pass);
        let now = now_ms();

        // Advance sim / orbit, update camera, then build scene.
        let use_materials = matches!(
            s.mode,
            Mode::Play(_)
                | Mode::Lobby {
                    prepared: Some(_),
                    ..
                }
        );
        let (eye, forward) = match &mut s.mode {
            Mode::Lobby {
                prepared: Some(play),
                ..
            } => play.view_pose().unwrap_or((s.camera.eye, s.camera.forward)),
            Mode::Demo { start_ms, .. } | Mode::Lobby { start_ms, .. } => {
                #[allow(clippy::cast_possible_truncation)]
                let t = (now - *start_ms) as f32 * 0.001;
                let cx = 128.0_f32;
                let cy = 128.0_f32;
                let radius = 140.0_f32;
                let yaw = t * 0.35;
                let eye_x = cx + libm::cosf(yaw) * radius;
                let eye_y = cy + libm::sinf(yaw) * radius;
                let fx = cx - eye_x;
                let fy = cy - eye_y;
                let flen = libm::sqrtf(fx * fx + fy * fy).max(1e-3);
                (
                    Vec3::new(eye_x, eye_y, 48.0),
                    Vec3::new(fx / flen, fy / flen, -0.15).normalize(),
                )
            }
            Mode::Play(play) => {
                let dt = if play.last_ms > 0.0 {
                    ((now - play.last_ms) as f32 * 0.001).clamp(0.0, 0.05)
                } else {
                    0.0
                };
                play.last_ms = now;
                input_hub::drain_into(play);
                play.tick_play(dt);
                // Keep the worklet ring topped up between 35 Hz tics.
                if let Some(audio) = play.audio.as_ref() {
                    if audio.needs_resume() {
                        audio.resume();
                    }
                    audio.feed_output();
                }
                play.view_pose().unwrap_or((s.camera.eye, s.camera.forward))
            }
        };
        s.camera.eye = eye;
        s.camera.forward = forward;

        // One full-frame view, or one viewport per player (spectator grid).
        let fb_w = s.config.width as f32;
        let fb_h = s.config.height as f32;
        let panes = match &s.mode {
            Mode::Play(play) => play.spectator_views(fb_w, fb_h),
            _ => None,
        };
        let views: Vec<(room_render::Camera, Option<u8>, Option<[f32; 4]>)> = match panes {
            Some(panes) => panes
                .into_iter()
                .map(|pane| {
                    let mut cam = s.camera;
                    cam.eye = pane.eye;
                    cam.forward = pane.forward;
                    cam.aspect = f64::from(pane.rect[2] / pane.rect[3].max(1.0));
                    (cam, Some(pane.seat), Some(pane.rect))
                })
                .collect(),
            None => vec![(s.camera, None, None)],
        };

        let Ok(frame) = s.surface.get_current_texture() else {
            return;
        };
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        for (i, (cam, seat, rect)) in views.iter().enumerate() {
            render_world_view(&mut s, &view, cam, *seat, *rect, i == 0, use_materials);
        }
        let mut encoder = s
            .gpu
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("hud") });
        // Weapon / HUD overlay (single colour target; Load the 3D result).
        if matches!(s.mode, Mode::Play(_)) {
            let hud_canvas = if let Mode::Play(play) = &mut s.mode {
                Some(play.build_hud_canvas(fb_w, fb_h))
            } else {
                None
            };
            if let Some(hud) = hud_canvas {
                if let Some(canvas_gpu) = s.canvas.as_mut() {
                    canvas_gpu.rasterize(&hud, None);
                }
                {
                    let FrameState { gpu, canvas, .. } = &mut *s;
                    if let Some(canvas_gpu) = canvas.as_mut() {
                        let mut rp = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                            label: Some("hud"),
                            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                                view: &view,
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
                }
            }
        }
        s.gpu.queue.submit(Some(encoder.finish()));
        frame.present();
    }

    /// Build, upload and draw the world from `cam` into `rect` (whole target if
    /// `None`). Each call submits its own encoder because `prepare` writes the
    /// shared view uniforms through the queue.
    #[allow(clippy::too_many_arguments)]
    fn render_world_view(
        s: &mut FrameState,
        view: &wgpu::TextureView,
        cam: &room_render::Camera,
        seat: Option<u8>,
        rect: Option<[f32; 4]>,
        clear: bool,
        use_materials: bool,
    ) {
        let sprites = match &mut s.mode {
            Mode::Play(play)
            | Mode::Lobby {
                prepared: Some(play),
                ..
            } => match seat {
                Some(seat) => play.collect_live_sprites_for(cam, seat),
                None => play.collect_live_sprites(cam),
            },
            Mode::Demo { .. } | Mode::Lobby { .. } => Vec::new(),
        };
        let inverse_colormap = match &s.mode {
            Mode::Play(play)
            | Mode::Lobby {
                prepared: Some(play),
                ..
            } => {
                play.fixed_colormap(seat.unwrap_or_else(|| play.local_seat_debug()))
                    == room_sim::INVERSE_COLORMAP
            }
            _ => false,
        };
        s.pass.inverse_colormap = inverse_colormap;
        let scene: SceneLists = match &s.mode {
            Mode::Demo { map, .. } => build_scene_with_sky(map, cam, None, None),
            Mode::Lobby {
                prepared: Some(play),
                ..
            }
            | Mode::Play(play) => {
                let sizes = play.materials.wall_texel_sizes();
                let mut lists = build_scene_with_sky_ex_sized(
                    &play.map,
                    cam,
                    Some(sprites.as_slice()),
                    Some("SKY1"),
                    None,
                    Some(&sizes),
                );
                lists.dyn_lights = play.collect_dyn_lights();
                room_render::apply_dyn_lights(&mut lists);
                lists
            }
            Mode::Lobby { demo, .. } => build_scene_with_sky(demo, cam, None, None),
        };

        {
            let FrameState { gpu, pass, .. } = &mut *s;
            pass.prepare(gpu, cam, &scene);
        }

        let mut encoder = s
            .gpu
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("frame"),
            });
        {
            let FrameState {
                gpu,
                pass,
                mode,
                depth,
                stencil_aux,
                ..
            } = &mut *s;
            let color_load = if clear {
                wgpu::LoadOp::Clear(wgpu::Color {
                    r: 0.02,
                    g: 0.02,
                    b: 0.03,
                    a: 1.0,
                })
            } else {
                wgpu::LoadOp::Load
            };
            let mut rp = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("map"),
                color_attachments: &[
                    Some(wgpu::RenderPassColorAttachment {
                        view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: color_load,
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
            if let Some([x, y, w, h]) = rect {
                rp.set_viewport(x, y, w, h, 0.0, 1.0);
                rp.set_scissor_rect(x as u32, y as u32, w as u32, h as u32);
            }
            if use_materials {
                if let Mode::Play(play) = mode {
                    pass.draw_materials(gpu, &mut rp, &scene, &mut play.materials, &play.vfs);
                }
            } else {
                pass.draw(&mut rp, &scene);
            }
        }
        s.gpu.queue.submit(Some(encoder.finish()));
    }

    fn find_canvas() -> Result<HtmlCanvasElement, String> {
        let win = window().ok_or_else(|| "no window".to_owned())?;
        let doc = win.document().ok_or_else(|| "no document".to_owned())?;
        let el = doc
            .get_element_by_id("room")
            .ok_or_else(|| "missing #room canvas".to_owned())?;
        el.dyn_into::<HtmlCanvasElement>()
            .map_err(|_| "#room is not a canvas".to_owned())
    }

    /// Web lighting: Bright + gain, and shader linear→sRGB encode for the
    /// non-sRGB WebGPU canvas (native windows use a hardware-sRGB swapchain).
    fn apply_web_lighting(pass: &mut UnlitPass) {
        pass.light_mode = LightMode::Bright;
        pass.glob_vis = 512.0;
        pass.output_gain = 1.85;
        pass.encode_srgb = true;
    }

    /// Largest 4:3…16:9 playfield in the window → CSS + GPU buffer.
    ///
    /// Caps ultrawide panels so FOV matches desktop (~90° H at 16:9), and
    /// letterboxes portrait phones at 4:3 so the view does not shrink to a
    /// slit. Buffer and CSS share the same aspect; `camera.aspect = w/h`.
    fn fit_canvas_buffer(canvas: &HtmlCanvasElement) -> (u32, u32) {
        const MAX_ASPECT: f64 = 16.0 / 9.0;
        const MIN_ASPECT: f64 = 4.0 / 3.0;
        let win = window();
        let dpr = win
            .as_ref()
            .map(|w| w.device_pixel_ratio())
            .unwrap_or(1.0)
            .clamp(1.0, 2.0);
        let (vw, vh) = match &win {
            Some(w) => {
                let iw = w
                    .inner_width()
                    .ok()
                    .and_then(|v| v.as_f64())
                    .unwrap_or(1280.0)
                    .max(1.0);
                let ih = w
                    .inner_height()
                    .ok()
                    .and_then(|v| v.as_f64())
                    .unwrap_or(720.0)
                    .max(1.0);
                (iw, ih)
            }
            None => (1280.0, 720.0),
        };
        // `data-fit="parent"` (editor panes): fill the parent box, any aspect.
        let parent_box = canvas
            .get_attribute("data-fit")
            .filter(|v| v == "parent")
            .and_then(|_| canvas.parent_element())
            .map(|p| {
                (
                    f64::from(p.client_width()).max(1.0),
                    f64::from(p.client_height()).max(1.0),
                )
            });
        let (css_w, css_h) = if let Some(size) = parent_box {
            size
        } else if vw / vh > MAX_ASPECT {
            (vh * MAX_ASPECT, vh)
        } else if vw / vh < MIN_ASPECT {
            (vw, vw / MIN_ASPECT)
        } else {
            (vw, vh)
        };
        let style = canvas.style();
        let _ = style.set_property("width", &format!("{css_w:.0}px"));
        let _ = style.set_property("height", &format!("{css_h:.0}px"));
        let w = (css_w * dpr).round().clamp(320.0, 3840.0) as u32;
        let h = (css_h * dpr).round().clamp(240.0, 2160.0) as u32;
        if canvas.width() != w {
            canvas.set_width(w);
        }
        if canvas.height() != h {
            canvas.set_height(h);
        }
        (w, h)
    }

    fn sync_surface_size(canvas: &HtmlCanvasElement, s: &mut FrameState) {
        let (w, h) = fit_canvas_buffer(canvas);
        s.camera.aspect = f64::from(w) / f64::from(h.max(1));
        // Match desktop: ~90° horizontal at 16:9.
        s.camera.fov_y_deg = 58.715;
        if w == s.config.width && h == s.config.height {
            return;
        }
        s.config.width = w;
        s.config.height = h;
        s.surface.configure(&s.gpu.device, &s.config);
        s.depth = create_depth(&s.gpu.device, w, h);
        s.stencil_aux = create_stencil_aux(&s.gpu.device, w, h);
        if s.canvas.is_some() {
            let fmt = s.config.format;
            s.canvas = Some(CanvasGpu::new(&s.gpu, fmt, w, h));
        }
    }

    fn now_ms() -> f64 {
        window()
            .and_then(|w| w.performance())
            .map(|p| p.now())
            .unwrap_or(0.0)
    }

    fn set_status(msg: &str) {
        let Some(win) = window() else {
            return;
        };
        let Some(doc) = win.document() else {
            return;
        };
        if let Some(el) = doc.get_element_by_id("status") {
            el.set_text_content(Some(msg));
            if let Ok(html) = el.dyn_into::<web_sys::HtmlElement>() {
                let _ = html.style().set_property("opacity", "1");
            }
        }
    }

    fn hide_boot_overlay() {
        let Some(win) = window() else {
            return;
        };
        let Some(doc) = win.document() else {
            return;
        };
        if let Some(el) = doc.get_element_by_id("boot") {
            if let Ok(html) = el.dyn_into::<web_sys::HtmlElement>() {
                let _ = html.style().set_property("display", "none");
            }
        }
    }

    fn hide_status_overlay() {
        let Some(win) = window() else {
            return;
        };
        let Some(doc) = win.document() else {
            return;
        };
        if let Some(el) = doc.get_element_by_id("status") {
            if let Ok(html) = el.dyn_into::<web_sys::HtmlElement>() {
                let _ = html.style().set_property("opacity", "0");
                let _ = html.style().set_property("pointer-events", "none");
            }
        }
    }

    /// Expose version to JS.
    #[wasm_bindgen(js_name = roomVersion)]
    #[must_use]
    pub fn room_version() -> String {
        crate::version().to_owned()
    }

    /// Install a GM SoundFont (SF2/SF3) for music — same synth as desktop
    /// `snd_mididevice auto`. Safe to call before or after audio starts.
    #[wasm_bindgen(js_name = roomSetSoundfont)]
    pub fn room_set_soundfont(data: &[u8]) -> Result<(), JsValue> {
        crate::web_audio::set_soundfont(data).map_err(|e| JsValue::from_str(&e))
    }

    /// Mute / unmute all game sound and music (play page and editor).
    #[wasm_bindgen(js_name = roomSetMuted)]
    pub fn room_set_muted(muted: bool) {
        crate::web_audio::set_muted(muted);
    }

    /// Current [`room_set_muted`] state.
    #[wasm_bindgen(js_name = roomMuted)]
    #[must_use]
    pub fn room_muted() -> bool {
        crate::web_audio::muted()
    }

    /// On-screen controls: press / release a game key by `KeyboardEvent.code`
    /// (`KeyW`, `ShiftLeft`, `ControlLeft` = fire, `KeyE` = use, `Tab`, …).
    #[wasm_bindgen(js_name = roomInputKey)]
    pub fn room_input_key(code: &str, down: bool) {
        input_hub::set_key(code, down);
    }

    /// On-screen controls: look by whole mouse counts (≈1.4° each; +x right, +y down).
    #[wasm_bindgen(js_name = roomInputLook)]
    pub fn room_input_look(dx: f64, dy: f64) {
        input_hub::add_mouse_delta(dx, dy);
    }

    /// On-screen controls: switch weapon (`1` = next, `-1` = previous).
    #[wasm_bindgen(js_name = roomInputWeaponCycle)]
    pub fn room_input_weapon_cycle(steps: i32) {
        input_hub::add_weapon_wheel(-f64::from(steps));
    }

    /// Read a URL query parameter (`?map=MAP01&iwad=…`).
    #[wasm_bindgen(js_name = roomUrlParam)]
    #[must_use]
    pub fn room_url_param(name: &str) -> Option<String> {
        let win = window()?;
        let search = win.location().search().ok()?;
        parse_query_param(&search, name)
    }

    /// Persist a small string for the embed host (`localStorage` key `room/<key>`).
    #[wasm_bindgen(js_name = roomOpfsSet)]
    pub fn room_opfs_set(key: &str, value: &str) -> Result<(), JsValue> {
        let win = window().ok_or_else(|| JsValue::from_str("no window"))?;
        let storage = win
            .local_storage()?
            .ok_or_else(|| JsValue::from_str("no localStorage"))?;
        storage.set_item(&format!("room/{key}"), value)?;
        Ok(())
    }

    /// Read a string previously stored with [`room_opfs_set`].
    #[wasm_bindgen(js_name = roomOpfsGet)]
    pub fn room_opfs_get(key: &str) -> Result<Option<String>, JsValue> {
        let win = window().ok_or_else(|| JsValue::from_str("no window"))?;
        let storage = win
            .local_storage()?
            .ok_or_else(|| JsValue::from_str("no localStorage"))?;
        storage.get_item(&format!("room/{key}"))
    }

    /// Queue a live-voice payload for the coop WebSocket (`MSG_VOICE`).
    #[wasm_bindgen(js_name = roomVoiceSend)]
    pub fn room_voice_send(data: &[u8]) {
        crate::voice_hub::enqueue_out(data);
    }

    /// Drain inbound voice frames. Each entry is `[fromSeat: number, bytes: Uint8Array]`.
    #[wasm_bindgen(js_name = roomVoicePoll)]
    pub fn room_voice_poll() -> js_sys::Array {
        let out = js_sys::Array::new();
        for (from, payload) in crate::voice_hub::take_inbox() {
            let row = js_sys::Array::new();
            row.push(&JsValue::from(from));
            let u8 = js_sys::Uint8Array::new_with_length(payload.len() as u32);
            u8.copy_from(&payload);
            row.push(&u8);
            out.push(&row);
        }
        out
    }

    /// `true` while a coop socket is pumping voice I/O.
    #[wasm_bindgen(js_name = roomVoiceActive)]
    #[must_use]
    pub fn room_voice_active() -> bool {
        crate::voice_hub::is_active()
    }

    /// Debug snapshot for input / lockstep (`held=w,a … net=tic/pending`).
    #[wasm_bindgen(js_name = roomDebugState)]
    #[must_use]
    pub fn room_debug_state() -> String {
        let held = crate::input_hub::debug_held();
        APP.with(|a| {
            let Some(app) = a.borrow().as_ref().cloned() else {
                return format!("no-app held={held}");
            };
            let Ok(s) = app.try_borrow() else {
                return format!("busy held={held}");
            };
            match &s.mode {
                Mode::Demo { .. } => format!("mode=demo held={held}"),
                Mode::Lobby { lobby, .. } => format!("mode=lobby {} held={held}", lobby.status()),
                Mode::Play(play) => {
                    let net = if let Some(n) = play.net_debug() {
                        format!(" net={}", n)
                    } else {
                        " net=solo".into()
                    };
                    format!(
                        "mode=play seat={} held={}{} pos={}",
                        play.local_seat_debug(),
                        held,
                        net,
                        play.pos_debug()
                    )
                }
            }
        })
    }

    /// Structured coop net stats for the on-screen graph (`null` when solo / idle).
    /// Audio latency / underrun stats (`null` before the first user gesture).
    #[wasm_bindgen(js_name = roomAudioStats)]
    #[must_use]
    pub fn room_audio_stats() -> JsValue {
        APP.with(|a| {
            let Some(app) = a.borrow().as_ref().cloned() else {
                return JsValue::NULL;
            };
            let Ok(s) = app.try_borrow() else {
                return JsValue::NULL;
            };
            let Mode::Play(play) = &s.mode else {
                return JsValue::NULL;
            };
            let Some(audio) = play.audio.as_ref() else {
                return JsValue::NULL;
            };
            let st = audio.stats();
            let obj = js_sys::Object::new();
            let set = |k: &str, v: JsValue| {
                let _ = js_sys::Reflect::set(&obj, &JsValue::from_str(k), &v);
            };
            set("backend", JsValue::from_str(st.backend));
            set("rate", JsValue::from(f64::from(audio.sample_rate())));
            set("targetMs", JsValue::from(st.target_ms));
            set("bufferedMs", JsValue::from(st.buffered_ms));
            set("underruns", JsValue::from(f64::from(st.underruns)));
            set("voices", JsValue::from(st.voices as f64));
            obj.into()
        })
    }

    /// Match countdown / final standings as JSON (`null` outside a level):
    /// `{time_left, over: {winner, winner_name, time_up}, closed, rows: [...],
    /// local_seat, spectator, solo}`.
    #[wasm_bindgen(js_name = roomMatchState)]
    #[must_use]
    pub fn room_match_state() -> String {
        APP.with(|a| {
            let Some(app) = a.borrow().as_ref().cloned() else {
                return "null".into();
            };
            let Ok(s) = app.try_borrow() else {
                return "null".into();
            };
            match &s.mode {
                Mode::Play(play) => play.match_state_json(),
                _ => "null".into(),
            }
        })
    }

    #[wasm_bindgen(js_name = roomNetStats)]
    #[must_use]
    pub fn room_net_stats() -> JsValue {
        APP.with(|a| {
            let Some(app) = a.borrow().as_ref().cloned() else {
                return JsValue::NULL;
            };
            let Ok(s) = app.try_borrow() else {
                return JsValue::NULL;
            };
            match &s.mode {
                Mode::Play(play) => play
                    .net_stats_js()
                    .map(JsValue::from)
                    .unwrap_or(JsValue::NULL),
                Mode::Lobby { .. } => {
                    let obj = js_sys::Object::new();
                    let _ = js_sys::Reflect::set(
                        &obj,
                        &JsValue::from_str("mode"),
                        &JsValue::from_str("lobby"),
                    );
                    let _ = js_sys::Reflect::set(
                        &obj,
                        &JsValue::from_str("held"),
                        &JsValue::from_str(&crate::input_hub::debug_held()),
                    );
                    obj.into()
                }
                Mode::Demo { .. } => JsValue::NULL,
            }
        })
    }

    fn parse_query_param(search: &str, name: &str) -> Option<String> {
        let q = search.trim_start_matches('?');
        for pair in q.split('&') {
            let mut it = pair.splitn(2, '=');
            let k = it.next()?;
            if k == name {
                let v = it.next().unwrap_or("");
                return Some(urlencoding_decode(v));
            }
        }
        None
    }

    fn urlencoding_decode(s: &str) -> String {
        let mut out = String::with_capacity(s.len());
        let bytes = s.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            match bytes[i] {
                b'+' => {
                    out.push(' ');
                    i += 1;
                }
                b'%' if i + 2 < bytes.len() => {
                    let h = |c: u8| -> Option<u8> {
                        match c {
                            b'0'..=b'9' => Some(c - b'0'),
                            b'a'..=b'f' => Some(c - b'a' + 10),
                            b'A'..=b'F' => Some(c - b'A' + 10),
                            _ => None,
                        }
                    };
                    if let (Some(a), Some(b)) = (h(bytes[i + 1]), h(bytes[i + 2])) {
                        out.push(char::from(a << 4 | b));
                        i += 3;
                    } else {
                        out.push('%');
                        i += 1;
                    }
                }
                c => {
                    out.push(char::from(c));
                    i += 1;
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_nonempty() {
        assert!(!version().is_empty());
    }

    #[cfg(feature = "browser")]
    #[test]
    fn embedded_demo_map_loads() {
        let map = load_embedded_demo_map().expect("demo map");
        assert_eq!(map.sectors.len(), 1);
        assert_eq!(map.vertices.len(), 4);
        assert!(map.things.iter().any(|t| t.type_id == 1));
    }
}
