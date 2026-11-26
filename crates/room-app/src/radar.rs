//! The phone minimap radar: a small round map in the top corner, turned so
//! the view faces up, as the web client's radar (`#minimap` on touch
//! screens). Lines are coloured as the web radar (walls red, steps brown /
//! gold, doors gold, locked doors in their key colour, exits green,
//! teleporters violet) and only explored lines show, unless the player has
//! the computer map. Drawn on the CPU into a small RGBA image, re-drawn only
//! when the pose or the explored lines change.

use room_map::MapGeometry;

/// Doom `ML_SECRET`: drawn as a plain wall.
const ML_SECRET: i16 = 0x0020;
/// Doom `ML_DONTDRAW`: never on the map.
const ML_DONTDRAW: i16 = 0x0080;
/// Doom `ML_MAPPED`: the player has seen the line.
const ML_MAPPED: i16 = 0x0100;

/// Screen pixels (at 1 dp) per map unit: twice the web radar's phone zoom
/// (1 px per 8 units × 0.7), which left a room the size of a thumbnail;
/// the radar shows about 280 units around the player.
pub(crate) const PX_PER_UNIT_DP: f32 = 1.4 / 8.0;

/// What a line is on the radar (index into [`STYLE`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    /// One-sided or secret.
    Wall = 0,
    /// Floor height changes.
    FloorStep = 1,
    /// Ceiling height changes.
    CeilingStep = 2,
    /// Two-sided, flat.
    Flat = 3,
    /// A door.
    Door = 4,
    /// Red key.
    RedLock = 5,
    /// Blue key.
    BlueLock = 6,
    /// Yellow key.
    YellowLock = 7,
    /// Another lock.
    OtherLock = 8,
    /// Level exit.
    Exit = 9,
    /// Teleporter.
    Teleporter = 10,
}

/// `[r, g, b, a]` per [`Kind`], from the web radar.
const STYLE: [[u8; 4]; 11] = [
    [0xe8, 0x3a, 0x2c, 255],
    [0xb9, 0x84, 0x46, 255],
    [0xe9, 0xc2, 0x5a, 255],
    [150, 150, 150, 72],
    [0xe9, 0xc2, 0x5a, 255],
    [0xff, 0x3b, 0x30, 255],
    [0x3f, 0x8c, 0xff, 255],
    [0xff, 0xd4, 0x3b, 255],
    [0xc7, 0x7d, 0xff, 255],
    [0x3d, 0xdc, 0x84, 255],
    [0x9d, 0x7b, 0xff, 255],
];

/// Lines not explored yet, shown by the computer map.
const UNSEEN: [u8; 4] = [120, 120, 120, 110];
/// The player arrow.
const ARROW: [u8; 4] = [0x5b, 0xff, 0x7a, 255];
/// 128-unit grid.
const GRID: [u8; 4] = [110, 70, 60, 40];
/// Ring around the well.
const RING: [u8; 4] = [233, 194, 90, 140];

/// The last radar image and what it was drawn from.
#[derive(Default)]
pub(crate) struct RadarCache {
    /// Lines of the current map.
    pub lines: Vec<RadarLine>,
    /// When [`Self::lines`] was read.
    pub lines_at: Option<std::time::Instant>,
    /// Map they belong to.
    pub map: String,
    /// What [`Self::rgba`] shows (pose steps, explored count, computer map,
    /// size).
    pub key: Option<(i32, i32, i32, usize, bool, u32)>,
    /// The image.
    pub rgba: Vec<u8>,
}

/// One map line for the radar.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct RadarLine {
    /// First end (map units).
    pub a: (f32, f32),
    /// Second end.
    pub b: (f32, f32),
    /// Colour class.
    pub kind: Kind,
    /// Explored (`ML_MAPPED`).
    pub seen: bool,
}

/// The lines of `map` for the radar (the web radar's `minimap_lines`).
pub(crate) fn lines(map: &MapGeometry) -> Vec<RadarLine> {
    let mut out = Vec::with_capacity(map.lines.len());
    for line in &map.lines {
        if line.flags & ML_DONTDRAW != 0 {
            continue;
        }
        let (Some(v1), Some(v2)) = (
            map.vertices.get(line.v1.raw() as usize),
            map.vertices.get(line.v2.raw() as usize),
        ) else {
            continue;
        };
        let sector =
            |id: Option<room_core::SectorId>| id.and_then(|s| map.sectors.get(s.raw() as usize));
        let lock = |n: i32| match n {
            1 | 4 | 129 => Kind::RedLock,
            2 | 5 | 130 => Kind::BlueLock,
            3 | 6 | 131 => Kind::YellowLock,
            _ => Kind::OtherLock,
        };
        // Hexen numbering: Doom / Boom specials are translated at load.
        let kind = match line.special {
            13 if line.args[3] != 0 => lock(line.args[3]),
            202 | 83 if line.args[4] != 0 => lock(line.args[4]),
            10..=14 | 105 | 106 | 202 | 249 => Kind::Door,
            74 | 75 | 243 | 244 => Kind::Exit,
            39 | 70 | 71 | 76 | 215 => Kind::Teleporter,
            _ => match (sector(line.front_sector), sector(line.back_sector)) {
                (Some(f), Some(b)) if line.flags & ML_SECRET == 0 => {
                    if (f.floor_height - b.floor_height).abs() > 0.5 {
                        Kind::FloorStep
                    } else if (f.ceiling_height - b.ceiling_height).abs() > 0.5 {
                        Kind::CeilingStep
                    } else {
                        Kind::Flat
                    }
                }
                _ => Kind::Wall,
            },
        };
        #[allow(clippy::cast_possible_truncation)]
        out.push(RadarLine {
            a: (v1.pos.x() as f32, v1.pos.y() as f32),
            b: (v2.pos.x() as f32, v2.pos.y() as f32),
            kind,
            seen: line.flags & ML_MAPPED != 0,
        });
    }
    out
}

/// Where the radar looks from.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Pose {
    /// Player X (map units).
    pub x: f32,
    /// Player Y.
    pub y: f32,
    /// Facing, degrees (0 = east, 90 = north).
    pub yaw_deg: f32,
}

/// Draw the radar: a `size`×`size` RGBA image (transparent outside the
/// circle). `dp`: physical pixels per dp (line widths and zoom scale with
/// it). `allmap`: the computer map shows unexplored lines dimmed.
#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]
pub(crate) fn render(lines: &[RadarLine], pose: Pose, allmap: bool, size: u32, dp: f32) -> Vec<u8> {
    let mut img = Img::new(size);
    let c = size as f32 / 2.0;
    let radius = c - 0.5;
    let scale = PX_PER_UNIT_DP * dp;
    // The view forward points up.
    let rot = (pose.yaw_deg - 90.0).to_radians();
    let (sin, cos) = rot.sin_cos();
    let to_screen = |x: f32, y: f32| {
        let dx = (x - pose.x) * scale;
        let dy = -(y - pose.y) * scale;
        (c + dx * cos - dy * sin, c + dx * sin + dy * cos)
    };
    // Well: deep black, a faint warm centre.
    for py in 0..size {
        for px in 0..size {
            let d = (px as f32 + 0.5 - c).hypot(py as f32 + 0.5 - c);
            let t = (d / radius).min(1.0);
            let warm = 1.0 - t;
            img.put(
                px,
                py,
                [
                    (18.0 * warm) as u8,
                    (8.0 * warm) as u8,
                    (6.0 * warm) as u8,
                    225,
                ],
                1.0,
            );
        }
    }
    // Grid every 128 units, world aligned.
    let reach = radius / scale + 128.0;
    let width = 1.0 * dp.max(1.0) * 0.6;
    let mut gx = ((pose.x - reach) / 128.0).floor() * 128.0;
    while gx <= pose.x + reach {
        img.line(
            to_screen(gx, pose.y - reach),
            to_screen(gx, pose.y + reach),
            width,
            GRID,
        );
        gx += 128.0;
    }
    let mut gy = ((pose.y - reach) / 128.0).floor() * 128.0;
    while gy <= pose.y + reach {
        img.line(
            to_screen(pose.x - reach, gy),
            to_screen(pose.x + reach, gy),
            width,
            GRID,
        );
        gy += 128.0;
    }
    // Map lines: flat two-sided lines first so walls stay on top.
    let reach = radius / scale;
    let near = |l: &RadarLine| {
        let (x0, x1) = (l.a.0.min(l.b.0), l.a.0.max(l.b.0));
        let (y0, y1) = (l.a.1.min(l.b.1), l.a.1.max(l.b.1));
        x1 >= pose.x - reach && x0 <= pose.x + reach && y1 >= pose.y - reach && y0 <= pose.y + reach
    };
    for pass in [true, false] {
        for l in lines
            .iter()
            .filter(|l| (l.kind == Kind::Flat) == pass && near(l))
        {
            let color = if l.seen {
                STYLE[l.kind as usize]
            } else if allmap {
                UNSEEN
            } else {
                continue;
            };
            let w = match l.kind {
                Kind::RedLock
                | Kind::BlueLock
                | Kind::YellowLock
                | Kind::OtherLock
                | Kind::Exit => 2.0,
                Kind::Flat => 1.0,
                _ => 1.5,
            } * dp.max(1.0)
                * 0.75;
            img.line(to_screen(l.a.0, l.a.1), to_screen(l.b.0, l.b.1), w, color);
        }
    }
    // The player: Doom's automap arrow, pointing up.
    let len = 7.0 * dp.max(1.0);
    let w = (1.6 * dp.max(1.0)).max(1.5);
    let p = |x: f32, y: f32| (c + x * len, c - y * len);
    for (a, b) in [
        ((0.0, -1.0), (0.0, 1.0)),
        ((0.0, 1.0), (-0.45, 0.55)),
        ((0.0, 1.0), (0.45, 0.55)),
        ((0.0, -1.0), (-0.4, -1.35)),
        ((0.0, -1.0), (0.4, -1.35)),
    ] {
        img.line(p(a.0, a.1), p(b.0, b.1), w, ARROW);
    }
    // Ring, then cut everything outside the circle (anti-aliased edge).
    let ring = 2.0 * dp.max(1.0);
    for py in 0..size {
        for px in 0..size {
            let d = (px as f32 + 0.5 - c).hypot(py as f32 + 0.5 - c);
            let on_ring = (ring / 2.0 + 0.5 - (d - (radius - ring / 2.0)).abs()).clamp(0.0, 1.0);
            if on_ring > 0.0 {
                img.put(px, py, RING, on_ring);
            }
            let inside = (radius - d + 0.5).clamp(0.0, 1.0);
            let i = ((py * size + px) * 4 + 3) as usize;
            img.px[i] = (f32::from(img.px[i]) * inside).round() as u8;
        }
    }
    img.px
}

/// Straight-alpha RGBA image with "over" blending.
struct Img {
    size: u32,
    px: Vec<u8>,
}

impl Img {
    fn new(size: u32) -> Self {
        Self {
            size,
            px: vec![0; (size * size * 4) as usize],
        }
    }

    /// Blend `color` (its alpha times `cover`) over pixel `(x, y)`.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    fn put(&mut self, x: u32, y: u32, color: [u8; 4], cover: f32) {
        if x >= self.size || y >= self.size {
            return;
        }
        let i = ((y * self.size + x) * 4) as usize;
        let sa = f32::from(color[3]) / 255.0 * cover.clamp(0.0, 1.0);
        if sa <= 0.0 {
            return;
        }
        let da = f32::from(self.px[i + 3]) / 255.0;
        let oa = sa + da * (1.0 - sa);
        for k in 0..3 {
            let s = f32::from(color[k]);
            let d = f32::from(self.px[i + k]);
            self.px[i + k] = ((s * sa + d * da * (1.0 - sa)) / oa.max(1e-6)).round() as u8;
        }
        self.px[i + 3] = (oa * 255.0).round() as u8;
    }

    /// An anti-aliased segment `width` px wide, clipped to the image.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    fn line(&mut self, a: (f32, f32), b: (f32, f32), width: f32, color: [u8; 4]) {
        let Some((a, b)) = clip(a, b, self.size as f32) else {
            return;
        };
        let (dx, dy) = (b.0 - a.0, b.1 - a.1);
        let len = dx.hypot(dy);
        let half = width / 2.0;
        // Walk the segment one pixel at a time and cover the pixels within
        // `half + 1` of it by their distance to the segment.
        let steps = len.ceil().max(1.0) as u32;
        let reach = (half + 1.0).ceil() as i32;
        let mut last: Option<(i32, i32)> = None;
        for s in 0..=steps {
            let t = s as f32 / steps as f32;
            let (cx, cy) = (a.0 + dx * t, a.1 + dy * t);
            let (ix, iy) = (cx.floor() as i32, cy.floor() as i32);
            for oy in -reach..=reach {
                for ox in -reach..=reach {
                    let (x, y) = (ix + ox, iy + oy);
                    if x < 0 || y < 0 {
                        continue;
                    }
                    // Each pixel once per segment: skip those the previous
                    // step's square already covered.
                    if let Some((lx, ly)) = last
                        && (x - lx).abs() <= reach
                        && (y - ly).abs() <= reach
                    {
                        continue;
                    }
                    let d = seg_dist((x as f32 + 0.5, y as f32 + 0.5), a, b);
                    let cover = half + 0.5 - d;
                    if cover > 0.0 {
                        self.put(x as u32, y as u32, color, cover.min(1.0));
                    }
                }
            }
            last = Some((ix, iy));
        }
    }
}

/// Distance from `p` to the segment `a`–`b`.
fn seg_dist(p: (f32, f32), a: (f32, f32), b: (f32, f32)) -> f32 {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let len2 = dx * dx + dy * dy;
    let t = if len2 <= 1e-9 {
        0.0
    } else {
        (((p.0 - a.0) * dx + (p.1 - a.1) * dy) / len2).clamp(0.0, 1.0)
    };
    (p.0 - (a.0 + dx * t)).hypot(p.1 - (a.1 + dy * t))
}

/// Clip a segment to `-2..size+2` square (Liang–Barsky); `None` when outside.
fn clip(a: (f32, f32), b: (f32, f32), size: f32) -> Option<((f32, f32), (f32, f32))> {
    let (lo, hi) = (-2.0, size + 2.0);
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let (mut t0, mut t1) = (0.0_f32, 1.0_f32);
    for (p, q) in [
        (-dx, a.0 - lo),
        (dx, hi - a.0),
        (-dy, a.1 - lo),
        (dy, hi - a.1),
    ] {
        if p.abs() < 1e-9 {
            if q < 0.0 {
                return None;
            }
            continue;
        }
        let r = q / p;
        if p < 0.0 {
            t0 = t0.max(r);
        } else {
            t1 = t1.min(r);
        }
        if t0 > t1 {
            return None;
        }
    }
    Some((
        (a.0 + dx * t0, a.1 + dy * t0),
        (a.0 + dx * t1, a.1 + dy * t1),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn px(img: &[u8], size: u32, x: u32, y: u32) -> [u8; 4] {
        let i = ((y * size + x) * 4) as usize;
        [img[i], img[i + 1], img[i + 2], img[i + 3]]
    }

    fn wall(a: (f32, f32), b: (f32, f32)) -> RadarLine {
        RadarLine {
            a,
            b,
            kind: Kind::Wall,
            seen: true,
        }
    }

    #[test]
    fn round_with_a_transparent_outside() {
        let img = render(
            &[],
            Pose {
                x: 0.0,
                y: 0.0,
                yaw_deg: 90.0,
            },
            false,
            100,
            1.0,
        );
        assert_eq!(img.len(), 100 * 100 * 4);
        assert_eq!(px(&img, 100, 0, 0)[3], 0, "corner outside the circle");
        assert_eq!(px(&img, 100, 99, 99)[3], 0);
        assert!(
            px(&img, 100, 50, 30)[3] > 200,
            "inside is opaque enough to read"
        );
        let ring = px(&img, 100, 50, 1);
        assert!(
            ring[0] > 100 && ring[1] > 80,
            "gold ring at the edge: {ring:?}"
        );
    }

    #[test]
    fn the_view_direction_points_up() {
        // A wall 200 units ahead of a player facing east must show above
        // the centre, whatever the yaw.
        for (yaw, ahead) in [
            (0.0, (200.0, 0.0)),
            (90.0, (0.0, 200.0)),
            (225.0, (-141.0, -141.0)),
        ] {
            let (ax, ay) = ahead;
            // A short wall across the view direction, centred on `ahead`.
            let (nx, ny) = (-ay / 200.0 * 15.0, ax / 200.0 * 15.0);
            let l = wall((ax - nx, ay - ny), (ax + nx, ay + ny));
            let img = render(
                &[l],
                Pose {
                    x: 0.0,
                    y: 0.0,
                    yaw_deg: yaw,
                },
                false,
                200,
                2.0,
            );
            // 200 units * 0.175 px/unit/dp * 2 dp = 70 px above the centre.
            let p = px(&img, 200, 100, 100 - 70);
            assert!(
                p[0] > 150 && p[1] < 120,
                "yaw {yaw}: red wall above the centre, got {p:?}"
            );
            let below = px(&img, 200, 100, 100 + 70);
            assert!(below[0] < 100, "yaw {yaw}: nothing below");
        }
    }

    #[test]
    fn unexplored_lines_need_the_computer_map() {
        let l = RadarLine {
            seen: false,
            ..wall((-100.0, 200.0), (100.0, 200.0))
        };
        let pose = Pose {
            x: 0.0,
            y: 0.0,
            yaw_deg: 90.0,
        };
        let hidden = render(&[l], pose, false, 200, 2.0);
        let shown = render(&[l], pose, true, 200, 2.0);
        let at = |img: &[u8]| px(img, 200, 100, 30);
        assert!(at(&hidden)[0] < 40, "fog of war: {:?}", at(&hidden));
        assert!(at(&shown)[0] > 60, "computer map: {:?}", at(&shown));
    }

    #[test]
    fn far_lines_are_skipped_and_clipping_holds() {
        assert!(clip((-50.0, 10.0), (-10.0, 20.0), 100.0).is_none());
        let (a, b) = clip((-50.0, 50.0), (150.0, 50.0), 100.0).expect("crosses");
        assert!((a.0 + 2.0).abs() < 1e-3 && (b.0 - 102.0).abs() < 1e-3);
        // A line far away leaves the well untouched.
        let pose = Pose {
            x: 0.0,
            y: 0.0,
            yaw_deg: 90.0,
        };
        let empty = render(&[], pose, false, 120, 1.0);
        let far = render(
            &[wall((9000.0, 9000.0), (9100.0, 9000.0))],
            pose,
            false,
            120,
            1.0,
        );
        assert_eq!(empty, far);
    }

    #[test]
    fn map_lines_are_classified() {
        let map = room_testkit::MapBuilder::new()
            .room("a", room_testkit::rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .room(
                "b",
                room_testkit::rect(128.0, 0.0, 256.0, 128.0),
                0.0,
                128.0,
            )
            .room(
                "c",
                room_testkit::rect(256.0, 0.0, 384.0, 128.0),
                24.0,
                128.0,
            )
            .door_between("a", "b", room_testkit::DoorSpec::default())
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .expect("map");
        let lines = lines(&map);
        assert!(lines.iter().any(|l| l.kind == Kind::Wall));
        assert!(lines.iter().any(|l| l.kind == Kind::Door), "Door_Raise");
        assert!(
            lines.iter().any(|l| l.kind == Kind::FloorStep),
            "b → c step"
        );
        assert!(lines.iter().all(|l| !l.seen), "nothing explored at load");
        assert_eq!(STYLE.len(), Kind::Teleporter as usize + 1);
    }
}
