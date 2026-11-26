//! Safe wrappers over the `"room"` game imports: world queries, actor
//! control, level flow, deterministic math / RNG, and typed events.

use crate::sys_game as sys;
use core::cell::{Cell, UnsafeCell};
use room_script_abi::{ActorRef, ClassRef};

pub use room_script_abi::game::{
    actor_f64, actor_i32, actor_ptr, level_stat, player_prop, rule, spawn_flags, thing_type,
};
pub use room_script_abi::{class_flag, class_prop, event_mask, hit_kind};

fn str_args(s: &str) -> (i32, i32) {
    (s.as_ptr() as i32, s.len() as i32)
}

fn buf_str(buf: &[u8], len: i32) -> &str {
    let n = usize::try_from(len).unwrap_or(0).min(buf.len());
    core::str::from_utf8(&buf[..n]).unwrap_or("")
}

/// Live actor handle (stale handles read as zero and ignore writes).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[repr(transparent)]
pub struct Actor(pub ActorRef);

impl Actor {
    /// No actor.
    pub const NULL: Self = Self(ActorRef::NULL);

    /// Raw handle bits for ABI imports (`0` = null).
    #[must_use]
    pub fn bits(self) -> i64 {
        self.0.0 as i64
    }

    /// Reconstruct from ABI bits (`None` when null).
    #[must_use]
    pub fn from_bits(bits: i64) -> Option<Self> {
        (bits != 0).then_some(Self(ActorRef(bits as u64)))
    }

    /// Set the `SoundClass` used for `$playersound` lookups.
    pub fn set_sound_class(self, name: &str) {
        // SAFETY: name slice lives for the call.
        unsafe { sys::actor_set_sound_class(self.bits(), name.as_ptr() as i32, name.len() as i32) }
    }

    /// Null handle.
    #[must_use]
    pub const fn is_null(self) -> bool {
        self.0.is_null()
    }

    /// Still in the world.
    #[must_use]
    pub fn exists(self) -> bool {
        crate::actor_exists(self.0)
    }

    /// Class handle.
    #[must_use]
    pub fn class(self) -> ClassRef {
        // SAFETY: scalar import.
        ClassRef(unsafe { sys::actor_class(self.bits()) } as u32)
    }

    /// Instance of `parent` (or a subclass).
    #[must_use]
    pub fn is_a(self, parent: ClassRef) -> bool {
        class_is(self.class(), parent)
    }

    /// Position `[x, y, z]`.
    #[must_use]
    pub fn pos(self) -> [f64; 3] {
        let mut out = [0.0f64; 3];
        // SAFETY: `out` is 24 writable bytes for the call.
        unsafe { sys::actor_pos(self.bits(), out.as_mut_ptr() as i32) };
        out
    }

    /// Velocity `[x, y, z]`.
    #[must_use]
    pub fn vel(self) -> [f64; 3] {
        let mut out = [0.0f64; 3];
        // SAFETY: `out` is 24 writable bytes for the call.
        unsafe { sys::actor_vel(self.bits(), out.as_mut_ptr() as i32) };
        out
    }

    /// Facing, degrees.
    #[must_use]
    pub fn angle(self) -> f64 {
        // SAFETY: scalar import.
        unsafe { sys::actor_angle(self.bits()) }
    }

    /// Integer property ([`actor_i32`]).
    #[must_use]
    pub fn get_i32(self, prop: i32) -> i32 {
        // SAFETY: scalar import.
        unsafe { sys::actor_get_i32(self.bits(), prop) }
    }

    /// Set an integer property ([`actor_i32`]).
    pub fn set_i32(self, prop: i32, v: i32) {
        // SAFETY: scalar import.
        unsafe { sys::actor_set_i32(self.bits(), prop, v) }
    }

    /// Float property ([`actor_f64`]).
    #[must_use]
    pub fn get_f64(self, prop: i32) -> f64 {
        // SAFETY: scalar import.
        unsafe { sys::actor_get_f64(self.bits(), prop) }
    }

    /// Set a float property ([`actor_f64`]).
    pub fn set_f64(self, prop: i32, v: f64) {
        // SAFETY: scalar import.
        unsafe { sys::actor_set_f64(self.bits(), prop, v) }
    }

    /// Actor pointer ([`actor_ptr`]).
    #[must_use]
    pub fn get_ref(self, ptr: i32) -> Option<Self> {
        // SAFETY: scalar import.
        Self::from_bits(unsafe { sys::actor_get_ref(self.bits(), ptr) })
    }

    /// Set an actor pointer ([`actor_ptr`]); `None` clears it.
    pub fn set_ref(self, ptr: i32, v: Option<Self>) {
        // SAFETY: scalar import.
        unsafe { sys::actor_set_ref(self.bits(), ptr, v.map_or(0, Self::bits)) }
    }

    /// Health.
    #[must_use]
    pub fn health(self) -> i32 {
        self.get_i32(actor_i32::HEALTH)
    }

    /// Set health.
    pub fn set_health(self, v: i32) {
        self.set_i32(actor_i32::HEALTH, v);
    }

    /// Counts as a monster kill.
    #[must_use]
    pub fn is_monster(self) -> bool {
        self.get_i32(actor_i32::IS_MONSTER) != 0
    }

    /// Health ≤ 0.
    #[must_use]
    pub fn is_dead(self) -> bool {
        self.get_i32(actor_i32::IS_DEAD) != 0
    }

    /// Player slot (0-based) when this is a player body.
    #[must_use]
    pub fn player(self) -> Option<i32> {
        let p = self.get_i32(actor_i32::PLAYER);
        (p > 0).then_some(p - 1)
    }

    /// Current target.
    #[must_use]
    pub fn target(self) -> Option<Self> {
        self.get_ref(actor_ptr::TARGET)
    }

    /// Missile impact kind ([`room_script_abi::hit_kind`]); 0 = none.
    #[must_use]
    pub fn hit_kind(self) -> i32 {
        self.get_i32(actor_i32::HIT_KIND)
    }

    /// Actor hit by this missile (if any).
    #[must_use]
    pub fn hit_actor(self) -> Option<Self> {
        self.get_ref(actor_ptr::HIT_ACTOR)
    }

    /// Missile hit info: `(kind, pos xyz, normal xyz)` (`docs/31` MA4).
    #[must_use]
    pub fn hit_info(self) -> (i32, [f64; 3], [f64; 3]) {
        let mut buf = [0u8; 48];
        // SAFETY: 48-byte LE f64 buffer for the host write.
        let kind = unsafe { sys::actor_hit_info(self.bits(), buf.as_mut_ptr() as i32) };
        let mut pos = [0.0f64; 3];
        let mut normal = [0.0f64; 3];
        for i in 0..3 {
            let o = i * 8;
            pos[i] = f64::from_le_bytes(buf[o..o + 8].try_into().unwrap_or([0; 8]));
            let o = 24 + i * 8;
            normal[i] = f64::from_le_bytes(buf[o..o + 8].try_into().unwrap_or([0; 8]));
        }
        (kind, pos, normal)
    }

    /// 3D distance.
    #[must_use]
    pub fn distance(self, other: Self) -> f64 {
        // SAFETY: scalar import.
        unsafe { sys::actor_distance(self.bits(), other.bits()) }
    }

    /// Line of sight to `other`.
    #[must_use]
    pub fn can_see(self, other: Self) -> bool {
        // SAFETY: scalar import.
        unsafe { sys::actor_check_sight(self.bits(), other.bits()) != 0 }
    }

    /// Inventory amount of `item`.
    #[must_use]
    pub fn count_inv(self, item: ClassRef) -> i32 {
        // SAFETY: scalar import.
        unsafe { sys::actor_count_inv(self.bits(), item.0 as i32) }
    }

    /// Give `amount` of `item` (weapons, ammo, armor, health, keys, powerups).
    pub fn give(self, item: ClassRef, amount: i32) -> bool {
        // SAFETY: scalar import.
        unsafe { sys::actor_give_inventory(self.bits(), item.0 as i32, amount) != 0 }
    }

    /// Take up to `amount` of `item`; returns how much was taken.
    pub fn take(self, item: ClassRef, amount: i32) -> i32 {
        // SAFETY: scalar import.
        unsafe { sys::actor_take_inventory(self.bits(), item.0 as i32, amount) }
    }

    /// Move (no collision check); `false` when the actor is gone.
    pub fn set_pos(self, p: [f64; 3]) -> bool {
        // SAFETY: scalar import.
        unsafe { sys::actor_set_pos(self.bits(), p[0], p[1], p[2]) != 0 }
    }

    /// Set velocity.
    pub fn set_vel(self, v: [f64; 3]) {
        // SAFETY: scalar import.
        unsafe { sys::actor_set_vel(self.bits(), v[0], v[1], v[2]) }
    }

    /// Set facing, degrees.
    pub fn set_angle(self, deg: f64) {
        // SAFETY: scalar import.
        unsafe { sys::actor_set_angle(self.bits(), deg) }
    }

    /// Deal damage through the normal damage path; returns damage dealt.
    pub fn damage(self, amount: i32, inflictor: Option<Self>, source: Option<Self>) -> i32 {
        // SAFETY: scalar import.
        unsafe {
            sys::actor_damage(
                self.bits(),
                inflictor.map_or(0, Self::bits),
                source.map_or(0, Self::bits),
                amount,
            )
        }
    }

    /// Remove from the world.
    pub fn destroy(self) {
        // SAFETY: scalar import.
        unsafe { sys::actor_destroy(self.bits()) }
    }

    /// Jump to a state label (`"See"`, `"Death"`, `"Pain"`, …).
    pub fn set_state(self, label: &str) -> bool {
        let (p, n) = str_args(label);
        // SAFETY: label slice lives for the call.
        unsafe { sys::actor_set_state(self.bits(), p, n) != 0 }
    }

    /// Play a SNDINFO sound from this actor.
    pub fn sound(self, name: &str, volume: f64) {
        let (p, n) = str_args(name);
        // SAFETY: name slice lives for the call.
        unsafe { sys::actor_start_sound(self.bits(), p, n, volume) }
    }
}

/// Spawn `class` at `pos` facing `angle` degrees ([`spawn_flags`]).
pub fn spawn(class: ClassRef, pos: [f64; 3], angle: f64, flags: i32) -> Option<Actor> {
    // SAFETY: scalar import.
    Actor::from_bits(unsafe {
        sys::actor_spawn(class.0 as i32, pos[0], pos[1], pos[2], angle, flags)
    })
}

/// Actors of `class` (`ClassRef::NULL` = every actor), into `out`; returns
/// the total match count (may exceed `out.len()`).
pub fn actors_of_class(class: ClassRef, subclasses: bool, out: &mut [Actor]) -> usize {
    let cap = i32::try_from(out.len()).unwrap_or(i32::MAX);
    // SAFETY: `out` is `cap` writable `u64` slots (`Actor` is transparent).
    let n = unsafe {
        sys::actor_iter_class(
            class.0 as i32,
            i32::from(subclasses),
            out.as_mut_ptr() as i32,
            cap,
        )
    };
    usize::try_from(n).unwrap_or(0)
}

/// Actors whose centre is within `radius` of `(x, y)`; returns the total.
pub fn actors_in_radius(x: f64, y: f64, radius: f64, out: &mut [Actor]) -> usize {
    let cap = i32::try_from(out.len()).unwrap_or(i32::MAX);
    // SAFETY: `out` is `cap` writable `u64` slots (`Actor` is transparent).
    let n = unsafe { sys::actor_iter_radius(x, y, radius, out.as_mut_ptr() as i32, cap) };
    usize::try_from(n).unwrap_or(0)
}

/// Player body for slot `slot` (0-based).
#[must_use]
pub fn player(slot: i32) -> Option<Actor> {
    // SAFETY: scalar import.
    Actor::from_bits(unsafe { sys::player_actor(slot + 1) })
}

/// Player stat ([`player_prop`]) for slot `slot` (0-based).
#[must_use]
pub fn player_stat(slot: i32, prop: i32) -> i32 {
    // SAFETY: scalar import.
    unsafe { sys::player_get_i32(slot + 1, prop) }
}

/// HUD message to one player (`Some(slot)`) or everyone (`None`).
pub fn print(slot: Option<i32>, msg: &str) {
    let (p, n) = str_args(msg);
    // SAFETY: message slice lives for the call.
    unsafe { sys::player_print(slot.map_or(0, |s| s + 1), p, n) }
}

/// Doom skill 1 (ITYTD) ..= 5 (Nightmare).
#[must_use]
pub fn skill() -> i32 {
    // SAFETY: scalar import.
    unsafe { sys::game_skill() }
}

/// Level counter (`level_stat`).
#[must_use]
pub fn level_stat(stat: i32) -> i32 {
    // SAFETY: scalar import.
    unsafe { sys::level_stat(stat) }
}

/// Current map lump name (`E1M1`), written into `buf`.
pub fn map_name(buf: &mut [u8]) -> &str {
    let cap = i32::try_from(buf.len()).unwrap_or(i32::MAX);
    // SAFETY: `buf` is `cap` writable bytes.
    let n = unsafe { sys::level_mapname(buf.as_mut_ptr() as i32, cap) };
    buf_str(buf, n)
}

/// Class name, written into `buf`.
pub fn class_name(class: ClassRef, buf: &mut [u8]) -> &str {
    let cap = i32::try_from(buf.len()).unwrap_or(i32::MAX);
    // SAFETY: `buf` is `cap` writable bytes.
    let n = unsafe { sys::class_name(class.0 as i32, buf.as_mut_ptr() as i32, cap) };
    buf_str(buf, n)
}

/// `class` is `parent` or derives from it.
#[must_use]
pub fn class_is(class: ClassRef, parent: ClassRef) -> bool {
    // SAFETY: scalar import.
    unsafe { sys::class_is_descendant(class.0 as i32, parent.0 as i32) != 0 }
}

/// Inclusive random integer from the playsim RNG (deterministic).
#[must_use]
pub fn random(lo: i32, hi: i32) -> i32 {
    // SAFETY: scalar import.
    unsafe { sys::rng_range(lo, hi) }
}

/// Deterministic sine (radians).
#[must_use]
pub fn sin(rad: f64) -> f64 {
    // SAFETY: scalar import.
    unsafe { sys::math_sin(rad) }
}

/// Deterministic cosine (radians).
#[must_use]
pub fn cos(rad: f64) -> f64 {
    // SAFETY: scalar import.
    unsafe { sys::math_cos(rad) }
}

/// Deterministic `atan2` (radians).
#[must_use]
pub fn atan2(y: f64, x: f64) -> f64 {
    // SAFETY: scalar import.
    unsafe { sys::math_atan2(y, x) }
}

/// Square root.
#[must_use]
pub fn sqrt(x: f64) -> f64 {
    // SAFETY: scalar import.
    unsafe { sys::math_sqrt(x) }
}

/// Floor height at `(x, y)`.
#[must_use]
pub fn floor_z(x: f64, y: f64) -> f64 {
    // SAFETY: scalar import.
    unsafe { sys::point_floor_z(x, y) }
}

/// Ceiling height at `(x, y)`.
#[must_use]
pub fn ceiling_z(x: f64, y: f64) -> f64 {
    // SAFETY: scalar import.
    unsafe { sys::point_ceiling_z(x, y) }
}

/// The point is in the level's main area: players can walk there from the
/// start and back (no closets, cages, pits or locked secrets).
pub fn point_reachable(x: f64, y: f64) -> bool {
    // SAFETY: scalar import.
    unsafe { sys::point_reachable(x, y) != 0 }
}

/// End the level (normal or secret exit).
pub fn exit_level(secret: bool) {
    // SAFETY: scalar import.
    unsafe { sys::level_exit(i32::from(secret)) }
}

/// Change the level music (`D_E1M1`, `""` = stop).
pub fn set_music(name: &str) {
    let (p, n) = str_args(name);
    // SAFETY: name slice lives for the call.
    unsafe { sys::level_set_music(p, n) }
}

/// Game rule ([`rule`]) value.
#[must_use]
pub fn get_rule(rule: i32) -> i32 {
    // SAFETY: scalar import.
    unsafe { sys::game_get_rule(rule) }
}

/// Set a game rule ([`rule`]); rules reset when a map starts.
pub fn set_rule(rule: i32, value: i32) {
    // SAFETY: scalar import.
    unsafe { sys::game_set_rule(rule, value) }
}

/// End the match: `Some(slot)` wins, `None` = no winner. Every seat shows the
/// final standings and a server room closes; only the first call counts.
pub fn end_match(winner: Option<i32>) {
    // SAFETY: scalar import.
    unsafe { sys::game_end_match(winner.map_or(0, |w| w + 1)) }
}

/// End the match with rich per-seat standings ([`room_script_abi::MatchStandingV1`]; BE11).
pub fn end_match_ex(winner: Option<i32>, rows: &[room_script_abi::MatchStandingV1]) {
    let n = i32::try_from(rows.len()).unwrap_or(0);
    if n == 0 {
        end_match(winner);
        return;
    }
    let mut buf = [0u8; 8 * 24];
    let cap = rows.len().min(buf.len() / 24);
    for (i, row) in rows.iter().take(cap).enumerate() {
        let mut row = *row;
        row.size = room_script_abi::MatchStandingV1::SIZE;
        let _ = row.write_le(&mut buf[i * 24..(i + 1) * 24]);
    }
    // SAFETY: `buf` is readable for `cap * 24` bytes for the call.
    unsafe {
        sys::game_end_match_ex(
            winner.map_or(0, |w| w + 1),
            buf.as_ptr() as i32,
            i32::try_from(cap).unwrap_or(0),
        );
    }
}

/// Map things of editor number `ednum` ([`thing_type`]) as `[x, y, angle°]`,
/// in map order; returns the total (may exceed `out.len()`).
pub fn level_things(ednum: i32, out: &mut [[f64; 3]]) -> usize {
    let cap = i32::try_from(out.len()).unwrap_or(i32::MAX);
    // SAFETY: `out` is `cap` writable `[f64; 3]` slots.
    let n = unsafe { sys::level_things(ednum, out.as_mut_ptr() as i32, cap) };
    usize::try_from(n).unwrap_or(0)
}

/// Create or replace a Mod HUD widget (`docs/27` §9). `seat1` is `0` for every
/// seat and `n` for seat `n - 1`. `params` is a little-endian
/// [`room_script_abi::hud::HudWidgetV1`] blob (leading `size`).
/// Prefer [`crate::hud::Hud`] when building that blob.
pub fn hud_widget_set(seat1: i32, id: u16, params: &[u8]) -> i32 {
    // SAFETY: guest buffer is readable for `params.len()`.
    unsafe {
        sys::hud_widget_set(
            seat1,
            i32::from(id),
            params.as_ptr() as i32,
            i32::try_from(params.len()).unwrap_or(0),
        )
    }
}

/// Patch one numeric field on a Mod HUD widget.
pub fn hud_widget_patch(seat1: i32, id: u16, field: i32, value: i32) -> i32 {
    // SAFETY: scalar import.
    unsafe { sys::hud_widget_patch(seat1, i32::from(id), field, value) }
}

/// Remove one Mod HUD widget.
pub fn hud_widget_remove(seat1: i32, id: u16) -> i32 {
    // SAFETY: scalar import.
    unsafe { sys::hud_widget_remove(seat1, i32::from(id)) }
}

/// Clear Mod HUD widgets for `seat1` (`0` = shared) owned by this module.
pub fn hud_clear(seat1: i32) -> i32 {
    // SAFETY: scalar import.
    unsafe { sys::hud_clear(seat1) }
}

/// Linear tween of a numeric widget field over `tics` (`easing` reserved; 0 = linear).
pub fn hud_widget_animate(
    seat1: i32,
    id: u16,
    field: i32,
    target: i32,
    tics: i32,
    easing: i32,
) -> i32 {
    // SAFETY: scalar import.
    unsafe { sys::hud_widget_animate(seat1, i32::from(id), field, target, tics, easing) }
}

pub use room_script_abi::zone::{err as zone_err, flags as zone_flags};

/// Publish or update a battle royale zone ring (`docs/28` §9 BE1).
#[allow(clippy::too_many_arguments)]
pub fn zone_set(
    id: i32,
    cx: i32,
    cy: i32,
    r: i32,
    next_cx: i32,
    next_cy: i32,
    next_r: i32,
    rgba: u32,
    flags: i32,
) -> i32 {
    // SAFETY: scalar import.
    unsafe { sys::zone_set(id, cx, cy, r, next_cx, next_cy, next_r, rgba as i32, flags) }
}

/// Remove a zone ring by id.
pub fn zone_clear(id: i32) -> i32 {
    // SAFETY: scalar import.
    unsafe { sys::zone_clear(id) }
}

/// Capture player input bits for menus (`mask` 0 = release). `slot` is 0-based.
pub fn player_capture_input(slot: i32, mask: u32) {
    // SAFETY: scalar import.
    unsafe { sys::player_capture_input(slot + 1, mask as i32) }
}

/// Write this tic's raw `PlayerCmdV1` for `slot` (0-based) into `out` (≥24 bytes).
pub fn player_cmd(slot: i32, out: &mut [u8; 24]) -> i32 {
    // SAFETY: `out` is 24 writable bytes for the call.
    unsafe { sys::player_cmd(slot + 1, out.as_mut_ptr() as i32) }
}

/// Play a non-positional UI sound for one seat (0-based).
pub fn player_sound(slot: i32, name: &str, volume: f64) {
    let (p, n) = str_args(name);
    // SAFETY: name slice lives for the call.
    unsafe { sys::player_sound(slot + 1, p, n, volume) }
}

/// Read a class-flag bit on an actor (`docs/27` Track C C2).
pub fn actor_flag(a: Actor, flag: i32) -> bool {
    // SAFETY: scalar import.
    unsafe { sys::actor_flag(a.bits(), flag) != 0 }
}

/// Set/clear a class-flag bit on an actor (C2).
pub fn actor_set_flag(a: Actor, flag: i32, on: bool) {
    // SAFETY: scalar import.
    unsafe { sys::actor_set_flag(a.bits(), flag, i32::from(on)) }
}

/// Set a player integer property (C3: damage/bonus/extralight flashes).
pub fn player_set_i32(slot: i32, prop: i32, value: i32) {
    // SAFETY: scalar import.
    unsafe { sys::player_set_i32(slot + 1, prop, value) }
}

/// Per-seat view actor for spectate (`docs/28` BE9). `actor` null clears the override.
pub fn player_set_camera(slot: i32, actor: Option<Actor>) {
    let bits = actor.map(|a| a.bits()).unwrap_or(0);
    // SAFETY: scalar import.
    unsafe { sys::player_set_camera(slot + 1, bits) }
}

/// Assign a squad team index (`255` = none; BE5).
pub fn player_set_team(slot: i32, team: i32) {
    // SAFETY: scalar import.
    unsafe { sys::player_set_team(slot + 1, team) }
}

/// Read squad team index (BE5).
pub fn player_team(slot: i32) -> i32 {
    // SAFETY: scalar import.
    unsafe { sys::player_team(slot + 1) }
}

/// View hitscan trace from the player's aim (BE7). Writes `PlayerTraceV1` to `out`.
pub fn player_trace(slot: i32, max_dist: f64, out: &mut [u8; 56]) -> i32 {
    // SAFETY: `out` is 56 writable bytes for the call.
    unsafe { sys::player_trace(slot + 1, max_dist, out.as_mut_ptr() as i32) }
}

/// Display name for `slot` (0-based, BE3). Returns full UTF-8 length.
pub fn player_name(slot: i32, out: &mut [u8]) -> i32 {
    // SAFETY: host import.
    unsafe { sys::player_name(slot + 1, out.as_mut_ptr() as i32, out.len() as i32) }
}

/// Start a quake centered on `a` (C4).
pub fn actor_quake(a: Actor, intensity: f64, duration: i32, damrad: f64, tremrad: f64) {
    // SAFETY: scalar import.
    unsafe { sys::actor_quake(a.bits(), intensity, duration, damrad, tremrad) }
}

/// Radius attack at `origin`'s position from `source` (C5).
pub fn actor_radius_attack(origin: Actor, source: Actor, damage: i32, distance: f64) {
    // SAFETY: scalar import.
    unsafe { sys::actor_radius_attack(origin.bits(), source.bits(), damage, distance) }
}

/// Spawn a missile from `src` aimed at `dest` (C5).
pub fn actor_spawn_missile(src: Actor, dest: Actor, class: ClassRef) -> Actor {
    // SAFETY: scalar import.
    unsafe {
        Actor::from_bits(sys::actor_spawn_missile(
            src.bits(),
            dest.bits(),
            class.0 as i32,
        ))
        .unwrap_or(Actor::NULL)
    }
}

/// One game event delivered to a handler.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Event {
    /// After each tic.
    Tick {
        /// Tics since the map started.
        maptime: i32,
    },
    /// Map started.
    Loaded {
        /// Tics since the map started (0).
        maptime: i32,
    },
    /// A player entered the map.
    PlayerEntered {
        /// Slot (0-based).
        player: i32,
        /// Player body.
        actor: Actor,
    },
    /// A player died.
    PlayerDied {
        /// Slot (0-based).
        player: i32,
        /// Player body.
        actor: Actor,
    },
    /// An actor spawned.
    ThingSpawned {
        /// The new actor.
        thing: Actor,
    },
    /// An actor took damage.
    ThingDamaged {
        /// Victim.
        thing: Actor,
        /// Who is responsible.
        source: Option<Actor>,
        /// What hit (missile / puff / source).
        inflictor: Option<Actor>,
        /// Health removed.
        damage: i32,
        /// Health after the hit.
        health: i32,
    },
    /// An actor died.
    ThingDied {
        /// Victim.
        thing: Actor,
        /// Killer.
        source: Option<Actor>,
    },
    /// An item was picked up.
    ItemPickup {
        /// Who picked it up.
        toucher: Actor,
        /// Item class.
        item: ClassRef,
    },
    /// An actor was destroyed without a death event (`docs/31` MA4).
    ThingDestroyed {
        /// Removed actor.
        thing: Actor,
    },
    /// Map ending.
    Unloaded,
    /// An event this SDK version does not decode (bit index).
    Other(i32),
}

fn rd_u64(p: &[u8], at: usize) -> u64 {
    p.get(at..at + 8)
        .and_then(|b| b.try_into().ok())
        .map_or(0, u64::from_le_bytes)
}

fn rd_i32(p: &[u8], at: usize) -> i32 {
    p.get(at..at + 4)
        .and_then(|b| b.try_into().ok())
        .map_or(0, i32::from_le_bytes)
}

fn actor_at(p: &[u8], at: usize) -> Actor {
    Actor(ActorRef(rd_u64(p, at)))
}

fn opt_actor_at(p: &[u8], at: usize) -> Option<Actor> {
    Actor::from_bits(rd_u64(p, at) as i64)
}

impl Event {
    /// Decode the payload for event bit `index` (`callback_id − callback_base`).
    #[must_use]
    pub fn decode(index: i32, p: &[u8]) -> Self {
        match index {
            0 => Self::Tick {
                maptime: rd_i32(p, 0),
            },
            1 => Self::ThingDied {
                thing: actor_at(p, 0),
                source: opt_actor_at(p, 8),
            },
            3 => Self::Loaded {
                maptime: rd_i32(p, 4),
            },
            4 => Self::ThingSpawned {
                thing: actor_at(p, 0),
            },
            5 => Self::ThingDamaged {
                thing: actor_at(p, 0),
                source: opt_actor_at(p, 8),
                inflictor: opt_actor_at(p, 16),
                damage: rd_i32(p, 24),
                health: rd_i32(p, 28),
            },
            6 => Self::PlayerEntered {
                actor: actor_at(p, 0),
                player: rd_i32(p, 8) - 1,
            },
            7 => Self::PlayerDied {
                actor: actor_at(p, 0),
                player: rd_i32(p, 8) - 1,
            },
            8 => Self::Unloaded,
            9 => Self::ItemPickup {
                toucher: actor_at(p, 0),
                item: ClassRef(rd_i32(p, 8) as u32),
            },
            11 => Self::ThingDestroyed {
                thing: actor_at(p, 0),
            },
            other => Self::Other(other),
        }
    }
}

/// A play-scope mod: one event handler plus optional class registration.
/// Use with [`crate::export_mod!`].
pub trait RoomMod: Default {
    /// Handler name (shown in logs).
    const NAME: &'static str;
    /// [`event_mask`] bits to receive.
    const EVENTS: u64;
    /// Handler order (lower runs first).
    const ORDER: i32 = 0;

    /// Registration phase: declare classes with [`crate::reg`].
    fn register(&mut self) {}

    /// New game / load (`SessionKind` as i32).
    fn init(&mut self, _session_kind: i32) {}

    /// One event.
    fn on_event(&mut self, event: Event);

    /// WASM state action (`reg_state` `callback_id` &lt; [`EVENT_CALLBACK_BASE`]).
    /// Return a `StateRef` jump (0 = continue).
    fn on_state_action(&mut self, _payload: crate::ActionPayload) -> i64 {
        0
    }

    /// ACS `ScriptCall` target registered with
    /// [`crate::reg::static_function`] under `STATIC_CALLBACK_BASE + index`.
    /// Read the arguments with [`StaticCall`]; return the integer result
    /// (or call [`StaticCall::return_str`]).
    fn on_static_call(&mut self, _index: i32, _call: StaticCall) -> i64 {
        0
    }

    /// `Inventory.ModifyDamage` virtual registered with
    /// [`crate::reg::class_virtual`]`(virtual_slot::MODIFY_DAMAGE,
    /// VIRTUAL_CALLBACK_BASE + index)`. Return the new damage.
    fn on_modify_damage(&mut self, _index: i32, ev: ModifyDamage<'_>) -> i32 {
        ev.damage
    }
}

/// Callback id of event bit 0 for [`crate::export_mod!`] handlers.
pub const EVENT_CALLBACK_BASE: i32 = 1000;
/// First callback id of [`RoomMod::on_static_call`] targets.
pub const STATIC_CALLBACK_BASE: i32 = 2000;
/// First callback id of [`RoomMod::on_modify_damage`] virtuals.
pub const VIRTUAL_CALLBACK_BASE: i32 = 3000;

pub use room_script_abi::virtual_slot;

/// Arguments of the ACS `ScriptCall` being served (after the class and
/// function names), read through the `script_arg_*` imports.
#[derive(Clone, Copy, Debug, Default)]
pub struct StaticCall;

impl StaticCall {
    /// Number of arguments.
    #[must_use]
    pub fn len(self) -> usize {
        // SAFETY: scalar import.
        usize::try_from(unsafe { sys::script_arg_count() }).unwrap_or(0)
    }

    /// True when the call has no arguments.
    #[must_use]
    pub fn is_empty(self) -> bool {
        self.len() == 0
    }

    /// Integer argument `i` (0 when missing — ACS fills omitted arguments
    /// with 0, so this doubles as the ZScript default for `int` parameters).
    #[must_use]
    pub fn int(self, i: usize) -> i32 {
        // SAFETY: scalar import.
        unsafe { sys::script_arg_i32(i as i32) }
    }

    /// String argument `i` copied into `buf` (`None` when it is not a
    /// string; truncated to `buf`).
    pub fn str_into(self, i: usize, buf: &mut [u8]) -> Option<&str> {
        // SAFETY: buf pointer/len valid for the call.
        let n = unsafe { sys::script_arg_str(i as i32, buf.as_mut_ptr() as i32, buf.len() as i32) };
        let n = usize::try_from(n).ok()?.min(buf.len());
        core::str::from_utf8(&buf[..n]).ok()
    }

    /// Make the call return `text` (ACS gets a string handle).
    pub fn return_str(self, text: &str) {
        // SAFETY: text slice lives for the call.
        unsafe { sys::script_return_str(text.as_ptr() as i32, text.len() as i32) }
    }
}

/// Decoded [`room_script_abi::ModifyDamageV1`] payload.
#[derive(Clone, Copy, Debug)]
pub struct ModifyDamage<'a> {
    /// Actor carrying the item.
    pub owner: Option<Actor>,
    /// Projectile / puff.
    pub inflictor: Option<Actor>,
    /// Attacker.
    pub source: Option<Actor>,
    /// Damage so far.
    pub damage: i32,
    /// True when the owner is being hurt (target-side item).
    pub passive: bool,
    /// `DMG_*` flags.
    pub flags: i32,
    /// Damage type name (`""` = normal).
    pub damage_type: &'a str,
}

impl<'a> ModifyDamage<'a> {
    /// Decode a payload (`None` when short).
    #[must_use]
    pub fn decode(bytes: &'a [u8]) -> Option<Self> {
        let (h, dtype) = room_script_abi::ModifyDamageV1::read_le(bytes)?;
        let actor = |bits: u64| Actor::from_bits(bits as i64);
        Some(Self {
            owner: actor(h.owner),
            inflictor: actor(h.inflictor),
            source: actor(h.source),
            damage: h.damage,
            passive: h.passive != 0,
            flags: h.flags,
            damage_type: core::str::from_utf8(dtype).unwrap_or(""),
        })
    }
}

/// A player's userinfo gender (0 male, 1 female, 2 neutral, 3 other).
#[must_use]
pub fn player_gender(slot: i32) -> i32 {
    // SAFETY: scalar import.
    unsafe { sys::player_gender(slot + 1) }
}

/// A player's user CVar (falls back to the global CVar, then 0).
#[must_use]
pub fn player_cvar(slot: i32, name: &str) -> i32 {
    // SAFETY: name slice lives for the call.
    unsafe { sys::player_cvar_i32(slot + 1, name.as_ptr() as i32, name.len() as i32) }
}

/// Single-threaded mod state slot used by [`crate::export_mod!`].
pub struct ModCell<T> {
    value: UnsafeCell<Option<T>>,
    busy: Cell<bool>,
}

// SAFETY: wasm32 guests run single-threaded; `busy` rejects re-entrant access.
unsafe impl<T> Sync for ModCell<T> {}

impl<T: Default> ModCell<T> {
    /// Empty slot.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            value: UnsafeCell::new(None),
            busy: Cell::new(false),
        }
    }

    /// Run `f` on the state (created on first use); `None` when re-entered.
    pub fn with<R>(&self, f: impl FnOnce(&mut T) -> R) -> Option<R> {
        if self.busy.replace(true) {
            return None;
        }
        // SAFETY: `busy` guarantees this is the only live reference.
        let r = f(unsafe { &mut *self.value.get() }.get_or_insert_with(T::default));
        self.busy.set(false);
        Some(r)
    }
}

impl<T: Default> Default for ModCell<T> {
    fn default() -> Self {
        Self::new()
    }
}

/// Bytes of an event payload handed to `room_dispatch`.
///
/// # Safety
///
/// `ptr`/`len` must be the arguments the host passed to `room_dispatch`
/// (a buffer the host wrote into this module's memory).
#[must_use]
pub unsafe fn payload<'a>(ptr: i32, len: i32) -> &'a [u8] {
    if ptr <= 0 || len <= 0 {
        return &[];
    }
    // SAFETY: caller contract — host-written buffer in our linear memory.
    unsafe { core::slice::from_raw_parts(ptr as usize as *const u8, len as usize) }
}

/// Register a string class property (`Translation`, `Tag`, `Obituary`).
pub fn class_property_str(prop: u32, value: &str) {
    let (p, n) = str_args(value);
    // SAFETY: value slice lives for the call.
    unsafe { sys::reg_class_property_str(prop as i32, p, n) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_damage_and_pickup_payloads() {
        let mut p = [0u8; 32];
        p[..8].copy_from_slice(&7u64.to_le_bytes());
        p[8..16].copy_from_slice(&9u64.to_le_bytes());
        p[24..28].copy_from_slice(&15i32.to_le_bytes());
        p[28..32].copy_from_slice(&(-3i32).to_le_bytes());
        let Event::ThingDamaged {
            thing,
            source,
            inflictor,
            damage,
            health,
        } = Event::decode(5, &p)
        else {
            panic!("wrong event");
        };
        assert_eq!(thing, Actor(ActorRef(7)));
        assert_eq!(source, Some(Actor(ActorRef(9))));
        assert_eq!(inflictor, None);
        assert_eq!((damage, health), (15, -3));
        assert_eq!(
            Event::decode(9, &p[..12]),
            Event::ItemPickup {
                toucher: Actor(ActorRef(7)),
                item: ClassRef(9)
            }
        );
        assert_eq!(Event::decode(2, &[]), Event::Other(2));
        assert_eq!(Event::decode(0, &[1]), Event::Tick { maptime: 0 });
    }

    #[test]
    fn mod_cell_rejects_reentry() {
        let cell: ModCell<i32> = ModCell::new();
        assert_eq!(cell.with(|v| *v += 2), Some(()));
        assert_eq!(
            cell.with(|v| cell.with(|_| ()).is_none() && *v == 2),
            Some(true)
        );
    }
}
