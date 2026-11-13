//! Build `.MAP` files, versions 7 (Duke Nukem 3D) and 8 (EDuke32 limits):
//! header (start position), then sectors (40 bytes), walls (32 bytes) and
//! sprites (44 bytes), all little-endian.

use crate::error::BuildError;
use serde::{Deserialize, Serialize};

/// Build sector.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[allow(missing_docs)]
pub struct Sector {
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

/// Build wall (one directed edge of a sector loop).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[allow(missing_docs)]
pub struct Wall {
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

/// Build sprite (actors, items, decorations, effectors).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[allow(missing_docs)]
pub struct Sprite {
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

/// A parsed map.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BuildMap {
    /// File version (7 or 8).
    pub version: i32,
    /// Player start x, y, z.
    pub start: [i32; 3],
    /// Player start angle (0–2047, 0 = east, clockwise on screen).
    pub start_ang: i16,
    /// Player start sector.
    pub start_sector: i16,
    /// Sectors.
    pub sectors: Vec<Sector>,
    /// Walls.
    pub walls: Vec<Wall>,
    /// Sprites.
    pub sprites: Vec<Sprite>,
}

struct Rd<'a> {
    data: &'a [u8],
    pos: usize,
}

impl Rd<'_> {
    fn take<const N: usize>(&mut self) -> Result<[u8; N], BuildError> {
        let b = self
            .data
            .get(self.pos..self.pos + N)
            .ok_or(BuildError::Truncated("MAP"))?;
        self.pos += N;
        let mut out = [0u8; N];
        out.copy_from_slice(b);
        Ok(out)
    }
    fn i32(&mut self) -> Result<i32, BuildError> {
        self.take::<4>().map(i32::from_le_bytes)
    }
    fn i16(&mut self) -> Result<i16, BuildError> {
        self.take::<2>().map(i16::from_le_bytes)
    }
    fn u16(&mut self) -> Result<u16, BuildError> {
        self.take::<2>().map(u16::from_le_bytes)
    }
    fn u8(&mut self) -> Result<u8, BuildError> {
        self.take::<1>().map(|b| b[0])
    }
    fn i8(&mut self) -> Result<i8, BuildError> {
        self.take::<1>().map(i8::from_le_bytes)
    }
    fn count(&mut self, size: usize) -> Result<usize, BuildError> {
        let n = usize::from(self.u16()?);
        if n * size > self.data.len().saturating_sub(self.pos) {
            return Err(BuildError::Truncated("MAP record table"));
        }
        Ok(n)
    }
}

/// Whether `data` is a Build `.MAP` this crate can read.
#[must_use]
pub fn is_build_map(data: &[u8]) -> bool {
    parse_map(data).is_ok()
}

/// Parse a version 7 / 8 `.MAP` file.
pub fn parse_map(data: &[u8]) -> Result<BuildMap, BuildError> {
    let mut r = Rd { data, pos: 0 };
    let version = r.i32()?;
    if !(7..=8).contains(&version) {
        return Err(BuildError::Invalid(
            "MAP version (only 7 and 8 are supported)",
        ));
    }
    let start = [r.i32()?, r.i32()?, r.i32()?];
    let start_ang = r.i16()?;
    let start_sector = r.i16()?;

    let n = r.count(40)?;
    let mut sectors = Vec::with_capacity(n);
    for _ in 0..n {
        sectors.push(Sector {
            wallptr: r.i16()?,
            wallnum: r.i16()?,
            ceilingz: r.i32()?,
            floorz: r.i32()?,
            ceilingstat: r.u16()?,
            floorstat: r.u16()?,
            ceilingpicnum: r.i16()?,
            ceilingheinum: r.i16()?,
            ceilingshade: r.i8()?,
            ceilingpal: r.u8()?,
            ceilingxpanning: r.u8()?,
            ceilingypanning: r.u8()?,
            floorpicnum: r.i16()?,
            floorheinum: r.i16()?,
            floorshade: r.i8()?,
            floorpal: r.u8()?,
            floorxpanning: r.u8()?,
            floorypanning: r.u8()?,
            visibility: r.u8()?,
            lotag: {
                let _filler = r.u8()?;
                r.i16()?
            },
            hitag: r.i16()?,
            extra: r.i16()?,
        });
    }

    let n = r.count(32)?;
    let mut walls = Vec::with_capacity(n);
    for _ in 0..n {
        walls.push(Wall {
            x: r.i32()?,
            y: r.i32()?,
            point2: r.i16()?,
            nextwall: r.i16()?,
            nextsector: r.i16()?,
            cstat: r.u16()?,
            picnum: r.i16()?,
            overpicnum: r.i16()?,
            shade: r.i8()?,
            pal: r.u8()?,
            xrepeat: r.u8()?,
            yrepeat: r.u8()?,
            xpanning: r.u8()?,
            ypanning: r.u8()?,
            lotag: r.i16()?,
            hitag: r.i16()?,
            extra: r.i16()?,
        });
    }

    let n = r.count(44)?;
    let mut sprites = Vec::with_capacity(n);
    for _ in 0..n {
        sprites.push(Sprite {
            x: r.i32()?,
            y: r.i32()?,
            z: r.i32()?,
            cstat: r.u16()?,
            picnum: r.i16()?,
            shade: r.i8()?,
            pal: r.u8()?,
            clipdist: r.u8()?,
            xrepeat: {
                let _filler = r.u8()?;
                r.u8()?
            },
            yrepeat: r.u8()?,
            xoffset: r.i8()?,
            yoffset: r.i8()?,
            sectnum: r.i16()?,
            statnum: r.i16()?,
            ang: r.i16()?,
            owner: r.i16()?,
            xvel: r.i16()?,
            yvel: r.i16()?,
            zvel: r.i16()?,
            lotag: r.i16()?,
            hitag: r.i16()?,
            extra: r.i16()?,
        });
    }

    Ok(BuildMap {
        version,
        start,
        start_ang,
        start_sector,
        sectors,
        walls,
        sprites,
    })
}

impl BuildMap {
    /// Sector owning each wall (`None` when no sector lists it).
    #[must_use]
    pub fn wall_sectors(&self) -> Vec<Option<usize>> {
        let mut out = vec![None; self.walls.len()];
        for (si, s) in self.sectors.iter().enumerate() {
            let start = usize::try_from(s.wallptr).unwrap_or(usize::MAX);
            let num = usize::try_from(s.wallnum).unwrap_or(0);
            for w in start..start.saturating_add(num) {
                if let Some(slot) = out.get_mut(w) {
                    *slot = Some(si);
                }
            }
        }
        out
    }

    /// Serialise back to version 7 bytes (tests, tools).
    #[must_use]
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut o = Vec::new();
        let version: i32 = if self.version == 8 { 8 } else { 7 };
        o.extend_from_slice(&version.to_le_bytes());
        for v in self.start {
            o.extend_from_slice(&v.to_le_bytes());
        }
        o.extend_from_slice(&self.start_ang.to_le_bytes());
        o.extend_from_slice(&self.start_sector.to_le_bytes());
        #[allow(clippy::cast_possible_truncation)]
        o.extend_from_slice(&(self.sectors.len() as u16).to_le_bytes());
        for s in &self.sectors {
            o.extend_from_slice(&s.wallptr.to_le_bytes());
            o.extend_from_slice(&s.wallnum.to_le_bytes());
            o.extend_from_slice(&s.ceilingz.to_le_bytes());
            o.extend_from_slice(&s.floorz.to_le_bytes());
            o.extend_from_slice(&s.ceilingstat.to_le_bytes());
            o.extend_from_slice(&s.floorstat.to_le_bytes());
            o.extend_from_slice(&s.ceilingpicnum.to_le_bytes());
            o.extend_from_slice(&s.ceilingheinum.to_le_bytes());
            o.extend_from_slice(&s.ceilingshade.to_le_bytes());
            o.extend_from_slice(&[s.ceilingpal, s.ceilingxpanning, s.ceilingypanning]);
            o.extend_from_slice(&s.floorpicnum.to_le_bytes());
            o.extend_from_slice(&s.floorheinum.to_le_bytes());
            o.extend_from_slice(&s.floorshade.to_le_bytes());
            o.extend_from_slice(&[
                s.floorpal,
                s.floorxpanning,
                s.floorypanning,
                s.visibility,
                0,
            ]);
            o.extend_from_slice(&s.lotag.to_le_bytes());
            o.extend_from_slice(&s.hitag.to_le_bytes());
            o.extend_from_slice(&s.extra.to_le_bytes());
        }
        #[allow(clippy::cast_possible_truncation)]
        o.extend_from_slice(&(self.walls.len() as u16).to_le_bytes());
        for w in &self.walls {
            o.extend_from_slice(&w.x.to_le_bytes());
            o.extend_from_slice(&w.y.to_le_bytes());
            o.extend_from_slice(&w.point2.to_le_bytes());
            o.extend_from_slice(&w.nextwall.to_le_bytes());
            o.extend_from_slice(&w.nextsector.to_le_bytes());
            o.extend_from_slice(&w.cstat.to_le_bytes());
            o.extend_from_slice(&w.picnum.to_le_bytes());
            o.extend_from_slice(&w.overpicnum.to_le_bytes());
            o.extend_from_slice(&w.shade.to_le_bytes());
            o.extend_from_slice(&[w.pal, w.xrepeat, w.yrepeat, w.xpanning, w.ypanning]);
            o.extend_from_slice(&w.lotag.to_le_bytes());
            o.extend_from_slice(&w.hitag.to_le_bytes());
            o.extend_from_slice(&w.extra.to_le_bytes());
        }
        #[allow(clippy::cast_possible_truncation)]
        o.extend_from_slice(&(self.sprites.len() as u16).to_le_bytes());
        for s in &self.sprites {
            for v in [s.x, s.y, s.z] {
                o.extend_from_slice(&v.to_le_bytes());
            }
            o.extend_from_slice(&s.cstat.to_le_bytes());
            o.extend_from_slice(&s.picnum.to_le_bytes());
            o.extend_from_slice(&s.shade.to_le_bytes());
            o.extend_from_slice(&[s.pal, s.clipdist, 0, s.xrepeat, s.yrepeat]);
            o.extend_from_slice(&s.xoffset.to_le_bytes());
            o.extend_from_slice(&s.yoffset.to_le_bytes());
            for v in [
                s.sectnum, s.statnum, s.ang, s.owner, s.xvel, s.yvel, s.zvel, s.lotag, s.hitag,
                s.extra,
            ] {
                o.extend_from_slice(&v.to_le_bytes());
            }
        }
        o
    }
}

/// One square sector, 1024 units wide, for tests.
#[cfg(test)]
pub(crate) fn square_map() -> BuildMap {
    let pts = [(0, 0), (1024, 0), (1024, 1024), (0, 1024)];
    let walls = pts
        .iter()
        .enumerate()
        .map(|(i, &(x, y))| Wall {
            x,
            y,
            point2: ((i + 1) % 4) as i16,
            nextwall: -1,
            nextsector: -1,
            picnum: 1,
            xrepeat: 8,
            yrepeat: 8,
            ..Wall::default()
        })
        .collect();
    BuildMap {
        version: 7,
        start: [512, 512, 0],
        start_ang: 512,
        start_sector: 0,
        sectors: vec![Sector {
            wallptr: 0,
            wallnum: 4,
            ceilingz: -16384,
            floorz: 0,
            ceilingpicnum: 2,
            floorpicnum: 3,
            ..Sector::default()
        }],
        walls,
        sprites: vec![Sprite {
            x: 100,
            y: 100,
            picnum: 1405,
            sectnum: 0,
            ..Sprite::default()
        }],
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]
    use super::*;

    #[test]
    fn roundtrips_a_map() {
        let m = square_map();
        let bytes = m.to_bytes();
        assert_eq!(bytes.len(), 4 + 12 + 4 + 2 + 40 + 2 + 4 * 32 + 2 + 44);
        assert_eq!(parse_map(&bytes).unwrap(), m);
        assert_eq!(m.wall_sectors(), vec![Some(0); 4]);
    }

    #[test]
    fn truncated_or_wrong_version_is_an_error() {
        let bytes = square_map().to_bytes();
        for cut in [0, 10, 30, 100, bytes.len() - 1] {
            assert!(parse_map(&bytes[..cut]).is_err(), "cut {cut}");
        }
        let mut v = bytes.clone();
        v[0] = 6;
        assert!(parse_map(&v).is_err());
    }
}
