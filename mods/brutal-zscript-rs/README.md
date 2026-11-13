# Brutal Doom ZScript — Rust port

Brutal Doom has three small ZScript files. This mod is the same code in
Rust, as a Room WASM mod, so it can be read, changed and tested like any Rust
crate.

| Brutal Doom (`zscript/…`)                     | Here                                   |
|-----------------------------------------------|----------------------------------------|
| `DamageHandler.zc` — `Script_DamageMult`      | `src/damage.rs` `player_entered`, `thing_spawned` |
| `DamageHandler.zc` — `Damage_Control`         | `src/damage.rs` `modify_damage`; class in `src/lib.rs` |
| `BD_WCS.zc` — `ZandroCompat` (ACS `ScriptCall`) | `src/zandro.rs`                     |
| `BD_WCS.zc` — `ZC_TMapHack`, `ZC_IntStrMap`   | `src/lumps.rs`                         |

`room.toml` declares `replaces_zscript` for those two files, so when this mod
is mounted after Brutal Doom, Room leaves them out of the ZScript it
compiles and this module serves everything they did:

- the `ScriptCall("ZandroCompat", …)` functions Brutal Doom's ACS uses
  (lump reading, player TIDs, gender / face tokens, `IsClient`, …), with
  string arguments and string results;
- the skill-6 damage token: `Damage_Control` is given to players and
  monsters, and its `ModifyDamage` multiplies the damage they take from
  players and monsters (×7, fire ×3, `TehLostSoul` ×2).

## Play

```sh
cargo run -p room-app --release -- --iwad testdata/freedoom/freedoom1.wad \
  --file brutal-doom.pk3 --file mods/brutal-zscript-rs/brutal-zscript.wad
```

## Change it

1. Edit `src/damage.rs` or `src/zandro.rs`. Game calls go through the
   `Host` trait (`src/host.rs`), so the logic needs no engine to test.
2. `cargo test` (in this directory) — `src/tests.rs` checks each function
   against the ZScript it replaces, on a fake engine.
3. `./build.sh` — builds `wasm32-unknown-unknown` and packs
   `brutal-zscript.wad` (`ROOMMOD` + `BDZSCRPT`).

Engine checks (from the repository root):

- `cargo test -p room-sim --lib brutal_zscript_port` — the built module in
  the playsim: tokens, `ModifyDamage`, `ScriptCall` with strings.
- `ROOM_BRUTALDOOM_PK3=/path/to/bd cargo test -p room-game --test bd_rust_port`
  — Brutal Doom with its ZScript and with this port, 1,500 tics of Freedoom
  E1M1 with a save and load halfway, compared tic by tic.

## Differences from the ZScript

- `ZC_IntStrMap` is a plain map here. Brutal Doom's version can keep a
  stale reference count when three or more lumps are open at once; the
  port does not have that bug (Brutal Doom opens one lump at a time).
- `PlayerPawn(source)` is a real check here (players' damage is scaled
  too, as in GZDoom).
- `ConsolePlayerNumber` is 0, as in Room's ZScript host: one playsim runs
  for every seat.
- Room plays skills 1–5, so the skill-6 token is only reached when a host
  sets a higher skill (the `room-sim` test does).
- `+INVENTORY.IGNORESKILL`, `UNDROPPABLE` and `UNTOSSABLE` are not set (no
  ABI flag ids; they change nothing for this token).
