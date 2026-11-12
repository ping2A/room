# 28 — Battle Royale 2.0: "AAA" upgrade of `mods/doom1-royale`

**Audience:** a coding agent implementing this in the Room repo. Read `AGENTS.md`,
`docs/02-conventions.md` (§Determinism), `docs/12-wasm-scripting.md` §15–16,
`docs/22-room-server.md` (rooms, mods, spectators, `result`/standings) and
**`docs/27-survival-director-aaa.md` §9 (Mod HUD)**, which this document builds on.
**Status:** **BR0**–**BR9** + **BE1**–**BE12** shipped (`docs/21` §0).
**BR9** deferrals: **S-BR2** OpenSkill / weekly reset. **S-BR1** /
**S-BR2** (MVP) / **S-BR4** shipped (`docs/21`, `docs/22` §match end).

"AAA" here means: a match that feels authored from lobby to victory screen —
a pre-game, a deliberate drop, a readable shrinking circle you can *see*, loot
you want, tension that peaks at the final circle, clear information about who
is left and who killed you, a satisfying victory/elimination moment, a reason
to queue again (squads, ranks, stats), and a room that is always full (bots).
It must stay deterministic and lockstep-safe: every seat runs the full sim.

---

## 0. TL;DR for the implementing agent

1. Prerequisite: the **Mod HUD** engine track MH0–MH4 from `docs/27` §9
   (widgets, input capture, world markers). Most of the visible upgrade depends
   on it. Also reuse doc 27's Track C items C2 (per-actor flags), C3 (screen
   flash), C4 (quake), C5 (radius attack / missiles), C7 (module memory in saves).
2. Refactor `mods/doom1-royale/src/lib.rs` (871 lines, one file) into modules
   with the same `Host` trait / `FakeHost` pattern as doc 27 §7.
3. Rebuild the match flow: **Pre-game lobby → Drop selection on a map →
   Deploy → Loot → Zone phases with next-circle preview → Final circle →
   Victory / standings** (§5).
4. Add BR-specific engine features (§9): visible **zone wall + ground ring**,
   a **minimap / map widget**, **player names**, **bindable mod buttons**
   (ping, map), **teams** with friendly-fire rules, a **`BUDDHA`-style downed
   state** for squads, a **per-seat camera** (spectate after death), **8 seats**
   on any map, and optional **deterministic bots** to fill rooms.
5. Server (§10): match results → **stats, ratings, leaderboards** in the lobby;
   squad parties; map pool / vote. No sim impact.
6. Every milestone ends green on: `cargo fmt --all`, `cargo clippy --all-targets -- -D warnings`,
   `cargo test --workspace`, `cargo build --target wasm32-unknown-unknown -p room-web --features browser`,
   `./scripts/room-server.sh mods`, `cargo test -p room-game --test wasm_mod_royale`,
   `cargo test -p room-server --features verifier --test verifier_headless`.

---

## 1. File map

| Path | Role |
|---|---|
| `mods/doom1-royale/src/lib.rs` | The mod (Rust, `room-script-sdk`, `no_std`, no alloc). Has unit tests for `zone_at` / `zone_damage`. |
| `mods/doom1-royale/room.toml` | Manifest; `solo = false` hides it from lobby "Play solo". Module lump `ROYALE`. |
| `mods/doom1-royale/README.md` | Player-facing description — rewrite at the end. |
| `crates/room-game/tests/wasm_mod_royale.rs` | E2E (monsters cleared, fists only, DM spawns, guns with ammo, PvP, no respawn, zone, winner + `end_match`, time limit, solo practice, determinism, doom1 E1M1). Asserts message strings — update deliberately. |
| `crates/room-server/tests/verifier_headless.rs` | Server verifier runs the mod headless (must keep passing). |
| `crates/room-sim/src/player.rs` | `MAX_PLAYERS = 8`, `TicCmd`, `Buttons` (bits 0–8 used: ATTACK…INVUSE). |
| `crates/room-sim/src/think_player.rs` | Where `TicCmd` is applied (input capture hooks, doc 27 E9). |
| `crates/room-sim/src/world.rs` | `World.messages`, `kill_events`, deathmatch flags; `checksum()`. |
| `crates/room-sim/src/script_mods.rs` | Host side of game imports. |
| `crates/room-web/src/play.rs` | Web shell: overlay (`build_overlay`: kill feed, spectator panes, Tab scoreboard), spectator grid / follow mode. |
| `crates/room-app/src/main.rs` | Desktop shell HUD/overlay. |
| `crates/room-ui/src/automap.rs` | Automap drawing (base for the minimap widget). |
| `crates/room-server` / `docs/22` | Rooms, lobby, `result` standings, spectators (frame-log replay), SFU voice, admin. |

---

## 2. Audit of the current mod

### 2.1 What it does well (keep)
- Monsters removed; their spots + unused player starts become loot spots.
- Tiered loot by distance to the zone centre (INNER / MIDDLE / OUTER weighted
  tables), guns spaced ≥ 384 units, rocket cap ⌈players/2⌉, chaingun cap,
  every gun with ammo, a shotgun near each player, forced rocket launcher.
- Fists-only start, players spread over deathmatch starts (farthest-first).
- `DEATHMATCH = 1`, `RESPAWN = 0`, frag heals +25.
- Zone: 45 s grace, 25 s hold / 20 s shrink to 60 %, min radius 160, damage
  `1 + 2·phase` per second (max 25), private direction hints.
- Winner / draw / time-limit resolution, `end_match`, solo practice.
- Zone logic once per second. Deterministic and tested.

### 2.2 Gaps vs. a AAA battle royale
1. **The zone is invisible.** Players only get text ("Run NE, 412 units").
   No wall, no ground ring, no map, no timer, no next-circle preview.
2. **Zone centre is fixed** for the whole match (chosen once near the map
   middle); circles are concentric → predictable, no rotations.
3. **No pre-game, no drop.** Players are teleported to a spot on entry.
   No choice, no landing tension.
4. **Players are "Player N"** in mod text (the mod cannot read callsigns).
5. **Feedback is chat lines**: no kill feed styling, no "players left"
   counter, no damage indicators, no elimination / victory screen (only the
   generic standings when the match ends).
6. **Death = black screen until the end** (the seat stays dead); no
   spectate-your-killer, no death recap.
7. **No squads**, no revive, no pings.
8. **Loot is flat**: Doom items only, no rarity readability, no supply drops,
   no reason to move except the zone.
9. **Room size 2–4** in practice (Doom coop has four player starts); the sim
   supports 8 seats.
10. **No match-to-match progression** (stats, ranks, leaderboards).
11. **Empty rooms**: no bots, so a 2-player room is a duel.

---

## 3. Hard constraints

Same as `docs/27` §3 (determinism, `no_std`, fixed arrays, fuel budgets,
512-actor query caps, event cascade caps, shareware availability, append-only
ABI). BR-specific:

- **Lockstep fairness:** every seat sees the same zone, loot, drops and bot
  decisions because they are sim state. Anything per-client (UI-scope modules,
  minimap rendering, sounds, key names) must never feed back into the sim.
- **Spectators** replay the frame log from tic 0 (`docs/22`): all mod state
  must derive from sim inputs; no server-provided runtime data enters the mod.
- **Server verifier** runs the mod headless: no shell-only imports in the play
  module (Mod HUD widgets are sim output and fine; they are simply not drawn).
- **8 seats max** (`MAX_PLAYERS`); design for 2–8 humans + bots, solos or
  squads of 2 (4 squads) / 4 (2 squads).

---

## 4. Design pillars

| Pillar | Requirement | Section |
|---|---|---|
| Readable circle | Zone visible in the world (wall + ground ring), on the minimap/map, with timer, next circle and distance. | 6.3, 9 BE1–BE2 |
| Meaningful drop | Pre-game lobby, then each player picks a landing point on the map; landing has a short vulnerable window. | 5, 6.1 |
| Loot worth fighting for | Rarity tiers you can read at a glance, supply drops, vaults, hot zones. | 6.2 |
| Information | Names, alive counter, kill feed, damage direction, hit markers, death recap, pings. | 6.5–6.6 |
| Moments | Final circle, "3 left", last duel; elimination and victory screens; killcam-lite (spectate killer). | 6.7 |
| Teams | Squads, friendly-fire rules, downed/revive, team pings, shared map. | 6.4 |
| Full rooms | Deterministic bots fill empty seats. | 9 BE8 |
| Reasons to replay | Stats, ratings, leaderboards, seasonal map pool. | 10 |

---

## 5. Match flow

```
Loaded
 └─ PREGAME (≤ 45 s, or until all humans Ready + 10 s countdown)
      - everyone spawns at the normal starts; DEATHMATCH = 0 (no damage), fists only
      - Mod HUD: "WAITING FOR PLAYERS 5/8", roster with names, squad colours, ready state
      - Ready = USE held 1 s (input capture USE); bots auto-ready
 └─ DROP SELECT (15 s, MODAL map widget, BE2)
      - full-screen map with the first circle and loot hot zones highlighted
      - cursor moved with movement keys / mouse-yaw deltas; confirm with USE or ATTACK
      - squads see teammates' picks; squad leader's pick is the default for the squad
      - no pick → random valid spot inside the first circle's outer half
 └─ DEPLOY (≈ 3 s)
      - each player is teleported to a spawnable point near their pick
        (nearest valid spot from the spot graph, §6.1), with telefog
      - "landing shield": DAMAGE_FACTOR 0 for 3 s or until they pick up a weapon/attack
 └─ LOOT (grace, 60 s; scales with map size)
      - zone visible, timer to first shrink; supply drop #1 announced at 40 s
 └─ ZONE PHASES 1..N (§6.3): HOLD (next circle shown) → SHRINK → …
      - supply drops at phases 2 and 4 (§6.2); "HUNTED" ping at ≤ 3 alive (§6.7)
 └─ FINAL CIRCLE: last phase shrinks to 0 over 60 s (no stalemates)
 └─ RESOLUTION
      - last squad / player standing → VICTORY screen (MODAL, all seats) 6 s
      - standings panel (place, kills, damage, survival time, revives)
      - game::end_match(winner) → shell standings / server result (unchanged contract)
```

Time-limit rule (room setting) keeps today's behaviour: best survivor by
kills, then damage dealt, then health.

---

## 6. Systems (mod side)

### 6.1 Spot graph and drop resolution (`spots.rs`)
Reuse doc 27 §6.1 (collection from `level_things` for monster/player/DM
starts + items, height check, 64-unit dedupe, `MAX_SPOTS = 256`). Add:
- **Map bounds** (min/max of spots) for the map widget projection.
- **Drop resolve:** for a picked map point, choose the nearest spot within
  512 that no other player/squad took within 256 (squads may share). Fallback:
  nearest free spot anywhere.
- **Loot clusters:** group spots by 384-unit proximity → "areas"; areas with
  many spots become named **hot zones** (`Area 3`, or MAPINFO level names are
  not per-area, so use compass + index: "NE Yard").

### 6.2 Loot (`loot.rs`)
- Keep the tiered INNER/MIDDLE/OUTER logic and caps; tiers are recomputed
  against the **first** circle centre (zone now moves, §6.3).
- **Rarity** (common / rare / epic / legendary) as registered subclasses of
  Doom items (`RareShotgun : Shotgun`, `EpicArmor : GreenArmor`, …) so they are
  identifiable in `ItemPickup` and visually distinct: `SCALE_PERCENT`, and in
  Track B a GLDEFS light colour per rarity + a ground-glow sprite.
  Effects implemented by the mod on pickup (no new engine damage hooks needed):
  - weapons: rarity grants bonus ammo on pickup and a **damage amp** tag on the
    player for that weapon class (applied in `ThingDamaged` when
    `source == player` by dealing an extra `damage × amp%` with a re-entrancy
    guard; when engine exposes the ready weapon (BE10), restrict it to that weapon);
  - armor: `BlueArmor` at epic, extra armor bonuses at legendary;
  - health: overheal cap 150/200 for epic/legendary medikits.
- **Supply drops:** at fixed times, a crate actor (registered subclass of a
  solid decoration) spawns at a random spot inside the *next* circle, announced
  with a world marker and map icon 15 s before it lands ("SUPPLY DROP INBOUND").
  On "landing" (crate spawn with telefog + `misc/teleport`), it holds a
  legendary weapon + armor; opening = USE within 64 units for 2 s (input
  capture of USE while in range; progress bar widget), then items spawn.
- **Vault:** one per match in a hot zone: legendary loot behind a timed lock
  (open after phase 2), announced on the map.
- **Death loot:** on a player's death, spawn their weapons (one ammo pack per
  weapon type they had) + armor as a cluster where they fell ("loot box"),
  marked on the killer's HUD for 10 s.

### 6.3 Zone 2.0 (`zone.rs`)
- **Moving circles:** phase k picks `next_center` uniformly (via `random`)
  inside the current circle so that the next circle is fully contained:
  `|next_center − center| ≤ r_k − r_{k+1}`; bias toward spots-dense areas so
  the end is never in a void. Shrink interpolates centre and radius together.
- **Schedule table** (scaled by map size `r0` and player count):

| Phase | Hold (s) | Shrink (s) | Radius → | Damage/s outside |
|---|---|---|---|---|
| 1 | 60 | 40 | 60 % | 2 |
| 2 | 45 | 30 | 55 % | 4 |
| 3 | 35 | 25 | 50 % | 7 |
| 4 | 25 | 20 | 45 % | 11 |
| 5 | 20 | 20 | 40 % | 16 |
| Final | 10 | 60 | 0 | 25 |

- Integer/fixed-point timing; geometry with `game::sqrt` only (deterministic).
- **Publishing the zone for rendering:** each second (and on phase change) the
  mod calls BE1 `zone_set(...)` with current centre/radius and next
  centre/radius; the engine renders the wall and rings and the map widget
  draws both circles. Without BE1 the mod still works (text + HUD).
- **HUD:** top-centre "ZONE CLOSING 0:24" / "NEXT ZONE 0:41" with a bar;
  distance-to-safe and arrow when outside (world `MARKER` at the nearest
  safe edge point); red vignette + zone sound while outside (C3 damagecount
  is applied naturally by damage).

### 6.4 Squads (`teams.rs`)
- Modes from the room settings string (mod reads a rule / BE5 `game_mode`):
  **Solo**, **Duos**, **Squads of 4**. Seat → team assignment by the server
  (parties, §10) or round-robin.
- Friendly fire off between teammates (BE5 `TEAM_DAMAGE` rule), team colours
  on names, markers and minimap.
- **Downed / revive (DBNO):** players get the per-actor `BUDDHA` flag (BE6) so
  lethal damage leaves 1 HP. The mod sees `ThingDamaged` with `health == 1` →
  **downed**: capture `ATTACK | WEAPON_SELECT | WEAPON_CYCLE`, speed ×0.3,
  red "DOWNED" banner, 30 s bleed-out bar (−1 HP… implemented as a timer, then
  remove `BUDDHA` and deal lethal damage, crediting the downer). A teammate
  holding USE within 64 units for 4 s revives (progress bar on both screens) →
  30 HP, release capture. Whole squad downed → all eliminated.
  Solo mode: no buddha, normal death.
- **Pings:** bindable mod button `+mod1` (BE4): trace from the player's view
  (BE7) → place a team `MARKER` ("Enemy" if an enemy actor is hit, item name
  if an item, else "Go here"), 8 s, max 2 per player, with a team-only sound.
- Squad HUD: teammate list (name, health bar, downed state) bottom-left.

### 6.5 Information & HUD (`ui.rs`, all Mod HUD widgets, doc 27 §9)

| Widget | Where | Content |
|---|---|---|
| Alive counter | top-right | "ALIVE 6 · KILLS 2" (squads: "TEAMS 3") |
| Zone timer | top-centre | state + time + bar |
| Minimap | top-right under counter (BE2) | local area, zone circles, pings, teammates, supply drops |
| Kill feed | right, OVER_HUD | "Killer [weapon icon] Victim" lines with names and team colours, 5 s each, max 5 |
| Elimination toast | centre-low | "ELIMINATED Bob — 145 dmg" on your kill (+ C3 flash) |
| Hit marker | crosshair | brief X on damage dealt (from `ThingDamaged` where source is you), red on kill |
| Damage direction | screen edge | arc toward the attacker for 1 s (marker kind with `actor` = attacker, edge-only flag) |
| Squad panel | bottom-left | teammates' names / health / downed |
| Interaction prompt | centre | "Hold {key:+use} to revive / open" + progress bar |
| Full map | MODAL on `+mod2` (BE4) | big map widget: circles, drops, pings, teammates; movement still allowed |

Names come from BE3 `player_name`. Text messages via `print` are reduced to
system lines.

### 6.6 Death experience (`death.rs`)
- On elimination: MODAL panel for that seat for 4 s — "YOU PLACED #4",
  killed by (name, weapon, distance, remaining HP of the killer), **death
  recap** (top 3 damage sources in the last 15 s from a per-seat ring buffer
  of `ThingDamaged`), then switch to spectating.
- **Spectate:** BE9 `player_set_camera(slot, actor)` → camera follows the
  killer (then the next alive player; `+mod1`/`ATTACK` cycles, captured). Squad
  members spectate teammates first. The dead seat keeps seeing the zone HUD,
  alive counter and kill feed.
- "Leave match" key hint (shell action; leaving keeps standings).

### 6.7 Moments (`moments.rs`, `announcer.rs`)
- Phase banners ("ZONE 3 — CLOSING"), "SUPPLY DROP INBOUND", "TOP 5", "3 LEFT",
  "FINAL CIRCLE", "LAST DUEL: Alice vs Bob" with stingers (existing SNDINFO
  names; Track B adds announcer lines).
- **Hunted:** at ≤ 3 alive, the player with most kills is revealed on everyone's
  map for 5 s every 30 s (anti-camping).
- **Music intensity** by alive count / phase via `set_music` (shareware:
  map track early, `D_E1M8` in the final circle; Doom 2 maps get stronger
  tracks).
- **Victory screen:** MODAL all seats, winner name(s) huge, "LAST ONE STANDING",
  their kills/damage, confetti-style particles (Track B DECORATE particle spawner
  around the winner, render-only), then standings, then `end_match`.
- **Practice mode** (1 human): bots (BE8) if available, else today's practice.

### 6.8 Scoring for standings
Per seat: place, kills, assists (damaged a victim within 10 s of their death),
damage dealt, revives, survival time. Sent to the shells through `end_match`
standings — extend BE11 so the mod can attach these fields to the `result`
message (server stores them, §10).

---

## 7. Code architecture

```
mods/doom1-royale/src/
  lib.rs        export_mod!, event routing
  host.rs       Host trait + SdkHost (+ FakeHost in tests), same pattern as doc 27 §7
  fixed.rs text.rs   fixed-capacity containers, Msg / say macros
  tables.rs     loot tables, rarity, zone schedule, timings per player count
  match_flow.rs Phase machine: Pregame, DropSelect, Deploy, Loot, Zone(k, hold|shrink), Final, Resolution
  spots.rs      spot graph, bounds, areas, drop resolve
  loot.rs       placement, rarity, supply drops, vault, death loot
  zone.rs       moving circles, damage, zone_set publishing   (keep zone_at-style pure fns + unit tests)
  teams.rs      squads, downed/revive, pings
  combat.rs     damage tracking ring buffers, assists, hit markers, damage amp
  death.rs      elimination panel, recap, spectate camera
  moments.rs    hunted, final circle, last duel, music
  ui.rs         all Mod HUD widgets and menus (pregame roster, drop map, full map, victory)
  bots.rs       (only if BE8 is not engine-side) — see §9 BE8
```

Per-tic budget: inputs/capture every tic for players in menus or interactions;
zone + damage once per second (as today); HUD patches every 5 tics; damage
ring buffers are fixed `[Hit; 32]` per seat.

---

## 8. Track B — content in `royale.wad`

Same packer extension as doc 27 §8 (`mods/<dir>/lumps/`, `sprites/`).
- **DECORATE:** rarity glow actors (render-only `+NOINTERACTION` sprites that
  follow items), supply crate, vault door actor, victory particles.
- **GLDEFS:** rarity colours (white/blue/purple/gold), supply drop beacon
  (pulse), zone-edge lights are not needed if BE1 renders the wall.
- **SNDINFO + sounds:** zone warning hum, phase stingers, supply drop, kill
  confirm, downed, revive, victory jingle (original audio).
- **Graphics:** map icons (drop, vault, ping types), weapon icons for the kill
  feed (or reuse `SHOTA0`/`MGUNA0`/`LAUNA0`/`CSAWA0`/`PUNGA0` sprites), rarity
  frames.
- **Optional dedicated map** (`ROYALE01`, UDMF, Freedoom-textured, GPL): a
  large open map with ≥ 16 DM starts, named areas and good sightlines — Doom
  E1 maps are corridors; a purpose-built map is the single biggest "AAA" win.

---

## 9. Engine features (BR-specific, beyond doc 27 §9 Mod HUD)

Same rules as doc 27 §9/§10: append-only imports in `game_fns!`, host impl
in `room-sim/src/script_mods.rs`, ABI constants, SDK + `room.ts` mirrors,
tests, docs. Render-only data lives in sim output (not in the checksum, saved).

| # | Feature | Imports / data | Crates | Notes |
|---|---|---|---|---|
| BE1 | **Zone rendering** | `zone_set(id, cx, cy, r, next_cx, next_cy, next_r, rgba, flags)`, `zone_clear(id)`; `World.mod_zones` (≤ 4) | `room-sim` (store), `room-render` (wgpu: translucent animated cylinder wall from floor to ceiling heights, clipped by map geometry via depth; ground ring decal / line), soft renderer fallback (screen tint when outside + ring on automap only) | Interpolate radius/centre between updates with `ticfrac`. Wall visible through walls optionally (`flags`). Must not affect sim. |
| BE2 | **Map widget** (minimap + full map) | Mod HUD widget kind `MAP` (`HudWidgetV1` kind 7): rect, scale, `follow_seat`, rotate flag, layers mask (geometry, zones, markers, teammates, items-of-interest) | `room-ui/src/automap.rs` reused to draw lines into a clipped rect; zones from BE1; markers from doc 27 E8 | Shows only what the seat is allowed to see: own position, teammates, markers addressed to that seat/team — never enemy positions unless a marker exposes them (Hunted). |
| BE3 | **Player names** | `player_name(slot, buf, cap) -> len`; deterministic: names are part of the session setup broadcast to every seat before tic 0 (room roster) and stored in the world | `room-sim` (session players), `room-net`/`room-web` (feed roster callsigns into the session at start), `room-app` (local name cvar) | Spectators replay with the same names (frame log + roster). Fallback "Player N". |
| BE4 | **Bindable mod buttons** | `Buttons::MOD1..MOD4` = bits 16–19 of `TicCmd.buttons` (already `u32` on the wire: **no protocol change**); console commands `+mod1..+mod4`, default binds Q (ping), M/Tab-hold (map), G, Z; `player_cmd` (doc 27 E9) exposes them; menu "Mod controls" with labels a mod can set via `reg_button_label(n, text)` | `room-sim/src/player.rs`, `room-input`, `room-ui` menus, web key handling | Ignored by the playsim; only mods read them. |
| BE5 | **Teams & damage rules** | `player_set_team(slot, team)` / `player_team(slot)`; rules `TEAM_DAMAGE` (0 = off), `SELF_DAMAGE`; `game_mode` read of room options string (`room_option(key)`) | `room-sim` damage path (`damage.rs`), scoreboard (team grouping), `docs/22` room create options | Team colour translation for player sprites if C6 (translation rendering) lands. |
| BE6 | **Per-actor flags incl. `BUDDHA`** | doc 27 C2 `actor_set_flag`, with `BUDDHA` (health floors at 1, GZDoom semantics) and `NOPAIN`, `INVULNERABLE`, `DONTDRAW` | `room-sim/src/damage.rs` | Downed state, landing shield, spectator body hiding. |
| BE7 | **View trace** | `player_trace(slot, max_dist, out_ptr) -> kind` → hit point, hit actor, hit line/sector; uses the player's view angle/pitch | `room-sim` (hitscan code in `hitscan.rs` without damage) | Pings, interaction targeting (open crate, revive). Deterministic. |
| BE8 | **Deterministic bots** (optional, large) | Server/lobby marks empty seats as `bot`; a `room-sim` bot brain generates that seat's `TicCmd` **inside the sim** each tic (from world state + playsim RNG), so every peer computes the same commands. Mod hooks: `bot_set_goal(slot, x, y)` / `bot_set_aggression(slot, n)` | new `room-sim/src/bot.rs` (navigation over the blockmap/BSP: waypoints from the mod's spot graph via `bot_add_waypoint`, line-of-sight aim with reaction delay, pickup seeking, zone awareness via goal), `room-server` seat type, lobby "fill with bots" | Bots must be cheap (≤ 0.2 ms/tic each) and deterministic across x86/arm/wasm (no float transcendental from std; use `room_core::dmath`). Spectator replay works since bots are sim. |
| BE9 | **Per-seat camera** | `player_set_camera(slot, ActorRef | 0)`: sim state like GZDoom `camera`; shells render that seat's view from the camera actor (with its HUD hidden or the followed player's HUD) | `room-sim/src/player.rs` (camera field, saved), `room-app`, `room-web` view selection | Used for spectate-after-death; spectator clients keep their own follow mode. |
| BE10 | **Player weapon / stats queries** | `player_get_i32` new props: `READY_WEAPON` (ClassRef), `DAMAGE_DEALT`, `ARMOR_TYPE`; `actor_get_i32 LAST_DAMAGE_TYPE` | `room-sim` | Kill feed weapon icons, damage amp per weapon, standings. |
| BE11 | **Rich match result** | `game_end_match_ex(winner_team, standings_ptr, n)` with per-seat `{place, kills, assists, damage, revives, survival_tics}` | `room-sim` world output → shells' standings screen + `result` message to the server (`docs/22`) | Server majority check compares the same struct. Keep `game_end_match` working. |
| BE12 | **8 seats on any map** | when a map has < N player starts, extra seats spawn at DM starts (then at player 1 start with an offset + telefog) instead of failing | `room-sim` player spawning, `room-server` max seats per mod (`room.toml` `max_players = 8`) | Removes the 2–4 cap. |
| BE13 | **Positional voice for proximity chat** (optional) | none in sim: web client already knows positions (lockstep) → `PannerNode` per remote seat (`docs/22` §SFU notes); team channel always, proximity for enemies (toggle) | `room-web` `voice.js` | Pure client feature; server moderation unchanged. |

Prioritise BE1, BE2, BE3, BE4, BE9 (core feel), then BE5–BE7 (squads),
BE10–BE12, then BE8, BE13.

---

## 10. Server & lobby (no sim impact)

| # | Feature | Notes |
|---|---|---|
| S-BR1 | **Match history & stats** | Store BE11 standings per callsign in `room_history` (`docs/22`): matches, wins, top-5s, kills, damage, avg place. |
| S-BR2 | **Rating & leaderboards** | Per mode (solo/duo/squad) rating (e.g. OpenSkill/TrueSkill-style from placements; pure Rust, allowed deps per Appendix A). Lobby "Leaderboard" tab; weekly reset option. |
| S-BR3 | **Parties / squads** | Lobby party invite; the room assigns teams from parties (BE5 `player_set_team` is called by the mod from a room option the server sets, e.g. `teams=0,0,1,1,…`). |
| S-BR4 | **Matchmaking queue** | "Quick play Battle Royale": fill rooms to 8, start after 60 s with bots for empty seats (BE8). |
| S-BR5 | **Map pool & vote** | Admin-configured map list for the royale mod; players vote in pregame (mod reads the chosen map from `room_option("map")` — the map is fixed at room creation, so vote happens in the lobby before creation). |
| S-BR6 | **Profile & cosmetics** (optional) | Player colour, name tag colour, victory emote id stored server-side, sent in the roster → BE3-like deterministic session data (`player_cosmetic(slot, key)`). |

---

## 11. Milestones and acceptance

Engine (`BE`) and server (`S-BR`) items can run in parallel with the first mod
milestones; each mod milestone lists what it needs.

| ID | Scope | Needs | Acceptance |
|---|---|---|---|
| **BR0** | Refactor into §7 layout + `Host`/`FakeHost`; behaviour parity | — | All `wasm_mod_royale` tests + `verifier_headless` pass unchanged; `zone_at` tests kept. |
| **BR1** | Match flow phases, pregame (no damage), deploy with landing shield, moving circles + new schedule (§5, 6.3) | — (text fallback) | E2E: pregame ignores damage; zone centre moves but next circle is contained (property test over 1000 seeds, FakeHost); determinism test still equal. |
| **BR2** | HUD: zone timer, alive counter, kill feed, elimination toast, hit markers, damage direction, interaction prompts (§6.5) | MH0–MH4, BE3 | E2E: widgets exist with expected text (names); two-seat lockstep run equal checksums. |
| **BR3** | Zone rendering + minimap/full map + drop selection screen (§5, 6.3) | BE1, BE2, BE4 | Drop select: scripted `TicCmd`s move cursor and confirm → player lands within 512 of the pick; offscreen golden of zone wall and map widget (4:3, 16:9). |
| **BR4** | Loot 2.0: rarity, supply drops, vault, death loot (§6.2) | Track B optional | E2E: supply crate spawns inside next circle; opening with held USE spawns legendary items; death drops victim's weapons. |
| **BR5** | Death experience: elimination panel, recap, spectate killer (§6.6) | BE9 | E2E: dead seat camera = killer actor; cycling works; recap lists top damage sources. |
| **BR6** | Squads: teams, no friendly fire, downed/revive, pings, squad HUD (§6.4) | BE4–BE7 | E2E duos: teammate damage = 0; lethal damage → downed (1 HP); revive after 4 s USE; bleed-out kills; ping marker visible to teammate only. |
| **BR7** | Moments, music, victory screen, rich standings (§6.7–6.8) | BE10, BE11 | E2E: "3 LEFT" and final circle banners; `end_match_ex` standings match recorded kills/damage; server stores them (S-BR1). |
| **BR8** | 8 seats, bots (§9 BE8, BE12), quick play (S-BR4) | BE8, BE12 | 8-seat room with 6 bots finishes a match headless in < 15 min sim time, deterministic across two runs and in the verifier; bots ≤ 0.2 ms/tic. |
| **BR9** | Track B content, optional ROYALE01 map, leaderboards, balance pass, docs | S-BR2, Track B | Room-server serves the WAD with lumps; lobby leaderboard shows ratings; README rewritten. |

---

## 12. Testing

- Native: `cargo test --manifest-path mods/doom1-royale/Cargo.toml` (zone
  geometry, loot caps, drop resolve, downed state machine, kill feed queue,
  standings math) with `FakeHost`.
- E2E: `cargo test -p room-game --test wasm_mod_royale` — extend `start(map, players, …)`
  helpers with scripted `TicCmd` sequences per seat (already used for PvP tests)
  for menus, pings, revives; add a `squads` fixture map with ≥ 8 DM starts.
- Lockstep: two-peer runs (`room-net` loopback) with captures, drops and bots,
  comparing `World::checksum()` every 35 tics.
- Server: `verifier_headless` with the new mod; a result-ingest test for BE11.
- Render: offscreen goldens for zone wall/ring and map widget (pattern of
  `tests/model_offscreen_smoke.rs` / goldens under `testdata/goldens/`).
- Perf: 8 seats + bots + zone at 35 Hz within the mod fuel budget (assert no
  "disabled" message over 20 000 tics).

## 13. Definition of done

- BR0–BR9 accepted; BE1–BE7, BE9–BE12 merged with tests and `docs/12` updates;
  S-BR1–S-BR4 shipped; optional BE8/BE13/S-BR5–6 tracked separately.
- `room.toml` `version = "0.2.0"`, `max_players = 8` (if BE12), description
  updated; README describes modes, controls (`+mod1..4`), HUD, squads.
- `docs/22` updated for the result payload, team assignment and room options;
  `docs/21` has BR/BE tables and log lines; `THIRD_PARTY.md` lists assets.
- Plays on shareware doom1 E1M1–E1M9, Doom 2 / Freedoom maps and ROYALE01,
  2–8 seats, solo/duos/squads, with spectators, without desync.

## 14. Open questions for the owner

1. Squads: duos only, or duos + squads of 4 (only 2 teams at 8 seats)?
2. Bots (BE8) in scope? They are the largest engine item but make 2-player
   rooms feel like a real BR.
3. Dedicated map ROYALE01: build one (or several), or stay on stock maps?
4. Ratings/leaderboards public, or per server only? Seasonal resets?
5. Proximity voice for enemies on by default, team-only, or off?
6. Should the zone wall be visible through walls (readability) or only in line
   of sight (realism)?
