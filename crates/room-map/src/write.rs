//! Editable map document ([`RawMap`]) and writers back to Doom / Hexen /
//! UDMF lumps — the inverse of [`crate::doom`] and [`crate::udmf`].

use crate::blockmap::build_blockmap;
use crate::doom::{
    parse_linedefs, parse_linedefs_hexen, parse_sectors, parse_sidedefs, parse_things,
    parse_things_hexen, parse_vertexes,
};
use crate::error::MapError;
use crate::geom::{MapFormat, MapGeometry, link_geometry};
use crate::raw::{RawLinedef, RawSector, RawSidedef, RawThing, RawVertex};
use crate::reject::build_reject;
use crate::spac;
use crate::udmf::parse_udmf;
use md5::{Digest, Md5};
use room_vfs::{Namespace, Vfs};
use serde::{Deserialize, Serialize};
use std::fmt::Write as _;

/// Unset sidedef / linedef reference.
pub const NO_SIDE: u16 = 0xFFFF;

impl Default for RawLinedef {
    fn default() -> Self {
        Self {
            v1: 0,
            v2: 0,
            flags: 0,
            special: 0,
            tag: 0,
            side_front: NO_SIDE,
            side_back: NO_SIDE,
            args: [0; 5],
            hexen: false,
            activation: 0,
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
            extra_ids: Vec::new(),
            alpha: 1.0,
            mid_additive: false,
            health: 0,
            health_group: 0,
            automap_style: 0,
            clip_midtex: false,
            wrap_midtex: false,
        }
    }
}

impl Default for RawSidedef {
    fn default() -> Self {
        Self {
            x_offset: 0,
            y_offset: 0,
            upper: "-".into(),
            lower: "-".into(),
            middle: "-".into(),
            sector: 0,
            light: i16::MIN,
            light_absolute: false,
            light_top: i16::MIN,
            light_mid: i16::MIN,
            light_bottom: i16::MIN,
            light_absolute_top: false,
            light_absolute_mid: false,
            light_absolute_bottom: false,
            scale_x: 1.0,
            scale_y: 1.0,
            scale_x_top: 1.0,
            scale_y_top: 1.0,
            scale_x_bottom: 1.0,
            scale_y_bottom: 1.0,
            x_offset_top: 0,
            y_offset_top: 0,
            x_offset_bottom: 0,
            y_offset_bottom: 0,
            no_fake_contrast: false,
            smooth_lighting: false,
            scroll_x: 0.0,
            scroll_y: 0.0,
            clip_midtex: false,
            wrap_midtex: false,
            no_decals: false,
        }
    }
}

impl Default for RawSector {
    fn default() -> Self {
        Self {
            floor_height: 0,
            ceiling_height: 128,
            floor_pic: "FLOOR0_1".into(),
            ceiling_pic: "CEIL1_1".into(),
            light: 160,
            special: 0,
            tag: 0,
            floor_plane: None,
            ceiling_plane: None,
            extra_ids: Vec::new(),
            floor_x_pan: 0.0,
            floor_y_pan: 0.0,
            ceiling_x_pan: 0.0,
            ceiling_y_pan: 0.0,
            floor_rotation: 0.0,
            ceiling_rotation: 0.0,
            floor_x_scale: 1.0,
            floor_y_scale: 1.0,
            ceiling_x_scale: 1.0,
            ceiling_y_scale: 1.0,
            gravity: -1.0,
            friction: 0,
            damage_amount: 0,
            damage_type: String::new(),
            damage_interval: 0,
            leakiness: 0,
            silent: false,
            waterzone: false,
            drop_actors: true,
            no_falling_damage: false,
            light_color: -1,
            fade_color: -1,
            fog_density: 0,
            desaturation: 0,
            color_floor: -1,
            color_ceiling: -1,
            color_walltop: -1,
            color_wallbottom: -1,
            color_sprites: -1,
            health_floor: 0,
            health_ceiling: 0,
            health_3d: 0,
            floor_alpha: 1.0,
            ceiling_alpha: 1.0,
            floor_additive: false,
            ceiling_additive: false,
        }
    }
}

/// A whole map as editable records (what an editor loads, mutates and saves).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct RawMap {
    /// Map marker name (`E1M1`, `MAP01`).
    pub name: String,
    /// Source / output format.
    pub format: MapFormat,
    /// Vertices.
    pub vertices: Vec<RawVertex>,
    /// Linedefs.
    pub linedefs: Vec<RawLinedef>,
    /// Sidedefs.
    pub sidedefs: Vec<RawSidedef>,
    /// Sectors.
    pub sectors: Vec<RawSector>,
    /// Things.
    pub things: Vec<RawThing>,
    /// Compiled ACS (`BEHAVIOR`) carried through unchanged.
    #[serde(skip)]
    pub behavior: Option<Vec<u8>>,
    /// Map lumps an editor does not touch (`SCRIPTS`, `DIALOGUE`, …).
    #[serde(skip)]
    pub extra_lumps: Vec<(String, Vec<u8>)>,
}

/// Lumps rebuilt by the engine (never copied through from the source).
const DERIVED_LUMPS: &[&str] = &[
    "SEGS", "SSECTORS", "NODES", "REJECT", "BLOCKMAP", "ZNODES", "GL_VERT", "GL_SEGS", "GL_SSECT",
    "GL_NODES", "GL_PVS",
];

/// Lumps owned by [`RawMap`] fields.
const OWNED_LUMPS: &[&str] = &[
    "THINGS", "LINEDEFS", "SIDEDEFS", "VERTEXES", "SECTORS", "TEXTMAP", "BEHAVIOR", "ENDMAP",
];

impl RawMap {
    /// A playable 512×512 room with a player 1 start (new-map template).
    #[must_use]
    pub fn starter_room(name: &str, format: MapFormat) -> Self {
        let hexen = format != MapFormat::Doom;
        // Clockwise, so every front side faces the interior.
        let corners = [(0, 0), (0, 512), (512, 512), (512, 0)];
        let vertices = corners.iter().map(|&(x, y)| RawVertex { x, y }).collect();
        let linedefs = (0..4_u16)
            .map(|i| RawLinedef {
                v1: i,
                v2: (i + 1) % 4,
                flags: 1,
                side_front: i,
                hexen,
                ..RawLinedef::default()
            })
            .collect();
        let sidedefs = (0..4)
            .map(|_| RawSidedef {
                middle: "STARTAN2".into(),
                ..RawSidedef::default()
            })
            .collect();
        let things = vec![RawThing {
            x: 256,
            y: 256,
            angle: 90,
            type_id: 1,
            flags: if format == MapFormat::Hexen { 0x707 } else { 7 },
            hexen,
            ..RawThing::default()
        }];
        Self {
            name: name.to_ascii_uppercase(),
            format,
            vertices,
            linedefs,
            sidedefs,
            sectors: vec![RawSector::default()],
            things,
            behavior: (format == MapFormat::Hexen).then(|| {
                [b'A', b'C', b'S', 0, 8, 0, 0, 0]
                    .iter()
                    .chain(&[0; 8])
                    .copied()
                    .collect()
            }),
            extra_lumps: Vec::new(),
        }
    }

    /// Load `name` from `vfs` without linking (Doom, Hexen or UDMF).
    pub fn load(vfs: &Vfs, name: &str) -> Result<Self, MapError> {
        let marker = vfs
            .find(name, Namespace::Global)
            .or_else(|| vfs.find_any_ns(name))
            .ok_or_else(|| MapError::NotFound(name.to_owned()))?;
        let lumps = crate::loader::collect_map_lumps(vfs, marker)?;
        let get = |n: &str| {
            lumps
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case(n))
                .map(|(_, d)| d.as_slice())
        };
        let behavior = get("BEHAVIOR").map(<[u8]>::to_vec);
        let extra_lumps = lumps
            .iter()
            .filter(|(k, _)| {
                let up = k.to_ascii_uppercase();
                !DERIVED_LUMPS.contains(&up.as_str()) && !OWNED_LUMPS.contains(&up.as_str())
            })
            .cloned()
            .collect();
        let mut map = if let Some(text) = get("TEXTMAP") {
            let text = std::str::from_utf8(text)
                .map_err(|_| MapError::Message("TEXTMAP not UTF-8".into()))?;
            let u = parse_udmf(text)?;
            Self {
                format: MapFormat::Udmf,
                vertices: u.vertices,
                linedefs: u.linedefs,
                sidedefs: u.sidedefs,
                sectors: u.sectors,
                things: u.things,
                ..Self::default()
            }
        } else {
            let need = |n: &'static str| get(n).ok_or(MapError::Truncated(n));
            let hexen = behavior.is_some();
            Self {
                format: if hexen {
                    MapFormat::Hexen
                } else {
                    MapFormat::Doom
                },
                vertices: parse_vertexes(need("VERTEXES")?)?,
                linedefs: if hexen {
                    parse_linedefs_hexen(need("LINEDEFS")?)?
                } else {
                    parse_linedefs(need("LINEDEFS")?)?
                },
                sidedefs: parse_sidedefs(need("SIDEDEFS")?)?,
                sectors: parse_sectors(need("SECTORS")?)?,
                things: if hexen {
                    parse_things_hexen(need("THINGS")?)?
                } else {
                    parse_things(need("THINGS")?)?
                },
                ..Self::default()
            }
        };
        map.name = name.to_ascii_uppercase();
        map.behavior = behavior;
        map.extra_lumps = extra_lumps;
        Ok(map)
    }

    /// Check references so a broken edit cannot crash linking.
    pub fn validate(&self) -> Result<(), MapError> {
        let nv = self.vertices.len();
        let ns = self.sidedefs.len();
        let nsec = self.sectors.len();
        for (i, l) in self.linedefs.iter().enumerate() {
            if usize::from(l.v1) >= nv || usize::from(l.v2) >= nv {
                return Err(MapError::Message(format!("linedef {i}: bad vertex")));
            }
            for s in [l.side_front, l.side_back] {
                if s != NO_SIDE && usize::from(s) >= ns {
                    return Err(MapError::Message(format!("linedef {i}: bad sidedef {s}")));
                }
            }
        }
        for (i, s) in self.sidedefs.iter().enumerate() {
            if usize::from(s.sector) >= nsec {
                return Err(MapError::Message(format!("sidedef {i}: bad sector")));
            }
        }
        Ok(())
    }

    /// Link into renderable / playable geometry (no BSP; run a node builder after).
    pub fn to_geometry(&self) -> Result<MapGeometry, MapError> {
        self.validate()?;
        let mut h = Md5::new();
        for (_, data) in self.to_lumps()? {
            h.update(&data);
        }
        let mut md5 = [0u8; 16];
        md5.copy_from_slice(&h.finalize());
        let mut geom = link_geometry(
            &self.name,
            self.format,
            self.vertices.clone(),
            self.linedefs.clone(),
            self.sidedefs.clone(),
            self.sectors.clone(),
            self.things.clone(),
            md5,
        );
        geom.blockmap = build_blockmap(&geom.vertices, &geom.lines);
        geom.reject = Some(build_reject(&geom.lines, geom.sectors.len()));
        geom.behavior.clone_from(&self.behavior);
        Ok(geom)
    }

    /// Marker + map lumps in WAD order for [`Self::format`].
    pub fn to_lumps(&self) -> Result<Vec<(String, Vec<u8>)>, MapError> {
        self.validate()?;
        let mut out = vec![(self.name.clone(), Vec::new())];
        match self.format {
            MapFormat::Udmf => {
                out.push(("TEXTMAP".into(), write_textmap(self).into_bytes()));
                if let Some(b) = &self.behavior {
                    out.push(("BEHAVIOR".into(), b.clone()));
                }
                out.extend(self.extra_lumps.iter().cloned());
                out.push(("ENDMAP".into(), Vec::new()));
            }
            MapFormat::Doom | MapFormat::Hexen => {
                let hexen = self.format == MapFormat::Hexen;
                out.push(("THINGS".into(), write_things(&self.things, hexen)));
                out.push(("LINEDEFS".into(), write_linedefs(&self.linedefs, hexen)));
                out.push(("SIDEDEFS".into(), write_sidedefs(&self.sidedefs)));
                out.push(("VERTEXES".into(), write_vertexes(&self.vertices)));
                out.push(("SECTORS".into(), write_sectors(&self.sectors)));
                if hexen {
                    out.push(("BEHAVIOR".into(), self.behavior.clone().unwrap_or_default()));
                }
                out.extend(self.extra_lumps.iter().cloned());
            }
        }
        Ok(out)
    }
}

fn put_i16(out: &mut Vec<u8>, v: i16) {
    out.extend_from_slice(&v.to_le_bytes());
}

fn put_u16(out: &mut Vec<u8>, v: u16) {
    out.extend_from_slice(&v.to_le_bytes());
}

fn put_name8(out: &mut Vec<u8>, name: &str) {
    let mut buf = [0u8; 8];
    for (dst, b) in buf.iter_mut().zip(name.trim().bytes()) {
        *dst = b.to_ascii_uppercase();
    }
    out.extend_from_slice(&buf);
}

fn arg_u8(v: i32) -> u8 {
    v.clamp(0, 255) as u8
}

/// Doom `VERTEXES`.
#[must_use]
pub fn write_vertexes(v: &[RawVertex]) -> Vec<u8> {
    let mut out = Vec::with_capacity(v.len() * 4);
    for p in v {
        put_i16(&mut out, p.x);
        put_i16(&mut out, p.y);
    }
    out
}

/// Doom (14-byte) or Hexen (16-byte) `LINEDEFS`.
#[must_use]
pub fn write_linedefs(lines: &[RawLinedef], hexen: bool) -> Vec<u8> {
    let mut out = Vec::with_capacity(lines.len() * 16);
    for l in lines {
        put_u16(&mut out, l.v1);
        put_u16(&mut out, l.v2);
        put_i16(&mut out, l.flags);
        if hexen {
            out.push(l.special.clamp(0, 255) as u8);
            for a in l.args {
                out.push(arg_u8(a));
            }
        } else {
            put_i16(&mut out, l.special);
            put_i16(&mut out, l.tag);
        }
        put_u16(&mut out, l.side_front);
        put_u16(&mut out, l.side_back);
    }
    out
}

/// Doom `SIDEDEFS` (30 bytes).
#[must_use]
pub fn write_sidedefs(sides: &[RawSidedef]) -> Vec<u8> {
    let mut out = Vec::with_capacity(sides.len() * 30);
    for s in sides {
        put_i16(&mut out, s.x_offset);
        put_i16(&mut out, s.y_offset);
        put_name8(&mut out, &s.upper);
        put_name8(&mut out, &s.lower);
        put_name8(&mut out, &s.middle);
        put_u16(&mut out, s.sector);
    }
    out
}

/// Doom `SECTORS` (26 bytes).
#[must_use]
pub fn write_sectors(sectors: &[RawSector]) -> Vec<u8> {
    let mut out = Vec::with_capacity(sectors.len() * 26);
    for s in sectors {
        put_i16(&mut out, s.floor_height);
        put_i16(&mut out, s.ceiling_height);
        put_name8(&mut out, &s.floor_pic);
        put_name8(&mut out, &s.ceiling_pic);
        put_i16(&mut out, s.light);
        put_i16(&mut out, s.special);
        put_i16(&mut out, s.tag);
    }
    out
}

/// Doom (10-byte) or Hexen (20-byte) `THINGS`.
#[must_use]
pub fn write_things(things: &[RawThing], hexen: bool) -> Vec<u8> {
    let mut out = Vec::with_capacity(things.len() * 20);
    for t in things {
        if hexen {
            put_i16(&mut out, t.tid);
        }
        put_i16(&mut out, t.x);
        put_i16(&mut out, t.y);
        if hexen {
            put_i16(&mut out, t.z);
        }
        put_i16(&mut out, t.angle);
        put_i16(&mut out, t.type_id);
        put_i16(&mut out, t.flags);
        if hexen {
            out.push(t.special);
            for a in t.args {
                out.push(arg_u8(a));
            }
        }
    }
    out
}

/// UDMF float literal (always has a decimal point).
fn fl(v: f64) -> String {
    let mut s = format!("{v:.4}");
    while s.ends_with('0') {
        s.pop();
    }
    if s.ends_with('.') {
        s.push('0');
    }
    s
}

fn quoted(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        if c == '"' || c == '\\' {
            out.push('\\');
        }
        out.push(c);
    }
    out.push('"');
    out
}

struct Block<'a> {
    out: &'a mut String,
}

impl Block<'_> {
    fn int(&mut self, k: &str, v: i64) {
        let _ = writeln!(self.out, "{k} = {v};");
    }
    fn int_nz(&mut self, k: &str, v: i64) {
        if v != 0 {
            self.int(k, v);
        }
    }
    fn float(&mut self, k: &str, v: f64) {
        let _ = writeln!(self.out, "{k} = {};", fl(v));
    }
    fn float_if(&mut self, k: &str, v: f64, default: f64) {
        if (v - default).abs() > 1e-9 {
            self.float(k, v);
        }
    }
    fn flag(&mut self, k: &str, on: bool) {
        if on {
            let _ = writeln!(self.out, "{k} = true;");
        }
    }
    fn string(&mut self, k: &str, v: &str) {
        let _ = writeln!(self.out, "{k} = {};", quoted(v));
    }
    fn string_nz(&mut self, k: &str, v: &str) {
        if !v.is_empty() {
            self.string(k, v);
        }
    }
    fn ids(&mut self, v: &[i32]) {
        if !v.is_empty() {
            let list: Vec<String> = v.iter().map(i32::to_string).collect();
            self.string("moreids", &list.join(" "));
        }
    }
}

/// UDMF `TEXTMAP` (`namespace = "zdoom"`), writing only non-default fields.
#[must_use]
pub fn write_textmap(map: &RawMap) -> String {
    let mut out = String::from("namespace = \"zdoom\";\n\n");
    for t in &map.things {
        out.push_str("thing\n{\n");
        let mut b = Block { out: &mut out };
        b.int_nz("id", i64::from(t.tid));
        b.float("x", f64::from(t.x));
        b.float("y", f64::from(t.y));
        if t.z != 0 {
            b.float("height", f64::from(t.z));
        }
        b.int("angle", i64::from(t.angle));
        b.int("type", i64::from(t.type_id));
        b.int_nz("special", i64::from(t.special));
        for (i, a) in t.args.iter().enumerate() {
            b.int_nz(&format!("arg{i}"), i64::from(*a));
        }
        b.string_nz("arg0str", &t.arg0_str);
        let f = t.flags;
        for (k, on) in [
            ("skill1", f & 1 != 0),
            ("skill2", f & 1 != 0),
            ("skill3", f & 2 != 0),
            ("skill4", f & 4 != 0),
            ("skill5", f & 4 != 0),
            ("ambush", f & 8 != 0),
            ("single", f & 16 == 0),
            ("coop", true),
            ("dm", true),
            ("dormant", f & 32 != 0),
            ("friend", f & 128 != 0),
            ("countsecret", t.count_secret),
            ("nocount", t.no_count),
        ] {
            b.flag(k, on);
        }
        if t.friendly_see_blocks >= 0 {
            b.int("friendlyseeblocks", i64::from(t.friendly_see_blocks));
        }
        b.int_nz("health", i64::from(t.health));
        if t.score != i32::MIN {
            b.int("score", i64::from(t.score));
        }
        for (k, v) in [
            ("gravity", t.gravity),
            ("alpha", t.alpha),
            ("meleerange", t.melee_range),
            ("viewheight", t.view_height),
            ("speed", t.speed),
            ("friction", t.friction),
            ("damagefactor", t.damage_factor),
            ("maxstepheight", t.max_step_height),
            ("maxdropoffheight", t.max_dropoff_height),
            ("floatspeed", t.float_speed),
            ("fastspeed", t.fast_speed),
            ("jumpz", t.jump_z),
            ("attackzoffset", t.attack_z_offset),
        ] {
            if v >= 0.0 {
                b.float(k, v);
            }
        }
        if t.scale_x > 0.0 {
            b.float("scalex", t.scale_x);
        }
        if t.scale_y > 0.0 {
            b.float("scaley", t.scale_y);
        }
        for (k, v) in [("pitch", t.pitch), ("roll", t.roll)] {
            if v != i16::MIN {
                b.int(k, i64::from(v));
            }
        }
        if t.fill_color >= 0 {
            b.int("fillcolor", i64::from(t.fill_color));
        }
        b.string_nz("renderstyle", &t.render_style);
        if t.float_bob_phase >= 0 {
            b.int("floatbobphase", i64::from(t.float_bob_phase));
        }
        b.int_nz("conversation", i64::from(t.conversation));
        for (k, v) in [
            ("accuracy", t.accuracy),
            ("stamina", t.stamina),
            ("mass", t.mass),
        ] {
            if v != i32::MIN {
                b.int(k, i64::from(v));
            }
        }
        b.string_nz("species", &t.species);
        out.push_str("}\n\n");
    }
    for v in &map.vertices {
        out.push_str("vertex\n{\n");
        let mut b = Block { out: &mut out };
        b.float("x", f64::from(v.x));
        b.float("y", f64::from(v.y));
        out.push_str("}\n\n");
    }
    for l in &map.linedefs {
        out.push_str("linedef\n{\n");
        let mut b = Block { out: &mut out };
        b.int("v1", i64::from(l.v1));
        b.int("v2", i64::from(l.v2));
        b.int("sidefront", side_ref(l.side_front));
        if l.side_back != NO_SIDE {
            b.int("sideback", i64::from(l.side_back));
        }
        b.int_nz("id", i64::from(l.tag));
        b.ids(&l.extra_ids);
        b.int_nz("special", i64::from(l.special));
        for (i, a) in l.args.iter().enumerate() {
            b.int_nz(&format!("arg{i}"), i64::from(*a));
        }
        b.string_nz("arg0str", &l.arg0_str);
        let f = l.flags as u16;
        for (k, bit) in [
            ("blocking", 0x0001),
            ("blockmonsters", 0x0002),
            ("twosided", 0x0004),
            ("dontpegtop", 0x0008),
            ("dontpegbottom", 0x0010),
            ("secret", 0x0020),
            ("blocksound", 0x0040),
            ("dontdraw", 0x0080),
            ("mapped", 0x0100),
            ("repeatspecial", 0x0200),
            ("blockplayers", 0x4000),
            ("blockeverything", 0x8000),
        ] {
            b.flag(k, f & bit != 0);
        }
        let a = l.activation;
        let use_back = a & spac::USEBACK != 0;
        for (k, on) in [
            ("playercross", a & spac::CROSS != 0),
            ("playeruse", a & spac::USE != 0 && !use_back),
            ("playeruseback", use_back),
            ("monstercross", a & spac::MCROSS != 0),
            ("impact", a & spac::IMPACT != 0),
            ("playerpush", a & spac::PUSH != 0),
            ("missilecross", a & spac::PCROSS != 0),
            ("monsteruse", a & spac::MUSE != 0),
            ("monsterpush", a & spac::MPUSH != 0),
            ("anycross", a & spac::ANYCROSS != 0),
            ("damagespecial", a & spac::DAMAGE != 0),
            ("deathspecial", a & spac::DEATH != 0),
            ("midtex3d", l.midtex3d),
            ("midtex3dimpassible", l.midtex3d_impassible),
            ("checkswitchrange", l.check_switch_range),
            ("firstsideonly", l.first_side_only),
            ("blockfloaters", l.block_floaters),
            ("blockprojectiles", l.block_projectiles),
            ("blocksight", l.block_sight),
            ("blockhitscan", l.block_hitscan),
            ("clipmidtex", l.clip_midtex),
            ("wrapmidtex", l.wrap_midtex),
        ] {
            b.flag(k, on);
        }
        b.int_nz("locknumber", i64::from(l.lock_number));
        b.float_if("alpha", f64::from(l.alpha), 1.0);
        if l.mid_additive {
            b.string("renderstyle", "add");
        }
        b.int_nz("health", i64::from(l.health));
        b.int_nz("healthgroup", i64::from(l.health_group));
        b.int_nz("automapstyle", i64::from(l.automap_style));
        out.push_str("}\n\n");
    }
    for s in &map.sidedefs {
        out.push_str("sidedef\n{\n");
        let mut b = Block { out: &mut out };
        b.int("sector", i64::from(s.sector));
        b.int_nz("offsetx", i64::from(s.x_offset));
        b.int_nz("offsety", i64::from(s.y_offset));
        for (k, v, base) in [
            ("offsetx_top", s.x_offset_top, s.x_offset),
            ("offsety_top", s.y_offset_top, s.y_offset),
            ("offsetx_bottom", s.x_offset_bottom, s.x_offset),
            ("offsety_bottom", s.y_offset_bottom, s.y_offset),
        ] {
            if v != base {
                b.int(k, i64::from(v));
            }
        }
        for (k, v) in [
            ("texturetop", &s.upper),
            ("texturebottom", &s.lower),
            ("texturemiddle", &s.middle),
        ] {
            if !v.is_empty() && v != "-" {
                b.string(k, v);
            }
        }
        for (k, v) in [
            ("light", s.light),
            ("light_top", s.light_top),
            ("light_mid", s.light_mid),
            ("light_bottom", s.light_bottom),
        ] {
            if v != i16::MIN {
                b.int(k, i64::from(v));
            }
        }
        for (k, on) in [
            ("lightabsolute", s.light_absolute),
            ("lightabsolute_top", s.light_absolute_top),
            ("lightabsolute_mid", s.light_absolute_mid),
            ("lightabsolute_bottom", s.light_absolute_bottom),
            ("nofakecontrast", s.no_fake_contrast),
            ("smoothlighting", s.smooth_lighting),
            ("clipmidtex", s.clip_midtex),
            ("wrapmidtex", s.wrap_midtex),
            ("nodecals", s.no_decals),
        ] {
            b.flag(k, on);
        }
        for (k, v) in [
            ("scalex_mid", s.scale_x),
            ("scaley_mid", s.scale_y),
            ("scalex_top", s.scale_x_top),
            ("scaley_top", s.scale_y_top),
            ("scalex_bottom", s.scale_x_bottom),
            ("scaley_bottom", s.scale_y_bottom),
        ] {
            b.float_if(k, f64::from(v), 1.0);
        }
        b.float_if("xscroll", f64::from(s.scroll_x), 0.0);
        b.float_if("yscroll", f64::from(s.scroll_y), 0.0);
        out.push_str("}\n\n");
    }
    for s in &map.sectors {
        out.push_str("sector\n{\n");
        let mut b = Block { out: &mut out };
        b.int("heightfloor", i64::from(s.floor_height));
        b.int("heightceiling", i64::from(s.ceiling_height));
        b.string("texturefloor", &s.floor_pic);
        b.string("textureceiling", &s.ceiling_pic);
        b.int("lightlevel", i64::from(s.light));
        b.int_nz("special", i64::from(s.special));
        b.int_nz("id", i64::from(s.tag));
        b.ids(&s.extra_ids);
        for (prefix, plane) in [
            ("floorplane", s.floor_plane),
            ("ceilingplane", s.ceiling_plane),
        ] {
            if let Some(p) = plane {
                for (suffix, v) in ["a", "b", "c", "d"].iter().zip(p) {
                    b.float(&format!("{prefix}_{suffix}"), v);
                }
            }
        }
        for (k, v, d) in [
            ("xpanningfloor", s.floor_x_pan, 0.0),
            ("ypanningfloor", s.floor_y_pan, 0.0),
            ("xpanningceiling", s.ceiling_x_pan, 0.0),
            ("ypanningceiling", s.ceiling_y_pan, 0.0),
            ("rotationfloor", s.floor_rotation, 0.0),
            ("rotationceiling", s.ceiling_rotation, 0.0),
            ("xscalefloor", s.floor_x_scale, 1.0),
            ("yscalefloor", s.floor_y_scale, 1.0),
            ("xscaleceiling", s.ceiling_x_scale, 1.0),
            ("yscaleceiling", s.ceiling_y_scale, 1.0),
            ("alphafloor", f64::from(s.floor_alpha), 1.0),
            ("alphaceiling", f64::from(s.ceiling_alpha), 1.0),
        ] {
            b.float_if(k, v, d);
        }
        if s.gravity >= 0.0 {
            b.float("gravity", s.gravity);
        }
        for (k, v) in [
            ("friction", s.friction),
            ("damageamount", s.damage_amount),
            ("damageinterval", s.damage_interval),
            ("leakiness", s.leakiness),
            ("fogdensity", s.fog_density),
            ("desaturation", s.desaturation),
            ("healthfloor", s.health_floor),
            ("healthceiling", s.health_ceiling),
            ("health3d", s.health_3d),
        ] {
            b.int_nz(k, i64::from(v));
        }
        b.string_nz("damagetype", &s.damage_type);
        for (k, v) in [
            ("lightcolor", s.light_color),
            ("fadecolor", s.fade_color),
            ("color_floor", s.color_floor),
            ("color_ceiling", s.color_ceiling),
            ("color_walltop", s.color_walltop),
            ("color_wallbottom", s.color_wallbottom),
            ("color_sprites", s.color_sprites),
        ] {
            if v >= 0 {
                b.int(k, i64::from(v));
            }
        }
        b.flag("silent", s.silent);
        b.flag("waterzone", s.waterzone);
        b.flag("nofallingdamage", s.no_falling_damage);
        if !s.drop_actors {
            let _ = writeln!(b.out, "dropactors = false;");
        }
        if s.floor_additive || s.ceiling_additive {
            b.string("renderstyle", "add");
        }
        out.push_str("}\n\n");
    }
    out
}

fn side_ref(s: u16) -> i64 {
    if s == NO_SIDE { -1 } else { i64::from(s) }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use room_vfs::{VfsBuilder, WadWriter};

    fn square() -> RawMap {
        let v = |x, y| RawVertex { x, y };
        let line = |v1, v2, side| RawLinedef {
            v1,
            v2,
            side_front: side,
            flags: 1,
            ..RawLinedef::default()
        };
        RawMap {
            name: "MAP01".into(),
            format: MapFormat::Doom,
            vertices: vec![v(0, 0), v(0, 128), v(128, 128), v(128, 0)],
            linedefs: vec![line(0, 1, 0), line(1, 2, 1), line(2, 3, 2), line(3, 0, 3)],
            sidedefs: (0..4)
                .map(|_| RawSidedef {
                    middle: "STARTAN3".into(),
                    ..RawSidedef::default()
                })
                .collect(),
            sectors: vec![RawSector::default()],
            things: vec![RawThing {
                x: 64,
                y: 64,
                type_id: 1,
                flags: 7,
                ..RawThing::default()
            }],
            ..RawMap::default()
        }
    }

    fn reload(map: &RawMap) -> RawMap {
        let mut w = WadWriter::pwad();
        for (name, data) in map.to_lumps().unwrap() {
            w.add(&name, data);
        }
        let bytes = w.finish().unwrap();
        let vfs = VfsBuilder::new()
            .mount_memory("t.wad", bytes::Bytes::from(bytes))
            .unwrap()
            .build()
            .unwrap();
        RawMap::load(&vfs, &map.name).unwrap()
    }

    #[test]
    fn doom_binary_roundtrip() {
        let map = square();
        let back = reload(&map);
        assert_eq!(back.format, MapFormat::Doom);
        assert_eq!(back.vertices, map.vertices);
        assert_eq!(back.linedefs, map.linedefs);
        assert_eq!(back.sidedefs[0].middle, "STARTAN3");
        assert_eq!(back.sectors[0].ceiling_pic, "CEIL1_1");
        assert_eq!(back.things[0].type_id, 1);
        let geom = back.to_geometry().unwrap();
        assert_eq!(geom.lines.len(), 4);
    }

    #[test]
    fn udmf_roundtrip_keeps_extended_fields() {
        let mut map = square();
        map.format = MapFormat::Udmf;
        map.sectors[0].light_color = 0x00FF_0000;
        map.sectors[0].gravity = 0.5;
        map.sidedefs[1].scale_x = 2.0;
        map.linedefs[0].activation = spac::USE;
        map.linedefs[0].special = 80;
        map.things[0].health = 300;
        let back = reload(&map);
        assert_eq!(back.format, MapFormat::Udmf);
        assert_eq!(back.sectors[0].light_color, 0x00FF_0000);
        assert!((back.sectors[0].gravity - 0.5).abs() < 1e-9);
        assert!((back.sidedefs[1].scale_x - 2.0).abs() < 1e-6);
        assert_eq!(back.linedefs[0].activation, spac::USE);
        assert_eq!(back.linedefs[0].special, 80);
        assert_eq!(back.things[0].health, 300);
        assert_eq!(back.things[0].flags & 7, 7);
    }

    #[test]
    fn starter_rooms_link_in_every_format() {
        for format in [MapFormat::Doom, MapFormat::Hexen, MapFormat::Udmf] {
            let doc = RawMap::starter_room("map01", format);
            let geom = doc.to_geometry().unwrap();
            assert_eq!(geom.lines.len(), 4);
            assert!(geom.lines.iter().all(|l| l.front_sector.is_some()));
            assert_eq!(geom.things.len(), 1);
            assert!(!doc.to_lumps().unwrap().is_empty());
        }
    }

    #[test]
    fn json_roundtrip_and_validate() {
        let map = square();
        let json = serde_json::to_string(&map).unwrap();
        let back: RawMap = serde_json::from_str(&json).unwrap();
        assert_eq!(back.linedefs, map.linedefs);
        let partial: RawMap =
            serde_json::from_str(r#"{"name":"MAP01","linedefs":[{"v1":9}]}"#).unwrap();
        assert!(partial.validate().is_err());
    }
}
