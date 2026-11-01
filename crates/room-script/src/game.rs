//! Game-control imports (`docs/12` §5.3): one table, linked by every backend,
//! served by a host [`GameBridge`] (the playsim implements it in `room-sim`).

use room_script_abi::Val;
use std::sync::Arc;

/// WASM value kind of an import parameter / result.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ValKind {
    /// `i32`.
    I32,
    /// `i64` (ActorRef).
    I64,
    /// `f64`.
    F64,
}

impl ValKind {
    /// Zero value of this kind.
    #[must_use]
    pub fn zero(self) -> Val {
        match self {
            Self::I32 => Val::I32(0),
            Self::I64 => Val::I64(0),
            Self::F64 => Val::F64(0f64.to_bits()),
        }
    }
}

macro_rules! game_fns {
    ($( $(#[$doc:meta])* $variant:ident = $name:literal ( $($p:ident),* ) $(-> $r:ident)? ; )*) => {
        /// One `"room"` game import.
        #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
        pub enum GameFn {
            $( $(#[$doc])* $variant, )*
        }

        impl GameFn {
            /// Every game import, in ABI order.
            pub const ALL: &'static [GameFn] = &[$(GameFn::$variant),*];

            /// Import name (module `"room"`).
            #[must_use]
            pub fn name(self) -> &'static str {
                match self { $(GameFn::$variant => $name,)* }
            }

            /// Parameter kinds.
            #[must_use]
            pub fn params(self) -> &'static [ValKind] {
                match self { $(GameFn::$variant => &[$(ValKind::$p),*],)* }
            }

            /// Result kind (`None` = no result).
            #[must_use]
            pub fn result(self) -> Option<ValKind> {
                match self { $(GameFn::$variant => game_fns!(@r $($r)?),)* }
            }
        }
    };
    (@r) => { None };
    (@r $r:ident) => { Some(ValKind::$r) };
}

game_fns! {
    /// `game_skill() -> i32` — Doom skill 1..=5.
    GameSkill = "game_skill"() -> I32;
    /// `level_mapname(buf, cap) -> len`.
    LevelMapname = "level_mapname"(I32, I32) -> I32;
    /// `level_stat(LevelStat) -> i32`.
    LevelStat = "level_stat"(I32) -> I32;
    /// `math_sin(rad) -> f64` (deterministic).
    MathSin = "math_sin"(F64) -> F64;
    /// `math_cos(rad) -> f64` (deterministic).
    MathCos = "math_cos"(F64) -> F64;
    /// `math_atan2(y, x) -> rad` (deterministic).
    MathAtan2 = "math_atan2"(F64, F64) -> F64;
    /// `math_sqrt(x) -> f64`.
    MathSqrt = "math_sqrt"(F64) -> F64;
    /// `rng_range(lo, hi) -> i32` inclusive, playsim RNG.
    RngRange = "rng_range"(I32, I32) -> I32;
    /// `player_actor(PlayerRef) -> ActorRef` (PlayerRef = slot + 1).
    PlayerActor = "player_actor"(I32) -> I64;
    /// `player_get_i32(PlayerRef, PlayerProp) -> i32`.
    PlayerGetI32 = "player_get_i32"(I32, I32) -> I32;
    /// `player_print(PlayerRef | 0 = everyone, ptr, len)`.
    PlayerPrint = "player_print"(I32, I32, I32);
    /// `actor_class(ActorRef) -> ClassRef`.
    ActorClass = "actor_class"(I64) -> I32;
    /// `class_name(ClassRef, buf, cap) -> len`.
    ClassName = "class_name"(I32, I32, I32) -> I32;
    /// `class_is_descendant(ClassRef, parent ClassRef) -> 0/1`.
    ClassIsDescendant = "class_is_descendant"(I32, I32) -> I32;
    /// `actor_pos(ActorRef, out 3×f64)`.
    ActorPos = "actor_pos"(I64, I32);
    /// `actor_vel(ActorRef, out 3×f64)`.
    ActorVel = "actor_vel"(I64, I32);
    /// `actor_angle(ActorRef) -> degrees`.
    ActorAngle = "actor_angle"(I64) -> F64;
    /// `actor_get_i32(ActorRef, ActorProp) -> i32`.
    ActorGetI32 = "actor_get_i32"(I64, I32) -> I32;
    /// `actor_get_f64(ActorRef, ActorProp) -> f64`.
    ActorGetF64 = "actor_get_f64"(I64, I32) -> F64;
    /// `actor_get_ref(ActorRef, ActorPtr) -> ActorRef`.
    ActorGetRef = "actor_get_ref"(I64, I32) -> I64;
    /// `actor_iter_class(ClassRef | 0 = all, subclass, out ActorRef[], cap) -> total`.
    ActorIterClass = "actor_iter_class"(I32, I32, I32, I32) -> I32;
    /// `actor_iter_radius(x, y, radius, out ActorRef[], cap) -> total`.
    ActorIterRadius = "actor_iter_radius"(F64, F64, F64, I32, I32) -> I32;
    /// `actor_check_sight(from, to) -> 0/1`.
    ActorCheckSight = "actor_check_sight"(I64, I64) -> I32;
    /// `actor_distance(a, b) -> f64` (3D).
    ActorDistance = "actor_distance"(I64, I64) -> F64;
    /// `actor_count_inv(ActorRef, ClassRef) -> i32`.
    ActorCountInv = "actor_count_inv"(I64, I32) -> I32;
    /// `point_floor_z(x, y) -> f64`.
    PointFloorZ = "point_floor_z"(F64, F64) -> F64;
    /// `point_ceiling_z(x, y) -> f64`.
    PointCeilingZ = "point_ceiling_z"(F64, F64) -> F64;
    /// `actor_spawn(ClassRef, x, y, z, angle°, SpawnFlags) -> ActorRef` (play).
    ActorSpawn = "actor_spawn"(I32, F64, F64, F64, F64, I32) -> I64;
    /// `actor_destroy(ActorRef)` (play).
    ActorDestroy = "actor_destroy"(I64);
    /// `actor_set_i32(ActorRef, ActorProp, v)` (play).
    ActorSetI32 = "actor_set_i32"(I64, I32, I32);
    /// `actor_set_f64(ActorRef, ActorProp, v)` (play).
    ActorSetF64 = "actor_set_f64"(I64, I32, F64);
    /// `actor_set_ref(ActorRef, ActorPtr, ActorRef)` (play).
    ActorSetRef = "actor_set_ref"(I64, I32, I64);
    /// `actor_set_pos(ActorRef, x, y, z) -> 0/1` (play).
    ActorSetPos = "actor_set_pos"(I64, F64, F64, F64) -> I32;
    /// `actor_set_vel(ActorRef, x, y, z)` (play).
    ActorSetVel = "actor_set_vel"(I64, F64, F64, F64);
    /// `actor_set_angle(ActorRef, degrees)` (play).
    ActorSetAngle = "actor_set_angle"(I64, F64);
    /// `actor_damage(target, inflictor, source, amount) -> dealt` (play).
    ActorDamage = "actor_damage"(I64, I64, I64, I32) -> I32;
    /// `actor_give_inventory(ActorRef, ClassRef, amount) -> 0/1` (play).
    ActorGiveInventory = "actor_give_inventory"(I64, I32, I32) -> I32;
    /// `actor_take_inventory(ActorRef, ClassRef, amount) -> taken` (play).
    ActorTakeInventory = "actor_take_inventory"(I64, I32, I32) -> I32;
    /// `actor_set_state(ActorRef, label ptr, len) -> 0/1` (play).
    ActorSetState = "actor_set_state"(I64, I32, I32) -> I32;
    /// `actor_start_sound(ActorRef, name ptr, len, volume)` (play).
    ActorStartSound = "actor_start_sound"(I64, I32, I32, F64);
    /// `level_exit(secret)` (play).
    LevelExit = "level_exit"(I32);
    /// `level_set_music(name ptr, len)` (play).
    LevelSetMusic = "level_set_music"(I32, I32);
    /// `game_get_rule(Rule) -> i32`.
    GameGetRule = "game_get_rule"(I32) -> I32;
    /// `game_set_rule(Rule, value)` (play).
    GameSetRule = "game_set_rule"(I32, I32);
    /// `game_end_match(winner PlayerRef | 0 = no winner)` (play): the match is
    /// over; shells show the final standings.
    GameEndMatch = "game_end_match"(I32);
    /// `game_end_match_ex(winner, standings_ptr, n)` (play; BE11): like
    /// [`GameEndMatch`], plus `n` little-endian [`MatchStandingV1`] rows.
    GameEndMatchEx = "game_end_match_ex"(I32, I32, I32);
    /// `level_things(editor number, out [x, y, angle°] f64 triples, cap) -> total`
    /// — map things of that type as placed in the map (e.g. deathmatch starts).
    LevelThings = "level_things"(I32, I32, I32) -> I32;
    /// `hud_widget_set(seat1, id, params_ptr, params_len) -> i32` (play; Mod HUD).
    HudWidgetSet = "hud_widget_set"(I32, I32, I32, I32) -> I32;
    /// `hud_widget_patch(seat1, id, field, value) -> i32` (play).
    HudWidgetPatch = "hud_widget_patch"(I32, I32, I32, I32) -> I32;
    /// `hud_widget_remove(seat1, id) -> i32` (play).
    HudWidgetRemove = "hud_widget_remove"(I32, I32) -> I32;
    /// `hud_clear(seat1) -> i32` (play; seat1 0 = shared for this module).
    HudClear = "hud_clear"(I32) -> I32;
    /// `player_capture_input(PlayerRef, mask)` (play; mask 0 releases).
    PlayerCaptureInput = "player_capture_input"(I32, I32);
    /// `player_cmd(PlayerRef, out_ptr) -> i32` (play; writes [`PlayerCmdV1`]).
    PlayerCmd = "player_cmd"(I32, I32) -> I32;
    /// `player_sound(PlayerRef, name_ptr, len, volume)` (play; that seat only).
    PlayerSound = "player_sound"(I32, I32, I32, F64);
    /// `hud_widget_animate(seat1, id, field, target, tics, easing) -> i32` (play; Mod HUD MH4).
    HudWidgetAnimate = "hud_widget_animate"(I32, I32, I32, I32, I32, I32) -> I32;
    /// `actor_flag(ActorRef, FlagId) -> 0/1` (play; Track C C2).
    ActorFlag = "actor_flag"(I64, I32) -> I32;
    /// `actor_set_flag(ActorRef, FlagId, on)` (play; C2).
    ActorSetFlag = "actor_set_flag"(I64, I32, I32);
    /// `player_set_i32(PlayerRef, PlayerProp, v)` (play; C3).
    PlayerSetI32 = "player_set_i32"(I32, I32, I32);
    /// `actor_quake(ActorRef, intensity, duration, damrad, tremrad)` (play; C4).
    ActorQuake = "actor_quake"(I64, F64, I32, F64, F64);
    /// `actor_radius_attack(ActorRef, source, damage, distance)` (play; C5).
    ActorRadiusAttack = "actor_radius_attack"(I64, I64, I32, F64);
    /// `actor_spawn_missile(src, dest, ClassRef) -> ActorRef` (play; C5).
    ActorSpawnMissile = "actor_spawn_missile"(I64, I64, I32) -> I64;
    /// `zone_set(id, cx, cy, r, next_cx, next_cy, next_r, rgba, flags) -> i32` (play; BE1).
    ZoneSet = "zone_set"(I32, I32, I32, I32, I32, I32, I32, I32, I32) -> I32;
    /// `zone_clear(id) -> i32` (play; BE1).
    ZoneClear = "zone_clear"(I32) -> I32;
    /// `player_set_camera(PlayerRef, ActorRef | 0)` (play; BE9 spectate).
    PlayerSetCamera = "player_set_camera"(I32, I64);
    /// `player_set_team(PlayerRef, team)` (play; BE5).
    PlayerSetTeam = "player_set_team"(I32, I32);
    /// `player_team(PlayerRef) -> team` (play; BE5).
    PlayerTeam = "player_team"(I32) -> I32;
    /// `player_trace(PlayerRef, max_dist, out_ptr) -> kind` (play; BE7).
    PlayerTrace = "player_trace"(I32, F64, I32) -> I32;
    /// `player_name(PlayerRef, buf, cap) -> len` (play; BE3 session names).
    PlayerName = "player_name"(I32, I32, I32) -> I32;
    /// `actor_hit_info(ActorRef, out_ptr) -> kind` — writes 6×f64 (pos xyz, normal xyz).
    ActorHitInfo = "actor_hit_info"(I64, I32) -> I32;
}

pub use room_script_abi::game::{
    actor_f64, actor_i32, actor_ptr, level_stat, player_prop, rule, spawn_flags, thing_type,
};

/// Bounded view of guest linear memory for one import call.
pub trait GuestMemory {
    /// Copy `len` bytes at `ptr` (None when out of bounds).
    fn read(&self, ptr: i32, len: i32) -> Option<Vec<u8>>;
    /// Write `data` at `ptr` (false when out of bounds).
    fn write(&mut self, ptr: i32, data: &[u8]) -> bool;
}

impl dyn GuestMemory + '_ {
    /// UTF-8 string argument (lossy; empty when out of bounds).
    #[must_use]
    pub fn read_str(&self, ptr: i32, len: i32) -> String {
        if len <= 0 {
            return String::new();
        }
        self.read(ptr, len.min(4096))
            .map(|b| String::from_utf8_lossy(&b).into_owned())
            .unwrap_or_default()
    }

    /// Copy `s` into `(buf, cap)`; returns the full length (may exceed `cap`).
    pub fn write_str(&mut self, buf: i32, cap: i32, s: &str) -> i32 {
        let n = s.len().min(cap.max(0) as usize);
        if n > 0 {
            let _ = self.write(buf, &s.as_bytes()[..n]);
        }
        i32::try_from(s.len()).unwrap_or(i32::MAX)
    }
}

/// Host implementation of [`GameFn`] imports.
pub trait GameBridge: Send + Sync {
    /// Serve one import. `args` match [`GameFn::params`]; return `None` for
    /// "no result" or to fall back to the zero value.
    fn call(&self, f: GameFn, args: &[Val], mem: &mut dyn GuestMemory) -> Option<Val>;
}

/// Shared bridge handle stored in [`crate::HostLinker`].
pub type SharedGameBridge = Arc<dyn GameBridge>;

/// Bridge result coerced to the declared result kind.
pub(crate) fn coerce(v: Option<Val>, kind: ValKind) -> Val {
    match (v, kind) {
        (Some(Val::I32(x)), ValKind::I32) => Val::I32(x),
        (Some(Val::I64(x)), ValKind::I64) => Val::I64(x),
        (Some(Val::I32(x)), ValKind::I64) => Val::I64(i64::from(x)),
        (Some(Val::F64(x)), ValKind::F64) => Val::F64(x),
        _ => kind.zero(),
    }
}

/// Decode helpers for bridge implementations.
pub mod args {
    use room_script_abi::Val;

    /// `i32` argument `i` (0 when missing / wrong kind).
    #[must_use]
    pub fn i32(a: &[Val], i: usize) -> i32 {
        match a.get(i) {
            Some(Val::I32(v)) => *v,
            Some(Val::I64(v)) => *v as i32,
            _ => 0,
        }
    }

    /// `i64` argument `i` as `u64` bits.
    #[must_use]
    pub fn u64(a: &[Val], i: usize) -> u64 {
        match a.get(i) {
            Some(Val::I64(v)) => *v as u64,
            Some(Val::I32(v)) => u64::from(*v as u32),
            _ => 0,
        }
    }

    /// `f64` argument `i` (NaN / inf → 0).
    #[must_use]
    pub fn f64(a: &[Val], i: usize) -> f64 {
        let v = match a.get(i) {
            Some(Val::F64(bits)) => f64::from_bits(*bits),
            Some(Val::F32(bits)) => f64::from(f32::from_bits(*bits)),
            Some(Val::I32(v)) => f64::from(*v),
            _ => 0.0,
        };
        if v.is_finite() { v } else { 0.0 }
    }

    /// `f64` result.
    #[must_use]
    pub fn ret_f64(v: f64) -> Option<Val> {
        Some(Val::F64(v.to_bits()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn import_names_are_unique() {
        let mut names: Vec<_> = GameFn::ALL.iter().map(|f| f.name()).collect();
        names.sort_unstable();
        let n = names.len();
        names.dedup();
        assert_eq!(names.len(), n);
        assert!(GameFn::ALL.iter().all(|f| !matches!(
            f.name(),
            "log"
                | "level_time"
                | "actor_exists"
                | "player_count"
                | "name_intern"
                | "class_find"
                | "acs_execute"
        )));
    }

    #[test]
    fn signatures_match_table() {
        assert_eq!(
            GameFn::ActorSpawn.params(),
            &[
                ValKind::I32,
                ValKind::F64,
                ValKind::F64,
                ValKind::F64,
                ValKind::F64,
                ValKind::I32
            ]
        );
        assert_eq!(GameFn::ActorSpawn.result(), Some(ValKind::I64));
        assert_eq!(GameFn::ActorDestroy.result(), None);
        assert_eq!(GameFn::ALL.len(), 69);
        assert_eq!(GameFn::ALL[GameFn::ALL.len() - 1], GameFn::ActorHitInfo);
        assert_eq!(
            GameFn::HudWidgetSet.params(),
            &[ValKind::I32, ValKind::I32, ValKind::I32, ValKind::I32]
        );
        assert_eq!(GameFn::HudWidgetSet.result(), Some(ValKind::I32));
        assert_eq!(GameFn::HudClear.params(), &[ValKind::I32]);
    }
}
