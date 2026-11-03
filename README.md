# Room

Pure-Rust re-implementation of the ZDoom/GZDoom engine family: same IWADs,
PWADs, PK3s, UDMF, DECORATE, ACS, and MAPINFO content, with **wgpu** rendering
(desktop Vulkan/Metal/DX12 and browser WebGPU/WebGL2) and **WebAssembly** mods
instead of ZScript.

Library-first: map loading, VFS, playsim, renderer, and UI are separate crates
you can reuse outside the full game.

## Status

Doom/Freedoom play works on desktop and in the browser. Multi-room web
multiplayer, example WASM mods, Duke Nukem 3D GRP conversion, and a large
catalogue of opt-in graphics CVars are available. Heretic/Hexen and Strife
wait on IWADs. Modern look FX stay **off by default** (faithful Doom look).

## Prerequisites

- Rust stable. For the browser client: `rustup target add wasm32-unknown-unknown`
- An IWAD (not committed):
  - Doom 1: `testdata/doom/doom1.wad`
  - Freedoom: `./scripts/fetch-freedoom.sh` → `testdata/freedoom/`
- Optional SoftFont music: `./scripts/fetch-soundfont.sh`

## Play — desktop

```bash
# Doom 1 E1M1. Brutal Doom pk3s select doom2.wad + MAP01.
./scripts/play-doom.sh
./scripts/play-doom.sh --title
./scripts/play-doom.sh --file ~/Downloads/brutal22test6.pk3
./scripts/play-doom.sh --game doom2 --map MAP02
./scripts/play-doom1.sh                       # same as --game doom1

# Duke Nukem 3D (needs testdata/duke/duke3d.grp)
./scripts/play-duke3d.sh
ROOM_MAP=E2M1 ./scripts/play-duke3d.sh

# Force OPL (GENMIDI) instead of SoftFont
cargo run -p room-app --release -- \
  --iwad testdata/doom/doom1.wad --map E1M1 --play --mididevice opl

# Freedoom
./scripts/fetch-freedoom.sh
./scripts/verify-freedoom-demos.sh            # headless demos
./scripts/play-combat-demo.sh                 # large arena: WASD / mouse / Ctrl / 1–7
./scripts/play-combat-demo.sh --god           # same + god mode from the start
cargo run -p room-app --release -- \
  --iwad testdata/freedoom/freedoom2.wad --map MAP01 --play

# 2-player coop (udp default; also tcp / tcp-server; optional Noise)
./scripts/play-doom1-coop.sh
ROOM_TRANSPORT=tcp-server ROOM_NOISE=1 ./scripts/play-doom1-coop.sh
```

## Play — browser & multiplayer

### Single-player

```bash
./scripts/play-web.sh
# Serves target/room-web (auto-loads doom1.wad / freedoom if present).
# Click the canvas for mouse look; press a key to unlock audio.
```

### Multi-room server

```bash
./scripts/room-server.sh start
./scripts/room-server.sh status
./scripts/room-server.sh stop
```

Docker (amd64 / arm64, baked `/play` client):

```bash
./scripts/docker-build.sh
cp deploy/.env.example deploy/.env    # set ROOM_ADMIN_BOOTSTRAP + ticket secret
docker compose -f deploy/docker-compose.yml --env-file deploy/.env up
# Multi-arch push example:
# ./scripts/docker-build.sh --platforms linux/amd64,linux/arm64 \
#   --tag ghcr.io/you/room-server:latest --push
```

| URL | What |
|-----|------|
| http://127.0.0.1:8765/ | Public lobby — create / join rooms |
| http://127.0.0.1:8765/play/ | Game client (same origin) |
| http://127.0.0.1:8766/admin | Operator console (first-run user `admin` / `admin`) |

From the lobby, **Play** opens the game in this tab; use a second browser
profile (or **New tab**) for another seat. Lockstep starts when every seat has
joined and is Ready. Optional built-in STUN/TURN handles WebRTC voice when
enabled on the server.

```bash
./scripts/play-web-coop.sh --v2 --no-open   # build client, start server, print URLs
```

### Browser controls

| Input | Action |
|-------|--------|
| WASD / arrows | Move / turn |
| Mouse (click canvas) | Look (pointer lock) |
| Ctrl / LMB | Attack |
| E / Space | Use |
| 1–7 | Weapons |
| **V** (hold) | Push-to-talk (WebRTC) |
| Top-left **Net** panel | Lockstep wait / rx·tx / held keys |

Keep coop tabs focused — background tabs throttle `requestAnimationFrame` and
stall lockstep for everyone.

## Mods

Mods are WASM modules (spawn, damage, inventory, HUD, rules, …) packed in a
PWAD with a `ROOMMOD` manifest (`room.toml`). Same hooks in every language:
register classes, then handle load / tick / deaths / pickups.

Shipped examples for `doom1.wad`:

| Mod | Language |
|-----|----------|
| [`mods/doom1-bounty`](mods/doom1-bounty) — Bounty Hunter | TypeScript (AssemblyScript) |
| [`mods/doom1-survival`](mods/doom1-survival) — Survival Director | Rust |
| [`mods/doom1-royale`](mods/doom1-royale) — Battle Royale | Rust |
| [`mods/doom1-python-hello`](mods/doom1-python-hello) — Hello | Python (desktop only) |

```bash
./scripts/room-server.sh mods   # build every mods/<dir> → *.wad
cargo run -p room-app --release -- --iwad testdata/doom/doom1.wad \
  --file mods/doom1-bounty/bounty.wad --map E1M1 --play
# Browser: http://127.0.0.1:8080/?mod=bounty
```

On a live server, admins allow mods under Settings → **Mods**; players pick
them in the lobby (or `POST /api/rooms` with `"mod": "bounty"`). Every seat
downloads and verifies the same mod WAD before the map loads.
Production: `--mods-dir` / `ROOM_MODS_DIR`.

### Rust (WASM)

Guest crate with `room-script-sdk`, target `wasm32-unknown-unknown`,
`crate-type = ["cdylib"]`. Full mods: `mods/doom1-survival`, `mods/doom1-royale`.

`room.toml`:

```toml
[mod]
id = "org.example.hello"
name = "Hello"
version = "0.1.0"
abi = 1

[[module]]
file = "HELLO"
name = "hello"
scope = "play"
```

`src/lib.rs`:

```rust
#![cfg_attr(target_arch = "wasm32", no_std)]

#[cfg(target_arch = "wasm32")]
#[panic_handler]
fn panic(_: &core::panic::PanicInfo<'_>) -> ! {
    core::arch::wasm32::unreachable()
}

use room_script_sdk::game::{self, event_mask, spawn_flags, Event, RoomMod};
use room_script_sdk::{class_find, export_mod};

#[derive(Default)]
struct Hello {
    kills: i32,
    spawned: bool,
}

impl RoomMod for Hello {
    const NAME: &'static str = "Hello";
    const EVENTS: u64 = event_mask::WORLD_LOADED
        | event_mask::WORLD_TICK
        | event_mask::WORLD_THING_DIED;

    fn on_event(&mut self, event: Event) {
        match event {
            Event::Loaded { .. } => {
                game::print(None, "Hello from Rust!");
            }
            Event::Tick { maptime } if !self.spawned && maptime >= 3 * 35 => {
                self.spawned = true;
                if let Some(p) = game::player(0) {
                    let mut pos = p.pos();
                    pos[0] += 256.0;
                    let _ = game::spawn(
                        class_find("DoomImp"),
                        pos,
                        180.0,
                        spawn_flags::TELEFOG | spawn_flags::CHECK_POSITION,
                    );
                }
            }
            Event::ThingDied {
                thing,
                source: Some(killer),
            } if killer.player().is_some() && thing.is_monster() => {
                self.kills += 1;
                killer.set_health(killer.health() + 10);
                game::print(None, "kill: +10 health");
            }
            _ => {}
        }
    }
}

export_mod!(Hello);
```

Build with `./scripts/room-server.sh mods` (or `cargo build --release --target wasm32-unknown-unknown` in the mod crate, then pack the `.wasm` as the `HELLO` lump next to `ROOMMOD`).

### TypeScript (AssemblyScript → WASM)

One `.ts` file using `crates/room-web/as/room.ts`. Full example:
`mods/doom1-bounty/bounty.ts`. Fixture: `testdata/mods/hello_as.ts`.

`room.toml` — set `source = "S_HELLO"` so the web editor can reopen the source:

```toml
[mod]
id = "org.example.hello-as"
name = "Hello AS"
version = "0.1.0"
abi = 1

[[module]]
file = "HELLO"
name = "hello"
source = "S_HELLO"
scope = "play"
```

`hello.ts`:

```typescript
import {
  RoomMod, Actor, register, print, player, skill, spawn,
  defineClass, classProperty, endClass, ClassProp, Spawn, TICRATE,
} from "./room";

class Hello extends RoomMod {
  kills: i32 = 0;

  register(): void {
    defineClass("GhostImp", "DoomImp");
    classProperty(ClassProp.HEALTH, 30);
    classProperty(ClassProp.ALPHA_PERCENT, 50);
    endClass();
  }

  onLoad(_maptime: i32): void {
    print("Hello from AssemblyScript! skill " + skill().toString());
  }

  onTick(t: i32): void {
    if (t == 3 * TICRATE) {
      const p = player(0);
      if (p != null) {
        const pos = p.pos;
        spawn(
          "GhostImp",
          pos.x + 256, pos.y, NaN, 180,
          Spawn.TELEFOG | Spawn.CHECK_POSITION,
        );
      }
    }
  }

  onThingDied(thing: Actor, killer: Actor | null): void {
    if (killer != null && killer.isPlayer && thing.isMonster) {
      this.kills++;
      killer.health = killer.health + 10;
      print("Kill #" + this.kills.toString() + ": +10 health");
    }
  }
}

register(new Hello("HelloAS"));
export * from "./room_abi";
```

```bash
node scripts/asc-compile.mjs hello.ts hello.wasm
./scripts/room-server.sh mods   # or pack ROOMMOD + HELLO + S_HELLO yourself
```

The web editor **Scripts** tab compiles AssemblyScript in the browser and
hot-reloads into a play test on Ctrl+S.

### Python (desktop host)

Same hooks in-process via CPython — great for learning, **not netplay-safe**.
Port to Rust or AssemblyScript for multiplayer. Full example:
`mods/doom1-python-hello`.

`room.toml`:

```toml
[mod]
id = "org.example.hello-python"
name = "Hello Python"
version = "0.1.0"
abi = 1

[[module]]
file = "HELLO"
name = "hello"
language = "python"
scope = "play"
```

`hello_mod.py`:

```python
from typing import Optional
from room.sdk import Actor, ModBase, TICRATE

class Hello(ModBase):
    def __init__(self) -> None:
        super().__init__("HelloPython")
        self.kills = 0
        self._spawned = False

    def on_load(self, maptime: int) -> None:
        self.print(f"Hello from Python! skill {self.skill()}")

    def on_tick(self, t: int) -> None:
        if self._spawned or t < 3 * TICRATE:
            return
        self._spawned = True
        p = self.player(0)
        if p is None:
            return
        aid = self.spawn("DoomImp", p.x + 256.0, p.y, angle=180.0)
        if aid is not None:
            self.print(f"Spawned DoomImp id={aid}")

    def on_thing_died(self, thing: Actor, killer: Optional[Actor]) -> None:
        if killer is None or not killer.is_player or not thing.is_monster:
            return
        self.kills += 1
        if self._session is not None:
            hp = self._session.player_health() or 0
            self._session.set_player_health(min(200, hp + 10))
        self.print(f"Kill #{self.kills} ({thing.class_name}): +10 health")

MOD = Hello()
```

```bash
./scripts/pack-python-mod.sh mods/doom1-python-hello
PYO3_USE_ABI3_FORWARD_COMPATIBILITY=1 \
  cargo run -p room-app --features python-mods -- \
  --iwad testdata/doom/doom1.wad \
  --file mods/doom1-python-hello/hello.wad --play

# Educational REPL (no window)
./scripts/setup-python-edu.sh
python -m room mods/doom1-python-hello/hello_mod.py
```

### Duke Nukem 3D

Point `--iwad` at a `DUKE3D.GRP` (browser: `?iwad=duke3d.grp`, or lobby
**Game: Duke Nukem 3D**). The GRP is converted on load into UDMF with maps,
enemies, items, weapons, status bar, sounds, and sector effects. CON is data
only — there is no CON VM.

## Shaders & graphics CVars

Modern look FX are **off by default**. Toggle them from:

| Where | How |
|-------|-----|
| Desktop console | `set gl_bloom 1`, `set r_quality 2`, … |
| Desktop menu | Options → **Graphics** (F5 in-game) |
| CLI | `--set gl_bloom=1 --set r_gore=2` |
| Web | Settings → **Graphics** (**Reset shaders to defaults** restores faithful) |
| Web JS | `roomSetShaderCvar("gl_bloom", "1")` |

`--combat-demo` resets every shader CVar on start and does not write them to
`room.ini`.

### Quality presets (`r_quality`)

| Value | Name | Summary |
|-------|------|---------|
| `0` | Faithful | Everything modern off |
| `1` | Low | Soft tonemap, FXAA, bloom, soft particles, hitflash, muzzle/tracers/impact/kick |
| `2` | Medium | AgX, SMAA→FXAA, bloom, SSAO, outlines, color grade, damage FX, liquids |
| `3` | Ultra | Khronos tonemap, TAA→FXAA, stronger SSAO/bloom, heat haze, shafts, enhanced gore |

### Catalogue

**Post / look**

| CVar | Values | Default | Notes |
|------|--------|---------|-------|
| `gl_tonemap` | `0`–`8` | `0` | 0 none … 6 AgX, 7 Khronos, 8 RoomSoft |
| `r_exposure` | float | `1` | Pre-tonemap exposure |
| `r_gamma` | float | `1` | Display gamma (post) |
| `gl_fxaa` / `r_aa` | `0`–`3` | `0` | 0 off, 1 FXAA, 2 SMAA→FXAA, 3 TAA→FXAA |
| `r_sharpen` | `0`–`1` | `0` | CAS-lite sharpen |
| `gl_bloom` | `0`/`1` | `0` | Soft bloom |
| `gl_bloom_amount` | float | `0.8` | Bloom strength |
| `gl_ssao` | `0`–`3` | `0` | Screen-space AO (Full GPU tier) |
| `gl_ssao_strength` | float | `1` | AO intensity |
| `r_postchain` | `0`/`1`/`2` | `0` | 0 auto, 1 force on, 2 force off |
| `r_crt` | `0`/`1` | `0` | CRT barrel + scanlines |
| `r_crt_preset` | `0`–`2` | `0` | 0 off, 1 TV, 2 PC monitor |
| `r_dither` | `0`/`1` | `0` | Bayer dither |
| `r_banding` | `0`/`1` | `0` | 5-bit banding |
| `r_vignette` | `0`–`1` | `0` | Edge darken |
| `r_grain` | `0`–`1` | `0` | Film grain |
| `r_chromatic` | `0`–`1` | `0` | Chromatic aberration |
| `r_damagefx` | `0`/`1` | `0` | Low-health red edge |
| `r_colorgrade` | `0`/`1` | `0` | Mild grade |
| `r_autoexposure` | `0`/`1` | `0` | Exposure from sector light |
| `r_powerupfx` | `0`/`1` | `0` | Suit / IR tint |
| `r_underwaterfx` | `0`/`1` | `0` | Underwater CA + tint |
| `r_menublur` | `0`/`1` | `0` | Soften scene behind menus |
| `r_dof` / `r_motionblur` | `0`/`1` | `0` | Soft stand-ins |
| `vid_scalefactor` | float | `1` | Internal scale (&lt;1 = upscale) |
| `vid_scalemode` | int | `0` | Upsample mode |

**Actors / atmosphere**

| CVar | Values | Default | Notes |
|------|--------|---------|-------|
| `r_softparticles` | `0`/`1` | `0` | Soft disc fade |
| `r_particlefx` | `0`/`1` | `0` | Motion-stretch particles |
| `r_outlines` | `0`/`1` | `0` | Edge pop |
| `r_hitflash` | `0`/`1` | `0` | Bright flash on hit actors |
| `r_actorspriteshadow` | `0`/`1` | `0` | Floor blob under monsters |
| `r_dissolve` | `0`/`1` | `0` | Death dissolve particles |
| `r_liquids` | `0`–`2` | `0` | Liquid / underwater look |
| `r_heathaze` | `0`/`1` | `0` | Explosion heat CA |
| `r_parallax` / `r_ssr` / `r_detail` / `r_skyfx` / `r_decalfx` | `0`/`1` | `0` | Partial / later |
| `r_lightshafts` / `r_volfog` / `r_ambientbounce` | `0`/`1` | `0` | Lighting extras |
| `gl_shadowmap_filter` | `0`–`2` | `0` | Shadow filter quality |
| `gl_light_sprites` / `gl_lightmap` / `r_autonormals` | `0`/`1` | `0` | Sprite / lightmap |
| `r_hudfx` | `0`/`1` | `0` | Mod HUD widget FX |
| `r_gpuprofile` | `0`/`1` | `0` | GPU profiler (WIP) |

**Shooting & blood** (`r_gore` gates blood layers)

| CVar | Values | Default | Notes |
|------|--------|---------|-------|
| `r_gore` | `0`–`3` | `1` | 0 none … 3 full |
| `r_bloodfx` | `0`/`1` | `0` | Spray + floor/wall splats (`r_gore` ≥ 2) |
| `r_blooddrips` | `0`/`1` | `0` | Wall drips |
| `r_bloodwet` | `0`/`1` | `0` | Wet shading (stub) |
| `r_corpsepools` | `0`/`1` | `0` | Pool under corpses |
| `r_gibs` | `0`/`1` | `0` | Extra gib particles |
| `r_screenblood` | `0`/`1` | `0` | Lens blood on heavy damage |
| `r_weaponblood` / `r_wounds` | `0`/`1` | `0` | Stub |
| `r_muzzlefx` | `0`/`1` | `0` | Muzzle flash particles |
| `r_tracers` | `0`/`1` | `0` | Hitscan tracers |
| `r_impactfx` | `0`/`1` | `0` | Wall impact sparks |
| `r_viewkick` | `0`/`1` | `0` | Render-only camera kick |
| `r_viewkick_scale` | float | `1` | Kick strength (`0`–`4`) |
| `r_explosionfx` | `0`/`1` | `0` | Explosion fireball particles |
| `r_trails` | `0`/`1` | `0` | Missile smoke |
| `r_casings` | `0`/`1` | `0` | Ejected shells |
| `r_weaponsmoke` | `0`/`1` | `0` | Barrel smoke |
| `r_hitconfirm` | `0`/`1` | `0` | Flash on confirmed hit |
| `r_reduce_flashing` | `0`/`1` | `0` | Photosensitivity cap |

```bash
cargo run -p room-app --release -- --combat-demo --skill 4 \
  --set r_gore=2 --set r_bloodfx=1 --set r_blooddrips=1 \
  --set r_muzzlefx=1 --set r_tracers=1 --set r_impactfx=1 --set r_viewkick=1
```

## Develop

```bash
cargo fmt --all
cargo clippy --all-targets -- -D warnings
cargo test --workspace
cargo build --target wasm32-unknown-unknown -p room-web --features browser
```

## Licence

**GPL-3.0-or-later**. Behaviour reference: [GZDoom](https://github.com/ZDoom/gzdoom),
[ZDoom wiki](https://zdoom.org/wiki/Main_Page).
