//! Content-loaded WASM mods with game control (`docs/12`).
//!
//! [`ModRuntime`] owns the play-scope instances for one game session. Between
//! tics the shell lends it the [`World`] (moved into a shared cell, so host
//! imports get full `&mut World` access without aliasing): it dispatches
//! `WorldLoaded` / `WorldTick` / queued sim events, then hands the world back.

use crate::actor::ActorId;
use crate::class::{
    ClassDef, ClassRegistry, FLAG_COUNTKILL, FLAG_FRIENDLY, FLAG_MISSILE, FLAG_SHOOTABLE,
};
use crate::state::StateTable;
use crate::world::World;
use room_core::{Angle, ClassId, DVec3, dmath};
use room_script::game::{
    actor_f64, actor_i32, actor_ptr, args, level_stat, player_prop, rule, spawn_flags,
};
use room_script::{
    GameBridge, GameFn, GuestLifecycle, GuestMemory, HostLinker, ModuleConfig, ModuleScope,
    RegisteredClass, RegisteredEventHandler, ScriptError, SessionKind, WasmBackend, WasmInstance,
    WasmiBackend, WasmiInstance, event_mask,
};
use room_script_abi::{Budget, Val};
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, MutexGuard};

/// Sim-side event queued for mod handlers (drained after each tic).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ScriptEvent {
    /// An actor was spawned.
    ThingSpawned {
        /// New actor.
        thing: ActorId,
    },
    /// An actor took damage (after armor; `health` is what remains).
    ThingDamaged {
        /// Victim.
        thing: ActorId,
        /// Attacker.
        source: Option<ActorId>,
        /// Projectile / puff.
        inflictor: Option<ActorId>,
        /// Health removed.
        damage: i32,
        /// Health after the hit.
        health: i32,
    },
    /// An actor died.
    ThingDied {
        /// Victim.
        thing: ActorId,
        /// Killer.
        source: Option<ActorId>,
    },
    /// A player picked up an item.
    ItemPickup {
        /// Player body.
        toucher: ActorId,
        /// Item class.
        item_class: ClassId,
    },
}

impl ScriptEvent {
    fn mask(&self) -> u64 {
        match self {
            Self::ThingSpawned { .. } => event_mask::WORLD_THING_SPAWNED,
            Self::ThingDamaged { .. } => event_mask::WORLD_THING_DAMAGED,
            Self::ThingDied { .. } => event_mask::WORLD_THING_DIED | event_mask::PLAYER_DIED,
            Self::ItemPickup { .. } => event_mask::WORLD_ITEM_PICKUP,
        }
    }
}

/// Max queued events per tic (runaway spawn loops in a mod).
const MAX_EVENTS_PER_TIC: usize = 4096;
/// Handler-triggered event rounds per tic.
const MAX_EVENT_ROUNDS: usize = 16;
/// Guest scratch buffer for event payloads.
const SCRATCH_BYTES: i32 = 256;

impl World {
    /// Queue `ev` when a loaded mod subscribed to it.
    pub(crate) fn push_script_event(&mut self, ev: ScriptEvent) {
        if self.script_event_mask & ev.mask() != 0 && self.script_events.len() < MAX_EVENTS_PER_TIC
        {
            self.script_events.push(ev);
        }
    }
}

/// `ActorRef` bits (`index | generation << 32`, 0 = null).
#[must_use]
pub fn actor_ref(id: ActorId) -> u64 {
    u64::from(id.index()) | (u64::from(id.generation()) << 32)
}

fn opt_ref(id: Option<ActorId>) -> u64 {
    id.map_or(0, actor_ref)
}

/// Live actor for `ActorRef` bits.
fn live(world: &World, bits: u64) -> Option<ActorId> {
    if bits == 0 {
        return None;
    }
    let id = ActorId::from_raw(bits as u32, (bits >> 32) as u32);
    world.actors.get(id).filter(|a| !a.destroyed).map(|_| id)
}

fn class_ref(id: ClassId) -> i32 {
    i32::try_from(id.raw().saturating_add(1)).unwrap_or(0)
}

fn class_of(world: &World, r: i32) -> Option<ClassId> {
    let raw = u32::try_from(r).ok()?.checked_sub(1)?;
    let id = ClassId::from_raw(raw);
    world.classes.get(id).map(|_| id)
}

fn is_descendant(classes: &ClassRegistry, mut class: ClassId, parent: ClassId) -> bool {
    for _ in 0..64 {
        if class == parent {
            return true;
        }
        match classes.get(class).and_then(|c| c.parent) {
            Some(p) if p != class => class = p,
            _ => return false,
        }
    }
    false
}

fn player_slot(world: &World, id: ActorId) -> Option<usize> {
    world
        .players
        .iter()
        .position(|p| p.as_ref().is_some_and(|p| p.mo == Some(id)))
}

fn write_refs(mem: &mut dyn GuestMemory, out: i32, cap: i32, ids: &[ActorId]) -> i32 {
    let cap = usize::try_from(cap).unwrap_or(0).min(ids.len());
    let mut buf = Vec::with_capacity(cap * 8);
    for &id in &ids[..cap] {
        buf.extend_from_slice(&actor_ref(id).to_le_bytes());
    }
    if !buf.is_empty() {
        let _ = mem.write(out, &buf);
    }
    i32::try_from(ids.len()).unwrap_or(i32::MAX)
}

fn write_vec3(mem: &mut dyn GuestMemory, out: i32, v: DVec3) {
    let mut buf = [0u8; 24];
    buf[..8].copy_from_slice(&v.x().to_le_bytes());
    buf[8..16].copy_from_slice(&v.y().to_le_bytes());
    buf[16..].copy_from_slice(&v.z().to_le_bytes());
    let _ = mem.write(out, &buf);
}

fn i32v(v: i32) -> Option<Val> {
    Some(Val::I32(v))
}

fn bool_v(b: bool) -> Option<Val> {
    i32v(i32::from(b))
}

/// Uniform deterministic integer in `lo..=hi` from the playsim RNG.
fn rng_range(world: &mut World, lo: i32, hi: i32) -> i32 {
    let (lo, hi) = if lo <= hi { (lo, hi) } else { (hi, lo) };
    let span = (i64::from(hi) - i64::from(lo) + 1) as u64;
    let mut r = 0u64;
    let mut bytes = 0;
    while bytes < 4 && (bytes == 0 || (1u64 << (8 * bytes)) < span) {
        r = (r << 8) | u64::from(world.p_random("pr_script"));
        bytes += 1;
    }
    (i64::from(lo) + (r % span) as i64) as i32
}

/// Serve one [`GameFn`] against the lent world.
#[allow(clippy::too_many_lines)]
fn serve(world: &mut World, f: GameFn, a: &[Val], mem: &mut dyn GuestMemory) -> Option<Val> {
    use args::{f64 as af, i32 as ai, ret_f64, u64 as au};
    match f {
        GameFn::GameSkill => i32v(i32::from(world.skill)),
        GameFn::LevelMapname => {
            let name = world.map.name.clone();
            i32v(mem.write_str(ai(a, 0), ai(a, 1), &name))
        }
        GameFn::LevelStat => {
            let l = &world.level;
            let v = match ai(a, 0) {
                level_stat::KILLS => l.found_kills,
                level_stat::TOTAL_KILLS => l.total_kills,
                level_stat::ITEMS => l.found_items,
                level_stat::TOTAL_ITEMS => l.total_items,
                level_stat::SECRETS => l.found_secrets,
                level_stat::TOTAL_SECRETS => l.total_secrets,
                level_stat::MAPTIME => l.maptime,
                _ => 0,
            };
            i32v(i32::try_from(v).unwrap_or(i32::MAX))
        }
        GameFn::MathSin => ret_f64(dmath::sin(af(a, 0))),
        GameFn::MathCos => ret_f64(dmath::cos(af(a, 0))),
        GameFn::MathAtan2 => ret_f64(dmath::atan2(af(a, 0), af(a, 1))),
        GameFn::MathSqrt => ret_f64(dmath::sqrt(af(a, 0).max(0.0))),
        GameFn::RngRange => i32v(rng_range(world, ai(a, 0), ai(a, 1))),
        GameFn::PlayerActor => {
            let slot = usize::try_from(ai(a, 0)).ok()?.checked_sub(1)?;
            let mo = world.players.get(slot)?.as_ref()?.mo;
            Some(Val::I64(opt_ref(mo) as i64))
        }
        GameFn::PlayerGetI32 => {
            let slot = usize::try_from(ai(a, 0)).ok()?.checked_sub(1)?;
            let p = world.players.get(slot)?.as_ref()?;
            let v = match ai(a, 1) {
                player_prop::IN_GAME => 1,
                player_prop::HEALTH => {
                    p.mo.and_then(|m| world.actors.get(m))
                        .map_or(0, |a| a.health)
                }
                player_prop::ARMOR => p.armor,
                player_prop::KILLS => i32::try_from(world.level.found_kills).unwrap_or(0),
                player_prop::ITEMS => i32::try_from(world.level.found_items).unwrap_or(0),
                player_prop::SECRETS => i32::try_from(world.level.found_secrets).unwrap_or(0),
                player_prop::FRAGS => world.player_frags(slot),
                _ => 0,
            };
            i32v(v)
        }
        GameFn::PlayerPrint => {
            let text = mem.read_str(ai(a, 1), ai(a, 2));
            let to = match u8::try_from(ai(a, 0)) {
                Ok(0) => Some(crate::world::MessageTo::All),
                Ok(n) if usize::from(n) <= world.players.len() => {
                    Some(crate::world::MessageTo::Seat(n - 1))
                }
                _ => None,
            };
            if let (Some(to), false) = (to, text.is_empty()) {
                world.push_message_to(to, text);
            }
            None
        }
        GameFn::ActorClass => {
            let id = live(world, au(a, 0))?;
            i32v(class_ref(world.actors.get(id)?.class))
        }
        GameFn::ClassName => {
            let name = class_of(world, ai(a, 0))
                .and_then(|c| world.classes.get(c))
                .map(|c| c.name.clone())
                .unwrap_or_default();
            i32v(mem.write_str(ai(a, 1), ai(a, 2), &name))
        }
        GameFn::ClassIsDescendant => {
            let (Some(c), Some(p)) = (class_of(world, ai(a, 0)), class_of(world, ai(a, 1))) else {
                return i32v(0);
            };
            bool_v(is_descendant(&world.classes, c, p))
        }
        GameFn::ActorPos | GameFn::ActorVel => {
            let id = live(world, au(a, 0))?;
            let actor = world.actors.get(id)?;
            let v = if f == GameFn::ActorPos {
                actor.pos
            } else {
                actor.vel
            };
            write_vec3(mem, ai(a, 1), v);
            None
        }
        GameFn::ActorAngle => {
            let id = live(world, au(a, 0))?;
            ret_f64(world.actors.get(id)?.angle.normalized().degrees())
        }
        GameFn::ActorGetI32 => {
            let id = live(world, au(a, 0))?;
            let actor = world.actors.get(id)?;
            let v = match ai(a, 1) {
                actor_i32::HEALTH => actor.health,
                actor_i32::SPAWN_HEALTH => world.classes.get(actor.class).map_or(0, |c| c.health),
                actor_i32::TID => actor.tid,
                actor_i32::FLAGS => actor.flags as u32 as i32,
                actor_i32::IS_MONSTER => i32::from(actor.flags & FLAG_COUNTKILL != 0),
                actor_i32::PLAYER => player_slot(world, id).map_or(0, |s| s as i32 + 1),
                actor_i32::IS_MISSILE => i32::from(actor.flags & FLAG_MISSILE != 0),
                actor_i32::IS_DEAD => i32::from(actor.health <= 0),
                actor_i32::DAMAGE => actor.damage,
                actor_i32::REACTION_TIME => actor.reaction_time,
                actor_i32::FRIENDLY => i32::from(actor.flags & FLAG_FRIENDLY != 0),
                _ => 0,
            };
            i32v(v)
        }
        GameFn::ActorGetF64 => {
            let id = live(world, au(a, 0))?;
            let actor = world.actors.get(id)?;
            let v = match ai(a, 1) {
                actor_f64::RADIUS => actor.radius,
                actor_f64::HEIGHT => actor.height,
                actor_f64::SPEED => actor.speed,
                actor_f64::ALPHA => actor.alpha,
                actor_f64::SCALE_X => actor.scale_x,
                actor_f64::SCALE_Y => actor.scale_y,
                actor_f64::GRAVITY => actor.gravity,
                actor_f64::DAMAGE_FACTOR => actor.damage_factor,
                actor_f64::FLOOR_Z => {
                    crate::physics::floor_at_xy(world, actor.pos.x(), actor.pos.y()).unwrap_or(0.0)
                }
                _ => 0.0,
            };
            ret_f64(v)
        }
        GameFn::ActorGetRef => {
            let id = live(world, au(a, 0))?;
            let actor = world.actors.get(id)?;
            let r = match ai(a, 1) {
                actor_ptr::TARGET => actor.target,
                actor_ptr::MASTER => actor.master,
                actor_ptr::TRACER => actor.tracer,
                _ => None,
            };
            Some(Val::I64(
                opt_ref(r.and_then(|r| live(world, actor_ref(r)))) as i64
            ))
        }
        GameFn::ActorIterClass => {
            let filter = class_of(world, ai(a, 0));
            if ai(a, 0) != 0 && filter.is_none() {
                return i32v(0);
            }
            let subclass = ai(a, 1) != 0;
            let ids: Vec<ActorId> = world
                .thinkers
                .default
                .iter()
                .copied()
                .filter(|&id| {
                    let Some(actor) = world.actors.get(id).filter(|a| !a.destroyed) else {
                        return false;
                    };
                    match filter {
                        None => true,
                        Some(c) if subclass => is_descendant(&world.classes, actor.class, c),
                        Some(c) => actor.class == c,
                    }
                })
                .collect();
            i32v(write_refs(mem, ai(a, 2), ai(a, 3), &ids))
        }
        GameFn::ActorIterRadius => {
            let (x, y, r) = (af(a, 0), af(a, 1), af(a, 2));
            let ids: Vec<ActorId> = world
                .thinkers
                .default
                .iter()
                .copied()
                .filter(|&id| {
                    world
                        .actors
                        .get(id)
                        .filter(|a| !a.destroyed)
                        .is_some_and(|a| {
                            let (dx, dy) = (a.pos.x() - x, a.pos.y() - y);
                            dx * dx + dy * dy <= r * r
                        })
                })
                .collect();
            i32v(write_refs(mem, ai(a, 3), ai(a, 4), &ids))
        }
        GameFn::ActorCheckSight => {
            let (Some(from), Some(to)) = (live(world, au(a, 0)), live(world, au(a, 1))) else {
                return i32v(0);
            };
            bool_v(crate::sight::check_sight(world, from, to))
        }
        GameFn::ActorDistance => {
            let pa = world.actors.get(live(world, au(a, 0))?)?.pos;
            let pb = world.actors.get(live(world, au(a, 1))?)?.pos;
            let d = pa - pb;
            ret_f64(dmath::sqrt(d.x() * d.x() + d.y() * d.y() + d.z() * d.z()))
        }
        GameFn::ActorCountInv => {
            let id = live(world, au(a, 0))?;
            let cls = class_of(world, ai(a, 1))?;
            i32v(world.inventory.count(id, cls))
        }
        GameFn::PointFloorZ => {
            ret_f64(crate::physics::floor_at_xy(world, af(a, 0), af(a, 1)).unwrap_or(0.0))
        }
        GameFn::PointCeilingZ => {
            ret_f64(crate::physics::ceiling_at_xy(world, af(a, 0), af(a, 1)).unwrap_or(0.0))
        }
        GameFn::ActorSpawn => {
            let cls = class_of(world, ai(a, 0))?;
            let pos = DVec3::new(af(a, 1), af(a, 2), af(a, 3));
            let angle = Angle::from_degrees(af(a, 4));
            let flags = ai(a, 5);
            let id = world.spawn_at(cls, pos, angle).ok()?;
            if flags & spawn_flags::CHECK_POSITION != 0 {
                let actor = world.actors.get(id)?.clone();
                if !crate::physics::check_position(world, &actor, actor.pos, Some(id)).ok {
                    let l = &mut world.level;
                    if actor.flags & crate::class::FLAG_COUNTKILL != 0 {
                        l.total_kills = l.total_kills.saturating_sub(1);
                    }
                    if actor.flags & crate::class::FLAG_COUNTITEM != 0 {
                        l.total_items = l.total_items.saturating_sub(1);
                    }
                    if matches!(world.script_events.last(), Some(ScriptEvent::ThingSpawned { thing }) if *thing == id)
                    {
                        world.script_events.pop();
                    }
                    if world.thinkers.default.last() == Some(&id) {
                        world.thinkers.default.pop();
                    } else {
                        world.thinkers.default.retain(|&t| t != id);
                    }
                    world.actors.remove(id);
                    return Some(Val::I64(0));
                }
            }
            if flags & spawn_flags::FRIENDLY != 0 {
                if let Some(actor) = world.actors.get_mut(id) {
                    actor.flags |= FLAG_FRIENDLY;
                }
            }
            if flags & spawn_flags::TELEFOG != 0 {
                let at = world.actors.get(id)?.pos;
                if let Some(fog) = world.classes.find_name("TeleportFog") {
                    let _ = world.spawn_at(fog, at, angle);
                }
                world.play_sound("misc/teleport", at.x(), at.y(), at.z(), 1.0);
            }
            Some(Val::I64(actor_ref(id) as i64))
        }
        GameFn::ActorDestroy => {
            if let Some(id) = live(world, au(a, 0)) {
                if player_slot(world, id).is_none() {
                    destroy(world, id);
                }
            }
            None
        }
        GameFn::ActorSetI32 => {
            let id = live(world, au(a, 0))?;
            let v = ai(a, 2);
            let actor = world.actors.get_mut(id)?;
            match ai(a, 1) {
                actor_i32::HEALTH => actor.health = v,
                actor_i32::TID => actor.tid = v,
                actor_i32::FLAGS => {
                    actor.flags = (actor.flags & !0xffff_ffff) | u64::from(v as u32);
                }
                actor_i32::DAMAGE => actor.damage = v,
                actor_i32::REACTION_TIME => actor.reaction_time = v.max(0),
                actor_i32::FRIENDLY => {
                    if v != 0 {
                        actor.flags |= FLAG_FRIENDLY;
                    } else {
                        actor.flags &= !FLAG_FRIENDLY;
                    }
                }
                _ => {}
            }
            None
        }
        GameFn::ActorSetF64 => {
            let id = live(world, au(a, 0))?;
            let v = af(a, 2);
            let actor = world.actors.get_mut(id)?;
            match ai(a, 1) {
                actor_f64::SPEED => actor.speed = v.max(0.0),
                actor_f64::ALPHA => actor.alpha = v.clamp(0.0, 1.0),
                actor_f64::SCALE_X => actor.scale_x = v.max(0.01),
                actor_f64::SCALE_Y => actor.scale_y = v.max(0.01),
                actor_f64::GRAVITY => actor.gravity = v,
                actor_f64::DAMAGE_FACTOR => actor.damage_factor = v.max(0.0),
                _ => {}
            }
            None
        }
        GameFn::ActorSetRef => {
            let id = live(world, au(a, 0))?;
            let other = live(world, au(a, 2));
            let actor = world.actors.get_mut(id)?;
            match ai(a, 1) {
                actor_ptr::TARGET => actor.target = other,
                actor_ptr::MASTER => actor.master = other,
                actor_ptr::TRACER => actor.tracer = other,
                _ => {}
            }
            None
        }
        GameFn::ActorSetPos => {
            let id = live(world, au(a, 0))?;
            world.actors.get_mut(id)?.pos = DVec3::new(af(a, 1), af(a, 2), af(a, 3));
            i32v(1)
        }
        GameFn::ActorSetVel => {
            let id = live(world, au(a, 0))?;
            world.actors.get_mut(id)?.vel = DVec3::new(af(a, 1), af(a, 2), af(a, 3));
            None
        }
        GameFn::ActorSetAngle => {
            let id = live(world, au(a, 0))?;
            world.actors.get_mut(id)?.angle = Angle::from_degrees(af(a, 1));
            None
        }
        GameFn::ActorDamage => {
            let target = live(world, au(a, 0))?;
            let shootable = world
                .actors
                .get(target)
                .is_some_and(|t| t.flags & FLAG_SHOOTABLE != 0 && t.health > 0);
            if !shootable {
                return i32v(0);
            }
            let inflictor = live(world, au(a, 1));
            let source = live(world, au(a, 2));
            i32v(crate::damage::damage_mobj(
                world,
                target,
                inflictor,
                source,
                ai(a, 3),
            ))
        }
        GameFn::ActorGiveInventory => {
            let id = live(world, au(a, 0))?;
            let cls = class_of(world, ai(a, 1))?;
            bool_v(crate::inventory::give_inventory(
                world,
                id,
                cls,
                ai(a, 2).max(1),
            ))
        }
        GameFn::ActorTakeInventory => {
            let id = live(world, au(a, 0))?;
            let cls = class_of(world, ai(a, 1))?;
            let taken = world.inventory.take(id, cls, ai(a, 2).max(0));
            if taken > 0 {
                if let Some(slot) = player_slot(world, id) {
                    if let Some(mut p) = world.players[slot].take() {
                        crate::think_player::drop_unowned_weapon(world, &mut p, id);
                        world.players[slot] = Some(p);
                    }
                }
            }
            i32v(taken)
        }
        GameFn::ActorSetState => {
            let id = live(world, au(a, 0))?;
            let label = mem.read_str(ai(a, 1), ai(a, 2));
            let class = world.actors.get(id)?.class;
            let sid = world.states.find_label(class, &label)?;
            world.set_state(id, Some(sid), true);
            i32v(1)
        }
        GameFn::ActorStartSound => {
            let id = live(world, au(a, 0))?;
            let name = mem.read_str(ai(a, 1), ai(a, 2));
            let p = world.actors.get(id)?.pos;
            let vol = af(a, 3).clamp(0.0, 1.0) as f32;
            if !name.is_empty() {
                world.play_sound(&name, p.x(), p.y(), p.z(), vol);
            }
            None
        }
        GameFn::LevelExit => {
            world.level.exiting = true;
            world.level.secret_exit = ai(a, 0) != 0;
            None
        }
        GameFn::LevelSetMusic => {
            let name = mem.read_str(ai(a, 0), ai(a, 1));
            if !name.is_empty() {
                world.music_request = Some(name);
            }
            None
        }
        GameFn::GameGetRule => i32v(match ai(a, 0) {
            rule::DEATHMATCH => i32::from(world.deathmatch),
            rule::RESPAWN => i32::from(!world.no_player_respawn),
            rule::TIME_LEFT => i32::try_from(world.match_tics_left).unwrap_or(i32::MAX),
            rule::MATCH_OVER => i32::from(world.match_over.is_some()),
            _ => 0,
        }),
        GameFn::GameEndMatch => {
            let winner = u8::try_from(ai(a, 0) - 1).ok();
            world.end_match(winner, false);
            None
        }
        GameFn::LevelThings => {
            let ednum = ai(a, 0);
            let cap = usize::try_from(ai(a, 2)).unwrap_or(0);
            let mut buf = Vec::new();
            let mut total = 0i32;
            for t in world
                .map
                .things
                .iter()
                .filter(|t| i32::from(t.type_id) == ednum)
            {
                if (total as usize) < cap {
                    for v in [f64::from(t.x), f64::from(t.y), f64::from(t.angle)] {
                        buf.extend_from_slice(&v.to_le_bytes());
                    }
                }
                total = total.saturating_add(1);
            }
            if !buf.is_empty() {
                let _ = mem.write(ai(a, 1), &buf);
            }
            i32v(total)
        }
        GameFn::GameSetRule => {
            let on = ai(a, 1) != 0;
            match ai(a, 0) {
                rule::DEATHMATCH => world.deathmatch = on,
                rule::RESPAWN => world.no_player_respawn = !on,
                _ => {}
            }
            None
        }
    }
}

fn destroy(world: &mut World, id: ActorId) {
    if let Some(a) = world.actors.get_mut(id) {
        a.destroyed = true;
        a.state = None;
        a.flags = 0;
        a.health = 0;
    }
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

type WorldCell = Arc<Mutex<Option<World>>>;

struct WorldBridge {
    cell: WorldCell,
}

impl GameBridge for WorldBridge {
    fn call(&self, f: GameFn, args: &[Val], mem: &mut dyn GuestMemory) -> Option<Val> {
        let mut g = lock(&self.cell);
        let world = g.as_mut()?;
        serve(world, f, args, mem)
    }
}

/// One mod module to load.
#[derive(Clone, Debug)]
pub struct ModSource {
    /// Mod id from the manifest (`org.example.mod`).
    pub mod_id: String,
    /// Module name (save identity / log prefix).
    pub name: String,
    /// Play or UI scope (UI modules are not run yet).
    pub scope: ModuleScope,
    /// `.wasm` bytes.
    pub bytes: Vec<u8>,
}

struct LoadedMod {
    name: String,
    instance: WasmiInstance,
    cfg: ModuleConfig,
    scratch: i32,
    failed: bool,
}

/// Play-scope WASM mods for one game session.
pub struct ModRuntime {
    cell: WorldCell,
    log: Arc<Mutex<Vec<String>>>,
    mods: Vec<LoadedMod>,
    handlers: Vec<RegisteredEventHandler>,
    classes: Vec<RegisteredClass>,
    mask: u64,
    hash: u64,
}

/// Event bit index → `callback_base + index` dispatch id.
fn bit_index(bit: u64) -> i32 {
    bit.trailing_zeros() as i32
}

impl ModRuntime {
    /// Compile, `register` and `init(NewGame)` every play module.
    /// `classes` resolves `class_find` during registration (before a world exists).
    ///
    /// # Errors
    ///
    /// Validation / link failures or a guest trap / non-zero return in
    /// `room_register` / `room_init`.
    pub fn new(sources: &[ModSource], classes: Arc<ClassRegistry>) -> Result<Self, ScriptError> {
        let cell: WorldCell = Arc::new(Mutex::new(None));
        let log: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let current = Arc::new(Mutex::new(String::new()));
        let mut host = HostLinker::default();
        {
            let (log, cell, current) = (Arc::clone(&log), Arc::clone(&cell), Arc::clone(&current));
            host.log = Arc::new(move |level, msg| {
                let line = format!("[{}] {msg}", lock(&current));
                if level <= room_script_sdk_levels::WARN {
                    if let Some(w) = lock(&cell).as_mut() {
                        w.push_message(line.clone());
                    }
                }
                let mut g = lock(&log);
                if g.len() < 512 {
                    g.push(line);
                }
            });
        }
        {
            let cell = Arc::clone(&cell);
            host.level_time =
                Arc::new(move || lock(&cell).as_ref().map_or(0, |w| w.level.maptime as i32));
        }
        {
            let cell = Arc::clone(&cell);
            host.player_count = Arc::new(move || {
                lock(&cell)
                    .as_ref()
                    .map_or(0, |w| w.players.iter().flatten().count() as i32)
            });
        }
        {
            let cell = Arc::clone(&cell);
            host.actor_exists = Arc::new(move |bits| {
                lock(&cell)
                    .as_ref()
                    .map_or(0, |w| i32::from(live(w, bits).is_some()))
            });
        }
        {
            let cell = Arc::clone(&cell);
            let reg = Arc::clone(&host.reg);
            host.class_find = Arc::new(move |bytes| {
                if bytes.is_empty() {
                    return 0;
                }
                let name = String::from_utf8_lossy(bytes);
                let g = lock(&cell);
                // In play, mod classes are merged into the world registry; the
                // session refs only mean something during registration.
                if let Some(w) = g.as_ref() {
                    return w
                        .classes
                        .find_name(&name)
                        .map_or(0, |id| id.raw().saturating_add(1));
                }
                if let Some(r) = lock(&reg).find_class(bytes) {
                    return r;
                }
                classes
                    .find_name(&name)
                    .map_or(0, |id| id.raw().saturating_add(1))
            });
        }
        {
            let cell = Arc::clone(&cell);
            host.acs_execute = Arc::new(move |script, map, extra| {
                let mut g = lock(&cell);
                let Some(w) = g.as_mut() else {
                    return 0;
                };
                let mut a = [0i32; 5];
                a[0] = script;
                a[1] = map;
                for (i, v) in extra.iter().take(3).enumerate() {
                    a[2 + i] = *v;
                }
                crate::acs::acs_execute(w, &a, false, None, None)
            });
        }
        host.game = Some(Arc::new(WorldBridge {
            cell: Arc::clone(&cell),
        }));

        let backend = WasmiBackend::new();
        let mut mods = Vec::new();
        let mut hash = 0xcbf2_9ce4_8422_2325_u64;
        for src in sources {
            if src.scope != ModuleScope::Play {
                lock(&log).push(format!("[{}] ui modules are not supported yet", src.name));
                continue;
            }
            *lock(&current) = src.name.clone();
            let fail = |e: ScriptError| ScriptError::Backend(format!("{}: {e}", src.name));
            let cfg = ModuleConfig {
                scope: ModuleScope::Play,
                ..Default::default()
            };
            let compiled = backend.compile(&src.bytes, &cfg).map_err(fail)?;
            let mut instance = backend.instantiate(&compiled, &host).map_err(fail)?;
            let mut budget = Budget::from_config(&cfg);
            let ver = instance.abi_version(&mut budget).map_err(fail)?;
            if ver != room_script::ABI_VERSION {
                return Err(fail(ScriptError::Invalid(format!("abi version {ver}"))));
            }
            lock(&host.reg).set_current_module(mods.len());
            if instance.register(&mut budget).map_err(fail)? != 0 {
                return Err(fail(ScriptError::Backend("room_register failed".into())));
            }
            if instance
                .init(SessionKind::NewGame, &mut budget)
                .map_err(fail)?
                != 0
            {
                return Err(fail(ScriptError::Backend("room_init failed".into())));
            }
            let mut res = [Val::I32(0)];
            instance
                .call(
                    "room_alloc",
                    &[Val::I32(SCRATCH_BYTES), Val::I32(8)],
                    &mut res,
                    &mut budget,
                )
                .map_err(fail)?;
            let scratch = match res[0] {
                Val::I32(p) if p > 0 => p,
                _ => return Err(fail(ScriptError::Backend("room_alloc failed".into()))),
            };
            for b in src.mod_id.bytes().chain(src.bytes.iter().copied()) {
                hash ^= u64::from(b);
                hash = hash.wrapping_mul(0x100_0000_01b3);
            }
            mods.push(LoadedMod {
                name: src.name.clone(),
                instance,
                cfg,
                scratch,
                failed: false,
            });
        }
        let reg = lock(&host.reg);
        let mut handlers = reg.event_handlers().to_vec();
        handlers.sort_by(|a, b| {
            a.order
                .cmp(&b.order)
                .then_with(|| a.module_index.cmp(&b.module_index))
        });
        let mask = handlers.iter().fold(0, |m, h| m | h.event_mask);
        let classes = reg.classes().to_vec();
        let hash = hash ^ reg.fingerprint();
        drop(reg);
        Ok(Self {
            cell,
            log,
            mods,
            handlers,
            classes,
            mask,
            hash,
        })
    }

    /// Loaded module names.
    #[must_use]
    pub fn module_names(&self) -> Vec<&str> {
        self.mods.iter().map(|m| m.name.as_str()).collect()
    }

    /// Event bits any handler subscribed to.
    #[must_use]
    pub fn event_mask(&self) -> u64 {
        self.mask
    }

    /// Identity of the loaded mod set (bytes + registration) for net/demo checks.
    #[must_use]
    pub fn fingerprint(&self) -> u64 {
        self.hash
    }

    /// Take accumulated `log` lines.
    pub fn drain_log(&self) -> Vec<String> {
        std::mem::take(&mut *lock(&self.log))
    }

    /// Merge mod-registered classes into copies of the session tables.
    /// A class with `replaces` and no editor number takes over the replaced
    /// class's editor number (map things spawn the mod class).
    ///
    /// # Errors
    ///
    /// Unknown parent or duplicate class.
    pub fn merge_classes(
        &self,
        classes: &ClassRegistry,
        states: &StateTable,
    ) -> Result<(ClassRegistry, StateTable), ScriptError> {
        let mut registry = classes.clone();
        let mut map: BTreeMap<u32, ClassId> = BTreeMap::new();
        let resolve = |registry: &ClassRegistry, map: &BTreeMap<u32, ClassId>, r: u32| {
            map.get(&r).copied().or_else(|| {
                let id = ClassId::from_raw(r.checked_sub(1)?);
                registry.get(id).map(|_| id)
            })
        };
        for c in &self.classes {
            let parent = if c.parent == 0 {
                registry.find_name("Actor")
            } else {
                resolve(&registry, &map, c.parent)
            }
            .ok_or_else(|| ScriptError::Backend(format!("{}: unknown parent class", c.name)))?;
            let mut reg = c.clone();
            if reg.doomednum < 0 && reg.replaces != 0 {
                reg.doomednum = resolve(&registry, &map, reg.replaces)
                    .and_then(|r| registry.get(r))
                    .and_then(|d: &ClassDef| d.doomednum)
                    .unwrap_or(-1);
            }
            let id = registry
                .import_script_class(&reg, parent)
                .map_err(|e| ScriptError::Backend(e.to_string()))?;
            map.insert(c.class_ref, id);
        }
        let mut states = states.clone();
        states.inherit_labels(&registry);
        Ok((registry, states))
    }

    /// Lend `slot`'s world to the mods for `f`, then hand it back.
    fn with_world(&mut self, slot: &mut Option<World>, f: impl FnOnce(&mut Self)) {
        let Some(world) = slot.take() else {
            return;
        };
        *lock(&self.cell) = Some(world);
        f(self);
        *slot = lock(&self.cell).take();
    }

    fn fire(&mut self, bit: u64, payload: &[u8]) {
        if self.mask & bit == 0 {
            return;
        }
        let idx = bit_index(bit);
        for h in self.handlers.clone() {
            if h.event_mask & bit == 0 {
                continue;
            }
            let Some(m) = self.mods.get_mut(h.module_index) else {
                continue;
            };
            if m.failed {
                continue;
            }
            let mut budget = Budget::from_config(&m.cfg);
            let len = i32::try_from(payload.len().min(SCRATCH_BYTES as usize)).unwrap_or(0);
            let res = m
                .instance
                .write_memory(m.scratch as usize, &payload[..len as usize])
                .and_then(|()| {
                    m.instance
                        .dispatch(h.callback_base + idx, m.scratch, len, &mut budget)
                });
            if let Err(e) = res {
                m.failed = true;
                let line = format!("[{}] disabled: {e}", m.name);
                if let Some(w) = lock(&self.cell).as_mut() {
                    w.push_message(line.clone());
                }
                lock(&self.log).push(line);
            }
        }
    }

    /// Map start: subscribe the world to sim events, then `WorldLoaded` and
    /// `PlayerEntered` for each player.
    pub fn begin_map(&mut self, slot: &mut Option<World>) {
        if let Some(w) = slot.as_mut() {
            w.script_event_mask = self.mask;
            w.script_events.clear();
        }
        self.with_world(slot, |rt| {
            let (maptime, players) = {
                let g = lock(&rt.cell);
                let Some(w) = g.as_ref() else {
                    return;
                };
                let players: Vec<(i32, u64)> = w
                    .players
                    .iter()
                    .enumerate()
                    .filter_map(|(i, p)| Some((i as i32 + 1, opt_ref(p.as_ref()?.mo))))
                    .collect();
                (w.level.maptime as i32, players)
            };
            let mut p = Vec::new();
            p.extend_from_slice(&0i32.to_le_bytes());
            p.extend_from_slice(&maptime.to_le_bytes());
            rt.fire(event_mask::WORLD_LOADED, &p);
            for (player, actor) in players {
                let mut p = actor.to_le_bytes().to_vec();
                p.extend_from_slice(&player.to_le_bytes());
                rt.fire(event_mask::PLAYER_ENTERED, &p);
            }
            rt.drain_events();
        });
    }

    /// After a tic: `WorldTick`, then queued sim events (and any they cause).
    pub fn tick(&mut self, slot: &mut Option<World>) {
        if self.mods.is_empty() {
            return;
        }
        self.with_world(slot, |rt| {
            rt.drain_events();
            let maptime = lock(&rt.cell)
                .as_ref()
                .map_or(0, |w| w.level.maptime as i32);
            rt.fire(event_mask::WORLD_TICK, &maptime.to_le_bytes());
            rt.drain_events();
        });
    }

    /// Map exit: `WorldUnloaded`.
    pub fn end_map(&mut self, slot: &mut Option<World>) {
        self.with_world(slot, |rt| {
            rt.fire(event_mask::WORLD_UNLOADED, &[]);
            if let Some(w) = lock(&rt.cell).as_mut() {
                w.script_event_mask = 0;
                w.script_events.clear();
            }
        });
    }

    fn drain_events(&mut self) {
        for _ in 0..MAX_EVENT_ROUNDS {
            let (events, players) = {
                let mut g = lock(&self.cell);
                let Some(w) = g.as_mut() else {
                    return;
                };
                let players: Vec<Option<ActorId>> = w
                    .players
                    .iter()
                    .map(|p| p.as_ref().and_then(|p| p.mo))
                    .collect();
                (std::mem::take(&mut w.script_events), players)
            };
            if events.is_empty() {
                return;
            }
            for ev in events {
                match ev {
                    ScriptEvent::ThingSpawned { thing } => {
                        self.fire(
                            event_mask::WORLD_THING_SPAWNED,
                            &actor_ref(thing).to_le_bytes(),
                        );
                    }
                    ScriptEvent::ThingDamaged {
                        thing,
                        source,
                        inflictor,
                        damage,
                        health,
                    } => {
                        let mut p = actor_ref(thing).to_le_bytes().to_vec();
                        p.extend_from_slice(&opt_ref(source).to_le_bytes());
                        p.extend_from_slice(&opt_ref(inflictor).to_le_bytes());
                        p.extend_from_slice(&damage.to_le_bytes());
                        p.extend_from_slice(&health.to_le_bytes());
                        self.fire(event_mask::WORLD_THING_DAMAGED, &p);
                    }
                    ScriptEvent::ThingDied { thing, source } => {
                        let mut p = actor_ref(thing).to_le_bytes().to_vec();
                        p.extend_from_slice(&opt_ref(source).to_le_bytes());
                        p.extend_from_slice(&0u64.to_le_bytes());
                        self.fire(event_mask::WORLD_THING_DIED, &p);
                        if let Some(slot) = players.iter().position(|m| *m == Some(thing)) {
                            let mut p = actor_ref(thing).to_le_bytes().to_vec();
                            p.extend_from_slice(&(slot as i32 + 1).to_le_bytes());
                            self.fire(event_mask::PLAYER_DIED, &p);
                        }
                    }
                    ScriptEvent::ItemPickup {
                        toucher,
                        item_class,
                    } => {
                        let mut p = actor_ref(toucher).to_le_bytes().to_vec();
                        p.extend_from_slice(&class_ref(item_class).to_le_bytes());
                        self.fire(event_mask::WORLD_ITEM_PICKUP, &p);
                    }
                }
            }
        }
    }
}

/// `log` levels (mirror `room-script-sdk`).
mod room_script_sdk_levels {
    pub const WARN: i32 = 1;
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use room_decorate::parse_decorate;
    use room_nodebuild::apply_nodes;
    use room_testkit::{MapBuilder, rect};

    const DIRECTOR: &str = r#"(module
  (import "room" "log" (func $log (param i32 i32 i32)))
  (import "room" "reg_event_handler" (func $reg_eh (param i32 i32 i32 i32 i32 i32 i32) (result i32)))
  (import "room" "class_find" (func $class_find (param i32 i32) (result i32)))
  (import "room" "player_actor" (func $player_actor (param i32) (result i64)))
  (import "room" "actor_pos" (func $actor_pos (param i64 i32)))
  (import "room" "actor_spawn" (func $spawn (param i32 f64 f64 f64 f64 i32) (result i64)))
  (import "room" "actor_damage" (func $damage (param i64 i64 i64 i32) (result i32)))
  (import "room" "player_print" (func $print (param i32 i32 i32)))
  (import "room" "actor_set_i32" (func $set_i32 (param i64 i32 i32)))
  (memory (export "memory") 1)
  (global $heap (mut i32) (i32.const 4096))
  (global $imp (mut i64) (i64.const 0))
  (global $deaths (mut i32) (i32.const 0))
  (data (i32.const 0) "Tester")
  (data (i32.const 16) "DoomImp")
  (data (i32.const 32) "imp down")
  (func (export "room_abi_version") (result i32) (i32.const 1))
  (func (export "room_alloc") (param $size i32) (param $align i32) (result i32)
    (local $p i32)
    (local.set $p (global.get $heap))
    (global.set $heap (i32.add (global.get $heap) (local.get $size)))
    (local.get $p))
  (func (export "room_free") (param i32 i32 i32))
  (func (export "room_register") (result i32)
    (drop (call $reg_eh (i32.const 0) (i32.const 6) (i32.const 0) (i32.const 0)
      (i32.const 11) (i32.const 0) (i32.const 100)))
    (i32.const 0))
  (func (export "room_init") (param i32) (result i32) (i32.const 0))
  (func (export "room_dispatch") (param $id i32) (param $ptr i32) (param $len i32) (result i64)
    (if (i32.eq (local.get $id) (i32.const 100))
      (then
        (if (i32.eq (i32.load (local.get $ptr)) (i32.const 3))
          (then
            (call $actor_pos (call $player_actor (i32.const 1)) (i32.const 64))
            (global.set $imp (call $spawn (call $class_find (i32.const 16) (i32.const 7))
              (f64.add (f64.load (i32.const 64)) (f64.const 256))
              (f64.load (i32.const 72)) (f64.const 0) (f64.const 180) (i32.const 0)))))
        (if (i32.eq (i32.load (local.get $ptr)) (i32.const 5))
          (then (drop (call $damage (global.get $imp) (i64.const 0) (i64.const 0) (i32.const 1000)))))))
    (if (i32.eq (local.get $id) (i32.const 101))
      (then
        (if (i64.eq (i64.load (local.get $ptr)) (global.get $imp))
          (then
            (global.set $deaths (i32.add (global.get $deaths) (i32.const 1)))
            (call $print (i32.const 0) (i32.const 32) (i32.const 8))))))
    (if (i32.eq (local.get $id) (i32.const 103))
      (then (call $set_i32 (call $player_actor (i32.const 1)) (i32.const 0) (i32.const 150))))
    (i64.extend_i32_s (global.get $deaths)))
)"#;

    const HUNTER: &str = r#"(module
  (import "room" "class_find" (func $class_find (param i32 i32) (result i32)))
  (import "room" "reg_class_begin" (func $begin (param i32 i32 i32 i32 i32 i32) (result i32)))
  (import "room" "reg_class_property_i32" (func $prop (param i32 i32)))
  (import "room" "reg_class_property_str" (func $prop_str (param i32 i32 i32)))
  (import "room" "reg_class_end" (func $end))
  (memory (export "memory") 1)
  (data (i32.const 0) "DoomImp")
  (data (i32.const 16) "HunterImp")
  (data (i32.const 32) "Ice")
  (func (export "room_abi_version") (result i32) (i32.const 1))
  (func (export "room_alloc") (param i32 i32) (result i32) (i32.const 1024))
  (func (export "room_free") (param i32 i32 i32))
  (func (export "room_register") (result i32)
    (local $imp i32)
    (local.set $imp (call $class_find (i32.const 0) (i32.const 7)))
    (drop (call $begin (i32.const 16) (i32.const 9) (local.get $imp) (local.get $imp) (i32.const -1) (i32.const 0)))
    (call $prop (i32.const 1) (i32.const 200))
    (call $prop_str (i32.const 20) (i32.const 32) (i32.const 3))
    (call $end)
    (i32.const 0))
  (func (export "room_init") (param i32) (result i32) (i32.const 0))
  (func (export "room_dispatch") (param i32 i32 i32) (result i64) (i64.const 0))
)"#;

    fn tables() -> (Arc<ClassRegistry>, Arc<StateTable>) {
        let ir = parse_decorate(include_str!("../../../wadsrc/room/actors/doom.txt")).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        (classes, states)
    }

    fn map_with_imp() -> room_map::MapGeometry {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 1024.0, 512.0), 0.0, 128.0)
            .player_start(64.0, 128.0, 0)
            .thing(3001, 900.0, 400.0, 180)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        map
    }

    fn source(name: &str, wat: &str) -> ModSource {
        ModSource {
            mod_id: format!("test.{name}"),
            name: name.into(),
            scope: ModuleScope::Play,
            bytes: wat::parse_str(wat).unwrap(),
        }
    }

    #[test]
    fn director_spawns_kills_and_reacts() {
        let (classes, states) = tables();
        let mut rt =
            ModRuntime::new(&[source("director", DIRECTOR)], Arc::clone(&classes)).unwrap();
        assert_eq!(rt.module_names(), ["director"]);
        let world = World::from_map_with_classes(map_with_imp(), classes, states).unwrap();
        let mut slot = Some(world);
        rt.begin_map(&mut slot);
        let w = slot.as_ref().unwrap();
        let p = w.players[0].as_ref().and_then(|p| p.mo).unwrap();
        assert_eq!(w.actors.get(p).unwrap().health, 150);
        let imps_before = imp_count(w);
        let mut msgs = Vec::new();
        for _ in 0..8 {
            slot.as_mut().unwrap().tick();
            rt.tick(&mut slot);
            msgs.extend(slot.as_mut().unwrap().drain_messages());
        }
        let w = slot.as_mut().unwrap();
        assert_eq!(imp_count(w), imps_before + 1, "spawned one imp");
        assert!(msgs.iter().any(|m| m == "imp down"), "{msgs:?}");
        assert!(rt.drain_log().is_empty());
    }

    fn imp_count(w: &World) -> usize {
        let imp = w.classes.find_name("DoomImp").unwrap();
        w.thinkers
            .default
            .iter()
            .filter(|&&id| w.actors.get(id).is_some_and(|a| a.class == imp))
            .count()
    }

    #[test]
    fn registered_class_replaces_map_imps_and_inherits_states() {
        let (classes, states) = tables();
        let rt = ModRuntime::new(&[source("hunter", HUNTER)], Arc::clone(&classes)).unwrap();
        let (classes, states) = rt.merge_classes(&classes, &states).unwrap();
        let hunter = classes.find_name("HunterImp").unwrap();
        let def = classes.get(hunter).unwrap();
        assert_eq!(def.health, 200);
        assert_eq!(def.translation, "Ice");
        assert_eq!(classes.find_doomednum(3001), Some(hunter));
        assert!(states.find_label(hunter, "See").is_some());
        let w = World::from_map_with_classes(map_with_imp(), Arc::new(classes), Arc::new(states))
            .unwrap();
        assert!(w.thinkers.default.iter().any(|&id| {
            w.actors
                .get(id)
                .is_some_and(|a| a.class == hunter && a.health == 200)
        }));
    }

    #[test]
    fn trapping_mod_is_disabled_not_fatal() {
        let wat = r#"(module
  (import "room" "reg_event_handler" (func $reg_eh (param i32 i32 i32 i32 i32 i32 i32) (result i32)))
  (memory (export "memory") 1)
  (data (i32.const 0) "Bad")
  (func (export "room_abi_version") (result i32) (i32.const 1))
  (func (export "room_alloc") (param i32 i32) (result i32) (i32.const 1024))
  (func (export "room_free") (param i32 i32 i32))
  (func (export "room_register") (result i32)
    (drop (call $reg_eh (i32.const 0) (i32.const 3) (i32.const 0) (i32.const 0) (i32.const 1) (i32.const 0) (i32.const 7)))
    (i32.const 0))
  (func (export "room_init") (param i32) (result i32) (i32.const 0))
  (func (export "room_dispatch") (param i32 i32 i32) (result i64) unreachable)
)"#;
        let (classes, states) = tables();
        let mut rt = ModRuntime::new(&[source("bad", wat)], Arc::clone(&classes)).unwrap();
        let mut slot = Some(World::from_map_with_classes(map_with_imp(), classes, states).unwrap());
        rt.begin_map(&mut slot);
        let mut msgs = Vec::new();
        for _ in 0..3 {
            slot.as_mut().unwrap().tick();
            rt.tick(&mut slot);
            msgs.extend(slot.as_mut().unwrap().drain_messages());
        }
        assert_eq!(
            slot.as_ref().unwrap().level.maptime,
            3,
            "world keeps running"
        );
        assert_eq!(msgs.iter().filter(|m| m.contains("disabled")).count(), 1);
    }
}
