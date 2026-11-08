//! Runtime sector / wall / sprite mirrors of Build map v7 fields.
//! Spec: `docs/build-engine-primitives/01-world.md`.

/// Build sector (playsim copy).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SectorT {
    pub wallptr: i16,
    pub wallnum: i16,
    pub ceilingz: i32,
    pub floorz: i32,
    pub ceilingstat: u16,
    pub floorstat: u16,
    pub ceilingpicnum: i16,
    pub ceilingheinum: i16,
    pub ceilingshade: i8,
    pub ceilingpal: u8,
    pub ceilingxpanning: u8,
    pub ceilingypanning: u8,
    pub floorpicnum: i16,
    pub floorheinum: i16,
    pub floorshade: i8,
    pub floorpal: u8,
    pub floorxpanning: u8,
    pub floorypanning: u8,
    pub visibility: u8,
    pub lotag: i16,
    pub hitag: i16,
    pub extra: i16,
}

/// Build wall (playsim copy).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct WallT {
    pub x: i32,
    pub y: i32,
    pub point2: i16,
    pub nextwall: i16,
    pub nextsector: i16,
    pub cstat: u16,
    pub picnum: i16,
    pub overpicnum: i16,
    pub shade: i8,
    pub pal: u8,
    pub xrepeat: u8,
    pub yrepeat: u8,
    pub xpanning: u8,
    pub ypanning: u8,
    pub lotag: i16,
    pub hitag: i16,
    pub extra: i16,
}

/// Build sprite (playsim copy).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SpriteT {
    pub x: i32,
    pub y: i32,
    pub z: i32,
    pub cstat: u16,
    pub picnum: i16,
    pub shade: i8,
    pub pal: u8,
    pub clipdist: u8,
    pub xrepeat: u8,
    pub yrepeat: u8,
    pub xoffset: i8,
    pub yoffset: i8,
    pub sectnum: i16,
    pub statnum: i16,
    pub ang: i16,
    pub owner: i16,
    pub xvel: i16,
    pub yvel: i16,
    pub zvel: i16,
    pub lotag: i16,
    pub hitag: i16,
    pub extra: i16,
}

impl WallT {
    pub fn xy(&self) -> crate::geom::WallXY {
        crate::geom::WallXY {
            x: self.x,
            y: self.y,
            point2: self.point2,
            nextsector: self.nextsector,
            cstat: self.cstat,
        }
    }
}

impl SectorT {
    pub fn walls(&self) -> crate::geom::SectorWalls {
        crate::geom::SectorWalls {
            wallptr: self.wallptr,
            wallnum: self.wallnum,
        }
    }

    pub fn z(&self) -> crate::geom::SectorZ {
        crate::geom::SectorZ {
            wallptr: self.wallptr,
            wallnum: self.wallnum,
            ceilingz: self.ceilingz,
            floorz: self.floorz,
            ceilingstat: self.ceilingstat,
            floorstat: self.floorstat,
            ceilingheinum: self.ceilingheinum,
            floorheinum: self.floorheinum,
        }
    }
}
