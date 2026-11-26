# 30 — Weapons audit: problems found and how to fix them

**Audience:** an external coding agent working in the Room repo. Read
`AGENTS.md`, `docs/02-conventions.md` (§Determinism), `docs/07-playsim.md`,
`docs/08-actors.md` first.
**Status:** audit done 2026-10-31; **WF0**–**WF4** + re-audit **N1–N5/N7/N8** + **N3** + **N6** (Royale stay off) + **W8.3** landed. Open: N6 altdeath/clip counts, W8.4 movebob CVars, W12 melee FX flag.
Milestones `WF0`–`WF4` (§7); tracked in `docs/21-milestone-progress.md`.
**Reference behaviour:** vanilla Doom (`linuxdoom-1.10` `info.c`, `p_pspr.c`,
`p_user.c`, `g_game.c`) for timing, frames and RNG; GZDoom
(`wadsrc/static/zscript/actors/doom/weapon*.zs`, `doomweapons.zs`,
`p_pspr.cpp`) for DECORATE semantics. AGENTS.md rule 5: GZDoom/vanilla
behaviour wins for content compatibility.

---

## 0. Summary

| # | Problem | Weapons | Severity |
|---|---|---|---|
| W1 | Super Shotgun state table wrong: reload frames G/H missing, sounds on wrong frames, **flash uses reload frames G/H instead of I/J** → fires **~27 % faster** when held (45 vs 57 tics between shots; 50 vs 62 for a full tap-to-Ready sequence) and draws hands-with-shells over the gun as the muzzle flash | SSG | Critical |
| W2 | Chaingun uses **1 bullet per 2 shots** (ammo taken once on entering `Fire`, `A_FireCGun` takes none) and still fires both shots with 1 bullet left | Chaingun, DukeRipper | Critical |
| W3 | Auto-switch when out of ammo picks the **first owned weapon in slot order** → almost always the **Fist** (slot 1 needs no ammo) instead of plasma → SSG → chaingun → shotgun → pistol… | all | Critical |
| W4 | Weapon switch is applied **immediately, even mid-fire**: cancels the Fire animation; for Rocket (8-tic wind-up) and BFG (30-tic wind-up) switching before the shot **loses the ammo without firing** | all, esp. Rocket/BFG | High |
| W5 | Shotgun has **no Flash state** (no muzzle flash, no `extralight`) | Shotgun | High |
| W6 | Every flash frame except the chaingun's lacks **`Bright`**; the psprite is also drawn **unlit** (fullbright) regardless of sector light, `extralight`, `Bright`, fuzz | all | High |
| W7 | Flash logic: flash restarted by `A_FireMissile` / `A_FireBFG` (vanilla doesn't) → rocket/BFG flash restarts and runs too long; chaingun 2nd shot shows `CHGF A` instead of `CHGF B`; plasma always `PLSF A` instead of a random A/B (**misses a `P_Random` call → demo desync risk**); `A_FireBullets` (DECORATE/Duke weapons) never starts a flash | Rocket, BFG, Chaingun, Plasma, custom | High |
| W8 | Weapon bob: horizontal/vertical sway period **20 tics instead of 64** (3.2× too fast; a regression — the code comment says it "fixed" the correct π/32); bob amplitude formula linear (`speed·0.5`) instead of vanilla `(momx²+momy²)/4` capped at 16 → too weak when walking; bob applied while firing / switching | all | High |
| W9 | Slot keys can't reach the second weapon of a slot: key **1 always picks Chainsaw** (Fist unreachable once you own it, even with Berserk), key **3 always picks SSG** (Shotgun unreachable) | Fist/Chainsaw, Shotgun/SSG | High |
| W10 | Melee: `A_Punch` plays the punch sound **on every swing** (vanilla: only on hit) and has **no angle jitter** (`P_SubRandom()<<18`, 2 RNG calls missing); `A_Saw` has no angle jitter, range 64 instead of 65 (`MELEERANGE+1`), and **snaps** the player's angle onto the target instead of vanilla's gradual pull (±ANG90/20) | Fist, Chainsaw, DukeKick | Medium |
| W11 | Chainsaw: Ready loop is `SAWG C` only (vanilla `SAWG C` / `SAWG D` vibrate), **no `Spawn` state** (map pickup has no sprite), idle sound depends on tic phase (`maptime % 4` checked only on state entry every 4 tics → plays every time or **never**) | Chainsaw | Medium |
| W12 | Render FX `WeaponFired` (docs/29 I1/I5 muzzle flash + view kick) only emitted by pistol, chaingun and `A_FireBullets`; missing for shotgun, SSG, rocket, plasma, BFG, punch, saw | most | Medium |
| W13 | No raise on spawn / level start / respawn (weapon appears at `WEAPONTOP`); on death the weapon vanishes instantly instead of lowering | all | Low |
| W14 | Ammo is consumed when entering `Fire` (in `fire_weapon` / `A_ReFire`), not in the `A_Fire*` action like vanilla/GZDoom → HUD ammo drops 3–20 tics early; root cause of W2 and the ammo loss in W4 | all | Medium (structural) |
| W15 | `A_CheckReload` jumps to `Ready` instead of `P_CheckAmmo` (switch weapon) | SSG | Low |
| W16 | Weapons are declared `: Actor`, not `: Weapon`; there is no `Weapon` base with `LightDone`, so `Goto LightDone` doesn't resolve (works only by fallback), DeHackEd frame 1 (`S_LIGHTDONE`) can't be patched, and weapon properties (`Weapon.SelectionOrder` for most weapons, `BobStyle/BobSpeed/BobRangeX/Y`, `YAdjust`, `UpSound/ReadySound`) are missing | all | Medium (structural) |
| W17 | No "weapon stay" in netgames: vanilla coop / deathmatch 1 leave weapons on the map (each player picks a weapon once); Room removes them → second coop player gets nothing | all (netplay, Battle Royale) | Medium |
| W18 | Pistol has no `Spawn` state (GZDoom `PIST A -1`; needed for dropped pistols / custom maps) | Pistol | Low |

Weapons checked: Fist, Chainsaw, Pistol, Shotgun, Super Shotgun, Chaingun,
Rocket Launcher, Plasma Rifle, BFG9000 (`wadsrc/room/actors/doom.txt`),
the shared player weapon logic (`crates/room-sim/src/think_player.rs`
`set_psprite` … `try_consume_ammo`), the fire actions
(`crates/room-sim/src/action.rs` `a_punch`, `a_saw`, `a_fire_*`,
`player_gun_shot`, `spawn_player_missile`), pickup (`pickup.rs`), slots
(`weapon_slots.rs`), HUD/bob (`world.rs` `player_hud`), psprite drawing
(`room-ui/src/weapon.rs`, `room-app/src/app.rs` / `room-web`), and the Duke
converter guns (`crates/room-build/src/duke.rs` `GUNS`).

What is correct (keep): Fist, Pistol, Shotgun, Chaingun, Rocket, Plasma and
BFG **Fire** frame timings; raise/lower speed 6, `WEAPONTOP` 32 /
`WEAPONBOTTOM` 128; pistol/chaingun first-shot accuracy via `refire`;
shotgun 7 pellets / SSG 20 pellets with vanilla spread and RNG order; bullet
damage dice; missile speeds and spawn height (z+32); ammo give amounts,
dropped-half and skill-doubled ammo; auto-switch to a newly picked weapon.

## 0.1 Status after the fixes (re-audit 2026-10-31 17:15)

Re-checked the working tree (uncommitted changes in `doom.txt`,
`think_player.rs`, `action.rs`, `world.rs`, `pickup.rs`, `class.rs`,
`room-ui/src/weapon.rs`, `draw.rs`, `canvas_bridge.rs`, both shells,
`room-build/src/duke.rs`). `docs/21` lists WF1/WF2 in progress and WF3/WF4 landed.

| # | Status | Notes |
|---|---|---|
| W1 SSG table | ✅ done | G/H reload frames, sounds on F/H, flash `SHT2 I 5 / J 4 Bright`; test `ssg_fire_includes_gh_reload_and_ij_flash`. |
| W2 chaingun ammo | ✅ done for Doom actions | `A_FireCGun` depletes per shot, sound before the check; test `chaingun_one_bullet_fires_once`. **But see N2** (generic actions). |
| W3 auto-switch order | ✅ Doom + Duke | Explicit Duke `SelectionOrder` values (N3). |
| W4 switch mid-fire | ✅ done | `request_weapon` only sets `pending_weapon`; `A_WeaponReady` enters `Deselect`. |
| W5 shotgun flash | ✅ done | |
| W6 Bright + psprite light | ✅ done | Bright + scalelight psprite light (N8) + fuzzoffset fuzz. |
| W7 flash selection | ✅ done | Fire actions request flash via `psprite_flash_request` after deplete; index-based `flash0+offset` (N1/N4/N5). |
| W8 bob | ✅ mostly | Amplitude + period 64 + **W8.3 freeze** in Ready only. Optional `movebob`/`wbobspeed` CVars still open. |
| W9 slot keys | ✅ done (GZDoom cycling) | Vanilla special cases still an open question (§8). |
| W10 melee | ✅ Punch & Saw / ✅ CustomPunch | Punch + Saw + CustomPunch jitter; CustomPunch uses `CPF_USEAMMO`. |
| W11 chainsaw | ✅ done | Spawn, Ready C/D; `Weapon.ReadySound` / `UpSound` parsed (N7). |
| W12 FX events | ⚠️ partial | Doom fires + `A_FireProjectile` FX. Still missing melee `melee` flag / rail. |
| W13 raise / lower | ✅ done | `Player::reset` parks at `WEAPONBOTTOM`; `bring_up_weapon` raises; death → `start_death_weapon_lower` (from `damage.rs`). |
| W14 ammo in actions | ✅ done | Doom + generic (`A_FireBullets` / `A_FireProjectile` / `A_RailAttack` / `A_CustomPunch`) with useammo flags (N2). |
| W15 `A_CheckReload` | ✅ done | |
| W16 `Weapon` base / `LightDone` | ✅ done | `actor Weapon` with `LightDone`; Doom guns inherit; Goto resolves parent labels (N1). |
| W17 weapon stay | ⚠️ partial | Coop/DM stay on; Royale sets `WEAPON_STAY 0` on load/loot (N6). Still open: altdeath + netgame clip counts. |
| W18 pistol spawn | ✅ done | |
| WF0 timeline harness | ✅ done | `testdata/weapons/*.txt` + `room-sim` `weapon_timeline` tests for all 9 Doom guns (Fire/Flash + held/full). |

### New problems found in the re-audit

| # | Status | Notes |
|---|---|---|
| **N1** LightDone / flash offsets | ✅ done | `Weapon` base; unresolved Goto → Stop; `flash0+offset`; test `weapon_lightdone_goto_stops_not_fallthrough` + `chaingun_second_shot_uses_flash_b`. |
| **N2** generic fire ammo | ✅ done | `FBF_USEAMMO` / `useammo` / `CPF_USEAMMO` / rail useammo; only when `psprite_weapon` is set. |
| **N3** Duke SelectionOrder | ✅ done | Explicit orders in `duke.rs` (Kick 3700, not slot×1000). |
| **N4** plasma flash RNG order | ✅ done | `A_FirePlasma` rolls + requests flash before spawn. |
| **N5** flash on empty shot | ✅ done | Flash only after successful deplete. |
| **N6** Royale weapon stay | ⚠️ partial | `WEAPON_STAY 0` on Loaded + Loot. Still open: altdeath + netgame clip counts + royale pickup assert. |
| **N7** ReadySound / UpSound | ✅ done | Parsed on `ClassDef`; used in Ready / raise. |
| **N8** psprite light table / fuzz | ✅ done | `psprite_scalelight` (Doom scalelight ends); fuzzoffset ±1-row darken in `canvas_bridge`. |
| **N9** held ROF doc | ✅ doc fix | — |
| **W8.3** sway freeze | ✅ done | `pspr_bob_x/y` written only in `A_WeaponReady`; HUD uses frozen values. |

### Next steps (in order)
1. N6 remainder (altdeath, netgame clip counts, royale pickup assert) + demo suites for N4.
2. W12 remaining FX (melee flag / rail); optional W8.4 `movebob`/`wbobspeed`.


---

## 1. Per-weapon reference vs. Room

Two numbers per weapon: **held** = tics between shots while fire is held
(`A_ReFire` executes when its frame *starts*, so that frame's tics are not
spent), and **full** = a single tap from entering `Fire` back to `Ready`.
"Room (audit)" is the state found in the first audit; §0.1 has the current
status.

| Weapon | Vanilla Fire sequence | Held | Full | Room (audit) | Frames / flash differences (audit) |
|---|---|---|---|---|---|
| Fist | B4 C4(A_Punch) D5 C4 B5(A_ReFire) | **17** | 22 | ✓ | W10 |
| Chainsaw | A4(A_Saw) B4(A_Saw) B0(A_ReFire) | **8** | 8 | ✓ | Ready `SAWG C 4` / `SAWG D 4` both `A_WeaponReady`; Spawn `CSAW A -1` (W11) |
| Pistol | A4 B6(A_FirePistol) C4 B5(A_ReFire) | **14** | 19 | ✓ | Flash `PISF A 7 Bright A_Light1` |
| Shotgun | A3 A7(A_FireShotgun) B5 C5 D4 C5 B5 A3 A7(A_ReFire) | **37** | 44 | ✓ | Flash `SHTF A 4 Bright A_Light1`, `SHTF B 3 Bright A_Light2` (W5) |
| Super Shotgun | A3 A7(A_FireShotgun2) B7 C7(A_CheckReload) D7(A_OpenShotgun2) E7 F7(A_LoadShotgun2) G6 H6(A_CloseShotgun2) A5(A_ReFire) | **57** | 62 | ✗ 45 / 50 | Flash `SHT2 I 5 Bright A_Light1`, `SHT2 J 4 Bright A_Light2` (W1) |
| Chaingun | A4(A_FireCGun) B4(A_FireCGun) B0(A_ReFire) | **8** (2 bullets) | 8 | ✗ 1 bullet | Flash `CHGF A 5 Bright A_Light1` after frame A, `CHGF B 5 Bright A_Light2` after frame B (W2, W7, N1) |
| Rocket Launcher | B8(A_GunFlash) B12(A_FireMissile) B0(A_ReFire) | **20** | 20 | ✓ | Flash `MISF A 3 Bright A_Light1`, `MISF B 4 Bright` (no action), `MISF C 4 Bright A_Light2`, `MISF D 4 Bright A_Light2` (W6, W7) |
| Plasma Rifle | A3(A_FirePlasma) B20(A_ReFire) | **3** | 23 | ✓ | Flash random `PLSF A 4 Bright A_Light1` or `PLSF B 4 Bright A_Light1` via `P_Random()&1` drawn **before** the missile spawn (W7, N1, N4) |
| BFG9000 | A20(A_BFGsound) B10(A_GunFlash) B10(A_FireBFG) B20(A_ReFire) | **40** | 60 | ✓ | Flash `BFGF A 11 Bright A_Light1`, `BFGF B 6 Bright A_Light2` (W6, W7) |

---

## 2. Findings in detail, with fixes

### W1 — Super Shotgun table (`wadsrc/room/actors/doom.txt`, `actor SuperShotgun`)
Replace `Fire` and `Flash` with the vanilla sequence:
```
  Fire:
    SHT2 A 3
    SHT2 A 7 A_FireShotgun2
    SHT2 B 7
    SHT2 C 7 A_CheckReload
    SHT2 D 7 A_OpenShotgun2
    SHT2 E 7
    SHT2 F 7 A_LoadShotgun2
    SHT2 G 6
    SHT2 H 6 A_CloseShotgun2
    SHT2 A 5 A_ReFire
    Goto Ready
  Flash:
    SHT2 I 5 Bright A_Light1
    SHT2 J 4 Bright A_Light2
    Goto LightDone
```
(Vanilla `S_DSGUNFLASH1` = I, 5 tics; `S_DSGUNFLASH2` = J, 4 tics. GZDoom uses
I 4 / J 3 because its flash starts on the same tic; match whichever the
psprite timing test (§6) proves correct against vanilla: the flash must end
on the same tic as in vanilla.) Also add `Weapon.SelectionOrder 400`.

### W2 + W14 — ammo is taken in the wrong place (`think_player.rs`, `action.rs`)
Vanilla `P_FireWeapon` only **checks** ammo (`P_CheckAmmo`); each `A_Fire*`
**removes** it (`A_FireCGun` returns without shooting if the ammo is 0).
GZDoom: `DepleteAmmo` inside the actions.

Fix:
1. `fire_weapon` and the `A_ReFire` branch call a new `check_ammo(world, player, mo, wpn) -> bool`
   (no removal; on failure: pick the next weapon via W3 order and set the
   pending weapon, then enter `Deselect` — vanilla `P_CheckAmmo` sets
   `pendingweapon` and the down state).
2. Each fire action depletes: `A_FirePistol`, `A_FireCGun`, `A_FireShotgun`
   (1), `A_FireShotgun2` (2), `A_FireMissile`, `A_FirePlasma` (1),
   `A_FireBFG` (`AmmoUse`, 40 by default; DeHackEd `BFG cells/shot`),
   `A_FireBullets` / `A_FireProjectile` / `A_FireCustomMissile` /
   `A_RailAttack` (`AmmoUse` unless the `useammo` argument is false), all via
   one helper `deplete_ammo(world, mo, weapon) -> bool` using
   `world.psprite_weapon`.
3. `A_FireCGun`: play the sound first (vanilla plays it even when empty),
   then `if !deplete_ammo { return }`, then flash (W7) and shoot.
4. Remove the up-front `try_consume_ammo` calls.

### W3 — auto-switch order (`think_player.rs` `best_weapon_with_ammo`, `drop_unowned_weapon`)
Use `Weapon.SelectionOrder` (lower = preferred), like GZDoom
`PickNewWeapon`, and skip weapons without enough ammo. Add the missing
values to `doom.txt` (GZDoom defaults):

| Weapon | SelectionOrder |
|---|---|
| PlasmaRifle | 100 |
| SuperShotgun | 400 |
| Chaingun | 700 |
| Shotgun | 1300 |
| Pistol | 1900 |
| Chainsaw | 2200 |
| RocketLauncher | 2500 |
| BFG9000 | 2800 |
| Fist | 3700 |

This reproduces vanilla `P_CheckAmmo` (plasma, SSG, chaingun, shotgun,
pistol, chainsaw, rocket, BFG, fist). Weapons with no SelectionOrder default
to a large value. Duke converter guns (`duke.rs` `GUNS`) need values too
(e.g. Ripper 700, Shotgun 1300, Pistol 1900, RPG 2500, Devastator 2600,
Shrinker/Expander 2700, Freezer 800, Kick 3700). Keep slot order for
next/prev cycling only.

### W4 — switching mid-fire (`think_player.rs` `request_weapon`)
Vanilla only lowers the weapon in `A_WeaponReady` (and via `P_CheckAmmo`).
`request_weapon` must only set `player.pending_weapon`; the switch starts
when `A_WeaponReady` runs and sees `pending_weapon.is_some()` (or health ≤ 0)
→ `Deselect`. Exceptions: no current weapon, or a weapon without `Ready`.
Also handle `A_ReFire` refusing to refire while a weapon is pending (already
done). With W14, ammo is no longer lost because nothing is taken before the
`A_Fire*` action runs.

### W5 — Shotgun flash
```
  Flash:
    SHTF A 4 Bright A_Light1
    SHTF B 3 Bright A_Light2
    Goto LightDone
```
(and `Weapon.SelectionOrder 1300`).

### W6 — `Bright` flags and psprite lighting
1. Add `Bright` to every flash frame: Pistol `PISF A`, Rocket `MISF A–D`,
   Plasma `PLSF A/B`, BFG `BFGF A/B`, SSG `SHT2 I/J`, Shotgun `SHTF A/B`.
2. Psprites are drawn as plain 2D patches (`room-ui/src/weapon.rs`
   `draw_weapon` → `UiDrawList::patch`), so they are always fullbright.
   Vanilla lights the weapon with the player's sector light plus
   `extralight` (`R_DrawPSprite`: `scalelight[(lightlevel>>4)+extralight][MAXLIGHTSCALE-1]`),
   fullbright for `Bright` frames, fixed colormaps for invulnerability /
   light amp, and draws it **fuzzed** with partial invisibility.
   Fix: extend `PlayerHud` (`world.rs` `player_hud`) with
   `weapon_light: f32` (sector light level of the player's sector, +
   `extralight`·16, clamped), `weapon_bright: bool`, `flash_bright: bool`,
   `weapon_fuzz: bool` (BlurSphere power); add a `light` / `colormap` /
   `fuzz` field to the patch draw command in `room-ui` (`DrawCmd::Texture`
   already has `color`), apply it in `draw_weapon` for both shells
   (`room-app/src/app.rs` and `room-web`), use the same light mode
   equation as the world (`docs/29` U4) so the gun matches the room; keep the
   existing invulnerability inversion.

### W7 — flash selection (`think_player.rs` `is_fire_action`, `start_flash_psprite`; `action.rs`)
1. Remove `A_FireMissile`, `A_FireBFG`, `A_Punch`, `A_Saw` from
   `is_fire_action` (vanilla: only `A_GunFlash` starts those flashes; melee
   has no flash).
2. Add a flash **offset**: `start_flash_psprite(world, player, offset: usize)`
   enters the `offset`-th state of the `Flash` label's sequence (states are
   contiguous; a `Goto` creates no state), validated to stay inside the
   weapon's own states (GZDoom `SetSafeFlash`).
   - `A_FireCGun`: offset = index of the current Fire state minus the first
     Fire state (frame A → 0, frame B → 1). Restructure the chaingun flash as
     `CHGF A 5 Bright A_Light1 / Goto LightDone / CHGF B 5 Bright A_Light2 / Goto LightDone`.
   - `A_FirePlasma`: offset = `P_Random() & 1` using the **vanilla RNG**
     (same table/index as other player weapon randomness) — this RNG call is
     required for demo compatibility. Flash: `PLSF A 4 Bright A_Light1 /
     Goto LightDone / PLSF B 4 Bright A_Light1 / Goto LightDone`.
   - `A_FirePistol`, `A_FireShotgun`, `A_FireShotgun2`: offset 0.
3. `A_FireBullets` starts the flash unless the `FBF_NOFLASH` flag is set
   (GZDoom); `A_FireProjectile`/`A_FireCustomMissile` don't (weapons use
   `A_GunFlash`). `A_GunFlash(state, flags)` with an optional state label.
4. Rocket flash frame B: remove `A_Light2` (vanilla: no action on `MISF B`).

### W8 — bob (`think_player.rs` bob, `world.rs` `player_hud`)
1. Amplitude (vanilla `P_CalcHeight`): `bob = (momx² + momy²) / 4` in map
   units (fixed-point `FixedMul(momx,momx)+FixedMul(momy,momy) >> 2`), capped
   at `MAXBOB = 16`. Today `bob = speed·0.5`. Use the playsim's
   pre-friction momentum exactly as vanilla (bob is computed in
   `P_CalcHeight` from `mo->momx/momy`). The **view** bob (`bob/2 · sin(2π·t/20)`)
   is already right except for the amplitude source.
2. Weapon sway (vanilla `A_WeaponReady`): `angle = (128·leveltime) & FINEMASK`
   → **period 64 tics**: `bob_x = bob · cos(2π·t/64)`,
   `bob_y = bob · |sin(2π·t/64)|` (the `& (FINEANGLES/2−1)` mask gives the
   positive half). Today `t·π/10` (period 20). Fix the constant to `π/32` and
   correct the comment (it claims π/32 was wrong).
3. Only update sway while in `A_WeaponReady`: store `pspr_bob_x/y` on the
   player, written by `A_WeaponReady`; during Fire/Select/Deselect the last
   values are kept (vanilla). Optional GZDoom-style smoothing (`r_weaponbob_smooth`)
   that eases back to centre is a render-only option.
4. Expose GZDoom's `movebob` / `wbobspeed` CVars (render scaling of the sway,
   never the sim `bob`, so netplay stays deterministic).

### W9 — slot keys (`think_player.rs` `select_weapon_slot`)
GZDoom behaviour: pressing a slot key while holding a weapon of that slot
selects the **next owned weapon in the slot** (wrapping); otherwise the first
owned one. Implement that, plus the vanilla special cases when
`compat_*`/vanilla mode is active: key 1 with Chainsaw owned picks the
Chainsaw unless you are holding it **and** have Berserk (then Fist); key 3 in
Doom 2 picks the SSG unless you are holding it (then Shotgun).

### W10 — melee (`action.rs` `a_punch`, `a_saw`, `a_custom_punch`)
Vanilla `A_Punch`:
```
damage = (P_Random()%10+1)<<1;            // ×10 with berserk
angle  = mo->angle + (P_SubRandom()<<18); // 2 RNG calls
slope  = P_AimLineAttack(mo, angle, MELEERANGE);
P_LineAttack(mo, angle, MELEERANGE, slope, damage);
if (linetarget) { S_StartSound(mo, sfx_punch); mo->angle = R_PointToAngle2(mo, linetarget); }
```
Vanilla `A_Saw`:
```
damage = 2*(P_Random()%10+1);
angle  = mo->angle + (P_SubRandom()<<18);
slope  = P_AimLineAttack(mo, angle, MELEERANGE+1);   // range 65
P_LineAttack(mo, angle, MELEERANGE+1, slope, damage);
if (!linetarget) { S_StartSound(sfx_sawful); return; }
S_StartSound(sfx_sawhit);
// turn toward target gradually:
angle = R_PointToAngle2(mo, linetarget);
if (angle - mo->angle > ANG180) {
    if ((int)(angle - mo->angle) < -ANG90/20) mo->angle = angle + ANG90/21;
    else mo->angle -= ANG90/20;
} else {
    if (angle - mo->angle > ANG90/20) mo->angle = angle - ANG90/21;
    else mo->angle += ANG90/20;
}
mo->flags |= MF_JUSTATTACKED;
```
Fix both to match (punch sound only on hit; RNG order damage → jitter;
saw gradual turn; saw range 65; `MF_JUSTATTACKED`). `A_CustomPunch`: GZDoom
also jitters the angle (`spread`) and plays `HitSound`/`MissSound` per flags.
**Demo note:** this adds 2 RNG calls per swing, as vanilla does. Run the demo
suites (§6) — if a demo currently syncs *because* the calls are missing,
something else is compensating; find it rather than skipping the fix.

### W11 — chainsaw
```
  Spawn:
    CSAW A -1
    Stop
  Ready:
    SAWG CD 4 A_WeaponReady
    Loop
```
Idle sound: vanilla plays `sfx_sawidl` in `A_WeaponReady` only when the
psprite is on the first Ready state (`S_SAW`, frame C) → every 8 tics. Replace
the `maptime % 4 == 0` test with "current state is the first state of the
`Ready` label" (GZDoom uses `ReadySound` on the weapon: add
`Weapon.ReadySound "weapons/sawidle"` and `Weapon.UpSound "weapons/sawup"`
and play them generically instead of the class-name checks for "Chainsaw" in
`set_psprite` / `bring_up_weapon`).

### W12 — FX events (`action.rs`)
Call `push_weapon_fired_fx` from every player fire action: `A_FireShotgun`,
`A_FireShotgun2`, `A_FireMissile`, `A_FirePlasma`, `A_FireBFG` (at the shot,
not at `A_BFGsound`), `A_FireProjectile` / `A_FireCustomMissile`,
`A_RailAttack`, `A_Punch` / `A_Saw` / `A_CustomPunch` (with a `melee` flag so
the renderer doesn't draw a muzzle flash). Render-only; no sim change.

### W13 — raise on spawn, lower on death
- Spawn / level start / respawn: vanilla `P_BringUpWeapon` sets
  `pspr_y = WEAPONBOTTOM` and enters `Select` (16 tics of raise). Do the same
  in `ensure_psprite` when entering a level or respawning (not for
  save-game loads).
- Death: `A_WeaponReady` with health ≤ 0 enters `Deselect`; `A_Lower` with
  a dead player keeps the weapon at `WEAPONBOTTOM` (vanilla). Remove the
  instant hide in `player_hud` (`.filter(|_| alive)`) once lowering works.

### W15 — `A_CheckReload`
Call the W2 `check_ammo` (switch weapon when not enough ammo) instead of
jumping to `Ready`; the state sequence continues as in vanilla.

### W16 — `Weapon` base class (`wadsrc/room/actors/doom.txt`)
Add, like GZDoom:
```
actor Weapon : Inventory          // or : Actor if Inventory isn't a base yet
{
  Weapon.Kickback 100
  Weapon.BobSpeed 1.0
  Weapon.BobRangeX 1.0
  Weapon.BobRangeY 1.0
  Weapon.YAdjust 0
  States
  {
  LightDone:
    SHTG E 0 A_Light0
    Stop
  }
}
actor DoomWeapon : Weapon { Weapon.Kickback 100 }
```
Derive the nine Doom weapons from `DoomWeapon`. Make `Goto LightDone`
resolve to the inherited state, map DeHackEd frame 1 to it
(`wadsrc/room/dehacked/states.txt` already names `Weapon LightDone`), and
parse/honour `Weapon.SelectionOrder`, `ReadySound`, `UpSound`, `YAdjust`,
`BobStyle`, `BobSpeed`, `BobRangeX/Y` in `room-sim/src/class.rs`.

### W17 — weapon stay (`pickup.rs`)
Vanilla `P_GiveWeapon`: in a netgame with `deathmatch != 2` and a
non-dropped weapon, the weapon is **not removed**; a player who already owns
it can't take it again; ammo is given once. Implement via the existing game
rules (`World` deathmatch flags; GZDoom `sv_weaponstay` / `dmflags`). Mods
(Battle Royale loot) must be able to turn it off (`game_set_rule`), so add a
`rule::WEAPON_STAY` selector to `room-script-abi` and default it from the
vanilla rule.

### W18 — Pistol spawn state
Add `Spawn: PIST A -1 Stop` (GZDoom).

---

## 3. Duke converter guns (`crates/room-build/src/duke.rs` `GUNS`)

The GRP converter builds DECORATE guns on top of the Doom weapons, so they
inherit W2/W3/W4/W7/W14. Specific items:
- `DukeRipper` calls `A_FireCGun` twice per cycle → same half-ammo bug (W2).
- `DukeRPG`, Shrinker, Expander, Devastator use `A_FireCustomMissile`; their
  ammo is taken on entering Fire (W14) and lost if switched during the
  4-tic wind-up (W4).
- Only pipebomb/tripbomb set `Weapon.SelectionOrder` (5000) → add values
  (§2 W3).
- Duke flashes are generated with `Bright` and `A_Light1` on the first frame
  only — fine — but the pistol/ripper flash frame is `HIDDEN` (TNT1), so the
  muzzle flash relies on the render FX (W12).
- Timings are hand-written approximations; the native Build runtime track
  (`docs/25`, `room-duke`, DEMO1 bit-exact) is the reference — once D6 is
  done, generate converter timings from the CON data or compare tic-by-tic.

---

## 4. Where to change what

| File | Changes |
|---|---|
| `wadsrc/room/actors/doom.txt` | W1, W5, W6 (Bright), W7 (flash layouts), W11, W16, W18, SelectionOrder values (W3) |
| `crates/room-sim/src/think_player.rs` | W2/W14 (`check_ammo`, remove up-front consume), W3, W4, W7 (`is_fire_action`, flash offset), W8 (bob amplitude + sway written in `A_WeaponReady`), W9, W11 (sounds), W13, W15 |
| `crates/room-sim/src/action.rs` | W2 (`deplete_ammo` in every `A_Fire*`), W7 (`A_FireCGun`/`A_FirePlasma` flash offsets, `A_FireBullets` flash), W10, W12 |
| `crates/room-sim/src/world.rs` | W8 (sway period, use stored sway), W6 (`PlayerHud` light fields), W13 (no instant hide) |
| `crates/room-sim/src/class.rs` | W16 (weapon properties) |
| `crates/room-sim/src/pickup.rs` | W17 |
| `crates/room-script-abi/src/lib.rs` | W17 (`rule::WEAPON_STAY`) |
| `crates/room-ui/src/weapon.rs`, `draw.rs`; `room-app/src/app.rs`; `room-web/src/*` | W6 psprite lighting / fuzz |
| `crates/room-build/src/duke.rs` | §3 |
| `crates/room-dehacked` | W16 frame 1 mapping, BFG cells/shot → `AmmoUse` |

---

## 5. Things to watch

- **Determinism / demos.** W7 (plasma `P_Random`), W10 (melee jitter) and W2
  (chaingun ammo, affects when weapons auto-switch) change RNG consumption
  or game state exactly toward vanilla. Re-run every demo suite after each
  change; a demo that breaks must be investigated, not "fixed" by removing
  the vanilla behaviour.
- **Netplay / saves.** New player fields (`pspr_bob_x/y`, weapon-stay
  ownership) must be saved and included in the world checksum.
- **Mods.** The Battle Royale and Survival mods take and give weapons
  through inventory imports; W3/W4/W17 change what happens after a
  weapon is taken (`drop_unowned_weapon`). Re-run
  `cargo test -p room-game --test wasm_mod_royale` and `--test wasm_mod_survival`.
- **KEYCONF / DeHackEd / DECORATE mods** that replace weapons must keep
  working: all logic must be label- and property-driven, not class-name
  checks (remove the `"Chainsaw"` name tests).

---

## 6. Tests to add

1. **Psprite timeline test per weapon** (`crates/room-sim` or
   `crates/room-game/tests/weapon_timing.rs`): give the weapon, hold fire for
   N tics on a test map, record per tic `(weapon state sprite+frame,
   flash sprite+frame, extralight, ammo)`; compare with a table transcribed
   from vanilla `info.c` (held and full intervals from §1) (store under `testdata/weapons/<weapon>.txt`). Covers
   W1, W2, W5, W7, W14 and the correct timings.
2. **Ammo**: chaingun with 1 bullet fires exactly one shot; with 0 bullets
   `A_FireCGun` plays the sound and doesn't shoot; SSG with 1 shell can't fire
   and switches to the next weapon (W15).
3. **Auto-switch order**: own Fist, Pistol, Shotgun, Chaingun, Plasma; empty
   the current weapon → expect Plasma, then Chaingun, … Fist last (W3).
4. **Switch during wind-up**: start firing the rocket launcher, press 2 at
   tic 3 → no rocket spawned, rocket ammo unchanged, switch happens after the
   Fire sequence returns to Ready (W4).
5. **Slot cycling**: own Chainsaw + Fist, press 1 twice → Chainsaw then Fist
   (W9).
6. **Bob**: walk at full speed; sway period 64 tics, amplitude 16 (W8);
   sway frozen while firing.
7. **Melee**: punch miss → no punch sound; saw hit turns the player by at most
   ANG90/20 per tic (W10); RNG index advances by 3 per punch.
8. **Weapon stay**: 2-player coop, player 1 picks the shotgun, it stays, player
   1 can't pick it again, player 2 can (W17).
9. **Psprite lighting golden**: weapon in a dark sector is dark; flash frames
   fullbright (W6).
10. Demo suites: `./scripts/verify-freedoom-demos.sh`, the Doom 1 demo tests
    in `room-game`, `cargo test -p room-game --test wasm_mod_royale`,
    `--test wasm_mod_survival`, `cargo test -p room-server --features verifier --test verifier_headless`.

---

## 7. Milestones

| ID | Scope | Acceptance |
|---|---|---|
| **WF0** | Timeline test harness + vanilla reference tables for the 9 Doom weapons (tests fail where §1 marks ✗) | Harness lands; failing cases listed in `docs/21`. |
| **WF1** | Data fixes in `doom.txt`: W1, W5, W6 (Bright), W7 flash layouts, W11, W18, SelectionOrder values, W16 `Weapon`/`DoomWeapon` base + `LightDone` | Timeline tests for SSG, Shotgun, Chainsaw pass (frames/flash); DeHackEd frame 1 patch test. |
| **WF2** | Logic fixes: W2/W14 ammo in actions, W3, W4, W7 flash offsets (+ plasma RNG), W9, W15 | Tests 1–5 pass; demo suites green. |
| **WF3** | Feel: W8 bob, W10 melee, W11 sounds, W13 raise/lower, W12 FX events | Tests 6–7 pass; demo suites green. |
| **WF4** | W6 psprite lighting/fuzz (both shells), W17 weapon stay + rule, Duke converter §3 | Tests 8–9 pass; mods and verifier tests green. |
| **WF5** | Re-audit fixes §0.1: N1 (W16 + Goto fix + index flash offsets), N2, N3, N4/N5, N6, N7, N8, W8.3, W12 rest | Chaingun 2nd-barrel flash test; Duke RPG ammo test; plasma demo stays in sync; Royale loot guns disappear on pickup; all demo/mod/verifier suites green. |

## 8. Open questions for the owner

1. SSG flash tics: follow vanilla (I 5 / J 4) or GZDoom (I 4 / J 3)? Recommend
   vanilla timing, validated by the timeline test.
2. Weapon sway: freeze while firing (vanilla) or ease to centre (GZDoom)
   by default?
3. Slot keys: vanilla special cases (chainsaw/berserk, SSG toggle) or
   GZDoom cycling by default?
4. Weapon stay default in Battle Royale rooms: off (each weapon is loot for
   one player) — confirm.
