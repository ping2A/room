//! Engine call abstraction: [`SdkHost`] on wasm, [`FakeHost`] in native tests.

use room_script_sdk::game::{self, level_stat, spawn_flags, Actor};
#[cfg(test)]
use room_script_sdk::game::actor_f64;
use room_script_sdk::{class_find, ClassRef};
#[cfg(test)]
use room_script_sdk::ActorRef;

/// Every engine call the Battle Royale mod needs.
pub trait Host {
    fn print(&mut self, seat: Option<i32>, msg: &str);
    fn random(&mut self, lo: i32, hi: i32) -> i32;
    fn sqrt(&self, x: f64) -> f64;
    fn atan2(&self, y: f64, x: f64) -> f64;
    fn floor_z(&self, x: f64, y: f64) -> f64;
    fn ceiling_z(&self, x: f64, y: f64) -> f64;
    fn actor_can_see(&self, a: Actor, b: Actor) -> bool;
    fn class_find(&self, name: &str) -> ClassRef;
    fn player(&self, slot: i32) -> Option<Actor>;
    fn player_stat(&self, slot: i32, prop: i32) -> i32;
    fn maptime(&self) -> i32;
    fn set_rule(&mut self, rule: i32, value: i32);
    fn get_rule(&self, rule: i32) -> i32;
    fn spawn(&mut self, class: ClassRef, pos: [f64; 3], angle: f64, flags: i32) -> Option<Actor>;
    fn actors_of_class(&self, class: ClassRef, subclasses: bool, out: &mut [Actor]) -> usize;
    fn level_things(&self, ednum: i32, out: &mut [[f64; 3]]) -> usize;

    fn actor_pos(&self, a: Actor) -> [f64; 3];
    fn actor_destroy(&mut self, a: Actor);
    fn actor_is_monster(&self, a: Actor) -> bool;
    fn actor_take(&mut self, a: Actor, item: ClassRef, amount: i32) -> i32;
    #[allow(dead_code)]
    fn class_is(&self, class: ClassRef, parent: ClassRef) -> bool;
    fn actor_set_pos(&mut self, a: Actor, pos: [f64; 3]) -> bool;
    fn actor_set_vel(&mut self, a: Actor, vel: [f64; 3]);
    fn actor_set_angle(&mut self, a: Actor, deg: f64);
    fn actor_sound(&mut self, a: Actor, name: &str, volume: f64);
    fn actor_damage(
        &mut self,
        target: Actor,
        amount: i32,
        inflictor: Option<Actor>,
        source: Option<Actor>,
    ) -> i32;
    fn actor_is_dead(&self, a: Actor) -> bool;
    fn actor_health(&self, a: Actor) -> i32;
    fn actor_set_health(&mut self, a: Actor, v: i32);
    fn actor_get_f64(&self, a: Actor, prop: i32) -> f64;
    fn actor_set_f64(&mut self, a: Actor, prop: i32, v: f64);
    fn actor_player(&self, a: Actor) -> Option<i32>;
    fn player_set_camera(&mut self, slot: i32, target: Option<Actor>);
    fn player_capture_input(&mut self, slot: i32, mask: u32);
}

/// Position-check spawn flag (loot side ammo).
pub const CHECK_POSITION: i32 = spawn_flags::CHECK_POSITION;

/// Forwards to [`room_script_sdk::game`].
#[derive(Default)]
pub struct SdkHost;

impl Host for SdkHost {
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
    fn actor_can_see(&self, a: Actor, b: Actor) -> bool {
        a.can_see(b)
    }
    fn class_find(&self, name: &str) -> ClassRef {
        class_find(name)
    }
    fn player(&self, slot: i32) -> Option<Actor> {
        game::player(slot)
    }
    fn player_stat(&self, slot: i32, prop: i32) -> i32 {
        game::player_stat(slot, prop)
    }
    fn maptime(&self) -> i32 {
        game::level_stat(level_stat::MAPTIME)
    }
    fn set_rule(&mut self, rule: i32, value: i32) {
        game::set_rule(rule, value);
    }
    fn get_rule(&self, rule: i32) -> i32 {
        game::get_rule(rule)
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
    fn actor_pos(&self, a: Actor) -> [f64; 3] {
        a.pos()
    }
    fn actor_destroy(&mut self, a: Actor) {
        a.destroy();
    }
    fn actor_is_monster(&self, a: Actor) -> bool {
        a.is_monster()
    }
    fn actor_take(&mut self, a: Actor, item: ClassRef, amount: i32) -> i32 {
        a.take(item, amount)
    }
    fn class_is(&self, class: ClassRef, parent: ClassRef) -> bool {
        game::class_is(class, parent)
    }
    fn actor_set_pos(&mut self, a: Actor, pos: [f64; 3]) -> bool {
        a.set_pos(pos)
    }
    fn actor_set_vel(&mut self, a: Actor, vel: [f64; 3]) {
        a.set_vel(vel);
    }
    fn actor_set_angle(&mut self, a: Actor, deg: f64) {
        a.set_angle(deg);
    }
    fn actor_sound(&mut self, a: Actor, name: &str, volume: f64) {
        a.sound(name, volume);
    }
    fn actor_damage(
        &mut self,
        target: Actor,
        amount: i32,
        inflictor: Option<Actor>,
        source: Option<Actor>,
    ) -> i32 {
        target.damage(amount, inflictor, source)
    }
    fn actor_is_dead(&self, a: Actor) -> bool {
        a.is_dead()
    }
    fn actor_health(&self, a: Actor) -> i32 {
        a.health()
    }
    fn actor_set_health(&mut self, a: Actor, v: i32) {
        a.set_health(v);
    }
    fn actor_get_f64(&self, a: Actor, prop: i32) -> f64 {
        a.get_f64(prop)
    }
    fn actor_set_f64(&mut self, a: Actor, prop: i32, v: f64) {
        a.set_f64(prop, v);
    }
    fn actor_player(&self, a: Actor) -> Option<i32> {
        a.player()
    }
    fn player_set_camera(&mut self, slot: i32, target: Option<Actor>) {
        game::player_set_camera(slot, target);
    }
    fn player_capture_input(&mut self, slot: i32, mask: u32) {
        game::player_capture_input(slot, mask);
    }
}

#[cfg(test)]
mod fake {
    use super::*;
    use std::string::String;
    use std::vec::Vec;

    #[derive(Clone)]
    struct FakeActor {
        pos: [f64; 3],
        health: i32,
        class: ClassRef,
        player: Option<i32>,
        is_monster: bool,
        alive: bool,
        damage_factor: f64,
        speed: f64,
        buddha: bool,
        inv: [(ClassRef, i32); 8],
        inv_n: u8,
    }

    /// Scripted host for native unit tests.
    pub struct FakeHost {
        pub prints: Vec<(Option<i32>, String)>,
        rng: u32,
        actors: Vec<FakeActor>,
        classes: Vec<(String, ClassRef)>,
        things: Vec<(i32, [f64; 3])>,
        rules: [(i32, i32); 8],
        #[allow(dead_code)]
        next_class: u32,
        pub cameras: Vec<(i32, Option<u64>)>,
    }

    impl FakeHost {
        /// Seeded RNG.
        #[must_use]
        pub fn new(seed: u32) -> Self {
            Self {
                prints: Vec::new(),
                rng: seed | 1,
                actors: Vec::new(),
                classes: Vec::new(),
                things: Vec::new(),
                rules: [(0, 0); 8],
                next_class: 1,
                cameras: Vec::new(),
            }
        }

        #[allow(dead_code)]
        pub fn ensure_class(&mut self, name: &str) -> ClassRef {
            if let Some((_, c)) = self.classes.iter().find(|(n, _)| n == name) {
                return *c;
            }
            let c = ClassRef(self.next_class);
            self.next_class = self.next_class.saturating_add(1);
            self.classes.push((name.into(), c));
            c
        }
    }

    impl Host for FakeHost {
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
            128.0
        }
        fn actor_can_see(&self, _a: Actor, _b: Actor) -> bool {
            true
        }
        fn class_find(&self, name: &str) -> ClassRef {
            self.classes
                .iter()
                .find(|(n, _)| n == name)
                .map(|(_, c)| *c)
                .unwrap_or(ClassRef::NULL)
        }
        fn player(&self, slot: i32) -> Option<Actor> {
            self.actors
                .iter()
                .enumerate()
                .find(|(_, a)| a.alive && a.player == Some(slot))
                .map(|(i, _)| Actor(ActorRef(i as u64 + 1)))
        }
        fn player_stat(&self, _slot: i32, _prop: i32) -> i32 {
            0
        }
        fn maptime(&self) -> i32 {
            0
        }
        fn set_rule(&mut self, rule: i32, value: i32) {
            for slot in &mut self.rules {
                if slot.0 == rule || slot.0 == 0 {
                    *slot = (rule, value);
                    return;
                }
            }
        }
        fn get_rule(&self, rule: i32) -> i32 {
            self.rules
                .iter()
                .find(|&&(r, _)| r == rule)
                .map(|&(_, v)| v)
                .unwrap_or(0)
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
            self.actors.push(FakeActor {
                pos,
                health: 20,
                class,
                player: None,
                is_monster: true,
                alive: true,
                damage_factor: 1.0,
                speed: 1.0,
                buddha: false,
                inv: [(ClassRef::NULL, 0); 8],
                inv_n: 0,
            });
            Some(Actor(ActorRef(id)))
        }
        fn actors_of_class(&self, class: ClassRef, _sub: bool, out: &mut [Actor]) -> usize {
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
        fn actor_pos(&self, a: Actor) -> [f64; 3] {
            let id = a.0.0;
            self.actors
                .get((id as usize).saturating_sub(1))
                .filter(|x| x.alive)
                .map(|x| x.pos)
                .unwrap_or([0.0; 3])
        }
        fn actor_destroy(&mut self, a: Actor) {
            if let Some(f) = self.actors.get_mut((a.0.0 as usize).saturating_sub(1)) {
                f.alive = false;
            }
        }
        fn actor_is_monster(&self, a: Actor) -> bool {
            self.actors
                .get((a.0.0 as usize).saturating_sub(1))
                .is_some_and(|x| x.alive && x.is_monster)
        }
        fn actor_take(&mut self, a: Actor, item: ClassRef, amount: i32) -> i32 {
            let Some(f) = self.actors.get_mut((a.0.0 as usize).saturating_sub(1)) else {
                return 0;
            };
            for slot in &mut f.inv[..usize::from(f.inv_n)] {
                if slot.0 == item {
                    let taken = slot.1.min(amount);
                    slot.1 -= taken;
                    return taken;
                }
            }
            amount
        }
        fn class_is(&self, class: ClassRef, parent: ClassRef) -> bool {
            class == parent
        }
        fn actor_set_pos(&mut self, a: Actor, pos: [f64; 3]) -> bool {
            if let Some(f) = self.actors.get_mut((a.0.0 as usize).saturating_sub(1)) {
                f.pos = pos;
                true
            } else {
                false
            }
        }
        fn actor_set_vel(&mut self, _a: Actor, _vel: [f64; 3]) {}
        fn actor_set_angle(&mut self, _a: Actor, _deg: f64) {}
        fn actor_sound(&mut self, _a: Actor, _name: &str, _volume: f64) {}
        fn actor_damage(
            &mut self,
            target: Actor,
            amount: i32,
            _inf: Option<Actor>,
            _src: Option<Actor>,
        ) -> i32 {
            let Some(f) = self.actors.get_mut((target.0.0 as usize).saturating_sub(1)) else {
                return 0;
            };
            let dealt = amount.min(f.health.max(0));
            f.health -= dealt;
            if f.health <= 0 && f.buddha {
                f.health = 1;
            }
            dealt
        }
        fn actor_is_dead(&self, a: Actor) -> bool {
            self.actors
                .get((a.0.0 as usize).saturating_sub(1))
                .is_none_or(|x| !x.alive || x.health <= 0)
        }
        fn actor_health(&self, a: Actor) -> i32 {
            self.actors
                .get((a.0.0 as usize).saturating_sub(1))
                .map(|x| x.health)
                .unwrap_or(0)
        }
        fn actor_set_health(&mut self, a: Actor, v: i32) {
            if let Some(f) = self.actors.get_mut((a.0.0 as usize).saturating_sub(1)) {
                f.health = v;
            }
        }
        fn actor_get_f64(&self, a: Actor, prop: i32) -> f64 {
            let Some(f) = self.actors.get((a.0.0 as usize).saturating_sub(1)) else {
                return 0.0;
            };
            if prop == actor_f64::DAMAGE_FACTOR {
                f.damage_factor
            } else if prop == actor_f64::SPEED {
                f.speed
            } else {
                0.0
            }
        }
        fn actor_set_f64(&mut self, a: Actor, prop: i32, v: f64) {
            if let Some(f) = self.actors.get_mut((a.0.0 as usize).saturating_sub(1)) {
                if prop == actor_f64::DAMAGE_FACTOR {
                    f.damage_factor = v;
                } else if prop == actor_f64::SPEED {
                    f.speed = v;
                }
            }
        }
        fn actor_player(&self, a: Actor) -> Option<i32> {
            self.actors
                .get((a.0.0 as usize).saturating_sub(1))
                .and_then(|x| x.player)
        }
        fn player_set_camera(&mut self, slot: i32, target: Option<Actor>) {
            self.cameras
                .push((slot, target.map(|a| a.0.0)));
        }
        fn player_capture_input(&mut self, _slot: i32, _mask: u32) {}
    }
}

#[cfg(test)]
pub use fake::FakeHost;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tell;

    #[test]
    fn fake_host_captures_print() {
        let mut h = FakeHost::new(1);
        tell!(h, None, "BATTLE ROYALE - {} players. No guns: loot the map!", 4);
        assert_eq!(h.prints.len(), 1);
        assert_eq!(
            h.prints[0].1,
            "BATTLE ROYALE - 4 players. No guns: loot the map!"
        );
    }
}
