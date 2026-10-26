//! Immutable render snapshot from a [`BuildWorld`].
//! Renderer never mutates the world (AGENTS rule 2). Spec: `docs/25` §7 / D3.

use crate::geom::inside;
use crate::world::{BuildWorld, MAXSECTORS};

/// Camera + visible-sector set for the classic Build draw path.
#[derive(Clone, Debug)]
pub struct BuildView {
    pub posx: i32,
    pub posy: i32,
    pub posz: i32,
    pub ang: i16,
    pub horiz: i16,
    pub sect: i16,
    /// Sectors reachable through portals from `sect` (BFS order).
    pub visible_sectors: Vec<i16>,
}

impl BuildWorld {
    /// Build a view snapshot. `sect` is updated via point-in-sector if needed.
    pub fn build_view(
        &self,
        posx: i32,
        posy: i32,
        posz: i32,
        ang: i16,
        horiz: i16,
        mut sect: i16,
    ) -> BuildView {
        let walls = self.wall_xy();
        let sectors = self.sector_walls();
        if sect < 0 || sect >= self.numsectors || !inside(posx, posy, sect, &sectors, &walls) {
            crate::geom::updatesector(posx, posy, &mut sect, &sectors, &walls);
        }
        let visible = portal_flood(self, sect);
        BuildView {
            posx,
            posy,
            posz,
            ang,
            horiz,
            sect,
            visible_sectors: visible,
        }
    }
}

fn portal_flood(world: &BuildWorld, start: i16) -> Vec<i16> {
    if start < 0 || start >= world.numsectors {
        return Vec::new();
    }
    let mut visited = [false; MAXSECTORS];
    let mut out = Vec::new();
    let mut queue = [0_i16; MAXSECTORS];
    let mut qh = 0_usize;
    let mut qt = 0_usize;
    queue[qt] = start;
    qt += 1;
    visited[start as usize] = true;
    while qh < qt {
        let s = queue[qh];
        qh += 1;
        out.push(s);
        let sec = &world.sectors[s as usize];
        let start_w = sec.wallptr as usize;
        let end = start_w
            .saturating_add(sec.wallnum as usize)
            .min(world.walls.len());
        for w in &world.walls[start_w..end] {
            let ns = w.nextsector;
            if ns >= 0 && (ns as usize) < world.sectors.len() && !visited[ns as usize] {
                visited[ns as usize] = true;
                if qt < queue.len() {
                    queue[qt] = ns;
                    qt += 1;
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use crate::BuildWorld;
    use crate::types::{SectorT, WallT};

    #[test]
    fn view_floods_two_sectors() {
        let sectors = vec![
            SectorT {
                wallptr: 0,
                wallnum: 4,
                ..SectorT::default()
            },
            SectorT {
                wallptr: 4,
                wallnum: 4,
                ..SectorT::default()
            },
        ];
        let walls = vec![
            WallT {
                x: 0,
                y: 0,
                point2: 1,
                nextsector: -1,
                ..WallT::default()
            },
            WallT {
                x: 100,
                y: 0,
                point2: 2,
                nextsector: 1,
                ..WallT::default()
            },
            WallT {
                x: 100,
                y: 100,
                point2: 3,
                nextsector: -1,
                ..WallT::default()
            },
            WallT {
                x: 0,
                y: 100,
                point2: 0,
                nextsector: -1,
                ..WallT::default()
            },
            WallT {
                x: 100,
                y: 0,
                point2: 5,
                nextsector: -1,
                ..WallT::default()
            },
            WallT {
                x: 200,
                y: 0,
                point2: 6,
                nextsector: -1,
                ..WallT::default()
            },
            WallT {
                x: 200,
                y: 100,
                point2: 7,
                nextsector: -1,
                ..WallT::default()
            },
            WallT {
                x: 100,
                y: 100,
                point2: 4,
                nextsector: 0,
                ..WallT::default()
            },
        ];
        let w = BuildWorld::load_arrays(sectors, walls, &[], [50, 50, 0], 0, 0, 1).unwrap();
        let v = w.build_view(50, 50, 0, 0, 100, 0);
        assert_eq!(v.sect, 0);
        assert!(v.visible_sectors.contains(&0));
        assert!(v.visible_sectors.contains(&1));
    }
}
