//! Actor instances (GZDoom `AActor` subset).

use crate::class::RenderStyle;
use room_core::{Angle, Arena, ClassId, DVec3, Handle, StateId};
use std::collections::BTreeMap;

/// Generational handle to an [`Actor`].
pub type ActorId = Handle<Actor>;

/// Live actor / mobj (M5 bootstrap fields).
#[derive(Clone, Debug)]
pub struct Actor {
    /// Class this instance was spawned from.
    pub class: ClassId,
    /// World position (map units).
    pub pos: DVec3,
    /// Velocity (map units / tic).
    pub vel: DVec3,
    /// Yaw.
    pub angle: Angle,
    /// Pitch (look / aim; `A_SetPitch` may change).
    pub pitch: Angle,
    /// Camera pitch from `A_SetViewPitch`. Absent means the aim pitch.
    pub view_pitch: Option<Angle>,
    /// Camera yaw from `A_SetViewAngle`. Absent means the aim angle.
    pub view_angle: Option<Angle>,
    /// Camera roll from `A_SetViewRoll`. Absent keeps the view upright.
    pub view_roll: Option<Angle>,
    /// Eye offset from `SetViewPos`: `(x, y, z, flags)`. Absent means the normal eye.
    pub view_pos: Option<(f64, f64, f64, i32)>,
    /// Sprite shift from `A_SpriteOffset`, in map units. X is to the sprite's right.
    pub sprite_ofs_x: f64,
    /// Sprite shift from `A_SpriteOffset`. Positive Y is up.
    pub sprite_ofs_y: f64,
    /// Roll (ACS `Get/SetActorRoll`; visual later).
    pub roll: Angle,
    /// Hit points.
    pub health: i32,
    /// Movement / projectile speed override (from class; `A_SetSpeed` may change).
    pub speed: f64,
    /// Gravity multiplier (from class; `A_SetGravity` may change).
    pub gravity: f64,
    /// Mass for thrust (from class; `A_SetMass` may change).
    pub mass: i32,
    /// Opacity 0..=1 (from class; `A_SetRenderStyle` / fade may change).
    pub alpha: f64,
    /// Render style (from class; `A_SetRenderStyle` may change).
    pub render_style: RenderStyle,
    /// Palette translation name (from class; `A_SetTranslation` may change).
    pub translation: String,
    /// ACS `SetActorTeleFog` source fog class (empty = default).
    pub telefog_source: String,
    /// ACS `SetActorTeleFog` destination fog class.
    pub telefog_dest: String,
    /// Damage type for hits this actor inflicts (`A_SetDamageType` may change).
    pub damage_type: String,
    /// Projectile / melee damage amount (`A_SetDamage`; `<0` → use class default).
    pub damage: i32,
    /// Script / frag score (`A_SetScore` / `APROP_Score`).
    pub score: i32,
    /// Collision radius.
    pub radius: f64,
    /// Collision height.
    pub height: f64,
    /// Melee range override (`A_SetMeleeRange`; from class at spawn).
    pub melee_range: f64,
    /// Jump Z impulse (`A_SetJumpZ` / `APROP_JumpZ`; from class at spawn).
    pub jump_z: f64,
    /// Ground friction multiplier (`A_SetFriction`; from class at spawn).
    pub friction: f64,
    /// Max stair step height (`A_SetMaxStepHeight`; from class at spawn).
    pub max_step_height: f64,
    /// Max dropoff height (`A_SetMaxDropOffHeight`; from class at spawn).
    pub max_dropoff_height: f64,
    /// Floating Z adjust speed (`A_SetFloatSpeed`; from class at spawn).
    pub float_speed: f64,
    /// Nightmare/fast chase speed override (`A_SetFastSpeed`; from class at spawn).
    pub fast_speed: f64,
    /// Runtime blood RGB (`A_SetBloodColor`; `None` → class / default).
    pub blood_color: Option<[u8; 3]>,
    /// Runtime fill/stencil RGB (`A_SetFillColor`; `None` → class / unset).
    pub fill_color: Option<[u8; 3]>,
    /// Eye / view height (`A_SetViewHeight`; from class at spawn).
    pub view_height: f64,
    /// Hitscan Z offset above view (`A_SetAttackZOffset`; from class at spawn).
    pub attack_z_offset: f64,
    /// Sprite X scale (`A_SetScale`; from class at spawn).
    pub scale_x: f64,
    /// Sprite Y scale (`A_SetScale`; from class at spawn).
    pub scale_y: f64,
    /// Death message when this actor kills a player (`A_SetObituary`).
    pub obituary: String,
    /// Melee death message (`A_SetHitObituary`).
    pub hit_obituary: String,
    /// Incoming damage multiplier (`A_SetDamageFactor`; from class at spawn).
    pub damage_factor: f64,
    /// Team id (`A_SetTeam`; 255 = none).
    pub designated_team: u8,
    /// Camera height override for camera actors (`A_SetCameraHeight`).
    pub camera_height: f64,
    /// Camera FOV degrees (`A_SetCameraFOV`; 0 = default).
    pub camera_fov: f64,
    /// Accuracy (`A_SetAccuracy` / `APROP_Accuracy`).
    pub accuracy: i32,
    /// Stamina (`A_SetStamina` / `APROP_Stamina`).
    pub stamina: i32,
    /// Projectile ripper level (`A_SetRipperLevel`).
    pub ripper_level: i32,
    /// Minimum ripper level that can rip this actor (`A_SetRipMin`).
    pub rip_level_min: i32,
    /// Maximum ripper level that can rip this actor (`A_SetRipMax`; 0 = no max).
    pub rip_level_max: i32,
    /// Current state (`None` = stopped / destroyed via Stop).
    pub state: Option<StateId>,
    /// State duration remaining (`-1` = forever).
    pub tics: i32,
    /// Sprite name override from state.
    pub sprite: String,
    /// Frame from state.
    pub frame: u8,
    /// `MF_*` subset (bit 0 = SOLID, bit 1 = SHOOTABLE for now).
    pub flags: u64,
    /// UZDoom flags outside the legacy `u64` (`docs/36` UZ1).
    pub extra_flags: crate::flags::ActorFlags,
    /// Countdown before chase/attack (from `ReactionTime`).
    pub reaction_time: i32,
    /// Remaining infighting hold tics (`Threshold`; 0 = may retarget).
    pub threshold: i32,
    /// Pain state gate (`PainThreshold`; `A_SetPainThreshold` may change).
    pub pain_threshold: i32,
    /// Current chase / attack target (`A_Look`, damage, missiles).
    pub target: Option<ActorId>,
    /// Master / owner (e.g. Pain Elemental for Lost Soul).
    pub master: Option<ActorId>,
    /// Homing aim pointer (e.g. Revenant tracer seek).
    pub tracer: Option<ActorId>,
    /// Patrol / scripted goal (`Thing_SetGoal`).
    pub goal: Option<ActorId>,
    /// Chase `goal` when no combat target (`Thing_SetGoal` chasegoal).
    pub chase_goal: bool,
    /// Flee from target when set (`APROP_Frightened` / `+FRIGHTENED`).
    pub frightened: bool,
    /// Thing special (`A_SetSpecial`).
    pub special: u16,
    /// Thing special args (`A_SetArg`).
    pub args: [i32; 5],
    /// Hexen/UDMF thing ID (`A_SetTid`).
    pub tid: i32,
    /// DECORATE `user_*` variables (`A_SetUserVar`).
    pub user_vars: BTreeMap<String, i32>,
    /// DECORATE `user_*` arrays (`A_SetUserArray`).
    pub user_arrays: BTreeMap<String, BTreeMap<i32, i32>>,
    /// Original spawn position (nightmare respawn / `P_NightmareRespawn`).
    pub spawn_point: DVec3,
    /// Original spawn yaw.
    pub spawn_angle: Angle,
    /// Runtime species override (`A_SetSpecies`; empty → class `Species`).
    pub species: String,
    /// Runtime display name (`A_SetTag` / `APROP_Nametag`; empty → class `Tag`).
    pub nametag: String,
    /// Runtime `SeeSound` override (`APROP_SeeSound`; empty → class).
    pub see_sound: String,
    /// Runtime `AttackSound` override.
    pub attack_sound: String,
    /// Runtime `PainSound` override.
    pub pain_sound: String,
    /// Runtime `DeathSound` override.
    pub death_sound: String,
    /// Runtime `ActiveSound` override.
    pub active_sound: String,
    /// Runtime `SoundClass` (`APROP_SoundClass`; empty → class / actor name).
    pub sound_class: String,
    /// Friendly look range in blockmap blocks (`APROP_FriendlySeeBlocks`; default 10).
    pub friendly_see_blocks: i32,
    /// Float-bob / weave phase (`0..63`; `A_SetFloatBobPhase` / `A_Weave`).
    pub float_bob_phase: u8,
    /// UDMF `countsecret` — pickup increments `found_secrets`.
    pub count_secret: bool,
    /// UDMF `arg0str` pending named ACS resolve (cleared after `init_acs`).
    pub arg0_str: String,
    /// UDMF / Strife `conversation` dialogue ID (`0` = none).
    pub conversation: i32,
    /// Chase / nightmare-respawn tic counter (`movecount`).
    pub move_count: i32,
    /// 8-way chase facing (`movedir`; `-1` = `DI_NODIR`).
    pub move_dir: i8,
    /// Marked for removal at end of tic.
    pub destroyed: bool,
    /// Skip the next N action dispatches (DecoBlock `if` false-path).
    pub action_skip: u32,
    /// Missile impact kind (`room_script_abi::hit_kind`; 0 = none).
    pub hit_kind: i32,
    /// Actor hit by this missile (if `hit_kind == ACTOR`).
    pub hit_actor: Option<ActorId>,
    /// Impact position.
    pub hit_pos: DVec3,
    /// Impact normal (wall outward or ±Z).
    pub hit_normal: DVec3,
}

/// No chase direction (`DI_NODIR`).
pub const MOVE_DIR_NONE: i8 = -1;

impl Actor {
    /// Pitch the camera uses. `A_SetViewPitch` overrides aim pitch without moving shots.
    #[must_use]
    pub fn camera_pitch(&self) -> Angle {
        self.view_pitch.unwrap_or(self.pitch)
    }

    /// Yaw the camera uses. `A_SetViewAngle` overrides aim yaw without turning the body.
    #[must_use]
    pub fn camera_yaw(&self) -> Angle {
        self.view_angle.unwrap_or(self.angle)
    }

    /// Roll the camera uses. `A_SetViewRoll` banks the view without rolling the sprite.
    #[must_use]
    pub fn camera_roll(&self) -> Angle {
        self.view_roll.unwrap_or(Angle::ZERO)
    }

    /// Eye position after `SetViewPos`.
    ///
    /// `eye_z` is the unshifted eye height. Flag 2 adds the offset in world axes.
    /// Flag 4 replaces the eye with the offset. Otherwise X is forward and Y is left.
    #[must_use]
    pub fn camera_eye(&self, eye_z: f64) -> (f64, f64, f64) {
        let Some((x, y, z, flags)) = self.view_pos else {
            return (self.pos.x(), self.pos.y(), eye_z);
        };
        if flags & 4 != 0 {
            return (x, y, z);
        }
        if flags & 2 != 0 {
            return (self.pos.x() + x, self.pos.y() + y, eye_z + z);
        }
        let yaw = self.angle.radians();
        let fx = room_core::dmath::cos(yaw);
        let fy = room_core::dmath::sin(yaw);
        (
            self.pos.x() + fx * x - fy * y,
            self.pos.y() + fy * x + fx * y,
            eye_z + z,
        )
    }

    /// Advance one tic of state timing. Returns action name to run when tics hit 0
    /// and we advance (caller resolves via [`crate::StateTable`]).
    pub fn tick_state_timer(&mut self) {
        if self.destroyed {
            return;
        }
        if self.tics > 0 {
            self.tics -= 1;
        }
    }
}

/// Generational actor store.
pub struct ActorArena {
    inner: Arena<Actor>,
}

impl Default for ActorArena {
    fn default() -> Self {
        Self {
            inner: Arena::new(),
        }
    }
}

impl ActorArena {
    /// Empty arena.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert an actor.
    pub fn insert(&mut self, actor: Actor) -> Result<ActorId, room_core::CoreError> {
        self.inner.insert(actor)
    }

    /// Borrow.
    #[must_use]
    pub fn get(&self, id: ActorId) -> Option<&Actor> {
        self.inner.get(id)
    }

    /// Borrow mutably.
    pub fn get_mut(&mut self, id: ActorId) -> Option<&mut Actor> {
        self.inner.get_mut(id)
    }

    /// Remove.
    pub fn remove(&mut self, id: ActorId) -> Option<Actor> {
        self.inner.remove(id)
    }

    /// Live count.
    #[must_use]
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    /// Empty?
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
}
