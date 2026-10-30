//! Player movement — Chocolate `processinput` (movement-only slice for D5).
//!
//! Full jump/crouch/jetpack/weapons land in later D5/D6 work. This module
//! applies `fvel`/`svel`/`avel` with vanilla friction and `clipmove_xy`.

use room_build_engine::geom::{
    SectorWalls, SectorZ, SpriteZClip, WallXY, clipmove_xy_at_z, getzrange_with_sprites, pushmove,
};
use room_build_engine::helpers::sincos_q14;
use room_build_engine::numeric::{klabs, mulscale};
use room_build_engine::types::{SectorT, SpriteT, WallT};

/// Chocolate `PHEIGHT` (`duke3d.h`) — sprite Z is `posz + PHEIGHT`.
pub const PHEIGHT: i32 = 38 << 8;

/// Chocolate `gc` default from Atomic `gamestartup` / 1.3D CON fallback.
pub const DUKE_GC: i32 = 176;

/// Chocolate `CLIPMASK0`.
pub const CLIPMASK0: u32 = (1 << 16) + 1;

/// Per-tic sync input (Chocolate `input` / `sync[]`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DukeInput {
    pub fvel: i16,
    pub svel: i16,
    /// Angular velocity (signed); applied as `avel << 1`.
    pub avel: i8,
    /// Horizon delta (`sync.horz` / DMO `horizon`).
    pub horz: i8,
    /// Chocolate `sync[].bits` (jump=1, crouch=2, …).
    pub bits: u32,
}

impl DukeInput {
    /// Map a parsed `.DMO` player record onto Room sync fields.
    #[must_use]
    pub fn from_dmo(d: &room_build::DmoInput) -> Self {
        Self {
            fvel: d.forward,
            svel: d.strafe,
            avel: d.turn,
            horz: d.horizon,
            bits: d.bits,
        }
    }
}

/// Atomic `USER.CON` `RUNNINGSPEED` → Chocolate `dukefriction` (`gamestartup`).
///
/// `global.c` defaults to `0xcc00` before CON load; retail Atomic sets `53200`.
pub const DUKE_FRICTION: i32 = 53200;

/// `TICSPERFRAME` when `TICRATE=120` and 30 Hz (`120/30 = 4`).
pub const TICS_PER_FRAME: i32 = 4;

/// ART size + picanm offsets for [`getzrange`] sprite planes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TileZMeta {
    pub width: i16,
    pub height: i16,
    pub x_offset: i8,
    pub y_offset: i8,
}

/// Build a [`SpriteZClip`] from a live sprite + ART meta table.
#[must_use]
pub fn sprite_z_clip(s: &SpriteT, tile_meta: &[TileZMeta]) -> SpriteZClip {
    let meta = tile_meta
        .get(s.picnum.max(0) as usize)
        .copied()
        .unwrap_or_default();
    SpriteZClip {
        x: s.x,
        y: s.y,
        z: s.z,
        cstat: s.cstat,
        clipdist: s.clipdist,
        xrepeat: s.xrepeat,
        yrepeat: s.yrepeat,
        xoffset: s.xoffset,
        yoffset: s.yoffset,
        ang: s.ang,
        sectnum: s.sectnum,
        tile_width: meta.width,
        tile_height: meta.height,
        picanm_xoff: meta.x_offset,
        picanm_yoff: meta.y_offset,
    }
}

/// Player pose + XY momentum (subset of `player_struct`).
#[derive(Clone, Debug)]
pub struct Player {
    pub posx: i32,
    pub posy: i32,
    pub posz: i32,
    /// Previous-tic pose for `findplayer` (Chocolate `opos*`).
    pub oposx: i32,
    pub oposy: i32,
    pub oposz: i32,
    pub ang: i16,
    pub horiz: i16,
    /// Chocolate `horizoff` (slope look).
    pub horizoff: i16,
    /// Chocolate `pyoff` (view Z bob).
    pub pyoff: i32,
    /// Chocolate `pycount` (bob phase, Build angle units).
    pub pycount: i16,
    /// Chocolate `bobpos*` — previous-tic pre-clip pose for walk `xvel`.
    pub bobposx: i32,
    pub bobposy: i32,
    pub cursectnum: i16,
    pub posxv: i32,
    pub posyv: i32,
    /// Vertical velocity (Chocolate `poszv`).
    pub poszv: i32,
    /// Chocolate `return_to_center` — recenters `horiz` toward 100.
    pub return_to_center: i16,
    /// Sprite index for the player body (`ps[].i`), or `-1`.
    pub sprite_i: i16,
    pub extra: i16,
    pub curr_weapon: i16,
    pub ammo: [i16; 12],
    /// Chocolate `gotweapon[MAX_WEAPONS]`.
    pub gotweapon: [bool; 12],
    /// Chocolate `kickback_pic` (weapon anim / fire phase).
    pub kickback_pic: i16,
    /// Chocolate `last_weapon` (−1 = none).
    pub last_weapon: i16,
    /// Chocolate `weapon_pos` (draw raise/lower).
    pub weapon_pos: i16,
    /// Chocolate `on_ground`.
    pub on_ground: bool,
    /// Chocolate `on_warping_sector` (set by `movetransports`).
    pub on_warping_sector: i16,
    /// Chocolate `transporter_hold`.
    pub transporter_hold: i16,
    /// Chocolate `jumping_counter` (0 = not jumping).
    pub jumping_counter: i16,
    /// Chocolate `jumping_toggle`.
    pub jumping_toggle: i16,
    /// Chocolate `jetpack_on` (0 = off).
    pub jetpack_on: i16,
    /// Chocolate `scuba_on` (0 = off).
    pub scuba_on: i16,
}

impl Default for Player {
    fn default() -> Self {
        Self {
            posx: 0,
            posy: 0,
            posz: 0,
            oposx: 0,
            oposy: 0,
            oposz: 0,
            ang: 0,
            horiz: 100,
            horizoff: 0,
            pyoff: 0,
            pycount: 0,
            bobposx: 0,
            bobposy: 0,
            cursectnum: 0,
            posxv: 0,
            posyv: 0,
            poszv: 0,
            return_to_center: 9,
            sprite_i: -1,
            extra: 100,
            curr_weapon: 1,
            ammo: [0, 48, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            gotweapon: [
                true, true, false, false, false, false, false, false, false, false, false, false,
            ],
            kickback_pic: 0,
            last_weapon: -1,
            weapon_pos: 0,
            on_ground: true,
            on_warping_sector: 0,
            transporter_hold: 0,
            jumping_counter: 0,
            jumping_toggle: 0,
            jetpack_on: 0,
            scuba_on: 0,
        }
    }
}

/// Movement-only `processinput`: angle, Z fall (getzrange + clamp), friction, clipmove.
pub fn processinput_move(
    p: &mut Player,
    sync: DukeInput,
    sectors: &[SectorT],
    walls: &[WallT],
    sprites: &[SpriteT],
    tile_meta: &[TileZMeta],
    dukefriction: i32,
) {
    let doubvel = TICS_PER_FRAME;

    // Chocolate `findplayer` (movefta / standables) reads `opos` after processinput.
    // Atomic snapshots `opos` mid-function *before* the main clipmove / late Z, so
    // movers still see the previous-tic pose. Snapshot at entry to match that.
    p.oposx = p.posx;
    p.oposy = p.posy;
    p.oposz = p.posz;

    // Chocolate early `s->xvel = ksqrt((pos-bobpos)²)` then refresh bobpos (pre-clip).
    // Walk bob `pyoff` later uses this previous-tic travel distance.
    let dx_bob = p.posx.wrapping_sub(p.bobposx);
    let dy_bob = p.posy.wrapping_sub(p.bobposy);
    let dist2_bob = dx_bob
        .saturating_mul(dx_bob)
        .saturating_add(dy_bob.saturating_mul(dy_bob));
    let walk_xvel = room_build_engine::numeric::ksqrt(dist2_bob) as i32;
    p.bobposx = p.posx;
    p.bobposy = p.posy;

    if sync.avel != 0 {
        // Chocolate: `tempang = avel<<1`; underwater lotag 2 → `(tempang-(tempang>>3))*ksgn(doubvel)`.
        let tempang = i32::from(sync.avel) << 1;
        let psectlotag = sectors
            .get(p.cursectnum as usize)
            .map(|s| s.lotag)
            .unwrap_or(0);
        let ksgn_dv = if doubvel > 0 {
            1
        } else if doubvel < 0 {
            -1
        } else {
            0
        };
        let angvel = if psectlotag == 2 {
            (tempang - (tempang >> 3)) * ksgn_dv
        } else {
            tempang * ksgn_dv
        };
        p.ang = ((i32::from(p.ang) + angvel) & 2047) as i16;
    }
    if sync.horz != 0 {
        p.horiz = (i32::from(p.horiz) + i32::from(sync.horz) * 2).clamp(-99, 299) as i16;
    }
    // Chocolate `return_to_center`: pull horiz toward ~100 each tic.
    if p.return_to_center > 0 {
        p.return_to_center -= 1;
        p.horiz = (i32::from(p.horiz) + 33 - (i32::from(p.horiz) / 3)).clamp(-99, 299) as i16;
    }
    // Chocolate non-`aim_mode`: snap near-center horiz to 100.
    if p.horiz > 95 && p.horiz < 105 {
        p.horiz = 100;
    }

    // Chocolate: getzrange + fall/ceiling clamp happen *before* XY clipmove.
    apply_player_fall_z(p, sync, sectors, walls, sprites, tile_meta);

    if p.posxv != 0 || p.posyv != 0 || sync.fvel != 0 || sync.svel != 0 {
        p.posxv += (i32::from(sync.fvel) * doubvel) << 6;
        p.posyv += (i32::from(sync.svel) * doubvel) << 6;

        let psectlotag = sectors
            .get(p.cursectnum as usize)
            .map(|s| s.lotag)
            .unwrap_or(0);
        // Chocolate: crouch bit2 → friction-0x2000; underwater → -0x1400; else plain.
        // (Knee+kickback>10 path omitted until kb is tracked.)
        let fric = if p.on_ground && (sync.bits & 2) != 0 {
            dukefriction - 0x2000
        } else if psectlotag == 2 {
            dukefriction - 0x1400
        } else {
            dukefriction
        };
        p.posxv = mulscale(p.posxv, fric, 16);
        p.posyv = mulscale(p.posyv, fric, 16);

        if p.posxv.abs() < 2048 && p.posyv.abs() < 2048 {
            p.posxv = 0;
            p.posyv = 0;
        }
    }

    let sec: Vec<SectorWalls> = sectors
        .iter()
        .map(|s| SectorWalls {
            wallptr: s.wallptr,
            wallnum: s.wallnum,
        })
        .collect();
    let sec_z: Vec<SectorZ> = sectors
        .iter()
        .map(|s| SectorZ {
            wallptr: s.wallptr,
            wallnum: s.wallnum,
            ceilingz: s.ceilingz,
            floorz: s.floorz,
            ceilingstat: s.ceilingstat,
            floorstat: s.floorstat,
            ceilingheinum: s.ceilingheinum,
            floorheinum: s.floorheinum,
        })
        .collect();
    let wxy: Vec<WallXY> = walls.iter().map(WallT::xy).collect();

    if p.posxv != 0 || p.posyv != 0 {
        // Chocolate `clipmove` goals are `pos + (posxv>>14)` — Room takes world
        // deltas. `flordist`/`ceildist` match player.c HORIZONLY (lotag 1 → 4<<8).
        let psectlotag = sectors
            .get(p.cursectnum as usize)
            .map(|s| s.lotag)
            .unwrap_or(0);
        let flordist = if psectlotag == 1 { 4 << 8 } else { 20 << 8 };
        let _hit = clipmove_xy_at_z(
            &mut p.posx,
            &mut p.posy,
            p.posz,
            &mut p.cursectnum,
            p.posxv >> 14,
            p.posyv >> 14,
            164,
            4 << 8,
            flordist,
            &sec,
            &wxy,
            Some(&sec_z),
        );
    }

    // Chocolate: after setsprite, always `pushmove(..., 164, 4<<8, 4<<8, CLIPMASK0)`.
    let _ = pushmove(
        &mut p.posx,
        &mut p.posy,
        p.posz,
        &mut p.cursectnum,
        164,
        4 << 8,
        4 << 8,
        CLIPMASK0,
        &sec,
        &sec_z,
        &wxy,
    );

    // Chocolate walk view-bob (`pyoff`) — feeds shotgun `sz = posz+pyoff+(4<<8)`.
    let psectlotag = sectors
        .get(p.cursectnum as usize)
        .map(|s| s.lotag)
        .unwrap_or(0);
    if p.jetpack_on == 0 {
        if walk_xvel > 16 {
            if psectlotag != 1 && psectlotag != 2 && p.on_ground {
                p.pycount = ((i32::from(p.pycount) + 52) & 2047) as i16;
                let (sin, _) = sincos_q14(i32::from(p.pycount));
                p.pyoff = klabs(walk_xvel.wrapping_mul(sin)) / 1596;
            }
        } else if psectlotag != 2 && psectlotag != 1 {
            p.pyoff = 0;
        }
    }
}

/// Chocolate fall + water/ground smooth + ceiling rest (`player.c` subset).
pub fn apply_player_fall_z(
    p: &mut Player,
    sync: DukeInput,
    sectors: &[SectorT],
    walls: &[WallT],
    sprites: &[SpriteT],
    tile_meta: &[TileZMeta],
) {
    if p.cursectnum < 0 || p.cursectnum as usize >= sectors.len() {
        return;
    }

    let secs: Vec<SectorZ> = sectors
        .iter()
        .map(|s| SectorZ {
            wallptr: s.wallptr,
            wallnum: s.wallnum,
            ceilingz: s.ceilingz,
            floorz: s.floorz,
            ceilingstat: s.ceilingstat,
            floorstat: s.floorstat,
            ceilingheinum: s.ceilingheinum,
            floorheinum: s.floorheinum,
        })
        .collect();
    let wxy: Vec<WallXY> = walls.iter().map(WallT::xy).collect();
    let zclips: Vec<SpriteZClip> = sprites
        .iter()
        .map(|s| sprite_z_clip(s, tile_meta))
        .collect();

    let zr = getzrange_with_sprites(
        p.posx,
        p.posy,
        p.posz,
        p.cursectnum,
        163,
        CLIPMASK0,
        &secs,
        &wxy,
        &zclips,
    );
    let mut cz = zr.ceilz;
    let fz = zr.florz;
    let mut psectlotag = sectors[p.cursectnum as usize].lotag;
    let truefz = sectors[p.cursectnum as usize].floorz;
    let truefdist = (p.posz - truefz).abs();
    // Chocolate: deep water floor from getzrange → treat as dry for fall `i`.
    if (zr.florhit & 49152) == 16384 && psectlotag == 1 && truefdist > PHEIGHT + (16 << 8) {
        psectlotag = 0;
    }
    // Sprite ceiling that is a living badguy → use true sector ceiling.
    if zr.ceilhit >= 0 && (zr.ceilhit & 49152) == 49152 {
        let si = (zr.ceilhit & (room_build_engine::world::MAXSPRITES as i32 - 1)) as usize;
        if let Some(s) = sprites.get(si) {
            if s.statnum == 1 && s.extra >= 0 {
                cz = sectors[p.cursectnum as usize].ceilingz;
            }
        }
    }

    // Chocolate underwater (sector lotag 2) — swim / damp, no gravity.
    if psectlotag == 2 {
        p.jumping_counter = 0;
        p.pycount = ((i32::from(p.pycount) + 32) & 2047) as i16;
        let (sin, _) = sincos_q14(i32::from(p.pycount));
        p.pyoff = sin >> 7;
        if sync.bits & 1 != 0 {
            if p.poszv > 0 {
                p.poszv = 0;
            }
            p.poszv -= 348;
            if p.poszv < -(256 * 6) {
                p.poszv = -(256 * 6);
            }
        } else if sync.bits & 2 != 0 {
            if p.poszv < 0 {
                p.poszv = 0;
            }
            p.poszv += 348;
            if p.poszv > 256 * 6 {
                p.poszv = 256 * 6;
            }
        } else {
            if p.poszv < 0 {
                p.poszv += 256;
                if p.poszv > 0 {
                    p.poszv = 0;
                }
            }
            if p.poszv > 0 {
                p.poszv -= 256;
                if p.poszv < 0 {
                    p.poszv = 0;
                }
            }
        }
        if p.poszv > 2048 {
            p.poszv >>= 1;
        }
        p.posz += p.poszv;
        if p.posz > fz - (15 << 8) {
            p.posz += ((fz - (15 << 8)) - p.posz) >> 1;
        }
        if p.posz < cz + (4 << 8) {
            p.posz = cz + (4 << 8);
            p.poszv = 0;
        }
        return;
    }

    // Non-jetpack path; `i` = 40 dry / 34 surface water.
    let i = if psectlotag == 1 { 34 } else { 40 };
    let contact = fz - (i << 8);

    if p.posz < contact {
        // Falling.
        p.on_ground = false;
        p.poszv += DUKE_GC + 80;
        if p.poszv >= 4096 + 2048 {
            p.poszv = 4096 + 2048;
        }
    } else {
        // On ground / water surface.
        p.on_ground = true;
        if i == 40 {
            let mut k = (contact - p.posz) >> 1;
            if k.abs() < 256 {
                k = 0;
            }
            p.posz += k;
            p.poszv -= 768;
            if p.poszv < 0 {
                p.poszv = 0;
            }
        } else if p.jumping_counter == 0 {
            // Chocolate water smooth — keep poszv so a dive can still fire.
            p.posz += ((fz - (i << 7)) - p.posz) >> 1;
            if p.on_warping_sector == 0 && p.posz > fz - (16 << 8) {
                p.posz = fz - (16 << 8);
                p.poszv >>= 1;
            }
        }
        p.on_warping_sector = 0;
        // Crouch pushes through the water surface (dive trigger).
        if sync.bits & 2 != 0 {
            p.posz += 2048 + 768;
        }
    }

    p.posz += p.poszv;

    if p.posz < cz + (4 << 8) {
        p.poszv = 128;
        p.posz = cz + (4 << 8);
    }
}

/// Chocolate `setsprite(ps.i, posx, posy, posz+PHEIGHT)` + bob `xvel`.
///
/// `xvel` is Chocolate `ksqrt((pos-bobpos)²)` used by CON `ifp pwalking` etc.
/// Room's `opos` is the pre-move pose (same role as `bobpos` at this point).
pub fn sync_player_sprite(p: &Player, sprites: &mut [SpriteT]) {
    if p.sprite_i < 0 {
        return;
    }
    let i = p.sprite_i as usize;
    if i >= sprites.len() {
        return;
    }
    let dx = p.posx.wrapping_sub(p.oposx);
    let dy = p.posy.wrapping_sub(p.oposy);
    // ksqrt arg is i32; distance squared can overflow for huge deltas — clamp.
    let dist2 = dx.saturating_mul(dx).saturating_add(dy.saturating_mul(dy));
    let xvel = room_build_engine::numeric::ksqrt(dist2) as i16;
    let s = &mut sprites[i];
    // Chocolate `setsprite(pi, pos…)` updates x/y/z/sect only — not `ang`.
    // `moveplayers` assigns `s->ang = p->ang` *after* APLAYER `execute` so
    // mid-tic `spawn(WATERBUBBLE)` copies the previous-tic sprite ang.
    s.x = p.posx;
    s.y = p.posy;
    s.z = p.posz + PHEIGHT;
    s.sectnum = p.cursectnum;
    s.xvel = xvel;
    // Chocolate `ifp palive` reads `sprite[ps[g_p].i].extra`.
    s.extra = p.extra;
}

#[cfg(test)]
mod tests {
    use super::*;
    use room_build_engine::types::{SectorT, WallT};

    fn unit_square() -> (Vec<SectorT>, Vec<WallT>) {
        let walls = vec![
            WallT {
                x: 0,
                y: 0,
                point2: 1,
                nextsector: -1,
                ..Default::default()
            },
            WallT {
                x: 1024,
                y: 0,
                point2: 2,
                nextsector: -1,
                ..Default::default()
            },
            WallT {
                x: 1024,
                y: 1024,
                point2: 3,
                nextsector: -1,
                ..Default::default()
            },
            WallT {
                x: 0,
                y: 1024,
                point2: 0,
                nextsector: -1,
                ..Default::default()
            },
        ];
        let sectors = vec![SectorT {
            wallptr: 0,
            wallnum: 4,
            floorz: 0,
            ceilingz: -(64 << 8),
            ..Default::default()
        }];
        (sectors, walls)
    }

    #[test]
    fn forward_input_moves_pos() {
        let (sectors, walls) = unit_square();
        let mut p = Player {
            posx: 512,
            posy: 512,
            posz: -(40 << 8),
            cursectnum: 0,
            ..Default::default()
        };
        processinput_move(
            &mut p,
            DukeInput {
                fvel: 100,
                svel: 0,
                avel: 0,
                horz: 0,
                bits: 0,
            },
            &sectors,
            &walls,
            &[],
            &[],
            DUKE_FRICTION,
        );
        assert!(p.posx != 512 || p.posy != 512);
    }

    #[test]
    fn avel_turns_ang() {
        let (sectors, walls) = unit_square();
        let mut p = Player {
            posx: 512,
            posy: 512,
            posz: -(40 << 8),
            ang: 0,
            cursectnum: 0,
            ..Default::default()
        };
        processinput_move(
            &mut p,
            DukeInput {
                fvel: 0,
                svel: 0,
                avel: 10,
                horz: 0,
                bits: 0,
            },
            &sectors,
            &walls,
            &[],
            &[],
            DUKE_FRICTION,
        );
        assert_eq!(p.ang, 20); // avel<<1
    }

    #[test]
    fn ceiling_clamp_sets_poszv_128() {
        // Sector ceiling high; face sprite provides cz = posz - 144.
        let (mut sectors, walls) = unit_square();
        sectors[0].ceilingz = -38912;
        sectors[0].floorz = 49152;
        let mut p = Player {
            posx: 512,
            posy: 512,
            posz: -24896,
            poszv: 0,
            cursectnum: 0,
            ..Default::default()
        };
        let sprites = [SpriteT {
            x: 512,
            y: 512,
            z: -24896,
            cstat: 1,
            picnum: 1405,
            clipdist: 64,
            xrepeat: 42,
            yrepeat: 36,
            sectnum: 0,
            ..Default::default()
        }];
        let mut meta = vec![TileZMeta::default(); 1406];
        meta[1405] = TileZMeta {
            width: 40,
            height: 82,
            x_offset: -1,
            y_offset: 1,
        };
        apply_player_fall_z(
            &mut p,
            DukeInput::default(),
            &sectors,
            &walls,
            &sprites,
            &meta,
        );
        assert_eq!(p.posz, -24016);
        assert_eq!(p.poszv, 128);
    }
}
