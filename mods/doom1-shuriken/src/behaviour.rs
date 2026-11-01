//! Impact, stick ring, drops, placement (`docs/32` §5).

use alloc::vec::Vec;
use room_script_abi::{class_flag, ActionPayload, SessionKind};
use room_script_sdk::game::{
    actor_flag, actor_set_flag, actors_of_class, atan2, ceiling_z, floor_z, hit_kind, player,
    spawn, spawn_flags, Actor, Event,
};
use room_script_sdk::{class_find, ClassRef};

use crate::fixed;
use crate::tables::STUCK_MAX;

const STUCK_IN_CAP: usize = 64;

#[derive(Clone, Copy)]
struct StuckIn {
    actor: u64,
    count: u8,
}

/// Runtime bookkeeping for stuck stars.
pub struct Behaviour {
    stuck_ring: Vec<u64>,
    stuck_in: [StuckIn; STUCK_IN_CAP],
    stuck_in_len: usize,
    save_load: bool,
    placed: bool,
}

impl Default for Behaviour {
    fn default() -> Self {
        Self {
            stuck_ring: Vec::new(),
            stuck_in: [StuckIn {
                actor: 0,
                count: 0,
            }; STUCK_IN_CAP],
            stuck_in_len: 0,
            save_load: false,
            placed: false,
        }
    }
}

impl Behaviour {
    /// Track whether this session is a save load.
    pub fn init(&mut self, session_kind: i32) {
        self.save_load = session_kind == SessionKind::LoadGame as i32;
        self.placed = false;
        self.stuck_ring.clear();
        self.stuck_in_len = 0;
    }

    /// Missile Death / XDeath WASM action.
    pub fn on_state_action(&mut self, payload: ActionPayload) -> i64 {
        let Some(missile) = Actor::from_bits(payload.self_ as i64) else {
            return 0;
        };
        self.impact(missile);
        0
    }

    /// Event handler.
    pub fn on_event(&mut self, event: Event) {
        match event {
            Event::Loaded { .. } => {
                if !self.save_load {
                    self.place_on_map();
                } else {
                    self.rebuild_ring_from_world();
                }
            }
            Event::ThingDied { thing, .. } => self.on_thing_died(thing),
            Event::ThingDestroyed { thing } => self.on_thing_destroyed(thing),
            Event::Tick { maptime } => {
                if maptime % 35 == 0 {
                    self.prune_ring();
                }
            }
            _ => {}
        }
    }

    fn impact(&mut self, missile: Actor) {
        let (kind, pos, normal) = missile.hit_info();
        match kind {
            x if x == hit_kind::ACTOR => {
                if let Some(victim) = missile.hit_actor() {
                    if actor_flag(victim, class_flag::NOBLOOD as i32)
                        || actor_flag(victim, class_flag::INVULNERABLE as i32)
                    {
                        missile.sound("shuriken/clink", 1.0);
                        self.spawn_dropped(pos);
                    } else {
                        missile.sound("shuriken/flesh", 1.0);
                        self.add_stuck_in(victim);
                    }
                }
            }
            x if x == hit_kind::WALL => {
                missile.sound("shuriken/clink", 1.0);
                let p = [
                    pos[0] - normal[0] * 2.0,
                    pos[1] - normal[1] * 2.0,
                    pos[2] - normal[2] * 2.0,
                ];
                if let Some(a) = self.spawn_stuck(p, true) {
                    actor_set_flag(a, class_flag::WALLSPRITE as i32, true);
                    // Face out of the wall using atan2 degrees.
                    let ang = atan2(normal[1], normal[0]) * 57.295_779_513;
                    a.set_angle(ang);
                }
            }
            x if x == hit_kind::FLOOR => {
                if let Some(a) = self.spawn_stuck(pos, false) {
                    actor_set_flag(a, class_flag::FLATSPRITE as i32, true);
                    actor_set_flag(a, class_flag::NOGRAVITY as i32, false);
                }
            }
            x if x == hit_kind::CEILING => {
                let p = [pos[0], pos[1], pos[2] - 2.0];
                let _ = self.spawn_stuck(p, true);
            }
            x if x == hit_kind::SKY || x == hit_kind::LIQUID => {}
            _ => {}
        }
    }

    fn spawn_stuck(&mut self, pos: [f64; 3], nograv: bool) -> Option<Actor> {
        let class = class_find("ShurikenStuck");
        if class.0 == 0 {
            return None;
        }
        let a = spawn(class, pos, 0.0, spawn_flags::CHECK_POSITION)?;
        if nograv {
            actor_set_flag(a, class_flag::NOGRAVITY as i32, true);
        }
        self.push_ring(a);
        Some(a)
    }

    fn spawn_dropped(&mut self, pos: [f64; 3]) {
        let class = class_find("ShurikenDropped");
        if class.0 == 0 {
            return;
        }
        if let Some(a) = spawn(class, pos, 0.0, spawn_flags::CHECK_POSITION) {
            actor_set_flag(a, class_flag::NOGRAVITY as i32, false);
            self.push_ring(a);
        }
    }

    fn push_ring(&mut self, a: Actor) {
        while self.stuck_ring.len() >= STUCK_MAX {
            if let Some(old) = self.stuck_ring.first().copied() {
                self.stuck_ring.remove(0);
                if let Some(actor) = Actor::from_bits(old as i64) {
                    actor.destroy();
                }
            } else {
                break;
            }
        }
        self.stuck_ring.push(a.bits() as u64);
    }

    fn prune_ring(&mut self) {
        self.stuck_ring.retain(|&bits| {
            Actor::from_bits(bits as i64).is_some_and(Actor::exists)
        });
    }

    fn rebuild_ring_from_world(&mut self) {
        self.stuck_ring.clear();
        let class = class_find("ShurikenStuck");
        if class.0 == 0 {
            return;
        }
        let mut buf = [Actor::NULL; 64];
        let n = actors_of_class(class, true, &mut buf);
        for a in buf.iter().take(n) {
            self.stuck_ring.push(a.bits() as u64);
        }
    }

    fn add_stuck_in(&mut self, victim: Actor) {
        let bits = victim.bits() as u64;
        for i in 0..self.stuck_in_len {
            if self.stuck_in[i].actor == bits {
                self.stuck_in[i].count = self.stuck_in[i].count.saturating_add(1);
                return;
            }
        }
        if self.stuck_in_len >= STUCK_IN_CAP {
            self.stuck_in.copy_within(1..STUCK_IN_CAP, 0);
            self.stuck_in_len = STUCK_IN_CAP - 1;
        }
        self.stuck_in[self.stuck_in_len] = StuckIn {
            actor: bits,
            count: 1,
        };
        self.stuck_in_len += 1;
    }

    fn on_thing_died(&mut self, thing: Actor) {
        let bits = thing.bits() as u64;
        let mut count = 0u8;
        let mut idx = None;
        for i in 0..self.stuck_in_len {
            if self.stuck_in[i].actor == bits {
                count = self.stuck_in[i].count;
                idx = Some(i);
                break;
            }
        }
        if let Some(i) = idx {
            self.stuck_in[i] = self.stuck_in[self.stuck_in_len - 1];
            self.stuck_in_len -= 1;
        }
        if count == 0 {
            return;
        }
        let pos = thing.pos();
        for i in 0..count {
            let ang = (i as u8).wrapping_mul(32);
            let ox = fixed::mul(fixed::cos(ang), 8 << 16) >> 16;
            let oy = fixed::mul(fixed::sin(ang), 8 << 16) >> 16;
            let p = [pos[0] + f64::from(ox), pos[1] + f64::from(oy), pos[2]];
            self.spawn_dropped(p);
        }
    }

    fn on_thing_destroyed(&mut self, thing: Actor) {
        let bits = thing.bits() as u64;
        self.stuck_ring.retain(|&b| b != bits);
        for i in 0..self.stuck_in_len {
            if self.stuck_in[i].actor == bits {
                self.stuck_in[i] = self.stuck_in[self.stuck_in_len - 1];
                self.stuck_in_len -= 1;
                break;
            }
        }
    }

    fn place_on_map(&mut self) {
        if self.placed {
            return;
        }
        self.placed = true;
        let place_weapon = room_script_sdk::reg::option_i32("place_weapon", 1) != 0;
        let replace_pct =
            room_script_sdk::reg::option_i32("ammo_replace_percent", 25).clamp(0, 100);

        if place_weapon {
            self.try_place_weapon();
        }
        if replace_pct > 0 {
            self.replace_clips(replace_pct);
        }
    }

    fn try_place_weapon(&self) {
        let weapon = class_find("Shuriken");
        let Some(mo) = player(0) else {
            return;
        };
        let ppos = mo.pos();
        let mut buf = [Actor::NULL; 128];
        let n = actors_of_class(ClassRef::NULL, false, &mut buf);
        for a in buf.iter().take(n) {
            if !a.exists() {
                continue;
            }
            let pos = a.pos();
            let dx = pos[0] - ppos[0];
            let dy = pos[1] - ppos[1];
            if dx * dx + dy * dy < 512.0 * 512.0 {
                continue;
            }
            let floor = floor_z(pos[0], pos[1]);
            let ceil = ceiling_z(pos[0], pos[1]);
            if ceil - floor < 56.0 {
                continue;
            }
            let _ = spawn(
                weapon,
                [pos[0], pos[1], floor],
                0.0,
                spawn_flags::CHECK_POSITION,
            );
            return;
        }
    }

    fn replace_clips(&self, percent: i32) {
        let clip = class_find("Clip");
        let box_c = class_find("ShurikenBox");
        if clip.0 == 0 || box_c.0 == 0 {
            return;
        }
        let mut buf = [Actor::NULL; 256];
        let n = actors_of_class(clip, false, &mut buf);
        for (i, a) in buf.iter().take(n).enumerate() {
            if !a.exists() {
                continue;
            }
            if percent < 100 && ((i as i32 * 100) % 100) >= percent {
                continue;
            }
            let pos = a.pos();
            a.destroy();
            let _ = spawn(box_c, pos, 0.0, 0);
        }
    }
}
