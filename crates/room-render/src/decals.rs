//! Renderer-owned wall decal store (M7 — wall-clip + fader/stretcher/slider + billboard fallback).

use crate::camera::Camera;
use crate::mesh::WallVertex;
use crate::scene::{MaterialRange, SceneLists};
use crate::sprites::{SpriteInstance, emit_sprite_quad};
use glam::{Vec2, Vec3};
use room_map::MapGeometry;

/// One placed decal in world space.
#[derive(Clone, Debug, PartialEq)]
pub struct DecalInstance {
    /// DECALDEF / patch name (texture lookup).
    pub name: String,
    /// World position.
    pub pos: Vec3,
    /// Width in map units (animated by stretcher).
    pub width: f32,
    /// Height in map units (animated by stretcher).
    pub height: f32,
    /// Placement width (stretch start).
    pub base_width: f32,
    /// Placement height (stretch start).
    pub base_height: f32,
    /// Light 0–1 (colorchanger animates toward goal luminance).
    pub light: f32,
    /// Placement light (colorchanger start).
    pub base_light: f32,
    /// Current opacity 0..=1 (fader animates this).
    pub alpha: f32,
    /// Tics since placement.
    pub age_tics: u32,
    /// Tics before fade begins (`u32::MAX` = never).
    pub fade_start_tics: u32,
    /// Fade duration in tics (`0` = no fade).
    pub fade_time_tics: u32,
    /// Stretch start tics (`u32::MAX` = none).
    pub stretch_start_tics: u32,
    /// Stretch duration (`0` = none).
    pub stretch_time_tics: u32,
    /// Goal X scale (`None` = leave width).
    pub stretch_goal_x: Option<f32>,
    /// Goal Y scale (`None` = leave height).
    pub stretch_goal_y: Option<f32>,
    /// Slide start tics (`u32::MAX` = none).
    pub slide_start_tics: u32,
    /// Slide duration (`0` = none).
    pub slide_time_tics: u32,
    /// Vertical slide distance (map units).
    pub slide_dist_y: f32,
    /// Z at slide start.
    pub slide_start_z: f32,
    /// Colorchanger start tics (`u32::MAX` = none).
    pub color_start_tics: u32,
    /// Colorchanger duration (`0` = none).
    pub color_time_tics: u32,
    /// Goal light (luminance of goal RGB).
    pub color_goal_light: f32,
    /// Optional linedef index (wall-clipped when map is provided).
    pub line: Option<u32>,
}

/// Cap on retained decals (`cl_maxdecals` bootstrap default).
pub const MAX_DECALS: usize = 1024;

/// Optional animator bundle at ingest time.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DecalIngestAnim {
    /// Fader `(start_tics, time_tics)`.
    pub fade: Option<(u32, u32)>,
    /// Stretch: start, time, goal_x, goal_y.
    pub stretch: Option<(u32, u32, Option<f32>, Option<f32>)>,
    /// Slide: start, time, dist_y.
    pub slide: Option<(u32, u32, f32)>,
    /// Colorchanger: start, time, goal_light.
    pub color: Option<(u32, u32, f32)>,
}

/// Persistent decal list owned by the renderer.
#[derive(Clone, Debug, Default)]
pub struct DecalStore {
    /// Active decals (oldest dropped when over [`MAX_DECALS`]).
    pub decals: Vec<DecalInstance>,
}

impl DecalStore {
    /// Push a decal, evicting the oldest when over the cap.
    pub fn push(&mut self, decal: DecalInstance) {
        if self.decals.len() >= MAX_DECALS {
            self.decals.remove(0);
        }
        self.decals.push(decal);
    }

    /// Ingest shell-facing events `(name, x, y, z, line)`.
    pub fn ingest(
        &mut self,
        events: impl IntoIterator<Item = (String, f64, f64, f64, Option<u32>)>,
    ) {
        for (name, x, y, z, line) in events {
            self.ingest_one(name, x, y, z, line, DecalIngestAnim::default());
        }
    }

    /// Ingest one event with optional fader/stretcher/slider.
    pub fn ingest_one(
        &mut self,
        name: String,
        x: f64,
        y: f64,
        z: f64,
        line: Option<u32>,
        anim: DecalIngestAnim,
    ) {
        if name.is_empty() {
            return;
        }
        let (fade_start_tics, fade_time_tics) = anim.fade.unwrap_or((u32::MAX, 0));
        let (stretch_start_tics, stretch_time_tics, stretch_goal_x, stretch_goal_y) =
            anim.stretch.unwrap_or((u32::MAX, 0, None, None));
        let (slide_start_tics, slide_time_tics, slide_dist_y) =
            anim.slide.unwrap_or((u32::MAX, 0, 0.0));
        let (color_start_tics, color_time_tics, color_goal_light) =
            anim.color.unwrap_or((u32::MAX, 0, 0.0));
        #[allow(clippy::cast_possible_truncation)]
        let zf = z as f32;
        #[allow(clippy::cast_possible_truncation)]
        self.push(DecalInstance {
            name,
            pos: Vec3::new(x as f32, y as f32, zf),
            width: 8.0,
            height: 8.0,
            base_width: 8.0,
            base_height: 8.0,
            light: 1.0,
            base_light: 1.0,
            alpha: 1.0,
            age_tics: 0,
            fade_start_tics,
            fade_time_tics,
            stretch_start_tics,
            stretch_time_tics,
            stretch_goal_x,
            stretch_goal_y,
            slide_start_tics,
            slide_time_tics,
            slide_dist_y,
            slide_start_z: zf,
            color_start_tics,
            color_time_tics,
            color_goal_light,
            line,
        });
    }

    /// Advance animators one tic; remove finished fades.
    pub fn tick(&mut self) {
        self.decals.retain_mut(|d| {
            d.age_tics = d.age_tics.saturating_add(1);
            tick_stretch(d);
            tick_slide(d);
            tick_color(d);
            if d.fade_time_tics == 0 || d.age_tics < d.fade_start_tics {
                return true;
            }
            let elapsed = d.age_tics - d.fade_start_tics;
            if elapsed >= d.fade_time_tics {
                return false;
            }
            #[allow(clippy::cast_precision_loss)]
            let frac = 1.0 - (elapsed as f32 / d.fade_time_tics as f32);
            d.alpha = frac.clamp(0.0, 1.0);
            true
        });
    }

    /// Clear all decals (map change).
    pub fn clear(&mut self) {
        self.decals.clear();
    }
}

fn tick_stretch(d: &mut DecalInstance) {
    if d.stretch_time_tics == 0 || d.age_tics < d.stretch_start_tics {
        return;
    }
    let elapsed = d.age_tics - d.stretch_start_tics;
    let frac = if elapsed >= d.stretch_time_tics {
        1.0
    } else {
        #[allow(clippy::cast_precision_loss)]
        {
            elapsed as f32 / d.stretch_time_tics as f32
        }
    };
    if let Some(gx) = d.stretch_goal_x {
        d.width = d.base_width * (1.0 + (gx - 1.0) * frac);
    }
    if let Some(gy) = d.stretch_goal_y {
        d.height = d.base_height * (1.0 + (gy - 1.0) * frac);
    }
}

fn tick_slide(d: &mut DecalInstance) {
    if d.slide_time_tics == 0 || d.age_tics < d.slide_start_tics {
        return;
    }
    let elapsed = d.age_tics - d.slide_start_tics;
    let frac = if elapsed >= d.slide_time_tics {
        1.0
    } else {
        #[allow(clippy::cast_precision_loss)]
        {
            elapsed as f32 / d.slide_time_tics as f32
        }
    };
    d.pos.z = d.slide_start_z + d.slide_dist_y * frac;
}

fn tick_color(d: &mut DecalInstance) {
    if d.color_time_tics == 0 || d.age_tics < d.color_start_tics {
        return;
    }
    let elapsed = d.age_tics - d.color_start_tics;
    let frac = if elapsed >= d.color_time_tics {
        1.0
    } else {
        #[allow(clippy::cast_precision_loss)]
        {
            elapsed as f32 / d.color_time_tics as f32
        }
    };
    d.light = d.base_light + (d.color_goal_light - d.base_light) * frac;
}

/// Append stored decals: wall-aligned when `line` + `map` resolve, else camera billboards.
pub fn append_decals(
    lists: &mut SceneLists,
    store: &DecalStore,
    camera: &Camera,
    map: Option<&MapGeometry>,
) {
    for d in &store.decals {
        if d.alpha <= 0.001 {
            continue;
        }
        if let (Some(map), Some(li)) = (map, d.line) {
            if emit_wall_decal(lists, map, d, li) {
                continue;
            }
        }
        #[allow(clippy::cast_possible_truncation)]
        let start = lists.sprites.len() as u32;
        let quad = emit_sprite_quad(
            SpriteInstance {
                pos: d.pos,
                width: d.width,
                height: d.height,
                light: d.light,
                alpha: d.alpha,
                fuzz: false,
                texture: d.name.clone(),
                flip_u: false,
                anchor_u: 0.5,
                orient: crate::sprites::SpriteOrient::Billboard,
            },
            camera.forward,
        );
        lists.sprites.extend_from_slice(&quad);
        lists.sprite_ranges.push(MaterialRange {
            name: d.name.clone(),
            start,
            count: 6,
        });
    }
}

/// Project onto linedef and emit a wall-space quad. Returns false if the line is missing.
fn emit_wall_decal(
    lists: &mut SceneLists,
    map: &MapGeometry,
    d: &DecalInstance,
    line_i: u32,
) -> bool {
    let Some(line) = map.lines.get(line_i as usize) else {
        return false;
    };
    let Some(v1) = map.vertices.get(line.v1.raw() as usize) else {
        return false;
    };
    let Some(v2) = map.vertices.get(line.v2.raw() as usize) else {
        return false;
    };
    let ax = v1.pos.x();
    let ay = v1.pos.y();
    let bx = v2.pos.x();
    let by = v2.pos.y();
    let dx = bx - ax;
    let dy = by - ay;
    let len2 = dx * dx + dy * dy;
    if len2 < 1e-8 {
        return false;
    }
    let px = f64::from(d.pos.x);
    let py = f64::from(d.pos.y);
    let t = ((px - ax) * dx + (py - ay) * dy) / len2;
    let t = t.clamp(0.0, 1.0);
    let cx = ax + dx * t;
    let cy = ay + dy * t;
    let len = len2.sqrt();
    let tx = dx / len;
    let ty = dy / len;
    let nx = -ty;
    let ny = tx;
    let bias = 0.5;
    let hw = f64::from(d.width) * 0.5;
    let hh = f64::from(d.height) * 0.5;
    let z = f64::from(d.pos.z);
    let x0 = cx - tx * hw + nx * bias;
    let y0 = cy - ty * hw + ny * bias;
    let x1 = cx + tx * hw + nx * bias;
    let y1 = cy + ty * hw + ny * bias;
    let z0 = z - hh;
    let z1 = z + hh;
    #[allow(clippy::cast_possible_truncation)]
    let light = d.light;
    let a = d.alpha;
    #[allow(clippy::cast_possible_truncation)]
    let verts = [
        WallVertex::new(
            Vec3::new(x0 as f32, y0 as f32, z0 as f32),
            Vec2::new(0.0, 1.0),
            light,
        )
        .with_alpha(a)
        .with_masked(),
        WallVertex::new(
            Vec3::new(x1 as f32, y1 as f32, z0 as f32),
            Vec2::new(1.0, 1.0),
            light,
        )
        .with_alpha(a)
        .with_masked(),
        WallVertex::new(
            Vec3::new(x1 as f32, y1 as f32, z1 as f32),
            Vec2::new(1.0, 0.0),
            light,
        )
        .with_alpha(a)
        .with_masked(),
        WallVertex::new(
            Vec3::new(x0 as f32, y0 as f32, z0 as f32),
            Vec2::new(0.0, 1.0),
            light,
        )
        .with_alpha(a)
        .with_masked(),
        WallVertex::new(
            Vec3::new(x1 as f32, y1 as f32, z1 as f32),
            Vec2::new(1.0, 0.0),
            light,
        )
        .with_alpha(a)
        .with_masked(),
        WallVertex::new(
            Vec3::new(x0 as f32, y0 as f32, z1 as f32),
            Vec2::new(0.0, 0.0),
            light,
        )
        .with_alpha(a)
        .with_masked(),
    ];
    #[allow(clippy::cast_possible_truncation)]
    let start = lists.translucent_walls.len() as u32;
    lists.translucent_walls.extend_from_slice(&verts);
    lists.translucent_wall_ranges.push(MaterialRange {
        name: d.name.clone(),
        start,
        count: 6,
    });
    true
}

/// Billboard helper used by tests / callers that already have a [`DecalInstance`].
pub fn emit_decal_quad(d: &DecalInstance, forward: Vec3) -> [WallVertex; 6] {
    emit_sprite_quad(
        SpriteInstance {
            pos: d.pos,
            width: d.width,
            height: d.height,
            light: d.light,
            alpha: d.alpha,
            fuzz: false,
            texture: d.name.clone(),
            flip_u: false,
            anchor_u: 0.5,
            orient: crate::sprites::SpriteOrient::Billboard,
        },
        forward,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use room_nodebuild::apply_nodes;
    use room_testkit::{MapBuilder, rect};

    fn blank_instance(name: &str) -> DecalInstance {
        DecalInstance {
            name: name.into(),
            pos: Vec3::ZERO,
            width: 8.0,
            height: 8.0,
            base_width: 8.0,
            base_height: 8.0,
            light: 1.0,
            base_light: 1.0,
            alpha: 1.0,
            age_tics: 0,
            fade_start_tics: u32::MAX,
            fade_time_tics: 0,
            stretch_start_tics: u32::MAX,
            stretch_time_tics: 0,
            stretch_goal_x: None,
            stretch_goal_y: None,
            slide_start_tics: u32::MAX,
            slide_time_tics: 0,
            slide_dist_y: 0.0,
            slide_start_z: 0.0,
            color_start_tics: u32::MAX,
            color_time_tics: 0,
            color_goal_light: 0.0,
            line: None,
        }
    }

    #[test]
    fn clear_drops_ingested_impacts() {
        let mut store = DecalStore::default();
        store.ingest([
            ("BulletChip".into(), 0.0, 0.0, 32.0, None),
            ("BulletPuff".into(), 8.0, 0.0, 32.0, Some(1)),
        ]);
        assert_eq!(store.decals.len(), 2);
        store.clear();
        let cam = Camera::default();
        let mut lists = SceneLists::default();
        append_decals(&mut lists, &store, &cam, None);
        assert!(store.decals.is_empty());
        assert!(lists.sprites.is_empty());
        assert!(lists.sprite_ranges.is_empty());
    }

    #[test]
    fn store_caps_at_max() {
        let mut store = DecalStore::default();
        for i in 0..(MAX_DECALS + 10) {
            let mut d = blank_instance(&format!("D{i}"));
            d.name = format!("D{i}");
            store.push(d);
        }
        assert_eq!(store.decals.len(), MAX_DECALS);
        assert_eq!(store.decals[0].name, format!("D{}", 10));
    }

    #[test]
    fn append_adds_six_verts_per_decal() {
        let mut store = DecalStore::default();
        store.ingest([("BulletChip".into(), 0.0, 0.0, 32.0, None)]);
        let cam = Camera::default();
        let mut lists = SceneLists::default();
        append_decals(&mut lists, &store, &cam, None);
        assert_eq!(lists.sprites.len(), 6);
        assert_eq!(lists.sprite_ranges[0].name, "BulletChip");
    }

    #[test]
    fn wall_line_emits_translucent_quad() {
        let Ok(mut map) = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
        else {
            panic!("build_udmf");
        };
        apply_nodes(&mut map);
        let line = 0u32;
        let mut store = DecalStore::default();
        store.ingest([("BloodSplat".into(), 128.0, 0.0, 40.0, Some(line))]);
        let cam = Camera::default();
        let mut lists = SceneLists::default();
        append_decals(&mut lists, &store, &cam, Some(&map));
        assert!(lists.sprites.is_empty(), "wall decals skip billboards");
        assert_eq!(lists.translucent_walls.len(), 6);
        assert_eq!(lists.translucent_wall_ranges[0].name, "BloodSplat");
    }

    #[test]
    fn fader_reduces_alpha_then_removes() {
        let mut store = DecalStore::default();
        store.ingest_one(
            "Chip".into(),
            0.0,
            0.0,
            0.0,
            None,
            DecalIngestAnim {
                fade: Some((2, 4)),
                ..Default::default()
            },
        );
        store.tick(); // age 1 — before fade
        assert_eq!(store.decals[0].alpha, 1.0);
        store.tick(); // age 2 — fade start
        assert!((store.decals[0].alpha - 1.0).abs() < 1e-4);
        store.tick(); // age 3 — 1/4 elapsed → alpha 0.75
        assert!((store.decals[0].alpha - 0.75).abs() < 1e-3);
        store.tick();
        store.tick();
        store.tick(); // age 6 — gone
        assert!(store.decals.is_empty());
    }

    #[test]
    fn stretcher_grows_height_to_goal() {
        let mut store = DecalStore::default();
        store.ingest_one(
            "Smear".into(),
            0.0,
            0.0,
            40.0,
            None,
            DecalIngestAnim {
                stretch: Some((0, 4, None, Some(2.0))),
                ..Default::default()
            },
        );
        store.tick(); // age 1 → 25%
        assert!((store.decals[0].height - 10.0).abs() < 1e-3);
        store.tick();
        store.tick();
        store.tick(); // age 4 → goal
        assert!((store.decals[0].height - 16.0).abs() < 1e-3);
        assert!((store.decals[0].width - 8.0).abs() < 1e-3);
    }

    #[test]
    fn slider_moves_z() {
        let mut store = DecalStore::default();
        store.ingest_one(
            "Smear".into(),
            0.0,
            0.0,
            40.0,
            None,
            DecalIngestAnim {
                slide: Some((0, 4, -8.0)),
                ..Default::default()
            },
        );
        store.tick();
        assert!((store.decals[0].pos.z - 38.0).abs() < 1e-3);
        store.tick();
        store.tick();
        store.tick();
        assert!((store.decals[0].pos.z - 32.0).abs() < 1e-3);
    }

    #[test]
    fn colorchanger_dims_light() {
        let mut store = DecalStore::default();
        store.ingest_one(
            "Chip".into(),
            0.0,
            0.0,
            0.0,
            None,
            DecalIngestAnim {
                color: Some((0, 4, 0.0)),
                ..Default::default()
            },
        );
        store.tick();
        assert!((store.decals[0].light - 0.75).abs() < 1e-3);
        store.tick();
        store.tick();
        store.tick();
        assert!(store.decals[0].light.abs() < 1e-3);
    }
}
