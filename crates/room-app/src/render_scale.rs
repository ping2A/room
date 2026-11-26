//! Render scale (docs/41 M6): the 3D view is drawn at a fraction of the
//! screen size and stretched to it; the HUD stays sharp. `r_scale` fixes the
//! fraction; with `r_dynres` it is the ceiling and [`DynamicScale`] lowers
//! it when frames run long and raises it again when there is room.

/// Smallest scale allowed.
pub(crate) const MIN_SCALE: f32 = 0.5;
/// Scale steps (avoid rebuilding targets for tiny changes).
pub(crate) const STEP: f32 = 0.05;
/// Frames averaged before a decision.
pub(crate) const WINDOW: usize = 30;
/// Good windows in a row before stepping up.
pub(crate) const GOOD_WINDOWS_TO_RAISE: u32 = 3;

/// `scale` clamped to `[MIN_SCALE, 1]` and rounded to a [`STEP`].
#[must_use]
pub(crate) fn quantize(scale: f32) -> f32 {
    let s = if scale.is_finite() { scale } else { 1.0 };
    // Divide (not multiply by 0.05) so steps land exactly on 0.95, 0.9, ….
    ((s.clamp(MIN_SCALE, 1.0) / STEP).round() / (1.0 / STEP)).clamp(MIN_SCALE, 1.0)
}

/// Pixel size of the 3D view for a `w × h` screen at `scale`.
#[must_use]
pub(crate) fn scaled_size(w: u32, h: u32, scale: f32) -> (u32, u32) {
    let s = quantize(scale);
    let f = |v: u32| ((v as f32 * s).round() as u32).max(1);
    (f(w), f(h))
}

/// Frame-time driven scale between [`MIN_SCALE`] and a ceiling.
#[derive(Clone, Debug)]
pub(crate) struct DynamicScale {
    scale: f32,
    window: Vec<f32>,
    good: u32,
}

impl DynamicScale {
    /// Start at `ceiling`.
    pub(crate) fn new(ceiling: f32) -> Self {
        Self {
            scale: quantize(ceiling),
            window: Vec::with_capacity(WINDOW),
            good: 0,
        }
    }

    /// Current scale.
    pub(crate) fn scale(&self) -> f32 {
        self.scale
    }

    /// Record one frame. Returns the new scale when it changes.
    ///
    /// A window averaging more than 10 % over the frame budget steps down at
    /// once; [`GOOD_WINDOWS_TO_RAISE`] windows in a row within 5 % of it
    /// step up (with vsync a frame that keeps up takes about the budget).
    pub(crate) fn frame(&mut self, ms: f32, target_fps: f32, ceiling: f32) -> Option<f32> {
        let ceiling = quantize(ceiling);
        if self.scale > ceiling {
            self.scale = ceiling;
            self.window.clear();
            self.good = 0;
            return Some(self.scale);
        }
        if !ms.is_finite() || ms <= 0.0 {
            return None;
        }
        self.window.push(ms);
        if self.window.len() < WINDOW {
            return None;
        }
        let avg = self.window.iter().sum::<f32>() / self.window.len() as f32;
        self.window.clear();
        let budget = 1000.0 / target_fps.max(1.0);
        let before = self.scale;
        if avg > budget * 1.10 {
            self.good = 0;
            self.scale = quantize(self.scale - STEP);
        } else if avg <= budget * 1.05 {
            self.good += 1;
            if self.good >= GOOD_WINDOWS_TO_RAISE {
                self.good = 0;
                self.scale = quantize((self.scale + STEP).min(ceiling));
            }
        } else {
            self.good = 0;
        }
        (self.scale != before).then_some(self.scale)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(d: &mut DynamicScale, ms: f32, frames: usize, ceiling: f32) -> Vec<f32> {
        (0..frames)
            .filter_map(|_| d.frame(ms, 60.0, ceiling))
            .collect()
    }

    #[test]
    fn sizes_and_steps() {
        assert_eq!(quantize(0.73), 0.75);
        assert_eq!(quantize(0.1), MIN_SCALE);
        assert_eq!(quantize(2.0), 1.0);
        assert_eq!(quantize(f32::NAN), 1.0);
        assert_eq!(scaled_size(2400, 1080, 1.0), (2400, 1080));
        assert_eq!(scaled_size(2400, 1080, 0.5), (1200, 540));
        assert_eq!(scaled_size(1, 1, 0.5), (1, 1));
    }

    #[test]
    fn slow_frames_step_down_to_the_floor() {
        let mut d = DynamicScale::new(1.0);
        // 25 fps against a 60 fps budget: one step per window.
        let steps = run(&mut d, 40.0, WINDOW * 20, 1.0);
        assert_eq!(steps.first().copied(), Some(0.95));
        assert_eq!(d.scale(), MIN_SCALE);
        assert_eq!(steps.len(), 10, "1.0 → 0.5 in 0.05 steps");
    }

    #[test]
    fn fast_frames_step_up_slowly_to_the_ceiling() {
        let mut d = DynamicScale::new(0.8);
        let mut d2 = d.clone();
        // Down once first.
        run(&mut d, 40.0, WINDOW, 0.8);
        assert_eq!(d.scale(), 0.75);
        // Keeping up (16.7 ms at 60 fps): up one step per three windows.
        let steps = run(&mut d, 16.7, WINDOW * 3, 0.8);
        assert_eq!(steps, [0.8]);
        assert!(
            run(&mut d, 16.7, WINDOW * 10, 0.8).is_empty(),
            "never above the ceiling"
        );
        // A window in between (7 % over) neither raises nor lowers.
        assert!(run(&mut d2, 17.8, WINDOW * 6, 0.8).is_empty());
    }

    #[test]
    fn a_lower_ceiling_applies_at_once() {
        let mut d = DynamicScale::new(1.0);
        assert_eq!(d.frame(16.0, 60.0, 0.6), Some(0.6));
        assert_eq!(d.frame(16.0, 60.0, 0.6), None);
    }
}
