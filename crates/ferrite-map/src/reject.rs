//! REJECT table load / build / query.

use crate::error::MapError;
use crate::geom::Line;
use ferrite_core::SectorId;

/// Sector visibility bit matrix (`num_sectors²` bits, row-major).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Reject {
    /// Sector count the table was built for.
    pub num_sectors: usize,
    /// Packed bits; bit `(row * num_sectors + col)` set ⇒ **cannot** see.
    pub bits: Vec<u8>,
}

impl Reject {
    /// True if sector `from` cannot see sector `to` (vanilla REJECT sense).
    #[must_use]
    pub fn rejects(&self, from: usize, to: usize) -> bool {
        if from >= self.num_sectors || to >= self.num_sectors || self.bits.is_empty() {
            return false;
        }
        let bit = from * self.num_sectors + to;
        let byte = bit / 8;
        let mask = 1u8 << (bit % 8);
        self.bits.get(byte).is_some_and(|b| b & mask != 0)
    }

    /// Serialize packed bits for a REJECT lump.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.bits
    }
}

/// Parse a REJECT lump for `num_sectors` sectors.
///
/// Short tables are zero-padded (visible); oversized trailing bytes ignored.
pub fn parse_reject(data: &[u8], num_sectors: usize) -> Result<Reject, MapError> {
    if num_sectors == 0 {
        return Ok(Reject::default());
    }
    let need_bits = num_sectors
        .checked_mul(num_sectors)
        .ok_or(MapError::Invalid("REJECT size"))?;
    let need_bytes = need_bits.div_ceil(8);
    let mut bits = vec![0u8; need_bytes];
    let copy = data.len().min(need_bytes);
    bits[..copy].copy_from_slice(&data[..copy]);
    Ok(Reject { num_sectors, bits })
}

/// All-clear REJECT (every sector pair may be visible). Safe for any map.
#[must_use]
pub fn build_reject_zero(num_sectors: usize) -> Reject {
    if num_sectors == 0 {
        return Reject::default();
    }
    let need_bytes = (num_sectors * num_sectors).div_ceil(8);
    Reject {
        num_sectors,
        bits: vec![0u8; need_bytes],
    }
}

/// Conservative REJECT from two-sided linedef connectivity (sector flood groups).
///
/// Sectors joined by any two-sided line share a group and may see each other
/// (bits left clear). Pairs in different groups reject both ways. This never
/// marks a false reject for same-group pairs; it is weaker than a full LOS
/// builder but correct as an early-out.
#[must_use]
pub fn build_reject(lines: &[Line], num_sectors: usize) -> Reject {
    if num_sectors == 0 {
        return Reject::default();
    }
    let mut parent: Vec<usize> = (0..num_sectors).collect();
    let mut rank = vec![0u8; num_sectors];

    for line in lines {
        let (Some(SectorId(a)), Some(SectorId(b))) = (line.front_sector, line.back_sector) else {
            continue;
        };
        let (a, b) = (a as usize, b as usize);
        if a >= num_sectors || b >= num_sectors || a == b {
            continue;
        }
        union(&mut parent, &mut rank, a, b);
    }

    let root_of: Vec<usize> = (0..num_sectors).map(|i| find(&mut parent, i)).collect();

    let mut bits = vec![0u8; (num_sectors * num_sectors).div_ceil(8)];
    for from in 0..num_sectors {
        for to in 0..num_sectors {
            if root_of[from] != root_of[to] {
                set_bit(&mut bits, from * num_sectors + to);
            }
        }
    }
    Reject { num_sectors, bits }
}

fn find(parent: &mut [usize], mut i: usize) -> usize {
    while parent[i] != i {
        parent[i] = parent[parent[i]];
        i = parent[i];
    }
    i
}

fn union(parent: &mut [usize], rank: &mut [u8], a: usize, b: usize) {
    let mut ra = find(parent, a);
    let mut rb = find(parent, b);
    if ra == rb {
        return;
    }
    if rank[ra] < rank[rb] {
        std::mem::swap(&mut ra, &mut rb);
    }
    parent[rb] = ra;
    if rank[ra] == rank[rb] {
        rank[ra] = rank[ra].saturating_add(1);
    }
}

fn set_bit(bits: &mut [u8], bit: usize) {
    let byte = bit / 8;
    let mask = 1u8 << (bit % 8);
    if let Some(b) = bits.get_mut(byte) {
        *b |= mask;
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use crate::geom::Line;
    use ferrite_core::{DVec2, SectorId, VertexId};

    #[test]
    fn rejects_bit() {
        // 2 sectors → 4 bits; set bit 1 (0 cannot see 1)
        let r = parse_reject(&[0b0000_0010], 2).unwrap();
        assert!(!r.rejects(0, 0));
        assert!(r.rejects(0, 1));
        assert!(!r.rejects(1, 0));
    }

    #[test]
    fn zero_reject_never_rejects() {
        let r = build_reject_zero(3);
        assert_eq!(r.as_bytes().len(), 2); // 9 bits → 2 bytes
        for a in 0..3 {
            for b in 0..3 {
                assert!(!r.rejects(a, b));
            }
        }
    }

    #[test]
    fn connectivity_reject_separates_components() {
        // Two isolated 2S pairs: sectors 0↔1 and 2↔3 (no bridge).
        let lines = [
            two_sided(0, 1),
            two_sided(2, 3),
            // one-sided noise
            Line {
                v1: VertexId(0),
                v2: VertexId(1),
                delta: DVec2::new(1.0, 0.0),
                sides: [None, None],
                front_sector: Some(SectorId(0)),
                back_sector: None,
                flags: 0,
                special: 0,
                tag: 0,
                args: [0; 5],
                activation: 0,
                mid_alpha: 1.0,
                mid_additive: false,
                arg0_str: String::new(),
                midtex3d: false,
                midtex3d_impassible: false,
                check_switch_range: false,
                first_side_only: false,
                block_floaters: false,
                lock_number: 0,
                block_projectiles: false,
                block_sight: false,
                block_hitscan: false,
                extra_ids: Default::default(),
                health: 0,
                health_group: 0,
                automap_style: 0,
                clip_midtex: false,
                wrap_midtex: false,
            },
        ];
        let r = build_reject(&lines, 4);
        assert!(!r.rejects(0, 1));
        assert!(!r.rejects(1, 0));
        assert!(!r.rejects(2, 3));
        assert!(r.rejects(0, 2));
        assert!(r.rejects(2, 0));
        assert!(r.rejects(1, 3));
        // Round-trip through parse.
        let r2 = parse_reject(r.as_bytes(), 4).unwrap();
        assert_eq!(r, r2);
    }

    fn two_sided(a: u32, b: u32) -> Line {
        Line {
            v1: VertexId(0),
            v2: VertexId(1),
            delta: DVec2::new(1.0, 0.0),
            sides: [None, None],
            front_sector: Some(SectorId(a)),
            back_sector: Some(SectorId(b)),
            flags: 4, // ML_TWOSIDED
            special: 0,
            tag: 0,
            args: [0; 5],
            activation: 0,
            mid_alpha: 1.0,
            mid_additive: false,
            arg0_str: String::new(),
            midtex3d: false,
            midtex3d_impassible: false,
            check_switch_range: false,
            first_side_only: false,
            block_floaters: false,
            lock_number: 0,
            block_projectiles: false,
            block_sight: false,
            block_hitscan: false,
            extra_ids: Default::default(),
            health: 0,
            health_group: 0,
            automap_style: 0,
            clip_midtex: false,
            wrap_midtex: false,
        }
    }
}
