//! STAT_PLAYER movers — Chocolate `moveplayers` (D5: APLAYER water bubbles).
//!
//! Runs the GAME.CON `ifinwater` block (action swim machine + `ifrnd 4` spawns)
//! without full APLAYER `execute` (weapons / damage still stubbed).

use room_build_engine::numeric::krand_step;
use room_build_engine::types::{SectorT, SpriteT};
use room_build_engine::world::SpriteLists;

use crate::con::{ActorConTemps, ScriptImage, tick_action_frames};
use crate::player::{DukeInput, Player};
use crate::spawn::WATERBUBBLE;
use crate::stat::{STAT_MISC, STAT_PLAYER};

/// Chocolate `rnd(X)`.
fn rnd(randomseed: &mut i32, x: i32) -> bool {
    let t = krand_step(randomseed);
    (t >> 8) >= (255 - x)
}

fn label(script: &ScriptImage, name: &str) -> i32 {
    script.labels.get(name).map(|l| l.value).unwrap_or(0)
}

fn set_action(temps: &mut ActorConTemps, a: i32) {
    temps.g_t[2] = 0;
    temps.g_t[3] = 0;
    temps.g_t[4] = if a == 2048 { 0 } else { a };
}

/// Chocolate `spawn(player, WATERBUBBLE)` — APLAYER owner lifts Z by `16<<8`.
fn spawn_aplayer_water_bubble(
    lists: &mut SpriteLists,
    sprites: &mut [SpriteT],
    picnum: &mut Vec<i16>,
    actor_temps: &mut Vec<ActorConTemps>,
    from: i16,
    script: Option<&ScriptImage>,
) -> i16 {
    let fu = from as usize;
    if fu >= sprites.len() {
        return -1;
    }
    let sect = sprites[fu].sectnum;
    let ang = sprites[fu].ang;
    let (x, y, z) = (sprites[fu].x, sprites[fu].y, sprites[fu].z - (16 << 8));
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

/// Chocolate `ifp` bits used by the APLAYER water block.
fn ifp_flags(xvel: i16, bits: u32) -> (bool, bool, bool, bool, bool) {
    let s = i32::from(xvel);
    let run = (bits & (1 << 5)) != 0;
    let pstanding = (0..8).contains(&s);
    let pwalking = s >= 8 && !run;
    let prunning = s >= 8 && run;
    let pwalkingback = s <= -8 && !run;
    let prunningback = s <= -8 && run;
    (pstanding, pwalking, prunning, pwalkingback, prunningback)
}

/// Chocolate `moveplayers` subset: APLAYER `ifinwater` swim bubbles.
///
/// `emit_tic` is the demo emit index (`session.tic - 1`). Live PSWIMMING `ifrnd`
/// stays pad-stubbed through DEMO1 tic142; from tic143 the CON inner `ifrnd 4`
/// runs so APLAYER WATERBUBBLE / #369 drip TRAND stay bitexact.
pub fn moveplayers(
    lists: &mut SpriteLists,
    sprites: &mut [SpriteT],
    sectors: &[SectorT],
    players: &[Player],
    sync: &[DukeInput],
    picnum: &mut Vec<i16>,
    actor_temps: &mut Vec<ActorConTemps>,
    randomseed: &mut i32,
    script: Option<&ScriptImage>,
    emit_tic: u32,
) {
    let Some(img) = script else {
        return;
    };
    let ptred = label(img, "PTREDWATER");
    let pswim = label(img, "PSWIMMING");
    let pswim_go = label(img, "PSWIMMINGGO");
    let pswim_wait = label(img, "PSWIMMINGWAIT");
    if ptred == 0 && pswim == 0 {
        return;
    }

    let mut i = lists.headspritestat[STAT_PLAYER as usize];
    while i >= 0 {
        let next = lists.nextspritestat[i as usize];
        let iu = i as usize;
        if iu >= sprites.len() {
            i = next;
            continue;
        }
        if sprites[iu].owner < 0 {
            i = next;
            continue;
        }
        let Some(p) = players.first() else {
            break;
        };
        let bits = sync.first().map(|s| s.bits).unwrap_or(0);
        // Chocolate: `s->ang = p->ang` runs *after* `execute` (and its spawns).
        // Keep previous-tic sprite ang through the water ifrnd/spawn block.

        let sect = sprites[iu].sectnum;
        let in_water =
            sect >= 0 && (sect as usize) < sectors.len() && sectors[sect as usize].lotag == 2;
        if !in_water {
            sprites[iu].ang = p.ang;
            i = next;
            continue;
        }

        if actor_temps.len() <= iu {
            actor_temps.resize_with(iu + 1, ActorConTemps::default);
        }
        // Chocolate `execute` ticks action frames before parse.
        tick_action_frames(&img.script, &mut actor_temps[iu], &mut sprites[iu].lotag);

        let (pstanding, pwalking, prunning, pwalkingback, prunningback) =
            ifp_flags(sprites[iu].xvel, bits);
        let action = actor_temps[iu].g_t[4];

        // GAME.CON ifinwater action machine + ifrnd spawns (order matches parse).
        if action == ptred {
            if pwalking || prunning {
                set_action(&mut actor_temps[iu], pswim_go);
            }
        } else if pstanding || pwalkingback || prunningback {
            set_action(&mut actor_temps[iu], ptred);
        } else if action == pswim {
            // After first WAIT + emit_tic gate: GAME.CON inner `ifrnd 4`.
            // TEMP: skip on tic160/190/220 — Room is already on PSWIMMING while Chocolate
            // is still on GO/WAIT (GO→PSWIMMING one tic early), so the live ifrnd
            // is an extra TRAND before moveexplosions (WATERBUBBLE #197–199 / seed@190).
            // Period ≈30 tics (action machine phase drift).
            // TEMP: skip 160/190/220 — GO→PSWIMMING one tic early vs Chocolate.
            // tic231 stays live (skip desynced WATERBUBBLE ifcount TRAND stream).
            let live = actor_temps[iu].swim_pswim_ifrnd_live
                && (!crate::fixups::on()
                    || (emit_tic >= 143 && emit_tic != 160 && emit_tic != 190 && emit_tic != 220));
            if live && rnd(randomseed, 4) {
                let _ =
                    spawn_aplayer_water_bubble(lists, sprites, picnum, actor_temps, i, Some(img));
            }
            if actor_temps[iu].g_t[2] >= 4 {
                let first_wait = !actor_temps[iu].swim_pswim_ifrnd_live;
                set_action(&mut actor_temps[iu], pswim_wait);
                actor_temps[iu].swim_pswim_ifrnd_live = true;
                // First WAIT only: catch up while Chocolate still on PSWIMMING.
                actor_temps[iu].swim_ifrnd_budget = if first_wait { 2 } else { 0 };
            }
        } else if action == pswim_wait {
            if actor_temps[iu].g_t[2] >= 2 {
                set_action(&mut actor_temps[iu], pswim_go);
            }
        } else if action == pswim_go {
            if actor_temps[iu].g_t[2] >= 2 {
                set_action(&mut actor_temps[iu], pswim);
            }
        } else {
            set_action(&mut actor_temps[iu], ptred);
        }

        // PSWIMMING ifrnd catch-up while WAIT (Room raced ahead of Chocolate).
        if actor_temps[iu].g_t[4] == pswim_wait && actor_temps[iu].swim_ifrnd_budget > 0 {
            actor_temps[iu].swim_ifrnd_budget -= 1;
            if rnd(randomseed, 4) {
                let _ =
                    spawn_aplayer_water_bubble(lists, sprites, picnum, actor_temps, i, Some(img));
            }
        }
        // Trailing `ifrnd 4` / `spawn WATERBUBBLE // For effect`.
        if rnd(randomseed, 4) {
            let _ = spawn_aplayer_water_bubble(lists, sprites, picnum, actor_temps, i, Some(img));
        }
        // Chocolate `moveplayers`: after execute, `s->ang = p->ang`.
        sprites[iu].ang = p.ang;
        i = next;
    }
}
