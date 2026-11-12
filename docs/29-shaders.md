# 29 — Shader system and shader roadmap (`room-render`)

**Audience:** an external coding agent working in the Room repo. Read `AGENTS.md`,
`docs/02-conventions.md`, `docs/13-renderer.md` (frame structure, light modes,
post, custom shaders) and `docs/18-platform-web.md` first.
**Status:** in progress — **SHR** first (`docs/21` §0), then SH0–SH1 + SH5/SH10.
Milestones `SH0`–`SH10` + **SHR** (§9). Every catalogue effect has a CVar
(defaults keep the classic look — modern FX off). Toggle via console
(`set gl_bloom 1`, `r_quality 2`, …), desktop Graphics menu, `--set`, or web
`roomSetShaderCvar`. `--combat-demo` resets shaders and skips ARCHIVE.

This document has two parts:
- **Part I (§1–§6)**: the *shader system*: how shaders are organised,
  composed, compiled, cached, parameterised, exposed to content (GZDoom
  `GLDEFS HardwareShader`, Room WGSL) and to mods, on both GPU tiers.
- **Part II (§7–§8)**: the *catalogue* of shaders to implement to improve the
  game, with technique, inputs, tier, cost budget, CVar and priority —
  including **hardcore shooting** (§8.I) and **blood & gore** (§8.J), which
  need a render-only FX event stream from the sim (§8.K).

Renderer code never touches the playsim. Nothing here may affect determinism.

---

## 0. TL;DR

0. **Read §1.1 first**: a review (2026-10-31) of every implemented shader with
   concrete bugs (bloom uniforms overwritten, bloom result discarded, FXAA on
   HDR, sharpen on pre-tonemap data, lightmap added instead of multiplied,
   normal maps without tangent frame, no surface normals, hard-coded fog,
   no real COLORMAP path) — milestone **SHR** fixes them before new effects.
1. **Wire the existing post stack into the real game.** *(Partly done: `frame.rs` `PostChain` is called by both shells — see §1.1.)* Today both shells
   (`room-app/src/main.rs`, `room-web/src/lib.rs`) draw the scene with
   `UnlitPass` straight into an LDR colour target in the swapchain format,
   then the HUD; `PostPass`, `BloomPass`, `SsaoPass`, `LightmapPass`, the
   particle and skinned passes and the `HardwareShaderCatalog` are only used by
   tests and offscreen tools. Step one is a shared **frame graph** in
   `room-render` with an HDR scene target and the post chain (SH0–SH1).
2. Move tonemapping and sRGB encoding **out of `unlit.wgsl`** (it currently
   encodes sRGB and applies a "soft toe" inline) into post. Keep the
   `ClassicPalette` / `DoomSoftware` look pixel-identical (golden test).
3. Add a small **WGSL module system** (imports + defines → permutations), a
   **pipeline cache**, and **hot reload** (SH2).
4. Finish **GZDoom `HardwareShader` compatibility** (material + post-process)
   on top of the existing `glsl_to_wgsl` (naga `glsl-in` → `wgsl-out`) and
   `compose_hardware_post_wgsl`, plus a Room-native WGSL extension (SH3–SH4).
5. Let **mods** toggle post shaders and set uniforms through sim output (like
   the Mod HUD in `docs/27` §9) (SH4).
6. Then implement the catalogue (§8) in priority order: P0 foundation → P1
   faithful-retro and core modern look → P2 materials/atmosphere → P3 extras.
7. Hardcore shooting and blood (§8.I–L, milestone SH10) start with a
   render-only **FX event stream** from the sim (shots, impacts, blood hits,
   deaths, explosions) and a render RNG, so effects never touch determinism.

---

## 1. Current state (read from the repo, 2026-10-30)

| Item | Where | State |
|---|---|---|
| wgpu / naga | workspace `Cargo.toml`: `wgpu = "25"`, `naga = "25"` (`wgsl-in` at build, `glsl-in` + `wgsl-out` at runtime) | |
| Build-time validation | `crates/room-render/build.rs` parses + validates every `src/shaders/*.wgsl` with naga; errors fail the build | Only whole files; no permutations. |
| Shaders | `src/shaders/`: `unlit.wgsl` (365 lines), `post.wgsl` (166), `ssao.wgsl` (106), `skinned.wgsl` (79), `bloom.wgsl` (54), `particles.wgsl` (49), `zone.wgsl` (39), `lightmap.wgsl` (34) | Loaded with `include_str!` per pass. |
| Main uber-shader | `unlit.wgsl` + `UnlitPass` (`pass.rs`, 1490 lines) | Group 0: `ViewUniforms` (view_proj, eye+time, light_params = lightmode/globVis/fog/fuzztype, clip_plane, lightmap_params), `LightUniforms` (≤32 forward lights), cluster mask texture, 1D shadowmaps, lightmap atlas. Group 1: albedo, brightmap, normal, metal-rough. Vertex: pos, uv, light, style (alpha / masked / fuzz), lm_uv. Fragments: `fs_main`, `fs_main_mrt` (dummy aux slot for Metal stencil), `fs_stencil_mask`. Implements light modes incl. `DoomSoftware`, fuzz variants, dyn lights, shadows, lightmaps, normal / GGX specular, **then sRGB-encodes and applies a soft toe in-shader**. |
| Separate pipelines | `UnlitPass` has `pipeline` + `flats_pipeline`, stencil mask / equal variants | |
| Post | `post.rs` (`PostPass`, `PostUniforms {params: tonemap mode/exposure/gamma/bloom threshold, blend, fxaa}`, `TonemapMode`, palette quantize **by looping all 256 PLAYPAL entries per pixel**) | Not used by shells. |
| Bloom / SSAO | `bloom.rs` (4-level mip chain), `ssao.rs` (half-res, bilateral blur, `R8Unorm`) | Not used by shells. |
| Custom shaders | `hardware_shader.rs` (`HardwareShaderCatalog`, `compile_material_glsl` wraps a `ProcessTexel` snippet with only `getTexel`, `compose_hardware_post_wgsl`, `create_hardware_post_pipeline`), `glsl_translate.rs` (`glsl_to_wgsl`); GLDEFS parse in `room-defs/src/gldefs.rs` | Exported, not used outside the crate. |
| Tiers | `device.rs`: `GpuTier::Full` / `GpuTier::Downlevel` (no compute → `Limits::downlevel_webgl2_defaults()`) | |
| Zone | `mod_zones.rs` + `zone.wgsl` (`ModZonePass`), used by both shells | Recent; good template for a mod-driven pass. |
| Frame in shells | desktop: main pass → `frame_color` (swapchain format) + `stencil_aux` (`R8Unorm`) + depth/stencil → HUD via `canvas_gpu` → copy to swapchain. Web: same structure in `room-web/src/lib.rs` | Duplicate plumbing in two shells. |
| Goldens | `testdata/goldens/*.png` with SSIM; offscreen API `room_render::offscreen` | |

---

## 1.1 Review of the implemented shaders (2026-10-31) — findings and fixes

Since §1 was written, part of SH0/SH1 has landed: `frame.rs` (`FrameTargets`,
`PostChain`: SSAO → bloom → tonemap pass), `settings.rs` (`RenderSettings`
with every CVar from §8, quality presets), an HDR `Rgba16Float` scene target on
the Full tier, and both shells calling `PostChain`. `post.wgsl` grew to 289
lines (32³ palette LUT, AgX / PBR Neutral / RoomSoft tonemaps, dither,
banding, grain, vignette, CRT, chromatic aberration, CAS-lite, damage
vignette). This section reviews every WGSL file as it is today and lists the
fixes. Each row has a priority: **B** = bug (wrong output or wasted work,
fix first), **Q** = quality/faithfulness, **P** = performance, **S** =
structure. Milestone **SHR** (§9) covers all **B** rows plus the **Q/P** rows
marked "SHR".

### 1.1.1 Frame / post plumbing (`frame.rs`, `bloom.rs`, `ssao.rs`, `post.rs`, `settings.rs`)

| # | Finding | Impact | Fix | Prio |
|---|---|---|---|---|
| R1 | `BloomPass` writes **one uniform buffer** several times per encoder (`gpu.queue.write_buffer(&self.ubo, …)` in `pass`, `downsample_chain`, `upsample_to_composite`). `write_buffer` executes at submit, before the encoder, so **every bloom pass sees the last value** (upsample mode, mip0 texel size): the extract pass never applies the threshold and downsample offsets are wrong. | Bloom output is wrong | One UBO **per level** (or one buffer with `min_uniform_buffer_offset_alignment`-aligned slots + dynamic offsets), written once per frame before encoding. Same audit for `SsaoPass` / `PostPass` if they ever run twice per frame (portals, camera textures). | B |
| R2 | `PostChain::apply_views` discards the bloom result (`let _ = self.bloom.apply(…)`); `post.wgsl` instead rebuilds a fake bloom by sampling the scene **32 times per pixel** at 4 radii. | Double cost; bloom is a blurry box, not a pyramid | Bind the `BloomPass` composite as a second texture in the post pass and add it (`color += bloom_tex * intensity`) **before** tonemap. Delete the in-shader 4-level loop. | B/P |
| R3 | Bloom pyramid only upsamples the **finest** mip to the composite; lower mips are never accumulated. | No wide glow | Proper dual-filter: downsample with a 13-tap (CoD/Jimenez) filter, upsample with a 9-tap tent, **additively** walking up the chain (`mip[i] += upsample(mip[i+1])`), 5–6 levels on Full, 3 on Downlevel. | Q (SHR) |
| R4 | Bloom extract: hard knee `avg * (luma − th)` (unnormalised, bright pixels effectively squared), no anti-firefly. | Flicker on single bright pixels (muzzle flashes, particles), over-bright highlights | Soft-knee threshold (`knee = th·0.5`, quadratic curve, divide by `max(luma, ε)`), Karis average on the first downsample (weight `1/(1+luma)`), clamp max input (e.g. 64) for HDR. | Q (SHR) |
| R5 | SSAO darkens the **final colour** (incl. fullbright sprites, brightmaps, dyn-light highlights, muzzle flashes). | Wrong look; dark halos on bright things | Apply AO to the ambient/sector term only (§3.4 two-output split), or at least skip pixels flagged emissive/fullbright in `scene_aux` (needs A6). | Q (SHR) |
| R6 | SSAO compares **non-linear hardware depth** with a fixed bias, uses a fixed screen-space radius (`radius·rcp·120`) independent of distance, and runs at **full resolution** with 5 × 8 = 40 depth taps + per-pixel work; the half-res `ao_view` buffer exists but is unused. | Over-occlusion near the camera, none far away; expensive | Linearise depth (`near·far / (far − d·(far−near))`, or reconstruct view Z from `inv_proj`), project a **world-space radius** (e.g. 24 map units) to screen, compute AO at **half res** into `ao_view` (R8), depth-aware upsample + 2-pass separable bilateral blur, 8–12 taps with per-pixel rotation from blue noise. | Q/P (SHR) |
| R7 | `needs_post()` returns true for CVars that are **declared but have no implementation** (`color_grade`, `auto_exposure`, `powerup_fx`, `underwater_fx`, `dof`, `motion_blur`, `menu_blur`, `scale_factor`). Turning one on switches the whole frame to the post path (and `RoomSoft` tonemap) while doing nothing else. | Surprising look changes; wasted passes | Only count a CVar in `needs_post` once its pass exists; until then the console prints "not implemented yet". Keep a test that every CVar listed in `needs_post` has a pass. | B |
| R8 | Many `RenderSettings` fields have **no consumer anywhere** (grep: `soft_particles`, `hud_fx`, `shadowmap_filter`, `light_sprites`, `autonormals`, `lightshafts`, `volfog`, `heat_haze`, `parallax`, `ssr`, `detail`, `sky_fx`, `decal_fx`, `muzzle_fx`, `tracers`, `impact_fx`, `weapon_smoke`, `casings`, `explosion_fx`, `trails`, `hit_confirm`, `blood_fx`, `blood_drips`, `gibs`, `gore`, `reduce_flashing`, `gpu_profile`, `scale_factor`). | Menu options that do nothing | Mark them `#[doc(hidden)]` + hide from menus/presets until implemented, or wire them as §8 items land. `apply_quality` must not advertise unimplemented effects. | S |
| R9 | AA modes `Smaa` / `Taa` silently fall back to FXAA. | Misleading setting | Hide SMAA/TAA until F4/F5 exist. | S |
| R10 | `PostChain::ensure_formats` ignores a changed LDR format (`let _ = ldr_format`). | Wrong pipeline after a surface format change (web canvas / HDR display switch) → validation error | Recreate `PostPass` when `ldr_format` changes. | B |
| R11 | Bind groups are created **every frame** in `BloomPass`, `SsaoPass`, `PostPass`. | CPU overhead, GC pressure on web | Cache bind groups per (target view generation, size); rebuild only on resize / format change. | P |
| R12 | The web shell sets `pass.output_gain = 1.85` (scene-shader brightness lift) and the scene shader can sRGB-encode (`lightmap_params.z`). With the post chain on, exposure/tonemap are applied **on top** of that gain/encode. | Double brightening / double encode on web | When `linear_scene()` is true: force `output_gain = 1.0` and the in-shader sRGB/toe **off**; move the web lift into `r_exposure` defaults for the web preset. Add a test comparing desktop vs web offscreen output for the same settings. | B |
| R13 | `scale_factor` / `vid_scalefactor` has no implementation (render resolution scaling + upscale F6). | No low-res or performance scaling | Render scene targets at `scale_factor`, upscale in post (nearest for `vid_scalemode` classic modes, EASU+RCAS otherwise). | Q |

### 1.1.2 `unlit.wgsl` (scene uber-shader)

| # | Finding | Impact | Fix | Prio |
|---|---|---|---|---|
| U1 | **No surface normal attribute.** Dyn lights apply no N·L on surfaces without a normal map; the "Blinn stand-in" assumes N = +Z for walls too. | Walls lit from behind/edge-on as if facing the light; flat look | Add `@location(5) normal: vec3<f32>` (walls: from the seg direction; flats: ±Z or slope normal; sprites: facing the camera) to `WallVertex`; use N·L (with GZDoom's wrap for `attenuate` lights, none for non-attenuated, per GLDEFS `attenuate` flag). | B |
| U2 | **Normal maps are used without a tangent frame**: `n_ts` (tangent space) is compared with world-space light vectors. | Normal mapping wrong on every wall orientation | Per-vertex tangent (wall direction / flat X axis) + bitangent sign → TBN; transform the sampled normal to world space before lighting. | B |
| U3 | **Lightmap is added as raw light** (`color += lm * lm_str`) instead of modulating albedo; surfaces without lightmap UVs sample the atlas at `fract(world_xy/256)`. | Washed-out lightmapped maps; garbage light on unmapped surfaces | `lit_term = mix(sector_light_term, lm, lm_str)` then `color = albedo * lit_term` (+ dyn lights); skip the sample contribution when the surface has no lightmap (flag bit in `style`). | B |
| U4 | `doom_software_shade` is **not** `R_DoomLightingEquation`: it clamps to `[0.22, 1]` and a softened depth factor `[0.55, 1.35]`. Light modes 0/1/2/16 use invented linear falloffs. `ClassicPalette` (32) runs the same code as mode 8 — **there is no COLORMAP path**. | `gl_lightmode` options are not faithful; the "pixel-exact" classic look promised in `docs/13` §5.1 does not exist | Port the exact GZDoom formulas from `main.fp` for 0/1/2/8/16 (keep today's softened curve as a Room mode, e.g. `lightmode 33 "RoomSoft"`). Implement `ClassicPalette`: upload paletted textures as **R8 index textures** alongside RGBA, compute the colormap row (0–31) from light + distance exactly like the software renderer, `textureLoad` the 256×34 `COLORMAP` and then `PLAYPAL` → exact colours. Golden test against `room-render-soft` (`soft-view.pgm` workflow) with 0 differing pixels on a fixture view. | Q (SHR) |
| U5 | Lighting math space: albedo is linear (sRGB uploads) but Doom light is a gamma-space multiply (see shader comments); the in-shader toe compensates. | Light ramps differ from GZDoom (mid-tones too dark/bright depending on path) | Faithful modes: convert albedo to gamma, apply sector light multiply in gamma, convert back to linear before dyn lights/post (or upload palette textures non-sRGB for those modes). Modern modes: physically linear. Document per mode. | Q (SHR) |
| U6 | **Fog colour is hard-coded** `vec3(0.05, 0.05, 0.08)`; density scale `0.001` ad hoc; sector fade colour (`Sector_SetFade`, UDMF `fadecolor`), `fogdensity`, `gl_fogmode` (planar vs radial) ignored. In software light mode, Doom fades toward the colormap fade, not exponential fog. | Coloured-fog maps render wrong; outdoor haze impossible | Pass per-vertex (or per-sector-uniform) fade colour + density; implement GZDoom's fog equations (`gl_fogmode 1/2`, distance from `pixelpos`); in `DoomSoftware`/`ClassicPalette` fade by colormap rows. | Q (SHR) |
| U7 | `hash21` uses `fract(sin(dot)) * 43758.5` | Precision artefacts / visible patterns on some GPUs (WebGPU mobile, Metal half precision) | Integer hash (`pcg2d` / `iqint`) in `lib/noise.wgsl`; blue-noise texture for dithering and stochastic effects. | Q |
| U8 | Fuzz variants sample noise in **texture UV space** (`uv * 64`); "software columns" mode is not the vanilla `fuzzoffset[50]` table. | Fuzz swims with the sprite, looks unlike Doom | Screen-space fuzz: sample the *already rendered* scene behind the sprite (needs a copy of `scene_color` before translucent sprites, or draw fuzz as a post-like pass using a sprite mask), offset by the vanilla table indexed by screen row at the 200-line grid, darken with colormap row 6 equivalent. Keep current variants as `gl_fuzztype` extras. | Q |
| U9 | **Cluster masks**: 32-bit masks uploaded as `RGBA8` and rebuilt from unorm floats (`round(t.r*255)…`); clusters are world-space boxes (origin/cell), not view-frustum clusters; hard limit 32 lights (`array<GpuLight, 32>` in a uniform). | Fragile decode, wasted clusters outside view, 32-light ceiling (docs/13 targets 500 lights) | Full tier: storage buffers for lights (≥ 512) and a **froxel** cluster grid in view space with an index list (`light_grid` + `light_indices`), assigned in compute (§13 already plans this). Downlevel: keep ≤ 32 uniform lights but use an `R32Uint` texture for masks (`textureLoad` on `texture_2d<u32>`, supported on WebGL2). | P/Q |
| U10 | Shadowmap: 512 samples, **hard** compare `dist <= occ * 1.02`, `atan2` per light per pixel, 1D map ignores Z (fine for GZDoom parity) | Aliased shadow edges, crawling | Bias in map units (+2), 3–5 tap PCF across neighbouring angle samples weighted by distance (C3), resolution 1024 (GZDoom default), cheaper angle mapping via `pseudo-angle` (dx/(|dx|+|dy|)) like GZDoom. | Q (SHR) |
| U11 | Dyn light falloff `pow(1 − d/r, 2)`; GZDoom uses linear falloff for standard lights and a different curve for attenuated lights; `dontlightself` / `dontlightactors` / `dontlightmap` flags not visible in the shader. | Lights look smaller/harder than in GZDoom mods | Match GZDoom attenuation per light flag; pass flags in `color_flags.w` bits (subtractive, attenuate, dontlightactors, dontlightmap) and a per-draw "is actor" bit. | Q |
| U12 | Spec/PBR: GGX D + Schlick F only (no geometry term, no energy normalisation), `× 0.25` fudge; "metal-rough without normal map" path uses a fixed Blinn on +Z. | Inconsistent highlights | Full GGX (D·G·F / 4·NL·NV) with Smith-GGX correlated G, roughness remap, energy-conserving diffuse (1 − F)·(1 − metallic); only after U1/U2. | Q |
| U13 | All maps sampled with the **albedo sampler** (nearest for pixel art) including normal and metal-rough. | Blocky lighting on normal-mapped surfaces | Second sampler (linear, mipmapped) for normal/roughness/AO/brightmap; albedo keeps `gl_texture_filter`. | Q |
| U14 | Invulnerability inverse colormap computed in-shader from albedo luminance; ignores Doom's actual `COLORMAP` row 32. | Slightly different colours from vanilla | Use the COLORMAP row 32 (inverse map) via U4's palette path in faithful modes; current formula stays for true-colour textures. | Q |
| U15 | One uber-shader with runtime branches for every feature (fuzz, lightmap, clusters, shadows, PBR, invuln, fog, sRGB). The lightmap texture is sampled for **every fragment** even when off. | Register pressure, wasted bandwidth | Split with permutations (§4.3): `LIGHTMAP`, `DYN_LIGHTS`, `CLUSTERS`, `SHADOWS`, `PBR`, `FUZZ`, `FAITHFUL_PALETTE`, `MRT_AUX`. Uniform-control-flow constraints disappear because disabled samples are compiled out. | P (SHR) |
| U16 | `fs_main_mrt` writes a dummy aux value (Metal stencil workaround); A6 wants packed normal + flags there. | Wasted target | Make aux the G-buffer-lite (octahedral normal in RG, flags in B: emissive/fullbright, sprite, liquid, sky) — feeds R5, outlines (E2), soft particles (E1), SSR (D5). | S |
| U17 | Scene shader still contains display transforms (`output_gain`, sRGB encode + toe). | Mixing scene and display concerns (see R12) | Remove once SH0's linear path is the only path; keep a temporary `#ifdef LEGACY_DISPLAY` permutation for the no-post fallback until the post chain is mandatory. | S |

### 1.1.3 `post.wgsl` (tonemap uber-pass)

| # | Finding | Impact | Fix | Prio |
|---|---|---|---|---|
| P1 | **FXAA runs before tonemap on HDR linear data** and computes luma on linear values; and FXAA is **skipped whenever chromatic aberration is on** (the `if/else if` chain). | FXAA misses edges in dark areas, over-blurs highlights; AA silently off with CA | Split: pass 1 = bloom add + exposure + tonemap + grade → LDR (store perceptual luma in alpha); pass 2 = FXAA/SMAA on LDR; pass 3 = "screen" effects (CA, CRT, grain, vignette, dither, blend). CA becomes a lens effect in pass 3, independent of AA. | B |
| P2 | **Sharpen samples `src_tex` (pre-tonemap HDR)** and adds `(color − blur)` to the **tonemapped** colour. | Wrong sharpening (halo / colour shifts), especially with bright HDR neighbours | Sharpen on the LDR output (pass 2/3) — proper CAS (AMD FidelityFX CAS formula, contrast-adaptive weights) or RCAS after upscaling. | B |
| P3 | Dither is applied **after** banding and after palette quantisation, with ±0.5/255 amplitude. | Dither does nothing visible; banding and palette mode get no dithering | Apply ordered/blue-noise dither **before** quantisation, amplitude = one quantisation step (1/31 for banding, palette: dither in LUT space by ~1/32 per channel); keep the final 8-bit dither (±0.5/255) separately for HDR→8-bit gradients. | B |
| P4 | Palette mode: 32³ LUT with nearest filtering → dark Doom ramps collapse (PLAYPAL has many close dark colours); the **screen blend is mixed in RGB after quantisation**, producing non-palette colours. | Not faithful; palette mode shows off-palette flashes | 64³ LUT (1 MiB RGBA8, or R8 index LUT + `PLAYPAL` lookup = 256 KiB), optionally refine by checking the 2–4 nearest candidates; in palette mode, implement flashes like vanilla: choose **PLAYPAL palette 1–13** (damage reds 1–8, bonus 9–12, rad suit 13) from the blend type and look up in that palette. | Q (SHR) |
| P5 | Gamma `pow(1/g)` is skipped for modes 3, 4 and 8 only, and `RoomSoft` (8) **sRGB-encodes manually**; the LDR target may itself be an `*Srgb` format. | Double or missing encoding depending on target format | Define one output transfer: tonemap outputs linear [0,1]; if the target format is `*Srgb`, write linear; else encode sRGB in-shader. Apply user gamma (`vid_gamma`) as a separate display LUT step. Remove the encode from `room_soft`. | B |
| P6 | AgX and PBR Neutral are approximations ("AgX-ish", scalar Neutral without the desaturation step). | Colours differ from reference implementations | Implement reference AgX (inset/outset matrices + polynomial contrast, "base"/"punchy" looks) and Khronos PBR Neutral (with the `desaturation` / `startCompression` terms); unit-test against known values. | Q |
| P7 | Grain uses a fixed `1920×1080` scale and white noise; CRT scanlines use **one line per output pixel** (`1/fxaa.z`), mask per output pixel column; barrel distortion samples outside [0,1] (edge smear, no black border). | Grain scale changes with resolution; CRT moiré at most resolutions | Grain: per-pixel at actual resolution, luminance-weighted, temporal blue noise. CRT: virtual line count uniform (200/240), gaussian beam profile, slot/aperture mask with configurable pitch, black outside the curved screen, phosphor bloom from the bloom mip (B5 presets). | Q |
| P8 | Damage vignette pulses with `sin(time·6.5)` regardless of `reduce_flashing`; it is driven by the classic blend alpha so both apply. | Photosensitivity; muddy red | Respect `r_reduce_flashing` (no pulse, capped intensity); drive F7 from `damage_count` + attacker direction (render-only data) and make it directional. | Q |
| P9 | Luma uses Rec.601 weights on linear data. | Slightly wrong edge/threshold detection | Rec.709 (`0.2126, 0.7152, 0.0722`) on linear, perceptual luma (sqrt or sRGB) where thresholds are perceptual (FXAA). | Q |
| P10 | Everything in one fragment shader with runtime branches. | Cost even when effects are off (uniform branches are cheap but samples/registers are not) | Permutations per enabled effect set (§4.3), or split passes as P1. | P |

### 1.1.4 Other shaders

| File | # | Finding | Fix | Prio |
|---|---|---|---|---|
| `bloom.wgsl` | — | See R1–R4: 4-tap box for both down and up, threshold only on level 0, no tent upsample, no Karis. | Rewrite as `post/bloom_down.wgsl` (13-tap + Karis on first level, soft knee) and `post/bloom_up.wgsl` (9-tap tent, additive). | B/Q (SHR) |
| `ssao.wgsl` | — | See R5–R6. Also samples `color_tex` and multiplies (fused composite) — prevents reuse of AO by other passes. | Output AO only (R8, half res), composite in the lighting split (§3.4) or a tiny composite pass. | Q (SHR) |
| `zone.wgsl` | Z1 | Alpha banding + pulse only; no depth-intersection glow, no distance fade, no refraction; `wall_through` variant just reuses it; `sin(time)` pulse ignores `reduce_flashing`. | H1: sample depth to add an intersection rim (`saturate(1 − (scene_depth − frag_depth)/k)`), fade by distance, hex/scanline pattern in world units (not UV), soft refraction when `heat_haze`/distortion is enabled, through-walls variant at reduced alpha with a different pattern; respect `reduce_flashing`. | Q |
| `particles.wgsl` | PA1 | No texture sampling (shape is analytic only), no depth fade, no lighting, no velocity stretch; square style uses `discard`; outputs premultiplied colour (`rgb * a`) — verify the pipeline blend state is premultiplied (`One, OneMinusSrcAlpha`) for translucent and (`One, One`) for additive. | E1 soft depth fade (needs depth texture binding), E6 lit + stretched, optional flipbook texture (needed by I3/J1/L1); replace `discard` with alpha-to-coverage or smooth alpha. Add a unit/golden test of blend modes. | Q |
| `skinned.wgsl` | SK1 | Model lighting ignores light modes, fog, dyn lights, brightmaps, fuzz/render styles; the declared `clip_plane` is **unused** (skinned models leak through mirrors/portals); bone indices passed as `f32`; 64-bone uniform cap; single colour target (must match MRT layout of the scene pass). | Share the scene lighting code via `#import "lib/lightmode.wgsl"`, `lights.wgsl`, `fog.wgsl` (needs SH2); apply the clip plane discard; `vec4<u32>` bone indices (`Uint8x4`/`Uint16x4` vertex format); storage buffer bones on Full (no 64 cap); write the aux MRT. Same for the (non-skinned) model path in `models.rs` if it has its own shading. | B (clip plane) / Q |
| `lightmap.wgsl` | — | Debug atlas draw only; fine. Real use is in `unlit.wgsl` (see U3). | — | — |
| *(all)* | X1 | Four copies of the fullscreen-triangle VS and of `ViewUniforms` variants (`unlit`, `zone`, `particles`, `skinned` each define their own, with different fields). | `lib/fullscreen.wgsl` and one `FrameUniforms` (§4.4) shared by every scene shader; a Rust↔WGSL layout test. | S (SH2) |
| *(all)* | X2 | UV y-flip convention differs between passes (`post`/`bloom`/`ssao` flip in VS, `HARDWARE_POST_HOST_WGSL` and `lightmap.wgsl` don't). | One convention in `fullscreen.wgsl`; test that a GLDEFS post shader is not upside down. | B (for SH4) |

### 1.1.5 Order of work for SHR

1. R1, R2, R7, R10, R12, P1, P2, P3, P5, SK1 clip plane (bugs).
2. U1, U2, U3 (scene correctness), then U16 (aux G-buffer) — unlocks R5, E1, E2.
3. R3, R4, R6 (+ R5 with U16), P4, U4–U6, U10, U15 (faithful look + cost).
4. R8, R9, R11, R13, X1, X2, remaining Q items as part of SH2/SH5.

Each fix lands with an offscreen golden (or a numeric unit test for pure
functions such as tonemaps, LUT build, light falloff) and keeps the
`ClassicPalette` invariance test green.

### 1.1.6 Progress (2026-10-31)

| ID | Status | Notes |
|---|---|---|
| R1 | **done** | `BloomPass` one UBO per draw slot; `bloom_pass_ubos_keep_distinct_params` |
| R2 | **done** | `PostChain` binds bloom composite; `post.wgsl` samples `bloom_tex` (no 32-tap fake) |
| R7 | **done** | `needs_post` ignores unimplemented CVars; unit test |
| R10 | **done** | `PostChain::ensure_formats` recreates `PostPass` when `ldr_format` changes |
| R12 | **done** | Web: `output_gain = 1` + `encode_srgb = false` whenever post is on |
| P1 | **done** (single-pass) | FXAA luma/blend via per-tap `hdr_to_ldr` (LDR space); CA no longer disables FXAA |
| P2 | **done** | Sharpen uses `hdr_to_ldr` neighbours |
| P3 | **done** | Bayer dither before banding (1/31); 8-bit dither when not banding |
| P5 | **done** | `room_soft` linear; `flags.x` encodes sRGB only for non-`*Srgb` targets |
| SK1 clip | **done** | `skinned.wgsl` discards outside `clip_plane` (world pos) |
| U3 | **done** | Lightmap modulates sector term (no longer additive wash) |
| R3 | **done** (MVP) | Additive upsample chain coarse→fine + tent filter |
| R4 | **done** (MVP) | Soft-knee threshold + Karis-weighted first downsample |
| U1 / U2 / U16 | **done** (MVP) | Vertex N+TBN on walls/flats; dyn lights use world N·L + TBN normal maps; aux MRT is RGBA8 octahedral RG + flags B |

---

## 2. Goals and non-goals

**Goals**
- One shared, data-driven frame graph for desktop and web.
- HDR linear lighting on the Full tier; graceful LDR path on Downlevel (WebGL2).
- Faithful-by-default: `ClassicPalette` and `DoomSoftware` stay pixel-exact;
  every modern effect is opt-in via CVars and quality presets.
- GZDoom shader-mod compatibility (`HardwareShader` material + post-process)
  to the extent naga's GLSL frontend allows, with clear diagnostics.
- A Room-native WGSL extension for new content and mods.
- Shader hot reload in development; zero runtime shader compilation stalls in
  play (precompile at level load).

**Non-goals**: ray tracing, mesh shaders, bindless (not portable to WebGPU),
VR/stereo, SPIR-V passthrough.

---

## 3. Frame graph (SH0–SH1)

### 3.1 Shared renderer entry point
Create `room-render/src/frame.rs`:

```rust
pub struct FrameTargets {            // recreated on resize / scale change
    pub scene_color: Tex,            // Full: Rgba16Float; Downlevel: Rgba8Unorm (linear) if half-float RT unsupported
    pub scene_aux: Tex,              // Rg16Float or Rgba8: packed view normal (oct) + material flags; replaces today's R8 stencil_aux dummy
    pub velocity: Option<Tex>,       // Rg16Float, Full only (TAA / motion blur)
    pub depth_stencil: Tex,          // DEPTH_STENCIL_FORMAT (existing)
    pub history: Option<[Tex; 2]>,   // TAA ping-pong
    pub post_ping_pong: [Tex; 2],    // same format as scene_color
    pub ldr_output: Tex,             // swapchain format, before HUD
}
pub struct FrameInputs<'a> { pub camera: &'a Camera, pub scene: &'a SceneLists, pub settings: &'a RenderSettings,
                             pub post_requests: &'a PostRequests, pub time: f32, pub frame_index: u64, pub blend: [f32;4], … }
pub struct Renderer { /* passes, caches, targets */ }
impl Renderer {
    pub fn render_frame(&mut self, gpu: &GpuContext, encoder: &mut wgpu::CommandEncoder, inputs: &FrameInputs, out: &wgpu::TextureView);
}
```

Both shells call `render_frame`, then draw the HUD/canvas on `out`. Move the
duplicated pass plumbing (main pass, zone pass, portals/mirrors/camera
textures via `render_offscreen`) from `room-app/src/main.rs` and
`room-web/src/lib.rs` into `Renderer`. Keep the shells responsible only for
surfaces, input and UI.

### 3.2 Pass order (Full tier)

```
 1. [compute] light clusters, 1D shadowmaps        (existing)
 2. portals / mirrors / camera textures            (existing, render into scene targets with stencil)
 3. opaque + masked (UnlitPass)  → scene_color (linear HDR), scene_aux, velocity, depth
 4. sky                          → scene_color
 5. zones / mod world effects    → scene_color     (ModZonePass)
 6. translucent sprites/particles/decals (sorted)  → scene_color
 7. SSAO (half-res, from depth+aux) → applied as multiply on ambient term only (requires split: see 3.4)
 8. post "beforebloom" custom shaders (GLDEFS stage)
 9. bloom (threshold on HDR; brightmap/emissive weighted)
10. post "scene" custom shaders
11. TAA (optional) / tonemap + exposure + colour grade LUT + palette quantize (LUT)
12. screen blend (V_AddBlend: damage / bonus / suit), powerup screen shaders
13. AA (FXAA/SMAA if no TAA), sharpen/upscale (vid_scalefactor)
14. post "screen" custom shaders, dither, CRT
15. → ldr_output (swapchain format); then HUD/canvas; then present
```

Downlevel tier: steps 1, 7, 11-TAA, motion-dependent effects skipped; bloom at
2 levels; everything else as fragment passes. Each pass declares
`fn supported(tier, formats) -> bool`.

### 3.3 Linear output and the classic look
- `unlit.wgsl` must output **linear radiance**; delete the in-shader sRGB
  encode and "soft toe". Move the toe into the tonemap (`TonemapMode::RoomSoft`).
- `ClassicPalette` mode (COLORMAP LUT path) produces exact palette colours;
  in that mode the post chain runs **identity tonemap + no bloom/AO/AA**,
  writes palette RGB directly (the output target is non-sRGB or values are
  pre-linearised so the swapchain re-encodes to the same bytes).
  Acceptance: golden `testdata/goldens/room_*.png` bit-exact before/after.
- Palette quantize: replace the 256-iteration loop in `post.wgsl` with a
  **32³ (or 64³) RGB→palette-index 3D LUT** built on the CPU at PLAYPAL load
  (`quantize_playpal` already exists for reference/tests), sampled with
  nearest filtering; optional dithered lookup (§8 B2).

### 3.4 Lighting split for AO / GI
Keep one uber-shader but write **two colour outputs** on the Full tier when
SSAO/GI are enabled: `direct` (dyn lights, fullbright, brightmap) and
`ambient` (sector light term). Composite `ambient * ao + direct` in a cheap
fullscreen pass. When disabled, compile the single-output permutation
(no cost).

---

## 4. Shader module system (SH2)

### 4.1 Source layout
```
crates/room-render/src/shaders/
  lib/        common.wgsl (constants, packing), view.wgsl (ViewUniforms, FrameUniforms),
              color.wgsl (srgb, luminance, tonemaps, LUT), noise.wgsl (hash, blue-noise lookup, IGN),
              lightmode.wgsl (GZDoom light modes; moved out of unlit), lights.wgsl (clusters, shadows),
              fog.wgsl, fuzz.wgsl, material.wgsl (Material struct, PBR), fullscreen.wgsl (VS triangle)
  scene/      unlit.wgsl, sky.wgsl, sprite.wgsl, particles.wgsl, skinned.wgsl, voxel.wgsl, zone.wgsl, decal.wgsl
  post/       tonemap.wgsl, bloom_*.wgsl, ssao.wgsl, taa.wgsl, fxaa.wgsl, smaa_*.wgsl, grade.wgsl, crt.wgsl, …
  compute/    clusters.wgsl, shadowmap.wgsl, autoexposure.wgsl, normalgen.wgsl, upscale.wgsl
  permutations.ron   list of (shader, defines[]) to validate at build and to precompile
```

### 4.2 Preprocessor (in-house, ~300 lines, no C deps)
- `#import "lib/color.wgsl"` (once-only, path-relative, cycle error),
  `#define NAME [value]`, `#ifdef / #ifndef / #if NAME == value / #else / #endif`.
- Embedded at compile time with `include_str!` via a generated `shader_sources.rs`
  (build.rs) so the web build needs no filesystem; native dev builds can read
  from disk for hot reload (`ROOM_SHADER_DIR`).
- Line mapping kept for error messages (`file:line` in naga errors).
- Alternative considered: `naga_oil` (pure Rust) — allowed only if its naga
  version matches the workspace (`naga 25`) and it is added to
  `docs/appendix-a-dependencies.md`; otherwise use the in-house preprocessor.

### 4.3 Permutations and pipeline cache
- `ShaderKey { shader: ShaderId, defines: SmallVec<(Name, i32)>, tier }` →
  `wgpu::ShaderModule`; `PipelineKey { shader_key, vertex_layout, color_formats, depth, blend, cull, stencil, msaa }` → pipeline.
- `ShaderRegistry` owns both maps; pipelines created **at level load** for the
  permutations the scene needs (materials, render styles, fuzz types, light
  modes), never mid-frame. Missing permutation mid-frame → use the fallback
  material that frame and compile asynchronously (`wgpu` creation is sync; do
  it between frames, max N per frame).
- Use `wgpu::PipelineCache` where the adapter supports it (Vulkan) and persist
  it under the user config dir.
- `build.rs`: expand `permutations.ron` and validate each with naga (extends
  today's whole-file validation). Fail the build on any error.

### 4.4 Uniform / binding conventions
| Group | Content | Frequency |
|---|---|---|
| 0 | `FrameUniforms` (view_proj, prev_view_proj, inv matrices, eye, time, frame index, resolution, jitter, light mode params, fog, clip plane), lights, clusters, shadowmaps, lightmap atlas, blue-noise texture | per frame / per view (portals) |
| 1 | material textures + `MaterialUniforms` (custom shader uniforms) | per material |
| 2 | object/instance (models, skinned bones) | per draw |
| 3 | pass-specific (post inputs) | per pass |

`#[repr(C)] Pod` structs with `const _: () = assert!(size_of::<T>() % 16 == 0)`;
WGSL structs generated or checked against Rust layout in a unit test (parse the
WGSL struct with naga and compare offsets).
Downlevel limits (`Limits::downlevel_webgl2_defaults()`): keep ≤ 4 bind groups,
≤ 16 sampled textures per stage, **no storage buffers in fragment shaders**,
uniform arrays for lights (existing ≤ 32).

### 4.5 Hot reload (native dev)
`r_shader_reload` console command and an optional file watcher (debug builds,
`notify` crate if allowed; else polling mtimes once per second). On reload:
re-preprocess, validate, rebuild affected pipelines; on error keep the old
pipeline and print the error to the console with file:line.

### 4.6 Profiling
If `Features::TIMESTAMP_QUERY` is available, wrap each pass; `r_gpuprofile 1`
draws a per-pass ms overlay (via the UI draw list). Web: use
`timestamp-query` when the browser exposes it, else CPU timings only.

---

## 5. Content-facing shaders (SH3–SH4)

### 5.1 GZDoom material shaders (`HardwareShader <texture> { Shader "x.fp" … }`)
GZDoom material shaders are GLSL snippets that define one of
`vec4 ProcessTexel()`, `Material ProcessMaterial()` (`SetupMaterial(inout Material)` in newer versions),
`vec4 ProcessLight(Material, vec4 color)`, with built-ins.

**Built-ins to provide** (wrapper header, GLSL 450):
`getTexel(vec2)`, `vTexCoord`, `vWorldNormal`, `vEyeNormal`, `pixelpos` (world pos + depth),
`timer` (seconds), `uObjectColor`, `uObjectColor2`, `uDesaturationFactor`,
`uTextureMode`, `uFogColor`, `uLightLevel`, `Material` struct (`Base`, `Bright`,
`Glow`, `Normal`, `Specular`, `Glossiness`, `SpecularLevel`, `Metallic`,
`Roughness`, `AO`), `desaturate(vec4)`, `getBrightmap…` helpers, named
textures declared in GLDEFS (`Texture name "path"`), and GLDEFS uniforms
(`Uniform float name`, `vec2/3/4`, `int`).

**Pipeline (extends `compile_material_glsl`):**
1. Build a GLSL 450 wrapper: built-ins declared as **globals at the exact set /
   binding / location the Room template uses**, user snippet pasted in, a
   `main` that calls the user entry point(s) and writes their results to
   fixed outputs (`Material` packed in 3 vec4 outputs, or the texel).
2. `glsl_to_wgsl` (naga `glsl-in` → validate → `wgsl-out`).
3. Rename all user symbols with a `m<hash>_` prefix; strip the generated global
   declarations of known built-ins and the generated entry point; keep user
   functions and the call shim.
4. Splice into `scene/unlit.wgsl` behind `#define CUSTOM_MATERIAL` at the hook
   points `material_custom(…) -> Material` / `texel_custom(…)` /
   `light_custom(…)`; validate the full module with naga.
5. On any failure: log `GLDEFS <lump>:<line>: shader "<name>": <naga error>`,
   fall back to the default material, never crash. Cache translated WGSL by
   source hash (memory + optional disk cache on native).
6. Build a **compatibility corpus** under `testdata/shaders/gzdoom/` of
   original (GPL-compatible, self-written) snippets covering: texel warp,
   scrolling, glow, normal/spec material, `ProcessLight` override, uniforms,
   extra textures. Test: each translates and renders an offscreen golden.
Known naga GLSL-frontend gaps must be listed in `docs/13` §10 as they are
found (unsupported built-ins, implicit conversions) with the workaround.

### 5.2 GZDoom post-process shaders
GLDEFS `HardwareShader PostProcess <stage> { Name "n" Shader "file.fp" 330 Uniform float x … Texture t "path" Enabled }`
with stages **`beforebloom`, `scene`, `screen`**. Built-ins: `InputTexture`,
`TexCoord`, `FragColor`, declared uniforms, extra textures. Wrapper → naga →
splice into `compose_hardware_post_wgsl` style host (`fullscreen.wgsl` VS,
`input_tex`/`input_samp` at group 3). Each enabled shader is one fullscreen
pass at its stage, in GLDEFS order. Disabled by default unless `Enabled`.

### 5.3 Room-native WGSL extension
GLDEFS (Room extension, document in `docs/13` §10):
```
HardwareShader texture "FWATER1" { wgsl "shaders/water.wgsl"  Uniform float flow_speed = 1.0 }
HardwareShader PostProcess scene { Name "nightvision" wgsl "shaders/nv.wgsl" Uniform float strength = 1.0 }
```
WGSL material files implement `fn material_custom(ctx: MaterialCtx) -> Material`;
post files implement `fn post_custom(ctx: PostCtx) -> vec4<f32>`. They may
`#import "lib/…"`. Same validation/fallback rules. This is the recommended path
for new content (no translation layer).

### 5.4 Runtime control (CVars, console, mods)
- Console: `r_postshader <name> on|off`, `r_postshader_uniform <name> <uniform> <value>`.
- **Mods** (play scope, like the Mod HUD — sim *output*, saved, not in the
  checksum; shells apply it to the console player's view):
  `shader_set_enabled(seat1, name_ptr, len, on)`,
  `shader_set_uniform_f32(seat1, name_ptr, len, uniform_ptr, ulen, value)`,
  `shader_set_uniform_vec4(…, x, y, z, w)`. Append to `game_fns!`
  (`crates/room-script/src/game.rs`), implement in
  `crates/room-sim/src/script_mods.rs` writing `World.post_requests`
  (ordered map, caps: 32 shaders × 16 uniforms per seat), SDK +
  `crates/room-web/as/room.ts` mirrors, `docs/12` §15/§16.3 tables.
  Examples: Battle Royale "outside the zone" distortion; Survival low-health
  vignette; boss phase chromatic pulse.
- UI-scope modules (when they exist, `docs/27` §9.4) may call the same API
  client-locally.

---

## 6. Rules for every new shader

1. WGSL only in-tree; validated at build for all listed permutations.
2. Declare tier support; provide a Downlevel fallback or a clean "off".
3. A CVar with GZDoom's name when one exists (`gl_bloom`, `gl_ssao`,
   `gl_tonemap`, `gl_fxaa`, `gl_fogmode`, `gl_lights`, `r_drawfuzz`,
   `gl_fuzztype`, `gl_texture_filter`, `vid_scalefactor`, …), otherwise
   `r_<effect>`; add to quality presets `r_quality 0..3` (low/medium/high/ultra).
4. Budget at 1080p on the reference GPU (desktop mid-range) and on WebGPU
   integrated (see `docs/13` §14 targets); record measured cost in this doc.
5. No effect that changes gameplay readability by default (no default motion
   blur, DOF, heavy grain or chromatic aberration).
6. Pixel-art awareness: anything that blurs textures (TAA, upscaling, mipmaps)
   must have a mode that preserves crisp texels (`gl_texture_filter 0`).
7. Tests: offscreen golden (SSIM threshold) per effect on a synthetic test map
   (`room-testkit`) and Freedoom MAP01 when present; ClassicPalette invariance;
   web build (`cargo build --target wasm32-unknown-unknown -p room-web --features browser`).
8. Document the effect in `docs/13` (§5/§9/§10) and update `docs/21`.

---

## 7. Priorities

- **P0 — foundation**: nothing in Part II is visible without it.
- **P1 — high impact, low risk**: core modern look and faithful-retro options.
- **P2 — atmosphere and materials**: bigger wins on specific maps/mods.
- **P3 — optional / ultra**.

---

## 8. Shader catalogue

Columns: **ID · Name** — what / technique — tier (F = Full only, D = also
Downlevel) — budget @1080p Full — CVar — prio.

### A. Foundation (P0)

| ID | Shader / work | Technique & details | Tier | Budget | CVar |
|---|---|---|---|---|---|
| A1 | HDR scene target + frame graph | §3: `Rgba16Float` scene colour, shared `Renderer::render_frame`, both shells migrated | F/D (D: Rgba8 linear) | — | — |
| A2 | Linear `unlit` output | remove in-shader sRGB + toe; output linear; toe moved to `RoomSoft` tonemap | F/D | 0 | — |
| A3 | Tonemap pass | modes: none, Reinhard, Hejl-Dawson, Uncharted2 (GZDoom set), linear, palette, **AgX**, **Khronos PBR Neutral**, `RoomSoft` (current look); exposure | F/D | 0.1 ms | `gl_tonemap`, `r_exposure` |
| A4 | Palette 3D LUT | 32³/64³ RGB→PLAYPAL LUT, replaces per-pixel 256 loop; used by palette tonemap and dither | F/D | 0.05 ms | `gl_tonemap 5` |
| A5 | Screen blend | `V_AddBlend` RGBA blend (existing uniform) as its own small pass before AA | F/D | 0.02 ms | — |
| A6 | Aux G-buffer-lite | `scene_aux`: octahedral view normal (from geometry or normal map) + flags (sky, sprite, emissive, liquid); `velocity` (F) from `prev_view_proj` + per-vertex previous position for movers/models | F (aux also D) | +0.2 ms | — |
| A7 | Module system, permutations, pipeline cache, hot reload, profiler | §4 | F/D | — | `r_gpuprofile` |
| A8 | `HardwareShader` material + post wiring | §5.1–5.3 | F/D | per shader | — |
| A9 | Mod post control | §5.4 | F/D | — | — |

### B. Faithful / retro look (P1)

| ID | Shader | Technique & details | Tier | Budget | CVar |
|---|---|---|---|---|---|
| B1 | Software-renderer parity audit | Verify `DoomSoftware` / `ClassicPalette` against `room-render-soft` output on the same view (`soft-view.pgm` workflow exists): per-pixel diff report, fix light-band edges, sprite clipping, fuzz | F/D | 0 | `gl_lightmode` |
| B2 | Ordered dither + banding | 4×4/8×8 Bayer or blue-noise dither before palette LUT; optional "8-bit banding" quantize of the shade (32 colormap bands) in non-palette modes | F/D | 0.05 ms | `r_dither`, `r_banding` |
| B3 | Exact software fuzz | Vanilla `fuzzoffset[50]` column table, per-column offsets in screen space at the original 320×200 grid scaled to output (GZDoom `gl_fuzztype` "software" variant) — existing `fuzz_modulate` has approximations | F/D | 0 | `gl_fuzztype 6` |
| B4 | Low-res pixel mode | Render 3D at 320×200 / 640×400 (1.2 aspect) into the scene target, **integer-nearest** upscale; HUD at native res | F/D | saves time | `vid_scalefactor`, `vid_scalemode` |
| B5 | CRT | Scanlines (gaussian beam by row), aperture/slot mask, barrel curvature, phosphor glow (from bloom mip), vignette; presets "consumer TV", "PC monitor 1994" | F/D | 0.3 ms | `r_crt`, `r_crt_preset` |
| B6 | Screen wipes on GPU | Melt (vanilla column offsets, `wipetype 1`), burn (GZDoom fire), crossfade; uses two captured frames | F/D | 0.05 ms | `wipetype` |
| B7 | Sky stretch / software sky parity | software-like cylinder sky mapping (`r_skymode 2`) audit in sky shader | F/D | 0 | `r_skymode` |

### C. Lighting (P1–P2)

| ID | Shader | Technique & details | Tier | Budget | CVar | Prio |
|---|---|---|---|---|---|---|
| C1 | SSAO wired | existing `SsaoPass` into the graph using real depth + `scene_aux` normals; applied to ambient term only (§3.4); bilateral upsample | F (D: off) | 0.6 ms | `gl_ssao`, `gl_ssao_strength` | P1 |
| C2 | GTAO upgrade | horizon-based AO (2 slices × 8 steps, half-res, temporal accumulate with TAA) | F | 0.8 ms | `gl_ssao 3` | P2 |
| C3 | Soft 1D shadows | PCF / PCSS-lite on the existing 1D shadowmap (sample neighbours by angle, penumbra from light radius) | F | +0.2 ms | `gl_shadowmap_filter` | P1 |
| C4 | Lit sprites | dyn lights on sprites with a "fake normal": billboard normal blended toward the light by luminance gradient (GZDoom-like `gl_spritelight`) and wrap lighting; optional per-sprite normal maps from GLDEFS | F/D | 0.1 ms | `gl_light_sprites` | P1 |
| C5 | Auto normal/height maps | compute at texture load: Sobel on luminance → normal map + height; cached; enabled per texture class (walls/flats) only when no GLDEFS normal map; strength CVar | F (D: CPU at load, small textures) | load-time | `r_autonormals` | P2 |
| C6 | Emissive / brightmap bloom weighting | write emissive (brightmap, fullbright frames, `BRIGHT` states, additive styles) into HDR at >1.0 so bloom picks lights, not white walls | F/D | 0 | `gl_bloom` | P1 |
| C7 | Light shafts | screen-space radial blur from projected dyn lights and sky pixels (sky flag in aux), occlusion from depth; quarter res | F | 0.5 ms | `r_lightshafts` | P2 |
| C8 | Volumetric sector fog | froxel grid (160×90×64) filled from sector fog colour/density (`fogdensity`, `Sector_SetFade`) + dyn lights in-scatter, compute; integrated in fragment by depth | F | 1.2 ms | `r_volfog` | P3 |
| C9 | Light probes / colour bleed | per-sector average colour of flats/walls lit by dyn lights → indirect tint (cheap GI) | F/D | 0.1 ms | `r_ambientbounce` | P3 |
| C10 | Lightmap + dynamic integration | ensure lightmap (`LIGHTMAP` lump) and dyn lights compose in linear HDR; light probes for actors in lightmapped maps | F/D | 0 | `gl_lightmap` | P2 |

### D. Surfaces and materials (P2)

| ID | Shader | Technique & details | Tier | Budget | CVar |
|---|---|---|---|---|---|
| D1 | Animated liquids | for liquid flats (NUKAGE*, FWATER*, LAVA*, BLOOD*, SLIME*; list in a data table, overridable by GLDEFS): two scrolling normal maps (procedural noise if none) → refraction offset of scene behind (translucent 3D-floor water) or of the texture itself, fresnel, specular from dyn lights, emissive for lava/nukage; keep `ANIMDEFS` frames as base colour | F/D | 0.2 ms | `r_liquids` |
| D2 | Caustics / liquid glow on walls | walls adjacent to liquid sectors get animated caustic projection (lava: orange glow gradient like GLDEFS `glow`) | F/D | 0.1 ms | `r_liquids 2` |
| D3 | Heat haze | screen-space refraction behind fire/plasma/lava sprites and above lava flats; noise-driven UV offset masked by an emissive/heat buffer | F | 0.2 ms | `r_heathaze` |
| D4 | Parallax occlusion mapping | height map (GLDEFS or C5) on walls/flats, 8–24 steps, self-shadow optional, distance fade | F | 0.4 ms | `r_parallax` |
| D5 | Screen-space reflections | for flats flagged reflective (`Sector_SetPlaneReflection` already handled by plane mirrors; SSR for "glossy" materials with roughness < 0.3), hierarchical depth trace, fallback to sky/cubemap | F | 0.8 ms | `r_ssr` |
| D6 | Detail textures | tile a detail map at high frequency, fades with distance (fixes blurry upscaled flats) | F/D | 0.05 ms | `r_detail` |
| D7 | Texture upscaling filters | at load (compute/CPU): xBRZ / ScaleFX / hqx-style 2×–4× upscale for walls, sprites, HUD graphics; GZDoom has `gl_texture_hqresize` — keep names | F/D | load-time | `gl_texture_hqresizemode`, `gl_texture_hqresizemult` |
| D8 | Improved sky | sky dome with horizon fog blend to sector fog colour, optional two-layer animated clouds for "sky2", sun/moon glow from GLDEFS skybox metadata | F/D | 0.05 ms | `r_skyfx` |
| D9 | Decal normals | decals perturb the surface normal (bullet holes catch dyn light), blood decals get gloss | F | 0.05 ms | `r_decalfx` |

### E. Sprites, actors and effects (P1–P2)

| ID | Shader | Technique & details | Tier | Budget | CVar | Prio |
|---|---|---|---|---|---|---|
| E1 | Soft particles / sprites | depth-fade alpha where additive/translucent sprites and particles intersect geometry (removes hard cuts on explosions/fire) | F/D (D: needs depth texture read) | 0.05 ms | `r_softparticles` | P1 |
| E2 | Actor outlines | screen-space outline from a per-actor ID/mask buffer (stencil or R8 mask): uses for items glow (rarity colours in Battle Royale), teammates through walls (squads), elite auras (Survival), pickup highlight on aim; colour/width/through-walls flags | F/D | 0.2 ms | `r_outlines` | P1 |
| E3 | Hit flash | white/red additive flash on actors for a few tics after damage; renderer needs `last_damage_tic` in the render actor snapshot (sim already knows damage; add to `SimView`, render-only) | F/D | 0 | `r_hitflash` | P1 |
| E4 | Improved spectre/invisibility | refraction-based "predator" fuzz variant (screen texture offset by noise inside the sprite mask) as an extra `gl_fuzztype` | F | 0.05 ms | `gl_fuzztype 8` | P2 |
| E5 | Sprite shadows | blob or projected sprite silhouette on the floor (`r_actorspriteshadow` semantics), darkening by sector light | F/D | 0.1 ms | `r_actorspriteshadow` | P2 |
| E6 | Lit + stretched particles | particles receive dyn lights; motion-stretched quads along velocity; additive HDR for sparks | F/D | 0.1 ms | `r_particlefx` | P2 |
| E7 | Muzzle / explosion light + flash | short-lived dyn light + bloom-friendly emissive on weapon fire and explosions (from existing sim sound/FX events, render-only) | F/D | 0 | `gl_lights` | P1 |
| E8 | Death dissolve (optional) | noise-threshold dissolve on corpses removed by mods (e.g. Survival recycling stuck monsters) instead of a pop | F/D | 0 | `r_dissolve` | P3 |
| E9 | Weapon (psprite) lighting | psprite lit by dyn lights near the player + sector light; optional subtle sway shading | F/D | 0 | `gl_light_sprites` | P2 |

### F. Post-processing (P1–P2)

| ID | Shader | Technique & details | Tier | Budget | CVar | Prio |
|---|---|---|---|---|---|---|
| F1 | Bloom wired + improved | existing `BloomPass` into the graph; energy-conserving dual-filter (Kawase down/up), threshold soft knee, emissive-weighted (C6) | F/D (D: 2 levels) | 0.4 ms | `gl_bloom`, `gl_bloom_amount` | P1 |
| F2 | Auto exposure | compute luminance histogram (64 bins) → smoothed EV; clamp range so dark Doom maps stay dark; off in palette modes | F | 0.1 ms | `r_autoexposure` | P2 |
| F3 | Colour grading LUT | 32³ LUT per map/level (MAPINFO key or GLDEFS `colorgrade "LUTNAME"` lump PNG strip), crossfade between LUTs; mods can swap via A9 | F/D | 0.05 ms | `r_colorgrade` | P1 |
| F4 | SMAA 1x | edge detect → blend weights → neighbourhood blend (3 passes), better than FXAA on sprite edges | F/D | 0.4 ms | `gl_fxaa 2` (`r_aa smaa`) | P1 |
| F5 | TAA | Halton jitter (8), velocity reprojection, neighbourhood clamp (YCoCg variance), sharpen; **pixel-art safe mode**: no jitter on textures sampled with nearest (jitter only geometry edges) or TAA disabled when `gl_texture_filter 0` | F | 0.5 ms | `r_aa taa` | P2 |
| F6 | Upscale + sharpen | FSR1-style EASU + RCAS (spatial, pure WGSL) when `vid_scalefactor < 1`; CAS sharpen otherwise | F/D | 0.3 ms | `vid_scalemode`, `r_sharpen` | P1 |
| F7 | Damage / health feedback | directional red vignette (angle from the last damage source, render-only data from `SimView`), desaturation + heartbeat pulse below 25 % health, complements the GZDoom palette flash (keep the flash by default; this is an option) | F/D | 0.05 ms | `r_damagefx` | P1 |
| F8 | Powerup screen shaders | radiation suit (green tint + subtle mask edge), light amp (night-vision: luminance boost, green phosphor, noise, vignette), invulnerability (inverse colormap exists; add soft edge), berserk (red edges pulse); each only when the classic flash mode is off | F/D | 0.05 ms | `r_powerupfx` | P2 |
| F9 | Underwater | when the camera is inside a liquid/`Transfer_Heights` underwater sector: sine distortion, tint from sector colormap, fog thickening, low-pass audio hook (audio side) | F/D | 0.05 ms | `r_underwaterfx` | P2 |
| F10 | Vignette, film grain, chromatic aberration | subtle, all off by default, used by mods via A9 (e.g. boss phase pulse) | F/D | 0.05 ms | `r_vignette`, `r_grain`, `r_chromatic` | P2 |
| F11 | Depth of field | gather bokeh (half-res), used for menus/intermission/death cam/spectator, never during play by default | F | 0.6 ms | `r_dof` | P3 |
| F12 | Motion blur | camera-only (from velocity buffer), 8 samples, off by default | F | 0.4 ms | `r_motionblur` | P3 |
| F13 | Kill-confirm / hit marker FX | brief screen-edge pulse driven by mod post control (A9) — demo of the mod API for Battle Royale / Survival | F/D | 0 | — | P2 |

### G. UI / 2D (P1–P2)

| ID | Shader | Technique & details | Tier | Budget | CVar | Prio |
|---|---|---|---|---|---|---|
| G1 | Menu background blur | dual-Kawase blur of the last 3D frame behind menus, intermission, Mod HUD `MODAL` layer (`docs/27` §9); replaces flat dim when enabled (`DimAllowed` logic kept) | F/D | 0.2 ms (only when visible) | `r_menublur` | P1 |
| G2 | Mod HUD widget effects | rounded panels (SDF rect), soft shadow, glow border, pulse, progress-bar gradients for `PANEL`/`BAR`/`MARKER` widgets; implemented in the GPU canvas (`canvas_gpu.rs`) — today the canvas rasterises on the CPU and blits (`canvas_gpu.rasterize` → `blit`): move widget kinds to GPU-instanced quads | F/D | 0.1 ms | `r_hudfx` | P1 |
| G3 | SDF / MSDF fonts | crisp scaling for `NewSmallFont`/TrueType text at high `uiscale`; palette fonts stay bitmap | F/D | 0.05 ms | `ui_sdffonts` | P2 |
| G4 | Automap / minimap shader | antialiased lines, textured automap (`am_textured`), zone circles and markers for the Mod HUD `MAP` widget (`docs/28` BE2) in one instanced pass | F/D | 0.1 ms | `am_*` | P1 (with BE2) |

### H. Game-mode and mod-specific (P1–P2, depend on docs 27/28)

| ID | Shader | For | Technique & details |
|---|---|---|---|
| H1 | Zone wall 2.0 | Battle Royale (`docs/28` BE1; `zone.wgsl` exists) | animated hex/scanline pattern with distance fade, refraction of the scene behind the wall, intersection glow with geometry (depth difference), ground scan ring, red screen-edge pulse while outside (A9) |
| H2 | Team outlines | Battle Royale squads | E2 with through-walls flag and team colour; spectator follow-cam outline |
| H3 | Rarity glow | Battle Royale loot | E2 + pulsing emissive halo sprite + light colour per rarity |
| H4 | Elite aura | Survival elites (`docs/27` §6.4) | E2 outline in affix colour + affix-specific overlay (regen: green rising particles mask; phasing: E4 refraction; volatile: heat haze D3) |
| H5 | Extraction beam | Survival finale | volumetric-looking additive cylinder (like H1) with scrolling noise + light shaft (C7) |
| H6 | Face-swap GPU composite | `docs/26` (F7 lists "GPU composite" as an intentional deferral) | composite captured face onto sprite heads in a fragment shader at sprite draw (anchor UV rect + mask), instead of CPU baking per frame in live mode |
| H7 | Spawn/teleport effect | both mods | screen-space ripple + flash at telefog positions (render-only from FX events) |

### I. Hardcore shooting — weapon feel and impacts (P1–P2)

Goal: every shot reads and *feels* heavy (flash, tracer, impact, sound-synced
camera kick, debris, lingering smoke), without changing the playsim. All of
it is render-only and driven by **FX events** (§8.K).

| ID | Shader / effect | Technique & details | Tier | Budget | CVar | Prio |
|---|---|---|---|---|---|---|
| I1 | Muzzle flash 2.0 | HDR additive flash on the psprite (existing flash states stay the source of timing) + procedural star/cone flash quad in front of the barrel: radial noise mask, 3–5 spikes, random rotation and scale per shot from a **render RNG** seeded by the event (§8.K), 1–2 frames, intensity > 1.0 so bloom (F1) catches it; world flash light via E7 (warm, 1 frame, radius by weapon) | F/D | 0.05 ms | `r_muzzlefx` | P1 |
| I2 | Hitscan tracers | `HitscanFired{origin, end}` → additive stretched billboard from the muzzle (psprite → world transform) to the impact, 2–4 frames fade, HDR core + soft glow; chaingun every 2nd–3rd bullet, shotgun pellets thin and short; depth-tested against the scene | F/D | 0.05 ms | `r_tracers` | P1 |
| I3 | Surface impact FX | per impact: material from the hit texture through a lookup table (TERRAIN lump + Room `SURFACEFX` lump: texture/flat pattern → `metal`, `stone`, `wood`, `flesh`, `liquid`, `computer`, `dirt`): sparks (metal: HDR orange, motion-stretched, bounce once, tiny dyn light 2 frames), dust/chips (stone: lit grey soft particles + small debris quads), splash (liquid: ring + droplets), electric arcs (computer panels). GPU instanced particles (E6), soft (E1), lit | F/D (D: fewer particles) | 0.2 ms @ 200 particles | `r_impactfx` | P1 |
| I4 | Bullet holes 2.0 | existing `BulletChip` decals rendered with a generated normal/height (D9) so dyn lights catch them; **hot edge**: emissive orange rim fading over 0.6 s for chaingun/rifle hits on metal; rocket/barrel **scorch** decals (radial, soot gradient, slight normal bump) on walls *and floors* (needs J2 projected decals) | F/D | 0.05 ms | `r_decalfx` | P1 |
| I5 | Camera kick & recoil (visual) | per `WeaponFired` for the console player: spring-damped view pitch/yaw/roll impulse applied **only to the render camera** (never to the sim angle, so aim/netplay are unchanged), weapon psprite kickback offset + rotation; strength per weapon table; combined with existing sim `quakes` (`World.quakes`) with smooth noise instead of raw jitter; global cap | F/D | 0 | `r_viewkick`, `r_viewkick_scale` | P1 |
| I6 | Barrel smoke & heat | after sustained fire (chaingun > 1 s, shotgun after each shot): lit soft smoke wisps rising from the muzzle in screen space for the psprite and in world space for other players; heat shimmer (D3) above the barrel | F/D | 0.05 ms | `r_weaponsmoke` | P2 |
| I7 | Shell casings | render-only casings ejected per shot (shotgun on pump frame, chaingun/pistol per bullet): small mesh or sprite, simple render-side physics against the sector floor height from `SimView` (bounce, spin, settle), metallic specular, fade after 10 s; LRU cap 256 | F/D | 0.05 ms | `r_casings` | P2 |
| I8 | Explosion 2.0 | `Explosion{pos, radius, damage}` (rocket, barrel, `A_Explode`): (a) HDR fireball from the existing sprite frames boosted by an emissive noise mask; (b) **shockwave**: expanding screen-space refraction ring (distortion from a ring normal, masked by depth); (c) bright flash light 3 frames (E7); (d) debris + lit smoke column (soft particles, rising, slow fade); (e) floor scorch (I4); (f) camera shake by distance (I5); (g) dust kicked from floors and ceilings within the radius | F (D: a, c, d, e, f) | 0.4 ms per active explosion, max 8 | `r_explosionfx` | P1 |
| I9 | Projectile trails | rockets: lit smoke trail ribbon (camera-facing strip from a position history, render-side), HDR exhaust glow; plasma/BFG balls: additive core + halo + distortion, lights (GLDEFS already) | F/D | 0.1 ms | `r_trails` | P1 |
| I10 | Hit confirm | on `HitscanFired` with `hit_actor` from the console player: crosshair pulse (UI) + optional subtle white flash on the victim (E3 hit flash) + kill confirm pulse (F13); all one CVar | F/D | 0 | `r_hitconfirm` | P1 |
| I11 | Weapon psprite lighting | psprite lit by its own muzzle flash (I1) and dyn lights near the player (E9), specular highlights on metal parts via generated normals (C5) | F/D | 0.02 ms | `gl_light_sprites` | P2 |

### J. Blood and gore (P1–P2)

Goal: violent, readable, physical blood that stays render-only. The sim keeps
spawning its `Blood` actors exactly as today (`room-sim/src/hitscan.rs`
`spawn_blood`, damage-tiered states, `BloodType` / `BloodColor` from
DECORATE), so demos, netplay and saves are unaffected; the renderer adds
layers on top from FX events. All effects respect the actor's **blood colour**
(red, Baron/Hell Knight green, Cacodemon blue, custom DECORATE `BloodColor`)
and the gore level (J10).

| ID | Shader / effect | Technique & details | Tier | Budget | CVar | Prio |
|---|---|---|---|---|---|---|
| J1 | Blood spray particles | `BloodHit{pos, dir, damage, color}`: GPU particles in a cone along the shot direction (exit spray) and a smaller back-spray toward the shooter; count/size by damage; gravity; lit (E6), soft (E1); dark linear red with slight emissive 0; on reaching the sector floor height (render-side test against `SimView` floor/3D-floor heights) a particle becomes a floor splat (J2); on hitting a wall within 64 units along `dir` (cheap ray vs. the BSP walls in the scene lists) → wall splat (J3) | F/D (D: half count) | 0.15 ms @ 500 particles | `cl_bloodtype` (GZDoom: 0 sprites, 1 sprites+particles, 2 particles), `r_bloodfx` | P1 |
| J2 | **Projected floor decals** (blood pools, splats, scorch) | new decal surface kind: today decals are per side (wall) only (`room-render/src/decals.rs`). Full tier: **deferred box projector** — draw a unit box per decal, reconstruct world pos from depth, reject by normal (`scene_aux`, A6) so floors, slopes and 3D floors all work; Downlevel: CPU-clipped quad at the sector floor height. Pools: radius **grows** over 2–4 s (stretcher), shape from a noise-thresholded mask (unique per decal via seed), slightly offset/rotated per splat | F/D | 0.1 ms @ 256 decals | `cl_bloodsplats`, `cl_maxdecals` | P1 |
| J3 | Directional wall splatter + drips | wall splats elongated along the impact direction (stretch UV by `dot(dir, wall)`), satellite droplets; **drips**: for big splats a vertical drip mask animated downward over 5–10 s with a thin wet trail (UV-scrolling mask in the decal shader, stops at the floor) | F/D | 0.05 ms | `r_blooddrips` | P1 |
| J4 | Wet blood shading | fresh blood: low roughness, fresnel specular from dyn lights and sky, slight darkening of the underlying surface, normal from the mask gradient (thick edges); **drying** over 30–60 s: roughness ↑, colour darkens toward brown-red, specular fades; SSR (D5) on fresh pools when enabled | F/D (D: no SSR) | 0.02 ms | `r_bloodwet` | P2 |
| J5 | Corpse pools | when a monster dies (`ActorDied{pos, color, mass}` event), a pool spreads under the corpse over 6 s (J2), size by mass/health; follows `Crush`/gib states (gibs → larger pool + splats) | F/D | 0 | `r_corpsepools` | P1 |
| J6 | Gibs 2.0 | on `XDeath`/gib and crusher deaths (`ActorGibbed`): render-only chunk particles (small sprite or low-poly meshes, coloured by blood colour), spin + bounce on floors, each leaves a short blood trail ribbon and a splat on landing; LRU cap 128; sim gib actors unchanged | F/D | 0.1 ms | `r_gibs` | P2 |
| J7 | Screen blood (lens) | when the console player takes heavy close damage (`damage_count` jump, explosion within 128) or gibs an enemy at melee range: screen-space droplets/smears on the "lens" — refraction of the scene + red tint, droplets slide down, fade over 2–3 s; intensity from damage; never covers the crosshair centre region | F/D | 0.05 ms | `r_screenblood` (default **off** in competitive rooms) | P2 |
| J8 | Weapon blood | fist (berserk) and chainsaw kills accumulate a blood mask on the psprite (per weapon, screen-space mask texture painted at contact points), decays over ~20 s or on weapon switch; chainsaw also sprays toward the camera (J1, short range) | F/D | 0.02 ms | `r_weaponblood` | P2 |
| J9 | Sprite wound overlay (optional) | darken + red-tint mask on a damaged monster sprite (frame-independent: mask in sprite-normalised UV from hit height), accumulates with damage, cleared on death; subtle so animation frames don't pop | F/D | 0.02 ms | `r_wounds` | P3 |
| J10 | Gore levels & limits | `r_gore` 0 = no blood (puffs only, for streaming / younger players), 1 = vanilla (sim blood sprites only), 2 = enhanced (J1–J5, J7 off), 3 = full (all). `r_bloodcolor_override` (e.g. force green — classic "low gore" option). Global caps: blood particles 4096, floor decals 512 (LRU, oldest fade out over 1 s), wall decals `cl_maxdecals`. Photosensitivity: cap screen-blood and kick intensity (`r_reduce_flashing`) | F/D | — | `r_gore` | P1 |

### K. Required engine data: render FX events (prerequisite for I and J)

The renderer only sees `SimView` and drained outputs (`World.decal_events`
via `drain_decal_events`, `World.quakes`, player `damage_count` /
`attacker`). Add a **render-only FX event stream**, drained by the shells and
passed to `Renderer`:

```rust
// room-sim/src/world.rs — never read by the sim, not in checksum, not saved
pub enum FxEvent {
    WeaponFired   { player: u8, weapon: ClassId, flash_state: Option<StateId> },
    HitscanFired  { shooter: ActorId, origin: DVec3, end: DVec3, normal: Option<DVec3>,
                    surface: SurfaceHit /* texture id + side/flat */, hit_actor: Option<ActorId>, damage: i32 },
    BloodHit      { victim: ActorId, pos: DVec3, dir: DVec3, damage: i32, color: Rgb8 },
    ActorDied     { actor: ActorId, pos: DVec3, color: Option<Rgb8>, mass: i32, gibbed: bool },
    Explosion     { pos: DVec3, radius: f64, damage: i32, source_class: ClassId },
    ProjectileImpact { pos: DVec3, normal: Option<DVec3>, class: ClassId },
}
pub fx_events: Vec<FxEvent>, // + drain_fx_events()
```

- Emit points: `hitscan.rs` (line attack end point, puff vs. blood branch near
  `spawn_blood`), weapon fire actions (`A_FireBullets`, `A_FireShotgun*`,
  `A_FireCGun`, `A_FireMissile`, `A_Saw`, `A_Punch`, DECORATE `A_FireBullets`
  / `A_CustomPunch` / `A_RailAttack`), `A_Explode` / radius attacks, death and
  gib state entry, missile `Death` state entry.
- Push only when a renderer is attached (flag on `World`, off in the server
  verifier and headless tests unless a test enables it) so the sim cost is zero
  on servers.
- **Render RNG**: every visual random value comes from a small PCG seeded with
  `hash(tic, event index)`, so a spectator replay or a demo playback looks the
  same as live play, and the sim RNG is never touched.
- Pauses / rewinds: events are tied to a tic; on demo seek the renderer drops
  queued effects.
- Mods: `docs/27`/`docs/28` mods can also request effects through the A9 mod
  post control (e.g. kill-confirm pulse); no mod-side FX imports needed for v1.

### L. Supporting work for I/J

| ID | Work | Notes |
|---|---|---|
| L1 | Render-side particle system 2.0 | today particles are ACS `SpawnParticle` billboards (`ParticleStore`, `particles.wgsl`). Add emitters (cone, sphere, ring), per-particle gravity/drag/bounce vs. floor height, colour/size curves, lit + soft + stretched modes, sub-UV flipbooks; GPU-simulated on Full (compute), CPU on Downlevel; global budget manager |
| L2 | Decal system 2.0 | wall + projected floor/ceiling decals (J2), per-decal seed for unique masks, material params (wetness, emissive rim, normal strength), LRU, save/restore via existing `export_decals` / `import_decals` |
| L3 | Effect definition data | `FXDEFS` text lump (Room extension, parsed in `room-defs`, fuzzed like every parser): per weapon (flash, tracer, kick, casings, smoke), per surface material (impact FX), per blood colour / class (spray, pools, gibs). Defaults shipped in `wadsrc/`; mods/PWADs override. Keeps the shaders generic and the content data-driven |
| L4 | Audio sync (outside this doc) | impact/blood events also drive positional sound variety (surface-specific impact sounds) in `room-audio`; same event stream |
| L5 | Competitive constraints | Battle Royale rooms (`docs/28`) may force caps: max `r_viewkick`, no screen blood, gore ≥ 1 allowed; enforced client-side as a room option shown in the lobby (render-only, cannot desync) |

---

## 9. Milestones

| ID | Scope | Acceptance |
|---|---|---|
| **SH0** | A1–A2, A5: shared `Renderer::render_frame`, HDR scene target, linear `unlit`, both shells migrated; no visual change in default settings except toe moved | Existing goldens pass (SSIM ≥ previous); ClassicPalette goldens **bit-exact**; desktop + web build; perf within 5 % of today on `freedoom_map01_offscreen_fps_floor`. |
| **SH1** | A3, A4, A6; wire C1 (SSAO), F1 (bloom), F4/FXAA, F6 upscale into the graph with CVars + `r_quality` presets | Per-effect offscreen goldens; Downlevel path renders (webgl2 smoke); presets switch at runtime without stalls. |
| **SHR** | Review fixes from §1.1 (all **B** rows + rows marked SHR), in the order of §1.1.5 | Each fix has a golden or unit test; bloom passes see their own uniforms (test reads back mip parameters); FXAA runs on LDR; palette mode output contains only PLAYPAL colours (test scans the image); skinned models respect mirror clip planes; desktop vs web offscreen output match for identical settings. |
| **SH2** | A7: preprocessor, permutations manifest validated in `build.rs`, pipeline cache, hot reload, GPU profiler | Every permutation validated at build; `r_shader_reload` works on desktop; profiler overlay lists passes. |
| **SH3** | A8 material path: GZDoom `HardwareShader` materials + Room WGSL materials; compatibility corpus | Corpus translates and renders goldens; broken shader falls back with a precise diagnostic. |
| **SH4** | A8 post path (3 stages) + A9 mod post control (imports, SDK, AS mirror, docs) | E2E: a test mod enables a post shader and sets a uniform on one seat; save/load keeps it; lockstep checksum unaffected. |
| **SH5** | P1 look: B2–B6, C3, C4, C6, E1, E2, E3, E7, F3, F7, G1, G2 | Goldens + `docs/13` sections; budgets measured and recorded in §8. |
| **SH6** | Liquids & atmosphere: D1–D3, D6–D8, F8, F9, C10 | Goldens on a liquids test map; Downlevel fallbacks verified. |
| **SH7** | Advanced: C2, C5, C7, D4, D5, D9, E4–E6, E9, F2, F5, F10, F13, G3, G4 | Goldens; TAA pixel-art safe mode verified on nearest-filtered textures. |
| **SH8** | Mod-specific H1–H7 (after docs 27/28 engine items land) | E2E with the mods; goldens. |
| **SH10** | Hardcore shooting & blood: K (FX events + render RNG), L1–L3, then I1–I5, I8–I10, J1–J3, J5, J10 (P1); then I6, I7, I11, J4, J6–J8 (P2); J9 optional | FX events emitted only with a renderer attached (server verifier unchanged, checksum unchanged); spectator replay and live view produce identical effects (render RNG); goldens of impact/blood/explosion scenes per tier; `r_gore 0` shows no blood at all; caps respected under a 60 s chaingun stress test at ≥ the `docs/13` §14 fps floor. |
| **SH9** | P3 extras: C8, C9, E8, F11, F12; final perf pass against `docs/13` §14 | Performance matrix recorded. |

## 10. Testing details

- **Golden harness**: extend `room_render::offscreen` with a `RenderSettings`
  argument so each test pins CVars; store PNGs under
  `testdata/goldens/shaders/<id>_<tier>.png`; compare with SSIM (existing
  helper) and a max-pixel-diff for palette modes (must be 0).
- **Downlevel in CI**: run the offscreen tests with the GL backend where a
  software adapter exists (lavapipe/SwiftShader), else skip with a log line —
  same policy as `docs/13` §13.
- **Permutation validation** at build (§4.3); a unit test asserts
  Rust↔WGSL uniform struct layouts match.
- **Perf**: soft floors like `freedoom_map01_lights_shadow_fps_floor`; add one
  per preset (`r_quality 0..3`).
- **Web**: `cargo build --target wasm32-unknown-unknown -p room-web --features browser`
  and a manual check in a WebGPU browser and a WebGL2-only browser.

## 11. Open questions for the owner

1. Default preset for new players: faithful (palette/ClassicPalette, no post)
   or "modern" (`r_quality 2`)? Separate defaults for desktop and web?
2. Should GZDoom shader-mod compatibility (SH3–SH4) come before the in-house
   effects (SH5), or the reverse?
3. Allow `naga_oil` (and `notify` for hot reload) as dependencies, or keep the
   in-house preprocessor and polling?
4. TAA at all, given pixel-art textures (SMAA might be enough)?
5. Which mod-facing effects should be allowed in competitive Battle Royale
   rooms (outlines through walls are a gameplay advantage — server option)?
6. Default gore level for new players (`r_gore 2` enhanced or `3` full), and
   whether lens blood (J7) and view kick (I5) are on by default.
7. Effect data in a new `FXDEFS` lump (L3), or reuse/extend GZDoom lumps only?
