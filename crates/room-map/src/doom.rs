//! Doom-format binary map lump parsers.

use crate::error::MapError;
use crate::raw::{RawLinedef, RawSector, RawSidedef, RawThing, RawVertex};

fn name8(raw: &[u8]) -> String {
    let end = raw.iter().position(|&b| b == 0).unwrap_or(8).min(8);
    String::from_utf8_lossy(&raw[..end]).to_ascii_uppercase()
}

/// Parse Doom `VERTEXES` (4 bytes each).
pub fn parse_vertexes(data: &[u8]) -> Result<Vec<RawVertex>, MapError> {
    if data.len() % 4 != 0 {
        return Err(MapError::Truncated("VERTEXES"));
    }
    let mut out = Vec::with_capacity(data.len() / 4);
    for chunk in data.chunks_exact(4) {
        out.push(RawVertex {
            x: i16::from_le_bytes([chunk[0], chunk[1]]),
            y: i16::from_le_bytes([chunk[2], chunk[3]]),
        });
    }
    Ok(out)
}

/// Parse Doom `LINEDEFS` (14 bytes each).
pub fn parse_linedefs(data: &[u8]) -> Result<Vec<RawLinedef>, MapError> {
    if data.len() % 14 != 0 {
        return Err(MapError::Truncated("LINEDEFS"));
    }
    let mut out = Vec::with_capacity(data.len() / 14);
    for chunk in data.chunks_exact(14) {
        out.push(RawLinedef {
            v1: u16::from_le_bytes([chunk[0], chunk[1]]),
            v2: u16::from_le_bytes([chunk[2], chunk[3]]),
            flags: i16::from_le_bytes([chunk[4], chunk[5]]),
            special: i16::from_le_bytes([chunk[6], chunk[7]]),
            tag: i16::from_le_bytes([chunk[8], chunk[9]]),
            side_front: u16::from_le_bytes([chunk[10], chunk[11]]),
            side_back: u16::from_le_bytes([chunk[12], chunk[13]]),
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
        });
    }
    Ok(out)
}

/// Parse Hexen `LINEDEFS` (16 bytes each).
pub fn parse_linedefs_hexen(data: &[u8]) -> Result<Vec<RawLinedef>, MapError> {
    if data.len() % 16 != 0 {
        return Err(MapError::Truncated("LINEDEFS hexen"));
    }
    let mut out = Vec::with_capacity(data.len() / 16);
    for chunk in data.chunks_exact(16) {
        out.push(RawLinedef {
            v1: u16::from_le_bytes([chunk[0], chunk[1]]),
            v2: u16::from_le_bytes([chunk[2], chunk[3]]),
            flags: i16::from_le_bytes([chunk[4], chunk[5]]),
            special: i16::from(chunk[6]),
            tag: 0,
            side_front: u16::from_le_bytes([chunk[12], chunk[13]]),
            side_back: u16::from_le_bytes([chunk[14], chunk[15]]),
            args: [
                i32::from(chunk[7]),
                i32::from(chunk[8]),
                i32::from(chunk[9]),
                i32::from(chunk[10]),
                i32::from(chunk[11]),
            ],
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
        });
    }
    Ok(out)
}

/// Parse `SIDEDEFS` (30 bytes each).
pub fn parse_sidedefs(data: &[u8]) -> Result<Vec<RawSidedef>, MapError> {
    if data.len() % 30 != 0 {
        return Err(MapError::Truncated("SIDEDEFS"));
    }
    let mut out = Vec::with_capacity(data.len() / 30);
    for chunk in data.chunks_exact(30) {
        // Doom format has one textureoffset/rowoffset for all tiers; UDMF can
        // override per-tier. Copy into top/bottom so the renderer does not
        // treat unset UDMF fields as a hard zero (E1M1 SW1COMP lives on lower).
        let x_offset = i16::from_le_bytes([chunk[0], chunk[1]]);
        let y_offset = i16::from_le_bytes([chunk[2], chunk[3]]);
        out.push(RawSidedef {
            x_offset,
            y_offset,
            upper: name8(&chunk[4..12]),
            lower: name8(&chunk[12..20]),
            middle: name8(&chunk[20..28]),
            sector: u16::from_le_bytes([chunk[28], chunk[29]]),
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
            x_offset_top: x_offset,
            y_offset_top: y_offset,
            x_offset_bottom: x_offset,
            y_offset_bottom: y_offset,
            no_fake_contrast: false,
            smooth_lighting: false,
            scroll_x: 0.0,
            scroll_y: 0.0,
            clip_midtex: false,
            wrap_midtex: false,
            no_decals: false,
        });
    }
    Ok(out)
}

/// Parse `SECTORS` (26 bytes each).
pub fn parse_sectors(data: &[u8]) -> Result<Vec<RawSector>, MapError> {
    if data.len() % 26 != 0 {
        return Err(MapError::Truncated("SECTORS"));
    }
    let mut out = Vec::with_capacity(data.len() / 26);
    for chunk in data.chunks_exact(26) {
        out.push(RawSector {
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
            floor_height: i16::from_le_bytes([chunk[0], chunk[1]]),
            ceiling_height: i16::from_le_bytes([chunk[2], chunk[3]]),
            floor_pic: name8(&chunk[4..12]),
            ceiling_pic: name8(&chunk[12..20]),
            light: i16::from_le_bytes([chunk[20], chunk[21]]),
            special: i16::from_le_bytes([chunk[22], chunk[23]]),
            tag: i16::from_le_bytes([chunk[24], chunk[25]]),
        });
    }
    Ok(out)
}

/// Parse Doom `THINGS` (10 bytes each).
pub fn parse_things(data: &[u8]) -> Result<Vec<RawThing>, MapError> {
    if data.len() % 10 != 0 {
        return Err(MapError::Truncated("THINGS"));
    }
    let mut out = Vec::with_capacity(data.len() / 10);
    for chunk in data.chunks_exact(10) {
        out.push(RawThing {
            tid: 0,
            x: i16::from_le_bytes([chunk[0], chunk[1]]),
            y: i16::from_le_bytes([chunk[2], chunk[3]]),
            z: 0,
            angle: i16::from_le_bytes([chunk[4], chunk[5]]),
            type_id: i16::from_le_bytes([chunk[6], chunk[7]]),
            flags: i16::from_le_bytes([chunk[8], chunk[9]]),
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
        });
    }
    Ok(out)
}

/// Parse Hexen `THINGS` (20 bytes each).
pub fn parse_things_hexen(data: &[u8]) -> Result<Vec<RawThing>, MapError> {
    if data.len() % 20 != 0 {
        return Err(MapError::Truncated("THINGS hexen"));
    }
    let mut out = Vec::with_capacity(data.len() / 20);
    for chunk in data.chunks_exact(20) {
        out.push(RawThing {
            tid: i16::from_le_bytes([chunk[0], chunk[1]]),
            x: i16::from_le_bytes([chunk[2], chunk[3]]),
            y: i16::from_le_bytes([chunk[4], chunk[5]]),
            z: i16::from_le_bytes([chunk[6], chunk[7]]),
            angle: i16::from_le_bytes([chunk[8], chunk[9]]),
            type_id: i16::from_le_bytes([chunk[10], chunk[11]]),
            flags: i16::from_le_bytes([chunk[12], chunk[13]]),
            special: chunk[14],
            args: [
                i32::from(chunk[15]),
                i32::from(chunk[16]),
                i32::from(chunk[17]),
                i32::from(chunk[18]),
                i32::from(chunk[19]),
            ],
            hexen: true,
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
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn vertexes_roundtrip_size() {
        let mut b = Vec::new();
        b.extend_from_slice(&0i16.to_le_bytes());
        b.extend_from_slice(&64i16.to_le_bytes());
        b.extend_from_slice(&128i16.to_le_bytes());
        b.extend_from_slice(&(-32i16).to_le_bytes());
        let v = parse_vertexes(&b).unwrap();
        assert_eq!(v.len(), 2);
        assert_eq!(v[1].x, 128);
        assert_eq!(v[1].y, -32);
    }
}
