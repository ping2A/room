//! Noclip free-fly camera for the D3 soft viewer.
//! Spec: `docs/25` §11 D3 — free-fly camera (no gravity / clip).

use crate::helpers::approx_sincos_q14;
use crate::view::BuildView;
use crate::world::BuildWorld;

/// Free-fly pose (Build coords / angle).
#[derive(Clone, Debug)]
pub struct FreeFly {
    pub posx: i32,
    pub posy: i32,
    pub posz: i32,
    pub ang: i16,
    pub horiz: i16,
    pub sect: i16,
}

/// One-step free-fly input (caller scales per tic / frame).
#[derive(Clone, Copy, Debug, Default)]
pub struct FreeFlyCmd {
    /// Forward (+) / back (−) in map units.
    pub forward: i32,
    /// Strafe right (+) / left (−) in map units.
    pub strafe: i32,
    /// Up (−Z in Build) / down (+Z). Build Z grows downward.
    pub up: i32,
    /// Turn left (−) / right (+) in Build angle units (0..2047).
    pub turn: i16,
    /// Look up (+) / down (−); added to `horiz` (eye-level ≈ 100).
    pub look: i16,
}

impl FreeFly {
    pub fn new(posx: i32, posy: i32, posz: i32, ang: i16, horiz: i16, sect: i16) -> Self {
        Self {
            posx,
            posy,
            posz,
            ang,
            horiz,
            sect,
        }
    }

    pub fn from_view(v: &BuildView) -> Self {
        Self::new(v.posx, v.posy, v.posz, v.ang, v.horiz, v.sect)
    }

    /// Apply `cmd` (noclip) and refresh `sect` via `updatesector`.
    pub fn apply(&mut self, world: &BuildWorld, cmd: FreeFlyCmd) {
        self.ang = (self.ang.wrapping_add(cmd.turn)) & 2047;
        self.horiz = self.horiz.saturating_add(cmd.look).clamp(0, 200);
        // Build Z: negative is up.
        self.posz = self.posz.wrapping_sub(cmd.up);

        if cmd.forward != 0 || cmd.strafe != 0 {
            let (sin, cos) = approx_sincos_q14(i32::from(self.ang));
            let sin = i64::from(sin);
            let cos = i64::from(cos);
            // Forward along (cos, sin); strafe along (sin, -cos) in Build XY.
            let fx = (i64::from(cmd.forward) * cos) >> 14;
            let fy = (i64::from(cmd.forward) * sin) >> 14;
            let sx = (i64::from(cmd.strafe) * sin) >> 14;
            let sy = -((i64::from(cmd.strafe) * cos) >> 14);
            self.posx = self.posx.wrapping_add((fx + sx) as i32);
            self.posy = self.posy.wrapping_add((fy + sy) as i32);
        }

        let walls = world.wall_xy();
        let sectors = world.sector_walls();
        crate::geom::updatesector(self.posx, self.posy, &mut self.sect, &sectors, &walls);
    }

    pub fn view(&self, world: &BuildWorld) -> BuildView {
        world.build_view(
            self.posx, self.posy, self.posz, self.ang, self.horiz, self.sect,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{SectorT, WallT};

    fn square_world() -> BuildWorld {
        let sectors = vec![SectorT {
            wallptr: 0,
            wallnum: 4,
            ceilingz: -8192,
            floorz: 0,
            ..SectorT::default()
        }];
        let walls = vec![
            WallT {
                x: 0,
                y: 0,
                point2: 1,
                nextsector: -1,
                ..WallT::default()
            },
            WallT {
                x: 512,
                y: 0,
                point2: 2,
                nextsector: -1,
                ..WallT::default()
            },
            WallT {
                x: 512,
                y: 512,
                point2: 3,
                nextsector: -1,
                ..WallT::default()
            },
            WallT {
                x: 0,
                y: 512,
                point2: 0,
                nextsector: -1,
                ..WallT::default()
            },
        ];
        BuildWorld::load_arrays(sectors, walls, &[], [256, 256, -1024], 512, 100, 0).unwrap()
    }

    #[test]
    fn freefly_moves_forward_and_updates_sect() {
        let w = square_world();
        let mut fly = FreeFly::new(256, 256, -1024, 512, 100, 0);
        fly.apply(
            &w,
            FreeFlyCmd {
                forward: 64,
                ..FreeFlyCmd::default()
            },
        );
        assert!(fly.posx != 256 || fly.posy != 256);
        assert_eq!(fly.sect, 0);
        let v = fly.view(&w);
        assert_eq!(v.sect, 0);
    }

    #[test]
    fn freefly_turn_look_clamp() {
        let w = square_world();
        let mut fly = FreeFly::new(256, 256, -1024, 0, 100, 0);
        fly.apply(
            &w,
            FreeFlyCmd {
                turn: 100,
                look: 50,
                up: 256,
                ..FreeFlyCmd::default()
            },
        );
        assert_eq!(fly.ang, 100);
        assert_eq!(fly.horiz, 150);
        assert_eq!(fly.posz, -1024 - 256);
    }
}
