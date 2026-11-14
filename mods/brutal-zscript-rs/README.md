# Brutal Doom ZScript, ported to Rust (WASM mod)

Brutal Doom's ZScript, rewritten by hand as a Rust crate and compiled to a
Room WASM mod. Load the original Brutal Doom PK3, then this WAD on top: Room
then **skips Brutal Doom's ZScript and runs this Rust module instead**. The
rest of Brutal Doom (actors, DECORATE, ACS, sprites, sounds) still comes from
the PK3.

| Brutal Doom source | Rust implementation |
|---|---|
| `zscript/DamageHandler.zc`: `Script_DamageMult` | `src/damage.rs`: player-entered and actor-spawn hooks |
| `zscript/DamageHandler.zc`: `Damage_Control` | `src/damage.rs`, `src/lib.rs`: skill 6 damage token |
| `zscript/BD_WCS.zc`: `ZandroCompat` | `src/zandro.rs`: functions ACS reaches with `ScriptCall` |
| `zscript/BD_WCS.zc`: `ZC_TMapHack`, `ZC_IntStrMap` | `src/lumps.rs`: open-lump table |

This covers the lump-reading, player TID, gender/face-token and client-query
helpers used by Brutal Doom's ACS, and its skill 6 damage token (damage ×7,
fire ×3, `TehLostSoul` ×2). Design notes: [docs/39](../../docs/39-zscript-to-rust-mods.md).

## Requirements

- Rust stable and the `wasm32-unknown-unknown` target:

  ```sh
  rustup target add wasm32-unknown-unknown
  ```

- `python3` (used by `build.sh` to pack the WAD).
- This Room checkout: the crate depends on `../../crates/room-script-sdk`.
- To play: a **Brutal Doom PK3** (for example `brutal22test6.pk3`) and a
  **Doom II IWAD**. Neither is included in the repository.

## Compile

```sh
cd mods/brutal-zscript-rs
./build.sh
```

`build.sh`:

1. builds the crate in release mode for `wasm32-unknown-unknown`
   (`opt-level = "z"`, LTO, about 52 KB);
2. copies the module to `brutal-zscript.wasm`;
3. packs a WAD, `brutal-zscript.wad`, with two lumps:

   | Lump | Content |
   |---|---|
   | `ROOMMOD` | `room.toml`: mod id, ABI 1, `replaces_zscript` list, module entry |
   | `BDZSCRPT` | the WASM module (`scope = "play"`) |

Both outputs are written next to `build.sh` and overwritten on every run.
The crate is standalone (not a workspace member), so it has its own
`Cargo.lock` and `target/`.

## Use

From the Room repository root, load the Brutal Doom PK3 **first** and this
WAD **after** it:

```sh
./scripts/play-doom.sh --iwad /path/to/DOOM2.WAD --map MAP01 \
  --file /path/to/brutal22test6.pk3 \
  --file mods/brutal-zscript-rs/brutal-zscript.wad
```

If the IWAD is at `testdata/doom/doom2.wad`, `--game doom2` replaces
`--iwad` (the launcher also picks Doom II by itself when it recognizes
Brutal Doom). Other options (`--skill`, `-- --god`, …) are listed in the
[main README](../../README.md#launcher-options).

### What runs

`room.toml` declares:

```toml
replaces_zscript = ["zscript/BD_WCS.zc", "zscript/DamageHandler.zc"]
```

When this WAD is mounted, Room removes those two files from the ZScript
build and comments out the `#include` lines that pull them in. Brutal Doom's
`zscript.zc` includes only these two files, so none of Brutal Doom's ZScript
is compiled or interpreted. The ZScript behaviour comes from the Rust module,
run by Room's WASM mod host. Everything else comes from the PK3.

Checking it is active, in the terminal log:

- `room: WASM mods: …` lists this mod;
- the ZScript load trace prints `room: ZScript note: zscript/BD_WCS.zc:
  replaced by a WASM mod (replaces_zscript)` and the same for
  `DamageHandler.zc` (with `--no-zscript-debug`, only the first note is shown).

Without this WAD, the same command runs Brutal Doom's original ZScript on the
interpreter.

### Skill 6

The damage token only applies at skill 6. The game UI and `--skill` go up to
5, so this part is exercised by engine tests that set the skill directly.

## Rust port or automatic conversion?

Room has a second, automatic way to turn Brutal Doom's ZScript into
WebAssembly: `room mod convert`. Use one or the other, not both.

| | This crate (hand-port) | `room mod convert` |
|---|---|---|
| Output | `brutal-zscript.wad`, loaded **after** the original PK3 | A new PK3, loaded **instead of** the original |
| Code | Readable Rust you can edit and unit-test | The same ZScript bytecode, compiled to WASM |
| ABI | Standalone `ROOMMOD` ABI v1 | Tied to the Room build's native table |
| Checked by | `cargo test`, `bd_rust_port` (1,500 tics) | Built-in verification (2,000 tics) |

Automatic conversion, from the repository root:

```sh
cargo run --release -p room-cli -- mod convert /path/to/brutal22test6.pk3 \
  -o bd-converted.pk3 --iwad /path/to/DOOM2.WAD --skill 6

./scripts/play-doom.sh --iwad /path/to/DOOM2.WAD --map MAP01 \
  --file bd-converted.pk3
```

See the [main README](../../README.md#convert-a-zscript-mod-to-webassembly)
and [docs/37](../../docs/37-zscript-to-wasm.md).

## Modify

1. Edit `src/damage.rs`, `src/zandro.rs` or `src/lumps.rs`. Engine calls go
   through the `Host` trait (`src/host.rs`): `SdkHost` calls the engine and
   the tests' `FakeHost` records calls, so the logic is testable without the
   game.
2. Test and rebuild, from `mods/brutal-zscript-rs`:

   ```sh
   cargo test
   ./build.sh
   ```

3. Run the game again with the command under [Use](#use).

To port another ZScript file, add its path to `replaces_zscript` in
`room.toml` and implement it in the crate.

## Engine integration tests

From the Room repository root. Registration, `ModifyDamage` and string-valued
`ScriptCall`:

```sh
cargo test -p room-sim --lib brutal_zscript_port
```

Original ZScript versus the Rust port, in game, for 1,500 tics with a
save/load round trip. Needs a local Brutal Doom PK3:

```sh
ROOM_BRUTALDOOM_PK3=/path/to/brutal22test6.pk3 \
  cargo test -p room-game --test bd_rust_port
```

## Differences from the source

- `ZC_IntStrMap` uses a plain map; it does not reproduce Brutal Doom's stale
  reference count when three or more lumps are open simultaneously.
- `PlayerPawn(source)` is checked, so players' damage is scaled too, matching
  GZDoom behavior.
- `ConsolePlayerNumber` is `0`, as in Room's current ZScript host model.
- Room exposes skills 1–5 through the normal game UI. The skill 6 damage token
  is covered by engine tests that set a higher skill directly.
- `+INVENTORY.IGNORESKILL`, `UNDROPPABLE`, and `UNTOSSABLE` are not set because
  their ABI flag IDs are not currently exposed; they do not change this token's
  behavior.
