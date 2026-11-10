//! Actor instances (GZDoom `AActor` subset).

use crate::class::RenderStyle;
use room_core::{Angle, Arena, ClassId, DVec3, Handle, StateId};
use std::collections::BTreeMap;

/// Generational handle to an [`Actor`].
pub type ActorId = Handle<Actor>;

/// A light attached with `A_AttachLight` / `A_AttachLightDef`.
#[derive(Clone, Debug, PartialEq)]
pub struct AttachedLight {
    /// Light id (`A_RemoveLight` key, lowercase).
    pub id: String,
    /// GLDEFS light name (`A_AttachLightDef`); empty for an explicit light.
    pub def: String,
    /// `DynamicLight` type (0 point, 1 pulse, 2 flicker, 3 sector, 4 random).
    pub kind: i32,
    /// Colour as `0xRRGGBB`.
    pub color: u32,
    /// Primary and secondary radius.
    pub radius: (f64, f64),
    /// `DYNAMICLIGHT_*` flags.
    pub flags: i32,
    /// Offset from the actor (forward, left, up rotated by the actor's yaw).
    pub offset: DVec3,
    /// Pulse / flicker parameter.
    pub param: f64,
    /// Spot inner and outer angles in degrees.
    pub spot: (f64, f64),
    /// Intensity multiplier.
    pub intensity: f64,
}

/// Model state changed by `A_ChangeModel` / `A_SetAnimation` / model flags.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ActorModelState {
    /// `A_ChangeModel` replacements: (model index, model path, model file, skin path, skin file).
    pub models: Vec<(i32, String, String, String, String)>,
    /// Replacement MODELDEF class (`modeldef` argument).
    pub modeldef: String,
    /// Animation set with `A_SetAnimation`: name, framerate (`-1` = model's),
    /// start / loop / end frames (`-1` = defaults), interpolation tics, flags.
    pub animation: Option<(String, f64, i32, i32, i32, i32, i32)>,
    /// Map tic when the animation started.
    pub animation_start: u32,
    /// `MDL_*` flags from `A_SetModelFlag` (`None` = MODELDEF's).
    pub flags: Option<u32>,
    /// Bone overrides from `SetBoneRotation` / `…Translation` / `…Scaling`
    /// (UZDoom 4.15 bone API), applied on top of the animated pose.
    pub bones: Vec<BoneOffset>,
    /// Animation playing before the last `A_SetAnimation`, its start tic and
    /// the tic of the switch: the renderer blends from it over the new
    /// animation's interpolation tics.
    pub prev_animation: Option<((String, f64, i32, i32, i32, i32, i32), u32, u32)>,
    /// `A_ChangeModel` animation source: (animation index, path, file); the
    /// IQM whose animations drive the model (`None` = the model's own).
    pub animation_file: Option<(i32, String, String)>,
}

/// Which part of a bone an override changes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BonePart {
    /// Rotation quaternion `(x, y, z, w)`.
    Rotation,
    /// Translation `(x, y, z)`.
    Translation,
    /// Scaling `(x, y, z)`.
    Scaling,
}

/// One component of a bone override, as UZDoom's `BoneOverrideComponent`:
/// `mode` 0 = none, 1 = `SB_ADD`, 2 = `SB_REPLACE`; the renderer eases from
/// `prev` / `prev_mode` to `cur` over `interp_len` tics after `switch_tic`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct BoneComponent {
    /// Mode.
    pub mode: i32,
    /// Mode eased out of.
    pub prev_mode: i32,
    /// Tic of the last set.
    pub switch_tic: f64,
    /// Ease length in tics.
    pub interp_len: f64,
    /// Value eased out of.
    pub prev: [f64; 4],
    /// Value.
    pub cur: [f64; 4],
}

fn default_value(part: BonePart) -> [f64; 4] {
    match part {
        BonePart::Rotation => [0.0, 0.0, 0.0, 1.0],
        BonePart::Translation | BonePart::Scaling => [0.0; 4],
    }
}

/// UZDoom `InterpolateQuat` / vector lerp.
fn lerp_part(part: BonePart, a: [f64; 4], b: [f64; 4], t: f64) -> [f64; 4] {
    let i = 1.0 - t;
    if part != BonePart::Rotation {
        return [a[0] * i + b[0] * t, a[1] * i + b[1] * t, a[2] * i + b[2] * t, 0.0];
    }
    let mut r = [a[0] * i, a[1] * i, a[2] * i, a[3] * i];
    let dot = r[0] * b[0] * t + r[1] * b[1] * t + r[2] * b[2] * t + r[3] * b[3] * t;
    if dot < 0.0 {
        r = [-r[0], -r[1], -r[2], -r[3]];
    }
    let q = [r[0] + b[0] * t, r[1] + b[1] * t, r[2] + b[2] * t, r[3] + b[3] * t];
    let len = (q[0] * q[0] + q[1] * q[1] + q[2] * q[2] + q[3] * q[3]).sqrt();
    if len < 1e-12 {
        return [0.0, 0.0, 0.0, 1.0];
    }
    [q[0] / len, q[1] / len, q[2] / len, q[3] / len]
}

impl BoneComponent {
    /// UZDoom `BoneOverrideComponent::Set`.
    pub fn set(&mut self, part: BonePart, value: [f64; 4], tic: f64, interp_len: f64, mode: i32) {
        let amt = if self.interp_len > 0.0 {
            ((tic - self.switch_tic) / self.interp_len).clamp(0.0, 1.0)
        } else {
            1.0
        };
        self.prev = if self.mode > 0 {
            lerp_part(part, self.prev, self.cur, amt)
        } else {
            default_value(part)
        };
        self.prev_mode = if self.mode == 0 && self.prev_mode != 0 && 1.0 - amt > 0.0 {
            1
        } else {
            self.mode
        };
        self.cur = if mode == 0 { default_value(part) } else { value };
        self.switch_tic = tic;
        self.interp_len = interp_len.max(0.0);
        self.mode = mode;
    }
}

/// Overrides of one bone, by index or by name (`SetNamedBone*`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BoneOffset {
    /// Bone index (`None` = by name).
    pub index: Option<i32>,
    /// Bone name (empty = by index).
    pub name: String,
    /// Translation.
    pub translation: BoneComponent,
    /// Rotation.
    pub rotation: BoneComponent,
    /// Scaling.
    pub scaling: BoneComponent,
}

impl ActorModelState {
    /// `SetBone*` / `SetNamedBone*` at `tic`.
    #[allow(clippy::too_many_arguments)]
    pub fn set_bone(
        &mut self,
        index: Option<i32>,
        name: &str,
        part: BonePart,
        value: [f64; 4],
        tic: f64,
        interp_len: f64,
        mode: i32,
    ) {
        let pos = self.bones.iter().position(|b| {
            (index.is_some() && b.index == index) || (!name.is_empty() && b.name.eq_ignore_ascii_case(name))
        });
        let slot = match pos {
            Some(p) => &mut self.bones[p],
            None => {
                self.bones.push(BoneOffset {
                    index,
                    name: name.to_owned(),
                    ..BoneOffset::default()
                });
                let last = self.bones.len() - 1;
                &mut self.bones[last]
            }
        };
        let comp = match part {
            BonePart::Rotation => &mut slot.rotation,
            BonePart::Translation => &mut slot.translation,
            BonePart::Scaling => &mut slot.scaling,
        };
        comp.set(part, value, tic, interp_len, mode);
    }
}

/// Per-actor state for actions that UZDoom keeps on `AActor` and Room did not.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ActorExt {
    /// `SpriteAngle` (`A_SetSpriteAngle`, degrees).
    pub sprite_angle: f64,
    /// `SpriteRotation` (`A_SetSpriteRotation`, degrees).
    pub sprite_rotation: f64,
    /// `VisibleStartAngle`, `VisibleEndAngle` (`A_SetVisibleRotation`; `None` = all).
    pub visible_angles: Option<(f64, f64)>,
    /// `VisibleStartPitch`, `VisibleEndPitch`.
    pub visible_pitch: Option<(f64, f64)>,
    /// Lights attached to this actor.
    pub lights: Vec<AttachedLight>,
    /// Model overrides.
    pub model: ActorModelState,
    /// Hexen `special1` / `special2` counters.
    pub special1: i32,
    /// Hexen `special2`.
    pub special2: i32,
    /// Actor pointers Hexen keeps in `special1` / `special2` (lightning, Korax spirits…).
    pub special_actor: (Option<ActorId>, Option<ActorId>),
    /// `WeaveIndexZ` (`A_Weave` vertical phase).
    pub weave_z: i32,
    /// Ice / freeze death in progress (`+ICECORPSE`).
    pub ice_corpse: bool,
    /// `Floorclip`: how far the sprite sinks into the floor (Hexen spikes, serpents).
    pub floorclip: f64,
    /// `WeaveIndexXY` for ports that drive it directly.
    pub weave_xy: i32,
    /// `LastEnemy` (Hexen lightning pairs).
    pub last_enemy: Option<ActorId>,
    /// `specialf1` / `specialf2`.
    pub specialf: (f64, f64),
    /// Map-thing flags the actor was placed with (`SpawnFlags`, `MTF_*`).
    pub spawn_flags: u32,
    /// Generic per-action scratch (Strife / Hexen loops keyed by name).
    pub scratch: BTreeMap<String, f64>,
    /// Poison received (`P_PoisonMobj`): damage, tics left, period, poisoner.
    pub poison: Option<(i32, i32, i32, Option<ActorId>)>,
}

impl ActorExt {
    /// Spawn values from class properties: `SpriteRotation`, `VisibleAngles`
    /// and `VisiblePitch` (`start, end` in degrees).
    #[must_use]
    pub fn from_class(c: &crate::class::ClassExt) -> Self {
        let pair = |name: &str| match (c.stored_f64_at(name, 0), c.stored_f64_at(name, 1)) {
            (Some(a), Some(b)) => Some((a, b)),
            _ => None,
        };
        Self {
            sprite_rotation: c.stored_f64("spriterotation").unwrap_or(0.0),
            sprite_angle: c.stored_f64("spriteangle").unwrap_or(0.0),
            visible_angles: pair("visibleangles"),
            visible_pitch: pair("visiblepitch"),
            ..Self::default()
        }
    }
}

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
    /// Fields for the UZDoom actions ported in `uzact` (`docs/36`).
    pub ext: Box<ActorExt>,
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
    /// Last actor whose noise this mobj heard (`A_Look`, `A_ClearLastHeard`).
    pub last_heard: Option<ActorId>,
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
    /// UZDoom `FastChaseStrafeCount`: tics left of a `CHF_FASTCHASE` strafe.
    pub fast_chase_strafe: i32,
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
