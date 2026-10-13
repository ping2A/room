# Freedoom playthrough checklist (M5)

Manual + automated coverage for Freedoom Phase 1 / Phase 2 start→finish.

## Automated (CI)

| Check | Command / job |
|-------|----------------|
| MAP01 smoke `.fdem` | `cargo test -p room-game --test freedoom_smoke freedoom2_map01_committed_demo_verifies` |
| MAP01–03 `.fdbn` | `cargo test -p room-game --test freedoom_smoke freedoom2_committed_map01_03_bundle_verifies` |
| MAP01–10 `.fdbn` | `cargo test -p room-game --test freedoom_smoke freedoom2_committed_map01_10_bundle_verifies` |
| MAP11–20 `.fdbn` | `cargo test -p room-game --test freedoom_smoke freedoom2_committed_map11_20_bundle_verifies` |
| MAP21–30 `.fdbn` | `cargo test -p room-game --test freedoom_smoke freedoom2_committed_map21_30_bundle_verifies` |
| Phase 1 E1 `.fdbn` | `cargo test -p room-game --test freedoom_smoke freedoom1_committed_e1_bundle_verifies` |
| Phase 1 E2–E4 `.fdbn` | `… committed_e2` / `committed_e3` / `committed_e4` |
| MAP01–05 exit chain | `cargo test -p room-game --test freedoom_smoke freedoom2_map01_through_map05_exit_chain` |
| MAP01–10 exit chain | `cargo test -p room-game --test freedoom_smoke freedoom2_map01_through_map10_exit_chain` |
| MAP11–20 / 21–29 | `… map11_through_map20_exit_chain` / `map21_through_map29_exit_chain` |
| MAP30 finale | `… freedoom2_map30_exit_enters_finale` |
| Phase 1 E1M1→E1M2 | `cargo test -p room-game --test freedoom_smoke freedoom1_e1m1_headless_exit_to_e1m2` |
| Phase 1 E1M1–E1M3 | `cargo test -p room-game --test freedoom_smoke freedoom1_e1m1_through_e1m3_exit_chain` |
| Phase 1 E1–E4 | `… e1m1_through_e1m8` / `e2m1_through_e2m8` / `e3m1_through_e3m8` / `e4m1_through_e4m8` |
| CLI bundle verify | `cargo run -p room-cli -- demo verify --wad testdata/freedoom/freedoom2.wad testdata/demos/freedoom2_map01_03.fdbn` |

Regenerate goldens: `ROOM_WRITE_DEMO=1 cargo test -p room-game --test freedoom_smoke …`

## Manual desktop (`room-app --play`)

### Freedoom Phase 2

- [ ] MAP01 Entryway → exit → intermission → MAP02
- [ ] MAP15 secret exit → MAP31 → MAP32 → return MAP16
- [ ] MAP30 exit → finale
- [ ] Full MAP01–MAP30 start→finish without traps/crashes (skill UV)

### Freedoom Phase 1

- [ ] E1M1 → … → E1M8 → finale
- [ ] E1M3 secret → E1M9 → E1M4
- [ ] Episodes 2–4 start→finish

## Notes

- Demos use `doom_basic` DECORATE (`testdata/actors/doom_basic.txt`) and RNG seed `1993` unless noted.
- Cross-platform determinism: CI `determinism` job verifies demos natively and
  checks `.fdem`/`.fdbn` fingerprints on `wasm32-wasip1` (wasmtime).
