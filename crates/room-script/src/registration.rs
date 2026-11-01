//! Registration-phase bookkeeping (`docs/12` §6).

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use crate::manifest::{ManifestOptions, OptionValue};

/// Seed `PropId` values (append-only; DECORATE property subset).
#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PropId {
    /// Health.
    Health = 1,
    /// Speed (map units per tic).
    Speed = 2,
    /// PainChance.
    PainChance = 3,
    /// Radius.
    Radius = 4,
    /// Height.
    Height = 5,
    /// Mass.
    Mass = 6,
    /// ReactionTime (tics).
    ReactionTime = 7,
    /// MeleeRange.
    MeleeRange = 8,
    /// DamageFactor in percent (50 = half damage taken).
    DamageFactorPercent = 9,
    /// Scale in percent (150 = 1.5×).
    ScalePercent = 10,
    /// FastSpeed (Nightmare / fast monsters).
    FastSpeed = 11,
    /// Alpha in percent.
    AlphaPercent = 12,
    /// Translation string (`"Ice"`, `"112:127=176:191"`) — `reg_class_property_str`.
    Translation = 20,
    /// Tag (display name) — `reg_class_property_str`.
    Tag = 21,
    /// Obituary — `reg_class_property_str`.
    Obituary = 22,
    // --- MA2 (`docs/31`) — append-only ---
    /// `Inventory.Amount`.
    InventoryAmount = 30,
    /// `Inventory.MaxAmount`.
    InventoryMaxAmount = 31,
    /// `Ammo.BackpackAmount`.
    AmmoBackpackAmount = 32,
    /// `Ammo.BackpackMaxAmount`.
    AmmoBackpackMaxAmount = 33,
    /// `Weapon.AmmoUse`.
    WeaponAmmoUse = 34,
    /// `Weapon.AmmoGive`.
    WeaponAmmoGive = 35,
    /// `Weapon.SelectionOrder`.
    WeaponSelectionOrder = 36,
    /// `Weapon.Kickback`.
    WeaponKickback = 37,
    /// `Weapon.SlotNumber`.
    WeaponSlotNumber = 38,
    /// `Damage` (missile/melee base).
    Damage = 39,
    /// `BounceType` (0 = none).
    BounceType = 40,
    /// `BounceCount` max.
    BounceCountMax = 41,
    /// `BounceFactor` in percent.
    BounceFactorPercent = 42,
    /// `WallBounceFactor` in percent.
    WallBounceFactorPercent = 43,
    /// `ProjectileKickBack`.
    ProjectileKickBack = 44,
    /// `Weapon.AmmoType` — str.
    WeaponAmmoType = 50,
    /// `Weapon.ReadySound` — str.
    WeaponReadySound = 51,
    /// `Weapon.UpSound` — str.
    WeaponUpSound = 52,
    /// `Inventory.PickupMessage` — str.
    InventoryPickupMessage = 53,
    /// `Inventory.PickupSound` — str.
    InventoryPickupSound = 54,
    /// `Inventory.Icon` — str.
    InventoryIcon = 55,
    /// `SeeSound` — str.
    SeeSound = 56,
    /// `DeathSound` — str.
    DeathSound = 57,
    /// `BounceSound` — str.
    BounceSound = 58,
    /// `ActiveSound` — str.
    ActiveSound = 59,
}

/// Virtual method slots for `reg_class_virtual`.
#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VirtualSlot {
    /// `Tick`.
    Tick = 1,
    /// `PostBeginPlay`.
    PostBeginPlay = 2,
    /// `DamageMobj`.
    DamageMobj = 3,
    /// `Die`.
    Die = 4,
}

pub use room_script_abi::{event_handler_flags, event_mask};

/// One recorded registration call (fingerprint / tests).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RegCall {
    /// `reg_class_begin` → assigned ClassRef.
    ClassBegin {
        /// Class name.
        name: String,
        /// Parent ClassRef (0 = Actor / default).
        parent: u32,
        /// Replaces ClassRef (0 = none).
        replaces: u32,
        /// Editor number (-1 = none).
        doomednum: i32,
        /// Registration flags.
        flags: u32,
        /// Assigned ClassRef.
        class_ref: u32,
    },
    /// `reg_class_property_i32`.
    PropertyI32 {
        /// Property id.
        prop: u32,
        /// Value.
        value: i32,
    },
    /// `reg_class_property_str`.
    PropertyStr {
        /// Property id.
        prop: u32,
        /// Value.
        value: String,
    },
    /// `reg_class_flag`.
    Flag {
        /// Flag id.
        flag: u32,
        /// Non-zero = on.
        on: bool,
    },
    /// `reg_class_end`.
    ClassEnd {
        /// Class that was closed.
        class_ref: u32,
    },
    /// `reg_state_label`.
    StateLabel {
        /// Label name.
        label: String,
    },
    /// `reg_state`.
    State {
        /// Four-character sprite name.
        sprite: String,
        /// Frame letter as byte.
        frame: u8,
        /// Duration in tics.
        tics: i32,
        /// State flags.
        flags: u32,
        /// Dispatch callback (-1 = none).
        callback_id: i32,
        /// Assigned StateRef.
        state_ref: u32,
    },
    /// `reg_state_flow`.
    StateFlow {
        /// [`room_script_abi::state_flow_kind`] value.
        kind: i32,
        /// Goto label (empty for Loop/Stop/Wait/Fail).
        label: String,
        /// Goto offset in states.
        offset: i32,
    },
    /// `reg_state_action` on the last appended state.
    StateAction {
        /// Native action name (`A_WeaponReady`).
        name: String,
        /// Raw DECORATE argument string (comma-separated).
        args: String,
    },
    /// `reg_state_offset` on the last appended state.
    StateOffset {
        /// Psprite X offset.
        x: i32,
        /// Psprite Y offset.
        y: i32,
    },
    /// `reg_class_virtual`.
    Virtual {
        /// Virtual slot.
        slot: u32,
        /// Dispatch callback id.
        callback_id: i32,
    },
    /// `reg_lump` (fingerprint hashes name/ns/len/bytes).
    Lump {
        /// Lump name (≤8, upper).
        name: String,
        /// [`room_script_abi::lump_ns`].
        namespace: i32,
        /// Raw lump bytes.
        data: Vec<u8>,
    },
    /// `reg_sound_alias`.
    SoundAlias {
        /// Logical SNDINFO name.
        logical: String,
        /// Lump name.
        lump: String,
        /// `$limit` (−1 = omit).
        limit: i32,
    },
    /// `reg_weapon_slot` (`docs/31` MA4).
    WeaponSlot {
        /// Slot 1..=10 (0/10 = tenth key).
        slot: i32,
        /// Weapon class name.
        class_name: String,
        /// Insert index (0 = first).
        priority: i32,
    },
    /// `reg_event_handler`.
    EventHandler {
        /// Handler type name.
        name: String,
        /// Sort key (then registration order).
        order: i32,
        /// Handler flags.
        flags: u32,
        /// Subscribed events.
        event_mask: u64,
        /// Base callback id for dispatch.
        callback_base: i32,
        /// Module index that registered this handler.
        module_index: usize,
    },
    /// `reg_thinker_class`.
    ThinkerClass {
        /// Thinker class name.
        name: String,
        /// Thinker statnum.
        statnum: i32,
        /// Tick callback id.
        callback_id: i32,
        /// Module index.
        module_index: usize,
    },
    /// `reg_static_function` (ACS `ScriptCall` target).
    StaticFunction {
        /// Class name.
        class_name: String,
        /// Function name.
        func_name: String,
        /// Dispatch callback id.
        callback_id: i32,
        /// Module index.
        module_index: usize,
    },
}

/// Registered play event handler.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RegisteredEventHandler {
    /// Type name.
    pub name: String,
    /// Order (ascending).
    pub order: i32,
    /// Flags ([`event_handler_flags`]).
    pub flags: u32,
    /// Event mask ([`event_mask`]).
    pub event_mask: u64,
    /// Base `room_dispatch` id (`WorldTick` uses this id directly in v1).
    pub callback_base: i32,
    /// Owning module index in the script host module list.
    pub module_index: usize,
}

/// Registered non-actor thinker class.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RegisteredThinkerClass {
    /// Class name.
    pub name: String,
    /// Statnum.
    pub statnum: i32,
    /// Tick callback id.
    pub callback_id: i32,
    /// Owning module index.
    pub module_index: usize,
}

/// ACS `ScriptCall` target registered from WASM.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RegisteredStaticFunction {
    /// Class name (case-insensitive match).
    pub class_name: String,
    /// Function name (case-insensitive match).
    pub func_name: String,
    /// Dispatch callback id.
    pub callback_id: i32,
    /// Owning module index.
    pub module_index: usize,
}

/// One lump produced during `room_register` (`docs/31` MA3).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GeneratedLump {
    /// Owning module index.
    pub module_index: usize,
    /// Lump name (≤8, upper).
    pub name: String,
    /// [`room_script_abi::lump_ns`] value.
    pub namespace: i32,
    /// Raw bytes (Doom patch / DMX / …).
    pub data: Vec<u8>,
}

/// SNDINFO alias recorded during registration.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GeneratedSoundAlias {
    /// Owning module index.
    pub module_index: usize,
    /// Logical name (`shuriken/throw`).
    pub logical: String,
    /// Lump name (`DSSHKTHR`).
    pub lump: String,
    /// `$limit` channels (−1 = omit).
    pub limit: i32,
}

/// Weapon slot recorded during registration (`docs/31` MA4).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GeneratedWeaponSlot {
    /// Owning module index.
    pub module_index: usize,
    /// Slot 1..=10.
    pub slot: i32,
    /// Weapon class name.
    pub class_name: String,
    /// Insert priority (0 = first).
    pub priority: i32,
}

/// Finished class recorded during registration.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RegisteredClass {
    /// ClassRef assigned at begin.
    pub class_ref: u32,
    /// Name.
    pub name: String,
    /// Parent ClassRef.
    pub parent: u32,
    /// Replaces ClassRef.
    pub replaces: u32,
    /// Doomednum (-1 none).
    pub doomednum: i32,
    /// Flags.
    pub flags: u32,
    /// i32 properties.
    pub props_i32: Vec<(u32, i32)>,
    /// String properties.
    pub props_str: Vec<(u32, String)>,
    /// Flags set/cleared.
    pub class_flags: Vec<(u32, bool)>,
    /// Virtual overrides.
    pub virtuals: Vec<(u32, i32)>,
    /// State labels and states in order.
    pub states: Vec<RegCall>,
    /// Module index that registered this class (`ModRuntime` / `ScriptHost`).
    pub module_index: usize,
}

struct OpenClass {
    class_ref: u32,
    name: String,
    parent: u32,
    replaces: u32,
    doomednum: i32,
    flags: u32,
    props_i32: Vec<(u32, i32)>,
    props_str: Vec<(u32, String)>,
    class_flags: Vec<(u32, bool)>,
    virtuals: Vec<(u32, i32)>,
    states: Vec<RegCall>,
}

/// Mutable registration session shared with WASM imports.
pub struct RegistrationSession {
    calls: Vec<RegCall>,
    classes: Vec<RegisteredClass>,
    event_handlers: Vec<RegisteredEventHandler>,
    thinker_classes: Vec<RegisteredThinkerClass>,
    static_functions: Vec<RegisteredStaticFunction>,
    lumps: Vec<GeneratedLump>,
    sound_aliases: Vec<GeneratedSoundAlias>,
    weapon_slots: Vec<GeneratedWeaponSlot>,
    lump_bytes_total: usize,
    open: Option<OpenClass>,
    next_class: u32,
    next_state: u32,
    /// Module currently executing `room_register` (set by ScriptHost).
    current_module: usize,
    /// Manifest `[options]` for the module being registered (`docs/31` MA4b).
    options: ManifestOptions,
    /// When true, generated lumps may replace IWAD names.
    pub override_lumps: bool,
}

impl Default for RegistrationSession {
    fn default() -> Self {
        Self::new()
    }
}

impl RegistrationSession {
    /// New empty session (ClassRefs start at 1).
    #[must_use]
    pub fn new() -> Self {
        Self {
            calls: Vec::new(),
            classes: Vec::new(),
            event_handlers: Vec::new(),
            thinker_classes: Vec::new(),
            static_functions: Vec::new(),
            lumps: Vec::new(),
            sound_aliases: Vec::new(),
            weapon_slots: Vec::new(),
            lump_bytes_total: 0,
            open: None,
            // High band avoids colliding with ClassRegistry ClassRef (= ClassId.raw()+1).
            next_class: 1_000,
            next_state: 1_000,
            current_module: 0,
            options: BTreeMap::new(),
            override_lumps: false,
        }
    }

    /// Generated lumps from all modules so far.
    #[must_use]
    pub fn lumps(&self) -> &[GeneratedLump] {
        &self.lumps
    }

    /// Sound aliases from all modules so far.
    #[must_use]
    pub fn sound_aliases(&self) -> &[GeneratedSoundAlias] {
        &self.sound_aliases
    }

    /// Weapon slots from all modules so far.
    #[must_use]
    pub fn weapon_slots(&self) -> &[GeneratedWeaponSlot] {
        &self.weapon_slots
    }

    /// Drain generated lumps (after registration).
    pub fn take_lumps(&mut self) -> Vec<GeneratedLump> {
        std::mem::take(&mut self.lumps)
    }

    /// Drain sound aliases.
    pub fn take_sound_aliases(&mut self) -> Vec<GeneratedSoundAlias> {
        std::mem::take(&mut self.sound_aliases)
    }

    /// Drain weapon slots.
    pub fn take_weapon_slots(&mut self) -> Vec<GeneratedWeaponSlot> {
        std::mem::take(&mut self.weapon_slots)
    }

    /// Set module index for subsequent `reg_event_handler` / `reg_thinker_class`.
    pub fn set_current_module(&mut self, index: usize) {
        self.current_module = index;
    }

    /// Replace `[options]` visible to `manifest_option_*` (and fingerprint them).
    pub fn set_options(&mut self, options: ManifestOptions) {
        self.options = options;
    }

    /// Current `[options]` map.
    #[must_use]
    pub fn options(&self) -> &ManifestOptions {
        &self.options
    }

    /// Registered event handlers (registration order).
    #[must_use]
    pub fn event_handlers(&self) -> &[RegisteredEventHandler] {
        &self.event_handlers
    }

    /// Registered thinker classes.
    #[must_use]
    pub fn thinker_classes(&self) -> &[RegisteredThinkerClass] {
        &self.thinker_classes
    }

    /// ACS `ScriptCall` targets.
    #[must_use]
    pub fn static_functions(&self) -> &[RegisteredStaticFunction] {
        &self.static_functions
    }

    /// Find a static function by class + name (case-insensitive).
    #[must_use]
    pub fn find_static_function(
        &self,
        class_name: &str,
        func_name: &str,
    ) -> Option<&RegisteredStaticFunction> {
        self.static_functions.iter().find(|f| {
            f.class_name.eq_ignore_ascii_case(class_name)
                && f.func_name.eq_ignore_ascii_case(func_name)
        })
    }

    /// Ordered registration calls (net/save fingerprint input).
    #[must_use]
    pub fn calls(&self) -> &[RegCall] {
        &self.calls
    }

    /// Completed classes.
    #[must_use]
    pub fn classes(&self) -> &[RegisteredClass] {
        &self.classes
    }

    /// Deterministic fingerprint of registration calls (net/save / conformance).
    #[must_use]
    pub fn fingerprint(&self) -> u64 {
        let mut h = 0xcbf2_9ce4_8422_2325_u64;
        for (k, v) in &self.options {
            let piece = format!("opt:{k}={v:?}");
            for b in piece.bytes() {
                h ^= u64::from(b);
                h = h.wrapping_mul(0x100_0000_01b3);
            }
        }
        for call in &self.calls {
            let piece = format!("{call:?}");
            for b in piece.bytes() {
                h ^= u64::from(b);
                h = h.wrapping_mul(0x100_0000_01b3);
            }
            h ^= 0xff;
            h = h.wrapping_mul(0x100_0000_01b3);
        }
        h
    }

    /// Lookup ClassRef by name (case-insensitive).
    #[must_use]
    pub fn find_class(&self, name: &[u8]) -> Option<u32> {
        let key = std::str::from_utf8(name).ok()?.to_ascii_lowercase();
        self.classes
            .iter()
            .find(|c| c.name.eq_ignore_ascii_case(&key))
            .map(|c| c.class_ref)
            .or_else(|| {
                self.open
                    .as_ref()
                    .and_then(|o| o.name.eq_ignore_ascii_case(&key).then_some(o.class_ref))
            })
    }

    /// `reg_class_begin` → ClassRef (0 on error).
    pub fn class_begin(
        &mut self,
        name: &[u8],
        parent: u32,
        replaces: u32,
        doomednum: i32,
        flags: u32,
    ) -> u32 {
        if self.open.is_some() || name.is_empty() {
            return 0;
        }
        let Ok(name) = std::str::from_utf8(name) else {
            return 0;
        };
        let class_ref = self.next_class;
        self.next_class = self.next_class.saturating_add(1);
        self.calls.push(RegCall::ClassBegin {
            name: name.to_owned(),
            parent,
            replaces,
            doomednum,
            flags,
            class_ref,
        });
        self.open = Some(OpenClass {
            class_ref,
            name: name.to_owned(),
            parent,
            replaces,
            doomednum,
            flags,
            props_i32: Vec::new(),
            props_str: Vec::new(),
            class_flags: Vec::new(),
            virtuals: Vec::new(),
            states: Vec::new(),
        });
        class_ref
    }

    /// `reg_class_property_str`.
    pub fn class_property_str(&mut self, prop: u32, value: &[u8]) {
        let Some(open) = self.open.as_mut() else {
            return;
        };
        let value = String::from_utf8_lossy(value).into_owned();
        open.props_str.push((prop, value.clone()));
        self.calls.push(RegCall::PropertyStr { prop, value });
    }

    /// `reg_class_property_i32`.
    pub fn class_property_i32(&mut self, prop: u32, value: i32) {
        let Some(open) = self.open.as_mut() else {
            return;
        };
        open.props_i32.push((prop, value));
        self.calls.push(RegCall::PropertyI32 { prop, value });
    }

    /// `reg_class_flag`.
    pub fn class_flag(&mut self, flag: u32, on: i32) {
        let Some(open) = self.open.as_mut() else {
            return;
        };
        let on = on != 0;
        open.class_flags.push((flag, on));
        self.calls.push(RegCall::Flag { flag, on });
    }

    /// `reg_class_end`.
    pub fn class_end(&mut self) {
        let Some(open) = self.open.take() else {
            return;
        };
        let class_ref = open.class_ref;
        self.calls.push(RegCall::ClassEnd { class_ref });
        self.classes.push(RegisteredClass {
            class_ref,
            name: open.name,
            parent: open.parent,
            replaces: open.replaces,
            doomednum: open.doomednum,
            flags: open.flags,
            props_i32: open.props_i32,
            props_str: open.props_str,
            class_flags: open.class_flags,
            virtuals: open.virtuals,
            states: open.states,
            module_index: self.current_module,
        });
    }

    /// `reg_state_label`.
    pub fn state_label(&mut self, label: &[u8]) {
        let Some(open) = self.open.as_mut() else {
            return;
        };
        let Ok(label) = std::str::from_utf8(label) else {
            return;
        };
        let call = RegCall::StateLabel {
            label: label.to_owned(),
        };
        open.states.push(call.clone());
        self.calls.push(call);
    }

    /// `reg_state` → StateRef (0 on error).
    pub fn state(
        &mut self,
        sprite: &[u8],
        frame: i32,
        tics: i32,
        flags: u32,
        callback_id: i32,
    ) -> u32 {
        let Some(open) = self.open.as_mut() else {
            return 0;
        };
        if sprite.len() < 4 {
            return 0;
        }
        let sprite = String::from_utf8_lossy(&sprite[..4]).into_owned();
        let state_ref = self.next_state;
        self.next_state = self.next_state.saturating_add(1);
        let frame = u8::try_from(frame).unwrap_or(0);
        let call = RegCall::State {
            sprite,
            frame,
            tics,
            flags,
            callback_id,
            state_ref,
        };
        open.states.push(call.clone());
        self.calls.push(call);
        state_ref
    }

    /// `reg_state_flow` → 0 ok, −1 error.
    pub fn state_flow(&mut self, kind: i32, label: &[u8], offset: i32) -> i32 {
        let Some(open) = self.open.as_mut() else {
            return -1;
        };
        if !matches!(kind, 0..=4) {
            return -1;
        }
        let label = if kind == room_script_abi::state_flow_kind::GOTO {
            match std::str::from_utf8(label) {
                Ok(s) if !s.is_empty() => s.to_owned(),
                _ => return -1,
            }
        } else {
            String::new()
        };
        let call = RegCall::StateFlow {
            kind,
            label,
            offset,
        };
        open.states.push(call.clone());
        self.calls.push(call);
        0
    }

    /// `reg_state_action` → 0 ok, −1 error.
    pub fn state_action(&mut self, name: &[u8], args: &[u8]) -> i32 {
        let Some(open) = self.open.as_mut() else {
            return -1;
        };
        let Ok(name) = std::str::from_utf8(name) else {
            return -1;
        };
        if name.is_empty() {
            return -1;
        }
        let args = std::str::from_utf8(args).unwrap_or("").to_owned();
        let call = RegCall::StateAction {
            name: name.to_owned(),
            args,
        };
        open.states.push(call.clone());
        self.calls.push(call);
        0
    }

    /// `reg_state_offset` → 0 ok, −1 error.
    pub fn state_offset(&mut self, x: i32, y: i32) -> i32 {
        let Some(open) = self.open.as_mut() else {
            return -1;
        };
        let call = RegCall::StateOffset { x, y };
        open.states.push(call.clone());
        self.calls.push(call);
        0
    }

    /// `reg_class_virtual`.
    pub fn class_virtual(&mut self, slot: u32, callback_id: i32) {
        let Some(open) = self.open.as_mut() else {
            return;
        };
        open.virtuals.push((slot, callback_id));
        self.calls.push(RegCall::Virtual { slot, callback_id });
    }

    /// `reg_event_handler` (0 = ok, -1 = error).
    pub fn event_handler(
        &mut self,
        name: &[u8],
        order: i32,
        flags: u32,
        event_mask: u64,
        callback_base: i32,
    ) -> i32 {
        if name.is_empty() {
            return -1;
        }
        let Ok(name) = std::str::from_utf8(name) else {
            return -1;
        };
        let module_index = self.current_module;
        let h = RegisteredEventHandler {
            name: name.to_owned(),
            order,
            flags,
            event_mask,
            callback_base,
            module_index,
        };
        self.calls.push(RegCall::EventHandler {
            name: h.name.clone(),
            order,
            flags,
            event_mask,
            callback_base,
            module_index,
        });
        self.event_handlers.push(h);
        0
    }

    /// `reg_thinker_class` (0 = ok, -1 = error).
    pub fn thinker_class(&mut self, name: &[u8], statnum: i32, callback_id: i32) -> i32 {
        if name.is_empty() {
            return -1;
        }
        let Ok(name) = std::str::from_utf8(name) else {
            return -1;
        };
        let module_index = self.current_module;
        let t = RegisteredThinkerClass {
            name: name.to_owned(),
            statnum,
            callback_id,
            module_index,
        };
        self.calls.push(RegCall::ThinkerClass {
            name: t.name.clone(),
            statnum,
            callback_id,
            module_index,
        });
        self.thinker_classes.push(t);
        0
    }

    /// `reg_static_function` (0 = ok, -1 = error).
    pub fn static_function(
        &mut self,
        class_name: &[u8],
        func_name: &[u8],
        callback_id: i32,
    ) -> i32 {
        if class_name.is_empty() || func_name.is_empty() {
            return -1;
        }
        let Ok(class_name) = std::str::from_utf8(class_name) else {
            return -1;
        };
        let Ok(func_name) = std::str::from_utf8(func_name) else {
            return -1;
        };
        let module_index = self.current_module;
        let f = RegisteredStaticFunction {
            class_name: class_name.to_owned(),
            func_name: func_name.to_owned(),
            callback_id,
            module_index,
        };
        self.calls.push(RegCall::StaticFunction {
            class_name: f.class_name.clone(),
            func_name: f.func_name.clone(),
            callback_id,
            module_index,
        });
        self.static_functions.push(f);
        0
    }

    /// `reg_lump` → 0 ok, −1 error (`docs/31` MA3).
    pub fn lump(&mut self, name: &[u8], namespace: i32, data: &[u8]) -> i32 {
        use room_script_abi::{lump_limits, lump_ns};
        if !matches!(
            namespace,
            lump_ns::GLOBAL | lump_ns::SPRITES | lump_ns::GRAPHICS | lump_ns::SOUNDS | lump_ns::FLATS
        ) {
            return -1;
        }
        let Ok(raw) = std::str::from_utf8(name) else {
            return -1;
        };
        let name = raw.trim();
        if name.is_empty() || name.len() > 8 {
            return -1;
        }
        if !name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-' || b == b'+')
        {
            return -1;
        }
        let name = name.to_ascii_uppercase();
        if data.len() > lump_limits::TOTAL_BYTES {
            return -1;
        }
        if self.lump_bytes_total.saturating_add(data.len()) > lump_limits::TOTAL_BYTES {
            return -1;
        }
        if self
            .lumps
            .iter()
            .any(|l| l.name == name && l.namespace == namespace)
        {
            return -1;
        }
        let lump = GeneratedLump {
            module_index: self.current_module,
            name: name.clone(),
            namespace,
            data: data.to_vec(),
        };
        self.calls.push(RegCall::Lump {
            name,
            namespace,
            data: lump.data.clone(),
        });
        self.lump_bytes_total += lump.data.len();
        self.lumps.push(lump);
        0
    }

    /// `reg_sound_alias` → 0 ok, −1 error.
    pub fn sound_alias(&mut self, logical: &[u8], lump: &[u8], limit: i32) -> i32 {
        let Ok(logical) = std::str::from_utf8(logical) else {
            return -1;
        };
        let Ok(lump) = std::str::from_utf8(lump) else {
            return -1;
        };
        let logical = logical.trim();
        let lump = lump.trim();
        if logical.is_empty() || lump.is_empty() || lump.len() > 8 {
            return -1;
        }
        let lump = lump.to_ascii_uppercase();
        let alias = GeneratedSoundAlias {
            module_index: self.current_module,
            logical: logical.to_owned(),
            lump: lump.clone(),
            limit,
        };
        self.calls.push(RegCall::SoundAlias {
            logical: alias.logical.clone(),
            lump,
            limit,
        });
        self.sound_aliases.push(alias);
        0
    }

    /// `reg_weapon_slot` → 0 ok, −1 error (`docs/31` MA4).
    pub fn weapon_slot(&mut self, slot: i32, class_name: &[u8], priority: i32) -> i32 {
        if !matches!(slot, 0 | 1..=10) {
            return -1;
        }
        let Ok(raw) = std::str::from_utf8(class_name) else {
            return -1;
        };
        let class_name = raw.trim();
        if class_name.is_empty() {
            return -1;
        }
        let entry = GeneratedWeaponSlot {
            module_index: self.current_module,
            slot,
            class_name: class_name.to_owned(),
            priority,
        };
        self.calls.push(RegCall::WeaponSlot {
            slot,
            class_name: entry.class_name.clone(),
            priority,
        });
        self.weapon_slots.push(entry);
        0
    }

    /// `manifest_option_i32` — absent key returns `default`.
    #[must_use]
    pub fn option_i32(&self, key: &[u8], default: i32) -> i32 {
        let Ok(key) = std::str::from_utf8(key) else {
            return default;
        };
        self.options
            .get(key.trim())
            .map_or(default, |v| v.as_i32(default))
    }

    /// `manifest_option_str` → length written into `buf` (−1 absent / OOB).
    pub fn option_str(&self, key: &[u8], buf: &mut [u8]) -> i32 {
        let Ok(key) = std::str::from_utf8(key) else {
            return -1;
        };
        let Some(v) = self.options.get(key.trim()) else {
            return -1;
        };
        let owned;
        let s = match v {
            OptionValue::Str(t) => t.as_str(),
            OptionValue::Int(n) => {
                owned = n.to_string();
                owned.as_str()
            }
            OptionValue::Bool(b) => {
                if *b {
                    "true"
                } else {
                    "false"
                }
            }
        };
        let full = s.len();
        let n = full.min(buf.len());
        buf[..n].copy_from_slice(&s.as_bytes()[..n]);
        full as i32
    }
}

/// Shared registration handle for [`crate::HostLinker`].
pub type SharedRegistration = Arc<Mutex<RegistrationSession>>;

/// Create a shared empty registration session.
#[must_use]
pub fn shared_registration() -> SharedRegistration {
    Arc::new(Mutex::new(RegistrationSession::new()))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn registers_class_with_state() {
        let mut s = RegistrationSession::new();
        let c = s.class_begin(b"FireImp", 0, 0, -1, 0);
        assert_eq!(c, 1_000);
        s.class_property_i32(PropId::Health as u32, 90);
        s.state_label(b"Spawn");
        let st = s.state(b"TROO", b'A' as i32, 10, 0, -1);
        assert_eq!(st, 1_000);
        s.class_virtual(VirtualSlot::Tick as u32, 7);
        s.class_end();
        assert_eq!(s.classes().len(), 1);
        assert_eq!(s.classes()[0].name, "FireImp");
        assert_eq!(s.find_class(b"fireimp"), Some(1_000));
        assert!(s.open.is_none());
    }

    #[test]
    fn registers_event_handler_and_thinker() {
        let mut s = RegistrationSession::new();
        s.set_current_module(2);
        assert_eq!(
            s.event_handler(b"ImpTracker", 0, 0, event_mask::WORLD_TICK, 10),
            0
        );
        assert_eq!(s.thinker_class(b"ImpThinker", 64, 11), 0);
        assert_eq!(s.event_handlers()[0].module_index, 2);
        assert_eq!(s.thinker_classes()[0].callback_id, 11);
        assert_ne!(s.fingerprint(), 0);
    }
}
