//! Threaded slices (SR9, UZDoom `r_scene_multithreaded`): the frame is cut
//! into vertical slices, each drawn by its own copy of the renderer on its
//! own thread into its own frame; the slices' columns are then copied into
//! the target and the weapon is drawn over the whole.
//!
//! Each slice walks the BSP with the other slices' columns marked solid,
//! so it stores only its own walls and planes. Things are projected by the
//! sectors a slice reaches; after the walks every slice also takes the
//! things of sectors only other slices reached, so a thing standing across
//! a slice edge is whole. The copies keep their buffers between frames.

#[cfg(not(target_arch = "wasm32"))]
use super::{SoftFrame, tint_of};
use super::{SoftRenderer, SoftTarget};
#[cfg(not(target_arch = "wasm32"))]
use crate::data::SoftData;
#[cfg(not(target_arch = "wasm32"))]
use crate::level::SoftLevel;

/// The slice renderers, their frames and the settings they were made for.
#[derive(Clone, Debug, Default)]
#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
pub(super) struct Slices {
    parts: Vec<(SoftRenderer, SoftTarget, (i32, i32))>,
    key: Option<SliceKey>,
    reached: Vec<bool>,
}

/// Settings a slice copy is made with.
#[derive(Clone, Copy, Debug, PartialEq)]
struct SliceKey {
    count: usize,
    width: i32,
    height: i32,
    projection: i32,
    truecolor: bool,
    boom: bool,
    vanilla_limits: bool,
    visibility: f64,
}

#[cfg(not(target_arch = "wasm32"))]
impl SoftRenderer {
    /// [`SoftRenderer::render`] on `self.threads` threads.
    pub(super) fn render_sliced(
        &mut self,
        level: &SoftLevel,
        data: &SoftData,
        frame: &SoftFrame,
        target: &mut SoftTarget,
    ) {
        // Slices narrower than 32 columns, or under a 640×400 frame's
        // pixels, are not worth a thread.
        let worth = ((self.width * self.height) as usize / (640 * 400)).max(1);
        let count = self
            .threads
            .min(worth)
            .min((self.width / 32).max(1) as usize)
            .max(1);
        let key = SliceKey {
            count,
            width: self.width,
            height: self.height,
            projection: self.projection,
            truecolor: self.truecolor,
            boom: self.boom,
            vanilla_limits: self.vanilla_limits,
            visibility: self.visibility,
        };
        let mut slices = std::mem::take(&mut self.slices);
        if slices.key != Some(key) {
            slices.parts.clear();
            for i in 0..count {
                let x0 = self.width * i as i32 / count as i32;
                let x1 = self.width * (i as i32 + 1) / count as i32 - 1;
                let r = self.sub_renderer();
                let t = SoftTarget::new(self.width as u32, self.height as u32);
                slices.parts.push((r, t, (x0, x1)));
            }
            slices.key = Some(key);
        }
        let sectors = frame.sectors.as_deref().unwrap_or(&level.sectors);
        // The walks.
        std::thread::scope(|s| {
            for (r, t, range) in &mut slices.parts {
                let range = *range;
                s.spawn(move || r.view_bsp(level, data, frame, None, Some(range), t));
            }
        });
        // Sectors any slice reached.
        slices.reached.clear();
        slices.reached.resize(sectors.len(), false);
        for (r, _, _) in &slices.parts {
            for (dst, &src) in slices.reached.iter_mut().zip(&r.sector_added) {
                *dst |= src;
            }
        }
        let reached = &slices.reached;
        std::thread::scope(|s| {
            for (r, t, _) in &mut slices.parts {
                s.spawn(move || {
                    r.add_reached_sprites(level, data, sectors, reached);
                    r.view_finish(level, data, frame, false, t);
                });
            }
        });
        // Each slice's columns into the target.
        let w = self.width as usize;
        if self.truecolor && target.rgb.len() != target.pixels.len() {
            target.rgb = vec![0; target.pixels.len()];
        }
        for (_, t, (x0, x1)) in &slices.parts {
            let (a, b) = (*x0 as usize, *x1 as usize + 1);
            for y in 0..self.height as usize {
                let row = y * w;
                target.pixels[row + a..row + b].copy_from_slice(&t.pixels[row + a..row + b]);
                if self.truecolor && t.rgb.len() == t.pixels.len() {
                    target.rgb[row + a..row + b].copy_from_slice(&t.rgb[row + a..row + b]);
                }
            }
        }
        // The weapon over the whole frame (slice 0 holds the view state).
        if let Some((r, _, _)) = slices.parts.first_mut() {
            r.draw_player_sprites(level, data, sectors, frame, target);
        }
        self.slices = slices;
    }

    /// Project the things of sectors other slices reached and this one did
    /// not (`R_AddSprites` with the sector's light).
    fn add_reached_sprites(
        &mut self,
        level: &SoftLevel,
        data: &SoftData,
        sectors: &[crate::level::SoftSector],
        reached: &[bool],
    ) {
        for (si, &r) in reached.iter().enumerate() {
            if !r || self.sector_added.get(si).copied().unwrap_or(true) {
                continue;
            }
            let (front, floorlight, ceilinglight) = self.fake_flat(level, sectors, si, false);
            self.tint = tint_of(&front);
            let heightsec = level.heightsec.get(si).copied().flatten();
            self.add_sprites(data, si, (floorlight + ceilinglight) / 2, heightsec);
        }
    }
}
