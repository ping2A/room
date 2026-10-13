# 12 — WebAssembly modules: the ZScript replacement

Crates: `ferrite-script-abi` (shared, `no_std`), `ferrite-script` (host),
`ferrite-script-sdk` + `ferrite-script-sdk-macros` (guest, Rust),
`ferrite-abi-gen` (tool: generates bindings for other languages).

## 1. Goals and constraints

| Goal | Consequence |
|---|---|
| Any language that compiles to WASM | Plain **core WebAssembly** modules with a C-like ABI. No WASI. Component Model not required (see §13 ADR). |
| Same power as ZScript for gameplay | Classes with inheritance from built-in classes, virtual overrides with `super` calls, custom fields, states, action functions, event handlers, HUD/status bars, menus, CVars, network events. |
| Deterministic play logic (netplay, demos, saves) | Play-scope modules run inside the tic with a deterministic WASM configuration and no non-deterministic imports. |
| Same code on desktop and web | Backends: wasmtime (native), wasmi (everywhere, interpreter), browser-native WebAssembly (web, S). Identical semantics. |
| Safe | Sandboxed: memory caps, call-depth caps, instruction budget, no host access beyond the API. |
| Save games | Module state = host-side declared fields + a snapshot of linear memory and globals. |
| Fast | ≤ 150 ns per host↔guest call on wasmtime; batch APIs for hot paths. |

## 2. Packaging

A mod declares modules with a manifest at the root of its archive:

```toml
# ferrite.toml
[mod]
id = "org.example.betterimps"        # reverse-DNS, unique
name = "Better Imps"
version = "1.4.0"
abi = 1                               # required ABI major version
authors = ["..."]

[[module]]
file = "modules/play.wasm"
scope = "play"                        # play | ui
name = "betterimps"                   # namespace for exported names (actions, events, cvars)

[[module]]
file = "modules/hud.wasm"
scope = "ui"
name = "betterimps_hud"

[dependencies]                        # optional, other mod ids with semver ranges
"org.example.corelib" = "^2"

[cvars]                               # optional: declarative CVars (same semantics as CVARINFO)
"betterimps_fireball_speed" = { type = "float", default = 12.0, scope = "server" }

[events]                              # handlers registered globally (like GAMEINFO AddEventHandlers)
handlers = ["betterimps::ImpTracker"]
```

- Modules are loaded in VFS order of their containers; dependencies must already be loaded (error otherwise — no reordering).
- A module's identity for saves and netplay = `(mod.id, module.name, sha256(wasm bytes))`.
- Maps can opt into map-scoped handlers through MAPINFO `EventHandlers = "betterimps::ArenaRules"`.

## 3. Scopes

Mirrors ZScript's `play` / `ui` / `clearscope` separation, enforced at link time.

| | `play` module | `ui` module |
|---|---|---|
| Runs | inside `World::tick`, during definition build (registration), during spawns/damage/etc. callbacks | in the frame loop: UI tick (35 Hz), render (per frame), input |
| Can mutate world | yes | **no** — read-only queries on `SimView` |
| Determinism rules | strict (§8) | none (may read client CVars, real time, etc.) |
| Instance lifetime | one instance per *game session* (new game / load game re-creates or restores it) | one instance per engine run |
| Saved | yes (§10) | no (persist via CVars) |
| Imports available | `common` + `play` | `common` + `ui` |
| Talks to the other scope | through world state (ui reads it) | `send_network_event` → arrives in play as `NetworkProcess` event on the next tic, on all peers |

A module importing a function outside its scope is rejected at load with a
message listing the offending imports.

## 4. Runtime backends (`ferrite-script`)

```rust
pub trait WasmBackend {
    type Instance: WasmInstance;
    fn compile(&self, bytes: &[u8], cfg: &ModuleConfig) -> Result<CompiledModule>;
    fn instantiate(&self, m: &CompiledModule, linker: &HostLinker) -> Result<Self::Instance>;
}
pub trait WasmInstance {
    fn call(&mut self, f: ExportId, args: &[Val], results: &mut [Val], budget: &mut Budget) -> Result<(), Trap>;
    fn memory(&self) -> &[u8]; fn memory_mut(&mut self) -> &mut [u8];
    fn snapshot(&self) -> InstanceSnapshot;                // memory + mutable globals
    fn restore(&mut self, s: &InstanceSnapshot) -> Result<()>;
}
```

| Backend | Feature | Platforms | Notes |
|---|---|---|---|
| `wasmtime` | `wasmtime` | native | Cranelift; config: `cranelift_nan_canonicalization(true)`, `wasm_threads(false)`, `wasm_relaxed_simd(false)`, `wasm_memory64(false)`, `consume_fuel(true)`, `epoch_interruption(false)`; ahead-of-time compile cache on disk keyed by sha256 + wasmtime version |
| `wasmi` | `wasmi` | all (incl. wasm32) | interpreter; deterministic by construction; fuel metering; reference backend for tests |
| `web` | `web-native` (S) | wasm32 + browser | compile with `WebAssembly.Module`, host imports provided as JS functions forwarding into the engine's exports; memory accessed via `Uint8Array` views. NaN canonicalization is not configurable → validate that play modules pass the deterministic-float lint (§8) or fall back to `wasmi` in netplay |

All backends must pass the same conformance suite (`ferrite-script/tests/conformance/`).

### 4.1 Validation (all backends, before compile)

Using `wasmparser`:
- Allowed proposals: MVP, mutable-globals, sign-ext, non-trapping float-to-int, multi-value, bulk-memory, reference-types (no `table.set/grow` on tables containing funcrefs in play modules), SIMD128 (non-relaxed), tail-call, extended-const.
- Rejected: threads/shared memory/atomics, relaxed-simd, memory64, exception-handling (v1), GC (v1), multi-memory (v1).
- Exactly one exported memory named `memory`; max pages ≤ `script_max_memory` (default 4096 pages = 256 MiB play, 1024 ui).
- Imports only from module `"ferrite"` and only functions present in the scope's import set, with exactly matching signatures.
- Required exports present (§5.1).

### 4.2 Budgets and traps

- Each top-level host→guest call gets an instruction budget (fuel). Default 50 M per call, 200 M per tic per module (configurable `script_budget_*`). Exhaustion = trap.
- Nested re-entrant calls share the outer call's budget.
- Max host↔guest nesting depth: 64.
- Trap handling (`script_trap_policy`): `log` (default single-player: disable the offending actor/handler, log stack trace with symbolicated names from the `name` custom section), `pause` (pause game, show error overlay), `abort` (end game). In netplay, a trap is deterministic for deterministic causes (unreachable, OOB, div by zero); budget exhaustion is treated as `abort` for all peers via a desync-safe "script error" net message.

## 5. ABI v1

All values little-endian. Pointers are `u32` offsets into the guest's `memory`.

### 5.1 Required guest exports

```
ferrite_abi_version() -> i32                       // must return 1
ferrite_alloc(size: i32, align: i32) -> i32        // host-requested allocations (string/event buffers)
ferrite_free(ptr: i32, size: i32, align: i32)
ferrite_register() -> i32                          // registration phase (§6). 0 = ok
ferrite_init(session_kind: i32) -> i32             // new instance for a session (play) / engine (ui). session_kind: NewGame | LoadGame | HotReload
ferrite_dispatch(callback_id: i32, payload_ptr: i32, payload_len: i32) -> i64   // generic callback entry (§5.4)
```

Optional exports: `ferrite_on_save(buf_ptr) -> i32` / `ferrite_on_load()` hooks (§10), `ferrite_panic_info() -> i32` (pointer to last panic message).

### 5.2 Handle types

| Type | Encoding | Null |
|---|---|---|
| `ActorRef` | `u64` = `index (u32) | generation (u32) << 32` | `0` |
| `SectorRef`, `LineRef`, `SideRef`, `VertexRef`, `PolyRef`, `PlayerRef` | `u32` index + 1 | `0` |
| `ClassRef`, `StateRef`, `SoundRef`, `TextureRef`, `FontRef`, `NameRef`, `TranslationRef`, `CvarRef` | `u32` registry id + 1 | `0` |
| `StrRef` | `(ptr: u32, len: u32)` UTF-8 in guest memory (arguments) | len 0 |
| `HostStr` | `u32` id of a host string (localized strings, names); materialize with `str_copy(id, buf, cap) -> len` | `0` |

Stale `ActorRef`s resolve to "null" on the host: getters return defaults, setters are no-ops, and `actor_exists(ref)` returns 0. This matches ZScript's null-on-destroy pointers.

### 5.3 Host imports (module `"ferrite"`)

The API is **defined once** in `ferrite-script-abi/api/*.ron` (function name, scope, params, results, doc, ZScript equivalent). `ferrite-abi-gen` generates from it: the host linker table, the Rust guest bindings (`ferrite-script-sdk/src/sys.rs`), `ferrite.h` (C/C++), `ferrite.zig`, AssemblyScript `ferrite.ts`, and `api.json` (for the ZScript transpiler and docs). Naming: `<area>_<verb>[_<noun>]`.

Representative subset (the full v1 list must cover everything in the table of §9):

```
// common (both scopes; read-only)
log(level: i32, msg_ptr: i32, msg_len: i32)
name_intern(ptr, len) -> NameRef              name_text(NameRef, buf, cap) -> i32
class_find(ptr, len) -> ClassRef              class_is_descendant(ClassRef, ClassRef) -> i32
class_replacement(ClassRef) -> ClassRef       class_default_i32/f64(ClassRef, PropId) -> i32/f64
cvar_find(ptr, len) -> CvarRef                cvar_get_i32/f64/bool(CvarRef) -> …   cvar_get_str(CvarRef, buf, cap) -> i32
level_time() -> i32   level_maptime() -> i32  level_mapname(buf, cap) -> i32       level_info_i32(LevelProp) -> i32
actor_exists(ActorRef) -> i32                 actor_class(ActorRef) -> ClassRef
actor_pos(ActorRef, out_ptr /* 3×f64 */)      actor_vel(ActorRef, out_ptr)          actor_angles(ActorRef, out_ptr)
actor_get_i32(ActorRef, PropId) -> i32        actor_get_f64(ActorRef, PropId) -> f64   actor_get_ref(ActorRef, PtrId) -> ActorRef
actor_flag(ActorRef, FlagId) -> i32           actor_field_get_i32/f64/ref(ActorRef, FieldId) -> …
actor_find_inventory(ActorRef, ClassRef, subclass: i32) -> ActorRef   actor_count_inv(ActorRef, ClassRef) -> i32
actor_state(ActorRef) -> StateRef             actor_in_state_sequence(ActorRef, StateRef) -> i32
actor_check_sight(ActorRef, ActorRef, flags) -> i32   actor_distance_2d/3d(ActorRef, ActorRef) -> f64
actor_iter_class(ClassRef, out_ptr, cap) -> i32       actor_iter_tid(tid, out_ptr, cap) -> i32
actor_iter_radius(x, y, z, radius, flags, out_ptr, cap) -> i32       (batch: fills ActorRef array; returns count)
sector_floor_z_at(SectorRef, x, y) -> f64     sector_get_i32/f64(SectorRef, SectorProp) -> …   sector_iter_tag(tag, out, cap) -> i32
point_in_sector(x, y) -> SectorRef            line_get_i32(LineRef, LineProp) -> i32
player_count() -> i32  player_actor(PlayerRef) -> ActorRef  player_get_i32(PlayerRef, PlayerProp) -> i32
player_cmd(PlayerRef, out_ptr)                 // TicCmd struct
trace(params_ptr, out_ptr) -> i32             // LineTrace / FTraceInfo equivalent, results in out struct
str_localize(key_ptr, len, buf, cap) -> i32

// play only (mutating, deterministic)
actor_spawn(ClassRef, x, y, z, flags) -> ActorRef
actor_destroy(ActorRef)
actor_set_i32/f64(ActorRef, PropId, v)        actor_set_ref(ActorRef, PtrId, ActorRef)   actor_set_flag(ActorRef, FlagId, on)
actor_set_pos(ActorRef, x, y, z, flags)       actor_set_vel(ActorRef, x, y, z)   actor_set_angles(ActorRef, yaw, pitch, roll, flags)
actor_try_move(ActorRef, x, y, dropoff) -> i32   actor_teleport(ActorRef, x, y, z, angle, flags) -> i32
actor_set_state(ActorRef, StateRef, nofunction) -> i32   actor_find_state(ActorRef, label_ptr, len, exact) -> StateRef
actor_field_set_i32/f64/ref(ActorRef, FieldId, v)
actor_damage(target, inflictor, source, damage, NameRef dmg_type, flags, angle) -> i32
actor_radius_attack(ActorRef, source, damage, distance, NameRef, flags, full_dist) -> i32
actor_line_attack(ActorRef, angle, distance, pitch, damage, NameRef, ClassRef puff, flags, out_ptr) -> ActorRef
actor_spawn_missile(ActorRef src, ActorRef dest, ClassRef, out ActorRef) -> ActorRef   actor_spawn_missile_angle(...)
actor_give_inventory(ActorRef, ClassRef, amount, flags) -> i32   actor_take_inventory(ActorRef, ClassRef, amount, flags) -> i32
actor_call_action(ActorRef self, ActionRef, args_ptr, args_len) -> i64   // call any registered action function (native, DECORATE-visible, other modules)
actor_call_super(ActorRef, VirtualSlot, args_ptr, args_len, ret_ptr) -> i32   // "Super.X()" for overridden virtuals
actor_start_sound(ActorRef, SoundRef, channel, flags, volume, attenuation, pitch, start_time)
actor_stop_sound(ActorRef, channel)
actor_a_*  (generated thin wrappers for the most used action functions: a_chase(flags…), a_look, a_face_target, a_spawn_item_ex(…), a_custom_missile(…), a_explode(…), …)
sector_set_*(SectorRef, prop, v)              sector_move_floor(SectorRef, params_ptr) -> i32   (mover thinkers)
line_set_*(…)  line_activate(LineRef, ActorRef, side, activation) -> i32
special_execute(special, args_ptr /*5×i32*/, ActorRef activator, LineRef, side) -> i32
acs_execute(script, map, args_ptr, n) -> i32  acs_named_execute(name_ptr, len, …) -> i32  acs_execute_with_result(...) -> i32
acs_get_var(scope, index) -> i32  acs_set_var(scope, index, v)
rng_named(name: NameRef) -> RngRef  rng_u8(RngRef) -> i32  rng_range(RngRef, lo, hi) -> i32  rng_f64(RngRef) -> f64
level_set_i32/f64(LevelProp, v)   level_exit(flags, map_ptr, len)   level_change_music(...)
player_set_*(...)  player_set_weapon(PlayerRef, ClassRef)   player_print(PlayerRef, msg_ptr, len, bold: i32)
thinker_create(statnum, callback_id, data: i64) -> ThinkerRef   thinker_destroy(ThinkerRef)   // non-actor thinkers (ZScript Thinker)
hud_message(params_ptr)                        // ACS-style HUD message as a sim output

// ui only
draw_texture(TextureRef, x: f64, y: f64, params_ptr)      // DrawTexture with DTA_* tags as a struct
draw_text(FontRef, color, x, y, str_ptr, len, params_ptr)
draw_fill(color_rgba, x, y, w, h)  draw_line(...)  draw_shape(verts_ptr, n, tex, params)  draw_clip(x, y, w, h)
screen_size(out_ptr)  hud_scale(out_ptr)  font_find(ptr, len) -> FontRef  font_string_width(FontRef, ptr, len) -> i32
texture_find(ptr, len, usetype) -> TextureRef  texture_size(TextureRef, out_ptr)
sbar_*                                         // BaseStatusBar helpers: draw_image, draw_string (HUDFont), draw_inventory_icon, draw_bar, begin_hud/begin_statusbar, fullscreen offsets…
menu_open(name_ptr, len)  menu_close()  menu_set_item_*(…)
send_network_event(name_ptr, len, a1, a2, a3)  // → play-scope NetworkProcess on all peers
console_command(ptr, len)                      // restricted whitelist (no `exec`, `quit` etc. without user confirmation)
cvar_set_*(CvarRef, v)                         // only for client/user cvars owned by this module or allowlisted
real_time_ms() -> f64                          // ui only (non-deterministic)
sound_play_ui(SoundRef, volume, flags)
```

`PropId`, `FlagId`, `PtrId`, `SectorProp`, `LevelProp`, … are stable `u32` enums defined in the ABI crate (one per DECORATE property / flag / ZScript field). New values may be appended in minor versions; existing values never change.

### 5.4 Callbacks (host → guest)

During registration the guest associates **callback ids** with its own code
(e.g. an action function, a virtual override, an event handler method). At run
time the host calls `ferrite_dispatch(callback_id, payload_ptr, payload_len)`.

- `payload` is a `#[repr(C)]` struct defined in `ferrite-script-abi::payload` (e.g. `ActionPayload { self_: ActorRef, invoker: ActorRef, state: StateRef, caller_kind: u32, args: [ArgVal; 8] }`, `DamagePayload { this, inflictor, source, damage, dmg_type: NameRef, flags, angle: f64 }`, `WorldEventPayload { … }`).
- The host writes the payload into a per-instance scratch region obtained once from `ferrite_alloc` (grown as needed; re-entrant calls use a stack of scratch frames).
- The `i64` return value carries the result (e.g. damage amount, state jump target as `StateRef`, bool).

Callbacks never receive host pointers; only handles and plain data.

## 6. Registration phase (class model)

When `Definitions` are built (pipeline phase 7, `05-text-lumps.md §2`), each play
module is instantiated in **registration mode** (imports restricted to the
`reg_*` set + `common` lookups of already-registered content) and
`ferrite_register()` is called. The instance is then dropped; registration has
no runtime state. UI modules register their UI elements the same way.

```
reg_class_begin(name_ptr, len, parent: ClassRef|name, replaces: ClassRef|0, doomednum: i32 (-1 none), flags: u32) -> ClassRef
reg_class_property_i32/f64/str/name(PropId, value…)        // any DECORATE property, typed
reg_class_flag(FlagId, on)
reg_class_field(name_ptr, len, FieldType, array_len) -> FieldId    // host-side per-actor storage (§7)
reg_state_label(label_ptr, len)                             // starts a label at the next state
reg_state(sprite_ptr /*4 bytes*/, frame: u8, tics: i32, flags: u32, callback_id: i32 (-1 none), action_args_ptr, n) -> StateRef
reg_state_flow(kind /*Loop, Stop, Wait, Fail, Goto*/, label_ptr, len, offset)
reg_class_virtual(VirtualSlot, callback_id)                 // override Tick, PostBeginPlay, DamageMobj, Die, Touch, TryPickup, Use, …
reg_class_end()
reg_action(name_ptr, len, signature_ptr, callback_id)       // globally callable action function, usable from DECORATE states of other mods
reg_event_handler(name_ptr, len, order: i32, flags (Static|MapOnly|…), event_mask: u64, callback_base: i32)
reg_thinker_class(name_ptr, len, statnum, callback_id)      // non-actor thinkers
reg_cvar(...)  reg_damage_type(...)                          // sounds stay in SNDINFO; no reg_sound
reg_status_bar(name_ptr, len, callbacks…)  reg_menu(name_ptr, len, kind, callbacks…)   // ui modules
reg_console_command(name_ptr, len, callback_id, flags)       // ui; play-scope effects must go through network events
reg_static_function(class_name, func_name, callback_id)     // ACS ScriptCall targets
```

Rules:
- Class names are global; registering an existing name is an error unless `flags & REPLACE_DEFINITION` (mod patching mods; logged).
- Parent may be any class (built-in, DECORATE, other module) already registered. Native kind is inherited (`08 §5`).
- States reference sprites by 4-char name; frames by letter; same semantics as DECORATE.
- Registration must be deterministic (no randomness, no dependence on unregistered data). The host records a hash of all registration calls; it becomes part of the netplay/save fingerprint.

## 7. Per-actor data

Two complementary mechanisms:

1. **Declared fields** (`reg_class_field`): stored host-side in the actor's `user_vars` storage (`08 §6`), typed (`i32, f64, bool, ActorRef, ClassRef, NameRef, StateRef, SoundRef, TextureRef, Vec2, Vec3` + fixed arrays). Accessible from the module (`actor_field_*`), from DECORATE expressions and ACS `GetUserVariable` if the field name starts with `user_`, saved automatically, survive hot reload, inspectable in the debugger console (`dumpactor`). **Recommended default**; the SDK maps struct fields to these.
2. **Guest-side storage**: modules may keep any data in linear memory keyed by `ActorRef`. The host calls the `OnDestroy` virtual (if registered) so the guest can clean up. Saved through the linear-memory snapshot.

## 8. Determinism rules for play modules

1. Only `ferrite` imports; no clocks, no I/O, no host randomness except `rng_*` (World RNG).
2. Floats: WASM float semantics are deterministic except NaN payloads; the backends canonicalize NaNs (wasmtime option; wasmi canonical by design). The `web-native` backend can't, so play modules running there must pass `ferrite-abi-gen lint --deterministic` (rejects modules where NaN bits can be observed, e.g. `i64.reinterpret_f64` on values not proven non-NaN — conservative: reject any `reinterpret` of float results) or the engine uses `wasmi` for them.
3. Math functions (`sin`, `atan2`, …) are compiled into the guest (Rust `libm` crate; the SDK provides `ferrite_sdk::math` which re-exports a deterministic pure-Rust libm). Host also provides `math_*` imports with deterministic implementations (`ferrite_core::dmath`) for languages without a libm.
4. Iteration order of anything returned by host batch APIs is deterministic (documented per API).
5. No dependence on memory addresses (e.g. hashing pointers) across save/load — the snapshot restores the exact same layout, so this is safe within a session, but hot reload resets memory.

## 9. Feature parity map (ZScript → WASM API)

| ZScript | Ferrite module |
|---|---|
| `class Foo : Actor replaces Bar { Default { … } States { … } }` | `reg_class_begin` + `reg_class_property/flag` + `reg_state*` (SDK: `#[derive(Actor)]` + `states!{}`) |
| member variables | declared fields (§7) |
| `override void Tick() { Super.Tick(); … }` | `reg_class_virtual(Tick, cb)` + `actor_call_super` |
| `action void A_Foo(int x)` | `reg_action` |
| `static` functions, `ScriptCall` from ACS | `reg_static_function` |
| `EventHandler` / `StaticEventHandler` | `reg_event_handler` (`Static` flag = lives across maps & games; play-scope static handlers are saved like others) |
| `Thinker` subclasses | `reg_thinker_class` + `thinker_create` |
| `Inventory`/`Weapon`/`PowerUp` subclasses with overrides (`TryPickup`, `HandlePickup`, `Use`, `ModifyDamage`, `AbsorbDamage`, `DoEffect`, `EndEffect`, `InitEffect`, `GetSpeedFactor`, …) | `VirtualSlot` entries for every virtual in `08 §5` |
| `BaseStatusBar` subclass | `reg_status_bar` (ui) with `Draw(state, ticfrac)`, `Tick`, `AttachToPlayer`, `NewGame`, `ShowPop`, … callbacks + `sbar_*` helpers |
| `OptionMenu`/`ListMenu` subclasses, custom `MenuItem`s | `reg_menu` (ui) + MENUDEF integration (`16-ui.md`) |
| `CVar.FindCVar`, `CVar.GetCVar(name, player)` | `cvar_find`, `cvar_get_*`, `player_cvar_get_*` (user CVars are synced) |
| `EventHandler.SendNetworkEvent` / `NetworkProcess` | `send_network_event` / `NetworkProcess` event |
| `level`, `players[]`, `consoleplayer` | `level_*`, `player_*`, `ui: console_player()` |
| `ThinkerIterator`, `BlockThingsIterator`, `BlockLinesIterator` | `actor_iter_*`, `line_iter_*` (batch) |
| `LineTracer` | `trace` with filter callback id |
| `Screen.Draw*`, `Font`, `TexMan` | `draw_*`, `font_*`, `texture_*` (ui) |
| `Shape2D` | `draw_shape` |
| `Console.Printf`, `A_Log` | `log`, `player_print` |
| `Dictionary`, `Array`, `Map` | language-native containers in guest memory |
| `Object.new()` / custom Object classes | language-native data in guest memory |
| `Wads.FindLump/ReadLump` | `lump_find`, `lump_read(buf)` (common, read-only, deterministic) |
| `StringTable.Localize` | `str_localize` |
| `VisualThinker` (4.12+) | S: `visual_thinker_*` |

Events (`event_mask` bits; payload structs in the ABI crate):

| Play scope | UI scope |
|---|---|
| `OnRegister`, `OnUnregister`, `NewGame`, `WorldLoaded(is_save_game, is_reopen)`, `WorldUnloaded`, `WorldThingSpawned`, `WorldThingDied`, `WorldThingGround`, `WorldThingRevived`, `WorldThingDamaged`, `WorldThingDestroyed`, `WorldLinePreActivated` (can cancel), `WorldLineActivated`, `WorldSectorDamaged`, `WorldLineDamaged`, `WorldLightning`, `WorldTick`, `PostWorldTick`, `PlayerEntered`, `PlayerSpawned`, `PlayerRespawned`, `PlayerDied`, `PlayerDisconnected`, `NetworkProcess`, `CheckReplacement`, `CheckReplacee`, `WorldHitscanFired`, `WorldHitscanPreFired` (can cancel), `WorldRailgunFired`, `WorldRailgunPreFired` | `UiTick`, `PostUiTick`, `RenderOverlay`, `RenderUnderlay`, `UiProcess` (requires `IsUiProcessor`), `InputProcess`, `ConsoleProcess`, `ConsoleCommand` |

(`CheckReplacement`/`CheckReplacee` run at spawn time in play scope but must be pure functions of their inputs — enforced by documentation and by calling them in a context where mutating imports trap.)

Handler order: by `order` value, then registration order. `IsUiProcessor`/`RequireMouse` are registration flags.

## 10. Save games and hot reload

**Save:** for each play module instance: `{ identity, memory: compressed bytes (deflate), mutable globals, scratch-stack depth == 0 asserted, optional ferrite_on_save blob }`. Host-side fields are part of actor data. Saving happens between tics only (no guest frames active).

**Load:** instantiate fresh, call `ferrite_init(LoadGame)`… then **overwrite** memory & globals from the snapshot, then call `ferrite_on_load()` if exported. Data segments are applied by instantiation before overwrite (harmless).

If a module's sha256 differs from the save's, refuse to load unless `save_allow_module_mismatch` is set, in which case memory is **not** restored (`ferrite_init(LoadGame)` gets a flag) and only host-side fields are kept — modules wanting cross-version saves should rely on declared fields.

**Hot reload** (dev only, `script_hot_reload 1`, directory mounts only): on file change → re-run registration in a scratch `Definitions` overlay; if the class layout (fields/states) is compatible, swap instance (fresh memory, `ferrite_init(HotReload)`), keep host-side data; else report and keep the old module.

## 11. Rust SDK (`ferrite-script-sdk`)

Target: `wasm32-unknown-unknown`, `#![no_std]` + `alloc` (a small allocator, e.g. `talc` or `dlmalloc`, bundled). Panics: abort with message exported via `ferrite_panic_info`.

```rust
use ferrite_sdk::prelude::*;

#[derive(Actor)]
#[actor(name = "FireImp", parent = "DoomImp", replaces = "DoomImp")]
#[defaults(health = 90, speed = 10.0, pain_chance = 150)]
#[flags(+FLOORCLIP, -NOINFIGHTING)]
pub struct FireImp {
    charge: i32,                   // → reg_class_field, host-side
    #[field(name = "user_heat")] heat: f64,   // visible to DECORATE/ACS as user_heat
}

states! { FireImp;
    Spawn:   "TROO" "AB" 10 a_look; loop;
    See:     "TROO" "AABBCCDD" 3 a_chase; loop;
    Missile: "TROO" "EF" 8 a_face_target; "TROO" "G" 6 fire_ball; goto See;
    Death:   inherit;              // keep parent label
}

#[actions]
impl FireImp {
    fn fire_ball(this: Actor<Self>, cx: &mut Play) {
        if let Some(t) = this.target(cx) {
            let n = this.charge(cx);
            for i in 0..=n.min(2) {
                this.spawn_missile_angle(cx, class!("DoomImpBall"), this.angle(cx) + (i as f64 - 1.0) * 5.0, 0.0);
            }
            this.set_charge(cx, 0);
        }
    }
}

impl ActorVirtuals for FireImp {
    fn tick(this: Actor<Self>, cx: &mut Play) {
        this.super_tick(cx);
        if cx.level().time() % 35 == 0 { this.set_charge(cx, this.charge(cx) + 1); }
    }
    fn damage_mobj(this: Actor<Self>, cx: &mut Play, d: DamageArgs) -> i32 {
        let dmg = if d.damage_type == name!("Fire") { 0 } else { d.damage };
        this.super_damage_mobj(cx, DamageArgs { damage: dmg, ..d })
    }
}

#[event_handler(order = 0)]
#[derive(Default)]
pub struct ImpTracker { kills: u32 }   // guest-side state, saved via memory snapshot

impl PlayEvents for ImpTracker {
    fn world_thing_died(&mut self, cx: &mut Play, e: &ThingDied) {
        if e.thing.is::<FireImp>(cx) { self.kills += 1; }
    }
}

ferrite_sdk::export_module!(classes = [FireImp], handlers = [ImpTracker]);
```

Design notes for the SDK implementer:
- `Actor<T>` is a `Copy` handle (`ActorRef` + `PhantomData<T>`); accessors generated by the derive call `actor_field_*` with `FieldId`s captured at registration in a `static` table.
- `Play` / `Ui` context types are zero-sized tokens proving scope; mutating methods require `&mut Play`. UI methods require `&mut Ui`. This mirrors scope rules at compile time.
- Re-entrancy: host calls can re-enter the guest (e.g. `spawn` triggers `PostBeginPlay` of another class in the same module). The SDK must not hold `RefCell` borrows across host calls; event handler `&mut self` is obtained from a slot table with a re-entrancy guard that panics with a clear message on aliasing.
- `class!("Name")`, `name!("Fire")`, `sound!("imp/attack")` cache lookups in statics (resolved in `ferrite_init`).
- Provide `ferrite_sdk::testing` (native target, links `ferrite-script` + `ferrite-sim` in-process) so modders can unit test modules with `cargo test` against synthetic maps.

`cargo ferrite` subcommand (in `ferrite-cli`): `cargo ferrite new`, `build` (wasm32 release + `wasm-opt`-free size optimizations via `wasm-encoder`-based stripping, keeps `name` section), `pack` (produces `.pk3` with manifest), `run` (launch engine with the mod, hot reload enabled).

## 12. Other languages

`ferrite-abi-gen` emits:
- `ferrite.h` + `ferrite_sdk.c` (C/C++ via `clang --target=wasm32`): raw imports + helper macros for registration.
- `ferrite.zig` (Zig), `ferrite.ts` (AssemblyScript), `api.json` (any other tooling).
Each generated SDK ships an "Imp replacement" example that passes the conformance test.

## 13. ADR: core modules instead of the Component Model

Context: WIT/components give rich types and multi-language bindings for free, but (as of the project start) browser-native support requires transpilation (jco), `wasmi` component support is incomplete, and cross-component calls add overhead in hot paths (actor ticks).
Decision: v1 uses core modules with a generated C-like ABI. The `api/*.ron` IDL is kept WIT-expressible (only scalars, handles, byte buffers), so a v2 ABI can be offered as a component world with an adapter.
Consequences: we own the binding generator; ABI evolution rules are ours to enforce (append-only enums, new functions only, major bump to remove).

## 14. Tests

- Conformance suite: a set of `.wasm` fixtures (built from `tests/modules/*` in Rust and C) run on every backend, asserting identical world checksums per tic.
- Determinism: fuzz-ish test spawning random module-driven actors, checksum across wasmtime/wasmi/x86/arm/wasm32.
- Save/load round-trip mid-session with active module state; hot-reload test.
- Validation tests for every rejected proposal and import scope violation.
- Benchmarks: 10k actors with a module `Tick` override at 35 Hz must run ≤ 4 ms/tic on wasmtime (desktop), ≤ 25 ms on wasmi.
