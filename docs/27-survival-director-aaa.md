# 27 — Survival Director 2.0: "AAA" upgrade of `mods/doom1-survival`

**Audience:** a coding agent implementing this in the Room repo. Read `AGENTS.md`,
`docs/02-conventions.md` (§Determinism) and `docs/12-wasm-scripting.md` §15–16 first.
**Status:** done — **SV0**–**SV9** + **MH0**–**MH4** accepted (`docs/21` §0). **MH5** optional. C6 translation render + C10 sector lights deferred.
Track progress with the `SV*` (mod) and `MH*` (engine, §9.5) milestones in §11.

"AAA" here does not mean new assets at any cost. It means the mod plays like a
designed product: a run with a beginning, escalation, choices, set pieces,
readable feedback and an ending, instead of "a timer spawns random monsters
forever". Most of the gameplay is achievable with the **current** ABI
(Track A); some presentation needs data lumps in the mod WAD (Track B), an
engine feature that lets mods **draw on screen and read player input** (the
Mod HUD, §9), and a few other small engine imports (Track C, §10).

---

## 0. TL;DR for the implementing agent

1. Keep the mod deterministic, `no_std`, allocation-free on hot paths, and
   working on **shareware `doom1.wad`** (fall back when a class is missing).
2. Refactor `mods/doom1-survival/src/lib.rs` (404 lines, one file) into modules
   with a host abstraction so game logic is unit-testable natively (§7).
3. Replace "timer → random wave" with a **Director**: threat budget, intensity
   curve, wave archetypes, elites, objectives, boss phases, an extraction
   finale, an in-run economy with an **on-screen draft menu** between waves
   (§5–6).
4. Fix coop: today almost everything is hard-wired to `player(0)` (§2.2).
5. Engine work, needed for the HUD and menus: the **Mod HUD** (§9): mods
   create widgets (text, panels, images, bars, world markers) drawn by the
   shells, and capture player input to drive menus. It is deterministic and
   lockstep-safe with no protocol change.
6. Then: Track B (DECORATE/GLDEFS/SNDINFO/sprites packed into `survival.wad`,
   §8) and Track C (other append-only imports: per-actor flags, screen
   flash/quake, radius attack, sector light, translation rendering, module
   memory in saves, §10).
7. Every milestone ends green on: `cargo fmt --all`, `cargo clippy --all-targets -- -D warnings`,
   `cargo test --workspace`, `cargo build --target wasm32-unknown-unknown -p room-web --features browser`,
   `./scripts/room-server.sh mods`, `cargo test -p room-game --test wasm_mod_survival`.

---

## 1. File map (what to read / touch)

| Path | Role |
|---|---|
| `mods/doom1-survival/src/lib.rs` | The mod (Rust, `room-script-sdk`). Standalone crate, not a workspace member (`[workspace]` in its `Cargo.toml`), `crate-type = ["cdylib","rlib"]`, `opt-level="z"`, `panic="abort"`. |
| `mods/doom1-survival/room.toml` | Manifest → packed as `ROOMMOD` lump; module lump `SURVIVAL`, scope `play`. |
| `mods/doom1-survival/README.md` | User-facing description — update at the end. |
| `mods/doom1-royale/src/lib.rs` | Reference for multi-player patterns: `level_things`, player starts, zone, `game_end_match`, `misc/teleport` sound, compass helper, fixed `Msg` buffer. |
| `mods/doom1-bounty/bounty.ts` | AssemblyScript reference (marked elite, compass hints, `Warden` boss). |
| `crates/room-script-sdk/src/game.rs` | Guest API: `Actor`, `spawn`, `actors_of_class` / `actors_in_radius`, `player`, `player_stat`, `print`, `skill`, `level_stat`, `map_name`, `random`, `sin/cos/atan2/sqrt`, `floor_z/ceiling_z`, `exit_level`, `set_music`, `get_rule/set_rule`, `end_match`, `level_things`, `Event`, `RoomMod`, `export_mod!`. |
| `crates/room-script-sdk/src/sys_game.rs` | Raw imports (`game_imports!` macro). Native builds get zero-returning stubs. |
| `crates/room-script-sdk/src/bump.rs` | 64 KiB bump heap for host allocations; `room_free` is a no-op → **do not allocate per tic**. |
| `crates/room-script-abi/src/lib.rs` | Constants: `event_mask`, `game::{level_stat, player_prop, actor_i32, actor_f64, actor_ptr, spawn_flags, rule, thing_type}`, `class_prop`, `class_flag`. |
| `crates/room-script/src/game.rs` | Host import table (`game_fns!` → `GameFn`, 44 imports, **ABI order = declaration order: append only**). |
| `crates/room-sim/src/script_mods.rs` | `ModRuntime` + the `match GameFn::…` host implementations (~1300 lines). |
| `crates/room-web/as/room.ts` | AssemblyScript mirror of the API (keep in sync for any new import). |
| `crates/room-game/tests/wasm_mod_survival.rs` | End-to-end test (registration, wave 1 spawn on schedule, determinism checksum, doom1 E1M1). It asserts exact strings such as `"Wave 1 - 6 incoming"` — update deliberately. |
| `scripts/room-server.sh` (`build_mods`) | Builds each `mods/<dir>` and packs `ROOMMOD` + module lump (+ `source` lump) into `<module>.wad`. Track B extends this. |
| `docs/12-wasm-scripting.md` §15–16 | Current runtime contract + API tables (update when adding imports). |

---

## 2. Audit of the current mod

### 2.1 Behaviour today
- `on_loaded`: resolves classes, collects up to 128 "spots" from **every actor
  on the map** (non-player, non-missile; items, decorations and monsters alike),
  first wave at `wave_gap/2` seconds.
- `on_tick`: every `wave_gap` seconds (60/50/40/32/25 by skill) spawns
  `2 + wave + extra` monsters (cap `MAX_WAVE = 24`) from a pool that unlocks one
  class per wave (`ZombieMan, ShotgunGuy, PhantomImp, Demon, Spectre, Juggernaut`),
  ≥ 320 units from player 0, telefog + position check, `target = player0`,
  `set_state("See")`. Every 5th wave adds a Baron and music `D_E1M8`.
- Kills: streak messages, heal killer by 5..1 (cap 100). Wave cleared → 20 clips
  + `4+wave` shells, next wave pulled to ≤ 10 s away.
- Low-health warning; "You survived N waves" on death.
- Two registered classes: `PhantomImp` (DoomImp, 80 HP, speed 12, alpha 55 %),
  `Juggernaut` (Demon, 400 HP, scale 130 %, radius 36, height 72, mass 1000).

### 2.2 Defects and limits to fix (SV1)
1. **Coop is broken in subtle ways:** spawn distance, targeting and the
   "re-target sleepers" loop only consider `player(0)`; if player 0 is dead no
   wave spawns at all (`spawn_wave` returns early) even if others are alive.
2. **Spots are unreliable:** decorations/items inside closets, secret rooms,
   behind locked doors or on unreachable ledges are used. No stuck detection;
   a wave can be "uncleared" forever because one monster is sealed in a closet.
3. **Waves stack** on a fixed timer regardless of the fight state; no pacing.
4. `alive` tracking silently drops monsters past 24; boss Baron is in `alive`
   only if spawned first.
5. No run structure: no goal, no ending, no score, no carry-over between maps.
6. Feedback is only `print` text (96-byte buffer), every message to everyone.
7. Boss music restore only works for `ExMy` maps (`music_len` logic); Doom 2 /
   Freedoom `MAPxx` maps never restore music.
8. Module memory is not stored in save games (`docs/12` §15 "Not yet"); a
   loaded save restarts the director state.

---

## 3. Hard constraints (non-negotiable)

- **Determinism** (`docs/02` §Determinism, `docs/12` §8): only `game::random`
  (playsim RNG) and `game::{sin,cos,atan2,sqrt}`; no `f64::sin` etc. from std
  (the crate is `no_std` on wasm anyway), no wall clock, no host state. Iterate
  fixed arrays in index order. The mod runs in lockstep rooms; any divergence
  desyncs the room.
- **Budgets** (`docs/12` §4.2): 50 M fuel per call, 200 M per tic per module;
  a trap or exhausted fuel **disables the mod** for the session. Stagger scans:
  never call `actors_of_class(NULL, …)` (≤ 512 results) more than once per
  second; spread expensive work across tics with a round-robin cursor.
- **Event cascade cap:** events caused by handlers are drained up to 16 rounds /
  4096 events. Do not spawn in `ThingSpawned` handlers unboundedly.
- **Available events** (today): `WORLD_LOADED`, `WORLD_TICK`, `WORLD_THING_DIED`,
  `WORLD_THING_SPAWNED`, `WORLD_THING_DAMAGED`, `PLAYER_ENTERED`, `PLAYER_DIED`,
  `WORLD_UNLOADED`, `WORLD_ITEM_PICKUP`, `NEW_GAME`. Subscribe only to what you use.
- **Memory:** no allocator by default. Use fixed-capacity arrays (`[T; N]` +
  `len`) and the existing `Msg` pattern. The SDK `alloc` feature exists but the
  host heap is a bump allocator — if you enable `alloc`, allocate only in
  `on_loaded`, never per tic.
- **Content availability:** shareware `doom1.wad` has only `ZombieMan,
  ShotgunGuy, DoomImp, Demon, Spectre, LostSoul, Cacodemon, BaronOfHell` and
  weapons `Fist, Chainsaw, Pistol, Shotgun, Chaingun, RocketLauncher` (no SSG,
  plasma, BFG, Doom 2 monsters, no `D_RUNNIN` etc.). Every table entry must be
  resolved with `class_find` at load and skipped if `ClassRef::is_null()`.
  Doom 2 / Freedoom unlock the full tables automatically.
- **Netplay messages:** `print(Some(slot), …)` shows only on that seat; use it
  for personal info (currency, perk choice) and `print(None, …)` for team info.
- **Registration is deterministic** and fingerprinted; class list must not
  depend on runtime data.
- **Map things vs. runtime:** game rules (`set_rule`) reset every map; set them in
  `Loaded`. Mod globals persist across maps within one game and reset on a new
  game (`docs/12` §16.4) → use this for run carry-over.

---

## 4. Design pillars ("AAA" translated into requirements)

| Pillar | Requirement | Section |
|---|---|---|
| Pacing | An AI Director with an intensity model (build-up → peak → relax), not a fixed timer. | 6.2 |
| Variety | Wave archetypes, elite affixes, objectives, set-piece bosses. | 6.3–6.7 |
| Agency | Between waves the player chooses upgrades (pickup drafts); currency rewards skilful play (combos, objectives). | 6.5 |
| Readability | Every threat is telegraphed (announcer line, sound, light/tint, spawn fog); every reward is acknowledged (flash, sound, message). | 6.8 |
| Stakes & ending | A run has a finale (extraction) and a score/grade; failing objectives costs something. | 5, 6.10 |
| Coop-first | Scales with 1–8 players; personal economy, shared threat. | 6.9 |
| Robustness | Never softlocks: stuck monsters are recycled, unreachable spots are learned and avoided, a wave always ends. | 6.1 |
| Fairness | No spawns in the player's view or close behind them; difficulty adapts to how the team is doing (rubber-band within limits). | 6.1–6.2 |

---

## 5. Run structure (the game loop)

```
Loaded ─► WARMUP (20–30 s: loot spawned, "Survival Director online")
   └─► repeat per round r = 1..R_final:
         BUILD_UP   (trickle, low intensity, 10–25 s)
         ASSAULT    (main wave budget released per archetype; optional objective)
         PEAK/HOLD  (director sustains intensity up to cap for N s)
         CLEAR      (remaining wave monsters hunted; stragglers recycled after timeout)
         INTERMISSION (15–25 s: pickup draft, resupply, "Round r cleared", score tally)
         every 5th round: BOSS round instead of ASSAULT (phased boss + adds)
   └─► FINALE at round R_final (default 15; 10 on ITYTD/HNTR): EXTRACTION
         - an exit beacon spawns at a far proven spot
         - team must hold within radius 160 for 60 s (timer pauses when nobody is inside)
         - director runs at max intensity, then exit_level(false)
   └─► next map: carry over round index (−2), Marks, perks; or ENDLESS if the map is the last
PlayerDied (all players dead / RESPAWN rule = 0) ─► RUN SUMMARY ─► end
```

- Single player: Doom's normal death/restart applies; on `PlayerDied` show the
  run summary (§6.10). The director resets on the next `Loaded` (globals keep a
  "best run" for the session).
- Coop: use `set_rule(rule::RESPAWN, 1)` (default) in normal rounds; optionally
  `0` during the finale ("no respawns during extraction") — make it a tuning flag.
- State machine lives in `director.rs` as `enum Phase { Warmup, BuildUp, Assault,
  Peak, Clear, Intermission, Boss, Finale, Summary }` with `phase_started_at`
  (tics). All durations are integer tics.

---

## 6. Systems

### 6.1 Spawn-point graph (`spots.rs`)

**Collection (on `Loaded`):**
1. Candidates from `level_things(ednum, …)` for monster editor numbers (3004,
   9, 3001, 3002, 58, 3006, 3005, 3003 and Doom 2 ones), player starts (1–4) and
   deathmatch starts (11). These are designer-placed, known-good spots. Then add
   item/decoration actors (current approach) as lower-priority candidates.
2. Reject if `ceiling_z - floor_z < 64` at the point. Dedupe on a 64-unit grid.
3. Capacity `MAX_SPOTS = 256` (royale uses 256). Store `{x, y, z, score: i16,
   uses: u16, fails: u16, last_used: i32}`.

**Selection (per spawn):**
- Team reference = alive players (§6.9). Candidate valid if its distance to
  **every** alive player is ≥ `min_dist` (default 384) and to the **nearest**
  ≤ `max_dist` (default 2048, widened ×1.5 after 4 failed tries).
- Prefer spots outside players' facing cone: penalty if
  `|angle_to_spot − player.angle()| < 60°`.
- Sight test after spawning: `m.can_see(player)` for players within 768 →
  if visible and the archetype isn't "ambush-visible", destroy and retry
  (deterministic; costs one spawn call). Cap 8 tries per monster, 64 per tic.
- Weighted pick: `w = base(score) − recency_penalty − facing_penalty`; choose
  with `random()` over cumulative integer weights (no floats in the weight sum).

**Learning (makes spots reliable):**
- Track each director monster `{actor, spot_idx, spawn_tic, last_pos, still_since}`.
- Every 35 tics, if a monster moved < 16 units in 12 s while having a target and
  no line of sight to any player → **stuck**: `destroy()`, refund its threat to
  the budget, `spot.fails += 1`, `score -= 8`.
- If a monster from a spot reaches within 256 of a player or damages one →
  `score += 2` ("proven").
- Spots with `fails ≥ 3 && score < −16` are disabled for the map.
- Global straggler rule: in `Clear`, after 30 s, remaining wave monsters are
  teleported (`set_pos` to a proven spot near the team, with telefog spawn of
  `TeleportFog` at both ends) or destroyed if no such spot. **A round always ends.**

### 6.2 Director: threat budget and intensity (`director.rs`)

Fixed-point integers only (`i32`, units ×100 where fractional).

- **Threat cost per class** (`tables.rs`): ZombieMan 2, ShotgunGuy 3, DoomImp 3,
  PhantomImp 4, LostSoul 2, Demon 4, Spectre 5, Cacodemon 8, Juggernaut 10,
  HellKnight 12, BaronOfHell 18; Doom 2: ChaingunGuy 6, Revenant 12, Fatso 16,
  Arachnotron 14, PainElemental 14, Archvile 30. Elite affix: +50 % cost.
- **Round budget:** `B(r) = (12 + 6·r + r²/4) · skill_mul · players_mul / 100`,
  with `skill_mul = [60, 80, 100, 130, 170]`, `players_mul = 100 + 55·(n−1)`.
- **Alive cap** (simultaneous threat): `C = 20 + 3·r` scaled the same way;
  hard cap 64 actors tracked (`MAX_TRACKED = 64`).
- **Intensity** per player `I_p ∈ [0, 10000]`:
  `+ damage_taken × 60`, `+ 150` per director monster within 512 that can see them,
  `+ 400` when health < 30; decays `−25` per tic when no damage in the last 3 s.
  Team intensity `I = max(I_p)` (L4D-style: the most stressed player drives relaxation).
- **Release rule (per 7 tics):** in `Assault`/`Peak`, spawn the next archetype
  group if `alive_threat + cost ≤ C` and `I < peak_threshold` (7000). When
  `I ≥ 7000` for 5 s → enter `PeakFade`: stop releasing for 6–10 s.
  `BuildUp` only releases cost ≤ 3 monsters and keeps `I < 3000`.
- **Adaptive assist (bounded):** if the team lost > 60 % health in the last 20 s
  twice in a row, the next round's budget is ×0.85 (min 0.7× nominal); if the
  last round was cleared with > 80 % health and < 60 s, ×1.1 (max 1.3×). Never
  change budgets mid-round. Disabled on Nightmare.

### 6.3 Wave archetypes (`encounters.rs`)

Each round picks one archetype (weighted, no repeat of the previous one) that
shapes *how* the budget is released. Data-driven table:

| Archetype | Behaviour | Constraints |
|---|---|---|
| Swarm | Many cheap units (cost ≤ 3) from 3–4 spots in quick pulses. | Default early. |
| Pincer | Two groups from spots on opposite sides of the team (angle diff > 120°). | Needs ≥ 2 valid spots. |
| Ambush | Group spawns at 384–640 behind the slowest player, delayed 2 s after an audible cue. | Max once per 3 rounds. |
| Elite Pack | 1–2 elites (§6.4) + escorts with `master` set to the elite. | From round 3. |
| Siege | Ranged units (Imp/Caco/ShotgunGuy) held back — spawn far (max_dist) with `target` set, plus a melee rush every 15 s. | From round 4. |
| Horde Rush | Peak archetype: Demons/Spectres/LostSouls released continuously for 25 s ignoring the peak threshold (cap still applies). | From round 6, telegraphed 5 s ahead. |
| Boss | §6.7. | Every 5th round. |

Monster pool unlocks by round (not "one per wave"): a class is eligible if
`round ≥ unlock_round[class]` and it resolves at load.

### 6.4 Elite affixes (`elites.rs`)

An elite = base monster + 1 affix (2 from round 10). Implemented with existing
imports and per-tic logic over the tracked set (iterate only elites, ≤ 8):

| Affix | Mechanism (Track A) |
|---|---|
| Brute | `health ×2.5` (set_health), `SCALE_X/Y ×1.25`, `DAMAGE_FACTOR 0.8`. |
| Haste | `SPEED ×1.5`. |
| Regenerating | +2 % spawn health every 35 tics while not damaged for 3 s. |
| Vampiric | On damaging a player (`ThingDamaged` with `source == elite`) heal elite by damage ×2. |
| Volatile | On death: spawn `ExplosiveBarrel` at corpse and `damage()` it so it explodes (or Track C `actor_radius_attack`). |
| Splitter | On death spawn 2 copies of the base class at half health, scale 0.8. |
| Summoner | Every 8 s while it can see a player, spawns a `LostSoul` with `master = elite` (max 3 alive). |
| Phasing | `ALPHA 0.3` and `DAMAGE_FACTOR 0.25` for 2 s every 6 s (telegraph with sound). |

- Elites are registered **subclasses** (`EliteImp`, `EliteDemon`, … via
  `reg::class_begin`) so Track B can attach GLDEFS lights / DECORATE states by
  class name, and so `ThingDied` can identify them cheaply with `is_a`.
- Every elite spawn is announced with its name: "Volatile Brute Imp".
- Elite kill = Marks bonus + guaranteed ammo/health drop (`spawn` an item at the corpse).

### 6.5 Economy and the draft menu (`economy.rs`, `perks.rs`, `ui.rs`)

Player choice happens in an **on-screen menu** drawn with the Mod HUD (§9):
widgets for the cards, input capture for the selection. Nothing is spawned in
the world.

- **Marks** (personal currency, `[i32; 8]` by slot): kill = threat cost ×
  combo multiplier (1.0 / 1.5 at 3-streak / 2.0 at 5 / 3.0 at 8), objective
  bonus, elite bonus, round-clear bonus split among alive players. Shown as a
  permanent HUD counter (top-right) with a "+12 ×1.5" pop that fades out
  (widget patch + tween), not as chat lines.
- **Draft (at `Intermission`, per alive player, personal):**
  1. Roll 3 perks for that seat (`random`, weighted by rarity; no duplicates;
     respect stack limits). Store the offer in `DraftState { seat, offer: [PerkId; 3],
     highlight: u8, chosen: Option<u8>, opened_at, rerolls }`.
  2. `player_capture_input(slot, WEAPON_SELECT | WEAPON_CYCLE | USE | ATTACK)`.
  3. Draw a `MODAL` panel for that seat only:
     - title "ROUND 5 CLEARED — CHOOSE AN UPGRADE" (BigFont, gold) + grade of the round;
     - 3 cards side by side (`PANEL` 92×110 each): icon (`IMAGE`, existing
       sprite/graphic such as `MEDIA0`, `PSTRA0`, `ARM1A0`, `SHOTA0`; Track B
       adds custom icons), name, rarity colour border (common white, rare blue,
       epic purple, legendary gold), 2–3 line description (wrap at card width),
       stack info ("2/3"), key hint `{key:slot 1}` / `{key:slot 2}` / `{key:slot 3}`;
     - highlighted card: brighter border + `out-back` scale/offset tween;
     - footer: Marks balance, "{key:weapnext}/{key:weapprev} move · {key:+use} confirm",
       a `BAR` showing the remaining intermission time, and a 4th small card
       "Reroll — 50 Marks" (key 4) when affordable.
  4. Input from `player_cmd` each tic: `weapon_select` 1–3 picks and confirms,
     4 rerolls; `pressed & WEAPONNEXT/WEAPONPREV` moves the highlight;
     `pressed & (USE|ATTACK)` confirms the highlight. Play `menu/cursor`,
     `menu/choose`, `menu/invalid` with `player_sound` (E11).
  5. On confirm: apply the perk, close the panel (fade out 8 tics), release
     capture, gold screen flash (Track C `BONUSCOUNT`), and show the team a
     small non-modal line "Anthony took **Lifesteal II**" (OVER_HUD, 3 s).
  6. Timeout (end of intermission): auto-pick the highlighted card, close, release.
     Death or disconnect during a draft: close and release; the offer is lost.
  7. **Pause while choosing:** opening a draft sets `rule::PAUSED` so thinkers /
     damage / `maptime` freeze; script `WorldTick` still runs for menu input.
     Closing the last open draft clears the pause, then a 5 s banner runs before
     the next wave. Draft timeout counts open-menu tics, not `maptime`.
- **Team ready-up:** while a draft is open, each seat shows a small strip of
  teammates' status (picking / ✓). If every alive player has chosen, the
  intermission ends early (5 s countdown banner).
- **Shop (optional, same widget kit):** at boss-round intermissions, a second
  page (`{key:weapnext}` to switch tab) sells ammo, armor and a rarity upgrade
  for Marks.
- **Fallback build:** a module importing functions the engine does not have
  is rejected at load (imports are linked, not feature-detected). So the HUD
  version requires an engine with MH0–MH3. Keep the old world-pedestal draft
  (pickup items in front of the player, chosen by `ItemPickup`) behind a
  cargo feature `legacy-draft` for older engines; the default build uses the
  menu.
- **Perk list (Track A-implementable):**

| Perk | Effect | Implementation |
|---|---|---|
| Vitality | Max health 100 → 125 → 150 (stacks 2) | Mod-enforced cap in heal logic; give `HealthBonus` repeatedly or `set_health`. |
| Lifesteal | Heal 3/5/8 per kill | `on_kill`. |
| Ammo Siphon | Kills refund ammo of current weapon type | Give `Clip`/`Shell`/`RocketAmmo` by killer inventory heuristics. |
| Plating | +10 armor at round start, up to 100 | Give `ArmorBonus` ×N. |
| Toughness | `DAMAGE_FACTOR` 0.9 / 0.8 on the player actor | `actor_set_f64` (host supports it on any actor). |
| Fleet | `SPEED` ×1.1 on player | **Verify** player movement honours `actor.speed`; else drop or Track C. |
| Second Wind | Once per round, when health < 20 → give `Soulsphere`, announce | `ThingDamaged` watcher. |
| Berserker | Round start: `Berserk` | Give. |
| Arsenal | Unlock next weapon in order (Shotgun → Chaingun → Rocket → Chainsaw; Doom 2 adds SSG/Plasma/BFG) + ammo | Give. |
| Bounty Hunter | +50 % Marks from elites | Economy multiplier. |
| Backpack | `Backpack` | Give. |

Perks are per-slot bitsets + stack counts (`[PerkState; 8]`). Re-apply
persistent effects (`DAMAGE_FACTOR`, speed) on `PlayerEntered` (respawn creates
a new body).

### 6.6 Objectives (`objectives.rs`)

From round 2, 50 % of non-boss rounds get one objective, announced at `Assault` start:

| Objective | Success | Fail | Reward |
|---|---|---|---|
| Hold Point | Team spends 25 s (cumulative) within 128 of a beacon spot | Round ends first | 60 Marks each + a bonus draft card |
| Assassinate | Kill the marked elite within 45 s | Timer | 80 Marks to killer, 30 to others |
| Salvage | Collect 5 `SalvageCrate` (registered subclass of a small pickup) spawned at far proven spots within 60 s | Timer | Ammo crate + 40 Marks |
| Protect | Spawned friendly `Survivor` (ZombieMan subclass, `spawn_flags::FRIENDLY`, 300 HP) survives the round | It dies | Survivor joins the team for next round |
| Flawless | No player drops below 50 % health this round | Health threshold | 100 Marks |

Beacons are spawned actors (e.g. a lamp/column decoration subclass; Track B
gives them a GLDEFS light) **plus a Mod HUD `MARKER` widget** anchored to the
beacon/target/crate (icon, distance label, off-screen edge arrow, §9 E8). An
objective tracker panel (top-left, `OVER_HUD`) shows the goal, a progress `BAR`
("Hold 14/25 s", "Crates 3/5") and the remaining time.

### 6.7 Bosses (`bosses.rs`)

Boss round = boss + escort budget at 40 %. Boss health ×(1 + 0.6·(players−1)).

- **Warden** (BaronOfHell subclass, the bounty mod's idea): phases at 66 % / 33 %:
  P1 normal; P2 `SPEED ×1.3`, summons 4 Imps (`master = boss`), announcer line;
  P3 enrage: Haste + Regenerating 1 %/s, music switch, brief invulnerability
  window (`DAMAGE_FACTOR 0` for 3 s — Track C uses a real flag).
- **Matriarch** (Cacodemon subclass, rounds 10+): periodically "births"
  LostSouls/Cacos, scale 1.6.
- Doom 2/registered: Cyberdemon/SpiderMastermind variants if `class_find` succeeds.
- Boss health readout: a top-centre boss bar (`BAR` 200×6 with 3 `segments`
  marking the phase thresholds, name above in BigFont, colour shifts per phase),
  patched every 5 tics with `hud_widget_patch(VALUE)`; a `MARKER` nameplate over
  the boss. Phase change: bar flashes (tween alpha) + red screen flash.
- Boss death: big reward, guaranteed draft of 4, music restore (store the map's
  music name for **both** `ExMy` → `D_ExMy` and `MAPxx` → lookup table of Doom 2
  music lumps; if unknown, don't change music at all).

### 6.8 Presentation & feedback (`announcer.rs`)

- **Announcer queue:** fixed ring of 16 messages with priority (0 info … 3
  critical) and per-seat target; at most one team message per 1.5 s; lower
  priority messages expire after 4 s. All `say!` calls go through it.
- **Sounds:** use SNDINFO logical names already used in-tree (`misc/teleport`
  in royale). Verify others exist in Room's built-in SNDINFO before use (grep
  `crates/` for the table); missing names are silently skipped by the mixer.
  Cues: round start, elite spawn, objective start/success/fail, draft
  available, boss phase, extraction open.
- **Music:** intermission/warmup → map music; Horde Rush/Boss → `D_E1M8`
  (shareware) or a Doom 2 track; finale → strongest available.
- **Telegraphs:** every spawn uses `TELEFOG`; ambush/horde give a 2–5 s warning
  line + sound; elites announced by name.
- **HUD layout (Mod HUD widgets, all per seat, ids from a constant table in `ui.rs`):**

| Widget | Anchor / layer | Content |
|---|---|---|
| Round banner | top-centre, OVER_HUD | "ROUND 7 — PINCER" (BigFont), fades in/out 2 s at phase start; "BOSS", "EXTRACTION" variants |
| Phase / timer | top-centre under banner | phase name + `BAR` for build-up / intermission / extraction hold time |
| Threat meter | top-left | segmented `BAR` of alive threat vs cap (tells the player how much is left) |
| Objective tracker | top-left under threat | goal text + progress bar + time left |
| Marks + combo | top-right | Marks counter, combo multiplier with decay bar, "+N" pops |
| Boss bar | top-centre | §6.7 |
| Markers | world | objective, elite nameplates (affix colour), teammate down, extraction zone |
| Draft / shop | centre, MODAL | §6.5 |
| Run summary | centre, MODAL | §6.10 |

  Chat-style `print` stays only for rare events (kill streak names, teammate picks).
  Budget: patch dynamic values at most every 5 tics; re-`set` text only when it changes.
- **Juice requiring Track C:** screen flash on Marks/perk (`BONUSCOUNT`), red
  flash/quake on boss phase change.

### 6.9 Coop & team model (`team.rs`)

- Each tick refresh (cheap) `players: [Option<Actor>; 8]` via `player(slot)` and
  `player_stat(slot, player_prop::IN_GAME)`; alive = exists && !is_dead.
- Targeting: new monsters target the **nearest alive player** to their spawn;
  the 1 Hz re-target loop re-assigns targets that are dead/null.
- Scaling: budget/alive cap via `players_mul`; boss health; draft per player;
  objectives radius +32 per extra player.
- Wave spawning continues while **any** player is alive.
- Personal UI is per-seat widgets (`seat1 = slot+1`); team UI uses `seat1 = 0`.
  Each seat sees only its own draft; the summary is shared.

### 6.10 Scoring and run summary

Score = Σ kill threat × combo + objectives + rounds × 100 + boss kills × 500 −
deaths × 200. Grade per round and run: S ≥ 90 % of par, A ≥ 75, B ≥ 55, C else
(par = budget-derived). At death/extraction show a MODAL **run summary panel**
(all seats): per-player rows (kills, elites, Marks earned, perks taken as icons,
deaths), team totals (rounds, objectives n/m, best streak, time), the big
grade letter with a tween, and "best run this session" (module globals). Input
capture on `USE` closes it; it auto-closes after 15 s.

### 6.11 Difficulty presets

Replace `TUNING` with a richer table indexed by skill (1..5): `skill_mul`,
`round_gap_ticks`, `heal_per_kill`, `elite_chance_pct`, `objective_chance_pct`,
`assist_enabled`, `final_round`, `marks_mul`. Nightmare: no assist, elites from
round 1, final round 20.

---

## 7. Code architecture

```
mods/doom1-survival/src/
  lib.rs          export_mod!, RoomMod impl, event routing only
  host.rs         trait Host { … } + SdkHost (wraps room_script_sdk::game) — the only file that calls the SDK
  fixed.rs        tiny fixed-capacity Vec<T, N> (no alloc), ring buffer
  text.rs         Msg buffer (raise to 128 bytes), say!/say_to! macros
  tables.rs       monster/threat/unlock tables, perks, archetypes, tuning per skill
  team.rs         players, intensity per player
  spots.rs        spot graph, selection, learning, stuck detection
  director.rs     Phase state machine, budget, release rule, adaptive assist
  encounters.rs   archetype planners → spawn requests
  elites.rs       affixes, per-tic elite logic
  economy.rs      Marks, combos, round rewards
  perks.rs        draft spawning, perk effects
  objectives.rs   objective types and tracking
  bosses.rs       boss definitions and phases
  announcer.rs    priority message queue, sound cues, music
  ui.rs           Mod HUD layout: widget ids, HUD panels, draft/shop/summary menus, input handling
  register.rs     all reg_class_* calls (classes, elites, crates, survivor, bosses; pedestals only with `legacy-draft`)
```

Rules:
- **`Host` trait** abstracts every engine call used by the logic (spawn, query,
  print, random, math, HUD widgets, `player_cmd`, input capture…). `SdkHost` forwards to `room_script_sdk::game`.
  Native unit tests implement `FakeHost` (scripted actors, seeded LCG for
  `random`) so the director can be simulated for thousands of tics in
  `cargo test --manifest-path mods/doom1-survival/Cargo.toml` (the crate is
  already `rlib` + native stubs exist in `sys_game.rs`).
- Store `Actor` handles, never assume they stay valid: check `exists()`
  (stale refs read as defaults per `docs/12` §5.2).
- One `Survival` struct owns sub-states; keep total static size reasonable
  (< 64 KiB) — it lives in linear memory.
- Per-tic work budget: team refresh every tic (≤ 8 calls), director release
  every 7 tics, elite logic every 5 tics, spot learning/stuck every 35 tics
  round-robin over tracked monsters, full actor scans only on `Loaded`.
- Keep `PhantomImp` and `Juggernaut` names (existing test + users).

---

## 8. Track B — content lumps inside `survival.wad`

The mod WAD is a normal PWAD mounted after the IWAD (desktop `--file`, web
`?mod=`, room-server download with fingerprint), so the engine's normal
definition pipeline (DECORATE, GLDEFS, SNDINFO, sprites — `docs/05`, `docs/13`
M10) applies to lumps inside it, and the fingerprint covers them in netplay.
Mod registration runs after definitions are built (`set_script_mods` after
`set_defs`), so mod classes may use DECORATE classes as parents and `class_find`
sees DECORATE classes.

1. **Packer:** extend `build_mods` in `scripts/room-server.sh` to append, after
   the module lumps, every file in `mods/<dir>/lumps/` (lump name = file stem,
   uppercased, ≤ 8 chars, sorted for reproducibility) and every file in
   `mods/<dir>/sprites/` between sprite namespace markers (check
   `docs/03-vfs.md` for the marker names Room recognises, e.g. `S_START`/`S_END`
   or `SS_START`/`SS_END`). Mirror the change in any other packer that builds
   mod WADs (`scripts/package-web.sh`, docker build) — grep for `ROOMMOD`.
2. **DECORATE** (`lumps/DECORATE`): salvage crate, beacon,
   survivor, elite visual variants with custom states using existing DECORATE
   actions (e.g. `A_SpawnItemEx`, `A_CustomMissile` for boss attack patterns).
   Keep logic in WASM; DECORATE is for visuals/attacks. Classes defined in
   DECORATE must **not** also be registered by the mod (names are global).
   The mod must still run when the lumps are absent (Track A fallbacks via
   `class_find` → null).
3. **GLDEFS** (`lumps/GLDEFS`): `object` light binds for elites (affix colour),
   beacons (pulse), boss (flicker). Lights are
   render-only, not sim → no determinism risk.
4. **SNDINFO + sounds**: announcer voice lines (original recordings only — no
   copyrighted samples), round/elite/objective stingers.
5. **Graphics**: perk card icons and card frames (graphic lumps, used by the
   draft menu `IMAGE` widgets), crate, beacon sprites (original art, GPL-compatible).
6. Licence: add sources/authors of any asset to `THIRD_PARTY.md`.
7. Test: `wasm_mod_survival.rs` loads the WAD built with lumps and asserts the
   DECORATE classes resolve; doom1 E1M1 test still passes.

---

## 9. Engine: on-screen drawing and input for mods ("Mod HUD")

Today a mod can only show text through `player_print` (one line pushed into
`World.messages`, drained by the shell). It cannot draw, cannot read a
player's buttons, and UI-scope modules are skipped at load
(`crates/room-sim/src/script_mods.rs`: `if src.scope != ModuleScope::Play`).
This section specifies what the **main engine** must add so any mod (not only
Survival) can draw HUD elements and menus on screen and let players choose
things with their keys. It replaces the old one-line `hud_message` /
"UI scope last" items of Track C.

Two layers, built in this order:

| Layer | What | Determinism | Covers |
|---|---|---|---|
| **A. Retained Mod HUD** (MH0–MH4) | Play-scope mods create/update/remove **widgets** (text, panels, images, bars, world markers) per seat. The widget set is sim state; shells only draw it. Player choice comes from **input capture**: the mod reads the player's `TicCmd` and the playsim ignores the captured parts. | Fully deterministic, lockstep-safe, saved with the game, **no protocol change** (TicCmds are already synced). | Draft cards, shop, boss bar, timers, objective tracker, Marks counter, combo meter, nameplates, run summary screen. |
| **B. UI-scope modules** (MH5, later) | Per-client modules that draw immediate-mode every frame (`RENDER_OVERLAY`) and receive raw input; choices reach the sim through **network events** carried in the tic stream. | UI module is client-local (non-deterministic); only its network events enter the sim. | Free-form animated UIs, mouse menus, minimaps, damage numbers. |

Layer A is enough for everything in this doc. Layer B is optional polish and
needs a netcode protocol bump.

### 9.1 Engine items (checklist for the engine agent)

| # | Item | Crates / files | Notes |
|---|---|---|---|
| E1 | `ModHud` widget store on `World` | `room-sim/src/world.rs` (new `mod_hud.rs`) | `pub mod_hud: ModHud` next to `messages`. Keyed `(owner_module, seat_or_all, id)`, ordered (`BTreeMap` or sorted `Vec`, never `HashMap` iteration). Caps: 96 widgets per seat, 160 bytes text per widget, 8 KiB text per seat; over-cap calls return an error code, never panic. Each widget stores `created_tic` and `updated_tic` so fades can be interpolated. |
| E2 | Save / load / hot swap | `room-sim` save format (`docs/17`) | Saved with the world. Hot swap and module disable (trap / fuel) **remove that module's widgets and input captures**. Map change (`WorldUnloaded`) clears widgets unless the widget has the `PERSIST_MAP` flag. |
| E3 | Checksum policy | `World::checksum` | Widgets are output only (the sim never reads them). Do **not** include them in the desync checksum. Add a separate `mod_hud_hash()` for tests. |
| E4 | Imports (play scope, appended to `game_fns!`) | `room-script/src/game.rs`, `room-sim/src/script_mods.rs` | `hud_widget_set(seat1, id, params_ptr, params_len) -> i32` (create or replace; `seat1` = 0 for all seats, `n` for seat `n-1`), `hud_widget_patch(seat1, id, field, value_i32)` (cheap per-tic updates: value, alpha, x, y, color, visibility), `hud_widget_remove(seat1, id)`, `hud_clear(seat1)`. Return `0` ok, `<0` error (bad params, cap). |
| E5 | Widget param struct | `room-script-abi/src/lib.rs` (`hud` module) | `#[repr(C)]` little-endian `HudWidgetV1` with a leading `size: u16` so fields can be appended later. See §9.2. |
| E6 | Draw in `room-ui` | new `room-ui/src/mod_hud.rs` | `pub fn draw_mod_hud(hud: &ModHud, seat: u8, ticfrac: f32, view: Option<&ViewProj>, list: &mut UiDrawList)`. Uses the existing `DrawCmd` set (`Text`, `Fill`, `Texture`/`patch`, `Line`) and fonts/colour ranges (`docs/16` §2). Virtual 320×200 space with anchors, widescreen-aware (same scaling rules as the status bar: `hud_scale`, `uiscale`). Fades interpolate with `ticfrac`. |
| E7 | Wire into both shells | `room-app/src/main.rs` (HUD build near the `UiDrawList::new()` per frame), `room-web/src/play.rs` (`build_overlay` and the per-frame ui list) | Draw layers in order: world → `UNDER_HUD` widgets → status bar / messages → `OVER_HUD` widgets → `MODAL` widgets (dims the screen, hides the crosshair while any modal widget exists for the seat) → console. Draw the console player's seat; spectator panes draw their seat. |
| E8 | World markers | E6 + view-projection from the renderer camera | Widget kind `Marker` anchored to an actor (`ActorRef`, follows it with interpolation) or a map point. Off-screen → clamped to the screen edge with an arrow; optional distance label; hidden when `occluded` flag set and no line of sight (use the sight check result computed in the sim once per 7 tics, stored on the widget, so drawing stays read-only). |
| E9 | Input capture | `room-sim/src/think_player.rs` (where `TicCmd` is applied: `weapon_select`, buttons, movement), `room-sim/src/player.rs` | New per-player sim state `input_capture: u32` + `prev_buttons: u32`. Imports: `player_capture_input(slot, mask)` and `player_cmd(slot, out_ptr) -> i32` (writes `PlayerCmdV1`, §9.3). When a bit is captured the playsim zeroes that part of the command **before** applying it, so the player stops switching weapons / firing / moving while the menu is open, but the mod still sees the raw command. Captures are released automatically on death, map unload, module disable, and after `max_capture_tics` (default 35×60) as a safety net. |
| E10 | Key-name tokens in text | `room-ui` text layout + `room-input` bindings | In widget text, `{key:+use}`, `{key:slot 1}`, `{key:weapnext}` are replaced at draw time by the console player's bound key name ("E", "1", "Mouse Wheel Up"). Render-only, so it can differ per client without desync. |
| E11 | Per-seat UI sound | `room-sim` sound events + shells | `player_sound(slot1, name_ptr, len, volume)`: non-positional sound heard only by that seat (menu move / confirm / reward). Uses existing SNDINFO names (`menu/cursor`, `menu/choose`, `menu/invalid` already exist for menus, `docs/16` §3.2). |
| E12 | Tweens (optional) | E1/E6 | `hud_widget_animate(seat1, id, field, target, tics, easing)`; interpolated by the shell, end value stored in the sim so state is exact at any tic. Easing ids: linear, out-quad, out-back (card pop), in-out-sine. |
| E13 | SDK + AssemblyScript | `room-script-sdk/src/{sys_game.rs,game.rs}` + new `hud.rs`; `room-web/as/room.ts` | Builder API: `Hud::text(id).anchor(TopCenter).at(0, 24).font(Big).color("Gold").text("ROUND 5").seat(s).show()`. No allocation (builder writes into a stack buffer). |
| E14 | Editor + docs | web editor Scripts tab "Game API" panel, `docs/12` §15/§16.3, `docs/16` §4 | Document widgets, layers, input capture, limits. Add an example to `mods/doom1-bounty` (AssemblyScript) to prove the AS path. |
| E15 | Tests | `room-sim` unit tests, `room-ui` draw-list tests, `room-game/tests/wasm_mod_hud.rs`, an offscreen golden | Widget set/patch/remove/caps; save→load round trip keeps widgets; capture blocks `weapon_select` and the mod sees it; two-peer lockstep run with captures stays in sync; golden PNG of a draft panel at 320×200 and 16:9. |

### 9.2 `HudWidgetV1` (ABI sketch — finalise in `room-script-abi`)

```rust
#[repr(C)]
pub struct HudWidgetV1 {
    pub size: u16,          // = size_of::<HudWidgetV1>(); host accepts larger (newer) and smaller (older) structs
    pub kind: u8,           // TEXT=0, FILL=1, IMAGE=2, BAR=3, BOX=4 (outline), PANEL=5 (fill+border), MARKER=6
    pub layer: u8,          // UNDER_HUD=0, OVER_HUD=1, MODAL=2
    pub anchor: u8,         // 0..8: TL, TC, TR, ML, C, MR, BL, BC, BR (of the virtual screen)
    pub align: u8,          // text: LEFT/CENTER/RIGHT | wrap flag
    pub font: u8,           // SMALL=0, BIG=1, CONSOLE=2, NEW_SMALL=3
    pub flags: u8,          // HIDDEN, PERSIST_MAP, OCCLUDED_HIDE, SHOW_DISTANCE, PULSE, KEEP_ASPECT
    pub x: i16, pub y: i16, // offset from anchor, virtual 320×200 units
    pub w: i16, pub h: i16, // 0 = natural size; text wraps at w when the wrap flag is set
    pub z: i16,             // order inside a layer (then id)
    pub color: u32,         // RGBA: fill / bar fg / border / image tint
    pub color2: u32,        // RGBA: bar bg / panel border
    pub text_color: u8,     // TEXTCOLO range index (Gold, Red, …); 255 = untranslated
    pub _pad: [u8; 3],
    pub value: i32,         // BAR: current
    pub max: i32,           // BAR: max (≤0 = hidden)
    pub segments: u16,      // BAR: segmented look (0 = solid)
    pub alpha: u16,         // 0..=1000
    pub fade_in: u16, pub hold: u16, pub fade_out: u16, // tics; hold 0 = until removed
    pub actor: u64,         // MARKER: ActorRef (0 = use world_xyz)
    pub world_xyz: [i32; 3],// MARKER: map units (integer, deterministic)
    pub name_off: u16, pub name_len: u16, // IMAGE/MARKER icon: graphic lump or sprite frame ("PINVA0", "M_SKULL1")
    pub text_off: u16, pub text_len: u16, // UTF-8, offsets from the struct start, inside params_len
}
```

Checked-in layout: `room_script_abi::hud::HudWidgetV1` is `#[repr(C, packed)]`, little-endian, `size` = 76. The host reads fields that fit in the guest's `size` and ignores a longer tail; a shorter prefix leaves the remaining fields at defaults (alpha 1000, top-center, y = 24, white, untranslated text). Text and image names live in the same params buffer at `text_off` / `name_off`.

`hud_widget_patch` fields: `VALUE, MAX, ALPHA, X, Y, COLOR, TEXT_COLOR,
HIDDEN, Z`. Text changes go through `hud_widget_set` (replace).

### 9.3 `PlayerCmdV1` and capture mask

```rust
#[repr(C)]
pub struct PlayerCmdV1 {
    pub size: u16, pub weapon_select: u8, pub _pad: u8,
    pub buttons: u32,   // Buttons bits (ATTACK, USE, JUMP, …) as in room-sim/src/player.rs
    pub pressed: u32,   // rising edges this tic (buttons & !prev_buttons)
    pub released: u32,
    pub forward: i16, pub side: i16, pub yaw: i16, pub pitch: i16,
}
pub mod capture {
    pub const WEAPON_SELECT: u32 = 1 << 0; // slot keys 1..7
    pub const WEAPON_CYCLE: u32  = 1 << 1; // weapnext / weapprev
    pub const ATTACK: u32        = 1 << 2;
    pub const USE: u32           = 1 << 3;
    pub const MOVE: u32          = 1 << 4; // forward / side / up
    pub const TURN: u32          = 1 << 5; // yaw / pitch
    pub const JUMP: u32          = 1 << 6;
}
```

Menu idiom: capture `WEAPON_SELECT | WEAPON_CYCLE | USE | ATTACK`; slot keys
pick a card directly, next/prev weapon moves the highlight, Use or Attack
confirms. Movement stays free so the player is never stuck. Because commands
are already synchronised in lockstep, every peer runs the same choice on the
same tic.

### 9.4 Layer B: UI-scope modules + network events (MH5, optional)

1. Stop skipping `ModuleScope::Ui` at load; instantiate per client (not
   lockstep), with the read-only `common` imports plus `draw_text`,
   `draw_fill`, `draw_texture`, `draw_line`, `screen_size`, `font_*`,
   `texture_*` (`docs/12` §5.3 "ui only"), feeding the frame's `UiDrawList`.
2. Events `UI_TICK` (35 Hz, `UiClock`), `RENDER_OVERLAY {ticfrac, w, h}`
   (bits already in `event_mask`), `INPUT_PROCESS` (key/mouse; return 1 to
   consume when registered as `IsUiProcessor`).
3. `send_network_event(name_ptr, len, a1, a2, a3)` (ui) → carried in the next
   `TicCmd` as an optional mod-event record → play-scope `NETWORK_PROCESS
   {player, name, a1..a3}` on every peer at the same tic. Needs: `room-sim`
   `TicCmd`, `room-net` / `room-proto` `NetTicCmd` (currently fixed
   `NET_TIC_CMD_LEN = 17`) → protocol version bump, demo format bump
   (`docs/17`), server relay limits.
4. Play → UI data: UI modules cannot read play-module memory. Add
   `hud_var_set(seat1, key, i32)` (play) / `hud_var_get(seat, key)` (ui) so a
   play mod publishes numbers (Marks, round, choice ids) the UI module draws.
5. Fuel budget per frame for UI modules; a trapping UI module is disabled on
   that client only (no desync since it has no sim access).

### 9.5 Mod HUD milestones (engine track)

| ID | Scope | Acceptance |
|---|---|---|
| **MH0** | E1–E3: widget store, save/load, hot-swap cleanup, caps | `room-sim` unit tests; save round trip keeps widgets; trap removes the module's widgets. |
| **MH1** | E6–E7: draw in `room-ui`, wire desktop + web, layers, anchors, widescreen | Draw-list tests; offscreen golden of a test panel at 320×200 and 1920×1080; web build passes. |
| **MH2** | E4–E5, E13, E14 (partial): imports + SDK builder + AS mirror + docs | `room-game/tests/wasm_mod_hud.rs`: a WAT/Rust test mod creates, patches and removes widgets; AS fixture compiles via `scripts/asc-compile.mjs`. |
| **MH3** | E9–E11: input capture, `player_cmd`, key tokens, per-seat sound | Headless: captured `weapon_select` does not switch weapons and the mod reads it; two-peer lockstep run with captures has equal checksums; capture released on death. |
| **MH4** | E8, E12: world markers + tweens | Marker follows an actor and clamps off-screen (draw-list test); tween end state exact after N tics. |
| **MH5** | §9.4 UI-scope modules + network events (optional) | Protocol/demo version bumped; two clients exchange a network event and stay in sync; UI-module trap is client-local. |

MH0–MH3 block Survival milestone SV4 (draft UI). MH4 blocks markers/nameplates
in SV5–SV6. MH5 is not required by this doc.

---

## 10. Track C — other engine / ABI extensions (prioritised)

On-screen drawing and input moved to §9 (Mod HUD). The same procedure applies to
the §9 imports.

Procedure for **each** new import (ABI stays v1: **append only**, never reorder):
1. Append a variant at the **end** of `game_fns!` in `crates/room-script/src/game.rs`
   with a doc comment (signature, scope).
2. Implement it in the `match` in `crates/room-sim/src/script_mods.rs`;
   invalid handles → no-op/default; never panic; deterministic.
3. Add constants (selectors) to `crates/room-script-abi/src/lib.rs`.
4. Add the raw import to `crates/room-script-sdk/src/sys_game.rs` and a safe
   wrapper in `game.rs`; mirror in `crates/room-web/as/room.ts`.
5. Unit test in `room-sim` + e2e assertion in a `room-game/tests/wasm_mod_*.rs`.
6. Update `docs/12` §15 (import count/list) and §16.3 tables.

| # | Import(s) | Purpose | Notes |
|---|---|---|---|
| C2 | `actor_flag(a, FlagId) -> i32`, `actor_set_flag(a, FlagId, on)` | Per-actor INVULNERABLE (boss phases), NOPAIN, FRIENDLY, DONTDRAW, NOGRAVITY | Reuse `class_flag` ids. |
| C3 | `player_set_i32(slot, prop, v)` with `DAMAGECOUNT`, `BONUSCOUNT`, `EXTRALIGHT` | Screen flashes (reward gold, phase red) | Cheapest "juice" win. |
| C4 | `actor_quake(a, intensity, duration, damrad, tremrad)` | Boss slams, horde rush start | GZDoom `A_Quake` semantics; render-side offset must not affect sim except as GZDoom does. |
| C5 | `actor_radius_attack(a, source, damage, distance)`, `actor_spawn_missile(src, dest, class) -> ActorRef` | Volatile affix, boss patterns without DECORATE | Doc §5.3 already names both. |
| C6 | Render `Translation` + `actor_set_translation(a, name)` | Elite tinting | `docs/12` §15: translation stored but not rendered. |
| C7 | Module memory in save games for game-control mods | Director survives save/load | `docs/12` §10/§15; host snapshot exists for M11 path (`scripts.bin`). |
| C8 | Events `PLAYER_RESPAWNED`, `WORLD_THING_DESTROYED` | Re-apply perks reliably; clean tracked lists | Append event_mask bits. |
| C10 | `point_in_sector` + `sector_set_i32(LIGHT_LEVEL)` | Arena mood: lights dim during Horde Rush, flash on boss phases | Movers/specials interplay — restore original light at round end. |

---

## 11. Milestones and acceptance

| ID | Scope | Acceptance |
|---|---|---|
| **SV0** | Refactor into §7 layout + `Host` trait + `FakeHost`; behaviour parity | Existing `wasm_mod_survival` tests pass unchanged; new native unit tests for spot selection & tuning; wasm size noted in README. |
| **SV1** | Coop correctness (§6.9), spot graph + learning + stuck recycling (§6.1), music restore fix | Test: 2-player headless game, player 0 dead → waves continue; sealed-closet fixture map → stuck monster recycled within 15 s and round ends; determinism checksum test still equal. |
| **SV2** | Director phases, threat budget, intensity, archetypes (§5, 6.2, 6.3) | Native sim over 20 rounds × 5 skills × {1,2,4} players: alive threat never exceeds cap; peak→fade observed; e2e: phase messages appear in order on `arena()` map. |
| **SV3** | Elites + announcer queue + sound/music cues (§6.4, 6.8) | Test each affix with FakeHost; e2e: an elite class spawns by round 3 on skill 4; no more than one team message per 1.5 s. |
| **SV4** | Marks, combos, draft menu, perks, HUD panels (§6.5, 6.8) — **needs MH0–MH3** | e2e: at intermission a MODAL draft (3 cards) exists for each alive seat (`mod_hud` query); a scripted `TicCmd` with `weapon_select = 2` applies the 2nd perk, removes the panel and releases capture; timeout auto-picks; weapon did not switch while the menu was open; 2-seat lockstep run stays in sync. |
| **SV5** | Objectives + tracker/markers, finale/extraction, run summary panel, map carry-over (§5, 6.6, 6.10) — markers need MH4 | e2e: forced short run (`final_round = 2` via test tuning) reaches extraction and calls `exit_level`; summary panel widgets exist after death. |
| **SV6** | Phased bosses + boss bar (§6.7) | e2e: Warden phase transitions at 66/33 % (drive damage via `damage()` in a test hook); boss bar value tracks health. |
| **SV7** | Track B packer + DECORATE/GLDEFS/SNDINFO/sprites (§8) | WAD contains lumps; classes resolve; mod still works with lumps stripped; room-server serves and clients verify fingerprint. |
| **SV8** | Track C C2–C5 (+C8) | Each import has unit + e2e test; AS mirror compiles (`node scripts/asc-compile.mjs testdata/mods/hello_as.ts …`); `docs/12` updated. |
| **SV9** | C6/C7/C10; optional MH5 polish; balance pass; docs | Save/load mid-round restores director; README rewritten; `docs/21` log entries. |

Never start SV(n+1) before SV(n) is green. The engine track MH0–MH4 (§9.5)
can run in parallel with SV0–SV3; SV4 waits for MH3.

---

## 12. Testing and verification

- **Mod HUD:** native tests of `ui.rs` against `FakeHost` (records widgets and
  replays `PlayerCmdV1` sequences); e2e asserts on `World.mod_hud`; one
  offscreen golden of the draft panel (`room-ui` draw list → PNG) at 4:3 and 16:9.
- **Native logic tests** (fast): `cargo test --manifest-path mods/doom1-survival/Cargo.toml`.
- **E2E:** `cargo test -p room-game --test wasm_mod_survival` (uses `Headless`,
  `set_script_mods`, an `arena()` test map, and `doom1.wad` E1M1 when present).
  Add fixtures: two-player arena, sealed-closet map, extraction map.
- **Determinism:** keep `survival_mod_is_deterministic` (two runs, equal
  `world().checksum()`); extend to 2 players and 6000+ tics with drafts.
- **Budget safety:** a soak test running 20 000 tics on skill 5 must not log
  "disabled" (trap/fuel) — assert like the existing `!m.contains("disabled")`.
- **Balance harness:** native sim with `FakeHost` prints a CSV (tic, phase,
  alive_threat, intensity) per preset into `target/`; not part of CI asserts
  beyond the caps, but used for tuning.
- **Manual:** `./scripts/room-server.sh mods` then
  `cargo run -p room-app --release -- --iwad testdata/doom/doom1.wad --file mods/doom1-survival/survival.wad --map E1M1 --play --skill 4`,
  web `?skill=4&mod=survival`, and a 2-seat room via `./scripts/room-server.sh start`.

---

## 13. Initial tuning (starting numbers, expect to change)

| Param | ITYTD | HNTR | HMP | UV | NM |
|---|---|---|---|---|---|
| skill_mul (%) | 60 | 80 | 100 | 130 | 170 |
| Intermission (s) | 25 | 22 | 20 | 18 | 15 |
| Heal per kill | 5 | 4 | 3 | 2 | 1 |
| Elite chance per group (%) | 5 | 10 | 15 | 22 | 30 |
| Objective chance (%) | 40 | 50 | 50 | 55 | 60 |
| Final round | 10 | 10 | 15 | 15 | 20 |
| Adaptive assist | on | on | on | on | off |
| Marks multiplier (%) | 120 | 110 | 100 | 100 | 90 |

---

## 14. Definition of done

- All SV and MH0–MH4 milestones accepted; the CI commands in §0 pass.
- `mods/doom1-survival/README.md` describes the run loop, perks, objectives,
  controls, and Track B/C requirements; `room.toml` description updated;
  `version = "0.2.0"` (manifest + Cargo).
- `docs/12` §15/§16.3 and `docs/16` §4 updated for the Mod HUD and any new import; `docs/21` gains an SV table
  and dated log lines; `THIRD_PARTY.md` lists any asset.
- The mod runs on: shareware doom1 (E1M1–E1M9), Freedoom 1/2, Doom 2 MAP01,
  solo and 2–4 player room-server coop, without desync.

## 15. Open questions for the owner

1. Finale: extraction then next map (campaign run) or endless on one map?
2. Should coop deaths be permanent during the finale (`RESPAWN = 0`)?
3. Track B assets: who provides announcer voice and perk card icons (or generate
   placeholder art)?
4. Are the Mod HUD (§9) and Track C allowed to touch `room-sim` / `room-ui` /
   shells now, given M14 hardening is the engine's active milestone? Should the
   Mod HUD get its own engine milestone in `docs/21`?
5. Face-swap tie-in (`docs/26`): use a player's captured face on the boss as a
   "Nemesis" option?
6. Is Layer B (UI-scope modules + network events, a netcode protocol bump)
   wanted at all, or is the retained Mod HUD enough?
