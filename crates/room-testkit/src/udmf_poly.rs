//! Polygon UDMF builder for portal tests: sectors are arbitrary polygons,
//! shared edges become two-sided lines, and line specials / ids are set by
//! endpoint. Produces TEXTMAP text only, so `room-map`'s own tests can use it.

use std::fmt::Write as _;

/// One sector polygon.
#[derive(Clone, Debug)]
struct PolySector {
    points: Vec<(f64, f64)>,
    floor: f64,
    ceiling: f64,
    tag: i32,
    floor_tex: String,
    ceiling_tex: String,
    extra: String,
}

#[derive(Clone, Debug)]
struct LineSpec {
    a: (f64, f64),
    b: (f64, f64),
    special: i32,
    args: [i32; 5],
    id: i32,
    /// Sector index the line's front must face.
    front: Option<usize>,
    extra: String,
}

#[derive(Clone, Debug)]
struct ThingSpec {
    x: f64,
    y: f64,
    height: f64,
    angle: i32,
    type_id: i32,
    tid: i32,
    args: [i32; 5],
}

/// Polygon map builder.
#[derive(Clone, Debug, Default)]
pub struct PolyMap {
    sectors: Vec<PolySector>,
    lines: Vec<LineSpec>,
    things: Vec<ThingSpec>,
}

impl PolyMap {
    /// Empty map.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a sector; returns its index.
    pub fn sector(&mut self, points: &[(f64, f64)], floor: f64, ceiling: f64, tag: i32) -> usize {
        self.sectors.push(PolySector {
            points: points.to_vec(),
            floor,
            ceiling,
            tag,
            floor_tex: "FLOOR0_1".into(),
            ceiling_tex: "CEIL1_1".into(),
            extra: String::new(),
        });
        self.sectors.len() - 1
    }

    /// Axis-aligned box sector.
    pub fn rect(
        &mut self,
        x0: f64,
        y0: f64,
        x1: f64,
        y1: f64,
        floor: f64,
        ceiling: f64,
        tag: i32,
    ) -> usize {
        self.sector(
            &[(x0, y0), (x1, y0), (x1, y1), (x0, y1)],
            floor,
            ceiling,
            tag,
        )
    }

    /// Set a sector's flats.
    pub fn textures(&mut self, sector: usize, floor: &str, ceiling: &str) {
        self.sectors[sector].floor_tex = floor.into();
        self.sectors[sector].ceiling_tex = ceiling.into();
    }

    /// Extra UDMF sector fields (`key = value;` text).
    pub fn sector_fields(&mut self, sector: usize, text: &str) {
        self.sectors[sector].extra.push_str(text);
    }

    /// Special / id on the line between `a` and `b`, with its front facing
    /// `front` when given.
    pub fn line(
        &mut self,
        a: (f64, f64),
        b: (f64, f64),
        special: i32,
        args: [i32; 5],
        id: i32,
        front: Option<usize>,
    ) {
        self.lines.retain(|l| !same_edge(l.a, l.b, a, b));
        self.lines.push(LineSpec {
            a,
            b,
            special,
            args,
            id,
            front,
            extra: String::new(),
        });
    }

    /// Extra UDMF fields on the line between `a` and `b` (added with
    /// [`Self::line`] if it has no spec yet).
    pub fn line_fields(&mut self, a: (f64, f64), b: (f64, f64), text: &str) {
        if let Some(l) = self.lines.iter_mut().find(|l| same_edge(l.a, l.b, a, b)) {
            l.extra.push_str(text);
            return;
        }
        self.lines.push(LineSpec {
            a,
            b,
            special: 0,
            args: [0; 5],
            id: 0,
            front: None,
            extra: text.into(),
        });
    }

    /// A thing.
    #[allow(clippy::too_many_arguments)]
    pub fn thing(
        &mut self,
        type_id: i32,
        x: f64,
        y: f64,
        height: f64,
        angle: i32,
        tid: i32,
        args: [i32; 5],
    ) {
        self.things.push(ThingSpec {
            x,
            y,
            height,
            angle,
            type_id,
            tid,
            args,
        });
    }

    /// Player 1 start.
    pub fn player(&mut self, x: f64, y: f64, angle: i32) {
        self.thing(1, x, y, 0.0, angle, 0, [0; 5]);
    }

    /// TEXTMAP text (`zdoom` namespace).
    #[must_use]
    pub fn textmap(&self) -> String {
        let mut verts: Vec<(f64, f64)> = Vec::new();
        let vid = |p: (f64, f64), verts: &mut Vec<(f64, f64)>| -> usize {
            if let Some(i) = verts
                .iter()
                .position(|v| (v.0 - p.0).abs() < 1e-6 && (v.1 - p.1).abs() < 1e-6)
            {
                return i;
            }
            verts.push(p);
            verts.len() - 1
        };
        // Edges: (v1, v2, front sector, back sector).
        let mut edges: Vec<(usize, usize, usize, Option<usize>)> = Vec::new();
        for (si, s) in self.sectors.iter().enumerate() {
            // Interior on the right: clockwise order.
            let area: f64 = s
                .points
                .iter()
                .zip(s.points.iter().cycle().skip(1))
                .map(|(p, q)| p.0 * q.1 - q.0 * p.1)
                .sum();
            let mut pts = s.points.clone();
            if area > 0.0 {
                pts.reverse();
            }
            for i in 0..pts.len() {
                let a = vid(pts[i], &mut verts);
                let b = vid(pts[(i + 1) % pts.len()], &mut verts);
                if let Some(e) = edges
                    .iter_mut()
                    .find(|e| e.0 == b && e.1 == a && e.3.is_none())
                {
                    e.3 = Some(si);
                } else {
                    edges.push((a, b, si, None));
                }
            }
        }
        let mut t = String::from("namespace = \"zdoom\";\n");
        for v in &verts {
            let _ = writeln!(t, "vertex {{ x = {:.3}; y = {:.3}; }}", v.0, v.1);
        }
        let mut sides = String::new();
        let mut nsides = 0usize;
        let mut side = |sector: usize, upper: bool, sides: &mut String| -> usize {
            let tex = if upper {
                "texturetop = \"STARTAN2\"; texturebottom = \"STARTAN2\";"
            } else {
                "texturemiddle = \"STARTAN2\";"
            };
            let _ = writeln!(sides, "sidedef {{ sector = {sector}; {tex} }}");
            nsides += 1;
            nsides - 1
        };
        for &(a0, b0, f0, back0) in &edges {
            let pa = verts[a0];
            let pb = verts[b0];
            let spec = self.lines.iter().find(|l| same_edge(l.a, l.b, pa, pb));
            let (mut a, mut b, mut f, mut back) = (a0, b0, f0, back0);
            if let Some(sp) = spec
                && let Some(want) = sp.front
                && f != want
                && back == Some(want)
            {
                std::mem::swap(&mut a, &mut b);
                back = Some(f);
                f = want;
            }
            let s1 = side(f, back.is_some(), &mut sides);
            let mut l = format!("linedef {{ v1 = {a}; v2 = {b}; sidefront = {s1}; ");
            if let Some(bk) = back {
                let s2 = side(bk, true, &mut sides);
                let _ = write!(l, "sideback = {s2}; twosided = true; ");
            } else {
                l.push_str("blocking = true; ");
            }
            if let Some(sp) = spec {
                if sp.special != 0 {
                    let _ = write!(l, "special = {}; ", sp.special);
                    for (i, v) in sp.args.iter().enumerate() {
                        if *v != 0 {
                            let _ = write!(l, "arg{i} = {v}; ");
                        }
                    }
                }
                if sp.id != 0 {
                    let _ = write!(l, "id = {}; ", sp.id);
                }
                l.push_str(&sp.extra);
            }
            l.push_str("}\n");
            t.push_str(&l);
        }
        t.push_str(&sides);
        for s in &self.sectors {
            let _ = writeln!(
                t,
                "sector {{ heightfloor = {}; heightceiling = {}; texturefloor = \"{}\"; textureceiling = \"{}\"; lightlevel = 192; id = {}; {} }}",
                s.floor as i32, s.ceiling as i32, s.floor_tex, s.ceiling_tex, s.tag, s.extra
            );
        }
        for th in &self.things {
            let mut line = format!(
                "thing {{ x = {:.3}; y = {:.3}; height = {:.3}; angle = {}; type = {}; skill1 = true; skill2 = true; skill3 = true; skill4 = true; skill5 = true; single = true; coop = true; dm = true; ",
                th.x, th.y, th.height, th.angle, th.type_id
            );
            if th.tid != 0 {
                let _ = write!(line, "id = {}; ", th.tid);
            }
            for (i, v) in th.args.iter().enumerate() {
                if *v != 0 {
                    let _ = write!(line, "arg{i} = {v}; ");
                }
            }
            line.push_str("}\n");
            t.push_str(&line);
        }
        t
    }
}

fn same_edge(a: (f64, f64), b: (f64, f64), p: (f64, f64), q: (f64, f64)) -> bool {
    let eq = |u: (f64, f64), v: (f64, f64)| (u.0 - v.0).abs() < 1e-6 && (u.1 - v.1).abs() < 1e-6;
    (eq(a, p) && eq(b, q)) || (eq(a, q) && eq(b, p))
}

/// Two rooms joined by a linked line portal (`Line_SetPortal` type 3).
///
/// Room A is x 0–256 (sector 0) with an empty strip behind its east wall
/// (x 256–272, sector 1); room B is x 1024–1280 (sector 2) with its strip
/// behind its west wall (x 1008–1024, sector 3). Portal line ids are 1 (A)
/// and 2 (B): walking east through x = 256 comes out at x = 1024.
#[must_use]
pub fn linked_line_portal_rooms(kind: i32) -> PolyMap {
    let mut m = PolyMap::new();
    let a = m.rect(0.0, 0.0, 256.0, 256.0, 0.0, 128.0, 0);
    let _da = m.rect(256.0, 0.0, 272.0, 256.0, 0.0, 128.0, 0);
    let b = m.rect(1024.0, 0.0, 1280.0, 256.0, 0.0, 128.0, 0);
    let _db = m.rect(1008.0, 0.0, 1024.0, 256.0, 0.0, 128.0, 0);
    m.line(
        (256.0, 0.0),
        (256.0, 256.0),
        156,
        [2, 1, kind, 0, 0],
        1,
        Some(a),
    );
    m.line(
        (1024.0, 0.0),
        (1024.0, 256.0),
        156,
        [1, 2, kind, 0, 0],
        2,
        Some(b),
    );
    m.player(128.0, 128.0, 0);
    m
}

/// Rooms joined by an interactive portal that turns 90°: room A's east wall
/// (x = 256) leads into room B (x 1024–1280, y 1024–1280) through its south
/// wall (y = 1024), so walking east in A walks north in B.
#[must_use]
pub fn rotated_line_portal_rooms(kind: i32) -> PolyMap {
    let mut m = PolyMap::new();
    let a = m.rect(0.0, 0.0, 256.0, 256.0, 0.0, 128.0, 0);
    let _da = m.rect(256.0, 0.0, 272.0, 256.0, 0.0, 128.0, 0);
    let b = m.rect(1024.0, 1024.0, 1280.0, 1280.0, 0.0, 128.0, 0);
    let _db = m.rect(1024.0, 1008.0, 1280.0, 1024.0, 0.0, 128.0, 0);
    m.line(
        (256.0, 0.0),
        (256.0, 256.0),
        156,
        [2, 1, kind, 0, 0],
        1,
        Some(a),
    );
    m.line(
        (1024.0, 1024.0),
        (1280.0, 1024.0),
        156,
        [1, 2, kind, 0, 0],
        2,
        Some(b),
    );
    m.player(128.0, 128.0, 0);
    m
}

/// A linked plane portal: room A (x 0–256, floor 0, ceiling 128) has a
/// ceiling portal into room B, which sits at x 1024–1280 with its floor at
/// 128 (B's floor portal leads back). `Sector_SetPortal(tag, 6, plane, 0/1)`
/// anchor / reference lines sit on the rooms' south walls.
#[must_use]
pub fn linked_plane_portal_rooms() -> PolyMap {
    let mut m = PolyMap::new();
    let a = m.rect(0.0, 0.0, 256.0, 256.0, 0.0, 128.0, 1);
    let b = m.rect(1024.0, 0.0, 1280.0, 256.0, 128.0, 512.0, 2);
    // A's ceiling looks into B: anchor in A (args[3] = 0), reference in B.
    m.line((0.0, 0.0), (256.0, 0.0), 57, [1, 6, 1, 0, 0], 0, Some(a));
    m.line(
        (1024.0, 0.0),
        (1280.0, 0.0),
        57,
        [1, 6, 1, 1, 0],
        0,
        Some(b),
    );
    // B's floor looks back into A.
    m.line(
        (0.0, 256.0),
        (256.0, 256.0),
        57,
        [2, 6, 0, 1, 0],
        0,
        Some(a),
    );
    m.line(
        (1024.0, 256.0),
        (1280.0, 256.0),
        57,
        [2, 6, 0, 0, 0],
        0,
        Some(b),
    );
    m.player(128.0, 128.0, 0);
    m
}
