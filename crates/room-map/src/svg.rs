//! 2D SVG dump of map geometry (lines + optional things).

use crate::geom::MapGeometry;
use std::fmt::Write as _;

/// Render a simple top-down SVG of linedefs (and player starts).
#[must_use]
pub fn map_to_svg(map: &MapGeometry, scale: f64) -> String {
    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    for v in &map.vertices {
        min_x = min_x.min(v.pos.x());
        min_y = min_y.min(v.pos.y());
        max_x = max_x.max(v.pos.x());
        max_y = max_y.max(v.pos.y());
    }
    if !min_x.is_finite() {
        min_x = 0.0;
        min_y = 0.0;
        max_x = 1.0;
        max_y = 1.0;
    }
    let pad = 32.0;
    let w = ((max_x - min_x) * scale + pad * 2.0).max(1.0);
    let h = ((max_y - min_y) * scale + pad * 2.0).max(1.0);

    let tx = |x: f64| (x - min_x) * scale + pad;
    // SVG Y grows downward; flip map Y.
    let ty = |y: f64| (max_y - y) * scale + pad;

    let mut out = String::new();
    let _ = writeln!(
        out,
        r##"<?xml version="1.0" encoding="UTF-8"?>
<svg xmlns="http://www.w3.org/2000/svg" width="{w:.0}" height="{h:.0}" viewBox="0 0 {w:.1} {h:.1}">
<rect width="100%" height="100%" fill="#1a1a1a"/>
<g stroke-linecap="round">"##
    );

    for line in &map.lines {
        let Some(v1) = map.vertices.get(line.v1.raw() as usize) else {
            continue;
        };
        let Some(v2) = map.vertices.get(line.v2.raw() as usize) else {
            continue;
        };
        let two_sided = line.back_sector.is_some();
        let (stroke, width) = if two_sided {
            ("#666666", 1.0)
        } else {
            ("#cccccc", 1.5)
        };
        let _ = writeln!(
            out,
            r##"<line x1="{:.2}" y1="{:.2}" x2="{:.2}" y2="{:.2}" stroke="{stroke}" stroke-width="{width}"/>"##,
            tx(v1.pos.x()),
            ty(v1.pos.y()),
            tx(v2.pos.x()),
            ty(v2.pos.y()),
        );
    }

    for thing in &map.things {
        if thing.type_id == 1 {
            let _ = writeln!(
                out,
                r##"<circle cx="{:.2}" cy="{:.2}" r="3" fill="#4caf50"/>"##,
                tx(f64::from(thing.x)),
                ty(f64::from(thing.y)),
            );
        }
    }

    let _ = writeln!(out, "</g>\n</svg>");
    out
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use crate::{MapFormat, RawLinedef, RawSector, RawSidedef, RawThing, RawVertex, link_geometry};

    #[test]
    fn svg_contains_line_and_start() {
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
                special: 0,
                tag: 0,
                side_front: i,
                side_back: 0xFFFF,
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
                extra_ids: Default::default(),
                alpha: 1.0,
                mid_additive: false,
                health: 0,
                health_group: 0,
                automap_style: 0,
                clip_midtex: false,
                wrap_midtex: false,
                ext: 0,
                user: Default::default(),
            })
            .collect();
        let sidedefs = (0u16..4)
            .map(|_| RawSidedef {
                x_offset: 0,
                y_offset: 0,
                upper: "-".into(),
                lower: "-".into(),
                middle: "STARTAN2".into(),
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
                user: Default::default(),
            })
            .collect();
        let sectors = vec![RawSector {
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
            ext: 0,
            user: Default::default(),
        }];
        let things = vec![RawThing {
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
        }];
        let map = link_geometry(
            "TEST",
            MapFormat::Doom,
            vertices,
            linedefs,
            sidedefs,
            sectors,
            things,
            [0u8; 16],
        );
        let svg = map_to_svg(&map, 1.0);
        assert!(svg.contains("<line "));
        assert!(svg.contains("<circle "));
        assert!(svg.contains("#4caf50"));
    }
}
