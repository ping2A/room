# 26 — Face swap: your face (or a photo) on enemy heads

> **Audience:** a coding agent that will implement this. Read `AGENTS.md`,
> `docs/01-architecture.md`, `docs/02-conventions.md`, `docs/13-renderer.md`
> and `crates/room-web/README.md` (editor section) first.
>
> Goal: turn the current one-frame **Camera face…** paste in the web
> editor into a proper feature. A player captures their face with a camera,
> or imports an existing photo of a head, and sees it on enemy sprites across
> **every frame and rotation**, lit and palette-matched so it looks like part
> of the game. The result can be baked into a shareable PWAD/PK3 or applied
> live at runtime. It works for Doom/Freedoom and for Duke Nukem 3D content.

---

## 0. TL;DR

| Today | Target |
|-------|--------|
| Webcam only (`editor-paint.js` `openCameraFace`) | Webcam **or** image file / drag-and-drop / paste. Desktop gets file import, and optionally a camera |
| Centre crop of the video frame, no face detection | Local face detection + 5-point landmarks, auto alignment, manual eye-point refinement |
| Pasted at a fixed spot (`upper-centre, 6% from top`) | Per-frame, per-rotation **head anchors** (ellipse + eye line + view yaw) from a data lump, an auto-estimator, and an anchor editor |
| One lump edited at a time | One action applies to every frame/rotation of a class (or a set of classes) |
| Flat paste, nearest-palette quantize | Lighting and colour transfer from the original head, feathered mask, outline, palette quantize with optional dither |
| Front view on every angle | Front / three-quarter / profile / back handling (side and back captures optional) |
| Editor-only, saved as `SS_` override lumps | **Bake** (editor + `room-cli`, exports a face pack PK3) **and** **Live** (game option, render-only, can use a live webcam) |
| No consent or storage rules | Local-only processing, explicit consent for photos of other people, opt-in sharing in multiplayer, one-click "forget all faces" |

New crate **`room-faceswap`** holds everything that isn't UI or device
access (pure Rust, no GPU, no I/O), so the web editor, the desktop app,
`room-cli` and the renderer share one implementation.

---

## 1. Current state (what you inherit)

- `crates/room-web/editor-paint.js`:
  - `openCameraFace()`: modal with a `<video>`, a circular guide, two modes
    (`face` = circular paste, `full` = replace whole frame), a size slider
    (35–90 %), Capture/Cancel, Esc/Enter keys. It requests
    `getUserMedia({ video: { facingMode: "user", 720×720 } })` and stops
    the tracks on close.
  - `applyCameraSource(src, {mode, faceScale, mirror})`: snapshot (mirrored),
    centre square crop, paste clipped to a circle at
    `dx = (w − diam)/2, dy = 0.06·h`, then `applyImageData` →
    `nearest()` palette quantize → lump override via `roomEditorImagePut`.
  - `importPng()`: replaces the *whole* picture (no face mode).
- `crates/room-web/editor.js:1475`: thing inspector "Camera face…" opens
  the paint tab of the thing's **current sprite lump** with
  `{ camera: true }`.
- `crates/room-web/src/browser/editor.rs`: `roomEditorImageGet` /
  `roomEditorImagePut` (palette indices + alpha, live swap into the view).
- `crates/room-web/voice.js`: already handles `getUserMedia` pitfalls
  (it drops pointer lock, so don't retry in a loop). Reuse that knowledge.
- Renderer: `room-render/src/texture.rs` `TextureManager::upsert_rgba` /
  `upsert_copy`. Camera-to-texture already exists for Duke viewscreens.

Keep the existing dialog working until F3 replaces it (§11).

---

## 2. Scope

### In scope

1. **Face sources:** front camera (web, desktop optional), image file
   (PNG/JPEG/WebP; HEIC only if the browser decodes it), drag-and-drop,
   clipboard paste. Several faces kept in a local **face gallery**.
2. **Targets:** any sprite-based actor. Ship anchor data for:
   - Doom / Doom II: `POSS`, `SPOS`, `CPOS`, `TROO`, `SARG` (and spectre),
     `HEAD` (cacodemon, special: the face is the body), `SKUL`, `PAIN`,
     `BOSS`/`BOS2`, `SKEL`, `FATT`, `BSPI`, `CYBR`, `SPID`, `VILE`,
     `SSWV`, `KEEN`, player `PLAY`.
   - Freedoom 1/2 equivalents. The art differs, so anchors are keyed by
     content hash (§4.1), not just lump name.
   - Duke Nukem 3D (`BTnnnn` tiles from `room-build`, and the native
     runtime from `docs/25` later): `LIZTROOP`, `PIGCOP`, `LIZMAN`,
     `COMMANDER`, `OCTABRAIN`, `NEWBEAST`, `BOSS1`–`BOSS4`, `APLAYER`.
3. **Assignment:** per class, per instance (random from a set, seeded),
   "player N's face on everything", bosses only, etc.
4. **Bake** to a PWAD/PK3 and **Live** runtime overlay (§7).
5. **Multiplayer sharing** (opt-in, §8).

### Out of scope

- Face *recognition* or identification of any kind. We detect a face and
  find landmarks only. No embeddings, no matching against other faces.
- Deepfake-style video synthesis, expression transfer, 3D head
  reconstruction.
- Models (MD2/MD3/IQM) and voxels. The spec only covers 2D sprites and tiles.
  The same anchor idea could apply to model skins later.
- Uploading faces to any server, or storing them server-side.

---

## 3. Face capture and import

### 3.1 Sources

| Source | Web | Desktop (`room-app`) |
|--------|-----|-----------------------|
| Camera | `getUserMedia` (existing) | Optional feature `camera`, behind a CVar. Crate choice is a decision (§12): it must use OS APIs only (AVFoundation / Media Foundation / V4L2) with no `cc` build (Appendix A rules) |
| File | `<input type=file accept="image/*">` | `rfd` file dialog (already allowed) |
| Drag-and-drop | `dragover`/`drop` on the dialog | winit `DroppedFile` |
| Paste | `paste` event (`clipboardData.items` image) | `arboard` image (already allowed) |

Decode with `createImageBitmap` on the web (strips EXIF orientation
correctly with `imageOrientation: "from-image"`) and `png`/`zune-jpeg`/
`image-webp` on desktop. **Apply the EXIF orientation, then drop all
metadata.** Only pixels go into `room-faceswap`.

### 3.2 Improved camera dialog (web)

Replace `openCameraFace` with a `face-capture.js` module used both by the
paint tab and by a new **Faces** panel (editor and play settings):

- **Device picker** (`enumerateDevices`, after permission), resolution
  choice, mirror toggle (preview mirrored, capture unmirrored by default,
  with a "mirror result" option).
- **Live detection overlay:** box + 5 landmarks drawn over the video,
  running at ≤ 10 Hz on a downscaled frame in a Web Worker (wasm
  `room-faceswap` build). Status line: "No face", "Move closer",
  "Too dark", "Hold still", "Ready".
- **Auto-capture:** when a face has been stable (landmark jitter below a
  threshold) for 0.5 s, count down 3-2-1 and take a **burst of 5** frames.
  Keep the sharpest one (Laplacian variance on the face crop). Manual
  Capture stays.
- **Guided multi-angle capture** (optional step): "Face the camera" →
  "Turn your head left" → "Turn right" → "Show the back of your head"
  (skippable). These fill the side and back views (§5.3). Show a small
  head diagram for each step.
- **Live sprite preview:** while the camera runs, show the target class
  with the composited face, cycling rotations 1–8 and the walk frames.
  The user sees the result *before* capturing. Uses the same compositor
  as the bake, at preview quality.
- **Adjust step after capture:** draggable eye points + chin point over the
  still, sliders for size, vertical offset, brightness, contrast,
  saturation, "keep sprite hair/helmet" (mask shape), and dither on/off.
- Accessibility: every control is keyboard reachable. Esc closes. The
  camera stops as soon as the dialog closes or the tab is hidden
  (`visibilitychange`). Restore pointer lock and focus afterwards (see
  `voice.js`).
- Error states: no camera, permission denied (explain how to re-enable it
  in browser settings, and offer **Import a photo** instead), the camera is
  used by another app, insecure context (camera needs HTTPS or localhost).

### 3.3 Importing an existing photo of someone

Same pipeline as the camera (detect → align → adjust):

- Run detection on the full image. If **several faces** are found, show
  them all with boxes and let the user pick one. If **none** are found,
  fall back to manual placement (click both eyes and the chin).
- A **consent step** is required when the source isn't the user's own live
  camera: a checkbox *"This is me, or I have this person's permission to use
  their photo"*. Import is blocked until it's ticked. Store the answer with
  the face entry (`consent: "self-camera" | "attested"`). The checkbox is
  not a legal mechanism. It's a deliberate pause that keeps the feature
  for fun with friends.
- Low resolution warning when the face crop is under 48 px wide (it will
  still work; sprite heads are often ~12–30 px).

### 3.4 Face detection and landmarks

`room-faceswap::detect` returns `Vec<FaceDetection { bbox, landmarks: [Pt; 5]
(left eye, right eye, nose, mouth left, mouth right), score }>`.

Recommended implementation (decision in §12):

- **Option A (recommended):** a small ONNX face detector with 5-point
  landmarks (e.g. YuNet from the OpenCV model zoo, ~300 KB) run with
  `tract-onnx` (pure Rust, works on wasm32). Check the model licence
  **(verify)** and record it in `THIRD_PARTY.md`. The model file ships as a
  web asset loaded on first use. It is not compiled into the main wasm.
- **Option B:** `rustface` (pure-Rust SeetaFace port, box only) + eye
  localisation by a heuristic (darkest blobs in the upper box half). Smaller
  but less robust.
- **Always available fallback:** manual points. The feature must work with
  detection disabled or failed.
- The browser `FaceDetector` (Shape Detection API) may be used as an
  accelerator only when present. It is not available everywhere, so never
  depend on it.

Detection runs locally. No network request is made during capture,
detection or compositing (test in §10).

### 3.5 Normalised face asset (`FaceProfile`)

After alignment, every face is stored in a canonical form:

```rust
pub struct FaceProfile {
    pub id: FaceId,                 // random u128, not derived from the image
    pub name: String,               // user label, "Me", "Alex"
    pub front: FaceView,            // required
    pub left: Option<FaceView>,     // three-quarter/profile captures
    pub right: Option<FaceView>,
    pub back: Option<FaceView>,
    pub tone: Rgb,                  // mean skin colour (neck/edge blending)
    pub hair: Rgb,                  // mean colour above the forehead (back-of-head fallback)
    pub consent: Consent,
    pub created: u64,               // local time, for the gallery only
}
pub struct FaceView {
    pub rgba: Image,                // 128×128, eyes at canonical positions
    pub mask: AlphaMask,            // feathered ellipse, user-adjustable
}
```

Canonical alignment: similarity transform (scale + rotation + translation)
mapping the detected eye centres to (40, 52) and (88, 52) in the 128×128
crop, chin near y = 118 **(tune)**. Store the full 128×128 crop. Sprite
heads are small, but the extra resolution helps the colour transfer and
future high-res sprites.

---

## 4. Head anchors

A face needs to know *where the head is* on each frame. That is the core of
the feature and what today's fixed upper-centre paste lacks.

### 4.1 Data model

Per sprite **lump** (frame + rotation):

```rust
pub struct HeadAnchor {
    pub center: (f32, f32),     // in lump pixels
    pub radius: (f32, f32),     // ellipse half-axes
    pub roll: f32,              // head tilt in degrees (eye line)
    pub yaw: HeadYaw,           // Front | ThreeQuarterLeft | ProfileLeft | Back | ProfileRight | ThreeQuarterRight
    pub visible: Visibility,    // Full | Partial(mask) | Hidden   (death/gib frames, arm in front)
    pub keep_top: f32,          // 0..1: fraction of the ellipse top kept from the sprite (helmet/hair/horns)
}
```

Anchors are keyed by **lump name + xxhash64 of the lump's decoded pixels**.
A replacement sprite from a mod (or Freedoom vs Doom) doesn't silently use
anchors drawn for different art. On a hash mismatch the auto-estimator
(§4.3) runs instead.

### 4.2 `HEADANCHORS` text lump

Parsed with `room-sc` like other text lumps. Shipped defaults live in the
engine resource WAD (`wadsrc/`). Mods may add or override entries.

```
// HEADANCHORS — one block per lump
Anchor "TROOA1"  hash 0x9f3c12aa77e01b42
{
    Center 21, 9
    Radius 7, 8
    Roll 0
    Yaw Front
    KeepTop 0.25        // keep the imp's brow spikes
}
Anchor "TROOA2A8" hash 0x…  { Center 19, 9  Radius 6, 8  Yaw ThreeQuarterLeft }
Anchor "TROOI0"   hash 0x…  { Hidden }       // gibbed
Class "DoomImp" { Exclude "TROOU0" }        // optional class-level overrides
```

Rules: mirrored rotations (`A2A8`) store the anchor for the *first* name. The
mirrored one is derived by flipping x and yaw. Unknown keywords warn, not
fail (Room text-lump policy, `docs/05`). It needs a fuzz target.

### 4.3 Auto-estimator (fallback and authoring aid)

`room-faceswap::estimate_anchor(lump, class_hint)`:

1. Take the alpha silhouette. Find the topmost rows, then the width
   profile. The "neck" is the first local minimum of width below the top
   bump.
2. The head ellipse = the region above the neck, clipped to ≤ 35 % of the
   sprite height.
3. Yaw from the rotation digit (1 = front, 2/8 = three-quarter, 3/7 =
   profile, 4/5/6 = back). Duke views follow the `room-build` rotation
   mapping.
4. Track across frames of one rotation: smooth the centre (median of
   neighbours) to avoid jitter in the walk cycle.
5. Mark `Hidden` when the silhouette area changes by > 60 % from the
   class's first frame (deaths, gibs) or the frame belongs to a `Death` /
   `XDeath` state with frame index ≥ 2 **(tune)**.

It's good enough for most bipeds. Shipped `HEADANCHORS` cover the stock
monsters, and the estimator handles mods.

### 4.4 Anchor editor

In the web editor sprite tab, add an **Anchors** mode: an ellipse gizmo
(drag the centre, edges and rotation handle), a yaw dropdown, a visibility
toggle, and onion-skinning of the previous/next frame. "Copy to all frames
of this rotation" and "Estimate" buttons. **Export** writes a
`HEADANCHORS` lump into the PWAD. That's how the shipped defaults get
authored. Write them with this tool for Freedoom first, which can be
committed (BSD licence). Doom/Duke anchors carry only coordinates and hashes,
no art, so they're safe to commit too.

---

## 5. Compositing (`room-faceswap::composite`)

Input: original lump (palette indices + alpha, or RGBA for true-colour
content), `HeadAnchor`, `FaceProfile`, palette (PLAYPAL/Build palette),
`CompositeOptions`. Output: a new lump of the **same size and offsets**
(`left`/`top` unchanged, so the actor doesn't shift in game).

### 5.1 Pipeline

1. **Pick the face view** by yaw (§5.3) and warp it with an affine
   transform: canonical eye line → anchor ellipse (centre, radius, roll).
   Use bilinear sampling with premultiplied alpha at 4× supersampling, then
   box-downsample. Sprite heads are tiny, so this is where quality comes
   from.
2. **Mask:** face mask ∩ anchor ellipse, minus the `keep_top` band (keeps
   helmets, horns, hair) ∩ the sprite's own alpha (never paint outside the
   silhouette, unless `allow_overflow` for very small heads, max 1 px).
3. **Colour transfer:** compute mean and stdev of luminance and chroma (in
   Oklab) of the *original* head region and of the face. Match the face's
   statistics to the original's, with a strength slider (default 0.7).
   Then **re-apply the sprite's shading:** multiply face luminance by the
   original head's low-pass luminance divided by its mean. The light
   direction painted into the sprite (top-lit imp, side-lit zombieman)
   carries over.
4. **Edge blend:** feather 1 px into the original. Optionally redraw a
   1-px dark outline where the original had one (detect by the darkest
   palette ramp at the silhouette edge).
5. **Posterise + quantize** to the target palette: nearest colour in Oklab
   (not RGB like today's `nearest()`), restricted to colours already used
   in the sprite plus skin ramps (keeps the sprite's look). Optional ordered
   dither (4×4 Bayer) for larger heads. Never produce index 247 (Doom's
   transparent sentinel in some tools) or the content's transparent index.
6. **Fullbright and translations:** if the head region used fullbright
   colours (palette indices the `COLORMAP` keeps bright), map them to
   non-fullbright ones, or the face glows in the dark. A sprite with a
   player translation keeps its translation range untouched. Faces avoid
   the green ramp (112–127 in Doom) **(verify range)** so player colours
   don't recolour the face.
7. **Duke:** quantize to the Build palette. Avoid the colours that
   `LOOKUP.DAT` palette swaps remap, or pick per-`pal` variants (a pal-21
   trooper captain would turn your face red otherwise). Simplest: bake one
   variant per used `pal` and register them as separate tiles.

### 5.2 Determinism and caching

The compositor is pure and deterministic (same inputs → same bytes on every
platform). It uses no `f32` transcendental functions from `std` (use `libm`),
so baked packs are reproducible and multiplayer peers that share a face
produce identical pixels. Cache key: `(lump hash, anchor hash, face id +
revision, options hash)`.

### 5.3 Non-front views

| Yaw | Uses | Fallback |
|-----|------|----------|
| Front (rot 1) | `front` | — |
| Three-quarter (2/8) | `left`/`right` if captured | `front` horizontally compressed to 80 % and shifted toward the facing side, far half darkened 15 % |
| Profile (3/7) | `left`/`right` | `front` compressed to 55 %, mostly the near half, plus `tone` fill. Option: keep the original sprite head |
| Back (4/5/6) | `back` | Fill the head ellipse with `hair` colour shaded like the original, or keep the original (user option, default: keep original) |

---

## 6. Assignment rules

`FaceAssignment` (saved with the face pack and in the live settings):

```toml
[[rule]]
classes = ["ZombieMan", "ShotgunGuy", "ChaingunGuy"]   # DECORATE class names (Duke: Duke* classes / tile names)
faces   = ["me", "alex"]                                 # gallery ids
pick    = "per-instance"                                 # "per-class" | "per-instance" | "fixed"
[[rule]]
classes = ["Cyberdemon", "SpiderMastermind"]
faces   = ["alex"]
```

- `per-instance` picks with a hash of `(actor spawn index, map name)`,
  **not** the sim RNG, so the playsim RNG and checksums are untouched.
- In Bake mode, per-instance needs a variant sprite per face. Bake emits
  `NNNN`-renamed sprite sets plus a generated DECORATE `replaces`/random
  spawner. In Live mode no sim change is needed (§7.2), which is why Live
  is the better fit for per-instance.

---

## 7. Delivery modes

### 7.1 Bake (editor + CLI)

- Web editor: **Faces** panel → choose face(s) + classes → **Bake**.
  Progress per class. The result is written as override lumps into the
  current PWAD (as the paint tab does) and can be exported as
  `facepack-<name>.pk3`.
- CLI: `room-cli faceswap bake --iwad doom2.wad --face me.png
  --classes DoomImp,ZombieMan -o faces.pk3` (manual eye points via
  `--eyes x1,y1,x2,y2` when detection is off).
- The PK3 contains `sprites/*.png` (palette-indexed PNG with `grAb`
  offsets), an optional generated `DECORATE`, and a `FACEPACK.txt` with
  the generator version. It contains **no** source photos and no metadata
  about the person.
- Budget: baking all Doom II monsters (all frames, all rotations) with one
  face ≤ 3 s on wasm32 in a worker, ≤ 1 s native.

### 7.2 Live (runtime overlay, render-only)

A game option ("Faces on enemies") that composites at load time, with no
PWAD needed:

- **Static faces:** when a level loads (or the setting changes), run the
  compositor for every sprite lump of the targeted classes and register the
  results as **render-side texture overrides** (`TextureManager::upsert_*`
  keyed by `(lump, face id)`). `SimView` already tells the renderer which
  actor uses which sprite frame. Add a renderer-only lookup
  `face_for_actor(actor_id) → Option<FaceId>` computed from the assignment
  rules (hash of spawn index, §6).
- **Live webcam faces (optional, fun mode):** the camera keeps running. Each
  ~100 ms the worker aligns the latest frame and updates a `FaceProfile`
  revision. Only the lumps **visible this frame** are re-composited, capped
  at N lumps per frame (default 16), with the rest updating round-robin.
  Show a red "camera on" indicator in the HUD while it runs. Turning the
  option off stops the tracks immediately.
- GPU path (later optimisation): a sprite shader variant that takes a face
  atlas + per-lump anchor UVs + a palette LUT and composites in the
  fragment shader. Same visual rules as §5. It must match the CPU output
  within a tolerance (golden test).
- **Never touches the sim.** Faces aren't part of the content fingerprint,
  checksums, demos or saves. A demo recorded with faces on plays identically
  with faces off.

---

## 8. Privacy, consent and multiplayer

Non-negotiable rules (they go into `docs/02-conventions.md` as a short
"Personal images" section):

1. **Local only.** Capture, detection, alignment and compositing run on the
   device. No network request carries face pixels unless rule 4 applies.
   No analytics or logs contain images or landmarks.
2. **Storage:** web → OPFS directory `faces/` (not `localStorage`). Desktop →
   config dir `faces/`. Each face is a folder with the 128×128 views + a
   JSON entry. Raw camera frames and source photos are **not** kept after
   alignment, unless the user ticks "keep original for re-cropping" (off by
   default).
3. **Forget:** the gallery has per-face delete and **Forget all faces**,
   which deletes the folder, clears caches and drops live texture overrides.
   Uninstall/clear site data also removes them (OPFS).
4. **Multiplayer sharing is opt-in, both ways:**
   - Default: each player sees only *their own* faces on enemies. Others see
     the normal sprites. Nothing is transmitted.
   - "Share my faces with this room" sends the **composited 128×128 front
     view(s) only** (never sources), size-capped (≤ 64 KB per face, ≤ 4
     faces), over the existing peer data channel (`room-net` / WebRTC),
     never via `room-server` storage. The server relays like any other
     message and never persists it.
   - Receiving peers get a prompt "Alex shared 2 faces. Show them?"
     (default **no** in public lobbies, **ask** in private rooms). Accepted
     faces live in memory for the session only.
   - Room operators can disable face sharing per room
     (`room-server` config + admin console toggle). Default: **disabled for
     public lobby rooms**.
   - A player can hide all shared faces at any time (one toggle) and block
     a specific peer's faces.
5. **Photos of other people** need the consent step (§3.3). The UI copy
   says plainly that the user is responsible for having permission.
6. **No identification:** never compute or store face embeddings, never
   compare faces, never label who a face is beyond the user-typed name.

---

## 9. Crate and file layout

```
crates/room-faceswap/          NEW, pure Rust, no I/O, no GPU, wasm32 + native
  src/lib.rs                   public API: detect, align, estimate_anchor, composite, bake_class
  src/detect/                  tract (option A) / rustface (option B) behind features; manual
  src/align.rs                 similarity transform, canonical crop
  src/anchor.rs                HeadAnchor, HEADANCHORS parser (room-sc), estimator
  src/composite.rs             §5 pipeline (Oklab, colour transfer, quantize, dither)
  src/profile.rs               FaceProfile, (de)serialization (serde + postcard/json)
  src/assign.rs                FaceAssignment rules, instance hashing
  README.md
crates/room-web/
  face-capture.js              NEW capture dialog (§3.2), replaces openCameraFace
  face-gallery.js              NEW gallery + consent + forget-all
  face-worker.js               NEW worker hosting the room-faceswap wasm for detection/bake
  editor-paint.js              anchors mode (§4.4), "Camera face…" → face-capture.js
  settings.js                  Live mode options
  src/browser/faces.rs         wasm-bindgen bindings to room-faceswap
crates/room-render/            face override lookup; later the GPU composite shader
crates/room-app/               file import, optional camera feature, settings UI
crates/room-cli/               `faceswap bake` / `faceswap anchors estimate`
wadsrc/…/HEADANCHORS           shipped anchors (Freedoom first, then Doom, Duke)
fuzz/                          targets: HEADANCHORS parser, image decode → align, composite with random anchors
```

Dependencies to add to `docs/appendix-a-dependencies.md` (all pure Rust):
`tract-onnx` (option A) or `rustface` (option B). Reuse the image crates
already allowed. Camera crate for desktop only after the §12 decision.

---

## 10. Testing

| Test | What |
|------|------|
| Alignment unit tests | Synthetic faces (generated ellipse + eye dots) at random scale/roll → canonical eyes within 1 px |
| Detection smoke | A small set of **synthetic or CC0/consented** face images under `testdata/faces/` (never photos of people who didn't agree). Box IoU and landmark error thresholds |
| Anchor coverage | Every Freedoom monster sprite lump has an anchor (shipped or estimated). With the GRP/IWAD present: Doom, Doom II, Duke, the same |
| Golden composites | `insta`/`image-compare` goldens: a synthetic face on Freedoom `TROO` A1–A8, `POSS`, `HEAD`. Run on x86-64, aarch64 and wasm32, which must be byte-identical (§5.2) |
| Offsets preserved | Composited lump has identical size, `left`, `top`, and the alpha mask ⊆ original alpha |
| Sim isolation | Freedoom demo verification passes with Live faces on. World checksums are equal with faces on and off |
| No network | wasm-bindgen test with a `fetch`/WebSocket spy: capture + detect + bake makes zero requests (except the one-time model asset load from the same origin) |
| Forget-all | After **Forget all faces**, the OPFS `faces/` dir is empty, caches are cleared and no override textures remain |
| Sharing | Two-peer netplay test: sharing off → no face message on the wire. On + accept → identical pixels on both. Public-room default → prompt not shown, faces not displayed |
| Performance | Bake budget (§7.1). Live mode ≤ 2 ms/frame CPU on the web at 16 lumps/frame |
| UI | Playwright run of `face-capture.js` with a fake media stream (Chromium `--use-fake-device-for-media-stream`): auto-capture triggers, permission-denied path offers import |

---

## 11. Milestones

| # | Deliverable | Acceptance |
|---|-------------|------------|
| F0 | `room-faceswap` scaffold, `FaceProfile`, manual-point alignment, §5 compositor for the front view | Golden composites on Freedoom `TROOA1`, deterministic across targets |
| F1 | `HeadAnchor`, `HEADANCHORS` parser + fuzz, auto-estimator, anchor editor mode, shipped Freedoom anchors | Anchor coverage test (Freedoom) |
| F2 | Bake one face onto every frame/rotation of a class (front + fallbacks §5.3), web Faces panel, PK3 export, `room-cli faceswap bake` | Bake budget. The baked PK3 loads in Room and the face shows on every rotation |
| F3 | New capture dialog (§3.2) with detection (§3.4), photo import + consent (§3.3), gallery, OPFS storage, forget-all. Remove the old `openCameraFace` | UI tests + no-network test |
| F4 | Live mode (static faces), assignment rules, settings | Sim isolation test, Live ≡ Bake pixels |
| F5 | Guided multi-angle capture, side/back views, live webcam faces mode | Manual checklist + perf budget |
| F6 | Multiplayer sharing + server toggle | Sharing tests |
| F7 | Duke content (palette + `pal` variants), Doom/Duke shipped anchors, desktop camera (if approved), optional GPU composite shader | Duke golden on `LIZTROOP` + `PIGCOP`, GPU ≈ CPU golden |

Update `docs/20-roadmap-testing.md` / `docs/21-milestone-progress.md` with
an **F** track and log each landing, per AGENTS §5.

---

## 12. Decisions for the project owner

1. **Detector:** option A (`tract-onnx` + YuNet-class model, best
   landmarks, ~300 KB asset + tract in the worker wasm) vs option B
   (`rustface`, box only, weaker eyes). Recommend A. Confirm the model
   licence first.
2. **Desktop camera:** add a camera crate (which one, after checking it
   pulls no `cc` build) or ship desktop with file/paste import only.
   Recommend import-only first.
3. **Public lobby default** for face sharing: disabled (recommended) vs
   ask.
4. **Bake per-instance variants** via generated DECORATE (changes content
   and the fingerprint) or limit Bake to per-class and leave per-instance
   to Live. Recommend the latter.
5. **Back-of-head default:** keep the original sprite head (recommended) vs
   hair-colour fill.
