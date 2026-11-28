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
    /// State of the script `Random` generator, so a loaded game draws the
    /// same numbers it would have drawn going on.
    pub script_rng: u64,
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
    #[allow(clippy::type_complexity)]
    field_cache: std::cell::RefCell<BTreeMap<(u32, &'static str), Option<(u8, usize)>>>,
    /// Map (MD5, portal revision) the published portal snapshot is for.
    portal_key: Option<([u8; 16], u32)>,
    /// Point → sector over a copy of the map's nodes, per map.
    portal_lookup: Option<([u8; 16], room_zvm::SectorLookup)>,
    /// Map the published `level.Sectors` / `Lines` objects are for.
    map_objects_key: Option<[u8; 16]>,
    /// Tic the map objects were last refreshed on.
    map_objects_tic: Option<u32>,
}

/// Copy for world snapshots (rollback), VM included (its compiled program
/// is shared, its heap copied).
impl Clone for ZscriptRuntime {
    fn clone(&self) -> Self {
        Self {
            program: self.program.clone(),
            vm: self.vm.clone(),
            actor_fields: self.actor_fields.clone(),
            begun_play: self.begun_play.clone(),
            event_handlers: self.event_handlers.clone(),
            handler_objs: self.handler_objs.clone(),
            modify_objs: self.modify_objs.clone(),
            actor_proxies: self.actor_proxies.clone(),
            event_actor_proxies: self.event_actor_proxies.clone(),
            target_sentinel: self.target_sentinel,
            map_handlers_active: self.map_handlers_active,
            actor_gen: self.actor_gen.clone(),
            field_cache: self.field_cache.clone(),
            portal_key: self.portal_key,
            portal_lookup: self.portal_lookup.clone(),
            map_objects_key: self.map_objects_key,
            map_objects_tic: self.map_objects_tic,
        }
    }
}

/// Parts of UZDoom's `PlayerPawn.PlayerThink` the playsim does natively
/// (crouching, weapons and psprites, use, environment, counters): left out
/// when a mod's movement script runs that think.
const PLAYER_THINK_HOST_PARTS: &[&str] = &[
    "CheckFOV",
    "CheckCheats",
    "CheckCrouch",
    "CheckMusicChange",
    "MorphPlayerThink",
    "MakeFootsteps",
    "CheckEnvironment",
    "CheckUse",
    "CheckUndoMorph",
    "TickPSprites",
    "CheckPoison",
    "CheckDegeneration",
    "CheckAirSupply",
];

/// Classes whose [`HOST_ACTIONS`] bodies the playsim replaces.
const HOST_ACTION_CLASSES: &[&str] = &["StateProvider", "Weapon"];

/// Library weapon actions Room implements natively (`think_player`,
/// `action.rs`): a script call to one queues the playsim action.
const HOST_ACTIONS: &[&str] = &[
    "A_WeaponReady",
    "A_ReFire",
    "A_ClearReFire",
    "A_GunFlash",
    "A_Lower",
    "A_Raise",
    "A_CheckReload",
    "A_FireBullets",
    "A_FireProjectile",
    "A_FireCustomMissile",
    "A_RailAttack",
    "A_CustomPunch",
    "A_Light",
    "A_Light0",
    "A_Light1",
    "A_Light2",
    "A_ZoomFactor",
    "A_SetCrosshair",
];

/// `Actor` library actions whose script bodies need engine natives Room
/// lacks (`GetPointer`): the playsim's DECORATE versions run instead.
const HOST_ACTOR_ACTIONS: &[&str] = &["A_GiveInventory", "A_TakeInventory", "A_SetInventory"];

/// Key range of weapon script objects in [`ZscriptRuntime::actor_fields`]
/// (above any actor index).
const WEAPON_OBJECT_KEY: u32 = 0xc000_0000;

/// Key range of the `PlayerInfo` objects weapon calls read through `player`.
const PLAYER_INFO_KEY: u32 = 0xa000_0000;

/// The weapon whose action function runs ([`ZscriptRuntime::call_weapon_action`]).
#[derive(Clone, Debug, Default)]
pub(crate) struct WeaponCallInfo {
    /// Weapon class name.
    pub class: String,
    /// `Weapon.AmmoType1` / `AmmoType2` (empty = none).
    pub ammo_type1: String,
    pub ammo_type2: String,
    /// `Weapon.AmmoUse1` / `AmmoUse2`.
    pub ammo_use1: i32,
    pub ammo_use2: i32,
    /// The owner's buttons this tic and last tic, as UZDoom `BT_*` bits.
    pub buttons: i32,
    pub old_buttons: i32,
    /// The owner's eye Z and crouch factor (`player.viewz`, `CrouchFactor`).
    pub view_z: f64,
    pub crouch_factor: f64,
}

/// An integer argument from state argument text (`3`, `1.5`, `true`).
fn arg_int(text: &str) -> i64 {
    if let Ok(v) = text.parse::<i64>() {
        return v;
    }
    if let Some(hex) = text.strip_prefix("0x").or_else(|| text.strip_prefix("0X")) {
        return i64::from_str_radix(hex, 16).unwrap_or(0);
    }
    match text.to_ascii_lowercase().as_str() {
        "true" => 1,
        "false" | "null" | "" => 0,
        _ => text.parse::<f64>().map_or(0, |v| v as i64),
    }
}

/// A float argument from state argument text.
fn arg_float(text: &str) -> f64 {
    text.parse::<f64>()
        .ok()
        .or_else(|| text.parse::<i64>().ok().map(|v| v as f64))
        .unwrap_or(0.0)
}

/// A string, name, class or state-label argument: quotes removed, `null`
/// and `""` empty.
fn arg_string(text: &str) -> String {
    let t = text.trim();
    if t.eq_ignore_ascii_case("null") {
        return String::new();
    }
    t.trim_matches('"').trim_matches('\'').to_owned()
}

/// Player state a movement script (`PlayerThink`) reads and writes
/// ([`ZscriptRuntime::call_player_think`]).
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct PlayerMoveState {
    pub buttons: u32,
    pub old_buttons: u32,
    pub forward_move: i16,
    pub side_move: i16,
    pub up_move: i16,
    pub yaw: i16,
    pub pitch: i16,
    pub on_ground: bool,
    pub jump_tics: i32,
    pub turn_ticks: i32,
    pub centering: bool,
    pub cheats: i32,
    pub health: i32,
    pub water_level: i32,
    pub view_z: f64,
    pub view_height: f64,
    pub delta_view_height: f64,
    pub crouch_factor: f64,
    pub crouch_view_delta: f64,
    /// The pawn's `ViewHeight` property.
    pub pawn_view_height: f64,
    /// The pawn's `Player.ViewBob`.
    pub view_bob: f64,
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
    #[allow(clippy::type_complexity)]
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

    fn load_program_inner(&mut self, mut program: CompiledProgram, stub_missing: bool) {
        // The UZDoom library's weapon and firing actions are script bodies
        // over engine natives Room does not have; Room runs them natively.
        program.route_to_host_actions(HOST_ACTION_CLASSES, HOST_ACTIONS);
        program.route_to_host_actions(&["Actor"], HOST_ACTOR_ACTIONS);
        // A mod's movement code (`PlayerThink` / `HandleMovement` overrides)
        // runs in the VM; the rest of UZDoom's player think is Room's own.
        program.route_to_noops(&["PlayerPawn"], PLAYER_THINK_HOST_PARTS);
        #[cfg(feature = "zvm-aot")]
        let shipped = room_zvm_wasm::shipped_for(&program);
        let program = Arc::new(program);
        let mut vm = if stub_missing {
            Zvm::new_with_stubs((*program).clone())
        } else {
            Zvm::new((*program).clone())
        };
        vm.host = 0;
        #[cfg(feature = "zvm-aot")]
        if let Some(m) = shipped {
            // A converted mod (`room mod convert`) runs its shipped module.
            match vm.attach_shipped(&m.bytes, m.compiled.clone(), m.hot.clone()) {
                Ok(()) => eprintln!(
                    "zscript: converted mod module attached ({}, {} functions)",
                    vm.aot_engine().unwrap_or("?"),
                    m.compiled.iter().filter(|c| **c).count()
                ),
                Err(e) => eprintln!("zscript: converted module rejected, interpreting: {e}"),
            }
        } else {
            Self::maybe_enable_aot(&mut vm);
        }
        #[cfg(not(feature = "zvm-aot"))]
        Self::maybe_enable_aot(&mut vm);
        // A host that builds modules out of band (the browser) attaches here.
        if !vm.has_aot() {
            room_zvm::provide_aot(&mut vm, &program);
        }
        vm.play.flag_names = Arc::new(crate::flags::flag_names());
        vm.play.class_names = program
            .classes
            .iter()
            .map(|class| class.name.clone())
            .collect();
        // `players[n]` is a `PlayerInfo` of this program's layout, so its
        // `mo` field reads the pawn proxy (`players.at`).
        vm.play.playerinfo_layout = program
            .classes
            .iter()
            .position(|c| c.name.eq_ignore_ascii_case("PlayerInfo"))
            .and_then(|id| {
                let c = &program.classes[id];
                let &(bank, slot) = c.fields.get("mo")?;
                (bank == 1).then_some(room_zvm::PlayerInfoLayout {
                    class_id: id as u32,
                    n_i: c.n_i,
                    n_a: c.n_a,
                    n_s: c.n_s,
                    n_f: c.n_f,
                    mo_slot: slot,
                })
            });
        self.vm = Some(vm);
        self.program = Some(Arc::clone(&program));
        self.map_objects_key = None;
        self.map_objects_tic = None;
        self.portal_key = None;
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

    /// `ROOM_ZVM_TIER=aot`: compile the program to WebAssembly (`docs/37`
    /// ZW3) and run compiled functions there. Falls back to the interpreter
    /// (with a log line) when code generation fails.
    #[cfg(feature = "zvm-aot")]
    fn maybe_enable_aot(vm: &mut Zvm) {
        let want = std::env::var("ROOM_ZVM_TIER")
            .is_ok_and(|v| v.eq_ignore_ascii_case("aot") || v.eq_ignore_ascii_case("aot-all"));
        if !want || vm.program.functions.is_empty() {
            return;
        }
        match room_zvm_wasm::enable_aot(vm) {
            Ok(stats) => eprintln!(
                "zscript: aot tier on {} ({}/{} functions compiled, {} bytes of wasm)",
                vm.aot_engine().unwrap_or("?"),
                stats.compiled,
                stats.functions,
                stats.bytes
            ),
            Err(e) => eprintln!("zscript: aot tier unavailable, interpreting: {e}"),
        }
    }

    #[cfg(not(feature = "zvm-aot"))]
    fn maybe_enable_aot(_vm: &mut Zvm) {}

    /// Enable the AOT tier for the loaded program regardless of
    /// `ROOM_ZVM_TIER` (tests, tools). Returns false without the `zvm-aot`
    /// feature or when code generation fails.
    pub fn enable_aot(&mut self) -> bool {
        #[cfg(feature = "zvm-aot")]
        if let Some(vm) = self.vm.as_mut() {
            if vm.aot_active() {
                return true;
            }
            return room_zvm_wasm::enable_aot(vm).is_ok();
        }
        false
    }

    /// Ask the AOT provider again (`room_zvm::set_aot_provider`): attaches a
    /// module that became ready after the map started. True when attached.
    pub fn refresh_aot(&mut self) -> bool {
        let (Some(vm), Some(program)) = (self.vm.as_mut(), self.program.as_ref()) else {
            return false;
        };
        !vm.has_aot() && room_zvm::provide_aot(vm, program)
    }

    /// Send every compiled function through the AOT tier, not only the ones
    /// with loops or long bodies (tests that compare tiers).
    pub fn set_aot_all(&mut self) {
        #[cfg(feature = "zvm-aot")]
        if let Some(vm) = self.vm.as_mut() {
            vm.set_aot_policy(room_zvm::AotPolicy::All);
        }
    }

    /// Wasm engine of the AOT tier (`wasmtime` / `wasmi`), when attached.
    #[must_use]
    pub fn aot_engine(&self) -> Option<&'static str> {
        #[cfg(feature = "zvm-aot")]
        if let Some(vm) = self.vm.as_ref() {
            return vm.aot_engine();
        }
        None
    }

    /// Top-level ZScript calls the AOT tier has served.
    #[must_use]
    pub fn aot_calls(&self) -> u64 {
        #[cfg(feature = "zvm-aot")]
        if let Some(vm) = self.vm.as_ref() {
            return vm.aot_calls();
        }
        0
    }

    /// Per-call world answers for the typed natives (`CheckSight`, `CountInv`, …).
    pub(crate) fn set_call_world(
        &mut self,
        sight: Vec<(ObjHandle, bool)>,
        melee_ok: bool,
        self_target: ObjHandle,
        inventory: Vec<(ObjHandle, Vec<room_zvm::InventoryEntry>)>,
    ) {
        if let Some(vm) = self.vm.as_mut() {
            vm.play.sight = sight;
            vm.play.melee_ok = melee_ok;
            vm.play.self_target = self_target;
            vm.play.inventory = inventory;
        }
    }

    /// Flag bits of the actors mirrored for this call (`victim.bSHOOTABLE`).
    pub(crate) fn set_call_flags(&mut self, flags: Vec<(ObjHandle, Vec<u64>)>) {
        if let Some(vm) = self.vm.as_mut() {
            vm.play.actor_flags = flags;
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
        self.field_cache
            .borrow_mut()
            .insert((class_id, name), found);
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

    /// Publish the map's portals for the portal natives (and the group
    /// count). Rebuilt when the map or its portals change.
    pub fn set_portals(&mut self, map: &room_map::MapGeometry) {
        self.set_portal_group_count(map.setup.portal_groups.size.max(1));
        let key = (map.md5, map.setup.portals.revision);
        if self.portal_key == Some(key) || self.vm.is_none() {
            return;
        }
        self.portal_key = Some(key);
        let info = portal_info(map, &mut self.portal_lookup);
        if let Some(play) = self.play_mut() {
            play.portals = Arc::new(info);
        }
    }

    /// Whether [`Self::sync_map_objects`] has anything to do for this map
    /// and tic.
    pub(crate) fn map_objects_due(&self, md5: [u8; 16], tic: u32) -> bool {
        self.vm.is_some()
            && (self.map_objects_key != Some(md5)
                || (self.map_objects_tic != Some(tic)
                    && self
                        .vm
                        .as_ref()
                        .is_some_and(|vm| !vm.play.map_objects.sectors.is_empty())))
    }

    /// Build the level's `Sector` / `Line` / `Side` / `Vertex` objects (once
    /// per map) and refresh what moves (plane heights, light, flats; once
    /// per tic). Needs a program with UZDoom's map structs.
    pub(crate) fn sync_map_objects(&mut self, m: &MapSync<'_>) {
        let Some(vm) = self.vm.as_mut() else {
            return;
        };
        if self.map_objects_key != Some(m.map.md5) {
            self.map_objects_key = Some(m.map.md5);
            self.map_objects_tic = None;
            vm.play.map_objects = build_map_objects(vm, m.map);
        }
        if vm.play.map_objects.sectors.is_empty() || self.map_objects_tic == Some(m.tic) {
            return;
        }
        self.map_objects_tic = Some(m.tic);
        refresh_map_objects(vm, m);
    }

    /// Point the actor object `h`'s `CurSector` at the map `Sector` object
    /// of `sector`.
    pub(crate) fn set_cur_sector(&mut self, h: ObjHandle, sector: Option<usize>) {
        let Some(vm) = self.vm.as_mut() else {
            return;
        };
        let Some(sec) = sector.and_then(|s| vm.play.map_objects.sectors.get(s).copied()) else {
            return;
        };
        for field in ["cursector", "floorsector", "ceilingsector"] {
            if let Some((1, slot)) = walk_field(vm, h, field) {
                if let Some(ObjKind::Object { fields_a, .. }) = vm.heap.get_mut(h) {
                    if let Some(v) = fields_a.get_mut(slot) {
                        *v = sec;
                    }
                }
            }
        }
    }

    /// Position of the actor the next call is for (`CurSector`).
    pub fn set_caller_pos(&mut self, pos: Option<[f64; 3]>) {
        if let Some(play) = self.play_mut() {
            play.caller_pos = pos;
        }
    }

    /// Publish the map's linked-portal group count (`GetPortalGroupCount`).
    pub fn set_portal_group_count(&mut self, n: usize) {
        if let Some(play) = self.play_mut() {
            play.portal_group_count = i32::try_from(n).unwrap_or(i32::MAX);
        }
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

    /// Integer value of a CVar the ZScript host knows (`None` when unset).
    #[must_use]
    pub fn cvar_int(&self, name: &str) -> Option<i32> {
        let play = &self.vm.as_ref()?.play;
        let key = name.to_ascii_lowercase();
        (play.cvars.contains_key(&key)
            || play.cvar_floats.contains_key(&key)
            || play.cvar_strings.contains_key(&key))
        .then(|| play.cvar(name))
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

    /// Set one mod CVar from its text value (CVARINFO / Settings): integers
    /// and bools as ints, decimals as floats, anything else as a string.
    pub fn set_cvar_text(&mut self, name: &str, value: &str) {
        if let Some(play) = self.play_mut() {
            let v = value.trim();
            if let Ok(i) = v.parse::<i32>() {
                play.set_cvar_int(name, i);
            } else if let Ok(f) = v.parse::<f64>() {
                play.set_cvar_float(name, f);
            } else {
                play.set_cvar_string(name, v);
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
        // One failing handler must not keep the others (or `WorldLoaded`)
        // from running: report the first trap after both passes.
        let registered = self.dispatch_handlers("OnRegister");
        let loaded = self.dispatch_handlers("WorldLoaded");
        self.map_handlers_active = true;
        registered.and(loaded)
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
        self.vm.as_mut().map(|vm| &mut *vm.play)
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
        let mut first = None;
        for class in handlers {
            if let Err(e) = self.dispatch_one_handler(&class, method, None, monster, false) {
                // Name the handler; keep dispatching to the rest.
                first.get_or_insert(room_zvm::Trap::Native(format!("{class}.{method}: {e}")));
            }
        }
        first.map_or(Ok(()), Err)
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
        // As `new` does: float fields and `Array<…>` members allocated.
        let h = vm.new_object_of(cid);
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
        let Some(vm) = self.vm.as_mut() else {
            return Ok(());
        };
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
        } else {
            // `OnRegister` and every other no-argument handler.
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

    /// Whether `class`'s script overrides UZDoom's player movement
    /// (`PlayerThink` or `HandleMovement` defined below `PlayerPawn`, as
    /// Project Brutality's `PlayerPawnBase` does).
    pub(crate) fn overrides_player_movement(&self, class: &str) -> bool {
        ["PlayerThink", "HandleMovement"].iter().any(|m| {
            self.resolve_method(class, m).is_some_and(|r| {
                r.rsplit_once('.')
                    .is_some_and(|(owner, _)| !owner.eq_ignore_ascii_case("PlayerPawn"))
            })
        })
    }

    /// Run the pawn's script `PlayerThink` (movement, jumping, view height
    /// and pitch; [`PLAYER_THINK_HOST_PARTS`] are Room's). `st` carries the
    /// player's state in and the script's changes out; the pawn's own fields
    /// go through [`World::zscript_sync_in`] / `sync_out` around this.
    pub(crate) fn call_player_think(
        &mut self,
        mo_index: u32,
        class: &str,
        st: &mut PlayerMoveState,
    ) -> Result<(), room_zvm::Trap> {
        let no_vm = || room_zvm::Trap::Native("no ZScript program".into());
        let resolved = self
            .resolve_method(class, "PlayerThink")
            .ok_or_else(|| room_zvm::Trap::Native(format!("{class} has no PlayerThink")))?;
        let self_h = self
            .ensure_actor_object(mo_index, class)
            .ok_or_else(no_vm)?;
        let info_h = self
            .ensure_actor_object(PLAYER_INFO_KEY | (mo_index & 0xffff), "PlayerInfo")
            .ok_or_else(no_vm)?;
        let vm = self.vm.as_mut().ok_or_else(no_vm)?;
        set_typed_object_field(vm, info_h, "mo", self_h);
        set_typed_object_field(vm, info_h, "camera", self_h);
        set_typed_object_field(vm, self_h, "player", info_h);
        set_typed_int_field(vm, info_h, "oldbuttons", i64::from(st.old_buttons));
        set_typed_int_field(vm, info_h, "onground", i64::from(st.on_ground));
        set_typed_int_field(vm, info_h, "jumptics", i64::from(st.jump_tics));
        set_typed_int_field(vm, info_h, "turnticks", i64::from(st.turn_ticks));
        set_typed_int_field(vm, info_h, "centering", i64::from(st.centering));
        set_typed_int_field(vm, info_h, "cheats", i64::from(st.cheats));
        set_typed_int_field(vm, info_h, "playerstate", 0);
        set_typed_int_field(vm, info_h, "health", i64::from(st.health));
        set_walk_float_field(vm, info_h, "viewz", st.view_z);
        set_walk_float_field(vm, info_h, "viewheight", st.view_height);
        set_walk_float_field(vm, info_h, "deltaviewheight", st.delta_view_height);
        set_walk_float_field(vm, info_h, "crouchfactor", st.crouch_factor);
        set_walk_float_field(vm, info_h, "crouchviewdelta", st.crouch_view_delta);
        set_walk_float_field(vm, info_h, "minpitch", -90.0);
        set_walk_float_field(vm, info_h, "maxpitch", 90.0);
        set_walk_float_field(vm, info_h, "desiredfov", 90.0);
        set_walk_float_field(vm, info_h, "fov", 90.0);
        set_typed_int_field(vm, self_h, "waterlevel", i64::from(st.water_level));
        set_walk_float_field(vm, self_h, "viewheight", st.pawn_view_height);
        set_walk_float_field(vm, self_h, "viewbob", st.view_bob);
        if let Some((1, index)) = typed_field(vm, info_h, "cmd") {
            let cmd = match vm.heap.get(info_h) {
                Some(ObjKind::Object { fields_a, .. }) => {
                    fields_a.get(index).copied().unwrap_or(ObjHandle::NULL)
                }
                _ => ObjHandle::NULL,
            };
            let cmd = if vm.heap.get(cmd).is_some() {
                cmd
            } else {
                let layout = vm
                    .program
                    .classes
                    .iter()
                    .enumerate()
                    .find(|(_, c)| c.name.eq_ignore_ascii_case("UserCmd"))
                    .map(|(id, c)| (id as u32, c.n_i, c.n_a, c.n_s, c.n_f));
                let (id, ni, na, ns, nf) = layout.unwrap_or((0, 8, 0, 0, 0));
                let h = vm.heap.new_object_f(id, ni.max(8), na, ns, nf);
                set_typed_object_field(vm, info_h, "cmd", h);
                h
            };
            set_typed_int_field(vm, cmd, "buttons", i64::from(st.buttons));
            set_typed_int_field(vm, cmd, "forwardmove", i64::from(st.forward_move));
            set_typed_int_field(vm, cmd, "sidemove", i64::from(st.side_move));
            set_typed_int_field(vm, cmd, "upmove", i64::from(st.up_move));
            set_typed_int_field(vm, cmd, "yaw", i64::from(st.yaw));
            set_typed_int_field(vm, cmd, "pitch", i64::from(st.pitch));
        }
        let result = vm.call_method_self(&resolved, self_h).map(|_| ());
        let vm = self.vm.as_mut().ok_or_else(no_vm)?;
        let int = |vm: &Zvm, h: ObjHandle, f: &str| -> Option<i64> {
            let (0, k) = walk_field(vm, h, f)? else {
                return None;
            };
            match vm.heap.get(h)? {
                ObjKind::Object { fields_i, .. } => fields_i.get(k).copied(),
                _ => None,
            }
        };
        let float = |vm: &Zvm, h: ObjHandle, f: &str| -> Option<f64> {
            let (3, k) = walk_field(vm, h, f)? else {
                return None;
            };
            match vm.heap.get(h)? {
                ObjKind::Object { fields_f, .. } => fields_f.get(k).copied(),
                _ => None,
            }
        };
        if let Some(v) = float(vm, info_h, "viewz") {
            st.view_z = v;
        }
        if let Some(v) = float(vm, info_h, "viewheight") {
            st.view_height = v;
        }
        if let Some(v) = float(vm, info_h, "deltaviewheight") {
            st.delta_view_height = v;
        }
        if let Some(v) = int(vm, info_h, "onground") {
            st.on_ground = v != 0;
        }
        if let Some(v) = int(vm, info_h, "jumptics") {
            st.jump_tics = i32::try_from(v).unwrap_or(0);
        }
        if let Some(v) = int(vm, info_h, "turnticks") {
            st.turn_ticks = i32::try_from(v).unwrap_or(0);
        }
        if let Some(v) = int(vm, info_h, "centering") {
            st.centering = v != 0;
        }
        result
    }

    /// The script object of `weapon_class` owned by actor `owner_id` (one
    /// per owner and weapon class, kept with the actor objects so it is
    /// saved and its fields last between calls).
    fn ensure_weapon_object(&mut self, owner_id: u32, weapon_class: &str) -> Option<ObjHandle> {
        let vm = self.vm.as_ref()?;
        let cid = vm
            .program
            .class_by_name
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(weapon_class))
            .map(|(_, &id)| id)?;
        let key = WEAPON_OBJECT_KEY | ((owner_id & 0xffff) << 14) | (cid & 0x3fff);
        self.ensure_actor_object(key, weapon_class)
    }

    /// Run a weapon's `action` function from its psprite state (UZDoom
    /// `CallAction`): `self` is the owner's script object, `invoker` the
    /// weapon's. `args` are the state's argument text; missing ones take the
    /// declared defaults. Returns its integer result and the label of the
    /// state it returned, if any.
    pub(crate) fn call_weapon_action(
        &mut self,
        owner_id: u32,
        owner_class: &str,
        weapon: &WeaponCallInfo,
        method: &str,
        args: &[String],
    ) -> Result<(i64, Option<String>), room_zvm::Trap> {
        let resolved = self.resolve_method(&weapon.class, method).ok_or_else(|| {
            room_zvm::Trap::Native(format!("unknown function {}.{method}", weapon.class))
        })?;
        let no_vm = || room_zvm::Trap::Native("no ZScript program".into());
        let self_h = self
            .ensure_actor_object(owner_id, owner_class)
            .ok_or_else(no_vm)?;
        let inv_h = self
            .ensure_weapon_object(owner_id, &weapon.class)
            .ok_or_else(no_vm)?;
        let info_h = self
            .ensure_actor_object(PLAYER_INFO_KEY | (owner_id & 0xffff), "PlayerInfo")
            .ok_or_else(no_vm)?;
        let vm = self.vm.as_mut().ok_or_else(no_vm)?;
        // `player` (`player.cmd.buttons`, `player.ReadyWeapon`) for the owner.
        set_typed_object_field(vm, info_h, "mo", self_h);
        set_typed_object_field(vm, info_h, "readyweapon", inv_h);
        set_typed_int_field(vm, info_h, "oldbuttons", i64::from(weapon.old_buttons));
        set_walk_float_field(vm, info_h, "viewz", weapon.view_z);
        set_walk_float_field(vm, info_h, "crouchfactor", weapon.crouch_factor);
        set_walk_float_field(vm, info_h, "desiredfov", 90.0);
        set_walk_float_field(vm, info_h, "fov", 90.0);
        if let Some((1, index)) = typed_field(vm, info_h, "cmd") {
            let cmd = match vm.heap.get(info_h) {
                Some(ObjKind::Object { fields_a, .. }) => {
                    fields_a.get(index).copied().unwrap_or(ObjHandle::NULL)
                }
                _ => ObjHandle::NULL,
            };
            let cmd = if vm.heap.get(cmd).is_some() {
                cmd
            } else {
                let layout = vm
                    .program
                    .classes
                    .iter()
                    .enumerate()
                    .find(|(_, c)| c.name.eq_ignore_ascii_case("UserCmd"))
                    .map(|(id, c)| (id as u32, c.n_i, c.n_a, c.n_s, c.n_f));
                let (id, ni, na, ns, nf) = layout.unwrap_or((0, 8, 0, 0, 0));
                let h = vm.heap.new_object_f(id, ni.max(8), na, ns, nf);
                set_typed_object_field(vm, info_h, "cmd", h);
                h
            };
            set_typed_int_field(vm, cmd, "buttons", i64::from(weapon.buttons));
        }
        set_typed_object_field(vm, self_h, "player", info_h);
        set_typed_object_field(vm, inv_h, "owner", self_h);
        set_typed_string_field(vm, inv_h, "ammotype1", &weapon.ammo_type1);
        set_typed_string_field(vm, inv_h, "ammotype2", &weapon.ammo_type2);
        set_typed_int_field(vm, inv_h, "ammouse1", i64::from(weapon.ammo_use1));
        set_typed_int_field(vm, inv_h, "ammouse2", i64::from(weapon.ammo_use2));
        let (params, defaults) = {
            let program = &vm.program;
            let params = program
                .func(&resolved)
                .map(|f| f.params.iter().skip(1).map(|p| p.bank).collect::<Vec<_>>())
                .unwrap_or_default();
            let defaults = program
                .action_defaults
                .get(&resolved)
                .cloned()
                .unwrap_or_default();
            (params, defaults)
        };
        // Defaults line up with the parameters unless one is a vector.
        let defaults_fit = defaults.len() == params.len();
        let (mut ints, mut floats, mut strings) = (Vec::new(), Vec::new(), Vec::new());
        let mut objects = Vec::new();
        for (k, bank) in params.iter().enumerate() {
            let text = args
                .get(k)
                .map(String::as_str)
                .filter(|t| !t.trim().is_empty())
                .or_else(|| defaults_fit.then(|| defaults[k].as_str()))
                .unwrap_or("")
                .trim();
            match bank {
                1 => floats.push(arg_float(text)),
                2 => strings.push(arg_string(text)),
                3 => objects.push(ObjHandle::NULL),
                _ => ints.push(arg_int(text)),
            }
        }
        let string_refs: Vec<&str> = strings.iter().map(String::as_str).collect();
        vm.play.invoker = inv_h;
        let result =
            vm.call_method_banks(&resolved, self_h, &ints, &floats, &string_refs, &objects);
        vm.play.invoker = ObjHandle::NULL;
        let value = result?;
        let returned = vm.last_a;
        Ok((value, room_zvm::state_token_label(&vm.heap, returned)))
    }

    /// Run `method` (already resolved) on actor `actor_id`'s script object
    /// with another actor's object as its one argument (`SpecialMissileHit`).
    pub(crate) fn call_with_actor(
        &mut self,
        actor_id: u32,
        class: &str,
        resolved: &str,
        other_id: u32,
        other_class: &str,
    ) -> Result<i64, room_zvm::Trap> {
        let no_vm = || room_zvm::Trap::Native("no ZScript program".into());
        let h = self
            .ensure_actor_object(actor_id, class)
            .ok_or_else(no_vm)?;
        let other = self
            .ensure_actor_object(other_id, other_class)
            .ok_or_else(no_vm)?;
        let vm = self.vm.as_mut().ok_or_else(no_vm)?;
        vm.call_method_banks(resolved, h, &[], &[], &[], &[other])
    }

    /// The `Actor.Spawn` calls the last script queued (`actor.spawn` ops),
    /// taken out of the native op queue in order: `(object, untouched copy,
    /// class, position, replace)`.
    pub(crate) fn take_spawn_ops(&mut self) -> Vec<(ObjHandle, ObjHandle, String, [f64; 3], i64)> {
        let Some(vm) = self.vm.as_mut() else {
            return Vec::new();
        };
        if !vm.play.native_ops.iter().any(|op| op.name == "actor.spawn") {
            return Vec::new();
        }
        let (spawns, rest): (Vec<_>, Vec<_>) = std::mem::take(&mut vm.play.native_ops)
            .into_iter()
            .partition(|op| op.name == "actor.spawn");
        vm.play.native_ops = rest;
        spawns
            .into_iter()
            .map(|op| {
                let f = |k: usize| op.f.get(k).copied().unwrap_or(0.0);
                (
                    op.recv,
                    op.objs.first().copied().unwrap_or(ObjHandle::NULL),
                    op.s.first().cloned().unwrap_or_default(),
                    [f(0), f(1), f(2)],
                    op.i.first().copied().unwrap_or(0),
                )
            })
            .collect()
    }

    /// Make `h` the script object of actor `index` (a `Spawn` result).
    pub(crate) fn bind_actor_object(&mut self, index: u32, generation: u32, h: ObjHandle) {
        // A stale object of a freed actor with this index is left to the
        // collector: a script may still hold it.
        self.actor_fields.insert(index, h);
        self.actor_gen.insert(index, generation);
    }

    /// After a spawned actor took the script's writes: give `h` the class's
    /// field defaults and arrays where the script left a slot as it was made
    /// (`initial`, then freed), keeping what the script wrote.
    pub(crate) fn finish_spawned_object(&mut self, h: ObjHandle, initial: ObjHandle) {
        let Some(vm) = self.vm.as_mut() else {
            return;
        };
        let before = vm.heap.get(initial).cloned();
        let now = vm.heap.get(h).cloned();
        vm.heap.destroy(initial);
        let (
            Some(ObjKind::Object {
                fields_i: bi,
                fields_a: ba,
                fields_s: bs,
                fields_f: bf,
                ..
            }),
            Some(ObjKind::Object {
                class_id,
                fields_i: ni,
                fields_a: na,
                fields_s: ns,
                fields_f: nf,
            }),
        ) = (before, now)
        else {
            return;
        };
        let class = vm
            .program
            .classes
            .get(class_id as usize)
            .map(|c| c.name.clone())
            .unwrap_or_default();
        apply_field_defaults(vm, h, &class);
        // Arrays as `new` makes them.
        let layout = vm.program.classes.get(class_id as usize).cloned();
        let mut arrays = Vec::new();
        if let Some(l) = layout {
            for &k in &l.array_i {
                arrays.push((k, vm.heap.new_array_i()));
            }
            for &k in &l.array_s {
                arrays.push((k, vm.heap.new_array_s()));
            }
        }
        if let Some(ObjKind::Object {
            fields_i,
            fields_a,
            fields_s,
            fields_f,
            ..
        }) = vm.heap.get_mut(h)
        {
            for (k, arr) in arrays {
                let k = usize::from(k);
                if na.get(k).is_none_or(|v| v.is_null()) {
                    if fields_a.len() <= k {
                        fields_a.resize(k + 1, ObjHandle::NULL);
                    }
                    fields_a[k] = arr;
                }
            }
            // The script's own writes win over the defaults.
            for (k, v) in ni.iter().enumerate() {
                if bi.get(k) != Some(v) {
                    if fields_i.len() <= k {
                        fields_i.resize(k + 1, 0);
                    }
                    fields_i[k] = *v;
                }
            }
            for (k, v) in na.iter().enumerate() {
                if ba.get(k) != Some(v) {
                    if fields_a.len() <= k {
                        fields_a.resize(k + 1, ObjHandle::NULL);
                    }
                    fields_a[k] = *v;
                }
            }
            for (k, v) in ns.iter().enumerate() {
                if bs.get(k) != Some(v) {
                    if let Some(slot) = fields_s.get_mut(k) {
                        *slot = *v;
                    }
                }
            }
            for (k, v) in nf.iter().enumerate() {
                if bf.get(k).map(|b| b.to_bits()) != Some(v.to_bits()) {
                    if fields_f.len() <= k {
                        fields_f.resize(k + 1, 0.0);
                    }
                    fields_f[k] = *v;
                }
            }
        }
    }

    /// Free a `Spawn` object whose actor could not be made.
    pub(crate) fn drop_spawned_object(&mut self, h: ObjHandle, initial: ObjHandle) {
        if let Some(vm) = self.vm.as_mut() {
            vm.heap.destroy(initial);
            vm.heap.destroy(h);
        }
    }

    /// Actor index of the script object `h`, if it is an actor's.
    pub(crate) fn actor_index_of(&self, h: ObjHandle) -> Option<u32> {
        if h.is_null() {
            return None;
        }
        self.actor_fields
            .iter()
            .find(|(k, v)| **v == h && **k < PLAYER_INFO_KEY)
            .map(|(k, _)| *k)
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
        if let Some(&h) = self.actor_fields.get(&actor_id) {
            return Some(h);
        }
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
        let h = vm
            .heap
            .new_object_f(cid, layout.n_i, layout.n_a, layout.n_s, layout.n_f);
        apply_field_defaults(vm, h, &layout.name);
        self.actor_fields.insert(actor_id, h);
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
                    self.ensure_actor_object(*index, class)
                        .unwrap_or(ObjHandle::NULL)
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
            script_rng: vm.play.rng,
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
        if blob.script_rng != 0 {
            vm.play.rng = blob.script_rng;
        }
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
    let layout = |name: &str| {
        program
            .classes
            .iter()
            .find(|c| c.name.eq_ignore_ascii_case(name))
    };
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
    match layout.fields.get(&name.to_ascii_lowercase()).copied() {
        Some((bank, index)) => Some((bank, usize::from(index))),
        // Inherited (`Actor.player` on a player class).
        None => walk_field(vm, object, name),
    }
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

/// The `Default` values of `class` and its ancestors (ancestors first) on a
/// new object: custom properties and flagdefs ([`CompiledProgram::field_defaults`]).
fn apply_field_defaults(vm: &mut Zvm, h: ObjHandle, class: &str) {
    if vm.program.field_defaults.is_empty() {
        return;
    }
    let mut chain = Vec::new();
    let mut cur = class.to_owned();
    for _ in 0..64 {
        if cur.is_empty() {
            break;
        }
        let Some(layout) = vm
            .program
            .classes
            .iter()
            .find(|c| c.name.eq_ignore_ascii_case(&cur))
        else {
            break;
        };
        chain.push(layout.name.clone());
        cur = layout.parent.clone();
    }
    let values: Vec<(String, String)> = chain
        .iter()
        .rev()
        .filter_map(|c| {
            vm.program
                .field_defaults
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case(c))
                .map(|(_, v)| v.clone())
        })
        .flatten()
        .collect();
    for (field, text) in values {
        match walk_field(vm, h, &field) {
            Some((0, _)) => set_typed_int_field(vm, h, &field, arg_int(&text)),
            Some((2, _)) => set_typed_string_field(vm, h, &field, &arg_string(&text)),
            Some((3, _)) => set_walk_float_field(vm, h, &field, arg_float(&text)),
            _ => {}
        }
    }
}

/// Set float field `name` (inherited ones too); missing fields are ignored.
fn set_walk_float_field(vm: &mut Zvm, object: ObjHandle, name: &str, value: f64) {
    let Some((3, index)) = walk_field(vm, object, name) else {
        return;
    };
    if let Some(ObjKind::Object { fields_f, .. }) = vm.heap.get_mut(object) {
        if fields_f.len() <= index {
            fields_f.resize(index + 1, 0.0);
        }
        fields_f[index] = value;
    }
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

/// What [`ZscriptRuntime::sync_map_objects`] reads from the playsim.
pub(crate) struct MapSync<'a> {
    pub map: &'a room_map::MapGeometry,
    /// `[floor, ceiling]` plane of each sector.
    pub planes: Vec<[room_zvm::ZsPlane; 2]>,
    /// Light level of each sector.
    pub light: Vec<i32>,
    /// `[floor, ceiling]` flat of each sector.
    pub flats: Vec<[String; 2]>,
    pub tic: u32,
}

fn set_obj_int(vm: &mut Zvm, h: ObjHandle, name: &str, v: i64) {
    if let Some((0, slot)) = walk_field(vm, h, name)
        && let Some(ObjKind::Object { fields_i, .. }) = vm.heap.get_mut(h)
        && let Some(x) = fields_i.get_mut(slot)
    {
        *x = v;
    }
}

fn set_obj_ref(vm: &mut Zvm, h: ObjHandle, name: &str, v: ObjHandle) {
    if let Some((1, slot)) = walk_field(vm, h, name)
        && let Some(ObjKind::Object { fields_a, .. }) = vm.heap.get_mut(h)
        && let Some(x) = fields_a.get_mut(slot)
    {
        *x = v;
    }
}

fn set_obj_floats(vm: &mut Zvm, h: ObjHandle, name: &str, v: &[f64]) {
    if let Some((3, slot)) = walk_field(vm, h, name)
        && let Some(ObjKind::Object { fields_f, .. }) = vm.heap.get_mut(h)
    {
        for (i, x) in v.iter().enumerate() {
            if let Some(f) = fields_f.get_mut(slot + i) {
                *f = *x;
            }
        }
    }
}

/// The array object in field `name` of `h`.
fn obj_array(vm: &Zvm, h: ObjHandle, name: &str) -> Option<ObjHandle> {
    let (1, slot) = walk_field(vm, h, name)? else {
        return None;
    };
    match vm.heap.get(h)? {
        ObjKind::Object { fields_a, .. } => fields_a.get(slot).copied(),
        _ => None,
    }
}

fn fill_obj_array(vm: &mut Zvm, h: ObjHandle, name: &str, items: &[ObjHandle]) {
    if let Some(arr) = obj_array(vm, h, name)
        && let Some(ObjKind::ArrayA(v)) = vm.heap.get_mut(arr)
    {
        v.clear();
        v.extend_from_slice(items);
    }
}

fn fill_int_array(vm: &mut Zvm, h: ObjHandle, name: &str, items: &[i64]) {
    if let Some(arr) = obj_array(vm, h, name)
        && let Some(ObjKind::ArrayI(v)) = vm.heap.get_mut(arr)
    {
        v.clear();
        v.extend_from_slice(items);
    }
}

/// UZDoom `secplane_t` fields for `z = z0 + gx·x + gy·y`: `Normal`, `D`,
/// `negiC` (so `ZatPoint = (D + N·p)·negiC`).
fn plane_fields(p: room_zvm::ZsPlane, ceiling: bool) -> ([f64; 3], f64, f64) {
    let len = (p.gx * p.gx + p.gy * p.gy + 1.0).sqrt();
    if ceiling {
        ([p.gx / len, p.gy / len, -1.0 / len], p.z0 / len, len)
    } else {
        ([-p.gx / len, -p.gy / len, 1.0 / len], -p.z0 / len, -len)
    }
}

/// The level's map objects in the VM heap, from the map's static data.
fn build_map_objects(vm: &mut Zvm, map: &room_map::MapGeometry) -> room_zvm::ZsMapObjects {
    use room_zvm::ZsMapKind;
    let mut out = room_zvm::ZsMapObjects::default();
    let class = |vm: &Zvm, n: &str| {
        vm.program
            .classes
            .iter()
            .position(|c| c.name.eq_ignore_ascii_case(n))
            .map(|i| i as u32)
    };
    let (Some(c_sector), Some(c_line), Some(c_side), Some(c_vertex)) = (
        class(vm, "Sector"),
        class(vm, "Line"),
        class(vm, "Side"),
        class(vm, "Vertex"),
    ) else {
        return out;
    };
    let c_plane = class(vm, "SecPlane");
    for (i, v) in map.vertices.iter().enumerate() {
        let h = vm.new_object_of(c_vertex);
        set_obj_floats(vm, h, "p", &[v.pos.x(), v.pos.y()]);
        out.index.insert(h, (ZsMapKind::Vertex, i as u32));
        out.vertices.push(h);
    }
    for (i, sec) in map.sectors.iter().enumerate() {
        let h = vm.new_object_of(c_sector);
        set_obj_int(vm, h, "sectornum", i as i64);
        set_obj_int(vm, h, "special", i64::from(sec.special));
        set_obj_int(vm, h, "lightlevel", i64::from(sec.light));
        set_obj_floats(vm, h, "friction", &[0.90625]);
        set_obj_floats(vm, h, "movefactor", &[0.03125]);
        set_obj_floats(vm, h, "gravity", &[1.0]);
        // `centerspot`: middle of the sector's bounding box.
        let mut bb = [f64::MAX, f64::MAX, f64::MIN, f64::MIN];
        for &lid in &sec.lines {
            if let Some(l) = map.lines.get(lid.raw() as usize) {
                for vid in [l.v1, l.v2] {
                    if let Some(v) = map.vertices.get(vid.raw() as usize) {
                        bb[0] = bb[0].min(v.pos.x());
                        bb[1] = bb[1].min(v.pos.y());
                        bb[2] = bb[2].max(v.pos.x());
                        bb[3] = bb[3].max(v.pos.y());
                    }
                }
            }
        }
        if bb[0] <= bb[2] {
            set_obj_floats(
                vm,
                h,
                "centerspot",
                &[(bb[0] + bb[2]) * 0.5, (bb[1] + bb[3]) * 0.5],
            );
        }
        let mut planes = [ObjHandle::NULL; 2];
        for (p, field) in [(0u8, "floorplane"), (1u8, "ceilingplane")] {
            if let Some(ph) = c_plane.map(|c| vm.new_object_of(c)) {
                set_obj_ref(vm, h, field, ph);
                out.index.insert(ph, (ZsMapKind::Plane(p), i as u32));
                planes[usize::from(p)] = ph;
            }
        }
        out.index.insert(h, (ZsMapKind::Sector, i as u32));
        out.sectors.push(h);
        let _ = planes;
    }
    for (i, side) in map.sides.iter().enumerate() {
        let h = vm.new_object_of(c_side);
        if let Some(&s) = out.sectors.get(side.sector.raw() as usize) {
            set_obj_ref(vm, h, "sector", s);
        }
        out.index.insert(h, (ZsMapKind::Side, i as u32));
        out.sides.push(h);
    }
    let portal_of: BTreeMap<u32, usize> = map
        .setup
        .portals
        .lines
        .iter()
        .enumerate()
        .map(|(k, p)| (p.line.raw(), k))
        .collect();
    for (i, line) in map.lines.iter().enumerate() {
        let h = vm.new_object_of(c_line);
        out.index.insert(h, (ZsMapKind::Line, i as u32));
        out.lines.push(h);
        if let Some(&v) = out.vertices.get(line.v1.raw() as usize) {
            set_obj_ref(vm, h, "v1", v);
        }
        if let Some(&v) = out.vertices.get(line.v2.raw() as usize) {
            set_obj_ref(vm, h, "v2", v);
        }
        set_obj_floats(vm, h, "delta", &[line.delta.x(), line.delta.y()]);
        set_obj_int(vm, h, "flags", i64::from(line.flags as u16));
        set_obj_int(vm, h, "activation", i64::from(line.activation));
        set_obj_int(vm, h, "special", i64::from(line.special));
        set_obj_int(vm, h, "locknumber", i64::from(line.lock_number));
        set_obj_floats(vm, h, "alpha", &[f64::from(line.mid_alpha)]);
        let portal = portal_of
            .get(&(i as u32))
            .map_or(i64::from(u32::MAX), |&k| k as i64);
        set_obj_int(vm, h, "portalindex", portal);
        fill_int_array(vm, h, "args", &line.args.map(i64::from));
        if let Some(s) = line
            .front_sector
            .and_then(|s| out.sectors.get(s.raw() as usize))
        {
            set_obj_ref(vm, h, "frontsector", *s);
        }
        if let Some(s) = line
            .back_sector
            .and_then(|s| out.sectors.get(s.raw() as usize))
        {
            set_obj_ref(vm, h, "backsector", *s);
        }
        let sides: Vec<ObjHandle> = line
            .sides
            .iter()
            .map(|s| {
                s.and_then(|s| out.sides.get(s.raw() as usize).copied())
                    .unwrap_or(ObjHandle::NULL)
            })
            .collect();
        fill_obj_array(vm, h, "sidedef", &sides);
        for s in line.sides.iter().flatten() {
            if let Some(&sh) = out.sides.get(s.raw() as usize) {
                set_obj_ref(vm, sh, "linedef", h);
            }
        }
    }
    for (i, sec) in map.sectors.iter().enumerate() {
        let lines: Vec<ObjHandle> = sec
            .lines
            .iter()
            .filter_map(|l| out.lines.get(l.raw() as usize).copied())
            .collect();
        let h = out.sectors[i];
        fill_obj_array(vm, h, "lines", &lines);
    }
    if let Some(level) = vm.new_object("LevelLocals") {
        let (s, l, d, v) = (
            out.sectors.clone(),
            out.lines.clone(),
            out.sides.clone(),
            out.vertices.clone(),
        );
        fill_obj_array(vm, level, "sectors", &s);
        fill_obj_array(vm, level, "lines", &l);
        fill_obj_array(vm, level, "sides", &d);
        fill_obj_array(vm, level, "vertexes", &v);
        out.level = level;
    }
    out.planes = vec![[room_zvm::ZsPlane::default(); 2]; map.sectors.len()];
    out.light = map.sectors.iter().map(|s| i32::from(s.light)).collect();
    out.flats = map
        .sectors
        .iter()
        .map(|s| [s.floor_pic.clone(), s.ceiling_pic.clone()])
        .collect();
    out
}

/// Plane heights, light and flats of the map objects, from the playsim.
fn refresh_map_objects(vm: &mut Zvm, m: &MapSync<'_>) {
    let n = vm.play.map_objects.sectors.len();
    for i in 0..n {
        let h = vm.play.map_objects.sectors[i];
        let light = m.light.get(i).copied().unwrap_or(0);
        set_obj_int(vm, h, "lightlevel", i64::from(light));
        let planes = m.planes.get(i).copied().unwrap_or_default();
        for (p, field) in [(0usize, "floorplane"), (1usize, "ceilingplane")] {
            let Some(ph) = obj_ref(vm, h, field) else {
                continue;
            };
            let (normal, d, negic) = plane_fields(planes[p], p == 1);
            set_obj_floats(vm, ph, "normal", &normal);
            set_obj_floats(vm, ph, "d", &[d]);
            set_obj_floats(vm, ph, "negic", &[negic]);
        }
    }
    let mo = &mut vm.play.map_objects;
    mo.planes.clone_from(&m.planes);
    mo.light.clone_from(&m.light);
    mo.flats.clone_from(&m.flats);
}

fn obj_ref(vm: &Zvm, h: ObjHandle, name: &str) -> Option<ObjHandle> {
    let (1, slot) = walk_field(vm, h, name)? else {
        return None;
    };
    match vm.heap.get(h)? {
        ObjKind::Object { fields_a, .. } => fields_a.get(slot).copied().filter(|x| !x.is_null()),
        _ => None,
    }
}

/// The level's portals as the ZScript natives see them.
fn portal_info(
    map: &room_map::MapGeometry,
    lookup_cache: &mut Option<([u8; 16], room_zvm::SectorLookup)>,
) -> room_zvm::ZsPortalInfo {
    use room_map::portals::{
        PLANEF_BLOCKSOUND, PLANEF_DISABLED, PLANEF_NOPASS, PLANEF_NORENDER, PLANEF_OBSTRUCTED,
        PORTF_INTERACTIVE, PORTS_LINKEDPORTAL,
    };
    let t = &map.setup.portals;
    let g = &map.setup.portal_groups;
    let any = g.has_links() || !t.lines.is_empty() || t.sectors.len() > 2;
    let lookup = match lookup_cache {
        Some((md5, l)) if *md5 == map.md5 => l.clone(),
        _ => {
            let snap = Arc::new(map.clone());
            let l = room_zvm::SectorLookup(Arc::new(move |x: f64, y: f64| {
                room_nodebuild::point_in_sector(&snap, x, y)
                    .or_else(|| room_nodebuild::point_in_sector_vanilla(&snap, x, y))
                    .map(|s| s.raw() as usize)
            }));
            *lookup_cache = Some((map.md5, l.clone()));
            l
        }
    };
    // No portals: only the point lookup (`PointInSector`, `CurSector`).
    if !any {
        return room_zvm::ZsPortalInfo {
            group_count: g.size.max(1),
            lookup: Some(lookup),
            ..room_zvm::ZsPortalInfo::default()
        };
    }
    let planes = (0..map.sectors.len())
        .map(|s| {
            [0usize, 1].map(|plane| {
                let p = t.plane_portal(s, plane);
                let flags = t.flags(s, plane);
                let linked = t.plane_is_linked(s, plane);
                let blocked_static = |mask: u16| !linked || flags & mask != 0;
                room_zvm::ZsPlanePortal {
                    kind: p.map_or(0, |p| i32::from(p.kind)),
                    plane_z: t.plane_z(s, plane),
                    displacement: t.plane_displacement(s, plane),
                    opposite_group: p
                        .filter(|p| p.kind == PORTS_LINKEDPORTAL)
                        .and_then(|p| p.destination)
                        .and_then(|d| g.group_of(d.raw() as usize))
                        .map_or(0, |v| i32::try_from(v).unwrap_or(0)),
                    linked,
                    blocks: [
                        // `PortalBlocksView` is only ever true on linked planes.
                        linked
                            && flags & (PLANEF_NORENDER | PLANEF_DISABLED | PLANEF_OBSTRUCTED) != 0,
                        blocked_static(
                            PLANEF_NORENDER | PLANEF_NOPASS | PLANEF_DISABLED | PLANEF_OBSTRUCTED,
                        ),
                        blocked_static(PLANEF_NOPASS | PLANEF_DISABLED | PLANEF_OBSTRUCTED),
                        blocked_static(PLANEF_BLOCKSOUND | PLANEF_DISABLED | PLANEF_OBSTRUCTED),
                    ],
                }
            })
        })
        .collect();
    let vpos = |v: room_core::VertexId| {
        map.vertices
            .get(v.raw() as usize)
            .map_or((0.0, 0.0), |v| (v.pos.x(), v.pos.y()))
    };
    let lines = t
        .lines
        .iter()
        .filter_map(|p| {
            let target = p.target?;
            let src = map.lines.get(p.line.raw() as usize)?;
            let dst = map.lines.get(target.raw() as usize)?;
            Some(room_zvm::ZsLinePortal {
                v1: vpos(src.v1),
                v2: vpos(src.v2),
                dest_v2: vpos(dst.v2),
                displacement: p.displacement,
                sin_rot: p.sin_rot,
                cos_rot: p.cos_rot,
                linked: p.is_linked(),
                interactive: p.flags & PORTF_INTERACTIVE != 0,
            })
        })
        .collect();
    let line_info = (0..map.lines.len())
        .map(|i| {
            t.line(room_core::LineId::from_raw(i as u32))
                .map(|p| room_zvm::ZsLineInfo {
                    destination: p.target.map(|l| l.raw()),
                    flags: p.flags,
                    align: p.align,
                    visible: p.flags & room_map::portals::PORTF_VISIBLE != 0,
                })
        })
        .collect();
    room_zvm::ZsPortalInfo {
        group_count: g.size.max(1),
        disp: g.disp.clone(),
        sector_group: g.sector_group.clone(),
        planes,
        lines,
        line_info,
        lookup: Some(lookup),
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::field_reassign_with_default)]

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
