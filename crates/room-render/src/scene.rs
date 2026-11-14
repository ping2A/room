//! CPU scene lists from BSP visibility (walls + flats).

use crate::bsp_walk::{VisibleSet, walk_bsp};
use crate::camera::Camera;
use crate::lightmap::{LightmapBindings, LightmapSurface, world_xy_to_surface_uv};
use crate::lights::ShadowOccluder;
use crate::mesh::{FlatVertex, WallVertex};
use crate::sky::{CubemapFaces, cubemap_sky, cylinder_sky};
use crate::sprites::{SpriteInstance, emit_sprite_quad};
use glam::{Vec2, Vec3};
use room_map::{GameNodes, GlNodes, MapGeometry, Seg};

/// Reorder sprite quads far-to-near (painter's order), as GZDoom sorts sprites.
///
/// Sprites write depth even where their texels are only partly opaque (soft
/// PNG edges, `Alpha 0.4` smoke and blood). Drawn in actor order, a smoke or
/// blood quad in front of a monster could land first and depth-reject the
/// monster behind it, so enemies blinked as Brutal Doom's particles came and
/// went. Far-to-near order draws the monster first; whatever lies in front is
/// blended over it. Equal distances keep their order (stable sort), so an
/// overlay at its owner's position never swaps with it from frame to frame.
#[must_use]
pub fn sprites_back_to_front(
    verts: &[crate::mesh::WallVertex],
    ranges: &[MaterialRange],
    eye: glam::Vec3,
) -> (Vec<crate::mesh::WallVertex>, Vec<MaterialRange>) {
    let mut keyed: Vec<(f32, &MaterialRange)> = ranges
        .iter()
        .filter_map(|r| {
            let (s, n) = (r.start as usize, r.count as usize);
            let quad = verts.get(s..s + n)?;
            if quad.is_empty() {
                return None;
            }
            let mut c = glam::Vec3::ZERO;
            for v in quad {
                c += glam::Vec3::from_array(v.pos);
            }
            #[allow(clippy::cast_precision_loss)]
            let c = c / quad.len() as f32;
            Some((c.distance_squared(eye), r))
        })
        .collect();
    keyed.sort_by(|a, b| b.0.total_cmp(&a.0));
    let mut out_v = Vec::with_capacity(verts.len());
    let mut out_r = Vec::with_capacity(keyed.len());
    for (_, r) in keyed {
        #[allow(clippy::cast_possible_truncation)]
        let start = out_v.len() as u32;
        let (s, n) = (r.start as usize, r.count as usize);
        out_v.extend_from_slice(&verts[s..s + n]);
        out_r.push(MaterialRange {
            name: r.name.clone(),
            start,
            count: r.count,
            flags: r.flags,
        });
    }
    (out_v, out_r)
}

/// Contiguous draw range sharing one material name.
#[derive(Clone, Debug)]
pub struct MaterialRange {
    /// Doom texture / flat name (`"-"` / empty = default).
    pub name: String,
    /// First vertex index into the matching buffer.
    pub start: u32,
    /// Vertex count.
    pub count: u32,
    /// [`MaterialRange::SUBTRACT`] / [`MaterialRange::NO_BRIGHTMAP`] bits.
    pub flags: u8,
}

impl MaterialRange {
    /// Sprite range drawn with reverse-subtract blending (`STYLE_Subtract`):
    /// its vertices carry the additive flag (premultiplied colour, alpha 0).
    pub const SUBTRACT: u8 = 1;
    /// Draw with the default shader: no GLDEFS brightmap (UZDoom particles
    /// without `SPF_ALLOWSHADERS` force `OverrideShader = 0`).
    pub const NO_BRIGHTMAP: u8 = 2;
    /// `+NODYNAMICLIGHTING` actor: dynamic lights skip this range.
    pub const NO_DYNLIGHT: u8 = 4;
}

/// Draw lists for one frame (CPU side).
#[derive(Clone, Debug, Default)]
pub struct SceneLists {
    /// Opaque wall triangles (two tris per quad).
    pub opaque_walls: Vec<WallVertex>,
    /// GZDoom "sky walls": depth-only quads from a sky ceiling's edge upward,
    /// so geometry behind a sky-capped wall stays hidden (vanilla look).
    /// UDMF `noskywalls` on the line or sector suppresses them.
    pub sky_walls: Vec<WallVertex>,
    /// Wall material ranges (parallel to [`Self::opaque_walls`]).
    pub wall_ranges: Vec<MaterialRange>,
    /// Translucent mid walls (`TranslucentLine`).
    pub translucent_walls: Vec<WallVertex>,
    /// Translucent wall material ranges.
    pub translucent_wall_ranges: Vec<MaterialRange>,
    /// Floor / ceiling triangles.
    pub flats: Vec<FlatVertex>,
    /// Flat material ranges.
    pub flat_ranges: Vec<MaterialRange>,
    /// Translucent floor / ceiling triangles (`Sector_SetTranslucent`).
    pub translucent_flats: Vec<FlatVertex>,
    /// Translucent flat material ranges.
    pub translucent_flat_ranges: Vec<MaterialRange>,
    /// Classic cylinder / cubemap sky (drawn after opaque, before translucent).
    pub sky_parts: Vec<WallVertex>,
    /// Per-face (cubemap) or single (cylinder) sky material ranges.
    pub sky_ranges: Vec<MaterialRange>,
    /// Sky texture / flat name (`SKY1`, …); empty = untextured default bind.
    /// Used when [`Self::sky_ranges`] is empty.
    pub sky_texture: String,
    /// Sprite billboards.
    pub sprites: Vec<WallVertex>,
    /// Sprite material ranges (Doom sprite lumps).
    pub sprite_ranges: Vec<MaterialRange>,
    /// Subsectors visited this frame.
    pub visible_subsectors: Vec<u32>,
    /// `Line_Mirror` surfaces drawn this frame (for canvas refresh).
    pub mirrors: Vec<MirrorSurface>,
    /// Linked/visual line portals (displaced camera canvas).
    pub linked_portals: Vec<LinkedPortalSurface>,
    /// Floor/ceiling plane reflections (`Sector_SetPlaneReflection`).
    pub plane_reflects: Vec<PlaneReflectSurface>,
    /// Dynamic lights visible this frame (GLDEFS / muzzle — M10 bootstrap).
    pub dyn_lights: Vec<DynLightInstance>,
    /// One-sided linedef segments for 1D dyn-light shadowmaps.
    pub shadow_occluders: Vec<ShadowOccluder>,
    /// FLMP / ZDRay surface UV rects (empty = no per-vertex lightmap UVs).
    pub lightmap_surfaces: Vec<LightmapSurface>,
    /// Side / sector → surface index.
    pub lightmap_bindings: LightmapBindings,
    /// TEXTUREX sizes (uppercase name → w,h) for wall UV; empty = assume 128×128.
    pub(crate) wall_tex_sizes: std::collections::BTreeMap<String, (f32, f32)>,
}

/// One dynamic light for the frame (CPU → future clustered shading).
#[derive(Clone, Debug, PartialEq)]
pub struct DynLightInstance {
    /// World position.
    pub pos: Vec3,
    /// RGB intensity 0..=1.
    pub color: [f32; 3],
    /// Radius in map units.
    pub radius: f32,
    /// Subtractive light.
    pub subtractive: bool,
    /// Spotlight direction (unit); `None` / zero = omni point.
    pub spot_dir: Option<Vec3>,
    /// Cosine of outer cone half-angle (`1` = omni).
    pub spot_outer_cos: f32,
    /// Cosine of inner cone half-angle (≥ outer cos).
    pub spot_inner_cos: f32,
}

impl DynLightInstance {
    /// Omni point light.
    #[must_use]
    pub fn point(pos: Vec3, color: [f32; 3], radius: f32) -> Self {
        Self {
            pos,
            color,
            radius,
            subtractive: false,
            spot_dir: None,
            spot_outer_cos: 1.0,
            spot_inner_cos: 1.0,
        }
    }

    /// Spotlight with direction and outer/inner half-angles in degrees.
    #[must_use]
    pub fn spot(
        pos: Vec3,
        color: [f32; 3],
        radius: f32,
        dir: Vec3,
        outer_deg: f32,
        inner_deg: f32,
    ) -> Self {
        let outer = outer_deg.to_radians().max(0.01);
        let inner = inner_deg.to_radians().clamp(0.0, outer);
        Self {
            pos,
            color,
            radius,
            subtractive: false,
            spot_dir: Some(dir.normalize_or_zero()),
            spot_outer_cos: outer.cos(),
            spot_inner_cos: inner.cos(),
        }
    }
}

/// One reflective floor or ceiling that needs a mirrored-Z canvas.
#[derive(Clone, Debug, PartialEq)]
pub struct PlaneReflectSurface {
    /// Sector index.
    pub sector: u32,
    /// Plane Z (map units).
    pub plane_z: f64,
    /// `true` = floor, `false` = ceiling.
    pub is_floor: bool,
    /// Material name (`__PREFL{n}F` / `__PREFL{n}C`).
    pub tex_name: String,
    /// Reflection strength 0..=1 (canvas blend weight later).
    pub amount: f32,
}

/// Canvas name for a reflective sector plane.
#[must_use]
pub fn plane_reflect_tex_name(sector: u32, floor: bool) -> String {
    if floor {
        format!("__PREFL{sector}F")
    } else {
        format!("__PREFL{sector}C")
    }
}

/// One `Line_Mirror` wall that needs a reflected canvas.
#[derive(Clone, Debug, PartialEq)]
pub struct MirrorSurface {
    /// Linedef index.
    pub line: u32,
    /// Segment endpoints (map XY).
    pub ax: f64,
    /// Y0.
    pub ay: f64,
    /// X1.
    pub bx: f64,
    /// Y1.
    pub by: f64,
    /// Material name (`__MIRROR{n}`).
    pub tex_name: String,
}

/// One linked/visual line portal that needs a displaced-camera canvas.
#[derive(Clone, Debug, PartialEq)]
pub struct LinkedPortalSurface {
    /// Source linedef index.
    pub line: u32,
    /// Destination linedef index.
    pub target: u32,
    /// Source segment endpoints (map XY).
    pub ax: f64,
    /// Y0.
    pub ay: f64,
    /// X1.
    pub bx: f64,
    /// Y1.
    pub by: f64,
    /// Add to eye XY to view through the portal (`target_mid - source_mid`).
    pub dx: f64,
    /// Y displacement.
    pub dy: f64,
    /// Material name (`__PORTAL{n}`).
    pub tex_name: String,
}

/// Synthetic texture name for a mirror linedef canvas.
#[must_use]
pub fn mirror_tex_name(line: u32) -> String {
    format!("__MIRROR{line}")
}

/// Synthetic texture name for a linked portal canvas.
#[must_use]
pub fn portal_tex_name(line: u32) -> String {
    format!("__PORTAL{line}")
}

/// Whether a portal offscreen view at `depth` should omit nested `__PORTAL*` quads.
///
/// Depth `0` is the first hop from the eye camera; at
/// [`crate::MAX_PORTAL_RECURSION`] hops, nested portals are stripped.
#[must_use]
pub fn portal_view_strips_portals(depth: u32) -> bool {
    depth + 1 >= crate::MAX_PORTAL_RECURSION
}

/// Build wall/flat geometry for the camera's map XY.
///
/// When `live_sprites` is `Some`, those billboards replace static map-thing
/// placeholders (play mode). Pass `None` in the free-fly viewer.
#[must_use]
pub fn build_scene(
    map: &MapGeometry,
    camera: &Camera,
    live_sprites: Option<&[SpriteInstance]>,
) -> SceneLists {
    build_scene_with_sky(map, camera, live_sprites, None)
}

/// Like [`build_scene`], with an optional MAPINFO/`sky1` texture name.
#[must_use]
pub fn build_scene_with_sky(
    map: &MapGeometry,
    camera: &Camera,
    live_sprites: Option<&[SpriteInstance]>,
    sky_texture: Option<&str>,
) -> SceneLists {
    build_scene_with_sky_ex(map, camera, live_sprites, sky_texture, None)
}

/// Like [`build_scene_with_sky`], with an optional GLDEFS cubemap.
#[must_use]
pub fn build_scene_with_sky_ex(
    map: &MapGeometry,
    camera: &Camera,
    live_sprites: Option<&[SpriteInstance]>,
    sky_texture: Option<&str>,
    cubemap: Option<&CubemapFaces>,
) -> SceneLists {
    build_scene_with_sky_ex_sized(map, camera, live_sprites, sky_texture, cubemap, None)
}

/// Like [`build_scene_with_sky_ex`], with TEXTUREX sizes for correct wall UV.
#[must_use]
pub fn build_scene_with_sky_ex_sized(
    map: &MapGeometry,
    camera: &Camera,
    live_sprites: Option<&[SpriteInstance]>,
    sky_texture: Option<&str>,
    cubemap: Option<&CubemapFaces>,
    wall_tex_sizes: Option<&std::collections::BTreeMap<String, (f32, f32)>>,
) -> SceneLists {
    let (vx, vy) = camera.map_xy();
    let vis = walk_bsp(map, vx, vy);
    let mut lists = build_scene_from_visible_ex(map, &vis, wall_tex_sizes);
    // One draw (and bind) per material instead of one per subsector/seg.
    coalesce_by_material(&mut lists.flats, &mut lists.flat_ranges);
    coalesce_by_material(&mut lists.opaque_walls, &mut lists.wall_ranges);
    apply_sky(&mut lists, sky_texture, cubemap);
    // Keep the sky cylinder/cubemap centred on the eye so outdoor `F_SKY1` holes
    // always look into the shell (mesh is authored at the origin).
    let eye = camera.eye;
    for v in &mut lists.sky_parts {
        v.pos[0] += eye.x;
        v.pos[1] += eye.y;
        v.pos[2] += eye.z;
    }
    match live_sprites {
        Some(sprites) => append_sprites_in_map(&mut lists, sprites, camera, Some(map)),
        None => emit_thing_sprites(map, camera, &mut lists),
    }
    lists
}

fn apply_sky(lists: &mut SceneLists, sky_texture: Option<&str>, cubemap: Option<&CubemapFaces>) {
    if let Some(faces) = cubemap.filter(|f| f.is_complete()) {
        let (verts, names) = cubemap_sky(4096.0, faces, 1.0);
        lists.sky_parts = verts;
        lists.sky_ranges = names
            .into_iter()
            .enumerate()
            .map(|(i, name)| MaterialRange {
                name,
                start: (i as u32) * 6,
                count: 6,
                flags: 0,
            })
            .collect();
        lists.sky_texture.clear();
        return;
    }
    if let Some(name) = sky_texture {
        if !name.is_empty() {
            lists.sky_texture = name.to_owned();
            lists.sky_ranges = crate::sky::cylinder_sky_ranges(name, lists.sky_parts.len());
        }
    }
}

/// Force fullbright on material ranges whose names appear in `glow_names`
/// (GLDEFS `Glow` flats/walls/`fullbright` textures).
pub fn apply_glow_fullbright(
    lists: &mut SceneLists,
    glow_names: &std::collections::BTreeSet<String>,
) {
    if glow_names.is_empty() {
        return;
    }
    let bump = |ranges: &[MaterialRange], verts: &mut [crate::mesh::WallVertex]| {
        for r in ranges {
            if !glow_names.contains(&r.name) {
                continue;
            }
            let start = r.start as usize;
            let end = start.saturating_add(r.count as usize).min(verts.len());
            for v in &mut verts[start..end] {
                v.light = 1.0;
            }
        }
    };
    let bump_flat = |ranges: &[MaterialRange], verts: &mut [crate::mesh::FlatVertex]| {
        for r in ranges {
            if !glow_names.contains(&r.name) {
                continue;
            }
            let start = r.start as usize;
            let end = start.saturating_add(r.count as usize).min(verts.len());
            for v in &mut verts[start..end] {
                v.light = 1.0;
            }
        }
    };
    bump(&lists.wall_ranges, &mut lists.opaque_walls);
    bump(&lists.translucent_wall_ranges, &mut lists.translucent_walls);
    bump_flat(&lists.flat_ranges, &mut lists.flats);
    bump_flat(&lists.translucent_flat_ranges, &mut lists.translucent_flats);
    bump(&lists.sprite_ranges, &mut lists.sprites);
}

/// Boost wall/flat/sprite vertex lights near dynamic lights (M10 CPU stand-in for
/// clustered shading). Fullbright verts (`light ≥ 0.999`) are left alone.
pub fn apply_dyn_lights(lists: &mut SceneLists) {
    if lists.dyn_lights.is_empty() {
        return;
    }
    let lights = lists.dyn_lights.clone();
    let boost = |pos: [f32; 3], light: &mut f32| {
        if *light >= 0.999 {
            return;
        }
        let p = Vec3::from_array(pos);
        let mut add = 0.0f32;
        for l in &lights {
            let d = p.distance(l.pos);
            if d >= l.radius || l.radius <= 1e-3 {
                continue;
            }
            let falloff = (1.0 - d / l.radius).max(0.0);
            let intensity = (l.color[0] + l.color[1] + l.color[2]) / 3.0;
            let contrib = falloff * falloff * intensity;
            if l.subtractive {
                add -= contrib;
            } else {
                add += contrib;
            }
        }
        *light = (*light + add).clamp(0.05, 1.0);
    };
    for v in &mut lists.opaque_walls {
        boost(v.pos, &mut v.light);
    }
    for v in &mut lists.translucent_walls {
        boost(v.pos, &mut v.light);
    }
    for v in &mut lists.flats {
        boost(v.pos, &mut v.light);
    }
    for v in &mut lists.translucent_flats {
        boost(v.pos, &mut v.light);
    }
    // `+NODYNAMICLIGHTING` sprite ranges are left alone.
    let mut skip = vec![false; lists.sprites.len()];
    for r in &lists.sprite_ranges {
        if r.flags & MaterialRange::NO_DYNLIGHT != 0 {
            let end = (r.start + r.count) as usize;
            for s in skip.iter_mut().take(end.min(lists.sprites.len())).skip(r.start as usize) {
                *s = true;
            }
        }
    }
    for (v, skip) in lists.sprites.iter_mut().zip(skip) {
        if !skip {
            boost(v.pos, &mut v.light);
        }
    }
}

/// Apply CPU dynamic lighting to an appended sprite range.
///
/// Particle billboards are appended after [`apply_dyn_lights`], so they use
/// this helper at insertion time and can honor `SPF_NODYNAMICLIGHTING`.
pub(crate) fn apply_dyn_lights_to_sprite_range(
    lists: &mut SceneLists,
    range: std::ops::Range<u32>,
) {
    if lists.dyn_lights.is_empty() {
        return;
    }
    let lights = &lists.dyn_lights;
    let start = range.start as usize;
    let end = (range.end as usize).min(lists.sprites.len());
    for vertex in lists.sprites.iter_mut().take(end).skip(start) {
        if vertex.light >= 0.999 {
            continue;
        }
        let pos = Vec3::from_array(vertex.pos);
        let mut add = 0.0f32;
        for light in lights {
            let distance = pos.distance(light.pos);
            if distance >= light.radius || light.radius <= 1e-3 {
                continue;
            }
            let falloff = (1.0 - distance / light.radius).max(0.0);
            let intensity = (light.color[0] + light.color[1] + light.color[2]) / 3.0;
            let contribution = falloff * falloff * intensity;
            if light.subtractive {
                add -= contribution;
            } else {
                add += contribution;
            }
        }
        vertex.light = (vertex.light + add).clamp(0.05, 1.0);
    }
}

/// Shift a surface sprite a fraction of a unit toward the camera along `normal`.
fn lift_sprite_off_surface(quad: &mut [crate::mesh::WallVertex], eye: Vec3, normal: Vec3) {
    let n = normal.normalize_or_zero();
    if n.length_squared() < 1e-6 {
        return;
    }
    let p = Vec3::from_array(quad[0].pos);
    let side = if n.dot(eye - p) >= 0.0 { n } else { -n };
    let shift = side * 0.35;
    for v in quad {
        v.pos = (Vec3::from_array(v.pos) + shift).to_array();
    }
}

/// Append pre-built sprite instances as Y-billboards facing the camera.
pub fn append_sprites(lists: &mut SceneLists, sprites: &[SpriteInstance], camera: &Camera) {
    append_sprites_in_map(lists, sprites, camera, None);
}

/// [`append_sprites`], de-clipping billboards against `map` walls first
/// (see [`declip_billboard`]).
pub fn append_sprites_in_map(
    lists: &mut SceneLists,
    sprites: &[SpriteInstance],
    camera: &Camera,
    map: Option<&MapGeometry>,
) {
    let mut scratch = Vec::new();
    for inst in sprites {
        #[allow(clippy::cast_possible_truncation)]
        let start = lists.sprites.len() as u32;
        let mut quad = emit_sprite_quad(inst.clone(), camera.forward);
        let eye = camera.eye;
        if let (Some(map), crate::sprites::SpriteOrient::Billboard) = (map, inst.orient) {
            declip_billboard(map, eye, inst.pos, &mut quad, &mut scratch);
        }
        match inst.orient {
            // Nudge a billboard toward the eye so feet and walls cannot win the
            // depth test and bury it (common HW Doom-port fix).
            crate::sprites::SpriteOrient::Billboard => {
                // A large pull (the old 1.5–8 unit shove) lifts a face-sprite
                // tag off the dumpster and parks it in front of the fire.
                // A fraction of a unit is enough to win the depth test.
                for v in &mut quad {
                    let p = Vec3::from_array(v.pos);
                    let to_eye = eye - p;
                    let dist = to_eye.length();
                    if dist > 1.0 {
                        v.pos = (p + to_eye * (0.35 / dist)).to_array();
                    }
                }
            }
            // Wall and floor sprites sit on a surface. Pulling them toward the
            // eye lifts a tag off its wall and turns a marquee into streaks.
            // A fraction of a unit along the normal only avoids z-fighting.
            crate::sprites::SpriteOrient::Wall { yaw } => {
                let n = Vec3::new(-yaw.cos(), -yaw.sin(), 0.0);
                lift_sprite_off_surface(&mut quad, eye, n);
            }
            crate::sprites::SpriteOrient::Flat { .. } => {
                lift_sprite_off_surface(&mut quad, eye, Vec3::Z);
            }
            crate::sprites::SpriteOrient::Facing { .. } => {}
        }
        lists.sprites.extend_from_slice(&quad);
        let mut flags = 0;
        if inst.fx.subtract {
            flags |= MaterialRange::SUBTRACT;
        }
        if inst.fx.no_dynlight {
            flags |= MaterialRange::NO_DYNLIGHT;
        }
        lists.sprite_ranges.push(MaterialRange {
            name: inst.texture.clone(),
            start,
            count: 6,
            flags,
        });
    }
}

/// Lines that can hide a sprite: one-sided walls, and two-sided lines with an
/// upper or lower wall (floor or ceiling heights differ) or a mid texture.
fn line_can_occlude(map: &MapGeometry, line: &room_map::Line) -> bool {
    let (Some(f), Some(b)) = (line.front_sector, line.back_sector) else {
        return true;
    };
    let (Some(fs), Some(bs)) = (
        map.sectors.get(f.raw() as usize),
        map.sectors.get(b.raw() as usize),
    ) else {
        return true;
    };
    if (fs.floor_height - bs.floor_height).abs() > 0.5
        || (fs.ceiling_height - bs.ceiling_height).abs() > 0.5
    {
        return true;
    }
    line.sides.iter().flatten().any(|sid| {
        map.sides
            .get(sid.raw() as usize)
            .is_some_and(|sd| !sd.middle.is_empty() && sd.middle != "-")
    })
}

fn cross2(a: Vec2, b: Vec2) -> f32 {
    a.x * b.y - a.y * b.x
}

/// Parameter `t` along `p0→p1` where it crosses segment `q0→q1`, if it does.
fn seg_cross_t(p0: Vec2, p1: Vec2, q0: Vec2, q1: Vec2) -> Option<f32> {
    let r = p1 - p0;
    let s = q1 - q0;
    let den = cross2(r, s);
    if den.abs() < 1e-9 {
        return None;
    }
    let qp = q0 - p0;
    let t = cross2(qp, s) / den;
    let u = cross2(qp, r) / den;
    ((0.0..=1.0).contains(&t) && (0.0..=1.0).contains(&u)).then_some(t)
}

/// Keep a billboard from being cut by walls its actor stands in front of.
///
/// Doom's software renderer never clips a sprite by a wall when the actor is
/// on the viewer's side of that wall (`R_DrawSprite` drawseg test). With a
/// depth buffer, a billboard wider than its actor (most Brutal Doom frames,
/// any monster beside a wall seen off-axis) pokes through the wall and is cut,
/// or vanishes near corners. This scales the quad toward the eye: every vertex
/// slides along its own view ray, so the picture on screen is unchanged and
/// only the depth moves. It moves just far enough to clear such walls, never
/// past a wall that stands between the eye and the actor, and never more than
/// half the sprite width plus 16 units.
pub fn declip_billboard(
    map: &MapGeometry,
    eye: Vec3,
    actor: Vec3,
    quad: &mut [crate::mesh::WallVertex; 6],
    scratch: &mut Vec<u32>,
) {
    let eye2 = eye.truncate();
    let c = actor.truncate();
    let to_c = c - eye2;
    let dist = to_c.length();
    if dist < 24.0 {
        return;
    }
    // quad[0] / quad[1] are the bottom-left / bottom-right corners.
    let e1 = Vec2::new(quad[0].pos[0], quad[0].pos[1]);
    let e2 = Vec2::new(quad[1].pos[0], quad[1].pos[1]);
    let half = (e2 - e1).length() * 0.5;
    let max_pull = (half + 16.0).min(dist - 16.0);
    if max_pull <= 0.0 {
        return;
    }
    let s_lo = 1.0 - max_pull / dist;
    let pulled = eye2 + to_c * s_lo;
    let lo = e1.min(e2).min(c).min(pulled) - Vec2::splat(2.0);
    let hi = e1.max(e2).max(c).max(pulled) + Vec2::splat(2.0);

    scratch.clear();
    let bm = &map.blockmap;
    if bm.columns > 0 && bm.rows > 0 && !bm.blocks.is_empty() {
        let ox = f32::from(bm.origin_x);
        let oy = f32::from(bm.origin_y);
        let cols = i32::from(bm.columns);
        let rows = i32::from(bm.rows);
        #[allow(clippy::cast_possible_truncation)]
        let cell = |v: f32, o: f32, n: i32| (((v - o) / 128.0).floor() as i32).clamp(0, n - 1);
        for by in cell(lo.y, oy, rows)..=cell(hi.y, oy, rows) {
            for bx in cell(lo.x, ox, cols)..=cell(hi.x, ox, cols) {
                #[allow(clippy::cast_sign_loss)]
                let idx = (by * cols + bx) as usize;
                if let Some(list) = bm.blocks.get(idx) {
                    scratch.extend(list.iter().map(|l| l.raw()));
                }
            }
        }
        scratch.sort_unstable();
        scratch.dedup();
    } else {
        #[allow(clippy::cast_possible_truncation)]
        scratch.extend(0..map.lines.len() as u32);
    }

    let mut want = 1.0f32; // largest scale that clears every wall the actor faces
    let mut floor = 0.0f32; // smallest scale that stays behind walls in front of it
    for &li in scratch.iter() {
        let Some(line) = map.lines.get(li as usize) else {
            continue;
        };
        let (Some(v1), Some(v2)) = (
            map.vertices.get(line.v1.raw() as usize),
            map.vertices.get(line.v2.raw() as usize),
        ) else {
            continue;
        };
        #[allow(clippy::cast_possible_truncation)]
        let p = Vec2::new(v1.pos.x() as f32, v1.pos.y() as f32);
        #[allow(clippy::cast_possible_truncation)]
        let q = Vec2::new(v2.pos.x() as f32, v2.pos.y() as f32);
        if p.max(q).cmplt(lo).any() || p.min(q).cmpgt(hi).any() {
            continue;
        }
        if !line_can_occlude(map, line) {
            continue;
        }
        let d = q - p;
        let se = cross2(d, eye2 - p);
        if se.abs() < 1e-3 {
            continue;
        }
        let sc = cross2(d, c - p);
        if se * sc > 0.0 {
            // Actor on the viewer's side: pull any far-side corner back over.
            for e in [e1, e2] {
                if cross2(d, e - p) * se >= 0.0 || seg_cross_t(c, e, p, q).is_none() {
                    continue;
                }
                let k = cross2(d, e - eye2);
                if k.abs() < 1e-6 {
                    continue;
                }
                let s_star = -se / k;
                let len = (e - eye2).length().max(1.0);
                want = want.min(s_star - 0.5 / len);
            }
        } else if let Some(t) = seg_cross_t(eye2, c, p, q) {
            // A wall between the eye and the actor must keep hiding it.
            floor = floor.max(t + 0.5 / dist);
        }
    }
    let s = want.max(s_lo).max(floor).min(1.0);
    if s >= 0.9999 {
        return;
    }
    for v in quad.iter_mut() {
        let pos = Vec3::from_array(v.pos);
        v.pos = (eye + (pos - eye) * s).to_array();
    }
}

/// Collect one-sided linedefs as XY occluders for dyn-light 1D shadowmaps.
#[must_use]
pub fn collect_shadow_occluders(map: &MapGeometry) -> Vec<ShadowOccluder> {
    let mut out = Vec::with_capacity(map.lines.len() / 2);
    for line in &map.lines {
        if line.back_sector.is_some() {
            continue;
        }
        let Some(v1) = map.vertices.get(line.v1.raw() as usize) else {
            continue;
        };
        let Some(v2) = map.vertices.get(line.v2.raw() as usize) else {
            continue;
        };
        out.push(ShadowOccluder {
            a: Vec2::new(v1.pos.x() as f32, v1.pos.y() as f32),
            b: Vec2::new(v2.pos.x() as f32, v2.pos.y() as f32),
        });
    }
    out
}

/// Build from an existing visibility set (tests / offscreen).
#[must_use]
pub fn build_scene_from_visible(map: &MapGeometry, vis: &VisibleSet) -> SceneLists {
    build_scene_from_visible_ex(map, vis, None)
}

/// Build from visibility, with optional TEXTUREX sizes for wall UV.
#[must_use]
pub fn build_scene_from_visible_ex(
    map: &MapGeometry,
    vis: &VisibleSet,
    wall_tex_sizes: Option<&std::collections::BTreeMap<String, (f32, f32)>>,
) -> SceneLists {
    let mut lists = SceneLists {
        visible_subsectors: vis.subsectors.clone(),
        shadow_occluders: collect_shadow_occluders(map),
        wall_tex_sizes: wall_tex_sizes.cloned().unwrap_or_default(),
        ..Default::default()
    };
    let Some(nodes) = map.game_nodes.as_ref() else {
        emit_polyobj_walls(map, &mut lists);
        emit_ffloor_sides(map, &mut lists);
        emit_linked_portal_canvases(map, &mut lists);
        // Tall closed shell: old height 1024 only covered ~±7° vertical FOV.
        lists.sky_parts = cylinder_sky(4096.0, 8192.0, 32, 1.0);
        return lists;
    };
    // Walls from BSP segs. Floors from each visible subsector's leaf polygon:
    // classic SEGS omit partition minisegs, but BSP leaves are convex, so the
    // convex hull of seg endpoints recovers the leaf (or a safe inner approx).
    // Sector-outline ear-clip is only for sectors that got zero leaf area.
    let mut flats_needed = vec![false; map.sectors.len()];
    let skip_lines = skip_wall_line_set(map);
    let parents = BspParents::new(map, nodes);
    for &ss in &vis.subsectors {
        emit_subsector(map, nodes, ss, &mut lists, &mut flats_needed, &skip_lines);
        for (si, ring) in game_subsector_flat_rings(map, nodes, &parents, ss) {
            emit_flats_for_sector_trusted(map, si, &ring, &mut lists);
        }
    }
    // Always supplement leaf hulls with sector-outline ear-clip. Hulls miss
    // partition gaps; outline ears that poke into neighbours are rejected by
    // the midpoint ownership filter. Same-Z overdraw is harmless.
    for (si, needed) in flats_needed.iter().enumerate() {
        if !*needed {
            continue;
        }
        let mut rings = sector_outline_rings(map, si);
        if rings.is_empty() {
            continue;
        }
        rings.sort_by(|a, b| {
            ring_abs_area(b)
                .partial_cmp(&ring_abs_area(a))
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        for ring in rings {
            if ring.len() >= 3 {
                emit_flats_for_sector(map, si, &ring, &mut lists);
            }
        }
    }
    emit_polyobj_walls(map, &mut lists);
    emit_ffloor_sides(map, &mut lists);
    emit_linked_portal_canvases(map, &mut lists);
    // Classic sky shell centred on origin; translated to the eye after build.
    lists.sky_parts = cylinder_sky(4096.0, 8192.0, 32, 1.0);
    lists
}

fn skip_wall_line_set(map: &MapGeometry) -> std::collections::BTreeSet<u32> {
    let mut s: std::collections::BTreeSet<u32> = map
        .setup
        .polyobjects
        .iter()
        .flat_map(|p| p.lines.iter().map(|l| l.raw()))
        .collect();
    for p in &map.setup.portals.lines {
        if p.skip_wall() {
            s.insert(p.line.raw());
        }
    }
    s
}

/// Extra walls for polyobjects (BSP segs stay at spawn; verts move in-place).
fn emit_polyobj_walls(map: &MapGeometry, lists: &mut SceneLists) {
    for po in &map.setup.polyobjects {
        for &lid in &po.lines {
            let Some(line) = map.lines.get(lid.raw() as usize) else {
                continue;
            };
            let fake = Seg {
                v1: line.v1,
                v2: line.v2,
                angle: 0,
                line: Some(lid),
                side: 0,
                offset: 0,
            };
            // Poly doors keep the raised floor on the near side, so the step
            // texture faces the far sector. Paint it on this face too.
            emit_seg_walls(map, &fake, lists, true);
            if line.sides[1].is_some() {
                let mut back = fake;
                back.side = 1;
                emit_seg_walls(map, &back, lists, true);
            }
        }
    }
}

fn emit_thing_sprites(map: &MapGeometry, camera: &Camera, lists: &mut SceneLists) {
    // Player start (type 1) is the camera — skip. Other things get placeholder pillars.
    for thing in &map.things {
        if thing.type_id == 1 {
            continue;
        }
        #[allow(clippy::cast_possible_truncation)]
        let pos = Vec3::new(f32::from(thing.x), f32::from(thing.y), 0.0);
        let inst = SpriteInstance {
            pos,
            width: 32.0,
            height: 56.0,
            light: 1.0,
            alpha: 1.0,
            fuzz: false,
            texture: String::new(),
            flip_u: false,
            anchor_u: 0.5,
            orient: crate::sprites::SpriteOrient::Billboard,
            additive: false,
            roll: 0.0,
            fx: Default::default(),
        };
        #[allow(clippy::cast_possible_truncation)]
        let start = lists.sprites.len() as u32;
        let quad = emit_sprite_quad(inst, camera.forward);
        lists.sprites.extend_from_slice(&quad);
        lists.sprite_ranges.push(MaterialRange {
            name: String::new(),
            start,
            count: 6,
            flags: 0,
        });
    }
}

fn emit_subsector(
    map: &MapGeometry,
    nodes: &GameNodes,
    ss: u32,
    lists: &mut SceneLists,
    flats_needed: &mut [bool],
    skip_lines: &std::collections::BTreeSet<u32>,
) {
    let Some(sub) = nodes.subsectors.get(ss as usize) else {
        return;
    };
    let start = sub.first_seg as usize;
    let end = start + sub.num_segs as usize;
    let segs = match nodes.segs.get(start..end) {
        Some(s) => s,
        None => return,
    };

    if let Some(sid) = segs.iter().find_map(|seg| sector_from_seg(map, seg)) {
        let si = sid.raw() as usize;
        if si < flats_needed.len() {
            flats_needed[si] = true;
        }
    }

    for seg in segs {
        if let Some(lid) = seg.line {
            if skip_lines.contains(&lid.raw()) {
                continue;
            }
        }
        emit_seg_walls(map, seg, lists, false);
    }
}

fn sector_from_seg(map: &MapGeometry, seg: &Seg) -> Option<room_core::SectorId> {
    let line_id = seg.line?;
    let line = map.lines.get(line_id.raw() as usize)?;
    let side_i = usize::from(seg.side != 0);
    let side_id = line.sides[side_i]?;
    let side = map.sides.get(side_id.raw() as usize)?;
    Some(side.sector)
}

/// One or two leaf polygons for a classic game subsector.
///
/// Emits the convex hull of seg endpoints (tight) and, when useful, the BSP
/// partition-clipped leaf (fills corners where classic `SEGS` omit partition
/// vertices). Same-sector overdraw is harmless.
fn game_subsector_flat_rings(
    map: &MapGeometry,
    nodes: &GameNodes,
    parents: &BspParents,
    ss: u32,
) -> Vec<(usize, Vec<(f64, f64)>)> {
    let mut out = Vec::new();
    let Some(sub) = nodes.subsectors.get(ss as usize) else {
        return out;
    };
    let start = sub.first_seg as usize;
    let Some(end) = start.checked_add(sub.num_segs as usize) else {
        return out;
    };
    let Some(segs) = nodes.segs.get(start..end) else {
        return out;
    };
    if segs.is_empty() {
        return out;
    }
    let Some(sector_i) = segs.iter().find_map(|seg| {
        let sid = sector_from_seg(map, seg)?;
        Some(sid.raw() as usize)
    }) else {
        return out;
    };
    let mut pts = Vec::with_capacity(segs.len() * 2);
    for seg in segs {
        let Some(a) = map.vertices.get(seg.v1.raw() as usize) else {
            return out;
        };
        let Some(b) = map.vertices.get(seg.v2.raw() as usize) else {
            return out;
        };
        pts.push((a.pos.x(), a.pos.y()));
        pts.push((b.pos.x(), b.pos.y()));
    }
    let hull = convex_hull_xy(&pts);
    if hull.len() >= 3 && ring_abs_area(&hull) > 1.0 {
        out.push((sector_i, hull));
    }
    if let Some(bsp) = leaf_ring_from_bsp(nodes, parents, ss) {
        if ring_abs_area(&bsp) > 1.0 {
            // Skip near-duplicate of the hull.
            let dup = out.last().is_some_and(|(_, h)| {
                (ring_abs_area(h) - ring_abs_area(&bsp)).abs() < 1.0 && h.len() == bsp.len()
            });
            if !dup {
                out.push((sector_i, bsp));
            }
        }
    }
    out
}

/// Per-frame BSP lookup: `(parent node, child side)` for every node and
/// subsector, plus the padded map bounds leaf clipping starts from.
struct BspParents {
    node: Vec<Option<(usize, usize)>>,
    leaf: Vec<Option<(usize, usize)>>,
    bounds: [(f64, f64); 4],
}

impl BspParents {
    fn new(map: &MapGeometry, nodes: &GameNodes) -> Self {
        let mut node = vec![None; nodes.nodes.len()];
        let mut leaf = vec![None; nodes.subsectors.len()];
        for (ni, n) in nodes.nodes.iter().enumerate() {
            for (side, &child) in n.children.iter().enumerate() {
                let slot = if child & room_map::NF_SUBSECTOR != 0 {
                    leaf.get_mut((child & !room_map::NF_SUBSECTOR) as usize)
                } else {
                    node.get_mut(child as usize)
                };
                if let Some(slot) = slot {
                    *slot = Some((ni, side));
                }
            }
        }
        let mut min_x = f64::MAX;
        let mut max_x = f64::MIN;
        let mut min_y = f64::MAX;
        let mut max_y = f64::MIN;
        for v in &map.vertices {
            min_x = min_x.min(v.pos.x());
            max_x = max_x.max(v.pos.x());
            min_y = min_y.min(v.pos.y());
            max_y = max_y.max(v.pos.y());
        }
        let pad = 64.0;
        Self {
            node,
            leaf,
            bounds: [
                (min_x - pad, min_y - pad),
                (max_x + pad, min_y - pad),
                (max_x + pad, max_y + pad),
                (min_x - pad, max_y + pad),
            ],
        }
    }

    /// Root→leaf path as `(node_index, child_side)` steps.
    fn path(&self, ss: u32) -> Option<Vec<(usize, usize)>> {
        let mut path = Vec::new();
        let mut cur = (*self.leaf.get(ss as usize)?)?;
        loop {
            path.push(cur);
            if path.len() > self.node.len() {
                return None;
            }
            match self.node.get(cur.0).copied().flatten() {
                Some(up) => cur = up,
                None => break,
            }
        }
        path.reverse();
        Some(path)
    }
}

/// Clip the map bounding quad by every partition on the path to `ss`.
fn leaf_ring_from_bsp(nodes: &GameNodes, bsp: &BspParents, ss: u32) -> Option<Vec<(f64, f64)>> {
    if nodes.nodes.is_empty() {
        return None;
    }
    let path = bsp.path(ss)?;
    let mut poly = bsp.bounds.to_vec();
    for (ni, side) in path {
        let node = nodes.nodes.get(ni)?;
        poly = clip_poly_halfplane(poly, node, side);
        if poly.len() < 3 {
            return None;
        }
    }
    (poly.len() >= 3).then_some(poly)
}

/// Keep the half-plane for `side` of a Doom BSP partition (Sutherland–Hodgman).
fn clip_poly_halfplane(
    poly: Vec<(f64, f64)>,
    node: &room_map::Node,
    side: usize,
) -> Vec<(f64, f64)> {
    if poly.is_empty() {
        return poly;
    }
    let nx = f64::from(node.x);
    let ny = f64::from(node.y);
    let dx = f64::from(node.dx);
    let dy = f64::from(node.dy);
    // Match nodebuild `point_on_side`: side 0 ⇔ (p.x-nx)*dy - (p.y-ny)*dx > 0.
    let signed = |p: (f64, f64)| (p.0 - nx) * dy - (p.1 - ny) * dx;
    let inside = |p: (f64, f64)| {
        let s = signed(p);
        if side == 0 { s > -1e-9 } else { s < 1e-9 }
    };
    let mut out = Vec::with_capacity(poly.len() + 2);
    for i in 0..poly.len() {
        let cur = poly[i];
        let prev = poly[(i + poly.len() - 1) % poly.len()];
        let cur_in = inside(cur);
        let prev_in = inside(prev);
        if cur_in {
            if !prev_in {
                if let Some(hit) = intersect_partition(prev, cur, nx, ny, dx, dy) {
                    out.push(hit);
                }
            }
            out.push(cur);
        } else if prev_in {
            if let Some(hit) = intersect_partition(prev, cur, nx, ny, dx, dy) {
                out.push(hit);
            }
        }
    }
    out
}

fn intersect_partition(
    a: (f64, f64),
    b: (f64, f64),
    nx: f64,
    ny: f64,
    dx: f64,
    dy: f64,
) -> Option<(f64, f64)> {
    let ax = a.0 - nx;
    let ay = a.1 - ny;
    let bx = b.0 - nx;
    let by = b.1 - ny;
    let sa = ax * dy - ay * dx;
    let sb = bx * dy - by * dx;
    let denom = sa - sb;
    if denom.abs() < 1e-12 {
        return None;
    }
    let t = sa / denom;
    Some((a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t))
}

/// Monotone-chain convex hull in XY (`CCW`, no repeated closing vertex).
fn convex_hull_xy(points: &[(f64, f64)]) -> Vec<(f64, f64)> {
    let mut pts: Vec<(f64, f64)> = points.to_vec();
    if pts.len() < 3 {
        return pts;
    }
    pts.sort_by(|a, b| {
        a.0.partial_cmp(&b.0)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
    });
    pts.dedup_by(|a, b| (a.0 - b.0).abs() < 1e-6 && (a.1 - b.1).abs() < 1e-6);
    if pts.len() < 3 {
        return pts;
    }
    let cross = |o: (f64, f64), a: (f64, f64), b: (f64, f64)| {
        (a.0 - o.0) * (b.1 - o.1) - (a.1 - o.1) * (b.0 - o.0)
    };
    let mut lower = Vec::new();
    for &p in &pts {
        while lower.len() >= 2 && cross(lower[lower.len() - 2], lower[lower.len() - 1], p) <= 0.0 {
            lower.pop();
        }
        lower.push(p);
    }
    let mut upper = Vec::new();
    for &p in pts.iter().rev() {
        while upper.len() >= 2 && cross(upper[upper.len() - 2], upper[upper.len() - 1], p) <= 0.0 {
            upper.pop();
        }
        upper.push(p);
    }
    lower.pop();
    upper.pop();
    lower.extend(upper);
    lower
}

/// Closed XY ring for a GL subsector (minisegs included), plus owning sector.
#[allow(dead_code)]
fn gl_subsector_flat_ring(
    map: &MapGeometry,
    gl: &GlNodes,
    ss: u32,
) -> Option<(usize, Vec<(f64, f64)>)> {
    let sub = gl.subsectors.get(ss as usize)?;
    let start = sub.first_seg as usize;
    let end = start.checked_add(sub.num_segs as usize)?;
    let segs = gl.segs.get(start..end)?;
    if segs.is_empty() {
        return None;
    }
    let sector_i = segs.iter().find_map(|seg| {
        let line_id = seg.line?;
        let line = map.lines.get(line_id.raw() as usize)?;
        let side_id = line.sides[usize::from(seg.side != 0)]?;
        let side = map.sides.get(side_id.raw() as usize)?;
        Some(side.sector.raw() as usize)
    })?;
    let mut pts = Vec::with_capacity(segs.len());
    for seg in segs {
        let v = map.vertices.get(seg.v1.raw() as usize)?;
        pts.push((v.pos.x(), v.pos.y()));
    }
    // Prefer convex hull — GL rings can still be listed in a non-convex order.
    let ring = convex_hull_xy(&pts);
    (ring.len() >= 3).then_some((sector_i, ring))
}

fn ring_abs_area(ring: &[(f64, f64)]) -> f64 {
    ring_signed_area(ring).abs()
}

/// All closed boundary loops for a sector (outer + holes / multi-regions).
fn sector_outline_rings(map: &MapGeometry, sector_i: usize) -> Vec<Vec<(f64, f64)>> {
    let Some(sector) = map.sectors.get(sector_i) else {
        return Vec::new();
    };
    let mut edges: Vec<((f64, f64), (f64, f64))> = Vec::new();
    for &room_core::LineId(li) in &sector.lines {
        let Some(line) = map.lines.get(li as usize) else {
            continue;
        };
        let facing = match (line.front_sector, line.back_sector) {
            (Some(room_core::SectorId(s)), _) if s as usize == sector_i => 0i16,
            (_, Some(room_core::SectorId(s))) if s as usize == sector_i => 1i16,
            _ => continue,
        };
        let (va, vb) = if facing == 0 {
            (line.v1, line.v2)
        } else {
            (line.v2, line.v1)
        };
        let Some(a) = map.vertices.get(va.raw() as usize) else {
            continue;
        };
        let Some(b) = map.vertices.get(vb.raw() as usize) else {
            continue;
        };
        edges.push(((a.pos.x(), a.pos.y()), (b.pos.x(), b.pos.y())));
    }
    let mut rings = Vec::new();
    let mut used = vec![false; edges.len()];
    for start_i in 0..edges.len() {
        if used[start_i] {
            continue;
        }
        let mut ring = Vec::with_capacity(edges.len());
        ring.push(edges[start_i].0);
        let mut cur = edges[start_i].1;
        used[start_i] = true;
        ring.push(cur);
        for _ in 1..edges.len() {
            let mut found = None;
            for (i, e) in edges.iter().enumerate() {
                if used[i] {
                    continue;
                }
                if (e.0.0 - cur.0).abs() < 1e-4 && (e.0.1 - cur.1).abs() < 1e-4 {
                    found = Some((i, e.1));
                    break;
                }
                if (e.1.0 - cur.0).abs() < 1e-4 && (e.1.1 - cur.1).abs() < 1e-4 {
                    found = Some((i, e.0));
                    break;
                }
            }
            let Some((i, next)) = found else {
                break;
            };
            used[i] = true;
            cur = next;
            ring.push(cur);
        }
        if let (Some(&first), Some(&last)) = (ring.first(), ring.last()) {
            if ring.len() > 1 && (first.0 - last.0).abs() < 1e-4 && (first.1 - last.1).abs() < 1e-4
            {
                ring.pop();
            }
        }
        if ring.len() >= 3 {
            rings.push(ring);
        }
    }
    rings
}

/// Signed area of a polygon (`> 0` = CCW when viewed from +Z).
fn ring_signed_area(ring: &[(f64, f64)]) -> f64 {
    let n = ring.len();
    if n < 3 {
        return 0.0;
    }
    let mut a = 0.0;
    for i in 0..n {
        let (x0, y0) = ring[i];
        let (x1, y1) = ring[(i + 1) % n];
        a += x0 * y1 - x1 * y0;
    }
    a * 0.5
}

/// Ensure ring vertices are CCW from +Z (required for floor normals / backface cull).
fn ring_ccw(ring: &[(f64, f64)]) -> Vec<(f64, f64)> {
    if ring_signed_area(ring) >= 0.0 {
        ring.to_vec()
    } else {
        let mut r = ring.to_vec();
        r.reverse();
        r
    }
}

/// Ear-clip a simple CCW ring into triangles (handles concave outlines).
///
/// If clipping stalls (degenerate / near-colinear verts), fall back to a fan of
/// the remainder so floors never leave clear holes into the sky.
fn earclip_tris(ring: &[(f64, f64)]) -> Vec<[(f64, f64); 3]> {
    let n = ring.len();
    if n < 3 {
        return Vec::new();
    }
    if n == 3 {
        return vec![[ring[0], ring[1], ring[2]]];
    }
    let mut idx: Vec<usize> = (0..n).collect();
    let mut out = Vec::with_capacity(n.saturating_sub(2));
    let mut guard = 0usize;
    while idx.len() > 3 && guard < n * n {
        guard += 1;
        let m = idx.len();
        let mut clipped = false;
        for i in 0..m {
            let i0 = idx[(i + m - 1) % m];
            let i1 = idx[i];
            let i2 = idx[(i + 1) % m];
            let a = ring[i0];
            let b = ring[i1];
            let c = ring[i2];
            // Convex ear at b (CCW ring ⇒ cross > 0).
            let cross = (b.0 - a.0) * (c.1 - a.1) - (b.1 - a.1) * (c.0 - a.0);
            if cross <= 1e-9 {
                continue;
            }
            let mut has_point = false;
            for &j in &idx {
                if j == i0 || j == i1 || j == i2 {
                    continue;
                }
                if point_in_tri(ring[j], a, b, c) {
                    has_point = true;
                    break;
                }
            }
            if has_point {
                continue;
            }
            out.push([a, b, c]);
            idx.remove(i);
            clipped = true;
            break;
        }
        if !clipped {
            // Fan the remainder from idx[0] so concave leftovers still fill.
            let i0 = idx[0];
            for k in 1..idx.len().saturating_sub(1) {
                out.push([ring[i0], ring[idx[k]], ring[idx[k + 1]]]);
            }
            idx.clear();
            break;
        }
    }
    if idx.len() == 3 {
        out.push([ring[idx[0]], ring[idx[1]], ring[idx[2]]]);
    }
    out
}

fn point_in_tri(p: (f64, f64), a: (f64, f64), b: (f64, f64), c: (f64, f64)) -> bool {
    let sign = |p1: (f64, f64), p2: (f64, f64), p3: (f64, f64)| {
        (p1.0 - p3.0) * (p2.1 - p3.1) - (p2.0 - p3.0) * (p1.1 - p3.1)
    };
    let b1 = sign(p, a, b) < 0.0;
    let b2 = sign(p, b, c) < 0.0;
    let b3 = sign(p, c, a) < 0.0;
    b1 == b2 && b2 == b3
}

fn emit_flats_for_sector(
    map: &MapGeometry,
    sector_i: usize,
    ring: &[(f64, f64)],
    lists: &mut SceneLists,
) {
    let Some(sec) = map.sectors.get(sector_i) else {
        return;
    };
    let light = doom_light(sec.light);
    let floor_light = (light * rgb_luma(sec.color_floor)).clamp(0.0, 1.0);
    let ceil_light = (light * rgb_luma(sec.color_ceiling)).clamp(0.0, 1.0);
    if ring.len() < 3 {
        return;
    }
    let ring = ring_ccw(ring);
    // Ear-clip triangulation + sector ownership filter. Sloped sectors use plane Z.
    // Walls use live `sec.floor_height` (lerped from sim). Setup planes bake the
    // load-time shape — add `(live - base)` so flats stay locked to walls on
    // steps, lifts, and crushers (matches `sector_z_at` in playsim).
    let planes = map.setup.planes.get(sector_i);
    let z_floor = |x: f64, y: f64| match planes {
        Some(p) => p.floor.z_at(x, y) + (sec.floor_height - p.base_floor),
        None => sec.floor_height,
    };
    let z_ceil = |x: f64, y: f64| match planes {
        Some(p) => p.ceiling.z_at(x, y) + (sec.ceiling_height - p.base_ceiling),
        None => sec.ceiling_height,
    };
    let skip_floor = is_sky_flat(&sec.floor_pic)
        || map.setup.portals.sectors.iter().any(|p| {
            p.sector.raw() as usize == sector_i
                && p.plane == 0
                && p.kind != room_map::PORTAL_KIND_FALL
        });
    // `F_SKY1` is a redirect marker — never draw it as a ceiling flat (sky cylinder
    // shows through). Drawing the sky graphic as a horizontal plane looks inverted.
    let skip_ceil = is_sky_flat(&sec.ceiling_pic)
        || map
            .setup
            .portals
            .sectors
            .iter()
            .any(|p| p.sector.raw() as usize == sector_i && p.plane == 1);
    let floor_tex = if sec.floor_reflection > 0.01 {
        let name = plane_reflect_tex_name(sector_i as u32, true);
        if !lists
            .plane_reflects
            .iter()
            .any(|p| p.sector == sector_i as u32 && p.is_floor)
        {
            lists.plane_reflects.push(PlaneReflectSurface {
                sector: sector_i as u32,
                plane_z: sec.floor_height,
                is_floor: true,
                tex_name: name.clone(),
                amount: sec.floor_reflection,
            });
        }
        name
    } else {
        sec.floor_pic.clone()
    };
    let ceil_tex = if sec.ceiling_reflection > 0.01 {
        let name = plane_reflect_tex_name(sector_i as u32, false);
        if !lists
            .plane_reflects
            .iter()
            .any(|p| p.sector == sector_i as u32 && !p.is_floor)
        {
            lists.plane_reflects.push(PlaneReflectSurface {
                sector: sector_i as u32,
                plane_z: sec.ceiling_height,
                is_floor: false,
                tex_name: name.clone(),
                amount: sec.ceiling_reflection,
            });
        }
        name
    } else {
        sec.ceiling_pic.clone()
    };
    let want = room_core::SectorId(sector_i as u32);
    for tri in earclip_tris(&ring) {
        let [(xa, ya), (xb, yb), (xc, yc)] = tri;
        // Strict: outline rings are sector-shaped but ear-clip still spills across
        // steps into neighbours (transparent risers when high floor overhangs).
        if !tri_belongs_to_sector(map, want, (xa, ya), (xb, yb), (xc, yc)) {
            continue;
        }
        push_floor_ceil_tri(
            lists,
            sec,
            sector_i as u32,
            &floor_tex,
            &ceil_tex,
            floor_light,
            ceil_light,
            skip_floor,
            skip_ceil,
            &z_floor,
            &z_ceil,
            (xa, ya),
            (xb, yb),
            (xc, yc),
        );
    }
    emit_ffloor_flats(map, sector_i, &ring, lists);
}

/// Emit flats for a BSP/GL leaf ring.
///
/// Leaf hulls of classic `SEGS` are convex and can fill bays that belong to
/// neighbours (E1M1 courtyard step: high floor overhanging the drop). Still
/// run the ownership filter per triangle — same as [`emit_flats_for_sector`].
fn emit_flats_for_sector_trusted(
    map: &MapGeometry,
    sector_i: usize,
    ring: &[(f64, f64)],
    lists: &mut SceneLists,
) {
    let Some(sec) = map.sectors.get(sector_i) else {
        return;
    };
    let light = doom_light(sec.light);
    let floor_light = (light * rgb_luma(sec.color_floor)).clamp(0.0, 1.0);
    let ceil_light = (light * rgb_luma(sec.color_ceiling)).clamp(0.0, 1.0);
    if ring.len() < 3 {
        return;
    }
    let ring = ring_ccw(ring);
    let planes = map.setup.planes.get(sector_i);
    let z_floor = |x: f64, y: f64| match planes {
        Some(p) => p.floor.z_at(x, y) + (sec.floor_height - p.base_floor),
        None => sec.floor_height,
    };
    let z_ceil = |x: f64, y: f64| match planes {
        Some(p) => p.ceiling.z_at(x, y) + (sec.ceiling_height - p.base_ceiling),
        None => sec.ceiling_height,
    };
    let skip_floor = is_sky_flat(&sec.floor_pic)
        || map.setup.portals.sectors.iter().any(|p| {
            p.sector.raw() as usize == sector_i
                && p.plane == 0
                && p.kind != room_map::PORTAL_KIND_FALL
        });
    let skip_ceil = is_sky_flat(&sec.ceiling_pic)
        || map
            .setup
            .portals
            .sectors
            .iter()
            .any(|p| p.sector.raw() as usize == sector_i && p.plane == 1);
    let floor_tex = if sec.floor_reflection > 0.01 {
        let name = plane_reflect_tex_name(sector_i as u32, true);
        if !lists
            .plane_reflects
            .iter()
            .any(|p| p.sector == sector_i as u32 && p.is_floor)
        {
            lists.plane_reflects.push(PlaneReflectSurface {
                sector: sector_i as u32,
                plane_z: sec.floor_height,
                is_floor: true,
                tex_name: name.clone(),
                amount: sec.floor_reflection,
            });
        }
        name
    } else {
        sec.floor_pic.clone()
    };
    let ceil_tex = if sec.ceiling_reflection > 0.01 {
        let name = plane_reflect_tex_name(sector_i as u32, false);
        if !lists
            .plane_reflects
            .iter()
            .any(|p| p.sector == sector_i as u32 && !p.is_floor)
        {
            lists.plane_reflects.push(PlaneReflectSurface {
                sector: sector_i as u32,
                plane_z: sec.ceiling_height,
                is_floor: false,
                tex_name: name.clone(),
                amount: sec.ceiling_reflection,
            });
        }
        name
    } else {
        sec.ceiling_pic.clone()
    };
    // Always ear-clip: BSP/GL leaves can be concave; a fan from ring[0] spills
    // triangles into neighboring sectors (wrong Z / clear holes).
    let want = room_core::SectorId(sector_i as u32);
    for tri in earclip_tris(&ring) {
        let [(xa, ya), (xb, yb), (xc, yc)] = tri;
        if !tri_belongs_to_sector(map, want, (xa, ya), (xb, yb), (xc, yc)) {
            continue;
        }
        push_floor_ceil_tri(
            lists,
            sec,
            sector_i as u32,
            &floor_tex,
            &ceil_tex,
            floor_light,
            ceil_light,
            skip_floor,
            skip_ceil,
            &z_floor,
            &z_ceil,
            (xa, ya),
            (xb, yb),
            (xc, yc),
        );
    }
    emit_ffloor_flats(map, sector_i, &ring, lists);
}

fn tri_belongs_to_sector(
    map: &MapGeometry,
    want: room_core::SectorId,
    a: (f64, f64),
    b: (f64, f64),
    c: (f64, f64),
) -> bool {
    let sector_at = |x: f64, y: f64| {
        room_nodebuild::point_in_sector(map, x, y)
            .or_else(|| room_nodebuild::point_in_sector_vanilla(map, x, y))
    };
    let hit = |x: f64, y: f64| sector_at(x, y) == Some(want);
    let cx = (a.0 + b.0 + c.0) / 3.0;
    let cy = (a.1 + b.1 + c.1) / 3.0;
    // Majority of centroid + edge midpoints. Requiring *all* four rejected thin
    // ears along sector borders and left sky holes in E1M1-style rooms.
    let samples = [
        (cx, cy),
        ((a.0 + b.0) * 0.5, (a.1 + b.1) * 0.5),
        ((b.0 + c.0) * 0.5, (b.1 + c.1) * 0.5),
        ((c.0 + a.0) * 0.5, (c.1 + a.1) * 0.5),
    ];
    let votes = samples.iter().filter(|&&(x, y)| hit(x, y)).count();
    if votes < 2 || !hit(cx, cy) {
        return false;
    }
    // Near-corner samples reject convex SEGS-hull / outline ears that spill
    // across a step into a neighbour (high floor overhang → transparent riser).
    let near_corners = [
        (cx * 0.1 + a.0 * 0.9, cy * 0.1 + a.1 * 0.9),
        (cx * 0.1 + b.0 * 0.9, cy * 0.1 + b.1 * 0.9),
        (cx * 0.1 + c.0 * 0.9, cy * 0.1 + c.1 * 0.9),
    ];
    for &(x, y) in &near_corners {
        if let Some(sid) = sector_at(x, y) {
            if sid != want {
                return false;
            }
        }
    }
    true
}

fn push_floor_ceil_tri(
    lists: &mut SceneLists,
    sec: &room_map::SectorStatic,
    sector_raw: u32,
    floor_tex: &str,
    ceil_tex: &str,
    floor_light: f32,
    ceil_light: f32,
    skip_floor: bool,
    skip_ceil: bool,
    z_floor: &dyn Fn(f64, f64) -> f64,
    z_ceil: &dyn Fn(f64, f64) -> f64,
    a: (f64, f64),
    b: (f64, f64),
    c: (f64, f64),
) {
    let (xa, ya) = a;
    let (xb, yb) = b;
    let (xc, yc) = c;
    if !skip_floor {
        push_flat_tri_oriented_ex(
            lists,
            floor_tex,
            [
                (xa, ya, z_floor(xa, ya)),
                (xb, yb, z_floor(xb, yb)),
                (xc, yc, z_floor(xc, yc)),
            ],
            floor_light,
            FlatUv {
                u_off: sec.floor_u_off,
                v_off: sec.floor_v_off,
                scale_u: sec.floor_scale_u,
                scale_v: sec.floor_scale_v,
                angle_deg: sec.floor_angle,
            },
            sec.floor_alpha,
            true,
            Some(sector_raw),
        );
    }
    if !skip_ceil {
        push_flat_tri_oriented_ex(
            lists,
            ceil_tex,
            [
                (xa, ya, z_ceil(xa, ya)),
                (xb, yb, z_ceil(xb, yb)),
                (xc, yc, z_ceil(xc, yc)),
            ],
            ceil_light,
            FlatUv {
                u_off: sec.ceiling_u_off,
                v_off: sec.ceiling_v_off,
                scale_u: sec.ceiling_scale_u,
                scale_v: sec.ceiling_scale_v,
                angle_deg: sec.ceiling_angle,
            },
            sec.ceiling_alpha,
            false,
            Some(sector_raw),
        );
    }
}

/// Vertical sides for 3D-floor slabs (control-sector textures on target linedefs).
fn emit_ffloor_sides(map: &MapGeometry, lists: &mut SceneLists) {
    if map.setup.ffloors.is_empty() {
        return;
    }
    for ff in &map.setup.ffloors {
        let ff_a = ff.opacity();
        if ff_a <= 0.001 {
            continue;
        }
        let ti = ff.target.raw() as usize;
        let ci = ff.control.raw() as usize;
        let Some(ctrl) = map.sectors.get(ci) else {
            continue;
        };
        let planes = map.setup.planes.get(ci);
        let z_floor = |x: f64, y: f64| match planes {
            Some(p) => p.floor.z_at(x, y) + (ctrl.floor_height - p.base_floor),
            None => ctrl.floor_height,
        };
        let z_ceil = |x: f64, y: f64| match planes {
            Some(p) => p.ceiling.z_at(x, y) + (ctrl.ceiling_height - p.base_ceiling),
            None => ctrl.ceiling_height,
        };
        let tex = ffloor_side_texture(map, ci, ctrl);
        if is_blank(&tex) {
            continue;
        }
        let light = doom_light(ctrl.light);
        let Some(target) = map.sectors.get(ti) else {
            continue;
        };
        for &lid in &target.lines {
            let Some(line) = map.lines.get(lid.raw() as usize) else {
                continue;
            };
            let Some(v1) = map.vertices.get(line.v1.raw() as usize) else {
                continue;
            };
            let Some(v2) = map.vertices.get(line.v2.raw() as usize) else {
                continue;
            };
            let x1 = v1.pos.x();
            let y1 = v1.pos.y();
            let x2 = v2.pos.x();
            let y2 = v2.pos.y();
            let z_lo = z_floor(x1, y1)
                .min(z_ceil(x1, y1))
                .min(z_floor(x2, y2).min(z_ceil(x2, y2)));
            let z_hi = z_floor(x1, y1)
                .max(z_ceil(x1, y1))
                .max(z_floor(x2, y2).max(z_ceil(x2, y2)));
            if z_hi <= z_lo + 1e-6 {
                continue;
            }
            let len = libm::hypot(x2 - x1, y2 - y1);
            #[allow(clippy::cast_possible_truncation)]
            let u2 = (len / f64::from(WALL_TEXEL)) as f32;
            let contrast = fake_contrast(x2 - x1, y2 - y1);
            push_wall_quad(
                lists,
                &tex,
                WallQuad {
                    x1,
                    y1,
                    z_lo,
                    x2,
                    y2,
                    z_hi,
                    u1: 0.0,
                    u2,
                    v_off: 0.0,
                    v_scale: 1.0,
                    light: (light * contrast).clamp(0.0, 1.0),
                    alpha: ff_a,
                    masked: false,
                    clamp_v: false,
                    single_v: false,
                    peg: WallPeg::Top,
                    side_raw: None,
                    lm_surf: None,
                },
            );
        }
    }
}

fn ffloor_side_texture(
    map: &MapGeometry,
    control_i: usize,
    ctrl: &room_map::SectorStatic,
) -> String {
    for line in &map.lines {
        let on_ctrl = line
            .front_sector
            .is_some_and(|s| s.raw() as usize == control_i)
            || line
                .back_sector
                .is_some_and(|s| s.raw() as usize == control_i);
        if !on_ctrl {
            continue;
        }
        for side_opt in line.sides {
            let Some(sid) = side_opt else {
                continue;
            };
            let Some(side) = map.sides.get(sid.raw() as usize) else {
                continue;
            };
            if side.sector.raw() as usize != control_i {
                continue;
            }
            if !is_blank(&side.middle) {
                return side.middle.clone();
            }
            if !is_blank(&side.lower) {
                return side.lower.clone();
            }
            if !is_blank(&side.upper) {
                return side.upper.clone();
            }
        }
    }
    if !is_blank(&ctrl.floor_pic) {
        ctrl.floor_pic.clone()
    } else {
        ctrl.ceiling_pic.clone()
    }
}

fn emit_ffloor_flats(
    map: &MapGeometry,
    sector_i: usize,
    ring: &[(f64, f64)],
    lists: &mut SceneLists,
) {
    if ring.len() < 3 {
        return;
    }
    let ring = ring_ccw(ring);
    let (x0, y0) = ring[0];
    for ff in &map.setup.ffloors {
        if ff.target.raw() as usize != sector_i {
            continue;
        }
        let ff_a = ff.opacity();
        if ff_a <= 0.001 {
            continue;
        }
        let ci = ff.control.raw() as usize;
        let Some(ctrl) = map.sectors.get(ci) else {
            continue;
        };
        let planes = map.setup.planes.get(ci);
        let z_floor = |x: f64, y: f64| match planes {
            Some(p) => p.floor.z_at(x, y) + (ctrl.floor_height - p.base_floor),
            None => ctrl.floor_height,
        };
        let z_ceil = |x: f64, y: f64| match planes {
            Some(p) => p.ceiling.z_at(x, y) + (ctrl.ceiling_height - p.base_ceiling),
            None => ctrl.ceiling_height,
        };
        let light = doom_light(ctrl.light);
        let (top_pic, bot_pic) = if ff.typ == 0 {
            (&ctrl.ceiling_pic, &ctrl.floor_pic)
        } else {
            (&ctrl.floor_pic, &ctrl.ceiling_pic)
        };
        for i in 1..ring.len().saturating_sub(1) {
            let (xa, ya) = ring[i];
            let (xb, yb) = ring[i + 1];
            let z_top = |x: f64, y: f64| z_floor(x, y).max(z_ceil(x, y));
            let z_bot = |x: f64, y: f64| z_floor(x, y).min(z_ceil(x, y));
            push_flat_tri_oriented(
                lists,
                top_pic,
                [
                    (x0, y0, z_top(x0, y0)),
                    (xa, ya, z_top(xa, ya)),
                    (xb, yb, z_top(xb, yb)),
                ],
                light,
                FlatUv {
                    u_off: ctrl.floor_u_off,
                    v_off: ctrl.floor_v_off,
                    scale_u: ctrl.floor_scale_u,
                    scale_v: ctrl.floor_scale_v,
                    angle_deg: ctrl.floor_angle,
                },
                (ctrl.floor_alpha * ff_a).clamp(0.0, 1.0),
                true,
            );
            push_flat_tri_oriented(
                lists,
                bot_pic,
                [
                    (x0, y0, z_bot(x0, y0)),
                    (xa, ya, z_bot(xa, ya)),
                    (xb, yb, z_bot(xb, yb)),
                ],
                light,
                FlatUv {
                    u_off: ctrl.ceiling_u_off,
                    v_off: ctrl.ceiling_v_off,
                    scale_u: ctrl.ceiling_scale_u,
                    scale_v: ctrl.ceiling_scale_v,
                    angle_deg: ctrl.ceiling_angle,
                },
                (ctrl.ceiling_alpha * ff_a).clamp(0.0, 1.0),
                false,
            );
        }
    }
}

/// Flat UV transform (scroll + scale + rotation).
#[derive(Clone, Copy, Debug)]
struct FlatUv {
    u_off: f64,
    v_off: f64,
    scale_u: f64,
    scale_v: f64,
    angle_deg: f64,
}

/// Push a floor (`want_up`) or ceiling (`!want_up`) triangle, flipping winding
/// so the geometric normal matches the expected hemisphere (survives Back cull).
fn push_flat_tri_oriented(
    lists: &mut SceneLists,
    tex: &str,
    verts: [(f64, f64, f64); 3],
    light: f32,
    uv: FlatUv,
    alpha: f32,
    want_up: bool,
) {
    push_flat_tri_oriented_ex(lists, tex, verts, light, uv, alpha, want_up, None);
}

fn push_flat_tri_oriented_ex(
    lists: &mut SceneLists,
    tex: &str,
    verts: [(f64, f64, f64); 3],
    light: f32,
    uv: FlatUv,
    alpha: f32,
    want_up: bool,
    sector_raw: Option<u32>,
) {
    let [(x0, y0, z0), (x1, y1, z1), (x2, y2, z2)] = verts;
    let e1x = x1 - x0;
    let e1y = y1 - y0;
    let e1z = z1 - z0;
    let e2x = x2 - x0;
    let e2y = y2 - y0;
    let e2z = z2 - z0;
    let nz = e1x * e2y - e1y * e2x;
    // Prefer Z; for steep slopes fall back to any non-zero axis toward want_up.
    let flip = if nz.abs() > 1e-12 {
        (nz > 0.0) != want_up
    } else {
        let ny = e1z * e2x - e1x * e2z;
        let nx = e1y * e2z - e1z * e2y;
        let upish = nz + ny + nx;
        (upish > 0.0) != want_up
    };
    let verts = if flip {
        [(x0, y0, z0), (x2, y2, z2), (x1, y1, z1)]
    } else {
        verts
    };
    let surf_i = lists.lightmap_bindings.resolve(None, sector_raw);
    let lm_surf = surf_i
        .and_then(|i| lists.lightmap_surfaces.get(i as usize).cloned())
        .or_else(|| lists.lightmap_surfaces.first().cloned());
    let translucent = alpha < 0.999;
    let (buf, ranges) = if translucent {
        (
            &mut lists.translucent_flats,
            &mut lists.translucent_flat_ranges,
        )
    } else {
        (&mut lists.flats, &mut lists.flat_ranges)
    };
    #[allow(clippy::cast_possible_truncation)]
    let start = buf.len() as u32;
    let su = if uv.scale_u.abs() > 1e-9 {
        uv.scale_u
    } else {
        1.0
    };
    let sv = if uv.scale_v.abs() > 1e-9 {
        uv.scale_v
    } else {
        1.0
    };
    let ang = uv.angle_deg.to_radians();
    let (ca, sa) = (ang.cos(), ang.sin());
    // Geometric normal from the (possibly flipped) triangle; tangent along UV X.
    let [(ax, ay, az), (bx, by, bz), (cx, cy, cz)] = verts;
    let n = {
        let e1 = glam::DVec3::new(bx - ax, by - ay, bz - az);
        let e2 = glam::DVec3::new(cx - ax, cy - ay, cz - az);
        let c = e1.cross(e2);
        let len = c.length().max(1e-12);
        #[allow(clippy::cast_possible_truncation)]
        glam::Vec3::new((c.x / len) as f32, (c.y / len) as f32, (c.z / len) as f32)
    };
    // Flat UV X axis in world XY, rotated by sector angle.
    let tangent = glam::Vec4::new(ca as f32, sa as f32, 0.0, 1.0);
    for (x, y, z) in verts {
        let xr = x * ca - y * sa;
        let yr = x * sa + y * ca;
        #[allow(clippy::cast_possible_truncation)]
        let pos = Vec3::new(x as f32, y as f32, z as f32);
        #[allow(clippy::cast_possible_truncation)]
        let uv = Vec2::new(
            ((xr + uv.u_off) / 64.0 / su) as f32,
            ((yr + uv.v_off) / 64.0 / sv) as f32,
        );
        let lm = lm_surf
            .as_ref()
            .map(|s| {
                let u = world_xy_to_surface_uv(s, pos.x, pos.y, 256.0);
                Vec2::new(u[0], u[1])
            })
            .unwrap_or(Vec2::ZERO);
        buf.push(
            FlatVertex::new(pos, uv, light)
                .with_alpha(alpha)
                .with_lm_uv(lm)
                .with_tbn(n, tangent),
        );
    }
    push_range(ranges, tex, start, 3);
}

fn line_is_mirror(map: &MapGeometry, line: room_core::LineId) -> bool {
    map.setup
        .portals
        .lines
        .iter()
        .any(|p| p.line == line && p.is_mirror())
}

fn note_mirror(lists: &mut SceneLists, line: u32, x1: f64, y1: f64, x2: f64, y2: f64) {
    let tex_name = mirror_tex_name(line);
    if lists.mirrors.iter().any(|m| m.line == line) {
        return;
    }
    lists.mirrors.push(MirrorSurface {
        line,
        ax: x1,
        ay: y1,
        bx: x2,
        by: y2,
        tex_name,
    });
}

fn line_mid(map: &MapGeometry, line: room_core::LineId) -> Option<(f64, f64)> {
    let l = map.lines.get(line.raw() as usize)?;
    let a = map.vertices.get(l.v1.raw() as usize)?;
    let b = map.vertices.get(l.v2.raw() as usize)?;
    Some((0.5 * (a.pos.x() + b.pos.x()), 0.5 * (a.pos.y() + b.pos.y())))
}

/// Emit `__PORTAL*` mid quads + refresh metadata for linked/visual line portals.
fn emit_linked_portal_canvases(map: &MapGeometry, lists: &mut SceneLists) {
    for p in &map.setup.portals.lines {
        if p.is_mirror() {
            continue;
        }
        let Some(target) = p.target else {
            continue;
        };
        let Some(line) = map.lines.get(p.line.raw() as usize) else {
            continue;
        };
        let Some(v1) = map.vertices.get(line.v1.raw() as usize) else {
            continue;
        };
        let Some(v2) = map.vertices.get(line.v2.raw() as usize) else {
            continue;
        };
        let (ax, ay) = (v1.pos.x(), v1.pos.y());
        let (bx, by) = (v2.pos.x(), v2.pos.y());
        let Some((sx, sy)) = line_mid(map, p.line) else {
            continue;
        };
        let Some((tx, ty)) = line_mid(map, target) else {
            continue;
        };
        let dx = tx - sx;
        let dy = ty - sy;
        let tex_name = portal_tex_name(p.line.raw());
        if lists.linked_portals.iter().any(|q| q.line == p.line.raw()) {
            continue;
        }
        lists.linked_portals.push(LinkedPortalSurface {
            line: p.line.raw(),
            target: target.raw(),
            ax,
            ay,
            bx,
            by,
            dx,
            dy,
            tex_name: tex_name.clone(),
        });
        let Some(front_id) = line.front_sector else {
            continue;
        };
        let Some(front) = map.sectors.get(front_id.raw() as usize) else {
            continue;
        };
        let (z_lo, z_hi) = if let Some(back_id) = line.back_sector {
            let back = map.sectors.get(back_id.raw() as usize);
            let z_lo = back.map_or(front.floor_height, |b| {
                front.floor_height.max(b.floor_height)
            });
            let z_hi = back.map_or(front.ceiling_height, |b| {
                front.ceiling_height.min(b.ceiling_height)
            });
            (z_lo, z_hi)
        } else {
            (front.floor_height, front.ceiling_height)
        };
        if z_hi <= z_lo {
            continue;
        }
        let light = doom_light(front.light);
        let len = libm::hypot(bx - ax, by - ay);
        #[allow(clippy::cast_possible_truncation)]
        let u2 = (len / f64::from(WALL_TEXEL)) as f32;
        push_wall_quad(
            lists,
            &tex_name,
            WallQuad {
                x1: ax,
                y1: ay,
                z_lo,
                x2: bx,
                y2: by,
                z_hi,
                u1: 0.0,
                u2,
                v_off: 0.0,
                v_scale: 1.0,
                light,
                alpha: 1.0,
                masked: false,
                clamp_v: false,
                single_v: false,
                peg: WallPeg::Top,
                side_raw: None,
                lm_surf: None,
            },
        );
    }
}

/// World clip plane for a mirror linedef: discard when `n·p + d > 0`
/// (positive half-space holds the reflected eye).
#[must_use]
pub fn mirror_clip_plane(ax: f64, ay: f64, bx: f64, by: f64, eye_x: f64, eye_y: f64) -> [f32; 4] {
    let dx = bx - ax;
    let dy = by - ay;
    let mut nx = -dy;
    let mut ny = dx;
    let mut d = dy * ax - dx * ay;
    let eye_s = nx * eye_x + ny * eye_y + d;
    if eye_s < 0.0 {
        nx = -nx;
        ny = -ny;
        d = -d;
    }
    [nx as f32, ny as f32, 0.0, d as f32]
}

/// Drop triangles entirely on the reflected-eye side of the mirror line
/// (CPU stand-in for a stencil / GPU clip plane).
pub fn clip_scene_to_mirror_line(
    lists: &mut SceneLists,
    ax: f64,
    ay: f64,
    bx: f64,
    by: f64,
    eye_x: f64,
    eye_y: f64,
) {
    let eye_side = line_side(ax, ay, bx, by, eye_x, eye_y);
    if eye_side.abs() < 1e-6 {
        return;
    }
    rebuild_walls_clipped(lists, ax, ay, bx, by, eye_side);
    clip_wall_buffer(
        &mut lists.translucent_walls,
        &mut lists.translucent_wall_ranges,
        ax,
        ay,
        bx,
        by,
        eye_side,
    );
    rebuild_flats_clipped(lists, ax, ay, bx, by, eye_side);
    rebuild_sprites_clipped(lists, ax, ay, bx, by, eye_side);
}

fn line_side(ax: f64, ay: f64, bx: f64, by: f64, x: f64, y: f64) -> f64 {
    (bx - ax) * (y - ay) - (by - ay) * (x - ax)
}

fn vert_behind_eye(ax: f64, ay: f64, bx: f64, by: f64, eye_side: f64, x: f32, y: f32) -> bool {
    let s = line_side(ax, ay, bx, by, f64::from(x), f64::from(y));
    s * eye_side > 1e-3
}

fn rebuild_walls_clipped(
    lists: &mut SceneLists,
    ax: f64,
    ay: f64,
    bx: f64,
    by: f64,
    eye_side: f64,
) {
    clip_wall_buffer(
        &mut lists.opaque_walls,
        &mut lists.wall_ranges,
        ax,
        ay,
        bx,
        by,
        eye_side,
    );
}

fn clip_wall_buffer(
    verts: &mut Vec<WallVertex>,
    ranges: &mut Vec<MaterialRange>,
    ax: f64,
    ay: f64,
    bx: f64,
    by: f64,
    eye_side: f64,
) {
    let old_verts = std::mem::take(verts);
    let old_ranges = std::mem::take(ranges);
    let mut new_verts = Vec::with_capacity(old_verts.len());
    let mut new_ranges = Vec::with_capacity(old_ranges.len());
    for r in old_ranges {
        let start = r.start as usize;
        let end = start.saturating_add(r.count as usize).min(old_verts.len());
        let slice = &old_verts[start..end];
        let range_start = new_verts.len() as u32;
        let mut i = 0;
        while i + 2 < slice.len() {
            let tri = &slice[i..i + 3];
            let all_behind = tri
                .iter()
                .all(|v| vert_behind_eye(ax, ay, bx, by, eye_side, v.pos[0], v.pos[1]));
            if !all_behind {
                new_verts.extend_from_slice(tri);
            }
            i += 3;
        }
        let count = new_verts.len() as u32 - range_start;
        if count > 0 {
            new_ranges.push(MaterialRange {
                name: r.name,
                start: range_start,
                count,
                flags: 0,
            });
        }
    }
    *verts = new_verts;
    *ranges = new_ranges;
}

fn rebuild_flats_clipped(
    lists: &mut SceneLists,
    ax: f64,
    ay: f64,
    bx: f64,
    by: f64,
    eye_side: f64,
) {
    clip_flat_buffer(
        &mut lists.flats,
        &mut lists.flat_ranges,
        ax,
        ay,
        bx,
        by,
        eye_side,
    );
    clip_flat_buffer(
        &mut lists.translucent_flats,
        &mut lists.translucent_flat_ranges,
        ax,
        ay,
        bx,
        by,
        eye_side,
    );
}

fn clip_flat_buffer(
    verts: &mut Vec<FlatVertex>,
    ranges: &mut Vec<MaterialRange>,
    ax: f64,
    ay: f64,
    bx: f64,
    by: f64,
    eye_side: f64,
) {
    let old_verts = std::mem::take(verts);
    let old_ranges = std::mem::take(ranges);
    let mut new_verts = Vec::with_capacity(old_verts.len());
    let mut new_ranges = Vec::with_capacity(old_ranges.len());
    for r in old_ranges {
        let start = r.start as usize;
        let end = start.saturating_add(r.count as usize).min(old_verts.len());
        let slice = &old_verts[start..end];
        let range_start = new_verts.len() as u32;
        let mut i = 0;
        while i + 2 < slice.len() {
            let tri = &slice[i..i + 3];
            let all_behind = tri
                .iter()
                .all(|v| vert_behind_eye(ax, ay, bx, by, eye_side, v.pos[0], v.pos[1]));
            if !all_behind {
                new_verts.extend_from_slice(tri);
            }
            i += 3;
        }
        let count = new_verts.len() as u32 - range_start;
        if count > 0 {
            new_ranges.push(MaterialRange {
                name: r.name,
                start: range_start,
                count,
                flags: 0,
            });
        }
    }
    *verts = new_verts;
    *ranges = new_ranges;
}

fn rebuild_sprites_clipped(
    lists: &mut SceneLists,
    ax: f64,
    ay: f64,
    bx: f64,
    by: f64,
    eye_side: f64,
) {
    let old_verts = std::mem::take(&mut lists.sprites);
    let old_ranges = std::mem::take(&mut lists.sprite_ranges);
    let mut new_verts = Vec::with_capacity(old_verts.len());
    let mut new_ranges = Vec::with_capacity(old_ranges.len());
    for r in old_ranges {
        let start = r.start as usize;
        let end = start.saturating_add(r.count as usize).min(old_verts.len());
        let slice = &old_verts[start..end];
        let range_start = new_verts.len() as u32;
        let mut i = 0;
        while i + 2 < slice.len() {
            let tri = &slice[i..i + 3];
            let all_behind = tri
                .iter()
                .all(|v| vert_behind_eye(ax, ay, bx, by, eye_side, v.pos[0], v.pos[1]));
            if !all_behind {
                new_verts.extend_from_slice(tri);
            }
            i += 3;
        }
        let count = new_verts.len() as u32 - range_start;
        if count > 0 {
            new_ranges.push(MaterialRange {
                name: r.name,
                start: range_start,
                count,
                flags: 0,
            });
        }
    }
    lists.sprites = new_verts;
    lists.sprite_ranges = new_ranges;
}

fn emit_seg_walls(map: &MapGeometry, seg: &Seg, lists: &mut SceneLists, step_on_high_side: bool) {
    let Some(line_id) = seg.line else {
        return;
    };
    let Some(line) = map.lines.get(line_id.raw() as usize) else {
        return;
    };
    let side_i = usize::from(seg.side != 0);
    let Some(side_id) = line.sides[side_i] else {
        return;
    };
    let side_raw = Some(side_id.raw());
    let Some(side) = map.sides.get(side_id.raw() as usize) else {
        return;
    };
    let Some(v1) = map.vertices.get(seg.v1.raw() as usize) else {
        return;
    };
    let Some(v2) = map.vertices.get(seg.v2.raw() as usize) else {
        return;
    };
    let x1 = v1.pos.x();
    let y1 = v1.pos.y();
    let x2 = v2.pos.x();
    let y2 = v2.pos.y();
    let front_sec = side.sector.raw() as usize;
    let Some(front) = map.sectors.get(front_sec) else {
        return;
    };
    let contrast = if side.no_fake_contrast {
        1.0
    } else if side.smooth_lighting {
        smooth_fake_contrast(x2 - x1, y2 - y1)
    } else {
        fake_contrast(x2 - x1, y2 - y1)
    };
    let light_mid =
        (doom_light(side_tier_light(side, front.light, TierLight::Mid)) * contrast).clamp(0.0, 1.0);
    let light_top = (doom_light(side_tier_light(side, front.light, TierLight::Top))
        * contrast
        * rgb_luma(front.color_walltop))
    .clamp(0.0, 1.0);
    let light_bot = (doom_light(side_tier_light(side, front.light, TierLight::Bottom))
        * contrast
        * rgb_luma(front.color_wallbottom))
    .clamp(0.0, 1.0);
    let light = light_mid;
    let len = libm::hypot(x2 - x1, y2 - y1);
    let (u1_mid, u2_mid, v_off_mid, sy_mid) = side_tier_uv(side, len, TierLight::Mid);
    let (u1_top, u2_top, v_off_top, sy_top) = side_tier_uv(side, len, TierLight::Top);
    let (u1_bot, u2_bot, v_off_bot, sy_bot) = side_tier_uv(side, len, TierLight::Bottom);
    let z_at = |si: usize, x: f64, y: f64, ceiling: bool| sector_z_at(map, si, x, y, ceiling);
    let (ff1, ff2) = (
        z_at(front_sec, x1, y1, false),
        z_at(front_sec, x2, y2, false),
    );
    let (fc1, fc2) = (z_at(front_sec, x1, y1, true), z_at(front_sec, x2, y2, true));
    let mirror = line_is_mirror(map, line_id);
    let mirror_tex = mirror.then(|| mirror_tex_name(line_id.raw()));
    emit_sky_wall(map, line_id.raw() as usize, line, side_i, front_sec, (x1, y1, x2, y2), (fc1, fc2), lists);

    if line.back_sector.is_none() {
        // One-sided: full mid wall (or mirror canvas).
        let tex = mirror_tex.as_deref().unwrap_or(side.middle.as_str());
        if !mirror && is_blank(&side.middle) {
            return;
        }
        if mirror {
            note_mirror(lists, line_id.raw(), x1, y1, x2, y2);
        }
        // One-sided: default peg to ceiling; `ML_DONTPEGBOTTOM` pegs door jambs to the floor
        // so DOORSTOP does not stretch as the door sector rises.
        let peg = if line.flags & ML_DONTPEGBOTTOM != 0 {
            WallPeg::Bottom
        } else {
            WallPeg::Top
        };
        push_sloped_wall_quad(
            lists,
            tex,
            WallQuad {
                x1,
                y1,
                z_lo: front.floor_height,
                x2,
                y2,
                z_hi: front.ceiling_height,
                u1: u1_mid,
                u2: u2_mid,
                v_off: v_off_mid,
                v_scale: sy_mid,
                light,
                alpha: side.alpha.unwrap_or(line.mid_alpha),
                // One-sided faces are solid; TEXTUREX gaps become black, not holes.
                masked: false,
                clamp_v: false,
                single_v: false,
                peg,
                side_raw,
                lm_surf: None,
            },
            (ff1, ff2, fc1, fc2),
        );
        return;
    }

    // Opposite sector for *this* sidedef (not always `line.back_sector`).
    let Some(other_id) = (if side_i == 0 {
        line.back_sector
    } else {
        line.front_sector
    }) else {
        return;
    };
    let back_sec = other_id.raw() as usize;
    let Some(back) = map.sectors.get(back_sec) else {
        return;
    };
    let (bf1, bf2) = (z_at(back_sec, x1, y1, false), z_at(back_sec, x2, y2, false));
    let (bc1, bc2) = (z_at(back_sec, x1, y1, true), z_at(back_sec, x2, y2, true));
    // Upper (front ceiling → back ceiling) when front is higher.
    // Doom "sky hack": both sides sky → omit upper (continuous sky silhouette).
    let both_sky = is_sky_flat(&front.ceiling_pic) && is_sky_flat(&back.ceiling_pic);
    if (fc1 > bc1 || fc2 > bc2) && !is_blank(&side.upper) && !both_sky {
        // Default upper pegs to the *lowest* ceiling (door underside) so the face
        // scrolls as the door opens; `ML_DONTPEGTOP` pegs to the highest ceiling.
        let peg = if line.flags & ML_DONTPEGTOP != 0 {
            WallPeg::Top
        } else {
            WallPeg::Bottom
        };
        push_sloped_wall_quad(
            lists,
            &side.upper,
            WallQuad {
                x1,
                y1,
                z_lo: back.ceiling_height,
                x2,
                y2,
                z_hi: front.ceiling_height,
                u1: u1_top,
                u2: u2_top,
                v_off: v_off_top,
                v_scale: sy_top,
                light: light_top,
                alpha: 1.0,
                masked: false,
                clamp_v: false,
                single_v: false,
                peg,
                side_raw,
                lm_surf: None,
            },
            (bc1, bc2, fc1, fc2),
        );
    }
    // Lower (back floor → front floor) when front is lower.
    if (ff1 < bf1 || ff2 < bf2) && !is_blank(&side.lower) {
        // Default lower pegs to the highest floor (top of the step); `ML_DONTPEGBOTTOM`
        // puts the texture image bottom on the lowest floor (not door-style Bottom).
        let peg = if line.flags & ML_DONTPEGBOTTOM != 0 {
            WallPeg::TexBottom
        } else {
            WallPeg::Top
        };
        // A closed poly door is a thin slab near the ceiling. The step face
        // stops at that floor, and the street sees clear color through the
        // rest of the opening. Carry the door picture up to the ceiling.
        let slab = back.floor_height > front.floor_height + 16.0
            && back.ceiling_height - back.floor_height
                < (back.floor_height - front.floor_height) + 8.0;
        let (z_hi, top1, top2) = if step_on_high_side && slab {
            let z = front.ceiling_height.min(back.ceiling_height);
            (z, fc1.min(bc1), fc2.min(bc2))
        } else {
            (back.floor_height, bf1, bf2)
        };
        push_sloped_wall_quad(
            lists,
            &side.lower,
            WallQuad {
                x1,
                y1,
                z_lo: front.floor_height,
                x2,
                y2,
                z_hi,
                u1: u1_bot,
                u2: u2_bot,
                v_off: v_off_bot,
                v_scale: sy_bot,
                light: light_bot,
                alpha: 1.0,
                masked: false,
                clamp_v: false,
                single_v: false,
                peg,
                side_raw,
                lm_surf: None,
            },
            (ff1, ff2, top1, top2),
        );
    } else if step_on_high_side
        && (ff1 > bf1 || ff2 > bf2)
        && !is_blank(&side.lower)
        && front.floor_height > back.floor_height + 16.0
        && front.ceiling_height - front.floor_height
            < (front.floor_height - back.floor_height) + 8.0
    {
        // Closed poly door: the walkable floor is the far sector, so the step
        // face would otherwise point away from the street and the opening is
        // the clear color. Paint this side's lower texture from that floor up
        // to the door ceiling.
        let peg = if line.flags & ML_DONTPEGBOTTOM != 0 {
            WallPeg::TexBottom
        } else {
            WallPeg::Top
        };
        let z_hi = front.ceiling_height.min(back.ceiling_height);
        push_sloped_wall_quad(
            lists,
            &side.lower,
            WallQuad {
                x1,
                y1,
                z_lo: back.floor_height,
                x2,
                y2,
                z_hi,
                u1: u1_bot,
                u2: u2_bot,
                v_off: v_off_bot,
                v_scale: sy_bot,
                light: light_bot,
                alpha: 1.0,
                masked: false,
                clamp_v: false,
                single_v: false,
                peg,
                side_raw,
                lm_surf: None,
            },
            (bf1, bf2, fc1.min(bc1), fc2.min(bc2)),
        );
    }
    // Mid texture on two-sided (railings) — mirrors always fill the opening.
    let opening_lo = front.floor_height.max(back.floor_height);
    let opening_hi = front.ceiling_height.min(back.ceiling_height);
    if opening_hi > opening_lo {
        if mirror {
            note_mirror(lists, line_id.raw(), x1, y1, x2, y2);
            if let Some(tex) = mirror_tex.as_deref() {
                push_wall_quad(
                    lists,
                    tex,
                    WallQuad {
                        x1,
                        y1,
                        z_lo: opening_lo,
                        x2,
                        y2,
                        z_hi: opening_hi,
                        u1: u1_mid,
                        u2: u2_mid,
                        v_off: v_off_mid,
                        v_scale: sy_mid,
                        light,
                        alpha: 1.0,
                        masked: false,
                        clamp_v: false,
                        single_v: false,
                        peg: WallPeg::Top,
                        side_raw,
                        lm_surf: None,
                    },
                );
            }
        } else if !is_blank(&side.middle) {
            // Line flag applies to both sides; else side-local (GZDoom WALLF_*).
            let wrap = line.wrap_midtex || side.wrap_midtex;
            let clip = line.clip_midtex || side.clip_midtex;
            let (z_lo, z_hi) = midtex_z_range(
                opening_lo,
                opening_hi,
                f64::from(side.y_offset),
                f64::from(sy_mid),
                line.flags,
                wrap,
                clip,
            );
            // Two-sided mid: DONTPEGBOTTOM pegs to floor (handled by midtex_z_range
            // for the quad extent); world-Z wrap still pegs V to the top of the
            // emitted slab unless DONTPEGBOTTOM is set.
            let peg = if line.flags & ML_DONTPEGBOTTOM != 0 {
                WallPeg::Bottom
            } else {
                WallPeg::Top
            };
            // A wrapping mid fills the opening, which slopes with the planes.
            let ends = if wrap {
                (ff1.max(bf1), ff2.max(bf2), fc1.min(bc1), fc2.min(bc2))
            } else {
                (z_lo, z_lo, z_hi, z_hi)
            };
            push_sloped_wall_quad(
                lists,
                &side.middle,
                WallQuad {
                    x1,
                    y1,
                    z_lo,
                    x2,
                    y2,
                    z_hi,
                    u1: u1_mid,
                    u2: u2_mid,
                    // Single-draw midtex maps V across [0,1]; wrap uses world Z.
                    v_off: if wrap { v_off_mid } else { 0.0 },
                    v_scale: if wrap { sy_mid } else { 1.0 },
                    light,
                    alpha: side.alpha.unwrap_or(line.mid_alpha),
                    masked: true,
                    clamp_v: !wrap,
                    single_v: !wrap,
                    peg,
                    side_raw,
                    lm_surf: None,
                },
                ends,
            );
        }
    }
}

/// Wall tier for UDMF per-tier sidedef light / UV.
enum TierLight {
    Top,
    Mid,
    Bottom,
}

/// U / V / Y-scale for a sidedef wall tier.
fn side_tier_uv(side: &room_map::Side, len: f64, tier: TierLight) -> (f32, f32, f32, f32) {
    let (sx, sy, xo, yo) = match tier {
        TierLight::Top => (
            side.scale_x_top,
            side.scale_y_top,
            side.x_offset_top,
            side.y_offset_top,
        ),
        TierLight::Mid => (side.scale_x, side.scale_y, side.x_offset, side.y_offset),
        TierLight::Bottom => (
            side.scale_x_bottom,
            side.scale_y_bottom,
            side.x_offset_bottom,
            side.y_offset_bottom,
        ),
    };
    let sx = if sx.abs() > 1e-6 { sx } else { 1.0 };
    let sy = if sy.abs() > 1e-6 { sy } else { 1.0 };
    #[allow(clippy::cast_possible_truncation)]
    // Doom wall textures are typically 128×128 (flats are 64×64).
    let u_off = f32::from(xo) / WALL_TEXEL / sx;
    #[allow(clippy::cast_possible_truncation)]
    let v_off = f32::from(yo) / WALL_TEXEL / sy;
    #[allow(clippy::cast_possible_truncation)]
    let u2 = u_off + (len / f64::from(WALL_TEXEL)) as f32 / sx;
    (u_off, u2, v_off, sy)
}

/// Resolve sidedef light for a wall tier (`light_*` → `light` → sector).
fn side_tier_light(side: &room_map::Side, sector_light: i16, tier: TierLight) -> i16 {
    let (tier_lvl, tier_abs) = match tier {
        TierLight::Top => (side.light_top, side.light_absolute_top),
        TierLight::Mid => (side.light_mid, side.light_absolute_mid),
        TierLight::Bottom => (side.light_bottom, side.light_absolute_bottom),
    };
    let (lvl, abs) = if tier_lvl != i16::MIN {
        (tier_lvl, tier_abs)
    } else {
        (side.light, side.light_absolute)
    };
    if lvl == i16::MIN {
        sector_light
    } else if abs {
        lvl.clamp(0, 255)
    } else {
        (i32::from(sector_light) + i32::from(lvl)).clamp(0, 255) as i16
    }
}

/// Doom `ML_DONTPEGTOP` — peg upper textures to the highest ceiling.
const ML_DONTPEGTOP: i16 = 0x0008;
/// Doom `ML_DONTPEGBOTTOM` — peg mid/lower to the lowest floor.
const ML_DONTPEGBOTTOM: i16 = 0x0010;

/// Where a wall tier's texture V=0 is anchored in world Z.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WallPeg {
    /// Attach at `z_hi` (V increases downward). Default one-sided / `ML_DONTPEGTOP` uppers.
    Top,
    /// Attach at `z_lo` with V=`v_off` (door underside scroll / rising peg).
    Bottom,
    /// Lower + `ML_DONTPEGBOTTOM`: bottom of the texture image sits on the low floor
    /// (E1M1 SW1COMP LED lives near V≈0.56–0.94 of a 128-tall texture).
    TexBottom,
}

/// Default midtex height in map units until the material atlas is consulted.
const DEFAULT_MID_TEX_HEIGHT: f64 = 128.0;

/// World units per wall-texture tile (most Doom wall textures are 128×128).
const WALL_TEXEL: f32 = 128.0;

/// Pegged midtex Z range; `clipmidtex` intersects the opening, `wrapmidtex` fills it.
fn midtex_z_range(
    opening_lo: f64,
    opening_hi: f64,
    y_offset: f64,
    y_scale: f64,
    flags: i16,
    wrap: bool,
    clip: bool,
) -> (f64, f64) {
    if wrap {
        return (opening_lo, opening_hi);
    }
    let sy = if y_scale.abs() > 1e-6 { y_scale } else { 1.0 };
    let tex_h = DEFAULT_MID_TEX_HEIGHT * sy;
    let (mut z_lo, mut z_hi) = if flags & ML_DONTPEGBOTTOM != 0 {
        let lo = opening_lo + y_offset;
        (lo, lo + tex_h)
    } else {
        let hi = opening_hi + y_offset;
        (hi - tex_h, hi)
    };
    if clip {
        z_lo = z_lo.max(opening_lo);
        z_hi = z_hi.min(opening_hi);
    }
    (z_lo, z_hi)
}

struct WallQuad {
    x1: f64,
    y1: f64,
    z_lo: f64,
    x2: f64,
    y2: f64,
    z_hi: f64,
    u1: f32,
    u2: f32,
    v_off: f32,
    /// Divides vertical UV span (`Line_SetTextureScale` Y).
    v_scale: f32,
    light: f32,
    /// Mid-texture opacity (`TranslucentLine`); 1 = opaque.
    alpha: f32,
    /// Two-sided mid / grate: discard transparent texels (else holes → black).
    masked: bool,
    /// When true, clamp vertical UV to `[0,1]` (no `wrapmidtex`).
    clamp_v: bool,
    /// When true, map V across `[0,1]` once (pegged midtex, not world-Z tiling).
    single_v: bool,
    /// World-Z anchor for texture V (door faces / jambs).
    peg: WallPeg,
    /// Sidedef raw id for lightmap binding (optional).
    side_raw: Option<u32>,
    /// Explicit lightmap surface index (overrides binding lookup).
    lm_surf: Option<u32>,
}

fn wall_texel_wh(lists: &SceneLists, tex: &str) -> (f32, f32) {
    let key = tex.to_ascii_uppercase();
    lists
        .wall_tex_sizes
        .get(&key)
        .copied()
        .unwrap_or((WALL_TEXEL, WALL_TEXEL))
}

/// Live floor / ceiling Z of sector `si` at `(x, y)`, following its slope.
fn sector_z_at(map: &MapGeometry, si: usize, x: f64, y: f64, ceiling: bool) -> f64 {
    let Some(sec) = map.sectors.get(si) else {
        return 0.0;
    };
    match map.setup.planes.get(si) {
        Some(p) if ceiling => p.ceiling.z_at(x, y) + (sec.ceiling_height - p.base_ceiling),
        Some(p) => p.floor.z_at(x, y) + (sec.floor_height - p.base_floor),
        None if ceiling => sec.ceiling_height,
        None => sec.floor_height,
    }
}

/// Wall span heights at each end: `(lo1, lo2, hi1, hi2)`.
type WallEnds = (f64, f64, f64, f64);

fn push_wall_quad(lists: &mut SceneLists, tex: &str, q: WallQuad) {
    let ends = (q.z_lo, q.z_lo, q.z_hi, q.z_hi);
    push_sloped_wall_quad(lists, tex, q, ends);
}

/// [`push_wall_quad`] whose bottom and top edges follow sloped planes. The
/// texture stays anchored to `q.z_lo` / `q.z_hi` (the sector heights).
fn push_sloped_wall_quad(lists: &mut SceneLists, tex: &str, q: WallQuad, ends: WallEnds) {
    let (lo1, lo2, hi1, hi2) = ends;
    let (hi1, hi2) = (hi1.max(lo1), hi2.max(lo2));
    if hi1 <= lo1 && hi2 <= lo2 {
        return;
    }
    let (tex_w, tex_h) = wall_texel_wh(lists, tex);
    // `side_tier_uv` authored U/V offsets against [`WALL_TEXEL`]; rescale to the
    // real TEXTUREX size so 64-wide switches (SW1COMP) are not half-sampled.
    let u_scale = WALL_TEXEL / tex_w.max(1.0);
    let v_scale_tex = WALL_TEXEL / tex_h.max(1.0);
    let u1 = q.u1 * u_scale;
    let u2 = q.u2 * u_scale;
    let v_off = q.v_off * v_scale_tex;
    #[allow(clippy::cast_possible_truncation)]
    let p = |x: f64, y: f64, z: f64| Vec3::new(x as f32, y as f32, z as f32);
    let vs = if q.v_scale.abs() > 1e-6 {
        q.v_scale
    } else {
        1.0
    };
    // Doom: texture row 0 is the top of the image; V increases downward.
    // Peg-top: V=`v_off` at z_hi. Peg-bottom (door underside): V=`v_off` at z_lo
    // so a rising door scrolls. TexBottom (lower+DONTPEGBOTTOM): V≈1 at z_lo.
    // V of world height `z`, anchored like the flat quad's edges.
    let v_at = |z: f64| -> f32 {
        #[allow(clippy::cast_possible_truncation)]
        let rows = |dz: f64| (dz / f64::from(tex_h)) as f32 / vs;
        let v = match q.peg {
            WallPeg::Top => v_off + rows(q.z_hi - z),
            WallPeg::Bottom => v_off - rows(z - q.z_lo),
            // `vtop = frontfloor + textureheight` (+rowoffset) → bottom of image at z_lo.
            WallPeg::TexBottom => 1.0 - v_off - rows(z - q.z_lo),
        };
        if q.clamp_v { v.clamp(0.0, 1.0) } else { v }
    };
    let (v_bot1, v_bot2, v_top1, v_top2) = if q.single_v {
        (1.0_f32, 1.0, 0.0, 0.0)
    } else {
        (v_at(lo1), v_at(lo2), v_at(hi1), v_at(hi2))
    };
    let surf_i = q
        .lm_surf
        .or_else(|| lists.lightmap_bindings.resolve(q.side_raw, None));
    let lm_surf = surf_i
        .and_then(|i| lists.lightmap_surfaces.get(i as usize).cloned())
        .or_else(|| lists.lightmap_surfaces.first().cloned());
    let lm = |x: f64, y: f64| -> Vec2 {
        lm_surf
            .as_ref()
            .map(|s| {
                let uv = world_xy_to_surface_uv(s, x as f32, y as f32, 256.0);
                Vec2::new(uv[0], uv[1])
            })
            .unwrap_or(Vec2::ZERO)
    };
    // Wall TBN: tangent along the seg, normal = left-hand perpendicular in XY
    // (Doom front side faces into the sector to the left of the directed seg).
    let dx = (q.x2 - q.x1) as f32;
    let dy = (q.y2 - q.y1) as f32;
    let len = (dx * dx + dy * dy).sqrt().max(1e-6);
    let tangent = glam::Vec4::new(dx / len, dy / len, 0.0, 1.0);
    let normal = glam::Vec3::new(-dy / len, dx / len, 0.0);
    let mut a = WallVertex::new(p(q.x1, q.y1, lo1), Vec2::new(u1, v_bot1), q.light)
        .with_alpha(q.alpha)
        .with_lm_uv(lm(q.x1, q.y1))
        .with_tbn(normal, tangent);
    let mut b = WallVertex::new(p(q.x2, q.y2, lo2), Vec2::new(u2, v_bot2), q.light)
        .with_alpha(q.alpha)
        .with_lm_uv(lm(q.x2, q.y2))
        .with_tbn(normal, tangent);
    let mut c = WallVertex::new(p(q.x2, q.y2, hi2), Vec2::new(u2, v_top2), q.light)
        .with_alpha(q.alpha)
        .with_lm_uv(lm(q.x2, q.y2))
        .with_tbn(normal, tangent);
    let mut d = WallVertex::new(p(q.x1, q.y1, hi1), Vec2::new(u1, v_top1), q.light)
        .with_alpha(q.alpha)
        .with_lm_uv(lm(q.x1, q.y1))
        .with_tbn(normal, tangent);
    if q.masked {
        a = a.with_masked();
        b = b.with_masked();
        c = c.with_masked();
        d = d.with_masked();
    }
    let translucent = q.alpha < 0.999;
    let (buf, ranges) = if translucent {
        (
            &mut lists.translucent_walls,
            &mut lists.translucent_wall_ranges,
        )
    } else {
        (&mut lists.opaque_walls, &mut lists.wall_ranges)
    };
    #[allow(clippy::cast_possible_truncation)]
    let start = buf.len() as u32;
    buf.extend_from_slice(&[a, b, c, a, c, d]);
    push_range(ranges, tex, start, 6);
}

/// Height sky walls reach (GZDoom draws them to "infinity").
const SKY_WALL_TOP: f64 = 32768.0;

/// Sky wall above a seg whose front sector has a sky ceiling and whose back
/// side (if any) does not: from the higher of the two ceilings up.
#[allow(clippy::too_many_arguments)]
fn emit_sky_wall(
    map: &MapGeometry,
    line_index: usize,
    line: &room_map::Line,
    side_i: usize,
    front_sec: usize,
    (x1, y1, x2, y2): (f64, f64, f64, f64),
    (fc1, fc2): (f64, f64),
    lists: &mut SceneLists,
) {
    use room_map::udmf::ext::{LINE_NOSKYWALLS, SECTOR_NOSKYWALLS};
    let Some(front) = map.sectors.get(front_sec) else {
        return;
    };
    if !is_sky_flat(&front.ceiling_pic)
        || map.udmf_user.line_ext(line_index) & LINE_NOSKYWALLS != 0
        || map.udmf_user.sector_ext(front_sec) & SECTOR_NOSKYWALLS != 0
    {
        return;
    }
    let other = if side_i == 0 {
        line.back_sector
    } else {
        line.front_sector
    };
    let (mut z1, mut z2) = (fc1, fc2);
    if let Some(o) = other {
        let oi = o.raw() as usize;
        let Some(back) = map.sectors.get(oi) else {
            return;
        };
        // Sky on both sides: the sky continues, no wall (Doom sky hack).
        if is_sky_flat(&back.ceiling_pic) {
            return;
        }
        z1 = z1.max(sector_z_at(map, oi, x1, y1, true));
        z2 = z2.max(sector_z_at(map, oi, x2, y2, true));
    }
    let v = |x: f64, y: f64, z: f64| {
        WallVertex::new(Vec3::new(x as f32, y as f32, z as f32), Vec2::ZERO, 1.0)
    };
    let (a, b) = (v(x1, y1, z1), v(x2, y2, z2));
    let (c, d) = (v(x2, y2, SKY_WALL_TOP), v(x1, y1, SKY_WALL_TOP));
    lists.sky_walls.extend_from_slice(&[a, b, c, a, c, d]);
}

/// Doom / Hexen / Strife sky flat names (`MAPINFO skyflatname` default set).
#[must_use]
pub fn is_sky_flat(name: &str) -> bool {
    let n = name.trim();
    n.eq_ignore_ascii_case("F_SKY1")
        || n.eq_ignore_ascii_case("F_SKY")
        || n.eq_ignore_ascii_case("F_SKY001")
}

/// Regroup depth-tested vertices so every material is one contiguous range.
///
/// Opaque walls and flats are order-independent (depth buffer, alpha-tested
/// masks), so drawing them grouped by `(name, flags)` turns ~1.5k per-subsector
/// draws on a typical map into a few dozen. Skipped unless the ranges cover
/// the vertex list exactly (nothing outside a range may be dropped).
pub fn coalesce_by_material<V: Copy>(verts: &mut Vec<V>, ranges: &mut Vec<MaterialRange>) {
    if ranges.len() < 2 {
        return;
    }
    let covered: usize = ranges.iter().map(|r| r.count as usize).sum();
    if covered != verts.len()
        || ranges
            .iter()
            .any(|r| (r.start as usize).saturating_add(r.count as usize) > verts.len())
    {
        return;
    }
    let mut order: Vec<usize> = (0..ranges.len()).collect();
    order.sort_by(|&a, &b| {
        (ranges[a].name.as_str(), ranges[a].flags).cmp(&(ranges[b].name.as_str(), ranges[b].flags))
    });
    let mut out = Vec::with_capacity(verts.len());
    let mut merged: Vec<MaterialRange> = Vec::new();
    for index in order {
        let r = &ranges[index];
        let start = r.start as usize;
        #[allow(clippy::cast_possible_truncation)]
        let at = out.len() as u32;
        out.extend_from_slice(&verts[start..start + r.count as usize]);
        match merged.last_mut() {
            Some(last) if last.name == r.name && last.flags == r.flags => last.count += r.count,
            _ => merged.push(MaterialRange {
                name: r.name.clone(),
                start: at,
                count: r.count,
                flags: r.flags,
            }),
        }
    }
    *verts = out;
    *ranges = merged;
}

fn push_range(ranges: &mut Vec<MaterialRange>, name: &str, start: u32, count: u32) {
    let key = name.trim().to_ascii_uppercase();
    if let Some(last) = ranges.last_mut() {
        if last.name == key && last.start + last.count == start {
            last.count += count;
            return;
        }
    }
    ranges.push(MaterialRange {
        name: key,
        start,
        count,
        flags: 0,
    });
}

fn is_blank(name: &str) -> bool {
    let t = name.trim();
    t.is_empty() || t == "-"
}

fn doom_light(level: i16) -> f32 {
    (f32::from(level).clamp(0.0, 255.0)) / 255.0
}

/// Luminance multiplier from UDMF RRGGBB (`-1` / unset → 1.0).
fn rgb_luma(rgb: i32) -> f32 {
    if rgb < 0 {
        return 1.0;
    }
    let r = ((rgb >> 16) & 0xff) as f32 / 255.0;
    let g = ((rgb >> 8) & 0xff) as f32 / 255.0;
    let b = (rgb & 0xff) as f32 / 255.0;
    (0.2126 * r + 0.7152 * g + 0.0722 * b).clamp(0.0, 1.0)
}

/// Fake contrast: vertical walls slightly darker (Doom hardware look).
fn fake_contrast(dx: f64, dy: f64) -> f32 {
    let ax = libm::fabs(dx);
    let ay = libm::fabs(dy);
    if ax > ay {
        0.9375 // ~15/16 horizontal run → "vertical" wall facing N/S
    } else if ay > ax {
        1.0625 // E/W facing slightly brighter
    } else {
        1.0
    }
}

/// Milder fake contrast (`smoothlighting`).
fn smooth_fake_contrast(dx: f64, dy: f64) -> f32 {
    let ax = libm::fabs(dx);
    let ay = libm::fabs(dy);
    if ax > ay {
        0.96875
    } else if ay > ax {
        1.03125
    } else {
        1.0
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use room_nodebuild::apply_nodes;
    use room_testkit::{MapBuilder, rect};

    #[test]
    fn clip_scene_drops_tris_on_eye_side() {
        let mut lists = SceneLists::default();
        // Triangle entirely on +Y of the X-axis mirror line (y=0).
        lists.opaque_walls.extend_from_slice(&[
            WallVertex::new(Vec3::new(0.0, 10.0, 0.0), Vec2::new(0.0, 0.0), 1.0),
            WallVertex::new(Vec3::new(10.0, 10.0, 0.0), Vec2::new(1.0, 0.0), 1.0),
            WallVertex::new(Vec3::new(0.0, 20.0, 0.0), Vec2::new(0.0, 1.0), 1.0),
        ]);
        // Triangle entirely on −Y (player side when eye is at y=-10).
        lists.opaque_walls.extend_from_slice(&[
            WallVertex::new(Vec3::new(0.0, -10.0, 0.0), Vec2::new(0.0, 0.0), 1.0),
            WallVertex::new(Vec3::new(10.0, -10.0, 0.0), Vec2::new(1.0, 0.0), 1.0),
            WallVertex::new(Vec3::new(0.0, -20.0, 0.0), Vec2::new(0.0, 1.0), 1.0),
        ]);
        lists.wall_ranges.push(MaterialRange {
            name: "TEST".into(),
            start: 0,
            count: 6,
            flags: 0,
        });
        // Mirror along X axis; reflected eye on +Y → drop +Y triangle.
        clip_scene_to_mirror_line(&mut lists, 0.0, 0.0, 100.0, 0.0, 5.0, 10.0);
        assert_eq!(lists.opaque_walls.len(), 3);
        assert!(lists.opaque_walls.iter().all(|v| v.pos[1] < 0.0));
    }

    #[test]
    fn mirror_clip_plane_puts_eye_on_positive_side() {
        // Mirror on X axis; reflected eye at +Y → positive half-space includes eye.
        let p = mirror_clip_plane(0.0, 0.0, 100.0, 0.0, 5.0, 10.0);
        let eye_s = p[0] * 5.0 + p[1] * 10.0 + p[3];
        assert!(eye_s > 0.0, "eye_s={eye_s}");
        // Point on −Y (kept side) should be ≤ 0.
        let kept = p[0] * 5.0 + p[1] * (-10.0) + p[3];
        assert!(kept < 0.0, "kept={kept}");
    }

    #[test]
    fn sky_ceiling_rooms_get_sky_walls_unless_noskywalls() {
        let mk = || {
            let mut map = MapBuilder::new()
                .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
                .player_start(64.0, 64.0, 0)
                .build_udmf("MAP01")
                .unwrap();
            apply_nodes(&mut map);
            map.sectors[0].ceiling_pic = "F_SKY1".into();
            map
        };
        let cam = Camera {
            eye: Vec3::new(64.0, 64.0, 41.0),
            ..Camera::default()
        };
        let scene = build_scene(&mk(), &cam, None);
        assert!(!scene.sky_walls.is_empty());
        assert!(scene.sky_walls.iter().all(|v| v.pos[2] >= 127.9));
        let mut map = mk();
        map.udmf_user.sector_ext = vec![room_map::udmf::ext::SECTOR_NOSKYWALLS];
        assert!(build_scene(&map, &cam, None).sky_walls.is_empty());
        let mut map = mk();
        map.udmf_user.line_ext = vec![room_map::udmf::ext::LINE_NOSKYWALLS; map.lines.len()];
        assert!(build_scene(&map, &cam, None).sky_walls.is_empty());
    }

    #[test]
    fn ceiling_scroll_shifts_flat_uv() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        map.sectors[0].ceiling_u_off = 64.0;
        map.sectors[0].ceiling_v_off = 0.0;
        let cam = Camera {
            eye: Vec3::new(64.0, 64.0, 41.0),
            ..Camera::default()
        };
        let scene = build_scene(&map, &cam, None);
        // Ceiling verts should have U shifted by 1.0 versus floor at same XY.
        let ceil_uvs: Vec<_> = scene
            .flats
            .iter()
            .filter(|v| (v.pos[2] - 128.0).abs() < 0.1)
            .map(|v| v.uv)
            .collect();
        assert!(!ceil_uvs.is_empty());
        let floor_uvs: Vec<_> = scene
            .flats
            .iter()
            .filter(|v| v.pos[2].abs() < 0.1)
            .map(|v| v.uv)
            .collect();
        assert!((ceil_uvs[0][0] - floor_uvs[0][0] - 1.0).abs() < 1e-4);
    }

    #[test]
    fn door_upper_pegs_to_rising_ceiling_not_front_ceiling() {
        // Yard (ceil 128) | door sector (ceil rises 0→64). Upper without
        // DONTPEGTOP must keep V at the door underside (z_lo) fixed so the face
        // scrolls instead of stretching from the yard ceiling.
        let mut map = MapBuilder::new()
            .room("yard", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .room("door", rect(256.0, 0.0, 512.0, 256.0), 0.0, 0.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut found = false;
        for line in &map.lines {
            if line.back_sector.is_none() {
                continue;
            }
            for si in 0..2 {
                let Some(sid) = line.sides[si] else {
                    continue;
                };
                map.sides[sid.raw() as usize].upper = "BIGDOOR2".into();
                found = true;
            }
        }
        assert!(found, "expected shared yard|door linedef");
        // Direct unit check of peg UV (avoids depending on BSP visibility).
        let mut lists = SceneLists::default();
        push_wall_quad(
            &mut lists,
            "BIGDOOR2",
            WallQuad {
                x1: 0.0,
                y1: 0.0,
                z_lo: 0.0,
                x2: 64.0,
                y2: 0.0,
                z_hi: 128.0,
                u1: 0.0,
                u2: 1.0,
                v_off: 0.0,
                v_scale: 1.0,
                light: 1.0,
                alpha: 1.0,
                masked: false,
                clamp_v: false,
                single_v: false,
                peg: WallPeg::Bottom,
                side_raw: None,
                lm_surf: None,
            },
        );
        let v_lo = lists
            .opaque_walls
            .iter()
            .filter(|v| v.pos[2].abs() < 1e-3)
            .map(|v| v.uv[1])
            .next()
            .unwrap();
        let v_hi = lists
            .opaque_walls
            .iter()
            .filter(|v| (v.pos[2] - 128.0).abs() < 1e-3)
            .map(|v| v.uv[1])
            .next()
            .unwrap();
        assert!(
            (v_lo - 0.0).abs() < 1e-4,
            "peg-bottom: V at z_lo should be v_off (got {v_lo})"
        );
        assert!(
            (v_hi - (-1.0)).abs() < 1e-4,
            "peg-bottom: V at z_hi should be v_off - span (got {v_hi})"
        );
        // Rising door underside: same V at new z_lo.
        let mut lists2 = SceneLists::default();
        push_wall_quad(
            &mut lists2,
            "BIGDOOR2",
            WallQuad {
                x1: 0.0,
                y1: 0.0,
                z_lo: 64.0,
                x2: 64.0,
                y2: 0.0,
                z_hi: 128.0,
                u1: 0.0,
                u2: 1.0,
                v_off: 0.0,
                v_scale: 1.0,
                light: 1.0,
                alpha: 1.0,
                masked: false,
                clamp_v: false,
                single_v: false,
                peg: WallPeg::Bottom,
                side_raw: None,
                lm_surf: None,
            },
        );
        let v_lo2 = lists2
            .opaque_walls
            .iter()
            .filter(|v| (v.pos[2] - 64.0).abs() < 1e-3)
            .map(|v| v.uv[1])
            .next()
            .unwrap();
        assert!(
            (v_lo2 - 0.0).abs() < 1e-4,
            "rising door underside V must stay pegged (got {v_lo2})"
        );
    }

    #[test]
    fn lower_dontpegbottom_shows_texture_bottom() {
        // E1M1 SW1COMP: 48mu step, 128-tall tex, y_offset=8 → LED near V=0.56.
        let mut lists = SceneLists::default();
        lists.wall_tex_sizes.insert("SW1COMP".into(), (64.0, 128.0));
        push_wall_quad(
            &mut lists,
            "SW1COMP",
            WallQuad {
                x1: 0.0,
                y1: 0.0,
                z_lo: -8.0,
                x2: 64.0,
                y2: 0.0,
                z_hi: 40.0,
                u1: 0.0,
                u2: 0.5, // authored against 128-wide; rescale → full 64-wide
                v_off: 8.0 / 128.0,
                v_scale: 1.0,
                light: 1.0,
                alpha: 1.0,
                masked: false,
                clamp_v: false,
                single_v: false,
                peg: WallPeg::TexBottom,
                side_raw: None,
                lm_surf: None,
            },
        );
        let v_lo = lists
            .opaque_walls
            .iter()
            .filter(|v| (v.pos[2] - (-8.0)).abs() < 1e-3)
            .map(|v| v.uv[1])
            .next()
            .unwrap();
        let v_hi = lists
            .opaque_walls
            .iter()
            .filter(|v| (v.pos[2] - 40.0).abs() < 1e-3)
            .map(|v| v.uv[1])
            .next()
            .unwrap();
        assert!(
            (v_lo - (1.0 - 8.0 / 128.0)).abs() < 1e-4,
            "TexBottom: V at low floor got {v_lo}"
        );
        assert!(
            (v_hi - (1.0 - 8.0 / 128.0 - 48.0 / 128.0)).abs() < 1e-4,
            "TexBottom: V at high floor got {v_hi}"
        );
        assert!(
            v_hi < 0.57 && v_hi > 0.55,
            "switch LED row (~0.5625) should sit at step top, got {v_hi}"
        );
    }

    #[test]
    fn midtex_z_range_pegs_and_clips() {
        // Peg to ceiling (default): 128mu tall from opening top.
        let (lo, hi) = midtex_z_range(0.0, 256.0, 0.0, 1.0, 0, false, false);
        assert!((hi - 256.0).abs() < 1e-9);
        assert!((lo - 128.0).abs() < 1e-9);
        // clipmidtex shrinks to opening.
        let (lo, hi) = midtex_z_range(0.0, 64.0, 0.0, 1.0, 0, false, true);
        assert!((lo - 0.0).abs() < 1e-9);
        assert!((hi - 64.0).abs() < 1e-9);
        // Without clip, taller than opening (bleed).
        let (lo, hi) = midtex_z_range(0.0, 64.0, 0.0, 1.0, 0, false, false);
        assert!((hi - 64.0).abs() < 1e-9);
        assert!((lo - (64.0 - 128.0)).abs() < 1e-9);
        // wrap fills opening.
        let (lo, hi) = midtex_z_range(0.0, 200.0, 0.0, 1.0, 0, true, false);
        assert!((lo - 0.0).abs() < 1e-9 && (hi - 200.0).abs() < 1e-9);
        // DONTPEGBOTTOM pegs to floor.
        let (lo, hi) = midtex_z_range(0.0, 256.0, 0.0, 1.0, ML_DONTPEGBOTTOM, false, false);
        assert!((lo - 0.0).abs() < 1e-9);
        assert!((hi - 128.0).abs() < 1e-9);
    }

    #[test]
    fn clipmidtex_limits_two_sided_mid_z() {
        // Two rooms sharing a midtex line: tall opening, short default midtex.
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 256.0)
            .room("b", rect(128.0, 0.0, 256.0, 128.0), 0.0, 256.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        // Find a two-sided line and set midtex.
        let mut found = false;
        for line in &mut map.lines {
            if line.back_sector.is_some() {
                if let Some(sid) = line.sides[0] {
                    map.sides[sid.raw() as usize].middle = "MIDTEX".into();
                }
                line.clip_midtex = false;
                line.wrap_midtex = false;
                found = true;
                break;
            }
        }
        assert!(found, "expected two-sided line");
        let cam = Camera {
            eye: Vec3::new(64.0, 64.0, 41.0),
            ..Camera::default()
        };
        let scene = build_scene(&map, &cam, None);
        let range = scene
            .wall_ranges
            .iter()
            .chain(scene.translucent_wall_ranges.iter())
            .find(|r| r.name == "MIDTEX")
            .expect("MIDTEX range");
        let buf = if scene
            .translucent_wall_ranges
            .iter()
            .any(|r| r.name == "MIDTEX")
        {
            &scene.translucent_walls
        } else {
            &scene.opaque_walls
        };
        let mid = &buf[range.start as usize..(range.start + range.count) as usize];
        assert!(!mid.is_empty());
        let zmin = mid.iter().map(|v| v.pos[2]).fold(f32::MAX, f32::min);
        let zmax = mid.iter().map(|v| v.pos[2]).fold(f32::MIN, f32::max);
        // Unclipped pegged: 128mu from ceiling at 256 → [128, 256].
        assert!(
            (zmax - 256.0).abs() < 1.0 && (zmin - 128.0).abs() < 1.0,
            "zmin={zmin} zmax={zmax}"
        );
    }

    #[test]
    fn translucent_mid_wall_uses_alpha_buffer() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        // One-sided walls get mid_alpha on the line.
        for line in &mut map.lines {
            line.mid_alpha = 0.25;
        }
        let cam = Camera {
            eye: Vec3::new(64.0, 64.0, 41.0),
            ..Camera::default()
        };
        let scene = build_scene(&map, &cam, None);
        assert!(!scene.translucent_walls.is_empty());
        assert!(
            scene
                .translucent_walls
                .iter()
                .all(|v| (v._pad[0] - 0.25).abs() < 1e-4)
        );
    }

    #[test]
    fn plane_reflection_emits_prefl_surface() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        map.sectors[0].floor_reflection = 0.75;
        let cam = Camera {
            eye: Vec3::new(64.0, 64.0, 41.0),
            ..Camera::default()
        };
        let scene = build_scene(&map, &cam, None);
        assert_eq!(scene.plane_reflects.len(), 1);
        assert!(scene.plane_reflects[0].is_floor);
        assert_eq!(scene.plane_reflects[0].tex_name, "__PREFL0F");
        assert!(
            scene.flat_ranges.iter().any(|r| r.name == "__PREFL0F")
                || scene
                    .translucent_flat_ranges
                    .iter()
                    .any(|r| r.name == "__PREFL0F")
        );
    }

    #[test]
    fn translucent_floor_uses_alpha_buffer() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        map.sectors[0].floor_alpha = 0.5;
        let cam = Camera {
            eye: Vec3::new(64.0, 64.0, 41.0),
            ..Camera::default()
        };
        let scene = build_scene(&map, &cam, None);
        assert!(
            scene
                .flats
                .iter()
                .all(|v| v.pos[2].abs() > 0.1 || v._pad[0] >= 0.999)
        );
        assert!(!scene.translucent_flats.is_empty());
        assert!(
            scene
                .translucent_flats
                .iter()
                .any(|v| v.pos[2].abs() < 0.1 && (v._pad[0] - 0.5).abs() < 1e-4)
        );
    }

    #[test]
    fn floor_scale_halves_flat_uv() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let cam = Camera {
            eye: Vec3::new(64.0, 64.0, 41.0),
            ..Camera::default()
        };
        let base = build_scene(&map, &cam, None);
        let base_u = base
            .flats
            .iter()
            .find(|v| v.pos[2].abs() < 0.1)
            .map(|v| v.uv[0])
            .unwrap();
        map.sectors[0].floor_scale_u = 2.0;
        map.sectors[0].floor_scale_v = 2.0;
        let scaled = build_scene(&map, &cam, None);
        let scaled_u = scaled
            .flats
            .iter()
            .find(|v| v.pos[2].abs() < 0.1)
            .map(|v| v.uv[0])
            .unwrap();
        assert!((scaled_u - base_u * 0.5).abs() < 1e-4);
    }

    #[test]
    fn side_scale_x_halves_wall_u_span() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let cam = Camera {
            eye: Vec3::new(64.0, 64.0, 41.0),
            ..Camera::default()
        };
        let base = build_scene(&map, &cam, None);
        let base_span = base.opaque_walls[1].uv[0] - base.opaque_walls[0].uv[0];
        map.sides[0].scale_x = 2.0;
        let scaled = build_scene(&map, &cam, None);
        let scaled_span = scaled.opaque_walls[1].uv[0] - scaled.opaque_walls[0].uv[0];
        assert!((scaled_span - base_span * 0.5).abs() < 1e-4);
    }

    #[test]
    fn room_emits_walls_and_flats() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let cam = Camera {
            eye: Vec3::new(128.0, 128.0, 41.0),
            ..Camera::default()
        };
        let scene = build_scene(&map, &cam, None);
        assert!(!scene.opaque_walls.is_empty(), "expected mid walls");
        assert!(!scene.flats.is_empty(), "expected floor/ceiling tris");
    }

    #[test]
    fn single_room_floor_normals_face_camera() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let cam = Camera {
            eye: Vec3::new(128.0, 128.0, 41.0),
            forward: Vec3::new(0.0, 1.0, 0.0),
            ..Camera::default()
        };
        let scene = build_scene(&map, &cam, None);
        let eye = glam::Vec3::new(128.0, 128.0, 41.0);
        let mut facing = 0usize;
        let mut back = 0usize;
        for tri in scene.flats.chunks_exact(3) {
            let zavg = (tri[0].pos[2] + tri[1].pos[2] + tri[2].pos[2]) / 3.0;
            if zavg > 8.0 {
                continue;
            }
            let a = glam::Vec3::from_array(tri[0].pos);
            let b = glam::Vec3::from_array(tri[1].pos);
            let c = glam::Vec3::from_array(tri[2].pos);
            let n = (b - a).cross(c - a);
            if n.dot(eye - a) > 0.0 {
                facing += 1;
            } else {
                back += 1;
            }
        }
        eprintln!("single room floor facing={facing} back={back}");
        assert!(facing > back, "facing={facing} back={back}");
    }

    #[test]
    fn sky_flat_names_detected() {
        assert!(is_sky_flat("F_SKY1"));
        assert!(is_sky_flat("f_sky1"));
        assert!(is_sky_flat("F_SKY"));
        assert!(is_sky_flat("F_SKY001"));
        assert!(!is_sky_flat("FLOOR0_1"));
        assert!(!is_sky_flat("SKY1"));
    }

    #[test]
    fn earclip_fan_fallback_fills_when_stuck() {
        // Near-colinear concave ring: strict ear clip can stall; fan must still fill.
        let ring = [
            (0.0, 0.0),
            (100.0, 0.0),
            (100.0, 100.0),
            (50.0, 100.0),
            (50.0, 50.0),
            (0.0, 50.0),
        ];
        let tris = earclip_tris(&ring);
        assert!(
            tris.len() >= ring.len() - 2,
            "expected full triangulation, got {}",
            tris.len()
        );
    }

    #[test]
    fn wall_quad_v_increases_toward_floor() {
        // Doom: texture top (V≈0) at ceiling, V grows toward the floor.
        let mut lists = SceneLists::default();
        push_wall_quad(
            &mut lists,
            "WALL",
            WallQuad {
                x1: 0.0,
                y1: 0.0,
                z_lo: 0.0,
                x2: 128.0,
                y2: 0.0,
                z_hi: 128.0,
                u1: 0.0,
                u2: 1.0,
                v_off: 0.0,
                v_scale: 1.0,
                light: 1.0,
                alpha: 1.0,
                masked: false,
                clamp_v: false,
                single_v: false,
                peg: WallPeg::Top,
                side_raw: None,
                lm_surf: None,
            },
        );
        assert_eq!(lists.opaque_walls.len(), 6);
        let floor_v = lists
            .opaque_walls
            .iter()
            .filter(|v| (v.pos[2] - 0.0).abs() < 1e-3)
            .map(|v| v.uv[1])
            .next()
            .unwrap();
        let ceil_v = lists
            .opaque_walls
            .iter()
            .filter(|v| (v.pos[2] - 128.0).abs() < 1e-3)
            .map(|v| v.uv[1])
            .next()
            .unwrap();
        assert!(
            ceil_v < floor_v,
            "ceil_v={ceil_v} should be < floor_v={floor_v}"
        );
        assert!((ceil_v - 0.0).abs() < 1e-4);
        assert!((floor_v - 1.0).abs() < 1e-4);
    }

    #[test]
    fn oriented_floor_tri_has_positive_nz() {
        let mut lists = SceneLists::default();
        // Deliberately clockwise in XY → should be flipped for floor.
        push_flat_tri_oriented(
            &mut lists,
            "TEST",
            [(0.0, 0.0, 0.0), (0.0, 64.0, 0.0), (64.0, 0.0, 0.0)],
            1.0,
            FlatUv {
                u_off: 0.0,
                v_off: 0.0,
                scale_u: 1.0,
                scale_v: 1.0,
                angle_deg: 0.0,
            },
            1.0,
            true,
        );
        assert_eq!(lists.flats.len(), 3);
        let a = glam::Vec3::from_array(lists.flats[0].pos);
        let b = glam::Vec3::from_array(lists.flats[1].pos);
        let c = glam::Vec3::from_array(lists.flats[2].pos);
        let nz = (b - a).cross(c - a).z;
        assert!(nz > 0.0, "nz={nz}");
    }

    #[test]
    fn combat_arena_floor_normals_face_camera() {
        use room_testkit::combat_arena;
        let mut map = combat_arena().build_udmf("MAP01").unwrap();
        apply_nodes(&mut map);
        let cam = Camera {
            eye: Vec3::new(1280.0, 144.0, 41.0),
            forward: Vec3::new(0.0, 1.0, 0.0),
            ..Camera::default()
        };
        let scene = build_scene(&map, &cam, None);
        assert!(!scene.flats.is_empty(), "need flats");
        let eye = glam::Vec3::new(1280.0, 144.0, 41.0);
        let mut floor_tris = 0usize;
        let mut facing = 0usize;
        let mut back = 0usize;
        let mut nz_neg = 0usize;
        for tri in scene.flats.chunks_exact(3) {
            let zavg = (tri[0].pos[2] + tri[1].pos[2] + tri[2].pos[2]) / 3.0;
            // Floors sit at 0 or raised cover (~24). Skip ceilings (incl. door
            // strips whose ceiling is z≈8 and would otherwise look like floors).
            let is_floor = zavg < 4.0 || (zavg - 24.0).abs() < 2.0;
            if !is_floor {
                continue;
            }
            floor_tris += 1;
            let a = glam::Vec3::from_array(tri[0].pos);
            let b = glam::Vec3::from_array(tri[1].pos);
            let c = glam::Vec3::from_array(tri[2].pos);
            let n = (b - a).cross(c - a);
            if n.z < 0.0 {
                nz_neg += 1;
            }
            if n.dot(eye - a) > 0.0 {
                facing += 1;
            } else {
                back += 1;
            }
        }
        eprintln!("floor_tris={floor_tris} nz_neg={nz_neg} eye_facing={facing} eye_back={back}");
        assert!(floor_tris > 0);
        // World +Z is what matters for a flat floor; eye-dot is unreliable when
        // looking horizontally (projected winding ≠ world normal test).
        assert_eq!(
            nz_neg, 0,
            "floor tris must have +Z normal, got {nz_neg} negative"
        );
    }

    #[test]
    fn live_sprites_replace_map_thing_placeholders() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        // Extra map thing would emit a placeholder without live sprites.
        let mut zombie = map.things[0].clone();
        zombie.type_id = 3004;
        zombie.x = 64;
        zombie.y = 64;
        map.things.push(zombie);
        apply_nodes(&mut map);
        let cam = Camera {
            eye: Vec3::new(128.0, 128.0, 41.0),
            ..Camera::default()
        };
        let with_things = build_scene(&map, &cam, None);
        assert_eq!(with_things.sprites.len(), 6); // one billboard (player start skipped)
        let live = [SpriteInstance {
            pos: Vec3::new(200.0, 200.0, 0.0),
            width: 32.0,
            height: 56.0,
            light: 1.0,
            alpha: 1.0,
            fuzz: false,
            texture: "POSSA1".into(),
            flip_u: false,
            anchor_u: 0.5,
            orient: crate::sprites::SpriteOrient::Billboard,
            additive: false,
            roll: 0.0,
            fx: Default::default(),
        }];
        let with_live = build_scene(&map, &cam, Some(&live));
        assert_eq!(with_live.sprites.len(), 6);
        // Live path should not also include the map-thing placeholder at 64,64.
        let avg_x: f32 = with_live.sprites.iter().map(|v| v.pos[0]).sum::<f32>()
            / with_live.sprites.len() as f32;
        assert!(
            avg_x > 150.0,
            "expected live sprite near 200, avg_x={avg_x}"
        );
    }

    fn corridor() -> MapGeometry {
        // 512×128 corridor; walls at y=0 and y=128.
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 512.0, 128.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        map
    }

    fn wide_billboard(x: f32, y: f32, width: f32) -> SpriteInstance {
        SpriteInstance {
            pos: Vec3::new(x, y, 0.0),
            width,
            height: 64.0,
            light: 1.0,
            alpha: 1.0,
            fuzz: false,
            texture: "POSSA1".into(),
            flip_u: false,
            anchor_u: 0.5,
            orient: crate::sprites::SpriteOrient::Billboard,
            additive: false,
            roll: 0.0,
            fx: Default::default(),
        }
    }

    fn on_view_ray(eye: Vec3, before: Vec3, after: Vec3) -> bool {
        (before - eye)
            .normalize()
            .cross((after - eye).normalize())
            .length()
            < 1e-4
    }

    #[test]
    fn billboard_beside_a_wall_is_pulled_out_of_it() {
        let map = corridor();
        // Eye in the middle of the corridor looking down +X; the monster stands
        // 24 units from the y=0 wall with an 80-wide frame, so 16 units of the
        // frame poke through that wall.
        let cam = Camera {
            eye: Vec3::new(100.0, 64.0, 41.0),
            forward: Vec3::new(1.0, 0.0, 0.0),
            ..Camera::default()
        };
        let inst = wide_billboard(300.0, 24.0, 80.0);
        let raw = emit_sprite_quad(inst.clone(), cam.forward);
        assert!(
            raw.iter().any(|v| v.pos[1] < 0.0),
            "precondition: crosses wall"
        );
        let mut lists = SceneLists::default();
        append_sprites_in_map(&mut lists, &[inst], &cam, Some(&map));
        for (r, v) in raw.iter().zip(&lists.sprites) {
            assert!(
                v.pos[1] >= -0.01,
                "vertex still behind the wall: {:?}",
                v.pos
            );
            assert!(
                on_view_ray(cam.eye, Vec3::from_array(r.pos), Vec3::from_array(v.pos)),
                "must stay on its view ray (same pixel on screen)"
            );
        }
    }

    #[test]
    fn sprites_are_drawn_far_to_near() {
        let cam = Camera {
            eye: Vec3::new(0.0, 0.0, 41.0),
            forward: Vec3::new(1.0, 0.0, 0.0),
            ..Camera::default()
        };
        let inst = |x: f32, tex: &str| SpriteInstance {
            pos: Vec3::new(x, 0.0, 0.0),
            width: 16.0,
            height: 16.0,
            light: 1.0,
            alpha: 1.0,
            fuzz: false,
            texture: tex.into(),
            flip_u: false,
            anchor_u: 0.5,
            orient: crate::sprites::SpriteOrient::Billboard,
            additive: false,
            roll: 0.0,
            fx: Default::default(),
        };
        let mut lists = SceneLists::default();
        // Actor order: near smoke, far monster, middle blood.
        append_sprites(
            &mut lists,
            &[inst(64.0, "SMOK"), inst(512.0, "POSS"), inst(200.0, "BLUD")],
            &cam,
        );
        let (v, r) = sprites_back_to_front(&lists.sprites, &lists.sprite_ranges, cam.eye);
        let names: Vec<&str> = r.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names, ["POSS", "BLUD", "SMOK"]);
        assert_eq!(v.len(), lists.sprites.len());
        assert_eq!((r[1].start, r[2].start), (6, 12));
        // Each range still carries its own quad.
        assert!(v[0].pos[0] > 400.0 && v[12].pos[0] < 100.0);
    }

    #[test]
    fn billboard_with_no_wall_nearby_is_untouched() {
        let map = corridor();
        let cam = Camera {
            eye: Vec3::new(100.0, 64.0, 41.0),
            forward: Vec3::new(1.0, 0.0, 0.0),
            ..Camera::default()
        };
        let inst = wide_billboard(300.0, 64.0, 48.0);
        let raw = emit_sprite_quad(inst.clone(), cam.forward);
        let mut lists = SceneLists::default();
        append_sprites_in_map(&mut lists, &[inst.clone()], &cam, Some(&map));
        let mut plain = SceneLists::default();
        append_sprites(&mut plain, &[inst], &cam);
        assert_eq!(lists.sprites.len(), 6);
        for (a, b) in lists.sprites.iter().zip(&plain.sprites) {
            assert_eq!(a.pos, b.pos);
        }
        let _ = raw;
    }

    #[test]
    fn wall_between_eye_and_actor_still_hides_it() {
        // Room a (floor 0) then room b (floor 64): the x=256 line is a 64-high
        // ledge between the eye and the monster. The monster also stands next to
        // b's y=0 wall, which alone would ask for a pull.
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 128.0), 0.0, 192.0)
            .room("b", rect(256.0, 0.0, 512.0, 128.0), 64.0, 192.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let cam = Camera {
            eye: Vec3::new(200.0, 64.0, 41.0),
            forward: Vec3::new(1.0, 0.0, 0.0),
            ..Camera::default()
        };
        // 6 units past the ledge: clearing the side wall would need a pull of
        // ~15 units, which would cross the ledge, so the ledge wins.
        let mut inst = wide_billboard(262.0, 20.0, 80.0);
        inst.pos.z = 64.0;
        let mut lists = SceneLists::default();
        append_sprites_in_map(&mut lists, &[inst], &cam, Some(&map));
        for v in &lists.sprites {
            assert!(
                v.pos[0] > 256.0,
                "pulled in front of the ledge: {:?}",
                v.pos
            );
        }
        let pulled = lists
            .sprites
            .iter()
            .map(|v| v.pos[0])
            .fold(f32::MAX, f32::min);
        assert!(
            pulled < 261.0,
            "it should still move as far as the ledge allows: {pulled}"
        );
    }

    #[test]
    fn wall_sprite_stays_on_its_plane() {
        let inst = SpriteInstance {
            pos: Vec3::new(100.0, 0.0, 0.0),
            width: 64.0,
            height: 32.0,
            light: 1.0,
            alpha: 1.0,
            fuzz: false,
            texture: "TAG".into(),
            flip_u: false,
            anchor_u: 0.5,
            orient: crate::sprites::SpriteOrient::Wall { yaw: 0.0 },
            additive: false,
            roll: 0.0,
            fx: Default::default(),
        };
        let cam = Camera {
            eye: Vec3::new(140.0, 0.0, 16.0),
            forward: Vec3::new(-1.0, 0.0, 0.0),
            ..Camera::default()
        };
        let mut lists = SceneLists::default();
        append_sprites(&mut lists, &[inst], &cam);
        let max_x = lists
            .sprites
            .iter()
            .map(|v| v.pos[0])
            .fold(0.0f32, f32::max);
        assert!(
            max_x < 101.0,
            "wall sprite pulled toward the eye, max_x={max_x}"
        );
    }

    #[test]
    fn sloped_floor_flats_vary_z() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        map.sectors[0].floor_plane = Some([0.0, 0.25, 1.0, 0.0]);
        map.setup = room_map::build_setup(&map);
        apply_nodes(&mut map);
        let cam = Camera {
            eye: Vec3::new(128.0, 128.0, 41.0),
            ..Camera::default()
        };
        let scene = build_scene(&map, &cam, None);
        let zs: Vec<f32> = scene
            .flats
            .iter()
            .filter(|v| v.pos[2] < 64.0)
            .map(|v| v.pos[2])
            .collect();
        assert!(zs.len() >= 3);
        let min = zs.iter().copied().fold(f32::MAX, f32::min);
        let max = zs.iter().copied().fold(f32::MIN, f32::max);
        assert!(
            (max - min).abs() > 1.0,
            "expected sloped floor Z range, min={min} max={max}"
        );
    }

    #[test]
    fn polyobj_walls_follow_moved_vertices() {
        use room_map::{
            MapFormat, RawLinedef, RawSector, RawSidedef, RawThing, RawVertex, link_geometry,
        };
        let vertices = vec![
            RawVertex { x: 0, y: 0 },
            RawVertex { x: 64, y: 0 },
            RawVertex { x: 64, y: 64 },
            RawVertex { x: 0, y: 64 },
        ];
        let linedefs = (0u16..4)
            .map(|i| RawLinedef {
                v1: i,
                v2: (i + 1) % 4,
                flags: 1,
                special: if i == 0 { 1 } else { 0 },
                tag: 0,
                side_front: i,
                side_back: 0xFFFF,
                args: if i == 0 { [7, 0, 0, 0, 0] } else { [0; 5] },
                hexen: true,
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
                extra_ids: Default::default(),
                alpha: 1.0,
                mid_additive: false,
                health: 0,
                health_group: 0,
                automap_style: 0,
                clip_midtex: false,
                wrap_midtex: false,
            })
            .collect();
        let sidedefs = (0u16..4)
            .map(|_| RawSidedef {
                x_offset: 0,
                y_offset: 0,
                upper: "-".into(),
                lower: "-".into(),
                middle: "WALL".into(),
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
                alpha: None,
                block_rendering: false,
            })
            .collect();
        let sectors = vec![RawSector {
            floor_height: 0,
            ceiling_height: 128,
            floor_pic: "FLAT".into(),
            ceiling_pic: "FLAT".into(),
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
            floor_alpha: 1.0,
            ceiling_alpha: 1.0,
            floor_additive: false,
            ceiling_additive: false,
        }];
        let things = vec![
            RawThing {
                tid: 0,
                x: 32,
                y: 32,
                z: 0,
                angle: 7,
                type_id: 9300,
                flags: 0,
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
            },
            RawThing {
                tid: 0,
                x: 96,
                y: 32,
                z: 0,
                angle: 7,
                type_id: 9301,
                flags: 0,
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
            },
            RawThing {
                tid: 0,
                x: 32,
                y: 32,
                z: 0,
                angle: 0,
                type_id: 1,
                flags: 0,
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
            },
        ];
        let mut map = link_geometry(
            "MAP01",
            MapFormat::Hexen,
            vertices,
            linedefs,
            sidedefs,
            sectors,
            things,
            [0; 16],
        );
        apply_nodes(&mut map);
        assert!(!map.setup.polyobjects.is_empty());
        let cam = Camera {
            eye: Vec3::new(32.0, 32.0, 41.0),
            ..Camera::default()
        };
        let before = build_scene(&map, &cam, None);
        assert!(!before.opaque_walls.is_empty());
        let x0 = before
            .opaque_walls
            .iter()
            .map(|v| v.pos[0])
            .fold(f32::MAX, f32::min);
        for v in &mut map.vertices {
            v.pos = room_core::DVec2::new(v.pos.x() + 40.0, v.pos.y());
        }
        let after = build_scene(&map, &cam, None);
        let x1 = after
            .opaque_walls
            .iter()
            .map(|v| v.pos[0])
            .fold(f32::MAX, f32::min);
        assert!(
            (x1 - x0 - 40.0).abs() < 1.0,
            "poly walls should track verts x0={x0} x1={x1}"
        );
    }

    #[test]
    fn ffloor_emits_extra_flats() {
        let mut map = MapBuilder::new()
            .room("play", rect(0.0, 0.0, 256.0, 256.0), 0.0, 192.0)
            .sector_tag("play", 3)
            .room("ctrl", rect(256.0, 0.0, 384.0, 256.0), 64.0, 80.0)
            .ffloor_between("ctrl", "play", 3, 1)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let cam = Camera {
            eye: Vec3::new(32.0, 32.0, 41.0),
            ..Camera::default()
        };
        let scene = build_scene(&map, &cam, None);
        let mid: Vec<f32> = scene
            .flats
            .iter()
            .map(|v| v.pos[2])
            .filter(|z| *z > 32.0 && *z < 160.0)
            .collect();
        assert!(
            mid.iter().any(|z| (*z - 64.0).abs() < 1.0),
            "expected 3D floor bottom flats, zs={mid:?}"
        );
        assert!(
            mid.iter().any(|z| (*z - 80.0).abs() < 1.0),
            "expected 3D floor top flats, zs={mid:?}"
        );
        let side_zs: Vec<f32> = scene
            .opaque_walls
            .iter()
            .map(|v| v.pos[2])
            .filter(|z| *z > 60.0 && *z < 84.0)
            .collect();
        assert!(
            side_zs.iter().any(|z| (*z - 64.0).abs() < 1.0)
                && side_zs.iter().any(|z| (*z - 80.0).abs() < 1.0),
            "expected 3D floor side walls spanning slab, zs={side_zs:?}"
        );
    }

    #[test]
    fn portal_lines_skip_opaque_walls() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .room("b", rect(512.0, 0.0, 768.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        let mut a_east = None;
        let mut b_west = None;
        for (i, line) in map.lines.iter().enumerate() {
            if line.back_sector.is_some() {
                continue;
            }
            let v1 = map.vertices[line.v1.raw() as usize].pos;
            let v2 = map.vertices[line.v2.raw() as usize].pos;
            if (v1.x() - 256.0).abs() < 1.0 && (v2.x() - 256.0).abs() < 1.0 {
                a_east = Some(i);
            }
            if (v1.x() - 512.0).abs() < 1.0 && (v2.x() - 512.0).abs() < 1.0 {
                b_west = Some(i);
            }
        }
        for i in [a_east.unwrap(), b_west.unwrap()] {
            let line = &mut map.lines[i];
            line.special = 156;
            line.args = [42, 0, 1, 0, 0];
            line.tag = 42;
        }
        map.setup = room_map::build_setup(&map);
        apply_nodes(&mut map);
        let cam = Camera {
            eye: Vec3::new(128.0, 128.0, 41.0),
            ..Camera::default()
        };
        let scene = build_scene(&map, &cam, None);
        let portal_quads = scene
            .wall_ranges
            .iter()
            .filter(|r| r.name.starts_with("__PORTAL"))
            .count();
        assert_eq!(portal_quads, 2, "both sides emit portal canvas quads");
        assert_eq!(
            scene.linked_portals.len(),
            2,
            "both sides list portal surfaces"
        );
        let opaque_at_east = scene.wall_ranges.iter().filter(|r| {
            !r.name.starts_with("__PORTAL")
                && scene.opaque_walls[r.start as usize..(r.start + r.count) as usize]
                    .iter()
                    .all(|v| (v.pos[0] - 256.0).abs() < 1.0)
        });
        assert_eq!(
            opaque_at_east.count(),
            0,
            "east portal should not emit normal opaque walls"
        );
    }

    #[test]
    fn sector_floor_portal_skips_floor_flats() {
        let mut map = MapBuilder::new()
            .room("from", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .room("to", rect(256.0, 0.0, 512.0, 256.0), 0.0, 128.0)
            .sector_tag("to", 5)
            .sector_portal_between("from", "to", 5, 3, 0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let cam = Camera {
            eye: Vec3::new(64.0, 64.0, 41.0),
            ..Camera::default()
        };
        let scene = build_scene(&map, &cam, None);
        let from_floor = scene
            .flats
            .iter()
            .any(|v| v.pos[2].abs() < 1.0 && v.pos[0] < 200.0);
        assert!(!from_floor, "source floor plane should be a portal hole");
        assert!(
            scene.flats.iter().any(|v| (v.pos[2] - 128.0).abs() < 1.0),
            "ceiling should still emit"
        );
    }

    #[test]
    fn portal_recursion_strips_at_max_depth() {
        assert!(!portal_view_strips_portals(0));
        assert!(portal_view_strips_portals(crate::MAX_PORTAL_RECURSION - 1));
        assert!(portal_view_strips_portals(crate::MAX_PORTAL_RECURSION));
    }

    #[test]
    fn dyn_lights_boost_nearby_wall_verts() {
        let mut lists = SceneLists::default();
        lists
            .opaque_walls
            .push(WallVertex::new(Vec3::new(0.0, 0.0, 32.0), Vec2::ZERO, 0.3));
        lists.opaque_walls.push(WallVertex::new(
            Vec3::new(500.0, 0.0, 32.0),
            Vec2::ZERO,
            0.3,
        ));
        lists.dyn_lights.push(DynLightInstance {
            pos: Vec3::new(0.0, 0.0, 32.0),
            color: [1.0, 1.0, 1.0],
            radius: 64.0,
            subtractive: false,
            spot_dir: None,
            spot_outer_cos: 1.0,
            spot_inner_cos: 1.0,
        });
        apply_dyn_lights(&mut lists);
        assert!(
            lists.opaque_walls[0].light > 0.5,
            "near vert should brighten"
        );
        assert!(
            (lists.opaque_walls[1].light - 0.3).abs() < 1e-3,
            "far vert unchanged"
        );
    }
}
