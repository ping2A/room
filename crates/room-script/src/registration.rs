//! Registration-phase bookkeeping (`docs/12` §6).

use std::sync::{Arc, Mutex};

/// Seed `PropId` values (append-only; DECORATE property subset).
#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PropId {
    /// Health.
    Health = 1,
    /// Speed (stored as i32 × 65536 until f64 path lands).
    Speed = 2,
    /// PainChance.
    PainChance = 3,
    /// Radius.
    Radius = 4,
    /// Height.
    Height = 5,
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

/// `reg_event_handler` event_mask bits (`docs/12` §9).
pub mod event_mask {
    /// `WorldTick`.
    pub const WORLD_TICK: u64 = 1 << 0;
    /// `WorldThingDied`.
    pub const WORLD_THING_DIED: u64 = 1 << 1;
    /// `NewGame`.
    pub const NEW_GAME: u64 = 1 << 2;
    /// `WorldLoaded`.
    pub const WORLD_LOADED: u64 = 1 << 3;
    /// `UiTick` (UI-scope modules).
    pub const UI_TICK: u64 = 1 << 16;
    /// `RenderOverlay` (UI-scope).
    pub const RENDER_OVERLAY: u64 = 1 << 17;
}

/// `reg_event_handler` flags.
pub mod event_handler_flags {
    /// Survives map changes (`StaticEventHandler`).
    pub const STATIC: u32 = 1 << 0;
    /// Map-only handler.
    pub const MAP_ONLY: u32 = 1 << 1;
}

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
    /// `reg_class_virtual`.
    Virtual {
        /// Virtual slot.
        slot: u32,
        /// Dispatch callback id.
        callback_id: i32,
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
    /// Flags set/cleared.
    pub class_flags: Vec<(u32, bool)>,
    /// Virtual overrides.
    pub virtuals: Vec<(u32, i32)>,
    /// State labels and states in order.
    pub states: Vec<RegCall>,
}

struct OpenClass {
    class_ref: u32,
    name: String,
    parent: u32,
    replaces: u32,
    doomednum: i32,
    flags: u32,
    props_i32: Vec<(u32, i32)>,
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
    open: Option<OpenClass>,
    next_class: u32,
    next_state: u32,
    /// Module currently executing `room_register` (set by ScriptHost).
    current_module: usize,
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
            open: None,
            // High band avoids colliding with ClassRegistry ClassRef (= ClassId.raw()+1).
            next_class: 1_000,
            next_state: 1_000,
            current_module: 0,
        }
    }

    /// Set module index for subsequent `reg_event_handler` / `reg_thinker_class`.
    pub fn set_current_module(&mut self, index: usize) {
        self.current_module = index;
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
            class_flags: Vec::new(),
            virtuals: Vec::new(),
            states: Vec::new(),
        });
        class_ref
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
            class_flags: open.class_flags,
            virtuals: open.virtuals,
            states: open.states,
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
