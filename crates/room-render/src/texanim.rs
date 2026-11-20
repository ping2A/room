//! ANIMDEFS texture / flat animation: swaps an animated base picture for the
//! frame due at the current game tic before materials are bound.
//!
//! Explicit `pic` lists (with `oscillate`) animate the base name, as in GZDoom.
//! `range` animations need the texture / flat lump order and are skipped.
//! `rand min max` frames get seeded random durations (see `RAND_PASSES`).

use crate::scene::{MaterialRange, SceneLists};
use room_defs::animdefs::AnimDefs;
use std::collections::BTreeMap;

/// Passes in the schedule of an animation with `rand` tics.
const RAND_PASSES: usize = 32;

fn name_seed(name: &str) -> u64 {
    name.bytes().fold(0xCBF2_9CE4_8422_2325, |h, b| {
        (h ^ u64::from(b.to_ascii_uppercase())).wrapping_mul(0x0100_0000_01B3)
    })
}

fn splitmix(mut x: u64) -> u64 {
    x = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = x;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

#[derive(Clone, Debug)]
struct Anim {
    /// `(frame, first tic of the frame within the cycle)`.
    frames: Vec<(String, u32)>,
    cycle: u32,
}

/// Name → current frame lookup built from [`AnimDefs`].
#[derive(Clone, Debug, Default)]
pub struct TextureAnimator {
    anims: BTreeMap<String, Anim>,
}

impl TextureAnimator {
    /// Animations from parsed ANIMDEFS (explicit `pic` lists only).
    #[must_use]
    pub fn new(defs: &AnimDefs) -> Self {
        let mut anims = BTreeMap::new();
        for def in &defs.anims {
            if def.pics.len() < 2 || def.pics.iter().any(|p| p.name.starts_with("RANGE:")) {
                continue;
            }
            // One pass: `(name, fixed tics or rand range)`.
            let mut pass: Vec<(String, (i64, i64))> = def
                .pics
                .iter()
                .map(|p| {
                    let range = p.rand.unwrap_or_else(|| {
                        let t = p.tics.unwrap_or(8);
                        (t, t)
                    });
                    (p.name.clone(), range)
                })
                .collect();
            if def.oscillate {
                let back: Vec<_> = pass[1..pass.len() - 1].iter().rev().cloned().collect();
                pass.extend(back);
            }
            // `rand min max`: GZDoom draws each frame's duration anew every
            // time it is shown (`pr_animatepictures`, outside the playsim).
            // Room's lookup is a pure function of the clock, so it plays a
            // long schedule of passes with seeded random durations instead
            // of the mean; the pattern repeats only after RAND_PASSES passes.
            let random = pass.iter().any(|(_, (a, b))| a != b);
            let passes = if random { RAND_PASSES } else { 1 };
            let mut seq: Vec<(String, u32)> = Vec::with_capacity(pass.len() * passes);
            let mut rng = name_seed(&def.name);
            for _ in 0..passes {
                for (name, (lo, hi)) in &pass {
                    let (lo, hi) = ((*lo).min(*hi), (*lo).max(*hi));
                    let tics = if lo == hi {
                        lo
                    } else {
                        rng = splitmix(rng);
                        let span = u64::try_from(hi - lo + 1).unwrap_or(1);
                        lo + i64::try_from(rng % span).unwrap_or(0)
                    };
                    seq.push((
                        name.clone(),
                        u32::try_from(tics.clamp(1, 65_535)).unwrap_or(1),
                    ));
                }
            }
            let mut at = 0u32;
            let frames = seq
                .into_iter()
                .map(|(n, t)| {
                    let start = at;
                    at = at.saturating_add(t);
                    (n, start)
                })
                .collect();
            anims.insert(def.name.clone(), Anim { frames, cycle: at });
        }
        Self { anims }
    }

    /// No animations defined.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.anims.is_empty()
    }

    /// Frame shown for `name` at game tic `tic` (`None` = not animated).
    #[must_use]
    pub fn frame(&self, name: &str, tic: u32) -> Option<&str> {
        let a = self.anims.get(name)?;
        if a.cycle == 0 {
            return None;
        }
        let t = tic % a.cycle;
        let k = a.frames.partition_point(|&(_, start)| start <= t);
        a.frames.get(k.checked_sub(1)?).map(|(n, _)| n.as_str())
    }

    /// Rename every animated wall / flat material of `scene` to its frame.
    pub fn apply(&self, scene: &mut SceneLists, tic: u32) {
        if self.anims.is_empty() {
            return;
        }
        let swap = |ranges: &mut Vec<MaterialRange>| {
            for r in ranges {
                if let Some(f) = self.frame(&r.name, tic) {
                    r.name = f.to_owned();
                }
            }
        };
        swap(&mut scene.wall_ranges);
        swap(&mut scene.translucent_wall_ranges);
        swap(&mut scene.flat_ranges);
        swap(&mut scene.translucent_flat_ranges);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use room_defs::animdefs::parse_animdefs;

    #[test]
    fn pic_lists_cycle_and_oscillate() {
        let defs = parse_animdefs(
            "texture A\n pic A tics 2\n pic B tics 3\n\
             texture C oscillate\n pic C tics 1\n pic D tics 1\n pic E tics 1\n\
             flat N\n pic N tics 8\n range Q tics 8\n",
        )
        .unwrap_or_default();
        let a = TextureAnimator::new(&defs);
        let seq: Vec<_> = (0..6).map(|t| a.frame("A", t).unwrap_or("-")).collect();
        assert_eq!(seq, ["A", "A", "B", "B", "B", "A"]);
        let osc: Vec<_> = (0..5).map(|t| a.frame("C", t).unwrap_or("-")).collect();
        assert_eq!(osc, ["C", "D", "E", "D", "C"]);
        assert_eq!(a.frame("N", 0), None, "ranges need lump order");
        assert_eq!(a.frame("B", 0), None, "only the base name animates");
    }

    #[test]
    fn rand_tics_vary_inside_their_range_and_are_repeatable() {
        let defs =
            parse_animdefs("texture R\n pic R rand 2 9\n pic S tics 1\n").unwrap_or_default();
        let a = TextureAnimator::new(&defs);
        // Lengths of each stretch of "R" over many passes.
        let mut runs = Vec::new();
        let mut run = 0u32;
        for t in 0..2000 {
            if a.frame("R", t) == Some("R") {
                run += 1;
            } else if run > 0 {
                runs.push(run);
                run = 0;
            }
        }
        assert!(runs.len() > 50);
        assert!(runs.iter().all(|r| (2..=9).contains(r)), "{runs:?}");
        let distinct: std::collections::BTreeSet<_> = runs.iter().collect();
        assert!(distinct.len() >= 5, "durations really vary: {distinct:?}");
        // Same defs, same schedule (render state only, but stable).
        let b = TextureAnimator::new(&defs);
        assert!((0..500).all(|t| a.frame("R", t) == b.frame("R", t)));
    }
}
