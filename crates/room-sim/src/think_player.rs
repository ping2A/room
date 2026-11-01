//! `P_PlayerThink` / `MovePlayer` subset.

use crate::actor::ActorId;
use crate::class::RenderStyle;
use crate::hitscan::line_attack;
use crate::physics::try_move;
use crate::player::{Buttons, Player, PlayerState, TicCmd};
use crate::use_lines::use_lines;
use crate::world::World;
use room_core::dmath;
use room_core::{Angle, DVec3, StateId};

/// Doom forward/side scale (`forwardmove[0]` ≈ 25 × 2048 / 65536 in map units).
const MOVE_SCALE: f64 = 1.0 / 2048.0;
/// Turn rate: yaw units → degrees (`angleturn` style).
const YAW_TO_DEG: f64 = 360.0 / 65536.0;

/// Duke 30 Hz `falling_counter >= 62` → Room 35 Hz (~72 tics, about 2 s).
const DUKE_FALL_HURT: u32 = 72;

fn is_duke_player(world: &World, player: &Player) -> bool {
    world
        .classes
        .get(player.cls)
        .is_some_and(|c| c.name == "DukePlayer")
}

/// Long fall: half health when `DukePlayer` lands after ~2 s of falling.
fn apply_duke_fall(world: &mut World, player: &mut Player, mo: ActorId) {
    if !is_duke_player(world, player) {
        return;
    }
    if player.fly_tics > 0 {
        player.falling_tics = 0;
        return;
    }
    let Some(a) = world.actors.get(mo) else {
        return;
    };
    if crate::physics::actor_waterlevel(world, a) >= 2
        || (a.flags & crate::class::FLAG_NOGRAVITY) != 0
    {
        player.falling_tics = 0;
        return;
    }
    if player.on_ground {
        if player.falling_tics >= DUKE_FALL_HURT {
            let dmg = (a.health / 2).max(1);
            crate::damage::damage_mobj(world, mo, None, None, dmg);
        }
        player.falling_tics = 0;
        return;
    }
    if a.vel.z() < 0.0 {
        player.falling_tics = player.falling_tics.saturating_add(1);
    }
}

/// Duke crouch: hold move-down on the ground (not flying / swimming).
fn apply_duke_crouch(world: &mut World, player: &mut Player, mo: ActorId, cmd: &TicCmd) {
    if !is_duke_player(world, player) {
        return;
    }
    let swimming = world
        .actors
        .get(mo)
        .is_some_and(|a| crate::physics::actor_waterlevel(world, a) >= 2);
    if player.fly_tics > 0 || swimming || player.frozen {
        if player.crouching {
            set_duke_stance(world, player, mo, false);
        }
        return;
    }
    let (stand_h, _) = duke_stance_sizes(world, player);
    let headroom = world.actors.get(mo).map(|a| {
        let planes = crate::physics::check_position(world, a, a.pos, Some(mo));
        planes.ceiling_z - planes.floor_z
    });
    let cramped = headroom.is_some_and(|gap| gap < stand_h + 1.0);
    let want = player.on_ground && cmd.up_move < 0;
    if want || cramped {
        set_duke_stance(world, player, mo, true);
    } else if player.crouching {
        set_duke_stance(world, player, mo, false);
    }
}

fn duke_stance_sizes(world: &World, player: &Player) -> (f64, f64) {
    world
        .classes
        .get(player.cls)
        .map(|c| {
            let h = if c.height > 0.0 { c.height } else { 40.0 };
            let v = if c.view_height > 0.0 {
                c.view_height
            } else {
                36.0
            };
            (h, v)
        })
        .unwrap_or((40.0, 36.0))
}

fn set_duke_stance(world: &mut World, player: &mut Player, mo: ActorId, crouch: bool) {
    let (stand_h, stand_v) = duke_stance_sizes(world, player);
    let (h, v) = if crouch {
        ((stand_h * 0.4).max(12.0), (stand_v * 0.4).max(8.0))
    } else {
        (stand_h, stand_v)
    };
    if let Some(a) = world.actors.get_mut(mo) {
        a.height = h;
    }
    player.view_height = v;
    player.crouching = crouch;
}

/// Tick timed powerups toward expiry.
fn tick_powerups(world: &mut World, player: &mut Player) {
    if player.berserk {
        player.berserk_tics = player.berserk_tics.saturating_add(1);
    }
    if player.invul_tics > 0 {
        player.invul_tics -= 1;
        if player.invul_tics == 0 {
            player.invulnerable = false;
        }
    }
    if player.blur_tics > 0 {
        player.blur_tics -= 1;
        if player.blur_tics == 0 {
            player.blur = false;
            if let Some(mo) = player.mo {
                if let Some(a) = world.actors.get_mut(mo) {
                    a.alpha = 1.0;
                    a.render_style = RenderStyle::Normal;
                }
            }
        }
    }
    if player.radsuit_tics > 0 {
        player.radsuit_tics -= 1;
        if player.radsuit_tics == 0 {
            player.radsuit = false;
        }
    }
    if player.infrared_tics > 0 {
        player.infrared_tics -= 1;
        if player.infrared_tics == 0 {
            player.infrared = false;
        }
    }
    if player.fly_tics > 0 {
        player.fly_tics -= 1;
        if player.fly_tics == 0 {
            if let Some(a) = player.mo.and_then(|mo| world.actors.get_mut(mo)) {
                a.flags &= !crate::class::FLAG_NOGRAVITY;
            }
        }
    }
    if player.steroid_tics > 0 {
        player.steroid_tics -= 1;
    }
}

/// Doom `P_DeathThink`: lower the eye 1 unit/tic to 6, then turn 5°/tic toward
/// the killer and fade the red flash once facing them. Returns true while the
/// view is locked onto a killer (manual look is ignored then).
fn death_think(world: &mut World, player: &mut Player, mo: ActorId) -> bool {
    const ANG5: u32 = 0x038E_38E3;
    player.view_height = (player.view_height - 1.0).max(6.0);
    let Some(a) = world.actors.get(mo) else {
        return false;
    };
    player.view_z = a.pos.z() + player.view_height;
    let killer = player
        .attacker
        .filter(|&k| k != mo)
        .and_then(|k| world.actors.get(k))
        .map(|k| (k.pos.x(), k.pos.y()));
    let Some((kx, ky)) = killer else {
        player.damage_count = (player.damage_count - 1).max(0);
        return false;
    };
    let fx = |v: f64| room_core::Fixed::from_f64(v).0;
    let angle = room_core::math::point_to_angle2(fx(a.pos.x()), fx(a.pos.y()), fx(kx), fx(ky));
    let current = a.angle.to_bam().0;
    let delta = angle.wrapping_sub(current);
    let next = if delta < ANG5 || delta > ANG5.wrapping_neg() {
        player.damage_count = (player.damage_count - 1).max(0);
        angle
    } else if delta < 0x8000_0000 {
        current.wrapping_add(ANG5)
    } else {
        current.wrapping_sub(ANG5)
    };
    if let Some(a) = world.actors.get_mut(mo) {
        a.angle = room_core::Bam(next).to_angle();
    }
    true
}

/// Apply this tic's `TicCmd` to the player's mobj.
pub fn player_think(world: &mut World, player: &mut Player) {
    let Some(mo) = player.mo else {
        return;
    };
    world.thinking_refire = player.refire;
    // Raw cmd from the shell / net; filter captured bits for playsim.
    let raw = player.cmd;
    player.raw_cmd = raw;
    if player.player_state == PlayerState::Dead {
        player.input_capture = 0;
        player.capture_module = u16::MAX;
    } else if player.input_capture != 0 {
        let age = world.level.maptime.saturating_sub(player.capture_started);
        if age > crate::player::MAX_CAPTURE_TICS as u32 {
            player.input_capture = 0;
            player.capture_module = u16::MAX;
        } else {
            crate::player::apply_input_capture(&mut player.cmd, player.input_capture);
        }
    }
    let cmd = player.cmd;
    // Dead: sink to the floor, face the killer; USE → respawn at Player 1 start.
    if player.player_state == PlayerState::Dead {
        // Keep lowering the weapon while dead (vanilla `P_MovePsprites`).
        if player.pspr_state.is_some() {
            advance_psprite(world, player, mo);
        }
        let tracking = death_think(world, player, mo);
        if !tracking && (cmd.yaw != 0 || cmd.pitch != 0) {
            if let Some(a) = world.actors.get_mut(mo) {
                if cmd.yaw != 0 {
                    a.angle =
                        Angle::from_degrees(a.angle.degrees() + f64::from(cmd.yaw) * YAW_TO_DEG);
                }
                if cmd.pitch != 0 {
                    let deg =
                        (a.pitch.degrees() + f64::from(cmd.pitch) * YAW_TO_DEG).clamp(-89.0, 89.0);
                    a.pitch = Angle::from_degrees(deg);
                }
            }
        }
        if cmd.buttons.any(Buttons::USE) && !world.no_player_respawn {
            if world.respawn_player_at(player) {
                if let Some(mo) = player.mo {
                    crate::acs::start_respawn_scripts(world, mo);
                }
            }
        }
        player.prev_buttons = raw.buttons.0;
        return;
    }
    if player.player_state != PlayerState::Live {
        player.prev_buttons = raw.buttons.0;
        return;
    }
    player.damage_count = (player.damage_count - 1).max(0);
    player.bonus_count = (player.bonus_count - 1).max(0);
    tick_powerups(world, player);
    apply_duke_crouch(world, player, mo, &cmd);
    // Yaw / pitch (same turn units as Doom `angleturn`).
    if cmd.yaw != 0 || cmd.pitch != 0 {
        if let Some(a) = world.actors.get_mut(mo) {
            if cmd.yaw != 0 {
                if world.math_mode == crate::vanilla::SimMathMode::VanillaFixed {
                    // Doom: `mo->angle += (cmd->angleturn << 16)` (ticcmd already `<<8`).
                    let bam = a.angle.to_bam().0.wrapping_add((cmd.yaw as u32) << 16);
                    a.angle = room_core::Bam(bam).to_angle();
                } else {
                    a.angle =
                        Angle::from_degrees(a.angle.degrees() + f64::from(cmd.yaw) * YAW_TO_DEG);
                }
            }
            if cmd.pitch != 0 {
                let mut deg = a.pitch.degrees() + f64::from(cmd.pitch) * YAW_TO_DEG;
                deg = deg.clamp(-89.0, 89.0);
                a.pitch = Angle::from_degrees(deg);
            }
        }
    }
    // Jump edge while grounded (`JumpZ` impulse → `z_movement`).
    let jump = cmd.buttons.any(Buttons::JUMP);
    if jump && !player.jump_down && player.on_ground && !player.crouching {
        let jump_z = world.actors.get(mo).map(|a| a.jump_z).unwrap_or(8.0);
        if jump_z > 0.0 {
            if let Some(a) = world.actors.get_mut(mo) {
                a.vel = DVec3::new(a.vel.x(), a.vel.y(), jump_z);
            }
            player.on_ground = false;
            play_player_sound(world, mo, "player/male/jump");
        }
    }
    player.jump_down = jump;
    // GZDoom `P_MovePlayer` flight: `upmove` sets vertical speed; jump climbs at 3.
    if player.fly_tics > 0 {
        let vz = if cmd.up_move != 0 {
            Some((f64::from(cmd.up_move) / 128.0).clamp(-8.0, 8.0))
        } else if jump {
            Some(3.0)
        } else {
            None
        };
        if let (Some(vz), Some(a)) = (vz, world.actors.get_mut(mo)) {
            a.vel = DVec3::new(a.vel.x(), a.vel.y(), vz);
            player.on_ground = false;
        }
    }
    // `ChangeCamera` revert: any move input restores the player eye.
    if player.camera_revert
        && (cmd.forward_move != 0 || cmd.side_move != 0 || cmd.yaw != 0 || cmd.pitch != 0)
    {
        player.camera = None;
        player.camera_revert = false;
    }
    if player.fta_tics > 0 {
        player.fta_tics -= 1;
    }
    if player.invdisp_tics > 0 {
        player.invdisp_tics -= 1;
    }
    if player.hurt_delay_tics > 0 {
        player.hurt_delay_tics -= 1;
    }
    if player.return_to_center > 0 {
        player.return_to_center -= 1;
        if let Some(a) = world.actors.get_mut(mo) {
            let deg = a.pitch.degrees();
            let next = if deg > 12.0 {
                deg - 12.0
            } else if deg < -12.0 {
                deg + 12.0
            } else {
                0.0
            };
            a.pitch = Angle::from_degrees(next);
        }
    }
    if player.rotscrnang != 0 {
        let n = i32::from(player.rotscrnang);
        let next = if n > 0 {
            (n - ((n >> 1) + 1)).max(0)
        } else {
            (n + (((-n) >> 1) + 1)).min(0)
        };
        player.rotscrnang = i16::try_from(next).unwrap_or(0);
        if let Some(a) = world.actors.get_mut(mo) {
            a.roll = Angle::from_degrees(f64::from(-(i32::from(player.rotscrnang) * 360) / 2048));
        }
    }
    if player.hard_landing > 0 {
        let n = player.hard_landing;
        player.hard_landing -= 1;
        if let Some(a) = world.actors.get_mut(mo) {
            let deg = (a.pitch.degrees() - f64::from(n)).clamp(-89.0, 89.0);
            a.pitch = Angle::from_degrees(deg);
        }
    }
    let transporter_hold = player.transporter_hold_tics > 0;
    if transporter_hold {
        player.transporter_hold_tics -= 1;
    }
    let tipping = player.tip_incs > 0;
    if tipping {
        player.tip_incs -= 1;
    }
    let mut move_cmd = cmd;
    if player.frozen || transporter_hold || tipping {
        move_cmd.forward_move = 0;
        move_cmd.side_move = 0;
        move_cmd.up_move = 0;
    } else if player.steroid_tics > 0 {
        move_cmd.forward_move = move_cmd.forward_move.saturating_mul(2);
        move_cmd.side_move = move_cmd.side_move.saturating_mul(2);
    }
    let scale = world
        .actors
        .get(mo)
        .and_then(|a| world.classes.get(a.class))
        .map(|c| c.forward_move)
        .filter(|s| *s > 0.0)
        .unwrap_or(1.0);
    if (scale - 1.0).abs() > 1e-9 {
        let scale_i16 = |v: i16| {
            (f64::from(v) * scale)
                .round()
                .clamp(f64::from(i16::MIN), f64::from(i16::MAX)) as i16
        };
        move_cmd.forward_move = scale_i16(move_cmd.forward_move);
        move_cmd.side_move = scale_i16(move_cmd.side_move);
    }
    if player.crouching {
        move_cmd.forward_move /= 2;
        move_cmd.side_move /= 2;
    }
    move_player(world, mo, &move_cmd);
    // Doom `P_MovePlayer` / GZDoom PlayRunning: walk cycle for other players' view.
    update_player_body_sprite(world, mo, &cmd);
    // Automap fog-of-war: reveal linedefs of the sector we stand in (`ML_MAPPED`).
    world.mark_automap_at(mo);
    // Pickups: Doom `PIT_CheckThing` during `P_TryMove`. Room runs
    // `touch_specials` after `xy_movement` in `World::tick` (not here — that
    // was pre-move and delayed DEMO1 healthbonus @64 by one tic).
    // Doom `player->usedown`: use fires once per press, not every held tic
    // (otherwise Door_Raise re-trigger reverses the door while E is held).
    let use_btn = cmd.buttons.any(Buttons::USE);
    if use_btn && !player.use_down {
        // Park the in-hand player so `activate_line` sees `players[].mo` (the
        // slot is empty for the duration of `player_think`). Without this,
        // non-door Use specials (Exit, floors, lights, …) are refused as
        // monster uses while doors still work via the monster whitelist.
        let used = world.with_parked_player(player, |world| {
            use_lines(world, mo) || crate::use_lines::use_actors(world, mo)
        });
        // Vanilla: failed use (nothing usable / refused) plays `*noway` / oof.
        if !used {
            play_player_sound(world, mo, "*noway");
        }
    }
    player.use_down = use_btn;
    let kick_btn = cmd.buttons.any(Buttons::QUICKKICK);
    if kick_btn && !player.quick_kick_down {
        start_quick_kick(world, player, mo);
    }
    player.quick_kick_down = kick_btn;
    let inv_btn = cmd.buttons.any(Buttons::INVUSE);
    if inv_btn {
        use_inventory(world, player, mo, !player.invuse_down);
    }
    player.invuse_down = inv_btn;
    if cmd.weapon_select != 0 {
        player.quick_kick_restore = None;
        select_weapon_slot(world, player, mo, cmd.weapon_select % 10);
    } else if cmd.buttons.any(Buttons::WEAPONNEXT) {
        player.quick_kick_restore = None;
        cycle_weapon(world, player, mo, 1);
    } else if cmd.buttons.any(Buttons::WEAPONPREV) {
        player.quick_kick_restore = None;
        cycle_weapon(world, player, mo, -1);
    }
    // Doom: fire starts from `A_WeaponReady` / `A_ReFire` during psprite advance
    // (not a pre-advance rising edge — that burns a Fire-frame tic early).
    let attack = cmd.buttons.any(Buttons::ATTACK);
    let alt_attack = cmd.buttons.any(Buttons::ALTATTACK);
    player.attack_down = attack;
    player.alt_attack_down = alt_attack;
    // Cold-start Ready without advancing the same tic (entering Fire must keep full tics).
    if player.pspr_state.is_some() {
        advance_psprite(world, player, mo);
    } else {
        ensure_psprite(world, player, mo);
    }
    advance_flash_psprite(world, player);
    // Sync view_z to eye height / grounded flag (pre-`z_movement` this tic).
    if let Some(a) = world.actors.get(mo).cloned() {
        let planes = crate::physics::check_position(world, &a, a.pos, Some(mo));
        if a.vel.z() <= 0.0 {
            player.on_ground = a.pos.z() <= planes.floor_z + 0.1;
        }
        apply_duke_fall(world, player, mo);
        // Vanilla `P_CalcHeight`: bob = (momx²+momy²)/4, capped at MAXBOB=16.
        let bob_sq = a.vel.x() * a.vel.x() + a.vel.y() * a.vel.y();
        player.bob = (bob_sq * 0.25).clamp(0.0, 16.0);
        // Doom `P_CalcHeight`: bob the eye when grounded (fine-angle from maptime).
        let bob_z = if player.on_ground {
            let t = f64::from(world.level.maptime) * (std::f64::consts::TAU / 20.0);
            player.bob * 0.5 * dmath::sin(t)
        } else {
            0.0
        };
        let mut view_z = a.pos.z() + player.view_height + bob_z;
        if let Some(n) = player.pycount {
            let n = (n.wrapping_add(27)) & 2047;
            player.pycount = Some(n);
            let t = f64::from(n) * (std::f64::consts::TAU / 2048.0);
            view_z += 0.5 * dmath::sin(t);
        }
        // Keep the eye under the ceiling (Doom clamps to `ceilingz - 4`).
        if view_z > planes.ceiling_z - 4.0 {
            view_z = planes.ceiling_z - 4.0;
        }
        player.view_z = view_z;
    }
    tick_idle_taunt(world, player, mo, &cmd);
    player.prev_buttons = raw.buttons.0;
}

/// Duke `crack_time`: after ~4 s still, play an `IdleTaunt`.
const IDLE_TAUNT_TICS: i32 = 140;

fn tick_idle_taunt(world: &mut World, player: &mut Player, mo: ActorId, cmd: &TicCmd) {
    let busy = cmd.forward_move != 0
        || cmd.side_move != 0
        || cmd.up_move != 0
        || cmd.yaw != 0
        || cmd.pitch != 0
        || cmd.buttons.any(
            Buttons::ATTACK
                | Buttons::USE
                | Buttons::JUMP
                | Buttons::WEAPONNEXT
                | Buttons::WEAPONPREV
                | Buttons::QUICKKICK
                | Buttons::INVUSE,
        )
        || cmd.weapon_select != 0;
    if busy || player.player_state != PlayerState::Live || !player.on_ground {
        player.idle_tics = 0;
        return;
    }
    let Some(class) = world.actors.get(mo).map(|a| a.class) else {
        return;
    };
    if world
        .classes
        .get(class)
        .is_none_or(|c| c.idle_taunts.is_empty())
    {
        player.idle_tics = 0;
        return;
    }
    player.idle_tics = player.idle_tics.saturating_add(1);
    if player.idle_tics < IDLE_TAUNT_TICS {
        return;
    }
    player.idle_tics = 0;
    let taunts = world
        .classes
        .get(class)
        .map(|c| c.idle_taunts.clone())
        .unwrap_or_default();
    if taunts.is_empty() {
        return;
    }
    let i = (world.p_random("pr_idletaunt") as usize) % taunts.len();
    play_player_sound(world, mo, &taunts[i]);
}

/// Jump the weapon psprite to a class label (`Ready`, `Fire`, …).
///
/// Doom `P_SetPsprite`: call the state's action on **enter**, then keep advancing
/// while `tics == 0` so `A_ReFire` / instant frames resolve in the same tic.
fn set_psprite(world: &mut World, player: &mut Player, mo: ActorId, mut state: StateId) {
    // Cap zero-tic chains so a broken Loop with tics=0 cannot spin forever.
    for _ in 0..64 {
        player.pspr_state = Some(state);
        let (tics, action, args, callback_id, module_index) = world
            .states
            .get(state)
            .map(|st| {
                (
                    i32::from(st.tics),
                    st.action.clone(),
                    st.action_args.clone(),
                    st.callback_id,
                    st.module_index,
                )
            })
            .unwrap_or((-1, None, Vec::new(), -1, 0));
        player.pspr_tics = tics;
        // WASM psprite action (`docs/31` MA1) before native arms.
        if callback_id >= 0 {
            let seat = world
                .players
                .iter()
                .position(|p| p.as_ref().and_then(|p| p.mo) == Some(mo))
                .map(|s| s as i32)
                .unwrap_or(-1);
            if let Some(jump) = crate::script_mods::dispatch_state_action(
                world,
                module_index,
                callback_id,
                mo,
                seat,
                state,
                room_script_abi::action_caller::PSPRITE,
            ) {
                if let Some(j) = jump {
                    state = j;
                    continue;
                }
            }
        } else if let Some(action) = action {
            match action.as_str() {
                "A_ReFire" => {
                    // Hold attack → restart Fire/AltFire (ammo permitting); else fall through.
                    let holding = if player.alt_fire_active {
                        player.alt_attack_down
                    } else {
                        player.attack_down
                    };
                    if holding
                        && player.pending_weapon.is_none()
                        && player.quick_kick_restore.is_none()
                        && world.actors.get(mo).is_some_and(|a| a.health > 0)
                    {
                        player.refire = player.refire.saturating_add(1);
                        world.thinking_refire = player.refire;
                        if let Some(wpn) = player.ready_weapon {
                            if has_ammo(world, mo, wpn) {
                                let named = args
                                    .first()
                                    .map(|s| s.trim().trim_matches('"'))
                                    .filter(|s| !s.is_empty())
                                    .and_then(|l| world.states.find_label(wpn, l));
                                let target = if player.alt_fire_active {
                                    named
                                        .or_else(|| world.states.find_label(wpn, "AltHold"))
                                        .or_else(|| world.states.find_label(wpn, "AltFire"))
                                } else {
                                    named
                                        .or_else(|| world.states.find_label(wpn, "Hold"))
                                        .or_else(|| world.states.find_label(wpn, "Fire"))
                                };
                                if let Some(fire) = target {
                                    let no_alert = world
                                        .classes
                                        .get(wpn)
                                        .is_some_and(|c| c.weapon_noalert);
                                    if !no_alert {
                                        crate::action::noise_alert(world, mo);
                                    }
                                    state = fire;
                                    continue;
                                }
                            } else if let Some(alt) = best_weapon_with_ammo(world, mo, Some(wpn)) {
                                request_weapon(world, player, mo, alt);
                            }
                        }
                    } else {
                        player.refire = 0;
                        world.thinking_refire = 0;
                        player.alt_fire_active = false;
                    }
                }
                "A_Lower" => a_lower(world, player, mo),
                "A_Raise" => a_raise(world, player, mo),
                "A_CheckReload" => {
                    // Vanilla `A_CheckReload` → `P_CheckAmmo` (switch weapon).
                    if let Some(wpn) = player.ready_weapon {
                        if !has_ammo(world, mo, wpn) {
                            if let Some(alt) = best_weapon_with_ammo(world, mo, Some(wpn)) {
                                request_weapon(world, player, mo, alt);
                            }
                        }
                    }
                }
                "A_OpenShotgun2" => play_player_sound(world, mo, "weapons/sshoto"),
                "A_LoadShotgun2" => play_player_sound(world, mo, "weapons/sshotl"),
                "A_CloseShotgun2" => play_player_sound(world, mo, "weapons/sshotc"),
                "A_GunFlash" => start_flash_psprite(world, player, 0),
                "A_Light1" => player.extralight = 1,
                "A_Light2" => player.extralight = 2,
                "A_Light0" | "A_LightDone" => player.extralight = 0,
                "A_WeaponReady" => {
                    if let Some(prev) = player.quick_kick_restore.take() {
                        player.ready_weapon = Some(prev);
                        player.pspr_y = crate::player::WEAPON_TOP;
                        if let Some(ready) = world.states.find_label(prev, "Ready") {
                            state = ready;
                            continue;
                        }
                    }
                    // Dead / dying: enter Deselect → A_Lower (docs/30 W13).
                    if world.actors.get(mo).is_some_and(|a| a.health <= 0) {
                        if let Some(cur) = player.ready_weapon {
                            if let Some(deselect) = world.states.find_label(cur, "Deselect") {
                                player.pending_weapon = None;
                                state = deselect;
                                continue;
                            }
                        }
                    }
                    // Pending switch starts here (vanilla: only from Ready).
                    if player.pending_weapon.is_some() {
                        if let Some(cur) = player.ready_weapon {
                            if let Some(deselect) = world.states.find_label(cur, "Deselect") {
                                state = deselect;
                                continue;
                            }
                        }
                    }
                    // Vanilla: sway only updates while Ready (docs/30 W8.3).
                    {
                        let t = f64::from(world.level.maptime) * (core::f64::consts::PI / 32.0);
                        player.pspr_bob_x = player.bob * dmath::cos(t);
                        player.pspr_bob_y = player.bob * dmath::sin(t).abs();
                    }
                    // `Weapon.ReadySound` on the first Ready frame (Chainsaw idle).
                    if let Some(wpn) = player.ready_weapon {
                        let ready_snd = world
                            .classes
                            .get(wpn)
                            .map(|c| c.ready_sound.clone())
                            .filter(|s| !s.is_empty());
                        if let Some(snd) = ready_snd {
                            let on_first = player.pspr_state.is_some_and(|sid| {
                                world
                                    .states
                                    .find_label(wpn, "Ready")
                                    .is_some_and(|ready| ready == sid)
                            });
                            if on_first {
                                play_player_sound(world, mo, &snd);
                            }
                        }
                    }
                    // Doom: held BT_ATTACK / ALTATTACK while Ready → fire (same tic).
                    if player.cmd.buttons.any(Buttons::ATTACK) {
                        player.alt_fire_active = false;
                        fire_weapon(world, player, mo, false);
                        return;
                    }
                    if player.cmd.buttons.any(Buttons::ALTATTACK) {
                        player.alt_fire_active = true;
                        fire_weapon(world, player, mo, true);
                        return;
                    }
                }
                other => {
                    world.psprite_weapon = player.ready_weapon;
                    world.psprite_state = Some(state);
                    world.psprite_flash_request = None;
                    let args = crate::deco_eval::eval_action_args(world, mo, &args);
                    let result = crate::action::call_action_args(world, mo, other, &args);
                    // Fire actions request their own flash after deplete (N4/N5).
                    if let Some(offset) = world.psprite_flash_request.take() {
                        start_flash_psprite(world, player, offset);
                    }
                    world.psprite_weapon = None;
                    world.psprite_state = None;
                    if let crate::action::ActionResult::Jump(j) = result {
                        state = j;
                        continue;
                    }
                }
            }
        }
        // Action may have jumped `pspr_state` (A_Lower / A_Raise / request_weapon).
        let Some(cur) = player.pspr_state else {
            return;
        };
        if cur != state {
            state = cur;
            continue;
        }
        if player.pspr_tics != 0 {
            return;
        }
        // Doom: keep calling `P_SetPsprite` while tics == 0.
        let Some(next) = world.states.get(state).and_then(|st| st.next) else {
            player.pspr_state = None;
            player.pspr_tics = 0;
            ensure_psprite(world, player, mo);
            return;
        };
        state = next;
    }
}

/// `A_Lower` — drop weapon; when off-screen, equip `pending_weapon` and raise.
fn a_lower(world: &mut World, player: &mut Player, mo: ActorId) {
    player.pspr_y += crate::player::WEAPON_SWITCH_SPEED;
    if player.pspr_y < crate::player::WEAPON_BOTTOM {
        return;
    }
    player.pspr_y = crate::player::WEAPON_BOTTOM;
    // Vanilla: dead players park the gun at `WEAPONBOTTOM` and stay there.
    let dead = player.player_state != PlayerState::Live
        || world.actors.get(mo).is_some_and(|a| a.health <= 0);
    if dead {
        player.pending_weapon = None;
        return;
    }
    let Some(next) = player.pending_weapon.take() else {
        // No pending → stay holstered (rare); snap ready off.
        player.pspr_state = None;
        player.pspr_tics = 0;
        return;
    };
    player.ready_weapon = Some(next);
    player.pspr_flash_state = None;
    player.pspr_flash_tics = 0;
    player.extralight = 0;
    bring_up_weapon(world, player, mo);
}

/// `A_Raise` — lift weapon to ready height, then enter Ready.
fn a_raise(world: &mut World, player: &mut Player, mo: ActorId) {
    player.pspr_y -= crate::player::WEAPON_SWITCH_SPEED;
    if player.pspr_y > crate::player::WEAPON_TOP {
        return;
    }
    player.pspr_y = crate::player::WEAPON_TOP;
    let Some(wpn) = player.ready_weapon else {
        return;
    };
    if let Some(ready) = world.states.find_label(wpn, "Ready") {
        set_psprite(world, player, mo, ready);
    }
}

fn bring_up_weapon(world: &mut World, player: &mut Player, mo: ActorId) {
    let Some(wpn) = player.ready_weapon else {
        return;
    };
    // Doom `P_BringUpWeapon`: start from off-screen, then Select / A_Raise.
    player.pspr_y = crate::player::WEAPON_BOTTOM;
    // `Weapon.UpSound` (Chainsaw: weapons/sawup).
    let up_snd = world
        .classes
        .get(wpn)
        .map(|c| c.up_sound.clone())
        .filter(|s| !s.is_empty());
    if let Some(snd) = up_snd {
        play_player_sound(world, mo, &snd);
    }
    if let Some(sel) = world.states.find_label(wpn, "Select") {
        set_psprite(world, player, mo, sel);
    } else if let Some(ready) = world.states.find_label(wpn, "Ready") {
        player.pspr_y = crate::player::WEAPON_TOP;
        set_psprite(world, player, mo, ready);
    } else {
        player.pspr_state = None;
        player.pspr_tics = 0;
    }
}

fn start_flash_psprite(world: &World, player: &mut Player, offset: usize) {
    let Some(wpn) = player.ready_weapon else {
        return;
    };
    let Some(flash0) = world.states.find_label(wpn, "Flash") else {
        return;
    };
    // GZDoom `SetSafeFlash`: flash0 + offset within the owning class's states.
    let owner0 = world.states.get(flash0).map(|s| s.owner);
    let mut flash = room_core::StateId::from_raw(flash0.raw().saturating_add(offset as u32));
    if world.states.get(flash).map(|s| s.owner) != owner0 {
        flash = flash0;
    }
    player.pspr_flash_state = Some(flash);
    let st = world.states.get(flash);
    player.pspr_flash_tics = st.map(|s| i32::from(s.tics)).unwrap_or(0);
    // Apply Flash frame light action immediately (usually `A_Light1`).
    if let Some(action) = st.and_then(|s| s.action.as_deref()) {
        match action {
            "A_Light1" => player.extralight = 1,
            "A_Light2" => player.extralight = 2,
            "A_Light0" | "A_LightDone" => player.extralight = 0,
            _ => {}
        }
    }
}

fn advance_flash_psprite(world: &World, player: &mut Player) {
    let Some(sid) = player.pspr_flash_state else {
        return;
    };
    if player.pspr_flash_tics < 0 {
        return;
    }
    if player.pspr_flash_tics > 0 {
        player.pspr_flash_tics -= 1;
    }
    if player.pspr_flash_tics != 0 {
        return;
    }
    let next = world.states.get(sid).and_then(|st| st.next);
    match next {
        Some(n) => {
            let Some(st) = world.states.get(n) else {
                player.pspr_flash_state = None;
                player.pspr_flash_tics = 0;
                player.extralight = 0;
                return;
            };
            if st.sprite.is_empty() {
                player.pspr_flash_state = None;
                player.pspr_flash_tics = 0;
                player.extralight = 0;
                return;
            }
            player.pspr_flash_state = Some(n);
            player.pspr_flash_tics = i32::from(st.tics);
            match st.action.as_deref() {
                Some("A_Light1") => player.extralight = 1,
                Some("A_Light2") => player.extralight = 2,
                Some("A_Light0" | "A_LightDone") => player.extralight = 0,
                _ => {}
            }
        }
        None => {
            player.pspr_flash_state = None;
            player.pspr_flash_tics = 0;
            player.extralight = 0;
        }
    }
}

/// Begin Deselect → `A_Lower` when the player dies (docs/30 W13).
pub(crate) fn start_death_weapon_lower(world: &mut World, player: &mut Player) {
    let Some(mo) = player.mo else {
        return;
    };
    player.pending_weapon = None;
    if let Some(wpn) = player.ready_weapon {
        if let Some(deselect) = world.states.find_label(wpn, "Deselect") {
            set_psprite(world, player, mo, deselect);
            return;
        }
    }
    player.pspr_y = crate::player::WEAPON_BOTTOM;
    player.pspr_state = None;
    player.pspr_tics = 0;
}

fn ensure_psprite(world: &mut World, player: &mut Player, mo: ActorId) {
    if player.pspr_state.is_some() {
        return;
    }
    if player.ready_weapon.is_none() {
        return;
    }
    // Spawn / respawn / first equip: raise from `WEAPONBOTTOM` (docs/30 W13).
    // Mid-sequence Stop with the gun already up → snap back to Ready.
    if (player.pspr_y - crate::player::WEAPON_TOP).abs() > 0.5 {
        bring_up_weapon(world, player, mo);
    } else if let Some(wpn) = player.ready_weapon {
        if let Some(ready) = world.states.find_label(wpn, "Ready") {
            set_psprite(world, player, mo, ready);
        }
    }
}

fn advance_psprite(world: &mut World, player: &mut Player, mo: ActorId) {
    let Some(sid) = player.pspr_state else {
        return;
    };
    if player.pspr_tics < 0 {
        return;
    }
    if player.pspr_tics > 0 {
        player.pspr_tics -= 1;
    }
    if player.pspr_tics != 0 {
        return;
    }
    let next = world.states.get(sid).and_then(|st| st.next);
    match next {
        Some(n) => set_psprite(world, player, mo, n),
        None => {
            // Stop → return to Ready if possible.
            player.pspr_state = None;
            player.pspr_tics = 0;
            ensure_psprite(world, player, mo);
        }
    }
}

/// Slot key: cycle owned weapons in the slot (GZDoom); otherwise first owned.
fn select_weapon_slot(world: &mut World, player: &mut Player, mo: ActorId, slot: u8) {
    let names = world.weapon_slots.weapons_for(slot);
    let owned: Vec<_> = names
        .iter()
        .filter_map(|name| {
            let cid = world.classes.find_name(name)?;
            (world.inventory.count(mo, cid) > 0).then_some(cid)
        })
        .collect();
    if owned.is_empty() {
        return;
    }
    let next = if let Some(cur) = player.ready_weapon.and_then(|c| owned.iter().position(|&x| x == c))
    {
        owned[(cur + 1) % owned.len()]
    } else {
        owned[0]
    };
    request_weapon(world, player, mo, next);
}

/// Queue a weapon change; prefer Deselect→A_Lower when available.
pub fn request_weapon_change(
    world: &mut World,
    player: &mut Player,
    mo: ActorId,
    cid: room_core::ClassId,
) {
    request_weapon(world, player, mo, cid);
}

/// After inventory was taken: switch away from a ready / pending weapon the
/// player no longer owns (GZDoom `PickNewWeapon`).
pub fn drop_unowned_weapon(world: &mut World, player: &mut Player, mo: ActorId) {
    let owns = |w: Option<room_core::ClassId>| w.is_some_and(|c| world.inventory.count(mo, c) > 0);
    if player.pending_weapon.is_some() && !owns(player.pending_weapon) {
        player.pending_weapon = None;
    }
    if player.ready_weapon.is_none() || owns(player.ready_weapon) {
        return;
    }
    let next = best_weapon_with_ammo(world, mo, None)
        .or_else(|| owned_weapon_order(world, mo).into_iter().next());
    match next {
        Some(cid) => request_weapon(world, player, mo, cid),
        None => {
            player.ready_weapon = None;
            player.pspr_state = None;
        }
    }
}

/// Queue a weapon change. Deselect starts from `A_WeaponReady` (docs/30 W4).
fn request_weapon(world: &mut World, player: &mut Player, mo: ActorId, cid: room_core::ClassId) {
    if player.ready_weapon == Some(cid) && player.pending_weapon.is_none() {
        return;
    }
    if player.pending_weapon == Some(cid) {
        return;
    }
    let Some(cur) = player.ready_weapon else {
        player.ready_weapon = Some(cid);
        player.pending_weapon = None;
        player.pspr_state = None;
        ensure_psprite(world, player, mo);
        return;
    };
    if cur == cid {
        player.pending_weapon = None;
        return;
    }
    // Only set pending; `A_WeaponReady` enters Deselect. Immediate switch when
    // there is no Ready/Deselect path (broken weapon).
    if world.states.find_label(cur, "Deselect").is_some() {
        player.pending_weapon = Some(cid);
    } else {
        player.ready_weapon = Some(cid);
        player.pending_weapon = None;
        player.pspr_state = None;
        player.pspr_y = crate::player::WEAPON_TOP;
        ensure_psprite(world, player, mo);
    }
}

/// Ordered weapon list for next/prev cycling (slot preference order).
fn owned_weapon_order(world: &World, mo: ActorId) -> Vec<room_core::ClassId> {
    let mut out = Vec::new();
    for slot in 1u8..=7 {
        for name in world.weapon_slots.weapons_for(slot) {
            let Some(cid) = world.classes.find_name(&name) else {
                continue;
            };
            if world.inventory.count(mo, cid) > 0 && !out.contains(&cid) {
                out.push(cid);
            }
        }
    }
    out
}

fn cycle_weapon(world: &mut World, player: &mut Player, mo: ActorId, dir: i32) {
    let owned = owned_weapon_order(world, mo);
    if owned.is_empty() {
        return;
    }
    let cur = player
        .ready_weapon
        .and_then(|c| owned.iter().position(|&x| x == c));
    let next = match (cur, dir.is_positive()) {
        (Some(i), true) => (i + 1) % owned.len(),
        (Some(i), false) => (i + owned.len() - 1) % owned.len(),
        (None, true) => 0,
        (None, false) => owned.len() - 1,
    };
    let next_wpn = owned[next];
    request_weapon(world, player, mo, next_wpn);
}

/// Duke medkit heals 1 HP per tic while held; other items start on a press.
fn use_inventory(world: &mut World, player: &mut Player, mo: ActorId, pressed: bool) {
    if pressed
        && (2..=7).contains(&player.inven_icon)
        && use_inven_icon(world, player, mo, player.inven_icon)
    {
        return;
    }
    if let Some(kit) = world.classes.find_name("DukeFirstAid") {
        if crate::inventory::count_inventory(world, mo, kit) > 0 {
            let hp = world.actors.get(mo).map(|a| a.health).unwrap_or(100);
            if hp < 100 {
                if let Some(a) = world.actors.get_mut(mo) {
                    a.health = (hp + 1).min(100);
                }
                let _ = crate::inventory::take_inventory(world, mo, kit, 1);
                return;
            }
        }
    }
    if !pressed {
        return;
    }
    if player.steroid_tics == 0 {
        if let Some(st) = world.classes.find_name("DukeSteroids") {
            if crate::inventory::count_inventory(world, mo, st) > 0 {
                let _ = crate::inventory::take_inventory(world, mo, st, 1);
                player.steroid_tics = 420;
                return;
            }
        }
    }
    if !player.infrared {
        if let Some(nv) = world.classes.find_name("DukeNightVision") {
            if crate::inventory::count_inventory(world, mo, nv) > 0 {
                let _ = crate::inventory::take_inventory(world, mo, nv, 1);
                player.infrared = true;
                player.infrared_tics = crate::player::INFRARED_TICS;
                return;
            }
        }
    }
    if !player.radsuit {
        for name in ["DukeScubaGear", "DukeBoots"] {
            if let Some(gear) = world.classes.find_name(name) {
                if crate::inventory::count_inventory(world, mo, gear) > 0 {
                    let _ = crate::inventory::take_inventory(world, mo, gear, 1);
                    player.radsuit = true;
                    player.radsuit_tics = crate::player::RADSUIT_TICS;
                    if name == "DukeScubaGear" {
                        let t = i32::try_from(world.level.time).unwrap_or(i32::MAX);
                        player.air_finished = t.saturating_add(60 * crate::player::TICRATE);
                    }
                    return;
                }
            }
        }
    }
    if let Some(holo) = world.classes.find_name("DukeHoloduke") {
        if crate::inventory::count_inventory(world, mo, holo) > 0 {
            if let Some(decoy) = world.classes.find_name("DukeHolodukeDecoy") {
                if let Some(a) = world.actors.get(mo) {
                    let pos = a.pos;
                    let angle = a.angle;
                    let _ = world.spawn_at(decoy, pos, angle);
                }
            }
            let _ = crate::inventory::take_inventory(world, mo, holo, 1);
        }
    }
}

/// Use the Duke status-bar selection (`inven_icon` 2 steroids … 7 boots).
fn use_inven_icon(world: &mut World, player: &mut Player, mo: ActorId, icon: u8) -> bool {
    match icon {
        2 => {
            if player.steroid_tics != 0 {
                return false;
            }
            let Some(st) = world.classes.find_name("DukeSteroids") else {
                return false;
            };
            if crate::inventory::count_inventory(world, mo, st) == 0 {
                return false;
            }
            let _ = crate::inventory::take_inventory(world, mo, st, 1);
            player.steroid_tics = 420;
            true
        }
        3 => {
            let Some(holo) = world.classes.find_name("DukeHoloduke") else {
                return false;
            };
            if crate::inventory::count_inventory(world, mo, holo) == 0 {
                return false;
            }
            if let Some(decoy) = world.classes.find_name("DukeHolodukeDecoy") {
                if let Some(a) = world.actors.get(mo) {
                    let pos = a.pos;
                    let angle = a.angle;
                    let _ = world.spawn_at(decoy, pos, angle);
                }
            }
            let _ = crate::inventory::take_inventory(world, mo, holo, 1);
            true
        }
        4 => {
            if player.fly_tics > 0 {
                return false;
            }
            let Some(jp) = world
                .classes
                .find_name("DukeJetpack")
                .or_else(|| world.classes.find_name("Jetpack"))
            else {
                return false;
            };
            if crate::inventory::count_inventory(world, mo, jp) == 0 {
                return false;
            }
            player.fly_tics = crate::player::FLIGHT_TICS;
            if let Some(a) = world.actors.get_mut(mo) {
                a.flags |= crate::class::FLAG_NOGRAVITY;
            }
            true
        }
        5 => {
            if player.infrared {
                return false;
            }
            let Some(nv) = world.classes.find_name("DukeNightVision") else {
                return false;
            };
            if crate::inventory::count_inventory(world, mo, nv) == 0 {
                return false;
            }
            let _ = crate::inventory::take_inventory(world, mo, nv, 1);
            player.infrared = true;
            player.infrared_tics = crate::player::INFRARED_TICS;
            true
        }
        6 | 7 => {
            if player.radsuit {
                return false;
            }
            let name = if icon == 6 {
                "DukeScubaGear"
            } else {
                "DukeBoots"
            };
            let Some(gear) = world.classes.find_name(name) else {
                return false;
            };
            if crate::inventory::count_inventory(world, mo, gear) == 0 {
                return false;
            }
            let _ = crate::inventory::take_inventory(world, mo, gear, 1);
            player.radsuit = true;
            player.radsuit_tics = crate::player::RADSUIT_TICS;
            if icon == 6 {
                let t = i32::try_from(world.level.time).unwrap_or(i32::MAX);
                player.air_finished = t.saturating_add(60 * crate::player::TICRATE);
            }
            true
        }
        _ => false,
    }
}

fn kick_weapon(world: &World) -> Option<room_core::ClassId> {
    world
        .classes
        .find_name("DukeKick")
        .or_else(|| world.classes.find_name("Fist"))
}

/// Play the kick Fire overlay, then restore the ready gun (`A_WeaponReady`).
fn start_quick_kick(world: &mut World, player: &mut Player, mo: ActorId) {
    if player.quick_kick_restore.is_some() {
        return;
    }
    let Some(kick) = kick_weapon(world) else {
        return;
    };
    if world.inventory.count(mo, kick) == 0 {
        return;
    }
    let Some(fire) = world.states.find_label(kick, "Fire") else {
        return;
    };
    if player.ready_weapon != Some(kick) {
        player.quick_kick_restore = player.ready_weapon;
    }
    player.pending_weapon = None;
    player.ready_weapon = Some(kick);
    player.pspr_y = crate::player::WEAPON_TOP;
    crate::action::noise_alert(world, mo);
    set_psprite(world, player, mo, fire);
}

fn fire_weapon(world: &mut World, player: &mut Player, mo: ActorId, alt: bool) {
    // Block fire while raising/lowering; pending is handled in Ready.
    if (player.pspr_y - crate::player::WEAPON_TOP).abs() > 0.5 {
        return;
    }
    if player.pending_weapon.is_some() {
        return;
    }
    if let Some(wpn) = player.ready_weapon {
        // Vanilla `P_FireWeapon` only *checks* ammo; actions deplete.
        if !has_ammo(world, mo, wpn) {
            if let Some(alt_w) = best_weapon_with_ammo(world, mo, Some(wpn)) {
                request_weapon(world, player, mo, alt_w);
            }
            return;
        }
        let label = if alt { "AltFire" } else { "Fire" };
        if let Some(fire) = world.states.find_label(wpn, label).or_else(|| {
            if alt {
                None
            } else {
                world.states.find_label(wpn, "Fire")
            }
        }) {
            player.alt_fire_active = alt;
            let no_alert = world.classes.get(wpn).is_some_and(|c| c.weapon_noalert);
            if !no_alert {
                crate::action::noise_alert(world, mo);
            }
            set_psprite(world, player, mo, fire);
            return;
        }
        if alt {
            // No AltFire label — ignore secondary.
            player.alt_fire_active = false;
            return;
        }
    }
    let angle = world.actors.get(mo).map(|a| a.angle).unwrap_or(Angle::ZERO);
    let _ = line_attack(world, mo, angle, 2048.0, 5);
}

/// Owned weapon with ammo, preferred by lowest `Weapon.SelectionOrder` (W3).
fn best_weapon_with_ammo(
    world: &World,
    mo: ActorId,
    exclude: Option<room_core::ClassId>,
) -> Option<room_core::ClassId> {
    let mut best: Option<(i32, room_core::ClassId)> = None;
    for cid in owned_weapon_order(world, mo) {
        if exclude == Some(cid) || !has_ammo(world, mo, cid) {
            continue;
        }
        let order = world
            .classes
            .get(cid)
            .map(|c| c.selection_order)
            .unwrap_or(9999);
        match best {
            Some((o, _)) if order >= o => {}
            _ => best = Some((order, cid)),
        }
    }
    best.map(|(_, c)| c)
}

fn play_player_sound(world: &mut World, mo: ActorId, name: &str) {
    if let Some(a) = world.actors.get(mo) {
        world.play_sound(name, a.pos.x(), a.pos.y(), a.pos.z(), 1.0);
    }
}

/// True if the player has at least `Weapon.AmmoUse` of the weapon's ammo type.
fn has_ammo(world: &World, mo: ActorId, weapon: room_core::ClassId) -> bool {
    let Some(def) = world.classes.get(weapon) else {
        return true;
    };
    if def.ammo_type.is_empty() || def.ammo_use <= 0 {
        return true;
    }
    let Some(ammo) = world.classes.find_name(&def.ammo_type) else {
        return false;
    };
    world.inventory.count(mo, ammo) >= def.ammo_use
}

/// Spend `Weapon.AmmoUse` of `Weapon.AmmoType`. Returns false if not enough ammo.
/// Called from fire actions (docs/30 W2/W14), not from `P_FireWeapon`.
pub(crate) fn deplete_ammo(world: &mut World, mo: ActorId, weapon: room_core::ClassId) -> bool {
    try_consume_ammo(world, mo, weapon)
}

fn try_consume_ammo(world: &mut World, mo: ActorId, weapon: room_core::ClassId) -> bool {
    let Some(def) = world.classes.get(weapon) else {
        return true;
    };
    if def.ammo_use <= 0 || def.ammo_type.is_empty() {
        return true;
    }
    let Some(ammo) = world.classes.find_name(&def.ammo_type) else {
        return true;
    };
    let have = world.inventory.count(mo, ammo);
    if have < def.ammo_use {
        if std::env::var_os("ROOM_TRACE_AMMO").is_some() {
            eprintln!(
                "AMMOFAIL tic={} wpn={} have={} need={}",
                world.level.time, def.name, have, def.ammo_use
            );
        }
        return false;
    }
    let taken = world.inventory.take(mo, ammo, def.ammo_use);
    if std::env::var_os("ROOM_TRACE_AMMO").is_some() {
        eprintln!(
            "AMMOUSE tic={} wpn={} use={} had={} now={}",
            world.level.time,
            def.name,
            def.ammo_use,
            have,
            world.inventory.count(mo, ammo)
        );
    }
    taken >= def.ammo_use
}

/// Advance the third-person `PLAY` sprite: See (run) while moving, Missile while
/// firing, Spawn when idle. Skips Pain/Death frames (`PLAY` G+).
fn update_player_body_sprite(world: &mut World, mo: ActorId, cmd: &crate::player::TicCmd) {
    let Some(a) = world.actors.get(mo) else {
        return;
    };
    // Pain = G (6), death = H+ — leave those alone.
    if !a.sprite.eq_ignore_ascii_case("PLAY") || a.frame >= 6 {
        return;
    }
    let class = a.class;
    let moving = cmd.forward_move != 0 || cmd.side_move != 0;
    let attacking = cmd.buttons.any(Buttons::ATTACK);
    let spawn = world.states.find_label(class, "Spawn");
    let see = world.states.find_label(class, "See");
    let missile = world.states.find_label(class, "Missile");
    let cur = a.state;
    let frame = a.frame;

    if attacking {
        if let Some(mis) = missile {
            // Frames E/F (4–5) are the attack pose; don't restart every tic.
            if frame < 4 || frame > 5 {
                world.set_state(mo, Some(mis), true);
            }
        }
        return;
    }

    if moving {
        if let Some(see) = see {
            // Enter run cycle from idle (Spawn) only; stay in See while moving.
            if spawn.is_some_and(|s| cur == Some(s)) {
                world.set_state(mo, Some(see), true);
            }
        }
    } else if let Some(spawn) = spawn {
        // Standing: return to idle (GZDoom PlayIdle). Run frames are A–D (0–3).
        if cur != Some(spawn) && frame <= 3 {
            world.set_state(mo, Some(spawn), false);
        }
    }
}

fn move_player(world: &mut World, mo: ActorId, cmd: &crate::player::TicCmd) {
    let Some(actor) = world.actors.get(mo).cloned() else {
        return;
    };
    let vanilla = world.math_mode == crate::vanilla::SimMathMode::VanillaFixed;
    if vanilla {
        // Doom `P_MovePlayer`: thrust only when `z <= floorz` (no air control).
        let floor_z = crate::check_position_fixed(world, &actor, actor.pos, Some(mo)).floor_z;
        if actor.pos.z() > floor_z {
            return;
        }
        // Chocolate/Doom `P_Thrust`: `mom += FixedMul(move*2048, fine{sin,cos})` where
        // `ticcmd_t::{forward,side}move` is a signed char (demo byte, not `<<8`).
        // Room TicCmd stores demo bytes `<<8` for other call sites — recover via `>>8`.
        let fwd = cmd.forward_move >> 8;
        let sid = cmd.side_move >> 8;
        if fwd == 0 && sid == 0 {
            return; // coast via `xy_movement` friction
        }
        let bam = actor.angle.to_bam().0;
        let fine = room_core::math::bam_to_fine(bam);
        let mut momx = room_core::Fixed::from_f64(actor.vel.x());
        let mut momy = room_core::Fixed::from_f64(actor.vel.y());
        if fwd != 0 {
            let mv = room_core::Fixed(i32::from(fwd) * 2048);
            momx = momx + room_core::Fixed(room_core::math::finecosine(fine)).mul(mv);
            momy = momy + room_core::Fixed(room_core::math::finesine(fine)).mul(mv);
        }
        if sid != 0 {
            // Strafe: angle − ANG90.
            let fine_s = room_core::math::bam_to_fine(bam.wrapping_sub(0x4000_0000));
            let mv = room_core::Fixed(i32::from(sid) * 2048);
            momx = momx + room_core::Fixed(room_core::math::finecosine(fine_s)).mul(mv);
            momy = momy + room_core::Fixed(room_core::math::finesine(fine_s)).mul(mv);
        }
        if let Some(a) = world.actors.get_mut(mo) {
            a.vel = DVec3::new(momx.to_f64(), momy.to_f64(), a.vel.z());
        }
        return;
    }
    let forward = f64::from(cmd.forward_move) * MOVE_SCALE;
    let side = f64::from(cmd.side_move) * MOVE_SCALE;
    if forward == 0.0 && side == 0.0 {
        // Duke: releasing move in the air keeps XY speed. Doom zeros it.
        let floor_z = crate::physics::floor_at_xy(world, actor.pos.x(), actor.pos.y())
            .unwrap_or(actor.pos.z());
        let airborne = actor.pos.z() > floor_z + 0.1;
        if !airborne {
            if let Some(a) = world.actors.get_mut(mo) {
                a.vel = DVec3::new(0.0, 0.0, a.vel.z());
            }
        }
        return;
    }
    let rad = actor.angle.radians();
    let (s, c) = (dmath::sin(rad), dmath::cos(rad));
    // Doom: forward along view; +side = strafe right (thrust toward angle−90°).
    // x += forward*cos(a) + side*sin(a); y += forward*sin(a) − side*cos(a)
    let tx = forward * c + side * s;
    let ty = forward * s - side * c;
    let floor_z =
        crate::physics::floor_at_xy(world, actor.pos.x(), actor.pos.y()).unwrap_or(actor.pos.z());
    let airborne = actor.pos.z() > floor_z + 0.1;
    let duke = world
        .classes
        .get(actor.class)
        .is_some_and(|c| c.name == "DukePlayer");
    // Duke: holding move in the air steers; it does not replace XY speed.
    let (vx, vy) = if duke && airborne {
        (actor.vel.x() + tx * 0.25, actor.vel.y() + ty * 0.25)
    } else {
        (tx, ty)
    };
    if let Some(a) = world.actors.get_mut(mo) {
        a.vel = DVec3::new(vx, vy, a.vel.z());
    }
    let new_pos = DVec3::new(actor.pos.x() + vx, actor.pos.y() + vy, actor.pos.z());
    if !try_move(world, mo, new_pos) {
        // Vanilla: bumping a wall while moving plays `sfx_oof` / `*noway`.
        play_player_sound(world, mo, "*noway");
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use crate::{Buttons, ClassRegistry, StateTable, World, give_inventory};
    use room_decorate::parse_decorate;
    use room_nodebuild::apply_nodes;
    use room_testkit::{MapBuilder, rect};
    use std::sync::Arc;

    #[test]
    fn next_weapon_cycles_owned() {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let file = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&file).unwrap());
        let states = Arc::new(StateTable::from_decorate(&file, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let fist = classes.find_name("Fist").unwrap();
        let pistol = classes.find_name("Pistol").unwrap();
        let shotgun = classes.find_name("Shotgun").unwrap();
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        let _ = give_inventory(&mut w, mo, shotgun, 1);
        if let Some(p) = w.players[0].as_mut() {
            p.ready_weapon = Some(fist);
            p.pspr_y = crate::player::WEAPON_TOP;
            p.cmd.buttons = Buttons(Buttons::WEAPONNEXT);
            let mut player = p.clone();
            player_think(&mut w, &mut player);
            assert_eq!(player.pending_weapon, Some(pistol));
            // Finish Deselect (A_Lower) + Select (A_Raise).
            player.cmd.buttons = Buttons(0);
            for _ in 0..40 {
                player_think(&mut w, &mut player);
            }
            assert_eq!(player.ready_weapon, Some(pistol));
            assert!(player.pending_weapon.is_none());
            player.cmd.buttons = Buttons(Buttons::WEAPONNEXT);
            player_think(&mut w, &mut player);
            player.cmd.buttons = Buttons(0);
            for _ in 0..40 {
                player_think(&mut w, &mut player);
            }
            assert_eq!(player.ready_weapon, Some(shotgun));
            player.cmd.buttons = Buttons(Buttons::WEAPONPREV);
            player_think(&mut w, &mut player);
            player.cmd.buttons = Buttons(0);
            for _ in 0..40 {
                player_think(&mut w, &mut player);
            }
            assert_eq!(player.ready_weapon, Some(pistol));
        }
    }

    #[test]
    fn quick_kick_returns_to_the_ready_gun() {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let file = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&file).unwrap());
        let states = Arc::new(StateTable::from_decorate(&file, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let fist = classes.find_name("Fist").unwrap();
        let pistol = classes.find_name("Pistol").unwrap();
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        let _ = give_inventory(&mut w, mo, fist, 1);
        let mut player = w.players[0].take().unwrap();
        player.ready_weapon = Some(pistol);
        player.pspr_y = crate::player::WEAPON_TOP;
        player.cmd.buttons = Buttons(Buttons::QUICKKICK);
        player_think(&mut w, &mut player);
        assert_eq!(player.ready_weapon, Some(fist));
        assert_eq!(player.quick_kick_restore, Some(pistol));
        player.cmd.buttons = Buttons(0);
        for _ in 0..40 {
            player_think(&mut w, &mut player);
        }
        assert_eq!(player.ready_weapon, Some(pistol));
        assert!(player.quick_kick_restore.is_none());
        w.players[0] = Some(player);
    }

    #[test]
    fn use_inventory_heals_from_the_medkit() {
        let src = format!(
            "{}\nactor DukeFirstAid : Actor\n{{\n  Inventory.Amount 1\n  Inventory.MaxAmount 100\n}}\n",
            include_str!("../../../testdata/actors/doom_basic.txt")
        );
        let file = parse_decorate(&src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&file).unwrap());
        let states = Arc::new(StateTable::from_decorate(&file, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let kit = classes.find_name("DukeFirstAid").unwrap();
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        w.actors.get_mut(mo).unwrap().health = 50;
        let _ = give_inventory(&mut w, mo, kit, 20);
        let mut player = w.players[0].take().unwrap();
        player.cmd.buttons = Buttons(Buttons::INVUSE);
        for _ in 0..10 {
            player_think(&mut w, &mut player);
        }
        assert_eq!(w.actors.get(mo).unwrap().health, 60);
        assert_eq!(crate::inventory::count_inventory(&w, mo, kit), 10);
        w.players[0] = Some(player);
    }

    #[test]
    fn use_inventory_starts_steroids_when_the_medkit_cannot_heal() {
        let src = format!(
            "{}\nactor DukeFirstAid : Actor\n{{\n  Inventory.Amount 1\n  Inventory.MaxAmount 100\n}}\n\
             actor DukeSteroids : Actor\n{{\n  Inventory.Amount 1\n  Inventory.MaxAmount 1\n}}\n",
            include_str!("../../../testdata/actors/doom_basic.txt")
        );
        let file = parse_decorate(&src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&file).unwrap());
        let states = Arc::new(StateTable::from_decorate(&file, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let kit = classes.find_name("DukeFirstAid").unwrap();
        let st = classes.find_name("DukeSteroids").unwrap();
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        w.actors.get_mut(mo).unwrap().health = 100;
        let _ = give_inventory(&mut w, mo, kit, 5);
        let _ = give_inventory(&mut w, mo, st, 1);
        let mut player = w.players[0].take().unwrap();
        player.cmd.buttons = Buttons(Buttons::INVUSE);
        player_think(&mut w, &mut player);
        assert_eq!(player.steroid_tics, 420);
        assert_eq!(crate::inventory::count_inventory(&w, mo, kit), 5);
        assert_eq!(crate::inventory::count_inventory(&w, mo, st), 0);
        player.cmd = TicCmd {
            forward_move: 25 * 256,
            buttons: Buttons(0),
            ..TicCmd::default()
        };
        player_think(&mut w, &mut player);
        let sped =
            w.actors.get(mo).unwrap().vel.x().abs() + w.actors.get(mo).unwrap().vel.y().abs();
        w.players[0] = Some(player);
        assert!(sped > 0.4, "steroid move {sped}");
    }

    #[test]
    fn use_inventory_uses_selected_steroids_while_hurt() {
        let src = format!(
            "{}\nactor DukeFirstAid : Actor\n{{\n  Inventory.Amount 1\n  Inventory.MaxAmount 100\n}}\n\
             actor DukeSteroids : Actor\n{{\n  Inventory.Amount 1\n  Inventory.MaxAmount 1\n}}\n",
            include_str!("../../../testdata/actors/doom_basic.txt")
        );
        let file = parse_decorate(&src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&file).unwrap());
        let states = Arc::new(StateTable::from_decorate(&file, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let kit = classes.find_name("DukeFirstAid").unwrap();
        let st = classes.find_name("DukeSteroids").unwrap();
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        w.actors.get_mut(mo).unwrap().health = 50;
        let _ = give_inventory(&mut w, mo, kit, 5);
        let _ = give_inventory(&mut w, mo, st, 1);
        let mut player = w.players[0].take().unwrap();
        player.inven_icon = 2;
        player.cmd.buttons = Buttons(Buttons::INVUSE);
        player_think(&mut w, &mut player);
        assert_eq!(player.steroid_tics, 420);
        assert_eq!(w.actors.get(mo).unwrap().health, 50);
        assert_eq!(crate::inventory::count_inventory(&w, mo, kit), 5);
        assert_eq!(crate::inventory::count_inventory(&w, mo, st), 0);
        w.players[0] = Some(player);
    }

    #[test]
    fn use_inventory_starts_night_vision_scuba_and_a_holoduke() {
        let src = format!(
            "{}\nactor DukeNightVision : Actor\n{{\n  Inventory.Amount 1\n  Inventory.MaxAmount 1\n}}\n\
             actor DukeScubaGear : Actor\n{{\n  Inventory.Amount 1\n  Inventory.MaxAmount 1\n}}\n\
             actor DukeHoloduke : Actor\n{{\n  Inventory.Amount 1\n  Inventory.MaxAmount 1\n}}\n\
             actor DukeHolodukeDecoy\n{{\n  Health 1\n  Radius 16\n  Height 56\n  +SHOOTABLE\n  States\n  {{\n  Spawn:\n    PLAY A -1\n  }}\n}}\n",
            include_str!("../../../testdata/actors/doom_basic.txt")
        );
        let file = parse_decorate(&src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&file).unwrap());
        let states = Arc::new(StateTable::from_decorate(&file, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let nv = classes.find_name("DukeNightVision").unwrap();
        let scuba = classes.find_name("DukeScubaGear").unwrap();
        let holo = classes.find_name("DukeHoloduke").unwrap();
        let decoy = classes.find_name("DukeHolodukeDecoy").unwrap();
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        w.actors.get_mut(mo).unwrap().health = 100;
        let _ = give_inventory(&mut w, mo, nv, 1);
        let _ = give_inventory(&mut w, mo, scuba, 1);
        let _ = give_inventory(&mut w, mo, holo, 1);
        let mut player = w.players[0].take().unwrap();
        player.cmd.buttons = Buttons(Buttons::INVUSE);
        player_think(&mut w, &mut player);
        assert!(player.infrared);
        assert_eq!(player.infrared_tics, crate::player::INFRARED_TICS);
        assert_eq!(crate::inventory::count_inventory(&w, mo, nv), 0);
        player.cmd.buttons = Buttons(0);
        player_think(&mut w, &mut player);
        player.cmd.buttons = Buttons(Buttons::INVUSE);
        player_think(&mut w, &mut player);
        assert!(player.radsuit);
        assert_eq!(player.radsuit_tics, crate::player::RADSUIT_TICS);
        assert!(player.air_finished > 0);
        assert_eq!(crate::inventory::count_inventory(&w, mo, scuba), 0);
        player.cmd.buttons = Buttons(0);
        player_think(&mut w, &mut player);
        player.cmd.buttons = Buttons(Buttons::INVUSE);
        player_think(&mut w, &mut player);
        assert_eq!(crate::inventory::count_inventory(&w, mo, holo), 0);
        assert!(
            w.thinkers
                .default
                .iter()
                .any(|&id| { w.actors.get(id).is_some_and(|a| a.class == decoy) }),
            "holoduke decoy"
        );
        w.players[0] = Some(player);
    }

    #[test]
    fn change_camera_reverts_on_move() {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let file = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&file).unwrap());
        let states = Arc::new(StateTable::from_decorate(&file, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let cam = w
            .spawn_at(
                classes
                    .find_name("DoomPlayer")
                    .unwrap_or_else(|| w.players[0].as_ref().map(|p| p.cls).expect("player class")),
                DVec3::new(200.0, 64.0, 0.0),
                Angle::ZERO,
            )
            .unwrap();
        let mut player = w.players[0].take().unwrap();
        player.camera = Some(cam);
        player.camera_revert = true;
        player.cmd.forward_move = 50 * 256;
        player_think(&mut w, &mut player);
        assert!(player.camera.is_none());
        assert!(!player.camera_revert);
        w.players[0] = Some(player);
    }

    #[test]
    fn duke_crouch_lowers_height_and_stands_back_up() {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let extra = r#"
actor DukePlayer : DoomPlayer
{
  Height 40
  ViewHeight 36
}
"#;
        let file = parse_decorate(&format!("{src}\n{extra}")).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&file).unwrap());
        let states = Arc::new(StateTable::from_decorate(&file, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let duke = classes.find_name("DukePlayer").unwrap();
        let mut player = w.players[0].take().unwrap();
        let mo = player.mo.unwrap();
        player.cls = duke;
        player.on_ground = true;
        if let Some(a) = w.actors.get_mut(mo) {
            a.class = duke;
            a.height = 40.0;
        }
        player.cmd.up_move = -1;
        player_think(&mut w, &mut player);
        assert!(player.crouching);
        assert!(
            w.actors.get(mo).unwrap().height < 20.0,
            "crouch height {}",
            w.actors.get(mo).unwrap().height
        );
        assert!(
            player.view_height < 20.0,
            "crouch view {}",
            player.view_height
        );
        player.cmd.up_move = 0;
        player_think(&mut w, &mut player);
        assert!(!player.crouching);
        assert!(
            (w.actors.get(mo).unwrap().height - 40.0).abs() < 0.1,
            "stand height {}",
            w.actors.get(mo).unwrap().height
        );
        w.players[0] = Some(player);
    }

    #[test]
    fn duke_crouch_fits_a_low_ceiling() {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let extra = r#"
actor DukePlayer : DoomPlayer
{
  Height 40
  ViewHeight 36
}
"#;
        let file = parse_decorate(&format!("{src}\n{extra}")).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&file).unwrap());
        let states = Arc::new(StateTable::from_decorate(&file, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 32.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let duke = classes.find_name("DukePlayer").unwrap();
        let mut player = w.players[0].take().unwrap();
        let mo = player.mo.unwrap();
        player.cls = duke;
        player.on_ground = true;
        if let Some(a) = w.actors.get_mut(mo) {
            a.class = duke;
            a.height = 40.0;
        }
        player_think(&mut w, &mut player);
        assert!(player.crouching);
        assert!(w.actors.get(mo).unwrap().height < 20.0);
        let a = w.actors.get(mo).unwrap();
        let dest = DVec3::new(a.pos.x() + 16.0, a.pos.y(), a.pos.z());
        assert!(crate::physics::try_move(&mut w, mo, dest));
        w.players[0] = Some(player);
    }

    #[test]
    fn duke_long_fall_takes_half_health() {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let extra = "actor DukePlayer : DoomPlayer { }\n";
        let file = parse_decorate(&format!("{src}\n{extra}")).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&file).unwrap());
        let states = Arc::new(StateTable::from_decorate(&file, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let duke = classes.find_name("DukePlayer").unwrap();
        let mut player = w.players[0].take().unwrap();
        let mo = player.mo.unwrap();
        player.cls = duke;
        player.on_ground = true;
        player.falling_tics = DUKE_FALL_HURT;
        if let Some(a) = w.actors.get_mut(mo) {
            a.class = duke;
            a.health = 100;
            a.flags |= crate::class::FLAG_SHOOTABLE;
        }
        player_think(&mut w, &mut player);
        assert_eq!(player.falling_tics, 0);
        assert_eq!(w.actors.get(mo).unwrap().health, 50);
        w.players[0] = Some(player);
    }

    #[test]
    fn duke_air_coast_keeps_xy_speed() {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let extra = "actor DukePlayer : DoomPlayer { JumpZ 10 Player.ForwardMove 1.25 }\n";
        let file = parse_decorate(&format!("{src}\n{extra}")).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&file).unwrap());
        let states = Arc::new(StateTable::from_decorate(&file, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let duke = classes.find_name("DukePlayer").unwrap();
        let mut player = w.players[0].take().unwrap();
        let mo = player.mo.unwrap();
        player.cls = duke;
        player.on_ground = false;
        player.cmd = TicCmd::default();
        if let Some(a) = w.actors.get_mut(mo) {
            a.class = duke;
            a.pos = DVec3::new(a.pos.x(), a.pos.y(), a.pos.z() + 24.0);
            a.vel = DVec3::new(8.0, 0.0, 2.0);
        }
        player_think(&mut w, &mut player);
        let v = w.actors.get(mo).unwrap().vel;
        assert!(
            v.x() > 7.0,
            "airborne coast should keep XY speed, got {v:?}"
        );
        w.players[0] = Some(player);
    }

    #[test]
    fn duke_air_hold_steers_without_replacing_xy() {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let extra = "actor DukePlayer : DoomPlayer { JumpZ 10 Player.ForwardMove 1.25 }\n";
        let file = parse_decorate(&format!("{src}\n{extra}")).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&file).unwrap());
        let states = Arc::new(StateTable::from_decorate(&file, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let duke = classes.find_name("DukePlayer").unwrap();
        let mut player = w.players[0].take().unwrap();
        let mo = player.mo.unwrap();
        player.cls = duke;
        player.on_ground = false;
        player.cmd.forward_move = 50 * 256;
        if let Some(a) = w.actors.get_mut(mo) {
            a.class = duke;
            a.angle = Angle::from_degrees(0.0);
            a.pos = DVec3::new(a.pos.x(), a.pos.y(), a.pos.z() + 24.0);
            a.vel = DVec3::new(8.0, 0.0, 2.0);
        }
        player_think(&mut w, &mut player);
        let v = w.actors.get(mo).unwrap().vel;
        assert!(
            v.x() > 8.0,
            "airborne hold should add to XY speed, got {v:?}"
        );
        w.players[0] = Some(player);
    }

    #[test]
    fn blocked_move_plays_noway() {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let file = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&file).unwrap());
        let states = Arc::new(StateTable::from_decorate(&file, &classes));
        // Narrow room: start near the west wall and run into it.
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(20.0, 64.0, 180)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let mut player = w.players[0].take().unwrap();
        let mo = player.mo.unwrap();
        if let Some(a) = w.actors.get_mut(mo) {
            a.angle = Angle::from_degrees(180.0);
        }
        let _ = w.drain_sound_events();
        player.cmd.forward_move = 50 * 256;
        let mut sounds = Vec::new();
        for _ in 0..8 {
            player_think(&mut w, &mut player);
            sounds.extend(w.drain_sound_events().into_iter().map(|e| e.name));
            if sounds.iter().any(|n| n == "*noway") {
                break;
            }
        }
        w.players[0] = Some(player);
        assert!(
            sounds.iter().any(|n| n == "*noway"),
            "blocked walk should play *noway, got {sounds:?}"
        );
    }

    #[test]
    fn float_strafe_right_faces_east_when_looking_north() {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let file = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&file).unwrap());
        let states = Arc::new(StateTable::from_decorate(&file, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 512.0, 512.0), 0.0, 128.0)
            .player_start(256.0, 256.0, 90)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, classes, states).unwrap();
        let mut player = w.players[0].take().unwrap();
        let mo = player.mo.unwrap();
        if let Some(a) = w.actors.get_mut(mo) {
            a.angle = Angle::from_degrees(90.0);
            a.vel = DVec3::ZERO;
        }
        player.cmd.side_move = 24 * 256; // strafe right
        player_think(&mut w, &mut player);
        let a = w.actors.get(mo).unwrap();
        assert!(
            a.vel.x() > 0.5,
            "strafe right while facing north should push +X, vel={:?}",
            a.vel
        );
        assert!(
            a.vel.y().abs() < 0.25,
            "strafe right while facing north should not push Y, vel={:?}",
            a.vel
        );
        w.players[0] = Some(player);
    }

    #[test]
    fn vanilla_fixed_thrust_matches_doom_scale() {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let file = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&file).unwrap());
        let states = Arc::new(StateTable::from_decorate(&file, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 512.0, 512.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, classes, states).unwrap();
        w.set_vanilla_demo_mode(true);
        let mut player = w.players[0].take().unwrap();
        let mo = player.mo.unwrap();
        if let Some(a) = w.actors.get_mut(mo) {
            a.angle = Angle::from_degrees(0.0);
            a.vel = DVec3::ZERO;
        }
        player.cmd.forward_move = 50 * 256; // demo byte 50 << 8
        player_think(&mut w, &mut player);
        let a = w.actors.get(mo).unwrap();
        // Chocolate: signed-char move 50 → FixedMul(50*2048, FRACUNIT) = 1.5625.
        // f64 path can land a few ulps off exact 1.5625.
        assert!(
            (a.vel.x() - 1.5625).abs() < 1e-4,
            "vel.x={} (expected 1.5625)",
            a.vel.x()
        );
        assert!(a.vel.y().abs() < 1e-3, "vel.y={}", a.vel.y());
        // Position unchanged until xy_movement.
        assert!((a.pos.x() - 128.0).abs() < 1e-6);
        w.players[0] = Some(player);
        crate::physics::xy_movement(&mut w, mo);
        let a = w.actors.get(mo).unwrap();
        assert!(a.pos.x() > 128.0, "x={}", a.pos.x());
    }

    #[test]
    fn use_is_edge_triggered_not_held() {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let file = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&file).unwrap());
        let states = Arc::new(StateTable::from_decorate(&file, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 8.0)
            .player_start(128.0, 32.0, 90)
            .build_udmf("MAP01")
            .unwrap();
        map.sectors[0].tags.push(1);
        for line in &mut map.lines {
            let v1 = map.vertices[line.v1.raw() as usize].pos;
            let v2 = map.vertices[line.v2.raw() as usize].pos;
            if (v1.y() - 256.0).abs() < 1.0 && (v2.y() - 256.0).abs() < 1.0 {
                line.special = crate::specials::num::DOOR_RAISE as i16;
                line.args = [1, 16, 150, 0, 0];
                line.flags |= crate::world::ML_REPEAT_SPECIAL as i16;
                line.activation = room_map::spac::USE;
                break;
            }
        }
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, classes, states).unwrap();
        let mut player = w.players[0].take().unwrap();
        let mo = player.mo.unwrap();
        if let Some(a) = w.actors.get_mut(mo) {
            a.pos = DVec3::new(128.0, 200.0, 0.0);
            a.angle = Angle::from_degrees(90.0);
        }
        player.cmd.buttons = Buttons(Buttons::USE);
        player_think(&mut w, &mut player);
        assert_eq!(w.doors.len(), 1);
        assert_eq!(w.doors[0].direction, 1);
        // Still holding USE — must not reverse the door this tic.
        player.cmd.buttons = Buttons(Buttons::USE);
        player_think(&mut w, &mut player);
        assert_eq!(w.doors.len(), 1);
        assert_eq!(w.doors[0].direction, 1, "held USE must not re-trigger");
        // Re-use reverse while rising is covered by `specials` door tests;
        // this case only asserts the edge-trigger (no reverse while held).
        w.players[0] = Some(player);
    }

    #[test]
    fn failed_use_plays_noway() {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let file = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&file).unwrap());
        let states = Arc::new(StateTable::from_decorate(&file, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let mut player = w.players[0].take().unwrap();
        let _ = w.drain_sound_events();
        player.cmd.buttons = Buttons(Buttons::USE);
        player_think(&mut w, &mut player);
        w.players[0] = Some(player);
        let sounds: Vec<_> = w.drain_sound_events().into_iter().map(|e| e.name).collect();
        assert!(
            sounds.iter().any(|n| n == "*noway"),
            "empty use should play *noway, got {sounds:?}"
        );
    }

    #[test]
    fn moving_enters_see_run_sprite_cycle() {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let file = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&file).unwrap());
        let states = Arc::new(StateTable::from_decorate(&file, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 512.0, 512.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, classes, states).unwrap();
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        let spawn = w.actors.get(mo).unwrap().state;
        {
            let mut player = w.players[0].take().unwrap();
            player.cmd.forward_move = 50 << 8;
            player_think(&mut w, &mut player);
            w.players[0] = Some(player);
        }
        let a = w.actors.get(mo).unwrap();
        assert_ne!(a.state, spawn, "moving should leave Spawn for See");
        assert!(a.sprite.eq_ignore_ascii_case("PLAY"));
        assert!(a.frame <= 3);
    }

    #[test]
    fn movement_sets_weapon_bob_amplitude() {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let file = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&file).unwrap());
        let states = Arc::new(StateTable::from_decorate(&file, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let pistol = classes.find_name("Pistol").unwrap();
        let mut player = w.players[0].take().unwrap();
        player.ready_weapon = Some(pistol);
        player.pspr_y = crate::player::WEAPON_TOP;
        player.pspr_state = None;
        player.cmd.buttons = Buttons(0);
        player_think(&mut w, &mut player); // Ready
        player.cmd.forward_move = 50 * 256; // Doom run-scale ticcmd units
        for t in 1..12 {
            w.level.maptime = t;
            player_think(&mut w, &mut player);
        }
        assert!(player.bob > 0.0, "bob={}", player.bob);
        assert!(
            player.pspr_bob_x != 0.0 || player.pspr_bob_y != 0.0,
            "Ready must write sway"
        );
        w.players[0] = Some(player);
        let hud = w.player_hud(0).unwrap();
        assert!(hud.bob_x != 0.0 || hud.bob_y != 0.0);
    }

    #[test]
    fn weapon_bob_sway_freezes_while_firing() {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let file = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&file).unwrap());
        let states = Arc::new(StateTable::from_decorate(&file, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 512.0, 512.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let pistol = classes.find_name("Pistol").unwrap();
        let mut player = w.players[0].take().unwrap();
        player.ready_weapon = Some(pistol);
        player.pspr_y = crate::player::WEAPON_TOP;
        player.pspr_state = None;
        player.cmd.forward_move = 50 * 256;
        player.cmd.buttons = Buttons(0);
        for t in 0..16 {
            w.level.maptime = t;
            player_think(&mut w, &mut player);
        }
        let frozen_x = player.pspr_bob_x;
        let frozen_y = player.pspr_bob_y;
        assert!(frozen_x != 0.0 || frozen_y != 0.0);
        // Enter Fire and keep walking — sway must not update (W8.3).
        player.attack_down = false;
        player.cmd.buttons = Buttons(Buttons::ATTACK);
        player_think(&mut w, &mut player);
        player.attack_down = true;
        player.cmd.buttons = Buttons(0);
        for t in 16..32 {
            w.level.maptime = t;
            player.cmd.forward_move = 50 * 256;
            player_think(&mut w, &mut player);
            assert_eq!(
                (player.pspr_bob_x, player.pspr_bob_y),
                (frozen_x, frozen_y),
                "sway must freeze outside A_WeaponReady (maptime={t})"
            );
        }
        w.players[0] = Some(player);
    }

    #[test]
    fn psprite_scalelight_matches_doom_table_ends() {
        // Bright sector + no extralight → colormap 0 → 1.0
        assert!((crate::psprite_scalelight(255, 0) - 1.0).abs() < 1e-5);
        // Pitch black → darkest colormap row → ~0.03
        let dark = crate::psprite_scalelight(0, 0);
        assert!(dark < 0.1, "dark={dark}");
        // extralight bumps lightnum (A_Light1 in a mid sector).
        assert!(crate::psprite_scalelight(128, 1) > crate::psprite_scalelight(128, 0));
    }

    #[test]
    fn pistol_fire_advances_psprite_frames() {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let file = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&file).unwrap());
        let states = Arc::new(StateTable::from_decorate(&file, &classes));
        let pistol = classes.find_name("Pistol").unwrap();
        let fire = states.find_label(pistol, "Fire").unwrap();
        let fa = states.get(fire).unwrap();
        assert_eq!((fa.sprite.as_str(), fa.frame, fa.tics), ("PISG", 0, 4));
        let fb = states.get(fa.next.expect("Fire A→B")).unwrap();
        assert_eq!((fb.sprite.as_str(), fb.frame, fb.tics), ("PISG", 1, 6));
        assert_eq!(fb.action.as_deref(), Some("A_FirePistol"));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let mut player = w.players[0].take().unwrap();
        player.ready_weapon = Some(pistol);
        player.pspr_state = None;
        // Skip the Select raise (W13 parks at WEAPONBOTTOM on spawn).
        player.pspr_y = crate::player::WEAPON_TOP;
        // Idle think → Ready (PISGA0).
        player.cmd.buttons = Buttons(0);
        player_think(&mut w, &mut player);
        w.players[0] = Some(player.clone());
        assert_eq!(w.player_hud(0).unwrap().weapon_patch, "PISGA0");
        // Attack edge → Fire A.
        player = w.players[0].take().unwrap();
        player.attack_down = false;
        player.cmd.buttons = Buttons(Buttons::ATTACK);
        player_think(&mut w, &mut player);
        let sid = player.pspr_state.unwrap();
        let st = w.states.get(sid).unwrap();
        assert_eq!(st.sprite.to_ascii_uppercase(), "PISG");
        assert_eq!(st.frame, 0);
        // Advance through Fire A's 4 tics into Fire B (A_FirePistol).
        player.cmd.buttons = Buttons(0);
        player.attack_down = true;
        for _ in 0..4 {
            player_think(&mut w, &mut player);
        }
        let sid = player.pspr_state.unwrap();
        let st = w.states.get(sid).unwrap();
        assert_eq!(st.frame, 1, "expected PISG B after wind-up");
        w.players[0] = Some(player);
        assert_eq!(w.player_hud(0).unwrap().weapon_patch, "PISGB0");
    }

    #[test]
    fn pistol_fire_raises_flash_psprite() {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let file = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&file).unwrap());
        let states = Arc::new(StateTable::from_decorate(&file, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let pistol = classes.find_name("Pistol").unwrap();
        let mut player = w.players[0].take().unwrap();
        player.ready_weapon = Some(pistol);
        player.pspr_state = None;
        player.pspr_y = crate::player::WEAPON_TOP;
        player.attack_down = false;
        player.cmd.buttons = Buttons(0);
        player_think(&mut w, &mut player); // enter Ready
        player.attack_down = false;
        player.cmd.buttons = Buttons(Buttons::ATTACK);
        player_think(&mut w, &mut player);
        // Wind up to A_FirePistol (Fire B) — Doom A=4 then B.
        player.cmd.buttons = Buttons(0);
        player.attack_down = true;
        for _ in 0..4 {
            player_think(&mut w, &mut player);
        }
        assert!(
            player.pspr_flash_state.is_some(),
            "flash should start with fire"
        );
        assert_eq!(player.extralight, 1, "PISF A_Light1");
        w.players[0] = Some(player);
        assert_eq!(w.player_hud(0).unwrap().flash_patch, "PISFA0");
    }

    #[test]
    fn mod_weapon_a_fire_projectile_uses_its_arguments() {
        let base = include_str!("../../../testdata/actors/doom_basic.txt");
        let src = format!(
            "{base}\n{}",
            r#"actor ModLauncher : RocketLauncher
{
  States
  {
  Fire:
    MISG B 1 A_FireProjectile("Rocket", 10, 1, 8, 4, FPF_NOAUTOAIM)
    MISG B 1 A_FireCustomMissile("PlasmaBall", 0, 1, 0, 0, 0, -20)
    Goto Ready
  }
}"#
        );
        let file = parse_decorate(&src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&file).unwrap());
        let states = Arc::new(StateTable::from_decorate(&file, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 1024.0, 1024.0), 0.0, 256.0)
            .player_start(64.0, 512.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let wpn = classes.find_name("ModLauncher").unwrap();
        let ammo = classes.find_name("RocketAmmo").unwrap();
        let rocket = classes.find_name("Rocket").unwrap();
        let plasma = classes.find_name("PlasmaBall").unwrap();
        let mo = w.players[0].as_ref().and_then(|p| p.mo).unwrap();
        let _ = give_inventory(&mut w, mo, wpn, 1);
        let _ = give_inventory(&mut w, mo, ammo, 10);
        if let Some(a) = w.actors.get_mut(mo) {
            a.angle = room_core::Angle::from_degrees(0.0);
        }
        let mut player = w.players[0].take().unwrap();
        player.ready_weapon = Some(wpn);
        player.pending_weapon = None;
        player.pspr_state = None;
        player.pspr_y = crate::player::WEAPON_TOP;
        player.attack_down = false;
        player.cmd.buttons = Buttons(Buttons::ATTACK);
        player_think(&mut w, &mut player);
        player_think(&mut w, &mut player);
        w.players[0] = Some(player);
        let find = |cid| {
            w.thinkers
                .default
                .iter()
                .find_map(|&id| w.actors.get(id).filter(|a| a.class == cid))
                .cloned()
        };
        let feet = w.actors.get(mo).unwrap().pos.z();
        let r = find(rocket).expect("A_FireProjectile spawned a rocket");
        assert!(
            (r.pos.z() - (feet + 36.0)).abs() < 1e-6,
            "32 + spawnheight 4"
        );
        assert!(r.pos.y() < 512.0, "spawnofs_xy 8 → right of the player");
        assert!((r.angle.degrees() - 10.0).abs() < 1e-6, "angle 10");
        assert!(
            r.vel.y() > 0.0 && r.vel.z().abs() < 1e-9,
            "turned left, level"
        );
        let p = find(plasma).expect("A_FireCustomMissile spawned a plasma ball");
        assert!(p.vel.z() > 0.0, "pitch -20 aims up (GZDoom sign)");
    }

    #[test]
    fn chaingun_fires_hitscan_on_first_fire_tic() {
        // A_FireCGun runs on *enter* of Fire A — not after the 4-tic sprite ends.
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let file = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&file).unwrap());
        let states = Arc::new(StateTable::from_decorate(&file, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 1024.0, 256.0), 0.0, 128.0)
            .player_start(64.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let cg = classes.find_name("Chaingun").unwrap();
        let clip = classes.find_name("Clip").unwrap();
        let mo = w.players[0].as_ref().and_then(|p| p.mo).unwrap();
        let _ = give_inventory(&mut w, mo, cg, 1);
        let _ = give_inventory(&mut w, mo, clip, 200);
        let mut player = w.players[0].take().unwrap();
        player.ready_weapon = Some(cg);
        player.pending_weapon = None;
        player.pspr_state = None;
        player.pspr_y = crate::player::WEAPON_TOP;
        player.attack_down = false;
        player.cmd.buttons = Buttons(Buttons::ATTACK);
        let before = w.decal_events.len();
        player_think(&mut w, &mut player);
        let after = w.decal_events.len();
        assert!(
            after > before,
            "chaingun must hitscan on the same tic it enters Fire (decals before={before} after={after})"
        );
        let sid = player.pspr_state.unwrap();
        let st = w.states.get(sid).unwrap();
        assert_eq!(st.sprite.to_ascii_uppercase(), "CHGG");
        assert_eq!(st.frame, 0, "still on Fire A after enter+fire");
        w.players[0] = Some(player);
    }

    #[test]
    fn refire_empty_ammo_autoswitches_weapon() {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let file = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&file).unwrap());
        let states = Arc::new(StateTable::from_decorate(&file, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let pistol = classes.find_name("Pistol").unwrap();
        let fist = classes.find_name("Fist").unwrap();
        let clip = classes.find_name("Clip").unwrap();
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        while w.inventory.count(mo, clip) > 0 {
            let _ = w.inventory.take(mo, clip, 1);
        }
        let mut player = w.players[0].take().unwrap();
        player.ready_weapon = Some(pistol);
        player.pending_weapon = None;
        player.pspr_y = crate::player::WEAPON_TOP;
        // Sit on the frame before A_ReFire so the next advance enters it.
        let fire = w.states.find_label(pistol, "Fire").unwrap();
        let mut sid = Some(fire);
        let mut before_refire = None;
        for _ in 0..16 {
            let Some(cur) = sid else {
                break;
            };
            let st = w.states.get(cur).unwrap();
            if st
                .next
                .and_then(|n| w.states.get(n))
                .and_then(|n| n.action.as_deref())
                == Some("A_ReFire")
            {
                before_refire = Some(cur);
                break;
            }
            sid = st.next;
        }
        assert!(
            before_refire.is_some(),
            "pistol Fire should include A_ReFire"
        );
        player.pspr_state = before_refire;
        player.pspr_tics = 1;
        player.attack_down = true;
        player.cmd.buttons = Buttons(Buttons::ATTACK);
        player_think(&mut w, &mut player);
        assert_eq!(
            player.pending_weapon,
            Some(fist),
            "empty pistol mid-refire should queue Fist"
        );
        w.players[0] = Some(player);
    }

    #[test]
    fn refire_restarts_fire_while_attack_held() {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let file = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&file).unwrap());
        let states = Arc::new(StateTable::from_decorate(&file, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let pistol = classes.find_name("Pistol").unwrap();
        let clip = classes.find_name("Clip").unwrap();
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        let mut player = w.players[0].take().unwrap();
        player.ready_weapon = Some(pistol);
        player.pspr_state = None;
        player.pspr_y = crate::player::WEAPON_TOP;
        player.cmd.buttons = Buttons(0);
        player_think(&mut w, &mut player); // enter Ready
        let before = w.inventory.count(mo, clip);
        // Hold attack long enough to reach A_ReFire and restart Fire.
        player.attack_down = false;
        player.cmd.buttons = Buttons(Buttons::ATTACK);
        for _ in 0..30 {
            player_think(&mut w, &mut player);
            player.attack_down = true;
            player.cmd.buttons = Buttons(Buttons::ATTACK);
        }
        w.players[0] = Some(player);
        let spent = before - w.inventory.count(mo, clip);
        assert!(
            spent >= 2,
            "held attack should refire (spent {spent}, before {before})"
        );
    }

    #[test]
    fn firing_pistol_consumes_clip() {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let file = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&file).unwrap());
        let states = Arc::new(StateTable::from_decorate(&file, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let pistol = classes.find_name("Pistol").unwrap();
        let clip = classes.find_name("Clip").unwrap();
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        let before = w.inventory.count(mo, clip);
        assert!(before >= 1);
        assert_eq!(classes.get(pistol).unwrap().ammo_use, 1);
        assert_eq!(classes.get(pistol).unwrap().ammo_type, "Clip");
        if let Some(p) = w.players[0].as_mut() {
            p.ready_weapon = Some(pistol);
            p.pspr_state = None;
            p.pspr_y = crate::player::WEAPON_TOP;
            p.cmd.buttons = Buttons(0);
            p.attack_down = false;
            let mut player = p.clone();
            player_think(&mut w, &mut player); // enter Ready
            player.cmd.buttons = Buttons(Buttons::ATTACK);
            player.attack_down = false;
            // Ammo depletes in `A_FirePistol` (Fire A 4 then B), not on Fire entry.
            for _ in 0..16 {
                player_think(&mut w, &mut player);
                player.cmd.buttons = Buttons(Buttons::ATTACK);
                player.attack_down = true;
                if w.inventory.count(mo, clip) < before {
                    break;
                }
            }
            assert_eq!(w.inventory.count(mo, clip), before - 1);
            // Dry fire when empty.
            while w.inventory.count(mo, clip) > 0 {
                let _ = w.inventory.take(mo, clip, 1);
            }
            player.attack_down = false;
            player.cmd.buttons = Buttons(Buttons::ATTACK);
            player_think(&mut w, &mut player);
            assert_eq!(w.inventory.count(mo, clip), 0);
        }
    }

    #[test]
    fn weapon_slot_2_selects_pistol() {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let file = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&file).unwrap());
        let states = Arc::new(StateTable::from_decorate(&file, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let fist = classes.find_name("Fist").unwrap();
        let pistol = classes.find_name("Pistol").unwrap();
        let shotgun = classes.find_name("Shotgun").unwrap();
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        let _ = give_inventory(&mut w, mo, shotgun, 1);
        // Prefer fist first.
        if let Some(p) = w.players[0].as_mut() {
            p.ready_weapon = Some(fist);
            p.pspr_y = crate::player::WEAPON_TOP;
            p.cmd.weapon_select = 2;
            let mut player = p.clone();
            player_think(&mut w, &mut player);
            assert_eq!(player.pending_weapon, Some(pistol));
            player.cmd.weapon_select = 0;
            for _ in 0..40 {
                player_think(&mut w, &mut player);
            }
            assert_eq!(player.ready_weapon, Some(pistol));
            // Slot 3 → shotgun.
            player.cmd.weapon_select = 3;
            player_think(&mut w, &mut player);
            player.cmd.weapon_select = 0;
            for _ in 0..40 {
                player_think(&mut w, &mut player);
            }
            assert_eq!(player.ready_weapon, Some(shotgun));
        }
    }

    #[test]
    fn chainsaw_ready_plays_idle_sound() {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let file = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&file).unwrap());
        let states = Arc::new(StateTable::from_decorate(&file, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let saw = classes.find_name("Chainsaw").unwrap();
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        let _ = give_inventory(&mut w, mo, saw, 1);
        let mut player = w.players[0].take().unwrap();
        player.ready_weapon = Some(saw);
        player.pspr_y = crate::player::WEAPON_TOP;
        player.pspr_state = None;
        w.level.maptime = 4;
        player_think(&mut w, &mut player);
        assert!(
            w.sound_events.iter().any(|e| e.name == "weapons/sawidle"),
            "events={:?}",
            w.sound_events
        );
        w.players[0] = Some(player);
    }

    #[test]
    fn standing_still_plays_an_idle_taunt() {
        let ir = parse_decorate(
            r#"
actor PlayerPawn : Actor 0
{
  Health 100
  IdleTaunt "duke/crack"
  States { Spawn: PLAY A -1 Stop }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, classes, states).unwrap();
        let mut player = w.players[0].take().unwrap();
        player.player_state = PlayerState::Live;
        player.on_ground = true;
        player.cmd = TicCmd::default();
        let _ = w.drain_sound_events();
        for _ in 0..IDLE_TAUNT_TICS {
            player_think(&mut w, &mut player);
            player.on_ground = true;
        }
        let sounds: Vec<_> = w.drain_sound_events().into_iter().map(|e| e.name).collect();
        assert!(
            sounds.iter().any(|n| n == "duke/crack"),
            "expected idle taunt, got {sounds:?}"
        );
        w.players[0] = Some(player);
    }

    #[test]
    fn pitch_cmd_updates_actor_pitch() {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let file = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&file).unwrap());
        let states = Arc::new(StateTable::from_decorate(&file, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, classes, states).unwrap();
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        let mut player = w.players[0].take().unwrap();
        player.cmd.pitch = 2048; // ~11.25°
        player_think(&mut w, &mut player);
        let pit = w.actors.get(mo).unwrap().pitch.degrees();
        assert!(pit > 10.0 && pit < 13.0, "pitch={pit}");
        w.players[0] = Some(player);
    }

    #[test]
    fn empty_pistol_switches_to_fist() {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let file = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&file).unwrap());
        let states = Arc::new(StateTable::from_decorate(&file, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let pistol = classes.find_name("Pistol").unwrap();
        let fist = classes.find_name("Fist").unwrap();
        let clip = classes.find_name("Clip").unwrap();
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        while w.inventory.count(mo, clip) > 0 {
            let _ = w.inventory.take(mo, clip, 1);
        }
        let mut player = w.players[0].take().unwrap();
        player.ready_weapon = Some(pistol);
        player.pspr_y = crate::player::WEAPON_TOP;
        player.pspr_state = None;
        player.attack_down = false;
        player.cmd.buttons = Buttons(Buttons::ATTACK);
        player_think(&mut w, &mut player);
        assert_eq!(player.pending_weapon, Some(fist));
        w.players[0] = Some(player);
    }

    #[test]
    fn ssg_check_reload_aborts_when_empty() {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let file = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&file).unwrap());
        let states = Arc::new(StateTable::from_decorate(&file, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let ssg = classes.find_name("SuperShotgun").unwrap();
        let shell = classes.find_name("Shell").unwrap();
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        let _ = give_inventory(&mut w, mo, ssg, 1);
        // Drain shells so AmmoUse 2 cannot be satisfied.
        while w.inventory.count(mo, shell) > 0 {
            let _ = w.inventory.take(mo, shell, 1);
        }
        let mut player = w.players[0].take().unwrap();
        player.ready_weapon = Some(ssg);
        player.pspr_y = crate::player::WEAPON_TOP;
        player.pspr_state = None;
        // Fire: A3, A7 FireShotgun2, B7, C7 CheckReload → offset 3.
        let check = w.states.find_label(ssg, "Fire").and_then(|fire| {
            let mut sid = fire;
            for _ in 0..3 {
                sid = w.states.get(sid)?.next?;
            }
            Some(sid)
        });
        let Some(check) = check else {
            panic!("SSG Fire+3 CheckReload");
        };
        assert_eq!(
            w.states.get(check).and_then(|s| s.action.as_deref()),
            Some("A_CheckReload")
        );
        set_psprite(&mut w, &mut player, mo, check);
        // W15: `A_CheckReload` → `P_CheckAmmo` (pending switch), not jump to Ready.
        assert!(
            player.pending_weapon.is_some() || player.ready_weapon != Some(ssg),
            "empty SSG should auto-switch via CheckReload"
        );
        w.players[0] = Some(player);
    }

    #[test]
    fn weapon_switch_lowers_then_raises() {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let file = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&file).unwrap());
        let states = Arc::new(StateTable::from_decorate(&file, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let fist = classes.find_name("Fist").unwrap();
        let pistol = classes.find_name("Pistol").unwrap();
        let mut player = w.players[0].take().unwrap();
        player.ready_weapon = Some(fist);
        player.pspr_y = crate::player::WEAPON_TOP;
        player.pspr_state = None;
        player.cmd.weapon_select = 2;
        player_think(&mut w, &mut player);
        assert!(player.pspr_y > crate::player::WEAPON_TOP || player.pending_weapon == Some(pistol));
        player.cmd.weapon_select = 0;
        let mut saw_bottom = false;
        for _ in 0..40 {
            player_think(&mut w, &mut player);
            if (player.pspr_y - crate::player::WEAPON_BOTTOM).abs() < 0.5 {
                saw_bottom = true;
            }
        }
        assert!(saw_bottom || player.ready_weapon == Some(pistol));
        assert_eq!(player.ready_weapon, Some(pistol));
        assert!((player.pspr_y - crate::player::WEAPON_TOP).abs() < 0.5);
        assert!(player.pending_weapon.is_none());
    }

    #[test]
    fn powerups_expire_after_tics() {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let file = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&file).unwrap());
        let states = Arc::new(StateTable::from_decorate(&file, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        if let Some(p) = w.players[0].as_mut() {
            p.invulnerable = true;
            p.invul_tics = 2;
            p.radsuit = true;
            p.radsuit_tics = 1;
            let mut player = p.clone();
            player_think(&mut w, &mut player);
            assert!(player.invulnerable);
            assert_eq!(player.invul_tics, 1);
            assert!(!player.radsuit);
            assert_eq!(player.radsuit_tics, 0);
            player_think(&mut w, &mut player);
            assert!(!player.invulnerable);
            assert_eq!(player.invul_tics, 0);
        }
    }

    #[test]
    fn jump_applies_class_jump_z() {
        let ir = parse_decorate(
            r#"
actor HighJumper : Actor
{
  JumpZ 12
  Height 56
  Radius 16
  States { Spawn: PLAY A -1 Stop }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let jumper = classes.find_name("HighJumper").unwrap();
        assert!((classes.get(jumper).unwrap().jump_z - 12.0).abs() < 1e-9);
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        // Retarget the pawn to HighJumper so JumpZ 12 applies.
        if let Some(a) = w.actors.get_mut(mo) {
            a.class = jumper;
            a.jump_z = classes.get(jumper).unwrap().jump_z;
        }
        if let Some(p) = w.players[0].as_mut() {
            p.on_ground = true;
            p.jump_down = false;
            p.cmd.buttons = Buttons(Buttons::JUMP);
            let mut player = p.clone();
            let _ = w.drain_sound_events();
            player_think(&mut w, &mut player);
            assert!(!player.on_ground);
            assert!(player.jump_down);
            assert!((w.actors.get(mo).unwrap().vel.z() - 12.0).abs() < 1e-9);
            assert!(
                w.drain_sound_events()
                    .iter()
                    .any(|e| e.name == "player/male/jump"),
                "jump should play player/male/jump"
            );
            // Hold jump — no second impulse.
            player_think(&mut w, &mut player);
            assert!((w.actors.get(mo).unwrap().vel.z() - 12.0).abs() < 1e-9);
        }
    }

    #[test]
    fn forward_move_scales_player_thrust() {
        let ir = parse_decorate(
            r#"
actor FastWalker : Actor
{
  Player.ForwardMove 2
  Height 56
  Radius 16
  States { Spawn: PLAY A -1 Stop }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 512.0, 512.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let fast = classes.find_name("FastWalker").unwrap();
        assert!((classes.get(fast).unwrap().forward_move - 2.0).abs() < 1e-9);
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        if let Some(a) = w.actors.get_mut(mo) {
            a.class = fast;
        }
        let x0 = w.actors.get(mo).unwrap().pos.x();
        if let Some(p) = w.players[0].as_mut() {
            p.cmd.forward_move = 50 * 256;
            p.cmd.side_move = 0;
            let mut player = p.clone();
            player_think(&mut w, &mut player);
        }
        let x1 = w.actors.get(mo).unwrap().pos.x();
        // 50*256/2048 = 6.25; scaled ×2 → 12.5.
        assert!(
            (x1 - x0 - 12.5).abs() < 0.5,
            "forward-move scale {x0} -> {x1}"
        );
    }

    #[test]
    fn mod_buttons_do_not_block_movement_and_stay_in_raw_cmd() {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let file = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&file).unwrap());
        let states = Arc::new(StateTable::from_decorate(&file, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        let x0 = w.actors.get(mo).unwrap().pos.x();
        let mut player = w.players[0].take().unwrap();
        player.cmd = TicCmd {
            forward_move: 25 * 256,
            buttons: Buttons(Buttons::MOD1),
            ..TicCmd::default()
        };
        player_think(&mut w, &mut player);
        let x1 = w.actors.get(mo).unwrap().pos.x();
        assert!(x1 > x0, "MOD1 must not block forward move");
        assert_eq!(player.raw_cmd.buttons.0 & Buttons::MOD1, Buttons::MOD1);
        w.players[0] = Some(player);
    }

    #[test]
    fn ssg_fire_includes_gh_reload_and_ij_flash() {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let file = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&file).unwrap());
        let states = Arc::new(StateTable::from_decorate(&file, &classes));
        let ssg = classes.find_name("SuperShotgun").unwrap();
        assert_eq!(classes.get(ssg).unwrap().selection_order, 400);
        let fire = states.find_label(ssg, "Fire").unwrap();
        let mut sid = Some(fire);
        let mut sprites = Vec::new();
        let mut tics_sum = 0i32;
        for _ in 0..32 {
            let Some(cur) = sid else {
                break;
            };
            let st = states.get(cur).unwrap();
            if st.action.as_deref() == Some("A_ReFire") {
                tics_sum += i32::from(st.tics);
                sprites.push((st.sprite.clone(), st.frame, st.tics));
                break;
            }
            if !st.sprite.is_empty() {
                sprites.push((st.sprite.clone(), st.frame, st.tics));
                tics_sum += i32::from(st.tics);
            }
            sid = st.next;
        }
        // Vanilla SSG Fire cycle = 62 tics through A_ReFire frame.
        assert_eq!(tics_sum, 62, "SSG fire tics={tics_sum} frames={sprites:?}");
        assert!(
            sprites.iter().any(|(s, f, _)| s.eq_ignore_ascii_case("SHT2") && *f == 6),
            "missing G frame: {sprites:?}"
        );
        assert!(
            sprites.iter().any(|(s, f, _)| s.eq_ignore_ascii_case("SHT2") && *f == 7),
            "missing H frame: {sprites:?}"
        );
        let flash = states.find_label(ssg, "Flash").unwrap();
        let fst = states.get(flash).unwrap();
        assert_eq!(fst.sprite.to_ascii_uppercase(), "SHT2");
        assert_eq!(fst.frame, 8, "flash starts at I"); // 'I' - 'A' = 8
    }

    #[test]
    fn chaingun_second_shot_uses_flash_b() {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let file = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&file).unwrap());
        let states = Arc::new(StateTable::from_decorate(&file, &classes));
        let cg = classes.find_name("Chaingun").unwrap();
        let flash0 = states.find_label(cg, "Flash").unwrap();
        let flash_b = room_core::StateId::from_raw(flash0.raw() + 1);
        let st_b = states.get(flash_b).unwrap();
        assert_eq!(st_b.sprite.to_ascii_uppercase(), "CHGF");
        assert_eq!(st_b.frame, 1, "flash offset 1 must be CHGF B");
        assert_eq!(st_b.tics, 5);
        assert_eq!(st_b.action.as_deref(), Some("A_Light2"));
        // LightDone terminator — no fallthrough into B from A.
        let st_a = states.get(flash0).unwrap();
        let ld = states.get(st_a.next.expect("Goto LightDone")).unwrap();
        assert_eq!(ld.action.as_deref(), Some("A_Light0"));

        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 1024.0, 256.0), 0.0, 128.0)
            .player_start(64.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), Arc::clone(&states)).unwrap();
        let clip = classes.find_name("Clip").unwrap();
        let mo = w.players[0].as_ref().and_then(|p| p.mo).unwrap();
        let _ = give_inventory(&mut w, mo, cg, 1);
        while w.inventory.count(mo, clip) > 0 {
            let _ = w.inventory.take(mo, clip, 1);
        }
        let _ = give_inventory(&mut w, mo, clip, 10);
        let mut player = w.players[0].take().unwrap();
        player.ready_weapon = Some(cg);
        player.pending_weapon = None;
        player.pspr_state = None;
        player.pspr_y = crate::player::WEAPON_TOP;
        player.attack_down = false;
        player.cmd.buttons = Buttons(0);
        player_think(&mut w, &mut player);
        // Hold fire through both Fire frames (A then B).
        player.cmd.buttons = Buttons(Buttons::ATTACK);
        for _ in 0..8 {
            player_think(&mut w, &mut player);
            if player.pspr_flash_state == Some(flash_b) {
                assert_eq!(player.extralight, 2, "CHGF B uses A_Light2");
                assert!(
                    player.pspr_flash_tics > 0,
                    "CHGF B must still be running"
                );
                w.players[0] = Some(player);
                return;
            }
        }
        panic!(
            "expected CHGF B flash; got {:?} extralight={}",
            player.pspr_flash_state, player.extralight
        );
    }

    #[test]
    fn chaingun_one_bullet_fires_once() {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let file = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&file).unwrap());
        let states = Arc::new(StateTable::from_decorate(&file, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 1024.0, 256.0), 0.0, 128.0)
            .player_start(64.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let cg = classes.find_name("Chaingun").unwrap();
        let clip = classes.find_name("Clip").unwrap();
        let mo = w.players[0].as_ref().and_then(|p| p.mo).unwrap();
        let _ = give_inventory(&mut w, mo, cg, 1);
        while w.inventory.count(mo, clip) > 0 {
            let _ = w.inventory.take(mo, clip, 1);
        }
        let _ = give_inventory(&mut w, mo, clip, 1);
        let mut player = w.players[0].take().unwrap();
        player.ready_weapon = Some(cg);
        player.pending_weapon = None;
        player.pspr_state = None;
        player.pspr_y = crate::player::WEAPON_TOP;
        player.cmd.buttons = Buttons(Buttons::ATTACK);
        // Hold fire for a full AB cycle + refire.
        for _ in 0..20 {
            player_think(&mut w, &mut player);
            player.cmd.buttons = Buttons(Buttons::ATTACK);
        }
        assert_eq!(
            w.inventory.count(mo, clip),
            0,
            "one bullet must be spent exactly once across two Fire frames"
        );
        w.players[0] = Some(player);
    }

    #[test]
    fn auto_switch_prefers_plasma_over_fist() {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let file = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&file).unwrap());
        assert!(
            classes.get(classes.find_name("PlasmaRifle").unwrap()).unwrap().selection_order
                < classes.get(classes.find_name("Fist").unwrap()).unwrap().selection_order
        );
        let states = Arc::new(StateTable::from_decorate(&file, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let plasma = classes.find_name("PlasmaRifle").unwrap();
        let fist = classes.find_name("Fist").unwrap();
        let pistol = classes.find_name("Pistol").unwrap();
        let cell = classes.find_name("Cell").unwrap();
        let clip = classes.find_name("Clip").unwrap();
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        let _ = give_inventory(&mut w, mo, plasma, 1);
        let _ = give_inventory(&mut w, mo, fist, 1);
        let _ = give_inventory(&mut w, mo, pistol, 1);
        let _ = give_inventory(&mut w, mo, cell, 40);
        while w.inventory.count(mo, clip) > 0 {
            let _ = w.inventory.take(mo, clip, 1);
        }
        let best = best_weapon_with_ammo(&w, mo, Some(pistol)).unwrap();
        assert_eq!(best, plasma, "SelectionOrder should prefer plasma");
    }

    /// Simulate a short mouse tap: attack held for `hold` tics, then released,
    /// then idle for `cooldown` so Fire/A_ReFire can finish.
    fn tap_fire(w: &mut World, player: &mut Player, hold: u32, cooldown: u32) {
        for _ in 0..hold {
            player.cmd.buttons = Buttons(Buttons::ATTACK);
            player_think(w, player);
        }
        player.cmd.buttons = Buttons(0);
        for _ in 0..cooldown {
            player_think(w, player);
        }
    }

    fn ready_with_weapon(
        w: &mut World,
        classes: &ClassRegistry,
        weapon: &str,
        ammo_name: &str,
        ammo_amt: i32,
    ) -> (ActorId, room_core::ClassId, room_core::ClassId) {
        let wpn = classes.find_name(weapon).unwrap();
        let ammo = classes.find_name(ammo_name).unwrap();
        let mo = w.players[0].as_ref().and_then(|p| p.mo).unwrap();
        let _ = give_inventory(w, mo, wpn, 1);
        while w.inventory.count(mo, ammo) > 0 {
            let _ = w.inventory.take(mo, ammo, 1);
        }
        let _ = give_inventory(w, mo, ammo, ammo_amt);
        let mut player = w.players[0].take().unwrap();
        player.ready_weapon = Some(wpn);
        player.pending_weapon = None;
        player.pspr_state = None;
        player.pspr_y = crate::player::WEAPON_TOP;
        player.attack_down = false;
        player.cmd.buttons = Buttons(0);
        // Enter Ready.
        player_think(w, &mut player);
        w.players[0] = Some(player);
        (mo, wpn, ammo)
    }

    #[test]
    fn pistol_tap_spends_exactly_one_bullet() {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let file = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&file).unwrap());
        let states = Arc::new(StateTable::from_decorate(&file, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let (mo, _, clip) = ready_with_weapon(&mut w, &classes, "Pistol", "Clip", 10);
        let before = w.inventory.count(mo, clip);
        let mut player = w.players[0].take().unwrap();
        // 3-tic hold matches desktop AttackLatch after the hold-tic fix.
        tap_fire(&mut w, &mut player, 3, 40);
        assert_eq!(
            w.inventory.count(mo, clip),
            before - 1,
            "pistol tap must spend exactly one Clip"
        );
        w.players[0] = Some(player);
    }

    #[test]
    fn plasma_tap_spends_exactly_one_cell() {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let file = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&file).unwrap());
        let states = Arc::new(StateTable::from_decorate(&file, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let (mo, _, cell) = ready_with_weapon(&mut w, &classes, "PlasmaRifle", "Cell", 40);
        let before = w.inventory.count(mo, cell);
        let mut player = w.players[0].take().unwrap();
        tap_fire(&mut w, &mut player, 3, 40);
        assert_eq!(
            w.inventory.count(mo, cell),
            before - 1,
            "plasma tap must spend exactly one Cell (no A_ReFire loop)"
        );
        w.players[0] = Some(player);
    }

    #[test]
    fn shotgun_tap_spends_exactly_one_shell() {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let file = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&file).unwrap());
        let states = Arc::new(StateTable::from_decorate(&file, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let (mo, _, shell) = ready_with_weapon(&mut w, &classes, "Shotgun", "Shell", 10);
        let before = w.inventory.count(mo, shell);
        let mut player = w.players[0].take().unwrap();
        tap_fire(&mut w, &mut player, 3, 50);
        assert_eq!(
            w.inventory.count(mo, shell),
            before - 1,
            "shotgun tap must spend exactly one Shell"
        );
        w.players[0] = Some(player);
    }

    #[test]
    fn chaingun_tap_spends_exactly_one_ab_pair() {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let file = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&file).unwrap());
        let states = Arc::new(StateTable::from_decorate(&file, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let (mo, _, clip) = ready_with_weapon(&mut w, &classes, "Chaingun", "Clip", 10);
        let before = w.inventory.count(mo, clip);
        let mut player = w.players[0].take().unwrap();
        tap_fire(&mut w, &mut player, 3, 30);
        // Vanilla: one Fire cycle is CHGG A + CHGG B (two A_FireCGun), then
        // A_ReFire — a tap must not loop into a second pair.
        assert_eq!(
            w.inventory.count(mo, clip),
            before - 2,
            "chaingun tap must spend exactly one A+B pair (2 Clip)"
        );
        w.players[0] = Some(player);
    }

    #[test]
    fn rocket_tap_spends_exactly_one_rocket() {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let file = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&file).unwrap());
        let states = Arc::new(StateTable::from_decorate(&file, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let (mo, _, ammo) =
            ready_with_weapon(&mut w, &classes, "RocketLauncher", "RocketAmmo", 5);
        let before = w.inventory.count(mo, ammo);
        let mut player = w.players[0].take().unwrap();
        tap_fire(&mut w, &mut player, 3, 40);
        assert_eq!(
            w.inventory.count(mo, ammo),
            before - 1,
            "rocket tap must spend exactly one RocketAmmo"
        );
        w.players[0] = Some(player);
    }

    #[test]
    fn plasma_held_fires_multiple_but_tap_does_not() {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let file = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&file).unwrap());
        let states = Arc::new(StateTable::from_decorate(&file, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let (mo, _, cell) = ready_with_weapon(&mut w, &classes, "PlasmaRifle", "Cell", 40);
        let mut player = w.players[0].take().unwrap();
        // Hold through several Fire cycles (3+20 per shot).
        for _ in 0..50 {
            player.cmd.buttons = Buttons(Buttons::ATTACK);
            player_think(&mut w, &mut player);
        }
        let spent = 40 - w.inventory.count(mo, cell);
        assert!(
            spent >= 2,
            "held plasma should fire repeatedly, spent={spent}"
        );
        w.players[0] = Some(player);
    }

    #[test]
    fn plasma_held_rof_is_vanilla_three_tics() {
        // Vanilla: PLSG A 3 A_FirePlasma → PLSG B 20 A_ReFire; while attack is
        // held, A_ReFire immediately restarts Fire (the 20-tic B frame is only
        // the release cooldown). Held ROF = 1 bolt / 3 tics ≈ 11.7 Hz.
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let file = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&file).unwrap());
        let states = Arc::new(StateTable::from_decorate(&file, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let (mo, _, cell) = ready_with_weapon(&mut w, &classes, "PlasmaRifle", "Cell", 40);
        let mut player = w.players[0].take().unwrap();
        let mut fire_tics = Vec::new();
        let mut prev = w.inventory.count(mo, cell);
        for t in 0..24u32 {
            player.cmd.buttons = Buttons(Buttons::ATTACK);
            player_think(&mut w, &mut player);
            w.level.maptime = w.level.maptime.saturating_add(1);
            let now = w.inventory.count(mo, cell);
            if now < prev {
                fire_tics.push(t);
                prev = now;
            }
        }
        assert!(
            fire_tics.len() >= 5,
            "expected several bolts in 24 tics, got {fire_tics:?}"
        );
        for w in fire_tics.windows(2) {
            assert_eq!(
                w[1] - w[0],
                3,
                "held plasma spacing must be 3 tics (vanilla), gaps={fire_tics:?}"
            );
        }
        w.players[0] = Some(player);
    }
}
