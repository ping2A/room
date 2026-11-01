# doom1-shuriken

Throwing-star weapon mod (`docs/32`). Sprites, sounds, and actor classes are
**generated in code** during `room_register()` — nothing ships as art/sound
files.

Parents are **`Clip` / `Pistol`** (no standalone `Ammo`/`Weapon` actor bases in
the IWAD tables yet); labels and props fully override the inherited states.

## Status

- **SK0** — art pipeline with pinned SHA-256 hashes
- **SK1** — synth + SNDINFO aliases (pinned hashes)
- **SK2** — classes (ammo / box / stuck / weapon / missile), slot 8, Fire + AltFire
- **SK3** — impact / stick ring / drops / map placement
- **SK4** — pack via `room-server.sh mods`, e2e `wasm_mod_shuriken`

## Build / pack

```bash
cargo build --release --target wasm32-unknown-unknown
# or pack every mods/* entry:
./scripts/room-server.sh mods
# → mods/doom1-shuriken/shuriken.wad (ROOMMOD + SHURIKEN)
```

## Test

```bash
cargo test --manifest-path mods/doom1-shuriken/Cargo.toml
cargo test -p room-game --test wasm_mod_shuriken
```

## Play

Mount `shuriken.wad` with a Doom IWAD, then `give Shuriken` (or pick it up if
`place_weapon` is on). Slot **8** by default (`[options] slot` in `room.toml`).
Primary = one star; RMB / `+altattack` = fan of three.
