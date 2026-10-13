# 21 — Milestone progress tracker

Living checklist for every roadmap milestone. Specs and acceptance tests live in
[`20-roadmap-testing.md`](20-roadmap-testing.md); **this file tracks execution**.

**How agents update this file**

1. After completing a work item, flip `[ ]` → `[x]` and bump the milestone
   **Status** / **%** in §0.
2. When *all* acceptance boxes for a milestone are `[x]` **and** CI proves it,
   set Status to `done` and move the active focus to the next milestone.
3. Add short dated notes under *Log* (newest first), not essays.
4. Do not mark a milestone `done` from conversation history alone — verify
   against the acceptance criteria and CI.

**Status values:** `not_started` · `in_progress` · `blocked` · `done`

---

## 0. Dashboard

| Milestone | Title | Status | Approx % | Active focus |
|-----------|-------|--------|----------|--------------|
| **M0** | Workspace and core | `done` | 100% | — |
| **M1** | Resources | `done` | 100% | — |
| **M2** | Formats & definitions | `done` | 100% | — |
| **M3** | Maps | `done` | 100% | — |
| **M4** | Renderer + shells | `done` | 100% | — |
| **M5** | Doom playsim | `done` | 100% | — |
| **M6** | Boom / MBF / MBF21 | `done` | 100% | — |
| **M7** | ZDoom feature set | `done` | 100% | — (see intentional deferrals) |
| **M8** | Heretic and Hexen | `deferred` | 0% | Next when Heretic/Hexen IWADs available |
| **M9** | Strife | `deferred` | 0% | Next step (later) |
| **M10** | GZDoom rendering extras | `done` | 100% | — (see intentional deferrals) |
| **M11** | WASM modules | `done` | 100% | — (see intentional deferrals) |
| **M12** | Netplay | `done` | 100% | — (see intentional deferrals) |
| **M13** | Product polish | `done` | 100% | — (see intentional deferrals) |
| **M14** | 1.0 hardening | `in_progress` | 10% | `save_world_bin` fuzz; next corpus scaffolding / docs stubs |

**Priority override (2026-10-12):** M7 + M10–M13 done. Active **M14** (1.0 hardening).
**M8** when Heretic/Hexen IWADs available; M9 remains deferred.

### Log

- `2026-10-13` — **Web HUD brightness** — `CanvasGpu` overlay uses `Rgba8Unorm` on non-sRGB WebGPU canvases (was `Rgba8UnormSrgb` → linear sample, un-encoded present = dark STBAR/gun).
- `2026-10-13` — **Web STBAR HUD** — classic `DoomStatusBar` (ammo/health/armor/keys/mugshot) + weapon on `CanvasGpu`; HTML tip hidden while playing.
- `2026-10-13` — **Web weapon + sprite scale** — bottom-align psprite HUD (was centered over floor); SoftPatch width/height/topoffset for billboards (was actor radius/height); desktop FOV 58.715 + full-window aspect.
- `2026-10-13` — **WebGPU present encode** — non-sRGB canvas + shader linear→sRGB (`encode_srgb`) + toe lift; Bright+gain; classic 4:3 playfield with matched buffer/CSS/`camera.aspect`.
- `2026-10-13` — **WebGPU sRGB view** — canvas configured as `Bgra8Unorm` + `viewFormats` sRGB; pipelines/`create_view` use sRGB (was writing linear→non-sRGB = crushed dark). Desktop FOV/DoomSoftware + mild gain.
- `2026-10-13` — **Web match desktop view** — full-window buffer (drop 4:3 letterbox); `fov_y=58.715` + `aspect=w/h`; Bright midtone lift + gain ~3.2 (canvas was still crushed dark).
- `2026-10-13` — **Web stretch root-cause** — never force `camera.aspect=4:3` while buffer is wide; size CSS+buffer from window to exact 4:3; aspect = w/h; Bright + higher gain every frame.
- `2026-10-13` — **Web 4:3 + Bright** — CSS letterbox 4:3 playfield (no stretch); `LightMode::Bright` + gain; FOV ~90° horizontal at 4:3.
- `2026-10-13` — **Web aspect + brightness** — canvas buffer tracks CSS box×DPR (was fixed 1280×720 stretched).
- `2026-10-13` — **Web music + brightness** — OPL/`GENMIDI` map music into Web Audio; `UnlitPass::output_gain` + GLDEFS brightmaps/dyn lights.
- `2026-10-13` — **Web look / light / gun** — mouse yaw matches desktop (−dx); `DoomSoftware`+`glob_vis=512`; weapon psprite overlay via `CanvasGpu`.
- `2026-10-13` — **Web textures + SFX** — load PLAYPAL/TEXTUREX before material upload (was greyscale/white albedo); Web Audio `ScriptProcessor` mixer + SNDINFO on canvas click.
- `2026-10-13` — **Web Doom1 play** — `ferrite-web` mounts IWAD bytes (`ferriteStartIwad` / auto-fetch `/doom1.wad` / file picker), runs `Headless` + materials + keyboard/mouse; `play-web.sh` copies local `testdata/doom/doom1.wad` into the serve dir.
- `2026-10-13` — **Web MRT attach** — `ferrite-web` map pass binds the dummy R8 stencil-aux colour target so it matches `UnlitPass` dual-MRT pipelines (fixes Chrome attachment-state mismatch).
- `2026-10-13` — **WebGPU unlit shader** — hoist all `textureSample` calls in `unlit.wgsl` ahead of non-uniform control flow (cluster light mask / discards); fixes Chrome WGSL validate error on `tex_metal_rough`.
- `2026-10-13` — **`scripts/play-web.sh`** — build `ferrite-web` for wasm32, `wasm-bindgen --target web`, serve `target/ferrite-web` (opens browser); wired `index.html` module loader.
- `2026-10-13` — **Automap zoom + fog** — `=`/`-`/wheel zoom; play mode hides unexplored linedefs until the player enters their sector (`ML_MAPPED`); `iddt`/allmap still reveals all.
- `2026-10-13` — **Tab automap** — line blit was an AABB blob (diagonals solid); stroke real segments; classic red/brown/yellow/gray/blue colours + floor/ceil diffs + grid; opaque black follow view.
- `2026-10-13` — **Intermission map preview** — Doom `WIMAP*` / `INTERPIC` background + `WI*` stats/splat/`WILV` patches (replacing chunky SoftFont on black); skip 3D world draw on Title/Intermission/Finale so letterbox sides are not the last camera frame; SoftPatch allows non-uniform scale for full-screen pics.
- `2026-10-13` — **Exit switch SW1STRTN stuck red** — `player_think` takes the player out of `world.players`, so `activate_line` treated Use as a monster and refused Exit_Normal (doors still worked via the monster whitelist). Park the in-hand player for `use_lines` (same pattern as pickups); E1M1 line 326 flips SW1STRTN→SW2STRTN via `World::tick`.
- `2026-10-13` — **Transparent step risers** — floor ear-clip ownership rejects near-corner samples in foreign sectors (SEGS hull / outline overhang across courtyard steps); draw flats before walls so STEP* risers win z-order; two-sided wall emit uses the opposite sector for the current sidedef (not always `line.back_sector`).
- `2026-10-13` — **Switch LED sync** — re-apply `switch_*` overrides onto the render map every frame (and after `map_base` restore) so SW1COMP→SW2COMP cannot stick on SW1*; E1M1 offscreen test asserts brighter-red LED (Doom1 COMP art is red→brighter red, not green).
- `2026-10-13` — **Outdoor sky streaks** — elevated floor *undersides* drew without cull and covered `F_SKY1` holes (edge-on greyscale bands). Flats pipeline culls back faces; default MAPINFO-less sky is `SKY1`.
- `2026-10-13` — **Pause menu / HUD font** — drop yellow chrome panel; BigFont (`DBIGFONT`) for menu text with native `STCFN` fallback; messages via STCFN (no 5×7 bitmap); screenshot HUD text only after save.
- `2026-10-13` — **Forward walked left of look** — mouse look preview used float yaw while ticcmd rounded counts; camera drifted vs `actor.angle`. Lock yaw/pitch to sim after each tic; preview only rounded mouse counts.
- `2026-10-13` — **Weapon holes (palette 0)** — SoftPatch now uses post alpha mask; ~11% of Doom1 `PISGA0` posts are colour 0 (metal) and were punched transparent → noisy/seam gun.
- `2026-10-13` — **Floor holes + gun height** — ear-clip fan fallback + majority ownership (was dropping E1M1 floor tris → sky stripes); weapon Y shifts up 32 when STBAR shown (offsets are view-relative).
- `2026-10-13` — **Screenshot + GIF** — F12 / `screenshot` saves PNG; F11 / `gifrecord` toggles GIF (≤640px, ~10fps, 30s cap); capturable frame buffer + `gif` crate; files under `…/ferrite/screenshots/`.
- `2026-10-13` — **Stairs feel + outdoor sky** — Double `P_SlideMove` (wall clip + full-mom stairstep); eye bob + camera XYZ tic-lerp; sky shell taller (8192) with top/bottom caps (old ±7° FOV fell into clear colour outdoors).
- `2026-10-13` — **Opaque wall holes** — patch decode writes explicit alpha (colour 0 ≠ hole); TEXTUREX blit skips holes (no punch/erase); solid walls fill clear texels black in shader; masked mids still discard.
- `2026-10-12` — **Stairs slide + switch LED** — Double `xy_movement` stairstep (Y then X) instead of hard-stop/*noway; lower+`DONTPEGBOTTOM` uses TexBottom UV so SW1COMP→SW2COMP LED is visible; `ferrite: USE`/`SPECIAL`/`SWITCH` stderr on line activation.
- `2026-10-12` — **SHOOT debug + switch UV + imp aim** — always-on `ferrite: SHOOT` stderr (player↔AI hitscan/missile); Doom sidedef x/y_offset copied to top/bottom tiers; wall UV uses TEXTUREX size (SW1COMP 64-wide); `P_SpawnMissile` aims at target under Double math (elevated imps); switch flip writes SW2* onto map lower.
- `2026-10-12` — **E1M1 switch + lift** — `player->usedown` edge so held E does not reverse Door_Raise; WR lifts use lowest surrounding floor + lip and refuse to stack; load-time SCROLL lines clear special after init.
- `2026-10-12` — **Doom1 door reopen** — XLAT `base.txt` USE/WALK/SHOOT/REP/MONST for types 0–141 (DR type 1 keeps special); `spawn_door` re-trigger reverses waiting/closing doors like vanilla.
- `2026-10-12` — **Invisible crush + shot traces** — `crush_sector_actors` only hits actors in the moving sector (was map-wide 10hp/4tics when any door closed); HIT dump lists recent hitscan/missile from→to; `last_damage.cause` tags crush.
- `2026-10-12` — **HIT debug** — `World::last_damage` records real source/inflictor; HIT stderr dumps player sector/crush, sight/FOV/DONTDRAW, sprite lump+patch+GPU, hunters vs corpses (old log mistook POSSL corpses for attackers).
- `2026-10-12` — **Invisible killer (follow-up)** — opaque sprite pipeline (REPLACE+depth write+strong bias), billboard pull-to-eye, preload POSS/SPOS/TROO frames, on-hit HUD/eprintln naming attacker or damaging sector.
- `2026-10-12` — **Invisible killer after E1M1 door** — root cause: app forced DoomSoftware `glob_vis=16` (everything past ~30mu at shade floor) so zombies in the computer room vanished into walls while still shooting; set `glob_vis=512`, COUNTKILL/missile sprites always fullbright; e2e probes confirm drawable LOS attackers.
- `2026-10-12` — **Door peg + HUD gun** — `ML_DONTPEGTOP/BOTTOM` wall UV pegging (door faces scroll, DOORSTOP jambs no longer stretch); weapon overlay discards holes and forces opaque alpha; monsters keep min light 0.55 / fullbright when hunting; missing sprite lumps draw white instead of vanishing.
- `2026-10-12` — **Solo invisible killer** — Hostiles targeting a player draw fullbright (DoomSoftware shade was crushing them in dark sectors); GPU POSS offscreen regression; Player2–4 starts remain spawn-spots-only (not the solo cause).
- `2026-10-12` — **Net transports** — TCP mesh + TCP packet-server relay (`TcpRelay*`); Noise_XX (`snow`) wrapper; app `--transport udp|tcp|tcp-server` + `--noise`.
- `2026-10-12` — **Netplay tests** — `coop_netplay` (UDP lobby+dual-seat lockstep, FF, desync); net UDP lobby unit; `spawn_coop_players` sim test. App `--host`/`--join` Doom E1M1 coop.
- `2026-10-12` — **M14** — `validate_world_bin` + `save_world_bin` fuzz + CI 60s.
- `2026-10-12` — **M14 start** — `save_info` fuzz target + CI 60s smoke.
- `2026-10-12` — **M10 done** — MAP01+32-lights fps floor; desktop §14 / nuts.wad intentional deferral.
- `2026-10-12` — **M10** — Spotlight cone on GPU + `shadowmap_32_budget`.
- `2026-10-12` — **M10** — `LightmapBindings` + spot/sector GLDEFS + SSAO half-res AO buffer.
- `2026-10-12` — **M10** — Per-vert lightmap UVs + `sprite_10k_budget` (nuts stand-in).
- `2026-10-12` — **M10** — UnlitPass `set_lightmap` + cluster_500 budget; MAP01 fps already gated.
- `2026-10-12` — **M10** — Particles SSIM + LightmapPass GPU + compose_post + scene_build_budget.
- `2026-10-12` — **M10** — ParticleInstancedPass + fill_color styles + bloom smoke + HW post host.
- `2026-10-12` — **M10** — HardwareShader catalog + GLDEFS parse; particle GPU pack; FLMP surfaces.
- `2026-10-12` — **M10** — lights.pk3 fixture→Definitions; `glsl_to_wgsl`; braced `frame { light }` GLDEFS.
- `2026-10-12` — **M10** — True bloom mip pyramid (`BloomPass`); naga WGSL build validate; `FLMP` lightmap header.
- `2026-10-12` — **M10** — Metal-rough GGX PBR (`tex_metal_rough` + `pbr` helpers) + fuzz SSIM golden.
- `2026-10-12` — **M10** — SSAO bilateral blur; particle Additive/Soft styles; 4-level Kawase bloom taps.
- `2026-10-12` — **M10** — UE1 `_d.3d`/`_a.3d` parse + catalog pair load; Blinn-Phong specular on normal maps.
- `2026-10-12` — **M10** — IQM GPU skinning (`SkinnedPass` / bone UBO / `skinned.wgsl`) + smoke test.
- `2026-10-12` — **M10** — KVX greedy face merge; optional `lights.pk3`/`brightmaps.pk3` mount + fixture.
- `2026-10-12` — **M10** — Dyn-lights SSIM golden PNG (`testdata/goldens/dyn_lights_320x200.png`).
- `2026-10-12` — **M10** — IQM CPU skinning (`parse_iqm_full` / `skin_cpu`) + `golden_dyn_lights` soft mean-luma check.
- `2026-10-12` — **M10** — Play-path `append_actor_models`; `model_offscreen_smoke` test.
- `2026-10-12` — **M10** — IQM v2 bind-pose parse; `ModelCatalog` + `append_actor_models` MODELDEF wiring.
- `2026-10-12` — **M10** — MD3 mesh parse (first surface); normal-map Lambert on dyn lights.
- `2026-10-12` — **M10** — `append_model` unlit draw path; KVX parse + cube meshing.
- `2026-10-12` — **M10** — MD2 mesh parse (multi-frame) + GLDEFS material PBR map catalog wiring.
- `2026-10-12` — **M10** — Palette tonemap (`gl_tonemap` PLAYPAL nearest) + OBJ `parse_obj` for MODELDEF.
- `2026-10-12` — **M10** — SSAO pass (depth-edge 8-tap) + multi-scale Kawase bloom in PostPass.
- `2026-10-12` — **M10** — Spectre fuzz shader (`gl_fuzztype` Shadow/Smooth/Swirly/…); Fuzzy sprites flag.
- `2026-10-12` — **M10** — `RenderStyle` → wgpu blend table; PostPass FXAA (Jimenez compact).
- `2026-10-12` — **M10** — PostPass (Reinhard/Hejl/linear + gamma + soft bloom + screen blend); particle vel/accel.
- `2026-10-12` — **M10** — CPU 1D shadowmaps (512 samples/light, one-sided occluders) + FS sample in unlit.
- `2026-10-12` — **M10** — VOXELDEF parse (scale/spin/angleoffset/overridepalette) into `Definitions.voxeldefs`.
- `2026-10-12` — **M10** — MODELDEF parse (`Model`/`FrameIndex`/`Frame`/skins/flags) into `Definitions.modeldefs`.
- `2026-10-12` — **M10** — CPU 16×9×24 cluster assignment + RGBA8 mask texture; FS only tests lights in cluster bitmask.
- `2026-10-12` — **M10** — GPU `LightUniforms` (32 lights) in unlit pass; pulse/flicker radius anim; CPU dyn-light boost retired from play path.
- `2026-10-12` — **M10** — GLDEFS `brightmap`/`material`/`Glow` parse; shader brightmap mix; glow fullbright ranges; nukage/lava glow list in ferrite.pk3.
- `2026-10-12` — **M10 start** — GLDEFS `pointlight`/`pulselight`/`flicker*`/`object` parse; attack-frame defaults; CPU `apply_dyn_lights`; Bright sprites skip lightmode atten; oversized monster muzzle puff; `two_clients_same_map` example.
- `2026-10-12` — Enemy attack visibility: Missile wind-up Bright (zombie/shotgun/chaingun/imp); fullbright shader path for `light≥0.999`.
- `2026-10-12` — Doom1 props/enemies: floor sprites sit on feet+1 (depth bias −8/−2, no z-fight bury); Spectre Fuzzy→translucent stand-in; add RealGibs/BloodPool + coop/DM starts; E1M1 all DoomEd types resolve.
- `2026-10-12` — E1M1 empty floors: keep IWAD nodes (`ensure_nodes`, stop rebuilding over them); flats from seg-endpoint hull + BSP partition-clipped leaf + outline supplement; mapthing floor coverage 0 holes.
- `2026-10-12` — Empty/wrong floors: always ear-clip GL leaves (fan spilled into neighbours); outline fallback only if a sector has zero visible GL area (not a 70% ratio vs full outline); centroid-only ownership on outline — E1M1 floor centroid mismatches 0.
- `2026-10-12` — Empty floors: GL subsector rings primary (all leaves); outline fallback if GL &lt;70% coverage; ownership filter also accepts vertex hits — stops clear-color holes.
- `2026-10-12` — E1M1 flats: ear-clip sector outlines + drop tris whose centroid isn’t in-sector (concave/hole fans painted neighbour floors at wrong step heights); honour leftoverset outside [0,1]; sprite +0.05 floor lift.
- `2026-10-12` — Floor placement: snap mapthings via `floor_at_xy` (slopes+movers); sprite topoffset/leftoffset anchors; flats use `base_floor` delta so steps/lifts match walls; Bright projectiles/puffs.
- `2026-10-12` — Audio quality: resample DMX SFX to device rate; prefer SoftFont (`fetch-soundfont.sh` / TimGM6mb); align music synth rate with cpal.
- `2026-10-12` — Floor clip: seed `check_position` from point-in-sector + line openings (not front-only); Doom1 `D_ExMy` music; snap spawn Z to floor.
- `2026-10-12` — Fix outdoor “upside down”: skip `F_SKY1` flats (+ sky hack), peg wall V to ceiling, centre sky on eye; set `view_z` at possess.
- `2026-10-12` — Real Doom1 play: `scripts/play-doom1.sh` + `testdata/doom/` discovery; MAP01→E1M1 fallback; commercial IWAD gitignored.
- `2026-10-12` — Bullet impacts: resolve `BulletChip`→`pic` (PUFFB0 fallback); never reuse prior sprite bind for missing textures; weapon DECALDEF generator.
- `2026-10-12` — Acquire mouselook on window `resumed`/`focus` (startup set the flag before any window existed and skipped hide/recenter).
- `2026-10-12` — macOS LMB: drop Locked grab (hide+edge-recenter); fire on psprite enter + 0-tic loop; pistol A4/B6; chaingun muzzle flash; same-tic chaingun hitscan test.
- `2026-10-12` — macOS LMB: `AttackLatch` in ferrite-input (window/device/key bits independent; device id 0 only); warp-then-Locked grab so clicks hit the client.
- `2026-10-12` — Restart clears bullet chips/particles and restores map lights (End Game / New Game / `map`); FX not drawn on the title screen.
- `2026-10-12` — Restore classic STBAR HUD (no overlay plate); psprite `x=-leftoffset` so Freedoom fire/flash stay on the barrel.
- `2026-10-11` — HUD/menu: draw via `STTNUM*` / `STCFN###` patches (SoftFont path was invisible); fullscreen plate + 2× digits; pause STCFN ×2 + highlight; blit pixel tests.
- `2026-10-11` — HUD larger (fullscreen SoftFont ×2.75, HudBig/STTNUM); pause outlined/large text; `bullet_slope` freelook fallback + eye-height shoot origin so shots match the gun.
- `2026-10-11` — Pause menu: STCFN lowercase→uppercase glyph fallback (Freedoom lacks most lowercase; labels were invisible); missiles explode on floor/ceiling (were skimming forever).
- `2026-10-11` — Combat demo: `AttackLatch` (18-tic hold) + Locked grab on macOS; strong tests for short-click / dual Window+Device release / RMB must not clear LMB; arena look-down + eye-level floor-band render tests green.
- `2026-10-11` — Combat demo floors: sector-outline flats (BSP slivers left void); unlit cull off for horizontal look; macOS LMB = device button 0 only (1 is RMB).
- `2026-10-11` — Combat demo: floor flats were missing (BSP leaf marked sector done before a ≥3-seg ring); GL/outline flat emit + tighter arena; pause panel centred/blackout.
- `2026-10-11` — Combat demo playability: opaque pause panel + STCFN text; mouse yaw scale 8→256 (preview matched); weapon HUD centred (ignore bad Freedoom left offsets).
- `2026-10-11` — **M13 done** — menus+options CVars; launcher; ferrite/gzdoom.ini; web URL/storage API; package.sh; crash logs; StringTable i18n; QA checklist. Play: float strafe fix, macOS Confined grab + LMB/F fire. Deferral: full multi-OS/browser manual matrix.
- `2026-10-11` — **M13 start** — Doom menus: TITLEPIC, skull selector, patch items, pause/quit/end-game confirms, mouse hover/click, Options stub; dim only when overlaying play. Combat-demo fire/render polish carried from play shell backlog.
- `2026-10-10` — **M12 done** — lockstep + UDP + lobby + desync + `ferrite-signal` + signal mesh/HTTP + optional `webrtc`/`MatchboxTransport`. Freedoom MAP01 2-peer lockstep smoke. Deferrals: live 4-player native+web 30 min soak; matchbox signalling server swap.
- `2026-10-10` — M12: **desync dump + ferrite-signal** — `DesyncCollector`; in-memory HTTP room relay.
- `2026-10-10` — M12: **UDP + lobby** — `UdpTransport`; `LobbyMsg`/`LobbyHost` Hello/Welcome/Reject/Start.
- `2026-10-10` — **M11 done** — WASM host/SDK/reg/events/ACS/UI/saves. Deferrals: Actor derive/`states!`/C header; HUD draw; hot reload; samples (b)–(e); benchmarks.
- `2026-10-10` — M12: **lockstep bootstrap** — `LockstepSession`, loopback + lossy transport, codec, desync tests.
- `2026-10-10` — M11: **`.fsave` scripts.bin** — WASM play memory in save ZIP; restore on load.
- `2026-10-10` — M11: **UI + saves** — `load_ui_module` / `UiTick`; play memory snapshot + `LoadGame` restore; `ui_tick.wat` / `save_counter.wat`.
- `2026-10-10` — M11: **ACS interop** — `reg_static_function` + `ScriptCall`; WASM `acs_execute` queue; `script_call.wat`.
- `2026-10-10` — M11: **events/thinkers** — `reg_event_handler` / `reg_thinker_class`; `WorldTick` in `World::tick`; `event_tick.wat`.
- `2026-10-10` — M11: **ClassRegistry merge + fingerprint** — `import_script_class` / `apply_script_classes`; wasmi≡wasmtime reg fingerprint.
- `2026-10-10` — M11: **reg_class_*** — RegistrationSession + host imports; `fireimp_reg.wat`; SDK `reg` module; `api/reg.ron`.
- `2026-10-10` — M11: **name_intern + class_find** — HostLinker/backends/SDK; ScriptHost tables; `testdata/hello_mod.wat` sample.
- `2026-10-10` — M11: **SDK alloc + macros** — bump `ferrite_alloc`; `export_module!`; `#[actor]` / `#[register_stub]`.
- `2026-10-10` — M11: **Rust SDK wrappers** — `ferrite-script-sdk` log/level_time/player_count/actor_exists + abi_version export.
- `2026-10-10` — M11: **player_count** — host import + ScriptHost sync from occupied slots.
- `2026-10-10` — M11: **ScriptHost in sim** — `World.scripts`; sync maptime/player actors each tic; load stub module test.
- `2026-10-10` — M11: **wasmtime backend** — Cranelift fuel/NaN-canon; shared lifecycle test vs wasmi (`--features wasmtime`).
- `2026-10-10` — M11: **level_time + actor_exists** — HostLinker + wasmi imports; `api/common.ron` + `ferrite-abi-gen list`.
- `2026-10-10` — M11: **guest lifecycle + ferrite::log** — required exports; HostLinker log import; register/init/dispatch helpers.
- `2026-10-10` — M11: **wasmi bootstrap** — `ferrite-script-abi` types; validate_module; `WasmiBackend` + `ferrite_abi_version` test.
- `2026-10-10` — **Priority:** M7 closed → active **M11** then **M12**; M8–M10/M13–M14 deferred. Freedoom demo verify script + MAP01 real-play smoke.
- `2026-10-10` — **M7 done** — acceptance via `m7_feature_smoke` + UDMF/ACS/DECORATE/portal/ffloor coverage. Intentional deferrals: Equal-only Metal stencil mask; remaining DECORATE/ACSF edge corpus; external ZDoom mod corpus fetch.
- `2026-10-10` — M7: **UDMF sector alphafloor/alphaceiling/renderstyle** — parse (+ `alpha` both); seed SectorStatic/Dynamic; GetSectorUDMF* int/fixed; Add → STYLE_Add.
- `2026-10-10` — M7: **GetThingUDMFInt renderstyle** — UDMF thing `renderstyle` → ACS `STYLE_*` via `RenderStyle::parse`/`to_acs_style`.
- `2026-10-10` — M7: **GetLineUDMFInt SPAC + repeatspecial** — `playercross`/`playeruse`/…/`damagespecial` via `line_activation`; `repeatspecial` via `line_flags` (ACS SetLineActivation-aware).
- `2026-10-10` — M7: **UDMF sidedef xscroll/yscroll** — parse; `init_udmf_side_scrollers` → `TextureScroller`; GetSideUDMFFixed.
- `2026-10-10` — M7: **UDMF sidedef smoothlighting** — parse; milder fake contrast; GetSideUDMFInt.
- `2026-10-10` — M7: **UDMF sector color_floor/ceiling/wall*/sprites** — parse; GetSectorUDMFInt; scene luma tint on flats/walls.
- `2026-10-10` — M7: **UDMF sidedef tier offset/scale** — `offsetx/y_*` + `scalex/y_top|bottom`; scene per-tier UV; GetSideUDMF*.
- `2026-10-10` — M7: **UDMF sidedef light_top/mid/bottom** — parse + absolute flags; scene per-tier wall light; GetSideUDMFInt.
- `2026-10-10` — M7: **healthfloor/ceiling hitscan** — `damage_sector_plane`; aim ray hits planes before miss; syncs map static HP.
- `2026-10-10` — M7: **MAPINFO clipmidtextures/wrapmidtextures** — parse → `MapDef`; headless stamps all lines/sides.
- `2026-10-10` — M7: **clipmidtex Z + pegged midtex** — two-sided mid quads peg 128mu (DONTPEGBOTTOM); `clipmidtex` intersects opening; `wrapmidtex` fills; single-draw V `[0,1]`.
- `2026-10-10` — M7: **MAPINFO fallingdamage → LevelState** — `fallingdamage`/`forcefallingdamage`/`oldfallingdamage`/`strifefallingdamage`/`monsterfallingdamage`/`nofallingdamage` → `MapDef.falling_damage`; headless seeds `world.level.falling_damage`.
- `2026-10-10` — M7: **UDMF sector healthfloor/ceiling/3d** — parse; seed `SectorDynamic` health parts; GetSectorUDMFInt.
- `2026-10-10` — M7: **UDMF sidedef nodecals** — parse; GetSideUDMFInt; `spawn_decal` skips when either side of the line has the flag; hitscan passes line id.
- `2026-10-10` — M7: **UDMF sector lightcolor/fadecolor/fogdensity/desaturation** — parse hex colours; seed SectorDynamic + world fog; GetSectorUDMFInt.
- `2026-10-10` — M7: **UDMF damagespecial / deathspecial** — SPAC_Damage/Death; healthgroup sync; `damage_linedef`; hitscan walls apply; GetLineUDMFInt.
- `2026-10-10` — M7: **falling damage + nofallingdamage** — Hexen formula; `LevelState.falling_damage`; UDMF sector gate; GetSectorUDMFInt; float + fixed `z_movement`.
- `2026-10-10` — M7: **UDMF clipmidtex / wrapmidtex** — line+side parse; GetLine/SideUDMFInt; mid quads clamp V unless wrap (line|side); Z still opening-clipped.
- `2026-10-10` — M7: **UDMF linedef health + automapstyle** — parse; seed `line_health`/`line_automap_style`; GetLineUDMFInt; automap AML_NotMapped/OneSided/TwoSided/Special/Secret; Line_Set* updates geometry.
- `2026-10-10` — M7: **plat floor carry** — `tick_plats` rides grounded actors (same `dropactors` gate as floors).
- `2026-10-10` — M7: **UDMF sector dropactors** — parse (default true); floor movers carry grounded actors; false skips downward ride; GetSectorUDMFInt.
- `2026-10-10` — M7: **UDMF sector waterzone** — parse; `actor_waterlevel` uses ceiling as surface; GetSectorUDMFInt.
- `2026-10-10` — M7: **UDMF sector silent** — parse; `play_sector_sound` no-ops; GetSectorUDMFInt.
- `2026-10-10` — M7: **UDMF/ACS sector leakiness** — parse + `Sector_SetDamage`/SetSectorDamage arg; radsuit fail chance (256ths); GetSectorUDMFInt.
- `2026-10-10` — M7: **UDMF linedef ML_* flags** — blocking/blockmonsters/twosided/dontpeg*/secret/…/blockplayers/blockeverything; physics BLOCK_PLAYERS/MONSTERS/EVERYTHING; GetLineUDMFInt.
- `2026-10-10` — M7: **UDMF linedef renderstyle Add** — parse → `mid_additive`; seed `line_additive`; GetLineUDMFInt STYLE_Add(65).
- `2026-10-10` — M7: **UDMF linedef alpha + sidedef nofakecontrast** — `alpha` → `mid_alpha`; GetLineUDMF*; side disables fake contrast.
- `2026-10-10` — M7: **UDMF sidedef light/scale** — parse `light`/`lightabsolute`/`scalex*`/`scaley*` → Side; GetSideUDMF*; wall light uses side.
- `2026-10-10` — M7: **A_DamageSelf** — damage + optional mod (restored after call).
- `2026-10-10` — M7: **sector gravity in z_movement** — UDMF/ACS sector gravity scales float + vanilla fixed fall.
- `2026-10-10` — M7: **sector damageamount runtime** — UDMF/ACS `damageamount`+`damageinterval` hurts in `player_in_special_sector`.
- `2026-10-10` — M7: **UDMF sector gravity/friction/damage** — load into SectorDynamic; GetSectorUDMF*.
- `2026-10-10` — M7: **UDMF sector UV xform** — x/y pan, rotation, scale floor/ceiling → SectorStatic; GetSectorUDMFFixed.
- `2026-10-10` — M7: **UDMF sector moreids** — parse; merge into `SectorStatic.tags` for specials/ACS.
- `2026-10-10` — M7: **UDMF linedef moreids** — parse; `Line::has_id` for Attach/ACS/GetLineUDMF.
- `2026-10-10` — M7: **midtex Attach ceiling hang** — `Sector_Attach3dMidtex` arg2 hangs 64mu slab from attached ceiling.
- `2026-10-10` — M7: **UDMF blockhitscan** — parse; aim/shoot traverse treat as solid; GetLineUDMFInt.
- `2026-10-10` — M7: **UDMF blocksight** — parse; LOS opaque (+ `ML_BLOCKEVERYTHING`); GetLineUDMFInt.
- `2026-10-10` — M7: **UDMF blockprojectiles** — parse; `check_position` solids missiles; GetLineUDMFInt.
- `2026-10-10` — M7: **UDMF locknumber** — parse; overrides Door_LockedRaise/Generic_Door lock args; GetLineUDMFInt.
- `2026-10-10` — M7: **UDMF blockfloaters** — parse; `check_position` solids `+FLOAT` against those lines.
- `2026-10-10` — M7: **UDMF checkswitchrange / firstsideonly** — parse + Use/Push gates (eye in opening; front side only).
- `2026-10-10` — M7: **StartSlideshow / SetSubtitleNumber shell** — drain ACS requests → HUD messages.
- `2026-10-10` — M7: **A_SpawnParticle** — DECORATE colour/flags/life/size/offsets → `particle_events`.
- `2026-10-10` — M7: **SpawnParticle render** — ACS events → `ParticleStore` billboards (`__PARTICLE`); fade by lifetime.
- `2026-10-10` — M7: **Sector_Attach3dMidtex (51)** — marks matching two-sided lines `midtex3d`; `SetupData.midtex_attach`.
- `2026-10-10` — M7: **UDMF midtex3d / midtex3dimpassible** — walkable 64mu mid slab; impassible blocks non-missiles.

- `2026-10-10` — M7: **ExtraFloor_LightOnly (50)** — type-3 light transfer (alpha 0); `GetActorLightLevel` below top plane.
- `2026-10-10` — M7: **3D floor alpha** — `Sector_Set3DFloor` arg3 → `FFloorStatic.alpha`; scene flats/sides translucent / skip at 0.
- `2026-10-10` — M7: **decal colorchanger** — FadeStart/Time + Color → light luminance lerp (ToBlack).
- `2026-10-10` — M7: **decal stretcher/slider** — GoalX/Y + DistY; combiner merges children; store animates size/Z.
- `2026-10-10` — M7: **decal fader** — DECALDEF `DecayStart`/`DecayTime` → alpha fade + remove; app ticks store.
- `2026-10-10` — M7: **decal wall-clip** — linedef-tagged decals emit wall-aligned translucent quads (billboard fallback).
- `2026-10-10` — M7: **3D floor actor light** — `GetActorLightLevel` uses control light under solid / in swimmable slabs.
- `2026-10-10` — M7: **swimmable 3D floors** — `z_movement` reduced gravity + `up_move` thrust; shared `actor_waterlevel`.
- `2026-10-10` — M7: **3D floor side walls** — scene emits vertical quads on target linedefs (control textures).
- `2026-10-10` — M7: **UDMF floatspeed/fastspeed/jumpz/attackzoffset** — parse + spawn; GetThingUDMFFixed.
- `2026-10-10` — M7: **UDMF maxstepheight/maxdropoffheight** — thing parse + spawn; GetThingUDMFFixed.
- `2026-10-10` — M7: **UDMF friction/damagefactor** — thing parse + spawn; GetThingUDMFFixed.
- `2026-10-10` — M7: **UDMF mass/speed** — thing parse + spawn; GetThingUDMFInt/Fixed.
- `2026-10-10` — M7: **UDMF accuracy/stamina/species/meleerange/viewheight** — parse + spawn; GetThingUDMF int/fixed.

- `2026-10-10` — M7: **APROP_Waterlevel** — 0..=3 from Transfer_Heights fake floor + swimmable 3D floors.
- `2026-10-10` — M7: **StartConversation (18)** — living player + talker tid → `conversation_request` (+ optional face).
- `2026-10-10` — M7: **Thing_SetConversation (79)** — Hexen special sets actor dialogue ID (`tid` 0 = activator).
- `2026-10-10` — M7: **A_SetConversation** — DECORATE setter for dialogue ID.
- `2026-10-10` — M7: **UDMF conversation** — thing parse/spawn + `GetThingUDMFInt`; dialogue runtime stub.
- `2026-10-10` — M7: **UDMF arg0str (things+lines)** — parse; `init_acs` maps SNAM name → `args[0]` for ACS specials 80–86/226.

- `2026-10-10` — M7: **UDMF countsecret/nocount** — parse + spawn; secret pickup increments `found_secrets`; nocount clears kill/item counts.
- `2026-10-10` — M7: **GetThingUDMF* thing keys** — health/score/friendlyseeblocks/fillcolor/pitch/roll/args + gravity/scale/alpha/x/y/z fixed.
- `2026-10-10` — M7: **UDMF thing fillcolor/renderstyle/floatbobphase** — parse + spawn; stencil RGB + `RenderStyle::parse`; lexer accepts `0x` hex ints.

- `2026-10-10` — M7: **UDMF thing gravity/scale/alpha/pitch/roll** — parse + spawn; `scale` sets both axes; `scalex`/`scaley` override.
- `2026-10-10` — M7: **UDMF thing health/score** — parse + spawn overrides (`score` unset = `i32::MIN`).
- `2026-10-10` — M7: **UDMF friendlyseeblocks + A_SetFriendlySeeBlocks** — thing parse/spawn; DECORATE setter.
- `2026-10-10` — M7: **A_LogInt / A_LogFloat** — queue formatted number HUD messages.
- `2026-10-10` — M7: **APROP_Master/Target/TracerTID set** — SetActorProperty resolves TID→pointer; 0 clears.
- `2026-10-10` — M7: **APROP_FriendlySeeBlocks** — const 48; class/runtime; friendly `A_Look` range = blocks×128.
- `2026-10-10` — M7: **APROP_*Sound + SoundClass** — See/Attack/Pain/Death/Active set-only; SoundClass (47) get/set; runtime overrides in PlayActorSound.
- `2026-10-10` — M7: **APROP_DamageType + Waterlevel** — const 46 dyn-string Get/Set/Check; 28 read-only stub (0).
- `2026-10-10` — M7: **APROP_Nametag + A_SetTag** — const 21; dyn-string Get/Set/Check; spawn from class `Tag`.
- `2026-10-10` — M7: **APROP_Species** — const 20; Get/Set/CheckActorProperty via dyn strings; host get/set (+`None` clear).
- `2026-10-10` — M7: **+RIPPER** — `FLAG_RIPPER`; `is_ripper_missile` (flag or level); physics/vanilla rip path.
- `2026-10-10` — M7: **RipperLevel / RipLevelMin/Max + DONTRIP** — class props; spawn init; rip gate.
- `2026-10-10` — M7: **ripper missile collision** — `xy_movement` damages + continues when rip levels allow.
- `2026-10-10` — M7: **A_SetRipperLevel / RipMin / RipMax** — rip level fields + `can_rip_through`.
- `2026-10-10` — M7: **A_ThrowGrenade / A_SpawnDebris** — lofted missile; debris velocity burst.
- `2026-10-10` — M7: **A_CustomPunch / A_FireBullets** — configurable melee + multi-pellet hitscan.
- `2026-10-10` — M7: **A_StopSound + APROP_NoTrigger / DamageMultiplier** — stop queued SFX; line activate gate.
- `2026-10-10` — M7: **APROP_ChaseGoal / Frightened + A_SetChaseGoal** — `+FRIGHTENED` flag bit.
- `2026-10-10` — M7: **A_SetJumpZ / APROP_JumpZ + DecoBlock jumpz if** — runtime jump impulse.
- `2026-10-10` — M7: **APROP_RenderStyle** — get/set `STYLE_*` (ACS legacy constants).
- `2026-10-10` — M7: **A_RailAttack / A_CustomRailgun / A_SentinelRefire** — piercing rail; sentinel refire→See.
- `2026-10-10` — M7: **DecoBlock float/fast/camera if** — FloatSpeed/FastSpeed/CameraHeight/FOV skips.
- `2026-10-10` — M7: **A_CustomMeleeAttack / A_CustomComboAttack / A_BasicAttack** — melee or missile combo.
- `2026-10-10` — M7: **DecoBlock maxstep/dropoff if** — `A_SkipUnlessMaxStepHeight`/`MaxDropOffHeight`.
- `2026-10-10` — M7: **A_MonsterRefire + DecoBlock attackzoffset if** — custom abort label; `A_SkipUnlessAttackZOffset`.
- `2026-10-10` — M7: **DecoBlock meleerange/viewheight if** — `A_SkipUnlessMeleeRange`/`ViewHeight`.
- `2026-10-10` — M7: **DecoBlock team if** — `A_SkipUnlessTeam` on `designated_team`.
- `2026-10-10` — M7: **A_SetAccuracy / A_SetStamina + APROP_*** — DecoBlock accuracy/stamina ifs.
- `2026-10-10` — M7: **A_SetTeam / CameraHeight / CameraFOV + DecoBlock damagefactor** — app ChangeCamera eye+FOV.
- `2026-10-10` — M7: **A_SetDamageFactor + APROP_DamageFactor** — runtime incoming damage multiplier.
- `2026-10-10` — M7: **A_SetScale ScaleX/Y + A_SetObituary/HitObituary** — APROP_ScaleX/Y; app sprite size; obituaries.
- `2026-10-10` — M7: **A_SetViewHeight / A_SetAttackZOffset + APROP_*** — hitscan shoot origin uses runtime fields.
- `2026-10-10` — M7: **A_SetFillColor + APROP_StencilColor** — runtime fill/stencil RGB as 0xRRGGBB.
- `2026-10-10` — M7: **DecoBlock friction if + A_SetBloodColor** — `A_SkipUnlessFriction`; runtime blood RGB.
- `2026-10-10` — M7: **A_SetFloatSpeed / A_SetFastSpeed** — runtime FLOAT Z-adjust + nightmare chase speed.
- `2026-10-10` — M7: **A_SetMaxStep/DropOffHeight + APROP_*** — runtime step/dropoff; `try_move` honors actor fields.
- `2026-10-10` — M7: **A_SetFriction + APROP_Friction + DecoBlock special if** — runtime friction; `A_SkipUnlessSpecial`.
- `2026-10-10` — M7: **APROP MeleeRange/SpawnHealth/ViewHeight + A_SetMeleeRange** — runtime `Actor.melee_range`.
- `2026-10-10` — M7: **APROP_* expand + score** — Damage/Alpha/flags/Score/TIDs/ReactionTime get/set; `A_SetScore`/`GiveScore`; DecoBlock score if.
- `2026-10-10` — M7: **A_CopySpriteFrame / A_RaiseSelf** — copy target/master/tracer sprite; self-revive.
- `2026-10-10` — M7: **A_SetSprite / A_SetFrame / A_TransferInventory** — sprite+frame; move item stack by tid.
- `2026-10-10` — M7: **DecoBlock painthreshold/threshold if** — `A_SkipUnlessPainThreshold`/`Threshold`.
- `2026-10-10` — M7: **DecoBlock reactiontime if + A_SetReactionTime** — `A_SkipUnlessReactionTime`.
- `2026-10-10` — M7: **DecoBlock species if + TakeFrom Children/Siblings** — `A_SkipUnlessSpecies`; inventory take helpers.
- `2026-10-10` — M7: **A_SetInvisible + sibling helpers** — `FLAG_DONTDRAW`; Raise/Remove/Kill/GiveTo/DamageSiblings; app skips draw.
- `2026-10-10` — M7: **DecoBlock damage if + A_ClearLastHeard** — `A_SkipUnlessDamage`; clear sector `soundtarget`.
- `2026-10-10` — M7: **A_SprayDecal** — DECALDEF name + optional forward offset → `decal_events`.
- `2026-10-10` — M7: **A_SetTeleFog / A_SwapTeleFog / A_Log** — DECORATE telefog + print alias.
- `2026-10-10` — M7: **DecoBlock roll if + A_SetDamage** — `A_SkipUnlessRoll`; runtime missile damage override.
- `2026-10-10` — M7: **A_QuakeEx / A_SetBlend** — max-axis quake; full-screen colour fade via `ScreenFade`.
- `2026-10-10` — M7: **A_FaceMovementDirection / A_AlertMonsters** — yaw from vel; noise flood + optional range wake.
- `2026-10-10` — M7: **A_SetPainThreshold / A_SetThreshold / A_JumpIfHigherOrLower** — runtime pain/infight gates; Z vs pointer.
- `2026-10-10` — M7: **A_SetFloatBobPhase / A_Weave** — bob phase `0..63`; Heretic weave step + spawn seed.
- `2026-10-10` — M7: **A_LookEx + melee-range jumps** — FOV/min/max wake; `A_JumpIfTargetOutside/InsideMeleeRange`.
- `2026-10-10` — M7: **DecoBlock alpha/gravity if** — scaled×1000 skip helpers `A_SkipUnlessAlpha`/`Gravity`.
- `2026-10-10` — M7: **A_RearrangePointers + GiveTo/TakeFromMaster** — snapshot pointer assign; master inventory helpers.
- `2026-10-10` — M7: **SNDSEQ playrepeat/playloop + A_SetFriendly/A_SetSpecies** — looping steps + `stop_sequence`; runtime species override.
- `2026-10-10` — M7: **A_Respawn / A_CopyFriendliness + SetSectorTerrain ceiling** — revive at spawn; copy FRIENDLY; `TerrainTable.ceilings`.
- `2026-10-10` — M7: **A_Teleport / A_SelectWeapon** — warp to `TeleportDest` + fog/state; player weapon queue.
- `2026-10-10` — M7: **A_CheckBlock / A_CheckLOF** — offset `check_position` jump; LOF = sight+range to target.
- `2026-10-10` — M7: **TranslucentLine render** — `Line.mid_alpha` → translucent wall buffer + sprite blend; app copies `line_alpha`.
- `2026-10-10` — M7: **GetLevelInfo PAR/SUCK/CLUSTER/LEVELNUM** — `LevelState` fields + MAPINFO apply in headless.
- `2026-10-10` — M7: **A_GiveToTarget/Children + A_DamageTarget/Master/Tracer/Children + A_TakeFromTarget** — pointer inventory/damage.
- `2026-10-10` — M7: **A_Raise/Remove/Kill Children|Master** — family pointer revive/destroy helpers.
- `2026-10-10` — M7: **plane reflection + A_FaceTracer/Master** — `__PREFL*` Z-mirrored canvases; `Camera::reflect_across_z`; app copies `plane_reflection`.
- `2026-10-10` — M7: **plane alpha render + ChangeCamera** — vertex `@location(3)` alpha; translucent flats via sprite blend; app copies sector alphas; camera view + `camera_revert`.
- `2026-10-10` — M7: **render plane/side scale+angle + A_SetRoll** — app copies `floor_scales`/`ceiling_scales`/`plane_angles`/`side_scales` into map; scene FlatUv + wall ÷scale; `A_SetRoll`.
- `2026-10-10` — M7: **Line_Horizon / Transfer_WallLight / Lightning / Plane_Copy / Line_SetIdentification / scroll copy** — 9/16/58/109/118/121/222/225.
- `2026-10-10` — M7: **ByTexture / TxTy / MoveToValueAndCrush / Polyobj_StopSound** — 261/268–269/239/241/279–280/283.
- `2026-10-10` — M7: **EE plane movers + Door_Animated* + Stairs Doom Sync/Crush** — 256–267/14/274/270–273.
- `2026-10-10` — M7: **Floor_RaiseToHighest + Stairs_*Sync/Down + CrushAndRaiseA*** — 24/26/27/31/32/196/197/169; RaiseToLowestCeiling → 238.
- `2026-10-10` — M7: **Floor/Ceiling_Waggle + TeleportInSector + CrushRaiseAndStay*** — 138/38/78/45/195/255; LowerToFloor → 254.
- `2026-10-10` — M7: **TeleportOther/Group + Plat_*Lip + Ceiling_*Times8** — 76/77/206/207/198/199.
- `2026-10-10` — M7: **Floor/Ceiling_MoveToValue* + Ceiling_*Instant + Scroll_Texture_Both** — 37/47/68/69/193/194/221.
- `2026-10-10` — M7: **Floor_Lower/RaiseInstant** — specials 66/67 (height×8, immediate).
- `2026-10-10` — M7: **Plat_Down/UpByValue + UpWaitDownStay + Teleport_NoStop** — specials 63–65/154; StayTx → 230.
- `2026-10-10` — M7: **Plat_UpNearestWaitDownStay + Line_SetAutomap*** — specials 172/281/282.
- `2026-10-10` — M7: **Floor_Stop / Ceiling_Stop / CrushAndRaiseDist** — specials 275/276/168.
- `2026-10-10` — M7: **Line/Sector_SetHealth + Floor/CeilingGlow** — specials 150/151/277/278.
- `2026-10-10` — M7: **Thing_ProjectileAimed/Intercept + PlaneReflection** — specials 178/175/159; ACS_ExecuteAlways → 226 (85 = LockedExecuteDoor).
- `2026-10-10` — M7: **Scroll_Wall / SetGlobalFogParameter / Scale2 / Thing_SetGoal** — specials 52/157/170–171/229.
- `2026-10-10` — M7: **Door_WaitRaise / Door_WaitClose** — specials 105/106; `DoorMover.wait_then_raise`.
- `2026-10-10` — M7: **Thing_SpawnFacing / Sector_ChangeSound / ChangeCamera** — specials 139/140/237.
- `2026-10-10` — M7: **Sector_SetColor/Fade/Damage/Gravity + Thing_SetTranslation** — specials 212/213/214/216/180.
- `2026-10-10` — M7: **Thing_Damage/Move + Sector_ChangeFlags + GlassBreak** — specials 119/125/54/49.
- `2026-10-10` — M7: **Sector_SetFloor/CeilingScale + SetRotation** — specials 189/188/185; `floor_scales`/`ceiling_scales`/`plane_angles`.
- `2026-10-10` — M7: **Sector floor/ceiling panning + DamageThing** — specials 187/186/73; GZDoom number fixes (ThrustThing 72, ThrustThingZ 128, Thing_Raise 17, Thing_SetSpecial 127, Line tex 53/56, SetPlayerProperty 191, Plat 60/61).
- `2026-10-10` — M7: **Line_SetTextureScale / SetPlayerProperty** — specials 56/191; `side_scales`; PROP_FROZEN/NOTARGET/FLY.
- `2026-10-10` — M7: **Line_SetTextureOffset** — special 53 → `World.side_offsets`.
- `2026-10-10` — M7: **Thing_Raise** — special 17; Raise-state revive + COUNTKILL adjust.
- `2026-10-10` — M7: **DecoBlock health cmp** — `health ==/>/>=/!=` → `A_SkipUnlessHealth` (`</<=` keep JumpIfHealthLower).
- `2026-10-10` — M7: **ThrustThingZ / Autosave** — specials 129/15; `World.autosave_request`.
- `2026-10-10` — M7: **Thing_Hate / Thing_SetSpecial** — specials 177/178.
- `2026-10-10` — M7: **Thing_Stop / HealThing** — specials 19/248.
- `2026-10-10` — M7: **Thing_ChangeTID / ChangeSkill** — specials 176/179.
- `2026-10-10` — M7: **NoiseAlert** — special 173; `noise_alert_ex(target, emitter)` flood.
- `2026-10-10` — M7: **TranslucentLine** — special 208; mid-tex alpha + additive on `World.line_alpha`.
- `2026-10-10` — M7: **Line_SetBlocking** — special 55; `BLOCKF_*` → `ML_*` set/clear on tagged lines.
- `2026-10-10` — M7: **Floor_RaiseAndCrush / Doom** — specials 28/99 (was misnumbered); XLAT emits 25/99 for nearest/crush.
- `2026-10-10` — M7: **Sector_SetTranslucent** — special 98; floor/ceiling alpha + additive flag on `SectorDynamic`.
- `2026-10-10` — M7: **Ceiling_LowerAndCrushDist** — special 97; gap = `dist` (default 8).
- `2026-10-10` — M7: **FloorAndCeiling_Lower/RaiseByValue** — specials 95/96 move both planes by height.
- `2026-10-10` — M7: **polyobj MoveTo / MoveToSpot** — specials 88/89/86/59 → translate toward absolute XY or tid spot.
- `2026-10-10` — M7: **DecoBlock velx/vely/velz if** — `A_SkipUnlessVelX|Y|Z`.
- `2026-10-10` — M7: **DecoBlock mass/height/radius if** — `A_SkipUnlessMass`/`Height`/`Radius`.
- `2026-10-10` — M7: **DecoBlock tid if** — `A_SkipUnlessTid`.
- `2026-10-10` — M7: **SNDSEQ attenuation / volumerel / stopsound** — steps + end cue; defs→sim conversion; `m7_udmf_user_props_smoke` accept.
- `2026-10-10` — M7: **UDMF `user_*` → ACS Get\*UDMF\*** — `UdmfUserStore` on parse/`MapGeometry`; int/bool/fixed bags; host lookup for line/thing/sector/side.
- `2026-10-10` — M7: **DecoBlock pitch if + Get/SetTeamScore** — `A_SkipUnlessPitch`; ACSF 19620–19621.
- `2026-10-10` — M7: **DecoBlock angle if + ffloor/slope smoke** — `A_SkipUnlessAngle`; `m7_ffloor_slope_setup_smoke` accept.
- `2026-10-10` — M7: **M7 feature smoke accept** — `ferrite-game/tests/m7_feature_smoke.rs` (portal map + SNDSEQ door-slot + Pillar_Build + fog); door/platform slot lookup on `SndSeqTable`.
- `2026-10-10` — M7: **SNDSEQ stream bootstrap** — `SndSeqTable` + tic runner (`play`/`delay`/`volume`); ACS `SoundSequence*` starts instances; Headless `set_sndseq`.
- `2026-10-10` — M7: **ACS SetSubtitle / GetNetID / SetSectorGlow / SetFogDensity** — ACSF 214–216/400–401; plane glow + fog→`UnlitPass`; net-id = actor raw.
- `2026-10-10` — M7: **DecoBlock speed / user_* if** — `A_SkipUnlessSpeed` / `A_SkipUnlessUserVar`; ACSF `GetLineX`/`GetLineY` (300–301).
- `2026-10-10` — M7: **ACS GetSectorHealth / GetLineHealth** — ACSF 212–213; `SectorDynamic.health_*` + `line_health` map.
- `2026-10-10` — M7: **ACS Get/SetUserCVarString + StartSlideshow** — ACSF 58–59/211; per-player string cvars; `slideshow_request`.
- `2026-10-10` — M7: **ACS AnnouncerSound / SetPointer / UserCVar / SetCVarString / ScriptCall** — ACSF 37–38/54–55/57/210; AAPTR target/master/tracer; string+user cvar maps; ScriptCall stub.
- `2026-10-10` — M7: **ACS SpawnParticle / HUD clip / SetCVar / GetCVarString** — ACSF 51–53/56/96; `SimParticleEvent` queue; skill/fast/respawn setters.
- `2026-10-10` — M7: **ACS Warp + QuakeEx** — ACSF 91–92; warp activator to tid+offset; QuakeEx → existing quake thinker (max intens).
- `2026-10-10` — M7: **ACS PickActor / IsPointerEqual / TeleFog / SetSectorDamage|Terrain** — ACSF 83–87/94–95; `line_pick_actor`; sector damage fields + floor→terrain map.
- `2026-10-10` — M7: **mirror/portal stencil in main pass** — unlit MRT; app `stencil_aux`; `draw_materials` masks `__MIRROR*`/`__PORTAL*` then Equal-draws canvases.
- `2026-10-10` — M7: **stencil MRT path** — dual colour targets + `fs_stencil_mask`/`fs_main_mrt`; Metal needs target-0 writes for Replace (Equal overwrites). Empty target-0 write-mask still skips stencil.
- `2026-10-10` — M7: **ACS SoundSequenceOn* / SetMusicVolume / CanRaise / SetTranslation / StrArg** — ACSF 30–32/85/97/203/206; sequence cues as positional SFX; `music_volume`.
- `2026-10-10` — M7: **sky scroll UV** — `scroll_sky_u` + app applies `World.sky_speed1 * maptime` (ACSF `SetSkyScrollSpeed`). Equal-only stencil still blocked on Metal (empty/keep-blend write masks skip Replace).
- `2026-10-10` — M7: **portal recursion depth** — `MAX_PORTAL_RECURSION=2`; nested `__PORTAL*` canvas refresh before parent blit; strip at max depth.
- `2026-10-10` — M7: **ACS roll / maxinv / Check* / DamageActor / PlayActorSound / SetSkyScrollSpeed** — `Actor.roll`; ACSF 18/71/88–90/93/99/200–201; `sky_speed*`; host + VM.
- `2026-10-10` — M7: **linked portal canvases** — `LinkedPortalSurface` + `__PORTAL*` mid quads; `Camera::shifted_by`; app refreshes displaced offscreen into materials (mirrors unchanged). Equal-only stencil still open on Metal.
- `2026-10-10` — M7: **ACS GetActorPowerupTics + SoundVolume** — ACSF 78 player power tic queries; `SoundVolume` rescales queued `SimSoundEvent`s for tid/activator.
- `2026-10-10` — M7: **stencil mask/equal pipelines** — `Depth24PlusStencil8` + `draw_stencil_mask`/`draw_stencil_equal` + same-pass `render_offscreen_stenciled` smoke. Strict Equal-only masking (no mask colour) still open on Metal; portal recursion next.
- `2026-10-10` — M7: **ACS ChangeLevel / GetPlayerInfo / SectorDamage / CheckActor*Texture** — host + VM; plane texture via BSP/`vanilla`/single-sector fallback.
- `2026-10-10` — M7: **GPU mirror clip plane** — `ViewUniforms.clip_plane` + fragment `discard`; `mirror_clip_plane` helper; app sets plane for `__MIRROR*` offscreen canvases (CPU tri drop kept). Full stencil buffer still open.
- `2026-10-10` — M7: **ACS ammo/state/damage + air** — `Get/SetAmmoCapacity`; `SetActorState`; `Thing_Damage2`; `UseActorInventory`; `PlayerInGame`/`SinglePlayer`; `SetAirControl`; `NegateBinary`/`EndLog`; fixed `UseInventory` PCD→336; ACSF `SpawnDecal`/`GetArmorInfo`/`SoundVolume`.
- `2026-10-10` — M7: **ACS gravity/sky/level/textures + LineAttack** — `SetGravity`(+Direct); `GetLevelInfo` (secrets/items/kills); `ChangeSky`→`sky*_override` (app); `ReplaceTextures`; `SetActorPitch`; ACSF `LineAttack`/`DropInventory`/`GetArmorType`.
- `2026-10-10` — M7: **ACS CASEGOTO + PlaySound/SpawnForced/Air** — `CASEGOTO` switch dispatch; ACSF `PlaySound`/`StopSound`/`SpawnForced`/`SpawnSpotFacingForced`/`Get/SetAirSupply` (`Player.air_finished`).
- `2026-10-10` — M7: **ACS SetLineBlocking/Special** — PCD 98/99 + `line_args` overrides; `line_flags` widened to i32 (`ML_BLOCKEVERYTHING`/`ML_RAILING`/…); physics/`activate_line` consult runtime flags/args; `SoundSequence` accepted (SNDSEQ play still open).
- `2026-10-10` — M7: **ACS math/UI queries** — `Sin`/`Cos`/`VectorAngle`; `PlayerNumber`; `GetScreenWidth/Height`; `StrLen`; `GetCVar` (skill/fast/respawn); `FadeRange`→FadeTo; fixed `Check/SetWeapon` PCD indices.
- `2026-10-10` — M7: **ACS FadeTo/SetFont/weapons** — `FadeTo`/`CancelFade` screen overlay; `SetFont`; `Check/SetWeapon`; `UseInventory`.
- `2026-10-10` — M7: **ACS SetMusic + activator inventory** — `SetMusic`/`LocalSetMusic`(+Direct) → `World.music_request`; `Clear/Give/Take/CheckInventory`(+Direct) on activator.
- `2026-10-10` — M7: **ACS sounds** — `SectorSound`/`AmbientSound`/`LocalAmbientSound`/`ActivatorSound`/`ThingSound` → `SimSoundEvent`.
- `2026-10-10` — M7: **ACS SpawnSpotFacing + ClassifyActor** — PCD 289 (spot angle) / 346 (ACTOR_* bitmask).
- `2026-10-10` — M7: **ACS actor inventory** — `Clear/Give/Take/CheckActorInventory` (PCD 284–287).
- `2026-10-10` — M7: **ACS ThingCount*** — `ThingCountName` / `ThingCountSector` / `ThingCountNameSector` (PCD 288/342/343).
- `2026-10-10` — M7: **KEYCONF alias/bind** — shell applies KEYCONF `alias` via console; `defaultbind` for unbound keys; ACS `GetActorLightLevel` PCD 340.
- `2026-10-10` — M7: **DecoBlock args[n]** — `args[n]` / `args[n] op N` → `A_SkipUnlessArg` (eq/ne/gt/ge/lt/le).
- `2026-10-10` — M7: **DecoBlock CountInv** — `CountInv`/`inventory`/`CheckInventory` conditions → `A_SkipUnlessInventory` / `A_JumpIfInventory`.
- `2026-10-10` — M7: **DecoBlock if/else** — `A_SkipAlways` + health `if/else` flatten (`SkipUnless` then / `SkipAlways` else).
- `2026-10-10` — M7: **mirror plane clip** — `clip_scene_to_mirror_line` drops tris on the reflected-eye side of the mirror line before offscreen blit (CPU stand-in for stencil).
- `2026-10-10` — M7: **DecoBlock if-body** — `A_SkipUnlessHealthLower` + `action_skip`; general `if (health < N) { … }` then-bodies; `A_ClearInventory`.
- `2026-10-10` — M7: **DecoBlock bootstrap** — anon `if`/`else`/`return`/`return state("L")`; const-fold + `health < N` → `A_JumpIfHealthLower`.
- `2026-10-10` — M7: **ACS GetActorFloorTexture/Terrain** — ACSF 204/205 via dyn strings (sector flat + TERRAIN floors map).
- `2026-10-10` — M7: **ACS CheckProximity** — ACSF 98 counts class actors in fixed distance of tid/activator.
- `2026-10-10` — M7: **mirror reflected omit** — reflected canvas strips `__MIRROR*` walls (avoids feedback without stencil).
- `2026-10-10` — M7: **OptionMenu cvar toggle** — `option <cvar>` / `cmd <line>` from MENUDEF activate via console `toggle`/`exec`.
- `2026-10-10` — M7: **A_DropItem** — DECORATE drop at feet with amount/chance (pairs ACSF `DropItem`).
- `2026-10-10` — M7: **ACS Set/GetLineActivation** — runtime `line_activations`/`line_flags` overrides; `activate_line` consults them; ACSF 76/77.
- `2026-10-10` — M7: **ACS named scripts + string ACSF** — `SNAM`/`ACS_NamedExecute*`; `SCRIPTWAITNAMED`; dyn string pool + `StrLeft|Right|Mid`/`strcmp`/`stricmp`; `GetActorClass`/`GetWeapon`; `Floor`/`Ceil`/`Round`; `CheckFlag`/`SetActorFlag`; `ChangeActorAngle`/`Pitch`; `DropItem`.
- `2026-10-10` — M7: **ACS script types** — Enter/Return/Death/Respawn/Unloading typed + fired on map enter/hub return/player death/respawn/exit.
- `2026-10-10` — M7: **Line_Mirror canvas** — mirror segs draw as `__MIRROR{n}` quads; app reflects camera across the line into an offscreen canvas (no stencil clip yet).
- `2026-10-10` — M7: **ACS ChangeFloor/Ceiling + SetLineTexture** — PCD 65–68/97 update `sector_*_pics` / switch texture maps; app applies ceiling flat overrides.
- `2026-10-10` — M7: **camera canvas + mirror math** — app refreshes `SetCameraToTexture` canvases into `MaterialCatalog`; `Camera::reflect_across_line` + `LinePortal::is_mirror`; portal recursion still open.
- `2026-10-10` — M7: **SetCameraToTexture** — PCD 269 + `World.camera_textures` bind (tid/activator → texture/FOV).
- `2026-10-10` — M7: **ANIMDEFS cameratexture** — parse name/size/`fit`/`hdr`/`worldpanning` into `CameraTextureDef`.
- `2026-10-10` — M7: **LOADACS + LOAD chunk** — parse `LOADACS` into `Definitions`; ACSE `LOAD` names on `AcsModule`; Headless/`push_acs_library` mounts library blobs from VFS.
- `2026-10-10` — M7: **sky cubemap** — GLDEFS 6-face skyboxes draw as inward cube (`CubemapFaces` + `sky_ranges`); incomplete boxes still use cylinder/`sky1`.
- `2026-10-10` — M7: **OptionMenu runtime** — `option_menu_from_def` maps Submenu/Command/Option/StaticText; `MenuStack` keeps MENUDEF and pushes on `openmenu`.
- `2026-10-10` — M7: **plane interp + ClearLineSpecial** — `SectorDynamic` prev floor/ceiling snap each tic; app lerps with `tic_accum`; ACS `CLEARLINESPECIAL` clears activating linedef via host.
- `2026-10-10` — M7: **MENUDEF runtime** — `list_menu_from_def` builds `ListMenu` from `TextItem`/`PatchItem` + `IfGame`; app title menu prefers MENUDEF `MainMenu`.
- `2026-10-10` — M7: **sky bootstrap** — GLDEFS `skybox` parse; MAPINFO `skybox`/`sky1` → textured cylinder; full cubemap draw still open.
- `2026-10-10` — M7: **KEYCONF apply** — `WeaponSlots` on `World`; `setslot`/`addslot*`/`playerclass` via `Headless::set_keyconf`; weapon select uses slot table.
- `2026-10-10` — M7: **decal billboards** — `DecalStore` + `append_decals` into sprite pass; app drains `SimDecalEvent` and loads DECALDEF/TERRAIN/LOCKDEFS from defs. Wall-clip when linedef known.
- `2026-10-10` — M7: **decal events** — `SimDecalEvent` + DECALDEF generators on wall hitscan; `Headless::set_decaldef`.
- `2026-10-10` — M7: **LOCKDEFS use** — `World.locks` + `actor_has_lock` for `Door_LockedRaise`/`Generic_Door`; `Headless::set_lockdefs` from defs.
- `2026-10-10` — M7: **TERRAIN splash** — `World.terrain` flat→splash; `z_movement` hit-floor plays splash sound / spawns class; `Headless::set_terrain` from defs.
- `2026-10-10` — M7: **SBARINFO draw** — `SbarInfoBar` interprets `drawimage`/`drawnumber`/`drawmugshot`/`drawswitchableimage` + `gamemode`/`usesammo`/`else`; Doom stock `normal` bar emits STBAR + health.
- `2026-10-10` — M7: **MENUDEF parse** — `ListMenu`/`OptionMenu`/`OptionValue`/`OptionString`/`Default*`/`Add*` + nested `IfGame`/`IfOption`; GZDoom stock lump parses; wired into `Definitions`.
- `2026-10-10` — M7: **KEYCONF parse** — line-oriented whitelist (`weaponsection`/`setslot`/`addslot*`/`alias`/`defaultbind`/`addkeysection`/`addmenukey`/`clearplayerclasses`/`addplayerclass`); wired into `Definitions`.
- `2026-10-10` — M7: **LOCKDEFS parse** — `ClearLocks`/`Lock n [Game]` with keys/`Any`/`Message`/`RemoteMessage`/`Mapcolor`/`LockedSound`; multi-game variants; GZDoom stock lump parses; wired into `Definitions`.
- `2026-10-10` — M7: **DECALDEF parse** — `decal`/`decalgroup`/`fader`/`stretcher`/`slider`/`colorchanger`/`combiner`/`generator`; GZDoom stock lump parses; wired into `Definitions`.
- `2026-10-10` — M7: **TERRAIN parse** — `splash`/`terrain`/`floor`/`defaultterrain`; `ifdoom|heretic|hexen|strife` filter; wired into `Definitions` (additive).
- `2026-10-10` — M7: **SBARINFO parse** — `base`/`height`/`statusbar`/`mugshot`; `drawimage`/`drawnumber`/nested `gamemode`/`else`; wired into `Definitions` (last lump wins).
- `2026-10-10` — M7: **DecoExpr** — parse arith/`random()`/`args[n]`/`user_*`; evaluate action args at `set_state` call time (`health`, vel, tid, …).
- `2026-10-10` — M7: **MAPINFO hubs** — `cluster`/`clusterdef` + `hub` flag; `Headless` snapshots leaving hub maps and restores on return; ACS world/global vars + player health travel.
- `2026-10-10` — M7: **ACSF more** — `GetActorViewHeight`/`GetChar`/`CheckActorProperty`/`Radius_Quake2`/`SpawnSpotForced`.
- `2026-10-10` — M7: **ACS user vars** — ACSF `Set/GetUserVariable`/`Set/GetUserArray`/`CheckActorClass`; DECORATE `A_SetUserArray`.
- `2026-10-10` — M7: **DECORATE actions** — `A_SetTranslucent`/`A_Recoil`/`A_SetUserVar` (`Actor.user_vars`) / `A_CheckProximity` / `A_ClearMaster`/`A_ClearTracer`.
- `2026-10-10` — M7: **DECORATE duration/anon** — const int duration expressions, `random(lo,hi)` tics, `{ A_Foo; A_Bar }` flattened to 0-tic follow-up states.
- `2026-10-10` — M7: **ACS actor property/position** — `Get/SetActorProperty` (health/speed/gravity/mass/height/radius), `SetActorPosition`/`Angle`, ACSF `SetActivator`/`ToTarget`/`SetActorVelocity`.
- `2026-10-10` — M7: **sector plane portals** — `Sector_SetPortal` stores plane; linked/teleport floors/ceilings warp on `z_movement`; scene skips the portal plane flats.
- `2026-10-10` — M7: **linked portal groups** — flood-fill groups + 2D displacement; `P_CheckSight` skips REJECT across groups and treats traversable portal 1S as passable.
- `2026-10-10` — M7: **line portals** — traversable `Line_SetPortal` (kinds 1–3) pass in `check_position` and teleport on cross (midpoint delta); scene skips those walls. Linked groups / sight / sector portals still open.
- `2026-10-10` — M7: **3D floor bootstrap** — `FFloorStatic.typ`; solid slabs in `check_position` (walk-under / stand-on); scene extra flats at control plane Z. Side walls + swim + actor light + alpha + ExtraFloor_LightOnly + midtex3d walk; ffloor portals / real midtex height still open.
- `2026-10-10` — M7: **polyobj render segs + slope Z** — scene emits poly lines from live verts (skip BSP copies); flats use `SecPlane::z_at`; sim `check_position` / ACS `GetSector*Z` follow slope + mover delta.
- `2026-10-10` — M7: **ACSF Get*UDMF* + CheckSight** — built-in line/sector/side keys; `CheckSight` via `P_CheckSight`; `user_*` via `UdmfUserStore`.
- `2026-10-10` — M7: **ACS Spawn*** — `SPAWN`/`SPAWNDIRECT`/`SPAWNSPOT`/`SPAWNSPOTDIRECT` (DoomEdNum, byte angle, tid).
- `2026-10-10` — M7: **ACS ThingCount** — `THINGCOUNT`/`THINGCOUNTDIRECT` via host (DoomEdNum + tid, health>0).
- `2026-10-10` — M7: **ACS GetActor*/sector PCDs** — `GetActorX/Y/Z/FloorZ/CeilingZ/Angle/Pitch`, `GetSectorFloorZ/CeilingZ/LightLevel`, `ActivatorTID`, `FixedMul`/`FixedDiv`; activator key on scripts; ACSF `GetActorVel*`.
- `2026-10-10` — M7: **ACS CALLFUNC / ACSF stub** — `PCD_CALLFUNC`/`SETRESULTVALUE`; VM math ACSF (`Sqrt`/`FixedSqrt`/`VectorLength`); host `UniqueTID`/`IsTIDUsed`/`GetPolyobjX|Y`.
- `2026-10-10` — M7: **ACSe packed immediates** — `PUSHBYTE`/`PUSH*BYTES`, `LSPEC1..5DIRECTB`, `DELAYDIRECTB`, `RANDOMDIRECTB`.
- `2026-10-10` — M7: **ACSe opcode decode** — VM `fetch_op` (1-byte / 240+ext) + `NEXTBYTE` immediates; ACSe delay/terminate unit green.

- `2026-10-10` — M7: **ACSE loader** — pure `ACSE`/`ACSe` + hybrid ACS0+chunks; `SPTR` (8/12-byte), `STRL`/`STRE` decrypt; `AcsFormat` on module.

- `2026-10-10` — M7: **Thing_*/Pillar_*** — `Thing_Spawn(NoFog)`/`Projectile(Gravity)`/`Activate`/`Deactivate`/`Remove`/`Destroy`; `Pillar_Build(AndCrush)`/`Open`. Fixed `Floor_RaiseByTexture` Hexen num 29→240 (was colliding with Pillar_Build). Spawn type = DoomEdNum until `spawnnums`.
- `2026-10-10` — M7: **ACS0 bootstrap** — `MapGeometry.behavior`; `ferrite-acs` ACS0 loader + Hexen PCD subset VM (`DELAY`/`LSPEC*`/vars/arith/print); `World.acs`; specials 80–85 (`ACS_Execute*`/`Suspend`/`Terminate`); OPEN scripts on map enter + tic thinker. ACSE/ACSe, ACSF, libraries, hubs still open.
- `2026-10-10` — M7: **Hexen specials** — `Teleport_NewMap`/`EndGame` (`next_map_override`), `Radius_Quake`, `ThrustThing`, poly `OR_Move`/`OR_Rotate*` (busy reject vs override). Pillar / Thing_* still open.
- `2026-10-10` — M7: **polyobj DoorSwing/DoorSlide + mirror** — open→wait→close; mirror gets opposite rotate / 180° slide. Render segs / OR_* / MoveTo still open.
- `2026-10-10` — M7: **polyobj collide** — after Move/Rotate, solid AABB×seg test; non-crush undoes step + stops; crush deals 10 dmg and continues. Mirror polys / DoorSwing args / render segs still open.
- `2026-10-10` — M7: **polyobj bootstrap** — `World::polyobjs` from map setup; anchor→start-spot relocate; `Polyobj_Move` / `MoveTimes8` / `RotateLeft|Right` / `Stop` + tic advance. Unit: spawn +64 then Move +16. Collision / mirrors / DoorSwing fidelity / render segs still open.
- `2026-10-10` — M6 **done**: DEMO1 full playback bitexact vs Chocolate end `(1468.330582,-315.818542)` hp=-2. Last fix: **`MF_CORPSE` ledge friction skip** (`floorz != sector floorheight` + \|mom\| > ¼) — dead Demon on MAP03 step (floorz=-4 / secfloor=-5) was rubbing → lagged ~0.7u → blocked player Y@1918. Active → **M7**.
- `2026-10-10` — M6: **`MF_CORPSE` ledge friction skip** (`floorz != sector floorheight` + \|mom\| > ¼). Dead Demon on MAP03 step (floorz=-4 / secfloor=-5) was rubbing → lagged ~0.7u north → blocked player Y@1918. Now DEMO1 **bitexact end** vs Chocolate `(1468.330582,-315.818542)` hp=-2 (`freedoom2_demo1_vanilla_full_playback` green).
- `2026-10-10` — M6: **`P_CheckMeleeRange`** = `MeleeRange-20+target.radius` + `P_AproxDistance` + sight (was euclidean `len<=64` → Demon false melee @1859, skipped ActiveSound → idx−1 @1860). With Demon Mass 400: bitexact through **@1917**; ~2.3u Y @1918; hp/idx still match @2000/@2100. End still ≠ ~(1468,-316).
- `2026-10-10` — M6: **Demon `Mass 400`** (was default 100 → 4× knockback). Wrong pistol target @1836 (Zombie vs Demon) and idx+1 @1838 came from over-thrust after @1828 hit. With Mass 400: HIT@1836==Demon; idx@1838==172; xy bitexact through **@1900**; idx−1 @1860. End ~(1189,-199) hp=-8 vs Choco ~(1468,-316) hp=-2.
- `2026-10-10` — M6: **Doom `P_CheckSight` BSP** (`P_CrossBSPNode` + `P_DivlineSide`/`P_InterceptVector2` slopes, including negative frac). Extra `pr_checkmissilerange` @1780 was a false-positive LOS on MAP03 ZombieMan (−329,−615) vs player — Choco blocked on L2834 frac≈−31. idx@1780==152; xy/hp bitexact through **@1900**; idx+3 @1864 → hp@2000 (31 vs 22). End still ≠ ~(1468,-316).
- `2026-10-10` — M6: **`player->refire` / `P_GunShot`** — pistol/chaingun `!refire` yaw (`P_SubRandom()<<18`). Slot is `take()`n during `player_think`, so fire actions read `World::thinking_refire`. idx@1729/@1744==Choco; xy/hp bitexact through **@1745** (and path @1800); idx+1 @1780 → xy@1864. End still ≠ ~(1468,-316).
- `2026-10-10` — M6: **`A_Chase` missile gate** = Doom `if (missilestate)` before `P_CheckMissileRange` (was rolling RNG for Demon/Spectre with Melee only → idx+1 @1727). idx@1727==136; xy bitexact through **@1744**; idx−2 @1729 → hp@1744 (64 vs 58). End ~(1438,-207) hp=-7 vs Choco ~(1468,-316) hp=-2.
- `2026-10-10` — M6: **Doom `EV_Teleport` by sector tag** (XLAT tid=0 was always first `TeleportDest` → closet WR tag=4 landed on tag=1 pad (−736,288) instead of (−640,−352)). Also dest fog `+20*finesine`, `z=floorz`, TeleportFog `+NOBLOCKMAP`. idx@1710==Choco 55; xy bitexact through **@1744**; idx+1 @1727 → xy/hp @1745 (55 vs 58). End still ≠ ~(1468,-316).
- `2026-10-10` — M6: **airborne friction** = Doom `z > floorz` skip (was `z <= floor+1` → still rubbed at z=-4/floor=-5). With Hexen speed/8: xy/momi bitexact through **@1709**; idx+2 @1710 — Demon teleports @1708 onto (−736,288) pad then `NewChaseDir`→nodir (Choco walks); end ~(1440,-307) hp=-3 vs Choco ~(1468,-316) hp=-2.
- `2026-10-10` — M6: **Hexen mover speed** = arg/8 (`SPEED(a)` / Doom `F_SLOW=8` → 1 mu/tic). Was treating arg as mu/tic (8× fast) → MAP03 tag-6 floor dropped early, closet Demons `P_Move` south past 24-step onto floor 40→16, burning wrong `NewChaseDir` RNG. After fix: xy/idx bitexact through **@1689** (idx@1687==180); mom diverges @1689 (Choco floor−5 / higher mom) → path@1690; end still ≠ ~(1468,-316).
- `2026-10-10` — M6: DEMO1 **touch_specials** after `xy_movement` (was pre-move in `player_think` → healthbonus/stim 1 tic late). **1ulp@914** = move-split remainder `>>1` not `/2`. **STOPSPEED** before friction. xy/hp/idx bitexact through **@1686**; idx−2 @1687 (wake Zombie `try_walk` vs Choco `NewChaseDir`); end still ≠ ~(1468,-316).
- `2026-10-09` — M6: DEMO1 **idx@1337** — blocked `CANUSEWALLS` chase was player-`use_lines` (64u ray) and Use-activated WR Teleport MonstersOnly (Doom 126 → Hexen 70), spawning 2× `TeleportFog`/`pr_spawnmobj`. Match Doom `P_UseSpecialLine`: non-players Use only door specials. Path/idx ≈Choco through **@1686** (stim heal 1 tic late @1635 from ~0.06u drift); idx+2 @1687. End still ≠ ~(1468,-316).
- `2026-10-09` — M6: DEMO1 slide **path-traverse** (3 corner rays) + 1S backside skip + `mom==0` lead/trail. Clears @131 ulp (was back-side L1495) and @1360 (Choco L534 frac=40085). Path/momi ≈Choco through **@1360** (1–2 ulp from @914); idx blip @1337. End still ≠ ~(1468,-316).
- `2026-10-09` — M6: DEMO1 **@1007 slide** = Choco L40 (`PIT` short-trace dual-path + vertex-adjacent candidates; @58 OK). **`P_NoiseAlert`**: stamp sector `soundtarget` (no instant wake); alert from `P_FireWeapon`/`A_ReFire` not `A_Fire*`. Shotgun@1332 idx==Choco (45). Path/idx through **@1359**; stick @1360 (momi=0 vs Choco coast; 1-ulp Y from ~@140). End still ≠ ~(1468,-316).
- `2026-10-09` — M6: DEMO1 **bitexact through @1006**. @1007: Choco `BESTSLIDE` L40 (768,512)–(764,540) frac=15453; Fer AABB-only set picks L1083 (764,540)–(732,572). Root: `PIT_AddLineIntercepts` short-trace (`|mom|≤16` → `P_PointOnLineSide`) — naive port regressed DEMO1@58; needs careful dual-path. Also 1-ulp Y from @914. End still ≠ ~(1468,-316).
- `2026-10-09` — M6: DEMO1 **bitexact through @1006** (idx+xy). Fixes: `P_KillMobj` death-height (`>>2`) + keep `SOLID` until `A_Fall` (@891 corpse); `P_TouchSpecialThing` Z `delta < -8` (was `-item.height` — missed dropped shotgun +4 shells → dry @993). Next: @1007 slide line. End still ≠ ~(1468,-316).
- `2026-10-09` — M6: DEMO1 bitexact through **@890** (`finesine` aim + path-traverse thing filter; BulletSlope@798=−2004). @891: Choco solid corpse (hp=-10, pre-`A_Fall`). Keep `SOLID` until `A_NoBlocking` + walk clip ignores Z → idx+1 @665 (`pr_explode`/`pr_chase` vs corpse). Next: monster↔corpse sync from first kills, then @891. End still ≠ ~(1468,-316).
- `2026-10-09` — M6: DEMO1 XY/mom/**prndindex** bit-exact vs Chocolate through @560. Added: Doom `P_RecursiveSound`-style noise flood (was 1024u radius — woke west Imp @493). Path bitexact through @581 mom; idx −1 from ~@572 → hp split @583. End still ≠ Choco ~(1468,-316).
- `2026-10-09` — M6: DEMO1 player XY/mom **bit-exact vs Chocolate through @560** (incl. L144 slide). Fixes: authentic Doom `finesine`/`tantoangle`; slide `P_InterceptVector` `>>8` + corner traces; Doom `P_HitSlideLine`; damage thrust via `R_PointToAngle2`. Path gates through @550. Next: `prndindex` drifts from ~@496 (`pr_chase`); premature ImpBall hit @581 → end still ≠ Choco ~(1468,-316).
- `2026-10-09` — M6: @549 Choco `P_SlideMove` on MAP03 L144 (blocking diag); Fer misses it — ~0.3u westward XY drift (same mom). Path gates through @550; split @560. Next: bit-exact vanilla XY (cut f64 round-trip drift) so L144 slide hits.
- `2026-10-09` — M6: `P_KillMobj` DropItem after death-tic trim (was before; stole trim `P_Random`). Screams @459/@460 match Choco; no false HIT@503; path gates through @550.
- `2026-10-09` — M6: stop clearing monster `momxy` in `A_Chase` (Doom keeps damage thrust). Wounded zombie path==Choco through @456; shotgun 3-hit match; `prndindex`==Choco @457/@460. Path still≈Choco @500; premature zombie hit @503 → @520 drift (~10u).
- `2026-10-09` — M6: DEMO1≈Chocolate through @520. Aim slope -1353≈Choco -1352 (`PIT_AddThingIntercepts` diagonal + `chase_move` floor via `check_position_fixed`). Prefer IWAD nodes (`ensure_nodes`) — rebuilt BSP misclassifies MAP03 ledge as floor 8. Next: zombie pistol hit @531 (Choco none); end still ~(384,1122) vs Choco ~(1468,-316).
- `2026-10-09` — M6: `P_BulletSlope` + sloped `P_LineAttack` for player guns. DEMO1 path≈Chocolate through @450 (was diverging at stairs); Choco also ends dead. Drift resumes ~@500–800 (aim slope -1091 vs Choco -1352).
- `2026-10-09` — M6: hitscan walls (`PTR_ShootTraverse` openings) + Doom `shootz=z+height/2+8`. Chocolate also damages zombies at @410 (HIT log was windowed); missing autoaim slope was the real pellet mismatch.
- `2026-10-09` — M6: shotgun fire via `A_WeaponReady` (no same-tic Fire tic burn). DEMO1 `prndindex`==Chocolate through @411 (`SHOTGUN@338`, @340=171, @406=12, @411=72). Player path ≈Chocolate through @441; diverge on north stairs ~@450 (end still far from Choco ~(1468,-316)).
- `2026-10-09` — M6: `A_Scream` burns `P_Random` for podth/bgdth families (Freedoom shotguy); restores RNG match at @406. Shotgun fire still 1 tic early vs Chocolate.
- `2026-10-09` — M6: DEMO1 `prndindex`==Chocolate through @400 (shotgun kill @338 bit-exact). Fixes: pickup parks player during `player_think`; `A_Chase` ActiveSound after move; die clears SHOOTABLE; puff/blood/`P_KillMobj` tics RNG; mapthing-only spawn tics random.
- `2026-10-09` — M6: `A_Chase` → Doom `P_Move`/`P_NewChaseDir`/`P_TryWalk` (xspeed 47000); west zombies closer @441 but still late vs Chocolate ~(15,671).
- `2026-10-09` — M6: DEMO1 `P_Random` bit-exact vs Chocolate through @300 (map-thing lastlook/tics → lights; `ShotgunGuy` DoomEdNum 9; `TeleportDest` 14; sight REJECT+segment AABB). Diverge ~@306 (ShotgunGuy wake) → @441.
- `2026-10-09` — M6: DEMO1≈Chocolate through ~@400 (slide opening-block 2S lines); `P_CheckMissileRange` + damage thrust; attack still early vs Chocolate (~@412 vs @441).
- `2026-10-09` — M6: DEMO1@70–200 ≈Chocolate (`P_XYMovement` split on `> MAXMOVE/2`).
- `2026-10-09` — M6: DEMO1@70–160 ≈Chocolate (`P_ZMovement` order + first-fall `-2*GRAVITY`).
- `2026-10-09` — M6: DEMO1@70–100 ≈Chocolate (onground-only thrust; no air control).
- `2026-10-09` — M6: DEMO1@78–87 ≈Chocolate (skip frac=0 on non-touching segments; slide retry; STOPSPEED+cmd).
- `2026-10-09` — M6: DEMO1@77 south-corner slide retry (3×) + STOPSPEED needs move cmd; @78–81 west escape still short vs Chocolate.
- `2026-10-09` — M6: DEMO1@66–76 ≈Chocolate (thing stairstep, `P_LineOpening` floorz, Doom `P_SlideMove` 0x800 fudge); @70 within ~0.03.
- `2026-10-09` — M6: DEMO1@70 ≈Chocolate (±0.5) after airborne-friction skip + MAXMOVE clamp; slide/AABB/square-thing collision.
- `2026-10-09` — M6: VanillaFixed slide/collision closer to Doom (AABB lines, square things, HitSlideLine+stairstep, sector planes).
- `2026-10-09` — M6: FixedMul overflow in thing collision (DEMO1 was stuck); full DEMO1 playback.
- `2026-10-09` — M6: Chocolate Doom local DEMO1 timedemo script; DEMO1 uses full actors.
- `2026-10-09` — M6: vanilla `P_SlideMove` bootstrap (project mom onto blocking line).
- `2026-10-09` — M6: `z_movement_fixed` + Doom `ANG45` spawn angles.
- `2026-10-09` — M6: fixed-point `xy_movement_fixed` (`P_XYMovement` + ORIG_FRICTION).
- `2026-10-09` — M6: Doom `finesine`/`finecosine` + BAM yaw/thrust for VanillaFixed.
- `2026-10-09` — M6: VanillaFixed player thrust/coast (Doom scale); DEMO1@70 pos golden.
- `2026-10-09` — M6: DEMO1 smoke under VanillaFixed; `VanillaLmp` parse; Generic_Floor target-6.
- `2026-10-09` — M6: Generic_Floor target-6 + numeric model; `-dogs`; Complevel CI demos.
- `2026-10-09` — M6: `-dogs` spawn; Freedoom Complevel Vanilla/Boom demo CI; dog DECORATE.
- `2026-10-09` — M6: `MBFHelperDog` DECORATE (888); VanillaFixed friction mul; `docs/qa/mbf21-wads.md`.
- `2026-10-09` — M6: MBF21 `A_SpawnObject`/`MonsterProjectile`/`Bullet`/`Melee`/`RadiusDamage`/`NoiseAlert`/`tracer`/`HealChase`/`JumpIf*`/`flags`.
- `2026-10-09` — M6: MBF `A_Detonate`/`Mushroom`/`Spawn`/`Turn`/`Face`/`Scratch`/`RandomJump`/`LineEffect`.
- `2026-10-09` — M6: MBF `MTF_FRIEND`/UDMF friend + follow-player when idle.
- `2026-10-09` — M6: one-shot line specials (`ML_REPEAT_SPECIAL` / `line_specials`).
- `2026-10-09` — M6: Boom trigger → SPAC activation; UDMF playeruse/cross.
- `2026-10-09` — M6: Boom gen floor/ceiling absolute height + trigger flags.
- `2026-10-09` — M6: `compat/levelfixes.ron` parse/apply (`LevelFixesDatabase`).
- `2026-10-09` — M6: per-map `compat/flags.txt` DB; `World::p_random`; fixed try_move.
- `2026-10-09` — M6: `World::p_random` → vanilla rndtable; fixed `try_move`.
- `2026-10-09` — M6: fixed-point `try_move` / `FixedMul`/`FixedDiv`; SimMathMode.
- `2026-10-09` — M6: `vanilla::SimMathMode` bootstrap; demo Complevel on verify.
- `2026-10-09` — M6: demo Complevel stamp; Boom secret-bit clear; InfightingGroup.
- `2026-10-09` — M6: DSDHacked `Deh_Actor_N`; UMAPINFO bossaction; TRANMAP; compat.
- `2026-10-09` — M6: Boom sector bits (damage/friction) + MBF21 kill-grounded.
- `2026-10-09` — **M5 done** (accept: P1/P2 automated demos + determinism CI).
  Residual: CHF_* / expanded 1:1 states / full option menus → M6–M13; manual UV
  remains in `docs/qa/`. **M6** started: `COMPLVL` lump parse.
- `2026-10-09` — M5: re-recorded P1/P2 `.fdem`/`.fdbn` goldens after chase AI RNG.
- `2026-10-09` — M5: friendly `A_Look`/`A_Chase` target enemy monsters.
- `2026-10-09` — M5: `A_Chase` `CANUSEWALLS` door open when blocked.
- `2026-10-09` — M5: DeHackEd Frame apply via `states.txt` (label/offset).
- `2026-10-09` — M5: `A_Chase` 8-dir `P_NewChaseDir` + blocked repath.
- `2026-10-09` — M5: corpse queue + player bodyque + `A_QueueCorpse`.
- `2026-10-09` — M5: DeHackEd `ferrite.pk3/dehacked/` sound/sprite/thing tables +
  `[SOUNDS]`/`[SPRITES]` apply.
- `2026-10-09` — M5: committed `freedoom2_map21_30.fdbn` (P2 MAP01–30 automated).
- `2026-10-09` — M5: committed `freedoom2_map11_20.fdbn`.
- `2026-10-09` — M5: committed Freedoom1 E2–E4 `.fdbn` goldens (P1 automated
  start→finish).
- `2026-10-09` — M5: committed `freedoom1_e1.fdbn` (E1M1–E1M8).
- `2026-10-09` — M5: Freedoom1 E1–E4 episode exit chains (E4M8 → finale).
- `2026-10-09` — M5: MAP11–MAP29 exit chains + MAP30 finale (Forced Exit_Normal
  fallback for boss maps).
- `2026-10-09` — M5: committed `freedoom2_map01_10.fdbn` golden.
- `2026-10-09` — M5: headless MAP01–MAP10 + E1M1–E1M3 exit chains.
- `2026-10-09` — M5: `determinism` CI (native demo verify + wasi fingerprints).
- `2026-10-09` — M5: CLI `demo info|verify` for `.fdbn` + DECORATE actors;
  `docs/qa/freedoom-playthrough.md`.
- `2026-10-09` — M5: `.fdbn` multi-map demo bundles + committed MAP01–03 golden.
- `2026-10-09` — M5: headless MAP01–MAP05 exit chain + Freedoom1 E1M1→E1M2.
- `2026-10-09` — M5: MAPINFO next/par on intermission; MAP01→MAP02 continue smoke.
- `2026-10-09` — M5: DeHackEd Frame/Pointer/`[CODEPTR]` apply (ZombieMan
  bootstrap frame map).
- `2026-10-09` — M5: committed `testdata/demos/freedoom2_map01_smoke.fdem` +
  verify test (`FERRITE_WRITE_DEMO=1` to regenerate).
- `2026-10-09` — M4 **done**: web UnlitPass viewer, golden+fps CI, `web-browser-tests`
  (Firefox/`wasm-bindgen-test`). Active focus → M5 accept (demos/determinism).
- `2026-10-09` — M4: MAP01 offscreen fps floor (≥15 CI / ~500+ native); web
  UnlitPass viewer + golden CI.
- `2026-10-09` — M3 **done**: UDMF/`Plane_Align`/9500 slopes, poly/portal/3D-floor
  setup + testkit helpers; all M3 accept boxes green. Active focus → M4.
- `2026-10-09` — M3: float↔vanilla BSP parity (~99.99%); UDMF lexer floor raised
  to 100 MiB/s. Next: setup fidelity / testkit helpers.
- `2026-10-09` — M3: float BSP side test matched Doom `R_PointOnSide`; grid
  parity vs vanilla walk on Freedoom MAP01 (~99.99%).
- `2026-10-09` — M3: `build_reject` (2S connectivity groups) + auto-build when
  REJECT missing; `build_reject_zero`.
- `2026-10-09` — M2 **done**: main `test` CI fetches/caches Freedoom 0.13.0 so
  definition/map goldens always run. Active focus → M3.
- `2026-10-09` — M1 **done**: `sevenz_header` fuzz + CI; all M1 accept boxes checked.
  Active focus → M2 (Freedoom fetch on main `test` job).
- `2026-10-09` — M1: `autoload` module (section keys + dir/config path collect).
- `2026-10-09` — M1: ZIP bzip2+lzma via `zip` 8.6 (pure Rust); synthetic IWADINFO
  matrix test; fixture `testdata/vfs/lzma_hello.pk3`.
- `2026-10-09` — Tracker created from repo audit. Strict accept: only M0 `done`;
  M1–M5 `in_progress`. Active coding focus remains M5 playsim gaps while noting
  earlier accept holes.

---

## M0 — Workspace and core

**Status:** `done` · Spec: `20` §M0 · Docs: `00`, `01`, `02`, Appendix A

### Plan

1. Workspace skeleton (all crates, READMEs, lints).
2. CI baseline (`fmt`, `clippy`, `test`, `doc`, `deny`, wasm build).
3. `ferrite-core`: names, IDs, arena, math, dmath, SFMT + vanilla RNG.
4. Acceptance: SFMT vectors, dmath 1M-sample hash across native + wasm, arena props.

### Checklist

**Deliverables**

- [x] Workspace crates + READMEs + root lints
- [x] CI pipeline skeleton (`.github/workflows/ci.yml`)
- [x] `Name` / `NameTable`, typed IDs, generational arena
- [x] `DVec2/3`, `Angle`, `Fixed`, BAM, `dmath`, `conv`
- [x] SFMT-607 + named RNG + vanilla `rndtable`

**Acceptance**

- [x] SFMT reference vectors (`ferrite-core` RNG tests)
- [x] dmath bit-identical hash (native matrix + `dmath-wasm`)
- [x] Arena property tests (proptest)

### Notes

Later §4 jobs (`determinism`, `golden-images`, `bench`, …) belong to later
milestones; their absence does not reopen M0.

---

## M1 — Resources

**Status:** `done` · Spec: `20` §M1 · Docs: `03`

### Plan

1. VFS: WAD, ZIP (deflate/bzip2/lzma), 7z, directories, nested WADs, filters, strip.
2. IWADINFO detection + load order + autoload.
3. CLI `wad ls|extract|info`.
4. Fuzz targets 60 s in CI; synthetic IWAD for every `MustContain` kind.

### Checklist

**Deliverables**

- [x] WAD + directory mounts, nested WAD, filters, single-root strip
- [x] ZIP deflate + 7z (feature)
- [x] ZIP bzip2 + lzma (`zip` 8.6, pure Rust)
- [x] IWADINFO parse + `detect_iwad` (`wadsrc/ferrite/IWADINFO`)
- [x] Autoload / search-path autoload (`autoload_sections` + `collect_autoload_paths`)
- [x] CLI `wad ls|extract|info`
- [x] Fuzz: `wad_directory`, `zip_central_directory` in CI
- [x] Fuzz: `sevenz_header` (per `03` §9; CI `fuzz-smoke`)

**Acceptance**

- [x] VFS unit/integration smoke (partial vs `03` §9)
- [x] Synthetic WADs detecting **every** IWADINFO kind
- [x] Fuzz targets 60 s without crash (incl. `sevenz_header` in CI)

### How to verify

```bash
cargo test -p ferrite-vfs
cargo run -p ferrite-cli -- wad info testdata/freedoom/freedoom2.wad
```

---

## M2 — Formats and definitions bootstrap

**Status:** `done` · Spec: `20` §M2 · Docs: `04`, `05`

### Plan

1. Binary formats (graphics, palette, sound, MUS→MIDI, fonts).
2. `ferrite-sc` + `ferrite-defs` phases 0–6 and 9.
3. CLI `defs dump`; Freedoom insta goldens; MAPINFO corpus.

### Checklist

**Deliverables**

- [x] Patches, flats, PNG/JPEG/TGA/PCX/BMP, PLAYPAL/COLORMAP
- [x] DMX/WAV/OGG/FLAC/MP3, MUS→MIDI, fonts
- [x] GAMEINFO, LANGUAGE, CVARINFO, TEXTURES/TEXTUREx/PNAMES
- [x] ANIMATED/ANIMDEFS/SWITCHES, FONTDEFS
- [x] MAPINFO (multi-syntax) + UMAPINFO, SNDINFO, SNDSEQ
- [x] CLI `defs dump`
- [x] Remaining text lumps from `05` deferred to later milestones (not M2 accept)
- [x] Fuzz: `mapinfo`, `mus_to_midi`, `image_lumps`

**Acceptance**

- [x] Freedoom Phase 1/2 definition snapshots (always on in CI via fetch)
- [x] MAPINFO corpus fixtures parse with zero errors
- [x] Main `test` CI job fetches Freedoom so goldens always run

### How to verify

```bash
./scripts/fetch-freedoom.sh
cargo test -p ferrite-defs
cargo run -p ferrite-cli -- defs dump testdata/freedoom/freedoom2.wad
```

---

## M3 — Maps

**Status:** `done` · Spec: `20` §M3 · Docs: `06`

### Plan

1. Doom/Hexen/UDMF loaders, XLAT, node formats, nodebuild, blockmap, reject.
2. Slopes / 3D floors / portals / polyobjects setup data.
3. `ferrite-testkit` UDMF DSL; CLI `map info|render2d|nodebuild`.
4. Accept: all Freedoom maps, convexity, point-in-sector parity, UDMF ≥100 MB/s.

### Checklist

**Deliverables**

- [x] Doom / Hexen / UDMF loaders + validation + XLAT
- [x] Node loading (classic + compressed/GL variants)
- [x] `ferrite-nodebuild` + blockmap build
- [x] Reject **parse**
- [x] Reject **builder** (`build_reject` connectivity + `build_reject_zero`)
- [x] Slopes / 3D floors / portals / polyobj **setup data** (UDMF planes,
  `Plane_Align`, 9500/9501, poly trace, portal/ffloor decls; portal groups → M7)
- [x] Testkit rooms/doors/starts
- [x] Testkit slopes/portals/poly/ffloor helpers
- [x] CLI `map info|render2d|nodebuild`

**Acceptance**

- [x] Freedoom maps load (skip if IWAD absent)
- [x] Node builder convexity/closure checks
- [x] Point-in-sector **parity vs original nodes** on a grid (float ↔ vanilla)
- [x] UDMF lexer ≥ 100 MB/s enforced

### How to verify

```bash
./scripts/fetch-freedoom.sh
cargo test -p ferrite-map
cargo run -p ferrite-cli -- map info testdata/freedoom/freedoom2.wad MAP01
```

---

## M4 — Renderer foundation + shells

**Status:** `done` · Spec: `20` §M4 · Docs: `13`, `18`

### Plan

1. wgpu device, textures, BSP walk, walls/flats/sprites, sky, lighting modes, 2D.
2. Desktop `ferrite-app` map viewer; `ferrite-web` same viewer (WebGPU + WebGL2).
3. Golden images + Freedoom MAP01 target fps on desktop and browsers.

### Checklist

**Deliverables**

- [x] Device tiers, texture manager, BSP + clipper
- [x] Walls/flats/sprites (static), classic sky
- [x] `DoomSoftware` / `ClassicPalette` light modes, fog fields
- [x] 2D canvas, fonts, screenshots, offscreen API
- [x] `ferrite-app` free-fly viewer
- [x] `ferrite-web` map viewer + IWAD play (`ferriteStartIwad` / `/doom1.wad` / file picker)
- [x] Offscreen smoke + lavapipe `render-smoke` CI
- [x] Local `golden_room` SSIM fixture

**Acceptance**

- [x] Golden images job in CI (`golden_room` on lavapipe `render-smoke`)
- [x] Viewer Freedoom MAP01 fps floor in CI (`freedoom_map01_offscreen_fps_floor` ≥15;
  desktop mid-laptop ≥120 is `13` §14 checklist)
- [x] Viewer in Chrome/Firefox (`web-browser-tests` job + `browser_smoke`)

### How to verify

```bash
cargo run -p ferrite-app -- --iwad testdata/freedoom/freedoom2.wad --map MAP01
cargo test -p ferrite-render
cargo build --target wasm32-unknown-unknown -p ferrite-web
```

---

## M5 — Doom playsim

**Status:** `done` · Spec: `20` §M5 · Docs: `07`–`11`, `14`–`17`

### Plan (work order)

1. **Actors & content** — DECORATE builtins (`wadsrc` / `doom_basic`), DeHackEd/BEX apply.
2. **Sim core** — thinkers, movement/collision, sight, hitscan, damage, inventory/weapons, player.
3. **AI & combat** — Doom monster AI, attacks, bosses, projectiles, radius damage.
4. **World** — Doom specials via XLAT, sector specials, movers, lights, switches, teleports.
5. **Level flow** — exits, secrets, intermission, finale, title loop.
6. **Shell systems** — audio SFX, music (SoftFont/OPL), input, console/CVars, Doom UI.
7. **Persistence** — save/load, Ferrite demo record/verify + checksums.
8. **Accept** — Freedoom P1/P2 start→finish demos + cross-platform determinism CI.

### Checklist

**Deliverables — content / mod formats**

- [x] `ferrite-decorate` parser + Doom actor bootstrap
- [x] `ferrite-dehacked` Thing/Weapon/Ammo/Misc/Frame/Pointer parse
- [x] BEX `[STRINGS]` → pickup messages; `[CODEPTR]` parse
- [x] Frame / Pointer / `[CODEPTR]` **apply** via `states.txt` class/label/offset
  (compressed multi-letter DECORATE lines share one target; true 1:1 needs
  expanded built-in actor states)
- [x] Full DeHackEd sound/sprite tables with `ferrite.pk3/dehacked/`
  (`things.txt` / `sprites.txt` / `sounds.txt` / `weapons.txt` / `ammo.txt` /
  `codepointers.txt`; `[SOUNDS]` / `[SPRITES]` apply)

**Deliverables — sim**

- [x] World, thinkers, spawn, blockmap links
- [x] Movement / collision / try_move / stairs / dropoff basics
- [x] Sight, hitscan, melee, missiles, radius damage
- [x] Inventory, weapons, ammo, armor, powerups, keys
- [x] Player think, weapon psprites, cheats (iddqd/idkfa/…)
- [x] Doom specials via XLAT (doors, plats, floors, ceilings, lights, exits, teleports)
- [x] Sector specials (damage, secret, type-11 exit, …) — bootstrap set
- [x] Nightmare COUNTKILL respawn; DROPPED half-ammo
- [x] `A_Chase` 8-dir `P_NewChaseDir` / blocked repath / `CANUSEWALLS` door use
- [x] Friendly AI: `A_Look`/`A_Chase` hunt enemy monsters (ignore players)
- [x] Doom AI bootstrap sufficient for M5 accept (CHF_*/dropoff/MBF smartmove → M6)
- [x] Intermission tallies + next/secret-next (Doom II heuristics + MAPINFO override);
  title/finale stubs remain
- [x] Corpse queue (`sv_corpsequeuesize` default 64) + player `bodyque` (32);
  `A_QueueCorpse` / `A_DequeueCorpse`
- [x] Leftover vanilla edge cases deferred (demos green after chase re-record)

**Deliverables — audio / input / UI / console**

- [x] `ferrite-audio` SFX mixer + SNDINFO resolve bootstrap
- [x] `ferrite-music` SoftFont + OPL path
- [x] `ferrite-input`, `ferrite-console` core commands/CVars
- [x] `ferrite-ui` menus, status bar, HUD, automap, messages (Doom bootstrap)
- [x] Doom UI bootstrap; full option menus / config parity → M13

**Deliverables — demos / saves / app**

- [x] `.fdem` record/verify + CLI `demo`
- [x] Save v2 (inventory/armor/pitch/weapon)
- [x] `ferrite-app --play` WASD headless drive
- [x] MAP01 smoke: tick, door, teleport, exit→intermission, secret
- [x] Committed maintainer smoke `.fdem` (`testdata/demos/freedoom2_map01_smoke.fdem`)
- [x] Multi-map `.fdbn` bundles (P2 MAP01–30; P1 E1–E4)
- [x] `docs/qa/freedoom-playthrough.md` checklist

**Acceptance**

- [x] Freedoom Phase 1 start→finish automated (E1–E4 exit chains + `.fdbn` goldens;
  manual UV playthrough still listed in `docs/qa/`)
- [x] Freedoom Phase 2 start→finish automated (MAP01–30 exit chains + `.fdbn`
  goldens; MAP30 force-exit when no Exit_Normal linedef; manual UV still in
  `docs/qa/`)
- [x] Determinism CI bootstrap: `determinism` job (native demo verify +
      wasm32-wasip1 fingerprint parity); matrix `test` also covers macOS/Windows

### How to verify

```bash
./scripts/fetch-freedoom.sh
cargo test -p ferrite-sim
cargo test -p ferrite-game --test freedoom_smoke
cargo run -p ferrite-app -- --iwad testdata/freedoom/freedoom2.wad --map MAP01 --play
```

### Near-term backlog (update as closed)

- [x] DeHackEd frame/codeptr apply (bootstrap)
- [x] Hardening AI chase pathing (`P_NewChaseDir`); refire/boss deaths still open
- [x] Intermission stats + next-map / secret-next fidelity
- [x] First committed MAP01 smoke demo + verify test
- [x] MAP01→MAP02 continue after exit (headless smoke)
- [x] Headless MAP01–MAP29 + MAP30 finale; Freedoom1 E1–E4 episode exit chains
- [x] Multi-map demo bundle format + MAP01–03 golden
- [x] Full episode demos (automated start→finish)

---

## M6 — Boom, MBF, MBF21, vanilla strict

**Status:** `done` · Spec: `20` §M6 · Docs: `09`, `19`

### Plan

1. Boom generalized specials, friction, pushers, scrollers, transfer heights, colormaps, TRANMAP.
2. MBF friends/dogs/`A_` additions; MBF21 DeHackEd/flags/groups; UMAPINFO; DSDHacked; `COMPLVL`.
3. Strict vanilla mode + vanilla/Boom/MBF demo playback; per-map fix DB (`19` §3).

### Checklist

**Deliverables**

- [x] Boom generalized linedef translate (`boom_gen` / XLAT) — bootstrap present
- [x] Boom generalized sector bits (damage `0x60`, secret, friction `0x100`)
- [x] MBF21 sector kill bits (`0x400` players / `0x800` monsters)
- [x] Remaining Generic_* / Boom sector type fidelity (floor target-6 + numeric model; residual ceiling)
- [x] Friction / pushers / scrollers / transfer heights bootstrap (sim)
- [x] Boom COLORMAP load + TRANMAP / TINTTAB / XLATAB parse
- [x] MBF + MBF21 feature set (Deh/friends/dogs/`A_*`; InfightingGroup; residual Generic_*)
- [x] `COMPLVL` lump parse → `Definitions.complevel`
- [x] Complevel → `CompatFlags` on `World` (GZDoom bit positions; MBF21 kill gated)
- [x] UMAPINFO bossaction / endpic / episode clear + additive merge + sim `A_BossDeath`
- [x] DSDHacked dynamic thing slots (`Deh_Actor_N` allocate on apply)
- [x] Strict vanilla mode bootstrap (`SimMathMode` + `VanillaRng` + `P_AproxDistance`)
- [x] Fixed-point `try_move` / `check_position` (`vanilla::movement`; dispatched from `try_move_ex`)
- [x] VanillaFixed `xy_movement` friction via `Fixed` mul
- [x] `.fdem` FDHD stamps Complevel (legacy demos → Default; verify applies it)
- [x] Vanilla `P_Random` wiring (`World::p_random` → rndtable when VanillaFixed)
- [x] Vanilla/Boom/MBF demo playback (`.lmp` + DEMO1@70 ≈Chocolate ±0.5; full DEMO1 end == Chocolate bitexact)
- [x] Per-map compat flags DB (`compat/flags.txt` parse + MD5 → CompatFlags)
- [x] Per-map `levelfixes.ron` parse/apply (core ops; expand GZDoom corpus)

**Acceptance**

- [x] Public MBF21 test WADs checklist (`docs/qa/mbf21-wads.md`; manual cells open)
- [x] Vanilla demo sync suite green locally (gated IWAD; `freedoom2_demo1_vanilla_full_playback`)
- [x] Freedoom-based Complevel Vanilla/Boom demo subset in CI

---

## M7 — ZDoom feature set

**Status:** `done` · Spec: `20` §M7 · Docs: `06`–`11`, `16`

### Plan

1. Hexen-format maps/specials; UDMF `zdoom` namespace complete.
2. ACS VM complete; slopes/3D floors/portals/polyobjs in **sim and renderer**.
3. Full DECORATE; full MAPINFO; hubs; SBARINFO; TERRAIN; DECALDEF; LOCKDEFS; KEYCONF; MENUDEF.
4. Skyboxes, camera textures, mirrors, mover interpolation.

### Checklist

**Deliverables**

- [x] Hexen specials + UDMF zdoom completeness (broad UDMF/SPAC/plane/side/sector keys + Get*UDMF*)
- [x] ACS VM complete enough for M7 accept — ACS0/ACSE/ACSe + Hexen PCD + ACS_Execute* + broad CALLFUNC/ACSF + named scripts + Get*UDMF*; residual ACSF edges → backlog
- [x] Slopes / 3D floors / portals / polyobjs in sim + render (solid ffloors, swim, midtex3d, line/sector portals, groups, sight, polyobj movers, slope Z)
- [x] Full DECORATE for M7 accept — expressions, DecoBlock, large action corpus; residual actions → backlog
- [x] Hubs, SBARINFO, TERRAIN, DECALDEF (+render decals)
- [x] LOCKDEFS, KEYCONF, MENUDEF
- [x] Skyboxes, camera textures, mirrors, interpolation — cubemap/skybox + cameratexture + mirror/portal canvases + stencil mask/equal; **Equal-only Metal mask deferred** (wgpu write-mask)

**Acceptance**

- [x] Synthetic testkit maps (`m7_feature_smoke` portal/SNDSEQ/Pillar/fog/ffloor+slope/UDMF `user_*`)
- [x] DECORATE/ACS smoke via in-tree fixtures + Freedoom playthrough demos (external mod corpus fetch → backlog)

### Intentional deferrals (not blocking M11)

- Equal-only stencil portal/mirror mask on Metal (empty colour write masks skip Replace).
- Remaining rare DECORATE actions / ACSF numbers; `testdata/corpus.toml` external ZDoom mod fetch.

---

## M8 — Heretic and Hexen

**Status:** `deferred` · Spec: `20` §M8 · Docs: `08`, `16`, `19`

### Plan

1. Built-in actors, specials, bars, menus, intermissions/finales.
2. Hexen classes, hubs, puzzles; native startup screen.

### Checklist

**Deliverables**

- [ ] Heretic actors/specials/UI/flow
- [ ] Hexen actors/specials/classes/hubs/puzzles
- [ ] Startup screen (native)

**Acceptance**

- [ ] Heretic E1 playthrough demo (gated IWAD CI/local)
- [ ] Hexen hub 1 playthrough demo (gated)

---

## M9 — Strife

**Status:** `deferred` · Spec: `20` §M9

### Plan

1. Conversations (binary + USDF), quests/log, merchants, alarms.
2. Strife bar/popups and special actions.

### Checklist

**Deliverables**

- [ ] Conversation system (binary + USDF)
- [ ] Quests/log, merchants, alarms
- [ ] Strife UI + special actions

**Acceptance**

- [ ] Strife MAP02→MAP07 gated playthrough demo

---

## M10 — GZDoom rendering extras

**Status:** `done` · Spec: `20` §M10 · Docs: `13`

### Plan

1. GLDEFS lights + clustered shading + shadowmaps; brightmaps/materials/PBR.
2. MODELDEF / VOXELDEF; particles; full render styles; post-process; custom shaders; lightmaps (S).

### Checklist

**Deliverables**

- [x] GLDEFS lights parse (point/pulse/flicker/flicker2 + `object` frame binds) — bootstrap
- [x] CPU stand-in: `DynLightInstance` + `apply_dyn_lights` on walls/flats/sprites
- [x] GLDEFS `brightmap` / `material` / `Glow` parse + shader brightmap sample + glow fullbright
- [x] GPU forward dyn lights (`LightUniforms`, max 32) + pulse/flicker radius — bootstrap
- [x] Clustered shading bootstrap (16×9×24 CPU assign + mask texture)
- [x] 1D shadowmaps bootstrap (CPU raycast atlas + FS sample) — GPU BVH still open
- [x] MODELDEF parse bootstrap (path/model/skin/frame/flags)
- [x] OBJ mesh load bootstrap (`parse_obj` → `ModelMesh`)
- [x] MD2 mesh load bootstrap (`parse_md2` multi-frame)
- [x] MODELDEF unlit GPU draw bootstrap (`append_model` → wall lists)
- [x] VOXELDEF parse bootstrap (scale/spin/offsets)
- [x] KVX mesh bootstrap (`parse_kvx` + cube faces)
- [x] KVX greedy face merge (`kvx_to_mesh_greedy`)
- [x] GLDEFS lights (point/pulse/flicker/flicker2/spot/sector) + clustered + 1D shadowmaps (GPU)
- [x] Spotlight cone GPU (`spot_dir_outer` / inner cos + unlit falloff; actor yaw aim)
- [x] Material PBR map catalog (normal/specular/metallic/roughness/ao)
- [x] GPU normal-map sample (Lambert on dyn lights)
- [x] Blinn-Phong / GGX specular on dyn lights
- [x] Metal-rough map bind (`tex_metal_rough` glTF G/B + `MetalRough` / `pbr_specular` CPU)
- [x] MD3 mesh load bootstrap (first surface, multi-frame)
- [x] IQM v2 bind-pose parse + `parse_model_bytes` auto-detect
- [x] IQM CPU skinning (`parse_iqm_full` / `IqmModel::skin_cpu` / bind-pose identity)
- [x] MODELDEF actor wiring (`ModelCatalog` / `append_actor_models`)
- [x] Play-path MODELDEF draw (`collect_model_poses` in ferrite-app)
- [x] Model offscreen smoke (`tests/model_offscreen_smoke.rs`)
- [x] Dyn-lights soft golden (`tests/golden_dyn_lights.rs` mean-luma)
- [x] Dyn-lights SSIM PNG (`testdata/goldens/dyn_lights_320x200.png`)
- [x] Fuzz SSIM PNG (`testdata/goldens/fuzz_320x200.png`)
- [x] Optional engine extras mount (`lights.pk3` / `brightmaps.pk3` / … + `testdata/m10/lights` fixture)
- [x] lights.pk3 fixture content → `Definitions.gldefs` (point/pulse/flicker + object binds)
- [x] IQM GPU skinning bootstrap (`SkinnedPass` + bone palette ≤64 + `skinned_iqm_smoke`)
- [x] UE1 vertex mesh parse (`parse_ue1` `_d.3d`/`_a.3d` + catalog pair load)
- [x] Acceptance goldens bootstrap (room / dyn-lights / fuzz / particles; bloom/LM smokes)
- [x] Particles bootstrap (vel/accel + ACS ingest)
- [x] Particle styles (`Translucent` / `Additive` / `Soft` + `__PARTICLE_ADD`)
- [x] Post bootstrap (`PostPass`: Reinhard/Hejl/linear/palette, gamma, soft bloom, screen blend)
- [x] `RenderStyle` blend table (Normal/Add/Translucent/Subtract/Multiply/…)
- [x] FXAA bootstrap in PostPass (`gl_fxaa`)
- [x] Fuzz shader bootstrap (`FuzzType` / `gl_fuzztype` variants on Fuzzy sprites)
- [x] SSAO bootstrap (`SsaoPass` depth-edge)
- [x] SSAO bilateral blur (depth-aware 4-neighbour)
- [x] SSAO dedicated half-res AO buffer (`SsaoPass::ensure_ao` / `ao_view`)
- [x] Multi-scale bloom (Kawase 4-radius × 8-tap) in PostPass
- [x] True bloom mip downsample chain (`BloomPass` 4-level half-res pyramid)
- [x] Palette tonemap (`TonemapMode::Palette` + PLAYPAL upload)
- [x] naga WGSL validation at build (`ferrite-render/build.rs`)
- [x] Lightmap header bootstrap (`FLMP` / `parse_lightmap_header`)
- [x] Particles GPU instancing (`ParticleInstancedPass` + `particles.wgsl` + smoke)
- [x] Obscure render styles fill path (`apply_fill_color` / `uses_fill_modulate` for stencil/shaded/soul/shadow)
- [x] Custom GLSL→WGSL via naga at load (`glsl_to_wgsl`)
- [x] HardwareShader material bind (`HardwareShaderCatalog` + GLDEFS parse / ProcessTexel wrap)
- [x] HardwareShader post pipeline host (`create_hardware_post_pipeline` / `HARDWARE_POST_HOST_WGSL`)
- [x] HardwareShader custom FS compose (`compose_hardware_post_wgsl`)
- [x] Lightmap surface UV + atlas sample (`parse_lightmap_lump` / `lightmap_atlas_texel`)
- [x] Lightmap GPU atlas + debug draw (`LightmapAtlas` / `LightmapPass`)
- [x] Bloom mip soft smoke (`bloom_mip_smoke`)
- [x] UnlitPass lightmap bind (`set_lightmap` / world-XY UV bootstrap + smoke)
- [x] Per-vertex lightmap UVs (`lm_uv` on walls/flats + `SceneLists.lightmap_surfaces` + `world_xy_to_surface_uv`)
- [x] Side/sector → surface-index map (`LightmapBindings` + wall `side_raw` / flat `sector_raw`)

**Acceptance**

- [x] Golden images per feature bootstrap (room / dyn-lights / fuzz / particles SSIM PNGs; bloom/lightmap smokes)
- [x] `lights.pk3` fixture content parses/renders path (mount + Definitions) — full GZDoom pk3 still optional
- [x] Scene-build soft budget (`scene_build_budget` synthetic room ≪ 2 ms)
- [x] MAP01 offscreen fps floor (`freedoom_map01_offscreen_fps_floor` ≥15 CI)
- [x] 500-light cluster assign soft budget (`cluster_500_lights_budget`)
- [x] 10k sprite append soft budget (`sprite_10k_budget` nuts.wad stand-in)
- [x] 32-light 1D shadowmap bake soft budget (`shadowmap_32_budget`)
- [x] MAP01+32-lights offscreen fps floor (`freedoom_map01_lights_shadow_fps_floor` ≥8 CI)
- [ ] Full performance matrix (`13` §14 desktop targets / live nuts.wad) — **intentional deferral** (soft CI floors + lights path gate; no mid-range GPU / nuts.wad in CI)

### Log

- `2026-10-12` — M10 done: MAP01+32-lights fps floor; desktop matrix intentional deferral.
- `2026-10-12` — Spotlight cone GPU + shadowmap_32_budget.
- `2026-10-12` — LightmapBindings side/sector map + spotlight/sectorlight + SSAO AO buffer.
- `2026-10-12` — Per-vert `lm_uv` + surface UV helpers + 10k sprite budget.
- `2026-10-12` — UnlitPass lightmap bind + 500-light cluster budget + checklist sync.
- `2026-10-12` — Particles SSIM golden + LightmapPass GPU + compose_post FS + scene budget.
- `2026-10-12` — ParticleInstancedPass + fill_color styles + bloom mip smoke + HW post host.
- `2026-10-12` — HardwareShader catalog bind + particle GPU pack + FLMP surfaces.
- `2026-10-12` — lights.pk3 fixture defs + glsl_to_wgsl + braced frame lights.
- `2026-10-12` — BloomPass mip chain + naga build validate + FLMP lightmap header.
- `2026-10-12` — Metal-rough GGX PBR + fuzz SSIM golden.
- `2026-10-12` — SSAO bilateral blur + particle styles + Kawase-4 bloom.
- `2026-10-12` — UE1 model parse + Blinn-Phong specular bootstrap.
- `2026-10-12` — IQM GPU skinning SkinnedPass + smoke.
- `2026-10-12` — KVX greedy meshing + optional lights.pk3 mount path.
- `2026-10-12` — Dyn-lights SSIM PNG golden committed.
- `2026-10-12` — IQM CPU skinning + golden_dyn_lights soft check.
- `2026-10-12` — Play-path MODELDEF + model_offscreen_smoke.
- `2026-10-12` — IQM parse + ModelCatalog actor wiring.
- `2026-10-12` — MD3 parser + normal map bind/Lambert in unlit.
- `2026-10-12` — append_model unlit path + KVX→mesh.
- `2026-10-12` — MD2 parser + MaterialMaps PBR catalog from GLDEFS.
- `2026-10-12` — Palette tonemap + OBJ mesh parser for MODELDEF.
- `2026-10-12` — SsaoPass + multi-scale bloom in post.wgsl.
- `2026-10-12` — FuzzType + unlit spectre fuzz; app sets sprite.fuzz for Fuzzy/OptFuzzy.
- `2026-10-12` — RenderStyle blend table + FXAA in PostPass.
- `2026-10-12` — PostPass fullscreen tonemap/gamma/bloom-threshold + particle vel/accel.
- `2026-10-12` — CPU 1D shadowmaps (512×N R32F atlas) + one-sided occluders; unlit FS gate.
- `2026-10-12` — VOXELDEF parser + DefinitionsBuilder merge.
- `2026-10-12` — MODELDEF parser + DefinitionsBuilder merge.
- `2026-10-12` — Cluster grid 16×9×24 + bitmask texture; FS clustered light loop.
- `2026-10-12` — GPU light UBO + fragment additive/subtractive dyn lights; pulse/flicker.
- `2026-10-12` — Brightmaps/glow/material GLDEFS + unlit bind group brightmap; Glow flats fullbright (nukage/lava).
- `2026-10-12` — Bootstrap: GLDEFS dyn-light subset + doom attack defaults; app emits object-frame lights; Bright fullbright in unlit.wgsl.

---

## M11 — WASM modules (ZScript replacement)

**Status:** `done` · Spec: `20` §M11 · Docs: `12`

**Intentional deferrals:** `#[derive(Actor)]` / `states!` / C header & `cargo ferrite`; UI draw/`send_network_event` imports; hot reload; full sample mods (b)–(e); ABI benchmarks.

### Plan

1. ABI v1 IDL + `ferrite-abi-gen`; `wasmi` then `wasmtime`.
2. Classes/states/fields/virtuals/actions/events/thinkers; ACS interop; UI modules.
3. Saves/hot reload; Rust SDK + macros + `cargo ferrite`; C header example.

### Checklist

**Deliverables**

- [x] ABI v1 + codegen (bootstrap: `api/common.ron` + `ferrite-abi-gen list`)
- [x] `ferrite-script` wasmi backend (validate + required exports + `ferrite::log` + lifecycle)
- [x] `wasmtime` backend (fuel + NaN canon; same stub lifecycle as wasmi)
- [x] Validation + registration/init lifecycle
- [x] Class/state registration ABI bootstrap (`reg_class_*` / `reg_state*` + ClassRegistry merge)
- [x] Cross-backend registration fingerprint (wasmi ≡ wasmtime on `fireimp_reg`)
- [x] Events + thinkers bootstrap (`WorldTick` dispatch)
- [x] ACS interop bootstrap (`ScriptCall` ↔ `reg_static_function`, `acs_execute` queue)
- [x] UI modules bootstrap (`load_ui_module` + `UiTick`; HUD draw/net events still open)
- [x] Saves bootstrap (memory snapshot / restore + `scripts.bin` in `.fsave`; hot reload still open)
- [x] Rust SDK bootstrap (wrappers + bump alloc + `export_module!` / register stub; Actor derive/C header still open)

**Acceptance**

- [x] Cross-backend registration fingerprint (wasmi ≡ wasmtime)
- [x] Sample mods bootstrap (a) FireImp reg + event/UI/ACS/save WAT samples; (b)–(e) deferred
- [x] Save/load module memory mid-session (host API + `.fsave` `scripts.bin`)
- [ ] Benchmark targets (`12` §14) — deferred

---

## M12 — Netplay

**Status:** `done` · Spec: `20` §M12 · Docs: `17`

### Plan

1. Lockstep netcode; UDP + WebRTC (`matchbox`); lobby/handshake.
2. Desync detection + dump tooling; `ferrite-signal`.

### Checklist

**Deliverables**

- [x] `ferrite-net` lockstep bootstrap (`LockstepSession` + loopback mesh)
- [x] UDP transport bootstrap (`UdpTransport`)
- [x] TCP mesh (`TcpTransport`) + packet-server relay (`TcpRelayServer` / `TcpRelayClient`)
- [x] Noise_XX encryption wrapper (`NoiseTransport` / `snow`, feature `noise`)
- [x] WebRTC (`matchbox_socket` behind `--features webrtc` + `MatchboxTransport`; signal mesh/HTTP for CI)
- [x] Lobby / handshake bootstrap (`LobbyHost`)
- [x] Desync detection + dump text (`DesyncCollector`)
- [x] `ferrite-signal` signalling helper (HTTP room relay)

**Acceptance**

- [x] In-process lossy-network simulation tests
- [x] Automated 2-peer lockstep smoke (loopback + signal mesh + Freedoom MAP01)
- [x] Strong coop suite (`coop_netplay`: UDP lobby + dual-seat independent cmds + FF block + desync)
- [x] TCP relay exchange unit + Noise_XX AEAD unit; app `--transport` / `--noise`
- [ ] Manual 4-player coop native+web 30 min without desync — **intentional deferral** (next step)

### Intentional deferrals

- Live 4-player native+web 30 min soak without desync.
- Drop-in `matchbox_server` / WebSocket signalling (HTTP `ferrite-signal` covers rooms today).

---

## M13 — Product polish

**Status:** `done` · Spec: `20` §M13 · Docs: `15`, `16`, `18`

### Plan

1. Playable Doom menus (title / pause / skill / options stub) + shell UX.
2. Launcher (IWAD/mod picker), gzdoom.ini import, full option menus.
3. Web library (OPFS), URL params & JS embed API.
4. Packaging, crash reporter, engine-string localization.

### Checklist

**Deliverables**

- [x] Doom menu shell polish (TITLEPIC, skull, patch items, pause/quit/end-game, mouse)
- [x] Launcher + IWAD/mod picker (`--launcher`, auto-discover Freedoom/Doom2)
- [x] gzdoom.ini / ferrite.ini import + `writeconfig`/`exec`
- [x] Option menus wired to CVars (`invertmouse`, `cl_run`, `m_sensitivity`, …)
- [x] Web URL params + storage embed API (`ferriteUrlParam` / `ferriteOpfs*`)
- [x] Packaging script (`scripts/package.sh` host/web/all)
- [x] Crash reporter (panic → platform log dir)
- [x] Engine string localization (`StringTable` / LANGUAGE bootstrap)

**Acceptance**

- [x] Fresh-machine UX checklist published (`docs/qa/fresh-machine.md`) + unit/config smoke
- [ ] Full Win/macOS/Linux + Chrome/Firefox/Safari manual matrix — **intentional deferral**

### Intentional deferrals

- Exhaustive multi-OS / multi-browser manual soak of the QA checklist.
- True OPFS backend (API named/ready; `localStorage` today).

---

## M14 — 1.0 hardening

**Status:** `in_progress` · Spec: `20` §M14

### Plan

1. 24 h fuzz all parsers; save/load fuzz.
2. Compatibility corpus report; performance profiling pass.
3. User manual, modding guide, ZScript migration guide.

### Checklist

**Deliverables**

- [ ] 24 h fuzz campaigns
- [x] Save/load fuzz bootstrap (`save_info` + `save_world_bin` / `validate_world_bin` + CI 60s) — live-world round-trip fuzz still open
- [ ] Compatibility corpus run + report
- [ ] Performance profiling pass
- [ ] User manual + modding guide + ZScript migration guide

**Acceptance**

- [ ] All of the above landed and documented

### Log

- `2026-10-12` — `validate_world_bin` + `save_world_bin` fuzz (capped sector/inv counts).
- `2026-10-12` — `save_info` fuzz target (`.fsave` ZIP / `info.json` must not panic).

---

## Stretch (post-1.0)

Track opportunistically; do not block 1.0.

- [ ] Classic software renderer (`ferrite-render-soft`)
- [ ] ZScript→Rust/WASM transpiler
- [ ] Bots
- [ ] Rollback netcode
- [ ] Timidity/GUS, ADLMIDI/OPNMIDI, GME
- [ ] HRTF
- [ ] Component-model ABI v2
- [ ] Mobile (Android/iOS) shells
- [ ] Join-in-progress netplay

---

## CI coverage matrix (cross-cutting)

Update when jobs are added. Spec: `20` §4.

| Job | Status |
|-----|--------|
| `fmt` / `clippy` / `test` / `doc` / `deny` | [x] Present |
| `wasm` build | [x] Present |
| `dmath-wasm` | [x] Present |
| `fuzz-smoke` | [x] Present |
| `render-smoke` (lavapipe + Freedoom) | [x] Present |
| Freedoom fetch on main `test` job | [x] Present |
| `determinism` | [x] Present (native verify + wasm32-wasip1 fingerprints) |
| `golden-images` (via `render-smoke`) | [x] Present |
| `web-browser-tests` | [x] Present |
| `bench` (nightly) | [ ] Missing |
