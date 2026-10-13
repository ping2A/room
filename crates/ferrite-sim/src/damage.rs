//! Damage and death (`P_DamageMobj` / `AActor::Die` subset).

use crate::actor::ActorId;
use crate::world::World;
use ferrite_core::{DVec3, FRACUNIT, Fixed, math};

/// `MF_SHOOTABLE` bit (matches class registry).
const FLAG_SHOOTABLE: u64 = 1 << 1;

/// Doom `P_DamageMobj` knockback: `thrust = damage*(FRACUNIT>>3)*100/mass`.
fn apply_damage_thrust(
    world: &mut World,
    target: ActorId,
    inflictor: ActorId,
    source: Option<ActorId>,
    damage: i32,
) {
    // Chainsaw does not shove the victim.
    let chainsaw = source.is_some_and(|src| {
        world
            .players
            .iter()
            .flatten()
            .find(|p| p.mo == Some(src))
            .and_then(|p| p.ready_weapon)
            .and_then(|w| world.classes.get(w))
            .is_some_and(|c| {
                let n = c.name.to_ascii_lowercase();
                n.contains("chainsaw")
            })
    });
    if chainsaw {
        return;
    }
    let noclip = world
        .players
        .iter()
        .flatten()
        .any(|p| p.noclip && p.mo == Some(target));
    if noclip {
        return;
    }
    let Some(tgt) = world.actors.get(target).cloned() else {
        return;
    };
    if (tgt.flags & crate::class::FLAG_DONTTHRUST) != 0 {
        return;
    }
    let Some(infl) = world.actors.get(inflictor).cloned() else {
        return;
    };
    let mass = world
        .classes
        .get(tgt.class)
        .map(|c| c.mass.max(1))
        .unwrap_or(100);
    // fixed: damage * 8192 * 100 / mass
    let mut thrust = Fixed(damage.saturating_mul(FRACUNIT >> 3).saturating_mul(100) / mass);
    // Doom `R_PointToAngle2(inflictor, target)` — not libm atan2.
    let ix = Fixed::from_f64(infl.pos.x()).0;
    let iy = Fixed::from_f64(infl.pos.y()).0;
    let tx = Fixed::from_f64(tgt.pos.x()).0;
    let ty = Fixed::from_f64(tgt.pos.y()).0;
    let mut ang_bam = math::point_to_angle2(ix, iy, tx, ty);
    // Occasional forward fall when finishing a high strike from below.
    if damage < 40
        && damage > tgt.health
        && tgt.pos.z() - infl.pos.z() > 64.0
        && (world.p_random("DamageThrust") & 1) != 0
    {
        ang_bam = ang_bam.wrapping_add(0x8000_0000);
        thrust = Fixed(thrust.0.saturating_mul(4));
    }
    let fine = math::bam_to_fine(ang_bam);
    let momx = Fixed::from_f64(tgt.vel.x()) + Fixed(math::finecosine(fine)).mul(thrust);
    let momy = Fixed::from_f64(tgt.vel.y()) + Fixed(math::finesine(fine)).mul(thrust);
    if let Some(a) = world.actors.get_mut(target) {
        a.vel = DVec3::new(momx.to_f64(), momy.to_f64(), a.vel.z());
    }
}

/// Apply damage. Returns damage actually dealt (0 if ignored).
pub fn damage_mobj(
    world: &mut World,
    target: ActorId,
    inflictor: Option<ActorId>,
    source: Option<ActorId>,
    damage: i32,
) -> i32 {
    if damage <= 0 {
        return 0;
    }
    if std::env::var_os("FERRITE_TRACE_HIT").is_some() {
        if let Some(a) = world.actors.get(target) {
            let name = world
                .classes
                .get(a.class)
                .map(|c| c.name.as_str())
                .unwrap_or("?");
            let src = source
                .and_then(|id| world.actors.get(id))
                .and_then(|s| world.classes.get(s.class))
                .map(|c| c.name.as_str())
                .unwrap_or("-");
            eprintln!(
                "HIT@{} {} dmg={damage} hp_before={} at=({:.1},{:.1}) src={src}",
                world.level.time,
                name,
                a.health,
                a.pos.x(),
                a.pos.y()
            );
        }
    }
    // God / invuln on possessing players.
    for slot in world.players.iter().flatten() {
        if (slot.god || slot.invulnerable) && slot.mo == Some(target) {
            return 0;
        }
    }
    // Coop: no player-vs-player damage (deathmatch will opt into friendly fire later).
    if let Some(src) = source {
        if src != target {
            let src_is_player = world.players.iter().flatten().any(|p| p.mo == Some(src));
            let tgt_is_player = world.players.iter().flatten().any(|p| p.mo == Some(target));
            if src_is_player && tgt_is_player {
                return 0;
            }
        }
    }
    let dtype = inflictor.and_then(|id| {
        let a = world.actors.get(id)?;
        if !a.damage_type.is_empty() {
            Some(a.damage_type.clone())
        } else {
            world
                .classes
                .get(a.class)
                .map(|c| c.damage_type.clone())
                .filter(|s| !s.is_empty())
        }
    });
    let factor = world
        .actors
        .get(target)
        .map(|a| {
            if let Some(ref t) = dtype {
                let key = t.to_ascii_lowercase();
                world
                    .classes
                    .get(a.class)
                    .and_then(|c| c.typed_damage_factors.get(&key).copied())
                    .unwrap_or(a.damage_factor)
            } else {
                a.damage_factor
            }
        })
        .unwrap_or(1.0);
    let damage = if (factor - 1.0).abs() < 1e-12 {
        damage
    } else {
        (f64::from(damage) * factor).round() as i32
    };
    if damage <= 0 {
        return 0;
    }
    // Armor absorb: green = ceil(dmg/3), blue = ceil(dmg/2).
    let mut health_damage = damage;
    for slot in world.players.iter_mut().flatten() {
        if slot.mo != Some(target) || slot.armor <= 0 {
            continue;
        }
        let saved = if slot.armor_class >= 2 {
            (damage + 1) / 2 // ceil(damage/2)
        } else {
            (damage + 2) / 3 // ceil(damage/3)
        };
        let take = saved.min(slot.armor);
        slot.armor -= take;
        if slot.armor <= 0 {
            slot.armor_class = 0;
        }
        health_damage = damage - take;
        break;
    }
    let class = world.actors.get(target).map(|a| a.class);
    let Some(class) = class else {
        return 0;
    };
    // `+DONTHARMSPECIES` / `+DONTHARMCLASS`: skip friendly-fire within group.
    if let Some(src) = source {
        if src != target {
            let src_a = world.actors.get(src);
            let src_class = src_a.map(|a| a.class);
            let src_flags = src_a.map(|a| a.flags).unwrap_or(0);
            let tgt_flags = world.actors.get(target).map(|a| a.flags).unwrap_or(0);
            if ((src_flags | tgt_flags) & crate::class::FLAG_DONTHARMCLASS) != 0
                && src_class == Some(class)
            {
                return 0;
            }
            let tgt_sp = world
                .actors
                .get(target)
                .map(|a| {
                    if !a.species.is_empty() {
                        a.species.clone()
                    } else {
                        world
                            .classes
                            .get(class)
                            .map(|c| c.species.clone())
                            .unwrap_or_default()
                    }
                })
                .unwrap_or_default();
            let src_sp = src_a
                .map(|a| {
                    if !a.species.is_empty() {
                        a.species.clone()
                    } else {
                        src_class
                            .and_then(|c| world.classes.get(c))
                            .map(|c| c.species.clone())
                            .unwrap_or_default()
                    }
                })
                .unwrap_or_default();
            if ((src_flags | tgt_flags) & crate::class::FLAG_DONTHARMSPECIES) != 0
                && !tgt_sp.is_empty()
                && tgt_sp.eq_ignore_ascii_case(&src_sp)
            {
                return 0;
            }
            let tgt_team = world
                .classes
                .get(class)
                .map(|c| c.designated_team)
                .unwrap_or(255);
            let src_team = src_class
                .and_then(|c| world.classes.get(c))
                .map(|c| c.designated_team)
                .unwrap_or(255);
            if tgt_team != 255 && tgt_team == src_team {
                return 0;
            }
            // MBF21 infighting groups: same group (≥0) do not damage each other.
            let tgt_ig = world
                .classes
                .get(class)
                .map(|c| c.infighting_group)
                .unwrap_or(-1);
            let src_ig = src_class
                .and_then(|c| world.classes.get(c))
                .map(|c| c.infighting_group)
                .unwrap_or(-1);
            if tgt_ig >= 0 && tgt_ig == src_ig {
                return 0;
            }
            // MBF21 projectile groups: projectile source in same group as target → no damage.
            let src_is_missile = src_a
                .map(|a| (a.flags & crate::class::FLAG_MISSILE) != 0)
                .unwrap_or(false);
            if src_is_missile {
                let src_pg = src_class
                    .and_then(|c| world.classes.get(c))
                    .map(|c| c.projectile_group)
                    .unwrap_or(-1);
                let tgt_pg = world
                    .classes
                    .get(class)
                    .map(|c| c.projectile_group)
                    .unwrap_or(-1);
                if src_pg >= 0 && src_pg == tgt_pg {
                    return 0;
                }
            }
        }
    }
    let def_th = world
        .classes
        .get(class)
        .map(|c| c.def_threshold)
        .unwrap_or(100);
    {
        let Some(actor) = world.actors.get(target) else {
            return 0;
        };
        if actor.destroyed || (actor.flags & FLAG_SHOOTABLE) == 0 {
            return 0;
        }
        if (actor.flags & (crate::class::FLAG_NODAMAGE | crate::class::FLAG_INVULNERABLE)) != 0 {
            return 0;
        }
    }
    // Doom applies knockback before armor/health (chainsaw exempt).
    if let Some(infl) = inflictor {
        apply_damage_thrust(world, target, infl, source, damage);
    }
    let source_class = source
        .and_then(|id| world.actors.get(id))
        .and_then(|a| world.classes.get(a.class))
        .map(|c| c.name.clone())
        .unwrap_or_default();
    let inflictor_class = inflictor
        .and_then(|id| world.actors.get(id))
        .and_then(|a| world.classes.get(a.class))
        .map(|c| c.name.clone())
        .unwrap_or_default();
    let is_player_pawn = world
        .players
        .iter()
        .flatten()
        .any(|p| p.mo == Some(target));
    let health = {
        let Some(actor) = world.actors.get_mut(target) else {
            return 0;
        };
        actor.health = actor.health.saturating_sub(health_damage);
        // Wake: cancel ReactionTime so chase can start immediately after See/Pain.
        actor.reaction_time = 0;
        actor.flags &= !crate::class::FLAG_DORMANT;
        // Infighting: hold current target for `DefThreshold` tics.
        actor.threshold = def_th;
        if let Some(src) = source {
            if src != target {
                actor.target = Some(src);
                // Doom `MF_JUSTHIT` — fight back on next `P_CheckMissileRange`.
                actor.flags |= crate::class::FLAG_JUSTHIT;
            }
        }
        actor.health
    };
    if is_player_pawn && health_damage > 0 {
        world.last_damage = Some(crate::world::LastDamageInfo {
            tic: world.level.time,
            dealt: health_damage,
            raw: damage,
            target: Some(target),
            source,
            inflictor,
            source_class,
            inflictor_class,
            cause: String::new(),
        });
    }
    if health <= 0 {
        die(world, target, class, source, dtype.as_deref());
        return health_damage;
    }
    let wound_health = world
        .classes
        .get(class)
        .map(|c| c.wound_health)
        .unwrap_or(0);
    if wound_health > 0 && health < wound_health {
        if let Some(wound) = world.states.find_label(class, "Wound") {
            world.set_state(target, Some(wound), true);
            return health_damage;
        }
    }
    let pain_threshold = world
        .actors
        .get(target)
        .map(|a| a.pain_threshold)
        .or_else(|| world.classes.get(class).map(|c| c.pain_threshold))
        .unwrap_or(0);
    if pain_threshold > 0 && health > pain_threshold {
        return health_damage;
    }
    let pain_chance = world
        .classes
        .get(class)
        .map(|c| {
            if let Some(ref t) = dtype {
                let key = t.to_ascii_lowercase();
                c.typed_pain_chances
                    .get(&key)
                    .copied()
                    .unwrap_or(c.pain_chance)
            } else {
                c.pain_chance
            }
        })
        .unwrap_or(0);
    if pain_chance > 0 {
        let roll = world.p_random("pr_pain");
        if i32::from(roll) < pain_chance {
            let pain = dtype
                .as_deref()
                .and_then(|dt| {
                    let name = format!("Pain.{dt}");
                    world.states.find_label(class, &name)
                })
                .or_else(|| world.states.find_label(class, "Pain"));
            if let Some(pain) = pain {
                world.set_state(target, Some(pain), true);
            }
        }
    }
    health_damage
}

/// Enter Death / XDeath (or destroy if neither label).
pub(crate) fn die(
    world: &mut World,
    target: ActorId,
    class: ferrite_core::ClassId,
    source: Option<ActorId>,
    damage_type: Option<&str>,
) {
    let (countkill, health) = world
        .actors
        .get(target)
        .map(|a| ((a.flags & crate::class::FLAG_COUNTKILL) != 0, a.health))
        .unwrap_or((false, 0));
    if countkill {
        world.level.found_kills = world.level.found_kills.saturating_add(1);
    }
    // Player death: mark Dead (blocks `player_think` input), show killer obituary.
    let is_player = world.players.iter().flatten().any(|p| p.mo == Some(target));
    let mut drop_player_weapon = None;
    if is_player {
        for slot in world.players.iter_mut() {
            let Some(p) = slot.as_mut() else {
                continue;
            };
            if p.mo != Some(target) {
                continue;
            }
            drop_player_weapon = p.ready_weapon;
            p.player_state = crate::player::PlayerState::Dead;
            p.pending_weapon = None;
            p.pspr_state = None;
            p.pspr_tics = 0;
            p.pspr_flash_state = None;
            p.pspr_flash_tics = 0;
            p.extralight = 0;
            p.view_height = 6.0; // Doom dead-view eye height
            if let Some(a) = world.actors.get(target) {
                p.view_z = a.pos.z() + p.view_height;
            }
            break;
        }
        crate::acs::start_death_scripts(world, target);
        if let Some(src) = source {
            if let Some(src_a) = world.actors.get(src) {
                let def = world.classes.get(src_a.class);
                let msg = def
                    .map(|c| {
                        if !c.obituary.is_empty() {
                            c.obituary.clone()
                        } else {
                            c.hit_obituary.clone()
                        }
                    })
                    .unwrap_or_default();
                if !msg.is_empty() {
                    world.push_message(msg);
                }
            }
        }
        spawn_dropped_weapon(world, target, drop_player_weapon);
    }
    // Gib: remaining health below `GibHealth` (default `-Health`) → XDeath.
    let gib_thresh = world
        .classes
        .get(class)
        .map(|c| {
            if c.gib_health != 0 {
                c.gib_health
            } else {
                -c.health
            }
        })
        .unwrap_or(-1000);
    let use_xdeath = health < gib_thresh;
    let typed = |base: &str| {
        damage_type.and_then(|dt| {
            let name = format!("{base}.{dt}");
            world.states.find_label(class, &name)
        })
    };
    let label = if use_xdeath {
        typed("XDeath")
            .or_else(|| typed("Death"))
            .or_else(|| world.states.find_label(class, "XDeath"))
            .or_else(|| world.states.find_label(class, "Death"))
    } else {
        typed("Death").or_else(|| world.states.find_label(class, "Death"))
    };
    // Doom `P_KillMobj`: clear shootable immediately so later pellets in the same
    // shotgun blast cannot keep damaging / re-entering death. Keep `SOLID` until
    // `A_Fall` / `A_NoBlocking` in the death sequence (DEMO1@891 corpse block).
    // Set `MF_CORPSE|MF_DROPOFF` so XY friction can skip on ledge steps.
    if let Some(a) = world.actors.get_mut(target) {
        a.flags &= !FLAG_SHOOTABLE;
        a.flags |= crate::class::FLAG_DROPOFF | crate::class::FLAG_CORPSE;
    }
    if let Some(death) = label {
        world.set_state(target, Some(death), true);
    } else if let Some(a) = world.actors.get_mut(target) {
        a.health = 0;
        a.destroyed = true;
        a.state = None;
    }
    // Doom: `target->tics -= P_Random()&3` (min 1) after death state.
    // DropItem spawn must come *after* this burn — Clip `P_SpawnMobj` lastlook
    // would otherwise steal the trim byte (DEMO1@456 death-frame desync).
    let trim = i32::from(world.p_random("pr_killmobj") & 3);
    if let Some(a) = world.actors.get_mut(target) {
        a.tics = (a.tics - trim).max(1);
    }
    // Doom drops MT_CLIP / shotgun / chaingun after the death-tic trim.
    spawn_drop_items(world, target, class);
    // Doom `P_KillMobj`: `height >>= 2` immediately so missiles can skim corpses
    // while `SOLID` remains until `A_Fall`. `DeathHeight` / `BurnHeight` override.
    let living_h = world.actors.get(target).map(|a| a.height).unwrap_or(0.0);
    let corpse_h = world.classes.get(class).map(|c| {
        let fire = damage_type.is_some_and(|t| t.eq_ignore_ascii_case("Fire"));
        if fire && c.burn_height > 0.0 {
            c.burn_height
        } else if c.death_height > 0.0 {
            c.death_height
        } else {
            living_h * 0.25
        }
    });
    if let Some(h) = corpse_h {
        if let Some(a) = world.actors.get_mut(target) {
            a.height = h;
        }
    }
    // Player bodies → bodyque; COUNTKILL monsters → corpse queue (GZDoom default).
    if is_player {
        world.queue_body(target);
    } else if countkill {
        world.queue_corpse(target);
    }
}

/// Vanilla `P_DropWeapon`: spawn the ready weapon pickup (not Fist/Pistol).
fn spawn_dropped_weapon(world: &mut World, target: ActorId, weapon: Option<ferrite_core::ClassId>) {
    let Some(wpn) = weapon else {
        return;
    };
    let skip = world.classes.get(wpn).is_some_and(|c| {
        c.name.eq_ignore_ascii_case("Fist") || c.name.eq_ignore_ascii_case("Pistol")
    });
    if skip {
        return;
    }
    let Some(src) = world.actors.get(target).cloned() else {
        return;
    };
    if let Ok(id) = world.spawn_at(wpn, src.pos, src.angle) {
        if let Some(a) = world.actors.get_mut(id) {
            a.flags |= crate::class::FLAG_DROPPED;
        }
    }
}

/// Spawn `DropItem` classes at the dying actor's feet.
fn spawn_drop_items(world: &mut World, target: ActorId, class: ferrite_core::ClassId) {
    let drops: Vec<String> = world
        .classes
        .get(class)
        .map(|c| c.drop_items.clone())
        .unwrap_or_default();
    if drops.is_empty() {
        return;
    }
    let Some(src) = world.actors.get(target).cloned() else {
        return;
    };
    for name in drops {
        let Some(drop_class) = world.classes.find_name(&name) else {
            continue;
        };
        if let Ok(id) = world.spawn_at(drop_class, src.pos, src.angle) {
            if let Some(a) = world.actors.get_mut(id) {
                a.flags |= crate::class::FLAG_DROPPED;
            }
            if std::env::var_os("FERRITE_TRACE_AMMO").is_some() {
                eprintln!(
                    "DROPSPAWN tic={} item={} at=({:.1},{:.1},{:.1}) from={}",
                    world.level.time,
                    name,
                    src.pos.x(),
                    src.pos.y(),
                    src.pos.z(),
                    world
                        .classes
                        .get(class)
                        .map(|c| c.name.as_str())
                        .unwrap_or("?")
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use crate::{ClassRegistry, StateTable, World};
    use ferrite_core::DVec3;
    use ferrite_decorate::parse_decorate;
    use ferrite_nodebuild::apply_nodes;
    use ferrite_testkit::{MapBuilder, rect};
    use std::sync::Arc;

    #[test]
    fn damage_kills_and_enters_death() {
        let ir = parse_decorate(
            r#"
actor ZombieMan : Actor 3004
{
  Health 20
  +SHOOTABLE
  States
  {
  Spawn:
    POSS A -1
    Stop
  Death:
    POSS H -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, classes.clone(), states).unwrap();
        let zid = classes.find_doomednum(3004).unwrap();
        let id = w
            .spawn_at(zid, DVec3::new(64.0, 64.0, 0.0), Default::default())
            .unwrap();
        assert_eq!(damage_mobj(&mut w, id, None, None, 20), 20);
        let a = w.actors.get(id).unwrap();
        assert!(a.health <= 0);
        assert_eq!(a.sprite, "POSS");
        assert_eq!(a.frame, b'H' - b'A');
    }

    #[test]
    fn god_mode_blocks_player_damage() {
        let ir = parse_decorate(
            r#"
actor PlayerPawn : Actor 0
{
  Health 100
  +SHOOTABLE
  States { Spawn: PLAY A -1 Stop }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, classes, states).unwrap();
        let mo = w.players[0].as_ref().and_then(|p| p.mo).unwrap();
        if let Some(p) = w.players[0].as_mut() {
            p.god = true;
        }
        assert_eq!(damage_mobj(&mut w, mo, None, None, 50), 0);
        assert_eq!(w.actors.get(mo).unwrap().health, 100);
    }

    #[test]
    fn player_use_while_dead_respawns_at_start() {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let file = ferrite_decorate::parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&file).unwrap());
        let states = Arc::new(StateTable::from_decorate(&file, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let mo = w.players[0].as_ref().and_then(|p| p.mo).unwrap();
        // Move away then kill.
        if let Some(a) = w.actors.get_mut(mo) {
            a.pos = DVec3::new(200.0, 200.0, 0.0);
        }
        let _ = damage_mobj(&mut w, mo, None, None, 999);
        assert_eq!(
            w.players[0].as_ref().unwrap().player_state,
            crate::player::PlayerState::Dead
        );
        let mut player = w.players[0].take().unwrap();
        player.cmd.buttons = crate::player::Buttons(crate::player::Buttons::USE);
        crate::think_player::player_think(&mut w, &mut player);
        assert_eq!(player.player_state, crate::player::PlayerState::Live);
        assert_eq!(w.actors.get(mo).unwrap().health, 100);
        assert!((w.actors.get(mo).unwrap().pos.x() - 64.0).abs() < 1e-6);
        assert!((w.actors.get(mo).unwrap().pos.y() - 64.0).abs() < 1e-6);
        w.players[0] = Some(player);
    }

    #[test]
    fn player_death_marks_dead_and_blocks_think() {
        let ir = parse_decorate(
            r#"
actor PlayerPawn : Actor 0
{
  Health 100
  DeathSound "pl_death"
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  Death:
    PLAY H 10 A_PlayerScream
    PLAY N -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, classes, states).unwrap();
        let mo = w.players[0].as_ref().and_then(|p| p.mo).unwrap();
        let start = w.actors.get(mo).unwrap().pos;
        let _ = damage_mobj(&mut w, mo, None, None, 999);
        assert_eq!(
            w.players[0].as_ref().unwrap().player_state,
            crate::player::PlayerState::Dead
        );
        assert_eq!(w.players[0].as_ref().unwrap().view_height, 6.0);
        // Dead players ignore movement commands.
        let mut player = w.players[0].take().unwrap();
        player.cmd.forward_move = 50 * 256;
        crate::think_player::player_think(&mut w, &mut player);
        w.players[0] = Some(player);
        let after = w.actors.get(mo).unwrap().pos;
        assert_eq!(after.x(), start.x());
        assert_eq!(after.y(), start.y());
    }

    #[test]
    fn player_death_drops_ready_weapon() {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let file = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&file).unwrap());
        let states = Arc::new(StateTable::from_decorate(&file, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let mo = w.players[0].as_ref().and_then(|p| p.mo).unwrap();
        let shotgun = classes.find_name("Shotgun").unwrap();
        let _ = crate::inventory::give_inventory(&mut w, mo, shotgun, 1);
        if let Some(p) = w.players[0].as_mut() {
            p.ready_weapon = Some(shotgun);
        }
        let before = w
            .thinkers
            .default
            .iter()
            .filter(|&&id| w.actors.get(id).is_some_and(|a| a.class == shotgun))
            .count();
        let _ = damage_mobj(&mut w, mo, None, None, 999);
        let after = w
            .thinkers
            .default
            .iter()
            .filter(|&&id| {
                w.actors
                    .get(id)
                    .is_some_and(|a| a.class == shotgun && !a.destroyed)
            })
            .count();
        assert!(after > before, "dead player should drop Shotgun pickup");
    }

    #[test]
    fn player_death_plays_death_sound() {
        let ir = parse_decorate(
            r#"
actor PlayerPawn : Actor 0
{
  Health 100
  DeathSound "pl_death"
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  Death:
    PLAY H 10 A_PlayerScream
    PLAY I 10 A_NoBlocking
    PLAY N -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, classes, states).unwrap();
        let mo = w.players[0].as_ref().and_then(|p| p.mo).unwrap();
        let class = w.actors.get(mo).unwrap().class;
        let death = w.states.find_label(class, "Death").unwrap();
        // Kill without extreme overkill so A_PlayerScream keeps sfx_pldeth.
        let _ = damage_mobj(&mut w, mo, None, None, 120);
        assert_eq!(w.actors.get(mo).unwrap().state, Some(death));
        assert!(
            w.sound_events.iter().any(|e| e.name == "pl_death"),
            "expected pl_death from A_PlayerScream, events={:?}",
            w.sound_events
        );
    }

    #[test]
    fn player_extreme_death_plays_xdeath_scream() {
        let ir = parse_decorate(
            r#"
actor PlayerPawn : Actor 0
{
  Health 100
  DeathSound "pl_death"
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  Death:
    PLAY H 10 A_PlayerScream
    PLAY N -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, classes, states).unwrap();
        let mo = w.players[0].as_ref().and_then(|p| p.mo).unwrap();
        // health 100 − 151 = −51 → vanilla A_PlayerScream uses sfx_pdiehi.
        let _ = damage_mobj(&mut w, mo, None, None, 151);
        assert!(
            w.sound_events.iter().any(|e| e.name == "pl_xdeath"),
            "expected pl_xdeath from extreme A_PlayerScream, events={:?}",
            w.sound_events
        );
    }

    #[test]
    fn player_pain_plays_pain_sound() {
        let ir = parse_decorate(
            r#"
actor PlayerPawn : Actor 0
{
  Health 100
  PainChance 256
  PainSound "pl_pain"
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  Pain:
    PLAY G 4 A_Pain
    PLAY G 4
    Goto Spawn
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, classes, states).unwrap();
        let mo = w.players[0].as_ref().and_then(|p| p.mo).unwrap();
        let class = w.actors.get(mo).unwrap().class;
        let pain = w.states.find_label(class, "Pain").unwrap();
        let _ = damage_mobj(&mut w, mo, None, None, 5);
        assert_eq!(w.actors.get(mo).unwrap().state, Some(pain));
        assert!(
            w.sound_events.iter().any(|e| e.name == "pl_pain"),
            "expected pl_pain from A_Pain, events={:?}",
            w.sound_events
        );
    }

    #[test]
    fn death_spawns_dropitem() {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let file = ferrite_decorate::parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&file).unwrap());
        let states = Arc::new(StateTable::from_decorate(&file, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let zombie = classes.find_name("ZombieMan").unwrap();
        let clip = classes.find_name("Clip").unwrap();
        assert!(
            classes
                .get(zombie)
                .unwrap()
                .drop_items
                .iter()
                .any(|d| d.eq_ignore_ascii_case("Clip"))
        );
        let id = w
            .spawn_at(zombie, DVec3::new(100.0, 64.0, 0.0), Default::default())
            .unwrap();
        let before = w
            .thinkers
            .default
            .iter()
            .filter(|&&aid| w.actors.get(aid).is_some_and(|a| a.class == clip))
            .count();
        let _ = damage_mobj(&mut w, id, None, None, 999);
        let after = w
            .thinkers
            .default
            .iter()
            .filter(|&&aid| w.actors.get(aid).is_some_and(|a| a.class == clip))
            .count();
        assert_eq!(after, before + 1);
        let dropped = w.thinkers.default.iter().find_map(|&aid| {
            w.actors.get(aid).and_then(|a| {
                (a.class == clip && !a.destroyed).then_some(a.flags & crate::class::FLAG_DROPPED)
            })
        });
        assert_eq!(
            dropped,
            Some(crate::class::FLAG_DROPPED),
            "DropItem should set MF_DROPPED"
        );
    }

    #[test]
    fn wolfenstein_ss_drops_clip() {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let file = ferrite_decorate::parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&file).unwrap());
        let states = Arc::new(StateTable::from_decorate(&file, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let ss = classes.find_name("WolfensteinSS").unwrap();
        let clip = classes.find_name("Clip").unwrap();
        assert!(
            classes
                .get(ss)
                .unwrap()
                .drop_items
                .iter()
                .any(|d| d.eq_ignore_ascii_case("Clip"))
        );
        let id = w
            .spawn_at(ss, DVec3::new(100.0, 64.0, 0.0), Default::default())
            .unwrap();
        let before = w
            .thinkers
            .default
            .iter()
            .filter(|&&aid| w.actors.get(aid).is_some_and(|a| a.class == clip))
            .count();
        let _ = damage_mobj(&mut w, id, None, None, 999);
        let drop = w.thinkers.default.iter().find_map(|&aid| {
            w.actors.get(aid).and_then(|a| {
                (a.class == clip && !a.destroyed && (a.flags & crate::class::FLAG_DROPPED) != 0)
                    .then_some(aid)
            })
        });
        assert!(drop.is_some(), "WolfensteinSS should drop a DROPPED Clip");
        assert_eq!(
            w.thinkers
                .default
                .iter()
                .filter(|&&aid| w.actors.get(aid).is_some_and(|a| a.class == clip))
                .count(),
            before + 1
        );
    }

    #[test]
    fn shotgun_guy_drop_gives_half_shells() {
        use crate::count_inventory;
        use crate::pickup::try_pickup;

        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let file = ferrite_decorate::parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&file).unwrap());
        let states = Arc::new(StateTable::from_decorate(&file, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let guy = classes.find_name("ShotgunGuy").unwrap();
        let shotgun = classes.find_name("Shotgun").unwrap();
        let shell = classes.find_name("Shell").unwrap();
        let ammo_give = classes.get(shotgun).unwrap().ammo_give;
        assert_eq!(ammo_give, 8);
        let id = w
            .spawn_at(guy, DVec3::new(100.0, 64.0, 0.0), Default::default())
            .unwrap();
        let _ = damage_mobj(&mut w, id, None, None, 999);
        let drop = w.thinkers.default.iter().find_map(|&aid| {
            w.actors.get(aid).and_then(|a| {
                (a.class == shotgun && !a.destroyed && (a.flags & crate::class::FLAG_DROPPED) != 0)
                    .then_some(aid)
            })
        });
        assert!(drop.is_some(), "ShotgunGuy should drop a DROPPED Shotgun");
        let drop = drop.unwrap();
        let player = w.players[0].as_ref().unwrap().mo.unwrap();
        assert_eq!(count_inventory(&w, player, shotgun), 0);
        let shells_before = count_inventory(&w, player, shell);
        assert!(try_pickup(&mut w, player, drop));
        assert_eq!(count_inventory(&w, player, shotgun), 1);
        assert_eq!(
            count_inventory(&w, player, shell),
            shells_before + (ammo_give / 2).max(1)
        );
    }

    #[test]
    fn zombie_man_drop_gives_half_clip_ammo() {
        use crate::count_inventory;
        use crate::pickup::try_pickup;

        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let file = ferrite_decorate::parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&file).unwrap());
        let states = Arc::new(StateTable::from_decorate(&file, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let zombie = classes.find_name("ZombieMan").unwrap();
        let clip = classes.find_name("Clip").unwrap();
        let amount = classes.get(clip).unwrap().inventory_amount.max(1);
        assert_eq!(amount, 10);
        let id = w
            .spawn_at(zombie, DVec3::new(100.0, 64.0, 0.0), Default::default())
            .unwrap();
        let _ = damage_mobj(&mut w, id, None, None, 999);
        let drop = w.thinkers.default.iter().find_map(|&aid| {
            w.actors.get(aid).and_then(|a| {
                (a.class == clip && !a.destroyed && (a.flags & crate::class::FLAG_DROPPED) != 0)
                    .then_some(aid)
            })
        });
        assert!(drop.is_some(), "ZombieMan should drop a DROPPED Clip");
        let drop = drop.unwrap();
        let player = w.players[0].as_ref().unwrap().mo.unwrap();
        let before = count_inventory(&w, player, clip);
        assert!(try_pickup(&mut w, player, drop));
        assert_eq!(
            count_inventory(&w, player, clip),
            before + (amount / 2).max(1)
        );
    }

    #[test]
    fn chaingun_guy_drop_gives_half_clips() {
        use crate::count_inventory;
        use crate::pickup::try_pickup;

        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let file = ferrite_decorate::parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&file).unwrap());
        let states = Arc::new(StateTable::from_decorate(&file, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let guy = classes.find_name("ChaingunGuy").unwrap();
        let chaingun = classes.find_name("Chaingun").unwrap();
        let clip = classes.find_name("Clip").unwrap();
        let ammo_give = classes.get(chaingun).unwrap().ammo_give;
        assert_eq!(ammo_give, 20);
        let id = w
            .spawn_at(guy, DVec3::new(100.0, 64.0, 0.0), Default::default())
            .unwrap();
        let _ = damage_mobj(&mut w, id, None, None, 999);
        let drop = w.thinkers.default.iter().find_map(|&aid| {
            w.actors.get(aid).and_then(|a| {
                (a.class == chaingun && !a.destroyed && (a.flags & crate::class::FLAG_DROPPED) != 0)
                    .then_some(aid)
            })
        });
        assert!(drop.is_some(), "ChaingunGuy should drop a DROPPED Chaingun");
        let drop = drop.unwrap();
        let player = w.players[0].as_ref().unwrap().mo.unwrap();
        assert_eq!(count_inventory(&w, player, chaingun), 0);
        let clips_before = count_inventory(&w, player, clip);
        assert!(try_pickup(&mut w, player, drop));
        assert_eq!(count_inventory(&w, player, chaingun), 1);
        assert_eq!(
            count_inventory(&w, player, clip),
            clips_before + (ammo_give / 2).max(1)
        );
    }

    #[test]
    fn player_death_pushes_killer_obituary() {
        let ir = parse_decorate(
            r#"
actor PlayerPawn : Actor 0
{
  Health 100
  +SHOOTABLE
  States { Spawn: PLAY A -1 Stop }
}
actor MeanImp : Actor 9940
{
  Health 60
  Obituary "%o was burned by an imp."
  +SHOOTABLE
  States { Spawn: TROO A -1 Stop }
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
        let imp = classes.find_name("MeanImp").unwrap();
        assert_eq!(
            classes.get(imp).unwrap().obituary,
            "%o was burned by an imp."
        );
        let killer = w
            .spawn_at(imp, DVec3::new(100.0, 64.0, 0.0), Default::default())
            .unwrap();
        let mo = w.players[0].as_ref().and_then(|p| p.mo).unwrap();
        let _ = damage_mobj(&mut w, mo, Some(killer), Some(killer), 999);
        assert!(
            w.drain_messages()
                .iter()
                .any(|m| m == "%o was burned by an imp.")
        );
    }

    #[test]
    fn pain_chance_256_always_enters_pain() {
        let ir = parse_decorate(
            r#"
actor Softie : Actor 9901
{
  Health 100
  PainChance 256
  +SHOOTABLE
  States
  {
  Spawn:
    POSS A -1
    Stop
  Pain:
    POSS B -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let softie = classes.find_name("Softie").unwrap();
        assert_eq!(classes.get(softie).unwrap().pain_chance, 256);
        let id = w
            .spawn_at(softie, DVec3::new(64.0, 64.0, 0.0), Default::default())
            .unwrap();
        let pain = w.states.find_label(softie, "Pain").unwrap();
        let _ = damage_mobj(&mut w, id, None, None, 1);
        assert_eq!(w.actors.get(id).unwrap().state, Some(pain));
    }

    #[test]
    fn nodamage_flag_ignores_hits() {
        let ir = parse_decorate(
            r#"
actor Immortal : Actor 9905
{
  Health 100
  +SHOOTABLE
  +NODAMAGE
  States { Spawn: POSS A -1 Stop }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let immortal = classes.find_name("Immortal").unwrap();
        assert_eq!(
            classes.get(immortal).unwrap().flags & crate::class::FLAG_NODAMAGE,
            crate::class::FLAG_NODAMAGE
        );
        let id = w
            .spawn_at(immortal, DVec3::new(64.0, 64.0, 0.0), Default::default())
            .unwrap();
        assert_eq!(damage_mobj(&mut w, id, None, None, 50), 0);
        assert_eq!(w.actors.get(id).unwrap().health, 100);
    }

    #[test]
    fn invulnerable_flag_ignores_hits() {
        let ir = parse_decorate(
            r#"
actor BossIdol : Actor 9906
{
  Health 100
  +SHOOTABLE
  +INVULNERABLE
  States { Spawn: POSS A -1 Stop }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let idol = classes.find_name("BossIdol").unwrap();
        assert_eq!(
            classes.get(idol).unwrap().flags & crate::class::FLAG_INVULNERABLE,
            crate::class::FLAG_INVULNERABLE
        );
        let id = w
            .spawn_at(idol, DVec3::new(64.0, 64.0, 0.0), Default::default())
            .unwrap();
        assert_eq!(damage_mobj(&mut w, id, None, None, 50), 0);
        assert_eq!(w.actors.get(id).unwrap().health, 100);
    }

    #[test]
    fn typed_death_state_uses_damage_type_label() {
        let ir = parse_decorate(
            r#"
actor Burnable : Actor 9911
{
  Health 20
  +SHOOTABLE
  States
  {
  Spawn: POSS A -1 Stop
  Death: POSS H 5 Stop
  Death.Fire: POSS G 5 Stop
  }
}
actor Ember : Actor 9912
{
  DamageType "Fire"
  Damage 50
  States { Spawn: BAL1 A -1 Stop }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let burnable = classes.find_name("Burnable").unwrap();
        let ember = classes.find_name("Ember").unwrap();
        let fire_death = w.states.find_label(burnable, "Death.Fire").unwrap();
        let id = w
            .spawn_at(burnable, DVec3::new(64.0, 64.0, 0.0), Default::default())
            .unwrap();
        let bolt = w
            .spawn_at(ember, DVec3::new(80.0, 64.0, 0.0), Default::default())
            .unwrap();
        let _ = damage_mobj(&mut w, id, Some(bolt), Some(bolt), 999);
        assert_eq!(w.actors.get(id).unwrap().state, Some(fire_death));
    }

    #[test]
    fn designated_team_blocks_teammate_damage() {
        let ir = parse_decorate(
            r#"
actor RedBot : Actor 9910
{
  Health 40
  DesignatedTeam 2
  +SHOOTABLE
  States { Spawn: POSS A -1 Stop }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let bot = classes.find_name("RedBot").unwrap();
        let a = w
            .spawn_at(bot, DVec3::new(64.0, 64.0, 0.0), Default::default())
            .unwrap();
        let b = w
            .spawn_at(bot, DVec3::new(80.0, 64.0, 0.0), Default::default())
            .unwrap();
        assert_eq!(damage_mobj(&mut w, b, Some(a), Some(a), 15), 0);
        assert_eq!(w.actors.get(b).unwrap().health, 40);
    }

    #[test]
    fn coop_players_do_not_damage_each_other() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        let mut p2 = map.things[0].clone();
        p2.x = 96;
        p2.type_id = 2;
        map.things.push(p2);
        apply_nodes(&mut map);
        let mut w = World::from_map(map).unwrap();
        assert!(w.spawn_coop_players(2).unwrap() >= 2);
        let mo0 = w.players[0].as_ref().unwrap().mo.unwrap();
        let mo1 = w.players[1].as_ref().unwrap().mo.unwrap();
        let hp1 = w.actors.get(mo1).unwrap().health;
        assert_eq!(damage_mobj(&mut w, mo1, Some(mo0), Some(mo0), 50), 0);
        assert_eq!(w.actors.get(mo1).unwrap().health, hp1);
    }

    #[test]
    fn mbf21_infighting_group_blocks_peer_damage() {
        let ir = parse_decorate(
            r#"
actor PackMember : Actor 9911
{
  Health 40
  InfightingGroup 3
  +SHOOTABLE
  States { Spawn: POSS A -1 Stop }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let cls = classes.find_name("PackMember").unwrap();
        assert_eq!(classes.get(cls).unwrap().infighting_group, 3);
        let a = w
            .spawn_at(cls, DVec3::new(64.0, 64.0, 0.0), Default::default())
            .unwrap();
        let b = w
            .spawn_at(cls, DVec3::new(80.0, 64.0, 0.0), Default::default())
            .unwrap();
        assert_eq!(damage_mobj(&mut w, b, Some(a), Some(a), 15), 0);
        assert_eq!(w.actors.get(b).unwrap().health, 40);
    }

    #[test]
    fn dontharmclass_blocks_same_class_damage() {
        let ir = parse_decorate(
            r#"
actor Clone : Actor 9909
{
  Health 40
  +SHOOTABLE
  +DONTHARMCLASS
  States { Spawn: POSS A -1 Stop }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let clone = classes.find_name("Clone").unwrap();
        assert_eq!(
            classes.get(clone).unwrap().flags & crate::class::FLAG_DONTHARMCLASS,
            crate::class::FLAG_DONTHARMCLASS
        );
        let a = w
            .spawn_at(clone, DVec3::new(64.0, 64.0, 0.0), Default::default())
            .unwrap();
        let b = w
            .spawn_at(clone, DVec3::new(80.0, 64.0, 0.0), Default::default())
            .unwrap();
        assert_eq!(damage_mobj(&mut w, b, Some(a), Some(a), 15), 0);
        assert_eq!(w.actors.get(b).unwrap().health, 40);
    }

    #[test]
    fn dontharmspecies_blocks_same_species_damage() {
        let ir = parse_decorate(
            r#"
actor PackMate : Actor 9907
{
  Health 50
  Species "Pack"
  +SHOOTABLE
  +DONTHARMSPECIES
  States { Spawn: POSS A -1 Stop }
}
actor PackFoe : Actor 9908
{
  Health 50
  Species "Pack"
  +SHOOTABLE
  States { Spawn: POSS A -1 Stop }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let mate = classes.find_name("PackMate").unwrap();
        let foe = classes.find_name("PackFoe").unwrap();
        assert_eq!(
            classes.get(mate).unwrap().flags & crate::class::FLAG_DONTHARMSPECIES,
            crate::class::FLAG_DONTHARMSPECIES
        );
        let a = w
            .spawn_at(mate, DVec3::new(64.0, 64.0, 0.0), Default::default())
            .unwrap();
        let b = w
            .spawn_at(foe, DVec3::new(80.0, 64.0, 0.0), Default::default())
            .unwrap();
        assert_eq!(damage_mobj(&mut w, b, Some(a), Some(a), 20), 0);
        assert_eq!(w.actors.get(b).unwrap().health, 50);
        // Without the flag on either side, damage applies.
        let c = w
            .spawn_at(foe, DVec3::new(96.0, 64.0, 0.0), Default::default())
            .unwrap();
        let d = w
            .spawn_at(foe, DVec3::new(112.0, 64.0, 0.0), Default::default())
            .unwrap();
        assert_eq!(damage_mobj(&mut w, d, Some(c), Some(c), 20), 20);
        assert_eq!(w.actors.get(d).unwrap().health, 30);
    }

    #[test]
    fn death_height_applied_on_die() {
        let ir = parse_decorate(
            r#"
actor Tall : Actor 9906
{
  Health 10
  Height 56
  DeathHeight 7
  +SHOOTABLE
  States
  {
  Spawn: POSS A -1 Stop
  Death: POSS H 5 Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let tall = classes.find_name("Tall").unwrap();
        let id = w
            .spawn_at(tall, DVec3::new(64.0, 64.0, 0.0), Default::default())
            .unwrap();
        assert!((w.actors.get(id).unwrap().height - 56.0).abs() < 1e-9);
        let _ = damage_mobj(&mut w, id, None, None, 999);
        assert!((w.actors.get(id).unwrap().height - 7.0).abs() < 1e-9);
    }

    #[test]
    fn corpse_queue_evicts_oldest_when_full() {
        let ir = parse_decorate(
            r#"
actor Grunt : Actor 3004
{
  Health 1
  +SHOOTABLE
  +COUNTKILL
  States
  {
  Spawn: POSS A -1 Stop
  Death: POSS H -1 Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 512.0, 512.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        w.corpse_queue_size = 2;
        let grunt = classes.find_name("Grunt").unwrap();
        let mut ids = Vec::new();
        for i in 0..3 {
            let id = w
                .spawn_at(
                    grunt,
                    DVec3::new(64.0 + f64::from(i) * 32.0, 64.0, 0.0),
                    Default::default(),
                )
                .unwrap();
            let _ = damage_mobj(&mut w, id, None, None, 999);
            ids.push(id);
        }
        assert_eq!(w.corpse_queue.len(), 2);
        assert!(w.actors.get(ids[0]).unwrap().destroyed);
        assert!(!w.actors.get(ids[1]).unwrap().destroyed);
        assert!(!w.actors.get(ids[2]).unwrap().destroyed);
        assert_eq!(w.corpse_queue[0], ids[1]);
        assert_eq!(w.corpse_queue[1], ids[2]);
    }

    #[test]
    fn wound_health_enters_wound_state() {
        let ir = parse_decorate(
            r#"
actor Wounded : Actor 9905
{
  Health 100
  WoundHealth 40
  PainChance 256
  +SHOOTABLE
  States
  {
  Spawn:
    POSS A -1
    Stop
  Wound:
    POSS C -1
    Stop
  Pain:
    POSS B -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let wounded = classes.find_name("Wounded").unwrap();
        assert_eq!(classes.get(wounded).unwrap().wound_health, 40);
        let id = w
            .spawn_at(wounded, DVec3::new(64.0, 64.0, 0.0), Default::default())
            .unwrap();
        let wound = w.states.find_label(wounded, "Wound").unwrap();
        // 100 → 50, still >= 40 → no Wound (Pain may fire).
        let _ = damage_mobj(&mut w, id, None, None, 50);
        assert_ne!(w.actors.get(id).unwrap().state, Some(wound));
        // Drop to 30 (< 40) → Wound (takes priority over Pain).
        let _ = damage_mobj(&mut w, id, None, None, 20);
        assert_eq!(w.actors.get(id).unwrap().state, Some(wound));
        assert_eq!(w.actors.get(id).unwrap().health, 30);
    }

    #[test]
    fn pain_threshold_skips_pain_while_healthy() {
        let ir = parse_decorate(
            r#"
actor Tank : Actor 9904
{
  Health 100
  PainChance 256
  PainThreshold 50
  +SHOOTABLE
  States
  {
  Spawn:
    POSS A -1
    Stop
  Pain:
    POSS B -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let tank = classes.find_name("Tank").unwrap();
        assert_eq!(classes.get(tank).unwrap().pain_threshold, 50);
        let id = w
            .spawn_at(tank, DVec3::new(64.0, 64.0, 0.0), Default::default())
            .unwrap();
        let spawn = w.actors.get(id).unwrap().state;
        let pain = w.states.find_label(tank, "Pain").unwrap();
        // 100 → 90, still above 50 → no Pain.
        let _ = damage_mobj(&mut w, id, None, None, 10);
        assert_eq!(w.actors.get(id).unwrap().state, spawn);
        // Drop to 40 (below threshold) → Pain.
        let _ = damage_mobj(&mut w, id, None, None, 50);
        assert_eq!(w.actors.get(id).unwrap().state, Some(pain));
    }

    #[test]
    fn typed_pain_state_uses_damage_type_label() {
        let ir = parse_decorate(
            r#"
actor Softie : Actor 9913
{
  Health 100
  PainChance 256
  +SHOOTABLE
  States
  {
  Spawn: POSS A -1 Stop
  Pain: POSS B -1 Stop
  Pain.Fire: POSS C -1 Stop
  }
}
actor Ember : Actor 9914
{
  DamageType "Fire"
  States { Spawn: BAL1 A -1 Stop }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let softie = classes.find_name("Softie").unwrap();
        let ember = classes.find_name("Ember").unwrap();
        let fire_pain = w.states.find_label(softie, "Pain.Fire").unwrap();
        let id = w
            .spawn_at(softie, DVec3::new(64.0, 64.0, 0.0), Default::default())
            .unwrap();
        let bolt = w
            .spawn_at(ember, DVec3::new(80.0, 64.0, 0.0), Default::default())
            .unwrap();
        let _ = damage_mobj(&mut w, id, Some(bolt), Some(bolt), 1);
        assert_eq!(w.actors.get(id).unwrap().state, Some(fire_pain));
    }

    #[test]
    fn typed_pain_chance_uses_inflictor_damage_type() {
        let ir = parse_decorate(
            r#"
actor Stoic : Actor 9902
{
  Health 100
  PainChance 0
  PainChance "Fire", 256
  +SHOOTABLE
  States
  {
  Spawn:
    POSS A -1
    Stop
  Pain:
    POSS B -1
    Stop
  }
}
actor Ember : Actor 9903
{
  DamageType "Fire"
  States { Spawn: BAL1 A -1 Stop }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let stoic = classes.find_name("Stoic").unwrap();
        let ember = classes.find_name("Ember").unwrap();
        assert_eq!(classes.get(stoic).unwrap().pain_chance, 0);
        assert_eq!(
            classes
                .get(stoic)
                .unwrap()
                .typed_pain_chances
                .get("fire")
                .copied(),
            Some(256)
        );
        let id = w
            .spawn_at(stoic, DVec3::new(64.0, 64.0, 0.0), Default::default())
            .unwrap();
        let bolt = w
            .spawn_at(ember, DVec3::new(80.0, 64.0, 0.0), Default::default())
            .unwrap();
        let spawn = w.actors.get(id).unwrap().state;
        // Untyped: PainChance 0 → stay in Spawn.
        let _ = damage_mobj(&mut w, id, None, None, 1);
        assert_eq!(w.actors.get(id).unwrap().state, spawn);
        // Fire: typed PainChance 256 → Pain.
        let pain = w.states.find_label(stoic, "Pain").unwrap();
        let _ = damage_mobj(&mut w, id, Some(bolt), Some(bolt), 1);
        assert_eq!(w.actors.get(id).unwrap().state, Some(pain));
    }

    #[test]
    fn damage_factor_scales_incoming() {
        let ir = parse_decorate(
            r#"
actor Tough : Actor 9601
{
  Health 100
  DamageFactor 0.5
  +SHOOTABLE
  States { Spawn: POSS A -1 Stop }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let tough = classes.find_name("Tough").unwrap();
        assert!((classes.get(tough).unwrap().damage_factor - 0.5).abs() < 1e-9);
        let id = w
            .spawn_at(tough, DVec3::new(64.0, 64.0, 0.0), Default::default())
            .unwrap();
        assert_eq!(damage_mobj(&mut w, id, None, None, 40), 20);
        assert_eq!(w.actors.get(id).unwrap().health, 80);
    }

    #[test]
    fn typed_damage_factor_uses_inflictor_damage_type() {
        let ir = parse_decorate(
            r#"
actor Fireproof : Actor 9602
{
  Health 100
  DamageFactor 1.0
  DamageFactor "Fire", 0.0
  +SHOOTABLE
  States { Spawn: POSS A -1 Stop }
}
actor FireBolt : Actor 9603
{
  Damage 40
  DamageType "Fire"
  +MISSILE
  States { Spawn: BAL1 A -1 Stop }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let proof = classes.find_name("Fireproof").unwrap();
        let bolt = classes.find_name("FireBolt").unwrap();
        assert_eq!(classes.get(bolt).unwrap().damage_type, "Fire");
        assert!(
            (classes
                .get(proof)
                .unwrap()
                .typed_damage_factors
                .get("fire")
                .copied()
                .unwrap_or(1.0)
                - 0.0)
                .abs()
                < 1e-9
        );
        let victim = w
            .spawn_at(proof, DVec3::new(64.0, 64.0, 0.0), Default::default())
            .unwrap();
        let missile = w
            .spawn_at(bolt, DVec3::new(80.0, 64.0, 0.0), Default::default())
            .unwrap();
        assert_eq!(
            damage_mobj(&mut w, victim, Some(missile), Some(missile), 40),
            0
        );
        assert_eq!(w.actors.get(victim).unwrap().health, 100);
        // Untyped damage still uses global DamageFactor 1.0.
        assert_eq!(damage_mobj(&mut w, victim, None, None, 10), 10);
        assert_eq!(w.actors.get(victim).unwrap().health, 90);
    }

    #[test]
    fn damage_clears_reaction_time() {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let file = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&file).unwrap());
        let states = Arc::new(StateTable::from_decorate(&file, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let zombie = classes.find_name("ZombieMan").unwrap();
        let id = w
            .spawn_at(zombie, DVec3::new(100.0, 64.0, 0.0), Default::default())
            .unwrap();
        assert!(w.actors.get(id).unwrap().reaction_time > 0);
        let _ = damage_mobj(&mut w, id, None, None, 1);
        assert_eq!(w.actors.get(id).unwrap().reaction_time, 0);
        assert_eq!(
            w.actors.get(id).unwrap().threshold,
            classes.get(zombie).unwrap().def_threshold
        );
    }

    #[test]
    fn gib_health_property_lowers_xdeath_threshold() {
        let ir = parse_decorate(
            r#"
actor Tough : Actor 9980
{
  Health 100
  GibHealth -10
  +SHOOTABLE
  States
  {
  Spawn: POSS A -1 Stop
  Death: POSS H 5 Stop
  XDeath: POSS N 5 Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let tough = classes.find_name("Tough").unwrap();
        let death = w.states.find_label(tough, "Death").unwrap();
        let xdeath = w.states.find_label(tough, "XDeath").unwrap();
        let id = w
            .spawn_at(tough, DVec3::new(100.0, 64.0, 0.0), Default::default())
            .unwrap();
        // health 100 - 105 = -5, which is not < -10 → Death not XDeath.
        let _ = damage_mobj(&mut w, id, None, None, 105);
        assert_eq!(w.actors.get(id).unwrap().state, Some(death));
        let id2 = w
            .spawn_at(tough, DVec3::new(120.0, 64.0, 0.0), Default::default())
            .unwrap();
        let _ = damage_mobj(&mut w, id2, None, None, 120);
        assert_eq!(w.actors.get(id2).unwrap().state, Some(xdeath));
    }

    #[test]
    fn overkill_enters_xdeath() {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let file = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&file).unwrap());
        let states = Arc::new(StateTable::from_decorate(&file, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let zombie = classes.find_name("ZombieMan").unwrap();
        let xdeath = w.states.find_label(zombie, "XDeath").unwrap();
        let id = w
            .spawn_at(zombie, DVec3::new(100.0, 64.0, 0.0), Default::default())
            .unwrap();
        // SpawnHealth 20 → need health < -20 after hit.
        let _ = damage_mobj(&mut w, id, None, None, 999);
        let a = w.actors.get(id).unwrap();
        assert_eq!(a.state, Some(xdeath));
    }

    #[test]
    fn armor_absorbs_third_of_damage() {
        let ir = parse_decorate(
            r#"
actor PlayerPawn : Actor 0
{
  Health 100
  +SHOOTABLE
  States { Spawn: PLAY A -1 Stop }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, classes, states).unwrap();
        let mo = w.players[0].as_ref().and_then(|p| p.mo).unwrap();
        if let Some(p) = w.players[0].as_mut() {
            p.armor = 100;
            p.armor_class = 1;
        }
        // damage 30 → save 10, health −20, armor 90
        assert_eq!(damage_mobj(&mut w, mo, None, None, 30), 20);
        assert_eq!(w.actors.get(mo).unwrap().health, 80);
        assert_eq!(w.players[0].as_ref().unwrap().armor, 90);
    }

    #[test]
    fn blue_armor_absorbs_half_of_damage() {
        let ir = parse_decorate(
            r#"
actor PlayerPawn : Actor 0
{
  Health 100
  +SHOOTABLE
  States { Spawn: PLAY A -1 Stop }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, classes, states).unwrap();
        let mo = w.players[0].as_ref().and_then(|p| p.mo).unwrap();
        if let Some(p) = w.players[0].as_mut() {
            p.armor = 200;
            p.armor_class = 2;
        }
        // damage 30 → save 15, health −15, armor 185
        assert_eq!(damage_mobj(&mut w, mo, None, None, 30), 15);
        assert_eq!(w.actors.get(mo).unwrap().health, 85);
        assert_eq!(w.players[0].as_ref().unwrap().armor, 185);
    }
}
