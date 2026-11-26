//! Headless screenshots (SR10): a map seen from its player 1 start (or any
//! eye), drawn by the software renderer and encoded as PNG, with no GPU.
//! `room soft-view` and `room-server`'s preset thumbnails use it.

use crate::{SoftFrame, SoftRenderer, SoftTarget, SoftView, map_things};
use room_map::MapGeometry;
use room_vfs::Vfs;

/// What to draw and how big.
#[derive(Clone, Debug)]
pub struct ShotOptions {
    /// Frame size (the software frame; see `display` for the PNG).
    pub width: u32,
    /// Frame height.
    pub height: u32,
    /// Eye (map units); `None` = player 1 start (or the map's middle).
    pub eye: Option<(f64, f64)>,
    /// Eye height (default: floor + 41).
    pub z: Option<f64>,
    /// Facing in degrees (default: the start's angle).
    pub angle: Option<f64>,
    /// Sky texture (default: vanilla's for the map name).
    pub sky: Option<String>,
    /// Draw the map's things (their spawn sprites).
    pub things: bool,
    /// True-colour drawers.
    pub truecolor: bool,
    /// Modern view with this field of view (`None` = vanilla's tables).
    pub fov: Option<f64>,
    /// PNG size with Doom's tall pixels and black bars (`None` = the frame
    /// as drawn).
    pub display: Option<(u32, u32)>,
}

impl Default for ShotOptions {
    fn default() -> Self {
        Self {
            width: 320,
            height: 200,
            eye: None,
            z: None,
            angle: None,
            sky: None,
            things: true,
            truecolor: false,
            fov: None,
            display: None,
        }
    }
}

/// Vanilla's sky for a map name: SKY1–SKY4 by episode, Doom II by map range.
#[must_use]
pub fn default_sky(map: &str) -> String {
    let up = map.to_ascii_uppercase();
    let b = up.as_bytes();
    if b.len() >= 4 && b[0] == b'E' && b[2] == b'M' && b[1].is_ascii_digit() {
        return format!("SKY{}", (b[1] - b'0').clamp(1, 4));
    }
    match up.strip_prefix("MAP").and_then(|n| n.parse::<u32>().ok()) {
        Some(n) if n >= 21 => "SKY3".into(),
        Some(n) if n >= 12 => "SKY2".into(),
        _ => "SKY1".into(),
    }
}

/// A drawn shot: the frame and the view (palette, level) it came from.
pub struct Shot {
    /// The frame.
    pub target: SoftTarget,
    /// The view (for its palette and level).
    pub view: SoftView,
}

impl Shot {
    /// RGBA8 of the frame (`display` size when set).
    #[must_use]
    pub fn rgba(&self, display: Option<(u32, u32)>) -> (u32, u32, Vec<u8>) {
        match display {
            Some((w, h)) => (w, h, self.target.to_rgba_fit(&self.view.data.palette, w, h)),
            None => (
                self.target.width,
                self.target.height,
                self.target.to_rgba(&self.view.data.palette),
            ),
        }
    }
}

/// Draw `map` (already loaded, with nodes) as `opts` says. `None` when the
/// map has no BSP nodes.
#[must_use]
pub fn shoot(vfs: &Vfs, map_name: &str, map: &MapGeometry, opts: &ShotOptions) -> Option<Shot> {
    let start = map.things.iter().find(|t| t.type_id == 1);
    let middle = || {
        let (mut lo, mut hi) = ((f64::MAX, f64::MAX), (f64::MIN, f64::MIN));
        for v in &map.vertices {
            lo = (lo.0.min(v.pos.x()), lo.1.min(v.pos.y()));
            hi = (hi.0.max(v.pos.x()), hi.1.max(v.pos.y()));
        }
        ((lo.0 + hi.0) / 2.0, (lo.1 + hi.1) / 2.0)
    };
    let (x, y) = opts
        .eye
        .or_else(|| start.map(|t| (f64::from(t.x), f64::from(t.y))))
        .unwrap_or_else(middle);
    let angle = opts
        .angle
        .or_else(|| start.map(|t| f64::from(t.angle)))
        .unwrap_or(0.0);
    let mut view = SoftView::new(vfs, map, opts.width, opts.height)?;
    if let Some(fov) = opts.fov {
        view.renderer = SoftRenderer::new_view(opts.width, opts.height, fov);
    }
    view.renderer.truecolor = opts.truecolor;
    let fx = |v: f64| (v * 65536.0).round() as i32;
    let floor = view
        .level
        .point_in_sector(fx(x), fx(y))
        .and_then(|s| view.level.sectors.get(s))
        .map_or(0, |s| s.floor);
    let z = opts.z.map_or(floor + (41 << 16), fx);
    let sky_name = opts.sky.clone().unwrap_or_else(|| default_sky(map_name));
    let sky = view.sky(vfs, &sky_name);
    let mut frame = SoftFrame {
        x: fx(x),
        y: fx(y),
        z,
        angle: ((angle.rem_euclid(360.0) / 360.0) * 4_294_967_296.0) as u64 as u32,
        sky,
        ..SoftFrame::default()
    };
    if opts.things {
        frame.things = map_things(map, &view.level);
    }
    view.prepare(vfs, &frame);
    let target = view.render(&frame);
    Some(Shot { target, view })
}

/// Load `map_name` from `vfs`, build nodes if needed, and draw it.
///
/// # Errors
/// When the map does not load or has no nodes.
pub fn shoot_map(vfs: &Vfs, map_name: &str, opts: &ShotOptions) -> Result<Shot, String> {
    let mut map = room_map::load_map(vfs, map_name).map_err(|e| format!("{map_name}: {e}"))?;
    room_nodebuild::ensure_nodes(&mut map);
    shoot(vfs, map_name, &map, opts).ok_or_else(|| format!("{map_name}: no BSP nodes"))
}

/// PNG of RGBA8 pixels.
///
/// # Errors
/// When encoding fails (wrong buffer size).
pub fn encode_png(width: u32, height: u32, rgba: &[u8]) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut out, width, height);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        enc.write_header()
            .and_then(|mut w| w.write_image_data(rgba))
            .map_err(|e| e.to_string())?;
    }
    Ok(out)
}

/// A map's thumbnail PNG (player 1 start, things drawn, `width` ×
/// `height` with Doom's tall pixels; the frame is drawn at 320×200 times
/// the smallest scale that covers it).
///
/// # Errors
/// When the map does not load, has no nodes, or encoding fails.
pub fn map_thumbnail(
    vfs: &Vfs,
    map_name: &str,
    width: u32,
    height: u32,
) -> Result<Vec<u8>, String> {
    let scale = (width.div_ceil(320)).max(height.div_ceil(240)).clamp(1, 4);
    let opts = ShotOptions {
        width: 320 * scale,
        height: 200 * scale,
        display: Some((width, height)),
        ..ShotOptions::default()
    };
    let shot = shoot_map(vfs, map_name, &opts)?;
    let (w, h, rgba) = shot.rgba(opts.display);
    encode_png(w, h, &rgba)
}
