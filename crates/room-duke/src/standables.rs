//! STAT_STANDABLE movers — Chocolate `movestandables` (D5 subset).
//!
//! `BOLT1`…`BOLT1+3` spark animation; `WATERBUBBLEMAKER` CON subset.

use room_build_engine::numeric::krand_step;
use room_build_engine::types::SpriteT;
use room_build_engine::world::SpriteLists;

use crate::con::ActorConTemps;
use crate::player::Player;
use crate::spawn::{BOLT1, WATERBUBBLE, WATERBUBBLEMAKER};
use crate::stat::{STAT_MISC, STAT_STANDABLE};

fn klabs(x: i32) -> i32 {
    if x == i32::MIN {
        i32::MIN
    } else {
        x.abs()
    }
}

/// Chocolate `findplayer` distance (single-player): manhattan on `opos` + Z/16.
fn bolt_player_dist(s: &SpriteT, p: &Player) -> i32 {
    klabs(p.oposx - s.x) + klabs(p.oposy - s.y) + (klabs(p.oposz - s.z + (28 << 8)) >> 4)
}

/// Chocolate `rnd(X)`.
fn rnd(randomseed: &mut i32, x: i32) -> bool {
    let t = krand_step(randomseed);
    (t >> 8) >= (255 - x)
}

/// GAME.CON `spawn WATERBUBBLE` from a non-APLAYER owner (no Z lift).
pub(crate) fn spawn_maker_bubble(
    lists: &mut SpriteLists,
    sprites: &mut [SpriteT],
    picnum: &mut Vec<i16>,
    actor_temps: &mut Vec<ActorConTemps>,
    from: i16,
    script: Option<&crate::con::ScriptImage>,
) -> i16 {
    let fu = from as usize;
    if fu >= sprites.len() {
        return -1;
    }
    let sect = sprites[fu].sectnum;
    let ang = sprites[fu].ang;
    let (x, y, z) = (sprites[fu].x, sprites[fu].y, sprites[fu].z);
    let i = lists.insertsprite(sect, STAT_MISC);
    if i < 0 {
        return -1;
    }
    let iu = i as usize;
    if iu >= sprites.len() {
        let _ = lists.deletesprite(i);
        return -1;
    }
    sprites[iu] = SpriteT {
        x,
        y,
        z,
        ang,
        sectnum: sect,
        picnum: WATERBUBBLE,
        xrepeat: 4,
        yrepeat: 4,
        owner: from,
        ..Default::default()
    };
    if picnum.len() <= iu {
        picnum.resize(iu + 1, 0);
    }
    picnum[iu] = WATERBUBBLE;
    if actor_temps.len() <= iu {
        actor_temps.resize_with(iu + 1, ActorConTemps::default);
    }
    if let Some(img) = script {
        let (extra, temps) = crate::con::init_actor_temps_from_script(img, WATERBUBBLE);
        sprites[iu].extra = extra;
        sprites[iu].hitag = crate::con::actor_header_hitag(img, WATERBUBBLE) as i16;
        actor_temps[iu] = temps;
    } else {
        actor_temps[iu] = ActorConTemps::default();
    }
    i
}

/// Chocolate `movestandables` — BOLT1 sparks + WATERBUBBLEMAKER.
pub fn movestandables(
    lists: &mut SpriteLists,
    sprites: &mut [SpriteT],
    picnum: &mut Vec<i16>,
    players: &[Player],
    randomseed: &mut i32,
    global_random: i32,
    actor_temps: &mut Vec<ActorConTemps>,
    script: Option<&crate::con::ScriptImage>,
) {
    let Some(p) = players.first() else {
        return;
    };
    let mut i = lists.headspritestat[STAT_STANDABLE as usize];
    while i >= 0 {
        let next = lists.nextspritestat[i as usize];
        let iu = i as usize;
        if iu >= sprites.len() {
            i = next;
            continue;
        }
        if sprites[iu].sectnum < 0 {
            let _ = lists.deletesprite(i);
            i = next;
            continue;
        }
        let pic = sprites[iu].picnum;
        if pic == WATERBUBBLEMAKER {
            // GAME.CON: ifpdistl 3084 / ifrnd 24 / spawn WATERBUBBLE; then move count++.
            if actor_temps.len() <= iu {
                actor_temps.resize_with(iu + 1, ActorConTemps::default);
            }
            if bolt_player_dist(&sprites[iu], p) < 3084 && rnd(randomseed, 24) {
                let _ = spawn_maker_bubble(lists, sprites, picnum, actor_temps, i, script);
            }
            actor_temps[iu].g_t[0] += 1;
            i = next;
            continue;
        }
        if !(BOLT1..=BOLT1 + 3).contains(&pic) {
            i = next;
            continue;
        }
        if actor_temps.len() <= iu {
            actor_temps.resize_with(iu + 1, ActorConTemps::default);
        }
        if bolt_player_dist(&sprites[iu], p) > 20480 {
            i = next;
            continue;
        }
        // Chocolate `CLEAR_THE_BOLT` loop body (one pass; re-entry via hide path).
        loop {
            let t2 = actor_temps[iu].g_t[2];
            if t2 != 0 {
                actor_temps[iu].g_t[2] = t2 - 1;
                break;
            }
            let s = &mut sprites[iu];
            if (s.xrepeat | s.yrepeat) == 0 {
                s.xrepeat = actor_temps[iu].g_t[0] as u8;
                s.yrepeat = actor_temps[iu].g_t[1] as u8;
            } else if (krand_step(randomseed) & 8) == 0 {
                actor_temps[iu].g_t[0] = i32::from(s.xrepeat);
                actor_temps[iu].g_t[1] = i32::from(s.yrepeat);
                actor_temps[iu].g_t[2] = global_random & 4;
                s.xrepeat = 0;
                s.yrepeat = 0;
                continue; // goto CLEAR_THE_BOLT
            }
            s.picnum = s.picnum.wrapping_add(1);
            let l = global_random & 7;
            s.xrepeat = (l + 8) as u8;
            if (l & 1) != 0 {
                s.cstat ^= 2;
            }
            // Chocolate: short-circuit still consumes TRAND when pic == BOLT1+1.
            if s.picnum == BOLT1 + 1 {
                let _ = krand_step(randomseed); // TRAND&7 sound gate (HURTRAIL)
            }
            if s.picnum == BOLT1 + 4 {
                s.picnum = BOLT1;
            }
            break;
        }
        i = next;
    }
}
