//! Game-independent UZDoom actions: `Actor` natives and the `Actor`-level
//! ZScript helpers (ice deaths, morph, special-spot items, scripted marines…).

use super::*;
use crate::actor::AttachedLight;

pub(super) const NAMES: &[&str] = &[
    "A_AttachLight",
    "A_AttachLightDef",
    "A_RemoveLight",
    "A_ChangeModel",
    "A_SetAnimation",
    "A_SetAnimationFrameRate",
    "A_SetModelFlag",
    "A_ClearModelFlag",
    "A_ResetModelFlags",
    "A_CheckTerrain",
    "A_ClassBossHealth",
    "A_DoChase",
    "A_Look2",
    "A_OverlayVertexOffset",
    "A_RestoreSpecialPosition",
    "A_SetMugshotState",
    "A_SetVisibleRotation",
    "A_WolfAttack",
    "A_SetSpriteAngle",
    "A_SetSpriteRotation",
    "A_PlayerSkinCheck",
    "A_Morph",
    "A_NailBomb",
    "A_LightInverse",
    "A_SpawnSingleItem",
    "A_FreezeDeath",
    "A_GenericFreezeDeath",
    "A_FreezeDeathChunks",
    "A_IceSetTics",
    "A_IceGuyDie",
    "A_Feathers",
    "A_DropWeaponPieces",
    "A_BishopMissileWeave",
    "A_CStaffMissileSlither",
    "A_Blast",
    "A_PigPain",
    "A_CentaurDefend",
    "A_RestoreSpecialDoomThing",
    "A_RestoreSpecialThing1",
    "A_RestoreSpecialThing2",
    "A_SkullPop",
    "A_CPosAttackDehacked",
    "A_CPosAttackInternal",
    "A_SPosAttackInternal",
    "A_FireSTGrenade",
    "A_M_Saw",
    "A_M_Refire",
    "A_M_SawRefire",
    "A_MarineNoise",
    "A_MarineChase",
    "A_MarineLook",
    "A_M_Punch",
    "A_M_FirePistol",
    "A_M_FireShotgun",
    "A_M_CheckAttack",
    "A_M_FireShotgun2",
    "A_M_FireCGun",
    "A_M_FireMissile",
    "A_M_FireRailgun",
    "A_M_FirePlasma",
    "A_M_BFGsound",
    "A_M_FireBFG",
];

pub(super) fn dispatch(
    w: &mut World,
    id: ActorId,
    name: &str,
    args: &[String],
) -> Option<ActionResult> {
    let r = match name {
        "A_AttachLight" => attach_light(w, id, args),
        "A_AttachLightDef" => attach_light_def(w, id, args),
        "A_RemoveLight" => remove_light(w, id, args),
        "A_ChangeModel" => change_model(w, id, args),
        "A_SetAnimation" => set_animation(w, id, args),
        "A_SetAnimationFrameRate" => set_animation_rate(w, id, args),
        "A_SetModelFlag" => model_flag(w, id, iarg(args, 0, 0), true),
        "A_ClearModelFlag" => model_flag(w, id, iarg(args, 0, 0), false),
        "A_ResetModelFlags" => {
            if let Some(a) = w.actors.get_mut(id) {
                a.ext.model.flags = None;
            }
            ActionResult::Continue
        }
        "A_CheckTerrain" => check_terrain(w, id),
        "A_ClassBossHealth" => class_boss_health(w, id),
        "A_DoChase" => do_chase(w, id, args),
        "A_Look2" => look2(w, id),
        "A_OverlayVertexOffset" => ActionResult::Continue,
        "A_RestoreSpecialPosition" => restore_special_position(w, id),
        "A_SetMugshotState" => {
            let state = sarg(args, 0, "");
            if !state.is_empty() {
                for p in w.players.iter_mut().flatten() {
                    if p.mo == Some(id) {
                        p.mugshot_state = Some(state.clone());
                    }
                }
            }
            ActionResult::Continue
        }
        "A_SetVisibleRotation" => set_visible_rotation(w, id, args),
        "A_WolfAttack" => wolf_attack(w, id, args),
        "A_SetSpriteAngle" => {
            if let Some(a) = ptr(w, id, iarg(args, 1, 0)).and_then(|p| w.actors.get_mut(p)) {
                a.ext.sprite_angle = farg(args, 0, 0.0);
            }
            ActionResult::Continue
        }
        "A_SetSpriteRotation" => {
            if let Some(a) = ptr(w, id, iarg(args, 1, 0)).and_then(|p| w.actors.get_mut(p)) {
                a.ext.sprite_rotation = farg(args, 0, 0.0);
            }
            ActionResult::Continue
        }
        "A_PlayerSkinCheck" => {
            // Room players always use the class's own sprites (no skins).
            ActionResult::Continue
        }
        "A_Morph" => morph(w, id, args),
        "A_NailBomb" => {
            // `A_Explode(nails: 30)`: the blast plus 30 nail hitscans.
            let r = call(w, id, "A_Explode", &["128", "128", "1"]);
            for i in 0..30 {
                let ang = f64::from(i) * 12.0;
                let _ = line_attack(w, id, ang, MISSILE_RANGE, 0.0, 10, "BulletPuff");
            }
            r
        }
        "A_LightInverse" => {
            for p in w.players.iter_mut().flatten() {
                if p.mo == Some(id) {
                    p.extralight = i32::MIN;
                }
            }
            ActionResult::Continue
        }
        "A_SpawnSingleItem" => spawn_single_item(w, id, args),
        "A_FreezeDeath" => freeze_death(w, id),
        "A_GenericFreezeDeath" => {
            let _ = call(w, id, "A_SetTranslation", &["Ice"]);
            freeze_death(w, id)
        }
        "A_FreezeDeathChunks" => freeze_death_chunks(w, id),
        "A_IceSetTics" => {
            // Ice chunks melt faster on lava, slower on ice (Hexen); plain: 70 + 0..63.
            let t = 70 + (rnd(w, "pr_icesettics") & 63);
            if let Some(a) = w.actors.get_mut(id) {
                a.tics = t;
            }
            ActionResult::Continue
        }
        "A_IceGuyDie" => {
            set_vel(w, id, DVec3::ZERO);
            reset_height(w, id);
            freeze_death_chunks(w, id)
        }
        "A_Feathers" => feathers(w, id),
        "A_DropWeaponPieces" => drop_weapon_pieces(w, id, args),
        "A_BishopMissileWeave" => call(w, id, "A_Weave", &["2", "2", "2.0", "1.0"]),
        "A_CStaffMissileSlither" => call(w, id, "A_Weave", &["3", "0", "1.0", "0.0"]),
        "A_Blast" => blast(w, id, args),
        "A_PigPain" => {
            let r = call(w, id, "A_Pain", &[]);
            let (floor, _) = floor_ceil(w, id);
            if pos(w, id).z() <= floor {
                set_vel_z(w, id, 3.5);
            }
            r
        }
        "A_CentaurDefend" => {
            face_target(w, id);
            if in_melee_range(w, id) && rnd(w, "pr_centaurdefend") < 32 {
                set_flag(w, id, "REFLECTIVE", false);
                set_flag(w, id, "INVULNERABLE", false);
                let _ = set_label(w, id, "Melee");
            }
            ActionResult::Continue
        }
        "A_RestoreSpecialDoomThing" => {
            set_flag(w, id, "INVISIBLE", false);
            set_flag(w, id, "SPECIAL", true);
            restore_gravity(w, id);
            let _ = set_label(w, id, "Spawn");
            sound(w, id, "misc/spawn");
            let p = pos(w, id);
            let _ = spawn(w, "ItemFog", p);
            ActionResult::Continue
        }
        "A_RestoreSpecialThing1" => {
            set_flag(w, id, "INVISIBLE", false);
            sound(w, id, "misc/spawn");
            ActionResult::Continue
        }
        "A_RestoreSpecialThing2" => {
            set_flag(w, id, "SPECIAL", true);
            restore_gravity(w, id);
            let _ = set_label(w, id, "Spawn");
            ActionResult::Continue
        }
        "A_SkullPop" => skull_pop(w, id, args),
        "A_CPosAttackDehacked" => cpos_attack(w, id, "chainguy/attack"),
        "A_CPosAttackInternal" => {
            let snd = sarg(args, 0, "chainguy/attack");
            cpos_attack(w, id, &snd)
        }
        "A_SPosAttackInternal" => spos_attack(w, id),
        "A_FireSTGrenade" => {
            let ty = sarg(args, 0, "Grenade");
            if ty.is_empty() || !is_player(w, id) || !deplete_ammo(w, id) {
                return Some(ActionResult::Continue);
            }
            // Raise the pitch 6.328125° so the grenade arcs up.
            let saved = w.actors.get(id).map(|a| a.pitch);
            if let Some(a) = w.actors.get_mut(id) {
                a.pitch = Angle::from_degrees(a.pitch.degrees() + 6.328_125);
            }
            let _ = spawn_player_missile(w, id, &ty, 0.0);
            if let (Some(p), Some(a)) = (saved, w.actors.get_mut(id)) {
                a.pitch = p;
            }
            ActionResult::Continue
        }
        "A_M_Saw" => marine_saw(w, id, args),
        "A_M_Refire" => marine_refire(w, id, args),
        "A_M_SawRefire" => {
            let ok = target(w, id).is_some_and(|t| health(w, t) > 0) && in_melee_range(w, id);
            if !ok {
                let _ = set_label(w, id, "See");
            }
            ActionResult::Continue
        }
        "A_MarineNoise" => {
            marine_noise(w, id);
            ActionResult::Continue
        }
        "A_MarineChase" => {
            marine_noise(w, id);
            crate::action::a_chase(w, id)
        }
        "A_MarineLook" => {
            marine_noise(w, id);
            crate::action::a_look(w, id)
        }
        "A_M_Punch" => marine_punch(w, id, iarg(args, 0, 1).max(1)),
        "A_M_FirePistol" | "A_M_FireCGun" => {
            if target(w, id).is_none() {
                return Some(ActionResult::Continue);
            }
            sound(
                w,
                id,
                if name == "A_M_FirePistol" {
                    "weapons/pistol"
                } else {
                    "weapons/chngun"
                },
            );
            face_target(w, id);
            let slope = aim(w, id, angle(w, id), MISSILE_RANGE);
            gun_shot2(w, id, crate::action::parse_bool_arg(args, 0), slope);
            ActionResult::Continue
        }
        "A_M_FireShotgun" => {
            if target(w, id).is_none() {
                return Some(ActionResult::Continue);
            }
            sound(w, id, "weapons/shotgf");
            face_target(w, id);
            let slope = aim(w, id, angle(w, id), MISSILE_RANGE);
            for _ in 0..7 {
                gun_shot2(w, id, false, slope);
            }
            marine_cooldown(w, id, 27);
            ActionResult::Continue
        }
        "A_M_CheckAttack" => {
            if marine_cooling(w, id) || target(w, id).is_none() {
                let _ = set_label(w, id, "SkipAttack");
            } else {
                face_target(w, id);
            }
            ActionResult::Continue
        }
        "A_M_FireShotgun2" => {
            if target(w, id).is_none() {
                return Some(ActionResult::Continue);
            }
            sound(w, id, "weapons/sshotf");
            face_target(w, id);
            let slope = aim(w, id, angle(w, id), MISSILE_RANGE);
            let base = angle(w, id);
            for _ in 0..20 {
                let dmg = 5 * rnd_range(w, "pr_smarinefiressg", 1, 3);
                let ang = base + f64::from(rnd2(w, "pr_smarinefiressg")) * (11.25 / 256.0);
                let s = slope
                    + (f64::from(rnd2(w, "pr_smarinefiressg")) * (7.097 / 256.0))
                        .to_radians()
                        .tan();
                let _ = line_attack(w, id, ang, MISSILE_RANGE, s, dmg, "BulletPuff");
            }
            marine_cooldown(w, id, 0);
            ActionResult::Continue
        }
        "A_M_FireMissile" => {
            let Some(t) = target(w, id) else {
                return Some(ActionResult::Continue);
            };
            if in_melee_range(w, id) {
                return Some(marine_punch(w, id, 1));
            }
            face_target(w, id);
            let _ = spawn_missile(w, id, t, "Rocket");
            ActionResult::Continue
        }
        "A_M_FireRailgun" => {
            if target(w, id).is_none() {
                return Some(ActionResult::Continue);
            }
            let r = call(w, id, "A_MonsterRail", &[]);
            marine_cooldown(w, id, 50);
            r
        }
        "A_M_FirePlasma" | "A_M_FireBFG" => {
            let Some(t) = target(w, id) else {
                return Some(ActionResult::Continue);
            };
            face_target(w, id);
            let (class, cool) = if name == "A_M_FirePlasma" {
                ("PlasmaBall", 20)
            } else {
                ("BFGBall", 30)
            };
            let _ = spawn_missile(w, id, t, class);
            marine_cooldown(w, id, cool);
            ActionResult::Continue
        }
        "A_M_BFGsound" => {
            if target(w, id).is_none() {
                return Some(ActionResult::Continue);
            }
            if marine_cooling(w, id) {
                let _ = set_label(w, id, "See");
            } else {
                face_target(w, id);
                sound(w, id, "weapons/bfgf");
            }
            ActionResult::Continue
        }
        _ => return None,
    };
    Some(r)
}

/// `GetPointer(ptr)` for the common `AAPTR_*` values.
pub(crate) fn ptr(w: &World, id: ActorId, aaptr: i32) -> Option<ActorId> {
    match aaptr {
        0 | 1 => Some(id),
        2 => target(w, id),
        4 => master(w, id),
        8 => tracer(w, id),
        _ => Some(id),
    }
}

fn restore_gravity(w: &mut World, id: ActorId) {
    let default_nograv = w
        .actors
        .get(id)
        .and_then(|a| w.classes.get(a.class))
        .is_some_and(|c| c.flags & crate::class::FLAG_NOGRAVITY != 0);
    if !default_nograv {
        set_flag(w, id, "NOGRAVITY", false);
    }
}

fn reset_height(w: &mut World, id: ActorId) {
    let h = w
        .actors
        .get(id)
        .and_then(|a| w.classes.get(a.class))
        .map(|c| c.height);
    if let (Some(h), Some(a)) = (h, w.actors.get_mut(id)) {
        a.height = h;
    }
}

/// Parse a light colour: `"ff 80 00"`, `"#ff8000"`, `0xff8000`, `"red"`.
pub(crate) fn parse_color(s: &str) -> u32 {
    let t = s.trim().trim_matches('"').trim();
    if let Some(h) = t.strip_prefix('#') {
        return u32::from_str_radix(h, 16).unwrap_or(0xffffff);
    }
    if let Some(h) = t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")) {
        return u32::from_str_radix(h, 16).unwrap_or(0xffffff);
    }
    let parts: Vec<&str> = t.split_whitespace().collect();
    if parts.len() == 3 {
        let c: Vec<u32> = parts
            .iter()
            .map(|p| u32::from_str_radix(p, 16).unwrap_or(0) & 0xff)
            .collect();
        return (c[0] << 16) | (c[1] << 8) | c[2];
    }
    if let Ok(n) = t.parse::<u32>() {
        return n;
    }
    match t.to_ascii_lowercase().as_str() {
        "red" => 0xff0000,
        "green" => 0x00ff00,
        "blue" => 0x0000ff,
        "yellow" => 0xffff00,
        "orange" => 0xff8000,
        "purple" => 0x800080,
        "cyan" => 0x00ffff,
        "black" => 0,
        _ => 0xffffff,
    }
}

/// Join a `(x, y, z)` vector that DECORATE argument splitting broke apart.
fn vector_arg(args: &[String], start: usize) -> (DVec3, usize) {
    let Some(first) = args.get(start) else {
        return (DVec3::ZERO, start);
    };
    if first.trim_start().starts_with('(') {
        let mut parts = Vec::new();
        let mut i = start;
        while i < args.len() {
            parts.push(
                args[i]
                    .trim()
                    .trim_start_matches('(')
                    .trim_end_matches(')')
                    .to_owned(),
            );
            let done = args[i].trim_end().ends_with(')');
            i += 1;
            if done {
                break;
            }
        }
        let n: Vec<f64> = parts
            .iter()
            .map(|p| p.parse::<f64>().unwrap_or(0.0))
            .collect();
        let g = |k: usize| n.get(k).copied().unwrap_or(0.0);
        return (DVec3::new(g(0), g(1), g(2)), i);
    }
    (DVec3::ZERO, start + 1)
}

/// `A_AttachLight(id, type, color, radius1, radius2 [, flags, ofs, param, spoti, spoto, spotp, intensity])`.
fn attach_light(w: &mut World, id: ActorId, args: &[String]) -> ActionResult {
    let name = sarg(args, 0, "").to_ascii_lowercase();
    if name.is_empty() {
        return ActionResult::Continue;
    }
    let (ofs, next) = vector_arg(args, 6);
    let light = AttachedLight {
        id: name.clone(),
        def: String::new(),
        kind: iarg(args, 1, 0),
        color: parse_color(args.get(2).map_or("ffffff", String::as_str)),
        radius: (farg(args, 3, 64.0), farg(args, 4, 64.0)),
        flags: iarg(args, 5, 0),
        offset: ofs,
        param: farg(args, next, 0.0),
        spot: (farg(args, next + 1, 10.0), farg(args, next + 2, 25.0)),
        intensity: farg(args, next + 4, 1.0),
    };
    put_light(w, id, light);
    ActionResult::Continue
}

/// `A_AttachLightDef(id, lightdef)`: a GLDEFS light by name.
fn attach_light_def(w: &mut World, id: ActorId, args: &[String]) -> ActionResult {
    let name = sarg(args, 0, "").to_ascii_lowercase();
    let def = sarg(args, 1, "");
    if name.is_empty() || def.is_empty() {
        return ActionResult::Continue;
    }
    put_light(
        w,
        id,
        AttachedLight {
            id: name,
            def,
            kind: 0,
            color: 0xffffff,
            radius: (0.0, 0.0),
            flags: 0,
            offset: DVec3::ZERO,
            param: 0.0,
            spot: (0.0, 0.0),
            intensity: 1.0,
        },
    );
    ActionResult::Continue
}

fn put_light(w: &mut World, id: ActorId, light: AttachedLight) {
    if let Some(a) = w.actors.get_mut(id) {
        if let Some(slot) = a.ext.lights.iter_mut().find(|l| l.id == light.id) {
            *slot = light;
        } else {
            a.ext.lights.push(light);
        }
    }
}

fn remove_light(w: &mut World, id: ActorId, args: &[String]) -> ActionResult {
    let name = sarg(args, 0, "").to_ascii_lowercase();
    if let Some(a) = w.actors.get_mut(id) {
        a.ext.lights.retain(|l| l.id != name);
    }
    ActionResult::Continue
}

/// `A_ChangeModel(modeldef, modelindex, modelpath, model, skinindex, skinpath, skin, flags, …)`.
fn change_model(w: &mut World, id: ActorId, args: &[String]) -> ActionResult {
    let modeldef = sarg(args, 0, "");
    let index = iarg(args, 1, 0);
    let entry = (
        index,
        sarg(args, 2, ""),
        sarg(args, 3, ""),
        sarg(args, 5, ""),
        sarg(args, 6, ""),
    );
    let flags = iarg(args, 7, 0);
    if let Some(a) = w.actors.get_mut(id) {
        let m = &mut a.ext.model;
        if !modeldef.is_empty() {
            m.modeldef = modeldef;
        }
        // CMDL_WEAPONTOPSPRITE (1) aside, an empty model and skin resets the slot.
        let _ = flags;
        m.models.retain(|e| e.0 != index);
        if !(entry.1.is_empty() && entry.2.is_empty() && entry.3.is_empty() && entry.4.is_empty()) {
            m.models.push(entry);
        }
        // `animationindex`, `animationpath`, `animation` (UZDoom 4.11+).
        let anim_path = sarg(args, 10, "");
        let anim_file = sarg(args, 11, "");
        if !anim_file.is_empty() {
            m.animation_file = Some((iarg(args, 9, 0), anim_path, anim_file));
        } else if args.len() > 11 {
            m.animation_file = None;
        }
    }
    ActionResult::Continue
}

fn set_animation(w: &mut World, id: ActorId, args: &[String]) -> ActionResult {
    let name = sarg(args, 0, "");
    let now = w.level.maptime;
    if let Some(a) = w.actors.get_mut(id) {
        // Keep what was playing so the renderer can blend out of it
        // (`interpolateTics`, UZDoom `prevAnim`).
        a.ext.model.prev_animation = a
            .ext
            .model
            .animation
            .take()
            .map(|prev| (prev, a.ext.model.animation_start, now));
        a.ext.model.animation = (!name.is_empty()).then(|| {
            (
                name,
                farg(args, 1, -1.0),
                iarg(args, 2, -1),
                iarg(args, 3, -1),
                iarg(args, 4, -1),
                iarg(args, 5, -1),
                iarg(args, 6, 0),
            )
        });
        a.ext.model.animation_start = now;
    }
    ActionResult::Continue
}

fn set_animation_rate(w: &mut World, id: ActorId, args: &[String]) -> ActionResult {
    let rate = farg(args, 0, -1.0);
    if let Some(anim) = w
        .actors
        .get_mut(id)
        .and_then(|a| a.ext.model.animation.as_mut())
    {
        anim.1 = rate;
    }
    ActionResult::Continue
}

fn model_flag(w: &mut World, id: ActorId, flag: i32, on: bool) -> ActionResult {
    let def_flags = 0u32;
    if let Some(a) = w.actors.get_mut(id) {
        let f = a.ext.model.flags.get_or_insert(def_flags);
        if on {
            *f |= flag as u32;
        } else {
            *f &= !(flag as u32);
        }
    }
    ActionResult::Continue
}

/// `A_CheckTerrain`: instant-death floors kill; Strife current floors push.
fn check_terrain(w: &mut World, id: ActorId) -> ActionResult {
    let (floor, _) = floor_ceil(w, id);
    if (pos(w, id).z() - floor).abs() > 0.01 {
        return ActionResult::Continue;
    }
    let p = pos(w, id);
    let Some(sid) = crate::physics::sector_at_xy(w, p.x(), p.y()) else {
        return ActionResult::Continue;
    };
    let Some(sec) = w.map.sectors.get(sid) else {
        return ActionResult::Continue;
    };
    let special = sec.special;
    let tag = sec.tags.first().copied().unwrap_or(0);
    let amount = w.sectors.get(sid).map_or(0, |s| s.damage_amount);
    if amount >= 1_000_000 {
        let _ = damage(w, id, None, None, 999);
    } else if special == 118 {
        // `Scroll_StrifeCurrent`: tag − 100 encodes direction (tens) and speed.
        let anglespeed = tag - 100;
        let speed = f64::from(anglespeed % 10) / 16.0;
        let dir = f64::from(anglespeed / 10) * 45.0;
        thrust(w, id, dir, speed);
    }
    ActionResult::Continue
}

fn class_boss_health(w: &mut World, id: ActorId) -> ActionResult {
    let coop = w.players.iter().flatten().count() > 1 && !w.deathmatch;
    if coop {
        if let Some(a) = w.actors.get_mut(id) {
            if a.ext.special1 == 0 {
                a.health *= 5;
                a.ext.special1 = 1;
            }
        }
    }
    ActionResult::Continue
}

/// `A_DoChase(melee, missile [, flags])`: `A_Chase` with explicit states.
fn do_chase(w: &mut World, id: ActorId, args: &[String]) -> ActionResult {
    let melee = sarg(args, 0, "Melee");
    let missile = sarg(args, 1, "Missile");
    let flags = iarg(args, 2, 0).to_string();
    let m1 = if melee.is_empty() {
        "null".to_owned()
    } else {
        melee
    };
    let m2 = if missile.is_empty() {
        "null".to_owned()
    } else {
        missile
    };
    call(w, id, "A_Chase", &[&m1, &m2, &flags])
}

/// `A_Look2` (Strife): wake on a heard enemy, else fidget through the
/// Spawn+1 / Spawn+2 / Spawn+3 frames.
fn look2(w: &mut World, id: ActorId) -> ActionResult {
    if has_flag(w, id, "INCONVERSATION") {
        return ActionResult::Continue;
    }
    if let Some(a) = w.actors.get_mut(id) {
        a.threshold = 0;
    }
    let heard = w
        .actors
        .get(id)
        .and_then(|a| a.last_heard)
        .filter(|&t| health(w, t) > 0 && has_flag(w, t, "SHOOTABLE"));
    if let Some(t) = heard {
        let friend = |w: &World, x: ActorId| has_flag(w, x, "FRIENDLY");
        if friend(w, id) != friend(w, t) {
            if !(has_flag(w, id, "AMBUSH") && !check_sight(w, id, t)) {
                if let Some(a) = w.actors.get_mut(id) {
                    a.target = Some(t);
                    a.threshold = 10;
                }
                let _ = set_label(w, id, "See");
                return ActionResult::Continue;
            }
        } else if let ActionResult::Jump(s) = crate::action::a_look(w, id) {
            set_flag(w, id, "INCOMBAT", true);
            return ActionResult::Jump(s);
        }
    }
    if rnd(w, "pr_look2") < 30 {
        let n = (rnd(w, "pr_look2") & 1) + 1;
        set_spawn_state_plus(w, id, n as u32);
    }
    if !has_flag(w, id, "STANDSTILL") && rnd(w, "pr_look2") < 40 {
        set_spawn_state_plus(w, id, 3);
    }
    ActionResult::Continue
}

/// `A_RestoreSpecialPosition`: an item returns to where it was placed.
fn restore_special_position(w: &mut World, id: ActorId) -> ActionResult {
    let Some(sp) = w.actors.get(id).map(|a| a.spawn_point) else {
        return ActionResult::Continue;
    };
    if let Some(a) = w.actors.get_mut(id) {
        a.pos = DVec3::new(sp.x(), sp.y(), a.pos.z());
    }
    let (floor, ceil) = floor_ceil(w, id);
    let spawnceiling = w
        .actors
        .get(id)
        .is_some_and(|a| a.flags & crate::class::FLAG_SPAWNCEILING != 0);
    let h = w.actors.get(id).map_or(0.0, |a| a.height);
    let z = if spawnceiling {
        ceil - h - sp.z()
    } else if has_flag(w, id, "SPAWNFLOAT") {
        let space = ceil - h - floor;
        if space > 48.0 {
            (space - 40.0) * f64::from(rnd(w, "pr_restore")) / 256.0 + floor + 40.0
        } else {
            floor
        }
    } else {
        sp.z() + floor
    };
    if let Some(a) = w.actors.get_mut(id) {
        a.pos = DVec3::new(a.pos.x(), a.pos.y(), z.max(floor));
    }
    ActionResult::Continue
}

fn set_visible_rotation(w: &mut World, id: ActorId, args: &[String]) -> ActionResult {
    let flags = iarg(args, 4, 0);
    let Some(t) = ptr(w, id, iarg(args, 5, 0)) else {
        return ActionResult::Continue;
    };
    let (a0, a1, p0, p1) = (
        farg(args, 0, 0.0),
        farg(args, 1, 0.0),
        farg(args, 2, 0.0),
        farg(args, 3, 0.0),
    );
    if let Some(a) = w.actors.get_mut(t) {
        let (mut s, mut e) = a.ext.visible_angles.unwrap_or((0.0, 0.0));
        if flags & 1 == 0 {
            s = a0;
        }
        if flags & 2 == 0 {
            e = a1;
        }
        a.ext.visible_angles = if s == 0.0 && e == 0.0 {
            None
        } else {
            Some((s, e))
        };
        let (mut ps, mut pe) = a.ext.visible_pitch.unwrap_or((0.0, 0.0));
        if flags & 4 == 0 {
            ps = p0;
        }
        if flags & 8 == 0 {
            pe = p1;
        }
        a.ext.visible_pitch = if ps == 0.0 && pe == 0.0 {
            None
        } else {
            Some((ps, pe))
        };
    }
    ActionResult::Continue
}

/// `A_WolfAttack([flags, sound, snipe, maxdamage, blocksize, pointblank, longrange, runspeed, puff])`.
fn wolf_attack(w: &mut World, id: ActorId, args: &[String]) -> ActionResult {
    let flags = iarg(args, 0, 0);
    let snd = sarg(args, 1, "weapons/pistol");
    let snipe = farg(args, 2, 1.0);
    let maxdamage = iarg(args, 3, 64).max(1);
    let blocksize = iarg(args, 4, 128).max(1);
    let pointblank = farg(args, 5, 2.0);
    let longrange = farg(args, 6, 4.0);
    let runspeed = farg(args, 7, 160.0);
    let puff = sarg(args, 8, "BulletPuff");
    let Some(t) = target(w, id) else {
        return ActionResult::Continue;
    };
    if !check_sight(w, id, t) {
        return ActionResult::Continue;
    }
    face_target(w, id);
    let from_target = angle_to(w, t, id);
    let facing = delta_angle(angle(w, t), from_target).abs();
    let dodge = check_sight(w, t, id) && facing < 30.0 * 256.0 / 360.0;
    let (pa, pb) = (pos(w, id), pos(w, t));
    let dist = (pb.x() - pa.x()).abs().max((pb.y() - pa.y()).abs()) * snipe;
    let idist = (dist / f64::from(blocksize)) as i32;
    let v = vel(w, t);
    let speed = v.x() * v.x() + v.y() * v.y() + v.z() * v.z();
    let mut hitchance: i32 = if speed < runspeed { 256 } else { 160 };
    hitchance -= idist * if dodge { 16 } else { 8 };
    if has_flag(w, t, "SHADOW") {
        hitchance >>= 2;
    }
    if rnd(w, "pr_cabullet") < hitchance {
        let mut dmg = if flags & 1 != 0 {
            maxdamage
        } else {
            1 + rnd(w, "pr_cabullet") % maxdamage
        };
        if dist >= pointblank * f64::from(blocksize) {
            dmg >>= 1;
        }
        if dist >= longrange * f64::from(blocksize) {
            dmg >>= 1;
        }
        if flags & 2 == 0 && has_flag(w, t, "GHOST") {
            dmg >>= 2;
        }
        if flags & 2 != 0 && !puff.is_empty() {
            let at = pos(w, t);
            let _ = spawn(
                w,
                &puff,
                DVec3::new(
                    at.x(),
                    at.y(),
                    at.z() + w.actors.get(t).map_or(0.0, |a| a.height) * 0.5,
                ),
            );
        }
        let _ = damage(w, t, Some(id), Some(id), dmg);
    }
    sound(w, id, &snd);
    ActionResult::Continue
}

/// `A_Morph(type [, duration, flags, morphflash, unmorphflash])`: turn a monster
/// into `type` (players go through the `PowerMorph` path).
fn morph(w: &mut World, id: ActorId, args: &[String]) -> ActionResult {
    let ty = sarg(args, 0, "");
    if ty.is_empty() {
        return ActionResult::Continue;
    }
    let duration = iarg(args, 1, 0);
    let flash = sarg(args, 3, "TeleportFog");
    if is_player(w, id) {
        return ActionResult::Continue;
    }
    morph_monster(w, id, &ty, duration, &flash);
    ActionResult::Continue
}

/// GZDoom `MorphMonster`: spawn `ty` in place, hide the original as its
/// `tracer`, and unmorph after `duration` (default 40 s + up to 8 s).
pub(crate) fn morph_monster(
    w: &mut World,
    id: ActorId,
    ty: &str,
    duration: i32,
    flash: &str,
) -> Option<ActorId> {
    if has_flag(w, id, "BOSS") || has_flag(w, id, "DONTMORPH") || health(w, id) <= 0 {
        return None;
    }
    let p = pos(w, id);
    let ang = angle(w, id);
    let new = spawn_facing(w, ty, p, ang)?;
    let tics = if duration > 0 {
        duration
    } else {
        40 * 35 + rnd(w, "pr_morphmonst") * 8 * 35 / 256
    };
    let (target_of, special, tid) = w
        .actors
        .get(id)
        .map(|a| (a.target, a.special, a.tid))
        .unwrap_or((None, 0, 0));
    if let Some(m) = w.actors.get_mut(new) {
        m.target = target_of;
        m.tracer = Some(id);
        m.special = special;
        m.tid = tid;
        m.ext.special1 = tics;
        m.ext.scratch.insert(
            "morph_unmorph_at".into(),
            f64::from(w.level.maptime) + f64::from(tics),
        );
    }
    if let Some(a) = w.actors.get_mut(id) {
        a.special = 0;
        a.tid = 0;
        a.flags &= !(crate::class::FLAG_SOLID | crate::class::FLAG_SHOOTABLE);
        a.flags |= crate::class::FLAG_NOGRAVITY;
        a.extra_flags.set_name("INVISIBLE", true);
        a.ext.scratch.insert("morphed".into(), 1.0);
        a.state = None;
        a.tics = -1;
    }
    if !flash.is_empty() {
        let _ = spawn(w, flash, DVec3::new(p.x(), p.y(), p.z() + 20.0));
    }
    Some(new)
}

/// `A_SpawnSingleItem(class [, fail_sp, fail_co, fail_dm])`: a special spot
/// spawns `class` at a random spot of its own class.
fn spawn_single_item(w: &mut World, id: ActorId, args: &[String]) -> ActionResult {
    let cls = sarg(args, 0, "");
    let spot_class = class_name(w, id);
    let spots = actors_of(w, &spot_class);
    if spots.is_empty() {
        return ActionResult::Continue;
    }
    let pick = spots[(rnd(w, "pr_spawnmace") as usize) % spots.len()];
    let mp = w.players.iter().flatten().count() > 1;
    let fail = if !mp {
        iarg(args, 1, 0)
    } else if !w.deathmatch {
        iarg(args, 2, 0)
    } else {
        iarg(args, 3, 0)
    };
    if rnd(w, "pr_spawnmace") < fail || cls.is_empty() {
        return ActionResult::Continue;
    }
    let at = pos(w, pick);
    if let Some(mo) = spawn(w, &cls, at) {
        let (floor, _) = floor_ceil(w, mo);
        if let Some(a) = w.actors.get_mut(mo) {
            a.pos = DVec3::new(at.x(), at.y(), floor);
        }
    }
    ActionResult::Continue
}

/// `A_FreezeDeath`: turn into a solid, pushable ice statue for 75 + 2d256 tics.
fn freeze_death(w: &mut World, id: ActorId) -> ActionResult {
    let t = rnd(w, "pr_freezedeath");
    let t2 = rnd(w, "pr_freezedeath");
    reset_height(w, id);
    if let Some(a) = w.actors.get_mut(id) {
        a.tics = 75 + t + t2;
        a.flags |=
            crate::class::FLAG_SOLID | crate::class::FLAG_SHOOTABLE | crate::class::FLAG_NOBLOOD;
        for f in [
            "ICECORPSE",
            "PUSHABLE",
            "TELESTOMP",
            "CANPASS",
            "SLIDESONWALLS",
            "CRASHED",
        ] {
            a.extra_flags.set_name(f, true);
        }
        a.ext.ice_corpse = true;
        a.render_style = crate::class::RenderStyle::Normal;
        if a.extra_flags.get_name("STEALTH") {
            a.alpha = 1.0;
        }
    }
    sound(w, id, "misc/freeze");
    if is_player(w, id) {
        for p in w.players.iter_mut().flatten() {
            if p.mo == Some(id) {
                p.damage_count = 0;
                p.bonus_count = 0;
            }
        }
    } else {
        let (special, args) = w
            .actors
            .get(id)
            .map_or((0, [0; 5]), |a| (a.special, a.args));
        if has_flag(w, id, "ISMONSTER") && special != 0 {
            let _ = crate::specials::execute_special(w, special, &args, Some(id), None);
            if let Some(a) = w.actors.get_mut(id) {
                a.special = 0;
            }
        }
    }
    ActionResult::Continue
}

/// `A_FreezeDeathChunks`: shatter into `IceChunk`s once the statue stops.
fn freeze_death_chunks(w: &mut World, id: ActorId) -> ActionResult {
    let v = vel(w, id);
    if (v.x() != 0.0 || v.y() != 0.0 || v.z() != 0.0) && !has_flag(w, id, "SHATTERING") {
        if let Some(a) = w.actors.get_mut(id) {
            a.tics = 3 * 35;
        }
        return ActionResult::Continue;
    }
    set_vel(w, id, DVec3::ZERO);
    sound(w, id, "misc/icebreak");
    let (r, h) = w
        .actors
        .get(id)
        .map_or((20.0, 56.0), |a| (a.radius, a.height.max(1.0)));
    let p = pos(w, id);
    let num = 4.max((r * h) as i32 / 32);
    let extra = rnd_range(w, "pr_freezedeathchunks", 0, (num / 4 - 1).max(0));
    let mut i = 24.max(num + extra);
    while i >= 0 {
        let xo = f64::from(rnd(w, "pr_freezedeathchunks") - 128) * r / 128.0;
        let yo = f64::from(rnd(w, "pr_freezedeathchunks") - 128) * r / 128.0;
        let zo = f64::from(rnd(w, "pr_freezedeathchunks")) * h / 255.0;
        if let Some(mo) = spawn(
            w,
            "IceChunk",
            DVec3::new(p.x() + xo, p.y() + yo, p.z() + zo),
        ) {
            let n = rnd_range(w, "pr_freezedeathchunks", 0, 2);
            set_spawn_state_plus(w, mo, n as u32);
            let vx = f64::from(rnd2(w, "pr_freezedeathchunks")) / 128.0;
            let vy = f64::from(rnd2(w, "pr_freezedeathchunks")) / 128.0;
            set_vel(w, mo, DVec3::new(vx, vy, zo / h * 4.0));
        }
        i -= 1;
    }
    if has_flag(w, id, "BOSSDEATH") {
        let _ = crate::action::a_boss_death(w, id);
    }
    let _ = crate::action::a_no_blocking(w, id);
    w.set_state(id, None, false);
    if let Some(a) = w.actors.get_mut(id) {
        a.destroyed = true;
    }
    ActionResult::Continue
}

/// `A_Feathers` (Heretic chicken): 1–2 feathers in pain, 5–8 on death.
fn feathers(w: &mut World, id: ActorId) -> ActionResult {
    let count = if health(w, id) > 0 {
        if rnd(w, "pr_feathers") < 32 { 2 } else { 1 }
    } else {
        5 + rnd_range(w, "pr_feathers", 0, 3)
    };
    let p = pos(w, id);
    for _ in 0..count {
        let Some(mo) = spawn(w, "Feather", DVec3::new(p.x(), p.y(), p.z() + 20.0)) else {
            continue;
        };
        let vx = f64::from(rnd2(w, "pr_feathers")) / 256.0;
        let vy = f64::from(rnd2(w, "pr_feathers")) / 256.0;
        let vz = 1.0 + f64::from(rnd(w, "pr_feathers")) / 128.0;
        if let Some(a) = w.actors.get_mut(mo) {
            a.target = Some(id);
        }
        set_vel(w, mo, DVec3::new(vx, vy, vz));
        let n = rnd_range(w, "pr_feathers", 0, 7);
        set_spawn_state_plus(w, mo, n as u32);
    }
    ActionResult::Continue
}

/// `A_DropWeaponPieces(p1, p2, p3)`: three pieces fly out 120° apart.
fn drop_weapon_pieces(w: &mut World, id: ActorId, args: &[String]) -> ActionResult {
    let classes = [sarg(args, 0, ""), sarg(args, 1, ""), sarg(args, 2, "")];
    let p = pos(w, id);
    let base = vel(w, id);
    let mut j = 0usize;
    for i in 0..3 {
        let cls = classes[j.min(2)].clone();
        if let Some(piece) = spawn(w, &cls, p) {
            let (dx, dy) = angle_vec(f64::from(i) * 120.0, 1.0);
            set_vel(w, piece, DVec3::new(base.x() + dx, base.y() + dy, base.z()));
            set_dropped(w, piece);
        }
        j = if j == 0 {
            rnd_range(w, "pr_piecedrop", 1, 2) as usize
        } else {
            3 - j
        };
    }
    ActionResult::Continue
}

/// `A_Blast([flags, strength, radius, speed, blasteffect, blastsound])` (Hexen
/// disc of repulsion): push monsters, players and missiles away.
fn blast(w: &mut World, id: ActorId, args: &[String]) -> ActionResult {
    let flags = iarg(args, 0, 0);
    let strength = farg(args, 1, 255.0);
    let radius = farg(args, 2, 255.0);
    let speed = farg(args, 3, 20.0);
    let effect = sarg(args, 4, "BlastEffect");
    let snd = sarg(args, 5, "BlastRadius");
    if is_player(w, id) && flags & 1 != 0 && !deplete_ammo(w, id) {
        return ActionResult::Continue;
    }
    sound(w, id, &snd);
    if flags & 8 == 0 {
        let _ = call(w, id, "A_AlertMonsters", &[]);
    }
    let ids: Vec<ActorId> = w.thinkers.default.clone();
    for mo in ids {
        if mo == id {
            continue;
        }
        let Some(a) = w.actors.get(mo) else {
            continue;
        };
        if a.destroyed {
            continue;
        }
        let boss = has_flag(w, mo, "BOSS");
        if (boss && flags & 2 == 0) || has_flag(w, mo, "DORMANT") || has_flag(w, mo, "DONTBLAST") {
            continue;
        }
        let monster = has_flag(w, mo, "ISMONSTER");
        let missile = has_flag(w, mo, "MISSILE");
        let player = is_player(w, mo);
        let special = has_flag(w, mo, "ICECORPSE") || has_flag(w, mo, "CANBLAST");
        if !special {
            if monster && health(w, mo) <= 0 {
                continue;
            }
            if !player
                && !missile
                && !monster
                && !has_flag(w, mo, "TOUCHY")
                && !has_flag(w, mo, "VULNERABLE")
            {
                continue;
            }
        }
        if dist2d(w, id, mo) > radius {
            continue;
        }
        if flags & 4 != 0 && !check_sight(w, id, mo) {
            continue;
        }
        blast_actor(w, id, mo, strength, speed, &effect, flags & 16 != 0);
    }
    ActionResult::Continue
}

/// GZDoom `BlastActor`.
fn blast_actor(
    w: &mut World,
    src: ActorId,
    victim: ActorId,
    strength: f64,
    speed: f64,
    effect: &str,
    no_impact: bool,
) {
    if !has_flag(w, victim, "MISSILE")
        && (has_flag(w, victim, "DONTBLAST") || has_flag(w, victim, "BOSS"))
    {
        return;
    }
    let ang = angle_to(w, src, victim);
    let (vx, vy) = angle_vec(ang, speed);
    let v = vel(w, victim);
    set_vel(w, victim, DVec3::new(vx, vy, v.z()));
    // Spawn the effect between the two at the victim's edge.
    let p = pos(w, victim);
    let r = w.actors.get(victim).map_or(0.0, |a| a.radius);
    let (ox, oy) = angle_vec(ang + 180.0, r);
    if let Some(fx) = spawn(w, effect, DVec3::new(p.x() + ox, p.y() + oy, p.z())) {
        set_vel(w, fx, DVec3::new(vx / 2.0, vy / 2.0, 0.0));
    }
    if has_flag(w, victim, "MISSILE") {
        // Reflect missiles back at the caster's side.
        if has_flag(w, victim, "MBFBOUNCER") || has_flag(w, victim, "BOUNCEONACTORS") {
            return;
        }
        set_angle(w, victim, ang);
        if let Some(a) = w.actors.get_mut(victim) {
            a.target = Some(src);
        }
        let _ = strength;
    } else {
        set_vel_z(
            w,
            victim,
            if strength < 255.0 {
                strength / 255.0 * 8.0
            } else {
                8.0
            },
        );
        if !no_impact {
            set_flag(w, victim, "BLASTED", true);
        }
    }
}

/// `A_SkullPop([skulltype])`: the player's head flies off as a `BloodySkull`.
fn skull_pop(w: &mut World, id: ActorId, args: &[String]) -> ActionResult {
    let ty = sarg(args, 0, "BloodySkull");
    if let Some(a) = w.actors.get_mut(id) {
        a.flags &= !crate::class::FLAG_SOLID;
    }
    let p = pos(w, id);
    let Some(mo) = spawn(w, &ty, DVec3::new(p.x(), p.y(), p.z() + 48.0)) else {
        return ActionResult::Continue;
    };
    let vx = f64::from(rnd2(w, "pr_skullpop")) / 128.0;
    let vy = f64::from(rnd2(w, "pr_skullpop")) / 128.0;
    let vz = 2.0 + f64::from(rnd(w, "pr_skullpop")) / 1024.0;
    set_vel(w, mo, DVec3::new(vx, vy, vz));
    let (hp, ang) = (health(w, id), angle(w, id));
    if let Some(a) = w.actors.get_mut(mo) {
        a.health = hp;
        a.angle = Angle::from_degrees(ang);
    }
    // The view follows the skull (`player.camera = mo`).
    for p in w.players.iter_mut().flatten() {
        if p.mo == Some(id) {
            p.camera = Some(mo);
            p.damage_count = 32;
        }
    }
    ActionResult::Continue
}

fn cpos_attack(w: &mut World, id: ActorId, snd: &str) -> ActionResult {
    if target(w, id).is_none() {
        return ActionResult::Continue;
    }
    sound(w, id, snd);
    face_target(w, id);
    let a = angle(w, id);
    let slope = aim(w, id, a, MISSILE_RANGE);
    let ang = a + f64::from(rnd2(w, "pr_cposattack")) * (22.5 / 256.0);
    let dmg = rnd_range(w, "pr_cposattack", 1, 5) * 3;
    let _ = line_attack(w, id, ang, MISSILE_RANGE, slope, dmg, "BulletPuff");
    ActionResult::Continue
}

fn spos_attack(w: &mut World, id: ActorId) -> ActionResult {
    if target(w, id).is_none() {
        return ActionResult::Continue;
    }
    face_target(w, id);
    let a = angle(w, id);
    let slope = aim(w, id, a, MISSILE_RANGE);
    for _ in 0..3 {
        let ang = a + f64::from(rnd2(w, "pr_sposattack")) * (22.5 / 256.0);
        let dmg = rnd_range(w, "pr_sposattack", 1, 5) * 3;
        let _ = line_attack(w, id, ang, MISSILE_RANGE, slope, dmg, "BulletPuff");
    }
    ActionResult::Continue
}

/// `ScriptedMarine.GunShot2`: 5–15 damage, ±5.6° spread unless accurate.
fn gun_shot2(w: &mut World, id: ActorId, accurate: bool, slope: f64) {
    let dmg = 5 * rnd_range(w, "pr_smarinegunshot", 1, 3);
    let mut ang = angle(w, id);
    if !accurate {
        ang += f64::from(rnd2(w, "pr_smarinegunshot")) * (5.625 / 256.0);
    }
    let _ = line_attack(w, id, ang, MISSILE_RANGE, slope, dmg, "BulletPuff");
}

/// `special1 = maptime + tics`: the marine waits before firing again.
fn marine_cooldown(w: &mut World, id: ActorId, tics: u32) {
    let until = w.level.maptime + tics;
    if let Some(a) = w.actors.get_mut(id) {
        a.ext.special1 = until as i32;
    }
}

fn marine_cooling(w: &World, id: ActorId) -> bool {
    w.actors
        .get(id)
        .is_some_and(|a| a.ext.special1 != 0 && a.ext.special1 as u32 > w.level.maptime)
}

fn marine_noise(w: &mut World, id: ActorId) {
    // `CurrentWeapon == WEAPON_Chainsaw`: the chainsaw marine's class.
    if class_name(w, id).eq_ignore_ascii_case("MarineChainsaw") {
        sound(w, id, "weapons/sawidle");
    }
}

fn marine_punch(w: &mut World, id: ActorId, mul: i32) -> ActionResult {
    if target(w, id).is_none() {
        return ActionResult::Continue;
    }
    let dmg = (rnd_range(w, "pr_smarinepunch", 1, 10) << 1) * mul;
    face_target(w, id);
    let ang = angle(w, id) + f64::from(rnd2(w, "pr_smarinepunch")) * (5.625 / 256.0);
    let slope = aim(w, id, ang, MELEE_RANGE);
    if let Some(hit) = line_attack(w, id, ang, MELEE_RANGE, slope, dmg, "BulletPuff") {
        sound(w, id, "*fist");
        let a = angle_to(w, id, hit);
        set_angle(w, id, a);
    }
    ActionResult::Continue
}

fn marine_saw(w: &mut World, id: ActorId, args: &[String]) -> ActionResult {
    let full = sarg(args, 0, "weapons/sawfull");
    let hit_snd = sarg(args, 1, "weapons/sawhit");
    let mut dmg = iarg(args, 2, 2);
    if dmg == 0 {
        dmg = 2;
    }
    let puff = sarg(args, 3, "BulletPuff");
    if target(w, id).is_none() {
        return ActionResult::Continue;
    }
    face_target(w, id);
    if !in_melee_range(w, id) {
        sound(w, id, &full);
        return ActionResult::Continue;
    }
    dmg *= rnd_range(w, "pr_smarinesaw", 1, 10);
    let a = angle(w, id);
    let _ = rnd2(w, "pr_smarinesaw");
    let slope = aim(w, id, a, 65.0);
    let Some(hit) = line_attack(w, id, a, 65.0, slope, dmg, &puff) else {
        sound(w, id, &full);
        return ActionResult::Continue;
    };
    sound(w, id, &hit_snd);
    let ang = angle_to(w, id, hit);
    let diff = delta_angle(a, ang);
    let new = if diff < 0.0 {
        if diff < -4.5 {
            ang + 90.0 / 21.0
        } else {
            a - 4.5
        }
    } else if diff > 4.5 {
        ang - 90.0 / 21.0
    } else {
        a + 4.5
    };
    set_angle(w, id, new);
    ActionResult::Continue
}

fn marine_refire(w: &mut World, id: ActorId, args: &[String]) -> ActionResult {
    let ignore_missile = crate::action::parse_bool_arg(args, 0);
    let jumpto = sarg(args, 1, "");
    let next = |w: &mut World| -> ActionResult {
        if !jumpto.is_empty() {
            return jump(w, id, &jumpto);
        }
        let cur = w.actors.get(id).and_then(|a| a.state);
        match cur {
            Some(s) => ActionResult::Jump(StateId::from_raw(s.raw() + 1)),
            None => ActionResult::Continue,
        }
    };
    let class = w.actors.get(id).map(|a| a.class);
    let has_missile = class.is_some_and(|c| w.states.find_label(c, "Missile").is_some());
    let Some(t) = target(w, id).filter(|&t| health(w, t) > 0) else {
        if has_missile && rnd(w, "pr_smarinerefire") < 160 {
            if let ActionResult::Jump(_) = crate::action::a_look(w, id) {
                if target(w, id).is_some() {
                    return ActionResult::Continue;
                }
            }
        }
        return next(w);
    };
    if ((ignore_missile || !has_missile) && !in_melee_range(w, id))
        || !check_sight(w, id, t)
        || rnd(w, "pr_smarinerefire") < 4
    {
        return next(w);
    }
    ActionResult::Continue
}
