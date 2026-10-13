//! `P_PlayerThink` / `MovePlayer` subset.

use crate::actor::ActorId;
use crate::class::RenderStyle;
use crate::hitscan::line_attack;
use crate::physics::try_move;
use crate::player::{Buttons, Player, PlayerState};
use crate::use_lines::use_lines;
use crate::world::World;
use room_core::dmath;
use room_core::{Angle, DVec3, StateId};

/// Doom forward/side scale (`forwardmove[0]` ≈ 25 × 2048 / 65536 in map units).
const MOVE_SCALE: f64 = 1.0 / 2048.0;
/// Turn rate: yaw units → degrees (`angleturn` style).
const YAW_TO_DEG: f64 = 360.0 / 65536.0;

/// Tick timed powerups toward expiry.
fn tick_powerups(world: &mut World, player: &mut Player) {
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
}

/// Apply this tic's `TicCmd` to the player's mobj.
pub fn player_think(world: &mut World, player: &mut Player) {
    let Some(mo) = player.mo else {
        return;
    };
    world.thinking_refire = player.refire;
    let cmd = player.cmd;
    // Dead: look around; USE → respawn at Player 1 start (coop-style).
    if player.player_state == PlayerState::Dead {
        if cmd.yaw != 0 || cmd.pitch != 0 {
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
        if cmd.buttons.any(Buttons::USE) {
            if world.respawn_player_at(player) {
                if let Some(mo) = player.mo {
                    crate::acs::start_respawn_scripts(world, mo);
                }
            }
        }
        return;
    }
    if player.player_state != PlayerState::Live {
        return;
    }
    tick_powerups(world, player);
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
    if jump && !player.jump_down && player.on_ground {
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
    // `ChangeCamera` revert: any move input restores the player eye.
    if player.camera_revert
        && (cmd.forward_move != 0 || cmd.side_move != 0 || cmd.yaw != 0 || cmd.pitch != 0)
    {
        player.camera = None;
        player.camera_revert = false;
    }
    move_player(world, mo, &cmd);
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
        let used = world.with_parked_player(player, |world| use_lines(world, mo));
        // Vanilla: failed use (nothing usable / refused) plays `*noway` / oof.
        if !used {
            play_player_sound(world, mo, "*noway");
        }
    }
    player.use_down = use_btn;
    if cmd.weapon_select != 0 {
        select_weapon_slot(world, player, mo, cmd.weapon_select);
    } else if cmd.buttons.any(Buttons::WEAPONNEXT) {
        cycle_weapon(world, player, mo, 1);
    } else if cmd.buttons.any(Buttons::WEAPONPREV) {
        cycle_weapon(world, player, mo, -1);
    }
    // Doom: fire starts from `A_WeaponReady` / `A_ReFire` during psprite advance
    // (not a pre-advance rising edge — that burns a Fire-frame tic early).
    let attack = cmd.buttons.any(Buttons::ATTACK);
    player.attack_down = attack;
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
        // Doom-ish bob amplitude from planar speed (`MAXBOB` ≈ 16).
        let speed = dmath::sqrt(a.vel.x() * a.vel.x() + a.vel.y() * a.vel.y());
        player.bob = (speed * 0.5).clamp(0.0, 16.0);
        // Doom `P_CalcHeight`: bob the eye when grounded (fine-angle from maptime).
        let bob_z = if player.on_ground {
            let t = f64::from(world.level.maptime) * (std::f64::consts::TAU / 20.0);
            player.bob * 0.5 * dmath::sin(t)
        } else {
            0.0
        };
        let mut view_z = a.pos.z() + player.view_height + bob_z;
        // Keep the eye under the ceiling (Doom clamps to `ceilingz - 4`).
        if view_z > planes.ceiling_z - 4.0 {
            view_z = planes.ceiling_z - 4.0;
        }
        player.view_z = view_z;
    }
}

/// Jump the weapon psprite to a class label (`Ready`, `Fire`, …).
///
/// Doom `P_SetPsprite`: call the state's action on **enter**, then keep advancing
/// while `tics == 0` so `A_ReFire` / instant frames resolve in the same tic.
fn set_psprite(world: &mut World, player: &mut Player, mo: ActorId, mut state: StateId) {
    // Cap zero-tic chains so a broken Loop with tics=0 cannot spin forever.
    for _ in 0..64 {
        player.pspr_state = Some(state);
        let (tics, action) = world
            .states
            .get(state)
            .map(|st| (i32::from(st.tics), st.action.clone()))
            .unwrap_or((-1, None));
        player.pspr_tics = tics;
        if let Some(action) = action {
            match action.as_str() {
                "A_ReFire" => {
                    // Hold attack → restart Fire (ammo permitting); else fall through.
                    // Doom `A_ReFire` → `P_FireWeapon` → `P_NoiseAlert`.
                    if player.attack_down
                        && player.pending_weapon.is_none()
                        && world.actors.get(mo).is_some_and(|a| a.health > 0)
                    {
                        // Doom `A_ReFire`: `player->refire++` then `P_FireWeapon`.
                        player.refire = player.refire.saturating_add(1);
                        world.thinking_refire = player.refire;
                        if let Some(wpn) = player.ready_weapon {
                            if try_consume_ammo(world, mo, wpn) {
                                if let Some(fire) = world.states.find_label(wpn, "Fire") {
                                    crate::action::noise_alert(world, mo);
                                    // Restart Fire via the outer loop (fresh tics / action).
                                    state = fire;
                                    continue;
                                }
                            } else if let Some(alt) = best_weapon_with_ammo(world, mo, Some(wpn)) {
                                // Empty clip mid-refire → auto-switch like `P_CheckAmmo`.
                                request_weapon(world, player, mo, alt);
                            }
                        }
                    } else {
                        player.refire = 0;
                        world.thinking_refire = 0;
                    }
                }
                "A_Lower" => a_lower(world, player, mo),
                "A_Raise" => a_raise(world, player, mo),
                "A_CheckReload" => {
                    // Abort SSG pump when ammo < AmmoUse → jump to Ready.
                    if let Some(wpn) = player.ready_weapon {
                        if !has_ammo(world, mo, wpn) {
                            if let Some(ready) = world.states.find_label(wpn, "Ready") {
                                state = ready;
                                continue;
                            }
                        }
                    }
                }
                "A_OpenShotgun2" => play_player_sound(world, mo, "weapons/sshoto"),
                "A_LoadShotgun2" => play_player_sound(world, mo, "weapons/sshotl"),
                "A_CloseShotgun2" => play_player_sound(world, mo, "weapons/sshotc"),
                "A_GunFlash" => start_flash_psprite(world, player),
                "A_Light1" => player.extralight = 1,
                "A_Light2" => player.extralight = 2,
                "A_Light0" | "A_LightDone" => player.extralight = 0,
                "A_WeaponReady" => {
                    // Chainsaw idle buzz while Ready (Doom `A_WeaponReady` / saw).
                    if player
                        .ready_weapon
                        .and_then(|w| world.classes.get(w))
                        .is_some_and(|c| c.name.eq_ignore_ascii_case("Chainsaw"))
                        && world.level.maptime % 4 == 0
                    {
                        play_player_sound(world, mo, "weapons/sawidle");
                    }
                    // Doom: held BT_ATTACK while Ready → `P_FireWeapon` (same tic, no tic burn).
                    if player.cmd.buttons.any(Buttons::ATTACK) {
                        fire_weapon(world, player, mo);
                        // `fire_weapon` already entered Fire (and any 0-tic chain).
                        return;
                    }
                }
                other => {
                    let _ = crate::action::call_action(world, mo, other);
                    if is_fire_action(other) {
                        start_flash_psprite(world, player);
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
    // Doom chainsaw Select plays `weapons/sawup` (DSSAWUP).
    if world
        .classes
        .get(wpn)
        .is_some_and(|c| c.name.eq_ignore_ascii_case("Chainsaw"))
    {
        play_player_sound(world, mo, "weapons/sawup");
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

fn is_fire_action(name: &str) -> bool {
    matches!(
        name,
        "A_FirePistol"
            | "A_FireCGun"
            | "A_FireShotgun"
            | "A_FireShotgun2"
            | "A_FireMissile"
            | "A_FirePlasma"
            | "A_FireBFG"
            | "A_Punch"
            | "A_Saw"
    )
}

fn start_flash_psprite(world: &World, player: &mut Player) {
    let Some(wpn) = player.ready_weapon else {
        return;
    };
    let Some(flash) = world.states.find_label(wpn, "Flash") else {
        return;
    };
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

fn ensure_psprite(world: &mut World, player: &mut Player, mo: ActorId) {
    if player.pspr_state.is_some() {
        return;
    }
    let Some(wpn) = player.ready_weapon else {
        return;
    };
    if let Some(ready) = world.states.find_label(wpn, "Ready") {
        set_psprite(world, player, mo, ready);
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

/// Doom weapon slots → preferred class names (first owned wins).
fn select_weapon_slot(world: &mut World, player: &mut Player, mo: ActorId, slot: u8) {
    let names = world.weapon_slots.weapons_for(slot);
    for name in names {
        let Some(cid) = world.classes.find_name(&name) else {
            continue;
        };
        if world.inventory.count(mo, cid) > 0 {
            request_weapon(world, player, mo, cid);
            return;
        }
    }
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

/// Queue a weapon change; prefer Deselect→A_Lower when available.
fn request_weapon(world: &mut World, player: &mut Player, mo: ActorId, cid: room_core::ClassId) {
    if player.ready_weapon == Some(cid) && player.pending_weapon.is_none() {
        return;
    }
    // Already lowering/raising toward this weapon.
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
        // Cancel a pending switch back to the current ready weapon.
        player.pending_weapon = None;
        return;
    }
    if let Some(deselect) = world.states.find_label(cur, "Deselect") {
        player.pending_weapon = Some(cid);
        set_psprite(world, player, mo, deselect);
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

fn fire_weapon(world: &mut World, player: &mut Player, mo: ActorId) {
    // Block fire while switching.
    if player.pending_weapon.is_some() || (player.pspr_y - crate::player::WEAPON_TOP).abs() > 0.5 {
        return;
    }
    if let Some(wpn) = player.ready_weapon {
        if !try_consume_ammo(world, mo, wpn) {
            // Doom: empty clip → pick the next owned weapon that still has ammo.
            if let Some(alt) = best_weapon_with_ammo(world, mo, Some(wpn)) {
                request_weapon(world, player, mo, alt);
            }
            return;
        }
        if let Some(fire) = world.states.find_label(wpn, "Fire") {
            // Doom `P_FireWeapon`: alert before entering atkstate (not in `A_Fire*`).
            crate::action::noise_alert(world, mo);
            set_psprite(world, player, mo, fire);
            return;
        }
    }
    // Fallback when weapon has no Fire label: immediate hitscan.
    let angle = world.actors.get(mo).map(|a| a.angle).unwrap_or(Angle::ZERO);
    let _ = line_attack(world, mo, angle, 2048.0, 5);
}

/// First owned weapon (Doom slot order) that still has ammo, skipping `exclude`.
fn best_weapon_with_ammo(
    world: &World,
    mo: ActorId,
    exclude: Option<room_core::ClassId>,
) -> Option<room_core::ClassId> {
    owned_weapon_order(world, mo)
        .into_iter()
        .find(|&cid| exclude != Some(cid) && has_ammo(world, mo, cid))
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
        if let Some(a) = world.actors.get_mut(mo) {
            a.vel = DVec3::new(0.0, 0.0, a.vel.z());
        }
        return;
    }
    let rad = actor.angle.radians();
    let (s, c) = (dmath::sin(rad), dmath::cos(rad));
    // Doom: forward along view; +side = strafe right (thrust toward angle−90°).
    // x += forward*cos(a) + side*sin(a); y += forward*sin(a) − side*cos(a)
    let tx = forward * c + side * s;
    let ty = forward * s - side * c;
    if let Some(a) = world.actors.get_mut(mo) {
        a.vel = DVec3::new(tx, ty, a.vel.z());
    }
    let new_pos = DVec3::new(actor.pos.x() + tx, actor.pos.y() + ty, actor.pos.z());
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
        player_think(&mut w, &mut player);
        w.players[0] = Some(player);
        let sounds: Vec<_> = w.drain_sound_events().into_iter().map(|e| e.name).collect();
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
        // Release then press again → reverse (vanilla manual re-use).
        player.cmd.buttons = Buttons(0);
        player_think(&mut w, &mut player);
        player.cmd.buttons = Buttons(Buttons::USE);
        player_think(&mut w, &mut player);
        assert_eq!(w.doors[0].direction, -1);
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
        let mut player = w.players[0].take().unwrap();
        player.cmd.forward_move = 50 * 256; // Doom run-scale ticcmd units
        player_think(&mut w, &mut player);
        assert!(player.bob > 0.0, "bob={}", player.bob);
        w.players[0] = Some(player);
        w.level.maptime = 8;
        let hud = w.player_hud(0).unwrap();
        assert!(hud.bob_x != 0.0 || hud.bob_y != 0.0);
    }

    #[test]
    fn pistol_fire_advances_psprite_frames() {
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
            p.cmd.buttons = Buttons(Buttons::ATTACK);
            p.attack_down = false;
            let mut player = p.clone();
            player_think(&mut w, &mut player);
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
        let ready = w.states.find_label(ssg, "Ready").unwrap();
        assert_eq!(player.pspr_state, Some(ready));
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
}
