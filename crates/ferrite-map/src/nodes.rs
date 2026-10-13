//! Doom classic + ZDoom extended (XNOD/ZNOD) node loaders.

use crate::error::MapError;
use ferrite_core::{DVec2, LineId, VertexId};
use flate2::read::ZlibDecoder;
use std::io::Read;

/// Child index with bit 31 set ⇒ subsector (ZDoom extended convention).
/// Classic Doom bit-15 flags are converted to this form on load.
pub const NF_SUBSECTOR: u32 = 0x8000_0000;

/// One BSP seg.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Seg {
    /// Start vertex.
    pub v1: VertexId,
    /// End vertex.
    pub v2: VertexId,
    /// Angle (BAM high 16 bits as stored; 0 if unknown).
    pub angle: i16,
    /// Linedef (`None` for minisegs in GL — Doom segs always have a line).
    pub line: Option<LineId>,
    /// Side (0 front, 1 back).
    pub side: i16,
    /// Distance along line.
    pub offset: i16,
}

/// Subsector: contiguous segs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Subsector {
    /// Number of segs.
    pub num_segs: u32,
    /// First seg index.
    pub first_seg: u32,
}

/// BSP node.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Node {
    /// Partition origin X (map units; from i16 or fixed→float).
    pub x: i16,
    /// Partition origin Y.
    pub y: i16,
    /// Partition delta X.
    pub dx: i16,
    /// Partition delta Y.
    pub dy: i16,
    /// Child bboxes `[right, left]` each `top,bottom,left,right`.
    pub bbox: [[i16; 4]; 2],
    /// Children; [`NF_SUBSECTOR`] set ⇒ subsector index in low 31 bits.
    pub children: [u32; 2],
}

/// Loaded classic / extended BSP tree (non-GL).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GameNodes {
    /// Segs.
    pub segs: Vec<Seg>,
    /// Subsectors.
    pub subsectors: Vec<Subsector>,
    /// Nodes (root is last).
    pub nodes: Vec<Node>,
    /// Extra vertices appended after VERTEXES (16.16 → f64); may be empty.
    pub extra_vertices: Vec<DVec2>,
}

/// One GL BSP seg (closed polygons; `v2` implied by next seg's `v1`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GlSeg {
    /// Start vertex.
    pub v1: VertexId,
    /// Partner seg index (`u32::MAX` = none).
    pub partner: u32,
    /// Linedef (`None` = miniseg).
    pub line: Option<LineId>,
    /// Side (0 front, 1 back).
    pub side: u8,
}

/// Loaded GL BSP tree (XGLN / XGL2 / XGL3).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GlNodes {
    /// GL segs.
    pub segs: Vec<GlSeg>,
    /// Subsectors.
    pub subsectors: Vec<Subsector>,
    /// Nodes (root is last).
    pub nodes: Vec<Node>,
    /// Extra vertices (16.16 → f64).
    pub extra_vertices: Vec<DVec2>,
}

/// Result of parsing an extended nodes lump.
#[derive(Clone, Debug, PartialEq)]
pub enum ExtendedNodes {
    /// Non-GL (XNOD/ZNOD).
    Game(GameNodes),
    /// GL (XGLN/XGL2/XGL3 + Z* compressed).
    Gl(GlNodes),
}

/// Parse Doom `SEGS` (12 bytes).
pub fn parse_segs(data: &[u8]) -> Result<Vec<Seg>, MapError> {
    if data.len() % 12 != 0 {
        return Err(MapError::Truncated("SEGS"));
    }
    let mut out = Vec::with_capacity(data.len() / 12);
    for chunk in data.chunks_exact(12) {
        out.push(Seg {
            v1: VertexId::from_raw(u32::from(u16::from_le_bytes([chunk[0], chunk[1]]))),
            v2: VertexId::from_raw(u32::from(u16::from_le_bytes([chunk[2], chunk[3]]))),
            angle: i16::from_le_bytes([chunk[4], chunk[5]]),
            line: Some(LineId::from_raw(u32::from(u16::from_le_bytes([
                chunk[6], chunk[7],
            ])))),
            side: i16::from_le_bytes([chunk[8], chunk[9]]),
            offset: i16::from_le_bytes([chunk[10], chunk[11]]),
        });
    }
    Ok(out)
}

/// Parse Doom `SSECTORS` (4 bytes).
pub fn parse_ssectors(data: &[u8]) -> Result<Vec<Subsector>, MapError> {
    if data.len() % 4 != 0 {
        return Err(MapError::Truncated("SSECTORS"));
    }
    let mut out = Vec::with_capacity(data.len() / 4);
    for chunk in data.chunks_exact(4) {
        out.push(Subsector {
            num_segs: u32::from(u16::from_le_bytes([chunk[0], chunk[1]])),
            first_seg: u32::from(u16::from_le_bytes([chunk[2], chunk[3]])),
        });
    }
    Ok(out)
}

/// Parse Doom `NODES` (28 bytes); children remapped to [`NF_SUBSECTOR`].
pub fn parse_nodes(data: &[u8]) -> Result<Vec<Node>, MapError> {
    if data.len() % 28 != 0 {
        return Err(MapError::Truncated("NODES"));
    }
    let mut out = Vec::with_capacity(data.len() / 28);
    for chunk in data.chunks_exact(28) {
        let mut bbox = [[0i16; 4]; 2];
        for (c, child_bb) in bbox.iter_mut().enumerate() {
            for (b, slot) in child_bb.iter_mut().enumerate() {
                let o = 8 + c * 8 + b * 2;
                *slot = i16::from_le_bytes([chunk[o], chunk[o + 1]]);
            }
        }
        let c0 = u16::from_le_bytes([chunk[24], chunk[25]]);
        let c1 = u16::from_le_bytes([chunk[26], chunk[27]]);
        out.push(Node {
            x: i16::from_le_bytes([chunk[0], chunk[1]]),
            y: i16::from_le_bytes([chunk[2], chunk[3]]),
            dx: i16::from_le_bytes([chunk[4], chunk[5]]),
            dy: i16::from_le_bytes([chunk[6], chunk[7]]),
            bbox,
            children: [classic_child_to_ext(c0), classic_child_to_ext(c1)],
        });
    }
    Ok(out)
}

fn classic_child_to_ext(c: u16) -> u32 {
    if c & 0x8000 != 0 {
        NF_SUBSECTOR | u32::from(c & 0x7FFF)
    } else {
        u32::from(c)
    }
}

/// Load game nodes from classic SEGS/SSECTORS/NODES, or a single XNOD/ZNOD blob.
pub fn load_game_nodes(
    segs: &[u8],
    ssectors: &[u8],
    nodes: &[u8],
) -> Result<Option<GameNodes>, MapError> {
    if nodes.len() >= 4 {
        if let Some(ext) = load_extended_nodes_lump(nodes)? {
            return Ok(Some(match ext {
                ExtendedNodes::Game(g) => g,
                ExtendedNodes::Gl(gl) => gl_nodes_to_game(&gl),
            }));
        }
    }
    if segs.is_empty() || ssectors.is_empty() || nodes.is_empty() {
        return Ok(None);
    }
    Ok(Some(GameNodes {
        segs: parse_segs(segs)?,
        subsectors: parse_ssectors(ssectors)?,
        nodes: parse_nodes(nodes)?,
        extra_vertices: Vec::new(),
    }))
}

/// Load from a `ZNODES` / `NODES` lump (XNOD/ZNOD/XGLN/XGL2/XGL3 + Z* zlib).
pub fn load_extended_nodes_lump(data: &[u8]) -> Result<Option<ExtendedNodes>, MapError> {
    if data.len() < 4 {
        return Ok(None);
    }
    let magic = &data[..4];
    Ok(Some(match magic {
        b"XNOD" => ExtendedNodes::Game(parse_xnod_body(&data[4..])?),
        b"ZNOD" => ExtendedNodes::Game(parse_xnod_body(&inflate_zlib(&data[4..], "ZNOD")?)?),
        b"XGLN" => ExtendedNodes::Gl(parse_gl_body(&data[4..], GlSegFmt::V1, false)?),
        b"ZGLN" => ExtendedNodes::Gl(parse_gl_body(
            &inflate_zlib(&data[4..], "ZGLN")?,
            GlSegFmt::V1,
            false,
        )?),
        b"XGL2" => ExtendedNodes::Gl(parse_gl_body(&data[4..], GlSegFmt::V2, false)?),
        b"ZGL2" => ExtendedNodes::Gl(parse_gl_body(
            &inflate_zlib(&data[4..], "ZGL2")?,
            GlSegFmt::V2,
            false,
        )?),
        b"XGL3" => ExtendedNodes::Gl(parse_gl_body(&data[4..], GlSegFmt::V2, true)?),
        b"ZGL3" => ExtendedNodes::Gl(parse_gl_body(
            &inflate_zlib(&data[4..], "ZGL3")?,
            GlSegFmt::V2,
            true,
        )?),
        _ => return Ok(None),
    }))
}

#[derive(Clone, Copy)]
enum GlSegFmt {
    /// u16 linedef (`0xFFFF` = none).
    V1,
    /// u32 linedef (`0xFFFF_FFFF` = none).
    V2,
}

fn inflate_zlib(compressed: &[u8], kind: &str) -> Result<Vec<u8>, MapError> {
    let mut dec = ZlibDecoder::new(compressed);
    let mut out = Vec::new();
    dec.read_to_end(&mut out)
        .map_err(|e| MapError::Message(format!("{kind} inflate: {e}")))?;
    Ok(out)
}

/// Convert GL nodes to classic-style segs (v2 = next v1 in the subsector ring).
#[must_use]
pub fn gl_nodes_to_game(gl: &GlNodes) -> GameNodes {
    let mut segs = Vec::with_capacity(gl.segs.len());
    for sub in &gl.subsectors {
        let start = sub.first_seg as usize;
        let n = sub.num_segs as usize;
        if n == 0 {
            continue;
        }
        for i in 0..n {
            let g = gl.segs[start + i];
            let next = gl.segs[start + ((i + 1) % n)];
            segs.push(Seg {
                v1: g.v1,
                v2: next.v1,
                angle: 0,
                line: g.line,
                side: i16::from(g.side),
                offset: 0,
            });
        }
    }
    GameNodes {
        segs,
        subsectors: gl.subsectors.clone(),
        nodes: gl.nodes.clone(),
        extra_vertices: gl.extra_vertices.clone(),
    }
}

/// Parse GL node body (after magic). `fixed_partition` = XGL3/ZGL3.
fn parse_gl_body(
    data: &[u8],
    seg_fmt: GlSegFmt,
    fixed_partition: bool,
) -> Result<GlNodes, MapError> {
    let mut off = 0;
    let _orig_verts = read_u32(data, &mut off)?;
    let new_verts = read_u32(data, &mut off)? as usize;
    let mut extra_vertices = Vec::with_capacity(new_verts);
    for _ in 0..new_verts {
        let x = read_i32(data, &mut off)?;
        let y = read_i32(data, &mut off)?;
        extra_vertices.push(DVec2::new(fixed16_16_to_f64(x), fixed16_16_to_f64(y)));
    }

    let num_subsectors = read_u32(data, &mut off)? as usize;
    let mut subsectors = Vec::with_capacity(num_subsectors);
    let mut first = 0u32;
    for _ in 0..num_subsectors {
        let count = read_u32(data, &mut off)?;
        subsectors.push(Subsector {
            num_segs: count,
            first_seg: first,
        });
        first = first.saturating_add(count);
    }

    let num_segs = read_u32(data, &mut off)? as usize;
    let mut segs = Vec::with_capacity(num_segs);
    for _ in 0..num_segs {
        let v1 = read_u32(data, &mut off)?;
        let partner = read_u32(data, &mut off)?;
        let line = match seg_fmt {
            GlSegFmt::V1 => {
                let l = read_u16(data, &mut off)?;
                if l == 0xFFFF {
                    None
                } else {
                    Some(LineId::from_raw(u32::from(l)))
                }
            }
            GlSegFmt::V2 => {
                let l = read_u32(data, &mut off)?;
                if l == 0xFFFF_FFFF {
                    None
                } else {
                    Some(LineId::from_raw(l))
                }
            }
        };
        let side = read_u8(data, &mut off)?;
        segs.push(GlSeg {
            v1: VertexId::from_raw(v1),
            partner,
            line,
            side,
        });
    }

    let num_nodes = read_u32(data, &mut off)? as usize;
    let mut nodes = Vec::with_capacity(num_nodes);
    for _ in 0..num_nodes {
        let (x, y, dx, dy) = if fixed_partition {
            (
                fixed_to_i16(read_i32(data, &mut off)?),
                fixed_to_i16(read_i32(data, &mut off)?),
                fixed_to_i16(read_i32(data, &mut off)?),
                fixed_to_i16(read_i32(data, &mut off)?),
            )
        } else {
            (
                read_i16(data, &mut off)?,
                read_i16(data, &mut off)?,
                read_i16(data, &mut off)?,
                read_i16(data, &mut off)?,
            )
        };
        let mut bbox = [[0i16; 4]; 2];
        for child_bb in &mut bbox {
            for slot in child_bb.iter_mut() {
                *slot = read_i16(data, &mut off)?;
            }
        }
        let c0 = read_u32(data, &mut off)?;
        let c1 = read_u32(data, &mut off)?;
        nodes.push(Node {
            x,
            y,
            dx,
            dy,
            bbox,
            children: [c0, c1],
        });
    }

    Ok(GlNodes {
        segs,
        subsectors,
        nodes,
        extra_vertices,
    })
}

fn fixed_to_i16(v: i32) -> i16 {
    let f = fixed16_16_to_f64(v).round();
    if f > f64::from(i16::MAX) {
        i16::MAX
    } else if f < f64::from(i16::MIN) {
        i16::MIN
    } else {
        f as i16
    }
}

fn read_u32(data: &[u8], off: &mut usize) -> Result<u32, MapError> {
    if *off + 4 > data.len() {
        return Err(MapError::Truncated("XNOD"));
    }
    let v = u32::from_le_bytes([data[*off], data[*off + 1], data[*off + 2], data[*off + 3]]);
    *off += 4;
    Ok(v)
}

fn read_u16(data: &[u8], off: &mut usize) -> Result<u16, MapError> {
    if *off + 2 > data.len() {
        return Err(MapError::Truncated("XNOD"));
    }
    let v = u16::from_le_bytes([data[*off], data[*off + 1]]);
    *off += 2;
    Ok(v)
}

fn read_u8(data: &[u8], off: &mut usize) -> Result<u8, MapError> {
    if *off >= data.len() {
        return Err(MapError::Truncated("XNOD"));
    }
    let v = data[*off];
    *off += 1;
    Ok(v)
}

fn read_i16(data: &[u8], off: &mut usize) -> Result<i16, MapError> {
    if *off + 2 > data.len() {
        return Err(MapError::Truncated("XNOD"));
    }
    let v = i16::from_le_bytes([data[*off], data[*off + 1]]);
    *off += 2;
    Ok(v)
}

fn read_i32(data: &[u8], off: &mut usize) -> Result<i32, MapError> {
    if *off + 4 > data.len() {
        return Err(MapError::Truncated("XNOD"));
    }
    let v = i32::from_le_bytes([data[*off], data[*off + 1], data[*off + 2], data[*off + 3]]);
    *off += 4;
    Ok(v)
}

fn fixed16_16_to_f64(v: i32) -> f64 {
    f64::from(v) / 65536.0
}

/// Parse XNOD body (after the 4-byte magic).
pub fn parse_xnod_body(data: &[u8]) -> Result<GameNodes, MapError> {
    let mut off = 0;
    let _orig_verts = read_u32(data, &mut off)?;
    let new_verts = read_u32(data, &mut off)? as usize;
    let mut extra_vertices = Vec::with_capacity(new_verts);
    for _ in 0..new_verts {
        let x = read_i32(data, &mut off)?;
        let y = read_i32(data, &mut off)?;
        extra_vertices.push(DVec2::new(fixed16_16_to_f64(x), fixed16_16_to_f64(y)));
    }

    let num_subsectors = read_u32(data, &mut off)? as usize;
    let mut subsectors = Vec::with_capacity(num_subsectors);
    let mut first = 0u32;
    for _ in 0..num_subsectors {
        let count = read_u32(data, &mut off)?;
        subsectors.push(Subsector {
            num_segs: count,
            first_seg: first,
        });
        first = first.saturating_add(count);
    }

    let num_segs = read_u32(data, &mut off)? as usize;
    let mut segs = Vec::with_capacity(num_segs);
    for _ in 0..num_segs {
        let v1 = read_u32(data, &mut off)?;
        let v2 = read_u32(data, &mut off)?;
        let line = read_u16(data, &mut off)?;
        let side = read_u8(data, &mut off)?;
        // 1 byte padding in some writers — XNOD is tightly packed per docs (no pad).
        segs.push(Seg {
            v1: VertexId::from_raw(v1),
            v2: VertexId::from_raw(v2),
            angle: 0,
            line: Some(LineId::from_raw(u32::from(line))),
            side: i16::from(side),
            offset: 0,
        });
    }

    let num_nodes = read_u32(data, &mut off)? as usize;
    let mut nodes = Vec::with_capacity(num_nodes);
    for _ in 0..num_nodes {
        let x = read_i16(data, &mut off)?;
        let y = read_i16(data, &mut off)?;
        let dx = read_i16(data, &mut off)?;
        let dy = read_i16(data, &mut off)?;
        let mut bbox = [[0i16; 4]; 2];
        for child_bb in &mut bbox {
            for slot in child_bb.iter_mut() {
                *slot = read_i16(data, &mut off)?;
            }
        }
        let c0 = read_u32(data, &mut off)?;
        let c1 = read_u32(data, &mut off)?;
        nodes.push(Node {
            x,
            y,
            dx,
            dy,
            bbox,
            children: [c0, c1],
        });
    }

    Ok(GameNodes {
        segs,
        subsectors,
        nodes,
        extra_vertices,
    })
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn classic_child_remap() {
        assert_eq!(classic_child_to_ext(0), 0);
        assert_eq!(classic_child_to_ext(0x8005), NF_SUBSECTOR | 5);
    }

    #[test]
    fn xnod_roundtrip_minimal() {
        // Build a tiny XNOD body: 0 new verts, 1 subsector with 0 segs, 0 nodes.
        let mut body = Vec::new();
        body.extend_from_slice(&0u32.to_le_bytes()); // orig
        body.extend_from_slice(&0u32.to_le_bytes()); // new
        body.extend_from_slice(&1u32.to_le_bytes()); // 1 ssector
        body.extend_from_slice(&0u32.to_le_bytes()); // 0 segs in it
        body.extend_from_slice(&0u32.to_le_bytes()); // 0 segs total
        body.extend_from_slice(&0u32.to_le_bytes()); // 0 nodes
        let g = parse_xnod_body(&body).unwrap();
        assert_eq!(g.subsectors.len(), 1);
        assert!(g.segs.is_empty());
        assert!(g.nodes.is_empty());
    }

    #[test]
    fn xgl2_minimal_detect() {
        let mut lump = Vec::from(&b"XGL2"[..]);
        lump.extend_from_slice(&0u32.to_le_bytes());
        lump.extend_from_slice(&0u32.to_le_bytes());
        lump.extend_from_slice(&1u32.to_le_bytes());
        lump.extend_from_slice(&0u32.to_le_bytes());
        lump.extend_from_slice(&0u32.to_le_bytes());
        lump.extend_from_slice(&0u32.to_le_bytes());
        match load_extended_nodes_lump(&lump).unwrap() {
            Some(ExtendedNodes::Gl(gl)) => {
                assert_eq!(gl.subsectors.len(), 1);
            }
            other => panic!("expected Gl, got {other:?}"),
        }
    }
}
