//! Battle Royale — example Room WASM mod for `doom1.wad` multiplayer rooms
//! (any Doom map works).
//!
//! * No monsters: every monster is removed when the map starts and its spot
//!   becomes a loot drop (weapons, ammo, health, armor).
//! * Players start with fists only, spread over the map's deathmatch starts
//!   (monster spots when a map has too few), and can hurt each other
//!   (deathmatch rules). Dead players stay down: one life each.
//! * Guns are placed, not sprinkled: everyone gets a shotgun at the nearest
//!   drop to their start, the strongest guns sit near the zone centre, guns
//!   keep their distance from each other and always come with ammo.
//! * After a grace period a zone closes in on a point of the map; standing
//!   outside it hurts, more with every phase.
//! * The last player standing wins (or the best survivor when the room's time
//!   limit runs out); the match then ends and every seat sees the standings.
#![cfg_attr(target_arch = "wasm32", no_std)]

use room_script_sdk::game::{self, Actor, Event, RoomMod, event_mask, rule, thing_type};
use room_script_sdk::{ClassRef, class_find};

#[cfg(target_arch = "wasm32")]
#[panic_handler]
fn panic(_: &core::panic::PanicInfo<'_>) -> ! {
    core::arch::wasm32::unreachable()
}

const TICRATE: i32 = 35;
const MAX_PLAYERS: usize = 8;
const MAX_SPOTS: usize = 256;
/// Seconds before the zone starts closing.
const GRACE_SECS: i32 = 45;
/// Seconds a zone phase holds before shrinking, then seconds it shrinks.
const HOLD_SECS: i32 = 25;
const SHRINK_SECS: i32 = 20;
/// Each phase keeps this share of the previous radius (percent).
const SHRINK_PERCENT: f64 = 0.6;
const MIN_RADIUS: f64 = 160.0;
/// Seconds between the last elimination and the final screen.
const END_SECS: i32 = 3;
/// Loot never drops this close to where a player was placed.
const LOOT_CLEARANCE: f64 = 96.0;
/// Two guns never drop closer than this; the later one becomes its ammo.
const GUN_SPACING: f64 = 384.0;
/// Health for a frag.
const FRAG_HEAL: i32 = 25;

/// Every item the mod drops (all present in shareware Doom).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
enum Item {
    Shotgun,
    Chaingun,
    RocketLauncher,
    Chainsaw,
    Shell,
    ShellBox,
    Clip,
    ClipBox,
    RocketAmmo,
    Stimpack,
    Medikit,
    GreenArmor,
    ArmorBonus,
    HealthBonus,
    Berserk,
}

const ITEM_CLASSES: [&str; 15] = [
    "Shotgun",
    "Chaingun",
    "RocketLauncher",
    "Chainsaw",
    "Shell",
    "ShellBox",
    "Clip",
    "ClipBox",
    "RocketAmmo",
    "Stimpack",
    "Medikit",
    "GreenArmor",
    "ArmorBonus",
    "HealthBonus",
    "Berserk",
];

impl Item {
    /// Ammo dropped next to a gun (and dropped instead of a gun that is too
    /// close to another one or over its cap).
    fn ammo(self) -> Option<Item> {
        match self {
            Item::Shotgun => Some(Item::Shell),
            Item::Chaingun => Some(Item::Clip),
            Item::RocketLauncher => Some(Item::RocketAmmo),
            _ => None,
        }
    }

    fn is_gun(self) -> bool {
        matches!(
            self,
            Item::Shotgun | Item::Chaingun | Item::RocketLauncher | Item::Chainsaw
        )
    }
}

/// Loot tables by distance to the zone centre: (item, weight).
static INNER: [(Item, i32); 9] = [
    (Item::RocketLauncher, 3),
    (Item::Chaingun, 6),
    (Item::Shotgun, 3),
    (Item::ShellBox, 4),
    (Item::ClipBox, 3),
    (Item::RocketAmmo, 5),
    (Item::Medikit, 5),
    (Item::GreenArmor, 4),
    (Item::Berserk, 2),
];
static MIDDLE: [(Item, i32); 10] = [
    (Item::Shotgun, 7),
    (Item::Chaingun, 4),
    (Item::Shell, 8),
    (Item::Clip, 6),
    (Item::ClipBox, 3),
    (Item::RocketAmmo, 2),
    (Item::Stimpack, 6),
    (Item::Medikit, 3),
    (Item::ArmorBonus, 4),
    (Item::GreenArmor, 2),
];
static OUTER: [(Item, i32); 7] = [
    (Item::Shotgun, 6),
    (Item::Chainsaw, 4),
    (Item::Shell, 10),
    (Item::Clip, 10),
    (Item::Stimpack, 8),
    (Item::ArmorBonus, 6),
    (Item::HealthBonus, 6),
];

/// Fixed-size text buffer for HUD messages (no allocator in this module).
struct Msg {
    buf: [u8; 128],
    len: usize,
}

impl Msg {
    fn new() -> Self {
        Self {
            buf: [0; 128],
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

/// HUD message: `tell!(None, ..)` for everyone, `tell!(Some(slot), ..)` for one player.
macro_rules! tell {
    ($to:expr, $($arg:tt)*) => {{
        let mut m = Msg::new();
        let _ = core::fmt::Write::write_fmt(&mut m, format_args!($($arg)*));
        game::print($to, m.as_str());
    }};
}

fn dist2(a: [f64; 2], b: [f64; 2]) -> f64 {
    let (dx, dy) = (a[0] - b[0], a[1] - b[1]);
    dx * dx + dy * dy
}

fn xy(s: [f64; 3]) -> [f64; 2] {
    [s[0], s[1]]
}

/// Compass word for the direction from `from` to `to` (Doom: +x east, +y north).
fn compass(from: [f64; 2], to: [f64; 2]) -> &'static str {
    const DIRS: [&str; 8] = ["E", "NE", "N", "NW", "W", "SW", "S", "SE"];
    let deg = game::atan2(to[1] - from[1], to[0] - from[0]).to_degrees();
    let octant = ((deg + 360.0 + 22.5) / 45.0) as usize % 8;
    DIRS[octant]
}

/// Zone radius at `t` tics: `(radius, phase, shrinking)`; phase 0 = grace.
fn zone_at(r0: f64, t: i32) -> (f64, i32, bool) {
    let grace = GRACE_SECS * TICRATE;
    if t < grace {
        return (r0, 0, false);
    }
    let (hold, shrink) = (HOLD_SECS * TICRATE, SHRINK_SECS * TICRATE);
    let k = (t - grace) / (hold + shrink);
    let into = (t - grace) % (hold + shrink);
    let mut from = r0;
    for _ in 0..k {
        from = (from * SHRINK_PERCENT).max(MIN_RADIUS);
        if from <= MIN_RADIUS {
            break;
        }
    }
    let to = (from * SHRINK_PERCENT).max(MIN_RADIUS);
    if into < hold {
        (from, k + 1, false)
    } else {
        let f = f64::from(into - hold) / f64::from(shrink);
        (from + (to - from) * f, k + 1, true)
    }
}

/// Zone damage per second in `phase` (1..).
fn zone_damage(phase: i32) -> i32 {
    (1 + 2 * phase).min(25)
}

/// Weighted pick from `table` with `roll` in `0..sum(weights)`.
fn pick(table: &[(Item, i32)], mut roll: i32) -> Item {
    for &(item, w) in table {
        if roll < w {
            return item;
        }
        roll -= w;
    }
    table[0].0
}

fn total(table: &[(Item, i32)]) -> i32 {
    table.iter().map(|&(_, w)| w).sum()
}

/// Loot table for the spot ranked `rank` of `n` by distance to the centre.
fn tier(rank: usize, n: usize) -> &'static [(Item, i32)] {
    if rank * 4 < n {
        &INNER
    } else if rank * 3 < n * 2 {
        &MIDDLE
    } else {
        &OUTER
    }
}

struct Royale {
    /// Loot / fallback spawn spots (monster spots, then unused player starts).
    spots: [[f64; 3]; MAX_SPOTS],
    nspots: usize,
    /// Deathmatch starts: preferred player spawns.
    dm: [[f64; 3]; 32],
    ndm: usize,
    dm_taken: [bool; 32],
    /// Spot taken by a placed player.
    taken: [bool; MAX_SPOTS],
    items: [ClassRef; ITEM_CLASSES.len()],
    pistol: ClassRef,
    clip: ClassRef,
    fog: ClassRef,
    placed: [[f64; 2]; MAX_PLAYERS],
    nplaced: usize,
    looted: bool,
    /// Player was in the map when the round started.
    contender: [bool; MAX_PLAYERS],
    out: [bool; MAX_PLAYERS],
    zone_hit: [i32; MAX_PLAYERS],
    hinted: [i32; MAX_PLAYERS],
    center: [f64; 2],
    r0: f64,
    phase: i32,
    shrinking: bool,
    /// Decided: `end_at` is when the final screen shows.
    over: bool,
    winner: Option<usize>,
    end_at: i32,
    ended: bool,
}

impl Default for Royale {
    fn default() -> Self {
        Self {
            spots: [[0.0; 3]; MAX_SPOTS],
            nspots: 0,
            dm: [[0.0; 3]; 32],
            ndm: 0,
            dm_taken: [false; 32],
            taken: [false; MAX_SPOTS],
            items: [ClassRef::NULL; ITEM_CLASSES.len()],
            pistol: ClassRef::NULL,
            clip: ClassRef::NULL,
            fog: ClassRef::NULL,
            placed: [[0.0; 2]; MAX_PLAYERS],
            nplaced: 0,
            looted: false,
            contender: [false; MAX_PLAYERS],
            out: [false; MAX_PLAYERS],
            zone_hit: [i32::MIN / 2; MAX_PLAYERS],
            hinted: [i32::MIN / 2; MAX_PLAYERS],
            center: [0.0; 2],
            r0: 0.0,
            phase: 0,
            shrinking: false,
            over: false,
            winner: None,
            end_at: 0,
            ended: false,
        }
    }
}

impl Royale {
    fn contenders(&self) -> usize {
        self.contender.iter().filter(|&&c| c).count()
    }

    fn alive(&self) -> usize {
        (0..MAX_PLAYERS)
            .filter(|&s| self.contender[s] && !self.out[s])
            .count()
    }

    fn push_spot(&mut self, x: f64, y: f64) {
        if self.nspots < MAX_SPOTS {
            self.spots[self.nspots] = [x, y, game::floor_z(x, y)];
            self.nspots += 1;
        }
    }

    /// Remove every monster; its spot becomes a loot drop / fallback spawn.
    fn clear_monsters(&mut self) {
        let mut all = [Actor::NULL; 4096];
        let n = game::actors_of_class(ClassRef::NULL, false, &mut all).min(all.len());
        for &a in &all[..n] {
            if !a.is_monster() {
                continue;
            }
            let p = a.pos();
            self.push_spot(p[0], p[1]);
            a.destroy();
        }
    }

    /// Deathmatch starts for spawning; co-op starts become loot spots.
    fn read_starts(&mut self) {
        let n = game::level_things(thing_type::DEATHMATCH_START, &mut self.dm);
        self.ndm = n.min(self.dm.len());
        for s in &mut self.dm[..self.ndm] {
            s[2] = game::floor_z(s[0], s[1]);
        }
        for k in 0..4 {
            let mut one = [[0.0; 3]; 1];
            if game::level_things(thing_type::PLAYER1_START + k, &mut one) > 0 {
                self.push_spot(one[0][0], one[0][1]);
            }
        }
    }

    /// Zone centre: a random one of the spots nearest the middle of the map.
    fn pick_zone(&mut self) {
        let Some(first) = game::player(0) else {
            return;
        };
        if self.nspots == 0 {
            let p = first.pos();
            self.center = [p[0], p[1]];
            self.r0 = 2048.0;
            return;
        }
        let n = self.nspots as f64;
        let mid = self.spots[..self.nspots]
            .iter()
            .fold([0.0, 0.0], |m, s| [m[0] + s[0] / n, m[1] + s[1] / n]);
        let mut near = [usize::MAX; 4];
        for i in 0..self.nspots {
            let d = dist2(xy(self.spots[i]), mid);
            for k in 0..near.len() {
                let worse = near[k] == usize::MAX || d < dist2(xy(self.spots[near[k]]), mid);
                if worse {
                    near[k..].rotate_right(1);
                    near[k] = i;
                    break;
                }
            }
        }
        let count = near.iter().filter(|&&i| i != usize::MAX).count() as i32;
        let pick = near[game::random(0, count - 1) as usize];
        self.center = xy(self.spots[pick]);
        let far = self.spots[..self.nspots]
            .iter()
            .chain(&self.dm[..self.ndm])
            .map(|s| dist2(xy(*s), self.center))
            .fold(0.0, f64::max);
        self.r0 = game::sqrt(far) + 256.0;
    }

    fn on_loaded(&mut self) {
        game::set_rule(rule::DEATHMATCH, 1);
        game::set_rule(rule::RESPAWN, 0);
        for (slot, name) in self.items.iter_mut().zip(ITEM_CLASSES) {
            *slot = class_find(name);
        }
        self.pistol = class_find("Pistol");
        self.clip = class_find("Clip");
        self.fog = class_find("TeleportFog");
        self.clear_monsters();
        self.read_starts();
        self.pick_zone();
        for s in 0..MAX_PLAYERS {
            self.contender[s] = game::player(s as i32).is_some();
        }
        let n = self.contenders();
        if n >= 2 {
            tell!(
                None,
                "BATTLE ROYALE - {} players. No guns: loot the map!",
                n
            );
            tell!(None, "Last one standing wins.");
        } else {
            tell!(None, "Battle Royale practice: 2+ players to win");
        }
        tell!(None, "The zone closes in {}s", GRACE_SECS);
    }

    /// Farthest free spot from the players placed so far (random first pick).
    fn farthest(&self, pool: &[[f64; 3]], taken: &[bool]) -> Option<usize> {
        if pool.is_empty() {
            return None;
        }
        let start = game::random(0, pool.len() as i32 - 1) as usize;
        let mut best = None;
        let mut best_d = -1.0;
        for k in 0..pool.len() {
            let i = (start + k) % pool.len();
            if taken[i] {
                continue;
            }
            if self.nplaced == 0 {
                return Some(i);
            }
            let d = self.placed[..self.nplaced]
                .iter()
                .map(|&p| dist2(p, xy(pool[i])))
                .fold(f64::MAX, f64::min);
            if d > best_d {
                best_d = d;
                best = Some(i);
            }
        }
        best
    }

    /// Fists only, then teleport to the free deathmatch start (else monster
    /// spot) farthest from the other players.
    fn on_entered(&mut self, body: Actor) {
        let _ = body.take(self.pistol, 1);
        let _ = body.take(self.clip, 999);
        let spot = if let Some(i) = self.farthest(&self.dm[..self.ndm], &self.dm_taken) {
            self.dm_taken[i] = true;
            self.dm[i]
        } else if let Some(i) = self.farthest(&self.spots[..self.nspots], &self.taken) {
            self.taken[i] = true;
            self.spots[i]
        } else {
            return;
        };
        let face = game::atan2(self.center[1] - spot[1], self.center[0] - spot[0]).to_degrees();
        body.set_pos(spot);
        body.set_vel([0.0; 3]);
        body.set_angle(face);
        if let Some(fog) = game::spawn(self.fog, spot, face, 0) {
            fog.sound("misc/teleport", 1.0);
        }
        if self.nplaced < MAX_PLAYERS {
            self.placed[self.nplaced] = xy(spot);
            self.nplaced += 1;
        }
    }

    fn spawn_item(&self, item: Item, at: [f64; 3]) {
        let class = self.items[item as usize];
        if !class.is_null() {
            let _ = game::spawn(class, at, 0.0, 0);
        }
    }

    /// Drop a gun with its ammo beside it (same spot when there is no room).
    fn spawn_gun(&self, gun: Item, at: [f64; 3]) {
        self.spawn_item(gun, at);
        if let Some(ammo) = gun.ammo() {
            let class = self.items[ammo as usize];
            let side = [at[0] + 24.0, at[1], at[2]];
            let fits = !class.is_null()
                && game::spawn(class, side, 0.0, game::spawn_flags::CHECK_POSITION).is_some();
            if !fits {
                self.spawn_item(ammo, at);
            }
        }
    }

    /// Place the loot. Guns: a shotgun nearest every player's start, a rocket
    /// launcher at the drop nearest the zone centre, then weighted tables by
    /// ring (strong inside, light outside) with spacing and per-gun caps.
    fn drop_loot(&mut self) {
        let mut order = [0usize; MAX_SPOTS];
        let mut n = 0;
        for i in 0..self.nspots {
            let s = xy(self.spots[i]);
            let near_player = self.placed[..self.nplaced]
                .iter()
                .any(|&p| dist2(p, s) < LOOT_CLEARANCE * LOOT_CLEARANCE);
            if !self.taken[i] && !near_player {
                order[n] = i;
                n += 1;
            }
        }
        if n == 0 {
            return;
        }
        let center = self.center;
        let spots = &self.spots;
        order[..n].sort_unstable_by(|&a, &b| {
            dist2(xy(spots[a]), center)
                .total_cmp(&dist2(xy(spots[b]), center))
                .then(a.cmp(&b))
        });
        // Guns the map already has count toward spacing and caps.
        let mut guns = [[0.0; 2]; MAX_SPOTS + 64];
        let mut nguns = 0;
        let mut rockets = 0;
        let mut chainguns = 0;
        for gun in [
            Item::Shotgun,
            Item::Chaingun,
            Item::RocketLauncher,
            Item::Chainsaw,
        ] {
            let mut found = [Actor::NULL; 64];
            let k = game::actors_of_class(self.items[gun as usize], false, &mut found)
                .min(found.len());
            for a in &found[..k] {
                let p = a.pos();
                guns[nguns] = [p[0], p[1]];
                nguns += 1;
            }
            match gun {
                Item::RocketLauncher => rockets += k,
                Item::Chaingun => chainguns += k,
                _ => {}
            }
        }
        // Per ranked drop: assigned item.
        let mut plan: [Option<Item>; MAX_SPOTS] = [None; MAX_SPOTS];
        if rockets == 0 {
            plan[0] = Some(Item::RocketLauncher);
            rockets = 1;
        }
        for p in 0..self.nplaced {
            let mut best = None;
            let mut best_d = f64::MAX;
            for (r, &i) in order[..n].iter().enumerate() {
                let d = dist2(self.placed[p], xy(self.spots[i]));
                if plan[r].is_none() && d < best_d {
                    best_d = d;
                    best = Some(r);
                }
            }
            if let Some(r) = best {
                plan[r] = Some(Item::Shotgun);
            }
        }
        let players = self.contenders().max(1);
        for r in 0..n {
            let at = self.spots[order[r]];
            let (item, forced) = match plan[r] {
                Some(item) => (item, true),
                None => {
                    let table = tier(r, n);
                    (pick(table, game::random(0, total(table) - 1)), false)
                }
            };
            let mut item = item;
            if item.is_gun() && !forced {
                let crowded = guns[..nguns]
                    .iter()
                    .any(|&g| dist2(g, xy(at)) < GUN_SPACING * GUN_SPACING);
                let capped = match item {
                    Item::RocketLauncher => rockets >= players.div_ceil(2),
                    Item::Chaingun => chainguns >= players,
                    _ => false,
                };
                if crowded || capped {
                    item = item.ammo().unwrap_or(Item::Stimpack);
                }
            }
            match item {
                Item::RocketLauncher if !forced => rockets += 1,
                Item::Chaingun => chainguns += 1,
                _ => {}
            }
            if item.is_gun() && nguns < guns.len() {
                guns[nguns] = xy(at);
                nguns += 1;
                self.spawn_gun(item, at);
            } else {
                self.spawn_item(item, at);
            }
        }
    }

    /// Announce and schedule the final screen (`winner` = last standing).
    fn decide(&mut self, winner: Option<usize>, t: i32) {
        if self.over {
            return;
        }
        self.over = true;
        self.winner = winner;
        self.end_at = t + END_SECS * TICRATE;
        match winner {
            Some(w) => {
                tell!(None, "PLAYER {} WINS THE BATTLE ROYALE!", w + 1);
                tell!(Some(w as i32), "Winner! Last one standing.");
            }
            None if self.contenders() < 2 => tell!(None, "Practice over"),
            None => tell!(None, "No survivors - it's a draw!"),
        }
    }

    /// Time is up: the survivor with the most frags (then health) wins.
    fn time_up(&mut self, t: i32) {
        let mut best: Option<(usize, i32, i32)> = None;
        let mut tie = false;
        for s in 0..MAX_PLAYERS {
            if !self.contender[s] || self.out[s] {
                continue;
            }
            let Some(body) = game::player(s as i32) else {
                continue;
            };
            let key = (
                game::player_stat(s as i32, game::player_prop::FRAGS),
                body.health(),
            );
            match best {
                Some((_, f, h)) if (key.0, key.1) == (f, h) => tie = true,
                Some((_, f, h)) if (key.0, key.1) < (f, h) => {}
                _ => {
                    best = Some((s, key.0, key.1));
                    tie = false;
                }
            }
        }
        tell!(None, "TIME!");
        let winner = best.filter(|_| !tie && self.contenders() >= 2).map(|b| b.0);
        self.decide(winner, t);
        self.end_at = t;
    }

    fn on_tick(&mut self, t: i32) {
        if !self.looted {
            self.looted = true;
            self.drop_loot();
        }
        if self.over {
            if !self.ended && t >= self.end_at {
                self.ended = true;
                game::end_match(self.winner.map(|w| w as i32));
            }
            return;
        }
        let left = game::get_rule(rule::TIME_LEFT);
        if left == 1 {
            self.time_up(t);
            if !self.ended {
                self.ended = true;
                game::end_match(self.winner.map(|w| w as i32));
            }
            return;
        }
        // The rest runs once a second.
        if t % TICRATE != 0 {
            return;
        }
        if left == 60 * TICRATE + 1 || left == 10 * TICRATE + 1 {
            tell!(None, "{} seconds left!", left / TICRATE);
        }
        if t == (GRACE_SECS - 10) * TICRATE {
            tell!(None, "The zone closes in 10s");
        }
        let (radius, phase, shrinking) = zone_at(self.r0, t);
        if phase != self.phase || shrinking != self.shrinking {
            self.phase = phase;
            self.shrinking = shrinking;
            if shrinking {
                tell!(None, "Zone phase {} - it is closing in!", phase);
            } else if phase > 0 {
                tell!(
                    None,
                    "Zone phase {}: {} dmg/s outside",
                    phase,
                    zone_damage(phase)
                );
            }
        }
        if phase == 0 {
            return;
        }
        for s in 0..MAX_PLAYERS {
            if !self.contender[s] || self.out[s] {
                continue;
            }
            let Some(body) = game::player(s as i32) else {
                continue;
            };
            let p = body.pos();
            let d2 = dist2([p[0], p[1]], self.center);
            if d2 <= radius * radius || body.is_dead() {
                continue;
            }
            self.zone_hit[s] = t;
            let _ = body.damage(zone_damage(phase), None, None);
            if t - self.hinted[s] >= 4 * TICRATE {
                self.hinted[s] = t;
                let gap = ((game::sqrt(d2) - radius) as i32).max(1);
                tell!(
                    Some(s as i32),
                    "Outside the zone! Run {} ({} units)",
                    compass([p[0], p[1]], self.center),
                    gap
                );
            }
        }
    }

    fn on_player_died(&mut self, victim: usize, source: Option<Actor>, t: i32) {
        if victim >= MAX_PLAYERS || !self.contender[victim] || self.out[victim] {
            return;
        }
        self.out[victim] = true;
        let left = self.alive();
        let killer = source.and_then(|s| s.player().map(|k| (s, k as usize)));
        match killer {
            Some((body, k)) if k != victim => {
                tell!(
                    None,
                    "Player {} fragged Player {} - {} left",
                    k + 1,
                    victim + 1,
                    left
                );
                let hp = body.health();
                if hp > 0 {
                    body.set_health((hp + FRAG_HEAL).min(100.max(hp)));
                }
            }
            Some(_) => tell!(
                None,
                "Player {} blew themselves up - {} left",
                victim + 1,
                left
            ),
            None if t - self.zone_hit[victim] <= 1 => {
                tell!(
                    None,
                    "Player {} was swallowed by the zone - {} left",
                    victim + 1,
                    left
                )
            }
            None => tell!(None, "Player {} is out - {} left", victim + 1, left),
        }
        if self.over {
            return;
        }
        tell!(
            Some(victim as i32),
            "Eliminated - you placed #{}.",
            left + 1
        );
        match left {
            0 => self.decide(None, t),
            1 if self.contenders() >= 2 => {
                let w = (0..MAX_PLAYERS).find(|&s| self.contender[s] && !self.out[s]);
                self.decide(w, t);
            }
            _ => {}
        }
    }
}

impl RoomMod for Royale {
    const NAME: &'static str = "BattleRoyale";
    const EVENTS: u64 = event_mask::WORLD_LOADED
        | event_mask::WORLD_TICK
        | event_mask::WORLD_THING_DIED
        | event_mask::PLAYER_ENTERED;

    fn on_event(&mut self, event: Event) {
        match event {
            Event::Loaded { .. } => {
                *self = Self::default();
                self.on_loaded();
            }
            Event::PlayerEntered { actor, .. } => self.on_entered(actor),
            Event::Tick { maptime } => self.on_tick(maptime),
            Event::ThingDied { thing, source } => {
                if let Some(victim) = thing.player().and_then(|p| usize::try_from(p).ok()) {
                    let t = game::level_stat(game::level_stat::MAPTIME);
                    self.on_player_died(victim, source, t);
                }
            }
            _ => {}
        }
    }
}

room_script_sdk::export_mod!(Royale);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zone_holds_then_shrinks_to_the_minimum() {
        let r0 = 3000.0;
        assert_eq!(zone_at(r0, 0), (r0, 0, false));
        let p1 = GRACE_SECS * TICRATE;
        assert_eq!(zone_at(r0, p1), (r0, 1, false));
        let (mid, phase, shrinking) = zone_at(r0, p1 + (HOLD_SECS + SHRINK_SECS / 2) * TICRATE);
        assert_eq!((phase, shrinking), (1, true));
        assert!(mid < r0 && mid > r0 * SHRINK_PERCENT);
        let late = zone_at(r0, p1 + 40 * (HOLD_SECS + SHRINK_SECS) * TICRATE);
        assert_eq!(late.0, MIN_RADIUS);
    }

    #[test]
    fn zone_damage_grows_and_caps() {
        assert_eq!(zone_damage(1), 3);
        assert!(zone_damage(5) > zone_damage(2));
        assert_eq!(zone_damage(50), 25);
    }

    #[test]
    fn strong_guns_live_in_the_inner_ring() {
        let has = |t: &[(Item, i32)], i: Item| t.iter().any(|&(x, _)| x == i);
        assert!(core::ptr::eq(tier(0, 100), INNER.as_slice()));
        assert!(core::ptr::eq(tier(50, 100), MIDDLE.as_slice()));
        assert!(core::ptr::eq(tier(99, 100), OUTER.as_slice()));
        assert!(has(&INNER, Item::RocketLauncher));
        assert!(!has(&MIDDLE, Item::RocketLauncher) && !has(&OUTER, Item::RocketLauncher));
        assert!(!has(&OUTER, Item::Chaingun));
        for t in [&INNER[..], &MIDDLE[..], &OUTER[..]] {
            let sum = total(t);
            assert_eq!(pick(t, 0), t[0].0);
            assert_eq!(pick(t, sum - 1), t[t.len() - 1].0);
        }
    }

    #[test]
    fn guns_come_with_their_ammo() {
        assert!(Item::Shotgun.ammo() == Some(Item::Shell));
        assert!(Item::RocketLauncher.ammo() == Some(Item::RocketAmmo));
        assert!(Item::Chainsaw.ammo().is_none() && Item::Chainsaw.is_gun());
        assert_eq!(ITEM_CLASSES[Item::Berserk as usize], "Berserk");
        assert_eq!(ITEM_CLASSES[Item::HealthBonus as usize], "HealthBonus");
    }
}
