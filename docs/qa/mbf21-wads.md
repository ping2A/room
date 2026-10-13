# MBF21 / Boom / MBF public WAD checklist (M6)

Manual parity checks vs DSDA-Doom (or GZDoom complevel MBF21). Automate where a
committed `.fdem` / headless smoke exists.

## Automated

| Check | Command |
|-------|---------|
| Engine data has helper dog | `cargo test -p ferrite-game load_wadsrc_ferrite_engine_data` |
| Complevel → CompatFlags | `cargo test -p ferrite-sim` (compat / complevel tests) |
| MBF / MBF21 `A_*` | `cargo test -p ferrite-sim action::` |
| Boom gen height / SPAC | `cargo test -p ferrite-map` |
| Freedoom Complevel Vanilla/Boom demo | `cargo test -p ferrite-game --test freedoom_smoke freedoom2_map01_complevel_vanilla_and_boom_demo_verifies` |
| Freedoom DEMO1 `.lmp` parse | `cargo test -p ferrite-game --test freedoom_smoke freedoom2_demo1_lmp_parses` |
| Freedoom DEMO1 VanillaFixed smoke | `cargo test -p ferrite-game --test freedoom_smoke freedoom2_demo1_vanilla_smoke_ticks` |
| Chocolate Doom DEMO1 timedemo (local) | `scripts/demo1-chocolate-timedemo.sh` (expects ~2664 gametics) |
| DEMO1@70 vs Chocolate pos | Smoke golden ~(527.958, 989.147) ±0.5; Ferrite ~±0.03 after slide/floorz fixes |
| DEMO1@66–200 mom/pos | Matches patched Chocolate `P_Ticker` dump (incl. MAXMOVE/2 step split) |
| Freedoom demos (compat stamp) | `cargo test -p ferrite-game --test freedoom_smoke` |
| Helper dogs `-dogs` | `cargo test -p ferrite-sim spawn_helper_dogs_at_player_starts` |

## Manual / external WADs

Place WADs under `testdata/mbf21/` (gitignored commercial content OK locally).

| WAD / feature | Expected | Status |
|---------------|----------|--------|
| Thing 888 + `MTF_FRIEND` | Friendly dog follows / assists | [ ] |
| `-dogs` style coop starts | Friendly dogs at P2–P4 (when wired) | [ ] |
| Boom generalized floors (24/32 absolute) | Correct dest height | [ ] |
| One-shot vs repeatable lines | Special clears after first fire | [ ] |
| MBF21 `A_SpawnObject` / projectile | Offset spawn + velocity | [ ] |
| MBF21 `A_MonsterMelee` / `RadiusDamage` | Damage matches DSDA | [ ] |
| `COMPLVL` lump `mbf21` | `World.compat` bits match | [ ] |
| UMAPINFO `bossaction` | Line special on boss death | [ ] |
| InfightingGroup / ProjectileGroup | Same-group no damage | [ ] |
| DSDHacked high thing indices | `Deh_Actor_N` spawn | [ ] |

## Notes

- Dog sprites (`DOGS*`) / SFX (`DSDG*`) are not in Freedoom; SNDINFO aliases to
  demon lumps until a dog asset pack is mounted.
- Vanilla demo bit-sync remains the long pole; `SimMathMode::VanillaFixed` covers
  `try_move` + friction mul so far.
