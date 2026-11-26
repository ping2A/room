//! `room soft-view`: draw one view of a Doom-engine map with the classic
//! software renderer (`room-render-soft`, `docs/40` SR0) and save it as PNG.

use anyhow::{Context, Result, bail};
use room_vfs::Vfs;
use std::path::Path;

/// Camera and output settings for [`run`].
pub struct SoftViewArgs<'a> {
    /// Map name (`E1M1`, `MAP01`).
    pub map: &'a str,
    /// Output PNG.
    pub output: &'a Path,
    /// Frame size.
    pub width: u32,
    /// Frame height.
    pub height: u32,
    /// Eye X (map units; default: player 1 start).
    pub x: Option<f64>,
    /// Eye Y.
    pub y: Option<f64>,
    /// Eye Z (default: floor + 41, Doom's view height).
    pub z: Option<f64>,
    /// Facing in degrees (default: player 1 start angle).
    pub angle: Option<f64>,
    /// Sky texture (default from the map name, as vanilla).
    pub sky: Option<&'a str>,
    /// Keep vanilla's drawseg / visplane / vissprite limits.
    pub vanilla_limits: bool,
    /// Draw the map's things (spawn sprites, `docs/40` SR2 preview).
    pub things: bool,
    /// Weapon sprite (frame A, resting position).
    pub weapon: Option<&'a str>,
    /// `leveltime` for animations.
    pub tic: u32,
    /// Modern view with this field of view (SR5).
    pub fov: Option<f64>,
    /// Free-look shear in rows.
    pub look: i32,
    /// True-colour drawers.
    pub truecolor: bool,
    /// Write the PNG at this size with Doom's tall pixels and black bars
    /// (a display screenshot; SR10). `None` = the frame as drawn.
    pub display: Option<(u32, u32)>,
}

/// Vanilla's sky for a map name (shared with `room-server` thumbnails).
pub use room_render_soft::shot::default_sky;

/// Render and write the PNG; returns the number of distinct colours
/// (palette indices, or RGB values in true colour), handy for smoke checks.
pub fn run(vfs: &Vfs, args: &SoftViewArgs<'_>) -> Result<usize> {
    let mut map =
        room_map::load_map(vfs, args.map).with_context(|| format!("load map {}", args.map))?;
    room_nodebuild::ensure_nodes(&mut map);
    let start = map.things.iter().find(|t| t.type_id == 1);
    let x = args
        .x
        .or_else(|| start.map(|t| f64::from(t.x)))
        .unwrap_or(0.0);
    let y = args
        .y
        .or_else(|| start.map(|t| f64::from(t.y)))
        .unwrap_or(0.0);
    let angle = args
        .angle
        .or_else(|| start.map(|t| f64::from(t.angle)))
        .unwrap_or(0.0);
    let floor = room_map::portals::sector_at(&map, x, y)
        .and_then(|s| map.sectors.get(s.raw() as usize))
        .map_or(0.0, |s| s.floor_height);
    let z = args.z.unwrap_or(floor + 41.0);
    let Some(mut view) = room_render_soft::SoftView::new(vfs, &map, args.width, args.height) else {
        bail!("{}: no BSP nodes", args.map);
    };
    if let Some(fov) = args.fov {
        view.renderer = room_render_soft::SoftRenderer::new_view(args.width, args.height, fov);
    }
    view.renderer.truecolor = args.truecolor;
    view.renderer.vanilla_limits = args.vanilla_limits;
    // `--vanilla-limits` also keeps vanilla's drawers (no Boom changes).
    view.renderer.boom = !args.vanilla_limits;
    let sky_name = args
        .sky
        .map_or_else(|| default_sky(args.map), str::to_owned);
    let sky = view.sky(vfs, &sky_name);
    let fx = |v: f64| (v * 65536.0).round() as i32;
    // Degrees → BAM, as `mapthing->angle / 45 * ANG45` for whole multiples.
    let bam = ((angle.rem_euclid(360.0) / 360.0) * 4_294_967_296.0) as u64 as u32;
    let mut frame = room_render_soft::SoftFrame {
        x: fx(x),
        y: fx(y),
        z: fx(z),
        angle: bam,
        extralight: 0,
        fixed_colormap: None,
        sectors: None,
        sky,
        ..room_render_soft::SoftFrame::default()
    };
    frame.leveltime = args.tic;
    frame.yshear = args.look;
    if args.things {
        frame.things = room_render_soft::map_things(&map, &view.level);
    }
    if let Some(w) = args.weapon {
        // Resting weapon: `sx` 1.0, `sy` WEAPONTOP (32.0).
        frame.psprites.push(room_render_soft::SoftPSprite {
            sprite: room_render_soft::sprite_name(w),
            frame: 0,
            sx: fx(1.0),
            sy: fx(32.0),
        });
    }
    view.prepare(vfs, &frame);
    let target = view.render(&frame);
    let (w, h, rgba) = match args.display {
        Some((w, h)) => (w, h, target.to_rgba_fit(&view.data.palette, w, h)),
        None => (
            target.width,
            target.height,
            target.to_rgba(&view.data.palette),
        ),
    };
    let png = room_render_soft::shot::encode_png(w, h, &rgba).map_err(anyhow::Error::msg)?;
    std::fs::write(args.output, png)
        .with_context(|| format!("create {}", args.output.display()))?;
    if !target.rgb.is_empty() {
        // True colour: distinct RGB values.
        let distinct: std::collections::HashSet<u32> = target.rgb.iter().copied().collect();
        return Ok(distinct.len());
    }
    let mut seen = [false; 256];
    for &p in &target.pixels {
        seen[usize::from(p)] = true;
    }
    Ok(seen.iter().filter(|s| **s).count())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skies_follow_vanilla() {
        assert_eq!(default_sky("E3M4"), "SKY3");
        assert_eq!(default_sky("map05"), "SKY1");
        assert_eq!(default_sky("MAP15"), "SKY2");
        assert_eq!(default_sky("MAP30"), "SKY3");
    }
}
