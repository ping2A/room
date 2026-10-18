//! Doom sprite names for Build tiles.
//!
//! Every generated class gets frames in its own sprite group; a group holds
//! up to 26 frames (`A`–`Z`) per 4-character name and spills into a fresh
//! name after that. Names are `Q` + three base-36 digits, which no Doom
//! sprite uses.

use std::collections::BTreeMap;

/// Where a lump's origin sits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    /// World sprite: bottom centre, shifted by the tile's picanm offsets.
    World,
    /// Weapon overlay: tile centre (Duke `rotatesprite`) at 320×200 `(x, y)`.
    Hud(i32, i32),
}

/// One sprite lump to emit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpriteLump {
    /// Lump name (`QA01A0`, `QA01B2B8`, …).
    pub name: String,
    /// Source tile.
    pub tile: i32,
    /// Palette swap.
    pub pal: u8,
    /// Origin rule.
    pub origin: Origin,
}

/// `(tile, pal, HUD centre)` of an allocated frame.
type FrameKey = (i32, i32, Option<(i32, i32)>);

#[derive(Debug, Default)]
struct Group {
    names: Vec<String>,
    used: u8,
    frames: BTreeMap<FrameKey, (String, char)>,
}

/// Sprite name allocator and lump list.
#[derive(Debug, Default)]
pub struct SpriteSet {
    next: u32,
    groups: BTreeMap<String, Group>,
    lumps: Vec<SpriteLump>,
}

/// Build views (`k`) shown for Doom rotations 1–5 (2–4 mirror onto 8–6).
fn view_for_rotation(views: i32) -> [i32; 5] {
    if views == 7 {
        [0, 1, 3, 4, 6]
    } else {
        [0, 1, 2, 3, 4]
    }
}

impl SpriteSet {
    /// Empty set.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Lumps registered so far.
    #[must_use]
    pub fn lumps(&self) -> &[SpriteLump] {
        &self.lumps
    }

    fn fresh_name(&mut self) -> String {
        const DIGITS: &[u8; 36] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ";
        let mut n = self.next;
        self.next += 1;
        let mut s = String::from("Q");
        for div in [1296, 36, 1] {
            s.push(char::from(DIGITS[(n / div % 36) as usize]));
            n %= div;
        }
        s
    }

    /// Frame for `tile` (with `views` rotations: 1, 5, 7 or 8) in `group`;
    /// returns the sprite name and frame letter for a DECORATE state.
    pub fn frame(&mut self, group: &str, tile: i32, views: i32, pal: u8) -> (String, char) {
        self.frame_at(group, tile, views, pal, None)
    }

    /// A world sprite lump with a fixed name (`BKEYA0` for Doom's key classes).
    pub fn named(&mut self, name: &str, tile: i32, pal: u8) {
        self.lumps.push(SpriteLump {
            name: name.to_owned(),
            tile,
            pal,
            origin: Origin::World,
        });
    }

    /// HUD weapon frame: `tile` centred at 320×200 `(x, y)`.
    pub fn hud_frame(&mut self, group: &str, tile: i32, x: i32, y: i32) -> (String, char) {
        self.frame_at(group, tile, 1, 0, Some((x, y)))
    }

    fn frame_at(
        &mut self,
        group: &str,
        tile: i32,
        views: i32,
        pal: u8,
        hud: Option<(i32, i32)>,
    ) -> (String, char) {
        let key = (tile, views, hud);
        if let Some(f) = self.groups.get(group).and_then(|g| g.frames.get(&key)) {
            return f.clone();
        }
        let needs_name = self
            .groups
            .get(group)
            .is_none_or(|g| g.names.is_empty() || g.used >= 26);
        let fresh = needs_name.then(|| self.fresh_name());
        let g = self.groups.entry(group.to_owned()).or_default();
        if let Some(name) = fresh {
            g.names.push(name);
            g.used = 0;
        }
        let sprite = g.names.last().cloned().unwrap_or_default();
        let letter = char::from(b'A' + g.used);
        g.used += 1;
        g.frames.insert(key, (sprite.clone(), letter));

        let origin = hud.map_or(Origin::World, |(x, y)| Origin::Hud(x, y));
        let mut push = |name: String, tile: i32| {
            self.lumps.push(SpriteLump {
                name,
                tile,
                pal,
                origin,
            });
        };
        match views {
            5 | 7 => {
                let k = view_for_rotation(views);
                push(format!("{sprite}{letter}1"), tile + k[0]);
                for (rot, &view) in k.iter().enumerate().skip(1).take(3) {
                    let mirror = 10 - (rot + 1);
                    push(
                        format!("{sprite}{letter}{}{letter}{mirror}", rot + 1),
                        tile + view,
                    );
                }
                push(format!("{sprite}{letter}5"), tile + k[4]);
            }
            8 => {
                for rot in 0..8 {
                    push(format!("{sprite}{letter}{}", rot + 1), tile + rot);
                }
            }
            _ => push(format!("{sprite}{letter}0"), tile),
        }
        (sprite, letter)
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]
    use super::*;

    #[test]
    fn groups_share_names_and_spill_after_z() {
        let mut s = SpriteSet::new();
        assert_eq!(s.frame("a", 10, 1, 0), ("Q000".into(), 'A'));
        assert_eq!(s.frame("a", 11, 1, 0), ("Q000".into(), 'B'));
        assert_eq!(s.frame("a", 10, 1, 0), ("Q000".into(), 'A'));
        assert_eq!(s.frame("b", 10, 1, 0), ("Q001".into(), 'A'));
        for t in 12..36 {
            s.frame("a", t, 1, 0);
        }
        assert_eq!(s.frame("a", 99, 1, 0), ("Q002".into(), 'A'));
        assert_eq!(s.lumps()[0].name, "Q000A0");
    }

    #[test]
    fn five_views_make_mirrored_rotation_lumps() {
        let mut s = SpriteSet::new();
        s.frame("t", 100, 5, 0);
        let names: Vec<_> = s
            .lumps()
            .iter()
            .map(|l| (l.name.as_str(), l.tile))
            .collect();
        assert_eq!(
            names,
            [
                ("Q000A1", 100),
                ("Q000A2A8", 101),
                ("Q000A3A7", 102),
                ("Q000A4A6", 103),
                ("Q000A5", 104)
            ]
        );
        let mut s = SpriteSet::new();
        s.frame("t", 0, 7, 0);
        assert_eq!(s.lumps()[4].tile, 6);
        assert_eq!(s.lumps()[2].tile, 3);
    }
}
