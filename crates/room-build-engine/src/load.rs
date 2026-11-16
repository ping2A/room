//! Load a Build map into [`BuildWorld`].
//! Spec: `docs/25-duke3d-full-support.md` §5.2 / D3.

use crate::types::{SectorT, SpriteT, WallT};
use crate::world::{BuildWorld, MAXSECTORS, MAXSPRITES, MAXWALLS, SpriteLists};

/// Errors loading a map into the engine world.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LoadError {
    TooManySectors(usize),
    TooManyWalls(usize),
    TooManySprites(usize),
    BadSpriteSlot(i16),
    Parse(String),
}

impl std::fmt::Display for LoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TooManySectors(n) => write!(f, "too many sectors ({n} > {MAXSECTORS})"),
            Self::TooManyWalls(n) => write!(f, "too many walls ({n} > {MAXWALLS})"),
            Self::TooManySprites(n) => write!(f, "too many sprites ({n} > {MAXSPRITES})"),
            Self::BadSpriteSlot(i) => write!(f, "could not place sprite {i}"),
            Self::Parse(s) => write!(f, "map parse: {s}"),
        }
    }
}

impl std::error::Error for LoadError {}

impl BuildWorld {
    /// Populate from already-decoded map arrays (no format crate needed).
    pub fn load_arrays(
        sectors: Vec<SectorT>,
        walls: Vec<WallT>,
        sprites: &[SpriteT],
        start: [i32; 3],
        start_ang: i16,
        start_sector: i16,
        randomseed: i32,
    ) -> Result<Self, LoadError> {
        if sectors.len() > MAXSECTORS {
            return Err(LoadError::TooManySectors(sectors.len()));
        }
        if walls.len() > MAXWALLS {
            return Err(LoadError::TooManyWalls(walls.len()));
        }
        if sprites.len() > MAXSPRITES {
            return Err(LoadError::TooManySprites(sprites.len()));
        }
        let mut lists = SpriteLists::new();
        let mut sprite_slot = [SpriteT::default(); MAXSPRITES];
        for (i, sp) in sprites.iter().enumerate() {
            let i16 = i as i16;
            if !lists.place_sprite_at(i16, sp.sectnum, sp.statnum) {
                return Err(LoadError::BadSpriteSlot(i16));
            }
            sprite_slot[i] = *sp;
        }
        Ok(Self {
            numsectors: sectors.len() as i16,
            numwalls: walls.len() as i16,
            sectors,
            walls,
            sprites: Box::new(sprite_slot),
            lists,
            randomseed,
            start,
            start_ang,
            start_sector,
            mirror_viewer: None,
            render_sprite_blends: Default::default(),
            mirror_render_sprites: Default::default(),
            mirror_render_sprite_blends: Default::default(),
            render_hud_draws: Default::default(),
            render_hide_native_weapon: false,
            render_hide_native_crosshair: false,
        })
    }

    /// Parse a Build `.MAP` (v7/v8) via `room-build` and load it.
    pub fn load_map_bytes(data: &[u8]) -> Result<Self, LoadError> {
        let map = room_build::map::parse_map(data).map_err(|e| LoadError::Parse(e.to_string()))?;
        let sectors: Vec<SectorT> = map
            .sectors
            .iter()
            .map(|s| SectorT {
                wallptr: s.wallptr,
                wallnum: s.wallnum,
                ceilingz: s.ceilingz,
                floorz: s.floorz,
                ceilingstat: s.ceilingstat,
                floorstat: s.floorstat,
                ceilingpicnum: s.ceilingpicnum,
                ceilingheinum: s.ceilingheinum,
                ceilingshade: s.ceilingshade,
                ceilingpal: s.ceilingpal,
                ceilingxpanning: s.ceilingxpanning,
                ceilingypanning: s.ceilingypanning,
                floorpicnum: s.floorpicnum,
                floorheinum: s.floorheinum,
                floorshade: s.floorshade,
                floorpal: s.floorpal,
                floorxpanning: s.floorxpanning,
                floorypanning: s.floorypanning,
                visibility: s.visibility,
                lotag: s.lotag,
                hitag: s.hitag,
                extra: s.extra,
            })
            .collect();
        let walls: Vec<WallT> = map
            .walls
            .iter()
            .map(|w| WallT {
                x: w.x,
                y: w.y,
                point2: w.point2,
                nextwall: w.nextwall,
                nextsector: w.nextsector,
                cstat: w.cstat,
                picnum: w.picnum,
                overpicnum: w.overpicnum,
                shade: w.shade,
                pal: w.pal,
                xrepeat: w.xrepeat,
                yrepeat: w.yrepeat,
                xpanning: w.xpanning,
                ypanning: w.ypanning,
                lotag: w.lotag,
                hitag: w.hitag,
                extra: w.extra,
            })
            .collect();
        let sprites: Vec<SpriteT> = map
            .sprites
            .iter()
            .map(|s| SpriteT {
                x: s.x,
                y: s.y,
                z: s.z,
                cstat: s.cstat,
                picnum: s.picnum,
                shade: s.shade,
                pal: s.pal,
                clipdist: s.clipdist,
                xrepeat: s.xrepeat,
                yrepeat: s.yrepeat,
                xoffset: s.xoffset,
                yoffset: s.yoffset,
                sectnum: s.sectnum,
                statnum: s.statnum,
                ang: s.ang,
                owner: s.owner,
                xvel: s.xvel,
                yvel: s.yvel,
                zvel: s.zvel,
                lotag: s.lotag,
                hitag: s.hitag,
                extra: s.extra,
            })
            .collect();
        Self::load_arrays(
            sectors,
            walls,
            &sprites,
            map.start,
            map.start_ang,
            map.start_sector,
            1,
        )
    }

    pub fn wall_xy(&self) -> Vec<crate::geom::WallXY> {
        self.walls.iter().map(WallT::xy).collect()
    }

    pub fn sector_walls(&self) -> Vec<crate::geom::SectorWalls> {
        self.sectors.iter().map(SectorT::walls).collect()
    }

    pub fn sector_z(&self) -> Vec<crate::geom::SectorZ> {
        self.sectors.iter().map(SectorT::z).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inside;

    #[test]
    fn load_arrays_square_and_inside() {
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
                x: 100,
                y: 0,
                point2: 2,
                nextsector: -1,
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
        ];
        let sprites = [SpriteT {
            x: 50,
            y: 50,
            z: 0,
            sectnum: 0,
            statnum: 0,
            ..SpriteT::default()
        }];
        let w = BuildWorld::load_arrays(sectors, walls, &sprites, [50, 50, 0], 0, 0, 1).unwrap();
        assert_eq!(w.numsectors, 1);
        assert_eq!(w.numwalls, 4);
        assert_eq!(w.lists.sprite_sect[0], 0);
        assert!(inside(50, 50, 0, &w.sector_walls(), &w.wall_xy()));
    }
}
