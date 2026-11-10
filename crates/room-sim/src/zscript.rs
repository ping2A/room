//! ZScript play-scope integration (`docs/34` ZS3–ZS6).

use room_zvm::{CompiledProgram, HeapImage, ObjHandle, ObjKind, Zvm};
use std::collections::BTreeMap;
use std::sync::Arc;

/// Serialized ZScript save blob (stable order).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ZscriptSaveBlob {
    /// Compiled program hash (must match on load unless overridden).
    pub program_hash: u64,
    /// Actor index → script field integers.
    pub actor_i: BTreeMap<u32, Vec<i64>>,
    /// Flattened heap object payloads (index, generation, class, fields_i).
    pub objects: Vec<SavedObject>,
    /// Whole heap, when present. Load restores these slots in place.
    pub heap: Option<HeapImage>,
    /// Handler class → object handle `(index, generation)`.
    pub handlers: Vec<(String, u32, u32)>,
    /// Actor id → script-object handle `(index, generation)`.
    pub actor_handles: Vec<(u32, u32, u32)>,
}

/// One heap object in a save.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SavedObject {
    /// Slot index.
    pub index: u32,
    /// Generation.
    pub generation: u32,
    /// Class id.
    pub class_id: u32,
    /// Integer fields.
    pub fields_i: Vec<i64>,
}

/// Actor data copied into a native `WorldEvent` payload for a single callback.
#[derive(Clone, Debug)]
pub(crate) struct ZscriptActorEventInfo {
    pub class_name: String,
    pub health: i32,
    pub tid: i32,
    pub is_monster: bool,
}

/// Per-world ZScript runtime state.
#[derive(Default)]
pub struct ZscriptRuntime {
    /// Linked program (shared).
    pub program: Option<Arc<CompiledProgram>>,
    /// VM instance (heap + natives).
    pub vm: Option<Zvm>,
    /// Actor id → script field object handle.
    pub actor_fields: BTreeMap<u32, ObjHandle>,
    /// Actors that already received `PostBeginPlay` this life.
    pub begun_play: BTreeMap<u32, ()>,
    /// Registered event handler class names (from MAPINFO / load).
    pub event_handlers: Vec<String>,
    /// Persistent handler instances (class name → object), for `OnRegister` state.
    pub handler_objs: BTreeMap<String, ObjHandle>,
    /// One script object per class that overrides `ModifyDamage`.
    modify_objs: BTreeMap<String, ObjHandle>,
    /// Reused shooter / projectile proxies (`bISMONSTER` in integer field 1).
    actor_proxies: Vec<ObjHandle>,
    /// Reused class-layout-aware Actor values for event payloads.
    event_actor_proxies: Vec<ObjHandle>,
    /// Non-null object stored in ZScript `target` when the playsim actor has one.
    target_sentinel: ObjHandle,
    /// True after [`Self::activate_map_handlers`] ran `OnRegister`/`WorldLoaded`.
    pub map_handlers_active: bool,
    /// Actor index → generation last synced into its script object.
    actor_gen: BTreeMap<u32, u32>,
    /// `(class id, field)` → `(bank, slot)` with parents walked.
    field_cache: std::cell::RefCell<BTreeMap<(u32, &'static str), Option<(u8, usize)>>>,
}

/// Native actor fields mirrored into a script object around a call.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct ActorFieldSnap {
    /// `(index, generation)`.
    pub id: (u32, u32),
    /// Class name.
    pub class: String,
    /// Float fields (vector components are `name.x` / `.y` / `.z`).
    pub floats: Vec<(&'static str, f64)>,
    /// Integer fields.
    pub ints: Vec<(&'static str, i64)>,
    /// Actor pointers as `(index, generation, class)`.
    pub ptrs: Vec<(&'static str, Option<(u32, u32, String)>)>,
}

impl ZscriptRuntime {
    /// Install a compiled program and reset the VM.
    pub fn load_program(&mut self, program: CompiledProgram) {
        self.load_program_inner(program, false);
    }

    /// Install a program and no-op-stub every native the builtin table lacks.
    ///
    /// The UZDoom stdlib reads engine globals before `LineAttack`. Those calls
    /// must return 0 instead of trapping.
    pub fn load_program_stubbed(&mut self, program: CompiledProgram) {
        self.load_program_inner(program, true);
    }

    fn load_program_inner(&mut self, program: CompiledProgram, stub_missing: bool) {
        let program = Arc::new(program);
        let mut vm = if stub_missing {
            Zvm::new_with_stubs((*program).clone())
        } else {
            Zvm::new((*program).clone())
        };
        vm.host = 0;
        vm.play.class_names = program
            .classes
            .iter()
            .map(|class| class.name.clone())
            .collect();
        self.vm = Some(vm);
        self.program = Some(Arc::clone(&program));
        self.actor_fields.clear();
        self.begun_play.clear();
        self.handler_objs.clear();
        self.modify_objs.clear();
        self.actor_proxies.clear();
        self.event_actor_proxies.clear();
        self.map_handlers_active = false;
        self.actor_gen.clear();
        self.field_cache.borrow_mut().clear();
        let slots = actor_slots_of(&program);
        if let Some(vm) = self.vm.as_mut() {
            vm.play.actor_slots = slots;
        }
    }

    /// Per-call world answers for the typed natives (`CheckSight`, `CountInv`, …).
    pub(crate) fn set_call_world(
        &mut self,
        sight: Vec<(ObjHandle, bool)>,
        melee_ok: bool,
        self_target: ObjHandle,
        inventory: Vec<(ObjHandle, Vec<(String, i32, i32)>)>,
    ) {
        if let Some(vm) = self.vm.as_mut() {
            vm.play.sight = sight;
            vm.play.melee_ok = melee_ok;
            vm.play.self_target = self_target;
            vm.play.inventory = inventory;
        }
    }

    fn cached_field(&self, vm: &Zvm, object: ObjHandle, name: &'static str) -> Option<(u8, usize)> {
        let class_id = match vm.heap.get(object)? {
            ObjKind::Object { class_id, .. } => *class_id,
            _ => return None,
        };
        if let Some(hit) = self.field_cache.borrow().get(&(class_id, name)) {
            return *hit;
        }
        let found = walk_field(vm, object, name);
        self.field_cache.borrow_mut().insert((class_id, name), found);
        found
    }

    /// True when `Class.Method` exists on that class (case-insensitive).
    ///
    /// Does not walk parents. [`resolve_method`](Self::resolve_method) does.
    #[must_use]
    pub fn has_method(&self, class: &str, method: &str) -> bool {
        let qname = format!("{class}.{method}");
        self.vm.as_ref().is_some_and(|vm| {
            vm.program
                .func_by_name
                .keys()
                .any(|k| k.eq_ignore_ascii_case(&qname))
        })
    }

    /// `Class.Method`, walking parents so `MarinePistol.A_M_FirePistol` finds
    /// the body declared on `ScriptedMarine`.
    #[must_use]
    pub fn resolve_method(&self, class: &str, method: &str) -> Option<String> {
        let vm = self.vm.as_ref()?;
        let mut cur = class.to_owned();
        for _ in 0..64 {
            let qname = format!("{cur}.{method}");
            if let Some(found) = vm
                .program
                .func_by_name
                .keys()
                .find(|k| k.eq_ignore_ascii_case(&qname))
            {
                return Some(found.clone());
            }
            let parent = vm
                .program
                .classes
                .iter()
                .find(|c| c.name.eq_ignore_ascii_case(&cur))?
                .parent
                .clone();
            if parent.is_empty() || parent.eq_ignore_ascii_case(&cur) {
                return None;
            }
            cur = parent;
        }
        None
    }

    /// Remember whether the playsim actor has a chase target.
    ///
    /// Writes the `target` object field from the class layout. A non-null
    /// sentinel makes `target == null` false inside `A_M_FirePistol`.
    pub fn mark_actor_target(&mut self, actor_id: u32, class: &str, has_target: bool) {
        if self.ensure_actor_object(actor_id, class).is_none() {
            return;
        }
        let slot = self.target_field_slot(class).unwrap_or(7);
        let sentinel = if has_target {
            if self.target_sentinel.is_null() {
                if let Some(vm) = self.vm.as_mut() {
                    self.target_sentinel = vm.heap.new_object(0, 1, 1, 1);
                }
            }
            self.target_sentinel
        } else {
            ObjHandle::NULL
        };
        let Some(h) = self.actor_fields.get(&actor_id).copied() else {
            return;
        };
        let Some(vm) = self.vm.as_mut() else {
            return;
        };
        if let Some(ObjKind::Object { fields_a, .. }) = vm.heap.get_mut(h) {
            if fields_a.len() <= slot {
                fields_a.resize(slot + 1, ObjHandle::NULL);
            }
            fields_a[slot] = sentinel;
        }
    }

    /// Object-bank index of `target`, walking the class parent chain.
    fn target_field_slot(&self, class: &str) -> Option<usize> {
        let vm = self.vm.as_ref()?;
        let mut cur = class.to_owned();
        for _ in 0..64 {
            let layout = vm
                .program
                .classes
                .iter()
                .find(|c| c.name.eq_ignore_ascii_case(&cur))?;
            if let Some((bank, slot)) = layout.fields.get("target").copied() {
                // ClassLayout bank 1 is the object-reference bank.
                if bank == 1 {
                    return Some(slot as usize);
                }
            }
            if layout.parent.is_empty() || layout.parent.eq_ignore_ascii_case(&cur) {
                return None;
            }
            cur = layout.parent.clone();
        }
        None
    }

    /// Play-scope natives queued since the last drain (UZ3 / UZ7), with their
    /// receivers resolved to playsim actor indexes where they are actors.
    pub fn take_native_ops(&mut self) -> Vec<(Option<u32>, room_zvm::ZsNativeOp)> {
        let ops = self
            .vm
            .as_mut()
            .map(|vm| std::mem::take(&mut vm.play.native_ops))
            .unwrap_or_default();
        ops.into_iter()
            .map(|op| {
                let actor = self
                    .actor_fields
                    .iter()
                    .find(|(_, h)| **h == op.recv)
                    .map(|(&id, _)| id);
                (actor, op)
            })
            .collect()
    }

    /// Whether the program calls `CheckProximity` (the playsim then fills
    /// the per-tic actor census).
    #[must_use]
    pub fn needs_census(&self) -> bool {
        self.program.as_ref().is_some_and(|p| {
            p.native_imports
                .iter()
                .any(|n| n.eq_ignore_ascii_case("actor.checkproximity"))
        })
    }

    /// Per-tic `(lowercase class, position)` of live actors.
    pub fn set_census(&mut self, census: Vec<(String, [f64; 3])>) {
        if let Some(play) = self.play_mut() {
            play.census = census;
        }
    }

    /// Class tables for `GetReplacement`, `GetParentClass`, `CheckProximity`
    /// and the weapon slots for `SlotSize` / `HasWeaponsInSlot`.
    pub fn set_class_tables(
        &mut self,
        replacements: BTreeMap<String, String>,
        parents: BTreeMap<String, String>,
        weapon_slots: Vec<Vec<String>>,
    ) {
        if let Some(play) = self.play_mut() {
            play.replacements = replacements;
            play.class_parents = parents;
            play.weapon_slots = weapon_slots;
        }
    }

    /// `LevelLocals` sky / fog natives queued since the last drain.
    pub fn take_level_ops(&mut self) -> Vec<room_zvm::ZsLevelOp> {
        self.vm
            .as_mut()
            .map(|vm| std::mem::take(&mut vm.play.level_ops))
            .unwrap_or_default()
    }

    /// Actor natives queued by the last call (`A_Look`, `LineAttack`, …).
    pub fn take_actor_ops(&mut self) -> Vec<room_zvm::ZsActorOp> {
        self.vm
            .as_mut()
            .map(|vm| std::mem::take(&mut vm.play.actor_ops))
            .unwrap_or_default()
    }

    /// Prime `A_CheckForResurrection` with the physical corpse candidate found
    /// by the playsim for the next actor method call.
    pub(crate) fn set_resurrection_candidate(&mut self, candidate: Option<(u32, u32)>) {
        if let Some(vm) = self.vm.as_mut() {
            vm.play.resurrection_candidate = candidate;
        }
    }

    /// Set observer distance/LOS snapshots for the next ZScript actor call.
    pub(crate) fn set_ai_observers(&mut self, observers: Vec<(f64, f64, bool)>) {
        if let Some(vm) = self.vm.as_mut() {
            vm.play.ai_observers = observers;
        }
    }

    /// Call `PostBeginPlay` once per actor when the class defines it (ZS3).
    pub fn notify_spawned(&mut self, actor_id: u32, class: &str) {
        if !self.is_active() || !self.has_method(class, "PostBeginPlay") {
            return;
        }
        if self.begun_play.contains_key(&actor_id) {
            return;
        }
        let _ = self.call_actor_virtual(actor_id, class, "PostBeginPlay");
        self.begun_play.insert(actor_id, ());
    }

    /// Call `Tick` when the actor's ZScript class overrides it (ZS3).
    pub fn tick_actor(&mut self, actor_id: u32, class: &str) {
        if !self.is_active() || !self.has_method(class, "Tick") {
            return;
        }
        // Ensure PostBeginPlay has run before the first Tick (spawn mid-tic, etc.).
        let resurrection_candidate = self
            .vm
            .as_ref()
            .and_then(|vm| vm.play.resurrection_candidate);
        self.notify_spawned(actor_id, class);
        if let Some(vm) = self.vm.as_mut() {
            vm.play.resurrection_candidate = resurrection_candidate;
        }
        let _ = self.call_actor_virtual(actor_id, class, "Tick");
    }

    /// Register MAPINFO `AddEventHandlers` class names (order preserved).
    pub fn set_event_handlers(&mut self, names: impl IntoIterator<Item = String>) {
        self.event_handlers = names.into_iter().collect();
    }

    /// Publish play-scope values read by ZVM natives for the next call.
    pub fn sync_host(&mut self, skill: u8, consoleplayer: i32, gamestate: i32) {
        let Some(play) = self.play_mut() else {
            return;
        };
        play.skill = i32::from(skill);
        play.consoleplayer = consoleplayer;
        play.gamestate = gamestate;
    }

    /// Publish the active map's MD5 for `LevelLocals.GetChecksum`.
    pub fn set_level_checksum(&mut self, checksum: [u8; 16]) {
        const HEX: &[u8; 16] = b"0123456789abcdef";
        let mut text = String::with_capacity(32);
        for byte in checksum {
            text.push(char::from(HEX[usize::from(byte >> 4)]));
            text.push(char::from(HEX[usize::from(byte & 0x0f)]));
        }
        if let Some(play) = self.play_mut() {
            play.level_checksum = text;
        }
    }

    /// Install in-memory wad lumps for `Wads.*` (tests / BD_Hash).
    pub fn set_wad_lumps(&mut self, lumps: BTreeMap<String, String>) {
        if let Some(play) = self.play_mut() {
            play.wads = lumps;
        }
    }

    /// Install integer CVars for `CVar.GetCVar` / `GetInt` (ZS4 / BD CVARINFO).
    pub fn set_cvars(&mut self, cvars: BTreeMap<String, i32>) {
        if let Some(play) = self.play_mut() {
            play.cvars.clear();
            play.cvar_floats.clear();
            play.cvar_strings.clear();
            for (name, value) in cvars {
                play.set_cvar_int(&name, value);
            }
        }
    }

    /// Selected player voice gender used by SNDINFO `$playersound` lookup.
    #[must_use]
    pub fn player_gender(&self) -> &'static str {
        let g = self.vm.as_ref().map_or(0, |vm| vm.play.cvar("gender"));
        crate::action::gender_name(u8::try_from(g.clamp(0, 3)).unwrap_or(0))
    }

    /// Install floating CVars for ZScript `CVar.GetFloat` / `SetFloat`.
    pub fn set_cvar_floats(&mut self, cvars: BTreeMap<String, f64>) {
        if let Some(play) = self.play_mut() {
            for (name, value) in cvars {
                play.set_cvar_float(&name, value);
            }
        }
    }

    /// Install floating-point defaults for ZScript CVar handles.
    pub fn set_cvar_float_defaults(&mut self, cvars: BTreeMap<String, f64>) {
        if let Some(play) = self.play_mut() {
            play.cvar_float_defaults.clear();
            for (name, value) in cvars {
                play.set_cvar_float_default(&name, value);
            }
        }
    }

    /// Install string CVars for ZScript `CVar.GetString` / `SetString`.
    pub fn set_cvar_strings(&mut self, cvars: BTreeMap<String, String>) {
        if let Some(play) = self.play_mut() {
            for (name, value) in cvars {
                play.set_cvar_string(&name, &value);
            }
        }
    }

    /// Install string defaults for ZScript CVar handles.
    pub fn set_cvar_string_defaults(&mut self, cvars: BTreeMap<String, String>) {
        if let Some(play) = self.play_mut() {
            play.cvar_string_defaults.clear();
            for (name, value) in cvars {
                play.set_cvar_string_default(&name, &value);
            }
        }
    }

    /// Drain `Console.Printf` messages captured by the host (tests).
    pub fn take_printf(&mut self) -> Vec<String> {
        self.play_mut()
            .map(|play| std::mem::take(&mut play.printf))
            .unwrap_or_default()
    }

    /// Allocate MAPINFO handlers, then run `OnRegister` / `WorldLoaded` (ZS4).
    ///
    /// Handlers that only override later events (`PlayerEntered`,
    /// `WorldThingSpawned`, …) are still instantiated so `EventHandler.Find`
    /// and ACS `ScriptCall` into static helpers on those classes work.
    pub fn activate_map_handlers(&mut self) -> Result<(), room_zvm::Trap> {
        if self.map_handlers_active || !self.is_active() {
            return Ok(());
        }
        let handlers = self.event_handlers.clone();
        for class in &handlers {
            let _ = self.handler_instance(class)?;
        }
        self.dispatch_handlers("OnRegister")?;
        self.dispatch_handlers("WorldLoaded")?;
        self.map_handlers_active = true;
        Ok(())
    }

    /// `PlayerEntered` for a joining player number (0-based).
    pub fn notify_player_entered(
        &mut self,
        player_number: i32,
        from_hub: bool,
    ) -> Result<(), room_zvm::Trap> {
        self.dispatch_player_event("PlayerEntered", player_number, from_hub)
    }

    /// Dispatch `PlayerSpawned` after a player pawn has been placed in the world.
    pub fn notify_player_spawned(&mut self, player_number: i32) -> Result<(), room_zvm::Trap> {
        self.dispatch_player_event("PlayerSpawned", player_number, false)
    }

    /// Dispatch `PlayerRespawned` after a dead player has returned to play.
    pub fn notify_player_respawned(&mut self, player_number: i32) -> Result<(), room_zvm::Trap> {
        self.dispatch_player_event("PlayerRespawned", player_number, false)
    }

    /// Dispatch the veto-capable `PlayerRespawning` callback.
    pub fn notify_player_respawning(&mut self, player_number: i32) -> Result<bool, room_zvm::Trap> {
        let handlers = self.event_handlers.clone();
        for class in handlers {
            let Some(resolved) = self.resolve_func_name(&class, "PlayerRespawning") else {
                continue;
            };
            let handler = self.handler_instance(&class)?;
            let vm = self
                .vm
                .as_mut()
                .ok_or_else(|| room_zvm::Trap::Native("no ZScript VM".into()))?;
            let event = new_typed_object(vm, "PlayerEvent");
            set_typed_int_field(vm, event, "PlayerNumber", i64::from(player_number));
            if call_handler_with_event(vm, &resolved, handler, event)? == 0 {
                return Ok(false);
            }
        }
        Ok(true)
    }

    /// Dispatch `PlayerDied` for a player killed by the playsim.
    pub fn notify_player_died(&mut self, player_number: i32) -> Result<(), room_zvm::Trap> {
        self.dispatch_player_event("PlayerDied", player_number, false)
    }

    /// Dispatch the no-argument `StaticEventHandler.WorldTick` event.
    pub fn notify_world_tick(&mut self) -> Result<(), room_zvm::Trap> {
        self.dispatch_handlers("WorldTick")
    }

    /// Dispatch `WorldThingDied` with a lightweight Actor proxy in `event.Thing`.
    pub(crate) fn notify_thing_died(
        &mut self,
        thing_info: &ZscriptActorEventInfo,
        inflictor_info: Option<&ZscriptActorEventInfo>,
    ) -> Result<(), room_zvm::Trap> {
        let handlers = self.event_handlers.clone();
        for class in handlers {
            let Some(resolved) = self.resolve_func_name(&class, "WorldThingDied") else {
                continue;
            };
            let handler = self.handler_instance(&class)?;
            let Some(vm) = self.vm.as_mut() else {
                return Err(room_zvm::Trap::Native("no ZScript VM".into()));
            };
            let thing = event_actor_proxy(vm, &mut self.event_actor_proxies, 0, thing_info);
            let inflictor = inflictor_info
                .map(|info| event_actor_proxy(vm, &mut self.event_actor_proxies, 1, info))
                .unwrap_or(ObjHandle::NULL);
            let event = new_typed_object(vm, "WorldEvent");
            set_typed_object_field(vm, event, "Thing", thing);
            set_typed_object_field(vm, event, "Inflictor", inflictor);
            let _ = call_handler_with_event(vm, &resolved, handler, event)?;
        }
        Ok(())
    }

    /// Dispatch `WorldThingDestroyed` with the actor being explicitly removed.
    pub(crate) fn notify_thing_destroyed(
        &mut self,
        thing_info: &ZscriptActorEventInfo,
    ) -> Result<(), room_zvm::Trap> {
        let handlers = self.event_handlers.clone();
        for class in handlers {
            let Some(resolved) = self.resolve_func_name(&class, "WorldThingDestroyed") else {
                continue;
            };
            let handler = self.handler_instance(&class)?;
            let Some(vm) = self.vm.as_mut() else {
                return Err(room_zvm::Trap::Native("no ZScript VM".into()));
            };
            let thing = event_actor_proxy(vm, &mut self.event_actor_proxies, 0, thing_info);
            let event = new_typed_object(vm, "WorldEvent");
            set_typed_object_field(vm, event, "Thing", thing);
            let _ = call_handler_with_event(vm, &resolved, handler, event)?;
        }
        Ok(())
    }

    /// Dispatch `WorldThingRevived` with the actor that has just been restored.
    pub(crate) fn notify_thing_revived(
        &mut self,
        thing_info: &ZscriptActorEventInfo,
    ) -> Result<(), room_zvm::Trap> {
        let handlers = self.event_handlers.clone();
        for class in handlers {
            let Some(resolved) = self.resolve_func_name(&class, "WorldThingRevived") else {
                continue;
            };
            let handler = self.handler_instance(&class)?;
            let Some(vm) = self.vm.as_mut() else {
                return Err(room_zvm::Trap::Native("no ZScript VM".into()));
            };
            let thing = event_actor_proxy(vm, &mut self.event_actor_proxies, 0, thing_info);
            let event = new_typed_object(vm, "WorldEvent");
            set_typed_object_field(vm, event, "Thing", thing);
            let _ = call_handler_with_event(vm, &resolved, handler, event)?;
        }
        Ok(())
    }

    /// Dispatch the full actor damage subset represented by `WorldEvent`.
    pub(crate) fn notify_thing_damaged(
        &mut self,
        thing_info: &ZscriptActorEventInfo,
        inflictor_info: Option<&ZscriptActorEventInfo>,
        source_info: Option<&ZscriptActorEventInfo>,
        damage: i32,
        damage_type: &str,
    ) -> Result<(), room_zvm::Trap> {
        let handlers = self.event_handlers.clone();
        for class in handlers {
            let Some(resolved) = self.resolve_func_name(&class, "WorldThingDamaged") else {
                continue;
            };
            let handler = self.handler_instance(&class)?;
            let Some(vm) = self.vm.as_mut() else {
                return Err(room_zvm::Trap::Native("no ZScript VM".into()));
            };
            let thing = event_actor_proxy(vm, &mut self.event_actor_proxies, 0, thing_info);
            let inflictor = inflictor_info
                .map(|info| event_actor_proxy(vm, &mut self.event_actor_proxies, 1, info))
                .unwrap_or(ObjHandle::NULL);
            let source = source_info
                .map(|info| event_actor_proxy(vm, &mut self.event_actor_proxies, 2, info))
                .unwrap_or(ObjHandle::NULL);
            let event = new_typed_object(vm, "WorldEvent");
            set_typed_object_field(vm, event, "Thing", thing);
            set_typed_object_field(vm, event, "Inflictor", inflictor);
            set_typed_object_field(vm, event, "DamageSource", source);
            set_typed_int_field(vm, event, "Damage", i64::from(damage));
            set_typed_string_field(vm, event, "DamageType", damage_type);
            let _ = call_handler_with_event(vm, &resolved, handler, event)?;
        }
        Ok(())
    }

    /// Dispatch map teardown callbacks once, before the world is discarded.
    pub fn unload_map_handlers(&mut self) -> Result<(), room_zvm::Trap> {
        if !self.map_handlers_active {
            return Ok(());
        }
        self.dispatch_handlers("WorldUnloaded")?;
        self.dispatch_handlers("OnUnregister")?;
        self.map_handlers_active = false;
        Ok(())
    }

    /// Drain `Actor.GiveInventory` grants recorded by the host (actor_cookie, item, amount).
    pub fn take_grants(&mut self) -> Vec<(i64, String, i32)> {
        self.play_mut()
            .map(|play| std::mem::take(&mut play.grants))
            .unwrap_or_default()
    }

    fn play_mut(&mut self) -> Option<&mut room_zvm::ZvmPlayHost> {
        self.vm.as_mut().map(|vm| &mut vm.play)
    }

    /// Player number a `players[n].mo` handle stands for (`players.at`
    /// makes a fresh VM object per access, not an actor mirror).
    #[must_use]
    pub fn player_of_cookie(&self, cookie: i64) -> Option<i32> {
        self.vm.as_ref()?.play.mo_player.get(&cookie).copied()
    }

    /// Drop pending grants (tests that start from a known host).
    pub fn clear_grants(&mut self) {
        if let Some(play) = self.play_mut() {
            play.grants.clear();
        }
    }

    /// Drain actor `ChangeTID` writes, resolving script object handles back to
    /// the playsim actor index. Player proxy writes are handled by player TIDs.
    pub fn take_tid_changes(&mut self) -> Vec<(u32, i32)> {
        let changes = self
            .play_mut()
            .map(|play| std::mem::take(&mut play.tid_changes))
            .unwrap_or_default();
        changes
            .into_iter()
            .filter_map(|(cookie, tid)| {
                self.actor_fields.iter().find_map(|(&actor_id, handle)| {
                    let handle_cookie =
                        (i64::from(handle.generation) << 32) | i64::from(handle.index);
                    (handle_cookie == cookie).then_some((actor_id, tid))
                })
            })
            .collect()
    }

    /// Override `PlayerInGame` answers.
    pub fn set_players_in_game(&mut self, map: BTreeMap<i32, bool>) {
        if let Some(play) = self.play_mut() {
            play.players_in_game = map;
        }
    }

    /// Seed `GetPlayerTID` / `SetPlayerTID`.
    pub fn set_player_tids(&mut self, map: BTreeMap<i32, i32>) {
        if let Some(play) = self.play_mut() {
            play.player_tids = map;
        }
    }

    /// Snapshot of the player TID map.
    #[must_use]
    pub fn player_tids(&self) -> BTreeMap<i32, i32> {
        self.vm
            .as_ref()
            .map(|vm| vm.play.player_tids.clone())
            .unwrap_or_default()
    }

    /// Take one player's `SoundClass`, if the script wrote it.
    pub fn take_sound_class(&mut self, player_number: i32) -> Option<String> {
        self.play_mut()
            .and_then(|play| play.sound_class.remove(&player_number))
    }

    fn publish_handler_objs(&mut self) {
        let handlers = self.handler_objs.clone();
        if let Some(play) = self.play_mut() {
            play.handlers = handlers;
        }
    }

    /// True when a ZScript program is loaded.
    #[must_use]
    pub fn is_active(&self) -> bool {
        self.program.is_some()
    }

    /// Call a static / qualified function (ACS `ScriptCall` bridge).
    pub fn script_call(&mut self, name: &str) -> Result<i64, room_zvm::Trap> {
        self.script_call_with(name, &[], &[])
    }

    /// ScriptCall with int / string args.
    pub fn script_call_with(
        &mut self,
        name: &str,
        i_args: &[i64],
        s_args: &[&str],
    ) -> Result<i64, room_zvm::Trap> {
        let vm = self
            .vm
            .as_mut()
            .ok_or_else(|| room_zvm::Trap::Native("no ZScript program".into()))?;
        // Accept `Class.Func` or already-qualified names; also try `Class.Func` from pieces.
        if vm.program.func_by_name.contains_key(name) {
            return vm.call_with(name, i_args, s_args);
        }
        // Case-insensitive fallback.
        let key = vm
            .program
            .func_by_name
            .keys()
            .find(|k| k.eq_ignore_ascii_case(name))
            .cloned();
        if let Some(k) = key {
            return vm.call_with(&k, i_args, s_args);
        }
        Err(room_zvm::Trap::Native(format!("unknown function {name}")))
    }

    /// Dispatch `Class.Method` for each registered event handler that defines it.
    pub fn dispatch_handlers(&mut self, method: &str) -> Result<(), room_zvm::Trap> {
        let handlers = self.event_handlers.clone();
        // Synthetic `dispatch_handlers("WorldThingSpawned")` defaults to monster
        // so BD skill≥6 grant probes keep working without a live actor.
        let monster = method.eq_ignore_ascii_case("WorldThingSpawned");
        for class in handlers {
            self.dispatch_one_handler(&class, method, None, monster, false)?;
        }
        Ok(())
    }

    fn handler_instance(&mut self, class: &str) -> Result<ObjHandle, room_zvm::Trap> {
        if let Some(&h) = self.handler_objs.get(class) {
            return Ok(h);
        }
        let vm = self
            .vm
            .as_mut()
            .ok_or_else(|| room_zvm::Trap::Native("no ZScript program".into()))?;
        let cid = vm
            .program
            .class_by_name
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(class))
            .map(|(_, &i)| i);
        let Some(cid) = cid else {
            return Err(room_zvm::Trap::Native(format!(
                "handler class '{class}' is not in the compiled program"
            )));
        };
        let layout = vm
            .program
            .classes
            .get(cid as usize)
            .cloned()
            .unwrap_or_default();
        let h = vm.heap.new_object(cid, layout.n_i, layout.n_a, layout.n_s);
        self.handler_objs.insert(class.to_owned(), h);
        self.publish_handler_objs();
        Ok(h)
    }

    fn resolve_func_name(&self, class: &str, method: &str) -> Option<String> {
        let qname = format!("{class}.{method}");
        self.vm.as_ref().and_then(|vm| {
            vm.program
                .func_by_name
                .keys()
                .find(|k| k.eq_ignore_ascii_case(&qname))
                .cloned()
        })
    }

    fn dispatch_one_handler(
        &mut self,
        class: &str,
        method: &str,
        player_number: Option<i32>,
        thing_is_monster: bool,
        player_is_return: bool,
    ) -> Result<(), room_zvm::Trap> {
        let Some(resolved) = self.resolve_func_name(class, method) else {
            return Ok(());
        };
        let handler = self.handler_instance(class)?;
        let vm = self.vm.as_mut().unwrap();
        if method.eq_ignore_ascii_case("WorldThingSpawned") {
            let thing = vm.heap.new_object(0, 2, 0, 0);
            if let Some(ObjKind::Object { fields_i, .. }) = vm.heap.get_mut(thing) {
                let flag_idx = if fields_i.len() > 1 { 1 } else { 0 };
                if let Some(slot) = fields_i.get_mut(flag_idx) {
                    *slot = i64::from(thing_is_monster);
                }
            }
            let event = new_typed_object(vm, "WorldEvent");
            set_typed_object_field(vm, event, "Thing", thing);
            let _ = call_handler_with_event(vm, &resolved, handler, event)?;
        } else if [
            "PlayerEntered",
            "PlayerSpawned",
            "PlayerRespawned",
            "PlayerDied",
        ]
        .iter()
        .any(|name| method.eq_ignore_ascii_case(name))
        {
            let event = new_typed_object(vm, "PlayerEvent");
            set_typed_int_field(
                vm,
                event,
                "PlayerNumber",
                i64::from(player_number.unwrap_or(0)),
            );
            set_typed_int_field(vm, event, "IsReturn", i64::from(player_is_return));
            let _ = call_handler_with_event(vm, &resolved, handler, event)?;
        } else if method.eq_ignore_ascii_case("WorldLoaded")
            || method.eq_ignore_ascii_case("WorldUnloaded")
        {
            let event = new_typed_object(vm, "WorldEvent");
            let _ = call_handler_with_event(vm, &resolved, handler, event)?;
        } else if method.eq_ignore_ascii_case("OnRegister") {
            let _ = vm.call_method_self(&resolved, handler)?;
        } else {
            let _ = vm.call_method_self(&resolved, handler)?;
        }
        Ok(())
    }

    fn dispatch_player_event(
        &mut self,
        method: &str,
        player_number: i32,
        is_return: bool,
    ) -> Result<(), room_zvm::Trap> {
        let handlers = self.event_handlers.clone();
        for class in handlers {
            self.dispatch_one_handler(&class, method, Some(player_number), false, is_return)?;
        }
        Ok(())
    }

    /// Dispatch `WorldThingSpawned` with the spawned actor's available fields.
    pub(crate) fn notify_thing_spawned(
        &mut self,
        info: &ZscriptActorEventInfo,
    ) -> Result<(), room_zvm::Trap> {
        let handlers = self.event_handlers.clone();
        for class in handlers {
            let Some(resolved) = self.resolve_func_name(&class, "WorldThingSpawned") else {
                continue;
            };
            let handler = self.handler_instance(&class)?;
            let vm = self
                .vm
                .as_mut()
                .ok_or_else(|| room_zvm::Trap::Native("no ZScript VM".into()))?;
            let thing = event_actor_proxy(vm, &mut self.event_actor_proxies, 0, info);
            let event = new_typed_object(vm, "WorldEvent");
            set_typed_object_field(vm, event, "Thing", thing);
            let _ = call_handler_with_event(vm, &resolved, handler, event)?;
        }
        Ok(())
    }

    /// Run `Class.Tick` (or `PostBeginPlay`) on a script object bound to `actor_id`.
    pub fn call_actor_virtual(
        &mut self,
        actor_id: u32,
        class: &str,
        method: &str,
    ) -> Result<i64, room_zvm::Trap> {
        let resolved = self
            .resolve_method(class, method)
            .ok_or_else(|| room_zvm::Trap::Native(format!("unknown function {class}.{method}")))?;
        let h = self
            .ensure_actor_object(actor_id, class)
            .ok_or_else(|| room_zvm::Trap::Native("no ZScript program".into()))?;
        let vm = self
            .vm
            .as_mut()
            .ok_or_else(|| room_zvm::Trap::Native("no ZScript program".into()))?;
        let result = vm.call_method_self(&resolved, h);
        vm.play.resurrection_candidate = None;
        result
    }

    /// Invoke the ZScript `Actor.CanResurrect(other, passive)` virtual for one
    /// side of UZDoom's two-way resurrection check. A missing script override
    /// accepts by default, matching `Actor.CanResurrect`.
    pub(crate) fn can_resurrect(
        &mut self,
        actor_id: u32,
        class: &str,
        other: &ZscriptActorEventInfo,
        passive: bool,
    ) -> bool {
        if !self.is_active() {
            return true;
        }
        let Some(resolved) = self.resolve_method(class, "CanResurrect") else {
            return true;
        };
        let Some(h) = self.ensure_actor_object(actor_id, class) else {
            return true;
        };
        let Some(vm) = self.vm.as_mut() else {
            return true;
        };
        let other = event_actor_proxy(vm, &mut self.event_actor_proxies, 0, other);
        let i_args = [i64::from(passive)];
        vm.call_method_full(&resolved, h, &i_args, &[], &[other])
            .is_ok_and(|result| result != 0)
    }

    fn ensure_actor_object(&mut self, actor_id: u32, class: &str) -> Option<ObjHandle> {
        let vm = self.vm.as_mut()?;
        let h = *self.actor_fields.entry(actor_id).or_insert_with(|| {
            let cid = vm
                .program
                .class_by_name
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case(class))
                .map(|(_, &id)| id)
                .unwrap_or(0);
            let layout = vm
                .program
                .classes
                .get(cid as usize)
                .cloned()
                .unwrap_or_default();
            vm.heap
                .new_object_f(cid, layout.n_i, layout.n_a, layout.n_s, layout.n_f)
        });
        Some(h)
    }

    /// Copy the playsim actor's native fields into its script object before a
    /// call, so `pos`, `vel`, `angle`, `health`, `target`, … read live values.
    pub(crate) fn write_actor_fields(&mut self, snap: &ActorFieldSnap) -> Option<ObjHandle> {
        let h = self.ensure_actor_object(snap.id.0, &snap.class)?;
        self.actor_gen.insert(snap.id.0, snap.id.1);
        let mut ptrs = Vec::new();
        for (name, other) in &snap.ptrs {
            let handle = match other {
                Some((index, generation, class)) => {
                    self.actor_gen.insert(*index, *generation);
                    self.ensure_actor_object(*index, class).unwrap_or(ObjHandle::NULL)
                }
                None => ObjHandle::NULL,
            };
            ptrs.push((*name, handle));
        }
        let slots: Vec<_> = {
            let vm = self.vm.as_ref()?;
            let floats: Vec<_> = snap
                .floats
                .iter()
                .map(|(n, v)| (self.cached_field(vm, h, n), *v))
                .collect();
            let ints: Vec<_> = snap
                .ints
                .iter()
                .map(|(n, v)| (self.cached_field(vm, h, n), *v))
                .collect();
            let objs: Vec<_> = ptrs
                .iter()
                .map(|(n, v)| (self.cached_field(vm, h, n), *v))
                .collect();
            vec![(floats, ints, objs)]
        };
        let vm = self.vm.as_mut()?;
        let Some(ObjKind::Object {
            fields_i,
            fields_a,
            fields_f,
            ..
        }) = vm.heap.get_mut(h)
        else {
            return Some(h);
        };
        for (floats, ints, objs) in slots {
            for (slot, v) in floats {
                if let Some((3, index)) = slot {
                    if fields_f.len() <= index {
                        fields_f.resize(index + 1, 0.0);
                    }
                    fields_f[index] = v;
                }
            }
            for (slot, v) in ints {
                if let Some((0, index)) = slot
                    && let Some(cell) = fields_i.get_mut(index)
                {
                    *cell = v;
                }
            }
            for (slot, v) in objs {
                if let Some((1, index)) = slot
                    && let Some(cell) = fields_a.get_mut(index)
                {
                    *cell = v;
                }
            }
        }
        Some(h)
    }

    /// Read the native fields back after a call. Pointers come back as
    /// `(index, generation)` of the actor whose script object they hold.
    pub(crate) fn read_actor_fields(&self, h: ObjHandle, like: &ActorFieldSnap) -> ActorFieldSnap {
        let mut out = ActorFieldSnap {
            id: like.id,
            class: like.class.clone(),
            ..ActorFieldSnap::default()
        };
        let Some(vm) = self.vm.as_ref() else {
            return out;
        };
        let Some(ObjKind::Object {
            fields_i,
            fields_a,
            fields_f,
            ..
        }) = vm.heap.get(h)
        else {
            return like.clone();
        };
        for (name, old) in &like.floats {
            let v = match self.cached_field(vm, h, name) {
                Some((3, index)) => fields_f.get(index).copied().unwrap_or(0.0),
                _ => *old,
            };
            out.floats.push((*name, v));
        }
        for (name, old) in &like.ints {
            let v = match self.cached_field(vm, h, name) {
                Some((0, index)) => fields_i.get(index).copied().unwrap_or(*old),
                _ => *old,
            };
            out.ints.push((*name, v));
        }
        for (name, old) in &like.ptrs {
            let got = match self.cached_field(vm, h, name) {
                Some((1, index)) => fields_a.get(index).copied(),
                _ => None,
            };
            let value = match got {
                None => old.clone(),
                Some(handle) if handle.is_null() => None,
                Some(handle) => self
                    .actor_fields
                    .iter()
                    .find(|(_, obj)| **obj == handle)
                    .and_then(|(&index, _)| {
                        let generation = *self.actor_gen.get(&index)?;
                        Some((index, generation, String::new()))
                    })
                    .or_else(|| old.clone()),
            };
            out.ptrs.push((*name, value));
        }
        out
    }

    /// Run `Class.ModifyDamage` for an inventory item (ZS4b).
    ///
    /// Integer parameters are `damage`, `newDamage` (starts equal to `damage`),
    /// `passive`, `flags`. The damage type name is `s[0]`. `inflictor` then
    /// `source` are object arguments; a monster source has `bISMONSTER` set.
    /// The value written to `newDamage` is returned.
    pub fn modify_damage(
        &mut self,
        class: &str,
        damage: i32,
        dtype: &str,
        passive: bool,
        has_source: bool,
        source_monster: bool,
        source_class: &str,
        has_inflictor: bool,
        inflictor_class: &str,
    ) -> i32 {
        if !self.is_active() || !self.has_method(class, "ModifyDamage") {
            return damage;
        }
        let qname = format!("{class}.ModifyDamage");
        let Some(h) = self.modify_object(class) else {
            return damage;
        };
        let inf = if has_inflictor {
            self.actor_proxy(0, false, inflictor_class)
        } else {
            ObjHandle::NULL
        };
        let src = if has_source {
            self.actor_proxy(1, source_monster, source_class)
        } else {
            ObjHandle::NULL
        };
        let Some(vm) = self.vm.as_mut() else {
            return damage;
        };
        let i_args = [i64::from(damage), i64::from(damage), i64::from(passive), 0];
        if vm
            .call_method_full(&qname, h, &i_args, &[dtype], &[inf, src])
            .is_err()
        {
            return damage;
        }
        vm.last_i[1].clamp(0, i64::from(i32::MAX)) as i32
    }

    fn actor_proxy(&mut self, slot: usize, monster: bool, class_name: &str) -> ObjHandle {
        while self.actor_proxies.len() <= slot {
            let h = self
                .vm
                .as_mut()
                .map(|vm| vm.heap.new_object(0, 4, 1, 1))
                .unwrap_or(ObjHandle::NULL);
            self.actor_proxies.push(h);
        }
        let h = self.actor_proxies[slot];
        if let Some(vm) = self.vm.as_mut() {
            let name = vm.heap.intern_string(class_name);
            if let Some(ObjKind::Object {
                fields_i, fields_s, ..
            }) = vm.heap.get_mut(h)
            {
                if fields_i.len() < 2 {
                    fields_i.resize(2, 0);
                }
                fields_i[1] = i64::from(monster);
                if fields_s.is_empty() {
                    fields_s.push(name);
                } else {
                    fields_s[0] = name;
                }
            }
        }
        h
    }

    fn modify_object(&mut self, class: &str) -> Option<ObjHandle> {
        let key = class.to_ascii_lowercase();
        if let Some(h) = self.modify_objs.get(&key).copied() {
            return Some(h);
        }
        let vm = self.vm.as_mut()?;
        let cid = vm
            .program
            .class_by_name
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(class))
            .map(|(_, id)| *id)
            .unwrap_or(0);
        let layout = vm
            .program
            .classes
            .get(cid as usize)
            .cloned()
            .unwrap_or_default();
        let h = vm.heap.new_object(cid, layout.n_i, layout.n_a, layout.n_s);
        self.modify_objs.insert(key, h);
        Some(h)
    }

    /// Bytes mixed into [`crate::world::World::checksum`] (ZS6 / demos / netplay).
    ///
    /// Stable digest of program hash + sorted actor field integers.
    #[must_use]
    pub fn checksum_bytes(&self) -> Option<[u8; 32]> {
        let prog = self.program.as_ref()?;
        let vm = self.vm.as_ref()?;
        let mut bytes = Vec::with_capacity(64);
        bytes.extend_from_slice(&prog.program_hash.to_le_bytes());
        for (&actor, &h) in &self.actor_fields {
            bytes.extend_from_slice(&actor.to_le_bytes());
            if let Some(ObjKind::Object { fields_i, .. }) = vm.heap.get(h) {
                bytes.extend_from_slice(&(fields_i.len() as u32).to_le_bytes());
                for v in fields_i {
                    bytes.extend_from_slice(&v.to_le_bytes());
                }
            }
        }
        bytes.extend_from_slice(&vm.heap.stable_digest().to_le_bytes());
        let mut out = [0u8; 32];
        out[..8].copy_from_slice(&prog.program_hash.to_le_bytes());
        let mut acc = [0u64; 3];
        for (i, chunk) in bytes.chunks(8).enumerate() {
            let mut tmp = [0u8; 8];
            tmp[..chunk.len()].copy_from_slice(chunk);
            acc[i % 3] ^= u64::from_le_bytes(tmp);
        }
        out[8..16].copy_from_slice(&acc[0].to_le_bytes());
        out[16..24].copy_from_slice(&acc[1].to_le_bytes());
        out[24..32].copy_from_slice(&acc[2].to_le_bytes());
        Some(out)
    }

    /// Snapshot script state for saves (ZS6).
    #[must_use]
    pub fn save_blob(&self) -> Option<ZscriptSaveBlob> {
        let prog = self.program.as_ref()?;
        let vm = self.vm.as_ref()?;
        let mut blob = ZscriptSaveBlob {
            program_hash: prog.program_hash,
            actor_i: BTreeMap::new(),
            objects: Vec::new(),
            heap: Some(vm.heap.image()),
            handlers: self
                .handler_objs
                .iter()
                .map(|(name, h)| (name.clone(), h.index, h.generation))
                .collect(),
            actor_handles: self
                .actor_fields
                .iter()
                .map(|(&actor, h)| (actor, h.index, h.generation))
                .collect(),
        };
        for (&actor, &h) in &self.actor_fields {
            if let Some(ObjKind::Object { fields_i, .. }) = vm.heap.get(h) {
                blob.actor_i.insert(actor, fields_i.clone());
            }
        }
        for &h in self.actor_fields.values() {
            if h.is_null() {
                continue;
            }
            if let Some(ObjKind::Object {
                class_id, fields_i, ..
            }) = vm.heap.get(h)
            {
                blob.objects.push(SavedObject {
                    index: h.index,
                    generation: h.generation,
                    class_id: *class_id,
                    fields_i: fields_i.clone(),
                });
            }
        }
        blob.objects.sort_by_key(|o| o.index);
        Some(blob)
    }

    /// Restore script fields from a save blob (ZS6).
    pub fn load_blob(
        &mut self,
        blob: &ZscriptSaveBlob,
        allow_mismatch: bool,
    ) -> Result<(), String> {
        let prog = self
            .program
            .as_ref()
            .ok_or_else(|| "no ZScript program loaded".to_owned())?;
        if prog.program_hash != blob.program_hash && !allow_mismatch {
            return Err(format!(
                "ZScript program hash mismatch: save {:#x} != loaded {:#x}",
                blob.program_hash, prog.program_hash
            ));
        }
        let vm = self.vm.as_mut().ok_or_else(|| "no ZScript VM".to_owned())?;
        if let Some(image) = &blob.heap {
            vm.heap.restore(image);
            self.actor_fields.clear();
            for &(actor, index, generation) in &blob.actor_handles {
                self.actor_fields
                    .insert(actor, ObjHandle { index, generation });
            }
            self.handler_objs.clear();
            for (name, index, generation) in &blob.handlers {
                self.handler_objs.insert(
                    name.clone(),
                    ObjHandle {
                        index: *index,
                        generation: *generation,
                    },
                );
            }
            self.publish_handler_objs();
            return Ok(());
        }
        self.actor_fields.clear();
        for (&actor, fields) in &blob.actor_i {
            let layout_n = fields.len() as u16;
            let h = vm.heap.new_object(0, layout_n, 0, 0);
            if let Some(ObjKind::Object { fields_i, .. }) = vm.heap.get_mut(h) {
                *fields_i = fields.clone();
            }
            self.actor_fields.insert(actor, h);
        }
        Ok(())
    }
}

/// Slots of the UZDoom `Actor` / `Inventory` native fields in this program.
fn actor_slots_of(program: &CompiledProgram) -> room_zvm::ActorSlots {
    let layout = |name: &str| program.classes.iter().find(|c| c.name.eq_ignore_ascii_case(name));
    let field = |class: &str, name: &str, bank: u8| {
        layout(class)
            .and_then(|l| l.fields.get(name).copied())
            .filter(|(b, _)| *b == bank)
            .map(|(_, i)| usize::from(i))
    };
    room_zvm::ActorSlots {
        pos: field("Actor", "pos", 3),
        vel: field("Actor", "vel", 3),
        angle: field("Actor", "angle", 3),
        pitch: field("Actor", "pitch", 3),
        radius: field("Actor", "radius", 3),
        height: field("Actor", "height", 3),
        amount: field("Inventory", "amount", 0),
        max_amount: field("Inventory", "maxamount", 0),
        target: field("Actor", "target", 1),
        master: field("Actor", "master", 1),
        tracer: field("Actor", "tracer", 1),
    }
}

fn call_handler_with_event(
    vm: &mut Zvm,
    name: &str,
    self_h: ObjHandle,
    event: ObjHandle,
) -> Result<i64, room_zvm::Trap> {
    vm.call_method_self_args(name, self_h, &[event])
}

fn new_typed_object(vm: &mut Zvm, type_name: &str) -> ObjHandle {
    let class_id = vm
        .program
        .class_by_name
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case(type_name))
        .map(|(_, &id)| id);
    let Some(class_id) = class_id else {
        return vm.heap.new_object(0, 8, 4, 4);
    };
    let layout = vm
        .program
        .classes
        .get(class_id as usize)
        .cloned()
        .unwrap_or_default();
    vm.heap
        .new_object_f(class_id, layout.n_i, layout.n_a, layout.n_s, layout.n_f)
}

fn typed_field(vm: &Zvm, object: ObjHandle, name: &str) -> Option<(u8, usize)> {
    let class_id = match vm.heap.get(object)? {
        ObjKind::Object { class_id, .. } => *class_id,
        _ => return None,
    };
    let layout = vm.program.classes.get(class_id as usize)?;
    let (bank, index) = layout.fields.get(&name.to_ascii_lowercase()).copied()?;
    Some((bank, usize::from(index)))
}

/// `(bank, slot)` of `name` on the object's class or an ancestor. A vector
/// component is spelled `pos.y`.
fn walk_field(vm: &Zvm, object: ObjHandle, name: &str) -> Option<(u8, usize)> {
    let class_id = match vm.heap.get(object)? {
        ObjKind::Object { class_id, .. } => *class_id,
        _ => return None,
    };
    let (base, offset) = match name.split_once('.') {
        Some((base, axis)) => (
            base,
            match axis {
                "x" => 0,
                "y" => 1,
                "z" => 2,
                _ => 3,
            },
        ),
        None => (name, 0),
    };
    let key = base.to_ascii_lowercase();
    let mut layout = vm.program.classes.get(class_id as usize)?;
    for _ in 0..64 {
        if let Some(&(bank, index)) = layout.fields.get(&key) {
            return Some((bank, usize::from(index) + offset));
        }
        if layout.parent.is_empty() {
            return None;
        }
        let parent = layout.parent.clone();
        layout = vm
            .program
            .classes
            .iter()
            .find(|c| c.name.eq_ignore_ascii_case(&parent))?;
    }
    None
}

fn set_typed_int_field(vm: &mut Zvm, object: ObjHandle, name: &str, value: i64) {
    let Some((0, index)) = typed_field(vm, object, name) else {
        return;
    };
    if let Some(ObjKind::Object { fields_i, .. }) = vm.heap.get_mut(object)
        && let Some(slot) = fields_i.get_mut(index)
    {
        *slot = value;
    }
}

fn set_typed_object_field(vm: &mut Zvm, object: ObjHandle, name: &str, value: ObjHandle) {
    let Some((1, index)) = typed_field(vm, object, name) else {
        return;
    };
    if let Some(ObjKind::Object { fields_a, .. }) = vm.heap.get_mut(object)
        && let Some(slot) = fields_a.get_mut(index)
    {
        *slot = value;
    }
}

fn set_typed_string_field(vm: &mut Zvm, object: ObjHandle, name: &str, value: &str) {
    let Some((2, index)) = typed_field(vm, object, name) else {
        return;
    };
    let text = vm.heap.intern_string(value);
    if let Some(ObjKind::Object { fields_s, .. }) = vm.heap.get_mut(object)
        && let Some(slot) = fields_s.get_mut(index)
    {
        *slot = text;
    }
}

fn event_actor_proxy(
    vm: &mut Zvm,
    proxies: &mut Vec<ObjHandle>,
    slot: usize,
    info: &ZscriptActorEventInfo,
) -> ObjHandle {
    while proxies.len() <= slot {
        let actor = new_typed_object(vm, "Actor");
        proxies.push(actor);
    }
    let actor = proxies[slot];
    set_typed_int_field(vm, actor, "Health", i64::from(info.health));
    set_typed_int_field(vm, actor, "TID", i64::from(info.tid));
    set_typed_int_field(vm, actor, "bIsMonster", i64::from(info.is_monster));
    set_typed_int_field(vm, actor, "bISMONSTER", i64::from(info.is_monster));
    set_typed_string_field(vm, actor, "ClassName", &info.class_name);
    actor
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use room_zvm::{CompiledFunction, Op};

    #[test]
    fn save_load_roundtrip() {
        let mut f = CompiledFunction {
            name: "nop".into(),
            n_i: 1,
            ..Default::default()
        };
        f.emit(Op::Return, 0, 0, 0);
        let mut p = CompiledProgram {
            functions: vec![f],
            ..Default::default()
        };
        p.finalize();
        let mut rt = ZscriptRuntime::default();
        rt.load_program(p);
        let vm = rt.vm.as_mut().unwrap();
        let h = vm.heap.new_object(0, 2, 0, 0);
        if let Some(ObjKind::Object { fields_i, .. }) = vm.heap.get_mut(h) {
            fields_i[0] = 42;
            fields_i[1] = 7;
        }
        rt.actor_fields.insert(3, h);
        let blob = rt.save_blob().unwrap();
        assert_eq!(blob.actor_i[&3], vec![42, 7]);

        let mut rt2 = ZscriptRuntime::default();
        let mut p2 = CompiledProgram::default();
        p2.program_hash = blob.program_hash;
        rt2.program = Some(Arc::new(p2));
        rt2.vm = Some(Zvm::new(CompiledProgram::default()));
        rt2.load_blob(&blob, true).unwrap();
        let h2 = rt2.actor_fields[&3];
        let fields = match rt2.vm.as_ref().unwrap().heap.get(h2) {
            Some(ObjKind::Object { fields_i, .. }) => fields_i.clone(),
            _ => panic!("missing"),
        };
        assert_eq!(fields, vec![42, 7]);
    }

    #[test]
    fn checksum_tracks_field_mutations() {
        let mut f = CompiledFunction {
            name: "nop".into(),
            n_i: 1,
            ..Default::default()
        };
        f.emit(Op::Return, 0, 0, 0);
        let mut p = CompiledProgram {
            functions: vec![f],
            ..Default::default()
        };
        p.finalize();
        let mut rt = ZscriptRuntime::default();
        rt.load_program(p);
        let h = {
            let vm = rt.vm.as_mut().unwrap();
            let h = vm.heap.new_object(0, 1, 0, 0);
            if let Some(ObjKind::Object { fields_i, .. }) = vm.heap.get_mut(h) {
                fields_i[0] = 1;
            }
            h
        };
        rt.actor_fields.insert(1, h);
        let a = rt.checksum_bytes().unwrap();
        if let Some(ObjKind::Object { fields_i, .. }) = rt.vm.as_mut().unwrap().heap.get_mut(h) {
            fields_i[0] = 2;
        }
        let b = rt.checksum_bytes().unwrap();
        assert_ne!(a, b, "checksum must change when script fields mutate");
    }
}
