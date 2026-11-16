//! Chocolate `animatesprites` — per-frame display picnum / mirror selection.
//!
//! Duke keeps an actor's `sprite.picnum` at its base tile; the drawn tile is
//! picked each frame from the CON action (start frame, view count, frame
//! index) and the viewing angle. This module applies that to a render copy of
//! the world so the simulation state is never touched.

use room_build_engine::helpers::getangle;
use room_build_engine::world::{BuildWorld, MAXSTATUS};

use crate::con::{ActorConTemps, ScriptImage};
use crate::player::TileZMeta;

const CHAIR3: i16 = 680;
const SCRAP6: i16 = 2390;
const SCRAP1: i16 = 2400;
const SCRAP5: i16 = 2416;
const APLAYER: i16 = 1405;
const BLIMP: i16 = 3400;
const CSTAT_XFLIP: u16 = 4;

/// Apply Chocolate `animatesprites` display rules to `view` (a render copy).
///
/// `cam_x`/`cam_y`/`cam_ang` are the camera position and heading.
pub fn animatesprites(
    view: &mut BuildWorld,
    temps: &[ActorConTemps],
    script: Option<&ScriptImage>,
    tile_meta: &[TileZMeta],
    cam_x: i32,
    cam_y: i32,
    cam_ang: i16,
) {
    let a = i32::from(cam_ang);
    let tile_width = |pic: i16| {
        usize::try_from(pic)
            .ok()
            .and_then(|p| tile_meta.get(p))
            .map_or(0, |t| t.width)
    };
    let count = view.sprites.len().min(view.lists.sprite_stat.len());
    for i in 0..count {
        if view.lists.sprite_stat[i] == MAXSTATUS as i16 {
            continue;
        }
        let s = view.sprites[i];
        if s.sectnum < 0 || s.cstat & 32768 != 0 {
            continue;
        }
        let t = temps.get(i);
        let t0 = t.map_or(0, |t| t.g_t[0]);
        let t3 = t.map_or(0, |t| t.g_t[3]);
        let t4 = t.map_or(0, |t| t.g_t[4]);
        let sprite = &mut view.sprites[i];
        match s.picnum {
            CHAIR3 => {
                let mut k = (((i32::from(s.ang) + 3072 + 128 - a) & 2047) >> 8) & 7;
                if k > 4 {
                    k = 8 - k;
                    sprite.cstat |= CSTAT_XFLIP;
                } else {
                    sprite.cstat &= !CSTAT_XFLIP;
                }
                sprite.picnum = s.picnum + k as i16;
                continue;
            }
            p if (SCRAP6..=SCRAP6 + 7).contains(&p)
                || [SCRAP1, 2404, 2408, 2412, SCRAP5].contains(&p) =>
            {
                let hit_pic = t.map_or(-1, |t| t.hit_picnum);
                if !(hit_pic == BLIMP && p == SCRAP1 && s.yvel >= 0) {
                    sprite.picnum = p.wrapping_add(t0 as i16);
                }
                sprite.shade = sprite.shade.saturating_sub(6);
                continue;
            }
            APLAYER => continue,
            _ => {}
        }
        let Some(script) = script else { continue };
        let pic = s.picnum as usize;
        if script.actorscrptr.get(pic).copied().unwrap_or(0) <= 0 {
            continue;
        }
        let Ok(ptr) = usize::try_from(t4) else {
            continue;
        };
        if ptr == 0 || ptr + 2 >= script.script.len() {
            continue;
        }
        let start = script.script[ptr];
        let views = script.script[ptr + 2];
        let mut cstat = sprite.cstat;
        let k = match views {
            2 => (((i32::from(s.ang) + 3072 + 128 - a) & 2047) >> 8) & 1,
            3 | 4 => {
                let mut k = (((i32::from(s.ang) + 3072 + 128 - a) & 2047) >> 7) & 7;
                if k > 3 {
                    cstat |= CSTAT_XFLIP;
                    k = 7 - k;
                } else {
                    cstat &= !CSTAT_XFLIP;
                }
                k
            }
            5 => {
                let g = getangle(s.x - cam_x, s.y - cam_y);
                let mut k = (((i32::from(s.ang) + 3072 + 128 - g) & 2047) >> 8) & 7;
                if k > 4 {
                    k = 8 - k;
                    cstat |= CSTAT_XFLIP;
                } else {
                    cstat &= !CSTAT_XFLIP;
                }
                k
            }
            7 => {
                let g = getangle(s.x - cam_x, s.y - cam_y);
                let mut k = ((i32::from(s.ang) + 3072 + 128 - g) & 2047) / 170;
                if k > 6 {
                    k = 12 - k;
                    cstat |= CSTAT_XFLIP;
                } else {
                    cstat &= !CSTAT_XFLIP;
                }
                k
            }
            8 => {
                cstat &= !CSTAT_XFLIP;
                (((i32::from(s.ang) + 3072 + 128 - a) & 2047) >> 8) & 7
            }
            _ => 0,
        };
        let mut drawn = i32::from(s.picnum) + k + start + views * t3;
        if views > 0 {
            // Chocolate's "hack, for actors": step back to a frame with art.
            while drawn > 0 && tile_width(drawn as i16) == 0 {
                drawn -= views;
            }
        }
        if drawn > 0 && drawn < i32::from(i16::MAX) {
            sprite.picnum = drawn as i16;
            sprite.cstat = cstat;
        }
    }
}
