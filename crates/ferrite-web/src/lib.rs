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

#[cfg(all(feature = "browser", target_arch = "wasm32"))]
mod play;

#[cfg(all(feature = "browser", target_arch = "wasm32"))]
mod web_audio;

#[cfg(feature = "browser")]
pub use demo_map::load_embedded_demo_map;

#[cfg(all(feature = "browser", target_arch = "wasm32"))]
mod browser {
    use crate::load_embedded_demo_map;
    use crate::play::PlaySession;
    use ferrite_nodebuild::apply_nodes;
    use ferrite_render::{
        Camera, CanvasGpu, GpuContext, LightMode, SceneLists, UnlitPass, build_scene_with_sky,
        build_scene_with_sky_ex_sized,
    };
    use glam::Vec3;
    use std::cell::RefCell;
    use std::rc::Rc;
    use wasm_bindgen::JsCast;
    use wasm_bindgen::prelude::*;
    use web_sys::{HtmlCanvasElement, window};

    enum Mode {
        Demo {
            map: Box<ferrite_map::MapGeometry>,
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
    }

    /// Start: locate `#ferrite` and run a wgpu rAF loop (demo until IWAD loads).
    #[wasm_bindgen(start)]
    pub fn start() {
        console_error_panic_hook::set_once();
        wasm_bindgen_futures::spawn_local(async {
            match init_loop().await {
                Ok(()) => {}
                Err(e) => web_sys::console::error_1(&format!("ferrite-web: {e}").into()),
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
                "ferrite-web: format={format:?} encode_srgb={} {}×{}",
                state.borrow().pass.encode_srgb,
                width,
                height
            )
            .into(),
        );
        APP.with(|a| {
            *a.borrow_mut() = Some(state.clone());
        });
        set_status("Ferrite web — demo map (load an IWAD to play Doom)");
        web_sys::console::log_1(
            &format!(
                "ferrite-web {}: demo viewer; call ferriteStartIwad(bytes) to play",
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
            if let Ok(ev) = web_sys::Event::new("ferrite-ready") {
                let _ = win.dispatch_event(&ev);
            }
        }
        request_frame(state);
        Ok(())
    }

    /// Start (or restart) a Doom/Freedoom play session from IWAD bytes.
    #[wasm_bindgen(js_name = ferriteStartIwad)]
    pub fn ferrite_start_iwad(data: &[u8]) -> Result<(), JsValue> {
        let map = ferrite_url_param("map");
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
        set_status("Loading IWAD…");
        let mut play =
            PlaySession::start(data, map).map_err(|e| JsValue::from_str(&e))?;
        {
            let mut s = state.borrow_mut();
            play.preload_materials(&s.gpu.device, &s.gpu.queue);
            play.last_ms = now_ms();
            play.apply_camera(&mut s.camera);
            apply_web_lighting(&mut s.pass);
            // Match desktop FOV; aspect = real buffer.
            s.camera.fov_y_deg = 58.715;
            s.camera.aspect =
                f64::from(s.config.width) / f64::from(s.config.height.max(1));
            web_sys::console::log_1(
                &format!(
                    "ferrite-web: {}×{} {:?} mode={:?} gain={} encode_srgb={} aspect={:.3}",
                    s.config.width,
                    s.config.height,
                    s.config.format,
                    s.pass.light_mode,
                    s.pass.output_gain,
                    s.pass.encode_srgb,
                    s.camera.aspect
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
            // STBAR owns the chrome; keep the HTML tip out of the way.
            hide_status_overlay();
        }
        web_sys::console::log_1(&"ferrite-web: play session started".into());
        Ok(())
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
            let state_kd = state.clone();
            let kd = Closure::wrap(Box::new(move |ev: web_sys::KeyboardEvent| {
                let code = ev.code();
                if matches!(
                    code.as_str(),
                    "Space" | "ArrowUp" | "ArrowDown" | "ArrowLeft" | "ArrowRight"
                ) {
                    ev.prevent_default();
                }
                if let Mode::Play(play) = &mut state_kd.borrow_mut().mode {
                    play.ensure_audio();
                    play.set_key(&code, true);
                }
            }) as Box<dyn FnMut(_)>);
            let _ = win.add_event_listener_with_callback("keydown", kd.as_ref().unchecked_ref());
            kd.forget();
        }
        {
            let state_ku = state.clone();
            let ku = Closure::wrap(Box::new(move |ev: web_sys::KeyboardEvent| {
                if let Mode::Play(play) = &mut state_ku.borrow_mut().mode {
                    play.set_key(&ev.code(), false);
                }
            }) as Box<dyn FnMut(_)>);
            let _ = win.add_event_listener_with_callback("keyup", ku.as_ref().unchecked_ref());
            ku.forget();
        }
        {
            let state_mm = state.clone();
            let mm = Closure::wrap(Box::new(move |ev: web_sys::MouseEvent| {
                let locked = doc
                    .pointer_lock_element()
                    .is_some_and(|el| el.id() == "ferrite");
                if !locked {
                    return;
                }
                if let Mode::Play(play) = &mut state_mm.borrow_mut().mode {
                    play.add_mouse_delta(ev.movement_x() as f64, ev.movement_y() as f64);
                }
            }) as Box<dyn FnMut(_)>);
            let _ = win.add_event_listener_with_callback("mousemove", mm.as_ref().unchecked_ref());
            mm.forget();
        }
        {
            let state_md = state.clone();
            let md = Closure::wrap(Box::new(move |ev: web_sys::MouseEvent| {
                if let Mode::Play(play) = &mut state_md.borrow_mut().mode {
                    play.set_mouse_button(ev.button(), true);
                }
            }) as Box<dyn FnMut(_)>);
            let _ = canvas.add_event_listener_with_callback("mousedown", md.as_ref().unchecked_ref());
            md.forget();
        }
        {
            let state_mu = state.clone();
            let mu = Closure::wrap(Box::new(move |ev: web_sys::MouseEvent| {
                if let Mode::Play(play) = &mut state_mu.borrow_mut().mode {
                    play.set_mouse_button(ev.button(), false);
                }
            }) as Box<dyn FnMut(_)>);
            let _ = win.add_event_listener_with_callback("mouseup", mu.as_ref().unchecked_ref());
            mu.forget();
        }
        {
            let state_click = state.clone();
            let click = Closure::wrap(Box::new(move |_ev: web_sys::MouseEvent| {
                if let Mode::Play(play) = &mut state_click.borrow_mut().mode {
                    play.ensure_audio();
                }
                let Ok(canvas) = find_canvas() else {
                    return;
                };
                let el: web_sys::HtmlElement = canvas.unchecked_into();
                el.request_pointer_lock();
            }) as Box<dyn FnMut(_)>);
            let _ = canvas.add_event_listener_with_callback("click", click.as_ref().unchecked_ref());
            click.forget();
        }
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
        let mut s = state.borrow_mut();
        // Keep GPU buffer + CSS box + camera.aspect in lockstep (never mismatch).
        if let Ok(canvas_el) = find_canvas() {
            sync_surface_size(&canvas_el, &mut s);
        }
        // Re-assert web lighting each frame.
        apply_web_lighting(&mut s.pass);
        let now = now_ms();

        // Advance sim / orbit, update camera, then build scene.
        let use_materials = matches!(s.mode, Mode::Play(_));
        let (eye, forward) = match &mut s.mode {
            Mode::Demo { start_ms, .. } => {
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
                play.tick_play(dt);
                play.view_pose()
                    .unwrap_or((s.camera.eye, s.camera.forward))
            }
        };
        s.camera.eye = eye;
        s.camera.forward = forward;

        let cam = s.camera;
        let sprites = match &mut s.mode {
            Mode::Play(play) => play.collect_live_sprites(&cam),
            Mode::Demo { .. } => Vec::new(),
        };
        let scene: SceneLists = match &s.mode {
            Mode::Demo { map, .. } => build_scene_with_sky(map, &s.camera, None, None),
            Mode::Play(play) => {
                let sizes = play.materials.wall_texel_sizes();
                let mut lists = build_scene_with_sky_ex_sized(
                    &play.map,
                    &s.camera,
                    Some(sprites.as_slice()),
                    Some("SKY1"),
                    None,
                    Some(&sizes),
                );
                lists.dyn_lights = play.collect_dyn_lights();
                ferrite_render::apply_dyn_lights(&mut lists);
                lists
            }
        };

        {
            let FrameState {
                gpu, pass, camera, ..
            } = &mut *s;
            pass.prepare(gpu, camera, &scene);
        }

        let Ok(frame) = s.surface.get_current_texture() else {
            return;
        };
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
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
            let mut rp = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("map"),
                color_attachments: &[
                    Some(wgpu::RenderPassColorAttachment {
                        view: &view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color {
                                r: 0.02,
                                g: 0.02,
                                b: 0.03,
                                a: 1.0,
                            }),
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
            if use_materials {
                if let Mode::Play(play) = mode {
                    pass.draw_materials(gpu, &mut rp, &scene, &mut play.materials, &play.vfs);
                }
            } else {
                pass.draw(&mut rp, &scene);
            }
        }
        // Weapon / HUD overlay (single colour target; Load the 3D result).
        if matches!(s.mode, Mode::Play(_)) {
            let fb_w = s.config.width as f32;
            let fb_h = s.config.height as f32;
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
                    let FrameState {
                        gpu, canvas, ..
                    } = &mut *s;
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

    fn find_canvas() -> Result<HtmlCanvasElement, String> {
        let win = window().ok_or_else(|| "no window".to_owned())?;
        let doc = win.document().ok_or_else(|| "no document".to_owned())?;
        let el = doc
            .get_element_by_id("ferrite")
            .ok_or_else(|| "missing #ferrite canvas".to_owned())?;
        el.dyn_into::<HtmlCanvasElement>()
            .map_err(|_| "#ferrite is not a canvas".to_owned())
    }

    /// Web lighting: Bright + gain, and shader linear→sRGB encode for the
    /// non-sRGB WebGPU canvas (native windows use a hardware-sRGB swapchain).
    fn apply_web_lighting(pass: &mut UnlitPass) {
        pass.light_mode = LightMode::Bright;
        pass.glob_vis = 512.0;
        pass.output_gain = 1.85;
        pass.encode_srgb = true;
    }

    /// Largest ≤16:9 playfield in the window → CSS + GPU buffer.
    ///
    /// Caps ultrawide panels so FOV matches desktop (~90° H at 16:9). Buffer and
    /// CSS share the same aspect; `camera.aspect = w/h`.
    fn fit_canvas_buffer(canvas: &HtmlCanvasElement) -> (u32, u32) {
        const MAX_ASPECT: f64 = 16.0 / 9.0;
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
        let (css_w, css_h) = if vw / vh > MAX_ASPECT {
            (vh * MAX_ASPECT, vh)
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
    #[wasm_bindgen(js_name = ferriteVersion)]
    #[must_use]
    pub fn ferrite_version() -> String {
        crate::version().to_owned()
    }

    /// Read a URL query parameter (`?map=MAP01&iwad=…`).
    #[wasm_bindgen(js_name = ferriteUrlParam)]
    #[must_use]
    pub fn ferrite_url_param(name: &str) -> Option<String> {
        let win = window()?;
        let search = win.location().search().ok()?;
        parse_query_param(&search, name)
    }

    /// Persist a small string for the embed host (`localStorage` key `ferrite/<key>`).
    #[wasm_bindgen(js_name = ferriteOpfsSet)]
    pub fn ferrite_opfs_set(key: &str, value: &str) -> Result<(), JsValue> {
        let win = window().ok_or_else(|| JsValue::from_str("no window"))?;
        let storage = win
            .local_storage()?
            .ok_or_else(|| JsValue::from_str("no localStorage"))?;
        storage.set_item(&format!("ferrite/{key}"), value)?;
        Ok(())
    }

    /// Read a string previously stored with [`ferrite_opfs_set`].
    #[wasm_bindgen(js_name = ferriteOpfsGet)]
    pub fn ferrite_opfs_get(key: &str) -> Result<Option<String>, JsValue> {
        let win = window().ok_or_else(|| JsValue::from_str("no window"))?;
        let storage = win
            .local_storage()?
            .ok_or_else(|| JsValue::from_str("no localStorage"))?;
        storage.get_item(&format!("ferrite/{key}"))
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
