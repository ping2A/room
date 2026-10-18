//! ANIMDEFS texture / flat animation: swaps an animated base picture for the
//! frame due at the current game tic before materials are bound.
//!
//! Explicit `pic` lists (with `oscillate`) animate the base name, as in GZDoom.
//! `range` animations need the texture / flat lump order and are skipped.

use crate::scene::{MaterialRange, SceneLists};
use room_defs::animdefs::AnimDefs;
use std::collections::BTreeMap;

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
            let mut seq: Vec<(String, u32)> = def
                .pics
                .iter()
                .map(|p| {
                    let tics = p
                        .tics
                        .or_else(|| p.rand.map(|(a, b)| (a + b) / 2))
                        .unwrap_or(8);
                    (
                        p.name.clone(),
                        u32::try_from(tics.clamp(1, 65_535)).unwrap_or(1),
                    )
                })
                .collect();
            if def.oscillate {
                let back: Vec<_> = seq[1..seq.len() - 1].iter().rev().cloned().collect();
                seq.extend(back);
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
}
