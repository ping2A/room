# 13 — Renderer (`room-render`)

GZDoom references: `src/rendering/hwrenderer/scene/*` (`hw_bsp.cpp`, `hw_clipper.cpp`,
`hw_walls.cpp`, `hw_flats.cpp`, `hw_sprites.cpp`, `hw_drawlist.cpp`, `hw_portal.cpp`,
`hw_sky*.cpp`, `hw_decal.cpp`, `hw_lighting.cpp`, `hw_fakeflat.cpp`), `src/common/rendering/hwrenderer/data/`,
`wadsrc/static/shaders/glsl/*` (main.fp/vp, lighting, fog, fuzz styles, tonemap, bloom, ssao),
`src/rendering/swrenderer/` (classic software renderer reference).

## 1. Principles

- **wgpu** only; shaders in **WGSL**, validated by naga at build time (`build.rs` runs naga on every shader so errors fail the build).
- Inputs: `&SimView`, `&Definitions`, `&UiDrawList`, interpolation `alpha`. The renderer owns GPU resources, decals storage, texture caches, and its own render-only state (sky scroll accumulators, particle GPU buffers).
- Two quality tiers sharing one code path, selected by device capabilities:
  - **Full** (Vulkan/Metal/DX12/WebGPU): compute shaders, storage buffers, clustered lights, shadowmaps, HDR post.
  - **Downlevel** (WebGL2 via wgpu GL backend): no compute → CPU light assignment per surface (GZDoom-style), no shadowmaps, LDR post subset.
- Faithful look first: Doom light diminishing, sprite clipping, sky behaviour, fuzz, translucency, palette flashes. Modern extras (bloom, SSAO, lights) are toggled by CVars with GZDoom names where they exist (`gl_lights`, `gl_bloom`, `gl_ssao`, `gl_tonemap`, `gl_fogmode`, `gl_lightmode`/`gl_lightmode` replacements, `r_drawfuzz`, `gl_texture_filter`, `gl_texture_filter_anisotropic`, `vid_scalefactor`, `gl_sclipping`, `gl_spriteclip`, `r_spriteadjust`, …).

## 2. Frame structure

```
render_frame(view, ui):
  1. resolve camera (player view or camera actor; interpolated; viewheight, bob, quake offsets, roll, FOV, chasecam)
  2. prepare dynamic texture updates (animated textures frame indices, warping textures, camera textures due this frame)
  3. scene = build_scene(view, camera)        // CPU: BSP traversal + clipping → draw lists (§3)
  4. upload per-frame buffers (streamed geometry, instance data, lights, uniforms)
  5. gpu passes:
       a. [full] light culling compute (clusters), shadowmap compute (1D). Particles are simulated in the sim, not here.
       b. portal passes (recursive, stencil) — each portal renders its own sub-scene (§6)
       c. main scene: opaque (front-to-back) → masked (alpha-test) → sky → translucent (sorted back-to-front) → weapon psprites
       d. post: ssao → bloom → tonemap → palette-flash blend → fxaa → gamma/contrast/saturation → scale to swapchain
       e. 2D canvas (HUD, status bar, menus, console, automap overlay) in output resolution
  6. present
```

Render resolution scaling (`vid_scalefactor`, `vid_scalemode`) renders the 3D scene into an offscreen target, then upsamples (nearest or linear) to the swapchain; the classic modes `320x200`/`640x400` with 1.2 pixel aspect ratio are available as scale modes.

## 3. Scene building (CPU)

Port GZDoom's hardware-renderer BSP walk:

- Traverse nodes front-to-back from the camera; maintain an **angle clipper** (`Clipper`: sorted list of occluded angular ranges, using `angle_t`-style pseudo-angles from `R_PointToPseudoAngle` for speed and exactness) to reject subsectors behind solid walls. Check bounding boxes against the clipper before descending.
- For each visible subsector: mark sector visible; emit flats (floor/ceiling + 3D floor planes) once per sector per plane (GZDoom draws flats per subsector list — follow its "render flats by sector with subsector lists" model so plane geometry can be cached per sector).
- For each seg: build wall parts (top/mid/bottom, 3D floor sides, translucent midtextures, sky walls, horizon/portal walls), compute texture coordinates (offsets, scales, pegging flags `ML_DONTPEGTOP/BOTTOM`, `WORLDPANNING`, wrap/clip midtex, `ML_3DMIDTEX`), light levels (per-tier light, fake contrast `vertwallshade/horizwallshade`, `smoothlighting`), colors (per-tier/sector, gradients `glow`), and add the solid range to the clipper.
- Sprites: for every actor linked into a visible sector (sector `thinglist` + render-touching sectors) emit a sprite/model/voxel instance; for particles, emit particle quads; for decals on visible sides, emit decal quads.
- Fake flats (Boom `Transfer_Heights` 242): compute the substituted sector for the camera's position like `hw_fakeflat.cpp` (`gl_FakeFlat`): above/below/inside cases with `ceilingplane`, `floorplane` swaps and fake light/colormap.
- Use rayon on native to parallelize wall/flat vertex generation after the visibility walk (the walk itself is sequential).

Output: `SceneLists { opaque_walls, masked_walls, translucent: Vec<SortItem>, flats, sky_parts, sprites, models, portals, decals, lights_visible }`.

## 4. GPU data model

### 4.1 Textures

- Every `TextureId` maps to a GPU material: base color texture + optional brightmap, normal/specular/PBR maps (GLDEFS materials), glow info, translation-aware variants.
- Paletted source textures are uploaded **twice when needed**: `R8Uint` index texture (for palette/classic mode and translations) and lazily an `Rgba8UnormSrgb` expanded version with mipmaps (for filtered/true-colour mode). True-colour sources (PNG RGBA, JPEG) only `Rgba8`.
- Binding strategy: if the adapter supports `TEXTURE_BINDING_ARRAY` + `SAMPLED_TEXTURE_AND_STORAGE_BUFFER_ARRAY_NON_UNIFORM_INDEXING` (native Vulkan/DX12/Metal), use a bindless texture array indexed per instance. Otherwise (WebGPU, WebGL2) bucket draws by material and use bind groups (cache keyed by material id).
- Sprites and patches: per-sprite-frame textures, optionally packed into atlases for the 2D canvas (fonts definitely atlased).
- Hi-res replacements (`hires/` namespace, `HI_START`), texture scaling (`XScale/YScale`), upscaling filters (`gl_texture_hqresize` xBRZ/hqNx in GZDoom) — C; implement xBRZ only if time permits.
- Texture filtering CVars: none/linear/trilinear, anisotropic; "nearest + mipmaps" classic look default.
- Warping textures (`warp`/`warp2` ANIMDEFS) and animated textures handled in shader (warp) / by frame index lookup (anim).
- Camera textures: render targets updated at most every N frames (default: at most once per tic) when visible.

### 4.2 Geometry buffers

- **Flats**: per-sector static vertex/index buffers of triangulated GL subsectors (fan triangulation of convex polygons). Heights computed in the vertex shader from the sector's current plane equation (`SecPlane` uploaded per frame in a storage/uniform buffer) so moving floors/sloped planes never require re-upload.
- **Walls**: generated per frame into a ring-buffer vertex arena (GZDoom does the same) — simplest & handles moving planes, scrolling, polyobjects.
- **Sprites/particles/decals**: instance buffers.
- **Models**: static vertex buffers per model frame set; per-instance (`frame_a`, `frame_b`, `lerp`, transform, light).

### 4.3 Uniform/storage buffers

`ViewUniforms` (matrices, camera pos, time, fog params, lightmode), `SectorPlanes[]`, `SectorLights[]` (light level, color, fade, desaturation, glow), `LightBuffer[]` (dynamic lights), `ClusterGrid`, `ShadowmapIndex[]`, `MaterialParams[]`, `Translations` (2D texture: 256 × N RGBA rows + index rows).

## 5. Lighting

### 5.1 Sector lighting (Doom light diminishing)

Implement the GZDoom light modes in shader, selected by `lightmode` (MAPINFO `lightmode` overrides CVar):

| Mode | Behaviour |
|---|---|
| `Standard` (0) | GZDoom classic hardware: linear distance attenuation of light level, `gl_light_ambient` floor |
| `Bright` (1) | as standard, brighter curve |
| `Doom` (2) | GZDoom "Dark": emulates Doom's diminishing with distance |
| `DoomSoftware` (8) | Software-renderer equation: `R_DoomLightingEquation` (shade = 64 − (light/4 ⋯) with `globVis` depth factor), 32 colormap bands, exact vanilla curves; `r_visibility` |
| `Build` (16) | Build-engine-like visibility (GZDoom 4.x) |
| `ClassicPalette` (Room) | `DoomSoftware` + paletted textures sampled through the actual `COLORMAP` LUT (index → shaded index → PLAYPAL), giving pixel-faithful vanilla banding and colours; true-colour textures fall back to computed shade |

Port the shader equations from `wadsrc/static/shaders/glsl/main.fp` (`R_DoomLightingEquation`, `getLightLevel`, fog functions) so outputs match GZDoom for modes 0/1/2/8/16.

Additional inputs: `extralight` (gun flash), fixed colormaps (invulnerability inverse map, light amp "fullbright"), sector light colour, fade colour (fog) and `fogdensity`, desaturation, `gl_fogmode` (off / standard / radial), wall fake contrast, sprite light (actor sector light + `LightLevel` override + `BRIGHT` frames + `+FORCEXYBILLBOARD` …), per-plane/per-tier light overrides, 3D floor light splits (light lists clip walls into bands — generate split wall pieces on CPU).

### 5.2 Dynamic lights

- Sources: GLDEFS lights attached to actor frames, light things (9800–9885 range), `A_AttachLight` from modules/DECORATE (GZDoom 4.x), muzzle-flash lights, sector lights.
- Types: point (with optional `attenuate`), spot (inner/outer angle, direction from actor angle/pitch), pulse, flicker, random flicker. GZDoom ticks dynamic lights in the playsim, but they never affect gameplay. Room animates them in the renderer from `SimView` data (light definition + spawn time + level time), so they stay out of the deterministic sim.
- Additive/subtractive/attenuated lights; `dontlightself`, `dontlightactors`, `dontlightmap`.
- **Full tier**: clustered forward shading (16×9×24 clusters, compute assignment, max 256 lights per cluster), plus GZDoom-compatible **1D shadowmaps**: for each shadow-casting light, a compute pass ray-casts 1024 directions in 2D against a GPU line BVH of solid walls (exactly GZDoom's approach, `gl_light_shadowmap`, `gl_shadowmap_quality`, `gl_shadowmap_filter`); fragments sample the 1D map by angle.
- **Downlevel tier**: per-surface light lists computed on CPU (GZDoom's original method), max 16 lights per draw.
- Lightmaps: GZDoom 4.12+ supports baked lightmaps (`LIGHTMAP` lump from ZDRay) — S priority: load and sample when present.

### 5.3 Material shading

Default model = unlit diffuse × light (Doom look). When GLDEFS defines normal/specular or PBR maps, use Blinn-Phong or a metallic/roughness BRDF with dynamic lights. Brightmaps add fullbright regions. Glowing flats (`glow` in GLDEFS / UDMF glow) add a height-based gradient on adjacent walls.

## 6. Portals

Port `hw_portal.cpp` concepts:

- Portal types: line portals (visual and linked), sector stacks (upper/lower), sky viewpoints (skybox actors), horizon, mirrors, plane mirrors (`Sector_SetPlaneReflection` — reflective floors), sky (special-cased).
- Algorithm: when a portal surface is visible, record it; after opaque scene, for each portal: (1) draw portal surface into stencil (increment), (2) clear depth inside stencil, (3) render the sub-scene from the transformed camera with a clip plane (user clip distance or manual discard in fragment shader on WebGPU), restricted to stencil, recursion depth limit `gl_portals_max_recursion` (default 4), (4) seal by writing portal surface depth.
- The sub-scene BSP walk starts with a clipper pre-filled from the portal's angular extent.
- Linked portals with consistent displacement can be rendered without recursion cost by shifting the view (GZDoom treats them as "line-to-line portals").

## 7. Sky

- Classic cylinder sky: `sky1` (+ `sky2` for doublesky with transparency), sky scroll speeds (MAPINFO `sky1 name speed`), `skyoffset` (ANIMDEFS), texture height rules (tall skies, `skystretch`/`forcenoskystretch`, `r_skymode`), `skyfog`, MBF sky transfers (per-sector sky via line 271/272 → texture + offsets + flipping).
- Skyboxes: GLDEFS `skybox` cubemaps (6-face or 3-face variant) and sky viewpoint portals (actor-based skyboxes render the sub-scene with the camera at the viewpoint, rotation only).
- Sky dome rendering as GZDoom's `FSkyVertexBuffer` (hemisphere mesh with texture coordinates matching software renderer horizon placement; `r_skymode 2` = cylinder ≈ software).
- Sky stencil: sky surfaces are drawn into a stencil/sky mask, sky drawn after opaque to fill masked pixels.

## 8. Sprites, models, voxels, particles, decals

- **Sprite selection**: rotation from viewer angle (8 rotations, or 16 with `A1A9`-style frames, mirrored rotations `A2A8`, single-rotation `A0`), `RF_ROLLSPRITE`, `RF_FLATSPRITE`, `RF_WALLSPRITE`, `RF_DONTFLIP`, `RF_XFLIP/YFLIP`, `RF_ROLLCENTER`, `RF_ABSMASKANGLE/PITCH`, `RF_SPRITEFLIP`, billboarding (`gl_billboard_mode`, `+FORCEYBILLBOARD`, `+FORCEXYBILLBOARD`), sprite offsets, `r_spriteadjust`/`gl_spriteclip` sprite-into-floor handling (`gl_spriteclip 1` smart clip), `+DONTINTERPOLATE`, `scale`, `SpriteOffset`, `WorldOffset`, `SpriteRotation`, `VisibleAngles/Pitch`, `RF_INVISIBLE`, `+NOSPRITESHADOW`, sprite shadows (`r_actorspriteshadow`), stealth, `fillcolor`.
- **Render styles** (all GZDoom `ERenderStyle` combos via a `(blend_op, src, dst, flags)` table): Normal, Fuzzy (`r_drawfuzz`/`gl_fuzztype` shader variants: shadow, smooth, swirly, translucent noise, software emulation, Jagged…), SoulTrans, OptFuzzy, Stencil, Translucent, Add, Shaded, TranslucentStencil, Shadow, Subtract, AddStencil, AddShaded, ColorBlend, ColorAdd, Multiply, plus `None`.
- **Psprites** (weapon): drawn in screen space after the scene with layer order, offsets, bob, `PSPF_*` flags, pivot/scale/rotation, vertex offsets, player translation, light from player's sector (+extralight), `r_drawplayersprites`, `fov`-independent placement as GZDoom (`r_psprite`... use GZDoom's 320×200-based psprite projection).
- **Models** (MODELDEF): MD2/MD3 vertex lerp between frames (interpolate on state duration), OBJ static, IQM GPU skinning (bones buffer, animations by frame), UE1; per-frame skins and surface skins, `Scale`, `Offset`, `ZOffset`, `AngleOffset/PitchOffset/RollOffset`, `Rotation-Speed/Vector/Center`, flags (`PITCHFROMMOMENTUM`, `IGNORETRANSLATION`, `INTERPOLATEDOUBLEDFRAMES`, `NOINTERPOLATION`, `USEACTORPITCH/ROLL`, `INHERITACTORPITCH/ROLL`, `ROTATING`, `DONTCULLBACKFACES`, `USEROTATIONCENTER`, `CORRECTPIXELSTRETCH`, `SCALEWEAPONFOV`), weapon models (psprite replaced by model), voxel replacement for sprites.
- **Voxels** (VOXELDEF/KVX): meshed at load into triangles (greedy meshing per slab), palette-indexed colours through the game palette, `Spin`, `PlacedSpin`, `DroppedSpin`, `AngleOffset`, `Scale`, `OverridePalette`.
- **Particles**: GPU instanced quads (round or square: `gl_particles_style`), additive/translucent, sizes, `SPF_*` flags (fullbright, relative pos/vel/accel, rolling, `SPF_NO_XY_BILLBOARD`…), textured particles.
- **Decals**: renderer-owned list per side (max `cl_maxdecals`, default 1024), created from `FxEvent::Decal` (DECALDEF lookup, translation, alpha, render style, animators: faders, stretchers, sliders, colour changers, combiners — animated in renderer using level time from SimView), clipped to wall parts; `SprayDecal` also. Decals are serialized in saves (requested by `room-game` via `Renderer::export_decals()` / `import_decals()`).

## 9. Post-processing

Ordered: SSAO (`gl_ssao`, half-res, bilateral blur) → bloom (`gl_bloom`, threshold + mip blur chain) → exposure/tonemap (`gl_tonemap`: none, Uncharted2, Hejl-Dawson, Reinhard, linear, **palette** (quantize to PLAYPAL via a 3D LUT — GZDoom has this), AgX, Khronos Neutral, **RoomSoft**) → lens/vignette (none by default) → palette flash blend (damage red, bonus gold, radiation suit green, `PowerupColor`, `Sector fade`, `Screen blend` from `A_SetBlend`; computed exactly like GZDoom's `V_AddBlend`/`P_AddPlayerBlend` logic into one RGBA blend) → FXAA (`gl_fxaa`) → colour correction (gamma, brightness, contrast, saturation; `vid_gamma`, `vid_contrast`, `vid_brightness`, `vid_saturation`) → dither (8-bit look option). Custom post-process shaders from GLDEFS `HardwareShader PostProcess` (§10).

**Implementation (SH0–SH1):** `room_render::PostChain` + `RenderSettings` run that order when any effect CVar is on (`needs_post()`). Defaults keep the classic path (post off). `r_quality 0..3` expands presets into individual CVars. Full catalogue and enable/disable list: [`docs/29-shaders.md`](29-shaders.md).

## 10. Custom shaders (GLDEFS)

GZDoom material shaders are GLSL fragments implementing `Material ProcessMaterial()` / `vec4 ProcessTexel()` / `vec4 ProcessLight(Material, vec4)` with access to uniforms (`timer`, user uniforms, textures) and helper functions. Post-process shaders implement `main()` with `InputTexture`, `TexCoord`, uniforms.

Plan:
1. Wrap the user GLSL snippet in a GLSL 450 template that defines GZDoom's documented built-ins (`getTexel`, `vTexCoord`, `uObjectColor`, `timer`, `normal`, …) and emits the result in a fixed interface.
2. Translate with **naga's GLSL frontend** → naga IR → WGSL backend (or directly use the IR with wgpu). This runs at load time; translation failures log a warning and fall back to the default material.
3. Uniforms declared in GLDEFS (`uniform float foo`) map to a per-material uniform block; updated from `Shader.SetUniform*` equivalents in UI modules (ZScript has `Shader.SetUniform1f(player, name, uniform, value)` for post shaders) → ui API `shader_set_uniform`.
4. Room-native alternative: materials may provide WGSL snippets directly (`HardwareShader … { wgsl "shaders/foo.wgsl" }`) — documented Room extension.

## 11. 2D canvas

The UI produces a `UiDrawList` (`16-ui.md`) of commands in virtual coordinates; the renderer maps them to the output framebuffer:
- Batched textured quads with per-vertex colour, translation index, render style, clip rect; `DTA_*` semantics resolved on the UI side into explicit rectangles.
- Shapes (`draw_shape`), lines, fills, dim (`DimAllowed`), blur behind menus (optional), burn/melt/crossfade screen wipes (`wipetype` 0–3: none, melt, burn, crossfade — melt must use the vanilla column offsets algorithm, burn GZDoom's fire effect).
- Text: glyphs from font atlases with GZDoom colour ranges (`TEXTCOLO` translation ranges per font).

## 12. Classic software renderer (C priority)

Separate crate `room-render-soft` implementing the vanilla column/span renderer (visplanes → drawsegs → masked) with ZDoom extensions (slopes via `R_DrawTiltedSpan`, 3D floors, portals, sprites clipping), outputting an 8-bit or 32-bit framebuffer uploaded as a texture each frame. Reference: GZDoom's `swrenderer` (the "zdoom" software renderer). Multi-threaded slices on native. Not required for 1.0.

## 13. Other features

- Screenshots (`screenshot` command): PNG of the final image (`png` crate), path per config.
- Offscreen API (`room_render::offscreen`) for tests and tools: render a `SimView` to an `Rgba8` image with a headless wgpu device (`wgpu` with `Backends::all()` + fallback adapter; in CI, use a software adapter: lavapipe/WARP/SwiftShader — or skip GPU tests when none is available).
- Precache: textures/sprites/models referenced by the level (MAPINFO precache lists + sector/side textures + spawned classes' sprites) uploaded during level load with a progress callback.
- Stereo/VR: out of scope.
- Automap: drawn by the UI layer using 2D line primitives (`16-ui.md §Automap`), including textured automap (`am_textured`) which needs flat polygons → expose `Renderer::draw_automap_textured(params)` that rasterizes sector triangles in 2D.

## 14. Performance targets

| Scene | Target (desktop mid-range GPU, 1440p) | Web (WebGPU, integrated GPU, 1080p) |
|---|---|---|
| Doom II MAP01 | ≥ 500 fps | ≥ 120 fps |
| Large community map (e.g. Sunlust MAP29-class, 20k+ lines) | ≥ 144 fps | ≥ 60 fps |
| nuts.wad (10k sprites) | ≥ 144 fps | ≥ 60 fps |
| 500 dynamic lights with shadowmaps | ≥ 120 fps | ≥ 30 fps |

CPU budget for scene building: ≤ 2 ms on large maps (native, 4 cores).

## 15. Tests

- Golden image tests (offscreen, software adapter where available): per feature synthetic maps (sloped plane, 3D floor, each render style, sky modes, portal types, decals, fuzz) with perceptual diff tolerance (e.g. `image-compare` SSIM ≥ 0.99, except fuzz/noise effects which are excluded or seeded).
- `ClassicPalette` mode: on a vanilla test map, compare against reference frames rendered by the C software renderer of Chocolate Doom (captured once, committed as PNG fixtures) — expect high but not perfect match.
- Clipper unit tests and BSP walk correctness (no missing walls) via debug view counters.
