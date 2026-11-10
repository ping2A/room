//! Raw Doom/Hexen map records before geometry linking.

use serde::{Deserialize, Serialize};

/// Doom/Hexen vertex (integer map units).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct RawVertex {
    /// X.
    pub x: i16,
    /// Y.
    pub y: i16,
}

/// Linedef.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RawLinedef {
    /// Start vertex.
    pub v1: u16,
    /// End vertex.
    pub v2: u16,
    /// ML_* flags.
    pub flags: i16,
    /// Special / action.
    pub special: i16,
    /// Tag (Doom) or unused (Hexen).
    pub tag: i16,
    /// Front sidedef (`0xFFFF` = none).
    pub side_front: u16,
    /// Back sidedef.
    pub side_back: u16,
    /// Hexen args.
    pub args: [i32; 5],
    /// True when parsed from Hexen 16-byte linedefs.
    pub hexen: bool,
    /// Activation mask (`spac::*`); 0 = unset / accept any (legacy Doom).
    pub activation: u32,
    /// UDMF `arg0str` — named ACS script for specials 80–86 / 226 (empty = unset).
    pub arg0_str: String,
    /// UDMF `midtex3d` — walkable 3D middle texture.
    pub midtex3d: bool,
    /// UDMF `midtex3dimpassible` — midtex blocks like a short impassable line (missiles pass).
    pub midtex3d_impassible: bool,
    /// UDMF `checkswitchrange` — Use activation requires vertical reach.
    pub check_switch_range: bool,
    /// UDMF `firstsideonly` — Use/Push only from the front side.
    pub first_side_only: bool,
    /// UDMF `blockfloaters` — blocks `+FLOAT` actors like a solid wall.
    pub block_floaters: bool,
    /// UDMF `locknumber` — key lock id (overrides special lock args when ≠ 0).
    pub lock_number: i32,
    /// UDMF `blockprojectiles` — blocks `+MISSILE` actors.
    pub block_projectiles: bool,
    /// UDMF `blocksight` — blocks line-of-sight through the linedef.
    pub block_sight: bool,
    /// UDMF `blockhitscan` — blocks hitscan traces through the linedef.
    pub block_hitscan: bool,
    /// UDMF `moreids` — additional line ids beyond `id`/`tag`.
    pub extra_ids: Vec<i32>,
    /// UDMF `alpha` — mid-texture opacity (1.0 = opaque).
    pub alpha: f32,
    /// UDMF `renderstyle` Add/Additive → mid additive blend.
    pub mid_additive: bool,
    /// UDMF `health` — destructible line HP (`0` = unset / immortal).
    pub health: i32,
    /// UDMF `healthgroup` — sync HP with other destructibles (`0` = none).
    pub health_group: i32,
    /// UDMF `automapstyle` — `AML_*` override (`0` = default).
    pub automap_style: i32,
    /// UDMF `clipmidtex` — mid textures clipped to floor and ceiling.
    pub clip_midtex: bool,
    /// UDMF `wrapmidtex` — mid textures wrap vertically.
    pub wrap_midtex: bool,
}

/// Sidedef.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RawSidedef {
    /// Texture X offset.
    pub x_offset: i16,
    /// Texture Y offset.
    pub y_offset: i16,
    /// Upper texture.
    pub upper: String,
    /// Lower texture.
    pub lower: String,
    /// Middle texture.
    pub middle: String,
    /// Sector index.
    pub sector: u16,
    /// UDMF `light` (`i16::MIN` = unset → use sector light).
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
    /// UDMF `scalex_mid` / `scalex` (1.0 = default).
    pub scale_x: f32,
    /// UDMF `scaley_mid` / `scaley`.
    pub scale_y: f32,
    /// UDMF `scalex_top`.
    pub scale_x_top: f32,
    /// UDMF `scaley_top`.
    pub scale_y_top: f32,
    /// UDMF `scalex_bottom`.
    pub scale_x_bottom: f32,
    /// UDMF `scaley_bottom`.
    pub scale_y_bottom: f32,
    /// UDMF `offsetx_top` (falls back to `offsetx` when unset at parse).
    pub x_offset_top: i16,
    /// UDMF `offsety_top`.
    pub y_offset_top: i16,
    /// UDMF `offsetx_bottom`.
    pub x_offset_bottom: i16,
    /// UDMF `offsety_bottom`.
    pub y_offset_bottom: i16,
    /// UDMF `nofakecontrast` — disable directional wall shading.
    pub no_fake_contrast: bool,
    /// UDMF `smoothlighting` — milder fake contrast.
    pub smooth_lighting: bool,
    /// UDMF `xscroll` — wall texture scroll X (texels / tic).
    pub scroll_x: f32,
    /// UDMF `yscroll` — wall texture scroll Y (texels / tic).
    pub scroll_y: f32,
    /// UDMF `clipmidtex` — side mid textures clipped to floor/ceiling.
    pub clip_midtex: bool,
    /// UDMF `wrapmidtex` — side mid textures wrap vertically.
    pub wrap_midtex: bool,
    /// UDMF `nodecals` — do not attach impact decals to this sidedef.
    pub no_decals: bool,
    /// UDMF sidedef `alpha` (UZDoom 5.0): overrides the linedef alpha for
    /// this side's mid texture. `None` = use the linedef's.
    pub alpha: Option<f32>,
    /// UDMF `blockrendering` (`WALLF_BLOCKRENDERING`): nothing behind this
    /// side is drawn when seen through it (a two-sided line rendered as solid).
    pub block_rendering: bool,
}

/// Sector.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RawSector {
    /// Floor height.
    pub floor_height: i16,
    /// Ceiling height.
    pub ceiling_height: i16,
    /// Floor flat.
    pub floor_pic: String,
    /// Ceiling flat.
    pub ceiling_pic: String,
    /// Light level.
    pub light: i16,
    /// Sector special.
    pub special: i16,
    /// Tag.
    pub tag: i16,
    /// Optional UDMF `floorplane_a..d` (nx, ny, nz, d).
    pub floor_plane: Option<[f64; 4]>,
    /// Optional UDMF `ceilingplane_a..d`.
    pub ceiling_plane: Option<[f64; 4]>,
    /// UDMF `moreids` — additional sector tags beyond `id`.
    pub extra_ids: Vec<i32>,
    /// UDMF `xpanningfloor` (map units).
    pub floor_x_pan: f64,
    /// UDMF `ypanningfloor`.
    pub floor_y_pan: f64,
    /// UDMF `xpanningceiling`.
    pub ceiling_x_pan: f64,
    /// UDMF `ypanningceiling`.
    pub ceiling_y_pan: f64,
    /// UDMF `rotationfloor` (degrees).
    pub floor_rotation: f64,
    /// UDMF `rotationceiling`.
    pub ceiling_rotation: f64,
    /// UDMF `xscalefloor` (1.0 = default).
    pub floor_x_scale: f64,
    /// UDMF `yscalefloor`.
    pub floor_y_scale: f64,
    /// UDMF `xscaleceiling`.
    pub ceiling_x_scale: f64,
    /// UDMF `yscaleceiling`.
    pub ceiling_y_scale: f64,
    /// UDMF `gravity` (1.0 = normal); `< 0` = unset.
    pub gravity: f64,
    /// UDMF `friction` Boom amount (0 = unset / use default).
    pub friction: i32,
    /// UDMF `damageamount`.
    pub damage_amount: i32,
    /// UDMF `damagetype`.
    pub damage_type: String,
    /// UDMF `damageinterval` (tics; 0 = unset).
    pub damage_interval: i32,
    /// UDMF `leakiness` — 0..=256 chance radsuit fails (256ths).
    pub leakiness: i32,
    /// UDMF `silent` — suppress floor/ceiling/door mover sounds.
    pub silent: bool,
    /// UDMF `soundsequence` (empty = default).
    pub sound_sequence: String,
    /// UDMF `waterzone` — sector is underwater (surface = ceiling).
    pub waterzone: bool,
    /// UDMF `dropactors` — actors ride lowering floors (default true).
    pub drop_actors: bool,
    /// UDMF `nofallingdamage` — disable falling damage on this floor.
    pub no_falling_damage: bool,
    /// UDMF `lightcolor` RRGGBB (`-1` = unset → white).
    pub light_color: i32,
    /// UDMF `fadecolor` RRGGBB (`-1` = unset → black).
    pub fade_color: i32,
    /// UDMF `fogdensity` (0–255; 0 = none).
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
    /// UDMF `alphafloor` (1.0 = opaque).
    pub floor_alpha: f32,
    /// UDMF `alphaceiling`.
    pub ceiling_alpha: f32,
    /// UDMF `renderstyle` Add/Additive → floor additive blend.
    pub floor_additive: bool,
    /// UDMF `renderstyle` Add/Additive → ceiling additive blend.
    pub ceiling_additive: bool,
}

/// Thing / mapthing.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RawThing {
    /// Hexen TID.
    pub tid: i16,
    /// X.
    pub x: i16,
    /// Y.
    pub y: i16,
    /// Z (Hexen).
    pub z: i16,
    /// Angle degrees.
    pub angle: i16,
    /// Editor number / DoomEdNum.
    pub type_id: i16,
    /// Flags.
    pub flags: i16,
    /// Hexen special.
    pub special: u8,
    /// Hexen args.
    pub args: [i32; 5],
    /// Hexen format.
    pub hexen: bool,
    /// UDMF `friendlyseeblocks` (`-1` = unset → class default).
    pub friendly_see_blocks: i16,
    /// UDMF `health` override (`0` = unset → class default).
    pub health: i32,
    /// UDMF `score` override (`i32::MIN` = unset).
    pub score: i32,
    /// UDMF `gravity` (`-1.0` = unset → class default).
    pub gravity: f64,
    /// UDMF `scalex` / `scale` (`0.0` = unset).
    pub scale_x: f64,
    /// UDMF `scaley` / `scale` (`0.0` = unset).
    pub scale_y: f64,
    /// UDMF `alpha` (`-1.0` = unset).
    pub alpha: f64,
    /// UDMF `pitch` degrees (`i16::MIN` = unset).
    pub pitch: i16,
    /// UDMF `roll` degrees (`i16::MIN` = unset).
    pub roll: i16,
    /// UDMF `fillcolor` RRGGBB (`-1` = unset).
    pub fill_color: i32,
    /// UDMF `renderstyle` name (empty = unset).
    pub render_style: String,
    /// UDMF `floatbobphase` (`-1` = unset; valid 0..=63).
    pub float_bob_phase: i16,
    /// UDMF `countsecret` — pickup counts as a secret.
    pub count_secret: bool,
    /// UDMF `nocount` — clear COUNTKILL/COUNTITEM contribution.
    pub no_count: bool,
    /// UDMF `arg0str` — named ACS script for specials 80–86 / 226 (empty = unset).
    pub arg0_str: String,
    /// UDMF `conversation` dialogue ID (`0` = none).
    pub conversation: i32,
    /// UDMF `accuracy` (`i32::MIN` = unset).
    pub accuracy: i32,
    /// UDMF `stamina` (`i32::MIN` = unset).
    pub stamina: i32,
    /// UDMF `species` (empty = unset → class default).
    pub species: String,
    /// UDMF `meleerange` (`-1.0` = unset).
    pub melee_range: f64,
    /// UDMF `viewheight` (`-1.0` = unset).
    pub view_height: f64,
    /// UDMF `mass` (`i32::MIN` = unset).
    pub mass: i32,
    /// UDMF `speed` (`-1.0` = unset).
    pub speed: f64,
    /// UDMF `friction` (`-1.0` = unset).
    pub friction: f64,
    /// UDMF `damagefactor` (`-1.0` = unset).
    pub damage_factor: f64,
    /// UDMF `maxstepheight` (`-1.0` = unset).
    pub max_step_height: f64,
    /// UDMF `maxdropoffheight` (`-1.0` = unset).
    pub max_dropoff_height: f64,
    /// UDMF `floatspeed` (`-1.0` = unset).
    pub float_speed: f64,
    /// UDMF `fastspeed` (`-1.0` = unset).
    pub fast_speed: f64,
    /// UDMF `jumpz` (`-1.0` = unset).
    pub jump_z: f64,
    /// UDMF `attackzoffset` (`-1.0` = unset).
    pub attack_z_offset: f64,
}

impl Default for RawThing {
    fn default() -> Self {
        Self {
            tid: 0,
            x: 0,
            y: 0,
            z: 0,
            angle: 0,
            type_id: 0,
            flags: 0,
            special: 0,
            args: [0; 5],
            hexen: false,
            friendly_see_blocks: -1,
            health: 0,
            score: i32::MIN,
            gravity: -1.0,
            scale_x: 0.0,
            scale_y: 0.0,
            alpha: -1.0,
            pitch: i16::MIN,
            roll: i16::MIN,
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
        }
    }
}
