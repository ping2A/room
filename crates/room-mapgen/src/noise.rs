//! Smooth value noise (random lattice, smoothstep interpolation) for
//! terrain heights, ceilings and streams.

use crate::rng::Rng;

/// A noise field over a room's cells.
#[derive(Clone, Debug)]
pub struct Noise {
    step: f64,
    w: usize,
    lattice: Vec<f64>,
}

impl Noise {
    /// Field covering `w` × `h` cells with features about `step` cells wide.
    pub fn new(rng: &mut Rng, w: i32, h: i32, step: f64) -> Self {
        let lw = (f64::from(w) / step).ceil() as usize + 2;
        let lh = (f64::from(h) / step).ceil() as usize + 2;
        Self {
            step,
            w: lw,
            lattice: (0..lw * lh).map(|_| rng.f64()).collect(),
        }
    }

    /// Value in `[0, 1]` at cell `(i, j)` (room-local).
    #[must_use]
    pub fn at(&self, i: i32, j: i32) -> f64 {
        let (x, y) = (
            f64::from(i.max(0)) / self.step,
            f64::from(j.max(0)) / self.step,
        );
        let (x0, y0) = (x.floor() as usize, y.floor() as usize);
        let (fx, fy) = (x - x.floor(), y - y.floor());
        let s = |t: f64| t * t * (3.0 - 2.0 * t);
        let v = |xx: usize, yy: usize| self.lattice.get(yy * self.w + xx).copied().unwrap_or(0.5);
        let a = v(x0, y0) + (v(x0 + 1, y0) - v(x0, y0)) * s(fx);
        let b = v(x0, y0 + 1) + (v(x0 + 1, y0 + 1) - v(x0, y0 + 1)) * s(fx);
        a + (b - a) * s(fy)
    }
}
