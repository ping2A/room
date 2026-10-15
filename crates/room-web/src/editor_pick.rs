//! Editor 3D picking: which wall part, plane or thing is under the cursor.

use glam::DVec3;
use room_map::MapGeometry;
use room_nodebuild::point_in_sector;

/// Wall section of a sidedef.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WallPart {
    /// Upper texture (above the neighbour's ceiling).
    Upper,
    /// Middle texture (one-sided wall).
    Middle,
    /// Lower texture (below the neighbour's floor).
    Lower,
}

/// Picked map element.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Pick {
    /// Thing index into `MapGeometry::things`.
    Thing(usize),
    /// Linedef, side `0` front / `1` back, part.
    Wall {
        /// Line index.
        line: usize,
        /// `0` front, `1` back.
        side: u8,
        /// Section.
        part: WallPart,
    },
    /// Sector floor (`ceiling = false`) or ceiling.
    Plane {
        /// Sector index.
        sector: usize,
        /// Ceiling instead of floor.
        ceiling: bool,
    },
}

/// Thing collision cylinder for picking.
#[derive(Clone, Copy, Debug)]
pub struct ThingBox {
    /// Index into `MapGeometry::things`.
    pub index: usize,
    /// Centre (bottom z).
    pub pos: DVec3,
    /// Radius.
    pub radius: f64,
    /// Height.
    pub height: f64,
}

/// Nearest hit of the ray `eye + t·dir` (`dir` need not be normalised).
#[must_use]
pub fn pick(map: &MapGeometry, things: &[ThingBox], eye: DVec3, dir: DVec3) -> Option<Pick> {
    let mut best: Option<(f64, Pick)> = None;
    for th in things {
        if let Some(t) = ray_cylinder(eye, dir, th) {
            if best.is_none_or(|(bt, _)| t < bt) {
                best = Some((t, Pick::Thing(th.index)));
            }
        }
    }
    let world = walk(map, eye, dir);
    match (best, world) {
        (Some((tt, p)), Some((tw, _))) if tt <= tw => Some(p),
        (_, Some((_, p))) => Some(p),
        (Some((_, p)), None) => Some(p),
        (None, None) => None,
    }
}

fn ray_cylinder(eye: DVec3, dir: DVec3, th: &ThingBox) -> Option<f64> {
    let ox = eye.x - th.pos.x;
    let oy = eye.y - th.pos.y;
    let a = dir.x * dir.x + dir.y * dir.y;
    if a < 1e-12 {
        return None;
    }
    let b = 2.0 * (ox * dir.x + oy * dir.y);
    let c = ox * ox + oy * oy - th.radius * th.radius;
    let disc = b * b - 4.0 * a * c;
    if disc < 0.0 {
        return None;
    }
    let t = (-b - disc.sqrt()) / (2.0 * a);
    if t <= 0.0 {
        return None;
    }
    let z = eye.z + dir.z * t;
    (z >= th.pos.z && z <= th.pos.z + th.height).then_some(t)
}

/// Ray-march sector to sector across linedefs.
fn walk(map: &MapGeometry, eye: DVec3, dir: DVec3) -> Option<(f64, Pick)> {
    let mut cur = point_in_sector(map, eye.x, eye.y)?.raw() as usize;
    let mut crossings: Vec<(f64, usize, bool)> = Vec::new();
    for (li, line) in map.lines.iter().enumerate() {
        let v1 = map.vertices.get(line.v1.raw() as usize)?.pos;
        let v2 = map.vertices.get(line.v2.raw() as usize)?.pos;
        let (ex, ey) = (v2.x() - v1.x(), v2.y() - v1.y());
        let denom = dir.x * ey - dir.y * ex;
        if denom.abs() < 1e-12 {
            continue;
        }
        let (wx, wy) = (v1.x() - eye.x, v1.y() - eye.y);
        let t = (wx * ey - wy * ex) / denom;
        let u = (wx * dir.y - wy * dir.x) / denom;
        if t <= 1e-9 || !(0.0..=1.0).contains(&u) {
            continue;
        }
        // Doom front side is to the right of v1→v2.
        let from_front = ex * (eye.y - v1.y()) - ey * (eye.x - v1.x()) < 0.0;
        crossings.push((t, li, from_front));
    }
    crossings.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut t_prev = 0.0;
    for (t, li, geo_front) in crossings {
        let line = &map.lines[li];
        let front = line.front_sector.map(|s| s.raw() as usize);
        let back = line.back_sector.map(|s| s.raw() as usize);
        let from_front = if front == Some(cur) && back != Some(cur) {
            true
        } else if back == Some(cur) && front != Some(cur) {
            false
        } else {
            geo_front
        };
        let near = if from_front {
            line.front_sector
        } else {
            line.back_sector
        };
        // Overlapping / unclosed geometry: re-sync to the side being left.
        if let Some(n) = near {
            cur = n.raw() as usize;
        }
        if let Some(hit) = plane_hit(map, cur, eye, dir, t_prev, t) {
            return Some(hit);
        }
        let side = u8::from(!from_front);
        let z = eye.z + dir.z * t;
        let other = if from_front {
            line.back_sector
        } else {
            line.front_sector
        };
        let Some(other) = other.filter(|_| line.sides[1].is_some()) else {
            return Some((
                t,
                Pick::Wall {
                    line: li,
                    side,
                    part: WallPart::Middle,
                },
            ));
        };
        let o = map.sectors.get(other.raw() as usize)?;
        if z < o.floor_height || o.floor_height >= o.ceiling_height {
            return Some((
                t,
                Pick::Wall {
                    line: li,
                    side,
                    part: if z < o.floor_height {
                        WallPart::Lower
                    } else {
                        WallPart::Upper
                    },
                },
            ));
        }
        if z > o.ceiling_height {
            return Some((
                t,
                Pick::Wall {
                    line: li,
                    side,
                    part: WallPart::Upper,
                },
            ));
        }
        cur = other.raw() as usize;
        t_prev = t;
    }
    plane_hit(map, cur, eye, dir, t_prev, f64::INFINITY)
}

fn plane_hit(
    map: &MapGeometry,
    sector: usize,
    eye: DVec3,
    dir: DVec3,
    t0: f64,
    t1: f64,
) -> Option<(f64, Pick)> {
    let s = map.sectors.get(sector)?;
    let (h, ceiling) = if dir.z < 0.0 {
        (s.floor_height, false)
    } else if dir.z > 0.0 {
        (s.ceiling_height, true)
    } else {
        return None;
    };
    let t = (h - eye.z) / dir.z;
    (t >= t0 && t <= t1).then_some((t, Pick::Plane { sector, ceiling }))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use room_nodebuild::apply_nodes;
    use room_testkit::{MapBuilder, rect};

    fn two_rooms() -> MapGeometry {
        // Room A (floor 0) next to room B with a raised floor (step).
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .room("b", rect(256.0, 0.0, 512.0, 256.0), 32.0, 96.0)
            .player_start(64.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        map
    }

    #[test]
    fn floor_ceiling_and_step_parts() {
        let map = two_rooms();
        let eye = DVec3::new(64.0, 128.0, 41.0);
        let down = pick(&map, &[], eye, DVec3::new(0.0, 0.0, -1.0)).unwrap();
        assert!(matches!(down, Pick::Plane { ceiling: false, .. }));
        let up = pick(&map, &[], eye, DVec3::new(0.0, 0.0, 1.0)).unwrap();
        assert!(matches!(up, Pick::Plane { ceiling: true, .. }));
        let low = pick(&map, &[], DVec3::new(64.0, 128.0, 8.0), DVec3::X).unwrap();
        assert!(matches!(
            low,
            Pick::Wall {
                part: WallPart::Lower,
                ..
            }
        ));
        let high = pick(&map, &[], DVec3::new(64.0, 128.0, 110.0), DVec3::X).unwrap();
        assert!(matches!(
            high,
            Pick::Wall {
                part: WallPart::Upper,
                ..
            }
        ));
        let far = pick(&map, &[], eye, DVec3::X).unwrap();
        assert!(matches!(
            far,
            Pick::Wall {
                part: WallPart::Middle,
                ..
            }
        ));
    }

    #[test]
    fn thing_in_front_wins() {
        let map = two_rooms();
        let th = ThingBox {
            index: 7,
            pos: DVec3::new(160.0, 128.0, 0.0),
            radius: 20.0,
            height: 56.0,
        };
        let eye = DVec3::new(64.0, 128.0, 41.0);
        assert_eq!(pick(&map, &[th], eye, DVec3::X), Some(Pick::Thing(7)));
    }
}
