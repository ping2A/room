//! Class registry built from DECORATE IR (M5 bootstrap).

use room_core::ClassId;
use room_decorate::{ActorDef, ClassDefIr, FlagOp, StateFlow, StateItem};
use room_script::{PropId, RegisteredClass};
use std::collections::BTreeMap;
use thiserror::Error;

/// Class registry errors.
#[derive(Debug, Error)]
pub enum ClassError {
    /// Duplicate / bad definition.
    #[error("{0}")]
    Message(String),
}

/// GZDoom `ERenderStyle` subset (stored for renderer / `A_SetRenderStyle`).
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum RenderStyle {
    /// Invisible.
    None,
    /// Opaque (default).
    #[default]
    Normal,
    /// Spectre fuzz.
    Fuzzy,
    /// Lost soul translucency.
    SoulTrans,
    /// Fuzzy when opaque, translucent otherwise.
    OptFuzzy,
    /// Solid fill colour stencil.
    Stencil,
    /// Alpha blend.
    Translucent,
    /// Additive blend.
    Add,
    /// Shaded (fill colour × alpha).
    Shaded,
    /// Stencil with alpha.
    TranslucentStencil,
    /// Soft shadow.
    Shadow,
    /// Subtractive blend.
    Subtract,
    /// Additive stencil.
    AddStencil,
    /// Additive shaded.
    AddShaded,
    /// Colour blend.
    ColorBlend,
    /// Colour add.
    ColorAdd,
    /// Multiply.
    Multiply,
}

impl RenderStyle {
    /// Parse a DECORATE / `A_SetRenderStyle` style name.
    #[must_use]
    pub fn parse(name: &str) -> Option<Self> {
        match name.trim_matches('"').to_ascii_lowercase().as_str() {
            "none" => Some(Self::None),
            "normal" => Some(Self::Normal),
            "fuzzy" => Some(Self::Fuzzy),
            "soultrans" => Some(Self::SoulTrans),
            "optfuzzy" => Some(Self::OptFuzzy),
            "stencil" => Some(Self::Stencil),
            "translucent" => Some(Self::Translucent),
            "add" | "additive" => Some(Self::Add),
            "shaded" => Some(Self::Shaded),
            "translucentstencil" => Some(Self::TranslucentStencil),
            "shadow" => Some(Self::Shadow),
            "subtract" | "subtractive" => Some(Self::Subtract),
            "addstencil" => Some(Self::AddStencil),
            "addshaded" => Some(Self::AddShaded),
            "colorblend" => Some(Self::ColorBlend),
            "coloradd" => Some(Self::ColorAdd),
            "multiply" => Some(Self::Multiply),
            _ => None,
        }
    }

    /// ACS `STYLE_*` constant for `APROP_RenderStyle` get.
    #[must_use]
    pub fn to_acs_style(self) -> i32 {
        match self {
            Self::None => 0,
            Self::Normal => 1,
            Self::Fuzzy => 2,
            Self::SoulTrans => 3,
            Self::OptFuzzy => 4,
            Self::Stencil => 5,
            Self::AddStencil => 6,
            Self::AddShaded => 7,
            Self::Translucent => 64,
            Self::Add => 65,
            Self::Shaded => 66,
            Self::TranslucentStencil => 67,
            Self::Shadow => 68,
            Self::Subtract => 69,
            Self::ColorBlend => 70,
            Self::ColorAdd => 71,
            Self::Multiply => 72,
        }
    }

    /// Parse ACS `STYLE_*` for `APROP_RenderStyle` set.
    #[must_use]
    pub fn from_acs_style(v: i32) -> Option<Self> {
        Some(match v {
            0 => Self::None,
            1 => Self::Normal,
            2 => Self::Fuzzy,
            3 => Self::SoulTrans,
            4 => Self::OptFuzzy,
            5 => Self::Stencil,
            6 => Self::AddStencil,
            7 => Self::AddShaded,
            64 => Self::Translucent,
            65 => Self::Add,
            66 => Self::Shaded,
            67 => Self::TranslucentStencil,
            68 => Self::Shadow,
            69 => Self::Subtract,
            70 => Self::ColorBlend,
            71 => Self::ColorAdd,
            72 => Self::Multiply,
            _ => return None,
        })
    }
}

/// Compiled class defaults (subset of GZDoom `PClassActor`).
#[derive(Clone, Debug)]
pub struct ClassDef {
    /// Stable id (= index in registry).
    pub id: ClassId,
    /// Class name (original casing).
    pub name: String,
    /// `Tag` display name (empty → use `name`).
    pub tag: String,
    /// Parent class id.
    pub parent: Option<ClassId>,
    /// Editor number.
    pub doomednum: Option<i32>,
    /// Default health.
    pub health: i32,
    /// `GibHealth` — XDeath when remaining health is below this (0 = `-Health`).
    pub gib_health: i32,
    /// `WoundHealth` — enter Wound state when remaining health is below this (0 = off).
    pub wound_health: i32,
    /// `DeathHeight` corpse collision height (0 = Doom `height >> 2`).
    pub death_height: f64,
    /// `BurnHeight` corpse height for fire deaths (0 = use `DeathHeight`).
    pub burn_height: f64,
    /// Default radius.
    pub radius: f64,
    /// Default height.
    pub height: f64,
    /// Movement / projectile speed (map units per tic).
    pub speed: f64,
    /// `MeleeRange` (map units; default 64).
    pub melee_range: f64,
    /// `MeleeThreshold` — prefer Melee over Missile when closer (0 = off).
    pub melee_threshold: f64,
    /// `Mass` (thrust divisor; default 100).
    pub mass: i32,
    /// `ReactionTime` (tics before chase; default 8).
    pub reaction_time: i32,
    /// `Threshold` — infighting hold tics after taking damage (Doom default 100).
    pub threshold: i32,
    /// `DefThreshold` reset value when a new target is acquired (default 100).
    pub def_threshold: i32,
    /// `Gravity` multiplier (default 1.0).
    pub gravity: f64,
    /// `Friction` multiplier for ground XY decay (default 1.0).
    pub friction: f64,
    /// `JumpZ` vertical impulse for player jump (default 8).
    pub jump_z: f64,
    /// `Player.ForwardMove` XY thrust scale (default 1).
    pub forward_move: f64,
    /// `RipperLevel` — projectile/monster rip strength (default 0).
    pub ripper_level: i32,
    /// `RipLevelMin` — minimum ripper level that can rip this actor (default 0).
    pub rip_level_min: i32,
    /// `RipLevelMax` — maximum ripper level that can rip this actor (0 = no max).
    pub rip_level_max: i32,
    /// `FloatBobStrength` amplitude multiplier for `+FLOATBOB` (default 1).
    pub float_bob_strength: f64,
    /// `MaxTargetRange` for missile attacks (0 = default 512).
    pub max_target_range: f64,
    /// `MinMissileChance` — larger = rarer missile attacks (default 200).
    pub min_missile_chance: i32,
    /// `DamageFactor` multiplier for incoming damage (default 1.0).
    pub damage_factor: f64,
    /// Typed `DamageFactor "Type", n` (lowercase keys).
    pub typed_damage_factors: BTreeMap<String, f64>,
    /// `DamageType` dealt by this class (missiles / attacks; empty = none).
    pub damage_type: String,
    /// `FloatSpeed` for `+FLOAT` actors (default 4).
    pub float_speed: f64,
    /// `FastSpeed` used when skill is Nightmare / fast monsters (0 = use Speed).
    pub fast_speed: f64,
    /// `MaxStepHeight` (default 24).
    pub max_step_height: f64,
    /// `MaxDropOffHeight` (default 24).
    pub max_dropoff_height: f64,
    /// `Species` name (empty = none); same species skips missile damage.
    pub species: String,
    /// MBF21 / DECORATE `InfightingGroup` (`-1` = none).
    pub infighting_group: i32,
    /// MBF21 `ProjectileGroup` (`-1` = none).
    pub projectile_group: i32,
    /// MBF21 `SplashGroup` (`-1` = none).
    pub splash_group: i32,
    /// `DesignatedTeam` (255 = none / unteamed).
    pub designated_team: u8,
    /// `ViewHeight` for camera / attacks (0 = half Height).
    pub view_height: f64,
    /// `CameraHeight` for security cameras (0 = use Height / 2).
    pub camera_height: f64,
    /// `CameraFOV` degrees (0 = engine default).
    pub camera_fov: f64,
    /// `AttackZOffset` added to attack origin Z.
    pub attack_z_offset: f64,
    /// `Obituary` death message (killer → player).
    pub obituary: String,
    /// `HitObituary` melee death message.
    pub hit_obituary: String,
    /// `BloodColor` RGB (`None` = default red blood).
    pub blood_color: Option<[u8; 3]>,
    /// `BloodType` blood actor class names (up to 3: spray / splatter / axe).
    pub blood_type: Vec<String>,
    /// `FillColor` RGB for stencil/fill render styles (`None` = unset).
    pub fill_color: Option<[u8; 3]>,
    /// `Scale` / `XScale` (default 1).
    pub scale_x: f64,
    /// `Scale` / `YScale` (default 1).
    pub scale_y: f64,
    /// `Alpha` opacity 0..=1 (default 1).
    pub alpha: f64,
    /// `RenderStyle` (default Normal).
    pub render_style: RenderStyle,
    /// `Translation` name (empty = none; resolved by renderer).
    pub translation: String,
    /// Default flags (`SOLID`/`SHOOTABLE` bits).
    pub flags: u64,
    /// Initial state duration from first Spawn frame (or `-1`).
    pub spawn_tics: i32,
    /// `Inventory.Amount` default give.
    pub inventory_amount: i32,
    /// `Inventory.MaxAmount`.
    pub inventory_max_amount: i32,
    /// Starting Clip ammo for player pawns (`StartBullets` / DeHackEd Misc Initial Bullets).
    pub start_bullets: i32,
    /// `Inventory.PickupMessage` (empty = no HUD message).
    pub pickup_message: String,
    /// `Inventory.PickupSound` logical name (empty = none).
    pub pickup_sound: String,
    /// True if this class is treated as a weapon (has Fire state or Weapon.* props).
    pub is_weapon: bool,
    /// `PainChance` 0..=256 (Doom scale).
    pub pain_chance: i32,
    /// `PainThreshold` — skip Pain while remaining health is above this (0 = off).
    pub pain_threshold: i32,
    /// Typed `PainChance "Type", n` (lowercase keys).
    pub typed_pain_chances: BTreeMap<String, i32>,
    /// `Damage` property (missile/melee base; 0 = none).
    pub damage: i32,
    /// `ContactDamage` — hurt overlapping shootable actors this much every 8 tics (`0` = none).
    pub contact_damage: i32,
    /// `KillTaunt` logical sounds played when this actor kills a `COUNTKILL`.
    pub kill_taunts: Vec<String>,
    /// `IdleTaunt` logical sounds played after standing still (`pr_idletaunt`).
    pub idle_taunts: Vec<String>,
    /// `Weapon.AmmoType` class name (empty = no ammo).
    pub ammo_type: String,
    /// `Weapon.AmmoUse` per shot.
    pub ammo_use: i32,
    /// `Weapon.AmmoGive` granted on first pickup.
    pub ammo_give: i32,
    /// `DropItem` class names (spawned on death).
    pub drop_items: Vec<String>,
    /// `SeeSound` logical name (empty = none).
    pub see_sound: String,
    /// `DeathSound` logical name.
    pub death_sound: String,
    /// `PainSound` logical name.
    pub pain_sound: String,
    /// `CrushPainSound` logical name (played when crushed; empty = silent).
    pub crush_pain_sound: String,
    /// `HowlSound` — `A_Pain` cue when health ≤ 0 (empty = use `PainSound`).
    pub howl_sound: String,
    /// `ActiveSound` logical name (ambient while chasing).
    pub active_sound: String,
    /// `AttackSound` logical name (melee / hitscan attack cue).
    pub attack_sound: String,
    /// `SoundClass` prefix for player/skin sounds (empty = class name).
    pub sound_class: String,
    /// `FriendlySeeBlocks` — friendly look range in 128-unit blocks (default 10).
    pub friendly_see_blocks: i32,
    /// `BounceSound` (floor/actor bounce; empty = none).
    pub bounce_sound: String,
    /// `WallBounceSound` (wall bounce; empty = use `BounceSound`).
    pub wall_bounce_sound: String,
    /// `BounceFactor` — velocity kept on a bounce (default 0.7).
    pub bounce_factor: f64,
    /// `PushSound` when shoved by a pusher (empty = none).
    pub push_sound: String,
    /// `UseSound` when the actor is used (empty = none).
    pub use_sound: String,
    /// `Player.StartItem` (class, amount) in order; empty = Doom default kit.
    pub start_items: Vec<(String, i32)>,
}

impl ClassDef {
    /// Pretty name for UI / obituaries (`Tag`, else class name).
    #[must_use]
    pub fn display_name(&self) -> &str {
        if self.tag.is_empty() {
            self.name.as_str()
        } else {
            self.tag.as_str()
        }
    }
}

/// Lookup tables for spawn / inheritance.
#[derive(Clone, Debug, Default)]
pub struct ClassRegistry {
    classes: Vec<ClassDef>,
    by_name: BTreeMap<String, ClassId>,
    by_doomednum: BTreeMap<i32, ClassId>,
    replaced_by: BTreeMap<ClassId, ClassId>,
}

impl ClassRegistry {
    /// Registry with the abstract `Actor` root only.
    #[must_use]
    pub fn new() -> Self {
        let mut reg = Self::default();
        let _ = reg.push(ClassDef {
            id: ClassId::from_raw(0),
            name: "Actor".into(),
            tag: String::new(),
            parent: None,
            doomednum: None,
            health: 1000,
            gib_health: 0,
            wound_health: 0,
            death_height: 0.0,
            burn_height: 0.0,
            radius: 20.0,
            height: 16.0,
            speed: 0.0,
            melee_range: 64.0,
            melee_threshold: 0.0,
            mass: 100,
            reaction_time: 8,
            threshold: 100,
            def_threshold: 100,
            gravity: 1.0,
            friction: 1.0,
            jump_z: 8.0,
            forward_move: 1.0,
            ripper_level: 0,
            rip_level_min: 0,
            rip_level_max: 0,
            float_bob_strength: 1.0,
            max_target_range: 0.0,
            min_missile_chance: 200,
            damage_factor: 1.0,
            typed_damage_factors: BTreeMap::new(),
            damage_type: String::new(),
            float_speed: 4.0,
            fast_speed: 0.0,
            max_step_height: 24.0,
            max_dropoff_height: 24.0,
            species: String::new(),
            infighting_group: -1,
            projectile_group: -1,
            splash_group: -1,
            designated_team: 255,
            view_height: 0.0,
            camera_height: 0.0,
            camera_fov: 0.0,
            attack_z_offset: 0.0,
            obituary: String::new(),
            hit_obituary: String::new(),
            blood_color: None,
            blood_type: Vec::new(),
            fill_color: None,
            scale_x: 1.0,
            scale_y: 1.0,
            alpha: 1.0,
            render_style: RenderStyle::Normal,
            translation: String::new(),
            flags: 0,
            spawn_tics: -1,
            inventory_amount: 1,
            inventory_max_amount: 1,
            start_bullets: 50,
            pickup_message: String::new(),
            pickup_sound: String::new(),
            is_weapon: false,
            pain_chance: 0,
            pain_threshold: 0,
            typed_pain_chances: BTreeMap::new(),
            damage: 0,
            contact_damage: 0,
            kill_taunts: Vec::new(),
            idle_taunts: Vec::new(),
            ammo_type: String::new(),
            ammo_use: 0,
            ammo_give: 0,
            drop_items: Vec::new(),
            see_sound: String::new(),
            death_sound: String::new(),
            pain_sound: String::new(),
            crush_pain_sound: String::new(),
            howl_sound: String::new(),
            active_sound: String::new(),
            attack_sound: String::new(),
            sound_class: String::new(),
            friendly_see_blocks: 10,
            bounce_sound: String::new(),
            wall_bounce_sound: String::new(),
            bounce_factor: 0.7,
            push_sound: String::new(),
            use_sound: String::new(),
            start_items: Vec::new(),
        });
        reg
    }

    /// Build from parsed DECORATE IR (single-pass; parents must appear first).
    pub fn from_decorate(ir: &ClassDefIr) -> Result<Self, ClassError> {
        let mut reg = Self::new();
        for def in &ir.actors {
            reg.add_actor(def)?;
        }
        Ok(reg)
    }

    /// Import a WASM-registered class (`docs/12` §6 / M11).
    ///
    /// Parent `0` means `Actor`. Non-zero parent ClassRefs must already be
    /// remapped by the caller into this registry (use script→ClassId map).
    ///
    /// # Errors
    ///
    /// Unknown parent, duplicate name (unless replace), or push failure.
    pub fn import_script_class(
        &mut self,
        reg: &RegisteredClass,
        parent_id: ClassId,
    ) -> Result<ClassId, ClassError> {
        if self.find_name(&reg.name).is_some() {
            if reg.flags & 1 == 0 {
                // bit0 = REPLACE_DEFINITION (bootstrap)
                return Err(ClassError::Message(format!("duplicate class {}", reg.name)));
            }
            // Replace: drop old name mapping; doomednum handled on push.
            let key = reg.name.to_ascii_lowercase();
            if let Some(old) = self.by_name.remove(&key) {
                if let Some(dn) = self.classes[old.raw() as usize].doomednum {
                    self.by_doomednum.remove(&dn);
                }
                // Leave stale slot; push appends a new ClassId (GZDoom-style redefine
                // keeps the old index in full ports — bootstrap appends).
            }
        }
        let parent = self
            .get(parent_id)
            .cloned()
            .ok_or_else(|| ClassError::Message("unknown parent ClassId".into()))?;
        let mut def = parent;
        def.name = reg.name.clone();
        def.parent = Some(parent_id);
        def.doomednum = if reg.doomednum >= 0 {
            Some(reg.doomednum)
        } else {
            None
        };
        for &(prop, value) in &reg.props_i32 {
            apply_script_prop_i32(&mut def, prop, value);
        }
        for (prop, value) in &reg.props_str {
            apply_script_prop_str(&mut def, *prop, value);
        }
        for &(flag, on) in &reg.class_flags {
            apply_script_flag(&mut def, flag, on);
        }
        self.push(def)
    }

    fn push(&mut self, mut def: ClassDef) -> Result<ClassId, ClassError> {
        let id = ClassId::from_raw(
            u32::try_from(self.classes.len())
                .map_err(|_| ClassError::Message("too many classes".into()))?,
        );
        def.id = id;
        let key = def.name.to_ascii_lowercase();
        if self.by_name.contains_key(&key) {
            return Err(ClassError::Message(format!("duplicate class {}", def.name)));
        }
        if let Some(dn) = def.doomednum {
            // GZDoom: a later definition takes over the editor number.
            if let Some(old) = self.by_doomednum.insert(dn, id) {
                if let Some(prev) = self.classes.get_mut(old.raw() as usize) {
                    prev.doomednum = None;
                }
            }
        }
        self.by_name.insert(key, id);
        self.classes.push(def);
        Ok(id)
    }

    fn add_actor(&mut self, def: &ActorDef) -> Result<ClassId, ClassError> {
        let parent_name = def.parent.as_deref().unwrap_or("Actor");
        let parent_id = self
            .find_name(parent_name)
            .ok_or_else(|| ClassError::Message(format!("unknown parent {parent_name}")))?;
        let parent = self.classes[parent_id.raw() as usize].clone();
        let mut tag = parent.tag.clone();
        let mut health = parent.health;
        let mut gib_health = parent.gib_health;
        let mut wound_health = parent.wound_health;
        let mut death_height = parent.death_height;
        let mut burn_height = parent.burn_height;
        let mut radius = parent.radius;
        let mut height = parent.height;
        let mut speed = parent.speed;
        let mut melee_range = parent.melee_range;
        let mut melee_threshold = parent.melee_threshold;
        let mut mass = parent.mass;
        let mut reaction_time = parent.reaction_time;
        let mut threshold = parent.threshold;
        let mut def_threshold = parent.def_threshold;
        let mut gravity = parent.gravity;
        let mut friction = parent.friction;
        let mut jump_z = parent.jump_z;
        let mut forward_move = parent.forward_move;
        let mut ripper_level = parent.ripper_level;
        let mut rip_level_min = parent.rip_level_min;
        let mut rip_level_max = parent.rip_level_max;
        let mut float_bob_strength = parent.float_bob_strength;
        let mut max_target_range = parent.max_target_range;
        let mut min_missile_chance = parent.min_missile_chance;
        let mut damage_factor = parent.damage_factor;
        let mut typed_damage_factors = parent.typed_damage_factors.clone();
        let mut damage_type = parent.damage_type.clone();
        let mut float_speed = parent.float_speed;
        let mut fast_speed = parent.fast_speed;
        let mut max_step_height = parent.max_step_height;
        let mut max_dropoff_height = parent.max_dropoff_height;
        let mut species = parent.species.clone();
        let mut infighting_group = parent.infighting_group;
        let mut projectile_group = parent.projectile_group;
        let mut splash_group = parent.splash_group;
        let mut designated_team = parent.designated_team;
        let mut view_height = parent.view_height;
        let mut camera_height = parent.camera_height;
        let mut camera_fov = parent.camera_fov;
        let mut attack_z_offset = parent.attack_z_offset;
        let mut obituary = parent.obituary.clone();
        let mut hit_obituary = parent.hit_obituary.clone();
        let mut blood_color = parent.blood_color;
        let mut blood_type = parent.blood_type.clone();
        let mut fill_color = parent.fill_color;
        let mut scale_x = parent.scale_x;
        let mut scale_y = parent.scale_y;
        let mut alpha = parent.alpha;
        let mut render_style = parent.render_style;
        let mut translation = parent.translation.clone();
        let mut flags = parent.flags;
        let mut inventory_amount = parent.inventory_amount;
        let mut inventory_max_amount = parent.inventory_max_amount;
        let mut start_bullets = parent.start_bullets;
        let mut pickup_message = parent.pickup_message.clone();
        let mut pickup_sound = parent.pickup_sound.clone();
        let mut is_weapon = parent.is_weapon;
        let mut pain_chance = parent.pain_chance;
        let mut pain_threshold = parent.pain_threshold;
        let mut typed_pain_chances = parent.typed_pain_chances.clone();
        let mut damage = parent.damage;
        let mut contact_damage = parent.contact_damage;
        let mut kill_taunts = parent.kill_taunts.clone();
        let mut own_kill_taunts = false;
        let mut idle_taunts = parent.idle_taunts.clone();
        let mut own_idle_taunts = false;
        let mut ammo_type = parent.ammo_type.clone();
        let mut ammo_use = parent.ammo_use;
        let mut ammo_give = parent.ammo_give;
        let mut drop_items = parent.drop_items.clone();
        let mut start_items = parent.start_items.clone();
        let mut own_start_items = false;
        let mut see_sound = parent.see_sound.clone();
        let mut death_sound = parent.death_sound.clone();
        let mut pain_sound = parent.pain_sound.clone();
        let mut crush_pain_sound = parent.crush_pain_sound.clone();
        let mut howl_sound = parent.howl_sound.clone();
        let mut active_sound = parent.active_sound.clone();
        let mut attack_sound = parent.attack_sound.clone();
        let mut sound_class = parent.sound_class.clone();
        let mut friendly_see_blocks = parent.friendly_see_blocks;
        let mut bounce_sound = parent.bounce_sound.clone();
        let mut wall_bounce_sound = parent.wall_bounce_sound.clone();
        let mut bounce_factor = parent.bounce_factor;
        let mut push_sound = parent.push_sound.clone();
        let mut use_sound = parent.use_sound.clone();
        for p in &def.properties {
            match p.name.to_ascii_lowercase().as_str() {
                "tag" => {
                    if let Some(a) = p.args.first() {
                        tag = a.trim_matches('"').to_owned();
                    }
                }
                "health" => {
                    if let Some(a) = p.args.first() {
                        health = a.parse().unwrap_or(health);
                    }
                }
                "gibhealth" => {
                    if let Some(a) = p.args.first() {
                        gib_health = a.parse().unwrap_or(gib_health);
                    }
                }
                "woundhealth" => {
                    if let Some(a) = p.args.first() {
                        wound_health = a.parse().unwrap_or(wound_health);
                    }
                }
                "deathheight" => {
                    if let Some(a) = p.args.first() {
                        death_height = a.parse().unwrap_or(death_height);
                    }
                }
                "burnheight" => {
                    if let Some(a) = p.args.first() {
                        burn_height = a.parse().unwrap_or(burn_height);
                    }
                }
                "radius" => {
                    if let Some(a) = p.args.first() {
                        radius = a.parse().unwrap_or(radius);
                    }
                }
                "height" => {
                    if let Some(a) = p.args.first() {
                        height = a.parse().unwrap_or(height);
                    }
                }
                "speed" => {
                    if let Some(a) = p.args.first() {
                        speed = a.parse().unwrap_or(speed);
                    }
                }
                "meleerange" => {
                    if let Some(a) = p.args.first() {
                        melee_range = a.parse().unwrap_or(melee_range);
                    }
                }
                "meleethreshold" => {
                    if let Some(a) = p.args.first() {
                        melee_threshold = a.parse().unwrap_or(melee_threshold);
                    }
                }
                "mass" => {
                    if let Some(a) = p.args.first() {
                        mass = a.parse().unwrap_or(mass);
                    }
                }
                "reactiontime" => {
                    if let Some(a) = p.args.first() {
                        reaction_time = a.parse().unwrap_or(reaction_time);
                    }
                }
                "threshold" => {
                    if let Some(a) = p.args.first() {
                        threshold = a.parse().unwrap_or(threshold);
                    }
                }
                "defthreshold" => {
                    if let Some(a) = p.args.first() {
                        def_threshold = a.parse().unwrap_or(def_threshold);
                    }
                }
                "gravity" => {
                    if let Some(a) = p.args.first() {
                        gravity = a.parse().unwrap_or(gravity);
                    }
                }
                "friction" => {
                    if let Some(a) = p.args.first() {
                        friction = a.parse::<f64>().unwrap_or(friction).max(0.0);
                    }
                }
                "jumpz" => {
                    if let Some(a) = p.args.first() {
                        jump_z = a.parse().unwrap_or(jump_z);
                    }
                }
                "player.forwardmove" => {
                    if let Some(a) = p.args.first() {
                        forward_move = a.parse::<f64>().unwrap_or(forward_move).max(0.0);
                    }
                }
                "ripperlevel" => {
                    if let Some(a) = p.args.first() {
                        ripper_level = a.parse().unwrap_or(ripper_level).max(0);
                    }
                }
                "riplevelmin" => {
                    if let Some(a) = p.args.first() {
                        rip_level_min = a.parse().unwrap_or(rip_level_min).max(0);
                    }
                }
                "riplevelmax" => {
                    if let Some(a) = p.args.first() {
                        rip_level_max = a.parse().unwrap_or(rip_level_max).max(0);
                    }
                }
                "floatbobstrength" => {
                    if let Some(a) = p.args.first() {
                        float_bob_strength = a.parse().unwrap_or(float_bob_strength);
                    }
                }
                "maxtargetrange" => {
                    if let Some(a) = p.args.first() {
                        max_target_range = a.parse().unwrap_or(max_target_range);
                    }
                }
                "minmissilechance" => {
                    if let Some(a) = p.args.first() {
                        min_missile_chance = a.parse().unwrap_or(min_missile_chance);
                    }
                }
                "damagefactor" => match p.args.as_slice() {
                    [ty, val] => {
                        if let Ok(f) = val.parse::<f64>() {
                            typed_damage_factors
                                .insert(ty.trim_matches('"').to_ascii_lowercase(), f);
                        }
                    }
                    [val] => {
                        damage_factor = val.parse().unwrap_or(damage_factor);
                    }
                    _ => {}
                },
                "damagetype" => {
                    if let Some(a) = p.args.first() {
                        damage_type = a.trim_matches('"').to_owned();
                    }
                }
                "floatspeed" => {
                    if let Some(a) = p.args.first() {
                        float_speed = a.parse().unwrap_or(float_speed);
                    }
                }
                "fastspeed" => {
                    if let Some(a) = p.args.first() {
                        fast_speed = a.parse().unwrap_or(fast_speed);
                    }
                }
                "maxstepheight" => {
                    if let Some(a) = p.args.first() {
                        max_step_height = a.parse().unwrap_or(max_step_height);
                    }
                }
                "maxdropoffheight" => {
                    if let Some(a) = p.args.first() {
                        max_dropoff_height = a.parse().unwrap_or(max_dropoff_height);
                    }
                }
                "species" => {
                    if let Some(a) = p.args.first() {
                        species = a.trim_matches('"').to_owned();
                    }
                }
                "infightinggroup" => {
                    if let Some(a) = p.args.first() {
                        infighting_group = a.parse().unwrap_or(infighting_group);
                    }
                }
                "projectilegroup" => {
                    if let Some(a) = p.args.first() {
                        projectile_group = a.parse().unwrap_or(projectile_group);
                    }
                }
                "splashgroup" => {
                    if let Some(a) = p.args.first() {
                        splash_group = a.parse().unwrap_or(splash_group);
                    }
                }
                "designatedteam" => {
                    if let Some(a) = p.args.first() {
                        if let Ok(v) = a.parse::<u8>() {
                            designated_team = v;
                        }
                    }
                }
                "viewheight" => {
                    if let Some(a) = p.args.first() {
                        view_height = a.parse().unwrap_or(view_height);
                    }
                }
                "cameraheight" => {
                    if let Some(a) = p.args.first() {
                        camera_height = a.parse().unwrap_or(camera_height);
                    }
                }
                "camerafov" => {
                    if let Some(a) = p.args.first() {
                        camera_fov = a.parse().unwrap_or(camera_fov);
                    }
                }
                "attackzoffset" => {
                    if let Some(a) = p.args.first() {
                        attack_z_offset = a.parse().unwrap_or(attack_z_offset);
                    }
                }
                "obituary" => {
                    if let Some(a) = p.args.first() {
                        obituary = a.trim_matches('"').to_owned();
                    }
                }
                "hitobituary" => {
                    if let Some(a) = p.args.first() {
                        hit_obituary = a.trim_matches('"').to_owned();
                    }
                }
                "bloodcolor" => {
                    if let Some(rgb) = parse_blood_color(&p.args) {
                        blood_color = Some(rgb);
                    }
                }
                "bloodtype" => {
                    blood_type = p
                        .args
                        .iter()
                        .take(3)
                        .map(|a| a.trim_matches('"').to_owned())
                        .filter(|s| !s.is_empty())
                        .collect();
                }
                "fillcolor" => {
                    if let Some(rgb) = parse_blood_color(&p.args) {
                        fill_color = Some(rgb);
                    }
                }
                "scale" => {
                    if let Some(a) = p.args.first() {
                        if let Ok(v) = a.parse::<f64>() {
                            scale_x = v;
                            scale_y = v;
                        }
                    }
                }
                "xscale" => {
                    if let Some(a) = p.args.first() {
                        scale_x = a.parse().unwrap_or(scale_x);
                    }
                }
                "yscale" => {
                    if let Some(a) = p.args.first() {
                        scale_y = a.parse().unwrap_or(scale_y);
                    }
                }
                "alpha" => {
                    if let Some(a) = p.args.first() {
                        alpha = a.parse::<f64>().unwrap_or(alpha).clamp(0.0, 1.0);
                    }
                }
                "renderstyle" => {
                    if let Some(a) = p.args.first() {
                        if let Some(rs) = RenderStyle::parse(a) {
                            render_style = rs;
                        }
                    }
                }
                "translation" => {
                    if let Some(a) = p.args.first() {
                        translation = a.trim_matches('"').to_owned();
                    }
                }
                "inventory.amount" => {
                    if let Some(a) = p.args.first() {
                        inventory_amount = a.parse().unwrap_or(inventory_amount);
                    }
                }
                "inventory.maxamount" => {
                    if let Some(a) = p.args.first() {
                        inventory_max_amount = a.parse().unwrap_or(inventory_max_amount);
                    }
                }
                "startbullets" => {
                    if let Some(a) = p.args.first() {
                        start_bullets = a.parse().unwrap_or(start_bullets);
                    }
                }
                "inventory.pickupmessage" => {
                    if let Some(a) = p.args.first() {
                        pickup_message = a.trim_matches('"').to_owned();
                    }
                }
                "inventory.pickupsound" => {
                    if let Some(a) = p.args.first() {
                        pickup_sound = a.trim_matches('"').to_owned();
                    }
                }
                "painchance" => match p.args.as_slice() {
                    [ty, val] => {
                        if let Ok(n) = val.parse::<i32>() {
                            typed_pain_chances.insert(ty.trim_matches('"').to_ascii_lowercase(), n);
                        }
                    }
                    [val] => {
                        pain_chance = val.parse().unwrap_or(pain_chance);
                    }
                    _ => {}
                },
                "painthreshold" => {
                    if let Some(a) = p.args.first() {
                        pain_threshold = a.parse().unwrap_or(pain_threshold);
                    }
                }
                "damage" => {
                    if let Some(a) = p.args.first() {
                        damage = a.parse().unwrap_or(damage);
                    }
                }
                "contactdamage" => {
                    if let Some(a) = p.args.first() {
                        contact_damage = a.parse().unwrap_or(contact_damage);
                    }
                }
                "killtaunt" => {
                    if !own_kill_taunts {
                        kill_taunts.clear();
                        own_kill_taunts = true;
                    }
                    if let Some(a) = p.args.first() {
                        let name = a.trim_matches('"').to_owned();
                        if !name.is_empty() {
                            kill_taunts.push(name);
                        }
                    }
                }
                "idletaunt" => {
                    if !own_idle_taunts {
                        idle_taunts.clear();
                        own_idle_taunts = true;
                    }
                    if let Some(a) = p.args.first() {
                        let name = a.trim_matches('"').to_owned();
                        if !name.is_empty() {
                            idle_taunts.push(name);
                        }
                    }
                }
                "weapon.ammotype" => {
                    is_weapon = true;
                    if let Some(a) = p.args.first() {
                        ammo_type = a.trim_matches('"').to_owned();
                    }
                }
                "weapon.ammouse" => {
                    is_weapon = true;
                    if let Some(a) = p.args.first() {
                        ammo_use = a.parse().unwrap_or(ammo_use);
                    }
                }
                "weapon.ammogive" => {
                    is_weapon = true;
                    if let Some(a) = p.args.first() {
                        ammo_give = a.parse().unwrap_or(ammo_give);
                    }
                }
                "player.startitem" => {
                    if !own_start_items {
                        start_items.clear();
                        own_start_items = true;
                    }
                    if let Some(a) = p.args.first() {
                        let amount = p.args.get(1).and_then(|n| n.parse().ok()).unwrap_or(1);
                        start_items.push((a.trim_matches('"').to_owned(), amount));
                    }
                }
                "dropitem" => {
                    if let Some(a) = p.args.first() {
                        drop_items.push(a.trim_matches('"').to_owned());
                    }
                }
                "seesound" => {
                    if let Some(a) = p.args.first() {
                        see_sound = a.trim_matches('"').to_owned();
                    }
                }
                "deathsound" => {
                    if let Some(a) = p.args.first() {
                        death_sound = a.trim_matches('"').to_owned();
                    }
                }
                "painsound" => {
                    if let Some(a) = p.args.first() {
                        pain_sound = a.trim_matches('"').to_owned();
                    }
                }
                "crushpainsound" => {
                    if let Some(a) = p.args.first() {
                        crush_pain_sound = a.trim_matches('"').to_owned();
                    }
                }
                "howlsound" => {
                    if let Some(a) = p.args.first() {
                        howl_sound = a.trim_matches('"').to_owned();
                    }
                }
                "bouncesound" => {
                    if let Some(a) = p.args.first() {
                        bounce_sound = a.trim_matches('"').to_owned();
                    }
                }
                "wallbouncesound" => {
                    if let Some(a) = p.args.first() {
                        wall_bounce_sound = a.trim_matches('"').to_owned();
                    }
                }
                "bouncefactor" => {
                    if let Some(a) = p.args.first() {
                        bounce_factor = a.parse::<f64>().unwrap_or(bounce_factor).clamp(0.0, 1.0);
                    }
                }
                "pushsound" => {
                    if let Some(a) = p.args.first() {
                        push_sound = a.trim_matches('"').to_owned();
                    }
                }
                "usesound" => {
                    if let Some(a) = p.args.first() {
                        use_sound = a.trim_matches('"').to_owned();
                    }
                }
                "activesound" => {
                    if let Some(a) = p.args.first() {
                        active_sound = a.trim_matches('"').to_owned();
                    }
                }
                "attacksound" => {
                    if let Some(a) = p.args.first() {
                        attack_sound = a.trim_matches('"').to_owned();
                    }
                }
                "soundclass" => {
                    if let Some(a) = p.args.first() {
                        sound_class = a.trim_matches('"').to_owned();
                    }
                }
                "friendlyseeblocks" => {
                    if let Some(a) = p.args.first() {
                        friendly_see_blocks = a.parse().unwrap_or(friendly_see_blocks);
                    }
                }
                name if name.starts_with("weapon.") => is_weapon = true,
                "monster" => {
                    flags |= FLAG_SOLID | FLAG_SHOOTABLE | FLAG_COUNTKILL | FLAG_CANUSEWALLS;
                }
                _ => {}
            }
        }
        // Inventory pickups with MaxAmount > 1 count as items.
        if inventory_max_amount > 1 || is_weapon {
            flags |= FLAG_COUNTITEM;
        }
        if def.states.as_ref().is_some_and(|s| {
            s.items
                .iter()
                .any(|i| matches!(i, StateItem::Label(l) if l.eq_ignore_ascii_case("Fire")))
        }) {
            is_weapon = true;
        }
        for f in &def.flags {
            apply_flag(&mut flags, f);
        }
        let spawn_tics = spawn_tics_from_states(def).unwrap_or(parent.spawn_tics);
        // Skip redefining Actor if present in IR.
        if def.name.eq_ignore_ascii_case("Actor") {
            let id = ClassId::from_raw(0);
            let slot = &mut self.classes[0];
            slot.tag = tag;
            slot.health = health;
            slot.gib_health = gib_health;
            slot.wound_health = wound_health;
            slot.death_height = death_height;
            slot.burn_height = burn_height;
            slot.radius = radius;
            slot.height = height;
            slot.speed = speed;
            slot.melee_range = melee_range;
            slot.melee_threshold = melee_threshold;
            slot.mass = mass;
            slot.reaction_time = reaction_time;
            slot.threshold = threshold;
            slot.def_threshold = def_threshold;
            slot.gravity = gravity;
            slot.friction = friction;
            slot.jump_z = jump_z;
            slot.forward_move = forward_move;
            slot.ripper_level = ripper_level;
            slot.rip_level_min = rip_level_min;
            slot.rip_level_max = rip_level_max;
            slot.float_bob_strength = float_bob_strength;
            slot.max_target_range = max_target_range;
            slot.min_missile_chance = min_missile_chance;
            slot.damage_factor = damage_factor;
            slot.typed_damage_factors = typed_damage_factors;
            slot.damage_type = damage_type;
            slot.float_speed = float_speed;
            slot.fast_speed = fast_speed;
            slot.max_step_height = max_step_height;
            slot.max_dropoff_height = max_dropoff_height;
            slot.species = species;
            slot.infighting_group = infighting_group;
            slot.projectile_group = projectile_group;
            slot.splash_group = splash_group;
            slot.designated_team = designated_team;
            slot.view_height = view_height;
            slot.camera_height = camera_height;
            slot.camera_fov = camera_fov;
            slot.attack_z_offset = attack_z_offset;
            slot.obituary = obituary;
            slot.hit_obituary = hit_obituary;
            slot.blood_color = blood_color;
            slot.blood_type = blood_type;
            slot.fill_color = fill_color;
            slot.scale_x = scale_x;
            slot.scale_y = scale_y;
            slot.alpha = alpha;
            slot.render_style = render_style;
            slot.translation = translation;
            slot.flags = flags;
            slot.spawn_tics = spawn_tics;
            slot.inventory_amount = inventory_amount;
            slot.inventory_max_amount = inventory_max_amount;
            slot.start_bullets = start_bullets;
            slot.pickup_message = pickup_message;
            slot.pickup_sound = pickup_sound;
            slot.is_weapon = is_weapon;
            slot.pain_chance = pain_chance;
            slot.pain_threshold = pain_threshold;
            slot.typed_pain_chances = typed_pain_chances;
            slot.damage = damage;
            slot.contact_damage = contact_damage;
            slot.kill_taunts = kill_taunts;
            slot.idle_taunts = idle_taunts;
            slot.ammo_type = ammo_type;
            slot.ammo_use = ammo_use;
            slot.ammo_give = ammo_give;
            slot.drop_items = drop_items;
            slot.start_items = start_items;
            slot.see_sound = see_sound;
            slot.death_sound = death_sound;
            slot.pain_sound = pain_sound;
            slot.crush_pain_sound = crush_pain_sound;
            slot.howl_sound = howl_sound;
            slot.active_sound = active_sound;
            slot.attack_sound = attack_sound;
            slot.sound_class = sound_class;
            slot.friendly_see_blocks = friendly_see_blocks;
            slot.bounce_sound = bounce_sound;
            slot.wall_bounce_sound = wall_bounce_sound;
            slot.bounce_factor = bounce_factor;
            slot.push_sound = push_sound;
            slot.use_sound = use_sound;
            slot.doomednum = def.doomednum.or(slot.doomednum);
            return Ok(id);
        }
        self.push(ClassDef {
            id: ClassId::from_raw(0),
            name: def.name.clone(),
            tag,
            parent: Some(parent_id),
            doomednum: def.doomednum,
            health,
            gib_health,
            wound_health,
            death_height,
            burn_height,
            radius,
            height,
            speed,
            melee_range,
            melee_threshold,
            mass,
            reaction_time,
            threshold,
            def_threshold,
            gravity,
            friction,
            jump_z,
            forward_move,
            ripper_level,
            rip_level_min,
            rip_level_max,
            float_bob_strength,
            max_target_range,
            min_missile_chance,
            damage_factor,
            typed_damage_factors,
            damage_type,
            float_speed,
            fast_speed,
            max_step_height,
            max_dropoff_height,
            species,
            infighting_group,
            projectile_group,
            splash_group,
            designated_team,
            view_height,
            camera_height,
            camera_fov,
            attack_z_offset,
            obituary,
            hit_obituary,
            blood_color,
            blood_type,
            fill_color,
            scale_x,
            scale_y,
            alpha,
            render_style,
            translation,
            flags,
            spawn_tics,
            inventory_amount,
            inventory_max_amount,
            start_bullets,
            pickup_message,
            pickup_sound,
            is_weapon,
            pain_chance,
            pain_threshold,
            typed_pain_chances,
            damage,
            contact_damage,
            kill_taunts,
            idle_taunts,
            ammo_type,
            ammo_use,
            ammo_give,
            drop_items,
            see_sound,
            death_sound,
            pain_sound,
            crush_pain_sound,
            howl_sound,
            active_sound,
            attack_sound,
            sound_class,
            friendly_see_blocks,
            bounce_sound,
            wall_bounce_sound,
            bounce_factor,
            push_sound,
            use_sound,
            start_items,
        })
        .inspect(|&id| self.apply_replaces(id, def.replaces.as_deref()))
    }

    /// `replaces X`: spawns of `X` (and things placed as `X`) become `id`.
    fn apply_replaces(&mut self, id: ClassId, target: Option<&str>) {
        let Some(old) = target.and_then(|t| self.find_name(t)) else {
            return;
        };
        if old != id {
            self.replaced_by.insert(old, id);
        }
        let Some(dn) = self.classes[old.raw() as usize].doomednum else {
            return;
        };
        if self.classes[id.raw() as usize].doomednum.is_none() {
            self.classes[id.raw() as usize].doomednum = Some(dn);
            self.by_doomednum.insert(dn, id);
        }
    }

    /// Final class a spawn of `id` produces after `replaces` chains.
    #[must_use]
    pub fn replacement(&self, id: ClassId) -> ClassId {
        let mut cur = id;
        for _ in 0..16 {
            match self.replaced_by.get(&cur) {
                Some(&next) if next != id => cur = next,
                _ => break,
            }
        }
        cur
    }

    /// Case-insensitive name lookup.
    #[must_use]
    pub fn find_name(&self, name: &str) -> Option<ClassId> {
        self.by_name.get(&name.to_ascii_lowercase()).copied()
    }

    /// DoomEdNum → class.
    #[must_use]
    pub fn find_doomednum(&self, num: i32) -> Option<ClassId> {
        self.by_doomednum.get(&num).copied()
    }

    /// Borrow class.
    #[must_use]
    pub fn get(&self, id: ClassId) -> Option<&ClassDef> {
        self.classes.get(id.raw() as usize)
    }

    /// Mutable class access (editor value overrides; not used during a tic).
    pub fn get_mut(&mut self, id: ClassId) -> Option<&mut ClassDef> {
        self.classes.get_mut(id.raw() as usize)
    }

    /// All classes in id order.
    pub fn iter(&self) -> impl Iterator<Item = &ClassDef> {
        self.classes.iter()
    }

    /// Class count.
    #[must_use]
    pub fn len(&self) -> usize {
        self.classes.len()
    }

    /// Empty?
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.classes.is_empty()
    }
}

fn apply_script_prop_i32(def: &mut ClassDef, prop: u32, value: i32) {
    match prop {
        x if x == PropId::Health as u32 => def.health = value,
        x if x == PropId::Speed as u32 => def.speed = f64::from(value),
        x if x == PropId::PainChance as u32 => def.pain_chance = value,
        x if x == PropId::Radius as u32 => def.radius = f64::from(value),
        x if x == PropId::Height as u32 => def.height = f64::from(value),
        x if x == PropId::Mass as u32 => def.mass = value,
        x if x == PropId::ReactionTime as u32 => def.reaction_time = value,
        x if x == PropId::MeleeRange as u32 => def.melee_range = f64::from(value),
        x if x == PropId::DamageFactorPercent as u32 => {
            def.damage_factor = f64::from(value.max(0)) / 100.0;
        }
        x if x == PropId::ScalePercent as u32 => {
            let s = f64::from(value.max(1)) / 100.0;
            def.scale_x = s;
            def.scale_y = s;
        }
        x if x == PropId::FastSpeed as u32 => def.fast_speed = f64::from(value),
        x if x == PropId::AlphaPercent as u32 => {
            def.alpha = f64::from(value.clamp(0, 100)) / 100.0;
        }
        _ => {}
    }
}

fn apply_script_prop_str(def: &mut ClassDef, prop: u32, value: &str) {
    match prop {
        x if x == PropId::Translation as u32 => def.translation = value.to_owned(),
        x if x == PropId::Tag as u32 => def.tag = value.to_owned(),
        x if x == PropId::Obituary as u32 => def.obituary = value.to_owned(),
        _ => {}
    }
}

fn apply_script_flag(def: &mut ClassDef, flag: u32, on: bool) {
    // ABI FlagId → engine bit (`room-script-sdk::flag`).
    let bit = match flag {
        1 => FLAG_SOLID,
        2 => FLAG_SHOOTABLE,
        3 => FLAG_COUNTKILL,
        4 => FLAG_FRIENDLY,
        5 => FLAG_NOGRAVITY,
        6 => FLAG_FLOAT,
        7 => FLAG_NOBLOOD,
        8 => FLAG_BOSS,
        9 => FLAG_NORADIUSDMG,
        10 => FLAG_INVULNERABLE,
        11 => FLAG_DONTDRAW,
        _ => return,
    };
    if on {
        def.flags |= bit;
    } else {
        def.flags &= !bit;
    }
}

/// Blocking collision (`MF_SOLID`).
pub const FLAG_SOLID: u64 = 1 << 0;
/// Takes damage (`MF_SHOOTABLE`).
pub const FLAG_SHOOTABLE: u64 = 1 << 1;
/// Count toward kill % (`MF_COUNTKILL`).
pub const FLAG_COUNTKILL: u64 = 1 << 2;
/// Count toward item % (`MF_COUNTITEM`).
pub const FLAG_COUNTITEM: u64 = 1 << 3;
/// Projectile (`MF_MISSILE`).
pub const FLAG_MISSILE: u64 = 1 << 4;
/// Ignore gravity (`MF_NOGRAVITY`).
pub const FLAG_NOGRAVITY: u64 = 1 << 5;
/// Deaf / ambush — ignore sound wake (`MF_AMBUSH`).
pub const FLAG_AMBUSH: u64 = 1 << 6;
/// Floating / flying (`MF_FLOAT`).
pub const FLAG_FLOAT: u64 = 1 << 7;
/// Can walk off ledges (`MF_DROPOFF`).
pub const FLAG_DROPOFF: u64 = 1 << 8;
/// Immune to radius/`A_Explode` damage (`MF2_NORADIUSDMG` / `+NORADIUSDMG`).
pub const FLAG_NORADIUSDMG: u64 = 1 << 9;
/// Skip splash/hitscan knockback (`MF7_DONTTHRUST` / `+DONTTHRUST`).
pub const FLAG_DONTTHRUST: u64 = 1 << 10;
/// Completely ignore damage (`MF5_NODAMAGE` / `+NODAMAGE`).
pub const FLAG_NODAMAGE: u64 = 1 << 11;
/// Invulnerable (`MF2_INVULNERABLE` / `+INVULNERABLE`).
pub const FLAG_INVULNERABLE: u64 = 1 << 12;
/// Frozen until woken (`MF2_DORMANT` / `+DORMANT`).
pub const FLAG_DORMANT: u64 = 1 << 13;
/// Monsters ignore this actor as a target (`MF_NOTARGET` / `+NOTARGET`).
pub const FLAG_NOTARGET: u64 = 1 << 14;
/// Allied to the player (`MF_FRIENDLY` / `+FRIENDLY`) — won't hunt players.
pub const FLAG_FRIENDLY: u64 = 1 << 15;
/// Actor does not spawn blood when damaged.
pub const FLAG_NOBLOOD: u64 = 1 << 16;
/// Bob vertically while floating (`MF2_FLOATBOB` / `+FLOATBOB`).
pub const FLAG_FLOATBOB: u64 = 1 << 17;
/// Don't harm (or take harm from) same `Species` (`MF6_DONTHARMSPECIES`).
pub const FLAG_DONTHARMSPECIES: u64 = 1 << 18;
/// Don't harm (or take harm from) the same class (`MF6_DONTHARMCLASS`).
pub const FLAG_DONTHARMCLASS: u64 = 1 << 19;
/// Boss actor (full-volume see/death cues; Cyberdemon / Spider Mastermind).
pub const FLAG_BOSS: u64 = 1 << 20;
/// Spawn hanging from ceiling (`MF_SPAWNCEILING` / `+SPAWNCEILING`).
pub const FLAG_SPAWNCEILING: u64 = 1 << 21;
/// Not linked for thing-thing collision (`MF_NOBLOCKMAP` / `+NOBLOCKMAP`).
pub const FLAG_NOBLOCKMAP: u64 = 1 << 22;
/// Feet clipped into floor for draw (`MF_FLOORCLIP` / `+FLOORCLIP`).
pub const FLAG_FLOORCLIP: u64 = 1 << 23;
/// Dropped by a dying actor (`MF_DROPPED` / `+DROPPED`) — half ammo on pickup.
pub const FLAG_DROPPED: u64 = 1 << 24;
/// May activate use-special linedefs while chasing (`MF_CANUSEWALLS` / `+CANUSEWALLS`).
pub const FLAG_CANUSEWALLS: u64 = 1 << 25;
/// Just took damage — force a missile check next chase (`MF_JUSTHIT`).
pub const FLAG_JUSTHIT: u64 = 1 << 26;
/// Just fired — skip a chase attack and re-path (`MF_JUSTATTACKED`).
pub const FLAG_JUSTATTACKED: u64 = 1 << 27;
/// Dead body (`MF_CORPSE`) — set in `P_KillMobj`; enables ledge slide friction skip.
pub const FLAG_CORPSE: u64 = 1 << 28;
/// Invisible to the renderer (`MF2_DONTDRAW` / `+DONTDRAW`).
pub const FLAG_DONTDRAW: u64 = 1 << 29;
/// Flee from target (`MF_FRIGHTENED` / `+FRIGHTENED`).
pub const FLAG_FRIGHTENED: u64 = 1 << 30;
/// Do not activate lines (`MF2_NOTRIGGER` / `+NOTRIGGER`).
pub const FLAG_NOTRIGGER: u64 = 1 << 31;
/// Immune to ripper projectiles (`+DONTRIP`).
pub const FLAG_DONTRIP: u64 = 1 << 32;
/// Projectile rips through shootables (`MF2_RIP` / `+RIPPER`).
pub const FLAG_RIPPER: u64 = 1 << 33;
/// Drawn as a plane along the actor's facing, not a billboard (`+WALLSPRITE`).
pub const FLAG_WALLSPRITE: u64 = 1 << 34;
/// Drawn flat on the floor / ceiling, rotated by the actor angle (`+FLATSPRITE`).
pub const FLAG_FLATSPRITE: u64 = 1 << 35;
/// Reflects off floors and ceilings instead of stopping (`+BOUNCEONFLOORS`).
pub const FLAG_BOUNCEONFLOORS: u64 = 1 << 36;
/// Reflects off walls instead of sliding (`+BOUNCEONWALLS`).
pub const FLAG_BOUNCEONWALLS: u64 = 1 << 37;
/// Player Use runs this actor's `Use` state (`+USESPECIAL`).
pub const FLAG_USESPECIAL: u64 = 1 << 38;

fn apply_flag(flags: &mut u64, op: &FlagOp) {
    let bit = match op.name.to_ascii_uppercase().as_str() {
        "SOLID" => FLAG_SOLID,
        "SHOOTABLE" => FLAG_SHOOTABLE,
        "COUNTKILL" => {
            // Vanilla countkill monsters open doors; keep CANUSEWALLS unless cleared later.
            *flags |= FLAG_CANUSEWALLS;
            FLAG_COUNTKILL
        }
        "CANUSEWALLS" => FLAG_CANUSEWALLS,
        "COUNTITEM" => FLAG_COUNTITEM,
        "MISSILE" => FLAG_MISSILE,
        "NOGRAVITY" => FLAG_NOGRAVITY,
        "AMBUSH" => FLAG_AMBUSH,
        "FLOAT" => FLAG_FLOAT,
        "DROPOFF" => FLAG_DROPOFF,
        "NORADIUSDMG" => FLAG_NORADIUSDMG,
        "DONTTHRUST" => FLAG_DONTTHRUST,
        "NODAMAGE" => FLAG_NODAMAGE,
        "INVULNERABLE" => FLAG_INVULNERABLE,
        "DORMANT" => FLAG_DORMANT,
        "NOTARGET" => FLAG_NOTARGET,
        "FRIENDLY" => FLAG_FRIENDLY,
        "NOBLOOD" => FLAG_NOBLOOD,
        "FLOATBOB" => FLAG_FLOATBOB,
        "DONTHARMSPECIES" => FLAG_DONTHARMSPECIES,
        "DONTHARMCLASS" => FLAG_DONTHARMCLASS,
        "BOSS" => FLAG_BOSS,
        "SPAWNCEILING" => FLAG_SPAWNCEILING,
        "NOBLOCKMAP" => FLAG_NOBLOCKMAP,
        "FLOORCLIP" => FLAG_FLOORCLIP,
        "DROPPED" => FLAG_DROPPED,
        "CORPSE" => FLAG_CORPSE,
        "DONTDRAW" | "INVISIBLE" => FLAG_DONTDRAW,
        "FRIGHTENED" => FLAG_FRIGHTENED,
        "NOTRIGGER" => FLAG_NOTRIGGER,
        "DONTRIP" => FLAG_DONTRIP,
        "RIPPER" => FLAG_RIPPER,
        "WALLSPRITE" => FLAG_WALLSPRITE,
        "FLATSPRITE" => FLAG_FLATSPRITE,
        "BOUNCEONFLOORS" | "BOUNCEONCEILINGS" => FLAG_BOUNCEONFLOORS,
        "BOUNCEONWALLS" => FLAG_BOUNCEONWALLS,
        "USESPECIAL" => FLAG_USESPECIAL,
        "DOOMBOUNCE" | "HEXENBOUNCE" => FLAG_BOUNCEONFLOORS | FLAG_BOUNCEONWALLS,
        "*" => {
            if !op.set {
                *flags = 0;
            }
            return;
        }
        _ => return,
    };
    if op.set {
        *flags |= bit;
    } else {
        *flags &= !bit;
    }
}

/// Parse `BloodColor` / `A_SetBloodColor` args: `r g b` or a known colour name.
pub(crate) fn parse_blood_color(args: &[String]) -> Option<[u8; 3]> {
    match args {
        [r, g, b] => {
            let r = r.parse().ok()?;
            let g = g.parse().ok()?;
            let b = b.parse().ok()?;
            Some([r, g, b])
        }
        [name] => {
            let key = name.trim_matches('"').to_ascii_lowercase();
            match key.as_str() {
                "red" => Some([255, 0, 0]),
                "green" => Some([0, 255, 0]),
                "blue" => Some([0, 0, 255]),
                "yellow" => Some([255, 255, 0]),
                "purple" | "violet" => Some([255, 0, 255]),
                "white" => Some([255, 255, 255]),
                "orange" => Some([255, 128, 0]),
                "black" => Some([0, 0, 0]),
                "grey" | "gray" => Some([128, 128, 128]),
                _ => None,
            }
        }
        _ => None,
    }
}

fn spawn_tics_from_states(def: &ActorDef) -> Option<i32> {
    let items = &def.states.as_ref()?.items;
    let mut in_spawn = false;
    for item in items {
        match item {
            StateItem::Label(l) if l.eq_ignore_ascii_case("Spawn") => in_spawn = true,
            StateItem::Label(_) => {
                if in_spawn {
                    break;
                }
            }
            StateItem::Line(line) if in_spawn => return Some(line.duration),
            StateItem::Flow(StateFlow::Stop | StateFlow::Wait | StateFlow::Fail) if in_spawn => {
                return Some(-1);
            }
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use room_decorate::parse_decorate;

    #[test]
    fn registry_from_zombieman() {
        let ir = parse_decorate(
            r#"
actor ZombieMan : Actor 3004
{
  Health 20
  Radius 20
  Height 56
  +SOLID
  +SHOOTABLE
  +FLOORCLIP
  States
  {
  Spawn:
    POSS AB 10 A_Look
    Loop
  }
}
"#,
        )
        .unwrap();
        let reg = ClassRegistry::from_decorate(&ir).unwrap();
        let id = reg.find_doomednum(3004).unwrap();
        let c = reg.get(id).unwrap();
        assert_eq!(c.name, "ZombieMan");
        assert_eq!(c.health, 20);
        assert_eq!(c.height, 56.0);
        assert_eq!(c.spawn_tics, 10);
        assert_eq!(c.flags & FLAG_SOLID, FLAG_SOLID);
        assert_eq!(c.flags & FLAG_FLOORCLIP, FLAG_FLOORCLIP);
        assert_eq!(c.display_name(), "ZombieMan");
    }

    #[test]
    fn later_doomednum_and_replaces_take_over_spawns() {
        let ir = parse_decorate(
            "actor ZombieMan 3004 { Health 20 }\n\
             actor Imp 3001 { Health 60 }\n\
             actor FastZombie : ZombieMan 3004 { Health 5 }\n\
             actor BigImp : Imp replaces Imp { Health 600 }\n",
        )
        .unwrap();
        let reg = ClassRegistry::from_decorate(&ir).unwrap();
        let z = reg.get(reg.find_doomednum(3004).unwrap()).unwrap();
        assert_eq!(z.name, "FastZombie");
        assert_eq!(z.health, 5);
        let imp = reg.get(reg.find_doomednum(3001).unwrap()).unwrap();
        assert_eq!(imp.name, "BigImp");
        assert_eq!(imp.health, 600);
        let by_name = reg.replacement(reg.find_name("Imp").unwrap());
        assert_eq!(reg.get(by_name).unwrap().name, "BigImp");
        let z = reg.find_name("ZombieMan").unwrap();
        assert_eq!(reg.replacement(z), z);
    }

    #[test]
    fn use_sound_parsed_from_decorate() {
        let ir = parse_decorate(
            r#"
actor User : Actor 9983
{
  UseSound "misc/i_pkup"
  States { Spawn: POSS A -1 Stop }
}
"#,
        )
        .unwrap();
        let reg = ClassRegistry::from_decorate(&ir).unwrap();
        let c = reg.get(reg.find_name("User").unwrap()).unwrap();
        assert_eq!(c.use_sound, "misc/i_pkup");
    }

    #[test]
    fn bounce_and_push_sounds_parsed_from_decorate() {
        let ir = parse_decorate(
            r#"
actor Bouncer : Actor 9982
{
  BounceSound "weapons/bounce"
  WallBounceSound "weapons/grbnce"
  PushSound "misc/push"
  States { Spawn: POSS A -1 Stop }
}
"#,
        )
        .unwrap();
        let reg = ClassRegistry::from_decorate(&ir).unwrap();
        let c = reg.get(reg.find_name("Bouncer").unwrap()).unwrap();
        assert_eq!(c.bounce_sound, "weapons/bounce");
        assert_eq!(c.wall_bounce_sound, "weapons/grbnce");
        assert_eq!(c.push_sound, "misc/push");
    }

    #[test]
    fn howl_sound_parsed_from_decorate() {
        let ir = parse_decorate(
            r#"
actor LostSoul : Actor 3006
{
  HowlSound "skull/howl"
  States { Spawn: SKUL A -1 Stop }
}
"#,
        )
        .unwrap();
        let reg = ClassRegistry::from_decorate(&ir).unwrap();
        let c = reg.get(reg.find_name("LostSoul").unwrap()).unwrap();
        assert_eq!(c.howl_sound, "skull/howl");
    }

    #[test]
    fn friendly_see_blocks_from_decorate() {
        let ir = room_decorate::parse_decorate(
            r#"
actor Scout : Actor 9951
{
  FriendlySeeBlocks 3
  States { Spawn: POSS A -1 Stop }
}
"#,
        )
        .unwrap();
        let reg = ClassRegistry::from_decorate(&ir).unwrap();
        let c = reg.get(reg.find_name("Scout").unwrap()).unwrap();
        assert_eq!(c.friendly_see_blocks, 3);
    }

    #[test]
    fn crush_pain_sound_parsed_from_decorate() {
        let ir = parse_decorate(
            r#"
actor SoftTarget : Actor 9979
{
  CrushPainSound "misc/crush"
  States { Spawn: POSS A -1 Stop }
}
"#,
        )
        .unwrap();
        let reg = ClassRegistry::from_decorate(&ir).unwrap();
        let c = reg.get(reg.find_name("SoftTarget").unwrap()).unwrap();
        assert_eq!(c.crush_pain_sound, "misc/crush");
    }

    #[test]
    fn death_height_parsed_from_decorate() {
        let ir = parse_decorate(
            r#"
actor CorpseProps : Actor 9978
{
  Height 56
  DeathHeight 8
  BurnHeight 4
  States { Spawn: POSS A -1 Stop }
}
"#,
        )
        .unwrap();
        let reg = ClassRegistry::from_decorate(&ir).unwrap();
        let c = reg.get(reg.find_name("CorpseProps").unwrap()).unwrap();
        assert!((c.death_height - 8.0).abs() < 1e-9);
        assert!((c.burn_height - 4.0).abs() < 1e-9);
    }

    #[test]
    fn ripper_level_properties_from_decorate() {
        let ir = room_decorate::parse_decorate(
            r#"
actor RipBall : Actor 9950
{
  RipperLevel 2
  RipLevelMin 1
  RipLevelMax 4
  +DONTRIP
  +RIPPER
  States { Spawn: BAL1 A -1 Stop }
}
"#,
        )
        .unwrap();
        let reg = ClassRegistry::from_decorate(&ir).unwrap();
        let id = reg.find_name("RipBall").unwrap();
        let c = reg.get(id).unwrap();
        assert_eq!(c.ripper_level, 2);
        assert_eq!(c.rip_level_min, 1);
        assert_eq!(c.rip_level_max, 4);
        assert_ne!(c.flags & FLAG_DONTRIP, 0);
        assert_ne!(c.flags & FLAG_RIPPER, 0);
    }

    #[test]
    fn fill_color_parsed_from_decorate() {
        let ir = parse_decorate(
            r#"
actor StencilGuy : Actor 9977
{
  FillColor "Red"
  States { Spawn: POSS A -1 Stop }
}
"#,
        )
        .unwrap();
        let reg = ClassRegistry::from_decorate(&ir).unwrap();
        let c = reg.get(reg.find_name("StencilGuy").unwrap()).unwrap();
        assert_eq!(c.fill_color, Some([255, 0, 0]));
    }

    #[test]
    fn translation_parsed_from_decorate() {
        let ir = parse_decorate(
            r#"
actor IceZombie : Actor 9976
{
  Translation "Ice"
  States { Spawn: POSS A -1 Stop }
}
"#,
        )
        .unwrap();
        let reg = ClassRegistry::from_decorate(&ir).unwrap();
        let c = reg.get(reg.find_name("IceZombie").unwrap()).unwrap();
        assert_eq!(c.translation, "Ice");
    }

    #[test]
    fn designated_team_parsed_from_decorate() {
        let ir = parse_decorate(
            r#"
actor RedTeamBot : Actor 9985
{
  DesignatedTeam 1
  States { Spawn: POSS A -1 Stop }
}
"#,
        )
        .unwrap();
        let reg = ClassRegistry::from_decorate(&ir).unwrap();
        let c = reg.get(reg.find_name("RedTeamBot").unwrap()).unwrap();
        assert_eq!(c.designated_team, 1);
    }

    #[test]
    fn float_bob_parsed_from_decorate() {
        let ir = parse_decorate(
            r#"
actor Floater : Actor 9984
{
  +FLOATBOB
  FloatBobStrength 0.5
  States { Spawn: POSS A -1 Stop }
}
"#,
        )
        .unwrap();
        let reg = ClassRegistry::from_decorate(&ir).unwrap();
        let c = reg.get(reg.find_name("Floater").unwrap()).unwrap();
        assert_eq!(c.flags & FLAG_FLOATBOB, FLAG_FLOATBOB);
        assert!((c.float_bob_strength - 0.5).abs() < 1e-9);
    }

    #[test]
    fn camera_height_and_fov_parsed_from_decorate() {
        let ir = parse_decorate(
            r#"
actor SecurityCamera : Actor 9975
{
  CameraHeight 25
  CameraFOV 90
  States { Spawn: POSS A -1 Stop }
}
"#,
        )
        .unwrap();
        let reg = ClassRegistry::from_decorate(&ir).unwrap();
        let c = reg.get(reg.find_name("SecurityCamera").unwrap()).unwrap();
        assert!((c.camera_height - 25.0).abs() < 1e-9);
        assert!((c.camera_fov - 90.0).abs() < 1e-9);
    }

    #[test]
    fn noblood_flag_parsed_from_decorate() {
        let ir = parse_decorate(
            r#"
actor StoneGolem : Actor 9974
{
  +NOBLOOD
  States { Spawn: POSS A -1 Stop }
}
"#,
        )
        .unwrap();
        let reg = ClassRegistry::from_decorate(&ir).unwrap();
        let c = reg.get(reg.find_name("StoneGolem").unwrap()).unwrap();
        assert_eq!(c.flags & FLAG_NOBLOOD, FLAG_NOBLOOD);
    }

    #[test]
    fn boss_flag_parsed_from_decorate() {
        let ir = parse_decorate(
            r#"
actor Cyberdemon : Actor 16
{
  +BOSS
  States { Spawn: CYBR A -1 Stop }
}
"#,
        )
        .unwrap();
        let reg = ClassRegistry::from_decorate(&ir).unwrap();
        let c = reg.get(reg.find_name("Cyberdemon").unwrap()).unwrap();
        assert_eq!(c.flags & FLAG_BOSS, FLAG_BOSS);
    }

    #[test]
    fn blood_type_parsed_from_decorate() {
        let ir = parse_decorate(
            r#"
actor Alien : Actor 9973
{
  BloodType "GreenBlood", "GreenBloodSplatter"
  States { Spawn: POSS A -1 Stop }
}
"#,
        )
        .unwrap();
        let reg = ClassRegistry::from_decorate(&ir).unwrap();
        let c = reg.get(reg.find_name("Alien").unwrap()).unwrap();
        assert_eq!(
            c.blood_type,
            vec!["GreenBlood".to_owned(), "GreenBloodSplatter".to_owned()]
        );
    }

    #[test]
    fn blood_color_parsed_from_decorate() {
        let ir = parse_decorate(
            r#"
actor BlueBlood : Actor 9971
{
  BloodColor "Blue"
  States { Spawn: POSS A -1 Stop }
}
actor CustomBlood : Actor 9972
{
  BloodColor 10, 20, 30
  States { Spawn: POSS A -1 Stop }
}
"#,
        )
        .unwrap();
        let reg = ClassRegistry::from_decorate(&ir).unwrap();
        assert_eq!(
            reg.get(reg.find_name("BlueBlood").unwrap())
                .unwrap()
                .blood_color,
            Some([0, 0, 255])
        );
        assert_eq!(
            reg.get(reg.find_name("CustomBlood").unwrap())
                .unwrap()
                .blood_color,
            Some([10, 20, 30])
        );
    }

    #[test]
    fn threshold_parsed_from_decorate() {
        let ir = parse_decorate(
            r#"
actor Stubborn : Actor 9970
{
  Threshold 50
  DefThreshold 75
  States { Spawn: POSS A -1 Stop }
}
"#,
        )
        .unwrap();
        let reg = ClassRegistry::from_decorate(&ir).unwrap();
        let c = reg.get(reg.find_name("Stubborn").unwrap()).unwrap();
        assert_eq!(c.threshold, 50);
        assert_eq!(c.def_threshold, 75);
    }

    #[test]
    fn alpha_parsed_from_decorate() {
        let ir = parse_decorate(
            r#"
actor Ghost : Actor 9960
{
  Alpha 0.4
  States { Spawn: POSS A -1 Stop }
}
"#,
        )
        .unwrap();
        let reg = ClassRegistry::from_decorate(&ir).unwrap();
        let c = reg.get(reg.find_name("Ghost").unwrap()).unwrap();
        assert!((c.alpha - 0.4).abs() < 1e-9);
    }

    #[test]
    fn render_style_parsed_from_decorate() {
        let ir = parse_decorate(
            r#"
actor GhostAdd : Actor 9961
{
  RenderStyle Add
  Alpha 0.5
  States { Spawn: POSS A -1 Stop }
}
actor Spectre : Actor 9962
{
  RenderStyle Fuzzy
  States { Spawn: SARG A -1 Stop }
}
"#,
        )
        .unwrap();
        let reg = ClassRegistry::from_decorate(&ir).unwrap();
        let add = reg.get(reg.find_name("GhostAdd").unwrap()).unwrap();
        assert_eq!(add.render_style, RenderStyle::Add);
        assert!((add.alpha - 0.5).abs() < 1e-9);
        let fuzz = reg.get(reg.find_name("Spectre").unwrap()).unwrap();
        assert_eq!(fuzz.render_style, RenderStyle::Fuzzy);
    }

    #[test]
    fn scale_parsed_from_decorate() {
        let ir = parse_decorate(
            r#"
actor BigZombie : Actor 3004
{
  Radius 20
  Height 56
  Scale 2.0
  States { Spawn: POSS A -1 Stop }
}
actor WideZombie : Actor 3005
{
  Radius 20
  Height 56
  XScale 1.5
  YScale 0.5
  States { Spawn: POSS A -1 Stop }
}
"#,
        )
        .unwrap();
        let reg = ClassRegistry::from_decorate(&ir).unwrap();
        let big = reg.get(reg.find_name("BigZombie").unwrap()).unwrap();
        assert!((big.scale_x - 2.0).abs() < 1e-9);
        assert!((big.scale_y - 2.0).abs() < 1e-9);
        let wide = reg.get(reg.find_name("WideZombie").unwrap()).unwrap();
        assert!((wide.scale_x - 1.5).abs() < 1e-9);
        assert!((wide.scale_y - 0.5).abs() < 1e-9);
    }

    #[test]
    fn tag_overrides_display_name() {
        let ir = parse_decorate(
            r#"
actor ZombieMan : Actor 3004
{
  Tag "Former Human"
  Health 20
  States { Spawn: POSS A -1 Stop }
}
"#,
        )
        .unwrap();
        let reg = ClassRegistry::from_decorate(&ir).unwrap();
        let c = reg.get(reg.find_name("ZombieMan").unwrap()).unwrap();
        assert_eq!(c.tag, "Former Human");
        assert_eq!(c.display_name(), "Former Human");
    }
}
