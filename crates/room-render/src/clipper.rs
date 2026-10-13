//! Angular clipper for BSP visibility (GZDoom `Clipper` subset).
//!
//! Ranges are half-open BAM intervals on the circle. A range that wraps across
//! 0 is stored as two pieces.

use room_core::{Bam, math::BAM_PER_TURN};

/// Sorted list of occluded angular ranges.
#[derive(Clone, Debug, Default)]
pub struct Clipper {
    /// Non-overlapping `[start, end)` BAM pairs, sorted by start.
    ranges: Vec<(u32, u32)>,
    /// True once the entire circle is covered.
    full: bool,
}

impl Clipper {
    /// Empty clipper (nothing occluded).
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// True if the full circle is occluded.
    #[must_use]
    pub fn is_full(&self) -> bool {
        self.full
    }

    /// True if any part of `[start, end)` (possibly wrapping) is still visible.
    #[must_use]
    pub fn is_range_visible(&self, start: Bam, end: Bam) -> bool {
        if self.full {
            return false;
        }
        let s = start.0;
        let e = end.0;
        if s == e {
            return false;
        }
        if s < e {
            self.visible_span(s, e)
        } else {
            self.visible_span(s, u32::MAX) || self.visible_span(0, e)
        }
    }

    fn visible_span(&self, start: u32, end: u32) -> bool {
        if start >= end {
            return false;
        }
        let mut cursor = start;
        for &(a, b) in &self.ranges {
            if b <= cursor {
                continue;
            }
            if a >= end {
                break;
            }
            if a > cursor {
                return true;
            }
            cursor = cursor.max(b);
            if cursor >= end {
                return false;
            }
        }
        cursor < end
    }

    /// Mark `[start, end)` as occluded (possibly wrapping).
    pub fn add_range(&mut self, start: Bam, end: Bam) {
        if self.full {
            return;
        }
        let s = start.0;
        let e = end.0;
        // Degenerate span — ignore (do not mark the whole circle occluded).
        if s == e {
            return;
        }
        if s < e {
            self.add_span(s, e);
        } else {
            self.add_span(s, u32::MAX);
            self.add_span(0, e);
        }
        self.check_full();
    }

    fn add_span(&mut self, start: u32, end: u32) {
        if start >= end {
            return;
        }
        let mut merged = (start, end);
        let mut out = Vec::with_capacity(self.ranges.len() + 1);
        let mut inserted = false;
        for &(a, b) in &self.ranges {
            if b < merged.0 {
                out.push((a, b));
            } else if a > merged.1 {
                if !inserted {
                    out.push(merged);
                    inserted = true;
                }
                out.push((a, b));
            } else {
                merged.0 = merged.0.min(a);
                merged.1 = merged.1.max(b);
            }
        }
        if !inserted {
            out.push(merged);
        }
        self.ranges = out;
    }

    fn check_full(&mut self) {
        if self.ranges.len() == 1 {
            let (a, b) = self.ranges[0];
            if a == 0 && b == u32::MAX {
                self.full = true;
                self.ranges.clear();
            }
        }
    }
}

/// Pseudo-angle of point `(x,y)` relative to view `(vx,vy)` as BAM.
#[must_use]
pub fn pseudo_angle(vx: f64, vy: f64, x: f64, y: f64) -> Bam {
    let dx = x - vx;
    let dy = y - vy;
    let rad = libm::atan2(dy, dx);
    let turn = rad / (2.0 * core::f64::consts::PI);
    let mut t = turn;
    if t < 0.0 {
        t += 1.0;
    }
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss
    )]
    Bam((t * BAM_PER_TURN).round() as u32)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn empty_is_visible() {
        let c = Clipper::new();
        assert!(c.is_range_visible(Bam(0), Bam(1000)));
        assert!(!c.is_full());
    }

    #[test]
    fn add_blocks_range() {
        let mut c = Clipper::new();
        c.add_range(Bam(100), Bam(200));
        assert!(!c.is_range_visible(Bam(120), Bam(150)));
        assert!(c.is_range_visible(Bam(0), Bam(50)));
        assert!(c.is_range_visible(Bam(250), Bam(300)));
    }

    #[test]
    fn merge_adjacent() {
        let mut c = Clipper::new();
        c.add_range(Bam(100), Bam(200));
        c.add_range(Bam(150), Bam(300));
        assert!(!c.is_range_visible(Bam(100), Bam(300)));
        assert!(c.is_range_visible(Bam(300), Bam(400)));
    }
}
