//! Engine call abstraction: [`SdkHost`] on wasm, [`FakeHost`] in native tests.

use room_script_sdk::game::{
    self, actor_i32, actor_ptr, level_stat, player_prop, spawn_flags, Actor,
};
#[cfg(test)]
use room_script_sdk::game::actor_f64;
use room_script_sdk::{ClassRef, class_find};
#[cfg(test)]
use room_script_sdk::ActorRef;

/// Every engine call the director needs. Logic never imports `game::` directly.
pub trait Host {
    /// Doom skill 1..=5.
    fn skill(&self) -> i32;
    /// Level maptime in tics.
    fn maptime(&self) -> i32;
    /// Current map lump name into `buf`.
    fn map_name<'a>(&self, buf: &'a mut [u8]) -> &'a str;
    /// Resolve a class by name (`ClassRef::NULL` if missing).
    fn class_find(&self, name: &str) -> ClassRef;
    /// `class` descends from `parent` (or equal).
    fn class_is(&self, class: ClassRef, parent: ClassRef) -> bool;
    /// Player body for seat `slot` (0-based).
    fn player(&self, slot: i32) -> Option<Actor>;
    /// Seat is participating (`player_prop::IN_GAME`).
    fn player_in_game(&self, slot: i32) -> bool;
    /// HUD print (`None` = everyone).
    fn print(&mut self, seat: Option<i32>, msg: &str);
    /// Inclusive playsim RNG.
    fn random(&mut self, lo: i32, hi: i32) -> i32;
    /// Deterministic sqrt.
    fn sqrt(&self, x: f64) -> f64;
    /// Deterministic atan2 (radians).
    fn atan2(&self, y: f64, x: f64) -> f64;
    /// Floor Z at map point.
    fn floor_z(&self, x: f64, y: f64) -> f64;
    /// Ceiling Z at map point.
    fn ceiling_z(&self, x: f64, y: f64) -> f64;
    /// Spawn an actor; `None` on failure.
    fn spawn(&mut self, class: ClassRef, pos: [f64; 3], angle: f64, flags: i32) -> Option<Actor>;
    /// Fill `out` with actors of `class` (or all when null).
    fn actors_of_class(&self, class: ClassRef, subclasses: bool, out: &mut [Actor]) -> usize;
    /// Thing positions for DoomEdNum `ednum`.
    fn level_things(&self, ednum: i32, out: &mut [[f64; 3]]) -> usize;
    /// Change music lump.
    fn set_music(&mut self, name: &str);
    /// End the map (`secret` selects secret exit).
    fn exit_level(&mut self, secret: bool);

    /// Actor still in the world.
    fn actor_exists(&self, a: Actor) -> bool;
    /// Class of `a`.
    fn actor_class(&self, a: Actor) -> ClassRef;
    /// World position.
    fn actor_pos(&self, a: Actor) -> [f64; 3];
    /// Facing angle in degrees.
    fn actor_angle(&self, a: Actor) -> f64;
    /// Health.
    fn actor_health(&self, a: Actor) -> i32;
    /// Set health.
    fn actor_set_health(&mut self, a: Actor, v: i32);
    /// Dead?
    fn actor_is_dead(&self, a: Actor) -> bool;
    /// Monster flag.
    fn actor_is_monster(&self, a: Actor) -> bool;
    /// Player seat of this body (`Some(1..)`), else `None`.
    fn actor_player(&self, a: Actor) -> Option<i32>;
    /// Integer property.
    fn actor_get_i32(&self, a: Actor, prop: i32) -> i32;
    /// Float property.
    fn actor_get_f64(&self, a: Actor, prop: i32) -> f64;
    /// Set float property.
    fn actor_set_f64(&mut self, a: Actor, prop: i32, v: f64);
    /// Target pointer.
    fn actor_target(&self, a: Actor) -> Option<Actor>;
    /// Line-of-sight check.
    fn actor_can_see(&self, a: Actor, other: Actor) -> bool;
    /// Set actor pointer property.
    fn actor_set_ref(&mut self, a: Actor, ptr: i32, v: Option<Actor>);
    /// Teleport / move.
    fn actor_set_pos(&mut self, a: Actor, pos: [f64; 3]) -> bool;
    /// Remove from the world.
    fn actor_destroy(&mut self, a: Actor);
    /// Jump to state label.
    fn actor_set_state(&mut self, a: Actor, label: &str) -> bool;
    /// Give inventory.
    fn actor_give(&mut self, a: Actor, item: ClassRef, amount: i32) -> bool;
    /// Deal damage; returns amount dealt.
    fn actor_damage(
        &mut self,
        target: Actor,
        inflictor: Option<Actor>,
        source: Option<Actor>,
        amount: i32,
    ) -> i32;
    /// Play a SNDINFO sound from `a`.
    fn actor_sound(&mut self, a: Actor, name: &str, volume: f64);
    /// Toggle a class-flag bit on an actor (`docs/27` C2).
    fn actor_set_flag(&mut self, a: Actor, flag: i32, on: bool);
    /// Screen flash props for seat `slot` (`docs/27` C3).
    fn player_set_i32(&mut self, slot: i32, prop: i32, value: i32);
}

/// Forwards to [`room_script_sdk::game`].
#[derive(Default)]
pub struct SdkHost;

impl Host for SdkHost {
    fn skill(&self) -> i32 {
        game::skill()
    }
    fn maptime(&self) -> i32 {
        game::level_stat(level_stat::MAPTIME)
    }
    fn map_name<'a>(&self, buf: &'a mut [u8]) -> &'a str {
        game::map_name(buf)
    }
    fn class_find(&self, name: &str) -> ClassRef {
        class_find(name)
    }
    fn class_is(&self, class: ClassRef, parent: ClassRef) -> bool {
        game::class_is(class, parent)
    }
    fn player(&self, slot: i32) -> Option<Actor> {
        game::player(slot)
    }
    fn player_in_game(&self, slot: i32) -> bool {
        game::player_stat(slot, player_prop::IN_GAME) != 0
    }
    fn print(&mut self, seat: Option<i32>, msg: &str) {
        game::print(seat, msg);
    }
    fn random(&mut self, lo: i32, hi: i32) -> i32 {
        game::random(lo, hi)
    }
    fn sqrt(&self, x: f64) -> f64 {
        game::sqrt(x)
    }
    fn atan2(&self, y: f64, x: f64) -> f64 {
        game::atan2(y, x)
    }
    fn floor_z(&self, x: f64, y: f64) -> f64 {
        game::floor_z(x, y)
    }
    fn ceiling_z(&self, x: f64, y: f64) -> f64 {
        game::ceiling_z(x, y)
    }
    fn spawn(&mut self, class: ClassRef, pos: [f64; 3], angle: f64, flags: i32) -> Option<Actor> {
        game::spawn(class, pos, angle, flags)
    }
    fn actors_of_class(&self, class: ClassRef, subclasses: bool, out: &mut [Actor]) -> usize {
        game::actors_of_class(class, subclasses, out)
    }
    fn level_things(&self, ednum: i32, out: &mut [[f64; 3]]) -> usize {
        game::level_things(ednum, out)
    }
    fn set_music(&mut self, name: &str) {
        game::set_music(name);
    }
    fn exit_level(&mut self, secret: bool) {
        game::exit_level(secret);
    }
    fn actor_exists(&self, a: Actor) -> bool {
        a.exists()
    }
    fn actor_class(&self, a: Actor) -> ClassRef {
        a.class()
    }
    fn actor_pos(&self, a: Actor) -> [f64; 3] {
        a.pos()
    }
    fn actor_angle(&self, a: Actor) -> f64 {
        a.angle()
    }
    fn actor_health(&self, a: Actor) -> i32 {
        a.health()
    }
    fn actor_set_health(&mut self, a: Actor, v: i32) {
        a.set_health(v);
    }
    fn actor_is_dead(&self, a: Actor) -> bool {
        a.is_dead()
    }
    fn actor_is_monster(&self, a: Actor) -> bool {
        a.is_monster()
    }
    fn actor_player(&self, a: Actor) -> Option<i32> {
        a.player()
    }
    fn actor_get_i32(&self, a: Actor, prop: i32) -> i32 {
        a.get_i32(prop)
    }
    fn actor_get_f64(&self, a: Actor, prop: i32) -> f64 {
        a.get_f64(prop)
    }
    fn actor_set_f64(&mut self, a: Actor, prop: i32, v: f64) {
        a.set_f64(prop, v);
    }
    fn actor_target(&self, a: Actor) -> Option<Actor> {
        a.target()
    }
    fn actor_can_see(&self, a: Actor, other: Actor) -> bool {
        a.can_see(other)
    }
    fn actor_set_ref(&mut self, a: Actor, ptr: i32, v: Option<Actor>) {
        a.set_ref(ptr, v);
    }
    fn actor_set_pos(&mut self, a: Actor, pos: [f64; 3]) -> bool {
        a.set_pos(pos)
    }
    fn actor_destroy(&mut self, a: Actor) {
        a.destroy();
    }
    fn actor_set_state(&mut self, a: Actor, label: &str) -> bool {
        a.set_state(label)
    }
    fn actor_give(&mut self, a: Actor, item: ClassRef, amount: i32) -> bool {
        a.give(item, amount)
    }
    fn actor_damage(
        &mut self,
        target: Actor,
        inflictor: Option<Actor>,
        source: Option<Actor>,
        amount: i32,
    ) -> i32 {
        target.damage(amount, inflictor, source)
    }
    fn actor_sound(&mut self, a: Actor, name: &str, volume: f64) {
        a.sound(name, volume);
    }
    fn actor_set_flag(&mut self, a: Actor, flag: i32, on: bool) {
        game::actor_set_flag(a, flag, on);
    }
    fn player_set_i32(&mut self, slot: i32, prop: i32, value: i32) {
        game::player_set_i32(slot, prop, value);
    }
}

/// Telefog + position-check spawn flags (shared with FakeHost).
#[must_use]
pub const fn default_spawn_flags() -> i32 {
    spawn_flags::TELEFOG | spawn_flags::CHECK_POSITION
}

/// Target pointer id.
pub const TARGET_PTR: i32 = actor_ptr::TARGET;
/// Master pointer id (escorts / summons).
pub const MASTER_PTR: i32 = actor_ptr::MASTER;
/// Missile flag property.
pub const IS_MISSILE: i32 = actor_i32::IS_MISSILE;

#[cfg(test)]
mod fake {
    use super::*;
    use std::string::String;
    use std::vec::Vec;

    #[derive(Clone)]
    struct FakeActor {
        pos: [f64; 3],
        angle: f64,
        health: i32,
        class: ClassRef,
        player: Option<i32>,
        is_missile: bool,
        is_monster: bool,
        target: Option<u64>,
        master: Option<u64>,
        alive: bool,
        speed: f64,
        scale_x: f64,
        scale_y: f64,
        alpha: f64,
        damage_factor: f64,
    }

    impl FakeActor {
        fn monster(pos: [f64; 3], class: ClassRef, health: i32) -> Self {
            Self {
                pos,
                angle: 0.0,
                health,
                class,
                player: None,
                is_missile: false,
                is_monster: true,
                target: None,
                master: None,
                alive: true,
                speed: 8.0,
                scale_x: 1.0,
                scale_y: 1.0,
                alpha: 1.0,
                damage_factor: 1.0,
            }
        }
    }

    /// Scripted host for native director / spot tests.
    pub struct FakeHost {
        pub skill: i32,
        pub maptime: i32,
        pub map: String,
        pub prints: Vec<(Option<i32>, String)>,
        pub music: Vec<String>,
        pub destroyed: Vec<u64>,
        pub sounds: Vec<(u64, String)>,
        rng: u32,
        actors: Vec<FakeActor>,
        /// (name, id, parent)
        classes: Vec<(String, ClassRef, ClassRef)>,
        things: Vec<(i32, [f64; 3])>,
        next_class: u32,
        ceiling: f64,
        can_see: bool,
        spawns: u32,
    }

    impl FakeHost {
        /// Skill `skill`, seeded RNG.
        #[must_use]
        pub fn new(skill: i32, seed: u32) -> Self {
            Self {
                skill,
                maptime: 0,
                map: "E1M1".into(),
                prints: Vec::new(),
                music: Vec::new(),
                destroyed: Vec::new(),
                sounds: Vec::new(),
                rng: seed | 1,
                actors: Vec::new(),
                classes: Vec::new(),
                things: Vec::new(),
                next_class: 1,
                ceiling: 128.0,
                can_see: true,
                spawns: 0,
            }
        }

        /// Register / look up a class name.
        pub fn ensure_class(&mut self, name: &str) -> ClassRef {
            if let Some((_, c, _)) = self.classes.iter().find(|(n, _, _)| n == name) {
                return *c;
            }
            let c = ClassRef(self.next_class);
            self.next_class = self.next_class.saturating_add(1);
            self.classes.push((name.into(), c, ClassRef::NULL));
            c
        }

        /// Set inheritance for `class_is` tests.
        pub fn set_class_parent(&mut self, name: &str, parent: &str) {
            let p = self.ensure_class(parent);
            let c = self.ensure_class(name);
            if let Some(slot) = self.classes.iter_mut().find(|(n, _, _)| n == name) {
                slot.2 = p;
                let _ = c;
            }
        }

        /// Force LOS results for stuck tests.
        pub fn set_can_see(&mut self, v: bool) {
            self.can_see = v;
        }

        /// Actors created via [`Host::spawn`].
        #[must_use]
        pub fn spawn_count(&self) -> u32 {
            self.spawns
        }

        /// Add a DoomEdNum thing position.
        pub fn add_thing(&mut self, ednum: i32, pos: [f64; 3]) {
            self.things.push((ednum, pos));
        }

        /// Add a player at `pos` (seat `slot` 0-based).
        pub fn add_player(&mut self, slot: i32, pos: [f64; 3]) -> Actor {
            let class = self.ensure_class("PlayerPawn");
            let id = self.actors.len() as u64 + 1;
            self.actors.push(FakeActor {
                pos,
                angle: 0.0,
                health: 100,
                class,
                player: Some(slot + 1),
                is_missile: false,
                is_monster: false,
                target: None,
                master: None,
                alive: true,
                speed: 8.0,
                scale_x: 1.0,
                scale_y: 1.0,
                alpha: 1.0,
                damage_factor: 1.0,
            });
            Actor(ActorRef(id))
        }

        /// Add a non-player prop used as a spawn spot candidate.
        pub fn add_prop(&mut self, pos: [f64; 3]) -> Actor {
            let class = self.ensure_class("Column");
            let id = self.actors.len() as u64 + 1;
            let mut a = FakeActor::monster(pos, class, 1);
            a.is_monster = false;
            self.actors.push(a);
            Actor(ActorRef(id))
        }

        /// Add a living monster (for stuck unit tests).
        pub fn add_monster(&mut self, pos: [f64; 3]) -> Actor {
            let class = self.ensure_class("ZombieMan");
            let id = self.actors.len() as u64 + 1;
            self.actors.push(FakeActor::monster(pos, class, 20));
            Actor(ActorRef(id))
        }

        fn idx(a: Actor) -> Option<usize> {
            let id = a.0.0;
            (id > 0).then_some((id - 1) as usize)
        }

        fn get(&self, a: Actor) -> Option<&FakeActor> {
            Self::idx(a)
                .and_then(|i| self.actors.get(i))
                .filter(|f| f.alive)
        }

        fn get_mut(&mut self, a: Actor) -> Option<&mut FakeActor> {
            let i = Self::idx(a)?;
            self.actors.get_mut(i).filter(|f| f.alive)
        }

        fn parent_of(&self, class: ClassRef) -> ClassRef {
            self.classes
                .iter()
                .find(|(_, c, _)| *c == class)
                .map(|(_, _, p)| *p)
                .unwrap_or(ClassRef::NULL)
        }
    }

    impl Host for FakeHost {
        fn skill(&self) -> i32 {
            self.skill
        }
        fn maptime(&self) -> i32 {
            self.maptime
        }
        fn map_name<'a>(&self, buf: &'a mut [u8]) -> &'a str {
            let b = self.map.as_bytes();
            let n = b.len().min(buf.len());
            buf[..n].copy_from_slice(&b[..n]);
            core::str::from_utf8(&buf[..n]).unwrap_or("")
        }
        fn class_find(&self, name: &str) -> ClassRef {
            self.classes
                .iter()
                .find(|(n, _, _)| n == name)
                .map(|(_, c, _)| *c)
                .unwrap_or(ClassRef::NULL)
        }
        fn class_is(&self, class: ClassRef, parent: ClassRef) -> bool {
            if class.is_null() || parent.is_null() {
                return false;
            }
            if class == parent {
                return true;
            }
            let mut cur = class;
            for _ in 0..16 {
                let p = self.parent_of(cur);
                if p.is_null() {
                    return false;
                }
                if p == parent {
                    return true;
                }
                cur = p;
            }
            false
        }
        fn player(&self, slot: i32) -> Option<Actor> {
            self.actors
                .iter()
                .enumerate()
                .find(|(_, a)| a.alive && a.player == Some(slot + 1))
                .map(|(i, _)| Actor(ActorRef(i as u64 + 1)))
        }
        fn player_in_game(&self, slot: i32) -> bool {
            self.player(slot).is_some()
        }
        fn print(&mut self, seat: Option<i32>, msg: &str) {
            self.prints.push((seat, msg.into()));
        }
        fn random(&mut self, lo: i32, hi: i32) -> i32 {
            if hi < lo {
                return lo;
            }
            self.rng = self.rng.wrapping_mul(1664525).wrapping_add(1013904223);
            let span = (hi as i64 - lo as i64 + 1) as u32;
            lo.wrapping_add((self.rng % span.max(1)) as i32)
        }
        fn sqrt(&self, x: f64) -> f64 {
            x.sqrt()
        }
        fn atan2(&self, y: f64, x: f64) -> f64 {
            y.atan2(x)
        }
        fn floor_z(&self, _x: f64, _y: f64) -> f64 {
            0.0
        }
        fn ceiling_z(&self, _x: f64, _y: f64) -> f64 {
            self.ceiling
        }
        fn spawn(
            &mut self,
            class: ClassRef,
            pos: [f64; 3],
            _angle: f64,
            _flags: i32,
        ) -> Option<Actor> {
            if class.is_null() {
                return None;
            }
            let id = self.actors.len() as u64 + 1;
            self.actors.push(FakeActor::monster(pos, class, 20));
            self.spawns = self.spawns.saturating_add(1);
            Some(Actor(ActorRef(id)))
        }
        fn actors_of_class(&self, class: ClassRef, _subclasses: bool, out: &mut [Actor]) -> usize {
            let mut n = 0;
            for (i, a) in self.actors.iter().enumerate() {
                if !a.alive || n >= out.len() {
                    break;
                }
                if class.is_null() || a.class == class {
                    out[n] = Actor(ActorRef(i as u64 + 1));
                    n += 1;
                }
            }
            n
        }
        fn level_things(&self, ednum: i32, out: &mut [[f64; 3]]) -> usize {
            let mut n = 0;
            for &(ed, pos) in &self.things {
                if ed == ednum && n < out.len() {
                    out[n] = pos;
                    n += 1;
                }
            }
            n
        }
        fn set_music(&mut self, name: &str) {
            self.music.push(name.into());
        }
        fn exit_level(&mut self, secret: bool) {
            self.music.push(if secret { "EXIT_SECRET" } else { "EXIT" }.into());
        }
        fn actor_exists(&self, a: Actor) -> bool {
            self.get(a).is_some()
        }
        fn actor_class(&self, a: Actor) -> ClassRef {
            self.get(a).map(|f| f.class).unwrap_or(ClassRef::NULL)
        }
        fn actor_pos(&self, a: Actor) -> [f64; 3] {
            self.get(a).map(|f| f.pos).unwrap_or([0.0; 3])
        }
        fn actor_angle(&self, a: Actor) -> f64 {
            self.get(a).map(|f| f.angle).unwrap_or(0.0)
        }
        fn actor_health(&self, a: Actor) -> i32 {
            self.get(a).map(|f| f.health).unwrap_or(0)
        }
        fn actor_set_health(&mut self, a: Actor, v: i32) {
            if let Some(f) = self.get_mut(a) {
                f.health = v;
            }
        }
        fn actor_is_dead(&self, a: Actor) -> bool {
            self.get(a).is_none_or(|f| f.health <= 0)
        }
        fn actor_is_monster(&self, a: Actor) -> bool {
            self.get(a).is_some_and(|f| f.is_monster)
        }
        fn actor_player(&self, a: Actor) -> Option<i32> {
            self.get(a).and_then(|f| f.player)
        }
        fn actor_get_i32(&self, a: Actor, prop: i32) -> i32 {
            if prop == IS_MISSILE {
                i32::from(self.get(a).is_some_and(|f| f.is_missile))
            } else {
                0
            }
        }
        fn actor_get_f64(&self, a: Actor, prop: i32) -> f64 {
            let Some(f) = self.get(a) else {
                return 0.0;
            };
            match prop {
                actor_f64::SPEED => f.speed,
                actor_f64::SCALE_X => f.scale_x,
                actor_f64::SCALE_Y => f.scale_y,
                actor_f64::ALPHA => f.alpha,
                actor_f64::DAMAGE_FACTOR => f.damage_factor,
                _ => 0.0,
            }
        }
        fn actor_set_f64(&mut self, a: Actor, prop: i32, v: f64) {
            let Some(f) = self.get_mut(a) else {
                return;
            };
            match prop {
                actor_f64::SPEED => f.speed = v,
                actor_f64::SCALE_X => f.scale_x = v,
                actor_f64::SCALE_Y => f.scale_y = v,
                actor_f64::ALPHA => f.alpha = v,
                actor_f64::DAMAGE_FACTOR => f.damage_factor = v,
                _ => {}
            }
        }
        fn actor_target(&self, a: Actor) -> Option<Actor> {
            self.get(a)
                .and_then(|f| f.target)
                .map(|id| Actor(ActorRef(id)))
        }
        fn actor_can_see(&self, _a: Actor, _other: Actor) -> bool {
            self.can_see
        }
        fn actor_set_ref(&mut self, a: Actor, ptr: i32, v: Option<Actor>) {
            if let Some(f) = self.get_mut(a) {
                match ptr {
                    TARGET_PTR => f.target = v.map(|x| x.0.0),
                    MASTER_PTR => f.master = v.map(|x| x.0.0),
                    _ => {}
                }
            }
        }
        fn actor_set_pos(&mut self, a: Actor, pos: [f64; 3]) -> bool {
            if let Some(f) = self.get_mut(a) {
                f.pos = pos;
                true
            } else {
                false
            }
        }
        fn actor_destroy(&mut self, a: Actor) {
            if let Some(f) = self.get_mut(a) {
                f.alive = false;
                self.destroyed.push(a.0.0);
            }
        }
        fn actor_set_state(&mut self, _a: Actor, _label: &str) -> bool {
            true
        }
        fn actor_give(&mut self, _a: Actor, _item: ClassRef, _amount: i32) -> bool {
            true
        }
        fn actor_damage(
            &mut self,
            target: Actor,
            _inflictor: Option<Actor>,
            _source: Option<Actor>,
            amount: i32,
        ) -> i32 {
            let Some(f) = self.get_mut(target) else {
                return 0;
            };
            let dealt = amount.min(f.health.max(0));
            f.health -= dealt;
            if f.health <= 0 {
                f.alive = false;
                self.destroyed.push(target.0.0);
            }
            dealt
        }
        fn actor_sound(&mut self, a: Actor, name: &str, _volume: f64) {
            self.sounds.push((a.0.0, name.into()));
        }
        fn actor_set_flag(&mut self, a: Actor, flag: i32, on: bool) {
            if let Some(f) = self.get_mut(a) {
                // INVULNERABLE / NOPAIN ≈ damage_factor 0 for FakeHost tests.
                if flag == 10 || flag == 12 {
                    f.damage_factor = if on { 0.0 } else { 1.0 };
                }
            }
        }
        fn player_set_i32(&mut self, _slot: i32, _prop: i32, _value: i32) {}
    }
}

#[cfg(test)]
pub use fake::FakeHost;
