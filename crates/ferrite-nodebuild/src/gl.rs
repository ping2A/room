//! Build GL nodes from classic game nodes (closed subsector polygons).

use ferrite_core::VertexId;
use ferrite_map::{GameNodes, GlNodes, GlSeg, MapGeometry, Seg, Subsector};

/// Convert [`GameNodes`] into [`GlNodes`], inserting minisegs so each
/// subsector's segs form a closed ring (v2 of each → v1 of next).
#[must_use]
pub fn game_to_gl(map: &MapGeometry, game: &GameNodes) -> GlNodes {
    let mut gl_segs = Vec::new();
    let mut gl_ss = Vec::new();

    for sub in &game.subsectors {
        let start = sub.first_seg as usize;
        let n = sub.num_segs as usize;
        let first_gl = gl_segs.len() as u32;
        if n == 0 {
            gl_ss.push(Subsector {
                num_segs: 0,
                first_seg: first_gl,
            });
            continue;
        }
        let slice = &game.segs[start..start + n];
        let ordered = order_segs(map, slice);
        emit_closed_ring(&ordered, &mut gl_segs);
        let count = (gl_segs.len() as u32).saturating_sub(first_gl);
        gl_ss.push(Subsector {
            num_segs: count,
            first_seg: first_gl,
        });
    }

    // Pair partners for two-sided linedef segs (same line, opposite side).
    assign_partners(&mut gl_segs);

    GlNodes {
        segs: gl_segs,
        subsectors: gl_ss,
        nodes: game.nodes.clone(),
        extra_vertices: game.extra_vertices.clone(),
    }
}

fn vert_pos(map: &MapGeometry, id: VertexId) -> Option<(f64, f64)> {
    map.vertices
        .get(id.raw() as usize)
        .map(|v| (v.pos.x(), v.pos.y()))
}

/// Order segs by chaining endpoints (greedy). Falls back to input order.
fn order_segs(map: &MapGeometry, segs: &[Seg]) -> Vec<Seg> {
    if segs.len() <= 1 {
        return segs.to_vec();
    }
    let mut used = vec![false; segs.len()];
    let mut out = Vec::with_capacity(segs.len());
    out.push(segs[0]);
    used[0] = true;
    for _ in 1..segs.len() {
        let Some((px, py)) = vert_pos(map, out[out.len() - 1].v2) else {
            break;
        };
        let mut found = None;
        for (i, s) in segs.iter().enumerate() {
            if used[i] {
                continue;
            }
            let Some((ax, ay)) = vert_pos(map, s.v1) else {
                continue;
            };
            if (ax - px).abs() < 1e-6 && (ay - py).abs() < 1e-6 {
                found = Some((i, *s));
                break;
            }
            let Some((bx, by)) = vert_pos(map, s.v2) else {
                continue;
            };
            if (bx - px).abs() < 1e-6 && (by - py).abs() < 1e-6 {
                // Reverse orientation for chaining.
                found = Some((
                    i,
                    Seg {
                        v1: s.v2,
                        v2: s.v1,
                        angle: s.angle,
                        line: s.line,
                        side: s.side,
                        offset: s.offset,
                    },
                ));
                break;
            }
        }
        if let Some((i, s)) = found {
            used[i] = true;
            out.push(s);
        } else {
            // Append remaining in original order.
            for (i, s) in segs.iter().enumerate() {
                if !used[i] {
                    used[i] = true;
                    out.push(*s);
                }
            }
            break;
        }
    }
    out
}

fn emit_closed_ring(ordered: &[Seg], out: &mut Vec<GlSeg>) {
    if ordered.is_empty() {
        return;
    }
    for (i, s) in ordered.iter().enumerate() {
        out.push(GlSeg {
            v1: s.v1,
            partner: u32::MAX,
            line: s.line,
            side: s.side as u8,
        });
        let next_v1 = ordered[(i + 1) % ordered.len()].v1;
        if s.v2 != next_v1 {
            // Gap → miniseg (no linedef).
            out.push(GlSeg {
                v1: s.v2,
                partner: u32::MAX,
                line: None,
                side: 0,
            });
        }
    }
}

fn assign_partners(segs: &mut [GlSeg]) {
    for i in 0..segs.len() {
        if segs[i].partner != u32::MAX {
            continue;
        }
        let Some(line) = segs[i].line else {
            continue;
        };
        let side = segs[i].side;
        for j in (i + 1)..segs.len() {
            if segs[j].line == Some(line) && segs[j].side != side {
                segs[i].partner = j as u32;
                segs[j].partner = i as u32;
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use crate::apply_nodes;
    use ferrite_testkit::{MapBuilder, rect};

    #[test]
    fn gl_ring_closes_single_room() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let gl = map.gl_nodes.as_ref().expect("gl nodes");
        assert!(!gl.segs.is_empty());
        assert!(!gl.subsectors.is_empty());
        for sub in &gl.subsectors {
            assert!(
                sub.num_segs >= 3 || sub.num_segs == 0,
                "subsector too small: {}",
                sub.num_segs
            );
        }
    }
}
