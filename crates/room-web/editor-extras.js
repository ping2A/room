// Editor workflow tools on top of `editor.js`: autosave + restore, tag links,
// sector shaping (stairs, gradients, slopes, merge), cut / measure / shape
// drawing, curves, find & replace with a box-select filter, prefabs and a
// background tracing image. Map logic lives in `editor-geom.js` (tested in
// Node); this file is the UI around it.

import {
  orderSectors, buildStairs, gradient, slopeSectors, clearSlope, mergeSectors, splitSector, curveLine,
  rectPoints, ellipsePoints, tagLinks, FIND_KINDS, matches, findAll, replaceAll, isPrefab, prefabSize,
  measure, sectorEdges, prefabOutline, prefabsToFile, mergePrefabs,
} from "./editor-geom.js";

const AUTOSAVE_KEY = "autosave";
/** Snapshot list (metadata); each snapshot's work is under `snap:<id>`. */
const SNAP_INDEX = "snapindex";
const AUTO_SNAP_EVERY = 10 * 60 * 1000;
const AUTO_SNAP_KEEP = 5;
const MANUAL_SNAP_KEEP = 30;
const PREFAB_KEY = "room/editor-prefabs";

// ------------------------------------------------------------------ IndexedDB

function idb() {
  return new Promise((resolve, reject) => {
    if (!window.indexedDB) { reject(new Error("IndexedDB is unavailable")); return; }
    const r = indexedDB.open("room-editor", 1);
    r.onupgradeneeded = () => r.result.createObjectStore("kv");
    r.onsuccess = () => resolve(r.result);
    r.onerror = () => reject(r.error || new Error("Could not open editor storage"));
  });
}
async function kv(mode, fn) {
  const db = await idb();
  try {
    return await new Promise((resolve, reject) => {
      const tx = db.transaction("kv", mode);
      const req = fn(tx.objectStore("kv"));
      tx.oncomplete = () => resolve(req?.result);
      tx.onerror = () => reject(tx.error);
      tx.onabort = () => reject(tx.error || new Error("Storage write aborted"));
    });
  } finally { db.close(); }
}
const kvGet = (k) => kv("readonly", (s) => s.get(k));
const kvPut = (k, v) => kv("readwrite", (s) => s.put(v, k));
const kvDel = (k) => kv("readwrite", (s) => s.delete(k));

const timeLabel = (t) => {
  const d = new Date(t);
  const today = new Date().toDateString() === d.toDateString();
  return today ? d.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" }) : d.toLocaleString([], { dateStyle: "medium", timeStyle: "short" });
};
const plural = (n, w) => `${n} ${w}${n === 1 ? "" : /(ch|sh|s|x)$/.test(w) ? "es" : "s"}`;

export function install(ctx) {
  const { S, $, el } = ctx;
  const X = {
    links: null,
    linksKey: "",
    tool: null, // cut / measure / shape gesture in progress
    measure: null,
    hinge: null, // slope hinge line shown in 2D
    filter: null, // { kind, value } — box selection keeps only matches
    armed: null, // prefab waiting for a click
    bg: null, // { img, name, x, y, scale, alpha }
  };

  // ================================================================== autosave

  let lumpGen = 0;
  let savedSig = "";
  let saving = false;
  const isOverride = (l) => l.src === "editor-overrides.wad" || l.src === "editor-overrides.pk3";
  const workSig = () => `${S.summary?.iwad}|${Object.keys(S.docs).length}|${S.version}|${lumpGen}|${S.acsGen || 0}`;

  function collectWork() {
    if (!S.summary || S.summary.duke || !S.mod) return null;
    const docs = { ...S.docs };
    const lumps = [];
    for (const l of S.summary.lumps || []) {
      if (!isOverride(l) || l.id == null) continue;
      try {
        lumps.push({ src: l.src, ns: l.ns, name: l.name, path: l.path || null, arch: !!l.arch, kind: l.kind, bytes: S.mod.roomEditorReadLump(l.id) });
      } catch (_) { /* unreadable: skip */ }
    }
    const acs = { ...(S.mapAcs || {}) };
    if (!Object.keys(docs).length && !lumps.length && !Object.keys(acs).length) return null;
    return {
      time: Date.now(),
      iwad: S.summary.iwad,
      pwads: [...(S.summary.pwads || [])],
      current: S.doc?.name || null,
      docs: JSON.stringify(docs),
      maps: [...new Set([...Object.keys(docs), ...Object.keys(acs)])],
      lumps,
      acs,
    };
  }

  async function autosave(force = false) {
    if (saving || S.restoreOffer) return;
    const sig = workSig();
    if (!force && sig === savedSig) return;
    const work = collectWork();
    if (!work) { savedSig = sig; return; }
    saving = true;
    try {
      await kvPut(AUTOSAVE_KEY, work);
      savedSig = sig;
      $("#st-saved") && ($("#st-saved").textContent = `Autosaved ${timeLabel(work.time)}`);
    } catch (e) {
      console.warn("editor autosave failed", e);
    } finally { saving = false; }
  }
  setInterval(() => { autosave(); }, 5000);
  document.addEventListener("visibilitychange", () => { if (document.hidden) autosave(); });

  function banner(text, buttons) {
    let b = $("#restore-banner");
    if (!b) { b = el("div", { id: "restore-banner", role: "alert" }); document.body.append(b); }
    b.replaceChildren(el("span", {}, text), ...buttons.map(([label, cls, fn]) => el("button", { class: `btn small ${cls}`, onclick: fn }, label)));
    b.hidden = false;
    return b;
  }
  const closeBanner = () => { const b = $("#restore-banner"); if (b) b.hidden = true; };

  async function restore(rec, { replace = false } = {}) {
    try {
      if (replace) {
        // A snapshot replaces the current edits: drop overrides it doesn't have.
        for (const l of S.summary?.lumps || []) if (isOverride(l) && l.id != null) ctx.setLumpData(l, null);
        ctx.setSummary(S.mod.roomEditorApplyLumps());
      }
      for (const l of rec.lumps || []) ctx.setLumpData(l, l.bytes);
      if (rec.lumps?.length) ctx.setSummary(S.mod.roomEditorApplyLumps());
      const docs = JSON.parse(rec.docs || "{}");
      for (const [name, doc] of Object.entries(docs)) {
        if (!S.summary.maps.includes(name)) {
          try { S.mod.roomEditorNewMap(name, String(doc.format || "doom").toLowerCase()); } catch (_) { /* keep the doc anyway */ }
        }
      }
      S.docs = docs;
      // Map scripts first: loading a map then keeps its compiled BEHAVIOR.
      S.mapAcs = {};
      for (const [map, src] of Object.entries(rec.acs || {})) {
        try {
          if (JSON.parse(S.mod.roomEditorSetMapAcs(map, src)).ok) S.mapAcs[map] = src;
        } catch (_) { /* map gone: drop its scripts */ }
      }
      ctx.setSummary(S.mod.roomEditorSummary());
      const first = rec.current && (docs[rec.current] || S.mapAcs[rec.current]) ? rec.current : Object.keys(docs)[0] || Object.keys(S.mapAcs)[0];
      if (first) ctx.loadMap(first);
      lumpGen++;
      savedSig = workSig();
      ctx.toast(`Restored ${plural(Object.keys(docs).length, "map")}${rec.lumps?.length ? ` and ${plural(rec.lumps.length, "file")}` : ""} from ${timeLabel(rec.time)}`);
    } catch (e) {
      ctx.toast(`Restore failed: ${ctx.errText(e)}`, true);
    }
  }

  /** After a game archive is opened: offer the work autosaved with it. */
  async function offerRestore() {
    let rec = null;
    try { rec = await kvGet(AUTOSAVE_KEY); } catch (_) { return; }
    if (!rec || !S.summary || S.summary.duke) return;
    const names = rec.maps || [];
    const mapText = names.length ? (names.length > 3 ? `${names.slice(0, 3).join(", ")} and ${names.length - 3} more` : names.join(", ")) : "";
    const what = [mapText, rec.lumps?.length && plural(rec.lumps.length, "edited file")].filter(Boolean).join(" and ");
    if (!what) return;
    if (rec.iwad !== S.summary.iwad) {
      banner(`Unsaved work from ${timeLabel(rec.time)} (${what}) was made with ${rec.iwad}. Open that game to restore it.`, [
        ["Dismiss", "", closeBanner],
      ]);
      return;
    }
    S.restoreOffer = true;
    const missing = (rec.pwads || []).filter((p) => !(S.summary.pwads || []).includes(p));
    const note = missing.length ? ` It used ${missing.join(", ")}; add ${missing.length > 1 ? "them" : "it"} first if the maps need ${missing.length > 1 ? "them" : "it"}.` : "";
    banner(`Unsaved work from ${timeLabel(rec.time)}: ${what}. Restore it?${note}`, [
      ["Restore", "primary", async () => { closeBanner(); S.restoreOffer = false; await restore(rec); }],
      ["Discard", "", async () => {
        if (!confirm("Discard the autosaved work? This cannot be undone.")) return;
        closeBanner(); S.restoreOffer = false;
        try { await kvDel(AUTOSAVE_KEY); } catch (_) { /* ignore */ }
        savedSig = workSig();
      }],
    ]);
  }

  /** On the start screen: say where autosaved work is waiting. */
  async function noteAutosave() {
    try {
      const rec = await kvGet(AUTOSAVE_KEY);
      if (rec && !S.summary) ctx.status(`Unsaved work from ${timeLabel(rec.time)} with ${rec.iwad} — open it to restore`);
    } catch (_) { /* no storage */ }
  }

  // ---------------------------------------------------------------- snapshots

  let exportedSig = "";
  let autoSnapSig = "";
  let lastAutoSnap = Date.now();
  const snapIndex = async () => (await kvGet(SNAP_INDEX).catch(() => null)) || [];

  /** Save the current work as a snapshot (`kind` manual / auto); returns its entry or null. */
  async function saveSnapshot(name, kind = "manual") {
    const work = collectWork();
    if (!work) return null;
    const id = `${Date.now()}-${Math.random().toString(36).slice(2, 7)}`;
    const entry = { id, name, kind, time: work.time, iwad: work.iwad, maps: work.maps, files: work.lumps.length };
    await kvPut(`snap:${id}`, work);
    let list = [entry, ...(await snapIndex())];
    // Keep the newest automatic and manual snapshots; delete the rest.
    const keep = (k, n) => list.filter((e) => e.kind === k).slice(n);
    for (const old of [...keep("auto", AUTO_SNAP_KEEP), ...keep("manual", MANUAL_SNAP_KEEP)]) {
      await kvDel(`snap:${old.id}`).catch(() => {});
      list = list.filter((e) => e !== old);
    }
    await kvPut(SNAP_INDEX, list);
    return entry;
  }

  // Every 10 minutes of changed work: an automatic snapshot.
  setInterval(async () => {
    if (Date.now() - lastAutoSnap < AUTO_SNAP_EVERY || S.restoreOffer) return;
    const sig = workSig();
    if (sig === autoSnapSig) return;
    lastAutoSnap = Date.now();
    try { if (await saveSnapshot(`Automatic ${timeLabel(Date.now())}`, "auto")) autoSnapSig = sig; } catch (e) { console.warn("snapshot failed", e); }
  }, 30 * 1000);

  async function restoreSnapshot(entry) {
    if (!confirm(`Replace the current work with "${entry.name}"? The current work is saved as a snapshot first.`)) return;
    const rec = await kvGet(`snap:${entry.id}`).catch(() => null);
    if (!rec) { ctx.toast("That snapshot is gone", true); return; }
    if (rec.iwad !== S.summary?.iwad) { ctx.toast(`That snapshot was made with ${rec.iwad}; open it first`, true); return; }
    await saveSnapshot(`Before restoring ${entry.name}`, "auto").catch(() => null);
    await restore(rec, { replace: true });
    renderSnapshots();
  }

  async function renderSnapshots() {
    const box = $("#snapshot-list");
    if (!box) return;
    const list = await snapIndex();
    box.replaceChildren();
    if (!list.length) { box.append(el("p", { class: "muted" }, "No snapshots yet.")); return; }
    for (const e of list) {
      const what = [e.maps?.length ? e.maps.slice(0, 3).join(", ") + (e.maps.length > 3 ? ` +${e.maps.length - 3}` : "") : "", e.files ? plural(e.files, "file") : ""].filter(Boolean).join(" · ");
      box.append(el("div", { class: "snap-row" },
        el("div", { class: "snap-info" },
          el("strong", {}, e.name),
          el("span", { class: "muted" }, `${timeLabel(e.time)}${e.kind === "auto" ? " · automatic" : ""}${what ? ` · ${what}` : ""}${e.iwad !== S.summary?.iwad ? ` · ${e.iwad}` : ""}`)),
        el("button", { class: "btn small", onclick: () => restoreSnapshot(e) }, "Restore"),
        el("button", { class: "btn small danger", title: "Delete this snapshot", onclick: async () => {
          if (!confirm(`Delete the snapshot "${e.name}"?`)) return;
          await kvDel(`snap:${e.id}`).catch(() => {});
          await kvPut(SNAP_INDEX, (await snapIndex()).filter((x) => x.id !== e.id));
          renderSnapshots();
        } }, "✕")));
    }
  }

  function openSnapshots() {
    const nameIn = el("input", { type: "text", placeholder: "Before the boss room", maxlength: "60" });
    const save = async () => {
      const name = nameIn.value.trim() || `Snapshot ${timeLabel(Date.now())}`;
      try {
        const e = await saveSnapshot(name);
        if (!e) { ctx.toast("Nothing to save yet — edit a map or a file first"); return; }
        nameIn.value = "";
        ctx.status(`Saved snapshot "${name}"`);
        renderSnapshots();
      } catch (err) { ctx.toast(`Snapshot failed: ${ctx.errText(err)}`, true); }
    };
    nameIn.addEventListener("keydown", (e) => { if (e.key === "Enter") save(); });
    const body = el("div", { class: "xcol" },
      el("p", { class: "gen-intro muted" }, `Snapshots keep your maps, edited files and map scripts in this browser. One is also taken automatically every 10 minutes while you work (the last ${AUTO_SNAP_KEEP} are kept).`),
      el("div", { class: "snap-save" }, nameIn, el("button", { class: "btn primary", onclick: save }, "Save")),
      el("div", { id: "snapshot-list", class: "xcol" }));
    panel("snapshot-panel", "Snapshots", body);
    renderSnapshots();
    nameIn.focus();
  }

  // Leaving with work that was never exported: the browser asks first.
  window.addEventListener("beforeunload", (e) => {
    if (!S.summary || S.summary.duke || workSig() === exportedSig) return;
    const work = Object.keys(S.docs).length || Object.keys(S.mapAcs || {}).length || (S.summary.lumps || []).some(isOverride);
    if (!work) return;
    e.preventDefault();
    e.returnValue = "";
  });

  /** Exported: the downloaded file now holds the work. */
  async function onExported() {
    savedSig = workSig();
    exportedSig = savedSig;
    try { await kvDel(AUTOSAVE_KEY); } catch (_) { /* ignore */ }
    $("#st-saved") && ($("#st-saved").textContent = "Exported");
  }

  // ================================================================== tag links

  function links() {
    const key = `${S.version}|${S.mode}|${[...S.sel].join(",")}|${S.doc?.name}`;
    if (key !== X.linksKey) {
      X.linksKey = key;
      X.links = S.doc && !S.dukeMap ? tagLinks(S.doc, S.mode, S.sel) : null;
    }
    return X.links;
  }

  function arrow(g, a, b, color, k) {
    const ang = Math.atan2(b[1] - a[1], b[0] - a[0]);
    g.strokeStyle = color; g.fillStyle = color;
    g.beginPath(); g.moveTo(a[0], a[1]); g.lineTo(b[0], b[1]); g.stroke();
    g.setLineDash([]);
    g.beginPath();
    g.moveTo(b[0], b[1]);
    g.lineTo(b[0] - Math.cos(ang - 0.4) * k, b[1] - Math.sin(ang - 0.4) * k);
    g.lineTo(b[0] - Math.cos(ang + 0.4) * k, b[1] - Math.sin(ang + 0.4) * k);
    g.closePath(); g.fill();
  }

  /** Linked sectors / lines / things and arrows (after the lines are drawn). */
  function drawLinks(g, geo, dpr, W, H) {
    const L = links();
    if (!L || (!L.sectors.size && !L.lines.size && !L.things.size)) return;
    const d = S.doc, sc = S.view.scale;
    const LINK = "#34d6e8", TELE = "#e07bff";
    g.save();
    g.setTransform(dpr * sc, 0, 0, -dpr * sc, dpr * (W / 2 - S.view.x * sc), dpr * (H / 2 + S.view.y * sc));
    for (const s of L.sectors) {
      const p = geo.paths[s];
      if (!p) continue;
      g.fillStyle = "rgba(52,214,232,0.22)";
      g.fill(p, "evenodd");
      g.lineWidth = 2 / sc; g.strokeStyle = LINK;
      g.stroke(p);
    }
    g.restore();
    g.lineWidth = 3; g.strokeStyle = LINK;
    for (const li of L.lines) {
      const l = d.linedefs[li], a = d.vertices[l.v1], b = d.vertices[l.v2];
      const [ax, ay] = ctx.toScreen(a.x, a.y), [bx, by] = ctx.toScreen(b.x, b.y);
      g.beginPath(); g.moveTo(ax, ay); g.lineTo(bx, by); g.stroke();
    }
    for (const ti of L.things) {
      const t = d.things[ti];
      const [x, y] = ctx.toScreen(t.x, t.y);
      g.lineWidth = 2.5; g.strokeStyle = LINK;
      g.beginPath(); g.arc(x, y, Math.max(7, 20 * sc), 0, Math.PI * 2); g.stroke();
    }
    g.lineWidth = 1.5;
    for (const a of L.arrows) {
      g.setLineDash(a.teleport ? [7, 4] : [3, 4]);
      arrow(g, ctx.toScreen(a.from.x, a.from.y), ctx.toScreen(a.to.x, a.to.y), a.teleport ? TELE : LINK, 9);
    }
    g.setLineDash([]);
    const parts = [L.sectors.size && plural(L.sectors.size, "sector"), L.lines.size && plural(L.lines.size, "line"), L.things.size && plural(L.things.size, "thing")].filter(Boolean);
    const msg = `Linked: ${parts.join(" · ")}`;
    g.font = "12px system-ui, sans-serif";
    const w = g.measureText(msg).width + 16;
    g.fillStyle = "rgba(10,12,16,0.78)";
    // Above the hint line at the bottom of the view.
    g.fillRect(W - w - 8, H - 58, w, 22);
    g.fillStyle = LINK;
    g.fillText(msg, W - w, H - 43);
  }

  // ================================================================== tools: cut / measure / shapes

  const shape = () => (S.mode === "draw" ? $("#draw-shape")?.value || "polygon" : "polygon");
  const snapPt = (wx, wy) => ({ x: ctx.clampI16(ctx.snap(wx)), y: ctx.clampI16(ctx.snap(wy)) });

  /** Cut ends: a vertex near the mouse, else the nearest point on a line. */
  function cutPoint(wx, wy) {
    const d = S.doc, tol = 9 / S.view.scale;
    let best = null, bd = tol;
    for (const v of d.vertices) {
      const dd = Math.hypot(v.x - wx, v.y - wy);
      if (dd < bd) { bd = dd; best = { x: v.x, y: v.y, vertex: true }; }
    }
    if (best) return best;
    bd = tol;
    for (const l of d.linedefs) {
      const a = d.vertices[l.v1], b = d.vertices[l.v2];
      const dx = b.x - a.x, dy = b.y - a.y, len2 = dx * dx + dy * dy;
      if (!len2) continue;
      let t = ((wx - a.x) * dx + (wy - a.y) * dy) / len2;
      if (t <= 0 || t >= 1) continue;
      // On a grid line where the wall crosses one, if that is close.
      const px = a.x + t * dx, py = a.y + t * dy;
      const dd = Math.hypot(px - wx, py - wy);
      if (dd < bd) { bd = dd; best = { x: px, y: py }; }
    }
    return best;
  }

  /** Mouse down on the 2D map (left button). Returns true when handled. */
  function mouseDown(wx, wy, e) {
    if (!S.doc || S.dukeMap) return false;
    if (X.armed) {
      const p = X.armed;
      X.armed = null;
      ctx.tools.setClip(p.clip);
      ctx.tools.paste();
      ctx.status(`Placed prefab ${p.name}`);
      return true;
    }
    if (S.mode === "cut") {
      const p = cutPoint(wx, wy);
      if (!p) { ctx.toast("Start and end the cut on a wall or a corner"); return true; }
      if (!X.tool) { X.tool = { kind: "cut", a: p }; return true; }
      const a = X.tool.a;
      X.tool = null;
      ctx.snapshot();
      const r = splitSector(S.doc, a, p, S.texture || "-");
      if (typeof r === "string") { S.undo.pop(); ctx.toast(r); return true; }
      S.sel = new Set();
      ctx.commit();
      ctx.status(`Sector ${r.sector} split — the new part is sector ${r.newSector}`);
      return true;
    }
    if (S.mode === "measure") {
      const p = S.snap ? snapPt(wx, wy) : { x: wx, y: wy };
      X.measure = { a: p, b: p, live: true };
      return true;
    }
    if (S.mode === "draw" && shape() !== "polygon") {
      const p = snapPt(wx, wy);
      X.tool = { kind: "shape", a: p, b: p };
      S.drawPts = [];
      return true;
    }
    return false;
  }

  function mouseMove(wx, wy) {
    if (S.mode === "measure" && X.measure?.live) {
      X.measure.b = S.snap ? snapPt(wx, wy) : { x: wx, y: wy };
      return true;
    }
    if (X.tool?.kind === "shape") { X.tool.b = snapPt(wx, wy); return true; }
    if (S.mode === "cut" || X.armed) return true; // repaint the preview
    return false;
  }

  function mouseUp() {
    if (X.measure?.live) { X.measure.live = false; return true; }
    if (X.tool?.kind === "shape") {
      const { a, b } = X.tool;
      X.tool = null;
      const sides = Math.max(3, Math.min(128, Number($("#draw-sides")?.value) || 16));
      const pts = shape() === "rect" ? rectPoints(a, b) : ellipsePoints(a, b, sides);
      if (pts.length < 3) return true;
      S.drawPts = pts;
      ctx.finishDraw();
      return true;
    }
    return false;
  }

  /** Previews and readouts on top of the map. */
  function drawTool(g, W, H) {
    const m = ctx.mouse.world;
    g.lineWidth = 2;
    if (X.tool?.kind === "cut" && m) {
      const p = cutPoint(m[0], m[1]) || { x: m[0], y: m[1] };
      const a = ctx.toScreen(X.tool.a.x, X.tool.a.y), b = ctx.toScreen(p.x, p.y);
      g.strokeStyle = "#ff7a59"; g.setLineDash([6, 4]);
      g.beginPath(); g.moveTo(a[0], a[1]); g.lineTo(b[0], b[1]); g.stroke();
      g.setLineDash([]);
      g.fillStyle = "#ff7a59"; g.fillRect(a[0] - 4, a[1] - 4, 8, 8);
    } else if (S.mode === "cut" && m) {
      const p = cutPoint(m[0], m[1]);
      if (p) { const s = ctx.toScreen(p.x, p.y); g.strokeStyle = "#ff7a59"; g.strokeRect(s[0] - 5, s[1] - 5, 10, 10); }
    }
    if (X.tool?.kind === "shape") {
      const { a, b } = X.tool;
      const sides = Math.max(3, Math.min(128, Number($("#draw-sides")?.value) || 16));
      const pts = shape() === "rect" ? rectPoints(a, b) : ellipsePoints(a, b, sides);
      if (pts.length) {
        g.strokeStyle = "#4cc38a";
        g.beginPath();
        pts.forEach((p, i) => { const [x, y] = ctx.toScreen(p.x, p.y); if (i) g.lineTo(x, y); else g.moveTo(x, y); });
        g.closePath(); g.stroke();
        const [x, y] = ctx.toScreen(Math.max(a.x, b.x), Math.min(a.y, b.y));
        label(g, `${Math.abs(b.x - a.x)} × ${Math.abs(b.y - a.y)}`, x + 8, y + 16, "#4cc38a");
      }
    }
    if (S.mode === "measure" && X.measure) {
      const { a, b } = X.measure;
      const r = measure(a, b);
      const pa = ctx.toScreen(a.x, a.y), pb = ctx.toScreen(b.x, b.y);
      g.strokeStyle = "#ffd24a"; g.setLineDash([]);
      g.beginPath(); g.moveTo(pa[0], pa[1]); g.lineTo(pb[0], pb[1]); g.stroke();
      for (const p of [pa, pb]) { g.fillStyle = "#ffd24a"; g.fillRect(p[0] - 3, p[1] - 3, 6, 6); }
      if (r.dist > 0) label(g, `${r.dist.toFixed(r.dist % 1 ? 1 : 0)} units · Δx ${Math.round(r.dx)} · Δy ${Math.round(r.dy)} · ${r.angle.toFixed(1)}°`, (pa[0] + pb[0]) / 2 + 10, (pa[1] + pb[1]) / 2 - 10, "#ffd24a");
    }
    if (X.hinge != null && S.mode === "sectors" && S.doc?.linedefs[X.hinge]) {
      const l = S.doc.linedefs[X.hinge], a = S.doc.vertices[l.v1], b = S.doc.vertices[l.v2];
      const pa = ctx.toScreen(a.x, a.y), pb = ctx.toScreen(b.x, b.y);
      g.strokeStyle = "#ff5ab4"; g.lineWidth = 4;
      g.beginPath(); g.moveTo(pa[0], pa[1]); g.lineTo(pb[0], pb[1]); g.stroke();
      label(g, "slope hinge", (pa[0] + pb[0]) / 2 + 8, (pa[1] + pb[1]) / 2, "#ff5ab4");
    }
    if (X.armed && m) {
      const [x, y] = ctx.toScreen(m[0], m[1]);
      const [w, h] = prefabSize(X.armed.clip);
      g.strokeStyle = "#ffd24a"; g.setLineDash([5, 4]); g.lineWidth = 1.5;
      g.strokeRect(x - (w * S.view.scale) / 2, y - (h * S.view.scale) / 2, w * S.view.scale, h * S.view.scale);
      g.setLineDash([]);
      label(g, `Place ${X.armed.name}`, x + 10, y - 10, "#ffd24a");
    }
  }

  function label(g, text, x, y, color) {
    g.font = "12px system-ui, sans-serif";
    const w = g.measureText(text).width + 10;
    g.fillStyle = "rgba(10,12,16,0.82)";
    g.fillRect(x - 5, y - 13, w, 18);
    g.fillStyle = color;
    g.fillText(text, x, y);
  }

  function onKey(e, { mod }) {
    if (mod && e.code === "KeyF" && S.doc && !S.dukeMap) { e.preventDefault(); openFind(); return true; }
    if (e.code === "Escape" && (X.tool || X.measure || X.armed)) {
      X.tool = null; X.measure = null;
      if (X.armed) { X.armed = null; ctx.status("Prefab placement cancelled"); }
      ctx.render2d();
      return true;
    }
    return false;
  }

  /** Box selection: with a Find filter on, only matching objects are picked. */
  function allowBox(mode, i) {
    const f = X.filter;
    if (!f || FIND_KINDS[f.kind]?.mode !== mode) return true;
    return matches(S.doc, f.kind, i, f.value);
  }

  // ================================================================== inspector panels

  const num = (attrs) => el("input", { type: "number", class: "xnum", ...attrs });
  const btn = (label, onclick, title = "") => el("button", { class: "btn small", onclick, title }, label);

  /** Stairs, gradients, slope and merge for the selected sectors. */
  function sectorPanel(idx) {
    const d = S.doc;
    const n = idx.length;
    const fs = el("fieldset", { class: "xshape" }, el("legend", {}, "Shape"));
    const order = () => orderSectors(d, idx);
    // Stairs.
    const step = num({ value: String(X.lastStep ?? 16), step: "8", title: "Height change from one sector to the next" });
    const what = el("select", {}, el("option", { value: "floor" }, "floors"), el("option", { value: "ceiling" }, "ceilings"), el("option", { value: "both" }, "both"));
    what.value = X.lastWhat || "floor";
    fs.append(el("div", { class: "xrow" }, el("span", {}, "Stairs"), step, what,
      btn("Build", () => {
        if (n < 2) { ctx.toast("Select two or more sectors, in walking order from the bottom step"); return; }
        X.lastStep = Number(step.value) || 0; X.lastWhat = what.value;
        ctx.snapshot();
        buildStairs(d, order(), X.lastStep, what.value);
        ctx.commit();
        ctx.status(`Stairs over ${plural(n, "sector")}, ${X.lastStep} per step`);
      }, "Each sector a step above the previous one, from the first one selected")));
    // Gradients.
    const grad = (key, label) => btn(label, () => {
      if (n < 3) { ctx.toast("Select three or more sectors: the first and last set the range"); return; }
      ctx.snapshot();
      gradient(d, order(), key);
      ctx.commit();
    }, `Blend ${label.toLowerCase()} from the first selected sector to the last`);
    fs.append(el("div", { class: "xrow" }, el("span", {}, "Gradient"), grad("light", "Light"), grad("floor_height", "Floor"), grad("ceiling_height", "Ceiling")));
    // Slope.
    const udmf = d.format === "Udmf";
    if (!udmf) {
      fs.append(el("p", { class: "muted xnote" }, "Slopes need a UDMF map — convert it in the Map inspector."));
    } else {
      const edges = sectorEdges(d, idx[0]);
      const hinge = el("select", { title: "The edge that stays level" });
      for (const [a, b, li] of edges) {
        const va = d.vertices[a], vb = d.vertices[b];
        hinge.append(el("option", { value: String(li) }, `line ${li} · ${Math.round(Math.hypot(vb.x - va.x, vb.y - va.y))} long`));
      }
      if (X.hinge != null && edges.some((e) => e[2] === X.hinge)) hinge.value = String(X.hinge);
      X.hinge = Number(hinge.value);
      hinge.addEventListener("change", () => { X.hinge = Number(hinge.value); ctx.render2d(); });
      const plane = el("select", {}, el("option", { value: "floor" }, "floor"), el("option", { value: "ceiling" }, "ceiling"));
      const rise = num({ value: String(X.lastRise ?? 32), step: "8", title: "Height gained at the far side (negative: down)" });
      fs.append(el("div", { class: "xrow" }, el("span", {}, "Slope"), plane, el("span", { class: "muted" }, "rise"), rise),
        el("div", { class: "xrow" }, el("span", {}, "from"), hinge,
          btn("Slope", () => {
            X.lastRise = Number(rise.value) || 0;
            ctx.snapshot();
            if (!slopeSectors(d, idx, plane.value, Number(hinge.value), X.lastRise)) { S.undo.pop(); ctx.toast("That edge cannot be a hinge for this selection"); return; }
            ctx.commit();
            ctx.status(`${n > 1 ? `${n} sectors` : "Sector"} ${plane.value} sloped ${X.lastRise} units`);
          }, "Level along the hinge, rising toward the farthest point of the selection"),
          btn("Flatten", () => { ctx.snapshot(); clearSlope(d, idx, plane.value); ctx.commit(); }, "Remove the slope")));
    }
    if (n >= 2) {
      fs.append(el("div", { class: "xrow" }, btn("Merge sectors", () => {
        ctx.snapshot();
        const kept = mergeSectors(d, idx);
        S.sel = new Set([kept]);
        ctx.commit();
        ctx.status(`Merged into sector ${kept}`);
      }, "Join the selection into its first sector; walls between them are removed")));
    }
    return fs;
  }

  /** Curve for the selected lines. */
  function linePanel(idx) {
    const segs = num({ value: String(X.lastSegs ?? 8), min: "2", max: "64", title: "Pieces" });
    const bulge = num({ value: String(X.lastBulge ?? 32), step: "8", title: "Distance of the middle from the straight line (negative: the other side)" });
    return el("fieldset", { class: "xshape" }, el("legend", {}, "Curve"),
      el("div", { class: "xrow" }, el("span", {}, "pieces"), segs, el("span", {}, "bulge"), bulge,
        btn("Curve", () => {
          X.lastSegs = Number(segs.value) || 8; X.lastBulge = Number(bulge.value) || 0;
          if (!X.lastBulge) return;
          ctx.snapshot();
          // Highest index first: splitting shifts the lines after it.
          const out = [];
          for (const li of [...idx].sort((a, b) => b - a)) {
            const made = curveLine(S.doc, li, X.lastSegs, X.lastBulge);
            for (let k = 0; k < out.length; k++) out[k] += made.length - 1;
            out.push(...made);
          }
          S.sel = new Set(out);
          ctx.commit();
        }, "Bend the line into an arc (left of its direction)")));
  }

  // ================================================================== floating panels

  function panel(id, title, body) {
    let p = $(`#${id}`);
    if (p) { p.hidden = false; return p; }
    p = el("div", { id, class: "xpanel", role: "dialog", "aria-label": title },
      el("div", { class: "gen-head" }, el("strong", {}, title),
        el("button", { class: "btn small icon", title: "Close (Esc)", "aria-label": "Close", onclick: () => { p.hidden = true; } }, "×")),
      el("div", { class: "gen-body" }, body));
    p.addEventListener("keydown", (e) => { if (e.code === "Escape") { p.hidden = true; e.stopPropagation(); } e.stopPropagation(); });
    document.body.append(p);
    return p;
  }

  // ---- Find & replace
  function openFind() {
    const kind = el("select", { id: "find-kind" }, ...Object.entries(FIND_KINDS).map(([k, v]) => el("option", { value: k }, v.label)));
    const find = el("input", { type: "text", id: "find-value", placeholder: "STARTAN2, 3004, 12…", autocomplete: "off" });
    const repl = el("input", { type: "text", id: "find-replace", placeholder: "new value (optional)", autocomplete: "off" });
    const out = el("p", { class: "muted", id: "find-result" }, "");
    const filter = el("input", { type: "checkbox", id: "find-filter" });
    const count = () => {
      if (!S.doc || !find.value.trim()) { out.textContent = ""; return []; }
      const hits = findAll(S.doc, kind.value, find.value);
      out.textContent = `${plural(hits.length, "match")} in ${FIND_KINDS[kind.value].mode}`;
      return hits;
    };
    const syncFilter = () => {
      X.filter = filter.checked && find.value.trim() ? { kind: kind.value, value: find.value } : null;
      ctx.status(X.filter ? `Box selection picks only ${FIND_KINDS[kind.value].label.toLowerCase()} ${find.value}` : "Box selection picks everything");
    };
    for (const i of [kind, find]) i.addEventListener("input", () => { count(); if (filter.checked) syncFilter(); });
    filter.addEventListener("change", syncFilter);
    const body = el("div", { class: "xcol" },
      el("label", {}, "Look for", kind),
      el("label", {}, "Value", find),
      el("label", {}, "Replace with", repl),
      out,
      el("div", { class: "gen-row" },
        el("button", { class: "btn primary", onclick: () => {
          const hits = count();
          if (!hits.length) return;
          ctx.tools.show(FIND_KINDS[kind.value].mode, hits);
        } }, "Select all"),
        el("button", { class: "btn", onclick: () => {
          if (!repl.value.trim()) { ctx.toast("Enter a replacement value"); return; }
          ctx.snapshot();
          const n = replaceAll(S.doc, kind.value, find.value, repl.value);
          if (!n) { S.undo.pop(); ctx.toast("Nothing to replace"); return; }
          ctx.commit();
          count();
          ctx.toast(`Replaced ${plural(n, "value")}`);
        } }, "Replace all")),
      el("label", { class: "xcheck" }, filter, "Box selection picks only matches"));
    const p = panel("find-panel", "Find & replace", body);
    p.hidden = false;
    ($("#find-value") || find).focus();
  }

  // ---- Prefabs
  const loadPrefabs = () => {
    try { return (JSON.parse(localStorage.getItem(PREFAB_KEY) || "[]") || []).filter(isPrefab); } catch (_) { return []; }
  };
  const savePrefabs = (list) => {
    try { localStorage.setItem(PREFAB_KEY, JSON.stringify(list)); return true; } catch (e) {
      ctx.toast(`Could not save prefabs: ${ctx.errText(e)}`, true);
      return false;
    }
  };
  function downloadText(text, name) {
    const url = URL.createObjectURL(new Blob([text], { type: "application/json" }));
    const a = el("a", { href: url, download: name });
    document.body.append(a);
    a.click();
    a.remove();
    setTimeout(() => URL.revokeObjectURL(url), 1000);
  }

  function importPrefabs() {
    const input = el("input", { type: "file", accept: ".json,application/json" });
    input.addEventListener("change", async () => {
      const f = input.files?.[0];
      if (!f) return;
      try {
        const r = mergePrefabs(loadPrefabs(), await f.text());
        if (!r.added.length) { ctx.toast(`No prefabs in ${f.name}`, true); return; }
        if (savePrefabs(r.list)) renderPrefabs(r.list);
        ctx.toast(`Imported ${plural(r.added.length, "prefab")}${r.skipped ? ` (${r.skipped} unreadable skipped)` : ""}`);
      } catch (e) { ctx.toast(`${f.name}: ${ctx.errText(e)}`, true); }
    });
    input.click();
  }

  /** Small drawing of a prefab: its lines (one-sided bright) and things. */
  function prefabThumb(clip) {
    const c = el("canvas", { class: "prefab-thumb", width: 56, height: 56 });
    const g = c.getContext("2d");
    const { segs, dots } = prefabOutline(clip, 56, 4);
    g.lineWidth = 1.5;
    for (const [x1, y1, x2, y2, two] of segs) {
      g.strokeStyle = two ? "#6b7380" : "#d7dbe0";
      g.beginPath(); g.moveTo(x1, y1); g.lineTo(x2, y2); g.stroke();
    }
    for (const [x, y, type] of dots) {
      g.fillStyle = ctx.thingColor ? ctx.thingColor(type) : "#ff5a4a";
      g.beginPath(); g.arc(x, y, 2.5, 0, Math.PI * 2); g.fill();
    }
    return c;
  }

  function renderPrefabs(list) {
    const host = $("#prefab-list");
    if (!host) return;
    host.replaceChildren();
    if (!list.length) {
      host.append(el("p", { class: "muted" }, "No prefabs yet. Select sectors, lines or things, then Save selection."));
      return;
    }
    for (const p of list) {
      const [w, h] = prefabSize(p.clip);
      const what = p.clip.kind === "sectors" ? plural(p.clip.sectors.length, "sector") : p.clip.kind === "lines" ? plural(p.clip.lines.length, "line") : plural(p.clip.things.length, "thing");
      host.append(el("div", { class: "prefab-row" },
        el("button", { class: "btn prefab-use", title: "Click, then click on the map to place it", onclick: () => {
          X.armed = p;
          ctx.status(`Click on the map to place ${p.name} · Esc cancels`);
          ctx.render2d();
        } }, prefabThumb(p.clip), el("span", { class: "prefab-text" }, el("strong", {}, p.name), el("span", { class: "muted" }, `${what} · ${w}×${h}`))),
        el("button", { class: "btn small icon", title: `Download ${p.name}`, "aria-label": `Download ${p.name}`,
          onclick: () => downloadText(prefabsToFile([p]), `${p.name.replace(/[^\w.-]+/g, "_") || "prefab"}.json`) }, "⬇"),
        el("button", { class: "btn small icon", title: `Delete ${p.name}`, "aria-label": `Delete ${p.name}`, onclick: () => {
          if (!confirm(`Delete the prefab ${p.name}?`)) return;
          const next = loadPrefabs().filter((q) => q.name !== p.name);
          if (savePrefabs(next)) renderPrefabs(next);
        } }, "×")));
    }
  }
  function openPrefabs() {
    const body = el("div", { class: "xcol" },
      el("p", { class: "gen-intro muted" }, "Save a door, lift or teleporter pad once, then place it in any map."),
      el("button", { class: "btn primary", onclick: () => {
        if (!S.doc || !S.sel.size || !ctx.tools.copy()) { ctx.toast("Select sectors, lines or things to save"); return; }
        const name = (prompt("Prefab name:", "door") || "").trim();
        if (!name) return;
        const list = loadPrefabs().filter((q) => q.name !== name);
        list.unshift({ name, clip: ctx.tools.getClip(), time: Date.now() });
        if (savePrefabs(list)) { renderPrefabs(list); ctx.status(`Saved prefab ${name}`); }
      } }, "Save selection"),
      el("div", { class: "gen-row" },
        el("button", { class: "btn", title: "Download every prefab as one file to share or back up", onclick: () => {
          const list = loadPrefabs();
          if (!list.length) { ctx.toast("No prefabs to download"); return; }
          downloadText(prefabsToFile(list), "room-prefabs.json");
        } }, "⬇ Download all"),
        el("button", { class: "btn", title: "Add the prefabs of a .json file (yours or someone else's)", onclick: importPrefabs }, "⬆ Import…")),
      el("div", { id: "prefab-list", class: "xcol" }));
    panel("prefab-panel", "Prefabs", body);
    renderPrefabs(loadPrefabs());
  }

  // ---- Background image
  function openTrace() {
    const file = el("input", { type: "file", accept: "image/*" });
    const alpha = el("input", { type: "range", min: "0.05", max: "1", step: "0.05", value: String(X.bg?.alpha ?? 0.5) });
    const scale = num({ value: String(X.bg?.scale ?? 1), step: "0.25", min: "0.01", title: "Map units per image pixel" });
    const bx = num({ value: String(X.bg?.x ?? 0), step: "8", title: "Map x of the image's left edge" });
    const by = num({ value: String(X.bg?.y ?? 0), step: "8", title: "Map y of the image's top edge" });
    const sync = () => {
      if (!X.bg) return;
      Object.assign(X.bg, { alpha: Number(alpha.value), scale: Number(scale.value) || 1, x: Number(bx.value) || 0, y: Number(by.value) || 0 });
      ctx.render2d();
    };
    for (const i of [alpha, scale, bx, by]) i.addEventListener("input", sync);
    file.addEventListener("change", () => {
      const f = file.files?.[0];
      if (!f) return;
      const img = new Image();
      img.onload = () => {
        const k = Number(scale.value) || 1;
        // Centre it on the view the first time.
        const x = Math.round(S.view.x - (img.width * k) / 2), y = Math.round(S.view.y + (img.height * k) / 2);
        X.bg = { img, name: f.name, x, y, scale: k, alpha: Number(alpha.value) };
        bx.value = String(x); by.value = String(y);
        ctx.render2d();
        ctx.status(`Tracing ${f.name} (${img.width}×${img.height} px)`);
      };
      img.onerror = () => ctx.toast("That file is not an image the browser can read", true);
      img.src = URL.createObjectURL(f);
    });
    const body = el("div", { class: "xcol" },
      el("p", { class: "gen-intro muted" }, "A sketch or screenshot under the map to draw over. It is not saved in the map."),
      el("label", {}, "Image", file),
      el("label", {}, "Opacity", alpha),
      el("label", {}, "Map units per pixel", scale),
      el("div", { class: "gen-row" }, el("label", {}, "Left (x)", bx), el("label", {}, "Top (y)", by)),
      el("div", { class: "gen-row" },
        el("button", { class: "btn", onclick: () => {
          if (!X.bg) return;
          bx.value = String(Math.round(S.view.x - (X.bg.img.width * X.bg.scale) / 2));
          by.value = String(Math.round(S.view.y + (X.bg.img.height * X.bg.scale) / 2));
          sync();
        } }, "Centre in view"),
        el("button", { class: "btn danger", onclick: () => { X.bg = null; ctx.render2d(); } }, "Remove")));
    panel("trace-panel", "Background image", body);
  }

  /** Under the map, after the grid. */
  function drawBackground(g) {
    const b = X.bg;
    if (!b || S.dukeMap) return;
    const [x0, y0] = ctx.toScreen(b.x, b.y);
    const [x1, y1] = ctx.toScreen(b.x + b.img.width * b.scale, b.y - b.img.height * b.scale);
    g.save();
    g.globalAlpha = b.alpha;
    g.imageSmoothingEnabled = S.view.scale * b.scale < 4;
    g.drawImage(b.img, x0, y0, x1 - x0, y1 - y0);
    g.restore();
  }

  // ================================================================== hints

  const HINTS = {
    cut: "Click a wall or corner, then another on the same sector: the sector splits along the cut · Esc cancels",
    measure: "Drag to measure distance, Δx, Δy and angle · G toggles snapping",
  };

  /** Mode changed: drop half-made gestures. */
  function reset() { X.tool = null; X.measure = null; }

  return {
    reset, autosave, offerRestore, noteAutosave, onExported, onSummary: () => { lumpGen++; },
    openSnapshots, saveSnapshot,
    drawLinks, drawTool, drawBackground, mouseDown, mouseMove, mouseUp, onKey, allowBox,
    sectorPanel, linePanel, openFind, openPrefabs, openTrace, HINTS,
    links,
  };
}
