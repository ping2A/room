//! Blockmap load and rebuild.

use crate::error::MapError;
use crate::geom::{Line, Vertex};
use ferrite_core::LineId;

/// 128×128 collision grid.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Blockmap {
    /// Origin X (map units).
    pub origin_x: i16,
    /// Origin Y.
    pub origin_y: i16,
    /// Column count.
    pub columns: u16,
    /// Row count.
    pub rows: u16,
    /// Per-block line lists (indices into `lines` of the map).
    pub blocks: Vec<Vec<LineId>>,
}

/// Parse a Doom `BLOCKMAP` lump. Returns `None` if truncated/malformed (caller rebuilds).
pub fn parse_blockmap(data: &[u8], num_lines: usize) -> Result<Option<Blockmap>, MapError> {
    if data.len() < 8 {
        return Ok(None);
    }
    let origin_x = i16::from_le_bytes([data[0], data[1]]);
    let origin_y = i16::from_le_bytes([data[2], data[3]]);
    let columns = u16::from_le_bytes([data[4], data[5]]);
    let rows = u16::from_le_bytes([data[6], data[7]]);
    let nblocks = usize::from(columns).saturating_mul(usize::from(rows));
    if nblocks == 0 || data.len() < 8 + nblocks * 2 {
        return Ok(None);
    }
    let words: Vec<u16> = data
        .chunks_exact(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .collect();
    if words.len() < 4 + nblocks {
        return Ok(None);
    }
    let mut blocks = Vec::with_capacity(nblocks);
    for i in 0..nblocks {
        let off = words[4 + i] as usize;
        if off >= words.len() {
            return Ok(None);
        }
        let mut list = Vec::new();
        let mut j = off;
        if words.get(j) == Some(&0) {
            j += 1;
        }
        loop {
            let Some(&w) = words.get(j) else {
                return Ok(None);
            };
            j += 1;
            if w == 0xFFFF {
                break;
            }
            if usize::from(w) >= num_lines {
                return Ok(None);
            }
            list.push(LineId::from_raw(u32::from(w)));
        }
        blocks.push(list);
    }
    Ok(Some(Blockmap {
        origin_x,
        origin_y,
        columns,
        rows,
        blocks,
    }))
}

const BLOCK_SIZE: i32 = 128;

/// Rebuild a blockmap from linedefs (simple AABB coverage).
pub fn build_blockmap(vertices: &[Vertex], lines: &[Line]) -> Blockmap {
    if lines.is_empty() || vertices.is_empty() {
        return Blockmap::default();
    }
    let mut min_x = i32::MAX;
    let mut min_y = i32::MAX;
    let mut max_x = i32::MIN;
    let mut max_y = i32::MIN;
    for v in vertices {
        let x = v.pos.x() as i32;
        let y = v.pos.y() as i32;
        min_x = min_x.min(x);
        min_y = min_y.min(y);
        max_x = max_x.max(x);
        max_y = max_y.max(y);
    }
    min_x -= BLOCK_SIZE;
    min_y -= BLOCK_SIZE;
    max_x += BLOCK_SIZE;
    max_y += BLOCK_SIZE;
    let columns = (((max_x - min_x) / BLOCK_SIZE) + 1).max(1) as u16;
    let rows = (((max_y - min_y) / BLOCK_SIZE) + 1).max(1) as u16;
    let n = usize::from(columns) * usize::from(rows);
    let mut blocks = vec![Vec::new(); n];

    for (li, line) in lines.iter().enumerate() {
        let Some(v1) = vertices.get(line.v1.raw() as usize) else {
            continue;
        };
        let Some(v2) = vertices.get(line.v2.raw() as usize) else {
            continue;
        };
        let x0 = v1.pos.x().min(v2.pos.x()) as i32;
        let x1 = v1.pos.x().max(v2.pos.x()) as i32;
        let y0 = v1.pos.y().min(v2.pos.y()) as i32;
        let y1 = v1.pos.y().max(v2.pos.y()) as i32;
        let max_bx = i32::from(columns.saturating_sub(1));
        let max_by = i32::from(rows.saturating_sub(1));
        let bx0 = ((x0 - min_x) / BLOCK_SIZE).max(0).min(max_bx) as u16;
        let bx1 = ((x1 - min_x) / BLOCK_SIZE).max(0).min(max_bx) as u16;
        let by0 = ((y0 - min_y) / BLOCK_SIZE).max(0).min(max_by) as u16;
        let by1 = ((y1 - min_y) / BLOCK_SIZE).max(0).min(max_by) as u16;
        let id = LineId::from_raw(li as u32);
        for by in by0..=by1 {
            for bx in bx0..=bx1 {
                let idx = usize::from(by) * usize::from(columns) + usize::from(bx);
                if let Some(cell) = blocks.get_mut(idx) {
                    cell.push(id);
                }
            }
        }
    }

    Blockmap {
        origin_x: min_x as i16,
        origin_y: min_y as i16,
        columns,
        rows,
        blocks,
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use crate::geom::Vertex;
    use ferrite_core::{DVec2, VertexId};

    #[test]
    fn build_covers_line() {
        let verts = vec![
            Vertex {
                pos: DVec2::new(0.0, 0.0),
            },
            Vertex {
                pos: DVec2::new(200.0, 0.0),
            },
        ];
        let lines = vec![Line {
            v1: VertexId::from_raw(0),
            v2: VertexId::from_raw(1),
            delta: DVec2::new(200.0, 0.0),
            sides: [None, None],
            front_sector: None,
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
        }];
        let bm = build_blockmap(&verts, &lines);
        assert!(bm.columns >= 1 && bm.rows >= 1);
        assert!(bm.blocks.iter().any(|b| !b.is_empty()));
    }
}
