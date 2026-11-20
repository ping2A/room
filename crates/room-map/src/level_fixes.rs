//! Per-map geometry / thing / special edits (`docs/19-compatibility.md` §3).
//!
//! Loaded from `room.pk3/compat/levelfixes.ron` (RON map of MD5 → ops).

use crate::error::MapError;
use crate::geom::MapGeometry;
use room_core::DVec2;
use serde::Deserialize;
use std::collections::BTreeMap;

/// MD5 digest as lowercase hex (32 chars).
pub type MapMd5Hex = String;

/// One fix operation (mirrors GZDoom `LevelPostProcessor` helpers).
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub enum LevelFixOp {
    /// Clear all tags on a sector.
    ClearSectorTags {
        /// Sector index.
        sector: u32,
    },
    /// Append a tag to a sector.
    AddSectorTag {
        /// Sector index.
        sector: u32,
        /// Tag value.
        tag: i32,
    },
    /// Offset floor or ceiling height by `delta` map units.
    OffsetSectorPlane {
        /// Sector index.
        sector: u32,
        /// `true` = floor, `false` = ceiling.
        floor: bool,
        /// Delta in map units.
        delta: f64,
    },
    /// Set sector special.
    SetSectorSpecial {
        /// Sector index.
        sector: u32,
        /// Special.
        special: i16,
    },
    /// Set floor or ceiling flat name.
    SetSectorTexture {
        /// Sector index.
        sector: u32,
        /// `true` = floor, `false` = ceiling.
        floor: bool,
        /// Texture / flat name.
        texture: String,
    },
    /// Set sector light level.
    SetSectorLight {
        /// Sector index.
        sector: u32,
        /// Light (0–255).
        light: i16,
    },
    /// Clear linedef tag / id (Doom tag field).
    ClearLineIDs {
        /// Linedef index.
        line: u32,
    },
    /// Set linedef tag (Doom id).
    AddLineID {
        /// Linedef index.
        line: u32,
        /// Tag / id.
        id: i16,
    },
    /// Set linedef special and args.
    SetLineSpecial {
        /// Linedef index.
        line: u32,
        /// Special number.
        special: i16,
        /// Args (defaults to zeros when omitted in RON via `#[serde(default)]`).
        #[serde(default)]
        args: [i32; 5],
    },
    /// Clear linedef special (and args).
    ClearLineSpecial {
        /// Linedef index.
        line: u32,
    },
    /// OR `set` bits and AND-NOT `clear` bits on linedef flags.
    SetLineFlags {
        /// Linedef index.
        line: u32,
        /// Bits to set.
        #[serde(default)]
        set: i16,
        /// Bits to clear.
        #[serde(default)]
        clear: i16,
    },
    /// Swap front/back sidedefs and sector refs.
    FlipLine {
        /// Linedef index.
        line: u32,
    },
    /// Flip only side sector refs (keep v1/v2).
    FlipLineSideRefs {
        /// Linedef index.
        line: u32,
    },
    /// Retarget a side's sector.
    SetLineSectorRef {
        /// Linedef index.
        line: u32,
        /// 0 = front, 1 = back.
        side: u8,
        /// Sector index.
        sector: u32,
    },
    /// Set a wall texture on a sidedef.
    SetWallTexture {
        /// Sidedef index.
        side: u32,
        /// `upper` / `lower` / `middle` (case-insensitive).
        part: String,
        /// Texture name.
        texture: String,
    },
    /// Set thing skill/flags bits (OR).
    SetThingFlags {
        /// Thing index.
        thing: u32,
        /// Flags to OR in.
        flags: i16,
    },
    /// Set thing XY.
    SetThingXY {
        /// Thing index.
        thing: u32,
        /// X.
        x: i16,
        /// Y.
        y: i16,
    },
    /// Set thing Z (Hexen height).
    SetThingZ {
        /// Thing index.
        thing: u32,
        /// Z.
        z: i16,
    },
    /// Set thing special.
    SetThingSpecial {
        /// Thing index.
        thing: u32,
        /// Special.
        special: u8,
    },
    /// Set one thing argument (`index` 0..4).
    SetThingArgument {
        /// Thing index.
        thing: u32,
        /// Arg slot 0..4.
        index: u8,
        /// Value.
        value: i32,
    },
    /// Move a vertex (map units). Caller may need a node rebuild.
    SetVertex {
        /// Vertex index.
        vertex: u32,
        /// X.
        x: f64,
        /// Y.
        y: f64,
    },
}

/// Parsed `levelfixes.ron`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LevelFixesDatabase {
    /// Map MD5 (lowercase hex) → ordered ops.
    pub entries: BTreeMap<MapMd5Hex, Vec<LevelFixOp>>,
}

impl LevelFixesDatabase {
    /// Parse a RON document: `{ "md5hex": [ Op(...), ... ], ... }`.
    pub fn parse(text: &str) -> Result<Self, MapError> {
        let raw: BTreeMap<String, Vec<LevelFixOp>> =
            ron::from_str(text).map_err(|e| MapError::Message(format!("levelfixes.ron: {e}")))?;
        let mut entries = BTreeMap::new();
        for (k, ops) in raw {
            let key = k.to_ascii_lowercase();
            if key.len() != 32 || !key.chars().all(|c| c.is_ascii_hexdigit()) {
                return Err(MapError::Message(format!(
                    "levelfixes.ron: invalid MD5 key `{k}`"
                )));
            }
            entries.insert(key, ops);
        }
        Ok(Self { entries })
    }

    /// Ops for a map MD5, if any.
    #[must_use]
    pub fn ops_for(&self, md5_hex: &str) -> Option<&[LevelFixOp]> {
        self.entries
            .get(&md5_hex.to_ascii_lowercase())
            .map(Vec::as_slice)
    }

    /// Apply ops for `map.md5` onto `map` (no-op if unknown).
    pub fn apply_to_map(&self, map: &mut MapGeometry) {
        let hex = crate::compat_db::format_md5(&map.md5);
        if let Some(ops) = self.ops_for(&hex) {
            apply_level_fixes(map, ops);
        }
    }
}

/// Apply a list of fix ops to linked geometry.
pub fn apply_level_fixes(map: &mut MapGeometry, ops: &[LevelFixOp]) {
    for op in ops {
        apply_one(map, op);
    }
}

fn apply_one(map: &mut MapGeometry, op: &LevelFixOp) {
    match *op {
        LevelFixOp::ClearSectorTags { sector } => {
            if let Some(s) = map.sectors.get_mut(sector as usize) {
                s.tags.clear();
            }
        }
        LevelFixOp::AddSectorTag { sector, tag } => {
            if let Some(s) = map.sectors.get_mut(sector as usize) {
                if !s.tags.contains(&tag) {
                    s.tags.push(tag);
                }
            }
        }
        LevelFixOp::OffsetSectorPlane {
            sector,
            floor,
            delta,
        } => {
            if let Some(s) = map.sectors.get_mut(sector as usize) {
                if floor {
                    s.floor_height += delta;
                } else {
                    s.ceiling_height += delta;
                }
            }
        }
        LevelFixOp::SetSectorSpecial { sector, special } => {
            if let Some(s) = map.sectors.get_mut(sector as usize) {
                s.special = special;
            }
        }
        LevelFixOp::SetSectorTexture {
            sector,
            floor,
            ref texture,
        } => {
            if let Some(s) = map.sectors.get_mut(sector as usize) {
                if floor {
                    s.floor_pic = texture.clone();
                } else {
                    s.ceiling_pic = texture.clone();
                }
            }
        }
        LevelFixOp::SetSectorLight { sector, light } => {
            if let Some(s) = map.sectors.get_mut(sector as usize) {
                s.light = light;
            }
        }
        LevelFixOp::ClearLineIDs { line } => {
            if let Some(l) = map.lines.get_mut(line as usize) {
                l.tag = 0;
            }
        }
        LevelFixOp::AddLineID { line, id } => {
            if let Some(l) = map.lines.get_mut(line as usize) {
                l.tag = id;
            }
        }
        LevelFixOp::SetLineSpecial {
            line,
            special,
            args,
        } => {
            if let Some(l) = map.lines.get_mut(line as usize) {
                l.special = special;
                l.args = args;
            }
        }
        LevelFixOp::ClearLineSpecial { line } => {
            if let Some(l) = map.lines.get_mut(line as usize) {
                l.special = 0;
                l.args = [0; 5];
            }
        }
        LevelFixOp::SetLineFlags { line, set, clear } => {
            if let Some(l) = map.lines.get_mut(line as usize) {
                l.flags = (l.flags | set) & !clear;
            }
        }
        LevelFixOp::FlipLine { line } => {
            flip_line(map, line as usize, true);
        }
        LevelFixOp::FlipLineSideRefs { line } => {
            flip_line(map, line as usize, false);
        }
        LevelFixOp::SetLineSectorRef { line, side, sector } => {
            let Some(l) = map.lines.get(line as usize) else {
                return;
            };
            let side_id = match side {
                0 => l.sides[0],
                1 => l.sides[1],
                _ => return,
            };
            let Some(sid) = side_id else {
                return;
            };
            if let Some(s) = map.sides.get_mut(sid.0 as usize) {
                s.sector = room_core::SectorId::from_raw(sector);
            }
            if let Some(l) = map.lines.get_mut(line as usize) {
                let sec = Some(room_core::SectorId::from_raw(sector));
                if side == 0 {
                    l.front_sector = sec;
                } else {
                    l.back_sector = sec;
                }
            }
        }
        LevelFixOp::SetWallTexture {
            side,
            ref part,
            ref texture,
        } => {
            if let Some(s) = map.sides.get_mut(side as usize) {
                match part.to_ascii_lowercase().as_str() {
                    "upper" | "top" => s.upper = texture.clone(),
                    "lower" | "bottom" => s.lower = texture.clone(),
                    "middle" | "mid" => s.middle = texture.clone(),
                    _ => {}
                }
            }
        }
        LevelFixOp::SetThingFlags { thing, flags } => {
            if let Some(t) = map.things.get_mut(thing as usize) {
                t.flags |= flags;
            }
        }
        LevelFixOp::SetThingXY { thing, x, y } => {
            if let Some(t) = map.things.get_mut(thing as usize) {
                t.x = x;
                t.y = y;
            }
        }
        LevelFixOp::SetThingZ { thing, z } => {
            if let Some(t) = map.things.get_mut(thing as usize) {
                t.z = z;
            }
        }
        LevelFixOp::SetThingSpecial { thing, special } => {
            if let Some(t) = map.things.get_mut(thing as usize) {
                t.special = special;
            }
        }
        LevelFixOp::SetThingArgument {
            thing,
            index,
            value,
        } => {
            if index < 5 {
                if let Some(t) = map.things.get_mut(thing as usize) {
                    t.args[index as usize] = value;
                }
            }
        }
        LevelFixOp::SetVertex { vertex, x, y } => {
            if map.vertices.get_mut(vertex as usize).is_none() {
                return;
            }
            if let Some(v) = map.vertices.get_mut(vertex as usize) {
                v.pos = DVec2::new(x, y);
            }
            let vid = room_core::VertexId::from_raw(vertex);
            let mut deltas = Vec::new();
            for (i, line) in map.lines.iter().enumerate() {
                if line.v1 == vid || line.v2 == vid {
                    let p1 = map
                        .vertices
                        .get(line.v1.0 as usize)
                        .map(|v| v.pos)
                        .unwrap_or_else(|| DVec2::new(0.0, 0.0));
                    let p2 = map
                        .vertices
                        .get(line.v2.0 as usize)
                        .map(|v| v.pos)
                        .unwrap_or_else(|| DVec2::new(0.0, 0.0));
                    deltas.push((i, DVec2::new(p2.x() - p1.x(), p2.y() - p1.y())));
                }
            }
            for (i, delta) in deltas {
                if let Some(line) = map.lines.get_mut(i) {
                    line.delta = delta;
                }
            }
        }
    }
}

fn flip_line(map: &mut MapGeometry, line_i: usize, swap_verts: bool) {
    let Some(line) = map.lines.get_mut(line_i) else {
        return;
    };
    if swap_verts {
        std::mem::swap(&mut line.v1, &mut line.v2);
        line.delta = DVec2::new(-line.delta.x(), -line.delta.y());
    }
    line.sides.swap(0, 1);
    std::mem::swap(&mut line.front_sector, &mut line.back_sector);
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use crate::geom::{Line, MapFormat, SectorStatic, Side, Vertex};
    use crate::raw::RawThing;
    use room_core::{LineId, SectorId, SideId, VertexId};
    use smallvec::SmallVec;

    fn toy_map() -> MapGeometry {
        MapGeometry {
            name: "MAP01".into(),
            format: MapFormat::Doom,
            vertices: vec![
                Vertex {
                    pos: DVec2::new(0.0, 0.0),
                },
                Vertex {
                    pos: DVec2::new(64.0, 0.0),
                },
            ],
            lines: vec![Line {
                v1: VertexId::from_raw(0),
                v2: VertexId::from_raw(1),
                delta: DVec2::new(64.0, 0.0),
                sides: [Some(SideId::from_raw(0)), None],
                front_sector: Some(SectorId::from_raw(0)),
                back_sector: None,
                flags: 1,
                special: 11,
                tag: 7,
                args: [1, 2, 3, 4, 5],
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
            }],
            sides: vec![Side {
                sector: SectorId::from_raw(0),
                line: LineId::from_raw(0),
                x_offset: 0,
                y_offset: 0,
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
                light: i16::MIN,
                light_absolute: false,
                light_top: i16::MIN,
                light_mid: i16::MIN,
                light_bottom: i16::MIN,
                light_absolute_top: false,
                light_absolute_mid: false,
                light_absolute_bottom: false,
                no_fake_contrast: false,
                smooth_lighting: false,
                scroll_x: 0.0,
                scroll_y: 0.0,
                clip_midtex: false,
                wrap_midtex: false,
                no_decals: false,
                alpha: None,
                block_rendering: false,
                upper: "-".into(),
                lower: "-".into(),
                middle: "STARTAN2".into(),
            }],
            sectors: vec![SectorStatic {
                lines: vec![LineId::from_raw(0)],
                tags: SmallVec::from_buf([99]),
                floor_height: 0.0,
                ceiling_height: 128.0,
                floor_pic: "FLOOR0_1".into(),
                ceiling_pic: "CEIL1_1".into(),
                light: 160,
                special: 1,
                floor_u_off: 0.0,
                floor_v_off: 0.0,
                ceiling_u_off: 0.0,
                ceiling_v_off: 0.0,
                floor_scale_u: 1.0,
                floor_scale_v: 1.0,
                ceiling_scale_u: 1.0,
                ceiling_scale_v: 1.0,
                floor_angle: 0.0,
                ceiling_angle: 0.0,
                floor_alpha: 1.0,
                ceiling_alpha: 1.0,
                floor_additive: false,
                ceiling_additive: false,
                floor_reflection: 0.0,
                ceiling_reflection: 0.0,
                floor_plane: None,
                ceiling_plane: None,
                gravity: -1.0,
                friction: 0,
                damage_amount: 0,
                damage_type: String::new(),
                damage_interval: 0,
                leakiness: 0,
                silent: false,
                sound_sequence: String::new(),
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
            }],
            things: vec![RawThing {
                tid: 0,
                x: 32,
                y: 32,
                z: 0,
                angle: 0,
                type_id: 1,
                flags: 7,
                special: 0,
                args: [0; 5],
                hexen: false,
                friendly_see_blocks: -1,
                health: 0,
                score: -2147483648,
                gravity: -1.0,
                scale_x: 0.0,
                scale_y: 0.0,
                alpha: -1.0,
                pitch: -32768,
                roll: -32768,
                fill_color: -1,
                render_style: String::new(),
                float_bob_phase: -1,
                count_secret: false,
                no_count: false,
                arg0_str: String::new(),
                conversation: 0,
                accuracy: i32::MIN,
                stamina: i32::MIN,
                species: String::new(),
                melee_range: -1.0,
                view_height: -1.0,
                mass: i32::MIN,
                speed: -1.0,
                friction: -1.0,
                damage_factor: -1.0,
                max_step_height: -1.0,
                max_dropoff_height: -1.0,
                float_speed: -1.0,
                fast_speed: -1.0,
                jump_z: -1.0,
                attack_z_offset: -1.0,
                ext: 0,
                user: Default::default(),
            }],
            blockmap: crate::blockmap::Blockmap {
                origin_x: 0,
                origin_y: 0,
                columns: 1,
                rows: 1,
                blocks: vec![vec![]],
            },
            game_nodes: None,
            gl_nodes: None,
            reject: None,
            setup: crate::setup::SetupData::default(),
            warnings: Vec::new(),
            md5: [0xab; 16],
            behavior: None,
            udmf_user: crate::udmf::UdmfUserStore::default(),
        }
    }

    #[test]
    fn parse_and_apply_core_ops() {
        let text = r#"{
            "abababababababababababababababab": [
                ClearLineSpecial(line: 0),
                SetSectorSpecial(sector: 0, special: 0),
                ClearSectorTags(sector: 0),
                AddSectorTag(sector: 0, tag: 42),
                OffsetSectorPlane(sector: 0, floor: true, delta: 16.0),
                SetSectorTexture(sector: 0, floor: true, texture: "FLAT1"),
                SetSectorLight(sector: 0, light: 200),
                SetWallTexture(side: 0, part: "middle", texture: "DOORTRAK"),
                SetThingXY(thing: 0, x: 10, y: 20),
                SetThingZ(thing: 0, z: 48),
                SetThingFlags(thing: 0, flags: 16),
                SetVertex(vertex: 1, x: 128.0, y: 16.0),
                SetLineFlags(line: 0, set: 4, clear: 1),
            ],
        }"#;
        let db = LevelFixesDatabase::parse(text).unwrap();
        let mut map = toy_map();
        assert_eq!(
            crate::compat_db::format_md5(&map.md5),
            "abababababababababababababababab"
        );
        db.apply_to_map(&mut map);
        assert_eq!(map.lines[0].special, 0);
        assert_eq!(map.lines[0].args, [0; 5]);
        assert_eq!(map.sectors[0].special, 0);
        assert_eq!(&map.sectors[0].tags[..], &[42]);
        assert_eq!(map.sectors[0].floor_height, 16.0);
        assert_eq!(map.sectors[0].floor_pic, "FLAT1");
        assert_eq!(map.sectors[0].light, 200);
        assert_eq!(map.sides[0].middle, "DOORTRAK");
        assert_eq!(map.things[0].x, 10);
        assert_eq!(map.things[0].y, 20);
        assert_eq!(map.things[0].z, 48);
        assert_eq!(map.things[0].flags, 7 | 16);
        assert_eq!(map.vertices[1].pos, DVec2::new(128.0, 16.0));
        assert_eq!(map.lines[0].delta, DVec2::new(128.0, 16.0));
        assert_eq!(map.lines[0].flags, (1 | 4) & !1); // 4
    }

    #[test]
    fn rejects_bad_md5_key() {
        let err = LevelFixesDatabase::parse(r#"{ "nope": [] }"#).unwrap_err();
        assert!(err.to_string().contains("invalid MD5"));
    }
}
