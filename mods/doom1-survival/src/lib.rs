//! Survival Director — example Room WASM mod for `doom1.wad` (any Doom map works).
//!
//! * Waves of monsters teleport in at the map's own thing spots, out of the
//!   player's way, and hunt the player. Wave timing, size and the monster pool
//!   scale with the skill level (ITYTD … Nightmare).
//! * Every fifth wave is a boss wave (Baron of Hell + the boss music).
//! * Two mod classes: `PhantomImp` (fast, translucent imp) and `Juggernaut`
//!   (big, tough demon).
//! * Kills heal the player a little (more on easy skills) and chain into
//!   kill-streak messages; clearing a wave hands out ammo.
#![cfg_attr(target_arch = "wasm32", no_std)]

use room_script_sdk::game::{
    self, Actor, Event, RoomMod, actor_ptr, class_prop, event_mask, spawn_flags,
};
use room_script_sdk::{ClassRef, class_find, reg};

#[cfg(target_arch = "wasm32")]
#[panic_handler]
fn panic(_: &core::panic::PanicInfo<'_>) -> ! {
    core::arch::wasm32::unreachable()
}

const TICRATE: i32 = 35;
const MAX_SPOTS: usize = 128;
const MAX_WAVE: usize = 24;
/// Kills closer together than this chain into a streak.
const STREAK_TICS: i32 = 3 * TICRATE;
/// Spawn spots closer than this to the player are skipped.
const MIN_SPAWN_DIST: f64 = 320.0;

/// Fixed-size text buffer for HUD messages (no allocator in this module).
struct Msg {
    buf: [u8; 96],
    len: usize,
}

impl Msg {
    fn new() -> Self {
        Self {
            buf: [0; 96],
            len: 0,
        }
    }

    fn as_str(&self) -> &str {
        core::str::from_utf8(&self.buf[..self.len]).unwrap_or("")
    }
}

impl core::fmt::Write for Msg {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        let n = s.len().min(self.buf.len() - self.len);
        self.buf[self.len..self.len + n].copy_from_slice(&s.as_bytes()[..n]);
        self.len += n;
        Ok(())
    }
}

macro_rules! say {
    ($($arg:tt)*) => {{
        let mut m = Msg::new();
        let _ = core::fmt::Write::write_fmt(&mut m, format_args!($($arg)*));
        game::print(None, m.as_str());
    }};
}

/// Per-skill tuning (index = skill − 1).
struct Tuning {
    /// Seconds between waves.
    wave_gap: i32,
    /// Extra monsters per wave.
    extra: i32,
    /// Health restored per kill.
    heal: i32,
}

const TUNING: [Tuning; 5] = [
    Tuning {
        wave_gap: 60,
        extra: 0,
        heal: 5,
    },
    Tuning {
        wave_gap: 50,
        extra: 1,
        heal: 4,
    },
    Tuning {
        wave_gap: 40,
        extra: 2,
        heal: 3,
    },
    Tuning {
        wave_gap: 32,
        extra: 3,
        heal: 2,
    },
    Tuning {
        wave_gap: 25,
        extra: 5,
        heal: 1,
    },
];

/// Monster pool, unlocked one entry per wave (all present in shareware Doom).
const POOL: [&str; 6] = [
    "ZombieMan",
    "ShotgunGuy",
    "PhantomImp",
    "Demon",
    "Spectre",
    "Juggernaut",
];

struct Survival {
    spots: [[f64; 3]; MAX_SPOTS],
    nspots: usize,
    pool: [ClassRef; POOL.len()],
    baron: ClassRef,
    clip: ClassRef,
    shell: ClassRef,
    wave: i32,
    next_wave_at: i32,
    alive: [Actor; MAX_WAVE],
    nalive: usize,
    streak: i32,
    last_kill: i32,
    warned_low_at: i32,
    music: [u8; 8],
    music_len: usize,
}

impl Default for Survival {
    fn default() -> Self {
        Self {
            spots: [[0.0; 3]; MAX_SPOTS],
            nspots: 0,
            pool: [ClassRef::NULL; POOL.len()],
            baron: ClassRef::NULL,
            clip: ClassRef::NULL,
            shell: ClassRef::NULL,
            wave: 0,
            next_wave_at: 0,
            alive: [Actor::NULL; MAX_WAVE],
            nalive: 0,
            streak: 0,
            last_kill: -STREAK_TICS,
            warned_low_at: -10 * TICRATE,
            music: [0; 8],
            music_len: 0,
        }
    }
}

fn tuning() -> &'static Tuning {
    let i = usize::try_from(game::skill() - 1).unwrap_or(0).min(4);
    &TUNING[i]
}

fn register_class(name: &str, parent: &str, props: &[(u32, i32)]) {
    let parent = class_find(parent);
    let _ = reg::class_begin(name, parent, ClassRef::NULL, -1, 0);
    for &(p, v) in props {
        reg::class_property_i32(p, v);
    }
    reg::class_end();
}

impl Survival {
    /// Remember where the map put its things: always reachable floor space.
    fn collect_spots(&mut self) {
        let mut all = [Actor::NULL; 512];
        let n = game::actors_of_class(ClassRef::NULL, false, &mut all).min(all.len());
        self.nspots = 0;
        for &a in &all[..n] {
            if self.nspots == MAX_SPOTS {
                break;
            }
            if a.player().is_some() || a.get_i32(game::actor_i32::IS_MISSILE) != 0 {
                continue;
            }
            let p = a.pos();
            self.spots[self.nspots] = [p[0], p[1], game::floor_z(p[0], p[1])];
            self.nspots += 1;
        }
    }

    fn wave_size(&self) -> i32 {
        (2 + self.wave + tuning().extra).min(MAX_WAVE as i32)
    }

    fn spawn_wave(&mut self) {
        let Some(player) = game::player(0) else {
            return;
        };
        if self.nspots == 0 || player.is_dead() {
            return;
        }
        self.wave += 1;
        self.nalive = 0;
        let boss = self.wave % 5 == 0;
        let unlocked = (self.wave as usize).min(POOL.len());
        let mut want = self.wave_size();
        if boss {
            if self.spawn_one(self.baron, player) {
                want -= 2;
            }
            if self.music_len > 0 {
                game::set_music("D_E1M8");
            }
            say!("Wave {} - a Baron approaches!", self.wave);
        } else {
            say!("Wave {} - {} incoming", self.wave, want);
        }
        let mut tries = 0;
        while want > 0 && tries < 64 {
            tries += 1;
            let pick = game::random(0, unlocked as i32 - 1) as usize;
            if self.spawn_one(self.pool[pick], player) {
                want -= 1;
            }
        }
    }

    fn spawn_one(&mut self, class: ClassRef, player: Actor) -> bool {
        if class.is_null() || self.nalive == MAX_WAVE {
            return false;
        }
        let pp = player.pos();
        for _ in 0..8 {
            let s = self.spots[game::random(0, self.nspots as i32 - 1) as usize];
            let (dx, dy) = (s[0] - pp[0], s[1] - pp[1]);
            if game::sqrt(dx * dx + dy * dy) < MIN_SPAWN_DIST {
                continue;
            }
            let face = game::atan2(-dy, -dx).to_degrees();
            let flags = spawn_flags::TELEFOG | spawn_flags::CHECK_POSITION;
            if let Some(m) = game::spawn(class, s, face, flags) {
                m.set_ref(actor_ptr::TARGET, Some(player));
                m.set_state("See");
                self.alive[self.nalive] = m;
                self.nalive += 1;
                return true;
            }
        }
        false
    }

    fn wave_killed(&mut self, thing: Actor) -> bool {
        let Some(i) = self.alive[..self.nalive].iter().position(|&a| a == thing) else {
            return false;
        };
        self.nalive -= 1;
        self.alive[i] = self.alive[self.nalive];
        true
    }

    fn on_kill(&mut self, thing: Actor, source: Option<Actor>) {
        let in_wave = self.wave_killed(thing);
        let Some(killer) = source.filter(|s| s.player().is_some()) else {
            return;
        };
        if !thing.is_monster() {
            return;
        }
        let now = game::level_stat(game::level_stat::MAPTIME);
        self.streak = if now - self.last_kill <= STREAK_TICS {
            self.streak + 1
        } else {
            1
        };
        self.last_kill = now;
        match self.streak {
            2 => say!("Double kill!"),
            3 => say!("Multi kill!"),
            5 => say!("Rampage!"),
            8 => say!("Unstoppable!"),
            _ => {}
        }
        let hp = killer.health();
        if hp > 0 && hp < 100 {
            killer.set_health((hp + tuning().heal).min(100));
        }
        if in_wave && self.nalive == 0 && self.wave > 0 {
            say!("Wave {} cleared!", self.wave);
            let _ = killer.give(self.clip, 20);
            let _ = killer.give(self.shell, 4 + self.wave);
            if self.wave % 5 == 0 && self.music_len > 0 {
                let name = core::str::from_utf8(&self.music[..self.music_len]).unwrap_or("");
                game::set_music(name);
            }
            let soon = now + 10 * TICRATE;
            self.next_wave_at = self.next_wave_at.min(soon);
        }
    }

    fn on_loaded(&mut self) {
        for (slot, name) in self.pool.iter_mut().zip(POOL) {
            *slot = class_find(name);
        }
        self.baron = class_find("BaronOfHell");
        self.clip = class_find("Clip");
        self.shell = class_find("Shell");
        self.wave = 0;
        self.nalive = 0;
        self.streak = 0;
        self.collect_spots();
        let mut buf = [0u8; 8];
        let map = game::map_name(&mut buf);
        self.music_len = 0;
        if map.len() == 4 && map.starts_with('E') {
            self.music[..2].copy_from_slice(b"D_");
            self.music[2..6].copy_from_slice(map.as_bytes());
            self.music_len = 6;
        }
        let gap = tuning().wave_gap;
        self.next_wave_at = gap * TICRATE / 2;
        say!(
            "Survival Director: skill {} - first wave in {}s",
            game::skill(),
            gap / 2
        );
    }

    fn on_tick(&mut self, maptime: i32) {
        let left = self.next_wave_at - maptime;
        if left == 5 * TICRATE {
            say!("Next wave in 5...");
        }
        if left <= 0 {
            self.spawn_wave();
            self.next_wave_at = maptime + tuning().wave_gap * TICRATE;
        }
        // Wave monsters that never woke up: keep them hunting.
        if maptime % TICRATE == 0
            && let Some(player) = game::player(0)
        {
            for &m in &self.alive[..self.nalive] {
                if m.exists() && m.target().is_none() {
                    m.set_ref(actor_ptr::TARGET, Some(player));
                }
            }
        }
    }
}

impl RoomMod for Survival {
    const NAME: &'static str = "SurvivalDirector";
    const EVENTS: u64 = event_mask::WORLD_LOADED
        | event_mask::WORLD_TICK
        | event_mask::WORLD_THING_DIED
        | event_mask::WORLD_THING_DAMAGED
        | event_mask::PLAYER_DIED;

    fn register(&mut self) {
        register_class(
            "PhantomImp",
            "DoomImp",
            &[
                (class_prop::HEALTH, 80),
                (class_prop::SPEED, 12),
                (class_prop::ALPHA_PERCENT, 55),
            ],
        );
        register_class(
            "Juggernaut",
            "Demon",
            &[
                (class_prop::HEALTH, 400),
                (class_prop::SPEED, 12),
                (class_prop::SCALE_PERCENT, 130),
                (class_prop::RADIUS, 36),
                (class_prop::HEIGHT, 72),
                (class_prop::MASS, 1000),
            ],
        );
    }

    fn on_event(&mut self, event: Event) {
        match event {
            Event::Loaded { .. } => self.on_loaded(),
            Event::Tick { maptime } => self.on_tick(maptime),
            Event::ThingDied { thing, source } => self.on_kill(thing, source),
            Event::ThingDamaged { thing, health, .. } => {
                let now = game::level_stat(game::level_stat::MAPTIME);
                if thing.player().is_some()
                    && health > 0
                    && health < 25
                    && now - self.warned_low_at > 10 * TICRATE
                {
                    self.warned_low_at = now;
                    say!("Low health - keep moving!");
                }
            }
            Event::PlayerDied { .. } => {
                say!("You survived {} waves", self.wave.saturating_sub(1).max(0));
            }
            _ => {}
        }
    }
}

room_script_sdk::export_mod!(Survival);
