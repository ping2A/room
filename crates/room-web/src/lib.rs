//! Browser shell (wasm-bindgen + web-sys).
//!
//! Enable the `browser` feature for wasm32 builds.

#![cfg_attr(not(test), deny(clippy::unwrap_used))]

/// Library version string (always available; used by native CI too).
#[must_use]
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// Netplay clock (input horizon + playout buffer); pure, tested natively.
pub mod net_clock;

#[cfg(feature = "browser")]
mod demo_map;

#[cfg(feature = "browser")]
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
mod zvm_web;

#[cfg(feature = "browser")]
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
mod editor_pick;

#[cfg(feature = "browser")]
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
mod feed;

#[cfg(feature = "browser")]
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
mod pk3_edit;

#[cfg(all(feature = "browser", target_arch = "wasm32"))]
mod netplay;

#[cfg(all(feature = "browser", target_arch = "wasm32"))]
mod play;

#[cfg(all(feature = "browser", target_arch = "wasm32"))]
mod web_audio;

#[cfg(all(feature = "browser", target_arch = "wasm32"))]
mod web_post;

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
    mod faces;
    mod mapgen_play;

    use crate::input_hub;
    use crate::load_embedded_demo_map;
    use crate::netplay::{NetJoinConfig, NetLobby, NetPlay, iwad_fingerprint};
    use crate::play::{PlaySession, PlayStartOpts};
    use glam::Vec3;
    use room_game::GameRuntime;
    use room_nodebuild::apply_nodes;
    use room_render::{
        Camera, Canvas, CanvasGpu, GpuContext, LightMode, ModZonePass, PostChain, PostChainInputs,
        RenderSettings, SceneLists, UnlitPass, ZoneDraw, build_scene_with_sky,
        build_scene_with_sky_ex_shared, build_zone_meshes,
    };
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;
    use wasm_bindgen::JsCast;
    use wasm_bindgen::prelude::*;
    use web_sys::{HtmlCanvasElement, window};

    const DUKE_RTS_SOUND_BASE: u32 = 0x1_0000;

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
            /// Native GRP session prepared before the Duke Ready barrier.
            prepared_duke: Option<Box<room_duke::DukeSession>>,
            prepared_duke_renderer: Option<Box<room_duke::DukeSoftRenderer>>,
            demo: Box<room_map::MapGeometry>,
            start_ms: f64,
        },
        Play(Box<PlaySession>),
        NativeDuke {
            session: Box<room_duke::DukeSession>,
            renderer: Box<room_duke::DukeSoftRenderer>,
            grp: Vec<Vec<u8>>,
            audio: Option<crate::web_audio::WebAudio>,
            backdrop: Box<room_map::MapGeometry>,
            last_ms: f64,
            tic_accum: f32,
            network: Option<crate::netplay::DukeNetPlay>,
            network_tic: u32,
            campaign_map: String,
            duke_music_track: Option<String>,
        },
    }

    struct FrameState {
        surface: wgpu::Surface<'static>,
        gpu: GpuContext,
        config: wgpu::SurfaceConfiguration,
        pass: UnlitPass,
        zone_pass: ModZonePass,
        depth: (wgpu::Texture, wgpu::TextureView),
        /// Dummy R8 MRT slot — `UnlitPass` pipelines expect two colour targets.
        stencil_aux: (wgpu::Texture, wgpu::TextureView),
        camera: Camera,
        mode: Mode,
        input_bound: bool,
        /// Weapon / HUD overlay (created when play starts).
        canvas: Option<CanvasGpu>,
        /// Shader / post toggles (`docs/29`); set via [`room_set_shader_cvar`].
        render_settings: RenderSettings,
        /// Scene colour (post source) + ping + LDR when post is enabled.
        post_chain: Option<PostChain>,
        scene_color: Option<(wgpu::Texture, wgpu::TextureView)>,
        post_ping: Option<(wgpu::Texture, wgpu::TextureView)>,
        post_ldr: Option<(wgpu::Texture, wgpu::TextureView)>,
        /// Size of the world targets (`depth`, `stencil_aux`, `scene_color`,
        /// `post_ping`): the canvas size times the 3D scale.
        world_size: (u32, u32),
        /// `vid_rendermode` (4 hardware, 0 / 1 software; `docs/40` SR3).
        rendermode: i32,
        /// `vid_softscale`: software view is 320×200 times this.
        softscale: u32,
        /// `fov` / `r_visibility` for the software view (SR5).
        soft_fov: f64,
        soft_visibility: f64,
        /// Software renderer for the current map, kept between frames.
        soft: Option<((usize, u32), room_render_soft::sim::SoftWorldView)>,
        /// The mods' GLDEFS post shaders (Color World, Red Noir, …).
        web_post: Option<crate::web_post::WebPost>,
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

    #[derive(Clone, Copy)]
    struct DukeAudioOptions {
        sound: bool,
        music: bool,
        talk: bool,
        lockout: bool,
        ambience: bool,
        reverse_stereo: bool,
        sfx_volume: u8,
        music_volume: u8,
    }

    impl Default for DukeAudioOptions {
        fn default() -> Self {
            Self {
                sound: true,
                music: true,
                talk: true,
                lockout: false,
                ambience: true,
                reverse_stereo: false,
                sfx_volume: u8::MAX,
                music_volume: u8::MAX,
            }
        }
    }

    struct BrowserDukeIncludes(Vec<(String, Vec<u8>)>);

    impl room_duke::con::IncludeResolver for BrowserDukeIncludes {
        fn read_include(&mut self, name: &str) -> Option<Vec<u8>> {
            let requested = name.replace('\\', "/");
            self.0
                .iter()
                .rev()
                .find(|(candidate, _)| {
                    candidate
                        .replace('\\', "/")
                        .eq_ignore_ascii_case(&requested)
                })
                .map(|(_, bytes)| bytes.clone())
        }
    }

    fn load_duke_grp_map(
        grp_layers: &[Vec<u8>],
        map: &str,
        skill: u8,
    ) -> Result<room_duke::DukeSession, String> {
        let layer_refs: Vec<&[u8]> = grp_layers.iter().map(Vec::as_slice).collect();
        let override_con = DUKE_CON.with(|script| script.borrow().clone());
        let mut includes = DUKE_CON_INCLUDES.with(|files| files.borrow().clone());
        DUKE_MOD_FILES.with(|files| {
            includes.extend(
                files
                    .borrow()
                    .iter()
                    .filter(|(name, _)| name.to_ascii_lowercase().ends_with(".con"))
                    .cloned(),
            );
        });
        let mut resolver = BrowserDukeIncludes(includes);
        let dialect = if DUKE_MOD_FILES.with(|files| !files.borrow().is_empty()) {
            room_duke::ConDialect::EDuke32
        } else {
            room_duke::ConDialect::Atomic
        };
        room_duke::session_from_grp_layers_with_con_and_includes_dialect(
            &layer_refs,
            map,
            skill,
            override_con.as_deref(),
            Some(&mut resolver),
            dialect,
        )
    }

    /// Seat whose player this client sees and hears in native Duke (its own;
    /// seat 0 when solo or watching).
    fn duke_view_seat(network: Option<&crate::netplay::DukeNetPlay>) -> usize {
        network.map_or(0, crate::netplay::DukeNetPlay::view_seat)
    }

    fn duke_grp_layers(base: &[u8]) -> Vec<Vec<u8>> {
        let mut layers = vec![base.to_vec()];
        DUKE_ADDON_GRPS.with(|addons| layers.extend(addons.borrow().iter().cloned()));
        DUKE_EDITOR_GRP.with(|grp| {
            if let Some(grp) = grp.borrow().as_ref() {
                layers.push(grp.clone());
            }
        });
        layers
    }

    fn duke_renderer(grp_layers: &[Vec<u8>]) -> Result<room_duke::DukeSoftRenderer, String> {
        let layer_refs: Vec<&[u8]> = grp_layers.iter().map(Vec::as_slice).collect();
        DUKE_MOD_FILES.with(|files| {
            let files = files.borrow();
            let def_files: Vec<_> = files
                .iter()
                .filter(|(name, _)| name.to_ascii_lowercase().ends_with(".def"))
                .map(|(name, bytes)| (name.as_str(), bytes.as_slice()))
                .collect();
            let mhk_files: Vec<_> = files
                .iter()
                .filter(|(name, _)| name.to_ascii_lowercase().ends_with(".mhk"))
                .map(|(name, bytes)| (name.as_str(), bytes.as_slice()))
                .collect();
            let tile_assets: Vec<_> = files
                .iter()
                .map(|(name, bytes)| (name.as_str(), bytes.as_slice()))
                .collect();
            room_duke::DukeSoftRenderer::from_grp_layers_with_mod_assets_and_mhks(
                &layer_refs,
                None,
                &def_files,
                &tile_assets,
                &mhk_files,
            )
        })
    }

    thread_local! {
        static APP: RefCell<Option<Shared>> = const { RefCell::new(None) };
        /// IWAD bytes received before GPU init finished.
        static PENDING_IWAD: RefCell<Option<(Vec<u8>, Option<String>)>> =
            const { RefCell::new(None) };
        /// Native Duke standalone map received before GPU init finished.
        static PENDING_DUKE_MAP: RefCell<Option<(Vec<u8>, Vec<u8>, u8)>> =
            const { RefCell::new(None) };
        /// PWADs (`roomAddPwad`) mounted by the next single-player start.
        static PWADS: RefCell<Vec<(String, Vec<u8>)>> = const { RefCell::new(Vec::new()) };
        /// Native Duke root CON override and external includes for GRP sessions.
        static DUKE_CON: RefCell<Option<Vec<u8>>> = const { RefCell::new(None) };
        static DUKE_CON_INCLUDES: RefCell<Vec<(String, Vec<u8>)>> = const { RefCell::new(Vec::new()) };
        /// Native Duke add-on GRPs, in increasing override priority.
        static DUKE_ADDON_GRPS: RefCell<Vec<Vec<u8>>> = const { RefCell::new(Vec::new()) };
        /// Extracted files from streamed Duke mod directories / ZIPs.
        static DUKE_MOD_FILES: RefCell<Vec<(String, Vec<u8>)>> = const { RefCell::new(Vec::new()) };
        /// Current native editor overlay (replaced between preview runs).
        static DUKE_EDITOR_GRP: RefCell<Option<Vec<u8>>> = const { RefCell::new(None) };
        /// Browser host settings corresponding to Duke's native F4 audio controls.
        static DUKE_AUDIO_OPTIONS: Cell<DukeAudioOptions> = const {
            Cell::new(DukeAudioOptions {
                sound: true,
                music: true,
                talk: true,
                lockout: false,
                ambience: true,
                reverse_stereo: false,
                sfx_volume: u8::MAX,
                music_volume: u8::MAX,
            })
        };
        /// Lobby pump timer id (`setInterval`) — rAF is throttled in background tabs.
        static LOBBY_INTERVAL: RefCell<Option<i32>> = const { RefCell::new(None) };
        /// Play lockstep pump — keeps TIC_CMD flowing when rAF is throttled.
        static PLAY_INTERVAL: RefCell<Option<i32>> = const { RefCell::new(None) };
        /// Download of the coop room's mod (`welcome.room.mod`).
        static NET_MOD: RefCell<NetModState> = const { RefCell::new(NetModState::Idle) };
        /// Content packs (`welcome.room.packs`), keyed by joined names.
        static NET_PACKS: RefCell<NetPacksState> = const { RefCell::new(NetPacksState::Idle) };
        static NET_CONTENT: RefCell<NetContentState> = const { RefCell::new(NetContentState::Idle) };
        /// Soft cap on the longest canvas edge (Settings → Resolution). 0 = device max.
        static RENDER_CAP: Cell<u32> = const { Cell::new(0) };
        /// Settings → 3D scale: the world renders at this fraction of the
        /// canvas size and is stretched onto it; the HUD stays sharp.
        static WORLD_SCALE: Cell<f32> = const { Cell::new(1.0) };
        /// Phone performance mode (`roomSetLowPower`): no dynamic lights.
        static LOW_POWER: Cell<bool> = const { Cell::new(false) };
        /// CSS playfield scale vs fitted viewport (Settings → Window size). 1.0 = full.
        static WINDOW_SCALE: Cell<f64> = const { Cell::new(1.0) };
        /// Settings → Show frame times: per-stage CPU frame times in `#fps`.
        static PERF_ON: Cell<bool> = const { Cell::new(false) };
        /// Graphics API and backing-store size, for the frame-time readout.
        static PERF_INFO: RefCell<(String, u32, u32)> = const { RefCell::new((String::new(), 0, 0)) };
        /// CPU milliseconds per stage since the last `#fps` update.
        static PERF_TOTALS: RefCell<FramePerf> = const { RefCell::new(FramePerf::ZERO) };
    }

    /// CPU time spent in [`draw_frame_inner`], split by stage (milliseconds).
    #[derive(Clone, Copy)]
    struct FramePerf {
        /// Input, game tics, audio and camera.
        sim: f64,
        /// Scene build, upload and the world pass (or the software renderer).
        world: f64,
        /// Post chain, HUD / native Duke canvas, upload and present.
        overlay: f64,
    }

    impl FramePerf {
        const ZERO: Self = Self {
            sim: 0.0,
            world: 0.0,
            overlay: 0.0,
        };
    }

    fn perf_note(sim: f64, world: f64, overlay: f64, width: u32, height: u32) {
        if !PERF_ON.get() {
            return;
        }
        PERF_TOTALS.with_borrow_mut(|t| {
            t.sim += sim;
            t.world += world;
            t.overlay += overlay;
        });
        PERF_INFO.with_borrow_mut(|info| {
            info.1 = width;
            info.2 = height;
        });
    }

    enum NetModState {
        Idle,
        Fetching,
        /// Catalog name + verified bytes.
        Ready(String, Vec<u8>),
        Failed(String),
    }

    /// Preset library files (`welcome.room.content` + `iwad`) for a coop room.
    enum NetContentState {
        Idle,
        Fetching(String),
        /// Key, preset IWAD bytes (if any), files in mount order.
        Ready(String, Option<Vec<u8>>, Vec<(String, Vec<u8>)>),
        Failed(String),
    }

    enum NetPacksState {
        Idle,
        Fetching(String),
        /// Key (sorted pack names) + file name/bytes pairs.
        Ready(String, Vec<(String, Vec<u8>)>),
        Failed(String),
    }

    /// Start: locate `#room` and run a wgpu rAF loop (demo until IWAD loads).
    #[wasm_bindgen(start)]
    pub fn start() {
        console_error_panic_hook::set_once();
        wasm_bindgen_futures::spawn_local(async {
            match init_loop().await {
                Ok(()) => {}
                Err(e) => {
                    let msg = format!("room-web: {e}");
                    web_sys::console::error_1(&msg.clone().into());
                    // Say it on screen too (phones show no console).
                    if let Some(f) = web_sys::window()
                        .and_then(|w| js_sys::Reflect::get(&w, &"roomShowCrash".into()).ok())
                        .and_then(|f| f.dyn_into::<js_sys::Function>().ok())
                    {
                        let _ = f.call1(&JsValue::NULL, &msg.into());
                    }
                }
            }
        });
    }

    async fn init_loop() -> Result<(), String> {
        let canvas = find_canvas()?;
        // Match backing-store size to CSS box so we never stretch 16:9 into a tall window.
        let (css_width, css_height) = fit_canvas_buffer(&canvas, u32::MAX);
        // `navigator.gpu` exists on Chrome/Linux even when `requestAdapter`
        // returns null. `Instance::new` then locks the instance to WebGPU and
        // never tries WebGL2. Probe first and drop that backend when it fails.
        let instance = wgpu::util::new_instance_with_webgpu_detection(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::BROWSER_WEBGPU | wgpu::Backends::GL,
            ..Default::default()
        })
        .await;
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
        let backend = match adapter.get_info().backend {
            wgpu::Backend::BrowserWebGpu => "WebGPU".to_string(),
            wgpu::Backend::Gl => "WebGL2".to_string(),
            other => format!("{other:?}"),
        };
        let gpu = GpuContext::from_adapter(instance, adapter)
            .await
            .map_err(|e| e.to_string())?;
        web_sys::console::log_1(&format!("room-web: graphics {backend}").into());
        PERF_INFO.with_borrow_mut(|info| info.0 = backend);
        // WebGL2's guaranteed max texture is 2048. A 2× backing store on a
        // large desktop window (2833×1668) is over that and Surface::configure
        // panics. CSS size stays the window; the bitmap is scaled down.
        let max_dim = render_max_dim(gpu.device.limits().max_texture_dimension_2d);
        let (width, height) = fit_canvas_buffer(&canvas, max_dim);
        if width != css_width || height != css_height {
            web_sys::console::log_1(
                &format!(
                    "room-web: backing store {css_width}×{css_height} clamped to {width}×{height} (max texture {max_dim})"
                )
                .into(),
            );
        }
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
        // Scene colour is always Rgba8Unorm (filterable on WebGL2). Unlit +
        // zone pipelines must match that RT; the swapchain may be Bgra8Unorm.
        let scene_format = wgpu::TextureFormat::Rgba8Unorm;
        let mut pass = UnlitPass::new(&gpu, scene_format);
        apply_web_lighting(&mut pass);
        let zone_pass = ModZonePass::new(&gpu, scene_format);
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
        // Final tonemap / present targets the swapchain format (often Bgra8Unorm).
        let post_chain = Some(PostChain::new(&gpu, scene_format, format));
        let scene_color = Some(create_color_rt(&gpu.device, scene_format, width, height));
        let post_ping = Some(create_color_rt(&gpu.device, scene_format, width, height));
        let post_ldr = Some(create_color_rt(&gpu.device, format, width, height));
        let state = Rc::new(RefCell::new(FrameState {
            surface,
            gpu,
            config,
            pass,
            zone_pass,
            depth,
            stencil_aux,
            camera,
            mode: Mode::Demo {
                map: Box::new(map),
                start_ms,
            },
            input_bound: false,
            canvas: None,
            render_settings: RenderSettings::default(),
            post_chain,
            scene_color,
            post_ping,
            post_ldr,
            world_size: (width, height),
            rendermode: 4,
            softscale: 2,
            soft_fov: 90.0,
            soft_visibility: 8.0,
            soft: None,
            web_post: None,
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
        let pending_duke = PENDING_DUKE_MAP.with(|p| p.borrow_mut().take());
        if let Some((grp, map, skill)) = pending_duke
            && let Err(error) = begin_duke_user_map(&state, &grp, &map, skill)
        {
            web_sys::console::error_1(&error);
        }
        if let Some(win) = window() {
            if let Ok(ev) = web_sys::Event::new("room-ready") {
                let _ = win.dispatch_event(&ev);
            }
        }
        request_frame(state);
        Ok(())
    }

    /// Mount a PWAD / PK3 / PK7 / ZIP (a WASM mod, a content pack or a user
    /// file) after the IWAD on single-player starts. A second add under the
    /// same name replaces the first in place, so reloading a file keeps its
    /// load order.
    #[wasm_bindgen(js_name = roomAddPwad)]
    pub fn room_add_pwad(name: &str, data: &[u8]) {
        PWADS.with(|p| {
            let mut list = p.borrow_mut();
            if let Some(slot) = list.iter_mut().find(|(n, _)| n.eq_ignore_ascii_case(name)) {
                slot.1 = data.to_vec();
            } else {
                list.push((name.to_owned(), data.to_vec()));
            }
        });
    }

    /// Unmount the PWAD added as `name` (from the next start). Returns whether
    /// it was mounted.
    #[wasm_bindgen(js_name = roomRemovePwad)]
    pub fn room_remove_pwad(name: &str) -> bool {
        PWADS.with(|p| {
            let mut list = p.borrow_mut();
            let before = list.len();
            list.retain(|(n, _)| !n.eq_ignore_ascii_case(name));
            list.len() != before
        })
    }

    /// Maps of the running single-player game as JSON
    /// `[{"name":"MAP01","title":"Entryway"}, …]` (`[]` when none is running).
    /// Doom uses maps from the mounted IWAD/mod stack; Duke uses native GRP
    /// Build map names such as `E1L1`.
    #[wasm_bindgen(js_name = roomMapList)]
    pub fn room_map_list() -> String {
        let list = APP
            .with(|app| {
                let app = app.borrow();
                let state = app.as_ref()?;
                let s = state.try_borrow().ok()?;
                match &s.mode {
                    Mode::Play(play) => Some(play.map_list()),
                    Mode::NativeDuke { grp, .. } => {
                        let layers: Vec<&[u8]> = grp.iter().map(Vec::as_slice).collect();
                        room_build::list_grp_maps(&layers).ok().map(|maps| {
                            maps.into_iter().map(|name| (name, String::new())).collect()
                        })
                    }
                    _ => None,
                }
            })
            .unwrap_or_default();
        let rows: Vec<serde_json::Value> = list
            .into_iter()
            .map(|(name, title)| serde_json::json!({ "name": name, "title": title }))
            .collect();
        serde_json::Value::Array(rows).to_string()
    }

    /// Map being played (`""` outside a single-player game).
    #[wasm_bindgen(js_name = roomCurrentMap)]
    pub fn room_current_map() -> String {
        APP.with(|app| {
            let app = app.borrow();
            let state = app.as_ref()?;
            let s = state.try_borrow().ok()?;
            match &s.mode {
                Mode::Play(play) => Some(play.current_map_name()),
                Mode::NativeDuke { campaign_map, .. } => Some(campaign_map.clone()),
                _ => None,
            }
        })
        .unwrap_or_default()
    }

    /// Names of the mounted PWADs, in load order.
    #[wasm_bindgen(js_name = roomPwadNames)]
    pub fn room_pwad_names() -> Vec<String> {
        PWADS.with(|p| p.borrow().iter().map(|(n, _)| n.clone()).collect())
    }

    /// Whether `data` is a resource archive Room can mount as a PWAD: a
    /// `PWAD` / `IWAD` WAD, or a ZIP-based PK3 / PK7 / IPK3 / PKE.
    #[wasm_bindgen(js_name = roomIsResourceArchive)]
    pub fn room_is_resource_archive(data: &[u8]) -> bool {
        data.len() >= 4
            && (&data[..4] == b"PWAD"
                || &data[..4] == b"IWAD"
                || &data[..4] == b"PK\x03\x04"
                || &data[..4] == b"7z\xbc\xaf")
    }

    /// Override the root CON script used by browser native Duke sessions.
    #[wasm_bindgen(js_name = roomSetDukeCon)]
    pub fn room_set_duke_con(data: &[u8]) {
        DUKE_CON.with(|script| *script.borrow_mut() = Some(data.to_vec()));
    }

    /// Add or replace an include file resolved by browser native Duke CON.
    #[wasm_bindgen(js_name = roomAddDukeConInclude)]
    pub fn room_add_duke_con_include(name: &str, data: &[u8]) {
        DUKE_CON_INCLUDES.with(|includes| {
            let mut includes = includes.borrow_mut();
            if let Some((_, bytes)) = includes
                .iter_mut()
                .find(|(existing, _)| existing.eq_ignore_ascii_case(name))
            {
                *bytes = data.to_vec();
            } else {
                includes.push((name.to_owned(), data.to_vec()));
            }
        });
    }

    /// Append an Atomic-compatible add-on GRP for native single-player Duke.
    /// Later archives override duplicate maps, tiles, sounds, and CON entries.
    #[wasm_bindgen(js_name = roomAddDukeGrp)]
    pub fn room_add_duke_grp(data: &[u8]) -> Result<(), JsValue> {
        if !room_build::is_grp(data) {
            return Err(JsValue::from_str("Duke add-on must be a KenSilverman GRP"));
        }
        DUKE_ADDON_GRPS.with(|addons| addons.borrow_mut().push(data.to_vec()));
        Ok(())
    }

    /// Mount the path-preserving `RDUKMOD1` bundle produced by
    /// `bundle-solo-content.sh`. The single browser/WASM transfer avoids
    /// thousands of per-file requests while retaining CON include paths.
    #[wasm_bindgen(js_name = roomAddDukeModBundle)]
    pub fn room_add_duke_mod_bundle(data: &[u8]) -> Result<(), JsValue> {
        const HEADER: &[u8; 8] = b"RDUKMOD1";
        const MAX_FILES: usize = 4096;
        const MAX_FILE_BYTES: usize = 64 * 1024 * 1024;
        const MAX_BUNDLE_BYTES: usize = 256 * 1024 * 1024;

        if data.len() > MAX_BUNDLE_BYTES {
            return Err(JsValue::from_str(
                "Duke mod bundle exceeds the 256 MiB limit",
            ));
        }
        if data.len() < 12 || &data[..8] != HEADER {
            return Err(JsValue::from_str("Duke mod bundle has an invalid header"));
        }
        let mut offset = 8;
        let file_count = u32::from_le_bytes(
            data[offset..offset + 4]
                .try_into()
                .map_err(|_| JsValue::from_str("Duke mod bundle header is truncated"))?,
        ) as usize;
        offset += 4;
        if file_count > MAX_FILES {
            return Err(JsValue::from_str("Duke mod bundle has too many files"));
        }

        let mut files = Vec::with_capacity(file_count);
        for _ in 0..file_count {
            let header_end = offset
                .checked_add(6)
                .filter(|end| *end <= data.len())
                .ok_or_else(|| JsValue::from_str("Duke mod bundle entry is truncated"))?;
            let path_len = u16::from_le_bytes([data[offset], data[offset + 1]]) as usize;
            let file_len = u32::from_le_bytes(
                data[offset + 2..header_end]
                    .try_into()
                    .map_err(|_| JsValue::from_str("Duke mod bundle entry is truncated"))?,
            ) as usize;
            offset = header_end;
            if path_len == 0 || file_len > MAX_FILE_BYTES {
                return Err(JsValue::from_str(
                    "Duke mod bundle entry exceeds its limits",
                ));
            }
            let path_end = offset
                .checked_add(path_len)
                .filter(|end| *end <= data.len())
                .ok_or_else(|| JsValue::from_str("Duke mod bundle path is truncated"))?;
            let path = std::str::from_utf8(&data[offset..path_end])
                .map_err(|_| JsValue::from_str("Duke mod bundle path is not UTF-8"))?
                .replace('\\', "/");
            let path = path.trim_start_matches("./");
            if path.is_empty()
                || path.starts_with('/')
                || path.as_bytes().get(1) == Some(&b':')
                || path
                    .split('/')
                    .any(|component| component.is_empty() || component == "." || component == "..")
            {
                return Err(JsValue::from_str("Duke mod bundle path is invalid"));
            }
            offset = path_end;
            let file_end = offset
                .checked_add(file_len)
                .filter(|end| *end <= data.len())
                .ok_or_else(|| JsValue::from_str("Duke mod bundle file is truncated"))?;
            files.push((path.to_owned(), data[offset..file_end].to_vec()));
            offset = file_end;
        }
        if offset != data.len() {
            return Err(JsValue::from_str("Duke mod bundle has trailing bytes"));
        }

        let root_con = files
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case("eduke.con"))
            .or_else(|| {
                files
                    .iter()
                    .rev()
                    .find(|(name, _)| name.to_ascii_lowercase().ends_with("/eduke.con"))
            })
            .map(|(_, bytes)| bytes.clone());
        if root_con.is_none() {
            return Err(JsValue::from_str(
                "Duke mod bundle does not contain eduke.con",
            ));
        }
        DUKE_MOD_FILES.with(|mounted| *mounted.borrow_mut() = files);
        DUKE_CON.with(|script| *script.borrow_mut() = root_con);
        Ok(())
    }

    /// Remove the loose files of a previously mounted web Duke mod.
    #[wasm_bindgen(js_name = roomClearDukeModFiles)]
    pub fn room_clear_duke_mod_files() {
        DUKE_MOD_FILES.with(|files| files.borrow_mut().clear());
        DUKE_CON.with(|script| *script.borrow_mut() = None);
    }

    /// Add one path-preserving file from a Duke mod directory or ZIP archive.
    /// The browser streams and caches files individually, so large mod packs
    /// do not need to be expanded into one extra in-memory archive.
    #[wasm_bindgen(js_name = roomAddDukeModFile)]
    pub fn room_add_duke_mod_file(name: &str, data: &[u8]) -> Result<(), JsValue> {
        let name = name.replace('\\', "/");
        let normalized = name.trim_start_matches("./");
        if normalized.is_empty()
            || normalized.starts_with('/')
            || normalized
                .split('/')
                .any(|component| component.is_empty() || component == "..")
        {
            return Err(JsValue::from_str("Duke mod file path is invalid"));
        }
        let key = normalized.to_ascii_lowercase();
        let bytes = data.to_vec();
        DUKE_MOD_FILES.with(|files| {
            let mut files = files.borrow_mut();
            if let Some((existing, existing_bytes)) = files
                .iter_mut()
                .find(|(existing, _)| existing.eq_ignore_ascii_case(normalized))
            {
                *existing = normalized.to_owned();
                *existing_bytes = bytes.clone();
            } else {
                files.push((normalized.to_owned(), bytes.clone()));
            }
        });
        if key == "eduke.con" || key.ends_with("/eduke.con") {
            DUKE_CON.with(|script| *script.borrow_mut() = Some(bytes));
        }
        Ok(())
    }

    /// Replace the current native editor GRP overlay used by live play tests.
    #[wasm_bindgen(js_name = roomSetDukeEditorGrp)]
    pub fn room_set_duke_editor_grp(data: &[u8]) -> Result<(), JsValue> {
        if !room_build::is_grp(data) {
            return Err(JsValue::from_str("Duke editor overlay must be a GRP"));
        }
        DUKE_EDITOR_GRP.with(|grp| *grp.borrow_mut() = Some(data.to_vec()));
        Ok(())
    }

    /// Start a native Duke session from a GRP resource archive and standalone
    /// Build `.MAP` bytes. Skill follows the 1..=5 Atomic user-facing range.
    #[wasm_bindgen(js_name = roomStartDukeMap)]
    pub fn room_start_duke_map(grp: &[u8], map: &[u8], skill: u8) -> Result<(), JsValue> {
        let skill = skill.clamp(1, 5).saturating_sub(1);
        let ready = APP.with(|app| app.borrow().clone());
        let Some(state) = ready else {
            PENDING_DUKE_MAP.with(|pending| {
                *pending.borrow_mut() = Some((grp.to_vec(), map.to_vec(), skill));
            });
            set_status("Duke map queued — waiting for GPU…");
            return Ok(());
        };
        begin_duke_user_map(&state, grp, map, skill)
    }

    fn begin_duke_user_map(
        state: &Shared,
        grp: &[u8],
        map: &[u8],
        skill: u8,
    ) -> Result<(), JsValue> {
        if !room_build::is_grp(grp) {
            return Err(JsValue::from_str(
                "native Duke user map requires a GRP resource archive",
            ));
        }
        let grp_layers = duke_grp_layers(grp);
        let layer_refs: Vec<&[u8]> = grp_layers.iter().map(Vec::as_slice).collect();
        let override_con = DUKE_CON.with(|script| script.borrow().clone());
        let mut includes = DUKE_CON_INCLUDES.with(|files| files.borrow().clone());
        DUKE_MOD_FILES.with(|files| {
            includes.extend(
                files
                    .borrow()
                    .iter()
                    .filter(|(name, _)| name.to_ascii_lowercase().ends_with(".con"))
                    .cloned(),
            );
        });
        let mut resolver = BrowserDukeIncludes(includes);
        let dialect = if DUKE_MOD_FILES.with(|files| !files.borrow().is_empty()) {
            room_duke::ConDialect::EDuke32
        } else {
            room_duke::ConDialect::Atomic
        };
        let session = room_duke::session_from_grp_map_bytes_with_con_and_includes_dialect(
            &layer_refs,
            map,
            skill,
            override_con.as_deref(),
            Some(&mut resolver),
            dialect,
        )
        .map_err(|error| JsValue::from_str(&format!("Duke user map: {error}")))?;
        let renderer = duke_renderer(&grp_layers)
            .map_err(|error| JsValue::from_str(&format!("Duke renderer: {error}")))?;
        enter_native_duke_mode(state, session, renderer, grp_layers, "USER")
    }

    pub(super) fn start_native_duke_map_for_editor(
        grp: &[u8],
        map: &[u8],
        skill: u8,
    ) -> Result<(), JsValue> {
        let state = APP
            .with(|app| app.borrow().clone())
            .ok_or_else(|| JsValue::from_str("native Duke renderer is not ready"))?;
        begin_duke_user_map(&state, grp, map, skill.clamp(1, 5).saturating_sub(1))
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
            if room_build::is_grp(data) && !ws.contains("/v2/") {
                return Err(JsValue::from_str(
                    "native Duke coop requires a room-server v2 (/v2/rooms/…) link",
                ));
            }
            if room_build::is_grp(data)
                && DUKE_ADDON_GRPS.with(|addons| !addons.borrow().is_empty())
            {
                return Err(JsValue::from_str(
                    "native Duke coop cannot send local add-on GRPs to the room; remove them before joining",
                ));
            }
            if room_build::is_grp(data) && DUKE_MOD_FILES.with(|files| !files.borrow().is_empty()) {
                return Err(JsValue::from_str(
                    "native Duke multiplayer cannot send local mod directory files to the room",
                ));
            }
            return begin_net_lobby(state, data, map, &ws);
        }
        set_status("Loading IWAD…");
        let skill = room_url_param("skill")
            .and_then(|s| s.parse::<u8>().ok())
            .unwrap_or(3)
            .clamp(1, 5);
        if room_build::is_grp(data) {
            let map_name = map.unwrap_or("E1L1");
            let grp_layers = duke_grp_layers(data);
            let renderer = duke_renderer(&grp_layers)
                .map_err(|e| JsValue::from_str(&format!("Duke renderer: {e}")))?;
            let mut session = load_duke_grp_map(&grp_layers, map_name, skill - 1)
                .map_err(|e| JsValue::from_str(&format!("Duke start: {e}")))?;
            session.campaign = duke_campaign_progress(map_name);
            enter_native_duke_mode(state, session, renderer, grp_layers, map_name)?;
            web_sys::console::log_1(
                &format!("room-web: entered native Duke {map_name} (skill {skill})").into(),
            );
            return Ok(());
        }
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

    fn enter_native_duke_mode(
        state: &Shared,
        session: room_duke::DukeSession,
        renderer: room_duke::DukeSoftRenderer,
        grp_layers: Vec<Vec<u8>>,
        map_name: &str,
    ) -> Result<(), JsValue> {
        let mut s = state.borrow_mut();
        let width = s.config.width.max(1);
        let height = s.config.height.max(1);
        let mut backdrop = load_embedded_demo_map().map_err(|e| JsValue::from_str(&e))?;
        apply_nodes(&mut backdrop);
        let (hud_w, hud_h) = hud_dims(width, height);
        s.canvas = Some(CanvasGpu::new(&s.gpu, s.config.format, hud_w, hud_h));
        s.mode = Mode::NativeDuke {
            session: Box::new(session),
            renderer: Box::new(renderer),
            grp: grp_layers,
            audio: None,
            backdrop: Box::new(backdrop),
            last_ms: now_ms(),
            tic_accum: 0.0,
            network: None,
            network_tic: 0,
            campaign_map: map_name.to_owned(),
            duke_music_track: None,
        };
        input_hub::reset();
        input_hub::set_duke_mode(true);
        set_status("Native Duke — WASD move, mouse/Q/arrows turn, Ctrl/click fire, Space jump");
        hide_boot_overlay();
        Ok(())
    }

    fn duke_campaign_progress(map: &str) -> Option<room_duke::DukeCampaignProgress> {
        let normalized = map.to_ascii_uppercase();
        let bytes = normalized.as_bytes();
        if bytes.len() != 4 || bytes[0] != b'E' || bytes[2] != b'L' {
            return None;
        }
        let volume = bytes[1].checked_sub(b'1')?;
        let level = bytes[3].checked_sub(b'1')?;
        room_duke::DukeCampaignProgress::new(volume, level)
    }

    /// Start/resume the browser mixer on a user gesture and load Duke VOCs
    /// from the active GRP using the native CON sound-number table.
    fn ensure_duke_audio(
        audio_slot: &mut Option<crate::web_audio::WebAudio>,
        music_track: &mut Option<String>,
        session: &room_duke::DukeSession,
        grp_layers: &[Vec<u8>],
    ) {
        if let Some(audio) = audio_slot.as_ref() {
            audio.resume();
            start_duke_map_music(audio, music_track, session, grp_layers);
            return;
        }
        let audio = match crate::web_audio::WebAudio::start() {
            Ok(audio) => audio,
            Err(error) => {
                web_sys::console::warn_1(&format!("room-web: Duke audio: {error:?}").into());
                return;
            }
        };
        let mut bank = room_audio::SampleBank::new();
        let layer_refs: Vec<&[u8]> = grp_layers.iter().map(Vec::as_slice).collect();
        if let Ok(grp) = room_build::grp::GrpStack::parse_layers(&layer_refs) {
            if let Some(script) = session.script.as_ref() {
                for (id, def) in script.sounds.iter().enumerate() {
                    let Some(def) = def else { continue };
                    let Some(bytes) = grp.get(&def.file) else {
                        continue;
                    };
                    let Ok(decoded) = room_formats::decode_sound(bytes) else {
                        continue;
                    };
                    let channels = usize::from(decoded.channels.max(1));
                    let mono: Vec<f32> = decoded
                        .samples
                        .chunks_exact(channels)
                        .map(|frame| frame.iter().sum::<f32>() / channels as f32)
                        .collect();
                    bank.insert_resampled(
                        room_core::SoundId::from_raw(id as u32),
                        &mono,
                        decoded.rate.max(1),
                        audio.sample_rate(),
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
                    let channels = usize::from(decoded.channels.max(1));
                    let mono: Vec<f32> = decoded
                        .samples
                        .chunks_exact(channels)
                        .map(|frame| frame.iter().sum::<f32>() / channels as f32)
                        .collect();
                    bank.insert_resampled(
                        room_core::SoundId::from_raw(DUKE_RTS_SOUND_BASE + index as u32),
                        &mono,
                        decoded.rate.max(1),
                        audio.sample_rate(),
                    );
                }
            }
        }
        if let Ok(mut mixer) = audio.mixer.try_borrow_mut() {
            mixer.bank.merge_missing(&bank);
        }
        start_duke_map_music(&audio, music_track, session, grp_layers);
        *audio_slot = Some(audio);
        web_sys::console::log_1(&"room-web: Duke weapon sounds ready".into());
    }

    /// Select Atomic's CON `music` entry for the active campaign map.
    fn start_duke_map_music(
        audio: &crate::web_audio::WebAudio,
        current_track: &mut Option<String>,
        session: &room_duke::DukeSession,
        grp_layers: &[Vec<u8>],
    ) {
        let Some(progress) = session.campaign else {
            return;
        };
        let Some(name) = session
            .script
            .as_ref()
            .and_then(|script| {
                script
                    .music_tracks
                    .get(usize::from(progress.volume) * 11 + usize::from(progress.level))
            })
            .filter(|name| !name.is_empty())
        else {
            return;
        };
        if current_track.as_deref() == Some(name.as_str()) {
            return;
        }
        let layer_refs: Vec<&[u8]> = grp_layers.iter().map(Vec::as_slice).collect();
        let Ok(grp) = room_build::grp::GrpStack::parse_layers(&layer_refs) else {
            return;
        };
        let Some(midi) = grp.get(name) else {
            web_sys::console::warn_1(
                &format!("room-web: Duke music {name} missing from GRP").into(),
            );
            return;
        };
        if let Ok(mut music) = audio.music.try_borrow_mut() {
            music.play_midi(name.clone(), midi.to_vec(), true);
            *current_track = Some(name.clone());
        }
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
            .clamp(2, 64);
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
            .unwrap_or_else(|| {
                if room_build::is_grp(data) {
                    "E1L1"
                } else {
                    "E1M1"
                }
                .into()
            });
        let ticket = room_url_param("ticket");
        let spectate = room_url_param("spectate").is_some_and(|v| v != "0");
        let cfg = NetJoinConfig {
            url: ws_url.to_owned(),
            players,
            name,
            fingerprint: iwad_fingerprint(data),
            map: map_name,
            skill,
            duke_game_mode: 1,
            duke_friendly_fire: false,
            doom_deathmatch: false,
            rng_seed,
            ticket,
            spectate,
            game_mod: None,
            packs: Vec::new(),
            content: Vec::new(),
            iwad: None,
            time_limit: 0,
            fill_bots: false,
            bot_first_seat: 0,
            bot_skill: 0,
        };
        NET_MOD.with(|n| *n.borrow_mut() = NetModState::Idle);
        NET_CONTENT.with(|n| *n.borrow_mut() = NetContentState::Idle);
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
            Mode::NativeDuke { .. } => {
                let mut m = load_embedded_demo_map().map_err(|e| JsValue::from_str(&e))?;
                apply_nodes(&mut m);
                (Box::new(m), now_ms())
            }
        };
        s.mode = Mode::Lobby {
            lobby: Box::new(lobby),
            iwad: data.to_vec(),
            prepared: None,
            prepared_duke: None,
            prepared_duke_renderer: None,
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
            let dt = 1.0 / play.game.tic_rate() as f32;
            let _ = play.tick_play(dt);
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
        input_hub::set_duke_mode(false);
        // Sound environments (`snd_reverb`, `snd_waterreverb`, both on by
        // default); `?reverb=0` / `?waterreverb=0` turn them off.
        play.set_reverb(
            room_url_param("reverb").is_none_or(|v| v != "0"),
            room_url_param("waterreverb").is_none_or(|v| v != "0"),
        );
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
            let (hud_w, hud_h) = hud_dims(s.config.width.max(1), s.config.height.max(1));
            s.canvas = Some(CanvasGpu::new(&s.gpu, fmt, hud_w, hud_h));
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
        // Native Duke rooms load a GRP rather than Doom IWAD + PWADs. Prepare
        // the Build session and validate all requested APLAYER seats before
        // entering the same Ready barrier as Doom rooms.
        let is_duke = {
            let s = state.borrow();
            matches!(&s.mode, Mode::Lobby { iwad, .. } if room_build::is_grp(iwad))
        };
        if is_duke {
            let (grp, map, skill, players, game_mode, friendly_fire) = {
                let s = state.borrow();
                let Mode::Lobby { lobby, iwad, .. } = &s.mode else {
                    return Ok(());
                };
                if !lobby.needs_map_prepare() {
                    return Ok(());
                }
                let Some((map, skill, _, players, _, game_mode, friendly_fire)) =
                    lobby.prepare_opts()
                else {
                    return Ok(());
                };
                (iwad.clone(), map, skill, players, game_mode, friendly_fire)
            };
            let grp_layers = duke_grp_layers(&grp);
            let mut session = load_duke_grp_map(&grp_layers, &map, skill.saturating_sub(1))
                .map_err(|e| JsValue::from_str(&format!("Duke prepare: {e}")))?;
            // Keep the Atomic episode/map cursor in networked sessions too;
            // the deterministic exit request is then resolved by every peer
            // from the same lockstep state.
            session.campaign = duke_campaign_progress(&map);
            if let Err(error) = session.validate_network_content() {
                let message = format!("Duke multiplayer unavailable: {error}");
                set_status(&message);
                set_lobby_html(&format!(
                    "<h2>Multiplayer unavailable</h2><p class=\"err\">{}</p>",
                    html_escape(&message)
                ));
                return Err(JsValue::from_str(&message));
            }
            session
                .configure_multiplayer_mode(usize::from(players), game_mode, friendly_fire)
                .map_err(|e| JsValue::from_str(&format!("Duke coop setup: {e}")))?;
            let renderer = duke_renderer(&grp_layers)
                .map_err(|e| JsValue::from_str(&format!("Duke renderer: {e}")))?;
            let mut s = state.borrow_mut();
            if let Mode::Lobby {
                lobby,
                prepared_duke,
                prepared_duke_renderer,
                ..
            } = &mut s.mode
            {
                *prepared_duke = Some(Box::new(session));
                *prepared_duke_renderer = Some(Box::new(renderer));
                lobby.send_v2_ready_after_prepare(Vec::new());
            }
            return Ok(());
        }
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
            let Some((map, skill, rng_seed, players, seat, _, _)) = lobby.prepare_opts() else {
                return Ok(());
            };
            (
                iwad.clone(),
                PlayStartOpts {
                    map_name: Some(map),
                    skill,
                    rng_seed,
                    coop_players: if lobby.fill_bots() {
                        players
                    } else {
                        players.min(4)
                    },
                    local_seat: seat,
                    pwads,
                    time_limit_secs: lobby.time_limit(),
                    session_names: lobby.session_names(),
                    // Bot seats are only known at the server's Start: prepare
                    // with none (set_bot_seats applies them before tic 0).
                    fill_bots: false,
                    bot_first_seat: 0,
                    bot_skill: lobby.bot_skill(),
                    deathmatch: lobby.doom_deathmatch(),
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

    /// PWADs for the room's mod + packs: `Some` once ready (empty = vanilla),
    /// `None` while a download runs.
    fn coop_mod_pwads(state: &Shared) -> Result<Option<NamedPwads>, JsValue> {
        let (game_mod, packs, ws_url, content, preset_iwad) = {
            let s = state.borrow();
            let Mode::Lobby { lobby, .. } = &s.mode else {
                return Ok(Some(Vec::new()));
            };
            (
                lobby.game_mod().cloned(),
                lobby.packs().to_vec(),
                lobby.ws_url().to_owned(),
                lobby.content().to_vec(),
                lobby.preset_iwad().cloned(),
            )
        };
        if let Err(e) = kick_coop_content_download(&content, preset_iwad.as_ref(), &ws_url) {
            set_status(&format!("Download failed: {e}"));
            set_lobby_html(&format!(
                "<h2>Download failed</h2><p class=\"err\">{}</p>",
                html_escape(&e)
            ));
            return Err(JsValue::from_str(&e));
        }
        let content_key = content_key(&content, preset_iwad.as_ref());
        let content_ready: Option<(Option<Vec<u8>>, NamedPwads)> = if content_key.is_empty() {
            Some((None, Vec::new()))
        } else {
            NET_CONTENT.with(|n| match &*n.borrow() {
                NetContentState::Ready(k, iwad, files) if *k == content_key => {
                    Some((iwad.clone(), files.clone()))
                }
                _ => None,
            })
        };
        if let Some(e) = NET_CONTENT.with(|n| match &*n.borrow() {
            NetContentState::Failed(e) => Some(e.clone()),
            _ => None,
        }) {
            set_status(&format!("Download failed: {e}"));
            return Err(JsValue::from_str(&e));
        }
        let Some((content_iwad, content_pwads)) = content_ready else {
            return Ok(None);
        };
        // The preset's base game replaces the local IWAD for this room.
        if let Some(bytes) = content_iwad {
            let mut s = state.borrow_mut();
            if let Mode::Lobby { iwad, .. } = &mut s.mode {
                if *iwad != bytes {
                    *iwad = bytes;
                }
            }
        }
        if let Err(e) = kick_coop_mod_download(game_mod.as_ref(), &ws_url) {
            set_status(&format!("Mod download failed: {e}"));
            set_lobby_html(&format!(
                "<h2>Mod download failed</h2><p class=\"err\">{}</p>",
                html_escape(&e)
            ));
            return Err(JsValue::from_str(&e));
        }
        if let Err(e) = kick_coop_packs_download(&packs, &ws_url) {
            set_status(&format!("Pack download failed: {e}"));
            set_lobby_html(&format!(
                "<h2>Pack download failed</h2><p class=\"err\">{}</p>",
                html_escape(&e)
            ));
            return Err(JsValue::from_str(&e));
        }
        let mod_pwads = match &game_mod {
            None => Some(Vec::new()),
            Some(m) => NET_MOD.with(|n| match &*n.borrow() {
                NetModState::Ready(name, bytes) if *name == m.name => {
                    Some(vec![(format!("{name}.wad"), bytes.clone())])
                }
                NetModState::Failed(e) => {
                    let _ = e;
                    None
                }
                _ => None,
            }),
        };
        let pack_key = packs_key(&packs);
        let pack_pwads = if packs.is_empty() {
            Some(Vec::new())
        } else {
            NET_PACKS.with(|n| match &*n.borrow() {
                NetPacksState::Ready(key, files) if *key == pack_key => Some(files.clone()),
                _ => None,
            })
        };
        match (mod_pwads, pack_pwads) {
            (Some(m), Some(p)) => {
                // Preset content first (resource mods), then gameplay mod, then packs.
                let mut all = content_pwads;
                all.extend(m);
                all.extend(p);
                Ok(Some(all))
            }
            _ => {
                // Surface a failed mod/pack that already finished.
                if let Some(e) = NET_MOD.with(|n| match &*n.borrow() {
                    NetModState::Failed(e) => Some(e.clone()),
                    _ => None,
                }) {
                    set_status(&format!("Mod download failed: {e}"));
                    return Err(JsValue::from_str(&e));
                }
                if let Some(e) = NET_PACKS.with(|n| match &*n.borrow() {
                    NetPacksState::Failed(e) => Some(e.clone()),
                    _ => None,
                }) {
                    set_status(&format!("Pack download failed: {e}"));
                    return Err(JsValue::from_str(&e));
                }
                Ok(None)
            }
        }
    }

    fn kick_coop_mod_download(
        game_mod: Option<&room_proto::RoomModRef>,
        ws_url: &str,
    ) -> Result<(), String> {
        let Some(m) = game_mod else {
            return Ok(());
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
        })?;
        if start {
            set_status(&format!("Downloading mod {}…", m.title));
            set_lobby_html(&format!(
                "<h2>Downloading mod…</h2><p class=\"muted\">{} ({} KiB)</p>",
                html_escape(&m.title),
                m.size.div_ceil(1024)
            ));
            let url = mod_download_url(ws_url, &m.name);
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
        Ok(())
    }

    fn content_key(
        content: &[room_proto::RoomContentRef],
        iwad: Option<&room_proto::RoomContentRef>,
    ) -> String {
        iwad.map(|i| format!("iwad:{}@{}", i.file, i.fingerprint))
            .into_iter()
            .chain(
                content
                    .iter()
                    .map(|c| format!("{}@{}", c.file, c.fingerprint)),
            )
            .collect::<Vec<_>>()
            .join(",")
    }

    /// `ws[s]://host/…` → `http[s]://host/api/content/{file}`.
    fn content_download_url(ws_url: &str, file: &str) -> String {
        let (scheme, rest) = if let Some(r) = ws_url.strip_prefix("wss://") {
            ("https", r)
        } else if let Some(r) = ws_url.strip_prefix("ws://") {
            ("http", r)
        } else {
            return format!("/api/content/{file}");
        };
        let host = rest.split('/').next().unwrap_or(rest);
        format!("{scheme}://{host}/api/content/{file}")
    }

    /// Download the room preset's IWAD + library files (fingerprint-checked).
    fn kick_coop_content_download(
        content: &[room_proto::RoomContentRef],
        iwad: Option<&room_proto::RoomContentRef>,
        ws_url: &str,
    ) -> Result<(), String> {
        let key = content_key(content, iwad);
        if key.is_empty() {
            return Ok(());
        }
        let start = NET_CONTENT.with(|n| {
            let mut n = n.borrow_mut();
            match &*n {
                NetContentState::Ready(k, ..) if *k == key => Ok(false),
                NetContentState::Fetching(k) if *k == key => Ok(false),
                NetContentState::Failed(e) => Err(e.clone()),
                _ => {
                    *n = NetContentState::Fetching(key.clone());
                    Ok(true)
                }
            }
        })?;
        if !start {
            return Ok(());
        }
        let total: u64 = iwad
            .iter()
            .map(|i| i.size)
            .chain(content.iter().map(|c| c.size))
            .sum();
        set_lobby_html(&format!(
            "<h2>Downloading room content…</h2><p class=\"muted\">{} file(s), {:.1} MB</p>",
            content.len() + usize::from(iwad.is_some()),
            total as f64 / (1024.0 * 1024.0)
        ));
        let content = content.to_vec();
        let iwad = iwad.cloned();
        let ws = ws_url.to_owned();
        wasm_bindgen_futures::spawn_local(async move {
            let fetch = |r: room_proto::RoomContentRef| {
                let url = content_download_url(&ws, &r.file);
                async move {
                    set_status(&format!(
                        "Downloading {} ({:.1} MB)…",
                        r.file,
                        r.size as f64 / (1024.0 * 1024.0)
                    ));
                    let bytes = fetch_bytes(&url).await?;
                    let fp = format!("{:016x}", room_proto::content_fingerprint(&bytes));
                    if fp == r.fingerprint {
                        Ok((r.file.clone(), bytes))
                    } else {
                        Err(format!(
                            "{} has fingerprint {fp}, the room expects {}",
                            r.file, r.fingerprint
                        ))
                    }
                }
            };
            let mut iwad_bytes = None;
            if let Some(r) = iwad {
                match fetch(r).await {
                    Ok((_, b)) => iwad_bytes = Some(b),
                    Err(e) => {
                        NET_CONTENT.with(|n| *n.borrow_mut() = NetContentState::Failed(e));
                        return;
                    }
                }
            }
            let mut files = Vec::new();
            for r in content {
                match fetch(r).await {
                    Ok(f) => files.push(f),
                    Err(e) => {
                        NET_CONTENT.with(|n| *n.borrow_mut() = NetContentState::Failed(e));
                        return;
                    }
                }
            }
            NET_CONTENT.with(|n| *n.borrow_mut() = NetContentState::Ready(key, iwad_bytes, files));
        });
        Ok(())
    }

    fn packs_key(packs: &[String]) -> String {
        let mut v = packs.to_vec();
        v.sort();
        v.join(",")
    }

    fn kick_coop_packs_download(packs: &[String], ws_url: &str) -> Result<(), String> {
        if packs.is_empty() {
            return Ok(());
        }
        let key = packs_key(packs);
        let start = NET_PACKS.with(|n| {
            let mut n = n.borrow_mut();
            match &*n {
                NetPacksState::Ready(k, _) if *k == key => Ok(false),
                NetPacksState::Fetching(k) if *k == key => Ok(false),
                NetPacksState::Failed(e) => Err(e.clone()),
                _ => {
                    *n = NetPacksState::Fetching(key.clone());
                    Ok(true)
                }
            }
        })?;
        if start {
            set_status(&format!("Downloading packs ({key})…"));
            let packs = packs.to_vec();
            let ws = ws_url.to_owned();
            let key_owned = key.clone();
            wasm_bindgen_futures::spawn_local(async move {
                let mut files = Vec::new();
                for name in &packs {
                    match fetch_pack_bytes(&ws, name).await {
                        Ok((file, bytes)) => files.push((file, bytes)),
                        Err(e) => {
                            NET_PACKS.with(|n| {
                                *n.borrow_mut() = NetPacksState::Failed(e);
                            });
                            return;
                        }
                    }
                }
                NET_PACKS.with(|n| {
                    *n.borrow_mut() = NetPacksState::Ready(key_owned, files);
                });
            });
        }
        Ok(())
    }

    /// The downloaded preset content + coop mod + packs, if any.
    fn ready_mod_pwads() -> Vec<(String, Vec<u8>)> {
        let mut out = NET_CONTENT.with(|n| match &*n.borrow() {
            NetContentState::Ready(_, _, files) => files.clone(),
            _ => Vec::new(),
        });
        out.extend(NET_MOD.with(|n| match &*n.borrow() {
            NetModState::Ready(name, bytes) => vec![(format!("{name}.wad"), bytes.clone())],
            _ => Vec::new(),
        }));
        NET_PACKS.with(|n| {
            if let NetPacksState::Ready(_, files) = &*n.borrow() {
                out.extend(files.iter().map(|(f, b)| (f.clone(), b.clone())));
            }
        });
        out
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

    /// Content packs live under `/play/mods/` on the game bind (same host as the WS).
    fn pack_download_urls(ws_url: &str, name: &str) -> Vec<String> {
        let (scheme, rest) = if let Some(r) = ws_url.strip_prefix("wss://") {
            ("https", r)
        } else if let Some(r) = ws_url.strip_prefix("ws://") {
            ("http", r)
        } else {
            return vec![
                format!("./mods/{name}.pk3"),
                format!("./mods/{name}.zip"),
                format!("/play/mods/{name}.pk3"),
                format!("/play/mods/{name}.zip"),
            ];
        };
        let host = rest.split('/').next().unwrap_or(rest);
        vec![
            format!("{scheme}://{host}/play/mods/{name}.pk3"),
            format!("{scheme}://{host}/play/mods/{name}.zip"),
            format!("./mods/{name}.pk3"),
            format!("./mods/{name}.zip"),
        ]
    }

    async fn fetch_pack_bytes(ws_url: &str, name: &str) -> Result<(String, Vec<u8>), String> {
        let mut last = format!("pack {name} not found");
        for url in pack_download_urls(ws_url, name) {
            match fetch_bytes(&url).await {
                Ok(bytes) => {
                    let file = url
                        .rsplit('/')
                        .next()
                        .unwrap_or(name)
                        .split('?')
                        .next()
                        .unwrap_or(name)
                        .to_owned();
                    return Ok((file, bytes));
                }
                Err(e) => last = e,
            }
        }
        Err(last)
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
        let (native_grp, map, local, peers) = {
            let s = state.borrow();
            let Mode::Lobby { iwad, .. } = &s.mode else {
                return Err(JsValue::from_str("lobby ended unexpectedly"));
            };
            (
                room_build::is_grp(iwad),
                ready.map.clone(),
                ready.local.0,
                ready.peer_count,
            )
        };
        if native_grp {
            let (grp, session, renderer) = {
                let mut s = state.borrow_mut();
                let Mode::Lobby {
                    iwad,
                    prepared_duke,
                    prepared_duke_renderer,
                    ..
                } = &mut s.mode
                else {
                    return Err(JsValue::from_str("lobby ended unexpectedly"));
                };
                let grp_layers = duke_grp_layers(iwad);
                // Move the prepared native state out of the lobby without
                // cloning its loaded world or renderer material cache.
                let session = prepared_duke
                    .take()
                    .ok_or_else(|| JsValue::from_str("Duke map was not prepared"))?;
                let renderer = prepared_duke_renderer
                    .take()
                    .ok_or_else(|| JsValue::from_str("Duke renderer was not prepared"))?;
                (grp_layers, *session, *renderer)
            };
            let network = crate::netplay::DukeNetPlay::from_ready(ready);
            stop_lobby_pump();
            hide_lobby_overlay();
            enter_native_duke_mode(state, session, renderer, grp, &map)?;
            if let Mode::NativeDuke { network: slot, .. } = &mut state.borrow_mut().mode {
                *slot = Some(network);
            }
            web_sys::console::log_1(
                &format!("room-web: entered native Duke coop map={map} seat={local}/{peers}")
                    .into(),
            );
            return Ok(());
        }
        let peer_count = ready.peer_count;
        let local = ready.local.0;
        let map = ready.map.clone();
        let skill = ready.skill;
        let rng_seed = ready.rng_seed;
        let time_limit_secs = ready.time_limit;
        let fill_bots = ready.fill_bots;
        let bot_first_seat = ready.bot_first_seat;
        let bot_skill = ready.bot_skill;
        let deathmatch = ready.deathmatch;
        let net = NetPlay::from_ready(ready);
        let (iwad, prepared) = {
            let mut s = state.borrow_mut();
            let Mode::Lobby { iwad, prepared, .. } = &mut s.mode else {
                return Err(JsValue::from_str("lobby ended unexpectedly"));
            };
            (iwad.clone(), prepared.take())
        };
        let mut play = if let Some(mut p) = prepared {
            // Prepared before Start: now the server said which seats are bots.
            p.game.set_bot_skill(bot_skill);
            p.game.set_bot_seats(fill_bots, bot_first_seat);
            *p
        } else {
            match PlaySession::start_with(
                &iwad,
                PlayStartOpts {
                    map_name: Some(map),
                    skill,
                    rng_seed,
                    coop_players: if fill_bots {
                        peer_count
                    } else {
                        peer_count.min(4)
                    },
                    local_seat: local,
                    pwads: ready_mod_pwads(),
                    time_limit_secs,
                    session_names: net.session_names(),
                    fill_bots,
                    bot_first_seat,
                    bot_skill,
                    deathmatch,
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
        // Client prediction of the local view (`docs/38`); `?predict=0` turns it off.
        play.set_predict(room_url_param("predict").is_none_or(|v| v != "0"));
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
        // Timed start (bot-fill games): a live countdown; otherwise the
        // game waits for every slot.
        let countdown = match p.starts_in_secs {
            Some(secs) if !p.started && !p.connecting => format!(
                "<p class=\"countdown\" aria-live=\"polite\"><span>Game starts in</span> \
                 <strong>{}:{:02}</strong></p>",
                secs / 60,
                secs % 60
            ),
            _ => String::new(),
        };
        // The match length, so the start countdown above is not mistaken
        // for it.
        let length = match p.time_limit_secs {
            0 => String::new(),
            s if s < 60 => format!(" · {s} s match"),
            s if s % 60 == 0 => format!(" · {} min match", s / 60),
            s => format!(" · {} min {} s match", s / 60, s % 60),
        };
        let hint = if p.starts_in_secs.is_some() {
            if p.fill_bots {
                "When the timer runs out, bots take the empty slots."
            } else {
                "The game starts when the timer runs out."
            }
        } else {
            "Starts when every slot is filled and every player has loaded the map."
        };
        let html = format!(
            "<h2>{phase}</h2>\
             {countdown}\
             <p class=\"meta\"><span>{map}</span> · skill {skill} · {present}/{max} players{length}</p>\
             <p class=\"muted\">{role}</p>\
             <ul class=\"seats\">{seats}</ul>\
             <p class=\"hint\">{hint}</p>",
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
                let in_play = matches!(
                    state_kd.borrow().mode,
                    Mode::Play(_) | Mode::NativeDuke { .. }
                );
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
                if !typing && input_hub::set_duke_rts_hotkey(&code, true, ev.alt_key()) {
                    ev.prevent_default();
                    return;
                }
                if matches!(code.as_str(), "F6" | "F9") && !in_lobby && !typing {
                    // Net save / load hotkeys, not the browser's.
                    ev.prevent_default();
                }
                if matches!(
                    code.as_str(),
                    "Space"
                        | "Tab"
                        | "ArrowUp"
                        | "ArrowDown"
                        | "ArrowLeft"
                        | "ArrowRight"
                        | "Backspace"
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
                let code = ev.code();
                input_hub::set_duke_rts_hotkey(&code, false, ev.alt_key());
                input_hub::set_key(&code, false);
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
                let in_play = matches!(
                    state_click.borrow().mode,
                    Mode::Play(_) | Mode::NativeDuke { .. }
                );
                if !in_play {
                    return;
                }
                {
                    let mut frame = state_click.borrow_mut();
                    match &mut frame.mode {
                        Mode::Play(play) => play.ensure_audio(),
                        Mode::NativeDuke {
                            session,
                            grp,
                            audio,
                            duke_music_track,
                            ..
                        } => ensure_duke_audio(audio, duke_music_track, session, grp),
                        _ => {}
                    }
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
                } else if let Mode::NativeDuke {
                    session,
                    grp,
                    audio,
                    duke_music_track,
                    ..
                } = &mut s.mode
                {
                    ensure_duke_audio(audio, duke_music_track, session, grp);
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
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let view = tex.create_view(&wgpu::TextureViewDescriptor::default());
        (tex, view)
    }

    fn create_color_rt(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        width: u32,
        height: u32,
    ) -> (wgpu::Texture, wgpu::TextureView) {
        let tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("web-color-rt"),
            size: wgpu::Extent3d {
                width: width.max(1),
                height: height.max(1),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::COPY_DST,
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
            // Must match unlit MRT slot 1 (docs/29 U16 G-buffer-lite).
            format: wgpu::TextureFormat::Rgba8Unorm,
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
        let meter = Rc::new(RefCell::new(FpsMeter::default()));
        let cb = Closure::new(move |t: f64| {
            draw_frame(&state2);
            note_fps(&meter, t);
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
        draw_frame_inner(state);
        // Rollback: the speculative world was only lent for drawing.
        if let Ok(mut s) = state.try_borrow_mut()
            && let Mode::Play(play) = &mut s.mode
        {
            play.end_rollback_view();
        }
    }

    fn draw_frame_inner(state: &Shared) {
        let t_start = now_ms();
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
        // Faithful path: Bright + in-shader sRGB. Post path keeps linear scene.
        if !s.render_settings.needs_post() {
            apply_web_lighting(&mut s.pass);
        }
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
                // Rollback: draw this frame from the copy run ahead.
                play.begin_rollback_view();
                // Keep the worklet ring topped up between 35 Hz tics.
                if let Some(audio) = play.audio.as_ref() {
                    if audio.needs_resume() {
                        audio.resume();
                    }
                    audio.feed_output();
                }
                play.view_pose().unwrap_or((s.camera.eye, s.camera.forward))
            }
            Mode::NativeDuke {
                session,
                audio,
                last_ms,
                tic_accum,
                network,
                network_tic,
                campaign_map,
                duke_music_track,
                grp,
                ..
            } => {
                let dt = ((now - *last_ms) as f32 * 0.001).clamp(0.0, 0.1);
                *last_ms = now;
                // Settings panel open in a solo game: no tics until it closes.
                let menu_paused =
                    network.is_none() && crate::play::MENU_PAUSE.with(std::cell::Cell::get);
                if menu_paused {
                    *tic_accum = 0.0;
                } else {
                    *tic_accum += dt;
                }
                let mut sound_events = Vec::new();
                let mut campaign_transition = None;
                if let Some(net) = network.as_mut() {
                    net.keepalive();
                }
                // The web build always looks with the mouse (pointer lock):
                // Chocolate's mouse-aiming mode, without the snap-to-centre
                // dead zone around horiz 100, as on the desktop.
                // Every client sets it for every seat (lockstep stays identical).
                for seat in 0..session.player_count() {
                    if let Some(p) = session.player_for_seat_mut(seat) {
                        p.aim_mode = 1;
                    }
                }
                while *tic_accum >= 1.0 / 30.0 {
                    *tic_accum -= 1.0 / 30.0;
                    if let Some(net) = network.as_mut() {
                        let ang = session
                            .player_for_seat(net.local_seat)
                            .map_or(session.player.ang, |p| p.ang);
                        let input = input_hub::take_duke_network_input(ang);
                        net.submit(input);
                        if let Some(inputs) = net.take_frame(*network_tic) {
                            if let Some(audio) = audio.as_ref()
                                && let Ok(mixer) = audio.mixer.try_borrow()
                            {
                                session.set_active_sound_owners(&mixer.duke_active_sound_owners());
                            }
                            session.tick_inputs(&inputs);
                            *network_tic = network_tic.wrapping_add(1);
                            sound_events.extend(session.drain_sound_events());
                            campaign_transition = session.take_campaign_exit_request();
                        }
                    } else {
                        if let Some(audio) = audio.as_ref()
                            && let Ok(mixer) = audio.mixer.try_borrow()
                        {
                            session.set_active_sound_owners(&mixer.duke_active_sound_owners());
                        }
                        input_hub::apply_duke_weapon(&mut session.player);
                        let input = input_hub::take_duke_input(session.player.ang);
                        session.tick(input);
                        sound_events.extend(session.drain_sound_events());
                        campaign_transition = session.take_campaign_exit_request();
                    }
                    if campaign_transition.is_some() {
                        break;
                    }
                }
                // Catch up: after a network hiccup the server's frames pile
                // up, and running one per 30 Hz step would keep this client
                // behind (input lag) for the rest of the match. A watcher who
                // joined mid-game replays the match from tic 0 the same way.
                if campaign_transition.is_none()
                    && let Some(net) = network.as_mut()
                {
                    let (cushion, budget) = if net.is_spectator() { (1, 300) } else { (2, 8) };
                    let mut ran = 0;
                    while ran < budget && net.backlog(*network_tic) > cushion {
                        let Some(inputs) = net.take_frame(*network_tic) else {
                            break;
                        };
                        let replaying = net.backlog(*network_tic) > 30;
                        session.tick_inputs(&inputs);
                        *network_tic = network_tic.wrapping_add(1);
                        let events = session.drain_sound_events();
                        // Fast replay: skip the sounds of tics nobody sees.
                        if !replaying {
                            sound_events.extend(events);
                        }
                        campaign_transition = session.take_campaign_exit_request();
                        ran += 1;
                        if campaign_transition.is_some() {
                            break;
                        }
                    }
                }
                if let Some(transition) = campaign_transition {
                    match transition {
                        room_duke::DukeCampaignTransition::EndGame => {
                            set_status("Duke campaign complete");
                        }
                        room_duke::DukeCampaignTransition::NextLevel { volume, level } => {
                            let next_map = format!("E{}L{}", volume + 1, level + 1);
                            match load_duke_grp_map(grp, &next_map, session.skill) {
                                Ok(mut next) => match next.inherit_campaign_roster(session) {
                                    Ok(()) => {
                                        next.campaign = duke_campaign_progress(&next_map);
                                        **session = next;
                                        *campaign_map = next_map.clone();
                                        if let Some(audio) = audio.as_ref() {
                                            start_duke_map_music(
                                                audio,
                                                duke_music_track,
                                                session,
                                                grp,
                                            );
                                        }
                                        set_status(&format!("Duke: entered {next_map}"));
                                    }
                                    Err(error) => {
                                        set_status(&format!("Duke next level roster: {error}"));
                                    }
                                },
                                Err(error) => {
                                    set_status(&format!("Duke next level failed: {error}"))
                                }
                            }
                        }
                    }
                }
                if let Some(audio) = audio.as_ref() {
                    if audio.needs_resume() {
                        audio.resume();
                    }
                    if let Ok(mut mixer) = audio.mixer.try_borrow_mut() {
                        mixer.set_paused(menu_paused);
                        let audio_options = DUKE_AUDIO_OPTIONS.get();
                        mixer.duke_sfx_gain = if audio_options.sound {
                            f32::from(audio_options.sfx_volume) / 255.0
                        } else {
                            0.0
                        };
                        mixer.music_gain = if audio_options.music {
                            f32::from(audio_options.music_volume) / 255.0
                        } else {
                            0.0
                        };
                        mixer.reverse_stereo = audio_options.reverse_stereo;
                        if audio_options.sound && !audio_options.lockout {
                            let mut rts_playing = mixer.voices.iter().any(|voice| {
                                let raw = voice.sound.raw();
                                (DUKE_RTS_SOUND_BASE..DUKE_RTS_SOUND_BASE + 10).contains(&raw)
                            });
                            for index in input_hub::take_duke_rts_requests() {
                                if rts_playing {
                                    break;
                                }
                                let sound = room_core::SoundId::from_raw(
                                    DUKE_RTS_SOUND_BASE + index as u32,
                                );
                                if mixer.bank.get(sound).is_none() {
                                    continue;
                                }
                                mixer.apply(&[room_audio::SoundEvent::Start {
                                    origin: room_audio::SoundOrigin::None,
                                    channel: -100 - index as i32,
                                    sound,
                                    check_sound: None,
                                    start_time: 0.0,
                                    volume: 1.0,
                                    attenuation: 1.0,
                                    pitch: 1.0,
                                    flags: room_audio::SoundFlags::default(),
                                    source_id: None,
                                    no_restart: false,
                                }]);
                                rts_playing = true;
                            }
                        } else {
                            let _ = input_hub::take_duke_rts_requests();
                        }
                        // Hear the world from this client's own player.
                        let (
                            view_posx,
                            view_posy,
                            view_posz,
                            view_ang,
                            view_time_before_exit,
                            view_cursectnum,
                        ) = {
                            let p = session
                                .player_for_seat(duke_view_seat(network.as_ref()))
                                .unwrap_or(&session.player);
                            (
                                p.posx,
                                p.posy,
                                p.posz,
                                p.ang,
                                p.time_before_exit,
                                p.cursectnum,
                            )
                        };
                        let listener = [view_posx, view_posy, view_posz];
                        let to_audio_point = |position: [i32; 3]| {
                            room_core::DVec3::new(
                                f64::from(position[0]) / 16.0,
                                f64::from(position[1]) / 16.0,
                                f64::from(position[2]) / 256.0,
                            )
                        };
                        let listener_angle =
                            f64::from(view_ang.rem_euclid(2048)) * (std::f64::consts::TAU / 2048.0);
                        mixer.set_listener(to_audio_point(listener), listener_angle);
                        mixer.set_reverb_level(i32::from(session.duke_reverb_level));
                        let visibility_geometry = session.world.as_ref().map(|world| {
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
                                .collect::<Vec<_>>();
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
                                .collect::<Vec<_>>();
                            (sectors, walls)
                        });
                        for event in sound_events {
                            let sound = room_core::SoundId::from_raw(u32::from(event.sound_id));
                            if event.stop {
                                mixer.apply(&[room_audio::SoundEvent::StopSound { sound }]);
                                continue;
                            }
                            let origin = if event.global {
                                room_audio::SoundOrigin::None
                            } else {
                                room_audio::SoundOrigin::Point(to_audio_point(event.position))
                            };
                            if event.stop_source_sound {
                                mixer.apply(&[room_audio::SoundEvent::Stop {
                                    origin,
                                    channel: i32::from(event.sound_id),
                                }]);
                                continue;
                            }
                            if let Some(cents) = event.pitch_offset {
                                mixer.apply(&[room_audio::SoundEvent::AdjustSoundPitch {
                                    sound,
                                    source_id: event.source_id,
                                    cents,
                                }]);
                                continue;
                            }
                            if view_time_before_exit > 0 && view_time_before_exit <= 26 * 3 {
                                // Chocolate rejects new `xyzsound` starts during
                                // the final three seconds of a timed exit.
                                continue;
                            }
                            let definition = session
                                .script
                                .as_ref()
                                .and_then(|script| script.sounds.get(usize::from(event.sound_id)))
                                .and_then(Option::as_ref);
                            let lockout_only = definition.is_some_and(|sound| sound.m & 8 != 0);
                            let talk_only = definition.is_some_and(|sound| sound.m & 4 != 0);
                            if !audio_options.sound
                                || (audio_options.lockout && lockout_only)
                                || (!audio_options.talk && talk_only)
                                || (!audio_options.ambience && event.ambience)
                            {
                                continue;
                            }
                            let distance_offset = if event.global || event.no_attenuation {
                                0
                            } else {
                                definition.map_or(0, |sound| sound.vo / 16)
                            };
                            let underwater = session
                                .world
                                .as_ref()
                                .and_then(|world| {
                                    world.sectors.get(view_cursectnum.max(0) as usize)
                                })
                                .is_some_and(|sector| sector.lotag == 2);
                            let pitch = room_duke::audio::pitch_scale(
                                session.script.as_ref(),
                                event.sound_id,
                                session.tic,
                                event.source_id,
                                underwater,
                                event.global,
                            );
                            let source_sector = event
                                .source_id
                                .and_then(|source| usize::try_from(source).ok())
                                .and_then(|source| session.world.as_ref()?.sprites.get(source))
                                .map_or(view_cursectnum, |sprite| sprite.sectnum);
                            let line_of_sight = event.global
                                || event.ambience
                                || visibility_geometry.as_ref().is_none_or(|(sectors, walls)| {
                                    !room_build_engine::geom::cansee(
                                        listener[0],
                                        listener[1],
                                        listener[2].wrapping_sub(24 << 8),
                                        view_cursectnum,
                                        event.position[0],
                                        event.position[1],
                                        event.position[2].wrapping_sub(24 << 8),
                                        source_sector,
                                        sectors,
                                        walls,
                                    )
                                });
                            let xyzsound_actor_voice =
                                !event.global && !event.ambience && !event.play_once;
                            if xyzsound_actor_voice {
                                let dead_badguy = event
                                    .source_id
                                    .and_then(|source| usize::try_from(source).ok())
                                    .and_then(|source| {
                                        let actor = session.world.as_ref()?.sprites.get(source)?;
                                        Some(
                                            actor.extra <= 0
                                                && room_duke::badguy_pic(
                                                    actor.picnum,
                                                    session.script.as_ref().map_or(&[], |script| {
                                                        script.actortype.as_slice()
                                                    }),
                                                ),
                                        )
                                    })
                                    .unwrap_or(false);
                                mixer.duke_xyzsound_stop_latest(
                                    sound,
                                    event.source_id,
                                    dead_badguy,
                                );
                            }
                            let flags = (if event.play_once {
                                room_audio::SoundFlags::UNIQUE_SOUND
                            } else {
                                0
                            }) | (if event.loop_sound {
                                room_audio::SoundFlags::LOOP
                            } else {
                                0
                            }) | match event.voice_limit {
                                1 => room_audio::SoundFlags::DUKE_LIMIT_ONE,
                                2..=4 => room_audio::SoundFlags::DUKE_LIMIT_FOUR,
                                _ => 0,
                            } | if event.exclusive {
                                room_audio::SoundFlags::DUKE_EXCLUSIVE
                            } else {
                                0
                            } | if event.global || event.no_attenuation {
                                room_audio::SoundFlags::ATTN_NONE
                            } else {
                                0
                            } | if event.ambience {
                                room_audio::SoundFlags::DUKE_AMBIENCE
                            } else {
                                0
                            } | room_audio::SoundFlags::with_duke_priority(i32::from(
                                event.priority,
                            ))
                            .0 | room_audio::SoundFlags::with_duke_distance_offset(
                                distance_offset,
                            )
                            .0 | if xyzsound_actor_voice {
                                room_audio::SoundFlags::OVERLAP
                            } else {
                                0
                            };
                            mixer.apply(&[room_audio::SoundEvent::Start {
                                origin,
                                channel: i32::from(event.sound_id),
                                sound,
                                check_sound: None,
                                start_time: 0.0,
                                volume: 1.0,
                                attenuation: if line_of_sight { 1.0 } else { 1.03125 },
                                pitch,
                                flags: room_audio::SoundFlags(flags),
                                source_id: event.source_id,
                                no_restart: false,
                            }]);
                        }
                    }
                    audio.feed_output();
                }
                (s.camera.eye, s.camera.forward)
            }
        };
        s.camera.eye = eye;
        s.camera.forward = forward;
        // `SetViewPos(…, VPSF_ORTHOGRAPHIC)`: orthographic projection.
        s.camera.ortho = match &s.mode {
            Mode::Play(play)
            | Mode::Lobby {
                prepared: Some(play),
                ..
            } => play.view_ortho(),
            _ => None,
        };
        {
            let FrameState { gpu, mode, .. } = &mut *s;
            match mode {
                Mode::Play(play)
                | Mode::Lobby {
                    prepared: Some(play),
                    ..
                } => play.refresh_camera_textures(gpu),
                _ => {}
            }
        }

        // One full-frame view, or one viewport per player (spectator grid).
        let fb_w = s.config.width as f32;
        let fb_h = s.config.height as f32;
        let native_fb_width = s.config.width;
        let native_fb_height = s.config.height;
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

        let t_sim = now_ms();
        let Ok(frame) = s.surface.get_current_texture() else {
            return;
        };
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let use_post = s.render_settings.needs_post();
        // Linear scene into post (R12): no gain/encode on top of tonemap.
        // Restore web Bright+gain+encode when post is off (SH0).
        if use_post {
            s.pass.encode_srgb = false;
            s.pass.output_gain = 1.0;
        } else {
            apply_web_lighting(&mut s.pass);
        }
        // Always draw the world into the Rgba8 scene RT (matches Unlit/zone
        // pipelines), then present to the swapchain via post (identity when FX
        // are off). Avoids Bgra filter/sample issues and surface COPY_DST.
        // `vid_rendermode` 0 / 1: the software renderer draws the level into
        // the HUD overlay and the GPU world pass is skipped (one full view only).
        let soft_pixels = if matches!(s.rendermode, 0 | 1) && views.len() == 1 {
            let (hud_w, hud_h) = hud_dims(native_fb_width, native_fb_height);
            let scale = s.softscale;
            let options = crate::play::SoftViewOptions {
                fov: s.soft_fov,
                truecolor: s.rendermode == 1,
                visibility: s.soft_visibility,
            };
            let FrameState {
                mode, soft, camera, ..
            } = &mut *s;
            match mode {
                Mode::Play(play) => play
                    .draw_software(soft, scale, camera, hud_w, hud_h, options)
                    .map(|px| (hud_w, hud_h, px)),
                _ => None,
            }
        } else {
            None
        };
        let scene_color = s.scene_color.take();
        if soft_pixels.is_none() {
            // Pane rects are in canvas pixels; the world targets may be
            // smaller (Settings → 3D scale).
            let kx = s.world_size.0 as f32 / native_fb_width.max(1) as f32;
            let ky = s.world_size.1 as f32 / native_fb_height.max(1) as f32;
            for (i, (cam, seat, rect)) in views.iter().enumerate() {
                let target = &scene_color.as_ref().expect("scene rt").1;
                let rect = rect.map(|[x, y, w, h]| [x * kx, y * ky, w * kx, h * ky]);
                render_world_view(&mut s, target, cam, *seat, rect, i == 0, use_materials);
            }
        }
        s.scene_color = scene_color;
        // Mods' GLDEFS post shaders (Color World, Red Noir): switched and
        // fed here, run below on the finished picture.
        let mut mod_post_on = false;
        {
            let FrameState {
                mode,
                gpu,
                config,
                web_post,
                ..
            } = &mut *s;
            if let Mode::Play(play) = mode {
                let ops = play.take_post_shader_ops();
                if web_post
                    .as_ref()
                    .is_none_or(|p| p.session != play.session_id || p.format != config.format)
                {
                    *web_post = Some(crate::web_post::WebPost::new(
                        &gpu.device,
                        config.format,
                        play.session_id,
                        play.post_shader_sources(),
                    ));
                }
                if let Some(post) = web_post.as_mut() {
                    post.apply(ops);
                    mod_post_on = soft_pixels.is_none() && post.any_enabled();
                }
            }
        }
        let t_world = now_ms();
        {
            let settings = s.render_settings.clone();
            let time = s.pass.time;
            // No post FX: the unlit pass already wrote sRGB (`encode_srgb`).
            let scene_encoded = s.pass.encode_srgb;
            // Texel size of the post sources: the world targets.
            let (w, h) = s.world_size;
            let mut encoder =
                s.gpu
                    .device
                    .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                        label: Some("post"),
                    });
            let mut chain = s.post_chain.take();
            let scene = s.scene_color.as_ref();
            let ping = s.post_ping.as_ref();
            if let (Some(chain), Some(scene), Some(ping)) = (chain.as_mut(), scene, ping) {
                // With a mod's post shader on, the chain's picture goes to
                // `post_ldr` (the window's format, gamma space) first.
                let dst = match s.post_ldr.as_ref() {
                    Some(ldr) if mod_post_on => &ldr.1,
                    _ => &view,
                };
                chain.apply_views(
                    &s.gpu,
                    &mut encoder,
                    &scene.1,
                    &s.depth.0,
                    &ping.1,
                    dst,
                    PostChainInputs {
                        settings: &settings,
                        blend: [0.0, 0.0, 0.0, 0.0],
                        width: w,
                        height: h,
                        time,
                        scene_encoded,
                    },
                );
            }
            s.post_chain = chain;
            s.gpu.queue.submit(Some(encoder.finish()));
        }
        // The weapon joins the picture, the mod shaders run on it into the
        // window, and the rest of the HUD is drawn on top below (as UZDoom:
        // the shaders see the weapon, not the status bar).
        let mut hud_after_post: Option<Canvas> = None;
        if mod_post_on {
            let (native_fb_width, native_fb_height) = hud_dims(native_fb_width, native_fb_height);
            let game = match &s.mode {
                Mode::Play(play) => play.game_seconds(),
                _ => 0.0,
            };
            let canvases = match &mut s.mode {
                Mode::Play(play) => Some(play.build_hud_canvases(
                    native_fb_width as f32,
                    native_fb_height as f32,
                    true,
                )),
                _ => None,
            };
            let time = s.pass.time;
            let FrameState {
                gpu,
                canvas,
                post_ldr,
                web_post,
                ..
            } = &mut *s;
            if let (Some((rest, weapon)), Some(ldr), Some(post)) =
                (canvases, post_ldr.as_ref(), web_post.as_mut())
            {
                if let (Some(weapon), Some(canvas_gpu)) = (weapon, canvas.as_mut()) {
                    canvas_gpu.rasterize_frame(weapon, None);
                    let mut encoder =
                        gpu.device
                            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                                label: Some("weapon-in-scene"),
                            });
                    {
                        let mut rp = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                            label: Some("weapon-in-scene"),
                            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                                view: &ldr.1,
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
                    gpu.queue.submit(Some(encoder.finish()));
                }
                let mut encoder =
                    gpu.device
                        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                            label: Some("mod-post"),
                        });
                post.run_to_view(
                    gpu,
                    &mut encoder,
                    &ldr.0,
                    &view,
                    room_render::PostTimes {
                        delta: 1.0 / 60.0,
                        time,
                        game,
                    },
                );
                gpu.queue.submit(Some(encoder.finish()));
                hud_after_post = Some(rest);
            }
        }
        let mut encoder = s
            .gpu
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("hud") });
        // Weapon / HUD overlay (single colour target; Load the 3D result).
        if matches!(s.mode, Mode::Play(_) | Mode::NativeDuke { .. }) {
            // The overlay is CPU-rasterized and uploaded every frame; keep it
            // at most `HUD_MAX_WIDTH` wide and let the blit quad scale it up.
            let (native_fb_width, native_fb_height) = hud_dims(native_fb_width, native_fb_height);
            let (hud_fw, hud_fh) = (native_fb_width as f32, native_fb_height as f32);
            let hud_canvas = match &mut s.mode {
                Mode::Play(_) if hud_after_post.is_some() => hud_after_post.take(),
                Mode::Play(play) => {
                    let mut hud = play.build_hud_canvas(hud_fw, hud_fh);
                    if let Some((w, h, pixels)) = soft_pixels {
                        // Under everything else of the overlay.
                        let at = hud
                            .cmds
                            .iter()
                            .take_while(|c| matches!(c, room_render::CanvasCmd::Clear { .. }))
                            .count();
                        hud.cmds.insert(
                            at,
                            room_render::CanvasCmd::BlitRgba {
                                x: 0.0,
                                y: 0.0,
                                w,
                                h,
                                pixels,
                            },
                        );
                    }
                    Some(hud)
                }
                Mode::NativeDuke {
                    session,
                    renderer,
                    network,
                    ..
                } => (|| -> Option<Canvas> {
                    let view_seat = duke_view_seat(network.as_ref());
                    let world = match session.render_world_with_eduke_events_viewport(
                        view_seat,
                        native_fb_width as usize,
                        native_fb_height as usize,
                    ) {
                        Ok(Some(world)) => world,
                        Ok(None) => return None,
                        Err(error) => {
                            web_sys::console::warn_1(
                                &format!("Duke CON EVENT_ANIMATESPRITES failed: {error}").into(),
                            );
                            session.render_world()?
                        }
                    };
                    for error in session.take_eduke_runtime_errors() {
                        web_sys::console::error_1(&format!("Duke CON {error}").into());
                    }
                    // Each client looks through its own player (seat 0 is
                    // only the host's view).
                    let view = session
                        .player_for_seat(view_seat)
                        .unwrap_or(&session.player);
                    let pixels = renderer.render_rgba_at_with_quake_view(
                        &world,
                        view,
                        native_fb_width,
                        native_fb_height,
                        session.tic,
                        session.earthquake_time,
                        input_hub::duke_shoulder_view(),
                    );
                    let mut canvas = Canvas::new();
                    canvas.blit_rgba(0.0, 0.0, native_fb_width, native_fb_height, pixels);
                    Some(canvas)
                })(),
                _ => None,
            };
            if let Some(hud) = hud_canvas {
                if let Some(canvas_gpu) = s.canvas.as_mut() {
                    canvas_gpu.rasterize_frame(hud, None);
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
        let t_end = now_ms();
        // The 3D render size (the canvas size times Settings → 3D scale).
        perf_note(
            t_sim - t_start,
            t_world - t_sim,
            t_end - t_world,
            s.world_size.0,
            s.world_size.1,
        );
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
            Mode::Demo { .. } | Mode::Lobby { .. } | Mode::NativeDuke { .. } => Vec::new(),
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
        // Texture filtering / upscaling CVars reach the play materials.
        let (aniso, hq) = (s.render_settings.anisotropy, s.render_settings.hq_resize);
        if let Mode::Play(play) = &mut s.mode {
            play.materials.anisotropy = aniso;
            play.materials.set_hq_resize(hq);
        }
        let mut voxel_uploads = Vec::new();
        let mut particle_uploads = Vec::new();
        let particle_stretch = s.render_settings.particle_fx;
        let mut scene: SceneLists = match &mut s.mode {
            Mode::Demo { map, .. } => build_scene_with_sky(map, cam, None, None),
            Mode::Lobby {
                prepared: Some(play),
                ..
            }
            | Mode::Play(play) => {
                let sizes = play.materials.wall_texel_sizes_shared();
                let sky = play.sky_texture();
                // The player's own view: leave out what solid walls hide
                // (portal views below build their own, unculled).
                let mut lists = build_scene_with_sky_ex_shared(
                    &play.map,
                    cam,
                    Some(sprites.as_slice()),
                    Some(sky.as_str()),
                    None,
                    Some(sizes),
                    true,
                );
                play.animate_textures(&mut lists);
                // Phones: every dynamic light is a per-pixel loop in the
                // world shader; skip them (sector lighting stays).
                if !LOW_POWER.with(Cell::get) {
                    lists.dyn_lights = play.collect_dyn_lights();
                    room_render::apply_dyn_lights(&mut lists);
                }
                voxel_uploads = play.append_voxel_poses(&mut lists);
                play.append_model_poses(&mut lists);
                play.append_particles(&mut lists, cam, particle_stretch);
                particle_uploads = play.particle_texture_uploads();
                lists
            }
            Mode::Lobby { demo, .. } => build_scene_with_sky(demo, cam, None, None),
            Mode::NativeDuke { backdrop, .. } => build_scene_with_sky(backdrop, cam, None, None),
        };
        if !voxel_uploads.is_empty() {
            let FrameState { gpu, mode, .. } = &mut *s;
            if let Mode::Play(play)
            | Mode::Lobby {
                prepared: Some(play),
                ..
            } = mode
            {
                play.upload_voxel_palettes(gpu, &voxel_uploads);
            }
        }
        if !particle_uploads.is_empty() {
            let FrameState { gpu, mode, .. } = &mut *s;
            if let Mode::Play(play)
            | Mode::Lobby {
                prepared: Some(play),
                ..
            } = mode
            {
                play.upload_particle_textures(gpu, &particle_uploads);
            }
        }

        let zone_meshes = match &s.mode {
            Mode::Play(play)
            | Mode::Lobby {
                prepared: Some(play),
                ..
            } => play.game.world().and_then(|world| {
                if world.mod_zones.is_empty() {
                    return None;
                }
                let ticfrac_milli =
                    (play.tic_accum * 1000.0 / (1.0 / 35.0)).clamp(0.0, 1000.0) as u32;
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
                Some(build_zone_meshes(&play.map, &draws))
            }),
            _ => None,
        };
        {
            let world_size = s.world_size;
            let FrameState {
                gpu, pass, mode, ..
            } = &mut *s;
            pass.time += 1.0 / 60.0;
            if use_materials
                && !scene.portal_views.is_empty()
                && let Mode::Play(play) = mode
            {
                let size = match rect {
                    Some([_, _, w, h]) => (w as u32, h as u32),
                    None => world_size,
                };
                play.render_portal_views(
                    gpu,
                    pass,
                    cam,
                    &mut scene,
                    sprites.as_slice(),
                    size,
                    particle_stretch,
                );
            }
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
                zone_pass,
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
            if let Some(meshes) = zone_meshes.as_ref() {
                if !meshes.wall_depth.is_empty()
                    || !meshes.wall_through.is_empty()
                    || !meshes.ring.is_empty()
                {
                    zone_pass.prepare(gpu, cam, pass.time);
                    zone_pass.upload(gpu, meshes);
                    zone_pass.draw(&mut rp, meshes);
                }
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
    fn fit_canvas_buffer(canvas: &HtmlCanvasElement, max_dim: u32) -> (u32, u32) {
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
        let (mut css_w, mut css_h) = if let Some(size) = parent_box {
            size
        } else if vw / vh > MAX_ASPECT {
            (vh * MAX_ASPECT, vh)
        } else if vw / vh < MIN_ASPECT {
            (vw, vw / MIN_ASPECT)
        } else {
            (vw, vh)
        };
        // Editor panes (`data-fit=parent`) keep the layout box; play uses Window size.
        if parent_box.is_none() {
            let scale = WINDOW_SCALE.get().clamp(0.35, 1.0);
            css_w = (css_w * scale).max(160.0);
            css_h = (css_h * scale).max(120.0);
        }
        let style = canvas.style();
        let _ = style.set_property("width", &format!("{css_w:.0}px"));
        let _ = style.set_property("height", &format!("{css_h:.0}px"));
        let mut w = (css_w * dpr).round().clamp(320.0, 3840.0);
        let mut h = (css_h * dpr).round().clamp(240.0, 2160.0);
        let max_dim = f64::from(max_dim.max(1));
        let longest = w.max(h);
        if longest > max_dim {
            let scale = max_dim / longest;
            w = (w * scale).floor().max(1.0);
            h = (h * scale).floor().max(1.0);
        }
        let w = w as u32;
        let h = h as u32;
        if canvas.width() != w {
            canvas.set_width(w);
        }
        if canvas.height() != h {
            canvas.set_height(h);
        }
        (w, h)
    }

    fn render_max_dim(device_max: u32) -> u32 {
        let cap = RENDER_CAP.get();
        if cap == 0 {
            device_max
        } else {
            device_max.min(cap)
        }
    }

    fn sync_surface_size(canvas: &HtmlCanvasElement, s: &mut FrameState) {
        let max_dim = render_max_dim(s.gpu.device.limits().max_texture_dimension_2d);
        let (w, h) = fit_canvas_buffer(canvas, max_dim);
        s.camera.aspect = f64::from(w) / f64::from(h.max(1));
        // Match desktop: ~90° horizontal at 16:9.
        s.camera.fov_y_deg = 58.715;
        let scale = WORLD_SCALE.get().min(s.render_settings.scale_factor);
        let world = room_render::world_render_size(w, h, scale);
        let resized = w != s.config.width || h != s.config.height;
        if !resized && world == s.world_size {
            return;
        }
        let fmt = s.config.format;
        if resized {
            s.config.width = w;
            s.config.height = h;
            s.surface.configure(&s.gpu.device, &s.config);
            s.post_ldr = Some(create_color_rt(&s.gpu.device, fmt, w, h));
            if s.canvas.is_some() {
                let (hud_w, hud_h) = hud_dims(w, h);
                s.canvas = Some(CanvasGpu::new(&s.gpu, fmt, hud_w, hud_h));
            }
        }
        let (ww, wh) = world;
        s.world_size = world;
        s.depth = create_depth(&s.gpu.device, ww, wh);
        s.stencil_aux = create_stencil_aux(&s.gpu.device, ww, wh);
        let scene_format = wgpu::TextureFormat::Rgba8Unorm;
        s.scene_color = Some(create_color_rt(&s.gpu.device, scene_format, ww, wh));
        s.post_ping = Some(create_color_rt(&s.gpu.device, scene_format, ww, wh));
        if let Some(chain) = s.post_chain.as_mut() {
            chain.ensure_formats(&s.gpu, scene_format, fmt);
        }
    }

    /// Width cap for the CPU-rasterized HUD / native Duke canvas.
    const HUD_MAX_WIDTH: u32 = 1280;

    /// HUD canvas size for a `w`×`h` framebuffer (same aspect, ≤ [`HUD_MAX_WIDTH`]).
    fn hud_dims(w: u32, h: u32) -> (u32, u32) {
        if w <= HUD_MAX_WIDTH {
            (w.max(1), h.max(1))
        } else {
            (
                HUD_MAX_WIDTH,
                ((u64::from(h) * u64::from(HUD_MAX_WIDTH) / u64::from(w)) as u32).max(1),
            )
        }
    }

    fn now_ms() -> f64 {
        window()
            .and_then(|w| w.performance())
            .map(|p| p.now())
            .unwrap_or(0.0)
    }

    #[derive(Default)]
    struct FpsMeter {
        window_start_ms: f64,
        frames: u32,
    }

    /// Painted frames over the last half-second, shown in `#fps`.
    fn note_fps(meter: &Rc<RefCell<FpsMeter>>, now_ms: f64) {
        let mut m = meter.borrow_mut();
        if m.window_start_ms == 0.0 {
            m.window_start_ms = now_ms;
        }
        m.frames += 1;
        let elapsed = now_ms - m.window_start_ms;
        if elapsed < 500.0 {
            return;
        }
        let rate = f64::from(m.frames) * 1000.0 / elapsed;
        m.window_start_ms = now_ms;
        m.frames = 0;
        drop(m);
        let Some(win) = window() else {
            return;
        };
        let Some(doc) = win.document() else {
            return;
        };
        if let Some(el) = doc.get_element_by_id("fps") {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let shown = rate.round().clamp(0.0, 999.0) as u32;
            if PERF_ON.get() {
                // Average CPU ms per painted frame. A frame time far above
                // the CPU total means the GPU (or the browser) is the limit.
                let frames = (rate * elapsed / 1000.0).max(1.0);
                let t = PERF_TOTALS.with_borrow_mut(|t| std::mem::replace(t, FramePerf::ZERO));
                let (backend, w, h) = PERF_INFO.with_borrow(Clone::clone);
                let line = format!(
                    "{shown} fps · {:.1} ms/frame · {backend} {w}×{h}\ncpu {:.1} ms: sim {:.1} · world {:.1} · overlay {:.1}",
                    elapsed / frames,
                    (t.sim + t.world + t.overlay) / frames,
                    t.sim / frames,
                    t.world / frames,
                    t.overlay / frames,
                );
                el.set_class_name("perf");
                el.set_text_content(Some(&line));
            } else {
                el.set_class_name("");
                el.set_text_content(Some(&format!("{shown} fps")));
            }
        }
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

    /// Set native Duke audio options matching the desktop F4 controls.
    /// Volumes are Atomic-style 0..=255 values; defaults are enabled/full.
    #[wasm_bindgen(js_name = roomSetDukeAudioOptions)]
    pub fn room_set_duke_audio_options(
        sound: bool,
        music: bool,
        talk: bool,
        lockout: bool,
        ambience: bool,
        reverse_stereo: bool,
        sfx_volume: u8,
        music_volume: u8,
    ) {
        DUKE_AUDIO_OPTIONS.set(DukeAudioOptions {
            sound,
            music,
            talk,
            lockout,
            ambience,
            reverse_stereo,
            sfx_volume,
            music_volume,
        });
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

    /// Apply Settings menu values. `binds_json` is `{ "KeyW": "w", … }` or `""` / `"{}"` for defaults.
    /// `render_cap` is the longest canvas edge in pixels (`0` = native / device max).
    /// `window_scale` is the CSS playfield size vs the fitted viewport (`1.0` = full).
    /// Phone performance mode: no dynamic lights (the page also caps the
    /// render resolution and leaves voxel models off on phones).
    #[wasm_bindgen(js_name = roomSetLowPower)]
    pub fn room_set_low_power(on: bool) {
        LOW_POWER.with(|c| c.set(on));
    }

    /// Settings → Show frame times: CPU milliseconds per stage, the
    /// graphics API and the render size next to the FPS counter.
    #[wasm_bindgen(js_name = roomSetPerfOverlay)]
    pub fn room_set_perf_overlay(on: bool) {
        PERF_ON.set(on);
        if !on {
            PERF_TOTALS.with_borrow_mut(|t| *t = FramePerf::ZERO);
        }
    }

    /// Settings → 3D scale: render the world at `scale` (0.25–1) of the
    /// canvas size and stretch it on screen; the HUD keeps the full size.
    #[wasm_bindgen(js_name = roomSetWorldScale)]
    pub fn room_set_world_scale(scale: f64) {
        #[allow(clippy::cast_possible_truncation)]
        let scale = if scale.is_finite() {
            scale.clamp(0.25, 1.0) as f32
        } else {
            1.0
        };
        WORLD_SCALE.set(scale);
    }

    #[wasm_bindgen(js_name = roomApplySettings)]
    pub fn room_apply_settings(
        mouse_sens: f64,
        invert_y: bool,
        render_cap: u32,
        window_scale: f64,
        binds_json: &str,
    ) -> Result<(), JsValue> {
        input_hub::set_mouse_sens(mouse_sens);
        input_hub::set_mouse_invert_y(invert_y);
        RENDER_CAP.set(render_cap);
        WINDOW_SCALE.set(window_scale.clamp(0.35, 1.0));
        let binds = if binds_json.trim().is_empty() || binds_json.trim() == "{}" {
            std::collections::BTreeMap::new()
        } else {
            serde_json::from_str::<std::collections::BTreeMap<String, String>>(binds_json)
                .map_err(|e| JsValue::from_str(&format!("binds json: {e}")))?
        };
        input_hub::set_binds(binds);
        // Resize immediately when the surface exists.
        if let Some(win) = window() {
            if let Some(doc) = win.document() {
                if let Some(el) = doc.get_element_by_id("room") {
                    if let Ok(canvas) = el.dyn_into::<HtmlCanvasElement>() {
                        APP.with(|app| {
                            if let Some(shared) = app.borrow().as_ref() {
                                if let Ok(mut s) = shared.try_borrow_mut() {
                                    sync_surface_size(&canvas, &mut s);
                                }
                            }
                        });
                    }
                }
            }
        }
        Ok(())
    }

    /// Toggle a renderer / post CVar (`docs/29-shaders.md`).
    ///
    /// Examples: `roomSetShaderCvar("gl_bloom", "1")`, `roomSetShaderCvar("r_quality", "2")`.
    /// Returns `false` if the name is unknown. Desktop post-chain wiring applies when
    /// `needs_post()`; web applies settings for subsequent frames (scene path uses
    /// `encode_srgb` / RoomSoft when a post effect is requested).
    #[wasm_bindgen(js_name = roomSetShaderCvar)]
    pub fn room_set_shader_cvar(name: &str, value: &str) -> bool {
        APP.with(|app| {
            let Some(shared) = app.borrow().as_ref().cloned() else {
                return false;
            };
            let Ok(mut s) = shared.try_borrow_mut() else {
                return false;
            };
            // Renderer choice (`docs/40` SR3): not a shader setting.
            if name.eq_ignore_ascii_case("vid_rendermode") {
                return match value.trim().parse::<i32>() {
                    Ok(v) => {
                        s.rendermode = v;
                        if !matches!(v, 0 | 1) {
                            s.soft = None;
                        }
                        true
                    }
                    Err(_) => false,
                };
            }
            if name.eq_ignore_ascii_case("fov") || name.eq_ignore_ascii_case("r_visibility") {
                return match value.trim().parse::<f64>() {
                    Ok(v) if v.is_finite() => {
                        if name.eq_ignore_ascii_case("fov") {
                            s.soft_fov = v.clamp(30.0, 120.0);
                        } else {
                            s.soft_visibility = v.clamp(0.5, 128.0);
                        }
                        true
                    }
                    _ => false,
                };
            }
            if name.eq_ignore_ascii_case("vid_softscale") {
                return match value.trim().parse::<u32>() {
                    Ok(v) => {
                        s.softscale = v.clamp(1, 4);
                        true
                    }
                    Err(_) => false,
                };
            }
            let ok = if name.eq_ignore_ascii_case("r_quality") {
                if let Ok(level) = value.parse::<i32>() {
                    s.render_settings.apply_quality(level.clamp(0, 3) as u8);
                    true
                } else {
                    false
                }
            } else {
                let lower = value.trim().to_ascii_lowercase();
                let b = matches!(lower.as_str(), "1" | "true" | "yes" | "on");
                let i = value.parse::<i32>().ok();
                let f = value.parse::<f64>().ok();
                s.render_settings.sync_cvar(
                    name,
                    if i.is_none() && f.is_none() {
                        Some(b)
                    } else {
                        None
                    },
                    i,
                    f,
                )
            };
            if ok {
                if s.render_settings.needs_post() {
                    s.pass.encode_srgb = false;
                    s.pass.output_gain = 1.0;
                } else {
                    apply_web_lighting(&mut s.pass);
                }
            }
            ok
        })
    }

    /// Default key bindings as JSON (`KeyboardEvent.code` → action).
    #[wasm_bindgen(js_name = roomDefaultBindsJson)]
    #[must_use]
    pub fn room_default_binds_json() -> String {
        serde_json::to_string(&input_hub::default_binds()).unwrap_or_else(|_| "{}".into())
    }

    /// Current effective binds as JSON.
    #[wasm_bindgen(js_name = roomBindsJson)]
    #[must_use]
    pub fn room_binds_json() -> String {
        serde_json::to_string(&input_hub::effective_binds()).unwrap_or_else(|_| "{}".into())
    }

    /// On-screen controls: look by whole mouse counts (≈1.4° each; +x right, +y down).
    #[wasm_bindgen(js_name = roomInputLook)]
    pub fn room_input_look(dx: f64, dy: f64) {
        input_hub::add_mouse_delta(dx, dy);
    }

    /// On-screen controls: analog stick (`x` right, `y` forward, each `-1..=1`;
    /// `0, 0` = released). Solo Doom moves at that fraction of run speed;
    /// netplay and Duke keep using the keys the stick also presses.
    #[wasm_bindgen(js_name = roomInputMove)]
    pub fn room_input_move(x: f64, y: f64) {
        input_hub::set_analog_move(x, y);
    }

    /// On-screen controls: switch weapon (`1` = next, `-1` = previous).
    #[wasm_bindgen(js_name = roomInputWeaponCycle)]
    pub fn room_input_weapon_cycle(steps: i32) {
        input_hub::add_weapon_wheel(-f64::from(steps));
    }

    /// On-screen controls: true while a native Duke session owns the input
    /// (shows the Duke-only KICK / ITEM buttons).
    #[wasm_bindgen(js_name = roomInputDukeMode)]
    #[must_use]
    pub fn room_input_duke_mode() -> bool {
        input_hub::duke_mode()
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
                Mode::NativeDuke { session, .. } => format!(
                    "mode=duke tic={} weapon={} hp={} held={held}",
                    session.tic, session.player.curr_weapon, session.player.extra
                ),
                Mode::Lobby { lobby, .. } => format!("mode=lobby {} held={held}", lobby.status()),
                Mode::Play(play) => {
                    let net = if let Some(n) = play.net_debug() {
                        format!(" net={}", n)
                    } else {
                        " net=solo".into()
                    };
                    let mods = play.script_mod_names();
                    let mods = if mods.is_empty() {
                        String::new()
                    } else {
                        format!(" mods={}", mods.join("+"))
                    };
                    format!(
                        "mode=play seat={} held={}{}{} pos={}",
                        play.local_seat_debug(),
                        held,
                        net,
                        mods,
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

    /// The Settings panel opened (`true`) or closed (`false`): a solo game
    /// (Doom-engine or native Duke) pauses while it is open. Netplay keeps
    /// running, since the other players' game cannot wait.
    #[wasm_bindgen(js_name = roomSetMenuPause)]
    pub fn room_set_menu_pause(on: bool) {
        crate::play::MENU_PAUSE.with(|c| c.set(on));
    }

    /// Settings → Network → Rollback: run the game up to `window` tics
    /// ahead of the confirmed online game with guessed input for the other
    /// players (`0` = off, max 16). Takes effect at once.
    #[wasm_bindgen(js_name = roomSetRollback)]
    pub fn room_set_rollback(window: u32) {
        crate::play::ROLLBACK_WINDOW.with(|c| c.set(window.min(16)));
    }

    /// Settings → Network → Prediction: draw your own movement ahead of the
    /// server (on by default; `?predict=0` turns it off for one visit).
    #[wasm_bindgen(js_name = roomSetPredict)]
    pub fn room_set_predict(on: bool) {
        crate::play::PREDICT_SETTING.with(|c| c.set(Some(on)));
    }

    /// The page draws in-game messages (as toasts) and the Tab stats
    /// itself; the canvas versions stop. Off = canvas again.
    #[wasm_bindgen(js_name = roomSetHtmlOverlays)]
    pub fn room_set_html_overlays(on: bool) {
        crate::play::HTML_OVERLAYS.with(|f| f.set(on));
    }

    /// Message kinds to hide, as a JSON array (`["pickup", "frag"]`).
    #[wasm_bindgen(js_name = roomSetHiddenMessages)]
    pub fn room_set_hidden_messages(kinds_json: &str) {
        let kinds: Vec<String> = serde_json::from_str(kinds_json).unwrap_or_default();
        crate::play::MSG_HIDDEN.with(|h| *h.borrow_mut() = kinds.into_iter().collect());
    }

    /// Messages since the last call (`[[kind, text], …]` JSON).
    #[wasm_bindgen(js_name = roomTakeMessages)]
    #[must_use]
    pub fn room_take_messages() -> String {
        APP.with(|a| {
            let Some(app) = a.borrow().as_ref().cloned() else {
                return "[]".into();
            };
            let Ok(mut s) = app.try_borrow_mut() else {
                return "[]".into();
            };
            match &mut s.mode {
                Mode::Play(play) => play.take_toasts_json(),
                _ => "[]".into(),
            }
        })
    }

    /// Level / player stats for the Tab screen (JSON, `null` outside a level).
    #[wasm_bindgen(js_name = roomStats)]
    #[must_use]
    pub fn room_stats() -> String {
        APP.with(|a| {
            let Some(app) = a.borrow().as_ref().cloned() else {
                return "null".into();
            };
            let Ok(s) = app.try_borrow() else {
                return "null".into();
            };
            match &s.mode {
                Mode::Play(play) => play.stats_json(),
                _ => "null".into(),
            }
        })
    }

    /// The loaded mods' settings (CVARINFO, labelled by their MENUDEF) as
    /// JSON with current values; `[]` when no game or no mod CVars.
    #[wasm_bindgen(js_name = roomModOptions)]
    #[must_use]
    pub fn room_mod_options() -> String {
        APP.with(|a| {
            let Some(app) = a.borrow().as_ref().cloned() else {
                return "[]".into();
            };
            let Ok(s) = app.try_borrow() else {
                return "[]".into();
            };
            match &s.mode {
                Mode::Play(play) => play.mod_options_json(),
                _ => "[]".into(),
            }
        })
    }

    /// Choose a mod CVar value: kept for later games and applied to the
    /// running one at once. `value` empty = back to the mod default.
    #[wasm_bindgen(js_name = roomSetModCvar)]
    pub fn room_set_mod_cvar(name: &str, value: &str) {
        let key = name.trim().to_ascii_lowercase();
        if key.is_empty() {
            return;
        }
        crate::play::MOD_CVAR_OVERRIDES.with(|o| {
            let mut o = o.borrow_mut();
            if value.is_empty() {
                o.remove(&key);
            } else {
                o.insert(key.clone(), value.to_owned());
            }
        });
        if value.is_empty() {
            return;
        }
        APP.with(|a| {
            let Some(app) = a.borrow().as_ref().cloned() else {
                return;
            };
            let Ok(mut s) = app.try_borrow_mut() else {
                return;
            };
            if let Mode::Play(play) = &mut s.mode {
                play.set_mod_cvar(name, value);
            }
        });
    }

    /// Minimap linedefs (6 floats each: `x1, y1, x2, y2, kind, flags`), empty
    /// outside a Doom-engine level. See `Play::minimap_lines`.
    #[wasm_bindgen(js_name = roomMinimapLines)]
    #[must_use]
    pub fn room_minimap_lines() -> Vec<f32> {
        APP.with(|a| {
            let Some(app) = a.borrow().as_ref().cloned() else {
                return Vec::new();
            };
            let Ok(s) = app.try_borrow() else {
                return Vec::new();
            };
            match &s.mode {
                Mode::Play(play) => play.minimap_lines(),
                _ => Vec::new(),
            }
        })
    }

    /// Minimap pose (`x, y, yaw_deg, allmap, n`, then `n` × `x, y, yaw, seat`).
    #[wasm_bindgen(js_name = roomMinimapState)]
    #[must_use]
    pub fn room_minimap_state() -> Vec<f32> {
        APP.with(|a| {
            let Some(app) = a.borrow().as_ref().cloned() else {
                return Vec::new();
            };
            let Ok(s) = app.try_borrow() else {
                return Vec::new();
            };
            match &s.mode {
                Mode::Play(play) => play.minimap_state(),
                _ => Vec::new(),
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
                Mode::NativeDuke { .. } => JsValue::NULL,
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
