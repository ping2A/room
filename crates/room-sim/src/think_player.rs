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
        if world.try_actor_jump(mo) {
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
        let mut view_z = a.pos.z() + player.view_height + player.view_ofs_z + bob_z;
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

/// `PowerDoubleFiringSpeed` in the player's inventory.
fn double_firing_speed(world: &World, mo: ActorId) -> bool {
    world
        .classes
        .find_name("PowerDoubleFiringSpeed")
        .is_some_and(|id| world.inventory.count(mo, id) > 0)
}

/// Jump the weapon psprite to a class label (`Ready`, `Fire`, …).
///
/// Doom `P_SetPsprite`: call the state's action on **enter**, then keep advancing
/// while `tics == 0` so `A_ReFire` / instant frames resolve in the same tic.
fn set_psprite(world: &mut World, player: &mut Player, mo: ActorId, mut state: StateId) {
    // Cap zero-tic chains so a broken Loop with tics=0 cannot spin forever.
    for _ in 0..64 {
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
                "A_WeaponOffset" => {
                    apply_weapon_offset(
                        player,
                        action_f64(&args, 0, 0.0),
                        action_f64(&args, 1, crate::player::WEAPON_TOP),
                        action_i32(&args, 2, 0),
                    );
                }
                "A_OverlayScale" => {
                    // A Y of 0 copies X. Layer 1 is the gun. Layer 2 is the overlay.
                    let sx = action_f64(&args, 1, 1.0);
                    let sy_arg = action_f64(&args, 2, 0.0);
                    let sy = if sy_arg == 0.0 { sx } else { sy_arg };
                    let flags = action_i32(&args, 3, 0);
                    match action_i32(&args, 0, 1) {
                        1 => {
                            player.weapon_scale =
                                Some(scaled_pair(player.weapon_scale, sx, sy, flags));
                        }
                        2 => {
                            player.overlay2_scale =
                                Some(scaled_pair(player.overlay2_scale, sx, sy, flags));
                        }
                        _ => {}
                    }
                }
                "A_Overlay" => {
                    // Layer 2 plays `start` on the ready weapon. Layer 1 stays the gun.
                    if action_i32(&args, 0, 0) == 2 {
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
                                    if !crate::state::sprite_unchanged(&st.sprite)
                                        && !st.sprite.is_empty()
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
                    let layer = action_i32(&args, 0, 1);
                    let flags = action_i32(&args, 1, 0);
                    let enable = args.get(2).is_none_or(|s| {
                        let t = s.trim().trim_matches('"');
                        !t.eq_ignore_ascii_case("false") && t != "0"
                    });
                    let flip_bit = (flags & (1 << 4)) != 0 || (flags & (1 << 6)) != 0;
                    if layer == 2 {
                        if flip_bit {
                            player.overlay2_flip = enable;
                        }
                        if (flags & (1 << 5)) != 0 {
                            player.overlay2_mirror = enable;
                        }
                        if (flags & (1 << 7)) != 0 {
                            player.overlay2_pivot_percent = enable;
                        }
                        if (flags & (1 << 1)) != 0 {
                            player.overlay2_add_bob = enable;
                        }
                        if (flags & 1) != 0 {
                            player.overlay2_add_weapon = enable;
                        }
                        if (flags & (1 << 8)) != 0 {
                            player.overlay2_interpolate = enable;
                        }
                        if (flags & (1 << 10)) != 0 {
                            player.overlay2_force_alpha = enable;
                        }
                        if (flags & (1 << 9)) != 0 {
                            player.overlay2_force_style = enable;
                        }
                        if (flags & (1 << 3)) != 0 {
                            player.overlay2_cvar_fast = enable;
                        }
                        if (flags & (1 << 2)) != 0 {
                            player.overlay2_pow_double = enable;
                        }
                    } else if layer == 1 {
                        // Bit 4 is GZDoom `PSPF_FLIP`. Bit 6 stays the earlier flip binding.
                        if flip_bit {
                            player.weapon_flip = enable;
                        }
                        // `PSPF_ADDBOB` is bit 1. Clearing it drops weapon sway.
                        if flags & (1 << 1) != 0 {
                            player.weapon_add_bob = enable;
                        }
                        // `PSPF_ADDWEAPON` is bit 0. Clearing it drops `A_WeaponOffset`.
                        if (flags & 1) != 0 {
                            player.weapon_add_ofs = enable;
                        }
                        // `PSPF_CVARFAST` is bit 3.
                        if (flags & (1 << 3)) != 0 {
                            player.weapon_cvar_fast = enable;
                        }
                        // `PSPF_POWDOUBLE` is bit 2.
                        if (flags & (1 << 2)) != 0 {
                            player.weapon_pow_double = enable;
                        }
                        // `PSPF_MIRROR` is bit 5.
                        if (flags & (1 << 5)) != 0 {
                            player.weapon_mirror = enable;
                        }
                        // `PSPF_PIVOTPERCENT` is bit 7.
                        if (flags & (1 << 7)) != 0 {
                            player.weapon_pivot_percent = enable;
                        }
                        // `PSPF_FORCEALPHA` is bit 10.
                        if (flags & (1 << 10)) != 0 {
                            player.weapon_force_alpha = enable;
                        }
                        // `PSPF_FORCESTYLE` is bit 9.
                        if (flags & (1 << 9)) != 0 {
                            player.weapon_force_style = enable;
                        }
                        // `PSPF_INTERPOLATE` is bit 8.
                        if (flags & (1 << 8)) != 0 {
                            player.weapon_interpolate = enable;
                        }
                    }
                }
                "A_OverlayPivot" => {
                    let pivot = (action_f64(&args, 1, 0.5), action_f64(&args, 2, 0.5));
                    match action_i32(&args, 0, 1) {
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
                    match action_i32(&args, 0, 1) {
                        1 => player.weapon_pivot = Some(pivot),
                        2 => player.overlay2_pivot = Some(pivot),
                        _ => {}
                    }
                }
                "A_OverlayRotate" => {
                    // Degrees, positive is counter-clockwise.
                    let angle = action_f64(&args, 1, 0.0);
                    match action_i32(&args, 0, 1) {
                        1 => player.weapon_angle = angle,
                        2 => player.overlay2_angle = angle,
                        _ => {}
                    }
                }
                "A_OverlayRenderStyle" => {
                    // `STYLE_Fuzzy` is 2, `STYLE_Normal` is 1.
                    let style = render_style_code(action_i32(&args, 1, 1));
                    match action_i32(&args, 0, 1) {
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
                    match action_i32(&args, 0, 1) {
                        1 => player.weapon_translation = name,
                        2 => player.overlay2_translation = name,
                        _ => {}
                    }
                }
                "A_OverlayAlpha" => {
                    let alpha = action_f64(&args, 1, 1.0).clamp(0.0, 1.0);
                    match action_i32(&args, 0, 1) {
                        1 => player.weapon_alpha = Some(alpha),
                        2 => player.overlay2_alpha = Some(alpha),
                        _ => {}
                    }
                }
                "A_OverlayOffset" => {
                    let layer = action_i32(&args, 0, 1);
                    let wx = action_f64(&args, 1, 0.0);
                    let wy = action_f64(&args, 2, crate::player::WEAPON_TOP);
                    let flags = action_i32(&args, 3, 0);
                    if layer == 1 {
                        apply_weapon_offset(player, wx, wy, flags);
                    } else if layer == 2 {
                        apply_overlay2_offset(player, wx, wy, flags);
                    }
                }
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
                "A_OpenShotgun2" => play_player_sound(world, mo, "weapons/sshoto"),
                "A_LoadShotgun2" => play_player_sound(world, mo, "weapons/sshotl"),
                "A_CloseShotgun2" => play_player_sound(world, mo, "weapons/sshotc"),
                "A_GunFlash" => start_flash_psprite(world, player, 0),
                "A_Light1" => player.extralight = 1,
                "A_Light2" => player.extralight = 2,
                "A_Light0" | "A_LightDone" => player.extralight = 0,
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
        .and_then(|s| s.trim().trim_matches('"').parse().ok())
        .unwrap_or(default)
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
            "{}\nACTOR Style2ForcePistol : Pistol\n{{\n  States\n  {{\n  Fire:\n    PISG A 1\n    PISG A 1 A_OverlayFlags(2, 512, true)\n    PISG A 1 A_OverlayRenderStyle(2, 2)\n    PISG A 1 A_OverlayRenderStyle(2, 1)\n    PISG A 1 A_OverlayFlags(2, 512, false)\n    PISG A 1 A_OverlayFlags(1, 512, true)\n    Goto Ready\n  }}\n}}\n",
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
            "{}\nACTOR Alpha2PistolForce : Pistol\n{{\n  States\n  {{\n  Fire:\n    PISG A 1\n    PISG A 1 A_OverlayFlags(2, 1024, true)\n    PISG A 1 A_OverlayFlags(1, 1024, true)\n    PISG A 1 A_OverlayFlags(2, 1024, false)\n    Goto Ready\n  }}\n}}\n",
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
            "{}\nACTOR Ease2Pistol : Pistol\n{{\n  States\n  {{\n  Fire:\n    PISG A 1\n    PISG A 1 A_OverlayFlags(2, 256, false)\n    PISG A 1 A_OverlayFlags(1, 256, false)\n    PISG A 1 A_OverlayFlags(2, 256, true)\n    Goto Ready\n  }}\n}}\n",
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
            "{}\nACTOR Pct2Pistol : Pistol\n{{\n  States\n  {{\n  Fire:\n    PISG A 1\n    PISG A 1 A_OverlayFlags(2, 128, false)\n    PISG A 1 A_OverlayPivot(2, 4, 4)\n    PISG A 1 A_OverlayFlags(1, 128, false)\n    PISG A 1 A_OverlayFlags(2, 128, true)\n    Goto Ready\n  }}\n}}\n",
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
            "{}\nACTOR Mir2Pistol : Pistol\n{{\n  States\n  {{\n  Fire:\n    PISG A 1\n    PISG A 1 A_OverlayFlags(2, 32, true)\n    PISG A 1 A_OverlayFlags(1, 32, true)\n    PISG A 1 A_OverlayFlags(2, 32, false)\n    Goto Ready\n  }}\n}}\n",
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
    fn overlay_flip_mirrors_layer_two_and_leaves_the_gun() {
        let src = format!(
            "{}\nACTOR Flip2Pistol : Pistol\n{{\n  States\n  {{\n  Fire:\n    PISG A 1\n    PISG A 1 A_OverlayFlags(2, 16, true)\n    PISG A 1 A_OverlayFlags(1, 16, true)\n    PISG A 1 A_OverlayFlags(2, 16, false)\n    Goto Ready\n  }}\n}}\n",
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
            "{}\nACTOR EasePistol : Pistol\n{{\n  States\n  {{\n  Fire:\n    PISG A 1\n    PISG A 1 A_OverlayFlags(1, 256, false)\n    PISG A 1 A_OverlayFlags(2, 256, true)\n    PISG A 1 A_OverlayFlags(1, 256, true)\n    Goto Ready\n  }}\n}}\n",
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
            "{}\nACTOR StylePistol : Pistol\n{{\n  States\n  {{\n  Fire:\n    PISG A 1\n    PISG A 1 A_OverlayFlags(1, 512, true)\n    PISG A 1 A_OverlayRenderStyle(1, 2)\n    PISG A 1 A_OverlayFlags(2, 512, false)\n    PISG A 1 A_OverlayRenderStyle(1, 1)\n    PISG A 1 A_OverlayFlags(1, 512, false)\n    Goto Ready\n  }}\n}}\n",
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
            "{}\nACTOR AlphaPistol : Pistol\n{{\n  States\n  {{\n  Fire:\n    PISG A 1\n    PISG A 1 A_OverlayFlags(1, 1024, true)\n    PISG A 1 A_OverlayFlags(2, 1024, false)\n    PISG A 1 A_OverlayFlags(1, 1024, false)\n    Goto Ready\n  }}\n}}\n",
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
            "{}\nACTOR PivotPistol : Pistol\n{{\n  States\n  {{\n  Fire:\n    PISG A 1\n    PISG A 1 A_OverlayFlags(1, 128, false)\n    PISG A 1 A_OverlayPivot(1, 4, 4)\n    PISG A 1 A_OverlayFlags(2, 128, true)\n    PISG A 1 A_OverlayFlags(1, 128, true)\n    Goto Ready\n  }}\n}}\n",
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
            "{}\nACTOR MirrorPistol : Pistol\n{{\n  States\n  {{\n  Fire:\n    PISG A 1\n    PISG A 1 A_OverlayFlags(1, 32, true)\n    PISG A 1 A_OverlayFlags(2, 32, false)\n    PISG A 1 A_OverlayFlags(1, 32, false)\n    Goto Ready\n  }}\n}}\n",
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
            "{}\nACTOR Flip16Pistol : Pistol\n{{\n  States\n  {{\n  Fire:\n    PISG A 1\n    PISG A 1 A_OverlayFlags(1, 16, true)\n    PISG A 1 A_OverlayFlags(2, 16, false)\n    PISG A 1 A_OverlayFlags(1, 16, false)\n    Goto Ready\n  }}\n}}\n",
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
            assert!((a.camera_pitch().degrees() - 20.0).abs() < 1e-6);
        }
        player_think(&mut w, &mut player);
        {
            let a = w.actors.get(mo).unwrap();
            assert!((a.pitch.degrees() - 5.0).abs() < 1e-6);
            assert!((a.camera_pitch().degrees() - 20.0).abs() < 1e-6);
        }
        player_think(&mut w, &mut player);
        let a = w.actors.get(mo).unwrap();
        assert!((a.pitch.degrees() - 5.0).abs() < 1e-6);
        assert!((a.camera_pitch().degrees() - 89.0).abs() < 1e-6);
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
