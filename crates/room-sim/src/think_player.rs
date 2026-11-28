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

/// GZDoom `P_CrouchMove`: holding move-down lowers the player to half height
/// over a few tics (eye height follows); letting go stands back up when the
/// ceiling allows. Flying and swimming use move-down to descend instead.
const CROUCH_STEP: f64 = 1.0 / 6.0;

fn apply_crouch(world: &mut World, player: &mut Player, mo: ActorId, cmd: &TicCmd) {
    if is_duke_player(world, player) {
        apply_duke_crouch(world, player, mo, cmd);
        return;
    }
    let Some(a) = world.actors.get(mo) else {
        return;
    };
    let swimming = crate::physics::actor_waterlevel(world, a) >= 2;
    let flying = player.fly_tics > 0 || a.flags & crate::class::FLAG_NOGRAVITY != 0;
    let want = cmd.up_move < 0
        && !world.no_crouch
        && !flying
        && !swimming
        && !player.frozen
        && world.math_mode != crate::vanilla::SimMathMode::VanillaFixed;
    let target = if want { 1.0 } else { 0.0 };
    if (player.crouch_amount - target).abs() < 1e-9 {
        return;
    }
    let next = if want {
        (player.crouch_amount + CROUCH_STEP).min(1.0)
    } else {
        (player.crouch_amount - CROUCH_STEP).max(0.0)
    };
    // Land exactly on the ends (six 1/6 steps leave float dust).
    let next = if next < 1e-6 {
        0.0
    } else if next > 1.0 - 1e-6 {
        1.0
    } else {
        next
    };
    let full = world
        .classes
        .get(player.cls)
        .map(|c| c.height)
        .filter(|h| *h > 0.0)
        .unwrap_or(56.0);
    let height = full * (1.0 - 0.5 * next);
    if next < player.crouch_amount {
        // Standing up needs headroom.
        let planes = crate::physics::check_position(world, a, a.pos, Some(mo));
        if planes.ceiling_z - a.pos.z() < height {
            return;
        }
    }
    if let Some(a) = world.actors.get_mut(mo) {
        a.height = height;
    }
    player.crouch_amount = next;
    player.crouching = next > 0.0;
}

/// Eye height after the GZDoom-style crouch (Duke stance changes
/// `view_height` itself).
fn crouched_view_height(player: &Player) -> f64 {
    player.view_height * (1.0 - 0.5 * player.crouch_amount)
}

/// Duke crouch: hold move-down on the ground (not flying / swimming).
fn apply_duke_crouch(world: &mut World, player: &mut Player, mo: ActorId, cmd: &TicCmd) {
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
    crate::powerups::tick_powers(world, player);
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
    player.view_z = a.pos.z() + player.view_height + player.view_ofs_z;
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
#[cfg(test)]
pub fn player_think(world: &mut World, player: &mut Player) {
    player_think_for_seat(world, player, 0);
}

/// Run one player's thinker with its stable player slot for ZScript events.
pub(crate) fn player_think_for_seat(world: &mut World, player: &mut Player, seat: i32) {
    let Some(mo) = player.mo else {
        return;
    };
    // `PowerTimeFreezer`: a player outside the freezer's mask stands still.
    if world.level.frozen_power
        && player.timefreezer == 0
        && !world
            .actors
            .get(mo)
            .is_some_and(|a| a.extra_flags.get_name("NOTIMEFREEZE"))
    {
        return;
    }
    world.thinking_refire = player.refire;
    world.thinking_powers =
        (!player.powers.is_empty()).then(|| (mo, player.powers.values().cloned().collect()));
    // `BasicArmor.Tick`: the per-tic absorb budget starts over.
    player.armor_absorb_count = 0;
    player_environment_tick(world, player, mo);
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
    let mut cmd = player.cmd;
    // `Weapon.LookScale`: turning and looking slow down while the gun is up
    // (sniper zoom).
    if let Some(k) = player
        .ready_weapon
        .and_then(|w| world.classes.get(w))
        .map(|c| c.ext.look_scale)
        .filter(|k| (k - 1.0).abs() > 1e-9)
    {
        let scale = |v: i16| (f64::from(v) * k).round().clamp(-32768.0, 32767.0) as i16;
        cmd.yaw = scale(cmd.yaw);
        cmd.pitch = scale(cmd.pitch);
    }
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
        if cmd.buttons.any(Buttons::USE)
            && !world.no_player_respawn
            && world.fire_player_respawning(seat, mo)
            && world.respawn_player_at_seat(player, seat as usize)
        {
            world.fire_player_respawned(seat, mo);
            if let Some(mo) = player.mo {
                crate::acs::start_respawn_scripts(world, mo);
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
    apply_crouch(world, player, mo, &cmd);
    // A mod's ZScript movement (`PlayerThink` / `HandleMovement` overrides,
    // Project Brutality) turns, moves, jumps and sets the eye instead of
    // Room's `P_MovePlayer`.
    let script_move = world.zscript_moves_player(mo);
    // Yaw / pitch (same turn units as Doom `angleturn`).
    if !script_move && (cmd.yaw != 0 || cmd.pitch != 0) {
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
    if jump
        && !script_move
        && !player.jump_down
        && player.on_ground
        && !player.crouching
        && !world.no_jump
        && world.try_actor_jump(mo)
    {
        player.on_ground = false;
        play_player_sound(world, mo, "player/male/jump");
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
    let (scale, side_scale, run_health) = world
        .actors
        .get(mo)
        .and_then(|a| world.classes.get(a.class))
        .map(|c| {
            let fwd = if c.forward_move > 0.0 {
                c.forward_move
            } else {
                1.0
            };
            // `Player.SideMove` scales strafing. Without it Room keeps its old
            // rule of strafing at the `Player.ForwardMove` scale.
            let side = if (c.ext.side_move - 1.0).abs() > 1e-9 {
                c.ext.side_move
            } else {
                fwd
            };
            (fwd, side, c.ext.run_health)
        })
        .unwrap_or((1.0, 1.0, 0));
    let scale_i16 = |v: i16, k: f64| {
        (f64::from(v) * k)
            .round()
            .clamp(f64::from(i16::MIN), f64::from(i16::MAX)) as i16
    };
    if (scale - 1.0).abs() > 1e-9 {
        move_cmd.forward_move = scale_i16(move_cmd.forward_move, scale);
    }
    if (side_scale - 1.0).abs() > 1e-9 {
        move_cmd.side_move = scale_i16(move_cmd.side_move, side_scale);
    }
    // `PowerSpeed`: 1.5× walk and run.
    let speed = crate::powerups::speed_factor(world, mo);
    if (speed - 1.0).abs() > 1e-9 {
        move_cmd.forward_move = scale_i16(move_cmd.forward_move, speed);
        move_cmd.side_move = scale_i16(move_cmd.side_move, speed);
    }
    // `Player.RunHealth` (`TweakSpeeds`): at or below it the player walks.
    if run_health > 0 && world.actors.get(mo).is_some_and(|a| a.health <= run_health) {
        move_cmd.forward_move = move_cmd.forward_move.clamp(-0x19 * 256, 0x19 * 256);
        move_cmd.side_move = move_cmd.side_move.clamp(-0x18 * 256, 0x18 * 256);
    }
    if player.crouching {
        move_cmd.forward_move /= 2;
        move_cmd.side_move /= 2;
    }
    if script_move {
        script_player_think(world, player, mo, &move_cmd);
    } else {
        move_player(world, mo, &move_cmd);
    }
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
    let was_use_down = player.use_down;
    // These traces are edge-triggered, so leave them on by default while
    // diagnosing vehicle interaction. They do not emit every simulation tic.
    let trace_vehicle = true;
    if trace_vehicle && use_btn != was_use_down {
        eprintln!(
            "room: VEHICLE INPUT tic={} player=#{} USE={} edge={} pos=({:.1},{:.1},{:.1}) angle={:.1}",
            world.level.time,
            mo.index(),
            use_btn,
            use_btn && !was_use_down,
            world.actors.get(mo).map_or(0.0, |a| a.pos.x()),
            world.actors.get(mo).map_or(0.0, |a| a.pos.y()),
            world.actors.get(mo).map_or(0.0, |a| a.pos.z()),
            world.actors.get(mo).map_or(0.0, |a| a.angle.degrees()),
        );
    }
    if !use_btn {
        player.acs_use_latched = false;
    }
    if use_btn && !player.use_down {
        // Park the in-hand player so `activate_line` sees `players[].mo` (the
        // slot is empty for the duration of `player_think`). Without this,
        // non-door Use specials (Exit, floors, lights, …) are refused as
        // monster uses while doors still work via the monster whitelist.
        let (used, used_actor) = world.with_parked_player(player, |world| {
            // Prefer a usable actor at the crosshair. If a linedef is hit
            // first, the short-circuit would otherwise swallow USE before
            // vehicles such as Brutal Doom's Bike can enter their Active state.
            let used_actor = crate::use_lines::use_actors(world, mo);
            (used_actor || use_lines(world, mo), used_actor)
        });
        if used_actor {
            player.acs_use_latched = true;
        }
        if trace_vehicle {
            eprintln!(
                "room: VEHICLE INPUT tic={} result={} actor={} ACS-use-latched={}",
                world.level.time,
                if used_actor {
                    "vehicle/actor"
                } else if used {
                    "line"
                } else {
                    "nothing"
                },
                used_actor,
                player.acs_use_latched,
            );
        }
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
    // Inventory bar (`invnext` / `invprev` / `invuse`).
    if player.inventory_tics > 0 {
        player.inventory_tics -= 1;
    }
    crate::invbar::validate_selection(world, player, mo);
    if cmd.buttons.any(Buttons::INVNEXT) {
        crate::invbar::cycle(world, player, mo, 1);
    } else if cmd.buttons.any(Buttons::INVPREV) {
        crate::invbar::cycle(world, player, mo, -1);
    }
    let inv_btn = cmd.buttons.any(Buttons::INVUSE);
    if inv_btn {
        let pressed = !player.invuse_down;
        if player.inv_sel.is_some() || player.inventory_tics > 0 {
            if pressed {
                let _ = crate::invbar::use_selected(world, player, mo);
            }
        } else {
            use_inventory(world, player, mo, pressed);
        }
    }
    player.invuse_down = inv_btn;
    // Slot/cycle selection; `WRF_DISABLESWITCH` is enforced inside `A_WeaponReady`
    // for pending Deselect — slot requests still queue `pending_weapon`.
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
    advance_overlay2(world, player);
    advance_extra_layers(world, player);
    // Sync view_z to eye height / grounded flag (pre-`z_movement` this tic).
    // A movement script set the eye itself (`CalcHeight`).
    if let Some(a) = world.actors.get(mo).cloned().filter(|_| !script_move) {
        let planes = crate::physics::check_position(world, &a, a.pos, Some(mo));
        if a.vel.z() <= 0.0 {
            player.on_ground = a.pos.z() <= planes.floor_z + 0.1;
        }
        apply_duke_fall(world, player, mo);
        // Vanilla `P_CalcHeight`: bob = (momx²+momy²)/4, capped at MAXBOB=16.
        let bob_sq = a.vel.x() * a.vel.x() + a.vel.y() * a.vel.y();
        player.bob = (bob_sq * 0.25).clamp(0.0, 16.0);
        // Doom `P_CalcHeight`: bob the eye when grounded (fine-angle from maptime).
        // `Player.ViewBobSpeed` is the bob period in tics (default 20, Doom's).
        let (scale, period, fly_bob) = world.classes.get(a.class).map_or((1.0, 20.0, 1.0), |c| {
            (
                c.ext.view_bob,
                c.ext
                    .stored_f64("player.viewbobspeed")
                    .filter(|v| *v > 0.0)
                    .unwrap_or(20.0),
                c.ext.stored_f64("player.flybob").unwrap_or(1.0),
            )
        });
        let flying = !player.on_ground && a.flags & crate::class::FLAG_NOGRAVITY != 0;
        if flying {
            // `Player.FlyBob`: a fixed bob while flying (`0.5 × FlyBob`).
            player.bob = (0.5 * fly_bob).abs().min(16.0);
        }
        let bob_z = if player.on_ground || flying {
            let t = f64::from(world.level.maptime) * (std::f64::consts::TAU / period);
            // `Player.ViewBob` scales the eye bob (not the weapon).
            player.bob * 0.5 * dmath::sin(t) * scale
        } else {
            0.0
        };
        let mut view_z = a.pos.z() + crouched_view_height(player) + player.view_ofs_z + bob_z;
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
    // Brutal Doom `CheckWeaponAction`: R / Q / G give a token the weapon's
    // Ready state jumps on. ACS `GiveInventory` does not implement that yet.
    pulse_bd_weapon_tokens(world, mo, cmd.buttons.0, player.prev_buttons);
    log_attack_status(world, player, mo, cmd.buttons.0);
    player.prev_buttons = raw.buttons.0;
}

/// Run the pawn's script `PlayerThink` (a mod's movement code) with the
/// player's state mirrored in and its changes (eye height, ground flag,
/// jump and turn counters) taken back. The pawn's fields (velocity, angle,
/// pitch, position, flags) go through the usual script sync.
fn script_player_think(world: &mut World, player: &mut Player, mo: ActorId, cmd: &TicCmd) {
    let Some(a) = world.actors.get(mo).cloned() else {
        return;
    };
    let Some(class) = world.classes.get(a.class).map(|c| c.name.clone()) else {
        return;
    };
    let (pawn_view_height, view_bob) = world
        .classes
        .get(a.class)
        .map_or((41.0, 1.0), |c| (c.view_height, c.ext.view_bob));
    // UZDoom's `BT_*` bits: run (`BT_RUN`, a run-speed move or the speed
    // key) and crouch (`BT_CROUCH`, move down) included.
    let uz_buttons = |b: u32, c: &TicCmd| {
        let mut g = crate::acs::gzdoom_button_bits(b) as u32;
        if b & Buttons::SPEED != 0
            || i32::from(c.forward_move).abs() > 25 * 256
            || i32::from(c.side_move).abs() > 24 * 256
        {
            g |= 1 << 25;
        }
        if c.up_move < 0 {
            g |= 1 << 3;
        }
        g
    };
    let mut st = crate::zscript::PlayerMoveState {
        buttons: uz_buttons(cmd.buttons.0, cmd),
        old_buttons: crate::acs::gzdoom_button_bits(player.prev_buttons) as u32,
        forward_move: cmd.forward_move,
        side_move: cmd.side_move,
        up_move: cmd.up_move,
        yaw: cmd.yaw,
        // UZDoom's `cmd.pitch` is positive looking up; Room's down.
        pitch: cmd.pitch.saturating_neg(),
        on_ground: player.on_ground,
        jump_tics: player.script_jump_tics,
        turn_ticks: player.script_turn_ticks,
        centering: player.script_centering,
        cheats: 0,
        health: a.health,
        water_level: crate::physics::actor_waterlevel(world, &a),
        view_z: player.view_z,
        view_height: player.script_view_height.unwrap_or(pawn_view_height),
        delta_view_height: player.script_delta_view_height,
        crouch_factor: 1.0 - 0.5 * player.crouch_amount,
        crouch_view_delta: -0.5 * player.crouch_amount * pawn_view_height,
        pawn_view_height,
        view_bob,
    };
    let synced = world.zscript_sync_in(mo);
    let result = world.zscript.call_player_think(mo.index(), &class, &mut st);
    world.zscript_sync_out(mo, synced);
    crate::action::apply_zscript_actor_ops(world, mo);
    if let Err(e) = result {
        static LOGGED: std::sync::Once = std::sync::Once::new();
        LOGGED.call_once(|| eprintln!("room: {class}.PlayerThink: {e:?}"));
        // Fall back to Room's turning and movement so the player is not
        // stuck.
        if let Some(a) = world.actors.get_mut(mo) {
            a.angle = Angle::from_degrees(a.angle.degrees() + f64::from(cmd.yaw) * YAW_TO_DEG);
        }
        move_player(world, mo, cmd);
        return;
    }
    player.on_ground = st.on_ground;
    player.view_z = st.view_z;
    player.script_view_height = Some(st.view_height);
    player.script_delta_view_height = st.delta_view_height;
    player.script_jump_tics = st.jump_tics;
    player.script_turn_ticks = st.turn_ticks;
    player.script_centering = st.centering;
}

/// Why the gun did or did not fire. Printed on the attack edge, and once a
/// second while the button is held in a state that refuses fire.
fn log_attack_status(world: &mut World, player: &Player, mo: ActorId, buttons: u32) {
    let attack = buttons & Buttons::ATTACK != 0;
    if !attack {
        return;
    }
    let edge = player.prev_buttons & Buttons::ATTACK == 0;
    let (sprite, frame, action, args, tics) = player
        .pspr_state
        .and_then(|sid| world.states.get(sid))
        .map(|st| {
            (
                st.sprite.clone(),
                st.frame,
                st.action.clone().unwrap_or_default(),
                st.action_args.clone(),
                player.pspr_tics,
            )
        })
        .unwrap_or_else(|| (String::new(), 0, String::new(), Vec::new(), 0));
    let nofire = action.eq_ignore_ascii_case("A_WeaponReady")
        && args
            .iter()
            .any(|a| a.to_ascii_uppercase().contains("NOFIRE"));
    let raising = (player.pspr_y - crate::player::WEAPON_TOP).abs() > 0.5;
    let reason = if player.pending_weapon.is_some() {
        "switching"
    } else if raising {
        "raising"
    } else if nofire {
        "NOFIRE"
    } else if action.eq_ignore_ascii_case("A_WeaponReady") {
        "ready"
    } else if action.is_empty() {
        "no-action"
    } else {
        "busy"
    };
    let stuck = matches!(reason, "switching" | "raising" | "NOFIRE" | "no-action");
    if !edge && (!stuck || world.level.maptime % 35 != 0) {
        return;
    }
    let wname = player
        .ready_weapon
        .and_then(|c| world.classes.get(c))
        .map(|c| c.name.clone())
        .unwrap_or_else(|| "-".into());
    let ammo = player
        .ready_weapon
        .and_then(|c| world.classes.get(c))
        .map(|def| {
            let one = |name: &str| -> String {
                if name.is_empty() {
                    return "-".into();
                }
                match world.classes.find_name(name) {
                    Some(cid) => format!("{name}={}", world.inventory.count(mo, cid)),
                    None => format!("{name}=MISSING"),
                }
            };
            format!("{} {}", one(&def.ammo_type), one(&def.ammo_type2))
        })
        .unwrap_or_else(|| "no-ammo-def".into());
    let can = player.ready_weapon.is_some_and(|w| has_ammo(world, mo, w));
    let frame_ch = if frame == crate::state::FRAME_NOCHANGE {
        '#'
    } else {
        char::from(b'A' + frame.min(25))
    };
    world.push_debug(format!(
        "FIRE {wname} {reason} {sprite}{frame_ch} act={action} tics={tics} y={:.0} can={can} {ammo}",
        player.pspr_y
    ));
}

/// Edge-triggered stand-in for ACS script `CheckWeaponAction`.
fn pulse_bd_weapon_tokens(world: &mut World, mo: ActorId, cur: u32, old: u32) {
    let pressed = cur & !old;
    if pressed == 0 {
        return;
    }
    let mut give = |name: &str| {
        if let Some(cid) = world.classes.find_name(name) {
            let _ = crate::inventory::give_inventory(world, mo, cid, 1);
        }
    };
    if pressed & Buttons::RELOAD != 0 {
        give("Reloading");
        give("BDWeaponAction");
        give("GoSpecial");
    }
    if pressed & Buttons::ZOOM != 0 {
        give("Kicking");
        give("BDWeaponAction");
        give("GoSpecial");
    }
    if pressed & Buttons::USER1 != 0 {
        give("TossGrenade");
        give("BDWeaponAction");
        give("GoSpecial");
    }
    if pressed & Buttons::USER2 != 0 {
        give("Unloading");
        give("BDWeaponAction");
        give("GoSpecial");
    }
    if pressed & Buttons::USER3 != 0 {
        give("StartDualWield");
        give("BDWeaponAction");
        give("GoSpecial");
    }
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

/// Layer-2 frame length after `PSPF_POWDOUBLE`, then `PSPF_CVARFAST`. They do not stack.
fn overlay2_frame_tics(player: &Player, world: &World, tics: i32) -> i32 {
    let powered = player.overlay2_pow_double
        && tics > 1
        && player.mo.is_some_and(|mo| double_firing_speed(world, mo));
    if powered {
        (tics / 2).max(1)
    } else if player.overlay2_cvar_fast && tics > 1 {
        match world.fast_weapons {
            1 => (tics / 2).max(1),
            n if n >= 2 => 1,
            _ => tics,
        }
    } else {
        tics
    }
}

/// `PowerDoubleFiringSpeed` in the player's inventory or running as a
/// generic power (`PowerupGiver`).
fn double_firing_speed(world: &World, mo: ActorId) -> bool {
    world
        .classes
        .find_name("PowerDoubleFiringSpeed")
        .is_some_and(|id| world.inventory.count(mo, id) > 0)
        || crate::powerups::power(world, mo, "doublefiringspeed").is_some()
}

/// Zero-tic psprite transitions allowed in one tic before the chain is
/// treated as an endless `Loop` and left for the next tic.
const MAX_PSPRITE_ZERO_TIC_CHAIN: u32 = 10_000;

/// Jump the weapon psprite to a class label (`Ready`, `Fire`, …).
///
/// Doom `P_SetPsprite`: call the state's action on **enter**, then keep advancing
/// while `tics == 0` so `A_ReFire` / instant frames resolve in the same tic.
fn set_psprite(world: &mut World, player: &mut Player, mo: ActorId, mut state: StateId) {
    // UZDoom `DPSprite::SetState` loops `while (!Tics)` with no cap; Brutal
    // Doom's reloads load each round through ~17 zero-tic states in one tic
    // (`InsertBullets`). Keep a guard only against a broken 0-tic `Loop`.
    // Gun actions a ZScript weapon action function called (`A_WeaponReady`
    // inside `A_DoPBWeaponAction`), run on this psprite after it returns.
    // A state change drops the rest (they were for the state that called).
    let mut script_queue: std::collections::VecDeque<(StateId, String, Vec<String>)> =
        std::collections::VecDeque::new();
    for _ in 0..MAX_PSPRITE_ZERO_TIC_CHAIN {
        if script_queue
            .front()
            .is_some_and(|(from, _, _)| *from != state)
        {
            script_queue.clear();
        }
        let (action, args, callback_id, module_index) = if let Some((_, name, queued_args)) =
            script_queue.pop_front()
        {
            (Some(name), queued_args, -1, 0)
        } else {
            player.pspr_state = Some(state);
            if let Some(st) = world.states.get(state) {
                if !crate::state::sprite_unchanged(&st.sprite) {
                    player.pspr_sprite = st.sprite.clone();
                }
                if st.frame != crate::state::FRAME_NOCHANGE {
                    player.pspr_frame = st.frame;
                }
            }
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
            let tics = if player.weapon_pow_double && tics > 1 && double_firing_speed(world, mo) {
                (tics / 2).max(1)
            } else if player.weapon_cvar_fast && tics > 1 {
                match world.fast_weapons {
                    1 => (tics / 2).max(1),
                    n if n >= 2 => 1,
                    _ => tics,
                }
            } else {
                tics
            };
            player.pspr_tics = tics;
            (action, args, callback_id, module_index)
        };
        // WASM psprite action (`docs/31` MA1) before native arms.
        if callback_id >= 0 {
            let seat = world
                .players
                .iter()
                .position(|p| p.as_ref().and_then(|p| p.mo) == Some(mo))
                .map(|s| s as i32)
                .unwrap_or(-1);
            if let Some(Some(j)) = crate::script_mods::dispatch_state_action(
                world,
                module_index,
                callback_id,
                mo,
                seat,
                state,
                room_script_abi::action_caller::PSPRITE,
            ) {
                state = j;
                continue;
            }
        } else if let Some(action) = action {
            // A flattened `if` skipped this action (`A_SkipUnless*`); the gun
            // arms below do not pass through `call_action_args`, which
            // counts the skips for every other action.
            let skipped = world
                .actors
                .get_mut(mo)
                .filter(|a| a.action_skip > 0)
                .map(|a| a.action_skip -= 1)
                .is_some();
            match action.as_str() {
                _ if skipped => {}
                name if overlay_action(world, player, name, &args) => {}
                "A_ZoomFactor" => {
                    // Weapon states run while the player slot is empty, so this
                    // cannot go through `player_seat_of`.
                    let scale = action_f64(&args, 0, 1.0) as f32;
                    player.weapon_zoom = scale.clamp(0.1, 10.0);
                }
                "A_SetCrosshair" => {
                    player.crosshair = action_i32(&args, 0, 0);
                }
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
                                    let no_alert =
                                        world.classes.get(wpn).is_some_and(|c| c.weapon_noalert);
                                    if !no_alert {
                                        crate::action::noise_alert(world, mo);
                                    }
                                    state = fire;
                                    continue;
                                }
                            } else if refire_auto_switch(&args) {
                                if let Some(alt) = best_weapon_with_ammo(world, mo, Some(wpn)) {
                                    request_weapon(world, player, mo, alt);
                                }
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
                "A_CheckForReload" => {
                    // UZDoom StateProvider.A_CheckForReload. ReloadCounter belongs
                    // to each weapon inventory, so preserve it across weapon swaps.
                    if let Some(wpn) = player.ready_weapon {
                        let count = action_i32(&args, 0, 0);
                        if count > 0 {
                            let dont_increment = args.get(2).is_some_and(|value| {
                                let value = value.trim().trim_matches('"');
                                value.eq_ignore_ascii_case("true") || value == "1"
                            });
                            let previous = player
                                .reload_counters
                                .iter()
                                .find(|(weapon, _)| *weapon == wpn)
                                .map(|(_, value)| *value)
                                .unwrap_or(0);
                            let next = if dont_increment && previous == 0 {
                                1
                            } else {
                                (previous + 1) % count
                            };
                            if !dont_increment {
                                set_reload_counter(player, wpn, next);
                            }
                            if next != 0 {
                                if let Some(label) = args.get(1) {
                                    let label = label.trim().trim_matches('"');
                                    if let Some(jump) = world.states.find_label(wpn, label) {
                                        state = jump;
                                        continue;
                                    }
                                }
                            } else if !has_ammo(world, mo, wpn) {
                                if let Some(alt) = best_weapon_with_ammo(world, mo, Some(wpn)) {
                                    request_weapon(world, player, mo, alt);
                                }
                            }
                        }
                    }
                }
                "A_ResetReloadCounter" => {
                    if let Some(wpn) = player.ready_weapon {
                        set_reload_counter(player, wpn, 0);
                    }
                }
                "A_OpenShotgun2" => play_player_sound(world, mo, "weapons/sshoto"),
                "A_LoadShotgun2" => play_player_sound(world, mo, "weapons/sshotl"),
                "A_CloseShotgun2" => play_player_sound(world, mo, "weapons/sshotc"),
                "A_GunFlash" => start_flash_psprite(world, player, 0),
                "A_Light1" => player.extralight = 1,
                "A_Light2" => player.extralight = 2,
                "A_Light0" | "A_LightDone" => player.extralight = 0,
                "A_Light" => player.extralight = action_i32(&args, 0, 0).clamp(-20, 20),
                "A_WeaponReady" => {
                    let wrf = parse_wrf_flags(args.first());
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
                    // `WRF_NOSWITCH` / `WRF_DISABLESWITCH` block Deselect (docs/33 BD4).
                    let switch_ok = wrf & (WRF_NOSWITCH | WRF_DISABLESWITCH) == 0;
                    if switch_ok && player.pending_weapon.is_some() {
                        if let Some(cur) = player.ready_weapon {
                            if let Some(deselect) = world.states.find_label(cur, "Deselect") {
                                state = deselect;
                                continue;
                            }
                        }
                    }
                    // Vanilla: sway only updates while Ready (docs/30 W8.3).
                    // `Weapon.BobSpeed` / `BobRangeX/Y` / `BobStyle` (docs/33 BD2).
                    if wrf & WRF_NOBOB == 0 {
                        let (bob_speed, range_x, range_y, style) = player
                            .ready_weapon
                            .and_then(|wpn| world.classes.get(wpn))
                            .map(|c| {
                                (
                                    c.bob_speed,
                                    c.bob_range_x,
                                    c.bob_range_y,
                                    c.bob_style.as_str(),
                                )
                            })
                            .unwrap_or((1.0, 1.0, 1.0, ""));
                        let speed = if bob_speed == 0.0 { 1.0 } else { bob_speed };
                        let t =
                            f64::from(world.level.maptime) * (core::f64::consts::PI / 32.0) * speed;
                        let mut bx = player.bob * dmath::cos(t) * range_x;
                        let by = player.bob * dmath::sin(t).abs() * range_y;
                        // Inverse* styles flip X. Alpha styles also scale by weapon alpha in
                        // GZDoom; Room keeps magnitude 1 until psprite alpha is tracked.
                        if style.eq_ignore_ascii_case("Inverse")
                            || style.eq_ignore_ascii_case("InverseAlpha")
                            || style.eq_ignore_ascii_case("InverseSmooth")
                        {
                            bx = -bx;
                        }
                        player.pspr_bob_x = bx;
                        player.pspr_bob_y = by;
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
                    // BD4: Reload / Zoom / User1–4 when WRF_ALLOW* and button held.
                    if let Some(wpn) = player.ready_weapon {
                        if let Some(next) =
                            weapon_ready_special_state(world, wpn, wrf, player.cmd.buttons)
                        {
                            state = next;
                            continue;
                        }
                    }
                    // Doom: held BT_ATTACK / ALTATTACK while Ready → fire (same tic).
                    let no_fire = wrf & WRF_NOFIRE != 0;
                    if !no_fire
                        && wrf & WRF_NOPRIMARY == 0
                        && player.cmd.buttons.any(Buttons::ATTACK)
                    {
                        player.alt_fire_active = false;
                        fire_weapon(world, player, mo, false);
                        return;
                    }
                    if !no_fire
                        && wrf & WRF_NOSECONDARY == 0
                        && player.cmd.buttons.any(Buttons::ALTATTACK)
                    {
                        player.alt_fire_active = true;
                        fire_weapon(world, player, mo, true);
                        return;
                    }
                }
                other => {
                    world.psprite_weapon = player.ready_weapon;
                    world.psprite_state = Some(state);
                    world.psprite_flash_request = None;
                    world.psprite_buttons = [
                        crate::acs::gzdoom_button_bits(player.cmd.buttons.0),
                        crate::acs::gzdoom_button_bits(player.prev_buttons),
                    ];
                    world.psprite_view = [player.view_z, if player.crouching { 0.5 } else { 1.0 }];
                    let args = crate::deco_eval::eval_action_args(world, mo, &args);
                    let result = crate::action::call_action_args(world, mo, other, &args);
                    drain_psp_queue(world, player, mo);
                    // Fire actions request their own flash after deplete (N4/N5).
                    if let Some(offset) = world.psprite_flash_request.take() {
                        start_flash_psprite(world, player, offset);
                    }
                    world.psprite_weapon = None;
                    world.psprite_state = None;
                    let gun_actions = std::mem::take(&mut world.psp_script_actions);
                    if let crate::action::ActionResult::Jump(j) = result {
                        state = j;
                        continue;
                    }
                    script_queue.extend(gun_actions.into_iter().map(|(n, a)| (state, n, a)));
                }
            }
        }
        if !script_queue.is_empty() {
            continue;
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

/// Actions that act on the running weapon psprite, so a ZScript weapon
/// action function's call to one runs in [`set_psprite`] (UZDoom calls them
/// with the psprite's `stateinfo`).
pub(crate) fn is_gun_action(name: &str) -> bool {
    const GUN: &[&str] = &[
        "A_WeaponReady",
        "A_ReFire",
        "A_Lower",
        "A_Raise",
        "A_CheckReload",
        "A_CheckForReload",
        "A_ResetReloadCounter",
        "A_GunFlash",
        "A_Light",
        "A_Light0",
        "A_Light1",
        "A_Light2",
        "A_LightDone",
        "A_ZoomFactor",
        "A_SetCrosshair",
    ];
    GUN.iter().any(|g| g.eq_ignore_ascii_case(name))
}

/// Layer argument of an `A_Overlay*` call: a number, `OverlayID()`, or 0 for
/// the layer running the action (UZDoom `stateinfo->mPSPIndex`).
fn layer_arg(player: &Player, args: &[String]) -> i32 {
    let current = if player.psp_current == 0 {
        1
    } else {
        player.psp_current
    };
    let Some(raw) = args.first() else {
        return 1;
    };
    let t = raw.trim().trim_matches('"');
    if t.eq_ignore_ascii_case("OverlayID()") || t.eq_ignore_ascii_case("OverlayID") {
        return current;
    }
    let value = t
        .parse::<i32>()
        .ok()
        .or_else(|| crate::uz_constants::eval_int(t).map(|v| v as i32));
    match value {
        Some(0) => current,
        Some(v) => v,
        None => 1,
    }
}

/// `A_OverlayVertexOffset(layer, index, x, y, flags)` on one layer's corners;
/// `WOF_KEEPX` 1, `WOF_KEEPY` 2, `WOF_ADD` 4.
fn apply_vertex_offset(vertex: &mut [(f64, f64); 4], args: &[String]) {
    let index = action_i32(args, 1, 0);
    let (vx, vy, flags) = (
        action_f64(args, 2, 0.0),
        action_f64(args, 3, 0.0),
        action_i32(args, 4, 0),
    );
    if flags & 3 == 3 {
        return;
    }
    if let Some(v) = usize::try_from(index).ok().and_then(|i| vertex.get_mut(i)) {
        if flags & 1 == 0 {
            v.0 = if flags & 4 != 0 { v.0 + vx } else { vx };
        }
        if flags & 2 == 0 {
            v.1 = if flags & 4 != 0 { v.1 + vy } else { vy };
        }
    }
}

fn is_extra_layer(id: i32) -> bool {
    id != 1 && id != 2 && id != crate::player::PSP_FLASH
}

/// State `label` on the ready weapon; `Null` / empty means none.
fn weapon_label_state(world: &World, player: &Player, label: &str) -> Option<StateId> {
    let label = label.trim().trim_matches('"');
    if label.is_empty() || label.eq_ignore_ascii_case("null") {
        return None;
    }
    let wpn = player.ready_weapon?;
    world.states.find_label(wpn, label)
}

/// `A_Overlay*` on layers other than 1, 2 and the flash.
fn overlay_extra_layer_action(
    world: &mut World,
    player: &mut Player,
    action: &str,
    args: &[String],
) -> bool {
    use crate::player::{PspLayer, pspf};
    if action == "A_ClearOverlays" {
        // `A_ClearOverlays(start, stop, safety = true)`: 0, 0 is every layer.
        let start = action_i32(args, 0, 0);
        let stop = action_i32(args, 1, 0);
        let safety = args.get(2).is_none_or(|s| {
            let t = s.trim().trim_matches('"');
            !t.eq_ignore_ascii_case("false") && t != "0"
        });
        player.clear_extra_layers(start, stop, safety);
        return false;
    }
    let is_overlay_action = matches!(
        action,
        "A_Overlay"
            | "A_OverlayOffset"
            | "A_OverlayAlpha"
            | "A_OverlayScale"
            | "A_OverlayFlags"
            | "A_OverlayPivot"
            | "A_OverlayPivotAlign"
            | "A_OverlayRotate"
            | "A_OverlayRenderStyle"
            | "A_OverlayTranslation"
            | "A_OverlayVertexOffset"
    );
    if !is_overlay_action {
        return false;
    }
    let id = layer_arg(player, args);
    if action == "A_OverlayVertexOffset" && !is_extra_layer(id) {
        match id {
            1 => apply_vertex_offset(&mut player.weapon_vertex, args),
            2 => apply_vertex_offset(&mut player.overlay2_vertex, args),
            _ => {}
        }
        return true;
    }
    if !is_extra_layer(id) {
        return false;
    }
    if action == "A_Overlay" {
        // `A_Overlay(layer, start, nooverride = false)`.
        let label = args.get(1).cloned().unwrap_or_default();
        let no_override = args.get(2).is_some_and(|s| {
            let t = s.trim().trim_matches('"');
            t.eq_ignore_ascii_case("true") || t == "1"
        });
        if no_override
            && player
                .psp_layers
                .get(&id)
                .is_some_and(|l| l.state.is_some())
        {
            return true;
        }
        match weapon_label_state(world, player, &label) {
            Some(state) => {
                let layer = PspLayer {
                    caller: player.ready_weapon,
                    fresh: true,
                    ..PspLayer::default()
                };
                player.psp_layers.insert(id, layer);
                set_layer_state(world, player, id, state);
            }
            None => {
                player.psp_layers.remove(&id);
            }
        }
        return true;
    }
    let Some(layer) = player.psp_layers.get_mut(&id) else {
        // UZDoom ignores calls on layers that do not exist.
        return true;
    };
    match action {
        "A_OverlayOffset" => {
            // `WOF_KEEPX` 1, `WOF_KEEPY` 2, `WOF_ADD` 4.
            let (wx, wy, flags) = (
                action_f64(args, 1, 0.0),
                action_f64(args, 2, 0.0),
                action_i32(args, 3, 0),
            );
            if flags & 1 == 0 {
                layer.x = if flags & 4 != 0 { layer.x + wx } else { wx };
            }
            if flags & 2 == 0 {
                layer.y = if flags & 4 != 0 { layer.y + wy } else { wy };
            }
        }
        "A_OverlayAlpha" => layer.alpha = action_f64(args, 1, 1.0).clamp(0.0, 1.0),
        "A_OverlayScale" => {
            // `WOF_ADD` adds; a Y of 0 copies X.
            let sx = action_f64(args, 1, 1.0);
            let sy_arg = action_f64(args, 2, 0.0);
            let sy = if sy_arg == 0.0 { sx } else { sy_arg };
            layer.scale = if action_i32(args, 3, 0) & 4 != 0 {
                (layer.scale.0 + sx, layer.scale.1 + sy)
            } else {
                (sx, sy)
            };
        }
        "A_OverlayFlags" => {
            let flags = action_i32(args, 1, 0) as u32;
            let enable = args.get(2).is_none_or(|s| {
                let t = s.trim().trim_matches('"');
                !t.eq_ignore_ascii_case("false") && t != "0"
            });
            if enable {
                layer.flags |= flags;
            } else {
                layer.flags &= !flags;
            }
        }
        "A_OverlayPivot" => {
            let (px, py) = (action_f64(args, 1, 0.5), action_f64(args, 2, 0.5));
            layer.pivot = if action_i32(args, 3, 0) & 4 != 0 {
                (layer.pivot.0 + px, layer.pivot.1 + py)
            } else {
                (px, py)
            };
        }
        "A_OverlayPivotAlign" => {
            let axis = |slot: usize| match action_i32(args, slot, 0) {
                1 => 0.5,
                2 => 1.0,
                _ => 0.0,
            };
            layer.pivot = (axis(1), axis(2));
            layer.flags |= pspf::PIVOTPERCENT;
        }
        "A_OverlayRotate" => {
            let a = action_f64(args, 1, 0.0);
            layer.angle = if action_i32(args, 2, 0) & 4 != 0 {
                layer.angle + a
            } else {
                a
            };
        }
        "A_OverlayRenderStyle" => {
            layer.style = Some(render_style_code(action_i32(args, 1, 1)));
        }
        "A_OverlayTranslation" => {
            let name = args
                .get(1)
                .map(|s| s.trim().trim_matches('"').to_string())
                .unwrap_or_default();
            layer.translation = if name.eq_ignore_ascii_case("none") {
                String::new()
            } else {
                name
            };
        }
        "A_OverlayVertexOffset" => apply_vertex_offset(&mut layer.vertex, args),
        _ => {}
    }
    true
}

/// Tics of a frame on an extra layer after `PSPF_POWDOUBLE` / `PSPF_CVARFAST`.
fn layer_frame_tics(world: &World, player: &Player, flags: u32, tics: i32) -> i32 {
    use crate::player::pspf;
    if flags & pspf::POWDOUBLE != 0
        && tics > 1
        && player.mo.is_some_and(|mo| double_firing_speed(world, mo))
    {
        return (tics / 2).max(1);
    }
    if flags & pspf::CVARFAST != 0 && tics > 1 {
        return match world.fast_weapons {
            1 => (tics / 2).max(1),
            n if n >= 2 => 1,
            _ => tics,
        };
    }
    tics
}

/// Enter `state` on extra layer `id`, running its action and any zero-tic
/// chain (UZDoom `DPSprite::SetState`).
fn set_layer_state(world: &mut World, player: &mut Player, id: i32, mut state: StateId) {
    let Some(mo) = player.mo else {
        return;
    };
    for _ in 0..MAX_PSPRITE_ZERO_TIC_CHAIN {
        let Some(st) = world.states.get(state).cloned() else {
            player.psp_layers.remove(&id);
            return;
        };
        let Some(layer) = player.psp_layers.get_mut(&id) else {
            return;
        };
        layer.state = Some(state);
        if !crate::state::sprite_unchanged(&st.sprite) && !st.sprite.is_empty() {
            layer.sprite = st.sprite.clone();
        }
        if st.frame != crate::state::FRAME_NOCHANGE {
            layer.frame = st.frame;
        }
        let flags = layer.flags;
        let tics = layer_frame_tics(world, player, flags, i32::from(st.tics));
        if let Some(layer) = player.psp_layers.get_mut(&id) {
            layer.tics = tics;
        }
        if let Some(action) = st.action.clone() {
            let saved = player.psp_current;
            player.psp_current = id;
            let jump = run_layer_action(world, player, mo, id, state, &action, &st.action_args);
            player.psp_current = saved;
            if let Some(j) = jump {
                state = j;
                continue;
            }
        }
        // The action may have replaced or removed this layer.
        let Some(layer) = player.psp_layers.get(&id) else {
            return;
        };
        if layer.state != Some(state) {
            match layer.state {
                Some(cur) => {
                    state = cur;
                    continue;
                }
                None => return,
            }
        }
        if layer.tics != 0 {
            return;
        }
        match st.next {
            Some(n) => state = n,
            None => {
                player.psp_layers.remove(&id);
                return;
            }
        }
    }
}

/// An action on an extra layer. Returns a jump inside that layer.
fn run_layer_action(
    world: &mut World,
    player: &mut Player,
    mo: ActorId,
    id: i32,
    state: StateId,
    action: &str,
    args: &[String],
) -> Option<StateId> {
    if overlay_action(world, player, action, args) {
        return None;
    }
    match action {
        "A_SetTics" => {
            if let Some(layer) = player.psp_layers.get_mut(&id) {
                layer.tics = action_i32(args, 0, layer.tics);
            }
            None
        }
        "A_GunFlash" => {
            start_flash_psprite(world, player, 0);
            None
        }
        "A_Light0" | "A_LightDone" => {
            player.extralight = 0;
            None
        }
        "A_Light1" => {
            player.extralight = 1;
            None
        }
        "A_Light2" => {
            player.extralight = 2;
            None
        }
        "A_Light" => {
            player.extralight = action_i32(args, 0, 0).clamp(-20, 20);
            None
        }
        // Gun-only actions keep acting on the gun, as in UZDoom.
        "A_WeaponReady" | "A_Lower" | "A_Raise" | "A_ReFire" => None,
        other => {
            world.psprite_weapon = player.ready_weapon;
            world.psprite_state = Some(state);
            world.psprite_flash_request = None;
            let args = crate::deco_eval::eval_action_args(world, mo, args);
            let result = crate::action::call_action_args(world, mo, other, &args);
            drain_psp_queue(world, player, mo);
            if let Some(offset) = world.psprite_flash_request.take() {
                start_flash_psprite(world, player, offset);
            }
            world.psprite_weapon = None;
            world.psprite_state = None;
            match result {
                crate::action::ActionResult::Jump(j) => Some(j),
                _ => None,
            }
        }
    }
}

/// Count down every extra layer and step the ones whose frame ran out.
fn advance_extra_layers(world: &mut World, player: &mut Player) {
    // UZDoom `TickPSprites`: a weapon's layers go when it is no longer ready.
    let ready = player.ready_weapon;
    player
        .psp_layers
        .retain(|_, l| l.caller.is_none() || l.caller == ready);
    let ids: Vec<i32> = player.psp_layers.keys().copied().collect();
    for id in ids {
        let Some(layer) = player.psp_layers.get_mut(&id) else {
            continue;
        };
        let Some(sid) = layer.state else {
            player.psp_layers.remove(&id);
            continue;
        };
        if layer.fresh {
            layer.fresh = false;
            continue;
        }
        if layer.tics < 0 {
            continue;
        }
        if layer.tics > 0 {
            layer.tics -= 1;
        }
        if layer.tics != 0 {
            continue;
        }
        match world.states.get(sid).and_then(|st| st.next) {
            Some(n) => set_layer_state(world, player, id, n),
            None => {
                player.psp_layers.remove(&id);
            }
        }
    }
}

/// `A_Overlay*` called on an actor (ZScript weapon functions, inventory). A
/// thinking player is out of its slot, so the call waits in the queue that
/// the psprite code drains after the action returns.
pub(crate) fn overlay_action_from_actor(
    world: &mut World,
    mo: ActorId,
    action: &str,
    args: &[String],
) {
    let Some(seat) = world.player_seat_of(mo) else {
        return;
    };
    let seat = usize::from(seat);
    match world.players.get_mut(seat).and_then(Option::take) {
        Some(mut player) => {
            overlay_action(world, &mut player, action, args);
            world.players[seat] = Some(player);
        }
        None => world
            .psp_action_queue
            .push((mo, action.to_owned(), args.to_vec())),
    }
}

/// Apply queued actor-context `A_Overlay*` calls to the thinking player.
fn drain_psp_queue(world: &mut World, player: &mut Player, mo: ActorId) {
    let queued: Vec<_> = std::mem::take(&mut world.psp_action_queue);
    for (who, action, args) in queued {
        if who == mo {
            overlay_action(world, player, &action, &args);
        } else {
            world.psp_action_queue.push((who, action, args));
        }
    }
}

/// `A_Overlay*` / `A_WeaponOffset` / `A_ClearOverlays` from any weapon layer.
/// Returns false for other actions.
fn overlay_action(world: &mut World, player: &mut Player, action: &str, args: &[String]) -> bool {
    let args = args.to_vec();
    if overlay_extra_layer_action(world, player, action, &args) {
        return true;
    }
    match action {
        "A_WeaponOffset" => {
            apply_weapon_offset(
                player,
                action_f64(&args, 0, 0.0),
                action_f64(&args, 1, crate::player::WEAPON_TOP),
                action_i32(&args, 2, 0),
            );
        }
        "A_ClearOverlays" => {
            let start = action_i32(&args, 0, 0);
            let stop = action_i32(&args, 1, 0);
            // Defaults clear all safe overlays; this runtime currently
            // models the weapon overlay at id 2 (GZDoom reserves id 1
            // for the weapon and id 0 for CHAN_AUTO-like selection).
            if (start == 0 && stop == 0) || (start <= 2 && stop >= 2) {
                player.clear_secondary_overlay();
            }
        }
        "A_OverlayScale" => {
            // A Y of 0 copies X. Layer 1 is the gun. Layer 2 is the overlay.
            let sx = action_f64(&args, 1, 1.0);
            let sy_arg = action_f64(&args, 2, 0.0);
            let sy = if sy_arg == 0.0 { sx } else { sy_arg };
            let flags = action_i32(&args, 3, 0);
            match layer_arg(player, &args) {
                1 => {
                    player.weapon_scale = Some(scaled_pair(player.weapon_scale, sx, sy, flags));
                }
                2 => {
                    player.overlay2_scale = Some(scaled_pair(player.overlay2_scale, sx, sy, flags));
                }
                _ => {}
            }
        }
        "A_Overlay" => {
            // Layer 2 plays `start` on the ready weapon. Layer 1 stays the gun.
            if layer_arg(player, &args) == 2 {
                if let Some(wpn) = player.ready_weapon {
                    let label = args
                        .get(1)
                        .map(|s| s.trim().trim_matches('"').to_string())
                        .unwrap_or_default();
                    if let Some(state) = world.states.find_label(wpn, &label) {
                        if let Some(st) = world.states.get(state) {
                            player.overlay2_state = Some(state);
                            player.overlay2_tics =
                                overlay2_frame_tics(player, world, i32::from(st.tics));
                            if !crate::state::sprite_unchanged(&st.sprite) && !st.sprite.is_empty()
                            {
                                player.overlay2_sprite = st.sprite.clone();
                                player.overlay2_frame = st.frame;
                            }
                        }
                    }
                }
            }
        }
        "A_OverlayFlags" => {
            use crate::player::pspf;
            let layer = layer_arg(player, &args);
            let flags = action_i32(&args, 1, 0) as u32;
            let enable = args.get(2).is_none_or(|s| {
                let t = s.trim().trim_matches('"');
                !t.eq_ignore_ascii_case("false") && t != "0"
            });
            // UZDoom bit layout (`p_pspr.h`).
            if layer == 2 {
                if flags & pspf::FLIP != 0 {
                    player.overlay2_flip = enable;
                }
                if flags & pspf::MIRROR != 0 {
                    player.overlay2_mirror = enable;
                }
                if flags & pspf::PIVOTPERCENT != 0 {
                    player.overlay2_pivot_percent = enable;
                }
                if flags & pspf::ADDBOB != 0 {
                    player.overlay2_add_bob = enable;
                }
                if flags & pspf::ADDWEAPON != 0 {
                    player.overlay2_add_weapon = enable;
                }
                if flags & pspf::INTERPOLATE != 0 {
                    player.overlay2_interpolate = enable;
                }
                if flags & pspf::FORCEALPHA != 0 {
                    player.overlay2_force_alpha = enable;
                }
                if flags & pspf::FORCESTYLE != 0 {
                    player.overlay2_force_style = enable;
                }
                if flags & pspf::CVARFAST != 0 {
                    player.overlay2_cvar_fast = enable;
                }
                if flags & pspf::POWDOUBLE != 0 {
                    player.overlay2_pow_double = enable;
                }
            } else if layer == 1 {
                if flags & pspf::FLIP != 0 {
                    player.weapon_flip = enable;
                }
                if flags & pspf::ADDBOB != 0 {
                    player.weapon_add_bob = enable;
                }
                if flags & pspf::ADDWEAPON != 0 {
                    player.weapon_add_ofs = enable;
                }
                if flags & pspf::CVARFAST != 0 {
                    player.weapon_cvar_fast = enable;
                }
                if flags & pspf::POWDOUBLE != 0 {
                    player.weapon_pow_double = enable;
                }
                if flags & pspf::MIRROR != 0 {
                    player.weapon_mirror = enable;
                }
                if flags & pspf::PIVOTPERCENT != 0 {
                    player.weapon_pivot_percent = enable;
                }
                if flags & pspf::FORCEALPHA != 0 {
                    player.weapon_force_alpha = enable;
                }
                if flags & pspf::FORCESTYLE != 0 {
                    player.weapon_force_style = enable;
                }
                if flags & pspf::INTERPOLATE != 0 {
                    player.weapon_interpolate = enable;
                }
            }
        }
        "A_OverlayPivot" => {
            let pivot = (action_f64(&args, 1, 0.5), action_f64(&args, 2, 0.5));
            match layer_arg(player, &args) {
                1 => player.weapon_pivot = Some(pivot),
                2 => player.overlay2_pivot = Some(pivot),
                _ => {}
            }
        }
        "A_OverlayPivotAlign" => {
            // 0 = left/top, 1 = center, 2 = right/bottom.
            let axis = |slot: usize| match action_i32(&args, slot, 1) {
                0 => 0.0,
                2 => 1.0,
                _ => 0.5,
            };
            let pivot = (axis(1), axis(2));
            match layer_arg(player, &args) {
                1 => player.weapon_pivot = Some(pivot),
                2 => player.overlay2_pivot = Some(pivot),
                _ => {}
            }
        }
        "A_OverlayRotate" => {
            // Degrees, positive is counter-clockwise.
            let angle = action_f64(&args, 1, 0.0);
            match layer_arg(player, &args) {
                1 => player.weapon_angle = angle,
                2 => player.overlay2_angle = angle,
                _ => {}
            }
        }
        "A_OverlayRenderStyle" => {
            // `STYLE_Fuzzy` is 2, `STYLE_Normal` is 1.
            let style = render_style_code(action_i32(&args, 1, 1));
            match layer_arg(player, &args) {
                1 => player.weapon_style = Some(style),
                2 => player.overlay2_style = Some(style),
                _ => {}
            }
        }
        "A_OverlayTranslation" => {
            // "None" clears the tint.
            let name = args
                .get(1)
                .map(|s| s.trim().trim_matches('"').to_string())
                .unwrap_or_default();
            let name = if name.eq_ignore_ascii_case("none") {
                String::new()
            } else {
                name
            };
            match layer_arg(player, &args) {
                1 => player.weapon_translation = name,
                2 => player.overlay2_translation = name,
                _ => {}
            }
        }
        "A_OverlayAlpha" => {
            let alpha = action_f64(&args, 1, 1.0).clamp(0.0, 1.0);
            match layer_arg(player, &args) {
                1 => player.weapon_alpha = Some(alpha),
                2 => player.overlay2_alpha = Some(alpha),
                _ => {}
            }
        }
        "A_OverlayOffset" => {
            let layer = layer_arg(player, &args);
            let wx = action_f64(&args, 1, 0.0);
            let wy = action_f64(&args, 2, crate::player::WEAPON_TOP);
            let flags = action_i32(&args, 3, 0);
            if layer == 1 {
                apply_weapon_offset(player, wx, wy, flags);
            } else if layer == 2 {
                apply_overlay2_offset(player, wx, wy, flags);
            }
        }
        _ => return false,
    }
    true
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
        // GZDoom `P_BringUpWeapon` with `WP_NOCHANGE`: the ready weapon
        // snaps back to `WEAPONTOP` in its Ready state. Brutal Doom's
        // `Select` states call `A_Lower` on purpose (the M202, the rifle…);
        // dropping the psprite there left the player with no weapon.
        match player
            .ready_weapon
            .and_then(|w| world.states.find_label(w, "Ready"))
        {
            Some(ready) => {
                player.pspr_y = crate::player::WEAPON_TOP;
                set_psprite(world, player, mo, ready);
            }
            None => {
                player.pspr_state = None;
                player.pspr_tics = 0;
            }
        }
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

/// Brutal Doom's draw animations are authored at `WEAPONTOP` and never call
/// `A_Raise`. Vanilla `Select` loops on `A_Raise` from `WEAPONBOTTOM`.
fn select_calls_raise(world: &World, start: room_core::StateId) -> bool {
    let mut sid = Some(start);
    for _ in 0..96 {
        let Some(cur) = sid else {
            break;
        };
        let Some(st) = world.states.get(cur) else {
            break;
        };
        if st
            .action
            .as_deref()
            .is_some_and(|a| a.eq_ignore_ascii_case("A_Raise"))
        {
            return true;
        }
        sid = st.next;
    }
    false
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
        // Overlay mods (Brutal Doom) play Select in view. Without `A_Raise`
        // the sprite stays at `WEAPONBOTTOM`, below the status bar.
        if !select_calls_raise(world, sel) {
            player.pspr_y = crate::player::WEAPON_TOP;
        }
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
            "A_Light" => {
                let args = st
                    .map(|state| state.action_args.as_slice())
                    .unwrap_or_default();
                player.extralight = action_i32(args, 0, 0).clamp(-20, 20);
            }
            "A_SetTics" => {
                let args = st
                    .map(|state| state.action_args.as_slice())
                    .unwrap_or_default();
                player.pspr_flash_tics = action_i32(args, 0, player.pspr_flash_tics);
            }
            _ => {}
        }
    }
}

fn advance_overlay2(world: &World, player: &mut Player) {
    let Some(sid) = player.overlay2_state else {
        return;
    };
    if player.overlay2_tics < 0 {
        return;
    }
    if player.overlay2_tics > 0 {
        player.overlay2_tics -= 1;
    }
    if player.overlay2_tics != 0 {
        return;
    }
    let next = world.states.get(sid).and_then(|st| st.next);
    let Some(n) = next else {
        player.overlay2_state = None;
        player.overlay2_tics = 0;
        player.overlay2_sprite.clear();
        player.overlay2_frame = 0;
        return;
    };
    let Some(st) = world.states.get(n) else {
        player.overlay2_state = None;
        player.overlay2_tics = 0;
        player.overlay2_sprite.clear();
        return;
    };
    player.overlay2_state = Some(n);
    player.overlay2_tics = overlay2_frame_tics(player, world, i32::from(st.tics));
    if !crate::state::sprite_unchanged(&st.sprite) && !st.sprite.is_empty() {
        player.overlay2_sprite = st.sprite.clone();
        player.overlay2_frame = st.frame;
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
                Some("A_Light") => {
                    player.extralight = action_i32(&st.action_args, 0, 0).clamp(-20, 20);
                }
                Some("A_SetTics") => {
                    player.pspr_flash_tics = action_i32(&st.action_args, 0, player.pspr_flash_tics);
                }
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
    let next = if let Some(cur) = player
        .ready_weapon
        .and_then(|c| owned.iter().position(|&x| x == c))
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

/// GZDoom `EWeaponReadyFlags` (`docs/33` BD4).
const WRF_NOBOB: i32 = 1;
const WRF_NOSWITCH: i32 = 2;
const WRF_NOPRIMARY: i32 = 4;
const WRF_NOSECONDARY: i32 = 8;
const WRF_NOFIRE: i32 = WRF_NOPRIMARY | WRF_NOSECONDARY;
const WRF_ALLOWRELOAD: i32 = 16;
const WRF_ALLOWZOOM: i32 = 32;
const WRF_DISABLESWITCH: i32 = 64;
const WRF_ALLOWUSER1: i32 = 128;
const WRF_ALLOWUSER2: i32 = 256;
const WRF_ALLOWUSER3: i32 = 512;
const WRF_ALLOWUSER4: i32 = 1024;

fn parse_wrf_flags(raw: Option<&String>) -> i32 {
    let Some(s) = raw.map(|s| s.trim()).filter(|s| !s.is_empty()) else {
        return 0;
    };
    if let Ok(n) = s.parse::<i32>() {
        return n;
    }
    let mut flags = 0i32;
    for part in s.split('|') {
        let p = part.trim().trim_matches('"');
        flags |= match p.to_ascii_uppercase().as_str() {
            "WRF_NOBOB" => WRF_NOBOB,
            "WRF_NOSWITCH" => WRF_NOSWITCH,
            "WRF_NOPRIMARY" => WRF_NOPRIMARY,
            "WRF_NOSECONDARY" => WRF_NOSECONDARY,
            "WRF_NOFIRE" => WRF_NOFIRE,
            "WRF_ALLOWRELOAD" => WRF_ALLOWRELOAD,
            "WRF_ALLOWZOOM" => WRF_ALLOWZOOM,
            "WRF_DISABLESWITCH" => WRF_DISABLESWITCH,
            "WRF_ALLOWUSER1" => WRF_ALLOWUSER1,
            "WRF_ALLOWUSER2" => WRF_ALLOWUSER2,
            "WRF_ALLOWUSER3" => WRF_ALLOWUSER3,
            "WRF_ALLOWUSER4" => WRF_ALLOWUSER4,
            _ => 0,
        };
    }
    flags
}

fn weapon_ready_special_state(
    world: &World,
    wpn: room_core::ClassId,
    wrf: i32,
    buttons: Buttons,
) -> Option<room_core::StateId> {
    let try_label = |flag: i32, btn: u32, label: &str| -> Option<room_core::StateId> {
        if wrf & flag != 0 && buttons.any(btn) {
            world.states.find_label(wpn, label)
        } else {
            None
        }
    };
    try_label(WRF_ALLOWRELOAD, Buttons::RELOAD, "Reload")
        .or_else(|| try_label(WRF_ALLOWZOOM, Buttons::ZOOM, "Zoom"))
        .or_else(|| try_label(WRF_ALLOWUSER1, Buttons::USER1, "User1"))
        .or_else(|| try_label(WRF_ALLOWUSER2, Buttons::USER2, "User2"))
        .or_else(|| try_label(WRF_ALLOWUSER3, Buttons::USER3, "User3"))
        .or_else(|| try_label(WRF_ALLOWUSER4, Buttons::USER4, "User4"))
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
        if !has_ammo_mode(world, mo, wpn, alt) {
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
/// `Weapon.MinSelectionAmmo1/2`: automatic switching skips a weapon with less.
fn below_min_selection_ammo(world: &World, mo: ActorId, weapon: room_core::ClassId) -> bool {
    let Some(def) = world.classes.get(weapon) else {
        return false;
    };
    let (min1, min2) = def.ext.min_selection_ammo;
    let low = |ammo: &str, min: i32| {
        min > 0
            && !ammo.is_empty()
            && world
                .classes
                .find_name(ammo)
                .is_some_and(|a| world.inventory.count(mo, a) < min)
    };
    low(&def.ammo_type, min1) || low(&def.ammo_type2, min2)
}

fn best_weapon_with_ammo(
    world: &World,
    mo: ActorId,
    exclude: Option<room_core::ClassId>,
) -> Option<room_core::ClassId> {
    let mut best: Option<(i32, room_core::ClassId)> = None;
    for cid in owned_weapon_order(world, mo) {
        if exclude == Some(cid)
            || !has_ammo(world, mo, cid)
            || below_min_selection_ammo(world, mo, cid)
        {
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

/// Air supply and falling scream (UZDoom `P_PlayerThink` / `PlayerPawn`).
///
/// * `Player.AirCapacity` × the level air supply (20 s): with the head under
///   water (`waterlevel` 3) the player then takes `2 + seconds over` damage
///   every 32 tics, and gasps (`*gasp`) on surfacing after running out.
/// * `Player.FallingScreamSpeed min, max` (default 35, 40): falling between
///   those speeds plays `*falling` once per fall.
fn player_environment_tick(world: &mut World, player: &mut Player, mo: ActorId) {
    let Some(a) = world.actors.get(mo) else {
        return;
    };
    if a.health <= 0 {
        player.falling_scream = false;
        return;
    }
    let ext = world
        .classes
        .get(a.class)
        .map(|c| c.ext.clone())
        .unwrap_or_default();
    let water = crate::physics::actor_waterlevel(world, a);
    let vz = a.vel.z();
    let now = world.level.maptime as i32;
    let capacity = ext.stored_f64("player.aircapacity").unwrap_or(1.0);
    let supply = (20.0 * 35.0 * capacity) as i32;
    let protected = player.god || player.invulnerable;
    if water < 3 || protected || supply <= 0 {
        if player.air_finished != 0 && player.air_finished <= now && water < 3 {
            play_player_sound(world, mo, "*gasp");
        }
        player.air_finished = if supply > 0 {
            now.saturating_add(supply)
        } else {
            i32::MAX
        };
    } else {
        if player.air_finished == 0 {
            player.air_finished = now.saturating_add(supply);
        }
        if player.air_finished <= now && now & 31 == 0 {
            let over = (now - player.air_finished) / 35;
            world.forced_damage_type = Some("Drowning".to_owned());
            crate::damage::damage_mobj(world, mo, None, None, 2 + over);
            world.forced_damage_type = None;
        }
    }
    let lo = ext
        .stored_f64_at("player.fallingscreamspeed", 0)
        .unwrap_or(35.0);
    let hi = ext
        .stored_f64_at("player.fallingscreamspeed", 1)
        .unwrap_or(40.0);
    if water == 0 && vz <= -lo && vz >= -hi {
        if !player.falling_scream {
            player.falling_scream = true;
            play_player_sound(world, mo, "*falling");
        }
    } else if vz > -lo {
        player.falling_scream = false;
    }
}

fn play_player_sound(world: &mut World, mo: ActorId, name: &str) {
    if let Some(a) = world.actors.get(mo) {
        world.play_sound(name, a.pos.x(), a.pos.y(), a.pos.z(), 1.0);
    }
}

/// `STYLE_Normal` is 1 and `STYLE_Fuzzy` is 2. Unknown codes are normal.
fn render_style_code(mode: i32) -> RenderStyle {
    match mode {
        0 => RenderStyle::None,
        2 => RenderStyle::Fuzzy,
        3 => RenderStyle::SoulTrans,
        4 => RenderStyle::OptFuzzy,
        5 => RenderStyle::Stencil,
        6 => RenderStyle::Translucent,
        7 => RenderStyle::Add,
        8 => RenderStyle::Shaded,
        9 => RenderStyle::TranslucentStencil,
        10 => RenderStyle::Shadow,
        11 => RenderStyle::Subtract,
        _ => RenderStyle::Normal,
    }
}

/// `WOF_KEEPX` is 1, `WOF_KEEPY` is 2, `WOF_ADD` is 4. A missing scale is 1×1.
fn scaled_pair(current: Option<(f64, f64)>, sx: f64, sy: f64, flags: i32) -> (f64, f64) {
    const KEEP_X: i32 = 1;
    const KEEP_Y: i32 = 2;
    const ADD: i32 = 4;
    let (ox, oy) = current.unwrap_or((1.0, 1.0));
    let nx = if flags & KEEP_X != 0 {
        ox
    } else if flags & ADD != 0 {
        ox + sx
    } else {
        sx
    };
    let ny = if flags & KEEP_Y != 0 {
        oy
    } else if flags & ADD != 0 {
        oy + sy
    } else {
        sy
    };
    (nx, ny)
}

/// Layer-2 `A_OverlayOffset`. Same flag bits as [`apply_weapon_offset`].
fn apply_overlay2_offset(player: &mut Player, wx: f64, wy: f64, flags: i32) {
    const KEEP_X: i32 = 1;
    const KEEP_Y: i32 = 2;
    const ADD: i32 = 4;
    if flags & KEEP_X == 0 {
        if flags & ADD != 0 {
            player.overlay2_ofs_x += wx;
        } else {
            player.overlay2_ofs_x = wx;
        }
    }
    if flags & KEEP_Y == 0 {
        if flags & ADD != 0 {
            player.overlay2_ofs_y += wy;
        } else {
            player.overlay2_ofs_y = wy - crate::player::WEAPON_TOP;
        }
    }
}

/// `A_WeaponOffset` / weapon-layer `A_OverlayOffset`.
/// `WOF_KEEPX` is 1, `WOF_KEEPY` is 2, `WOF_ADD` is 4.
fn apply_weapon_offset(player: &mut Player, wx: f64, wy: f64, flags: i32) {
    const KEEP_X: i32 = 1;
    const KEEP_Y: i32 = 2;
    const ADD: i32 = 4;
    if flags & KEEP_X == 0 {
        if flags & ADD != 0 {
            player.weapon_ofs_x += wx;
        } else {
            player.weapon_ofs_x = wx;
        }
    }
    if flags & KEEP_Y == 0 {
        if flags & ADD != 0 {
            player.weapon_ofs_y += wy;
        } else {
            player.weapon_ofs_y = wy - crate::player::WEAPON_TOP;
        }
    }
}

fn action_f64(args: &[String], index: usize, default: f64) -> f64 {
    args.get(index)
        .and_then(|s| s.trim().trim_matches('"').parse().ok())
        .unwrap_or(default)
}

fn action_i32(args: &[String], index: usize, default: i32) -> i32 {
    args.get(index)
        .and_then(|s| {
            let t = s.trim().trim_matches('"');
            t.parse().ok().or_else(|| {
                // `PSPF_ADDWEAPON|PSPF_ADDBOB`, `WOF_ADD`, `PSP_WEAPON`, …
                crate::uz_constants::eval_int(t).map(|v| v as i32)
            })
        })
        .unwrap_or(default)
}

fn set_reload_counter(player: &mut Player, weapon: room_core::ClassId, value: i32) {
    if let Some((_, counter)) = player
        .reload_counters
        .iter_mut()
        .find(|(class, _)| *class == weapon)
    {
        *counter = value;
    } else {
        player.reload_counters.push((weapon, value));
    }
}

/// `A_ReFire`'s second argument. Omitted means switch when the gun is empty.
fn refire_auto_switch(args: &[String]) -> bool {
    args.get(1).is_none_or(|s| {
        let t = s.trim().trim_matches('"');
        !t.eq_ignore_ascii_case("false") && t != "0"
    })
}

/// True if the player has at least `Weapon.AmmoUse` of the weapon's ammo type.
pub(crate) fn has_ammo(world: &World, mo: ActorId, weapon: room_core::ClassId) -> bool {
    has_ammo_mode(world, mo, weapon, false)
}

/// Primary (`alt=false`) or secondary (`AmmoType2` / `AmmoUse2`) ammo check.
pub(crate) fn has_ammo_mode(
    world: &World,
    mo: ActorId,
    weapon: room_core::ClassId,
    alt: bool,
) -> bool {
    let Some(def) = world.classes.get(weapon) else {
        return true;
    };
    let (ty, need) = if alt && !def.ammo_type2.is_empty() {
        (def.ammo_type2.as_str(), def.ammo_use2)
    } else {
        (def.ammo_type.as_str(), def.ammo_use)
    };
    if ty.is_empty() || need <= 0 {
        return true;
    }
    let Some(ammo) = world.classes.find_name(ty) else {
        return false;
    };
    world.inventory.count(mo, ammo) >= need
}

/// Spend `Weapon.AmmoUse` of `Weapon.AmmoType`. Returns false if not enough ammo.
/// Called from fire actions (docs/30 W2/W14), not from `P_FireWeapon`.
#[allow(dead_code)]
pub(crate) fn deplete_ammo(world: &mut World, mo: ActorId, weapon: room_core::ClassId) -> bool {
    try_consume_ammo(world, mo, weapon, false)
}

/// Spend primary or secondary ammo (BD4 dual ammo).
pub(crate) fn deplete_ammo_mode(
    world: &mut World,
    mo: ActorId,
    weapon: room_core::ClassId,
    alt: bool,
) -> bool {
    try_consume_ammo(world, mo, weapon, alt)
}

fn try_consume_ammo(world: &mut World, mo: ActorId, weapon: room_core::ClassId, alt: bool) -> bool {
    let Some(def) = world.classes.get(weapon) else {
        return true;
    };
    // `PowerInfiniteAmmo` (and `+WEAPON.AMMO_OPTIONAL` weapons with none left
    // are handled by the ammo checks below).
    if crate::powerups::infinite_ammo(world, mo) {
        return true;
    }
    let (ty, need) = if alt && !def.ammo_type2.is_empty() {
        (def.ammo_type2.as_str(), def.ammo_use2)
    } else {
        (def.ammo_type.as_str(), def.ammo_use)
    };
    if need <= 0 || ty.is_empty() {
        return true;
    }
    let Some(ammo) = world.classes.find_name(ty) else {
        return true;
    };
    let have = world.inventory.count(mo, ammo);
    if have < need {
        if std::env::var_os("ROOM_TRACE_AMMO").is_some() {
            eprintln!(
                "AMMOFAIL tic={} wpn={} have={} need={}",
                world.level.time, def.name, have, need
            );
        }
        return false;
    }
    let taken = world.inventory.take(mo, ammo, need);
    if std::env::var_os("ROOM_TRACE_AMMO").is_some() {
        eprintln!(
            "AMMOUSE tic={} wpn={} use={} had={} now={}",
            world.level.time,
            def.name,
            need,
            have,
            world.inventory.count(mo, ammo)
        );
    }
    taken >= need
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
            if !(4..=5).contains(&frame) {
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
    // Doom and GZDoom play nothing when walking into a wall (only a failed
    // *use* grunts); a bump sound here repeated every tic while held.
    let _ = try_move(world, mo, new_pos);
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
    fn inventory_bar_selects_cycles_and_uses_items() {
        let src = format!(
            "{}\nactor CustomInventory : Actor\n{{\n}}\nACTOR BarMedkit : CustomInventory\n{{\n  Inventory.MaxAmount 5\n  +INVENTORY.INVBAR\n  States\n  {{\n  Spawn:\n    MEDI A -1\n    Stop\n  Use:\n    TNT1 A 0 A_GiveInventory(\"Clip\", 3)\n    Stop\n  }}\n}}\nACTOR BarTorch : CustomInventory\n{{\n  Inventory.MaxAmount 5\n  +INVENTORY.INVBAR\n  States\n  {{\n  Spawn:\n    MEDI A -1\n    Stop\n  Use:\n    TNT1 A 0\n    Stop\n  }}\n}}\n",
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
        let kit = classes.find_name("BarMedkit").unwrap();
        let torch = classes.find_name("BarTorch").unwrap();
        let clip = classes.find_name("Clip").unwrap();
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        let _ = give_inventory(&mut w, mo, kit, 2);
        let _ = give_inventory(&mut w, mo, clip, 5);
        let _ = give_inventory(&mut w, mo, torch, 1);
        // Only bar items, first pickup first.
        assert_eq!(crate::invbar::invbar_items(&w, mo), vec![kit, torch]);
        let clip_before = crate::inventory::count_inventory(&w, mo, clip);
        let mut player = w.players[0].take().unwrap();
        player_think(&mut w, &mut player);
        assert_eq!(player.inv_sel, Some(kit), "first item selected on its own");
        player.cmd.buttons = Buttons(Buttons::INVNEXT);
        player_think(&mut w, &mut player);
        assert_eq!(player.inv_sel, Some(torch));
        assert!(player.inventory_tics > 0, "cycling opens the bar");
        player.cmd.buttons = Buttons(Buttons::INVNEXT);
        player_think(&mut w, &mut player);
        assert_eq!(player.inv_sel, Some(kit), "wraps");
        // Use while open only closes the bar.
        player.cmd.buttons = Buttons(Buttons::INVUSE);
        player_think(&mut w, &mut player);
        assert_eq!(player.inventory_tics, 0);
        assert_eq!(crate::inventory::count_inventory(&w, mo, kit), 2);
        player.cmd.buttons = Buttons(0);
        player_think(&mut w, &mut player);
        player.cmd.buttons = Buttons(Buttons::INVUSE);
        player_think(&mut w, &mut player);
        assert_eq!(crate::inventory::count_inventory(&w, mo, kit), 1);
        assert_eq!(
            crate::inventory::count_inventory(&w, mo, clip),
            clip_before + 3
        );
        w.players[0] = Some(player);
        let items = w.status_bar_items(0).unwrap();
        assert_eq!(items.inv_sel.as_deref(), Some("BarMedkit"));
        assert_eq!(
            items.inv_bar,
            vec!["BarMedkit".to_owned(), "BarTorch".to_owned()]
        );
    }

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

    /// GZDoom `P_BringUpWeapon` with no pending weapon: `A_Lower` at the
    /// bottom brings the ready weapon straight back up in its Ready state
    /// (Brutal Doom's M202 calls `A_Lower` from `Select`).
    #[test]
    fn a_lower_without_pending_weapon_returns_to_ready() {
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
        let mut w =
            World::from_map_with_classes(map, Arc::clone(&classes), Arc::clone(&states)).unwrap();
        let fist = classes.find_name("Fist").unwrap();
        let pistol = classes.find_name("Pistol").unwrap();
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        let mut player = w.players[0].take().unwrap();
        player.ready_weapon = Some(pistol);
        player.pending_weapon = None;
        player.pspr_y = crate::player::WEAPON_BOTTOM;
        a_lower(&mut w, &mut player, mo);
        let ready = states.find_label(pistol, "Ready").unwrap();
        assert!(
            player.pspr_state.is_some(),
            "the player is left with no weapon"
        );
        assert_eq!(player.ready_weapon, Some(pistol));
        assert_eq!(player.pspr_y, crate::player::WEAPON_TOP);
        assert_eq!(
            player
                .pspr_state
                .and_then(|s| w.states.get(s))
                .map(|s| s.sprite.clone()),
            w.states.get(ready).map(|s| s.sprite.clone())
        );
        // A pending weapon still wins.
        player.pending_weapon = Some(fist);
        player.pspr_y = crate::player::WEAPON_BOTTOM;
        a_lower(&mut w, &mut player, mo);
        assert_eq!(player.ready_weapon, Some(fist));
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

    fn doom_world(ceiling: f64) -> (World, Player, ActorId) {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let file = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&file).unwrap());
        let states = Arc::new(StateTable::from_decorate(&file, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, ceiling)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let mut player = w.players[0].take().unwrap();
        let mo = player.mo.unwrap();
        player.on_ground = true;
        // Like `World::tick`: the thinking slot is out of `players`.
        w.taken_player = Some((0, mo));
        (w, player, mo)
    }

    #[test]
    fn doom_player_crouches_to_half_height_and_stands_up() {
        let (mut w, mut player, mo) = doom_world(128.0);
        let full = w.actors.get(mo).unwrap().height;
        let stand_shot = crate::hitscan::shoot_origin(&w, mo).unwrap().0.z();
        player.cmd.up_move = -256;
        for _ in 0..6 {
            player_think(&mut w, &mut player);
        }
        assert!(player.crouching);
        assert!((player.crouch_amount - 1.0).abs() < 1e-9);
        let crouch_shot = crate::hitscan::shoot_origin(&w, mo).unwrap().0.z();
        assert!(
            crouch_shot < stand_shot - 10.0,
            "shots leave from the crouched eye: {stand_shot} -> {crouch_shot}"
        );
        let h = w.actors.get(mo).unwrap().height;
        assert!(
            (h - full * 0.5).abs() < 0.01,
            "crouched height {h} of {full}"
        );
        let eye = player.view_z - w.actors.get(mo).unwrap().pos.z();
        assert!(eye < player.view_height * 0.6, "crouched eye {eye}");
        player.cmd.up_move = 0;
        for _ in 0..6 {
            player_think(&mut w, &mut player);
        }
        assert!(!player.crouching);
        assert!((w.actors.get(mo).unwrap().height - full).abs() < 0.01);
        w.players[0] = Some(player);
    }

    #[test]
    fn doom_crouch_stays_down_under_a_low_ceiling_and_obeys_nocrouch() {
        let (mut w, mut player, mo) = doom_world(128.0);
        let full = w.actors.get(mo).unwrap().height;
        player.cmd.up_move = -256;
        for _ in 0..6 {
            player_think(&mut w, &mut player);
        }
        // Drop the ceiling under standing height: letting go keeps the crouch.
        let sec = 0; // the single room
        w.sectors[sec].ceiling_z = full * 0.75;
        player.cmd.up_move = 0;
        for _ in 0..6 {
            player_think(&mut w, &mut player);
        }
        assert!(player.crouching, "no headroom to stand");
        w.sectors[sec].ceiling_z = 128.0;
        for _ in 0..6 {
            player_think(&mut w, &mut player);
        }
        assert!(!player.crouching);

        w.no_crouch = true;
        player.cmd.up_move = -256;
        player_think(&mut w, &mut player);
        assert!(!player.crouching, "MAPINFO NoCrouch");
        w.players[0] = Some(player);
    }

    #[test]
    fn bot_seats_get_a_callsign_and_humans_keep_theirs() {
        let (mut w, player, _) = doom_world(128.0);
        w.players[0] = Some(player);
        w.taken_player = None;
        w.rng.set_seed(7);
        w.set_seat_bot(0, true);
        let named = String::from_utf8(w.player_names.stored(0).to_vec()).unwrap();
        assert!(crate::bot::BOT_NAMES.contains(&named.as_str()), "{named}");
        assert_eq!(w.humans_alive(), 0, "only a bot");
        w.set_seat_bot(0, false);
        assert!(
            w.player_names.stored(0).is_empty(),
            "back to the fallback name"
        );
        assert_eq!(w.humans_alive(), 1);
        // A session name is never replaced.
        w.player_names.set(0, "Anthony");
        w.set_seat_bot(0, true);
        assert_eq!(w.player_names.stored(0), b"Anthony");
    }

    #[test]
    fn countdown_ammo_and_weapon_are_given_back() {
        let (mut w, player, mo) = doom_world(128.0);
        w.players[0] = Some(player);
        w.taken_player = None;
        let clip = w.classes.find_name("Clip").unwrap();
        let before = w.inventory.count(mo, clip);
        assert!(before > 0, "starts with bullets");
        let ready = w.players[0].as_ref().unwrap().ready_weapon;
        w.save_player_inventories();
        // Shots fired during the countdown, and a weapon switch.
        let _ = w.inventory.take(mo, clip, before - 1);
        w.players[0].as_mut().unwrap().ready_weapon = w.classes.find_name("Fist");
        w.save_player_inventories(); // a later save keeps the first copy
        w.restore_player_inventories();
        assert_eq!(w.inventory.count(mo, clip), before, "ammo refilled");
        assert_eq!(w.players[0].as_ref().unwrap().ready_weapon, ready);
        assert!(w.saved_inventories.is_empty(), "restored once");
    }

    #[test]
    fn nojump_map_flag_blocks_jumping() {
        let (mut w, mut player, mo) = doom_world(128.0);
        w.no_jump = true;
        player.cmd.buttons = Buttons(Buttons::JUMP);
        player_think(&mut w, &mut player);
        assert!(w.actors.get(mo).unwrap().vel.z() <= 0.0, "NoJump");
        w.no_jump = false;
        player.cmd.buttons = Buttons(0);
        player_think(&mut w, &mut player);
        player.on_ground = true;
        player.cmd.buttons = Buttons(Buttons::JUMP);
        player_think(&mut w, &mut player);
        assert!(
            w.actors.get(mo).unwrap().vel.z() > 0.0,
            "jump allowed by default"
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
    fn blocked_move_is_silent() {
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
        }
        w.players[0] = Some(player);
        assert!(
            !sounds.iter().any(|n| n == "*noway"),
            "walking into a wall must not grunt (GZDoom), got {sounds:?}"
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
    fn wrf_nofire_and_reload_state() {
        let mut src = include_str!("../../../testdata/actors/doom_basic.txt").to_owned();
        src.push_str(
            r#"
actor TestGun : Weapon
{
  Weapon.AmmoType1 "Clip"
  Weapon.AmmoUse1 1
  Weapon.AmmoType2 "Clip"
  Weapon.AmmoUse2 2
  States
  {
  Ready:
    PISG A 1 A_WeaponReady(WRF_NOFIRE|WRF_ALLOWRELOAD|WRF_NOBOB)
    Loop
  Fire:
    PISG B 4
    Goto Ready
  Reload:
    PISG C 4
    Goto Ready
  }
}
"#,
        );
        let file = parse_decorate(&src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&file).unwrap());
        let states = Arc::new(StateTable::from_decorate(&file, &classes));
        let gun = classes.find_name("TestGun").unwrap();
        let def = classes.get(gun).unwrap();
        assert_eq!(def.ammo_type, "Clip");
        assert_eq!(def.ammo_use, 1);
        assert_eq!(def.ammo_type2, "Clip");
        assert_eq!(def.ammo_use2, 2);
        assert_eq!(
            parse_wrf_flags(Some(&"WRF_NOFIRE|WRF_ALLOWRELOAD".into())),
            WRF_NOFIRE | WRF_ALLOWRELOAD
        );

        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w =
            World::from_map_with_classes(map, Arc::clone(&classes), Arc::clone(&states)).unwrap();
        let mut player = w.players[0].take().unwrap();
        let mo = player.mo.unwrap();
        player.ready_weapon = Some(gun);
        player.pspr_y = crate::player::WEAPON_TOP;
        let ready = states.find_label(gun, "Ready").unwrap();
        let reload = states.find_label(gun, "Reload").unwrap();
        set_psprite(&mut w, &mut player, mo, ready);
        // Attack must be ignored under WRF_NOFIRE.
        player.cmd.buttons = Buttons(Buttons::ATTACK);
        player_think(&mut w, &mut player);
        assert_eq!(player.pspr_state, Some(ready), "NOFIRE must block Attack");
        // Reload button → Reload state.
        player.cmd.buttons = Buttons(Buttons::RELOAD);
        player_think(&mut w, &mut player);
        assert_eq!(
            player.pspr_state,
            Some(reload),
            "ALLOWRELOAD+RELOAD → Reload"
        );
        w.players[0] = Some(player);
    }

    #[test]
    fn overlay_weapon_without_a_raise_starts_on_screen() {
        let mut src = include_str!("../../../testdata/actors/doom_basic.txt").to_owned();
        src.push_str(
            r#"
actor OverlayGun : Weapon
{
  States
  {
  Select:
    TNT1 A 1
    Goto Ready
  Ready:
    PIST A 1 A_WeaponReady
    Loop
  }
}
"#,
        );
        let file = parse_decorate(&src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&file).unwrap());
        let states = Arc::new(StateTable::from_decorate(&file, &classes));
        let gun = classes.find_name("OverlayGun").unwrap();
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, classes, states).unwrap();
        let mut player = w.players[0].take().unwrap();
        let mo = player.mo.unwrap();
        player.ready_weapon = Some(gun);
        player.pspr_state = None;
        player.pspr_y = crate::player::WEAPON_BOTTOM;
        player_think(&mut w, &mut player);
        assert!(
            (player.pspr_y - crate::player::WEAPON_TOP).abs() < 0.5,
            "draw anim without A_Raise must sit at WEAPONTOP, y={}",
            player.pspr_y
        );
        w.players[0] = Some(player);
        let _ = mo;
    }

    #[test]
    fn long_zero_tic_reload_chain_finishes_in_one_tic() {
        // Brutal Doom `InsertBullets`: each round goes through a handful of
        // zero-tic states, all in one tic (UZDoom `DPSprite::SetState` loops
        // while `Tics == 0`). A 64-state cap stopped after a few rounds and
        // showed the `TNT1` frame in the middle of the reload.
        let mut src = include_str!("../../../testdata/actors/doom_basic.txt").to_owned();
        src.push_str(
            r#####"
actor Round : Actor
{
  Inventory.MaxAmount 30
}
actor LoaderGun : Weapon
{
  States
  {
  Ready:
    PIST A 1 A_WeaponReady
    Loop
  Reload:
  InsertBullets:
    TNT1 AAAA 0
    PIST A 0 A_JumpIfInventory("Round", 30, "Done")
    TNT1 AAAAAA 0
    PIST A 0 A_GiveInventory("Round", 1)
    Goto InsertBullets
  Done:
    PIST B 5
    Goto Ready
  }
}
"#####,
        );
        let file = parse_decorate(&src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&file).unwrap());
        let states = Arc::new(StateTable::from_decorate(&file, &classes));
        let gun = classes.find_name("LoaderGun").unwrap();
        let round = classes.find_name("Round").unwrap();
        let reload = states.find_label(gun, "Reload").unwrap();
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w =
            World::from_map_with_classes(map, Arc::clone(&classes), Arc::clone(&states)).unwrap();
        let mut player = w.players[0].take().unwrap();
        let mo = player.mo.unwrap();
        player.ready_weapon = Some(gun);
        player.pspr_y = crate::player::WEAPON_TOP;
        set_psprite(&mut w, &mut player, mo, reload);
        assert_eq!(w.inventory.count(mo, round), 30, "all 30 rounds in one tic");
        assert_eq!(
            (player.pspr_sprite.as_str(), player.pspr_frame),
            ("PIST", 1)
        );
        assert_eq!(player.pspr_tics, 5);
        w.players[0] = Some(player);
    }

    #[test]
    fn keep_sprite_and_inventory_offset_pick_the_gun_frame() {
        // Brutal Doom Ready: offset jump, then `"####" "#"` holds the frame.
        let mut src = include_str!("../../../testdata/actors/doom_basic.txt").to_owned();
        src.push_str(
            r#####"
actor Mag : Actor
{
  Inventory.MaxAmount 16
}
actor BdGun : Weapon
{
  States
  {
  Ready:
    PIST E 0 A_JumpIfInventory("Mag", 1, 2)
    PIST I 0
    "####" "#" 0
    "####" "#" 1 A_WeaponReady
    Loop
  }
}
"#####,
        );
        let file = parse_decorate(&src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&file).unwrap());
        let states = Arc::new(StateTable::from_decorate(&file, &classes));
        let gun = classes.find_name("BdGun").unwrap();
        let mag = classes.find_name("Mag").unwrap();
        let ready = states.find_label(gun, "Ready").unwrap();
        let hold = states.get(ready).unwrap().next.unwrap();
        let hold = states.get(hold).unwrap().next.unwrap();
        assert_eq!(states.get(hold).unwrap().sprite, "####");
        assert_eq!(
            states.get(hold).unwrap().frame,
            crate::state::FRAME_NOCHANGE
        );

        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w =
            World::from_map_with_classes(map, Arc::clone(&classes), Arc::clone(&states)).unwrap();
        let mut player = w.players[0].take().unwrap();
        let mo = player.mo.unwrap();
        player.ready_weapon = Some(gun);
        player.pspr_y = crate::player::WEAPON_TOP;
        set_psprite(&mut w, &mut player, mo, ready);
        assert_eq!(player.pspr_sprite, "PIST");
        assert_eq!(player.pspr_frame, 8, "no mag stays on the empty frame");
        w.players[0] = Some(player);
        assert_eq!(w.player_hud(0).unwrap().weapon_patch, "PISTI0");

        let _ = give_inventory(&mut w, mo, mag, 1);
        let mut player = w.players[0].take().unwrap();
        player.pspr_sprite.clear();
        set_psprite(&mut w, &mut player, mo, ready);
        assert_eq!(player.pspr_sprite, "PIST");
        assert_eq!(player.pspr_frame, 4, "mag jumps to the loaded frame");
        w.players[0] = Some(player);
        assert_eq!(w.player_hud(0).unwrap().weapon_patch, "PISTE0");
    }

    #[test]
    fn bd4_weapon_impulses_zoom_user_and_crosshair() {
        // docs/33 BD4: Zoom/User1/User3 labels + A_ZoomFactor / A_SetCrosshair.
        let mut src = include_str!("../../../testdata/actors/doom_basic.txt").to_owned();
        src.push_str(
            r#"
actor BdPistol : Weapon
{
  States
  {
  Ready:
    PISG A 1 A_WeaponReady(WRF_ALLOWZOOM|WRF_ALLOWUSER1|WRF_ALLOWUSER3)
    Loop
  Fire:
    PISG B 4
    Goto Ready
  Zoom:
    PISG D 4
    Goto Ready
  User1:
    PISG E 4
    Goto Ready
  User3:
    PISG F 4
    Goto Ready
  }
}
"#,
        );
        let file = parse_decorate(&src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&file).unwrap());
        let states = Arc::new(StateTable::from_decorate(&file, &classes));
        let gun = classes.find_name("BdPistol").unwrap();
        let zoom = states.find_label(gun, "Zoom").unwrap();
        let user1 = states.find_label(gun, "User1").unwrap();
        let user3 = states.find_label(gun, "User3").unwrap();
        let ready = states.find_label(gun, "Ready").unwrap();
        assert_eq!(
            parse_wrf_flags(Some(&"WRF_ALLOWZOOM|WRF_ALLOWUSER1|WRF_ALLOWUSER3".into())),
            WRF_ALLOWZOOM | WRF_ALLOWUSER1 | WRF_ALLOWUSER3
        );

        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w =
            World::from_map_with_classes(map, Arc::clone(&classes), Arc::clone(&states)).unwrap();
        let mut player = w.players[0].take().unwrap();
        let mo = player.mo.unwrap();
        player.ready_weapon = Some(gun);
        player.pspr_y = crate::player::WEAPON_TOP;

        player.cmd.buttons = Buttons(Buttons::ZOOM);
        set_psprite(&mut w, &mut player, mo, ready);
        player_think(&mut w, &mut player);
        assert_eq!(player.pspr_state, Some(zoom), "ALLOWZOOM+ZOOM → Zoom");

        player.cmd.buttons = Buttons(Buttons::USER1);
        set_psprite(&mut w, &mut player, mo, ready);
        player_think(&mut w, &mut player);
        assert_eq!(player.pspr_state, Some(user1), "grenade / +user1");

        player.cmd.buttons = Buttons(Buttons::USER3);
        set_psprite(&mut w, &mut player, mo, ready);
        player_think(&mut w, &mut player);
        assert_eq!(player.pspr_state, Some(user3), "dual-wield / +user3");

        w.players[0] = Some(player);
        let _ = crate::action::call_action_args(&mut w, mo, "A_SetCrosshair", &["2".into()]);
        let _ = crate::action::call_action_args(&mut w, mo, "A_ZoomFactor", &["2.0".into()]);
        let p = w.players[0].as_ref().unwrap();
        assert_eq!(p.crosshair, 2);
        assert!((p.weapon_zoom - 2.0).abs() < 1e-5);
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
    fn refire_false_autoswitch_keeps_the_empty_weapon() {
        let src = format!(
            "{}\nACTOR NoSwitchPistol : Pistol\n{{\n  States\n  {{\n  Fire:\n    PISG A 1\n    PISG B 1 A_ReFire(null, false)\n    Goto Ready\n  }}\n}}\n",
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
        let pistol = classes.find_name("NoSwitchPistol").unwrap();
        let clip = classes.find_name("Clip").unwrap();
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        while w.inventory.count(mo, clip) > 0 {
            let _ = w.inventory.take(mo, clip, 1);
        }
        let mut player = w.players[0].take().unwrap();
        player.ready_weapon = Some(pistol);
        player.pending_weapon = None;
        player.pspr_y = crate::player::WEAPON_TOP;
        let fire = w.states.find_label(pistol, "Fire").unwrap();
        let action = w
            .states
            .get(fire)
            .unwrap()
            .next
            .and_then(|n| w.states.get(n))
            .and_then(|n| n.action.as_deref());
        assert_eq!(action, Some("A_ReFire"));
        player.pspr_state = Some(fire);
        player.pspr_tics = 1;
        player.attack_down = true;
        player.cmd.buttons = Buttons(Buttons::ATTACK);
        player_think(&mut w, &mut player);
        assert_eq!(player.pending_weapon, None);
        assert_eq!(player.ready_weapon, Some(pistol));
        w.players[0] = Some(player);
    }

    #[test]
    fn weapon_offset_moves_the_gun_and_keepx_holds_x() {
        let src = format!(
            "{}\nACTOR OffsetPistol : Pistol\n{{\n  States\n  {{\n  Fire:\n    PISG A 1\n    PISG A 1 A_WeaponOffset(10, 48)\n    PISG A 1 A_WeaponOffset(1, 2, 5)\n    Goto Ready\n  }}\n}}\n",
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
        let pistol = classes.find_name("OffsetPistol").unwrap();
        let mut player = w.players[0].take().unwrap();
        player.ready_weapon = Some(pistol);
        player.pending_weapon = None;
        player.pspr_y = crate::player::WEAPON_TOP;
        let fire = w.states.find_label(pistol, "Fire").unwrap();
        let next_action = w
            .states
            .get(fire)
            .unwrap()
            .next
            .and_then(|n| w.states.get(n))
            .and_then(|n| n.action.as_deref());
        assert_eq!(next_action, Some("A_WeaponOffset"));
        player.pspr_state = Some(fire);
        player.pspr_tics = 1;
        player_think(&mut w, &mut player);
        assert_eq!(player.weapon_ofs_x, 10.0);
        assert_eq!(player.weapon_ofs_y, 16.0);
        player_think(&mut w, &mut player);
        assert_eq!(player.weapon_ofs_x, 10.0);
        assert_eq!(player.weapon_ofs_y, 18.0);
        w.players[0] = Some(player);
        let hud = w.player_hud(0).unwrap();
        assert!((hud.bob_x - 10.0).abs() < 1e-4);
        assert!((hud.bob_y - 18.0).abs() < 1e-4);
    }

    #[test]
    fn overlay_force_style_keeps_layer_two_solid_while_the_player_is_fuzzy() {
        let src = format!(
            "{}\nACTOR Style2ForcePistol : Pistol\n{{\n  States\n  {{\n  Fire:\n    PISG A 1\n    PISG A 1 A_OverlayFlags(2, 256, true)\n    PISG A 1 A_OverlayRenderStyle(2, 2)\n    PISG A 1 A_OverlayRenderStyle(2, 1)\n    PISG A 1 A_OverlayFlags(2, 256, false)\n    PISG A 1 A_OverlayFlags(1, 256, true)\n    Goto Ready\n  }}\n}}\n",
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
        let pistol = classes.find_name("Style2ForcePistol").unwrap();
        let mut player = w.players[0].take().unwrap();
        player.blur = true;
        player.ready_weapon = Some(pistol);
        player.pending_weapon = None;
        player.pspr_y = crate::player::WEAPON_TOP;
        player.pspr_state = Some(w.states.find_label(pistol, "Fire").unwrap());
        player.pspr_tics = 1;
        w.players[0] = Some(player);
        let hud = w.player_hud(0).unwrap();
        assert!(hud.overlay_fuzz);
        assert!(hud.weapon_fuzz);
        let mut player = w.players[0].take().unwrap();
        player_think(&mut w, &mut player);
        assert!(player.overlay2_force_style);
        assert!(!player.weapon_force_style);
        w.players[0] = Some(player);
        let hud = w.player_hud(0).unwrap();
        assert!(!hud.overlay_fuzz);
        assert!(hud.weapon_fuzz);
        let mut player = w.players[0].take().unwrap();
        player_think(&mut w, &mut player);
        w.players[0] = Some(player);
        let hud = w.player_hud(0).unwrap();
        assert!(hud.overlay_fuzz);
        assert!(hud.weapon_fuzz);
        let mut player = w.players[0].take().unwrap();
        player_think(&mut w, &mut player);
        w.players[0] = Some(player);
        let hud = w.player_hud(0).unwrap();
        assert!(!hud.overlay_fuzz);
        assert!(hud.weapon_fuzz);
        let mut player = w.players[0].take().unwrap();
        player_think(&mut w, &mut player);
        assert!(!player.overlay2_force_style);
        w.players[0] = Some(player);
        let hud = w.player_hud(0).unwrap();
        assert!(hud.overlay_fuzz);
        assert!(hud.weapon_fuzz);
        let mut player = w.players[0].take().unwrap();
        player_think(&mut w, &mut player);
        assert!(player.weapon_force_style);
        assert!(!player.overlay2_force_style);
        w.players[0] = Some(player);
        let hud = w.player_hud(0).unwrap();
        assert!(!hud.weapon_fuzz);
        assert!(hud.overlay_fuzz);
    }

    #[test]
    fn overlay_force_alpha_keeps_layer_two_off_the_body() {
        let src = format!(
            "{}\nACTOR Alpha2PistolForce : Pistol\n{{\n  States\n  {{\n  Fire:\n    PISG A 1\n    PISG A 1 A_OverlayFlags(2, 128, true)\n    PISG A 1 A_OverlayFlags(1, 128, true)\n    PISG A 1 A_OverlayFlags(2, 128, false)\n    Goto Ready\n  }}\n}}\n",
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
        let pistol = classes.find_name("Alpha2PistolForce").unwrap();
        let mut player = w.players[0].take().unwrap();
        let mo = player.mo.unwrap();
        w.actors.get_mut(mo).unwrap().alpha = 0.25;
        player.overlay2_alpha = Some(0.5);
        player.ready_weapon = Some(pistol);
        player.pending_weapon = None;
        player.pspr_y = crate::player::WEAPON_TOP;
        player.pspr_state = Some(w.states.find_label(pistol, "Fire").unwrap());
        player.pspr_tics = 1;
        w.players[0] = Some(player);
        let hud = w.player_hud(0).unwrap();
        assert!((hud.overlay_alpha - 0.125).abs() < 1e-3);
        assert!((hud.weapon_alpha - 0.25).abs() < 1e-3);
        let mut player = w.players[0].take().unwrap();
        player_think(&mut w, &mut player);
        assert!(player.overlay2_force_alpha);
        assert!(!player.weapon_force_alpha);
        w.players[0] = Some(player);
        let hud = w.player_hud(0).unwrap();
        assert!((hud.overlay_alpha - 0.5).abs() < 1e-3);
        assert!((hud.weapon_alpha - 0.25).abs() < 1e-3);
        let mut player = w.players[0].take().unwrap();
        player_think(&mut w, &mut player);
        assert!(player.weapon_force_alpha);
        assert!(player.overlay2_force_alpha);
        w.players[0] = Some(player);
        let hud = w.player_hud(0).unwrap();
        assert!((hud.weapon_alpha - 1.0).abs() < 1e-3);
        assert!((hud.overlay_alpha - 0.5).abs() < 1e-3);
        let mut player = w.players[0].take().unwrap();
        player_think(&mut w, &mut player);
        assert!(!player.overlay2_force_alpha);
        assert!(player.weapon_force_alpha);
        w.players[0] = Some(player);
        let hud = w.player_hud(0).unwrap();
        assert!((hud.overlay_alpha - 0.125).abs() < 1e-3);
        assert!((hud.weapon_alpha - 1.0).abs() < 1e-3);
    }

    #[test]
    fn overlay_interpolate_off_snaps_layer_two() {
        let src = format!(
            "{}\nACTOR Ease2Pistol : Pistol\n{{\n  States\n  {{\n  Fire:\n    PISG A 1\n    PISG A 1 A_OverlayFlags(2, 4096, false)\n    PISG A 1 A_OverlayFlags(1, 4096, false)\n    PISG A 1 A_OverlayFlags(2, 4096, true)\n    Goto Ready\n  }}\n}}\n",
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
        let pistol = classes.find_name("Ease2Pistol").unwrap();
        let mut player = w.players[0].take().unwrap();
        assert!(player.overlay2_interpolate);
        player.ready_weapon = Some(pistol);
        player.pending_weapon = None;
        player.pspr_y = crate::player::WEAPON_TOP;
        player.pspr_state = Some(w.states.find_label(pistol, "Fire").unwrap());
        player.pspr_tics = 1;
        w.players[0] = Some(player);
        assert!(w.player_hud(0).unwrap().overlay_interpolate);
        assert!(w.player_hud(0).unwrap().weapon_interpolate);
        let mut player = w.players[0].take().unwrap();
        player_think(&mut w, &mut player);
        assert!(!player.overlay2_interpolate);
        assert!(player.weapon_interpolate);
        w.players[0] = Some(player);
        let hud = w.player_hud(0).unwrap();
        assert!(!hud.overlay_interpolate);
        assert!(hud.weapon_interpolate);
        let mut player = w.players[0].take().unwrap();
        player_think(&mut w, &mut player);
        assert!(!player.weapon_interpolate);
        assert!(!player.overlay2_interpolate);
        w.players[0] = Some(player);
        assert!(!w.player_hud(0).unwrap().weapon_interpolate);
        let mut player = w.players[0].take().unwrap();
        player_think(&mut w, &mut player);
        assert!(player.overlay2_interpolate);
        assert!(!player.weapon_interpolate);
        w.players[0] = Some(player);
        let hud = w.player_hud(0).unwrap();
        assert!(hud.overlay_interpolate);
        assert!(!hud.weapon_interpolate);
    }

    #[test]
    fn overlay_addweapon_off_drops_the_gun_offset_on_layer_two() {
        let src = format!(
            "{}\nACTOR Ofs2Pistol : Pistol\n{{\n  States\n  {{\n  Fire:\n    PISG A 1\n    PISG A 1 A_OverlayFlags(2, 1, false)\n    PISG A 1 A_OverlayFlags(1, 1, false)\n    PISG A 1 A_OverlayFlags(2, 1, true)\n    Goto Ready\n  }}\n}}\n",
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
        let pistol = classes.find_name("Ofs2Pistol").unwrap();
        let mut player = w.players[0].take().unwrap();
        assert!(player.overlay2_add_weapon);
        player.ready_weapon = Some(pistol);
        player.pending_weapon = None;
        player.pspr_y = crate::player::WEAPON_TOP;
        player.pspr_bob_x = 10.0;
        player.weapon_ofs_x = 3.0;
        player.overlay2_ofs_x = 6.0;
        player.pspr_state = Some(w.states.find_label(pistol, "Fire").unwrap());
        player.pspr_tics = 1;
        w.players[0] = Some(player);
        let hud = w.player_hud(0).unwrap();
        assert!((hud.bob_x - 13.0).abs() < 1e-3);
        assert!((hud.bob_ofs_x - 3.0).abs() < 1e-3);
        assert!(hud.overlay_add_weapon);
        let mut player = w.players[0].take().unwrap();
        player_think(&mut w, &mut player);
        assert!(!player.overlay2_add_weapon);
        assert!(player.weapon_add_ofs);
        w.players[0] = Some(player);
        let hud = w.player_hud(0).unwrap();
        assert!(!hud.overlay_add_weapon);
        assert!((hud.bob_x - 13.0).abs() < 1e-3);
        assert!((hud.bob_ofs_x - 3.0).abs() < 1e-3);
        let mut player = w.players[0].take().unwrap();
        player_think(&mut w, &mut player);
        assert!(!player.weapon_add_ofs);
        assert!(!player.overlay2_add_weapon);
        w.players[0] = Some(player);
        let hud = w.player_hud(0).unwrap();
        assert!((hud.bob_x - 10.0).abs() < 1e-3);
        assert!(hud.bob_ofs_x.abs() < 1e-3);
        let mut player = w.players[0].take().unwrap();
        player_think(&mut w, &mut player);
        assert!(player.overlay2_add_weapon);
        assert!(!player.weapon_add_ofs);
        w.players[0] = Some(player);
        let hud = w.player_hud(0).unwrap();
        assert!(hud.overlay_add_weapon);
        assert!((hud.bob_x - 10.0).abs() < 1e-3);
    }

    #[test]
    fn overlay_addbob_off_drops_sway_on_layer_two() {
        let src = format!(
            "{}\nACTOR Bob2Pistol : Pistol\n{{\n  States\n  {{\n  Fire:\n    PISG A 1\n    PISG A 1 A_OverlayFlags(2, 2, false)\n    PISG A 1 A_OverlayFlags(1, 2, false)\n    PISG A 1 A_OverlayFlags(2, 2, true)\n    Goto Ready\n  }}\n}}\n",
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
        let pistol = classes.find_name("Bob2Pistol").unwrap();
        let mut player = w.players[0].take().unwrap();
        assert!(player.overlay2_add_bob);
        player.ready_weapon = Some(pistol);
        player.pending_weapon = None;
        player.pspr_y = crate::player::WEAPON_TOP;
        player.pspr_bob_x = 10.0;
        player.weapon_ofs_x = 3.0;
        player.overlay2_ofs_x = 6.0;
        player.pspr_state = Some(w.states.find_label(pistol, "Fire").unwrap());
        player.pspr_tics = 1;
        w.players[0] = Some(player);
        let hud = w.player_hud(0).unwrap();
        assert!((hud.bob_x - 13.0).abs() < 1e-3);
        assert!(hud.overlay_add_bob);
        assert!((hud.overlay_ofs_x - 6.0).abs() < 1e-3);
        let mut player = w.players[0].take().unwrap();
        player_think(&mut w, &mut player);
        assert!(!player.overlay2_add_bob);
        assert!(player.weapon_add_bob);
        w.players[0] = Some(player);
        let hud = w.player_hud(0).unwrap();
        assert!(!hud.overlay_add_bob);
        assert!((hud.bob_x - 13.0).abs() < 1e-3);
        let mut player = w.players[0].take().unwrap();
        player_think(&mut w, &mut player);
        assert!(!player.weapon_add_bob);
        assert!(!player.overlay2_add_bob);
        w.players[0] = Some(player);
        let hud = w.player_hud(0).unwrap();
        assert!((hud.bob_x - 3.0).abs() < 1e-3);
        assert!(!hud.overlay_add_bob);
        let mut player = w.players[0].take().unwrap();
        player_think(&mut w, &mut player);
        assert!(player.overlay2_add_bob);
        assert!(!player.weapon_add_bob);
        w.players[0] = Some(player);
        let hud = w.player_hud(0).unwrap();
        assert!(hud.overlay_add_bob);
        assert!((hud.bob_x - 3.0).abs() < 1e-3);
    }

    #[test]
    fn overlay_pivot_percent_off_uses_pixels_on_layer_two() {
        let src = format!(
            "{}\nACTOR Pct2Pistol : Pistol\n{{\n  States\n  {{\n  Fire:\n    PISG A 1\n    PISG A 1 A_OverlayFlags(2, 2048, false)\n    PISG A 1 A_OverlayPivot(2, 4, 4)\n    PISG A 1 A_OverlayFlags(1, 2048, false)\n    PISG A 1 A_OverlayFlags(2, 2048, true)\n    Goto Ready\n  }}\n}}\n",
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
        let pistol = classes.find_name("Pct2Pistol").unwrap();
        let mut player = w.players[0].take().unwrap();
        assert!(player.overlay2_pivot_percent);
        player.ready_weapon = Some(pistol);
        player.pending_weapon = None;
        player.pspr_y = crate::player::WEAPON_TOP;
        player.pspr_state = Some(w.states.find_label(pistol, "Fire").unwrap());
        player.pspr_tics = 1;
        player_think(&mut w, &mut player);
        assert!(!player.overlay2_pivot_percent);
        assert!(player.weapon_pivot_percent);
        player_think(&mut w, &mut player);
        assert_eq!(player.overlay2_pivot, Some((4.0, 4.0)));
        player_think(&mut w, &mut player);
        assert!(!player.weapon_pivot_percent);
        assert!(!player.overlay2_pivot_percent);
        w.players[0] = Some(player);
        assert!(!w.player_hud(0).unwrap().overlay_pivot_percent);
        let mut player = w.players[0].take().unwrap();
        player_think(&mut w, &mut player);
        assert!(player.overlay2_pivot_percent);
        assert!(!player.weapon_pivot_percent);
    }

    #[test]
    fn overlay_render_style_fuzzes_layer_two_and_leaves_the_gun() {
        let src = format!(
            "{}\nACTOR Style2Pistol : Pistol\n{{\n  States\n  {{\n  Fire:\n    PISG A 1\n    PISG A 1 A_OverlayRenderStyle(2, 2)\n    PISG A 1 A_OverlayRenderStyle(1, 2)\n    PISG A 1 A_OverlayRenderStyle(2, 1)\n    Goto Ready\n  }}\n}}\n",
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
        let pistol = classes.find_name("Style2Pistol").unwrap();
        let mut player = w.players[0].take().unwrap();
        player.ready_weapon = Some(pistol);
        player.pending_weapon = None;
        player.pspr_y = crate::player::WEAPON_TOP;
        player.pspr_state = Some(w.states.find_label(pistol, "Fire").unwrap());
        player.pspr_tics = 1;
        player_think(&mut w, &mut player);
        assert_eq!(player.overlay2_style, Some(RenderStyle::Fuzzy));
        assert!(player.weapon_style.is_none());
        w.players[0] = Some(player);
        let hud = w.player_hud(0).unwrap();
        assert!(hud.overlay_fuzz);
        assert!(!hud.weapon_fuzz);
        let mut player = w.players[0].take().unwrap();
        player_think(&mut w, &mut player);
        w.players[0] = Some(player);
        let hud = w.player_hud(0).unwrap();
        assert!(hud.overlay_fuzz);
        assert!(hud.weapon_fuzz);
        let mut player = w.players[0].take().unwrap();
        player_think(&mut w, &mut player);
        w.players[0] = Some(player);
        let hud = w.player_hud(0).unwrap();
        assert!(!hud.overlay_fuzz);
        assert!(hud.weapon_fuzz);
    }

    #[test]
    fn overlay_translation_greens_layer_two_and_leaves_the_gun() {
        let src = format!(
            "{}\nACTOR Tint2Pistol : Pistol\n{{\n  States\n  {{\n  Fire:\n    PISG A 1\n    PISG A 1 A_OverlayTranslation(2, \"Green\")\n    PISG A 1 A_OverlayTranslation(1, \"Red\")\n    PISG A 1 A_OverlayTranslation(2, \"None\")\n    Goto Ready\n  }}\n}}\n",
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
        let pistol = classes.find_name("Tint2Pistol").unwrap();
        let mut player = w.players[0].take().unwrap();
        player.ready_weapon = Some(pistol);
        player.pending_weapon = None;
        player.pspr_y = crate::player::WEAPON_TOP;
        player.pspr_state = Some(w.states.find_label(pistol, "Fire").unwrap());
        player.pspr_tics = 1;
        player_think(&mut w, &mut player);
        assert_eq!(player.overlay2_translation, "Green");
        assert!(player.weapon_translation.is_empty());
        player_think(&mut w, &mut player);
        assert_eq!(player.weapon_translation, "Red");
        assert_eq!(player.overlay2_translation, "Green");
        w.players[0] = Some(player);
        let hud = w.player_hud(0).unwrap();
        assert!(hud.overlay_tint_g > hud.overlay_tint_r);
        assert!(hud.weapon_tint_r > hud.weapon_tint_g);
        let mut player = w.players[0].take().unwrap();
        player_think(&mut w, &mut player);
        assert!(player.overlay2_translation.is_empty());
        w.players[0] = Some(player);
        let hud = w.player_hud(0).unwrap();
        assert!((hud.overlay_tint_r - 1.0).abs() < 1e-3);
        assert!((hud.overlay_tint_g - 1.0).abs() < 1e-3);
        assert!(hud.weapon_tint_r > hud.weapon_tint_g);
    }

    #[test]
    fn overlay_pivot_align_pins_layer_two_and_leaves_the_gun() {
        let src = format!(
            "{}\nACTOR Align2Pistol : Pistol\n{{\n  States\n  {{\n  Fire:\n    PISG A 1\n    PISG A 1 A_OverlayPivotAlign(2, 2, 0)\n    PISG A 1 A_OverlayPivotAlign(1, 0, 2)\n    Goto Ready\n  }}\n}}\n",
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
        let pistol = classes.find_name("Align2Pistol").unwrap();
        let mut player = w.players[0].take().unwrap();
        player.ready_weapon = Some(pistol);
        player.pending_weapon = None;
        player.pspr_y = crate::player::WEAPON_TOP;
        player.pspr_state = Some(w.states.find_label(pistol, "Fire").unwrap());
        player.pspr_tics = 1;
        player_think(&mut w, &mut player);
        assert_eq!(player.overlay2_pivot, Some((1.0, 0.0)));
        assert!(player.weapon_pivot.is_none());
        player_think(&mut w, &mut player);
        assert_eq!(player.weapon_pivot, Some((0.0, 1.0)));
        assert_eq!(player.overlay2_pivot, Some((1.0, 0.0)));
        w.players[0] = Some(player);
        let hud = w.player_hud(0).unwrap();
        assert!((hud.overlay_pivot_x - 1.0).abs() < 1e-3);
        assert!(hud.overlay_pivot_y.abs() < 1e-3);
        assert!(hud.weapon_pivot_x.abs() < 1e-3);
        assert!((hud.weapon_pivot_y - 1.0).abs() < 1e-3);
    }

    #[test]
    fn overlay_pivot_moves_layer_two_and_leaves_the_gun() {
        let src = format!(
            "{}\nACTOR Piv2Pistol : Pistol\n{{\n  States\n  {{\n  Fire:\n    PISG A 1\n    PISG A 1 A_OverlayPivot(2, 0, 0)\n    PISG A 1 A_OverlayPivot(1, 0.25, 0.75)\n    Goto Ready\n  }}\n}}\n",
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
        let pistol = classes.find_name("Piv2Pistol").unwrap();
        let mut player = w.players[0].take().unwrap();
        player.ready_weapon = Some(pistol);
        player.pending_weapon = None;
        player.pspr_y = crate::player::WEAPON_TOP;
        player.pspr_state = Some(w.states.find_label(pistol, "Fire").unwrap());
        player.pspr_tics = 1;
        player_think(&mut w, &mut player);
        assert_eq!(player.overlay2_pivot, Some((0.0, 0.0)));
        assert!(player.weapon_pivot.is_none());
        player_think(&mut w, &mut player);
        assert_eq!(player.weapon_pivot, Some((0.25, 0.75)));
        assert_eq!(player.overlay2_pivot, Some((0.0, 0.0)));
        w.players[0] = Some(player);
        let hud = w.player_hud(0).unwrap();
        assert!(hud.overlay_pivot_x.abs() < 1e-3);
        assert!(hud.overlay_pivot_y.abs() < 1e-3);
        assert!((hud.weapon_pivot_x - 0.25).abs() < 1e-3);
        assert!((hud.weapon_pivot_y - 0.75).abs() < 1e-3);
    }

    #[test]
    fn overlay_rotate_turns_layer_two_and_leaves_the_gun() {
        let src = format!(
            "{}\nACTOR Rot2Pistol : Pistol\n{{\n  States\n  {{\n  Fire:\n    PISG A 1\n    PISG A 1 A_OverlayRotate(2, 90)\n    PISG A 1 A_OverlayRotate(1, 15)\n    Goto Ready\n  }}\n}}\n",
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
        let pistol = classes.find_name("Rot2Pistol").unwrap();
        let mut player = w.players[0].take().unwrap();
        player.ready_weapon = Some(pistol);
        player.pending_weapon = None;
        player.pspr_y = crate::player::WEAPON_TOP;
        player.pspr_state = Some(w.states.find_label(pistol, "Fire").unwrap());
        player.pspr_tics = 1;
        player_think(&mut w, &mut player);
        assert!((player.overlay2_angle - 90.0).abs() < 1e-6);
        assert!((player.weapon_angle).abs() < 1e-6);
        player_think(&mut w, &mut player);
        assert!((player.weapon_angle - 15.0).abs() < 1e-6);
        assert!((player.overlay2_angle - 90.0).abs() < 1e-6);
        w.players[0] = Some(player);
        let hud = w.player_hud(0).unwrap();
        assert!((hud.overlay_angle - 90.0).abs() < 1e-3);
        assert!((hud.weapon_angle - 15.0).abs() < 1e-3);
    }

    #[test]
    fn overlay_mirror_flips_layer_two_and_leaves_the_gun() {
        let src = format!(
            "{}\nACTOR Mir2Pistol : Pistol\n{{\n  States\n  {{\n  Fire:\n    PISG A 1\n    PISG A 1 A_OverlayFlags(2, 512, true)\n    PISG A 1 A_OverlayFlags(1, 512, true)\n    PISG A 1 A_OverlayFlags(2, 512, false)\n    Goto Ready\n  }}\n}}\n",
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
        let pistol = classes.find_name("Mir2Pistol").unwrap();
        let mut player = w.players[0].take().unwrap();
        player.ready_weapon = Some(pistol);
        player.pending_weapon = None;
        player.pspr_y = crate::player::WEAPON_TOP;
        player.pspr_state = Some(w.states.find_label(pistol, "Fire").unwrap());
        player.pspr_tics = 1;
        player_think(&mut w, &mut player);
        assert!(player.overlay2_mirror);
        assert!(!player.weapon_mirror);
        player_think(&mut w, &mut player);
        assert!(player.weapon_mirror);
        assert!(player.overlay2_mirror);
        w.players[0] = Some(player);
        assert!(w.player_hud(0).unwrap().overlay_mirror);
        let mut player = w.players[0].take().unwrap();
        player_think(&mut w, &mut player);
        assert!(!player.overlay2_mirror);
        assert!(player.weapon_mirror);
    }

    #[test]
    fn overlay_vertex_offset_moves_gun_and_layer_two_corners() {
        // `A_OverlayVertexOffset(layer, index, x, y, flags)`: WOF_ADD 4, WOF_KEEPY 2.
        let src = format!(
            "{}\nACTOR VertPistol : Pistol\n{{\n  States\n  {{\n  Fire:\n    PISG A 1\n    PISG A 1 A_OverlayVertexOffset(1, 2, 5, -3)\n    PISG A 1 A_OverlayVertexOffset(1, 2, 1, 9, 6)\n    PISG A 1 A_OverlayVertexOffset(2, 3, 0, 4)\n    Goto Ready\n  }}\n}}\n",
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
        let pistol = classes.find_name("VertPistol").unwrap();
        let mut player = w.players[0].take().unwrap();
        player.ready_weapon = Some(pistol);
        player.pending_weapon = None;
        player.pspr_y = crate::player::WEAPON_TOP;
        player.pspr_state = Some(w.states.find_label(pistol, "Fire").unwrap());
        player.pspr_tics = 1;
        player_think(&mut w, &mut player);
        assert_eq!(player.weapon_vertex[2], (5.0, -3.0));
        player_think(&mut w, &mut player);
        // Added to X, Y kept.
        assert_eq!(player.weapon_vertex[2], (6.0, -3.0));
        player_think(&mut w, &mut player);
        assert_eq!(player.overlay2_vertex[3], (0.0, 4.0));
        assert_eq!(player.weapon_vertex[3], (0.0, 0.0));
        w.players[0] = Some(player);
        let hud = w.player_hud(0).unwrap();
        assert_eq!(hud.weapon_vertex[2], (6.0, -3.0));
        assert_eq!(hud.overlay_vertex[3], (0.0, 4.0));
    }

    #[test]
    fn overlay_flip_mirrors_layer_two_and_leaves_the_gun() {
        let src = format!(
            "{}\nACTOR Flip2Pistol : Pistol\n{{\n  States\n  {{\n  Fire:\n    PISG A 1\n    PISG A 1 A_OverlayFlags(2, 64, true)\n    PISG A 1 A_OverlayFlags(1, 64, true)\n    PISG A 1 A_OverlayFlags(2, 64, false)\n    Goto Ready\n  }}\n}}\n",
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
        let pistol = classes.find_name("Flip2Pistol").unwrap();
        let mut player = w.players[0].take().unwrap();
        player.ready_weapon = Some(pistol);
        player.pending_weapon = None;
        player.pspr_y = crate::player::WEAPON_TOP;
        player.pspr_state = Some(w.states.find_label(pistol, "Fire").unwrap());
        player.pspr_tics = 1;
        player_think(&mut w, &mut player);
        assert!(player.overlay2_flip);
        assert!(!player.weapon_flip);
        player_think(&mut w, &mut player);
        assert!(player.weapon_flip);
        assert!(player.overlay2_flip);
        w.players[0] = Some(player);
        assert!(w.player_hud(0).unwrap().overlay_flip);
        let mut player = w.players[0].take().unwrap();
        player_think(&mut w, &mut player);
        assert!(!player.overlay2_flip);
        assert!(player.weapon_flip);
    }

    #[test]
    fn overlay_scale_grows_layer_two_and_leaves_the_gun() {
        let src = format!(
            "{}\nACTOR Scale2Pistol : Pistol\n{{\n  States\n  {{\n  Fire:\n    PISG A 1\n    PISG A 1 A_OverlayScale(2, 2)\n    PISG A 1 A_OverlayScale(2, 2, 3)\n    PISG A 1 A_OverlayScale(1, 4)\n    Goto Ready\n  }}\n}}\n",
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
        let pistol = classes.find_name("Scale2Pistol").unwrap();
        let mut player = w.players[0].take().unwrap();
        player.ready_weapon = Some(pistol);
        player.pending_weapon = None;
        player.pspr_y = crate::player::WEAPON_TOP;
        player.pspr_state = Some(w.states.find_label(pistol, "Fire").unwrap());
        player.pspr_tics = 1;
        player_think(&mut w, &mut player);
        assert_eq!(player.overlay2_scale, Some((2.0, 2.0)));
        assert!(player.weapon_scale.is_none());
        player_think(&mut w, &mut player);
        assert_eq!(player.overlay2_scale, Some((2.0, 3.0)));
        player_think(&mut w, &mut player);
        assert_eq!(player.weapon_scale, Some((4.0, 4.0)));
        assert_eq!(player.overlay2_scale, Some((2.0, 3.0)));
        w.players[0] = Some(player);
        let hud = w.player_hud(0).unwrap();
        assert!((hud.overlay_scale_x - 2.0).abs() < 1e-3);
        assert!((hud.overlay_scale_y - 3.0).abs() < 1e-3);
        assert!((hud.weapon_scale_x - 4.0).abs() < 1e-3);
    }

    #[test]
    fn extra_overlay_layers_run_their_own_states_and_draw_by_id() {
        let src = format!(
            "{}\nACTOR LayerPistol : Pistol\n{{\n  States\n  {{\n  Fire:\n    PISG A 1\n    PISG A 1 A_Overlay(-3, \"Hands\")\n    PISG A 1 A_Overlay(5, \"Glow\")\n    PISG A 4\n    PISG A 1 A_ClearOverlays(4, 10)\n    Goto Ready\n  Hands:\n    PISF A 2 A_OverlayOffset(OverlayID(), 10, 5)\n    PISF B 3\n    Stop\n  Glow:\n    PISG B 1 A_OverlayFlags(0, PSPF_ALPHA|PSPF_FLIP, true)\n    PISG B 1 A_OverlayAlpha(OverlayID(), 0.5)\n    PISG C -1\n    Stop\n  }}\n}}\n",
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
        let pistol = classes.find_name("LayerPistol").unwrap();
        let mut player = w.players[0].take().unwrap();
        player.ready_weapon = Some(pistol);
        player.pending_weapon = None;
        player.pspr_y = crate::player::WEAPON_TOP;
        player.pspr_state = Some(w.states.find_label(pistol, "Fire").unwrap());
        player.pspr_tics = 1;
        // Tic 1: gun enters Fire, layer -3 starts with its offset.
        player_think(&mut w, &mut player);
        let hands = player.psp_layers.get(&-3).expect("hands layer");
        assert_eq!(hands.sprite, "PISF");
        assert_eq!((hands.x, hands.y), (10.0, 5.0));
        assert_eq!(hands.tics, 2);
        // Tic 2: layer 5 starts; its zero-tic flags apply, then the hands count down.
        player_think(&mut w, &mut player);
        let glow = player.psp_layers.get(&5).expect("glow layer");
        assert_ne!(glow.flags & crate::player::pspf::ALPHA, 0);
        assert_ne!(glow.flags & crate::player::pspf::FLIP, 0);
        assert_eq!(player.psp_layers[&-3].tics, 1);
        // Tic 3: hands step to frame B, glow sets its alpha.
        player_think(&mut w, &mut player);
        assert_eq!(player.psp_layers[&-3].frame, 1);
        assert_eq!(player.psp_layers[&5].alpha, 0.5);
        w.players[0] = Some(player.clone());
        let hud = w.player_hud(0).unwrap();
        let ids: Vec<i32> = hud.layers.iter().map(|l| l.id).collect();
        assert_eq!(ids, vec![-3, 5]);
        let glow_hud = &hud.layers[1];
        assert!(
            (glow_hud.alpha - 0.5).abs() < 1e-6,
            "PSPF_ALPHA uses the layer alpha"
        );
        assert!(glow_hud.flip);
        assert_eq!(hud.layers[0].patch, "PISFB0");
        player = w.players[0].take().unwrap();
        // Hands run out after their 3-tic frame B.
        for _ in 0..3 {
            player_think(&mut w, &mut player);
        }
        assert!(
            !player.psp_layers.contains_key(&-3),
            "Stop removes the layer"
        );
        assert!(player.psp_layers.contains_key(&5), "glow holds at -1");
        // `A_ClearOverlays(4, 10)` drops layer 5.
        player_think(&mut w, &mut player);
        assert!(!player.psp_layers.contains_key(&5));
        w.players[0] = Some(player);
    }

    #[test]
    fn extra_layers_need_pspf_alpha_and_go_with_their_weapon() {
        let src = format!(
            "{}\nACTOR AlphaLayerPistol : Pistol\n{{\n  States\n  {{\n  Fire:\n    PISG A 1\n    PISG A 1 A_Overlay(3, \"Glow\")\n    PISG A 1 A_OverlayAlpha(3, 0.25)\n    PISG A 5\n    Goto Ready\n  Glow:\n    PISG B -1\n    Stop\n  }}\n}}\n",
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
        let pistol = classes.find_name("AlphaLayerPistol").unwrap();
        let mut player = w.players[0].take().unwrap();
        player.ready_weapon = Some(pistol);
        player.pending_weapon = None;
        player.pspr_y = crate::player::WEAPON_TOP;
        player.pspr_state = Some(w.states.find_label(pistol, "Fire").unwrap());
        player.pspr_tics = 1;
        player_think(&mut w, &mut player);
        player_think(&mut w, &mut player);
        w.players[0] = Some(player);
        let hud = w.player_hud(0).unwrap();
        assert_eq!(hud.layers.len(), 1);
        assert!(
            (hud.layers[0].alpha - 1.0).abs() < 1e-6,
            "without PSPF_ALPHA the layer keeps the player's alpha"
        );
        let mut player = w.players[0].take().unwrap();
        // Putting the weapon away drops the layers it started.
        player.ready_weapon = classes.find_name("Fist");
        player_think(&mut w, &mut player);
        assert!(player.psp_layers.is_empty());
        w.players[0] = Some(player);
    }

    #[test]
    fn overlay_alpha_dims_layer_two_and_leaves_the_gun() {
        let src = format!(
            "{}\nACTOR Alpha2Pistol : Pistol\n{{\n  States\n  {{\n  Fire:\n    PISG A 1\n    PISG A 1 A_OverlayAlpha(2, 0.25)\n    PISG A 1 A_OverlayAlpha(1, 0.5)\n    Goto Ready\n  }}\n}}\n",
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
        let pistol = classes.find_name("Alpha2Pistol").unwrap();
        let mut player = w.players[0].take().unwrap();
        player.ready_weapon = Some(pistol);
        player.pending_weapon = None;
        player.pspr_y = crate::player::WEAPON_TOP;
        player.pspr_state = Some(w.states.find_label(pistol, "Fire").unwrap());
        player.pspr_tics = 1;
        player_think(&mut w, &mut player);
        assert_eq!(player.overlay2_alpha, Some(0.25));
        assert!(player.weapon_alpha.is_none());
        player_think(&mut w, &mut player);
        assert_eq!(player.overlay2_alpha, Some(0.25));
        assert_eq!(player.weapon_alpha, Some(0.5));
        w.players[0] = Some(player);
        let hud = w.player_hud(0).unwrap();
        assert!((hud.overlay_alpha - 0.25).abs() < 1e-3);
        assert!((hud.weapon_alpha - 0.5).abs() < 1e-3);
    }

    #[test]
    fn overlay_offset_moves_layer_two_and_leaves_the_gun() {
        let src = format!(
            "{}\nACTOR Ofs2Pistol : Pistol\n{{\n  States\n  {{\n  Fire:\n    PISG A 1\n    PISG A 1 A_OverlayOffset(2, 6, 40)\n    PISG A 1 A_OverlayOffset(1, 3, 36)\n    Goto Ready\n  }}\n}}\n",
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
        let pistol = classes.find_name("Ofs2Pistol").unwrap();
        let mut player = w.players[0].take().unwrap();
        player.ready_weapon = Some(pistol);
        player.pending_weapon = None;
        player.pspr_y = crate::player::WEAPON_TOP;
        player.pspr_state = Some(w.states.find_label(pistol, "Fire").unwrap());
        player.pspr_tics = 1;
        player_think(&mut w, &mut player);
        assert_eq!(player.overlay2_ofs_x, 6.0);
        assert_eq!(player.overlay2_ofs_y, 8.0);
        assert_eq!(player.weapon_ofs_x, 0.0);
        player_think(&mut w, &mut player);
        assert_eq!(player.overlay2_ofs_x, 6.0);
        assert_eq!(player.weapon_ofs_x, 3.0);
        assert_eq!(player.weapon_ofs_y, 4.0);
        w.players[0] = Some(player);
        let hud = w.player_hud(0).unwrap();
        assert!((hud.overlay_ofs_x - 6.0).abs() < 1e-3);
        assert!((hud.overlay_ofs_y - 8.0).abs() < 1e-3);
        assert!((hud.bob_x - 3.0).abs() < 1e-3);
    }

    #[test]
    fn overlay_offset_moves_the_weapon_layer_only() {
        let src = format!(
            "{}\nACTOR OverlayPistol : Pistol\n{{\n  States\n  {{\n  Fire:\n    PISG A 1\n    PISG A 1 A_OverlayOffset(1, 4, 40)\n    PISG A 1 A_OverlayOffset(2, 9, 50)\n    Goto Ready\n  }}\n}}\n",
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
        let pistol = classes.find_name("OverlayPistol").unwrap();
        let mut player = w.players[0].take().unwrap();
        player.ready_weapon = Some(pistol);
        player.pending_weapon = None;
        player.pspr_y = crate::player::WEAPON_TOP;
        let fire = w.states.find_label(pistol, "Fire").unwrap();
        player.pspr_state = Some(fire);
        player.pspr_tics = 1;
        player_think(&mut w, &mut player);
        assert_eq!(player.weapon_ofs_x, 4.0);
        assert_eq!(player.weapon_ofs_y, 8.0);
        player_think(&mut w, &mut player);
        assert_eq!(player.weapon_ofs_x, 4.0);
        assert_eq!(player.weapon_ofs_y, 8.0);
        w.players[0] = Some(player);
        let hud = w.player_hud(0).unwrap();
        assert!((hud.bob_x - 4.0).abs() < 1e-4);
        assert!((hud.bob_y - 8.0).abs() < 1e-4);
    }

    #[test]
    fn overlay_alpha_dims_the_weapon_layer_only() {
        let src = format!(
            "{}\nACTOR AlphaPistol : Pistol\n{{\n  States\n  {{\n  Fire:\n    PISG A 1\n    PISG A 1 A_OverlayAlpha(1, 0.25)\n    PISG A 1 A_OverlayAlpha(2, 0.9)\n    Goto Ready\n  }}\n}}\n",
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
        let pistol = classes.find_name("AlphaPistol").unwrap();
        let mut player = w.players[0].take().unwrap();
        player.ready_weapon = Some(pistol);
        player.pending_weapon = None;
        player.pspr_y = crate::player::WEAPON_TOP;
        let fire = w.states.find_label(pistol, "Fire").unwrap();
        player.pspr_state = Some(fire);
        player.pspr_tics = 1;
        player_think(&mut w, &mut player);
        assert_eq!(player.weapon_alpha, Some(0.25));
        player_think(&mut w, &mut player);
        assert_eq!(player.weapon_alpha, Some(0.25));
        w.players[0] = Some(player);
        let hud = w.player_hud(0).unwrap();
        assert!((hud.weapon_alpha - 0.25).abs() < 1e-4);
    }

    #[test]
    fn overlay_scale_sizes_the_weapon_layer_only() {
        let src = format!(
            "{}\nACTOR ScalePistol : Pistol\n{{\n  States\n  {{\n  Fire:\n    PISG A 1\n    PISG A 1 A_OverlayScale(1, 2)\n    PISG A 1 A_OverlayScale(1, 2, 3)\n    PISG A 1 A_OverlayScale(2, 9, 9)\n    Goto Ready\n  }}\n}}\n",
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
        let pistol = classes.find_name("ScalePistol").unwrap();
        let mut player = w.players[0].take().unwrap();
        player.ready_weapon = Some(pistol);
        player.pending_weapon = None;
        player.pspr_y = crate::player::WEAPON_TOP;
        let fire = w.states.find_label(pistol, "Fire").unwrap();
        player.pspr_state = Some(fire);
        player.pspr_tics = 1;
        player_think(&mut w, &mut player);
        assert_eq!(player.weapon_scale, Some((2.0, 2.0)));
        player_think(&mut w, &mut player);
        assert_eq!(player.weapon_scale, Some((2.0, 3.0)));
        player_think(&mut w, &mut player);
        assert_eq!(player.weapon_scale, Some((2.0, 3.0)));
        w.players[0] = Some(player);
        let hud = w.player_hud(0).unwrap();
        assert!((hud.weapon_scale_x - 2.0).abs() < 1e-4);
        assert!((hud.weapon_scale_y - 3.0).abs() < 1e-4);
    }

    #[test]
    fn overlay_interpolate_flag_can_snap_the_gun() {
        let src = format!(
            "{}\nACTOR EasePistol : Pistol\n{{\n  States\n  {{\n  Fire:\n    PISG A 1\n    PISG A 1 A_OverlayFlags(1, 4096, false)\n    PISG A 1 A_OverlayFlags(2, 4096, true)\n    PISG A 1 A_OverlayFlags(1, 4096, true)\n    Goto Ready\n  }}\n}}\n",
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
        let pistol = classes.find_name("EasePistol").unwrap();
        let mut player = w.players[0].take().unwrap();
        assert!(player.weapon_interpolate);
        player.ready_weapon = Some(pistol);
        player.pending_weapon = None;
        player.pspr_y = crate::player::WEAPON_TOP;
        player.pspr_state = Some(w.states.find_label(pistol, "Fire").unwrap());
        player.pspr_tics = 1;
        player_think(&mut w, &mut player);
        assert!(!player.weapon_interpolate);
        w.players[0] = Some(player);
        assert!(!w.player_hud(0).unwrap().weapon_interpolate);
        let mut player = w.players[0].take().unwrap();
        player_think(&mut w, &mut player);
        assert!(!player.weapon_interpolate);
        player_think(&mut w, &mut player);
        assert!(player.weapon_interpolate);
    }

    #[test]
    fn overlay_force_style_ignores_player_fuzz() {
        let src = format!(
            "{}\nACTOR StylePistol : Pistol\n{{\n  States\n  {{\n  Fire:\n    PISG A 1\n    PISG A 1 A_OverlayFlags(1, 256, true)\n    PISG A 1 A_OverlayRenderStyle(1, 2)\n    PISG A 1 A_OverlayFlags(2, 256, false)\n    PISG A 1 A_OverlayRenderStyle(1, 1)\n    PISG A 1 A_OverlayFlags(1, 256, false)\n    Goto Ready\n  }}\n}}\n",
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
        let pistol = classes.find_name("StylePistol").unwrap();
        let mut player = w.players[0].take().unwrap();
        player.blur = true;
        player.ready_weapon = Some(pistol);
        player.pending_weapon = None;
        player.pspr_y = crate::player::WEAPON_TOP;
        player.pspr_state = Some(w.states.find_label(pistol, "Fire").unwrap());
        player.pspr_tics = 1;
        w.players[0] = Some(player);
        assert!(w.player_hud(0).unwrap().weapon_fuzz);
        let mut player = w.players[0].take().unwrap();
        player_think(&mut w, &mut player);
        assert!(player.weapon_force_style);
        w.players[0] = Some(player);
        assert!(!w.player_hud(0).unwrap().weapon_fuzz);
        let mut player = w.players[0].take().unwrap();
        player_think(&mut w, &mut player);
        w.players[0] = Some(player);
        assert!(w.player_hud(0).unwrap().weapon_fuzz);
        let mut player = w.players[0].take().unwrap();
        player_think(&mut w, &mut player);
        assert!(player.weapon_force_style);
        player_think(&mut w, &mut player);
        w.players[0] = Some(player);
        assert!(!w.player_hud(0).unwrap().weapon_fuzz);
        let mut player = w.players[0].take().unwrap();
        player_think(&mut w, &mut player);
        assert!(!player.weapon_force_style);
        w.players[0] = Some(player);
        assert!(w.player_hud(0).unwrap().weapon_fuzz);
    }

    #[test]
    fn overlay_force_alpha_ignores_player_translucency() {
        let src = format!(
            "{}\nACTOR AlphaPistol : Pistol\n{{\n  States\n  {{\n  Fire:\n    PISG A 1\n    PISG A 1 A_OverlayFlags(1, 128, true)\n    PISG A 1 A_OverlayFlags(2, 128, false)\n    PISG A 1 A_OverlayFlags(1, 128, false)\n    Goto Ready\n  }}\n}}\n",
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
        let pistol = classes.find_name("AlphaPistol").unwrap();
        let mut player = w.players[0].take().unwrap();
        let mo = player.mo.unwrap();
        w.actors.get_mut(mo).unwrap().alpha = 0.25;
        player.ready_weapon = Some(pistol);
        player.pending_weapon = None;
        player.pspr_y = crate::player::WEAPON_TOP;
        player.pspr_state = Some(w.states.find_label(pistol, "Fire").unwrap());
        player.pspr_tics = 1;
        w.players[0] = Some(player);
        let hud = w.player_hud(0).unwrap();
        assert!((hud.weapon_alpha - 0.25).abs() < 1e-3);
        let mut player = w.players[0].take().unwrap();
        player_think(&mut w, &mut player);
        assert!(player.weapon_force_alpha);
        player.weapon_alpha = Some(0.5);
        w.players[0] = Some(player);
        let hud = w.player_hud(0).unwrap();
        assert!((hud.weapon_alpha - 0.5).abs() < 1e-3);
        let mut player = w.players[0].take().unwrap();
        player_think(&mut w, &mut player);
        assert!(player.weapon_force_alpha);
        player_think(&mut w, &mut player);
        assert!(!player.weapon_force_alpha);
        w.players[0] = Some(player);
        let hud = w.player_hud(0).unwrap();
        assert!((hud.weapon_alpha - 0.125).abs() < 1e-3);
    }

    #[test]
    fn overlay_pivot_percent_off_keeps_pixel_pivot() {
        let src = format!(
            "{}\nACTOR PivotPistol : Pistol\n{{\n  States\n  {{\n  Fire:\n    PISG A 1\n    PISG A 1 A_OverlayFlags(1, 2048, false)\n    PISG A 1 A_OverlayPivot(1, 4, 4)\n    PISG A 1 A_OverlayFlags(2, 2048, true)\n    PISG A 1 A_OverlayFlags(1, 2048, true)\n    Goto Ready\n  }}\n}}\n",
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
        let pistol = classes.find_name("PivotPistol").unwrap();
        let mut player = w.players[0].take().unwrap();
        player.ready_weapon = Some(pistol);
        player.pending_weapon = None;
        player.pspr_y = crate::player::WEAPON_TOP;
        player.pspr_state = Some(w.states.find_label(pistol, "Fire").unwrap());
        player.pspr_tics = 1;
        player_think(&mut w, &mut player);
        assert!(!player.weapon_pivot_percent);
        player_think(&mut w, &mut player);
        assert_eq!(player.weapon_pivot, Some((4.0, 4.0)));
        w.players[0] = Some(player);
        let hud = w.player_hud(0).unwrap();
        assert!(!hud.weapon_pivot_percent);
        assert!((hud.weapon_pivot_x - 4.0).abs() < 1e-3);
        let mut player = w.players[0].take().unwrap();
        player_think(&mut w, &mut player);
        assert!(!player.weapon_pivot_percent);
        player_think(&mut w, &mut player);
        assert!(player.weapon_pivot_percent);
    }

    #[test]
    fn overlay_mirror_flag_flips_the_weapon_layer_vertically() {
        let src = format!(
            "{}\nACTOR MirrorPistol : Pistol\n{{\n  States\n  {{\n  Fire:\n    PISG A 1\n    PISG A 1 A_OverlayFlags(1, 512, true)\n    PISG A 1 A_OverlayFlags(2, 512, false)\n    PISG A 1 A_OverlayFlags(1, 512, false)\n    Goto Ready\n  }}\n}}\n",
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
        let pistol = classes.find_name("MirrorPistol").unwrap();
        let mut player = w.players[0].take().unwrap();
        player.ready_weapon = Some(pistol);
        player.pending_weapon = None;
        player.pspr_y = crate::player::WEAPON_TOP;
        player.pspr_state = Some(w.states.find_label(pistol, "Fire").unwrap());
        player.pspr_tics = 1;
        player_think(&mut w, &mut player);
        assert!(player.weapon_mirror);
        w.players[0] = Some(player);
        assert!(w.player_hud(0).unwrap().weapon_mirror);
        let mut player = w.players[0].take().unwrap();
        player_think(&mut w, &mut player);
        assert!(player.weapon_mirror);
        player_think(&mut w, &mut player);
        assert!(!player.weapon_mirror);
    }

    #[test]
    fn overlay_powdouble_halves_layer_two_and_leaves_the_gun() {
        let src = format!(
            "{}\nACTOR PowerDoubleFiringSpeed {{ Inventory.MaxAmount 1 }}\nACTOR Pow2Pistol : Pistol\n{{\n  States\n  {{\n  Fire:\n    PISG A 1\n    PISG A 0 A_Overlay(2, \"Hold\")\n    PISG A 4\n    Stop\n  Hold:\n    PISG B 4\n    Stop\n  Off:\n    PISG A 1\n    PISG A 0 A_OverlayFlags(2, 4, false)\n    PISG A 0 A_Overlay(2, \"Hold\")\n    PISG A 4\n    Stop\n  }}\n}}\n",
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
        let pistol = classes.find_name("Pow2Pistol").unwrap();
        let power = classes.find_name("PowerDoubleFiringSpeed").unwrap();
        let mut player = w.players[0].take().unwrap();
        let mo = player.mo.unwrap();
        assert!(player.overlay2_pow_double);
        player.ready_weapon = Some(pistol);
        player.pending_weapon = None;
        player.pspr_y = crate::player::WEAPON_TOP;
        player.pspr_state = Some(w.states.find_label(pistol, "Fire").unwrap());
        player.pspr_tics = 1;
        player_think(&mut w, &mut player);
        assert_eq!(player.overlay2_tics, 3);
        assert_eq!(player.pspr_tics, 4);
        assert!(!player.weapon_pow_double);
        w.fast_weapons = 1;
        let _ = give_inventory(&mut w, mo, power, 1);
        player.pspr_state = Some(w.states.find_label(pistol, "Fire").unwrap());
        player.pspr_tics = 1;
        player_think(&mut w, &mut player);
        assert_eq!(player.overlay2_tics, 1);
        assert_eq!(player.pspr_tics, 4);
        assert!(!player.weapon_pow_double);
        w.fast_weapons = 0;
        player.pspr_state = Some(w.states.find_label(pistol, "Off").unwrap());
        player.pspr_tics = 1;
        player_think(&mut w, &mut player);
        assert!(!player.overlay2_pow_double);
        assert_eq!(player.overlay2_tics, 3);
        assert_eq!(player.pspr_tics, 4);
    }

    #[test]
    fn overlay_powdouble_halves_a_weapon_frame() {
        let src = format!(
            "{}\nACTOR PowerDoubleFiringSpeed {{ Inventory.MaxAmount 1 }}\nACTOR PowPistol : Pistol\n{{\n  States\n  {{\n  Fire:\n    PISG A 1\n    PISG A 0 A_OverlayFlags(1, 4, true)\n    PISG A 4\n    Stop\n  }}\n}}\nACTOR PowLayer : Pistol\n{{\n  States\n  {{\n  Fire:\n    PISG A 1\n    PISG A 0 A_OverlayFlags(2, 4, true)\n    PISG A 4\n    Stop\n  }}\n}}\n",
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
        let pistol = classes.find_name("PowPistol").unwrap();
        let power = classes.find_name("PowerDoubleFiringSpeed").unwrap();
        let mut player = w.players[0].take().unwrap();
        let mo = player.mo.unwrap();
        player.ready_weapon = Some(pistol);
        player.pending_weapon = None;
        player.pspr_y = crate::player::WEAPON_TOP;
        player.pspr_state = Some(w.states.find_label(pistol, "Fire").unwrap());
        player.pspr_tics = 1;
        player_think(&mut w, &mut player);
        assert!(player.weapon_pow_double);
        assert_eq!(player.pspr_tics, 4);
        let _ = give_inventory(&mut w, mo, power, 1);
        player.pspr_state = Some(w.states.find_label(pistol, "Fire").unwrap());
        player.pspr_tics = 1;
        player_think(&mut w, &mut player);
        assert_eq!(player.pspr_tics, 2);
        let layer = classes.find_name("PowLayer").unwrap();
        player.weapon_pow_double = false;
        player.ready_weapon = Some(layer);
        player.pspr_state = Some(w.states.find_label(layer, "Fire").unwrap());
        player.pspr_tics = 1;
        player_think(&mut w, &mut player);
        assert!(!player.weapon_pow_double);
        assert_eq!(player.pspr_tics, 4);
    }

    #[test]
    fn overlay_cvarfast_halves_layer_two_and_leaves_the_gun() {
        let src = format!(
            "{}\nACTOR Fast2Pistol : Pistol\n{{\n  States\n  {{\n  Fire:\n    PISG A 1\n    PISG A 0 A_Overlay(2, \"Hold\")\n    PISG A 4\n    Stop\n  Hold:\n    PISG B 4\n    Stop\n  Slow:\n    PISG A 1\n    PISG A 0 A_OverlayFlags(2, 8, false)\n    PISG A 0 A_Overlay(2, \"Hold\")\n    PISG A 4\n    Stop\n  }}\n}}\n",
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
        w.fast_weapons = 1;
        let pistol = classes.find_name("Fast2Pistol").unwrap();
        let mut player = w.players[0].take().unwrap();
        assert!(player.overlay2_cvar_fast);
        player.ready_weapon = Some(pistol);
        player.pending_weapon = None;
        player.pspr_y = crate::player::WEAPON_TOP;
        player.pspr_state = Some(w.states.find_label(pistol, "Fire").unwrap());
        player.pspr_tics = 1;
        player_think(&mut w, &mut player);
        assert_eq!(player.overlay2_tics, 1);
        assert_eq!(player.pspr_tics, 4);
        assert!(!player.weapon_cvar_fast);
        w.fast_weapons = 2;
        player.pspr_state = Some(w.states.find_label(pistol, "Fire").unwrap());
        player.pspr_tics = 1;
        player_think(&mut w, &mut player);
        assert!(player.overlay2_state.is_none());
        assert_eq!(player.pspr_tics, 4);
        w.fast_weapons = 1;
        player.pspr_state = Some(w.states.find_label(pistol, "Slow").unwrap());
        player.pspr_tics = 1;
        player_think(&mut w, &mut player);
        assert!(!player.overlay2_cvar_fast);
        assert_eq!(player.overlay2_tics, 3);
        assert_eq!(player.pspr_tics, 4);
        assert!(!player.weapon_cvar_fast);
    }

    #[test]
    fn overlay_cvarfast_halves_a_weapon_frame() {
        let src = format!(
            "{}\nACTOR FastPistol : Pistol\n{{\n  States\n  {{\n  Fire:\n    PISG A 1\n    PISG A 0 A_OverlayFlags(1, 8, true)\n    PISG A 4\n    Stop\n  }}\n}}\nACTOR LayerPistol : Pistol\n{{\n  States\n  {{\n  Fire:\n    PISG A 1\n    PISG A 0 A_OverlayFlags(2, 8, true)\n    PISG A 4\n    Stop\n  }}\n}}\n",
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
        let mut w =
            World::from_map_with_classes(map, Arc::clone(&classes), Arc::clone(&states)).unwrap();
        w.fast_weapons = 1;
        let pistol = classes.find_name("FastPistol").unwrap();
        let mut player = w.players[0].take().unwrap();
        player.ready_weapon = Some(pistol);
        player.pending_weapon = None;
        player.pspr_y = crate::player::WEAPON_TOP;
        player.pspr_state = Some(w.states.find_label(pistol, "Fire").unwrap());
        player.pspr_tics = 1;
        player_think(&mut w, &mut player);
        assert!(player.weapon_cvar_fast);
        assert_eq!(player.pspr_tics, 2);
        w.fast_weapons = 2;
        player.pspr_state = Some(w.states.find_label(pistol, "Fire").unwrap());
        player.pspr_tics = 1;
        player_think(&mut w, &mut player);
        assert_eq!(player.pspr_tics, 1);
        let layer = classes.find_name("LayerPistol").unwrap();
        w.fast_weapons = 1;
        player.weapon_cvar_fast = false;
        player.ready_weapon = Some(layer);
        player.pspr_state = Some(w.states.find_label(layer, "Fire").unwrap());
        player.pspr_tics = 1;
        player_think(&mut w, &mut player);
        assert!(!player.weapon_cvar_fast);
        assert_eq!(player.pspr_tics, 4);
    }

    #[test]
    fn overlay_addweapon_flag_drops_the_gun_offset() {
        let src = format!(
            "{}\nACTOR OfsPistol : Pistol\n{{\n  States\n  {{\n  Fire:\n    PISG A 1\n    PISG A 1 A_OverlayFlags(1, 1, false)\n    PISG A 1 A_OverlayFlags(2, 1, true)\n    PISG A 1 A_OverlayFlags(1, 1, true)\n    Goto Ready\n  }}\n}}\n",
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
        let pistol = classes.find_name("OfsPistol").unwrap();
        let mut player = w.players[0].take().unwrap();
        player.ready_weapon = Some(pistol);
        player.pending_weapon = None;
        player.pspr_y = crate::player::WEAPON_TOP;
        player.pspr_bob_x = 10.0;
        player.weapon_ofs_x = 3.0;
        player.pspr_state = Some(w.states.find_label(pistol, "Fire").unwrap());
        player.pspr_tics = 1;
        w.players[0] = Some(player);
        let hud = w.player_hud(0).unwrap();
        assert!((hud.bob_x - 13.0).abs() < 1e-3);
        let mut player = w.players[0].take().unwrap();
        player_think(&mut w, &mut player);
        assert!(!player.weapon_add_ofs);
        w.players[0] = Some(player);
        let hud = w.player_hud(0).unwrap();
        assert!((hud.bob_x - 10.0).abs() < 1e-3);
        let mut player = w.players[0].take().unwrap();
        player_think(&mut w, &mut player);
        assert!(!player.weapon_add_ofs);
        player_think(&mut w, &mut player);
        assert!(player.weapon_add_ofs);
        w.players[0] = Some(player);
        let hud = w.player_hud(0).unwrap();
        assert!((hud.bob_x - 13.0).abs() < 1e-3);
    }

    #[test]
    fn overlay_addbob_flag_drops_weapon_sway() {
        let src = format!(
            "{}\nACTOR BobPistol : Pistol\n{{\n  States\n  {{\n  Fire:\n    PISG A 1\n    PISG A 1 A_OverlayFlags(1, 2, false)\n    PISG A 1 A_OverlayFlags(2, 2, true)\n    PISG A 1 A_OverlayFlags(1, 2, true)\n    Goto Ready\n  }}\n}}\n",
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
        let pistol = classes.find_name("BobPistol").unwrap();
        let mut player = w.players[0].take().unwrap();
        player.ready_weapon = Some(pistol);
        player.pending_weapon = None;
        player.pspr_y = crate::player::WEAPON_TOP;
        player.pspr_bob_x = 10.0;
        player.weapon_ofs_x = 3.0;
        player.pspr_state = Some(w.states.find_label(pistol, "Fire").unwrap());
        player.pspr_tics = 1;
        w.players[0] = Some(player);
        let hud = w.player_hud(0).unwrap();
        assert!((hud.bob_x - 13.0).abs() < 1e-3);
        let mut player = w.players[0].take().unwrap();
        player_think(&mut w, &mut player);
        assert!(!player.weapon_add_bob);
        w.players[0] = Some(player);
        let hud = w.player_hud(0).unwrap();
        assert!((hud.bob_x - 3.0).abs() < 1e-3);
        let mut player = w.players[0].take().unwrap();
        player_think(&mut w, &mut player);
        assert!(!player.weapon_add_bob);
        player_think(&mut w, &mut player);
        assert!(player.weapon_add_bob);
        w.players[0] = Some(player);
        let hud = w.player_hud(0).unwrap();
        assert!((hud.bob_x - 13.0).abs() < 1e-3);
    }

    #[test]
    fn overlay_layer_two_shows_its_sprite() {
        let src = format!(
            "{}\nACTOR LayerPistol : Pistol\n{{\n  States\n  {{\n  Fire:\n    PISG A 1\n    PISG A 1 A_Overlay(1, \"Flash\")\n    PISG A 1 A_Overlay(2, \"Flash\")\n    PISG B 5\n    Stop\n  Flash:\n    PISF A 2\n    Stop\n  }}\n}}\n",
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
        let pistol = classes.find_name("LayerPistol").unwrap();
        let mut player = w.players[0].take().unwrap();
        player.ready_weapon = Some(pistol);
        player.pending_weapon = None;
        player.pspr_y = crate::player::WEAPON_TOP;
        player.pspr_state = Some(w.states.find_label(pistol, "Fire").unwrap());
        player.pspr_tics = 1;
        player_think(&mut w, &mut player);
        assert!(player.overlay2_sprite.is_empty());
        player_think(&mut w, &mut player);
        assert_eq!(player.overlay2_sprite, "PISF");
        assert_eq!(player.overlay2_tics, 1);
        w.players[0] = Some(player);
        assert_eq!(w.player_hud(0).unwrap().overlay_patch, "PISFA0");
        let mut player = w.players[0].take().unwrap();
        player_think(&mut w, &mut player);
        assert!(player.overlay2_state.is_none());
        assert!(player.overlay2_sprite.is_empty());
    }

    #[test]
    fn overlay_flip_bit4_mirrors_the_weapon_layer_only() {
        let src = format!(
            "{}\nACTOR Flip16Pistol : Pistol\n{{\n  States\n  {{\n  Fire:\n    PISG A 1\n    PISG A 1 A_OverlayFlags(1, 64, true)\n    PISG A 1 A_OverlayFlags(2, 64, false)\n    PISG A 1 A_OverlayFlags(1, 64, false)\n    Goto Ready\n  }}\n}}\n",
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
        let pistol = classes.find_name("Flip16Pistol").unwrap();
        let mut player = w.players[0].take().unwrap();
        player.ready_weapon = Some(pistol);
        player.pending_weapon = None;
        player.pspr_y = crate::player::WEAPON_TOP;
        player.pspr_state = Some(w.states.find_label(pistol, "Fire").unwrap());
        player.pspr_tics = 1;
        player_think(&mut w, &mut player);
        assert!(player.weapon_flip);
        w.players[0] = Some(player);
        assert!(w.player_hud(0).unwrap().weapon_flip);
        let mut player = w.players[0].take().unwrap();
        player_think(&mut w, &mut player);
        assert!(player.weapon_flip);
        player_think(&mut w, &mut player);
        assert!(!player.weapon_flip);
    }

    #[test]
    fn overlay_flip_flag_mirrors_the_weapon_layer_only() {
        let src = format!(
            "{}\nACTOR FlipPistol : Pistol\n{{\n  States\n  {{\n  Fire:\n    PISG A 1\n    PISG A 1 A_OverlayFlags(1, 64, true)\n    PISG A 1 A_OverlayFlags(2, 64, false)\n    PISG A 1 A_OverlayFlags(1, 64, false)\n    Goto Ready\n  }}\n}}\n",
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
        let pistol = classes.find_name("FlipPistol").unwrap();
        let mut player = w.players[0].take().unwrap();
        player.ready_weapon = Some(pistol);
        player.pending_weapon = None;
        player.pspr_y = crate::player::WEAPON_TOP;
        let fire = w.states.find_label(pistol, "Fire").unwrap();
        player.pspr_state = Some(fire);
        player.pspr_tics = 1;
        player_think(&mut w, &mut player);
        assert!(player.weapon_flip);
        player_think(&mut w, &mut player);
        assert!(player.weapon_flip);
        player_think(&mut w, &mut player);
        assert!(!player.weapon_flip);
        w.players[0] = Some(player);
        let hud = w.player_hud(0).unwrap();
        assert!(!hud.weapon_flip);
    }

    #[test]
    fn overlay_pivot_is_where_the_weapon_layer_scales() {
        let src = format!(
            "{}\nACTOR PivotPistol : Pistol\n{{\n  States\n  {{\n  Fire:\n    PISG A 1\n    PISG A 1 A_OverlayPivot(1, 0, 0)\n    PISG A 1 A_OverlayPivot(2, 1, 1)\n    Goto Ready\n  }}\n}}\n",
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
        let pistol = classes.find_name("PivotPistol").unwrap();
        let mut player = w.players[0].take().unwrap();
        player.ready_weapon = Some(pistol);
        player.pending_weapon = None;
        player.pspr_y = crate::player::WEAPON_TOP;
        let fire = w.states.find_label(pistol, "Fire").unwrap();
        player.pspr_state = Some(fire);
        player.pspr_tics = 1;
        player_think(&mut w, &mut player);
        assert_eq!(player.weapon_pivot, Some((0.0, 0.0)));
        player_think(&mut w, &mut player);
        assert_eq!(player.weapon_pivot, Some((0.0, 0.0)));
        w.players[0] = Some(player);
        let hud = w.player_hud(0).unwrap();
        assert!(hud.weapon_pivot_x.abs() < 1e-4);
        assert!(hud.weapon_pivot_y.abs() < 1e-4);
    }

    #[test]
    fn overlay_pivot_align_picks_the_right_and_top() {
        let src = format!(
            "{}\nACTOR AlignPistol : Pistol\n{{\n  States\n  {{\n  Fire:\n    PISG A 1\n    PISG A 1 A_OverlayPivotAlign(1, 2, 0)\n    PISG A 1 A_OverlayPivotAlign(2, 0, 2)\n    Goto Ready\n  }}\n}}\n",
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
        let pistol = classes.find_name("AlignPistol").unwrap();
        let mut player = w.players[0].take().unwrap();
        player.ready_weapon = Some(pistol);
        player.pending_weapon = None;
        player.pspr_y = crate::player::WEAPON_TOP;
        let fire = w.states.find_label(pistol, "Fire").unwrap();
        player.pspr_state = Some(fire);
        player.pspr_tics = 1;
        player_think(&mut w, &mut player);
        assert_eq!(player.weapon_pivot, Some((1.0, 0.0)));
        player_think(&mut w, &mut player);
        assert_eq!(player.weapon_pivot, Some((1.0, 0.0)));
        w.players[0] = Some(player);
        let hud = w.player_hud(0).unwrap();
        assert!((hud.weapon_pivot_x - 1.0).abs() < 1e-4);
        assert!(hud.weapon_pivot_y.abs() < 1e-4);
    }

    #[test]
    fn overlay_rotate_turns_the_weapon_layer() {
        let src = format!(
            "{}\nACTOR RotatePistol : Pistol\n{{\n  States\n  {{\n  Fire:\n    PISG A 1\n    PISG A 1 A_OverlayRotate(1, 90)\n    PISG A 1 A_OverlayRotate(2, 180)\n    Goto Ready\n  }}\n}}\n",
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
        let pistol = classes.find_name("RotatePistol").unwrap();
        let mut player = w.players[0].take().unwrap();
        player.ready_weapon = Some(pistol);
        player.pending_weapon = None;
        player.pspr_y = crate::player::WEAPON_TOP;
        let fire = w.states.find_label(pistol, "Fire").unwrap();
        player.pspr_state = Some(fire);
        player.pspr_tics = 1;
        player_think(&mut w, &mut player);
        assert!((player.weapon_angle - 90.0).abs() < 1e-6);
        player_think(&mut w, &mut player);
        assert!((player.weapon_angle - 90.0).abs() < 1e-6);
        w.players[0] = Some(player);
        let hud = w.player_hud(0).unwrap();
        assert!((hud.weapon_angle - 90.0).abs() < 1e-4);
    }

    #[test]
    fn overlay_render_style_fuzzes_the_weapon_layer() {
        let src = format!(
            "{}\nACTOR StylePistol : Pistol\n{{\n  States\n  {{\n  Fire:\n    PISG A 1\n    PISG A 1 A_OverlayRenderStyle(1, 2)\n    PISG A 1 A_OverlayRenderStyle(2, 1)\n    PISG A 1 A_OverlayRenderStyle(1, 1)\n    Goto Ready\n  }}\n}}\n",
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
        let pistol = classes.find_name("StylePistol").unwrap();
        let mut player = w.players[0].take().unwrap();
        player.ready_weapon = Some(pistol);
        player.pending_weapon = None;
        player.pspr_y = crate::player::WEAPON_TOP;
        let fire = w.states.find_label(pistol, "Fire").unwrap();
        player.pspr_state = Some(fire);
        player.pspr_tics = 1;
        player_think(&mut w, &mut player);
        assert_eq!(player.weapon_style, Some(RenderStyle::Fuzzy));
        w.players[0] = Some(player);
        let hud = w.player_hud(0).unwrap();
        assert!(hud.weapon_fuzz);
        let mut player = w.players[0].take().unwrap();
        player_think(&mut w, &mut player);
        assert_eq!(player.weapon_style, Some(RenderStyle::Fuzzy));
        player_think(&mut w, &mut player);
        assert_eq!(player.weapon_style, Some(RenderStyle::Normal));
        w.players[0] = Some(player);
        let hud = w.player_hud(0).unwrap();
        assert!(!hud.weapon_fuzz);
    }

    #[test]
    fn set_view_pitch_moves_the_camera_and_leaves_aim() {
        let src = format!(
            "{}\nACTOR ViewPistol : Pistol\n{{\n  States\n  {{\n  Fire:\n    PISG A 1\n    PISG A 1 A_SetViewPitch(20)\n    PISG A 1 A_SetPitch(5)\n    PISG A 1 A_SetViewPitch(100, 1)\n    Goto Ready\n  }}\n}}\n",
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
        let pistol = classes.find_name("ViewPistol").unwrap();
        let mut player = w.players[0].take().unwrap();
        player.ready_weapon = Some(pistol);
        player.pending_weapon = None;
        player.pspr_y = crate::player::WEAPON_TOP;
        let fire = w.states.find_label(pistol, "Fire").unwrap();
        player.pspr_state = Some(fire);
        player.pspr_tics = 1;
        let mo = player.mo.unwrap();
        player_think(&mut w, &mut player);
        {
            let a = w.actors.get(mo).unwrap();
            assert!(a.pitch.degrees().abs() < 1e-6);
            assert!((a.camera_pitch().degrees() + 20.0).abs() < 1e-6);
        }
        player_think(&mut w, &mut player);
        {
            let a = w.actors.get(mo).unwrap();
            assert!((a.pitch.degrees() + 5.0).abs() < 1e-6);
            assert!((a.camera_pitch().degrees() + 20.0).abs() < 1e-6);
        }
        player_think(&mut w, &mut player);
        let a = w.actors.get(mo).unwrap();
        assert!((a.pitch.degrees() + 5.0).abs() < 1e-6);
        assert!((a.camera_pitch().degrees() + 89.0).abs() < 1e-6);
    }

    #[test]
    fn set_view_angle_turns_the_camera_and_leaves_aim() {
        let src = format!(
            "{}\nACTOR YawPistol : Pistol\n{{\n  States\n  {{\n  Fire:\n    PISG A 1\n    PISG A 1 A_SetViewAngle(30)\n    PISG A 1 A_SetAngle(90)\n    PISG A 1 A_SetViewAngle(200, 0, 1)\n    Goto Ready\n  }}\n}}\n",
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
        let pistol = classes.find_name("YawPistol").unwrap();
        let mut player = w.players[0].take().unwrap();
        player.ready_weapon = Some(pistol);
        player.pending_weapon = None;
        player.pspr_y = crate::player::WEAPON_TOP;
        let fire = w.states.find_label(pistol, "Fire").unwrap();
        player.pspr_state = Some(fire);
        player.pspr_tics = 1;
        let mo = player.mo.unwrap();
        player_think(&mut w, &mut player);
        {
            let a = w.actors.get(mo).unwrap();
            assert!(a.angle.degrees().abs() < 1e-6);
            assert!((a.camera_yaw().degrees() - 30.0).abs() < 1e-6);
        }
        player_think(&mut w, &mut player);
        {
            let a = w.actors.get(mo).unwrap();
            assert!((a.angle.degrees() - 90.0).abs() < 1e-6);
            assert!((a.camera_yaw().degrees() - 30.0).abs() < 1e-6);
        }
        player_think(&mut w, &mut player);
        let a = w.actors.get(mo).unwrap();
        assert!((a.angle.degrees() - 90.0).abs() < 1e-6);
        assert!((a.camera_yaw().degrees() - 30.0).abs() < 1e-6);
    }

    #[test]
    fn set_view_roll_banks_the_camera_and_leaves_the_sprite() {
        let src = format!(
            "{}\nACTOR RollPistol : Pistol\n{{\n  States\n  {{\n  Fire:\n    PISG A 1\n    PISG A 1 A_SetViewRoll(15)\n    PISG A 1 A_SetRoll(40)\n    PISG A 1 A_SetViewRoll(200, 0, 1)\n    Goto Ready\n  }}\n}}\n",
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
        let pistol = classes.find_name("RollPistol").unwrap();
        let mut player = w.players[0].take().unwrap();
        player.ready_weapon = Some(pistol);
        player.pending_weapon = None;
        player.pspr_y = crate::player::WEAPON_TOP;
        let fire = w.states.find_label(pistol, "Fire").unwrap();
        player.pspr_state = Some(fire);
        player.pspr_tics = 1;
        let mo = player.mo.unwrap();
        player_think(&mut w, &mut player);
        {
            let a = w.actors.get(mo).unwrap();
            assert!(a.roll.degrees().abs() < 1e-6);
            assert!((a.camera_roll().degrees() - 15.0).abs() < 1e-6);
        }
        player_think(&mut w, &mut player);
        {
            let a = w.actors.get(mo).unwrap();
            assert!((a.roll.degrees() - 40.0).abs() < 1e-6);
            assert!((a.camera_roll().degrees() - 15.0).abs() < 1e-6);
        }
        player_think(&mut w, &mut player);
        let a = w.actors.get(mo).unwrap();
        assert!((a.roll.degrees() - 40.0).abs() < 1e-6);
        assert!((a.camera_roll().degrees() - 15.0).abs() < 1e-6);
    }

    #[test]
    fn set_view_pos_moves_the_eye_and_leaves_the_actor() {
        let src = format!(
            "{}\nACTOR EyePistol : Pistol\n{{\n  States\n  {{\n  Fire:\n    PISG A 1\n    PISG A 1 SetViewPos(8, 0, 2)\n    PISG A 1 A_SetAngle(90)\n    PISG A 1 SetViewPos(0, 6, 0, 2)\n    PISG A 1 SetViewPos(10, 20, 30, 4)\n    Goto Ready\n  }}\n}}\n",
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
        let pistol = classes.find_name("EyePistol").unwrap();
        let mut player = w.players[0].take().unwrap();
        player.ready_weapon = Some(pistol);
        player.pending_weapon = None;
        player.pspr_y = crate::player::WEAPON_TOP;
        let fire = w.states.find_label(pistol, "Fire").unwrap();
        player.pspr_state = Some(fire);
        player.pspr_tics = 1;
        let mo = player.mo.unwrap();
        let origin = w.actors.get(mo).unwrap().pos;
        player_think(&mut w, &mut player);
        {
            let a = w.actors.get(mo).unwrap();
            let (x, y, z) = a.camera_eye(100.0);
            assert!((a.pos.x() - origin.x()).abs() < 1e-6);
            assert!((x - (origin.x() + 8.0)).abs() < 1e-6);
            assert!((y - origin.y()).abs() < 1e-6);
            assert!((z - 102.0).abs() < 1e-6);
        }
        player_think(&mut w, &mut player);
        {
            let a = w.actors.get(mo).unwrap();
            let (x, y, _) = a.camera_eye(100.0);
            assert!((a.angle.degrees() - 90.0).abs() < 1e-6);
            assert!((x - origin.x()).abs() < 1e-6);
            assert!((y - (origin.y() + 8.0)).abs() < 1e-6);
        }
        player_think(&mut w, &mut player);
        {
            let a = w.actors.get(mo).unwrap();
            let (x, y, z) = a.camera_eye(100.0);
            assert!((x - origin.x()).abs() < 1e-6);
            assert!((y - (origin.y() + 6.0)).abs() < 1e-6);
            assert!((z - 100.0).abs() < 1e-6);
        }
        player_think(&mut w, &mut player);
        let a = w.actors.get(mo).unwrap();
        assert!((a.pos.x() - origin.x()).abs() < 1e-6);
        assert_eq!(a.camera_eye(100.0), (10.0, 20.0, 30.0));
    }

    #[test]
    fn sprite_offset_shifts_the_picture_and_leaves_the_actor() {
        let src = format!(
            "{}\nACTOR ShiftPistol : Pistol\n{{\n  States\n  {{\n  Fire:\n    PISG A 1\n    PISG A 1 A_SpriteOffset(4, 6)\n    PISG A 1 A_SpriteOffset(1, 2)\n    Goto Ready\n  }}\n}}\n",
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
        let pistol = classes.find_name("ShiftPistol").unwrap();
        let mut player = w.players[0].take().unwrap();
        player.ready_weapon = Some(pistol);
        player.pending_weapon = None;
        player.pspr_y = crate::player::WEAPON_TOP;
        let fire = w.states.find_label(pistol, "Fire").unwrap();
        player.pspr_state = Some(fire);
        player.pspr_tics = 1;
        let mo = player.mo.unwrap();
        let origin = w.actors.get(mo).unwrap().pos;
        player_think(&mut w, &mut player);
        {
            let a = w.actors.get(mo).unwrap();
            assert!((a.pos.x() - origin.x()).abs() < 1e-6);
            assert!((a.sprite_ofs_x - 4.0).abs() < 1e-6);
            assert!((a.sprite_ofs_y - 6.0).abs() < 1e-6);
        }
        player_think(&mut w, &mut player);
        let a = w.actors.get(mo).unwrap();
        assert!((a.pos.x() - origin.x()).abs() < 1e-6);
        assert!((a.sprite_ofs_x - 1.0).abs() < 1e-6);
        assert!((a.sprite_ofs_y - 2.0).abs() < 1e-6);
    }

    #[test]
    fn zoom_factor_halves_a_ninety_degree_view() {
        let src = format!(
            "{}\nACTOR ZoomPistol : Pistol\n{{\n  States\n  {{\n  Fire:\n    PISG A 1\n    PISG A 1 A_ZoomFactor(2)\n    PISG A 1 A_ZoomFactor(1)\n    Goto Ready\n  }}\n}}\n",
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
        let pistol = classes.find_name("ZoomPistol").unwrap();
        let mut player = w.players[0].take().unwrap();
        player.ready_weapon = Some(pistol);
        player.pending_weapon = None;
        player.pspr_y = crate::player::WEAPON_TOP;
        let fire = w.states.find_label(pistol, "Fire").unwrap();
        player.pspr_state = Some(fire);
        player.pspr_tics = 1;
        player_think(&mut w, &mut player);
        assert!((player.weapon_zoom - 2.0).abs() < 1e-5);
        assert!((crate::player::Player::zoomed_fov(90.0, player.weapon_zoom) - 45.0).abs() < 1e-4);
        player_think(&mut w, &mut player);
        assert!((player.weapon_zoom - 1.0).abs() < 1e-5);
        assert!((crate::player::Player::zoomed_fov(90.0, player.weapon_zoom) - 90.0).abs() < 1e-4);
    }

    #[test]
    fn set_crosshair_two_is_what_the_hud_draws() {
        let src = format!(
            "{}\nACTOR HairPistol : Pistol\n{{\n  States\n  {{\n  Fire:\n    PISG A 1\n    PISG A 1 A_SetCrosshair(2)\n    PISG A 1 A_SetCrosshair(0)\n    Goto Ready\n  }}\n}}\n",
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
        let pistol = classes.find_name("HairPistol").unwrap();
        let mut player = w.players[0].take().unwrap();
        player.ready_weapon = Some(pistol);
        player.pending_weapon = None;
        player.pspr_y = crate::player::WEAPON_TOP;
        let fire = w.states.find_label(pistol, "Fire").unwrap();
        player.pspr_state = Some(fire);
        player.pspr_tics = 1;
        player_think(&mut w, &mut player);
        assert_eq!(player.crosshair, 2);
        w.players[0] = Some(player);
        let hud = w.player_hud(0).unwrap();
        assert_eq!(hud.crosshair, 2);
        let mut player = w.players[0].take().unwrap();
        player_think(&mut w, &mut player);
        assert_eq!(player.crosshair, 0);
    }

    #[test]
    fn overlay_translation_greens_the_weapon_layer() {
        let src = format!(
            "{}\nACTOR TintPistol : Pistol\n{{\n  States\n  {{\n  Fire:\n    PISG A 1\n    PISG A 1 A_OverlayTranslation(1, \"Green\")\n    PISG A 1 A_OverlayTranslation(2, \"Red\")\n    PISG A 1 A_OverlayTranslation(1, \"None\")\n    Goto Ready\n  }}\n}}\n",
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
        let pistol = classes.find_name("TintPistol").unwrap();
        let mut player = w.players[0].take().unwrap();
        player.ready_weapon = Some(pistol);
        player.pending_weapon = None;
        player.pspr_y = crate::player::WEAPON_TOP;
        let fire = w.states.find_label(pistol, "Fire").unwrap();
        player.pspr_state = Some(fire);
        player.pspr_tics = 1;
        player_think(&mut w, &mut player);
        assert_eq!(player.weapon_translation, "Green");
        w.players[0] = Some(player);
        let hud = w.player_hud(0).unwrap();
        assert!(hud.weapon_tint_g > hud.weapon_tint_r);
        let mut player = w.players[0].take().unwrap();
        player_think(&mut w, &mut player);
        assert_eq!(player.weapon_translation, "Green");
        player_think(&mut w, &mut player);
        assert!(player.weapon_translation.is_empty());
        w.players[0] = Some(player);
        let hud = w.player_hud(0).unwrap();
        assert!((hud.weapon_tint_r - 1.0).abs() < 1e-4);
        assert!((hud.weapon_tint_g - 1.0).abs() < 1e-4);
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

    /// A mod's ZScript `HandleMovement` / `CalcHeight` overrides move the
    /// player and set the eye instead of Room's `P_MovePlayer` (Project
    /// Brutality's movement), reached through UZDoom's `PlayerThink`.
    #[test]
    fn zscript_movement_overrides_replace_native_movement() {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let file = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&file).unwrap());
        let states = Arc::new(StateTable::from_decorate(&file, &classes));
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../wadsrc/uzdoom");
        let entry = std::fs::read_to_string(root.join("zscript.txt")).unwrap();
        let mut sources = vec![("zscript.txt".to_owned(), entry.clone())];
        for line in entry.lines() {
            if let Some(rest) = line.trim().strip_prefix("#include") {
                let path = rest.trim().trim_matches('"');
                sources.push((
                    path.to_owned(),
                    std::fs::read_to_string(root.join(path)).unwrap(),
                ));
            }
        }
        sources.push((
            "move.zs".into(),
            r#"
class MovePawn : DoomPlayer
{
    override void HandleMovement()
    {
        UserCmd cmd = player.cmd;
        angle += cmd.yaw * (360. / 65536.);
        if (cmd.forwardmove > 0 && (cmd.buttons & BT_RUN)) vel.xy = RotateVector((7, 0), angle);
    }
    override void CalcHeight()
    {
        player.viewz = pos.z + 10;
    }
}
"#
            .into(),
        ));
        let result = room_zscript::check(&sources);
        assert!(
            result.ok(),
            "{:?}",
            result
                .diagnostics
                .iter()
                .filter(|d| d.severity == room_zscript::Severity::Error)
                .take(3)
                .collect::<Vec<_>>()
        );
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        w.load_zscript_program(result.program.unwrap());
        // The test map's start spawns a bare `PlayerPawn`; play as the mod's
        // pawn, not frozen by the spawn.
        let pawn = w
            .classes
            .find_name("MovePawn")
            .expect("script class registered");
        w.actors.get_mut(mo).unwrap().class = pawn;
        w.actors.get_mut(mo).unwrap().reaction_time = 0;
        assert!(w.zscript_moves_player(mo));
        let mut player = w.players[0].take().unwrap();
        // A run-speed forward move and a quarter turn left.
        player.cmd = TicCmd {
            forward_move: 50 * 256,
            yaw: 16384,
            ..TicCmd::default()
        };
        player_think(&mut w, &mut player);
        let a = w.actors.get(mo).unwrap();
        assert!(
            (a.angle.degrees() - 90.0).abs() < 1e-6,
            "{}",
            a.angle.degrees()
        );
        assert!(
            a.vel.x().abs() < 1e-6 && (a.vel.y() - 7.0).abs() < 1e-6,
            "{:?}",
            a.vel
        );
        assert!(
            (player.view_z - (a.pos.z() + 10.0)).abs() < 1e-6,
            "{}",
            player.view_z
        );
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
            sprites
                .iter()
                .any(|(s, f, _)| s.eq_ignore_ascii_case("SHT2") && *f == 6),
            "missing G frame: {sprites:?}"
        );
        assert!(
            sprites
                .iter()
                .any(|(s, f, _)| s.eq_ignore_ascii_case("SHT2") && *f == 7),
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
        let mut w =
            World::from_map_with_classes(map, Arc::clone(&classes), Arc::clone(&states)).unwrap();
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
                assert!(player.pspr_flash_tics > 0, "CHGF B must still be running");
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
            classes
                .get(classes.find_name("PlasmaRifle").unwrap())
                .unwrap()
                .selection_order
                < classes
                    .get(classes.find_name("Fist").unwrap())
                    .unwrap()
                    .selection_order
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
        let (mo, _, ammo) = ready_with_weapon(&mut w, &classes, "RocketLauncher", "RocketAmmo", 5);
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
