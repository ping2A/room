//! Test helpers: UDMF map builder DSL and headless scenario runners.

#![cfg_attr(not(test), deny(clippy::unwrap_used))]

use ferrite_map::{MapGeometry, load_map};
use ferrite_vfs::{VfsBuilder, WadWriter};
use std::collections::BTreeMap;
use std::fmt::Write as _;

/// Axis-aligned rectangle in map units.
#[derive(Clone, Copy, Debug)]
pub struct Rect {
    /// Left X.
    pub x0: f64,
    /// Bottom Y.
    pub y0: f64,
    /// Right X.
    pub x1: f64,
    /// Top Y.
    pub y1: f64,
}

/// Construct a rectangle.
#[must_use]
pub fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Rect {
    Rect { x0, y0, x1, y1 }
}

/// Spec for a door special on a shared wall.
#[derive(Clone, Debug)]
pub struct DoorSpec {
    /// UDMF special number (Hexen-style), e.g. 12 = `Door_Raise`.
    pub special: i32,
    /// Args `[speed, delay, lighttag, …]`.
    pub args: [i32; 5],
    /// Activation: `playeruse` by default.
    pub playeruse: bool,
    /// Repeatable.
    pub repeat: bool,
}

impl Default for DoorSpec {
    fn default() -> Self {
        Self {
            special: 12, // Door_Raise
            args: [16, 150, 0, 0, 0],
            playeruse: true,
            repeat: true,
        }
    }
}

#[derive(Clone, Debug)]
struct RoomSpec {
    rect: Rect,
    floor: f64,
    ceil: f64,
    light: i32,
    floor_tex: String,
    ceil_tex: String,
    /// Sector tag / UDMF `id`.
    tag: i32,
    /// Optional `floorplane_a..d`.
    floor_plane: Option<[f64; 4]>,
    /// Optional `ceilingplane_a..d`.
    ceiling_plane: Option<[f64; 4]>,
}

#[derive(Clone, Debug)]
struct DoorLink {
    room_a: String,
    room_b: String,
    spec: DoorSpec,
    /// Optional UDMF line id comment/name for tests.
    comment: String,
}

/// Line special on the shared wall between two rooms (portal, 3D floor, …).
#[derive(Clone, Debug)]
pub struct LineSpecialSpec {
    /// Special number (e.g. 156 = `Line_SetPortal`, 160 = `Sector_Set3DFloor`).
    pub special: i32,
    /// Args.
    pub args: [i32; 5],
    /// UDMF line `id` / tag (0 = omit).
    pub id: i32,
}

#[derive(Clone, Debug)]
struct SpecialLink {
    room_a: String,
    room_b: String,
    spec: LineSpecialSpec,
    comment: String,
}

/// Extra mapthing for polyobjects / slope helpers.
#[derive(Clone, Debug)]
struct ExtraThing {
    x: f64,
    y: f64,
    z: f64,
    angle: i32,
    type_id: i32,
    args: [i32; 5],
}

/// Fluent UDMF map builder for tests.
#[derive(Clone, Debug, Default)]
pub struct MapBuilder {
    rooms: BTreeMap<String, RoomSpec>,
    doors: Vec<DoorLink>,
    specials: Vec<SpecialLink>,
    player: Option<(f64, f64, i32)>,
    extras: Vec<ExtraThing>,
    namespace: String,
    /// One-sided wall midtexture (default `STARTAN2`).
    wall_tex: Option<String>,
}

impl MapBuilder {
    /// Start a new builder (`zdoom` namespace).
    #[must_use]
    pub fn new() -> Self {
        Self {
            namespace: "zdoom".into(),
            ..Default::default()
        }
    }

    /// Add a rectangular room.
    #[must_use]
    pub fn room(mut self, name: &str, r: Rect, floor: f64, ceil: f64) -> Self {
        self.rooms.insert(
            name.to_owned(),
            RoomSpec {
                rect: r,
                floor,
                ceil,
                light: 160,
                floor_tex: "FLOOR0_1".into(),
                ceil_tex: "CEIL1_1".into(),
                tag: 0,
                floor_plane: None,
                ceiling_plane: None,
            },
        );
        self
    }

    /// Set sector tag (`id`) on a room.
    #[must_use]
    pub fn sector_tag(mut self, name: &str, tag: i32) -> Self {
        if let Some(r) = self.rooms.get_mut(name) {
            r.tag = tag;
        }
        self
    }

    /// Set floor / ceiling flat names on a room.
    #[must_use]
    pub fn sector_textures(mut self, name: &str, floor: &str, ceiling: &str) -> Self {
        if let Some(r) = self.rooms.get_mut(name) {
            r.floor_tex = floor.to_owned();
            r.ceil_tex = ceiling.to_owned();
        }
        self
    }

    /// Set sector light level (0..=255) on a room.
    #[must_use]
    pub fn sector_light(mut self, name: &str, light: i32) -> Self {
        if let Some(r) = self.rooms.get_mut(name) {
            r.light = light.clamp(0, 255);
        }
        self
    }

    /// Midtexture for one-sided outer walls.
    #[must_use]
    pub fn wall_texture(mut self, name: &str) -> Self {
        self.wall_tex = Some(name.to_owned());
        self
    }

    /// Set UDMF floor plane coefficients on a room.
    #[must_use]
    pub fn floor_plane(mut self, name: &str, abcd: [f64; 4]) -> Self {
        if let Some(r) = self.rooms.get_mut(name) {
            r.floor_plane = Some(abcd);
        }
        self
    }

    /// Mark the shared wall between two axis-aligned rooms as a use-door.
    #[must_use]
    pub fn door_between(mut self, a: &str, b: &str, spec: DoorSpec) -> Self {
        self.doors.push(DoorLink {
            room_a: a.to_owned(),
            room_b: b.to_owned(),
            spec,
            comment: "door".into(),
        });
        self
    }

    /// Put a Hexen/UDMF line special on the shared wall between two rooms.
    #[must_use]
    pub fn special_between(mut self, a: &str, b: &str, spec: LineSpecialSpec) -> Self {
        self.specials.push(SpecialLink {
            room_a: a.to_owned(),
            room_b: b.to_owned(),
            spec,
            comment: "special".into(),
        });
        self
    }

    /// `Sector_SetPortal` (57) on the shared wall (`args[0]` = dest tag).
    #[must_use]
    pub fn sector_portal_between(self, a: &str, b: &str, tag: i32, kind: i32, plane: i32) -> Self {
        self.special_between(
            a,
            b,
            LineSpecialSpec {
                special: 57,
                args: [tag, kind, plane, 0, 0],
                id: 0,
            },
        )
    }

    /// `Line_SetPortal` (156) on the shared wall (`args[0]` = link tag).
    #[must_use]
    pub fn portal_between(self, a: &str, b: &str, tag: i32, kind: i32) -> Self {
        self.special_between(
            a,
            b,
            LineSpecialSpec {
                special: 156,
                args: [tag, 0, kind, 0, 0],
                id: tag,
            },
        )
    }

    /// Set player start.
    #[must_use]
    pub fn player_start(mut self, x: f64, y: f64, angle: i32) -> Self {
        self.player = Some((x, y, angle));
        self
    }

    /// Place a DoomEdNum mapthing (monsters, weapons, ammo, …).
    #[must_use]
    pub fn thing(mut self, type_id: i32, x: f64, y: f64, angle: i32) -> Self {
        self.extras.push(ExtraThing {
            x,
            y,
            z: 0.0,
            angle,
            type_id,
            args: [0; 5],
        });
        self
    }

    /// Polyobject anchor thing (`9300`); `angle` is the poly id.
    #[must_use]
    pub fn poly_anchor(mut self, x: f64, y: f64, poly_id: i32) -> Self {
        self.extras.push(ExtraThing {
            x,
            y,
            z: 0.0,
            angle: poly_id,
            type_id: 9300,
            args: [0; 5],
        });
        self
    }

    /// Polyobject spawn spot (`9301`); `angle` is the poly id.
    #[must_use]
    pub fn poly_spawn(mut self, x: f64, y: f64, poly_id: i32) -> Self {
        self.extras.push(ExtraThing {
            x,
            y,
            z: 0.0,
            angle: poly_id,
            type_id: 9301,
            args: [0; 5],
        });
        self
    }

    /// `Sector_Set3DFloor` (160) on the shared wall (`args[0]` = sector tag).
    #[must_use]
    pub fn ffloor_between(self, a: &str, b: &str, tag: i32, flags: i32) -> Self {
        self.special_between(
            a,
            b,
            LineSpecialSpec {
                special: 160,
                args: [tag, 1, flags, 255, 0],
                id: 0,
            },
        )
    }

    /// Slope-floor-to-line thing (`9500`); `line_id` matches a linedef `id`.
    #[must_use]
    pub fn slope_floor_to_line(mut self, x: f64, y: f64, z: f64, line_id: i32) -> Self {
        self.extras.push(ExtraThing {
            x,
            y,
            z,
            angle: 0,
            type_id: 9500,
            args: [line_id, 0, 0, 0, 0],
        });
        self
    }

    /// Emit UDMF TEXTMAP text.
    #[must_use]
    pub fn build_udmf_text(&self) -> String {
        let mut out = String::new();
        let _ = writeln!(out, "namespace = \"{}\";", self.namespace);

        let mut vertices: Vec<(f64, f64)> = Vec::new();
        let vert_index = |x: f64, y: f64, verts: &mut Vec<(f64, f64)>| -> usize {
            if let Some(i) = verts.iter().position(|&(vx, vy)| vx == x && vy == y) {
                return i;
            }
            verts.push((x, y));
            verts.len() - 1
        };

        // sector index by room name
        let room_names: Vec<String> = self.rooms.keys().cloned().collect();
        let mut room_sec: BTreeMap<String, usize> = BTreeMap::new();
        for (i, name) in room_names.iter().enumerate() {
            room_sec.insert(name.clone(), i);
        }

        // Edges: undirected key (min_v, max_v) → occupants (room, oriented v1→v2)
        #[derive(Clone)]
        struct EdgeOcc {
            room: String,
            v1: usize,
            v2: usize,
        }
        let mut edge_map: BTreeMap<(usize, usize), Vec<EdgeOcc>> = BTreeMap::new();

        for (name, room) in &self.rooms {
            let r = room.rect;
            let v0 = vert_index(r.x0, r.y0, &mut vertices);
            let v1 = vert_index(r.x1, r.y0, &mut vertices);
            let v2 = vert_index(r.x1, r.y1, &mut vertices);
            let v3 = vert_index(r.x0, r.y1, &mut vertices);
            for (a, b) in [(v0, v1), (v1, v2), (v2, v3), (v3, v0)] {
                let key = if a < b { (a, b) } else { (b, a) };
                edge_map.entry(key).or_default().push(EdgeOcc {
                    room: name.clone(),
                    v1: a,
                    v2: b,
                });
            }
        }

        // Door lookup: unordered room pair → DoorSpec
        let mut door_for: BTreeMap<(String, String), &DoorLink> = BTreeMap::new();
        for d in &self.doors {
            let key = if d.room_a <= d.room_b {
                (d.room_a.clone(), d.room_b.clone())
            } else {
                (d.room_b.clone(), d.room_a.clone())
            };
            door_for.insert(key, d);
        }
        let mut special_for: BTreeMap<(String, String), &SpecialLink> = BTreeMap::new();
        for s in &self.specials {
            let key = if s.room_a <= s.room_b {
                (s.room_a.clone(), s.room_b.clone())
            } else {
                (s.room_b.clone(), s.room_a.clone())
            };
            special_for.insert(key, s);
        }

        struct LineOut {
            v1: usize,
            v2: usize,
            side_front: usize,
            side_back: Option<usize>,
            special: Option<(i32, [i32; 5], bool, bool)>,
            line_id: i32,
            comment: Option<String>,
        }
        struct SideOut {
            sector: usize,
            mid: String,
            top: String,
            bottom: String,
        }

        let mut sidedefs: Vec<SideOut> = Vec::new();
        let mut linedefs: Vec<LineOut> = Vec::new();
        let default_wall = self.wall_tex.clone().unwrap_or_else(|| "STARTAN2".into());

        for occs in edge_map.values() {
            if occs.len() == 1 {
                let o = &occs[0];
                let sec = room_sec[&o.room];
                let si = sidedefs.len();
                sidedefs.push(SideOut {
                    sector: sec,
                    mid: default_wall.clone(),
                    top: "-".into(),
                    bottom: "-".into(),
                });
                linedefs.push(LineOut {
                    v1: o.v1,
                    v2: o.v2,
                    side_front: si,
                    side_back: None,
                    special: None,
                    line_id: 0,
                    comment: None,
                });
            } else if occs.len() == 2 {
                let a = &occs[0];
                let b = &occs[1];
                let sa = room_sec[&a.room];
                let sb = room_sec[&b.room];
                // Peg step faces so raised platforms / uneven ceilings draw.
                let step = default_wall.clone();
                let front = sidedefs.len();
                sidedefs.push(SideOut {
                    sector: sa,
                    mid: "-".into(),
                    top: step.clone(),
                    bottom: step.clone(),
                });
                let back = sidedefs.len();
                sidedefs.push(SideOut {
                    sector: sb,
                    mid: "-".into(),
                    top: step.clone(),
                    bottom: step,
                });
                let key = if a.room <= b.room {
                    (a.room.clone(), b.room.clone())
                } else {
                    (b.room.clone(), a.room.clone())
                };
                let (special, line_id, comment) = if let Some(d) = door_for.get(&key) {
                    (
                        Some((d.spec.special, d.spec.args, d.spec.playeruse, d.spec.repeat)),
                        0,
                        Some(d.comment.clone()),
                    )
                } else if let Some(s) = special_for.get(&key) {
                    (
                        Some((s.spec.special, s.spec.args, false, false)),
                        s.spec.id,
                        Some(s.comment.clone()),
                    )
                } else {
                    (None, 0, None)
                };
                linedefs.push(LineOut {
                    v1: a.v1,
                    v2: a.v2,
                    side_front: front,
                    side_back: Some(back),
                    special,
                    line_id,
                    comment,
                });
            }
            // >2: skip degenerate overlaps
        }

        for (x, y) in &vertices {
            let _ = writeln!(out, "vertex {{ x = {x:.3}; y = {y:.3}; }}");
        }
        for name in &room_names {
            let room = &self.rooms[name];
            let _ = writeln!(out, "sector {{");
            let _ = writeln!(
                out,
                "  heightfloor = {};\n  heightceiling = {};\n  texturefloor = \"{}\";\n  textureceiling = \"{}\";\n  lightlevel = {};",
                room.floor as i32, room.ceil as i32, room.floor_tex, room.ceil_tex, room.light
            );
            if room.tag != 0 {
                let _ = writeln!(out, "  id = {};", room.tag);
            }
            if let Some([a, b, c, d]) = room.floor_plane {
                let _ = writeln!(
                    out,
                    "  floorplane_a = {a};\n  floorplane_b = {b};\n  floorplane_c = {c};\n  floorplane_d = {d};"
                );
            }
            if let Some([a, b, c, d]) = room.ceiling_plane {
                let _ = writeln!(
                    out,
                    "  ceilingplane_a = {a};\n  ceilingplane_b = {b};\n  ceilingplane_c = {c};\n  ceilingplane_d = {d};"
                );
            }
            let _ = writeln!(out, "}}");
        }
        for side in &sidedefs {
            let _ = writeln!(out, "sidedef {{");
            let _ = writeln!(out, "  sector = {};", side.sector);
            let _ = writeln!(out, "  texturemiddle = \"{}\";", side.mid);
            if side.top != "-" {
                let _ = writeln!(out, "  texturetop = \"{}\";", side.top);
            }
            if side.bottom != "-" {
                let _ = writeln!(out, "  texturebottom = \"{}\";", side.bottom);
            }
            let _ = writeln!(out, "}}");
        }
        for line in &linedefs {
            let _ = writeln!(out, "linedef {{");
            if let Some(c) = &line.comment {
                let _ = writeln!(out, "  // {c}");
            }
            let _ = writeln!(
                out,
                "  v1 = {};\n  v2 = {};\n  sidefront = {};",
                line.v1, line.v2, line.side_front
            );
            if let Some(back) = line.side_back {
                let _ = writeln!(out, "  sideback = {back};");
                let _ = writeln!(out, "  twosided = true;");
            }
            if line.line_id != 0 {
                let _ = writeln!(out, "  id = {};", line.line_id);
            }
            if let Some((special, args, playeruse, repeat)) = line.special {
                let _ = writeln!(out, "  special = {special};");
                for (i, a) in args.iter().enumerate() {
                    if *a != 0 || i < 2 {
                        let _ = writeln!(out, "  arg{i} = {a};");
                    }
                }
                if playeruse {
                    let _ = writeln!(out, "  playeruse = true;");
                }
                if repeat {
                    let _ = writeln!(out, "  repeatspecial = true;");
                }
            }
            let _ = writeln!(out, "}}");
        }
        if let Some((x, y, angle)) = self.player {
            let _ = writeln!(
                out,
                "thing {{\n  x = {x:.3};\n  y = {y:.3};\n  angle = {angle};\n  type = 1;\n}}"
            );
        }
        for th in &self.extras {
            let _ = writeln!(out, "thing {{");
            let _ = writeln!(
                out,
                "  x = {:.3};\n  y = {:.3};\n  height = {:.3};\n  angle = {};\n  type = {};",
                th.x, th.y, th.z, th.angle, th.type_id
            );
            for (i, a) in th.args.iter().enumerate() {
                if *a != 0 {
                    let _ = writeln!(out, "  arg{i} = {a};");
                }
            }
            let _ = writeln!(out, "}}");
        }
        out
    }

    /// Build a PWAD containing `MAP01` as UDMF and load it.
    pub fn build_udmf(self, map_name: &str) -> Result<MapGeometry, String> {
        let text = self.build_udmf_text();
        let wad = WadWriter::pwad()
            .add(map_name, Vec::new())
            .add("TEXTMAP", text.into_bytes())
            .add("ENDMAP", Vec::new())
            .finish()
            .map_err(|e| e.to_string())?;
        let vfs = VfsBuilder::new()
            .mount_memory("testkit.wad", wad)
            .map_err(|e| e.to_string())?
            .build()
            .map_err(|e| e.to_string())?;
        load_map(&vfs, map_name).map_err(|e| e.to_string())
    }
}

/// Closed combat arena: weapons/ammo at the south wall, monsters facing the player.
///
/// DoomEdNums: ZombieMan 3004, ShotgunGuy 9, DoomImp 3001, ChaingunGuy 65;
/// weapons Shotgun/Chaingun/RL/Plasma/SSG + ammo.
///
/// Uses common Freedoom/Doom flats + `STARTAN2` walls (one-sided sides from [`MapBuilder`]).
#[must_use]
pub fn combat_arena() -> MapBuilder {
    // Compact multi-room arena (shared edges → step sidedefs). Lower ceiling
    // + side cover platforms so it reads as a Doom fight space, not a hangar.
    // Player faces north (Doom angle 90); monsters face south (270).
    MapBuilder::new()
        .room("spawn", rect(256.0, 0.0, 768.0, 192.0), 0.0, 128.0)
        .sector_textures("spawn", "FLOOR5_1", "CEIL3_5")
        .sector_light("spawn", 200)
        .room("arena", rect(256.0, 192.0, 768.0, 768.0), 0.0, 144.0)
        .sector_textures("arena", "FLOOR4_8", "FLAT5_4")
        .sector_light("arena", 192)
        // West corridor + raised cover slab
        .room("west_s", rect(64.0, 192.0, 256.0, 384.0), 0.0, 128.0)
        .sector_textures("west_s", "FLOOR0_1", "CEIL3_5")
        .sector_light("west_s", 168)
        .room("west_hi", rect(64.0, 384.0, 256.0, 576.0), 24.0, 128.0)
        .sector_textures("west_hi", "FLOOR7_1", "CEIL3_5")
        .sector_light("west_hi", 192)
        .room("west_n", rect(64.0, 576.0, 256.0, 768.0), 0.0, 128.0)
        .sector_textures("west_n", "FLOOR0_1", "CEIL3_5")
        .sector_light("west_n", 152)
        // East corridor + raised cover slab
        .room("east_s", rect(768.0, 192.0, 960.0, 384.0), 0.0, 128.0)
        .sector_textures("east_s", "FLOOR0_1", "CEIL3_5")
        .sector_light("east_s", 168)
        .room("east_hi", rect(768.0, 384.0, 960.0, 576.0), 24.0, 128.0)
        .sector_textures("east_hi", "FLOOR7_1", "CEIL3_5")
        .sector_light("east_hi", 192)
        .room("east_n", rect(768.0, 576.0, 960.0, 768.0), 0.0, 128.0)
        .sector_textures("east_n", "FLOOR0_1", "CEIL3_5")
        .sector_light("east_n", 152)
        .wall_texture("STARTAN2")
        .player_start(512.0, 96.0, 90)
        // Arsenal strip (spawn vestibule)
        .thing(82, 320.0, 140.0, 0) // SuperShotgun
        .thing(2001, 384.0, 140.0, 0) // Shotgun
        .thing(2002, 448.0, 140.0, 0) // Chaingun
        .thing(2003, 512.0, 140.0, 0) // RocketLauncher
        .thing(2004, 576.0, 140.0, 0) // PlasmaRifle
        .thing(2008, 352.0, 168.0, 0) // Shell
        .thing(2049, 400.0, 168.0, 0) // ShellBox
        .thing(2007, 448.0, 168.0, 0) // Clip
        .thing(2048, 496.0, 168.0, 0) // ClipBox
        .thing(2010, 544.0, 168.0, 0) // RocketAmmo
        .thing(2046, 592.0, 168.0, 0) // RocketBox
        .thing(2047, 640.0, 168.0, 0) // Cell
        .thing(17, 688.0, 168.0, 0) // CellPack
        // Front line — zombiemen
        .thing(3004, 400.0, 560.0, 270)
        .thing(3004, 464.0, 560.0, 270)
        .thing(3004, 528.0, 560.0, 270)
        .thing(3004, 592.0, 560.0, 270)
        .thing(3004, 656.0, 560.0, 270)
        // Shotgunners + chaingunner
        .thing(9, 400.0, 640.0, 270)
        .thing(9, 624.0, 640.0, 270)
        .thing(65, 512.0, 640.0, 270)
        // Imps behind
        .thing(3001, 448.0, 704.0, 270)
        .thing(3001, 512.0, 704.0, 270)
        .thing(3001, 576.0, 704.0, 270)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    #[test]
    fn combat_arena_thing_count() {
        let map = combat_arena().build_udmf("MAP01").unwrap();
        assert!(map.things.len() > 15);
        assert!(map.things.iter().any(|t| t.type_id == 1));
        assert!(map.things.iter().any(|t| t.type_id == 3004));
        assert!(map.things.iter().any(|t| t.type_id == 2001));
        assert!(
            map.sectors.len() >= 8,
            "expected multi-room arena, got {} sectors",
            map.sectors.len()
        );
        assert!(
            map.sides
                .iter()
                .any(|s| s.lower != "-" && !s.lower.is_empty()),
            "expected step lower textures on raised platforms"
        );
    }

    #[test]
    fn two_room_map_loads() {
        let map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .room("b", rect(256.0, 0.0, 512.0, 256.0), 0.0, 128.0)
            .player_start(64.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        assert_eq!(map.format, ferrite_map::MapFormat::Udmf);
        assert_eq!(map.sectors.len(), 2);
        assert_eq!(map.things.len(), 1);
        assert!(map.vertices.len() >= 6); // shared edge vertices
        // Shared wall should be two-sided (one line with back sector).
        assert!(
            map.lines.iter().any(|l| l.back_sector.is_some()),
            "expected a two-sided shared wall"
        );
    }

    #[test]
    fn door_between_sets_special() {
        let map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .room("b", rect(256.0, 0.0, 512.0, 256.0), 0.0, 128.0)
            .door_between("a", "b", DoorSpec::default())
            .player_start(64.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        let door = map
            .lines
            .iter()
            .find(|l| l.special == 12)
            .expect("door special 12");
        assert!(door.back_sector.is_some());
        assert_eq!(door.args[0], 16);
        assert_eq!(door.args[1], 150);
    }

    #[test]
    fn floor_plane_and_portal_setup() {
        // Gentle slope: z = 0.25 * y  →  0*x + (-0.25)*y + 1*z = 0
        let map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .floor_plane("a", [0.0, -0.25, 1.0, 0.0])
            .room("b", rect(256.0, 0.0, 512.0, 256.0), 0.0, 128.0)
            .sector_tag("b", 7)
            .portal_between("a", "b", 42, 0)
            .player_start(64.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        assert_eq!(map.sectors[0].floor_plane, Some([0.0, -0.25, 1.0, 0.0]));
        assert!(map.setup.planes[0].floor.is_slope());
        assert!((map.setup.planes[0].floor.z_at(0.0, 64.0) - 16.0).abs() < 1e-6);
        assert!(
            map.lines.iter().any(|l| l.special == 156),
            "expected Line_SetPortal special on shared wall"
        );
        assert!(
            !map.setup.portals.lines.is_empty(),
            "setup should record the portal line"
        );
        assert!(map.sectors[1].tags.contains(&7));
    }

    #[test]
    fn poly_and_ffloor_helpers() {
        let map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .room("b", rect(256.0, 0.0, 512.0, 256.0), 64.0, 96.0)
            .sector_tag("b", 3)
            .ffloor_between("a", "b", 3, 1)
            .poly_anchor(64.0, 64.0, 1)
            .poly_spawn(80.0, 80.0, 1)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        assert!(map.things.iter().any(|t| t.type_id == 9300 && t.angle == 1));
        assert!(map.things.iter().any(|t| t.type_id == 9301 && t.angle == 1));
        assert!(map.lines.iter().any(|l| l.special == 160));
        assert!(
            !map.setup.ffloors.is_empty(),
            "expected Sector_Set3DFloor setup entries"
        );
    }
}
