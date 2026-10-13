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
pub use demo_map::load_embedded_demo_map;

#[cfg(all(feature = "browser", target_arch = "wasm32"))]
mod browser {
    use crate::load_embedded_demo_map;
    use ferrite_nodebuild::apply_nodes;
    use ferrite_render::{Camera, GpuContext, SceneLists, UnlitPass, build_scene};
    use glam::Vec3;
    use std::cell::RefCell;
    use std::rc::Rc;
    use wasm_bindgen::JsCast;
    use wasm_bindgen::prelude::*;
    use web_sys::{HtmlCanvasElement, window};

    struct FrameState {
        surface: wgpu::Surface<'static>,
        gpu: GpuContext,
        config: wgpu::SurfaceConfiguration,
        pass: UnlitPass,
        depth: (wgpu::Texture, wgpu::TextureView),
        map: ferrite_map::MapGeometry,
        camera: Camera,
        start_ms: f64,
    }

    type RafCallback = Rc<RefCell<Option<Closure<dyn FnMut(f64)>>>>;

    /// Start: locate `#ferrite` and run a wgpu map-viewer rAF loop.
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
        let format = caps
            .formats
            .iter()
            .copied()
            .find(wgpu::TextureFormat::is_srgb)
            .or_else(|| caps.formats.first().copied())
            .ok_or_else(|| "no surface formats".to_owned())?;
        let alpha = caps
            .alpha_modes
            .first()
            .copied()
            .unwrap_or(wgpu::CompositeAlphaMode::Auto);
        let width = canvas.width().max(1);
        let height = canvas.height().max(1);
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
        let pass = UnlitPass::new(&gpu, format);
        let depth = create_depth(&gpu.device, width, height);
        let mut map = load_embedded_demo_map()?;
        apply_nodes(&mut map);
        let mut camera = Camera {
            eye: Vec3::new(128.0, 64.0, 41.0),
            forward: Vec3::new(0.0, 1.0, 0.0),
            ..Camera::default()
        };
        camera.aspect = f64::from(width) / f64::from(height.max(1));
        let start_ms = window()
            .and_then(|w| w.performance())
            .map(|p| p.now())
            .unwrap_or(0.0);
        let state = Rc::new(RefCell::new(FrameState {
            surface,
            gpu,
            config,
            pass,
            depth,
            map,
            camera,
            start_ms,
        }));
        web_sys::console::log_1(
            &format!(
                "ferrite-web {}: map viewer (embedded UDMF room; drop IWAD TBD)",
                crate::version()
            )
            .into(),
        );
        request_frame(state);
        Ok(())
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

    fn request_frame(state: Rc<RefCell<FrameState>>) {
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

    fn draw_frame(state: &Rc<RefCell<FrameState>>) {
        let mut s = state.borrow_mut();
        let now = window()
            .and_then(|w| w.performance())
            .map(|p| p.now())
            .unwrap_or(s.start_ms);
        #[allow(clippy::cast_possible_truncation)]
        let t = (now - s.start_ms) as f32 * 0.001;
        let cx = 128.0_f32;
        let cy = 128.0_f32;
        let radius = 140.0_f32;
        let yaw = t * 0.35;
        let eye_x = cx + libm::cosf(yaw) * radius;
        let eye_y = cy + libm::sinf(yaw) * radius;
        s.camera.eye = Vec3::new(eye_x, eye_y, 48.0);
        let fx = cx - eye_x;
        let fy = cy - eye_y;
        let flen = libm::sqrtf(fx * fx + fy * fy).max(1e-3);
        s.camera.forward = Vec3::new(fx / flen, fy / flen, -0.15).normalize();
        s.camera.aspect = f64::from(s.config.width) / f64::from(s.config.height.max(1));

        let scene: SceneLists = build_scene(&s.map, &s.camera, None);
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
            let mut rp = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("map"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
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
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &s.depth.1,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            s.pass.draw(&mut rp, &scene);
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
    ///
    /// Named `Opfs` for the M13 embed API; browsers without durable storage still
    /// get a working key/value path. Future: swap to OPFS when `web-sys` features land.
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
        Ok(storage.get_item(&format!("ferrite/{key}"))?)
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
        // Minimal `application/x-www-form-urlencoded` decode.
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
