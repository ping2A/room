//! Linked map geometry (M3 subset).

use crate::blockmap::Blockmap;
use crate::nodes::{GameNodes, GlNodes};
use crate::raw::{RawLinedef, RawSector, RawSidedef, RawThing, RawVertex};
use crate::reject::Reject;
use room_core::{DVec2, LineId, SectorId, SideId, VertexId};
use smallvec::SmallVec;

/// Map binary / text format.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum MapFormat {
    /// Doom format (no BEHAVIOR).
    #[default]
    Doom,
    /// Hexen format (BEHAVIOR present).
    Hexen,
    /// UDMF (`TEXTMAP`).
    Udmf,
}

/// Immutable map geometry after load/link (nodes/blockmap filled later).
#[derive(Clone, Debug)]
pub struct MapGeometry {
    /// Map lump name (`MAP01`).
    pub name: String,
    /// Format.
    pub format: MapFormat,
    /// Vertices.
    pub vertices: Vec<Vertex>,
    /// Linedefs.
    pub lines: Vec<Line>,
    /// Sidedefs.
    pub sides: Vec<Side>,
    /// Sectors (static fields).
    pub sectors: Vec<SectorStatic>,
    /// Things for spawning.
    pub things: Vec<RawThing>,
    /// Collision blockmap.
    pub blockmap: Blockmap,
    /// Original Doom/Hexen BSP (if present).
    pub game_nodes: Option<GameNodes>,
    /// GL BSP (XGLN/XGL2/XGL3) when present.
    pub gl_nodes: Option<GlNodes>,
    /// REJECT table (if present / valid size).
    pub reject: Option<Reject>,
    /// Load-time slopes / portals / polyobjects / 3D floors.
    pub setup: crate::setup::SetupData,
    /// Warnings collected during load.
    pub warnings: Vec<String>,
    /// MD5 of map lumps (checksum).
    pub md5: [u8; 16],
    /// Hexen/UDMF `BEHAVIOR` lump bytes (ACS bytecode), when present.
    pub behavior: Option<Vec<u8>>,
    /// Custom UDMF `user_*` properties (empty for Doom/Hexen binary maps).
    pub udmf_user: crate::udmf::UdmfUserStore,
}

/// Vertex position.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Vertex {
    /// XY in map units.
    pub pos: DVec2,
}

/// Linked linedef.
#[derive(Clone, Debug)]
pub struct Line {
    /// Start.
    pub v1: VertexId,
    /// End.
    pub v2: VertexId,
    /// Delta v2−v1.
    pub delta: DVec2,
    /// Front / back sides.
    pub sides: [Option<SideId>; 2],
    /// Front sector (from front side).
    pub front_sector: Option<SectorId>,
    /// Back sector.
    pub back_sector: Option<SectorId>,
    /// Flags.
    pub flags: i16,
    /// Special.
    pub special: i16,
    /// Tag / id.
    pub tag: i16,
    /// Hexen args.
    pub args: [i32; 5],
    /// Activation mask (`crate::spac`); 0 = accept any (legacy Doom).
    pub activation: u32,
    /// Mid-texture opacity (`TranslucentLine`; 1 = opaque).
    pub mid_alpha: f32,
    /// Mid-texture additive blend (`TranslucentLine` / UDMF `renderstyle`).
    pub mid_additive: bool,
    /// UDMF `arg0str` — named ACS script (resolved into `args[0]` at ACS init).
    pub arg0_str: String,
    /// UDMF `midtex3d` — actors can walk on the middle texture.
    pub midtex3d: bool,
    /// UDMF `midtex3dimpassible` — height-restricted blocking (missiles ignore).
    pub midtex3d_impassible: bool,
    /// UDMF `checkswitchrange` — Use requires eye Z within the line opening.
    pub check_switch_range: bool,
    /// UDMF `firstsideonly` — activate only from the front side.
    pub first_side_only: bool,
    /// UDMF `blockfloaters` — blocks `+FLOAT` actors.
    pub block_floaters: bool,
    /// UDMF `locknumber` — key lock id (overrides special lock args when ≠ 0).
    pub lock_number: i32,
    /// UDMF `blockprojectiles` — blocks `+MISSILE` actors.
    pub block_projectiles: bool,
    /// UDMF `blocksight` — blocks line-of-sight through the linedef.
    pub block_sight: bool,
    /// UDMF `blockhitscan` — blocks hitscan traces through the linedef.
    pub block_hitscan: bool,
    /// UDMF `moreids` — additional line ids beyond `tag`.
    pub extra_ids: SmallVec<[i32; 2]>,
    /// UDMF `health` — destructible line HP (`0` = unset / immortal).
    pub health: i32,
    /// UDMF `healthgroup` — sync HP with other destructibles (`0` = none).
    pub health_group: i32,
    /// UDMF `automapstyle` — `AML_*` override (`0` = default).
    pub automap_style: i32,
    /// UDMF `clipmidtex` — mid texture clipped to floor/ceiling opening.
    pub clip_midtex: bool,
    /// UDMF `wrapmidtex` — mid texture wraps vertically.
    pub wrap_midtex: bool,
}

impl Line {
    /// True if `id` matches primary `tag` or any `moreids` entry.
    #[must_use]
    pub fn has_id(&self, id: i32) -> bool {
        i32::from(self.tag) == id || self.extra_ids.contains(&id)
    }
}

/// Linked sidedef.
#[derive(Clone, Debug)]
pub struct Side {
    /// Owning sector.
    pub sector: SectorId,
    /// Owning line.
    pub line: LineId,
    /// X offset.
    pub x_offset: i16,
    /// Y offset.
    pub y_offset: i16,
    /// Runtime X scale (`Line_SetTextureScale` / UDMF `scalex_mid`; 1.0 = default).
    pub scale_x: f32,
    /// Runtime Y scale (mid).
    pub scale_y: f32,
    /// UDMF `scalex_top`.
    pub scale_x_top: f32,
    /// UDMF `scaley_top`.
    pub scale_y_top: f32,
    /// UDMF `scalex_bottom`.
    pub scale_x_bottom: f32,
    /// UDMF `scaley_bottom`.
    pub scale_y_bottom: f32,
    /// UDMF `offsetx_top`.
    pub x_offset_top: i16,
    /// UDMF `offsety_top`.
    pub y_offset_top: i16,
    /// UDMF `offsetx_bottom`.
    pub x_offset_bottom: i16,
    /// UDMF `offsety_bottom`.
    pub y_offset_bottom: i16,
    /// UDMF `light` (`i16::MIN` = unset → sector light).
    pub light: i16,
    /// UDMF `lightabsolute`.
    pub light_absolute: bool,
    /// UDMF `light_top` (`i16::MIN` = unset → fall back to `light`).
    pub light_top: i16,
    /// UDMF `light_mid`.
    pub light_mid: i16,
    /// UDMF `light_bottom`.
    pub light_bottom: i16,
    /// UDMF `lightabsolute_top`.
    pub light_absolute_top: bool,
    /// UDMF `lightabsolute_mid`.
    pub light_absolute_mid: bool,
    /// UDMF `lightabsolute_bottom`.
    pub light_absolute_bottom: bool,
    /// UDMF `nofakecontrast`.
    pub no_fake_contrast: bool,
    /// UDMF `smoothlighting` — milder fake contrast.
    pub smooth_lighting: bool,
    /// UDMF `xscroll` — wall texture scroll X (texels / tic).
    pub scroll_x: f32,
    /// UDMF `yscroll` — wall texture scroll Y (texels / tic).
    pub scroll_y: f32,
    /// UDMF `clipmidtex` — mid texture clipped to opening (side-local).
    pub clip_midtex: bool,
    /// UDMF `wrapmidtex` — mid texture wraps vertically (side-local).
    pub wrap_midtex: bool,
    /// UDMF `nodecals` — suppress impact decals on this side.
    pub no_decals: bool,
    /// Upper texture.
    pub upper: String,
    /// Lower texture.
    pub lower: String,
    /// Middle texture.
    pub middle: String,
}

/// Static sector fields.
#[derive(Clone, Debug)]
pub struct SectorStatic {
    /// Bounding lines.
    pub lines: Vec<LineId>,
    /// Tags.
    pub tags: SmallVec<[i32; 1]>,
    /// Floor height (initial).
    pub floor_height: f64,
    /// Ceiling height (initial).
    pub ceiling_height: f64,
    /// Floor flat name.
    pub floor_pic: String,
    /// Ceiling flat name.
    pub ceiling_pic: String,
    /// Light level.
    pub light: i16,
    /// Special.
    pub special: i16,
    /// Runtime floor flat U scroll offset (map units; ÷64 for UV).
    pub floor_u_off: f64,
    /// Runtime floor flat V scroll offset.
    pub floor_v_off: f64,
    /// Runtime ceiling flat U scroll offset (map units; ÷64 for UV).
    pub ceiling_u_off: f64,
    /// Runtime ceiling flat V scroll offset.
    pub ceiling_v_off: f64,
    /// Runtime floor U scale (`Sector_SetFloorScale*`; 1.0 = default).
    pub floor_scale_u: f64,
    /// Runtime floor V scale.
    pub floor_scale_v: f64,
    /// Runtime ceiling U scale.
    pub ceiling_scale_u: f64,
    /// Runtime ceiling V scale.
    pub ceiling_scale_v: f64,
    /// Runtime floor flat rotation degrees.
    pub floor_angle: f64,
    /// Runtime ceiling flat rotation degrees.
    pub ceiling_angle: f64,
    /// Runtime floor opacity (`Sector_SetTranslucent` / UDMF `alphafloor`; 1 = opaque).
    pub floor_alpha: f32,
    /// Runtime ceiling opacity.
    pub ceiling_alpha: f32,
    /// Floor additive blend when translucent (UDMF `renderstyle` Add).
    pub floor_additive: bool,
    /// Ceiling additive blend when translucent.
    pub ceiling_additive: bool,
    /// Floor plane reflection strength (`Sector_SetPlaneReflection`; 0..=1).
    pub floor_reflection: f32,
    /// Ceiling plane reflection strength.
    pub ceiling_reflection: f32,
    /// Optional explicit floor plane (`floorplane_a..d`).
    pub floor_plane: Option<[f64; 4]>,
    /// Optional explicit ceiling plane.
    pub ceiling_plane: Option<[f64; 4]>,
    /// UDMF `gravity` (1.0 = normal); `< 0` = unset.
    pub gravity: f64,
    /// UDMF `friction` Boom amount (0 = unset).
    pub friction: i32,
    /// UDMF `damageamount`.
    pub damage_amount: i32,
    /// UDMF `damagetype`.
    pub damage_type: String,
    /// UDMF `damageinterval` (0 = unset).
    pub damage_interval: i32,
    /// UDMF `leakiness` — 0..=256 radsuit leak chance (256ths).
    pub leakiness: i32,
    /// UDMF `silent` — suppress mover sounds for this sector.
    pub silent: bool,
    /// UDMF `soundsequence` — SNDSEQ name for this sector's movers (empty = default).
    pub sound_sequence: String,
    /// UDMF `waterzone` — filled water; immersion vs dynamic ceiling.
    pub waterzone: bool,
    /// UDMF `dropactors` — ride lowering floors (default true).
    pub drop_actors: bool,
    /// UDMF `nofallingdamage` — disable falling damage on this floor.
    pub no_falling_damage: bool,
    /// UDMF `lightcolor` RRGGBB (`-1` = unset → white).
    pub light_color: i32,
    /// UDMF `fadecolor` RRGGBB (`-1` = unset → black).
    pub fade_color: i32,
    /// UDMF `fogdensity` (0–255).
    pub fog_density: i32,
    /// UDMF `desaturation` (0–255).
    pub desaturation: i32,
    /// UDMF `color_floor` RRGGBB (`-1` = unset).
    pub color_floor: i32,
    /// UDMF `color_ceiling` RRGGBB (`-1` = unset).
    pub color_ceiling: i32,
    /// UDMF `color_walltop` RRGGBB (`-1` = unset).
    pub color_walltop: i32,
    /// UDMF `color_wallbottom` RRGGBB (`-1` = unset).
    pub color_wallbottom: i32,
    /// UDMF `color_sprites` RRGGBB (`-1` = unset).
    pub color_sprites: i32,
    /// UDMF `healthfloor` — destructible floor HP (`0` = unset).
    pub health_floor: i32,
    /// UDMF `healthceiling`.
    pub health_ceiling: i32,
    /// UDMF `health3d`.
    pub health_3d: i32,
}

/// Build linked geometry from raw Doom/Hexen records.
#[allow(clippy::too_many_arguments)]
pub fn link_geometry(
    name: &str,
    format: MapFormat,
    vertices: Vec<RawVertex>,
    linedefs: Vec<RawLinedef>,
    sidedefs: Vec<RawSidedef>,
    sectors: Vec<RawSector>,
    things: Vec<RawThing>,
    md5: [u8; 16],
) -> MapGeometry {
    let positions = vertices
        .iter()
        .map(|v| DVec2::new(f64::from(v.x), f64::from(v.y)))
        .collect();
    link_geometry_at(
        name, format, positions, linedefs, sidedefs, sectors, things, md5,
    )
}

/// [`link_geometry`] with exact vertex positions (UDMF float vertices).
#[allow(clippy::too_many_arguments)]
pub fn link_geometry_at(
    name: &str,
    format: MapFormat,
    positions: Vec<DVec2>,
    linedefs: Vec<RawLinedef>,
    sidedefs: Vec<RawSidedef>,
    sectors: Vec<RawSector>,
    things: Vec<RawThing>,
    md5: [u8; 16],
) -> MapGeometry {
    let nsec = sectors.len();
    let verts: Vec<Vertex> = positions.into_iter().map(|pos| Vertex { pos }).collect();

    let mut sides: Vec<Side> = sidedefs
        .iter()
        .map(|s| {
            let si = usize::from(s.sector);
            let sector = if nsec == 0 {
                SectorId::from_raw(0)
            } else {
                SectorId::from_raw(si.min(nsec - 1) as u32)
            };
            Side {
                sector,
                line: LineId::from_raw(0),
                x_offset: s.x_offset,
                y_offset: s.y_offset,
                scale_x: s.scale_x,
                scale_y: s.scale_y,
                scale_x_top: s.scale_x_top,
                scale_y_top: s.scale_y_top,
                scale_x_bottom: s.scale_x_bottom,
                scale_y_bottom: s.scale_y_bottom,
                x_offset_top: s.x_offset_top,
                y_offset_top: s.y_offset_top,
                x_offset_bottom: s.x_offset_bottom,
                y_offset_bottom: s.y_offset_bottom,
                light: s.light,
                light_absolute: s.light_absolute,
                light_top: s.light_top,
                light_mid: s.light_mid,
                light_bottom: s.light_bottom,
                light_absolute_top: s.light_absolute_top,
                light_absolute_mid: s.light_absolute_mid,
                light_absolute_bottom: s.light_absolute_bottom,
                no_fake_contrast: s.no_fake_contrast,
                smooth_lighting: s.smooth_lighting,
                scroll_x: s.scroll_x,
                scroll_y: s.scroll_y,
                clip_midtex: s.clip_midtex,
                wrap_midtex: s.wrap_midtex,
                no_decals: s.no_decals,
                upper: s.upper.clone(),
                lower: s.lower.clone(),
                middle: s.middle.clone(),
            }
        })
        .collect();

    let mut lines = Vec::with_capacity(linedefs.len());
    for (li, ld) in linedefs.iter().enumerate() {
        let v1 = VertexId::from_raw(u32::from(ld.v1));
        let v2 = VertexId::from_raw(u32::from(ld.v2));
        let p1 = verts
            .get(usize::from(ld.v1))
            .map(|v| v.pos)
            .unwrap_or_else(|| DVec2::new(0.0, 0.0));
        let p2 = verts
            .get(usize::from(ld.v2))
            .map(|v| v.pos)
            .unwrap_or_else(|| DVec2::new(0.0, 0.0));
        let front = if ld.side_front == 0xFFFF {
            None
        } else {
            Some(SideId::from_raw(u32::from(ld.side_front)))
        };
        let back = if ld.side_back == 0xFFFF {
            None
        } else {
            Some(SideId::from_raw(u32::from(ld.side_back)))
        };
        if let Some(SideId(si)) = front {
            if let Some(side) = sides.get_mut(si as usize) {
                side.line = LineId::from_raw(li as u32);
            }
        }
        if let Some(SideId(si)) = back {
            if let Some(side) = sides.get_mut(si as usize) {
                side.line = LineId::from_raw(li as u32);
            }
        }
        let front_sector = front.and_then(|SideId(si)| sides.get(si as usize).map(|s| s.sector));
        let back_sector = back.and_then(|SideId(si)| sides.get(si as usize).map(|s| s.sector));
        let xlat = crate::xlat::translate_doom_linedef(ld);
        // UDMF uses explicit playeruse/… bits (`ld.activation`); Hexen binary
        // packs SPAC in flags bits 10–12. Doom generalized sets `xlat.activation`.
        let activation = if xlat.activation != 0 {
            xlat.activation
        } else if ld.activation != 0 {
            ld.activation
        } else if format == MapFormat::Hexen {
            crate::spac::from_hexen_flags(xlat.flags)
        } else {
            0
        };
        lines.push(Line {
            v1,
            v2,
            delta: DVec2::new(p2.x() - p1.x(), p2.y() - p1.y()),
            sides: [front, back],
            front_sector,
            back_sector,
            flags: xlat.flags,
            special: xlat.special,
            tag: ld.tag,
            args: xlat.args,
            activation,
            mid_alpha: ld.alpha,
            mid_additive: ld.mid_additive,
            arg0_str: ld.arg0_str.clone(),
            midtex3d: ld.midtex3d,
            midtex3d_impassible: ld.midtex3d_impassible,
            check_switch_range: ld.check_switch_range,
            first_side_only: ld.first_side_only,
            block_floaters: ld.block_floaters,
            lock_number: ld.lock_number,
            block_projectiles: ld.block_projectiles,
            block_sight: ld.block_sight,
            block_hitscan: ld.block_hitscan,
            extra_ids: ld.extra_ids.iter().copied().collect(),
            health: ld.health,
            health_group: ld.health_group,
            automap_style: ld.automap_style,
            clip_midtex: ld.clip_midtex,
            wrap_midtex: ld.wrap_midtex,
        });
    }

    let mut sec_out: Vec<SectorStatic> = sectors
        .iter()
        .map(|s| {
            let mut tags = SmallVec::new();
            if s.tag != 0 {
                tags.push(i32::from(s.tag));
            }
            for &id in &s.extra_ids {
                if id != 0 && !tags.contains(&id) {
                    tags.push(id);
                }
            }
            SectorStatic {
                lines: Vec::new(),
                tags,
                floor_height: f64::from(s.floor_height),
                ceiling_height: f64::from(s.ceiling_height),
                floor_pic: s.floor_pic.clone(),
                ceiling_pic: s.ceiling_pic.clone(),
                light: s.light,
                special: s.special,
                floor_u_off: s.floor_x_pan,
                floor_v_off: s.floor_y_pan,
                ceiling_u_off: s.ceiling_x_pan,
                ceiling_v_off: s.ceiling_y_pan,
                floor_scale_u: s.floor_x_scale,
                floor_scale_v: s.floor_y_scale,
                ceiling_scale_u: s.ceiling_x_scale,
                ceiling_scale_v: s.ceiling_y_scale,
                floor_angle: s.floor_rotation,
                ceiling_angle: s.ceiling_rotation,
                floor_alpha: s.floor_alpha,
                ceiling_alpha: s.ceiling_alpha,
                floor_additive: s.floor_additive,
                ceiling_additive: s.ceiling_additive,
                floor_reflection: 0.0,
                ceiling_reflection: 0.0,
                floor_plane: s.floor_plane,
                ceiling_plane: s.ceiling_plane,
                gravity: s.gravity,
                friction: s.friction,
                damage_amount: s.damage_amount,
                damage_type: s.damage_type.clone(),
                damage_interval: s.damage_interval,
                leakiness: s.leakiness,
                silent: s.silent,
                sound_sequence: s.sound_sequence.clone(),
                waterzone: s.waterzone,
                drop_actors: s.drop_actors,
                no_falling_damage: s.no_falling_damage,
                light_color: s.light_color,
                fade_color: s.fade_color,
                fog_density: s.fog_density,
                desaturation: s.desaturation,
                color_floor: s.color_floor,
                color_ceiling: s.color_ceiling,
                color_walltop: s.color_walltop,
                color_wallbottom: s.color_wallbottom,
                color_sprites: s.color_sprites,
                health_floor: s.health_floor,
                health_ceiling: s.health_ceiling,
                health_3d: s.health_3d,
            }
        })
        .collect();

    for (li, line) in lines.iter().enumerate() {
        let id = LineId::from_raw(li as u32);
        if let Some(SectorId(s)) = line.front_sector {
            if let Some(sec) = sec_out.get_mut(s as usize) {
                sec.lines.push(id);
            }
        }
        if let Some(SectorId(s)) = line.back_sector {
            if line.front_sector != line.back_sector {
                if let Some(sec) = sec_out.get_mut(s as usize) {
                    sec.lines.push(id);
                }
            }
        }
    }

    let blockmap = crate::blockmap::build_blockmap(&verts, &lines);
    let reject = crate::reject::build_reject(&lines, sec_out.len());
    let mut geom = MapGeometry {
        name: name.to_ascii_uppercase(),
        format,
        vertices: verts,
        lines,
        sides,
        sectors: sec_out,
        things,
        blockmap,
        game_nodes: None,
        gl_nodes: None,
        reject: Some(reject),
        setup: crate::setup::SetupData::default(),
        warnings: Vec::new(),
        md5,
        behavior: None,
        udmf_user: crate::udmf::UdmfUserStore::default(),
    };
    crate::setup::apply_attach_3d_midtex(&mut geom);
    geom.setup = crate::setup::build_setup(&geom);
    geom
}
