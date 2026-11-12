# 32 — Shuriken mod: a weapon generated at startup (`mods/doom1-shuriken`)

**Audience:** an external coding agent working in the Room repo. Read
`AGENTS.md`, `docs/02-conventions.md` (§Determinism), `docs/12-wasm-scripting.md`
§15–16 and **`docs/31-mod-api-extensions.md`** first — this mod needs those
engine features (milestones `MA1`–`MA4`) and is their acceptance case.
**Status:** **SK0**–**SK4** done (art/synth/classes/behaviour + `wasm_mod_shuriken`;
parents `Clip`/`Pistol`). Milestones `SK0`–`SK4` (§9).

**Idea.** A WASM mod that adds a throwing weapon — **shurikens** — to any
Doom game. Nothing is shipped as art or data files: during `room_register()`
the mod **generates everything in code**: sprites (drawn from vector shapes
and quantised to the game's palette), sounds (synthesised PCM), the ammo,
weapon, projectile and pickup classes, their states and their behaviour.
The result behaves like a native weapon: slot key, pickups, HUD icon, ammo
counter, obituaries; works in netplay, demos and on the server verifier.

---

## 0. TL;DR

1. Wait for (or build first) `docs/31` MA1–MA4: module states/actions,
   weapon/ammo/projectile properties, generated lumps, slots/alt fire/missile
   hit info/manifest options.
2. Mod modules: `art.rs` + `raster.rs` + `palette.rs` (sprites), `synth.rs`
   (sounds), `classes.rs` (registration), `behaviour.rs` (impact, sticking,
   retrieval, drops, placement), `tables.rs` (balance).
3. Generation must be bit-identical everywhere (integer maths, const sine
   table, fixed-seed noise); every generated lump's hash is pinned in a test.

---

## 1. Game design

| Aspect | Design |
|---|---|
| Name / tag | "Shuriken" (`Tag`), obituary "%o was diced by %k's shuriken." |
| Slot | Key **8** by default (free in Doom; configurable in `room.toml`, §3.6) |
| Ammo | `ShurikenAmmo`: max 30, backpack doubles to 60; weapon pickup gives 10, `ShurikenBox` gives 10, one retrieved star gives 1 |
| Primary fire | Throw one star. Fast (`Speed 30`; rocket 20, plasma 25), small (`Radius 6`, `Height 6`), `Damage 3` → Doom projectile damage `((P_Random()%8)+1)·3` = 3–24 (avg 13.5). Held interval **11 tics** (≈3.2 throws/s ≈ 43 DPS: between pistol ~25 and chaingun ~87) |
| Alt fire | **Fan of 3** (−6°, 0°, +6°), costs 3 ammo, held interval 24 tics |
| Silent | The weapon has `+WEAPON.NOALERT`: throwing does **not** call `P_NoiseAlert`. Impact sounds don't wake monsters either (they are plain sounds). Stealth niche. |
| Sticking | A star that hits a **wall, floor or ceiling** stays stuck there as a `ShurikenStuck` pickup (+1 ammo when walked over, 64-unit pickup radius as normal touch); despawns after 60 s; at most 32 per map (oldest removed). A star hitting **sky** or a **liquid** floor (TERRAIN splash / `damage` floor) is lost. |
| Hitting monsters | Damage + flesh sound + blood (normal missile path). The mod remembers which monster holds how many stars; when that monster dies, they **drop** as `ShurikenStuck` pickups around the corpse. Stars hitting non-bleeding/`NOBLOOD` actors bounce off with a clink and drop to the floor. |
| Visuals | 4-point star, steel with a bright edge; 4 spin frames (0°, 22.5°, 45°, 67.5°) at 2 tics each; first-person gloved hand holding a fan of stars with wind-up / release / follow-through frames; pickup sprite (a small pile), box sprite, stuck sprite (star half-buried), HUD icon. |
| Sounds | Throw "whoosh", wall "clink", flesh "thunk", pickup "shing", box pickup, weapon raise "slide". |
| Map placement | On each map load the mod places the weapon and ammo deterministically (§5.4); `give Shuriken` / `summon ShurikenPickup` work from the console. |

Balance numbers live in one table (`tables.rs`) and are used both for
registration and for tests.

---

---

## 2. Engine features used (from `docs/31`)

| Feature | Used for |
|---|---|
| MA1 module states + native/WASM actions | weapon `Ready/Fire/AltFire`, missile spin, `ShurikenImpact()` WASM action |
| MA2 properties/flags | `Ammo`/`Weapon`/`Inventory` props, `Projectile` flags, `+WEAPON.NOALERT`, `A_FireProjectile` ammo use |
| MA3 generated lumps | all sprites (`SHK*`) and sounds (`DSSHK*`) + logical sound names |
| MA4 slots, alt fire, hit info, options | slot 8, fan throw, sticking logic, `[options]` in `room.toml` |

If `docs/30` W16 (`Weapon`/`Ammo` base classes) has not landed, use
`Clip`/`Pistol` as parents and override everything (the module's labels
replace the inherited states), and say so in the README.

---

## 3. The mod: structure and registration

### 3.1 Layout
```
mods/doom1-shuriken/
  Cargo.toml            # standalone guest crate, cdylib+rlib, opt-level "z" (copy doom1-survival's)
  room.toml             # manifest, §3.6
  README.md
  src/
    lib.rs              # RoomMod impl, export_mod!, event routing
    tables.rs           # balance numbers, names, colours (single source of truth)
    fixed.rs            # Q16.16 helpers, const sine table (256 entries), isqrt
    raster.rs           # polygon scanline fill, outline, shading, supersample-free
    palette.rs          # PLAYPAL load (lump_read) + nearest-colour LUT (32³, built once)
    art.rs              # vector shapes for every sprite → indexed pixels → reg_lump
    synth.rs            # integer PCM synth → DMX → reg_lump + reg_sound_alias
    classes.rs          # reg_class_*/reg_state*/reg_weapon_slot
    behaviour.rs        # impact handling, sticking, retrieval, drops, placement
```
No art or sound files in the repo. `scripts/room-server.sh mods` already
builds every `mods/<dir>` with a `Cargo.toml`.

### 3.2 Determinism rules for generation
- Integer / Q16.16 maths only; rotation uses a **const 256-entry sine table**
  in `fixed.rs` (game `math_*` imports are play-scope and not available during
  registration).
- No `f32/f64` arithmetic in generation (or only with exactly-representable
  operations and no transcendental functions) — simplest is to ban floats in
  `raster.rs`/`synth.rs` (clippy `float_arithmetic` lint on those modules).
- Noise for sound uses a fixed-seed xorshift32 in `synth.rs`.
- Iteration in index order; no hashing of pointers.
- Unit test: SHA-256 of every generated lump is pinned in the test (changes
  must be deliberate).

### 3.3 Art generation (`art.rs`, `raster.rs`, `palette.rs`)
1. Read `PLAYPAL` (palette 0) via `lump_find`/`lump_read`; build a 32³
   RGB→index LUT once (nearest colour, Euclidean in RGB, ties → lower index).
   Index 247 (often transparent-looking cyan in Doom) is never used as a
   colour; transparency is the patch mask.
2. Shapes are described as polygons in a 1/16-pixel integer grid with a
   colour ramp id; the rasteriser fills with even-odd scanlines at pixel
   centres, then draws a 1-px dark outline (Doom sprites have dark borders),
   then applies simple lighting: steel ramp chosen by the dot of the face
   normal with a fixed light direction (precomputed per face), plus a 1-px
   bright edge highlight.
3. Sprites to generate (names follow Doom conventions; `0` = all rotations):

| Lump(s) | Size | Content |
|---|---|---|
| `SHKMA0`–`SHKMD0` | 16×16 | Flying star, 4 spin frames (rotations 0°, 22.5°, 45°, 67.5°) |
| `SHKSA0` | 16×10 | Star stuck in a surface (half buried, seen edge-on; pivot at the bottom) |
| `SHKPA0` | 20×12 | Single star lying on the floor (weapon/ammo pickup look) |
| `SHKBA0` | 24×16 | Box of stars (ammo box pickup): wooden box + 3 visible stars |
| `SHKWA0` | 28×16 | Weapon pickup: fan of 5 stars on a cloth wrap |
| `SHKGA0` | ~120×80 | First-person Ready: gloved hand bottom-right holding a fan of 3 stars |
| `SHKGB0` | ~130×90 | Wind-up: hand pulled back/up |
| `SHKGC0` | ~110×70 | Release: arm forward, hand open, no star |
| `SHKGD0` | ~110×70 | Follow-through: hand lower, reaching for next star |
| `SHKGE0` | ~130×90 | Alt wind-up: wider fan of 5 stars |
| `SHKIA0` | 16×16 | HUD / status bar icon |

   Psprite offsets (`left/top` in the patch header) are set so the hand sits
   at the bottom-right like vanilla weapons: `left ≈ −(160+20)`, `top ≈ −(100+30)`
   relative to `WEAPONTOP` conventions (verify with `room-ui/src/weapon.rs`
   `psprite_pos` and the existing `weapon.rs` tests; status-bar shift is
   applied by the shell).
4. Register each with `reg_lump(name, SPRITES, encode_patch(...))`.
5. Optional variants (6-point star, gold skin) chosen by manifest options
   (§3.6) — never by local cvars, so every client generates the same bytes.

### 3.4 Sound generation (`synth.rs`)
11025 Hz, 8-bit unsigned (Doom DMX), all integer:

| Logical name | Lump | Recipe |
|---|---|---|
| `shuriken/throw` | `DSSHKTHR` | 180 ms band-passed noise (two one-pole filters) with a downward cutoff sweep, fast attack, exponential decay |
| `shuriken/clink` | `DSSHKCLK` | 3 damped sine partials (2.1, 3.7, 5.9 kHz; table sine), 250 ms, slight random detune per partial (fixed seed) |
| `shuriken/flesh` | `DSSHKFLS` | 90 ms low noise burst + 140 Hz thump |
| `shuriken/pickup` | `DSSHKPKP` | 2 short metallic pings rising, 200 ms |
| `shuriken/box` | `DSSHKBOX` | wood knock (noise burst through resonant filter at 400 Hz) + ping |
| `shuriken/up` | `DSSHKUP` | soft cloth slide (noise, low-passed, 300 ms) |

Register with `reg_lump(.., SOUNDS, encode_dmx(..))` and
`reg_sound_alias(logical, lump)`.

### 3.5 Classes and states (`classes.rs`)
What the mod registers, written as DECORATE for clarity (the code issues the
equivalent `reg_*` calls):

```
actor ShurikenAmmo : Ammo
{
  Inventory.Amount 1
  Inventory.MaxAmount 30
  Ammo.BackpackAmount 10
  Ammo.BackpackMaxAmount 60
  Inventory.Icon "SHKIA0"
  Inventory.PickupMessage "Picked up a shuriken."
  Inventory.PickupSound "shuriken/pickup"
  States { Spawn: SHKP A -1  Stop }
}

actor ShurikenBox : ShurikenAmmo
{
  Inventory.Amount 10
  Inventory.PickupMessage "Picked up a box of shurikens."
  Inventory.PickupSound "shuriken/box"
  States { Spawn: SHKB A -1  Stop }
}

actor ShurikenStuck : ShurikenAmmo      // retrievable star (spawned at runtime)
{
  Inventory.Amount 1
  +NOGRAVITY                            // stays where it stuck (walls/ceilings)
  States { Spawn: SHKS A 2100 Stop }    // 60 s, then disappears
}

actor Shuriken : Weapon
{
  Tag "Shuriken"
  Obituary "%o was diced by %k's shuriken."
  Weapon.AmmoType "ShurikenAmmo"
  Weapon.AmmoUse 1
  Weapon.AmmoGive 10
  Weapon.SelectionOrder 1600            // after shotgun, before pistol
  Weapon.SlotNumber 8
  Weapon.UpSound "shuriken/up"
  Weapon.Kickback 50
  +WEAPON.NOALERT
  Inventory.PickupMessage "You got the shurikens!"
  States
  {
  Spawn:    SHKW A -1                    Stop
  Ready:    SHKG A 1 A_WeaponReady       Loop
  Deselect: SHKG A 1 A_Lower             Loop
  Select:   SHKG A 1 A_Raise             Loop
  Fire:
    SHKG B 4
    SHKG C 2 A_FireProjectile("ShurikenMissile", 0, 1)   // useammo = 1
    SHKG C 0 A_StartSound("shuriken/throw", CHAN_WEAPON)
    SHKG D 5
    SHKG A 4 A_ReFire                    // held interval 4+2+5 = 11 tics
    Goto Ready
  AltFire:
    SHKG E 6
    SHKG C 2 A_FireProjectile("ShurikenMissile", -6, 1)
    SHKG C 0 A_FireProjectile("ShurikenMissile",  0, 1)
    SHKG C 0 A_FireProjectile("ShurikenMissile",  6, 1)
    SHKG C 0 A_StartSound("shuriken/throw", CHAN_WEAPON)
    SHKG D 8
    SHKG A 8
    SHKG A 6 A_ReFire                    // held interval 24 tics
    Goto Ready
  }
}

actor ShurikenMissile
{
  Radius 6
  Height 6
  Speed 30
  Damage 3
  Projectile                             // = +MISSILE +NOGRAVITY +NOBLOCKMAP +DROPOFF +NOTELEPORT
  SeeSound ""                            // sound is played by the weapon
  DeathSound ""                          // impact sound chosen by the mod (clink/flesh)
  +FORCEXYBILLBOARD
  States
  {
  Spawn:  SHKM ABCD 2                    Loop
  Death:  TNT1 A 0 ShurikenImpact()      // WASM action (callback), §5.1
          Stop
  XDeath: TNT1 A 0 ShurikenImpact()
          Stop
  }
}
```
Plus `reg_weapon_slot(8, "Shuriken", 0)`. All names are exported in
`tables.rs` so other mods can `class_find("Shuriken")`.

Classes inherit from `Ammo` / `Weapon`. If `docs/30` W16 (base classes) has
not landed, use `Clip`/`Pistol` as parents and override everything (the
inherited states are replaced by the module's labels), and say so in the
README.

### 3.6 Manifest (`room.toml`)
```toml
[mod]
id = "room.example.doom1-shuriken"
name = "Shuriken"
version = "0.1.0"
abi = 1
description = "Throwing stars generated at startup: silent, retrievable, slot 8."

[options]            # read at registration via a manifest-options import; identical for all clients
slot = 8
style = "4point"     # or "6point"
skin = "steel"       # or "gold"
place_weapon = true
ammo_replace_percent = 25

[[module]]
file = "SHURIKEN"
scope = "play"
name = "shuriken"
```
`[options]` are read with `manifest_option_i32/str` (`docs/31` MA4b).

---

## 4. Startup sequence

```
room_register():
  1. pal = palette::load()                       // lump_read("PLAYPAL")
  2. for each sprite in art::SPRITES: px = raster(shape, pal); reg_lump(name, SPRITES, encode_patch(px))
  3. for each sound in synth::SOUNDS: pcm = synth(recipe); reg_lump(lump, SOUNDS, encode_dmx(pcm)); reg_sound_alias(logical, lump)
  4. classes::register()                         // ammo, box, stuck, weapon, missile (states + actions)
  5. reg_weapon_slot(slot, "Shuriken", 0)
  6. register the event handler (export_mod! does this): LOADED, TICK (1 Hz bookkeeping), THING_DIED, THING_DESTROYED
Host: merges classes/states, mounts the generated-lump layer, refreshes caches, records the fingerprint.
```
Budget: generation must finish well within the registration fuel budget
(50 M instructions, `docs/12` §4.2). Rough estimate: ~12 sprites ≤ 130×90
px + 6 sounds ≤ 3 k samples each ≈ a few M instructions. Measure in a test
and keep a 5× margin.

---

## 5. Runtime behaviour (`behaviour.rs`)

### 5.1 `ShurikenImpact` (WASM action on the missile's Death/XDeath)
Read `hit_kind`, `hit_actor`, `hit_pos`, `hit_normal` (`docs/31` MA4):

| hit_kind | Action |
|---|---|
| actor (bleeds) | Play `shuriken/flesh` at the impact; add 1 to that actor's stuck count (`stuck_in: [(Actor, u8); 64]`, oldest-out). |
| actor (`NOBLOOD` / invulnerable / barrel) | Play `shuriken/clink`; spawn `ShurikenStuck` on the floor below the impact with gravity (spawn at `hit_pos`, then clear `NOGRAVITY` via a flag import or spawn a gravity variant `ShurikenDropped`). |
| wall | Play `shuriken/clink`; spawn `ShurikenStuck` at `hit_pos − normal·2`, angle facing out of the wall (`WALLSPRITE` render flag), `NOGRAVITY`. |
| floor | Spawn `ShurikenStuck` lying flat (`FLATSPRITE`) at floor z. If the floor is liquid (TERRAIN splash or damaging sector): no star, splash only. |
| ceiling | `ShurikenStuck` at ceiling z − 2, `NOGRAVITY`. |
| sky | Nothing (lost). |

Keep a ring of at most 32 stuck stars per map: when full, `destroy()` the
oldest. Use `WORLD_THING_DESTROYED` to drop entries that were picked up or
expired.

### 5.2 Drops from dead monsters
On `ThingDied { thing }`: if `thing` is in `stuck_in`, spawn that many
`ShurikenDropped` pickups around the corpse (deterministic offsets: 8 units on
a circle at angles from the fixed sine table, `CHECK_POSITION`), remove the
entry.

### 5.3 Retrieval
`ShurikenStuck`/`ShurikenDropped` are normal ammo pickups (touch to collect),
so no mod code is needed. Stars stuck high on walls are reachable only if the
player can touch them (Doom touch uses the 3D box), which is intended.

### 5.4 Placement on map load (`WORLD_LOADED`)
Deterministic, using the playsim RNG (`game::random`) after the map is loaded:
1. **Weapon**: on the first map of an episode (or any map if the player
   doesn't own it yet and `place_weapon = true`): spawn `Shuriken` at the
   first item spot (from `actors_of_class(NULL)` filtered to pickups) that is
   ≥ 512 units from player 1's start and reachable (same validation as
   `docs/27` §6.1 spots: floor/ceiling gap ≥ 56).
2. **Ammo**: replace `ammo_replace_percent` % of `Clip` pickups with
   `ShurikenBox` (destroy the clip, spawn the box at the same spot);
   deathmatch: also next to every `Shotgun`.
3. Never touch maps where the mod is disabled; never change anything when
   loading a save (`Loaded { is_save_game }`).

### 5.5 Netplay, demos, saves
- Everything is play-scope and deterministic; demos recorded with the mod
  play back with the mod.
- `stuck_in` / ring state lives in module memory → needs module memory in
  saves (`docs/27` C7) for exact save/load; until then, on load rebuild the
  ring from `actors_of_class("ShurikenStuck")` and accept that stars inside
  monsters are not dropped after a load (document it).
- Royale/Survival compatibility: classes are global; `docs/28` loot tables can
  add `Shuriken`/`ShurikenBox` via `class_find` when present.

---

## 6. Balance table (`tables.rs`)

| Item | Value | Notes |
|---|---|---|
| Missile speed / radius / height | 30 / 6 / 6 | |
| Damage | 3 → 3–24 per star (avg 13.5) | Doom missile formula |
| Primary held interval | 11 tics | ≈ 43 DPS |
| Alt fan | 3 stars, ±6°, 24 tics, 3 ammo | ≈ 50 DPS at close range |
| Max ammo | 30 (60 with backpack) | |
| Weapon pickup ammo | 10 (dropped: 5) | |
| Box | 10 | |
| Stuck lifetime | 60 s | max 32 per map |
| SelectionOrder | 1600 | auto-switch prefers it over pistol, not over shotgun |
| Kickback | 50 | light knockback |

Skill multipliers follow the engine's normal ammo rules (ITYTD/NM double).

---

## 7. Tests

| Test | Where | Checks |
|---|---|---|
| Generation is deterministic | `cargo test --manifest-path mods/doom1-shuriken/Cargo.toml` | SHA-256 of every lump pinned; LUT never returns index 247; patch encoder output decodes with the `room-formats` patch decoder |
| Registration budget | same | instruction count of `room_register` under wasmi < 10 M (5× margin) |
| Mod e2e | `crates/room-game/tests/wasm_mod_shuriken.rs` | classes exist; `give Shuriken`; tap fire → 1 missile, ammo −1, no noise alert (sleeping monster in another sector stays asleep); held fire interval 11 tics; alt → 3 missiles, ammo −3; wall hit → `ShurikenStuck` at the wall; walking over it → ammo +1; monster with 2 stars dies → 2 pickups; sky hit → nothing; determinism (two runs, equal `World::checksum()`) |
| Placement | same | weapon placed ≥ 512 units from the start; `ammo_replace_percent` of clips replaced; nothing changed on save load |
| Netplay / server | `cargo test -p room-server --features verifier --test verifier_headless` with the mod | checksums agree |
| Web | browser build + manual check | sprites and sounds appear |

---

## 8. Files

| Area | Files |
|---|---|
| Mod | `mods/doom1-shuriken/{Cargo.toml, room.toml, README.md, src/*}` |
| Tests | `crates/room-game/tests/wasm_mod_shuriken.rs` |
| Build | nothing new: `scripts/room-server.sh mods` builds every `mods/<dir>` with a `Cargo.toml` |
| Docs | `docs/12` §15 (list the example), `docs/21` (milestones), this file |

---

## 9. Milestones

| ID | Scope | Needs | Acceptance |
|---|---|---|---|
| **SK0** | Skeleton, `fixed.rs`, `palette.rs`, `raster.rs`, `art.rs` (all sprites) with pinned hashes | MA3 | **done** — native tests green (SHA-256 pins) |
| **SK1** | `synth.rs` sounds + aliases | MA3 | **done** — pinned DMX hashes |
| **SK2** | `classes.rs`: ammo, box, stuck, weapon, missile; primary + alt fire | MA1, MA2, MA4 | **done** — Clip/Pistol parents; `wasm_mod_shuriken` fire |
| **SK3** | `behaviour.rs`: impact, sticking, retrieval, drops, placement | MA4 (hit info, `THING_DESTROYED`) | **done** — impact callback + ring/drops/placement |
| **SK4** | Polish: README, pack, e2e | — | **done** — `shuriken.wad` + README + e2e |
| **SK5** | Bug fixes from §11: D1 psprite offsets, D2 outline order, D3 `replace_clips`, D4 sound normalisation, D15 placement retry | — | §11.3 tests for those items green; new pinned hashes |
| **SK6** | Art pass A1–A3 (+A5 real-palette pins): hand rig, bevelled stars, 8 spin frames/rotations, world sprites | — | psprite visibility + outline + border tests; goldens |
| **SK7** | Feel B1–B5: timings, AltFire ammo gate, impact sparks/FX, sound pass + fly loop, ceiling drop, blink, magnet | MA2 `AmmoUse2` (optional) | §11.3 e2e tests |
| **SK8** | Extras A4, A6, B6 BR loot hook, B7 HUD widget | docs/27 §9 MH, docs/28 loot | manual playtest + options test |

## 10. Open questions for the owner

1. Slot 8, or share slot 1 with Fist/Chainsaw (GZDoom slot cycling makes that
   possible)?
2. Should throwing really be silent (`NOALERT`), or alert at a short radius?
3. Do retrieved stars count as items for the end-of-level item percentage?
   (Recommendation: no — spawned at runtime, not `COUNTITEM`.)
4. Ship variants (6-point / gold) as manifest options, or keep one look?
5. Backstab ×3 crit (§11.2 B3) and star wear (B5): in by default, or options only?
6. Flurry refire (8 tics after the first throw, B1): keep, or one flat 11-tic interval?

---

## 11. Design review of the implemented mod (2026-11-01)

What I checked: `mods/doom1-shuriken` (SK0–SK4, all marked done). I built
the guest natively, wrote every generated sprite with the real Doom
`PLAYPAL`, decoded the patches, and drew them at their psprite and world
positions using the same formula as `room-ui/src/weapon.rs::psprite_pos`
(`x = −left`, `y = 32 − top`, minus 32 when the status bar is shown).
I also decoded all six DMX lumps and measured their length and peak level.
The mod works mechanically. The **look and sound are well below vanilla
quality**, and three of the problems are plain bugs.

### 11.1 Findings

| # | Area | Finding (measured) | Severity |
|---|---|---|---|
| D1 | Psprite placement | Every `SHKG*` frame uses `left −180, top −130`. With the bar shown, `SHKGA0` (120×80) starts at y=130. The glove body starts 40 px lower, at y=170, which is behind the status bar. Only the fingertips and the three stars (≈y 152–168) are visible. In fullscreen it starts at y=162 and the glove body is off-screen (y ≥ 202). **The player sees a few floating white stars and no hand.** Vanilla rule: `top = h − 168`, so the bottom edge is flush with y=200 in fullscreen (`PISGA0` 62 px tall → −106). Here `top` should be about −88 for 80 px and −78 for 70 px. | **bug** |
| D2 | Star rasteriser | `paint_star` runs `outline(dark)` and then `highlight_edge`. Almost every outline pixel also has a transparent diagonal neighbour, so the highlight pass paints it level-7 white. **The dark outline is erased.** Stars come out as white blobs with no border. They disappear against bright walls, sky and the `STARTAN`/`COMPUTE` textures, and on the HUD icon. | **bug** |
| D3 | `replace_clips` | The test is `((i*100) % 100) >= percent`, which is always `0 >= percent`. So with `ammo_replace_percent = 25`, **every** Clip is replaced, not one in four. | **bug** |
| D4 | Sound levels | Measured peaks out of ±128: `DSSHKTHR` **9**, `DSSHKUP` **6**, `DSSHKBOX` 35, `DSSHKFLS` 70, `DSSHKPKP` 95, `DSSHKCLK` **128 (clipping)**. The throw and the raise are practically silent, while the clink distorts. | **bug** |
| D5 | Shading | §3.3 asks for per-face lighting (a steel ramp chosen by face normal against a fixed light). The code fills each star with **one** flat ramp level. The glove is one flat green (48,90,48) block with four 5-px rectangles for fingers. There is no arm, no knuckles and no shading. The box and the cloth wrap are flat rectangles. | design |
| D6 | Spin frames | Rotating a 4-point star by 0/22.5/45/67.5° at 16 px does not give four consistent shapes. `SHKMC0` reads as a square with spikes, while B and D look like pinwheels. The silhouette pulses instead of spinning. The missile also has `FORCEXYBILLBOARD`, so it always faces the camera: from the side it looks like a frisbee seen face-on. | design |
| D7 | Stuck / pickup sprites | `SHKSA0` copies rows 6–15 of a 45° star, which reads as a grey box. On walls it is drawn as a `WALLSPRITE` facing out, so the player sees a box glued to the wall rather than a star with one point buried. `SHKPA0` is a cropped star, not one lying flat on the floor. `SHKWA0` crops the top of its stars. `SHKBA0` stars at 8 px become vertical white ticks. | design |
| D8 | Fire pose | `SHKGC0` (release) shows no star at all, and no motion cue. The throw has no frame where the star is visibly leaving the hand, so the projectile appears from nowhere about 20 units ahead. | design |
| D9 | Timing vs spec | §1 says the held interval is 11 tics. The code's Fire chain is B4 + C2 + C0 + D5 + A4 = **15 tics** (≈2.3/s, ≈31 DPS, close to the pistol). AltFire is 6+2+0+0+0+0+0+8+8+6 = **30 tics**, against 24 in the spec. Either the table or the states are wrong. | balance |
| D10 | AltFire with < 3 ammo | `A_FireProjectile(...,1)` calls `deplete_ready_ammo` once per star. With 1–2 ammo the full 30-tic alt animation plays and only 1–2 stars leave. Nothing tells the player why. | design |
| D11 | Impacts | Death/XDeath are a 0-tic `TNT1` state plus the callback. There is no spark puff on metal, no dust on floors and no hit flash, and no FX event (docs/29 K, `fx_events.rs`). A clink with no visual feels like a miss. | design |
| D12 | Flight | No flight loop, so a thrown star is silent until impact. That is consistent with `NOALERT`, but players expect a faint spinning whistle. A **local-only** sound (`CHAN_BODY`, attenuated 1.5, no noise alert) keeps the stealth design. | polish |
| D13 | Ceiling hits | A star stuck in the **ceiling** becomes a `ShurikenStuck` pickup that nobody can reach. Ammo is silently lost. | design |
| D14 | Despawn | `ShurikenStuck` lasts 2100 tics and then vanishes at once. There is no blink warning, so players walk back and the star is gone. | polish |
| D15 | Weapon placement | `try_place_weapon` scans the first 128 actors of *any* class. It takes the first one ≥512 units from player 0 with a ceiling height of at least 56, spawns at that actor's spot (often a solid monster or lamp, so `CHECK_POSITION` fails) and then **returns anyway**. On many maps the weapon never appears. The scan never prefers sensible places such as item spots, secret sectors or a fixed early position. | bug-ish |
| D16 | Options | `room.toml` declares `style = "4point"` and `skin = "steel"`, but nothing reads them. Either implement them (§11.2 A6) or remove them from the manifest. | cleanup |
| D17 | Grey fallback | `Palette::fallback()` is a greyscale ramp. The pinned hashes are computed with it, so the tests never exercise the real-palette mapping that players see. | test gap |

### 11.2 Improvements

#### A. Art (sprites stay generated, deterministic, integer-only)

**A1 Psprite rig (fixes D1, D5, D8).** Replace the "block + finger bars"
glove with a small 2D rig drawn from polygons in the 1/16-px grid:

```
forearm (trapezoid, sleeve colour = Doomguy green ramp 3 levels)
  └─ palm (rounded quad, glove brown/black ramp)
      ├─ thumb (2 segments)          ├─ 4 fingers (2 segments each, curl angle per pose)
      └─ held stars (0..5, fan angle, each a full A2 star)
```

* Pose table in `tables.rs`: per frame `{forearm_angle, wrist_offset,
  finger_curl, stars, star_fan_deg, star_spin}`, so poses are data and the
  frames below come from one function.
* Light: fixed light from the top-left. Each polygon picks a ramp level from
  `dot(face_normal, light)`, precomputed per polygon as an integer. Then a
  1-px dark outline, and a highlight only on edges that face the light (see A2).
* Frames: keep `SHKGA`–`SHKGE` (hashes change) and add:
  * `SHKGF` "release": the star 20 px ahead of the fingers with a 3-px motion
    smear made of two translucent-looking dithered copies (palette darken,
    no alpha).
  * `SHKGG` "empty hand" for 0 ammo (`Ready` → `A_JumpIfNoAmmo` to an
    `EmptyReady` loop).
  * `SHKGH` 2-frame idle flourish: the star spins between the fingers every
    4 s, through a `Ready` sub-loop counter or a `A_Jump(8, "Flourish")` on
    the playsim RNG.
* Offsets: compute instead of hard-coding:
  `left = −(320 − w − margin_r)` and `top = h − 168 + lift`, where `lift`
  comes from the pose table (wind-up raises the hand 10–14 px). Add a native
  test that asserts, for every `SHKG*` frame, that the visible bounding box
  with the bar shown overlaps `y ∈ [100,168)` by at least 30 px and
  `x ∈ [140,320)`.

**A2 Star renderer (fixes D2, D6).**
1. Order: fill, then bevel shading, then highlight, then outline **last**,
   so nothing overwrites the border.
2. Bevel: split each point into two triangles (left and right of the spine).
   One is lit and one is shadowed using the light dot. Add a centre hole
   (classic shuriken) as a 2-px dark disc. This alone makes the star read
   at 16 px.
3. Highlight only on pixels whose outward normal faces the light (test
   `(-1,-1)`/`(0,-1)`/`(-1,0)` neighbours only), at ramp 6 and not 7. Keep 7
   for one specular pixel per point.
4. Spin frames: render at 4× (64×64) in the grid, then box-downsample with
   integer averaging *in RGB* and re-map to the palette (or mode-pick the
   index). That gives the same silhouette weight at every angle. Use 8
   frames (`SHKMA`–`SHKMH`, 11.25° steps) at 1 tic each, a full 90° cycle in
   8 tics. The visual spin speed is unchanged and the motion is smoother.
5. Edge-on look (D6): sprites cannot depend on view pitch, so
   **drop `FORCEXYBILLBOARD`** and use 8
   **rotations** (`SHKMA1`–`A8`). Frontal rotations show the star squashed
   (it flies flat, parallel to the floor), side rotations show it nearly
   edge-on. That needs 8 frames × 5 views (1/5 mirrored), which is cheap to
   generate. If an engine `ROLLSPRITE`/`FLATSPRITE` + per-tic roll path is
   added later (none exists in `room-render` today), switch to a single
   flat-sprite star that the sim spins with `angle += 45` per tic.

**A3 World sprites (fixes D7).**
* `SHKSA0`: draw a full star rotated so one point goes into the surface,
  and clip the buried point with a 1-px dark "slot" line. For walls, keep
  `WALLSPRITE` and offset 1 unit out from the wall to avoid z-fighting
  (already −2·normal; make it `+0.5·normal` after the sprite is centred on
  the buried point).
* `SHKPA0`: a star lying flat, drawn as a polygon squashed vertically to
  40% with a 1-px shadow ellipse under it (palette index 0 dithered).
* `SHKBA0`: a 3/4-view box with front, top and side faces (3 ramp levels,
  wood grain = every 3rd row one level darker), lid open, and 3 stars at
  12 px with the A2 renderer standing in the box.
* `SHKWA0`: a rolled cloth wrap seen in 3/4 view with 5 star points
  sticking out. Size the canvas from the polygons' bounds so nothing gets
  cropped. Add a test that no opaque pixel touches the canvas border.
* `SHKIA0`: render at 12 px inside 16×16 with a guaranteed 1-px dark
  outline, plus a 2-colour variant `SHKIB0` for the low-ammo blink
  (A_SetInventoryIcon if supported, else HUD widget via MH).

**A4 Gibs/props (new, optional).** `SHKXA0`–`C0` are fragments of a broken
star, for when a star hits a `NOBLOOD` metal actor or after the 8th
re-use (see B5).

**A5 Palette tests (fixes D17).** Keep the fallback hashes and add a second
pinned set generated from a **checked-in 768-byte Doom palette fixture**
(`tests/fixtures/playpal_doom.lmp`; PLAYPAL is factual palette data, but if
licensing is a concern use Freedoom's PLAYPAL). This is the set players
actually see.

**A6 Variants (fixes D16).** Implement `style ∈ {4point, 6point, star8,
kunai}` and `skin ∈ {steel, black, gold}` as different polygon tables and
ramp tables. All are manifest-driven and fingerprinted, so netplay stays
identical. `kunai` is a throwing knife, a single sprite with 8 rotations.
It reuses everything else.

#### B. Feel and gameplay

**B1 Timings (fixes D9).** Settle the numbers in `tables.rs` and make the
states read them:

| Chain | States (tics) | Held interval |
|---|---|---|
| Fire | wind `B` 3 → release `F` 1 *(fire)* → follow `D` 4 → `A` 3 + `A_ReFire` | **11** |
| Fire (refire held) | skip `A`: `D` 4 → `A_ReFire` → `B` | **8** after the first throw ("flurry"), 3 stars/s+ |
| AltFire | wind `E` 5 → release `F` 1 *(3 stars)* → `D` 6 → `A` 6 + `A_ReFire` | **18** |

Add a native test that walks the registered state chain and sums durations
(the same method docs/30 §1 uses for vanilla weapons).

**B2 AltFire ammo (fixes D10).** Use `A_JumpIfInventory("ShurikenAmmo", 3,
"AltFireOK")` at the top of AltFire; otherwise go to `Fire`. Or, once MA2
exposes `Weapon.AmmoUse2` and dual ammo (BD4 already implements
`ammo_type2`/`ammo_use2` in `try_consume_ammo`), set `AmmoType2 =
ShurikenAmmo`, `AmmoUse2 = 3` and fire the three stars with `useammo=0`
after one depletion. The engine then refuses AltFire with < 3 ammo, as
GZDoom does.

**B3 Impact feedback (fixes D11).**
* Wall/floor: spawn a `ShurikenSpark` actor (2 frames, `+BRIGHT`, 4 tics,
  generated 6×6 sprite) and emit `FX_IMPACT_METAL` through the docs/29 K FX
  channel (render-only sparks and decal). The decal `ShurikenNick` is a
  generated 4×4 patch registered through DECALDEF-equivalent API, or skipped
  if there is no decal API yet.
* Flesh: the normal blood path already runs. Add a short `FX_HIT_CONFIRM`
  (hit-marker tick, docs/29 I) for the shooter.
* Crit: a backstab bonus. If the victim is not targeting the shooter
  (`victim.target != shooter`) and the hit comes from within ±60° of its
  back, deal ×3 damage. This is deterministic, computed in the impact
  callback from actor angles and applied as extra `damage_mobj`. It rewards
  the silent design.

**B4 Sound pass (fixes D4, D12).**
* Normalise every recipe to a −1 dBFS peak (|s| ≤ 115) **before**
  quantisation, with an integer peak scan and integer scaling, and pin the
  new hashes. Add a test that asserts `20 ≤ peak ≤ 120` for each lump.
* Throw: layer the noise burst with a 1.2→0.6 kHz falling sine "zip" at
  −12 dB, 140 ms.
* Raise: a metallic "shink", a 3-partial inharmonic ring (1, 2.76, 5.4 ratios)
  with a 250 ms exponential decay.
* Clink: lower the gain by 6 dB and add a random-free pitch variant
  `DSSHKCL2` (+3 semitones). `$random shuriken/clink { ... }` uses the
  playsim RNG, so it stays deterministic.
* New `DSSHKFLY`: a 350 ms seamless spin whistle loop played on the missile
  with `A_StartSound("shuriken/fly", CHAN_BODY, CHANF_LOOPING, 0.4,
  ATTN_STATIC)`. It is a plain sound, so there is no noise alert.

**B5 Retrieval (fixes D13, D14).**
* Ceiling hits: spawn `ShurikenDropped` with gravity, so the star falls,
  plays a clink on landing and stays as floor ammo.
* Blink: `ShurikenStuck` Spawn becomes `SHKS A (lifetime−105)` → 15×
  (`SHKS A 4` / `TNT1 A 3`) → Stop.
* Optional wear: each star carries a reuse counter in `Behaviour` (keyed by
  actor bits, like `stuck_in`). After the 8th reuse it breaks into A4
  fragments instead of being collectible, which caps infinite ammo loops.
  The default is off (`wear = 0` in the manifest).
* Magnet: picking up one star also collects any others within 48 units,
  so players can clear a cluster in one step instead of zig-zagging.

**B6 Placement (fixes D15).**
1. Candidate list: every **inventory pickup** spawned by the map (Clip,
   Shell, Stimpack, …), sorted by actor index, which is deterministic.
2. Skip candidates < 512 units from *any* player start, as well as those in
   damaging sectors and those whose sector is unreachable (no 2-sided line
   to the start sector's island; a simple flood over sectors is enough).
3. Spawn at the candidate's position **offset by 24 units** toward the
   sector centre with `CHECK_POSITION`. If that fails, try the next
   candidate (up to 32 tries).
4. Multiplayer: place one per 4 players. In BR (docs/28) defer to the BR
   loot table: register `Shuriken` as a common-tier loot entry instead of
   self-placing (`session_kind`/game mode check).

**B7 HUD and readability.**
* Ammo counter: show it through the status bar `ammo` slot (it already
  works as `ShurikenAmmo` is an Ammo subclass). In fullscreen HUD, add a
  `HudWidgetV1` (docs/27 §9) row of up to 10 small star icons that blink
  red at ≤ 3.
* Stuck-star markers: a tiny automap dot for stars within 512 units
  (mod HUD map layer if MH exposes it; else skip).

### 11.3 Acceptance

* `cargo test -p room-mod-doom1-shuriken` has new tests:
  * psprite visibility box (A1)
  * outline survives on every sprite: ≥ 1 dark pixel on every opaque
    boundary run (A2)
  * no border-touching pixels (A3)
  * sound peak range (B4)
  * state-chain tic sums matching `tables.rs` (B1)
  * `replace_clips` replaces `round(n·pct/100)` clips (D3)
* `room-game/tests/wasm_mod_shuriken.rs` adds tests for:
  * AltFire with 2 ammo falls back to Fire
  * a ceiling hit ends as a floor pickup
  * a stuck star blinks and then disappears at exactly `STUCK_LIFETIME_TICS`
  * the weapon exists after `WORLD_LOADED` on E1M1–E1M9 (doom1.wad when
    present, skipped otherwise)
* Golden: `room-cli render` of a test scene (player facing a wall with 3 stuck
  stars, a box and a weapon pickup in view, weapon in Ready) is added to
  `testdata/goldens/shuriken_*.png`, at 320×200 with the status bar and
  in fullscreen.
