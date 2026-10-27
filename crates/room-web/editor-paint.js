// Pixel editor for wall textures, flats, patches, sprites and graphics.
// Pictures are edited as PLAYPAL indices; every stroke is written back as a lump
// override (`roomEditorImagePut`) and swapped into the running 3D / play view.
// Wall textures save as TX_START lumps, which replace the TEXTURE1 composite.

const MAX_UNDO = 120;
const TRANSPARENT = -1;
/** Namespaces whose edits show live without restarting the level. */
const LIVE_NS = new Set(["textures", "flats", "patches", "sprites", "global"]);
const TOOLS = [
  ["pencil", "✏", "Pencil (B)"],
  ["eraser", "⌫", "Eraser → transparent (E)"],
  ["fill", "🪣", "Fill (G) · Shift: replace colour everywhere"],
  ["line", "╱", "Line (L)"],
  ["rect", "▭", "Rectangle (R) · Shift: filled"],
  ["picker", "💧", "Pick colour (I) · Alt+click with any tool"],
];
const TOOL_KEYS = { KeyB: "pencil", KeyE: "eraser", KeyG: "fill", KeyL: "line", KeyR: "rect", KeyI: "picker" };

/** Lump namespace (`summary.lumps[].ns` / kind) → pixel-editor namespace. */
export function paintNs(l) {
  if (l.ns === "flats" || l.kind === "flat") return "flats";
  if (l.ns === "patches" || l.kind === "patch") return "patches";
  if (l.ns === "sprites" || l.kind === "sprite") return "sprites";
  if (l.ns === "textures" || l.kind === "texture") return "textures";
  return "global";
}

export function install(ctx) {
  const { S, el } = ctx;

  /**
   * `opts.num`: editor number of the thing being edited (picks its animations).
   * `opts.camera`: open the webcam face capture as soon as the tab is ready.
   */
  function open(ns, name, opts = {}) {
    if (!S.mod || !S.summary) { ctx.toast("Open a WAD first", true); return; }
    name = String(name || "").trim().toUpperCase();
    if (!name || name === "-") { ctx.toast("Nothing to edit here (no texture)", true); return; }
    const id = `paint:${ns}:${name}`;
    if (S.lumpTabs.has(id)) {
      ctx.selectDocTab(id);
      if (opts.camera) S.lumpTabs.get(id).view._openCamera?.();
      return;
    }
    let img;
    try { img = S.mod.roomEditorImageGet(ns, name); } catch (e) { ctx.toast(ctx.errText(e), true); return; }
    if (!img) { ctx.toast(`${name}: no picture found in ${ns}`, true); return; }
    const view = build(ns, name, img, opts);
    S.lumpTabs.set(id, { title: `🎨 ${name}`, view });
    ctx.selectDocTab(id);
    view.querySelector(".px-canvas")?.focus();
    if (opts.camera) queueMicrotask(() => view._openCamera?.());
  }

  function build(ns, name, img, opts) {
    const pal = S.mod.roomEditorPalette();
    const P = {
      ns, name, w: img.w, h: img.h, left: img.left, top: img.top,
      idx: Uint8Array.from(img.indices), alpha: Uint8Array.from(img.alpha),
      fg: 0, bg: TRANSPARENT, tool: "pencil", size: 1, fillShapes: false,
      zoom: [16, 12, 8, 6, 4, 3, 2, 1].find((z) => z * Math.max(img.w, img.h) <= 512) || 1,
      grid: true, tile: false, undo: [], redo: [], dirty: false,
      opaque: ns === "flats",
      baseW: img.w, baseH: img.h,
    };
    // Wall UVs use the size the level was started with: resized walls wait for Save.
    const sizePending = () => P.ns === "textures" && (P.w !== P.baseW || P.h !== P.baseH);
    // Default colours: the most common opaque index, then transparent / black.
    {
      const hist = new Uint32Array(256);
      for (let i = 0; i < P.idx.length; i++) if (P.alpha[i]) hist[P.idx[i]]++;
      P.fg = hist.indexOf(Math.max(...hist));
      if (P.opaque) P.bg = nearest(0, 0, 0);
    }
    const live = LIVE_NS.has(ns);

    // ---------------------------------------------------------------- DOM
    const view = el("div", { class: "painter" });
    const canvas = el("canvas", { class: "px-canvas", tabindex: "0" });
    const wrap = el("div", { class: "px-stage" }, canvas);
    const pill = el("span", { class: "pill" }, "unchanged");
    const info = el("span", { class: "muted px-info" });
    const swFg = el("div", { class: "px-sw fg", title: "Primary (left click)" });
    const swBg = el("div", { class: "px-sw bg", title: "Secondary (right click) · X swaps" });
    const palGrid = el("div", { class: "px-pal" });
    const toolBtns = new Map();
    const tools = el("div", { class: "px-tools" }, ...TOOLS.map(([id, icon, title]) => {
      const b = el("button", { class: "btn icon", title, onclick: () => setTool(id) }, icon);
      toolBtns.set(id, b);
      return b;
    }));
    const sizeSel = el("select", { title: "Brush size ([ / ])" }, ...[1, 2, 3, 4, 6, 8].map((n) => el("option", { value: n }, `${n}px`)));
    sizeSel.addEventListener("change", () => { P.size = Number(sizeSel.value); });
    const zoomSel = el("select", { title: "Zoom (wheel, + / −)" }, ...[1, 2, 3, 4, 6, 8, 12, 16, 24].map((z) => el("option", { value: z }, `${z}×`)));
    zoomSel.addEventListener("change", () => { P.zoom = Number(zoomSel.value); draw(); });
    const gridCb = el("input", { type: "checkbox", checked: P.grid });
    gridCb.addEventListener("change", () => { P.grid = gridCb.checked; draw(); });
    const tileCb = el("input", { type: "checkbox", checked: P.tile });
    tileCb.addEventListener("change", () => { P.tile = tileCb.checked; draw(); });
    const fillCb = el("input", { type: "checkbox", checked: P.fillShapes });
    fillCb.addEventListener("change", () => { P.fillShapes = fillCb.checked; });

    const pixels = document.createElement("canvas");
    const pctx = pixels.getContext("2d");
    let imgData = null;

    // ---------------------------------------------------------------- colour helpers
    const rgb = (i) => [pal[i * 3], pal[i * 3 + 1], pal[i * 3 + 2]];
    const css = (i) => (i === TRANSPARENT ? "transparent" : `rgb(${rgb(i).join(",")})`);
    const nearCache = new Map();
    function nearest(r, g, b) {
      const key = (r << 16) | (g << 8) | b;
      const hit = nearCache.get(key);
      if (hit !== undefined) return hit;
      let best = 0, bd = Infinity;
      for (let i = 0; i < 256; i++) {
        const dr = pal[i * 3] - r, dg = pal[i * 3 + 1] - g, db = pal[i * 3 + 2] - b;
        const d = dr * dr + dg * dg + db * db;
        if (d < bd) { bd = d; best = i; }
      }
      nearCache.set(key, best);
      return best;
    }
    const colorAt = (x, y) => { const i = y * P.w + x; return P.alpha[i] ? P.idx[i] : TRANSPARENT; };
    function setPx(x, y, c) {
      if (x < 0 || y < 0 || x >= P.w || y >= P.h) return;
      if (c === TRANSPARENT && P.opaque) return;
      const i = y * P.w + x;
      if (c === TRANSPARENT) { P.alpha[i] = 0; P.idx[i] = 0; } else { P.alpha[i] = 255; P.idx[i] = c; }
      paintPx(i);
    }
    function paintPx(i) {
      const d = imgData.data, o = i * 4;
      if (P.alpha[i]) { const k = P.idx[i] * 3; d[o] = pal[k]; d[o + 1] = pal[k + 1]; d[o + 2] = pal[k + 2]; d[o + 3] = 255; } else d[o + 3] = 0;
    }
    function refreshPixels() {
      pixels.width = P.w; pixels.height = P.h;
      imgData = pctx.createImageData(P.w, P.h);
      for (let i = 0; i < P.idx.length; i++) paintPx(i);
    }

    // ---------------------------------------------------------------- history
    const snap = () => ({ w: P.w, h: P.h, idx: P.idx.slice(), alpha: P.alpha.slice() });
    function pushUndo() {
      P.undo.push(snap());
      if (P.undo.length > MAX_UNDO) P.undo.shift();
      P.redo = [];
    }
    function restore(s) {
      const resized = s.w !== P.w || s.h !== P.h;
      P.w = s.w; P.h = s.h; P.idx = s.idx; P.alpha = s.alpha;
      refreshPixels();
      draw();
      if (resized) renderSide();
      changed();
    }
    const undo = () => { if (P.undo.length) { P.redo.push(snap()); restore(P.undo.pop()); } };
    const redo = () => { if (P.redo.length) { P.undo.push(snap()); restore(P.redo.pop()); } };

    // ---------------------------------------------------------------- engine sync
    let commitTimer = 0;
    let pushTimer = 0;
    function changed() {
      P.dirty = true;
      if (!live || sizePending()) {
        setPill(sizePending() ? "resized — Save restarts the level" : "unsaved — Save to apply", "warn");
        return;
      }
      setPill("editing", "warn");
      // Patches recompose every texture that uses them (a VFS rebuild): debounce.
      clearTimeout(pushTimer);
      pushTimer = setTimeout(() => push(false), ns === "patches" ? 350 : 0);
      clearTimeout(commitTimer);
      commitTimer = setTimeout(() => push(true), 1500);
    }
    function push(commit) {
      clearTimeout(pushTimer);
      if (commit) clearTimeout(commitTimer);
      try {
        const how = S.mod.roomEditorImagePut(P.ns, P.name, P.w, P.h, P.left, P.top, P.idx, P.opaque ? new Uint8Array() : P.alpha, commit);
        if (how === "apply" && !commit) {
          setPill("unsaved — Save to apply", "warn");
        } else if (how === "apply") {
          ctx.setSummary(S.mod.roomEditorApplyLumps());
          ctx.clearThumbs(); ctx.render2d();
          P.baseW = P.w; P.baseH = P.h;
          setPill("saved · level restarted", "restart");
        } else {
          if (commit) { ctx.clearThumbs(); ctx.render2d(); anim?.refresh(); }
          setPill(commit ? "saved" : "live", "live");
        }
        if (commit) P.dirty = false;
        return how;
      } catch (e) {
        setPill("error", "error");
        ctx.toast(`${P.name}: ${ctx.errText(e)}`, true);
        return null;
      }
    }
    function save() {
      clearTimeout(commitTimer);
      const how = push(true);
      if (how) ctx.toast(how === "apply" ? `${P.name} saved — level restarted to show it` : `${P.name} saved · live in the 3D view · included in Export`);
    }
    function revert() {
      if (!confirm(`Drop all pixel edits to ${P.name} and reload the original?`)) return;
      clearTimeout(commitTimer); clearTimeout(pushTimer);
      try {
        S.mod.roomEditorSetLump(P.ns, P.name, null);
        ctx.setSummary(S.mod.roomEditorApplyLumps());
        ctx.clearThumbs(); ctx.render2d();
        const img = S.mod.roomEditorImageGet(P.ns, P.name);
        if (img) {
          pushUndo();
          Object.assign(P, { w: img.w, h: img.h, baseW: img.w, baseH: img.h, left: img.left, top: img.top, idx: Uint8Array.from(img.indices), alpha: Uint8Array.from(img.alpha), dirty: false });
          refreshPixels(); draw(); renderSide();
        }
        setPill("original", "");
        ctx.toast(`${P.name} reverted to the WAD version`);
      } catch (e) { ctx.toast(ctx.errText(e), true); }
    }

    // ---------------------------------------------------------------- drawing ops
    function stamp(x, y, c) {
      const r = P.size, o = Math.floor((r - 1) / 2);
      for (let dy = 0; dy < r; dy++) for (let dx = 0; dx < r; dx++) setPx(wrapX(x - o + dx), wrapY(y - o + dy), c);
    }
    const wrapX = (x) => (P.tile ? ((x % P.w) + P.w) % P.w : x);
    const wrapY = (y) => (P.tile ? ((y % P.h) + P.h) % P.h : y);
    function lineOp(x0, y0, x1, y1, fn) {
      const dx = Math.abs(x1 - x0), dy = -Math.abs(y1 - y0);
      const sx = x0 < x1 ? 1 : -1, sy = y0 < y1 ? 1 : -1;
      let err = dx + dy;
      for (;;) {
        fn(x0, y0);
        if (x0 === x1 && y0 === y1) break;
        const e2 = 2 * err;
        if (e2 >= dy) { err += dy; x0 += sx; }
        if (e2 <= dx) { err += dx; y0 += sy; }
      }
    }
    function rectOp(x0, y0, x1, y1, filled, fn) {
      const [ax, bx] = [Math.min(x0, x1), Math.max(x0, x1)];
      const [ay, by] = [Math.min(y0, y1), Math.max(y0, y1)];
      for (let y = ay; y <= by; y++) for (let x = ax; x <= bx; x++) {
        if (filled || y === ay || y === by || x === ax || x === bx) fn(x, y);
      }
    }
    function fill(x, y, c, global) {
      const target = colorAt(x, y);
      if (target === c || (c === TRANSPARENT && P.opaque)) return;
      if (global) {
        for (let yy = 0; yy < P.h; yy++) for (let xx = 0; xx < P.w; xx++) if (colorAt(xx, yy) === target) setPx(xx, yy, c);
        return;
      }
      const stack = [[x, y]];
      const seen = new Uint8Array(P.w * P.h);
      while (stack.length) {
        const [px, py] = stack.pop();
        const sxp = wrapX(px), syp = wrapY(py);
        if (sxp < 0 || syp < 0 || sxp >= P.w || syp >= P.h) continue;
        const i = syp * P.w + sxp;
        if (seen[i] || colorAt(sxp, syp) !== target) continue;
        seen[i] = 1;
        setPx(sxp, syp, c);
        stack.push([sxp + 1, syp], [sxp - 1, syp], [sxp, syp + 1], [sxp, syp - 1]);
      }
    }
    function transformImage(kind) {
      const { w, h } = P;
      if (kind === "rot" && w !== h) { ctx.toast("Rotate needs a square picture", true); return; }
      pushUndo();
      const ni = new Uint8Array(w * h), na = new Uint8Array(w * h);
      for (let y = 0; y < h; y++) for (let x = 0; x < w; x++) {
        const [sx, sy] = {
          flipH: [w - 1 - x, y], flipV: [x, h - 1 - y], rot: [y, w - 1 - x],
          shift: [(x + (w >> 1)) % w, (y + (h >> 1)) % h],
        }[kind];
        ni[y * w + x] = P.idx[sy * w + sx]; na[y * w + x] = P.alpha[sy * w + sx];
      }
      P.idx = ni; P.alpha = na;
      refreshPixels(); draw(); changed();
    }
    function resize(nw, nh) {
      nw = Math.round(nw); nh = Math.round(nh);
      if (!(nw >= 1 && nh >= 1 && nw <= 2048 && nh <= 2048)) { ctx.toast("Size must be 1–2048", true); return; }
      if (nw === P.w && nh === P.h) return;
      pushUndo();
      const ni = new Uint8Array(nw * nh), na = new Uint8Array(nw * nh);
      for (let y = 0; y < Math.min(nh, P.h); y++) for (let x = 0; x < Math.min(nw, P.w); x++) {
        ni[y * nw + x] = P.idx[y * P.w + x]; na[y * nw + x] = P.alpha[y * P.w + x];
      }
      P.w = nw; P.h = nh; P.idx = ni; P.alpha = na;
      refreshPixels(); draw(); renderSide();
      changed();
    }
    /** Map RGBA `ImageData` (size `P.w×P.h`) onto the palette canvas. */
    function applyImageData(data, label) {
      pushUndo();
      for (let i = 0; i < P.w * P.h; i++) {
        const o = i * 4;
        if (data[o + 3] < 128 && !P.opaque) {
          P.alpha[i] = 0;
          P.idx[i] = 0;
        } else {
          P.alpha[i] = 255;
          P.idx[i] = nearest(data[o], data[o + 1], data[o + 2]);
        }
      }
      refreshPixels();
      draw();
      changed();
      if (label) ctx.toast(label);
    }

    /**
     * Draw `src` into the sprite.
     * `mode`: `full` stretches; `face` pastes a circular crop.
     * Face placement: `faceScale` (0.2–1), `cx`/`cy` paste centre in sprite
     * pixels, `panX`/`panY` (−1…1) shift which part of the photo is used.
     */
    function applyCameraSource(src, {
      mode = "face",
      faceScale = 0.55,
      cx = null,
      cy = null,
      panX = 0,
      panY = 0,
      sw,
      sh,
      mirror = true,
      label,
    } = {}) {
      const srcW = sw || src.videoWidth || src.width || 0;
      const srcH = sh || src.videoHeight || src.height || 0;
      if (!(srcW > 0 && srcH > 0)) throw new Error("empty camera frame");

      const snap = document.createElement("canvas");
      snap.width = srcW;
      snap.height = srcH;
      const sg = snap.getContext("2d");
      if (mirror) {
        sg.translate(srcW, 0);
        sg.scale(-1, 1);
      }
      sg.drawImage(src, 0, 0);

      const out = document.createElement("canvas");
      out.width = P.w;
      out.height = P.h;
      const g = out.getContext("2d");
      g.imageSmoothingEnabled = true;

      if (mode === "full") {
        g.drawImage(snap, 0, 0, srcW, srcH, 0, 0, P.w, P.h);
      } else {
        pctx.putImageData(imgData, 0, 0);
        g.drawImage(pixels, 0, 0);
        const side = Math.min(srcW, srcH);
        const maxOff = (Math.max(srcW, srcH) - side) / 2;
        const sx = (srcW - side) / 2 + panX * maxOff;
        const sy = (srcH - side) / 2 + panY * maxOff;
        const diam = Math.max(4, Math.round(Math.min(P.w, P.h) * faceScale));
        const dx = Math.round((cx == null ? P.w / 2 : cx) - diam / 2);
        const dy = Math.round((cy == null ? P.h * 0.06 + diam / 2 : cy) - diam / 2);
        g.save();
        g.beginPath();
        g.arc(dx + diam / 2, dy + diam / 2, diam / 2, 0, Math.PI * 2);
        g.closePath();
        g.clip();
        g.drawImage(snap, sx, sy, side, side, dx, dy, diam, diam);
        g.restore();
      }
      applyImageData(
        g.getImageData(0, 0, P.w, P.h).data,
        label
          || (mode === "full"
            ? `Camera → ${P.w}×${P.h} (full frame)`
            : `Camera face → ${P.name}`),
      );
    }

    /**
     * After capture: live triple preview (sprite target, face crop, pixelized
     * result). Pixelized is the primary canvas; drag/resize/pan to place.
     */
    async function placeFaceOnSprite(srcCanvas) {
      const { ensureDlgStyles } = await import("./sound-record.js");
      ensureDlgStyles();
      if (!document.getElementById("room-face-place-css")) {
        const s = document.createElement("style");
        s.id = "room-face-place-css";
        s.textContent = `
.room-face-place .room-dlg{max-width:min(560px,94vw);padding:1.15rem 1.25rem 1.2rem}
.room-face-place .fp-head{display:flex;align-items:baseline;justify-content:space-between;gap:.75rem;margin:0 0 .85rem}
.room-face-place .fp-head h2{margin:0;font-size:1.05rem;font-weight:600;color:#eef0f3}
.room-face-place .fp-keys{display:flex;flex-wrap:wrap;gap:.3rem;margin:0;padding:0;list-style:none}
.room-face-place .fp-keys li{
  font:11px/1 ui-monospace,SFMono-Regular,Menlo,monospace;color:#aeb6c0;
  background:#242830;border:1px solid #343a44;border-radius:999px;padding:.28rem .55rem}
.room-face-place .fp-stage{
  display:grid;grid-template-columns:1.35fr .85fr;gap:.7rem;align-items:start;margin:0 0 .9rem}
.room-face-place .fp-main,.room-face-place .fp-side{display:flex;flex-direction:column;gap:.45rem;min-width:0}
.room-face-place .fp-side{gap:.55rem}
.room-face-place .fp-card{
  position:relative;background:#101216;border:1px solid #2c323b;border-radius:6px;overflow:hidden}
.room-face-place .fp-card.primary{border-color:#4a5564;box-shadow:inset 0 0 0 1px rgba(255,255,255,.03)}
.room-face-place .fp-tag{
  position:absolute;left:.45rem;top:.45rem;z-index:1;
  font:10px/1.1 ui-monospace,SFMono-Regular,Menlo,monospace;letter-spacing:.04em;text-transform:uppercase;
  color:#d7dbe0;background:rgba(16,18,22,.82);border:1px solid #3a414c;border-radius:4px;padding:.28rem .4rem}
.room-face-place .fp-card canvas{
  display:block;width:100%!important;height:auto!important;max-width:100%;margin:0;border:0;border-radius:0;
  cursor:grab;touch-action:none;image-rendering:pixelated;background:#0a0a0a}
.room-face-place .fp-card canvas:active{cursor:grabbing}
.room-face-place .fp-size{
  display:grid;grid-template-columns:auto 1fr auto;align-items:center;gap:.65rem;
  margin:0 0 .95rem;font-size:.8rem;color:#c9ced6}
.room-face-place .fp-size input[type=range]{width:100%;accent-color:#c45a3a}
.room-face-place .fp-size output{font:12px/1 ui-monospace,SFMono-Regular,Menlo,monospace;color:#d7dbe0;min-width:2.6rem;text-align:right}
.room-face-place .foot{margin-top:0}
@media (max-width:560px){
  .room-face-place .fp-stage{grid-template-columns:1fr}
  .room-face-place .fp-head{flex-direction:column;align-items:flex-start}
}
`;
        document.head.appendChild(s);
      }
      const overlay = document.createElement("div");
      overlay.className = "room-dlg-back room-face-place";
      const maxPrev = Math.min(320, Math.max(140, Math.round(Math.min(window.innerWidth, window.innerHeight) * 0.42)));
      const scale = Math.max(1, Math.floor(maxPrev / Math.max(P.w, P.h)));
      const pw = P.w * scale;
      const ph = P.h * scale;
      const sideMax = Math.min(180, Math.max(96, Math.round(maxPrev * 0.55)));
      const sideScale = Math.max(1, Math.floor(sideMax / Math.max(P.w, P.h)));
      const sw = P.w * sideScale;
      const sh = P.h * sideScale;
      overlay.innerHTML = `
        <div class="room-dlg" role="dialog" aria-label="Place face">
          <div class="fp-head">
            <h2>Place face · ${P.name}</h2>
            <ul class="fp-keys" aria-label="Controls">
              <li>Drag</li>
              <li>Shift pan</li>
              <li>Scroll size</li>
            </ul>
          </div>
          <div class="fp-stage">
            <div class="fp-main">
              <div class="fp-card primary">
                <span class="fp-tag">Pixelized</span>
                <canvas data-pix width="${pw}" height="${ph}"></canvas>
              </div>
            </div>
            <div class="fp-side">
              <div class="fp-card">
                <span class="fp-tag">Sprite</span>
                <canvas data-bg width="${sw}" height="${sh}"></canvas>
              </div>
              <div class="fp-card">
                <span class="fp-tag">Face</span>
                <canvas data-face width="${sw}" height="${sh}"></canvas>
              </div>
            </div>
          </div>
          <label class="fp-size">
            <span>Size</span>
            <input data-size type="range" min="20" max="95" value="55" />
            <output data-size-val>55%</output>
          </label>
          <div class="foot">
            <button type="button" data-act="cancel">Cancel</button>
            <button type="button" class="go" data-act="apply">Apply</button>
          </div>
        </div>`;
      document.body.appendChild(overlay);

      const bgC = overlay.querySelector("[data-bg]");
      const faceC = overlay.querySelector("[data-face]");
      const pixC = overlay.querySelector("[data-pix]");
      const sizeIn = overlay.querySelector("[data-size]");
      const sizeVal = overlay.querySelector("[data-size-val]");
      const bg = bgC.getContext("2d");
      const fc = faceC.getContext("2d");
      const px = pixC.getContext("2d");
      for (const g of [bg, fc, px]) g.imageSmoothingEnabled = false;

      pctx.putImageData(imgData, 0, 0);
      let faceScale = 0.55;
      let cx = P.w / 2;
      let cy = P.h * 0.06 + Math.min(P.w, P.h) * faceScale / 2;
      let panX = 0;
      let panY = 0;
      let drag = null;
      let paintQueued = 0;

      const srcW = srcCanvas.width;
      const srcH = srcCanvas.height;
      const side = Math.min(srcW, srcH);
      const compose = document.createElement("canvas");
      compose.width = P.w;
      compose.height = P.h;
      const cg = compose.getContext("2d");

      function paintChecker(g, cw, ch) {
        g.fillStyle = "#0d0f12";
        g.fillRect(0, 0, cw, ch);
        const cell = Math.max(4, Math.round(Math.min(cw, ch) / 16));
        for (let y = 0; y < ch; y += cell) {
          for (let x = 0; x < cw; x += cell) {
            g.fillStyle = ((x / cell ^ y / cell) & 1) ? "#16191e" : "#101318";
            g.fillRect(x, y, cell, cell);
          }
        }
      }

      function strokeGuide(g, sc) {
        const rad = (Math.max(4, Math.min(P.w, P.h) * faceScale) / 2) * sc;
        g.strokeStyle = "rgba(232, 196, 110, .92)";
        g.lineWidth = Math.max(1.5, sc * 0.15);
        g.beginPath();
        g.arc(cx * sc, cy * sc, rad, 0, Math.PI * 2);
        g.stroke();
        g.strokeStyle = "rgba(0,0,0,.5)";
        g.lineWidth = 1;
        g.stroke();
      }

      function placement() {
        const diam = Math.max(4, Math.min(P.w, P.h) * faceScale);
        const dx = cx - diam / 2;
        const dy = cy - diam / 2;
        const maxOff = (Math.max(srcW, srcH) - side) / 2;
        const sx = (srcW - side) / 2 + panX * maxOff;
        const sy = (srcH - side) / 2 + panY * maxOff;
        return { diam, dx, dy, sx, sy };
      }

      function paintSide(g, cw, ch, sc, mode) {
        const { diam, dx, dy, sx, sy } = placement();
        paintChecker(g, cw, ch);
        if (mode === "sprite") {
          g.drawImage(pixels, 0, 0, P.w, P.h, 0, 0, cw, ch);
          g.fillStyle = "rgba(232, 196, 110, .16)";
          g.beginPath();
          g.arc(cx * sc, cy * sc, (diam / 2) * sc, 0, Math.PI * 2);
          g.fill();
        } else {
          g.save();
          g.beginPath();
          g.arc(cx * sc, cy * sc, (diam / 2) * sc, 0, Math.PI * 2);
          g.closePath();
          g.clip();
          g.imageSmoothingEnabled = true;
          g.drawImage(srcCanvas, sx, sy, side, side, dx * sc, dy * sc, diam * sc, diam * sc);
          g.restore();
          g.imageSmoothingEnabled = false;
        }
        strokeGuide(g, sc);
      }

      function paintPreview() {
        const { diam, dx, dy, sx, sy } = placement();

        paintSide(bg, sw, sh, sideScale, "sprite");
        paintSide(fc, sw, sh, sideScale, "face");

        // Pixelized primary — sprite + face, PLAYPAL-quantized in the circle.
        cg.imageSmoothingEnabled = false;
        cg.clearRect(0, 0, P.w, P.h);
        cg.drawImage(pixels, 0, 0);
        cg.save();
        cg.beginPath();
        cg.arc(cx, cy, diam / 2, 0, Math.PI * 2);
        cg.closePath();
        cg.clip();
        cg.imageSmoothingEnabled = true;
        cg.drawImage(srcCanvas, sx, sy, side, side, dx, dy, diam, diam);
        cg.restore();
        const x0 = Math.max(0, Math.floor(dx));
        const y0 = Math.max(0, Math.floor(dy));
        const x1 = Math.min(P.w, Math.ceil(dx + diam));
        const y1 = Math.min(P.h, Math.ceil(dy + diam));
        const bw = Math.max(0, x1 - x0);
        const bh = Math.max(0, y1 - y0);
        if (bw > 0 && bh > 0) {
          const id = cg.getImageData(x0, y0, bw, bh);
          const d = id.data;
          const r2 = (diam / 2) * (diam / 2);
          for (let y = 0; y < bh; y++) {
            for (let x = 0; x < bw; x++) {
              const o = (y * bw + x) * 4;
              if (d[o + 3] < 8) continue;
              const ddx = x0 + x + 0.5 - cx;
              const ddy = y0 + y + 0.5 - cy;
              if (ddx * ddx + ddy * ddy > r2) continue;
              const i = nearest(d[o], d[o + 1], d[o + 2]);
              d[o] = pal[i * 3];
              d[o + 1] = pal[i * 3 + 1];
              d[o + 2] = pal[i * 3 + 2];
              d[o + 3] = 255;
            }
          }
          cg.putImageData(id, x0, y0);
        }
        paintChecker(px, pw, ph);
        px.drawImage(compose, 0, 0, P.w, P.h, 0, 0, pw, ph);
        strokeGuide(px, scale);
      }

      function schedulePaint() {
        if (paintQueued) return;
        paintQueued = requestAnimationFrame(() => {
          paintQueued = 0;
          paintPreview();
        });
      }
      paintPreview();

      function clientToSprite(ev, canvasEl) {
        const r = canvasEl.getBoundingClientRect();
        const x = ((ev.clientX - r.left) / Math.max(1, r.width)) * P.w;
        const y = ((ev.clientY - r.top) / Math.max(1, r.height)) * P.h;
        return { x, y };
      }

      function bindCanvas(canvasEl) {
        function onPointerDown(ev) {
          ev.preventDefault();
          canvasEl.setPointerCapture(ev.pointerId);
          const p = clientToSprite(ev, canvasEl);
          drag = {
            el: canvasEl,
            shift: ev.shiftKey,
            ox: p.x,
            oy: p.y,
            cx,
            cy,
            panX,
            panY,
          };
        }
        function onPointerMove(ev) {
          if (!drag || drag.el !== canvasEl) return;
          const p = clientToSprite(ev, canvasEl);
          if (drag.shift) {
            const diam = Math.min(P.w, P.h) * faceScale;
            panX = Math.max(-1, Math.min(1, drag.panX + (p.x - drag.ox) / Math.max(1, diam)));
            panY = Math.max(-1, Math.min(1, drag.panY + (p.y - drag.oy) / Math.max(1, diam)));
          } else {
            cx = drag.cx + (p.x - drag.ox);
            cy = drag.cy + (p.y - drag.oy);
          }
          schedulePaint();
        }
        function onPointerUp() {
          drag = null;
        }
        canvasEl.addEventListener("pointerdown", onPointerDown);
        canvasEl.addEventListener("pointermove", onPointerMove);
        canvasEl.addEventListener("pointerup", onPointerUp);
        canvasEl.addEventListener("pointercancel", onPointerUp);
        canvasEl.addEventListener("wheel", (ev) => {
          ev.preventDefault();
          const next = Math.round(faceScale * 100 + (ev.deltaY > 0 ? -3 : 3));
          sizeIn.value = String(Math.max(20, Math.min(95, next)));
          sizeIn.dispatchEvent(new Event("input"));
        }, { passive: false });
      }
      bindCanvas(bgC);
      bindCanvas(faceC);
      bindCanvas(pixC);

      sizeIn.addEventListener("input", () => {
        faceScale = Number(sizeIn.value) / 100;
        sizeVal.value = `${sizeIn.value}%`;
        sizeVal.textContent = `${sizeIn.value}%`;
        schedulePaint();
      });

      function close() {
        if (paintQueued) cancelAnimationFrame(paintQueued);
        overlay.remove();
        window.removeEventListener("keydown", onKey);
      }
      function onKey(e) {
        if (e.key === "Escape") close();
        if (e.key === "Enter") apply();
      }
      function apply() {
        try {
          applyCameraSource(srcCanvas, {
            mode: "face",
            faceScale,
            cx,
            cy,
            panX,
            panY,
            sw: srcW,
            sh: srcH,
            mirror: false,
            label: `Camera face → ${P.name} (placed)`,
          });
        } catch (e) {
          ctx.toast(`Place failed: ${ctx.errText(e)}`, true);
        }
        close();
      }
      window.addEventListener("keydown", onKey);
      overlay.addEventListener("click", (e) => {
        const act = e.target?.getAttribute?.("data-act");
        if (act === "cancel") close();
        if (act === "apply") apply();
      });
    }

    async function importPng() {
      const inp = el("input", { type: "file", accept: "image/*" });
      inp.addEventListener("change", async () => {
        const f = inp.files[0];
        if (!f) return;
        try {
          const bmp = await createImageBitmap(f);
          const c = document.createElement("canvas");
          c.width = P.w;
          c.height = P.h;
          const g = c.getContext("2d");
          g.imageSmoothingEnabled = false;
          g.drawImage(bmp, 0, 0, P.w, P.h);
          applyImageData(
            g.getImageData(0, 0, P.w, P.h).data,
            `Imported ${f.name} → ${P.w}×${P.h}, mapped to the palette`,
          );
        } catch (e) { ctx.toast(`Import failed: ${ctx.errText(e)}`, true); }
      });
      inp.click();
    }
    function exportPng() {
      pctx.putImageData(imgData, 0, 0);
      pixels.toBlob((b) => {
        const url = URL.createObjectURL(b);
        const a = el("a", { href: url, download: `${P.name}.png` });
        document.body.append(a); a.click(); a.remove();
        setTimeout(() => URL.revokeObjectURL(url), 2000);
      }, "image/png");
    }

    /** Webcam / import → place on sprite → palette via `face-capture.js`. */
    async function openCameraFace() {
      try {
        const { openFaceCapture } = await import("./face-capture.js");
        openFaceCapture({
          onCapture: (canvas) => placeFaceOnSprite(canvas),
        });
      } catch (e) {
        ctx.toast(`Face capture unavailable: ${ctx.errText(e)}`, true);
      }
    }
    view._openCamera = openCameraFace;

    // ---------------------------------------------------------------- render
    const PAD = 12;
    let hover = null;
    let shape = null;
    function draw() {
      zoomSel.value = String(P.zoom);
      const z = P.zoom, reps = P.tile ? 3 : 1;
      const W = P.w * z * reps + PAD * 2, H = P.h * z * reps + PAD * 2;
      if (canvas.width !== W || canvas.height !== H) { canvas.width = W; canvas.height = H; }
      const g = canvas.getContext("2d");
      g.imageSmoothingEnabled = false;
      g.clearRect(0, 0, W, H);
      pctx.putImageData(imgData, 0, 0);
      for (let ty = 0; ty < reps; ty++) for (let tx = 0; tx < reps; tx++) {
        g.drawImage(pixels, PAD + tx * P.w * z, PAD + ty * P.h * z, P.w * z, P.h * z);
      }
      const ox = PAD + (P.tile ? P.w * z : 0), oy = PAD + (P.tile ? P.h * z : 0);
      if (P.tile) {
        g.fillStyle = "rgba(0,0,0,0.35)";
        g.fillRect(PAD, PAD, W - PAD * 2, oy - PAD); g.fillRect(PAD, oy + P.h * z, W - PAD * 2, oy - PAD);
        g.fillRect(PAD, oy, ox - PAD, P.h * z); g.fillRect(ox + P.w * z, oy, ox - PAD, P.h * z);
      }
      if (P.grid && z >= 6) {
        g.strokeStyle = "rgba(255,255,255,0.08)";
        g.beginPath();
        for (let x = 0; x <= P.w; x++) { g.moveTo(ox + x * z + 0.5, oy); g.lineTo(ox + x * z + 0.5, oy + P.h * z); }
        for (let y = 0; y <= P.h; y++) { g.moveTo(ox, oy + y * z + 0.5); g.lineTo(ox + P.w * z, oy + y * z + 0.5); }
        g.stroke();
        // 8 / 16 / 64 px guides help with Doom-style tiling.
        g.strokeStyle = "rgba(90,176,255,0.22)";
        g.beginPath();
        const major = P.w >= 64 ? 16 : 8;
        for (let x = major; x < P.w; x += major) { g.moveTo(ox + x * z + 0.5, oy); g.lineTo(ox + x * z + 0.5, oy + P.h * z); }
        for (let y = major; y < P.h; y += major) { g.moveTo(ox, oy + y * z + 0.5); g.lineTo(ox + P.w * z, oy + y * z + 0.5); }
        g.stroke();
      }
      g.strokeStyle = "rgba(255,255,255,0.55)";
      g.setLineDash([4, 3]);
      g.strokeRect(ox - 0.5, oy - 0.5, P.w * z + 1, P.h * z + 1);
      g.setLineDash([]);
      if ((P.ns === "sprites" || P.ns === "global" || P.ns === "patches") && (P.left || P.top)) {
        // Offset hot-spot (sprite origin / HUD anchor).
        const hx = ox + P.left * z, hy = oy + P.top * z;
        g.strokeStyle = "#ff5a4a";
        g.beginPath(); g.moveTo(hx - 6, hy); g.lineTo(hx + 6, hy); g.moveTo(hx, hy - 6); g.lineTo(hx, hy + 6); g.stroke();
      }
      const mark = (pts, color) => {
        g.fillStyle = color === TRANSPARENT ? "rgba(255,255,255,0.35)" : css(color);
        for (const [x, y] of pts) {
          const px = wrapX(x), py = wrapY(y);
          if (px < 0 || py < 0 || px >= P.w || py >= P.h) continue;
          g.fillRect(ox + px * z, oy + py * z, z, z);
        }
      };
      if (shape) {
        const pts = [];
        const add = (x, y) => pts.push([x, y]);
        if (shape.tool === "line") lineOp(shape.x0, shape.y0, shape.x1, shape.y1, add);
        else rectOp(shape.x0, shape.y0, shape.x1, shape.y1, shape.filled, add);
        mark(pts, shape.color);
      } else if (hover) {
        const r = P.tool === "pencil" || P.tool === "eraser" ? P.size : 1;
        const o = Math.floor((r - 1) / 2);
        g.strokeStyle = "#ffcc33";
        g.lineWidth = 1;
        g.strokeRect(ox + (wrapX(hover.x) - o) * z + 0.5, oy + (wrapY(hover.y) - o) * z + 0.5, r * z - 1, r * z - 1);
      }
      const hc = hover && hover.x >= 0 && hover.y >= 0 && hover.x < P.w && hover.y < P.h ? colorAt(hover.x, hover.y) : null;
      info.textContent = `${P.w}×${P.h}${hover ? ` · ${hover.x},${hover.y}` : ""}${hc == null ? "" : hc === TRANSPARENT ? " · transparent" : ` · index ${hc}`}`;
    }
    function renderSwatches() {
      for (const [sw, c] of [[swFg, P.fg], [swBg, P.bg]]) {
        sw.style.background = c === TRANSPARENT ? "" : css(c);
        sw.classList.toggle("clear", c === TRANSPARENT);
        sw.textContent = c === TRANSPARENT ? "" : c;
      }
      palGrid.querySelectorAll(".on").forEach((x) => x.classList.remove("on", "fgsel", "bgsel"));
      for (const [c, cls] of [[P.fg, "fgsel"], [P.bg, "bgsel"]]) palGrid.querySelector(`[data-i="${c}"]`)?.classList.add("on", cls);
    }
    function setTool(id) {
      P.tool = id;
      for (const [k, b] of toolBtns) b.classList.toggle("active", k === id);
      canvas.style.cursor = id === "picker" ? "copy" : "crosshair";
    }
    {
      const cells = [];
      for (let i = 0; i < 256; i++) cells.push(el("div", { class: "px-cell", "data-i": i, title: `${i}: rgb(${rgb(i).join(", ")})`, style: `background:${css(i)}` }));
      if (!P.opaque) cells.push(el("div", { class: "px-cell clear", "data-i": TRANSPARENT, title: "Transparent" }));
      palGrid.append(...cells);
      palGrid.addEventListener("mousedown", (e) => {
        const i = e.target.dataset?.i;
        if (i == null) return;
        e.preventDefault();
        if (e.button === 2) P.bg = Number(i); else P.fg = Number(i);
        renderSwatches();
      });
      palGrid.addEventListener("contextmenu", (e) => e.preventDefault());
    }

    // ---------------------------------------------------------------- pointer
    function cellAt(e) {
      const r = canvas.getBoundingClientRect();
      const sx = (e.clientX - r.left) * (canvas.width / r.width);
      const sy = (e.clientY - r.top) * (canvas.height / r.height);
      const ox = PAD + (P.tile ? P.w * P.zoom : 0), oy = PAD + (P.tile ? P.h * P.zoom : 0);
      return { x: Math.floor((sx - ox) / P.zoom), y: Math.floor((sy - oy) / P.zoom) };
    }
    const inImage = (p) => P.tile || (p.x >= 0 && p.y >= 0 && p.x < P.w && p.y < P.h);
    let stroke = null;
    canvas.addEventListener("contextmenu", (e) => e.preventDefault());
    canvas.addEventListener("pointerdown", (e) => {
      canvas.focus();
      if (e.button !== 0 && e.button !== 2) return;
      const p = cellAt(e);
      if (!inImage(p)) return;
      const color = e.button === 2 ? P.bg : P.fg;
      const tool = e.altKey ? "picker" : P.tool;
      if (tool === "picker") {
        const c = colorAt(wrapX(p.x), wrapY(p.y));
        if (e.button === 2) P.bg = c; else P.fg = c;
        renderSwatches();
        return;
      }
      canvas.setPointerCapture(e.pointerId);
      pushUndo();
      if (tool === "fill") { fill(wrapX(p.x), wrapY(p.y), color, e.shiftKey); draw(); changed(); return; }
      if (tool === "line" || tool === "rect") {
        shape = { tool, x0: p.x, y0: p.y, x1: p.x, y1: p.y, color, filled: P.fillShapes || e.shiftKey };
        draw();
        return;
      }
      stroke = { last: p, color: tool === "eraser" ? TRANSPARENT : color };
      stamp(p.x, p.y, stroke.color);
      draw();
    });
    canvas.addEventListener("pointermove", (e) => {
      const p = cellAt(e);
      hover = inImage(p) ? p : null;
      if (stroke) {
        lineOp(stroke.last.x, stroke.last.y, p.x, p.y, (x, y) => stamp(x, y, stroke.color));
        stroke.last = p;
      } else if (shape) {
        shape.x1 = p.x; shape.y1 = p.y;
        shape.filled = P.fillShapes || e.shiftKey;
      }
      draw();
    });
    const endStroke = () => {
      if (shape) {
        const s = shape;
        shape = null;
        const plot = (x, y) => setPx(wrapX(x), wrapY(y), s.color);
        if (s.tool === "line") lineOp(s.x0, s.y0, s.x1, s.y1, plot); else rectOp(s.x0, s.y0, s.x1, s.y1, s.filled, plot);
        draw(); changed();
      } else if (stroke) { stroke = null; changed(); }
    };
    canvas.addEventListener("pointerup", endStroke);
    canvas.addEventListener("pointercancel", endStroke);
    canvas.addEventListener("pointerleave", () => { if (!stroke && !shape) { hover = null; draw(); } });
    // Wheel zooms around the cursor; Shift+wheel scrolls. Trackpads send many
    // small deltas, so steps are accumulated.
    let wheelAcc = 0;
    wrap.addEventListener("wheel", (e) => {
      if (e.shiftKey) return;
      e.preventDefault();
      wheelAcc += e.deltaMode === 1 ? e.deltaY * 40 : e.deltaY;
      if (Math.abs(wheelAcc) < 40) return;
      const dir = wheelAcc < 0 ? 1 : -1;
      wheelAcc = 0;
      zoomBy(dir, e);
    }, { passive: false });
    const ZOOMS = [1, 2, 3, 4, 6, 8, 12, 16, 24];
    function zoomBy(dir, anchor) {
      const i = ZOOMS.indexOf(P.zoom);
      const next = ZOOMS[Math.max(0, Math.min(ZOOMS.length - 1, (i < 0 ? 3 : i) + dir))];
      if (next === P.zoom) return;
      let keep = null;
      if (anchor) {
        const r = canvas.getBoundingClientRect(), w = wrap.getBoundingClientRect();
        keep = {
          fx: (anchor.clientX - r.left - PAD) / (r.width - PAD * 2),
          fy: (anchor.clientY - r.top - PAD) / (r.height - PAD * 2),
          vx: anchor.clientX - w.left, vy: anchor.clientY - w.top,
        };
      }
      P.zoom = next;
      draw();
      if (keep) {
        const r = canvas.getBoundingClientRect(), w = wrap.getBoundingClientRect();
        const cx = r.left - w.left + wrap.scrollLeft + PAD + keep.fx * (r.width - PAD * 2);
        const cy = r.top - w.top + wrap.scrollTop + PAD + keep.fy * (r.height - PAD * 2);
        wrap.scrollLeft = cx - keep.vx;
        wrap.scrollTop = cy - keep.vy;
      }
    }

    view.addEventListener("keydown", (e) => {
      if (e.target instanceof HTMLInputElement || e.target instanceof HTMLSelectElement) return;
      const mod = e.ctrlKey || e.metaKey;
      let handled = true;
      if (mod && e.code === "KeyZ") { if (e.shiftKey) redo(); else undo(); }
      else if (mod && e.code === "KeyY") redo();
      else if (mod && e.code === "KeyS") save();
      else if (mod) handled = false;
      else if (TOOL_KEYS[e.code]) setTool(TOOL_KEYS[e.code]);
      else if (e.code === "KeyX") { [P.fg, P.bg] = [P.bg, P.fg]; if (P.opaque && P.fg === TRANSPARENT) P.fg = 0; renderSwatches(); }
      else if (e.code === "BracketLeft" || e.code === "BracketRight") {
        const opts = [1, 2, 3, 4, 6, 8];
        const i = Math.max(0, Math.min(opts.length - 1, opts.indexOf(P.size) + (e.code === "BracketRight" ? 1 : -1)));
        P.size = opts[i]; sizeSel.value = String(P.size); draw();
      } else if (e.code === "Equal" || e.code === "NumpadAdd") zoomBy(1);
      else if (e.code === "Minus" || e.code === "NumpadSubtract") zoomBy(-1);
      else if (e.code === "KeyT") { P.tile = !P.tile; tileCb.checked = P.tile; draw(); }
      else if (e.code === "KeyH" && !e.shiftKey) transformImage("flipH");
      else if (e.code === "KeyH" && e.shiftKey) transformImage("flipV");
      else handled = false;
      if (handled) { e.preventDefault(); e.stopPropagation(); }
    });

    // ---------------------------------------------------------------- animation (sprites)
    const anim = P.ns === "sprites" ? animPanel() : null;
    function animPanel() {
      let data = null;
      try {
        const j = S.mod.roomEditorSpriteAnims(P.name, opts.num ?? undefined);
        data = j ? JSON.parse(j) : null;
      } catch (_) { data = null; }
      if (!data) return null;
      const lumps = data.lumps;
      const seqs = data.sequences.map((s) => ({ label: s.label, frames: s.frames.filter((f) => f.sprite !== "TNT1") }))
        .filter((s) => s.frames.length);
      const spr = P.name.slice(0, 4), fr = P.name[4];
      const rotOf = (n) => (n[4] === fr && n[5] !== "0" ? Number(n[5]) : n[6] === fr && n[7] ? Number(n[7]) : 1);
      const A = {
        seq: Math.max(0, seqs.findIndex((s) => s.frames.some((f) => f.sprite === spr && f.frame === fr))),
        rot: rotOf(P.name) || 1, playing: true, step: 0, timer: 0,
      };
      const offs = new Map();
      function offsetsOf(n) {
        if (n === P.name) return { w: P.w, h: P.h, left: P.left, top: P.top };
        if (!offs.has(n)) {
          let o = null;
          try { const im = S.mod.roomEditorImageGet("sprites", n); if (im) o = { w: im.w, h: im.h, left: im.left, top: im.top }; } catch (_) { o = null; }
          offs.set(n, o);
        }
        return offs.get(n);
      }
      /** Lump (and mirror flag) that draws `sprite` frame `f` from rotation `rot`. */
      function lumpFor(sprite, f, rot) {
        const r = String(rot);
        let any = null;
        for (const n of lumps) {
          if (!n.startsWith(sprite)) continue;
          if (n[4] === f && (n[5] === "0" || n[5] === r)) return { name: n, flip: false };
          if (n.length >= 8 && n[6] === f && n[7] === r) return { name: n, flip: true };
          if (!any && (n[4] === f || n[6] === f)) any = { name: n, flip: false };
        }
        return any;
      }
      const src = (n) => (n === P.name ? pixels : ctx.thumb(n));

      const seqSel = el("select", { title: "Animation (state label)" },
        ...seqs.map((s, i) => el("option", { value: i, selected: i === A.seq }, `${s.label} (${s.frames.length})`)));
      seqSel.addEventListener("change", () => { A.seq = Number(seqSel.value); A.step = 0; renderStrip(); tick(); });
      const rotSel = el("select", { title: "View angle (sprite rotation)" },
        ...[1, 2, 3, 4, 5, 6, 7, 8].map((r) => el("option", { value: r, selected: r === A.rot }, `rot ${r}`)));
      rotSel.addEventListener("change", () => { A.rot = Number(rotSel.value); renderStrip(); tick(); });
      const playBtn = el("button", { class: "btn small", title: "Play / pause" }, "⏸");
      playBtn.addEventListener("click", () => { A.playing = !A.playing; playBtn.textContent = A.playing ? "⏸" : "▶"; tick(); });
      const stepBtn = (d, t) => el("button", { class: "btn small", title: t, onclick: () => { A.playing = false; playBtn.textContent = "▶"; A.step += d; tick(); } }, d < 0 ? "⏮" : "⏭");
      const stage = el("canvas", { class: "px-anim-stage", width: 160, height: 160 });
      const status = el("div", { class: "muted px-anim-info" });
      const strip = el("div", { class: "px-frames" });
      const all = el("div", { class: "px-frames" });
      const allBox = el("details", {}, el("summary", {}, `All images (${lumps.length})`), all);
      const box = el("fieldset", { class: "px-anim" }, el("legend", {}, `Animation${data.class ? ` — ${data.class}` : ""}`),
        seqs.length
          ? el("div", { class: "tex-field" }, seqSel, rotSel, stepBtn(-1, "Previous frame"), playBtn, stepBtn(1, "Next frame"))
          : el("p", { class: "muted" }, "No state animations found for this sprite."),
        stage, status, strip, allBox);

      function frameTile(n, flip, onCls) {
        const c = el("canvas", { width: 40, height: 40 });
        const s = src(n);
        if (s) {
          const g = c.getContext("2d");
          const k = Math.min(40 / s.width, 40 / s.height, 2);
          const w = s.width * k, h = s.height * k;
          g.imageSmoothingEnabled = false;
          g.save();
          if (flip) { g.translate(40, 0); g.scale(-1, 1); }
          g.drawImage(s, (40 - w) / 2, (40 - h) / 2, w, h);
          g.restore();
        }
        return el("div", { class: `px-frame${n === P.name ? " cur" : ""}${onCls ? ` ${onCls}` : ""}`, title: `${n}${flip ? " (mirrored)" : ""} — click to edit`, "data-n": n, onclick: () => switchTo(n) },
          c, el("span", {}, n.slice(4)));
      }
      function seqLumps() {
        const s = seqs[A.seq];
        return s ? s.frames.map((f) => ({ ...f, lump: lumpFor(f.sprite, f.frame, A.rot) })) : [];
      }
      function renderStrip() {
        const tiles = [];
        let prev = null;
        seqLumps().forEach((f, i) => {
          if (!f.lump || f.lump.name === prev) return;
          prev = f.lump.name;
          const t = frameTile(f.lump.name, f.lump.flip);
          t.dataset.step = i;
          tiles.push(t);
        });
        strip.replaceChildren(...tiles);
        const groups = new Map();
        for (const n of lumps) { const k = `${n.slice(0, 4)} ${n[4]}`; if (!groups.has(k)) groups.set(k, []); groups.get(k).push(n); }
        all.replaceChildren(...[...groups].map(([k, ns]) => el("div", { class: "px-frame-row" },
          el("span", { class: "muted" }, k), ...ns.map((n) => frameTile(n, false)))));
      }
      function tick() {
        clearTimeout(A.timer);
        const frames = seqLumps();
        if (!frames.length) return;
        A.step = ((A.step % frames.length) + frames.length) % frames.length;
        const f = frames[A.step];
        drawStage(frames, f);
        status.textContent = `${f.sprite} ${f.frame}${f.bright ? " · bright" : ""} · ${f.tics < 0 ? "∞" : f.tics} tics${f.lump ? ` · ${f.lump.name}${f.lump.flip ? " (mirrored)" : ""}` : " · missing"}`;
        strip.querySelectorAll(".px-frame.on").forEach((t) => t.classList.remove("on"));
        let on = null;
        for (const t of strip.children) if (Number(t.dataset.step) <= A.step) on = t;
        on?.classList.add("on");
        if (!A.playing) return;
        const ms = f.tics < 0 ? 1200 : Math.max(1, f.tics) * (1000 / 35);
        A.timer = setTimeout(() => {
          if (!view.isConnected) { A.timer = setTimeout(tick, 500); return; }
          A.step++;
          tick();
        }, ms);
      }
      function drawStage(frames, f) {
        const g = stage.getContext("2d");
        g.setTransform(1, 0, 0, 1, 0, 0);
        g.clearRect(0, 0, stage.width, stage.height);
        // Fit every frame of the sequence around the shared sprite origin (as in game).
        let x0 = 0, x1 = 1, y0 = -1, y1 = 0;
        for (const fr of frames) {
          const o = fr.lump && offsetsOf(fr.lump.name);
          if (!o) continue;
          const l = fr.lump.flip ? o.w - o.left : o.left;
          x0 = Math.min(x0, -l); x1 = Math.max(x1, o.w - l);
          y0 = Math.min(y0, -o.top); y1 = Math.max(y1, o.h - o.top);
        }
        const k = Math.max(0.25, Math.min(4, Math.floor(((stage.width - 12) / Math.max(x1 - x0, y1 - y0)) * 4) / 4));
        const ox = stage.width / 2 - ((x0 + x1) / 2) * k, oy = stage.height - 6 - y1 * k;
        g.strokeStyle = "rgba(255,255,255,0.15)";
        g.beginPath(); g.moveTo(0, oy + 0.5); g.lineTo(stage.width, oy + 0.5); g.stroke();
        const o = f.lump && offsetsOf(f.lump.name);
        const s = f.lump && src(f.lump.name);
        if (!o || !s) return;
        g.imageSmoothingEnabled = false;
        if (f.lump.name === P.name && imgData) pctx.putImageData(imgData, 0, 0);
        g.translate(ox, oy);
        if (f.lump.flip) g.scale(-1, 1);
        g.drawImage(s, -o.left * k, -o.top * k, o.w * k, o.h * k);
      }
      function switchTo(n) {
        if (n === P.name) return;
        clearTimeout(A.timer);
        if (P.dirty) push(true);
        S.lumpTabs.delete(`paint:${P.ns}:${P.name}`);
        open("sprites", n, opts);
      }
      // First frame once the editable pixels exist (they are set up after the panels).
      setTimeout(() => { renderStrip(); tick(); }, 0);
      return { box, refresh: () => { offs.clear(); renderStrip(); tick(); } };
    }

    // ---------------------------------------------------------------- side panel
    const side = el("div", { class: "px-side" });
    function num(val, onset, w = 64) {
      const i = el("input", { type: "number", value: val, style: `width:${w}px` });
      i.addEventListener("change", () => onset(Math.round(Number(i.value) || 0)));
      return i;
    }
    let soundsUi = null;
    function thingClassName() {
      if (opts.className) return opts.className;
      if (opts.num != null && S.summary?.things) {
        const t = S.summary.things.find((x) => x.num === opts.num);
        if (t?.name) return t.name;
      }
      try {
        const j = S.mod.roomEditorSpriteAnims(P.name, opts.num ?? undefined);
        const d = j ? JSON.parse(j) : null;
        return d?.class || null;
      } catch {
        return null;
      }
    }
    async function ensureSoundsPanel() {
      if (P.ns !== "sprites" || !ctx.actorSoundSlots) return null;
      const cls = thingClassName();
      if (!cls) return null;
      if (soundsUi?.className === cls) return soundsUi;
      const { mountThingSoundsPanel } = await import("./sound-record.js");
      let audioCtx = null;
      const ui = mountThingSoundsPanel({
        el,
        className: cls,
        getSlots: () => ctx.actorSoundSlots(cls),
        decode: (id) => S.mod.roomEditorDecodeSound(id),
        play: (snd) => {
          try {
            audioCtx = audioCtx || new AudioContext();
            const ch = snd.channels || 1;
            const frames = Math.floor(snd.samples.length / ch);
            const buf = audioCtx.createBuffer(ch, frames, snd.rate);
            for (let k = 0; k < ch; k++) {
              const d = buf.getChannelData(k);
              for (let i = 0; i < frames; i++) d[i] = snd.samples[i * ch + k];
            }
            const src = audioCtx.createBufferSource();
            src.buffer = buf;
            src.connect(audioCtx.destination);
            src.start();
          } catch (e) {
            ctx.toast(ctx.errText(e), true);
          }
        },
        replace: (slot, bytes) => {
          if (!slot.lump) {
            ctx.toast("No lump mapped for that sound", true);
            return;
          }
          try {
            const err = ctx.overrideLump({ name: slot.lump, ns: "global" }, bytes);
            ctx.toast(
              err ? `Replaced ${slot.lump} — ${err}` : `${slot.lump} replaced with your recording`,
              !!err,
            );
          } catch (e) {
            ctx.toast(ctx.errText(e), true);
          }
        },
      });
      ui.className = cls;
      soundsUi = ui;
      return ui;
    }
    async function renderSide() {
      const kids = [
        ...(anim ? [anim.box] : []),
        el("fieldset", {}, el("legend", {}, "Colours"),
          el("div", { class: "px-swatches" }, swFg, swBg,
            el("span", { class: "muted" }, "Left / right click · X swaps · Alt+click picks")),
          palGrid),
      ];
      if (P.ns !== "flats") {
        const wIn = num(P.w, () => {}), hIn = num(P.h, () => {});
        const offs = P.ns === "textures" ? null : el("div", { class: "row" }, el("label", {}, "Offsets"),
          el("div", { class: "tex-field" },
            num(P.left, (v) => { P.left = v; draw(); changed(); }, 58),
            num(P.top, (v) => { P.top = v; draw(); changed(); }, 58)));
        kids.push(el("fieldset", {}, el("legend", {}, "Size"),
          el("div", { class: "row" }, el("label", {}, "W × H"),
            el("div", { class: "tex-field" }, wIn, hIn,
              el("button", { class: "btn small", title: "Resize the canvas (anchored top-left)", onclick: () => resize(Number(wIn.value), Number(hIn.value)) }, "Resize"))),
          offs));
      }
      const snd = await ensureSoundsPanel();
      if (snd) {
        snd.refresh();
        kids.push(snd.box);
      }
      const where = {
        textures: "Saved as a TX_START lump: it replaces the TEXTURE1 composite of the same name.",
        flats: "Saved as a raw flat (F_START).",
        patches: "Saved as a patch: every TEXTURE1 composite using it updates.",
        sprites: "Saved as a sprite frame (S_START): actors using it update live.",
        global: "Saved as a Doom picture: HUD / status bar / menu graphics update live in play.",
      }[P.ns];
      kids.push(el("p", { class: "muted" }, where, " Pixel edits are part of Export PWAD."),
        el("p", { class: "muted" }, "B pencil · E eraser · G fill · L line · R rect · I picker · [ ] brush · wheel or + − zoom (Shift+wheel scrolls) · T tile · H flip (Shift: vertical) · Ctrl+Z / Y · Ctrl+S save"));
      side.replaceChildren(...kids);
      renderSwatches();
    }
    function setPill(text, kind) { pill.textContent = text; pill.className = `pill ${kind}`; }

    view.append(
      el("div", { class: "doc-head" }, el("h2", {}, `🎨 ${P.name}`),
        el("span", { class: `kind ${{ textures: "texture", flats: "flat", patches: "patch", sprites: "sprite" }[P.ns] || "graphic"}` }, P.ns), info, pill),
      el("div", { class: "actions" },
        tools,
        el("label", { class: "muted" }, "Brush ", sizeSel),
        el("label", { class: "muted" }, "Zoom ", zoomSel),
        el("label", { class: "muted", title: "Grid at zoom ≥ 6" }, gridCb, " Grid"),
        el("label", { class: "muted", title: "3×3 tiling preview; strokes wrap around (T)" }, tileCb, " Tile"),
        el("label", { class: "muted", title: "Rectangle tool draws filled" }, fillCb, " Filled"),
        el("button", { class: "btn small", title: "Flip horizontally (H)", onclick: () => transformImage("flipH") }, "⇋"),
        el("button", { class: "btn small", title: "Flip vertically (Shift+H)", onclick: () => transformImage("flipV") }, "⇵"),
        el("button", { class: "btn small", title: "Rotate 90° (square pictures)", onclick: () => transformImage("rot") }, "⟳"),
        el("button", { class: "btn small", title: "Wrap by half the size to check tiling seams", onclick: () => transformImage("shift") }, "½"),
        el("button", { class: "btn small", onclick: importPng }, "Import image…"),
        ...(P.ns === "sprites"
          ? [el("button", {
            class: "btn small",
            title: "Webcam → paste a face onto this enemy sprite (live)",
            onclick: openCameraFace,
          }, "Camera face…")]
          : []),
        el("button", { class: "btn small", onclick: exportPng }, "Export PNG"),
        el("button", { class: "btn small", onclick: () => { S[P.ns === "flats" ? "flat" : "texture"] = P.name; ctx.renderInspector(); ctx.toast(`Paint ${P.ns === "flats" ? "flat" : "texture"}: ${P.name}`); } }, "Use as paint"),
        el("button", { class: "btn small danger", onclick: revert }, "Revert"),
        el("button", { class: "btn small go", onclick: save }, "Save (Ctrl+S)")),
      el("div", { class: "px-grid" }, wrap, side));

    refreshPixels();
    setTool("pencil");
    sizeSel.value = "1";
    renderSide();
    draw();
    setPill(live ? "live edit" : "edit · save to apply", "");
    return view;
  }

  return { open };
}
