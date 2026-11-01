# room-render

wgpu renderer: 3D scene plus 2D canvas backend.

**Shaders / post / blood FX CVars** (how to set values, quality presets): see the
root [`README.md`](../../README.md#shaders--graphics-cvars) and
[`docs/29-shaders.md`](../../docs/29-shaders.md).

## M4 status

- `GpuContext` / `GpuTier` — headless device init (Full vs Downlevel)
- `Clipper` + `pseudo_angle` — angular occlusion for BSP walk
- `walk_bsp` — CPU visibility → subsector list
- `build_scene` — walls (mid/upper/lower) + floor/ceiling fans + sky cylinder
- `UnlitPass` + `OffscreenTarget` — checker unlit draw + lightmode attenuations
- `ModZonePass` — BE1 battle-royale zone cylinder wall + ground ring (translucent, depth-tested; `tests/zone_wall_smoke.rs`)
- `TextureManager::upload_image` — PLAYPAL expand → RGBA8 GPU upload
- Material sampler — nearest + **Repeat** (Doom walls/flats tile in UV; ClampToEdge smears)
- `read_rgba` / `encode_png` — golden-image capture path (offscreen smoke test)
- `room-app` — free-fly map viewer (`cargo run -p room-app`)
- `room-web` — wasm stub (`--features browser`)

Freedoom Phase 2 MAP01 offscreen smoke is green (`tests/freedoom_map01.rs`, ~823 material ranges).

Synthetic room SSIM golden: `testdata/goldens/room_320x200.png`. Dyn-lights: `dyn_lights_320x200.png`. Fuzz: `fuzz_320x200.png` (`ROOM_UPDATE_GOLDEN=1` to refresh).

CI `render-smoke` runs offscreen + Freedoom MAP01 on lavapipe. Local SSIM golden is Metal-flavoured (`golden_room`); lavapipe PNG goldens TBD.
