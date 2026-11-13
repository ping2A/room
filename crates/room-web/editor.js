// Room web editor: 2D map editing + live 3D / play test through the Room engine (WASM).
// The map document is the engine's `RawMap` as JSON; every change is pushed back with
// `roomEditorUpdateMap`, which patches the running level in place when it can.

import { install as installTools, HEXEN_LINE_SPECIALS } from "./editor-tools.js";
import { install as installPaint, paintNs } from "./editor-paint.js";
import { install as installScripts, highlight, textLang, includesOf, zscriptClosure } from "./editor-scripts.js";

/** Keep the native Duke editor separate from the Doom map/resource browser. */
const ROOM_UI_SHOW_DUKE = true;
const isDukeGrpBytes = (buf) =>
  buf && buf.length >= 12 && String.fromCharCode(...buf.slice(0, 12)) === "KenSilverman";

const $ = (sel) => document.querySelector(sel);
const el = (tag, attrs = {}, ...kids) => {
  const e = document.createElement(tag);
  for (const [k, v] of Object.entries(attrs)) {
    if (k === "class") e.className = v;
    else if (k === "style") e.style.cssText = v;
    else if (k.startsWith("on")) e.addEventListener(k.slice(2), v);
    else if (v === true) e.setAttribute(k, "");
    else if (v !== false && v != null) e.setAttribute(k, v);
  }
  for (const kid of kids.flat()) if (kid != null) e.append(kid instanceof Node ? kid : document.createTextNode(String(kid)));
  return e;
};

const NO_SIDE = 0xffff;
const FLOAT_KEYS = new Set([
  "alpha", "scale_x", "scale_y", "scale_x_top", "scale_y_top", "scale_x_bottom", "scale_y_bottom",
  "scroll_x", "scroll_y", "gravity", "floor_x_pan", "floor_y_pan", "ceiling_x_pan", "ceiling_y_pan",
  "floor_rotation", "ceiling_rotation", "floor_x_scale", "floor_y_scale", "ceiling_x_scale",
  "ceiling_y_scale", "floor_alpha", "ceiling_alpha",
]);
const TEX_KEYS = new Set(["upper", "middle", "lower"]);
const FLAT_KEYS = new Set(["floor_pic", "ceiling_pic"]);

const DOOM_LINE_FLAGS = [[1, "Impassable"], [2, "Block monsters"], [4, "Two-sided"], [8, "Upper unpegged"],
  [16, "Lower unpegged"], [32, "Secret (map)"], [64, "Block sound"], [128, "Hidden on map"],
  [256, "Shown on map"], [512, "Pass use (Boom)"]];
const HEXEN_LINE_FLAGS = [[1, "Impassable"], [2, "Block monsters"], [4, "Two-sided"], [8, "Upper unpegged"],
  [16, "Lower unpegged"], [32, "Secret (map)"], [64, "Block sound"], [128, "Hidden on map"],
  [256, "Shown on map"], [512, "Repeatable"]];
const SPAC_BITS = [[1, "Player cross"], [2, "Player use"], [4, "Monster cross"], [8, "Impact"],
  [16, "Push"], [32, "Projectile cross"], [64, "Use through"], [128, "Any cross"], [256, "Monster use"],
  [512, "Monster push"], [1024, "Use back"]];
const HEXEN_ACTIVATION = ["Player cross", "Player use", "Monster cross", "Impact", "Push", "Projectile cross"];
const DOOM_THING_FLAGS = [[1, "Easy"], [2, "Medium"], [4, "Hard"], [8, "Ambush"], [16, "Multiplayer only"],
  [32, "Not deathmatch"], [64, "Not coop"], [128, "Friendly"]];
const HEXEN_THING_FLAGS = [[1, "Easy"], [2, "Medium"], [4, "Hard"], [8, "Ambush"], [16, "Dormant"],
  [32, "Fighter"], [64, "Cleric"], [128, "Mage"], [256, "Single"], [512, "Coop"], [1024, "Deathmatch"]];
const DOOM_SECTOR_SPECIALS = { 0: "Normal", 1: "Light blinks (random)", 2: "Light blinks 0.5s", 3: "Light blinks 1s",
  4: "20% damage + blink", 5: "10% damage", 7: "5% damage", 8: "Light glows", 9: "Secret",
  10: "Door closes after 30s", 11: "20% damage, exit on death", 12: "Blink 1s (sync)", 13: "Blink 0.5s (sync)",
  14: "Door opens after 5 min", 16: "20% damage", 17: "Light flickers" };
const DOOM_LINE_SPECIALS = { 0: "None", 1: "DR Door", 2: "W1 Door open stay", 3: "W1 Door close", 4: "W1 Door",
  5: "W1 Floor raise to lowest ceiling", 7: "S1 Stairs raise 8", 8: "W1 Stairs raise 8", 9: "S1 Donut",
  10: "W1 Lift", 11: "S1 Exit level", 12: "W1 Light to brightest neighbour", 13: "W1 Light 255",
  14: "S1 Floor raise 32 + tex", 15: "S1 Floor raise 24 + tex", 16: "W1 Door close 30s then open",
  17: "W1 Light blink", 18: "S1 Floor raise to next higher", 19: "W1 Floor lower to highest neighbour",
  20: "S1 Floor raise next + tex", 21: "S1 Lift", 22: "W1 Floor raise next + tex", 23: "S1 Floor lower to lowest",
  24: "G1 Floor raise to lowest ceiling", 25: "W1 Crusher", 26: "DR Door (blue key)", 27: "DR Door (yellow key)",
  28: "DR Door (red key)", 29: "S1 Door", 30: "W1 Floor raise by shortest lower", 31: "D1 Door open stay",
  32: "D1 Door blue open stay", 33: "D1 Door red open stay", 34: "D1 Door yellow open stay", 35: "W1 Light 35",
  36: "W1 Floor lower to 8 above highest", 37: "W1 Floor lower to lowest + tex", 38: "W1 Floor lower to lowest",
  39: "W1 Teleport", 40: "W1 Ceiling raise to highest", 41: "S1 Ceiling lower to floor", 42: "SR Door close",
  43: "SR Ceiling lower to floor", 44: "W1 Ceiling lower to 8 above floor", 45: "SR Floor lower to highest",
  46: "GR Door open stay", 47: "G1 Floor raise next + tex", 48: "Scroll wall left", 49: "S1 Crusher",
  50: "S1 Door close", 51: "S1 Secret exit", 52: "W1 Exit level", 53: "W1 Perpetual lift", 54: "W1 Lift stop",
  55: "S1 Floor raise to 8 below ceiling (crush)", 56: "W1 Floor raise to 8 below ceiling (crush)",
  57: "W1 Crusher stop", 58: "W1 Floor raise 24", 59: "W1 Floor raise 24 + tex", 60: "SR Floor lower to lowest",
  61: "SR Door open stay", 62: "SR Lift", 63: "SR Door", 64: "SR Floor raise to lowest ceiling",
  65: "SR Floor raise to 8 below ceiling (crush)", 66: "SR Floor raise 24 + tex", 67: "SR Floor raise 32 + tex",
  68: "SR Floor raise next + tex", 69: "SR Floor raise to next higher", 70: "SR Floor lower to 8 above highest",
  71: "S1 Floor lower to 8 above highest", 72: "WR Ceiling lower to 8 above floor", 73: "WR Crusher",
  74: "WR Crusher stop", 75: "WR Door close", 76: "WR Door close 30s then open", 77: "WR Fast crusher",
  78: "SR Floor change tex", 79: "WR Light 35", 80: "WR Light to brightest neighbour", 81: "WR Light 255",
  82: "WR Floor lower to lowest", 83: "WR Floor lower to highest", 84: "WR Floor lower to lowest + tex",
  86: "WR Door open stay", 87: "WR Perpetual lift", 88: "WR Lift", 89: "WR Lift stop", 90: "WR Door",
  91: "WR Floor raise to lowest ceiling", 92: "WR Floor raise 24", 93: "WR Floor raise 24 + tex",
  94: "WR Floor raise to 8 below ceiling (crush)", 95: "WR Floor raise next + tex",
  96: "WR Floor raise by shortest lower", 97: "WR Teleport", 98: "WR Floor lower to 8 above highest",
  99: "SR Blue door open stay (fast)", 100: "W1 Stairs raise 16 (fast)", 101: "S1 Floor raise to lowest ceiling",
  102: "S1 Floor lower to highest", 103: "S1 Door open stay", 104: "W1 Light to darkest neighbour",
  105: "WR Door (fast)", 106: "WR Door open stay (fast)", 107: "WR Door close (fast)", 108: "W1 Door (fast)",
  109: "W1 Door open stay (fast)", 110: "W1 Door close (fast)", 111: "S1 Door (fast)",
  112: "S1 Door open stay (fast)", 113: "S1 Door close (fast)", 114: "SR Door (fast)",
  115: "SR Door open stay (fast)", 116: "SR Door close (fast)", 117: "DR Door (fast)", 118: "D1 Door (fast)",
  119: "W1 Floor raise to next higher", 120: "WR Lift (fast)", 121: "W1 Lift (fast)", 122: "S1 Lift (fast)",
  123: "SR Lift (fast)", 124: "W1 Secret exit", 125: "W1 Teleport (monsters)", 126: "WR Teleport (monsters)",
  127: "S1 Stairs raise 16 (fast)", 128: "WR Floor raise to next higher", 129: "WR Floor raise to next (fast)",
  130: "W1 Floor raise to next (fast)", 131: "S1 Floor raise to next (fast)", 132: "SR Floor raise to next (fast)",
  133: "S1 Blue door open stay (fast)", 134: "SR Red door open stay (fast)", 135: "S1 Red door open stay (fast)",
  136: "SR Yellow door open stay (fast)", 137: "S1 Yellow door open stay (fast)", 138: "SR Light 255",
  139: "SR Light 35", 140: "S1 Floor raise 512", 141: "W1 Silent crusher" };
const CAT_COLORS = { player: "#4cc38a", monster: "#ff5a4a", weapon: "#f0a03c", pickup: "#5ab0ff",
  obstacle: "#b48cff", decoration: "#9aa3ad", special: "#ffd24a" };
const CAT_ORDER = ["player", "monster", "weapon", "pickup", "obstacle", "decoration", "special"];

const S = {
  mod: null,
  summary: null,
  dukeGrp: null,
  dukeMap: null,
  dukeMapBytes: null,
  dukeMapFile: null,
  dukeSelection: null,
  dukeUndo: [],
  dukeRedo: [],
  dukePersistBefore: null,
  dukeDragging: null,
  dukeTesting: false,
  thingInfo: new Map(),
  doc: null,
  docs: {},
  mode: "sectors",
  sel: new Set(),
  hover: -1,
  pick3d: null,
  view: { x: 0, y: 0, scale: 0.5 },
  grid: 32,
  snap: true,
  textured: true,
  dukeFloors: true,
  undo: [],
  redo: [],
  thingType: 3004,
  texture: "STARTAN2",
  flat: "FLOOR4_8",
  playing: false,
  cam: null,
  version: 0,
  geo: null,
  drawPts: [],
  tab: "maps",
  docTab: "map",
  lumpTabs: new Map(),
  openDirs: new Set(), // expanded folders in the Lumps tab ("src:dir/")
  actors: null,
  actorSel: null,
  /** Doom Builder–style immersive 3D edit (M). */
  visual: false,
  /** Layout before entering visual mode (`2d` / `split` / `3d`). */
  layoutBeforeVisual: "split",
};

// ---------------------------------------------------------------- engine boot

const status = (msg) => { $("#st-msg").textContent = msg; };
let toastTimer = 0;
function toast(msg, error = false) {
  const t = $("#toast");
  t.textContent = msg;
  t.className = error ? "error" : "";
  t.hidden = false;
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => { t.hidden = true; }, error ? 6000 : 2600);
}
function syncPill(kind, text) {
  const p = $("#st-sync");
  p.className = `pill ${kind}`;
  p.textContent = text || kind;
}
const errText = (e) => (typeof e === "string" ? e : e?.message || String(e));

let editorLoadingDepth = 0;
async function withEditorLoading(title, detail, work) {
  const overlay = $("#editor-loading");
  editorLoadingDepth++;
  $("#editor-loading-title").textContent = title;
  $("#editor-loading-detail").textContent = detail || "";
  overlay.hidden = false;
  try {
    // Let the browser paint the overlay before synchronous archive parsing starts.
    await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)));
    return await work();
  } catch (e) {
    console.error("editor: file loading", e);
    toast(`Loading failed: ${errText(e)}`, true);
  } finally {
    editorLoadingDepth--;
    if (editorLoadingDepth === 0) overlay.hidden = true;
  }
}

async function boot() {
  const ready = new Promise((res) => {
    window.addEventListener("room-ready", res, { once: true });
    setTimeout(res, 15000);
  });
  const mod = await import("./room_web.js");
  await mod.default();
  S.mod = mod;
  try { applyMute(localStorage.getItem("room/editor-muted") === "1"); } catch (_) { /* private mode */ }
  await ready;
  status("Engine ready");
  const params = new URLSearchParams(location.search);
  if (params.get("midi") !== "opl") {
    fetch(params.get("soundfont") || "soundfont.sf2", { cache: "force-cache" })
      .then((r) => (r.ok ? r.arrayBuffer() : null))
      .then((b) => { if (b && b.byteLength > 64) mod.roomSetSoundfont(new Uint8Array(b)); })
      .catch(() => {});
  }
  const urls = [params.get("iwad")].filter(Boolean);
  for (const url of urls) {
    try {
      const res = await fetch(url, { cache: "no-cache" });
      if (!res.ok) continue;
      const buf = new Uint8Array(await res.arrayBuffer());
      if (buf.length < 12) continue;
      if (!openIwad(buf, url.split("/").pop())) continue;
      const pwad = params.get("pwad");
      if (pwad) {
        const r2 = await fetch(pwad);
        if (r2.ok) addPwad(new Uint8Array(await r2.arrayBuffer()), pwad.split("/").pop());
      }
      const want = params.get("map");
      if (want) {
        const requested = engineMapName(want.toUpperCase());
        if (S.summary.maps.includes(requested)) loadMap(requested);
      }
      return;
    } catch (e) {
      console.warn("editor: autoload", url, e);
    }
  }
  setIwadLabel("");
  document.body.classList.add("editor-start");
  status("Choose Doom or Duke Nukem 3D to start the matching map editor");
}

function displayMapName(name) {
  if (!S.summary?.duke) return name;
  return String(name).replace(/^E(\d+)M(\d+)$/i, "E$1L$2");
}

function engineMapName(name) {
  if (!S.summary?.duke) return name;
  return String(name).replace(/^E(\d+)L(\d+)$/i, "E$1M$2");
}

function setSummary(json) {
  S.summary = JSON.parse(json);
  S.zsKeys = null;
  S.thingInfo = new Map(S.summary.things.map((t) => [t.num, t]));
  const sel = $("#map-select");
  const cur = S.doc?.name;
  const mapOptions = [el("option", { value: "", selected: !cur }, "Choose map…")];
  mapOptions.push(...S.summary.maps.map((m) => el("option", { value: m, selected: m === cur }, displayMapName(m))));
  for (const name of Object.keys(S.docs)) {
    if (!S.summary.maps.includes(name)) mapOptions.push(el("option", { value: name, selected: name === cur }, `${displayMapName(name)} (new)`));
  }
  sel.replaceChildren(...mapOptions);
  sel.disabled = !S.summary.maps.length && !Object.keys(S.docs).length;
  document.body.classList.remove("editor-start");
  if (S.summary.decorateError) toast(`DECORATE: ${S.summary.decorateError}`, true);
  S.actors = null;
  document.body.classList.toggle("duke-editor", !!S.summary.duke);
  const modeBadge = $("#editor-game-mode");
  modeBadge.hidden = false;
  modeBadge.textContent = S.summary.duke ? "DUKE BUILD MAPS" : "DOOM MAPS";
  const gameTab = $("#browser-tabs [data-tab=\"iwad\"]");
  gameTab.textContent = S.summary.duke ? "GRP" : "Game";
  if (S.summary.duke && !["iwad", "maps", "textures"].includes(S.tab)) S.tab = "maps";
  for (const tab of document.querySelectorAll("#browser-tabs [data-tab]")) {
    tab.hidden = !!S.summary.duke && !["iwad", "maps", "textures"].includes(tab.dataset.tab);
  }
  if (S.summary.duke) {
    applyLayout("split");
    $("#btn-export").textContent = "Export GRP";
    $("#btn-export").title = "Download a native Duke GRP with Build map and ART tile edits (Ctrl+S)";
    $("#btn-play").textContent = "▶ Native test";
    $("#btn-play").title = "Play the selected Build map in the native Duke runtime (F5)";
    $("#btn-play-here").hidden = true;
    $("#btn-new-map").hidden = false;
    $("#btn-new-map").textContent = "New Build map";
    $("#btn-new-map").title = "Create an editable native Build .MAP in the working Duke GRP";
    $("#btn-visual").hidden = true;
    $("#btn-check").hidden = true;
    $("#btn-composer").hidden = true;
    $("#btn-export-pk3").hidden = true;
    $("#btn-open-pk3").hidden = true;
    document.querySelector(".packs").hidden = true;
    document.querySelector(".layout").hidden = true;
    $("#textured").parentElement.hidden = true;
    $("#duke-floor-toggle").hidden = false;
    ["#btn-copy", "#btn-paste", "#btn-rotate", "#btn-mirror"].forEach((id) => { $(id).hidden = true; });
    const labels = { vertices: "Vertices", lines: "Walls", sectors: "Sectors", things: "Sprites", draw: "Draw" };
    document.querySelectorAll("#modes .btn").forEach((b) => {
      b.textContent = labels[b.dataset.mode] || b.textContent;
      b.hidden = false;
    });
  } else {
    $("#btn-export").textContent = "Export WAD";
    $("#btn-export").title = "Download edited maps and resources as a PWAD (Ctrl+S)";
    $("#btn-play").textContent = "▶ Play";
    $("#btn-play").title = "Play from the player start (F5)";
    $("#btn-play-here").hidden = false;
    $("#btn-new-map").hidden = false;
    $("#btn-new-map").textContent = "New map";
    $("#btn-new-map").title = "New map";
    $("#btn-visual").hidden = false;
    $("#btn-check").hidden = false;
    $("#btn-composer").hidden = false;
    $("#btn-export-pk3").hidden = false;
    $("#btn-open-pk3").hidden = false;
    document.querySelector(".packs").hidden = false;
    document.querySelector(".layout").hidden = false;
    $("#textured").parentElement.hidden = false;
    $("#duke-floor-toggle").hidden = true;
    ["#btn-copy", "#btn-paste", "#btn-rotate", "#btn-mirror"].forEach((id) => { $(id).hidden = false; });
    const labels = { vertices: "Vertices", lines: "Lines", sectors: "Sectors", things: "Things", draw: "Draw" };
    document.querySelectorAll("#modes .btn").forEach((b) => {
      b.textContent = labels[b.dataset.mode] || b.textContent;
      b.hidden = false;
    });
  }
  syncPacksUi();
  syncBrowserTab();
  renderBrowser();
}

function setIwadLabel(name) {
  const label = $("#iwad-label");
  if (!label) return;
  label.textContent = name || "—";
  label.title = name
    ? `${S.summary?.duke ? "Duke GRP" : "Doom IWAD / archive"}: ${name} — click for details`
    : "No game archive loaded";
  label.style.cursor = name ? "pointer" : "";
  label.onclick = name ? () => { S.tab = "iwad"; syncBrowserTab(); renderBrowser(); openIwadPage(); } : null;
}

function syncBrowserTab() {
  document.querySelectorAll("#browser-tabs button").forEach((x) => {
    x.classList.toggle("active", x.dataset.tab === S.tab);
  });
}

function openIwadPage() {
  if (!S.summary) { toast("Open an IWAD first", true); return; }
  const id = "iwad-info";
  const view = buildIwadPage();
  S.lumpTabs.set(id, { title: S.summary.duke ? "GRP" : "IWAD", view });
  selectDocTab(id);
}

function buildIwadPage() {
  const sm = S.summary;
  if (sm.duke) {
    const wrap = el("div", { class: "iwad-page duke-grp-page" },
      el("div", { class: "doc-head" },
        el("h2", { class: "iwad-title" }, "Duke Nukem 3D"),
        el("span", { class: "kind map" }, "Native Build")),
      el("p", { class: "iwad-sub" }, sm.duke.grp || "Duke GRP"),
      el("div", { class: "iwad-grid" },
        el("div", { class: "iwad-stat" }, el("div", { class: "n" }, sm.maps.length), el("div", { class: "l" }, "Build maps")),
        el("div", { class: "iwad-stat" }, el("div", { class: "n" }, sm.textures.length), el("div", { class: "l" }, "ART tiles"))),
      el("fieldset", {}, el("legend", {}, "Native maps"),
        el("div", { class: "iwad-maps" }, ...sm.maps.map((map) => el("button", {
          class: "btn small",
          onclick: () => { S.tab = "maps"; syncBrowserTab(); renderBrowser(); loadMap(map); },
        }, displayMapName(map))))),
      el("p", { class: "muted" }, "Duke maps use the Build editor. Doom map formats, flats, actor types and scripts are not shown in this workspace."));
    return wrap;
  }
  const det = sm.iwadDetected;
  const wrap = el("div", { class: "iwad-page" });
  wrap.append(
    el("div", { class: "doc-head" },
      el("h2", { class: "iwad-title" }, det?.name || sm.iwad || "IWAD"),
      el("span", { class: "kind map" }, det?.game || (sm.duke ? "Duke" : "unknown"))),
    el("div", { class: "iwad-sub" },
      sm.iwad || "—",
      sm.iwadMagic ? ` · ${sm.iwadMagic}` : "",
      ` · ${fmtSize(sm.iwadBytes || 0)}`,
      det?.autoname ? ` · ${det.autoname}` : ""));

  const stats = [
    ["Maps", sm.maps?.length || 0],
    ["Lumps", sm.lumps?.length || 0],
    ["Textures", sm.textures?.length || 0],
    ["Flats", Array.isArray(sm.flats) ? sm.flats.length : Object.keys(sm.flats || {}).length],
    ["Patches", Array.isArray(sm.patches) ? sm.patches.length : Object.keys(sm.patches || {}).length],
    ["Things", sm.things?.length || 0],
    ["PWADs", sm.pwads?.length || 0],
    ["Overrides", sm.overrides?.length || 0],
  ];
  wrap.append(el("div", { class: "iwad-grid" },
    ...stats.map(([l, n]) => el("div", { class: "iwad-stat" }, el("div", { class: "n" }, n), el("div", { class: "l" }, l)))));

  const kv = (rows) => el("table", { class: "iwad-kv" },
    ...rows.filter(([, v]) => v != null && v !== "").map(([k, v]) => el("tr", {}, el("th", {}, k), el("td", {}, v instanceof Node ? v : String(v)))));

  wrap.append(el("fieldset", {}, el("legend", {}, "Identity"), kv([
    ["File", sm.iwad],
    ["Size", fmtSize(sm.iwadBytes || 0)],
    ["Magic", sm.iwadMagic || "—"],
    ["Detected", det?.name || "No IWADINFO match"],
    ["Game", det?.game || (ROOM_UI_SHOW_DUKE && sm.duke ? "Duke Nukem 3D" : "—")],
    ["Autoname", det?.autoname || "—"],
    ["Config", det?.config || "—"],
    ["Filters", det?.filters?.length ? el("div", { class: "iwad-chips" }, ...det.filters.map((f) => el("span", {}, f))) : "—"],
    ["Must contain", det?.mustContain?.length ? el("div", { class: "iwad-chips" }, ...det.mustContain.map((f) => el("span", {}, f))) : "—"],
    ["Known names", det?.names?.length ? el("div", { class: "iwad-chips" }, ...det.names.map((f) => el("span", {}, f))) : "—"],
  ])));

  // Duke GRP summary — kept; gated so Doom-only UI stays clean.
  if (ROOM_UI_SHOW_DUKE && sm.duke) {
    wrap.append(el("fieldset", {}, el("legend", {}, "Duke Nukem 3D"), kv([
      ["GRP", sm.duke.grp],
      ["User maps", (sm.duke.userMaps || []).join(", ") || "—"],
    ])));
  }

  const removable = listRemovableArchives();
  const packBox = el("fieldset", {}, el("legend", {}, "Mounted archives"),
    el("p", { class: "muted" }, "Add with Open…, drop, or + Pack…. Remove undoes a PWAD / pack (not the base IWAD)."));
  if (!removable.length) {
    packBox.append(el("p", { class: "muted" }, "No extra archives mounted."));
  } else {
    const list = el("div", { class: "iwad-pack-list" });
    for (const name of removable) {
      list.append(el("div", { class: "iwad-pack-row" },
        el("span", { class: "name" }, name),
        el("button", {
          class: "btn small danger",
          type: "button",
          title: `Unmount ${name}`,
          onclick: () => removePwad(name),
        }, "Remove")));
    }
    packBox.append(list);
  }
  packBox.append(el("div", { class: "actions" },
    el("button", {
      class: "btn small",
      onclick: () => $("#file-pack")?.click(),
    }, "+ Add pack…")));
  wrap.append(packBox);

  if (sm.decorateError) {
    wrap.append(el("div", { class: "err-box" }, `DECORATE: ${sm.decorateError}`));
  }

  const mounts = sm.containers || [];
  if (mounts.length) {
    const remSet = new Set(removable.map((n) => n.toLowerCase()));
    const rows = mounts.map((c) => {
      const canRemove = remSet.has(String(c.name || "").toLowerCase());
      return el("tr", {},
        el("td", {}, c.name),
        el("td", {}, c.kind),
        el("td", {}, String(c.lumps)),
        el("td", {}, fmtSize(c.bytes || 0)),
        el("td", { style: "font-size:10px" }, (c.md5 || "").slice(0, 16) + (c.md5 ? "…" : "")),
        el("td", {}, canRemove
          ? el("button", {
            class: "btn small danger",
            type: "button",
            onclick: () => removePwad(c.name),
          }, "Remove")
          : el("span", { class: "muted" }, "—")));
    });
    wrap.append(el("fieldset", {}, el("legend", {}, "All containers"),
      el("table", { class: "iwad-kv" },
        el("tr", {}, el("th", {}, "Name"), el("th", {}, "Kind"), el("th", {}, "Lumps"), el("th", {}, "Bytes"), el("th", {}, "MD5"), el("th", {}, "")),
        ...rows)));
  }

  const kindEntries = Object.entries(sm.kindCounts || {}).sort((a, b) => b[1] - a[1]);
  if (kindEntries.length) {
    wrap.append(el("fieldset", {}, el("legend", {}, "Lumps by kind"),
      el("div", { class: "iwad-chips" },
        ...kindEntries.map(([k, n]) => el("span", {}, `${k} ${n}`)))));
  }
  const nsEntries = Object.entries(sm.nsCounts || {}).sort((a, b) => b[1] - a[1]);
  if (nsEntries.length) {
    wrap.append(el("fieldset", {}, el("legend", {}, "Lumps by namespace"),
      el("div", { class: "iwad-chips" },
        ...nsEntries.map(([k, n]) => el("span", {}, `${k} ${n}`)))));
  }

  if (sm.maps?.length) {
    wrap.append(el("fieldset", {}, el("legend", {}, `Maps (${sm.maps.length})`),
      el("div", { class: "iwad-maps" },
        ...sm.maps.map((m) => el("button", {
          class: `btn small${S.doc?.name === m ? " active" : ""}`,
          onclick: () => loadMapOrDoc(m),
        }, displayMapName(m))))));
  }

  wrap.append(el("div", { class: "actions" },
    el("button", { class: "btn small", onclick: () => { S.tab = "lumps"; syncBrowserTab(); renderBrowser(); } }, "Browse lumps"),
    el("button", { class: "btn small", onclick: () => { S.tab = "things"; syncBrowserTab(); renderBrowser(); } }, "Thing types"),
    el("button", { class: "btn small", onclick: openIwadPage }, "Refresh")));

  return wrap;
}

/** Load only an IWAD explicitly named in the editor URL. */
async function loadQueryIwad() {
  const params = new URLSearchParams(location.search);
  const url = params.get("iwad");
  if (!url) return false;
  try {
    const res = await fetch(url, { cache: "no-cache" });
    if (!res.ok) return false;
    const buf = new Uint8Array(await res.arrayBuffer());
    return buf.length >= 12 && !isPk3Bytes(buf) && openIwad(buf, url.split("/").pop());
  } catch (_) {
    return false;
  }
}

function openIwad(bytes, name) {
  try {
    if (S.playing) setPlaying(false);
    const summary = S.mod.roomEditorOpen(bytes, name);
    S.dukeGrp = isDukeGrpBytes(bytes) ? bytes.slice() : null;
    S.dukeMap = null;
    S.dukeMapBytes = null;
    S.dukeMapFile = null;
    S.dukeUndo = [];
    S.dukeRedo = [];
    S.dukePersistBefore = null;
    S.dukeDragging = null;
    S.dukeSelection = null;
    S.drawPts = [];
    S.doc = null;
    S.docs = {};
    S.mode = "sectors";
    S.sel.clear();
    S.undo = [];
    S.redo = [];
    S.lumpTabs.clear();
    S.docTab = "map";
    S.tab = "maps";
    S.workPk3 = null;
    S.actorSel = null;
    S.openDirs.clear();
    document.body.classList.remove("duke-testing");
    selectDocTab("map");
    setSummary(summary);
    setIwadLabel(name);
    // Play-test skills 1–5 run as Duke 1, 1, 2, 3, 4 for a GRP.
    const labels = (ROOM_UI_SHOW_DUKE && S.summary.duke)
      ? ["Cake-", "Cake", "Rock", "Some", "Damn"]
      : ["ITYTD", "HNTR", "HMP", "UV", "NM"];
    [...$("#skill").options].forEach((o, i) => (o.textContent = labels[i]));
    syncModeButtons();
    renderInspector();
    render2d();
    status(S.summary.duke
      ? `Opened ${name} — choose a Build map from the Maps list`
      : `Opened ${name} — choose a Doom map from the Maps list`);
    return true;
  } catch (e) {
    toast(`Open failed: ${errText(e)}`, true);
    return false;
  }
}

/** ZIP local header, empty archive, or spanned marker — PK3 / ZIP; or a 7z signature — PK7. */
function isPk3Bytes(buf) {
  if (!buf || buf.length < 4) return false;
  if (buf[0] === 0x50 && buf[1] === 0x4b && (buf[2] === 0x03 || buf[2] === 0x05 || buf[2] === 0x07)) return true;
  return buf.length >= 6 && buf[0] === 0x37 && buf[1] === 0x7a && buf[2] === 0xbc && buf[3] === 0xaf && buf[4] === 0x27 && buf[5] === 0x1c;
}

/** IPK3 / IPK7 (a whole game): its name says so or it ships a PLAYPAL. */
function isGamePack(name, bytes) {
  return /\.ipk[37]$/i.test(String(name || "")) || bytesHaveName(bytes, "PLAYPAL");
}

/**
 * Import a PK3 / PK7 / ZIP / IPK3 as the working archive, with or without a
 * base game open: an explicitly supplied URL IWAD may provide palette / game
 * data; otherwise the selected pack becomes the base. The pack is mounted on
 * top when a base is already open, and its files are shown in the Lumps tab.
 * Export PK3 rebuilds it.
 */
async function importPk3Files(files) {
  if (!S.mod) { toast("The engine is still loading — try again in a moment", true); return; }
  for (const f of files || []) {
    const buf = new Uint8Array(await f.arrayBuffer());
    if (!isPk3Bytes(buf)) { toast(`${f.name}: not a PK3 / PK7 / ZIP archive`, true); continue; }
    status(`Importing ${f.name} (${fmtSize(buf.length)})…`);
    let ok;
    let note = "";
    if (!S.summary && !isGamePack(f.name, buf)) await loadQueryIwad();
    if (!S.summary) {
      ok = openIwad(buf, f.name);
      if (ok && !S.summary.iwadDetected && !/\.ipk[37]$/i.test(f.name)) note = `No IWAD found — ${f.name} opened on its own (textures it does not ship will be missing; use Game… to add a base game)`;
    } else {
      if (S.summary.pwads.includes(f.name)) {
        // Re-import replaces the mounted copy.
        try { setSummary(S.mod.roomEditorRemovePwad(f.name)); } catch (_) { /* add anyway */ }
      }
      ok = addPwad(buf, f.name);
    }
    if (ok) focusArchive(f.name, note);
  }
}

/** Make `name` the working archive: Lumps tab with its folders open, summary toast (`note` = extra warning). */
function focusArchive(name, note = "") {
  S.workPk3 = name;
  const files = S.summary.lumps.filter((l) => l.src === name);
  const maps = S.summary.lumps.filter((l) => l.kind === "map" && (l.src === name || l.src.startsWith(`${name}:`)));
  const zs = zscriptLumps(name).length;
  const tops = new Set(files.map((l) => (l.path || "").split("/")).filter((p) => p.length > 1).map((p) => p[0]));
  for (const d of tops) if (/^(zscript|maps|actors|decorate|acs)$/i.test(d)) S.openDirs.add(`${name}:${d}/`);
  S.tab = "lumps";
  syncBrowserTab();
  renderBrowser();
  const warn = [S.summary.decorateError && `definitions: ${S.summary.decorateError}`, note].filter(Boolean).join(" — ");
  toast(`${name}: ${files.length} file(s), ${maps.length} map(s), ${zs} ZScript file(s)${warn ? ` — ${warn}` : ""}`, !!warn);
  status(`Working archive: ${name}`);
  // Mods with ZScript: open their ZSCRIPT root, ready to edit.
  if (zs) openZscriptOf(name);
}

/** Replace the base IWAD / PK3 / GRP from a file picker. */
async function openIwadFile(file) {
  if (!file) return;
  const buf = new Uint8Array(await file.arrayBuffer());
  const magic4 = String.fromCharCode(...buf.slice(0, 4));
  const isGrp = isDukeGrpBytes(buf);
  const isPack = isPk3Bytes(buf) || isContentPackName(file.name);
  if (isGrp && !ROOM_UI_SHOW_DUKE) {
    toast("Duke Nukem 3D is hidden in this UI — pick a Doom / Freedoom IWAD (.wad) or a PK3", true);
    return;
  }
  if (isPack) {
    // Chosen as the base game: replaces whatever is open.
    status(`Loading ${file.name}…`);
    if (!openIwad(buf, file.name)) return;
    focusArchive(file.name);
    return;
  }
  if (!isGrp && magic4 !== "IWAD" && magic4 !== "PWAD") {
    toast(ROOM_UI_SHOW_DUKE
      ? "Pick a Doom WAD, a PK3, or Duke DUKE3D.GRP"
      : "Pick a Doom WAD or a PK3", true);
    return;
  }
  // Game… explicitly replaces the working archive, so a user-selected PWAD
  // is valid as a standalone editor base. Open… mounts it as an add-on when a
  // base archive is already loaded.
  status(isGrp ? `Converting ${file.name}…` : `Loading ${file.name}…`);
  openIwad(buf, file.name);
}

/** Archive that may carry VOXELDEF / extra content. */
function isContentPackName(name) {
  return /\.(pk3|pk7|zip|ipk3|ipk7)$/i.test(String(name || ""));
}

/** True if bytes look like a ZIP/PK3 that includes a VOXELDEF lump path. */
function bytesHaveVoxeldef(bytes) {
  return bytesHaveName(bytes, "VOXELDEF");
}

/** Case-insensitive ASCII `word` in the first / last 512 KiB (ZIP entry names). */
function bytesHaveName(bytes, word) {
  if (!bytes || bytes.length < 32) return false;
  // Local headers + central directory (often near EOF) both store entry names.
  const needle = [...word.toUpperCase()].map((c) => c.charCodeAt(0));
  const windows = [];
  windows.push([0, Math.min(bytes.length, 512 * 1024)]);
  if (bytes.length > 512 * 1024) {
    windows.push([Math.max(0, bytes.length - 512 * 1024), bytes.length]);
  }
  for (const [lo, hi] of windows) {
    outer: for (let i = lo; i <= hi - needle.length; i++) {
      for (let j = 0; j < needle.length; j++) {
        const c = bytes[i + j];
        // Case-insensitive ASCII.
        if (c !== needle[j] && c !== (needle[j] | 0x20)) continue outer;
      }
      return true;
    }
  }
  return false;
}

function shouldRemountForPack(name, bytes) {
  return isContentPackName(name) || bytesHaveVoxeldef(bytes);
}

/** PWADs / packs the user can unmount (not the base IWAD). */
function listRemovableArchives() {
  return [...(S.summary?.pwads || [])];
}

function syncPacksUi() {
  const label = $("#packs-label");
  if (!label) return;
  const packs = listRemovableArchives();
  label.replaceChildren();
  if (!packs.length) {
    label.textContent = "—";
    label.title = "No extra archives — use + Pack… or Open…";
    return;
  }
  label.title = `Mounted: ${packs.join(", ")}`;
  for (const name of packs) {
    const chip = el("span", { class: "pack-chip", title: name },
      el("span", {}, name.length > 18 ? `${name.slice(0, 16)}…` : name),
      el("button", {
        type: "button",
        class: "pack-chip-x",
        title: `Remove ${name}`,
        onclick: (e) => { e.stopPropagation(); removePwad(name); },
      }, "×"));
    label.append(chip);
  }
}

function addPwad(bytes, name) {
  try {
    const before = new Set(S.summary?.duke?.userMaps || []);
    const mapBefore = S.doc?.name;
    if (S.doc) S.docs[S.doc.name] = S.doc;
    const remount = shouldRemountForPack(name, bytes);
    setSummary(S.mod.roomEditorAddPwad(bytes, name));
    status(`Mounted ${name}`);
    syncPacksUi();
    if (ROOM_UI_SHOW_DUKE && /\.map$/i.test(name) && S.summary.duke) {
      const added = S.summary.duke.userMaps.find((m) => !before.has(m)) || S.summary.duke.userMaps.at(-1);
      if (added) {
        loadMap(added);
        toast("Native Build .MAP opened · edit geometry and properties, then run a native Duke test");
      }
      return true;
    }
    // Maps of the pack itself and of its maps/*.wad ("pack.pk3:maps/x.wad").
    const own = S.summary.lumps.filter((l) => (l.src === name || l.src.startsWith(`${name}:`)) && l.kind === "map").map((l) => l.name);
    if (own.length) {
      loadMap(own[0]);
      return true;
    }
    // VOXELDEF/KVX (and other defs) apply after the live session remounts.
    if (remount && mapBefore) loadMap(mapBefore);
    return true;
  } catch (e) {
    toast(`Mount failed: ${errText(e)}`, true);
    return false;
  }
}

function removePwad(name) {
  if (!name || !S.mod) return;
  try {
    const mapBefore = S.doc?.name;
    if (S.doc) S.docs[S.doc.name] = S.doc;
    setSummary(S.mod.roomEditorRemovePwad(name));
    status(`Removed ${name}`);
    toast(`Unmounted ${name}`);
    syncPacksUi();
    if (mapBefore && (S.summary.maps || []).includes(mapBefore)) loadMap(mapBefore);
    if (S.tab === "iwad" || S.lumpTabs.has("iwad-info")) openIwadPage();
  } catch (e) {
    toast(`Remove failed: ${errText(e)}`, true);
  }
}

async function openFiles(files) {
  for (const f of files) {
    const buf = new Uint8Array(await f.arrayBuffer());
    const magic = String.fromCharCode(...buf.slice(0, 4));
    if (/\.(zs|zc|zsc)$/i.test(f.name) || (/\.txt$/i.test(f.name) && textLang("", f.name, new TextDecoder().decode(buf.slice(0, 16384))) === "zs")) {
      openLooseZscript(f.name, buf);
      continue;
    }
    if (isDukeGrpBytes(buf)) {
      if (!ROOM_UI_SHOW_DUKE) {
        toast("Duke Nukem 3D is hidden in this UI — pick a Doom / Freedoom IWAD (.wad)", true);
        continue;
      }
      status(`Converting ${f.name}…`);
      openIwad(buf, f.name);
    } else if (magic === "IWAD" || (!S.summary && magic === "PWAD")) {
      openIwad(buf, f.name);
    } else if (isPk3Bytes(buf) || isContentPackName(f.name)) {
      await importPk3Files([f]);
    } else if (!S.summary) {
      toast("Open an IWAD or PK3 first", true);
    } else if (!ROOM_UI_SHOW_DUKE && /\.map$/i.test(f.name)) {
      toast("Duke .MAP files are hidden in this UI", true);
    } else if (addPwad(buf, f.name)) {
      const n = openZscriptOf(f.name);
      if (n) toast(`${f.name}: ${n} ZScript file(s) — ZSCRIPT opened for editing`);
    }
  }
}

// ---------------------------------------------------------------- document / sync

function loadMap(name) {
  try {
    const json = S.mod.roomEditorLoadMap(name);
    S.doc = S.docs[name] ? S.docs[name] : JSON.parse(json);
    if (S.summary?.duke) {
      const native = JSON.parse(S.mod.roomEditorDukeMapGet(name));
      S.dukeMap = native.map;
      S.dukeMapFile = native.file;
      S.dukeMapBytes = S.mod.roomEditorDukeMapPut(name, JSON.stringify(S.dukeMap));
      S.dukeSelection = null;
      S.dukeUndo = [];
      S.dukeRedo = [];
      S.dukePersistBefore = null;
    } else {
      S.dukeMap = null;
      S.dukeMapBytes = null;
      S.dukeMapFile = null;
      S.dukePersistBefore = null;
    }
    if (S.docs[name]) pushNow();
    S.undo = []; S.redo = []; S.sel.clear(); S.pick3d = null; S.drawPts = [];
    S.version++;
    $("#map-select").value = name;
    setPlaying(false);
    if (S.dukeMap) fitDukeView(); else fitView();
    renderInspector();
    render2d();
    status(`${displayMapName(name)} loaded`);
    syncPill("live", "synced");
    selectDocTab("map");
  } catch (e) {
    toast(`Load ${name}: ${errText(e)}`, true);
  }
}

let pushTimer = 0;
function schedulePush(delay = 40) {
  clearTimeout(pushTimer);
  pushTimer = setTimeout(pushNow, delay);
}
function pushNow() {
  clearTimeout(pushTimer);
  if (!S.doc) return;
  try {
    const how = S.mod.roomEditorUpdateMap(JSON.stringify(S.doc));
    const label = { live: "live patch", restart: "level restarted", rebuild: "nodes rebuilt" }[how] || how;
    syncPill(how, label);
  } catch (e) {
    syncPill("error", "error");
    status(`Map error: ${errText(e)}`);
  }
}

function snapshot() {
  S.undo.push(JSON.stringify(S.doc));
  if (S.undo.length > 200) S.undo.shift();
  S.redo = [];
}
function commit({ push = true, inspector = true } = {}) {
  S.version++;
  S.docs[S.doc.name] = S.doc;
  if (push) schedulePush();
  if (inspector) renderInspector();
  render2d();
}
function undo() {
  if (S.dukeMap) return dukeUndoAction(false);
  if (!S.undo.length) return;
  S.redo.push(JSON.stringify(S.doc));
  S.doc = JSON.parse(S.undo.pop());
  S.sel.clear();
  commit();
}
function redo() {
  if (S.dukeMap) return dukeUndoAction(true);
  if (!S.redo.length) return;
  S.undo.push(JSON.stringify(S.doc));
  S.doc = JSON.parse(S.redo.pop());
  S.sel.clear();
  commit();
}

// ---------------------------------------------------------------- geometry helpers

const clampI16 = (v) => Math.max(-32768, Math.min(32767, Math.round(v)));
const vtx = (i) => S.doc.vertices[i];
const side = (i) => (i === NO_SIDE || i == null ? null : S.doc.sidedefs[i]);
const isHexen = () => S.doc?.format === "Hexen";
const isUdmf = () => S.doc?.format === "Udmf";

function geometry() {
  if (S.geo && S.geo.version === S.version) return S.geo;
  const d = S.doc;
  const n = d.sectors.length;
  const edges = Array.from({ length: n }, () => []);
  const bbox = Array.from({ length: n }, () => [Infinity, Infinity, -Infinity, -Infinity]);
  d.linedefs.forEach((l) => {
    const a = d.vertices[l.v1], b = d.vertices[l.v2];
    if (!a || !b) return;
    for (const [sk, p, q] of [["side_front", a, b], ["side_back", b, a]]) {
      const s = side(l[sk]);
      if (!s || s.sector >= n) continue;
      edges[s.sector].push([p, q]);
      const bb = bbox[s.sector];
      bb[0] = Math.min(bb[0], p.x, q.x); bb[1] = Math.min(bb[1], p.y, q.y);
      bb[2] = Math.max(bb[2], p.x, q.x); bb[3] = Math.max(bb[3], p.y, q.y);
    }
  });
  const paths = edges.map((list) => {
    const path = new Path2D();
    for (const [p, q] of list) { path.moveTo(p.x, p.y); path.lineTo(q.x, q.y); }
    // Even-odd fill needs closed loops: chain edges by shared endpoints.
    const fill = new Path2D();
    const byStart = new Map();
    list.forEach((e, i) => {
      const k = `${e[0].x},${e[0].y}`;
      if (!byStart.has(k)) byStart.set(k, []);
      byStart.get(k).push(i);
    });
    const used = new Uint8Array(list.length);
    for (let i = 0; i < list.length; i++) {
      if (used[i]) continue;
      used[i] = 1;
      const start = list[i][0];
      fill.moveTo(start.x, start.y);
      let cur = list[i];
      for (let guard = 0; guard < list.length; guard++) {
        fill.lineTo(cur[1].x, cur[1].y);
        if (cur[1].x === start.x && cur[1].y === start.y) break;
        const nexts = byStart.get(`${cur[1].x},${cur[1].y}`) || [];
        const ni = nexts.find((j) => !used[j]);
        if (ni == null) break;
        used[ni] = 1;
        cur = list[ni];
      }
      fill.closePath();
    }
    return fill;
  });
  S.geo = { version: S.version, paths, bbox };
  return S.geo;
}

const hitCtx = new OffscreenCanvas(1, 1).getContext("2d");
function sectorAt(wx, wy) {
  const g = geometry();
  let best = -1, bestArea = Infinity;
  g.paths.forEach((p, i) => {
    const bb = g.bbox[i];
    if (wx < bb[0] || wx > bb[2] || wy < bb[1] || wy > bb[3]) return;
    if (!hitCtx.isPointInPath(p, wx, wy, "evenodd")) return;
    const area = (bb[2] - bb[0]) * (bb[3] - bb[1]);
    if (area < bestArea) { bestArea = area; best = i; }
  });
  return best;
}

function distToSeg(px, py, a, b) {
  const dx = b.x - a.x, dy = b.y - a.y;
  const len2 = dx * dx + dy * dy || 1;
  const t = Math.max(0, Math.min(1, ((px - a.x) * dx + (py - a.y) * dy) / len2));
  return Math.hypot(px - (a.x + t * dx), py - (a.y + t * dy));
}

function hitTest(wx, wy, mode = S.mode) {
  const tol = 7 / S.view.scale;
  const d = S.doc;
  if (!d) return -1;
  if (mode === "vertices") {
    let best = -1, bd = tol;
    d.vertices.forEach((v, i) => { const dd = Math.hypot(v.x - wx, v.y - wy); if (dd < bd) { bd = dd; best = i; } });
    return best;
  }
  if (mode === "lines") {
    let best = -1, bd = tol;
    d.linedefs.forEach((l, i) => {
      const a = vtx(l.v1), b = vtx(l.v2);
      if (!a || !b) return;
      const dd = distToSeg(wx, wy, a, b);
      if (dd < bd) { bd = dd; best = i; }
    });
    return best;
  }
  if (mode === "things") {
    let best = -1, bd = Infinity;
    d.things.forEach((t, i) => {
      const r = Math.max(thingRadius(t), tol);
      const dd = Math.hypot(t.x - wx, t.y - wy);
      if (dd <= r && dd < bd) { bd = dd; best = i; }
    });
    return best;
  }
  if (mode === "sectors") return sectorAt(wx, wy);
  return -1;
}

const thingRadius = (t) => S.thingInfo.get(t.type_id)?.radius || 16;

function selectionVertices() {
  const d = S.doc, out = new Set();
  if (S.mode === "vertices") S.sel.forEach((i) => out.add(i));
  else if (S.mode === "lines") S.sel.forEach((i) => { const l = d.linedefs[i]; if (l) { out.add(l.v1); out.add(l.v2); } });
  else if (S.mode === "sectors") {
    d.linedefs.forEach((l) => {
      const f = side(l.side_front), b = side(l.side_back);
      if ((f && S.sel.has(f.sector)) || (b && S.sel.has(b.sector))) { out.add(l.v1); out.add(l.v2); }
    });
  }
  return out;
}

function sectorLines(sec) {
  const out = [];
  S.doc.linedefs.forEach((l, i) => {
    const f = side(l.side_front), b = side(l.side_back);
    if ((f && f.sector === sec) || (b && b.sector === sec)) out.push(i);
  });
  return out;
}

// ---------------------------------------------------------------- edit operations

/** New record shaped like the map's existing ones (so every field is editable). */
function tpl(kind, fields) {
  const list = S.doc[kind];
  const base = list.length ? structuredClone(list[0]) : {};
  const resets = {
    sidedefs: { x_offset: 0, y_offset: 0, upper: "-", lower: "-", middle: "-", sector: 0 },
    linedefs: { flags: 0, special: 0, tag: 0, args: [0, 0, 0, 0, 0], side_front: NO_SIDE, side_back: NO_SIDE },
    sectors: { special: 0, tag: 0 },
    things: { tid: 0, special: 0, args: [0, 0, 0, 0, 0] },
  }[kind] || {};
  if (kind === "linedefs" && "activation" in base) resets.activation = 0;
  return Object.assign(base, resets, fields);
}

function compact() {
  const d = S.doc;
  const sideMap = new Map(), sides = [];
  const vMap = new Map(), verts = [];
  for (const l of d.linedefs) {
    for (const k of ["side_front", "side_back"]) {
      const s = l[k];
      if (s === NO_SIDE || s >= d.sidedefs.length) { l[k] = NO_SIDE; continue; }
      if (!sideMap.has(s)) { sideMap.set(s, sides.length); sides.push(d.sidedefs[s]); }
      l[k] = sideMap.get(s);
    }
    for (const k of ["v1", "v2"]) {
      const v = l[k];
      if (!vMap.has(v)) { vMap.set(v, verts.length); verts.push(d.vertices[v]); }
      l[k] = vMap.get(v);
    }
  }
  d.sidedefs = sides;
  d.vertices = verts;
}

function deleteSelection() {
  const d = S.doc;
  if (!d || !S.sel.size) return;
  snapshot();
  if (S.mode === "things") {
    d.things = d.things.filter((_, i) => !S.sel.has(i));
  } else if (S.mode === "vertices") {
    d.linedefs = d.linedefs.filter((l) => !S.sel.has(l.v1) && !S.sel.has(l.v2));
    compact();
  } else if (S.mode === "lines") {
    d.linedefs = d.linedefs.filter((_, i) => !S.sel.has(i));
    compact();
  } else if (S.mode === "sectors") {
    const gone = S.sel;
    const keep = [];
    for (const l of d.linedefs) {
      const f = side(l.side_front), b = side(l.side_back);
      const fGone = !f || gone.has(f.sector), bGone = !b || gone.has(b.sector);
      if (fGone && bGone) continue;
      if (fGone && !bGone) {
        [l.v1, l.v2] = [l.v2, l.v1];
        l.side_front = l.side_back;
      }
      if (fGone || bGone) {
        l.side_back = NO_SIDE;
        l.flags = (l.flags & ~4) | 1;
        const s = side(l.side_front);
        if (s && s.middle === "-") s.middle = s.lower !== "-" ? s.lower : s.upper !== "-" ? s.upper : S.texture;
      }
      keep.push(l);
    }
    d.linedefs = keep;
    compact();
    const remap = [];
    let n = 0;
    d.sectors.forEach((_, i) => { remap[i] = gone.has(i) ? -1 : n++; });
    d.sectors = d.sectors.filter((_, i) => !gone.has(i));
    d.sidedefs.forEach((s) => { s.sector = Math.max(0, remap[s.sector] ?? 0); });
  }
  S.sel.clear();
  commit();
}

function splitLines() {
  if (S.mode !== "lines" || !S.sel.size) return;
  snapshot();
  const d = S.doc;
  for (const i of [...S.sel]) {
    const l = d.linedefs[i];
    const a = vtx(l.v1), b = vtx(l.v2);
    const mid = d.vertices.length;
    d.vertices.push({ x: clampI16((a.x + b.x) / 2), y: clampI16((a.y + b.y) / 2) });
    const copy = structuredClone(l);
    copy.v1 = mid;
    for (const k of ["side_front", "side_back"]) {
      if (l[k] !== NO_SIDE) { copy[k] = d.sidedefs.length; d.sidedefs.push(structuredClone(d.sidedefs[l[k]])); }
    }
    l.v2 = mid;
    d.linedefs.push(copy);
    S.sel.add(d.linedefs.length - 1);
  }
  commit();
}

function flipLines() {
  if (S.mode !== "lines" || !S.sel.size) return;
  snapshot();
  for (const i of S.sel) {
    const l = S.doc.linedefs[i];
    [l.v1, l.v2] = [l.v2, l.v1];
    if (l.side_back !== NO_SIDE) [l.side_front, l.side_back] = [l.side_back, l.side_front];
  }
  commit();
}

function makeDoor() {
  if (S.mode !== "sectors" || !S.sel.size) return;
  snapshot();
  const d = S.doc;
  for (const sec of S.sel) {
    const s = d.sectors[sec];
    s.ceiling_height = s.floor_height;
    for (const li of sectorLines(sec)) {
      const l = d.linedefs[li];
      if (l.side_back === NO_SIDE) {
        const f = side(l.side_front);
        if (f) { f.middle = f.middle === "-" ? "DOORTRAK" : f.middle; }
        l.flags |= 16;
        continue;
      }
      // Door specials fire from the front side: face it away from the door.
      if (side(l.side_front).sector === sec) {
        [l.v1, l.v2] = [l.v2, l.v1];
        [l.side_front, l.side_back] = [l.side_back, l.side_front];
      }
      const outer = side(l.side_front);
      if (outer.upper === "-") outer.upper = "BIGDOOR2";
      if (isHexen() || isUdmf()) {
        l.special = 12; l.args = [0, 16, 150, 0, 0];
        if (isHexen()) l.flags = (l.flags & ~0x1c00) | (1 << 10) | 0x200;
        else { l.activation = 2; l.flags |= 0x200; }
      } else {
        l.special = 1;
      }
    }
  }
  commit();
  toast("Door made — front sides face out; play-test with Use");
}

function adjustSectors(key, delta) {
  const secs = S.mode === "sectors" ? [...S.sel] : [];
  if (!secs.length) return false;
  snapshot();
  for (const i of secs) {
    const s = S.doc.sectors[i];
    if (key === "light") s.light = Math.max(0, Math.min(255, s.light + delta));
    else s[key] = clampI16(s[key] + delta);
  }
  commit();
  return true;
}

function insertThing(wx, wy) {
  snapshot();
  const info = S.thingInfo.get(S.thingType);
  const t = tpl("things", {
    x: clampI16(snap(wx)), y: clampI16(snap(wy)), z: 0, angle: 90, type_id: S.thingType,
    flags: isHexen() ? 0x707 : 7, hexen: isHexen() || isUdmf(),
  });
  S.doc.things.push(t);
  S.mode = "things";
  syncModeButtons();
  S.sel = new Set([S.doc.things.length - 1]);
  commit();
  status(`Placed ${info?.title || S.thingType}`);
}

function finishDraw() {
  const pts = S.drawPts;
  S.drawPts = [];
  if (pts.length < 3) { render2d(); return; }
  snapshot();
  const d = S.doc;
  let area = 0;
  pts.forEach((p, i) => { const q = pts[(i + 1) % pts.length]; area += p.x * q.y - q.x * p.y; });
  const clockwise = area < 0;
  const cx = pts.reduce((s, p) => s + p.x, 0) / pts.length;
  const cy = pts.reduce((s, p) => s + p.y, 0) / pts.length;
  const outer = sectorAt(cx, cy);
  const base = outer >= 0 ? tpl("sectors", structuredClone(d.sectors[outer]))
    : tpl("sectors", { floor_height: 0, ceiling_height: 128, floor_pic: S.flat, ceiling_pic: "CEIL3_5", light: 160 });
  base.special = 0; base.tag = 0;
  const sec = d.sectors.length;
  d.sectors.push(base);
  const vid = pts.map((p) => {
    const found = d.vertices.findIndex((v) => v.x === p.x && v.y === p.y);
    if (found >= 0) return found;
    d.vertices.push({ x: p.x, y: p.y });
    return d.vertices.length - 1;
  });
  const newSide = (sector, middle) => {
    d.sidedefs.push(tpl("sidedefs", { middle, sector }));
    return d.sidedefs.length - 1;
  };
  for (let i = 0; i < vid.length; i++) {
    const a = vid[i], b = vid[(i + 1) % vid.length];
    const existing = d.linedefs.find((l) => (l.v1 === a && l.v2 === b) || (l.v1 === b && l.v2 === a));
    if (existing) {
      // Interior is on the right of a→b when the polygon is clockwise.
      const frontFacesIn = (existing.v1 === a) === clockwise;
      if (frontFacesIn) {
        side(existing.side_front).sector = sec;
      } else if (existing.side_back !== NO_SIDE) {
        side(existing.side_back).sector = sec;
      } else {
        const f = side(existing.side_front);
        const tex = f.middle !== "-" ? f.middle : S.texture;
        f.middle = "-"; f.upper = tex; f.lower = tex;
        existing.side_back = newSide(sec, "-");
        existing.flags = (existing.flags | 4) & ~1;
      }
      continue;
    }
    const [v1, v2] = clockwise ? [a, b] : [b, a];
    const line = tpl("linedefs", { v1, v2, flags: 1, hexen: isHexen() || isUdmf(), side_front: newSide(sec, S.texture) });
    if (outer >= 0) {
      line.flags = 4;
      side(line.side_front).middle = "-";
      line.side_back = newSide(outer, "-");
      side(line.side_front).lower = S.texture; side(line.side_front).upper = S.texture;
      side(line.side_back).lower = S.texture; side(line.side_back).upper = S.texture;
    }
    d.linedefs.push(line);
  }
  S.mode = "sectors";
  syncModeButtons();
  S.sel = new Set([sec]);
  commit();
  status(outer >= 0 ? `Sector ${sec} drawn inside sector ${outer}` : `Sector ${sec} drawn`);
}

// ---------------------------------------------------------------- 2D view

const cv = $("#view2d");
const ctx = cv.getContext("2d");
const thumbs = new Map();
function thumb(name) {
  if (!name || name === "-") return null;
  const key = name.toUpperCase();
  if (thumbs.has(key)) return thumbs.get(key);
  let out = null;
  try {
    const img = S.mod.roomEditorPreview(key);
    if (img) {
      const c = document.createElement("canvas");
      c.width = img.w; c.height = img.h;
      c.getContext("2d").putImageData(new ImageData(img.rgba, img.w, img.h), 0, 0);
      out = c;
    }
  } catch (_) { out = null; }
  thumbs.set(key, out);
  return out;
}
function drawThumb(canvas, name, size = 64) {
  const src = thumb(name);
  canvas.width = size; canvas.height = size;
  const c = canvas.getContext("2d");
  c.clearRect(0, 0, size, size);
  if (!src) return;
  const k = Math.min(size / src.width, size / src.height);
  const w = src.width * k, h = src.height * k;
  c.imageSmoothingEnabled = false;
  c.drawImage(src, (size - w) / 2, (size - h) / 2, w, h);
}

const toScreen = (wx, wy) => [(wx - S.view.x) * S.view.scale + cv.clientWidth / 2, cv.clientHeight / 2 - (wy - S.view.y) * S.view.scale];
const toWorld = (sx, sy) => [(sx - cv.clientWidth / 2) / S.view.scale + S.view.x, (cv.clientHeight / 2 - sy) / S.view.scale + S.view.y];
const snap = (v) => (S.snap ? Math.round(v / S.grid) * S.grid : Math.round(v));

function fitView() {
  const d = S.doc;
  if (!d || !d.vertices.length) return;
  let x0 = Infinity, y0 = Infinity, x1 = -Infinity, y1 = -Infinity;
  for (const v of d.vertices) { x0 = Math.min(x0, v.x); y0 = Math.min(y0, v.y); x1 = Math.max(x1, v.x); y1 = Math.max(y1, v.y); }
  S.view.x = (x0 + x1) / 2; S.view.y = (y0 + y1) / 2;
  S.view.scale = Math.min(cv.clientWidth / Math.max(64, x1 - x0), cv.clientHeight / Math.max(64, y1 - y0)) * 0.9;
  render2d();
}

function fitDukeView() {
  const m = S.dukeMap;
  if (!m?.walls?.length) return;
  let x0 = Infinity, y0 = Infinity, x1 = -Infinity, y1 = -Infinity;
  for (const w of m.walls) { x0 = Math.min(x0, w.x); x1 = Math.max(x1, w.x); y0 = Math.min(y0, w.y); y1 = Math.max(y1, w.y); }
  S.view.x = (x0 + x1) / 2;
  S.view.y = (y0 + y1) / 2;
  S.view.scale = Math.min(cv.clientWidth / Math.max(64, x1 - x0), cv.clientHeight / Math.max(64, y1 - y0)) * 0.9;
  render2d();
}

function dukeToScreen(wx, wy) {
  return [(wx - S.view.x) * S.view.scale + cv.clientWidth / 2,
    (wy - S.view.y) * S.view.scale + cv.clientHeight / 2];
}

function dukeToWorld(sx, sy) {
  return [(sx - cv.clientWidth / 2) / S.view.scale + S.view.x,
    (sy - cv.clientHeight / 2) / S.view.scale + S.view.y];
}

function dukeSectorLoops(sector) {
  const m = S.dukeMap;
  const begin = Math.max(0, sector.wallptr);
  const end = Math.min(m.walls.length, begin + Math.max(0, sector.wallnum));
  const found = new Set();
  const loops = [];
  for (let first = begin; first < end; first++) {
    if (found.has(first)) continue;
    const loop = [];
    let at = first;
    for (let n = 0; n <= end - begin; n++) {
      if (at < begin || at >= end || found.has(at)) break;
      found.add(at);
      loop.push(m.walls[at]);
      at = m.walls[at].point2;
      if (at === first) break;
    }
    if (loop.length >= 3) loops.push(loop);
  }
  return loops;
}

function drawDuke2d(W, H, dpr) {
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  ctx.fillStyle = "#0b0c0e";
  ctx.fillRect(0, 0, W, H);
  const m = S.dukeMap;
  if (!m) return;
  let grid = S.grid;
  while (grid * S.view.scale < 8) grid *= 2;
  const [wx0, wy0] = dukeToWorld(0, 0), [wx1, wy1] = dukeToWorld(W, H);
  ctx.lineWidth = 1;
  for (let pass = 0; pass < 2; pass++) {
    const step = pass ? grid * 8 : grid;
    ctx.strokeStyle = pass ? "#252a31" : "#171a1e";
    ctx.beginPath();
    for (let x = Math.floor(wx0 / step) * step; x <= wx1; x += step) {
      const [sx] = dukeToScreen(x, 0); ctx.moveTo(sx, 0); ctx.lineTo(sx, H);
    }
    for (let y = Math.floor(wy0 / step) * step; y <= wy1; y += step) {
      const [, sy] = dukeToScreen(0, y); ctx.moveTo(0, sy); ctx.lineTo(W, sy);
    }
    ctx.stroke();
  }
  for (let si = 0; si < m.sectors.length; si++) {
    const loops = dukeSectorLoops(m.sectors[si]);
    ctx.beginPath();
    for (const loop of loops) {
      loop.forEach((wall, i) => {
        const [x, y] = dukeToScreen(wall.x, wall.y);
        if (i) ctx.lineTo(x, y); else ctx.moveTo(x, y);
      });
      ctx.closePath();
    }
    ctx.fillStyle = S.dukeSelection?.kind === "sector" && S.dukeSelection.index === si
      ? "rgba(255,204,51,.24)" : `rgba(${50 + (si * 37) % 45},${60 + (si * 23) % 40},${74 + (si * 17) % 42},.24)`;
    ctx.fill("evenodd");
  }
  m.walls.forEach((wall, i) => {
    const end = m.walls[wall.point2];
    if (!end) return;
    const [ax, ay] = dukeToScreen(wall.x, wall.y), [bx, by] = dukeToScreen(end.x, end.y);
    const selected = S.dukeSelection?.kind === "wall" && S.dukeSelection.index === i;
    ctx.beginPath(); ctx.moveTo(ax, ay); ctx.lineTo(bx, by);
    ctx.strokeStyle = selected ? "#ffcc33" : wall.nextwall >= 0 ? "#7d8792" : "#d6d9de";
    ctx.lineWidth = selected ? 2.5 : 1.35;
    ctx.stroke();
    if (S.mode === "vertices") {
      const vertexSelected = S.dukeSelection?.kind === "vertex" && S.dukeSelection.x === wall.x && S.dukeSelection.y === wall.y;
      const r = vertexSelected ? 3.5 : (S.view.scale > 0.12 ? 2.25 : 0);
      if (r) { ctx.fillStyle = vertexSelected ? "#ffcc33" : "#9fb4ff"; ctx.fillRect(ax - r, ay - r, r * 2, r * 2); }
    }
  });
  if (S.dukeFloors && (S.view.scale >= 0.18 || S.dukeSelection?.kind === "sector")) {
    m.sectors.forEach((sector, si) => {
      const loops = dukeSectorLoops(sector);
      let x0 = Infinity, y0 = Infinity, x1 = -Infinity, y1 = -Infinity;
      let centerX = 0, centerY = 0, largestArea = 0;
      for (const loop of loops) for (const point of loop) {
        x0 = Math.min(x0, point.x); y0 = Math.min(y0, point.y);
        x1 = Math.max(x1, point.x); y1 = Math.max(y1, point.y);
      }
      if (!Number.isFinite(x0)) return;
      for (const loop of loops) {
        let twiceArea = 0, cx = 0, cy = 0;
        for (let i = 0; i < loop.length; i++) {
          const a = loop[i], b = loop[(i + 1) % loop.length];
          const cross = a.x * b.y - b.x * a.y;
          twiceArea += cross;
          cx += (a.x + b.x) * cross;
          cy += (a.y + b.y) * cross;
        }
        if (Math.abs(twiceArea) > largestArea) {
          largestArea = Math.abs(twiceArea);
          centerX = cx / (3 * twiceArea);
          centerY = cy / (3 * twiceArea);
        }
      }
      if (!largestArea) { centerX = (x0 + x1) / 2; centerY = (y0 + y1) / 2; }
      const [sx, sy] = dukeToScreen(centerX, centerY);
      const selected = S.dukeSelection?.kind === "sector" && S.dukeSelection.index === si;
      const tile = sector.floorpicnum >= 0 ? `BT${String(sector.floorpicnum).padStart(4, "0")}` : "—";
      const line1 = `S${si}  Z ${sector.floorz}`;
      const line2 = tile;
      ctx.font = "10px ui-monospace, monospace";
      const width = Math.max(ctx.measureText(line1).width, ctx.measureText(line2).width) + 10;
      const height = 27;
      if (!selected && (Math.abs((x1 - x0) * S.view.scale) < width + 6 || Math.abs((y1 - y0) * S.view.scale) < height + 6)) return;
      if (!selected && (sx < -width || sx > W + width || sy < -height || sy > H + height)) return;
      ctx.fillStyle = selected ? "rgba(16,18,22,.94)" : "rgba(8,10,13,.82)";
      ctx.fillRect(sx - width / 2, sy - height / 2, width, height);
      ctx.strokeStyle = selected ? "#ffcc33" : "rgba(190,205,220,.35)";
      ctx.lineWidth = selected ? 1.5 : 1;
      ctx.strokeRect(sx - width / 2, sy - height / 2, width, height);
      ctx.textAlign = "center";
      ctx.textBaseline = "middle";
      ctx.fillStyle = selected ? "#ffdf82" : "#e4e9ef";
      ctx.fillText(line1, sx, sy - 5);
      ctx.fillStyle = "#aab8c8";
      ctx.fillText(line2, sx, sy + 6);
      ctx.textAlign = "start";
    });
  }
  if (S.mode === "draw" && S.drawPts.length) {
    ctx.strokeStyle = "#4cc38a";
    ctx.lineWidth = 2;
    ctx.beginPath();
    S.drawPts.forEach((p, i) => {
      const [x, y] = dukeToScreen(p.x, p.y);
      if (i) ctx.lineTo(x, y); else ctx.moveTo(x, y);
    });
    if (mouse.world) {
      const [x, y] = dukeToScreen(snap(mouse.world[0]), snap(mouse.world[1]));
      ctx.lineTo(x, y);
    }
    ctx.stroke();
    S.drawPts.forEach((p) => {
      const [x, y] = dukeToScreen(p.x, p.y);
      ctx.fillStyle = "#4cc38a";
      ctx.fillRect(x - 3, y - 3, 6, 6);
    });
  }
  m.sprites.forEach((sprite, i) => {
    const [x, y] = dukeToScreen(sprite.x, sprite.y);
    const selected = S.dukeSelection?.kind === "sprite" && S.dukeSelection.index === i;
    const r = selected ? 6 : 4;
    ctx.fillStyle = selected ? "#ffcc33" : "#4cc38a";
    ctx.strokeStyle = "#111";
    ctx.beginPath(); ctx.arc(x, y, r, 0, Math.PI * 2); ctx.fill(); ctx.stroke();
    const a = sprite.ang * Math.PI / 1024;
    ctx.beginPath(); ctx.moveTo(x, y); ctx.lineTo(x + Math.cos(a) * 10, y + Math.sin(a) * 10); ctx.stroke();
  });
  const [sx, sy] = dukeToScreen(m.start[0], m.start[1]);
  ctx.strokeStyle = "#59aaff"; ctx.lineWidth = 2;
  ctx.beginPath(); ctx.moveTo(sx - 6, sy); ctx.lineTo(sx + 6, sy); ctx.moveTo(sx, sy - 6); ctx.lineTo(sx, sy + 6); ctx.stroke();
  const mode = { vertices: "vertices", lines: "walls", sectors: "sectors", things: "sprites" }[S.mode] || S.mode;
  $("#hint2d").textContent = `Build ${mode} · click to select · drag vertices / sprites · wheel zoom · right-drag pan · F5 native test`;
}

let renderQueued = false;
function render2d() {
  if (renderQueued) return;
  renderQueued = true;
  requestAnimationFrame(() => { renderQueued = false; draw2d(); });
}

function draw2d() {
  const dpr = window.devicePixelRatio || 1;
  const W = cv.clientWidth, H = cv.clientHeight;
  if (cv.width !== Math.round(W * dpr) || cv.height !== Math.round(H * dpr)) {
    cv.width = Math.round(W * dpr); cv.height = Math.round(H * dpr);
  }
  if (S.dukeMap) {
    drawDuke2d(W, H, dpr);
    return;
  }
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  ctx.fillStyle = "#0b0c0e";
  ctx.fillRect(0, 0, W, H);
  const d = S.doc;
  if (!d) return;
  const sc = S.view.scale;
  // Grid.
  let g = S.grid;
  while (g * sc < 6) g *= 2;
  const [wx0, wy1] = toWorld(0, 0), [wx1, wy0] = toWorld(W, H);
  ctx.lineWidth = 1;
  for (let pass = 0; pass < 2; pass++) {
    const step = pass ? g * 8 : g;
    ctx.strokeStyle = pass ? "#252a31" : "#171a1e";
    ctx.beginPath();
    for (let x = Math.floor(wx0 / step) * step; x <= wx1; x += step) { const [sx] = toScreen(x, 0); ctx.moveTo(Math.round(sx) + 0.5, 0); ctx.lineTo(Math.round(sx) + 0.5, H); }
    for (let y = Math.floor(wy0 / step) * step; y <= wy1; y += step) { const [, sy] = toScreen(0, y); ctx.moveTo(0, Math.round(sy) + 0.5); ctx.lineTo(W, Math.round(sy) + 0.5); }
    ctx.stroke();
  }
  // Sectors.
  const geo = geometry();
  ctx.save();
  ctx.setTransform(dpr * sc, 0, 0, -dpr * sc, dpr * (W / 2 - S.view.x * sc), dpr * (H / 2 + S.view.y * sc));
  d.sectors.forEach((s, i) => {
    const path = geo.paths[i];
    const flat = S.textured ? thumb(s.floor_pic) : null;
    if (flat) {
      const pat = ctx.createPattern(flat, "repeat");
      ctx.globalAlpha = 0.85;
      ctx.fillStyle = pat;
      ctx.fill(path, "evenodd");
      ctx.globalAlpha = 1;
    }
    const dark = 1 - Math.max(0, Math.min(255, s.light)) / 255;
    ctx.fillStyle = flat ? `rgba(0,0,0,${0.15 + dark * 0.7})` : `rgba(${60 + s.light / 3},${60 + s.light / 3},${70 + s.light / 3},0.35)`;
    ctx.fill(path, "evenodd");
    if (S.mode === "sectors" && (S.sel.has(i) || S.hover === i)) {
      ctx.fillStyle = S.sel.has(i) ? "rgba(255,204,51,0.32)" : "rgba(255,140,60,0.18)";
      ctx.fill(path, "evenodd");
    }
  });
  ctx.restore();
  // Lines.
  const hiLines = new Set();
  if (S.mode === "lines") S.sel.forEach((i) => hiLines.add(i));
  if (S.mode === "sectors") S.sel.forEach((s) => sectorLines(s).forEach((i) => hiLines.add(i)));
  d.linedefs.forEach((l, i) => {
    const a = vtx(l.v1), b = vtx(l.v2);
    if (!a || !b) return;
    const [ax, ay] = toScreen(a.x, a.y), [bx, by] = toScreen(b.x, b.y);
    let color = l.side_back === NO_SIDE ? "#d6d9de" : "#6c737d";
    if (l.special) color = "#5ab0ff";
    if (S.mode === "lines" && S.hover === i) color = "#ff9a4a";
    if (hiLines.has(i)) color = "#ffcc33";
    ctx.strokeStyle = color;
    ctx.lineWidth = hiLines.has(i) ? 2 : 1.2;
    ctx.beginPath(); ctx.moveTo(ax, ay); ctx.lineTo(bx, by); ctx.stroke();
    if (S.mode === "lines" && sc > 0.15) {
      // Front-side tick.
      const mx = (ax + bx) / 2, my = (ay + by) / 2;
      const len = Math.hypot(bx - ax, by - ay) || 1;
      const nx = -(by - ay) / len, ny = (bx - ax) / len;
      ctx.beginPath(); ctx.moveTo(mx, my); ctx.lineTo(mx - nx * 6, my - ny * 6); ctx.stroke();
    }
  });
  // Vertices.
  if (S.mode === "vertices" || S.mode === "lines" || S.mode === "draw" || sc > 0.6) {
    d.vertices.forEach((v, i) => {
      const [x, y] = toScreen(v.x, v.y);
      if (x < -5 || y < -5 || x > W + 5 || y > H + 5) return;
      const sel = S.mode === "vertices" && S.sel.has(i);
      const hov = S.mode === "vertices" && S.hover === i;
      ctx.fillStyle = sel ? "#ffcc33" : hov ? "#ff9a4a" : "#9fb4ff";
      const r = sel || hov ? 3.5 : 2;
      ctx.fillRect(x - r, y - r, r * 2, r * 2);
    });
  }
  // Things.
  const showThings = S.mode === "things" || sc > 0.12;
  if (showThings) {
    d.things.forEach((t, i) => {
      const info = S.thingInfo.get(t.type_id);
      const [x, y] = toScreen(t.x, t.y);
      const r = Math.max(3, (info?.radius || 16) * sc);
      if (x < -r || y < -r || x > W + r || y > H + r) return;
      const color = CAT_COLORS[info?.cat] || "#ff00ff";
      const sel = S.mode === "things" && S.sel.has(i);
      const hov = S.mode === "things" && S.hover === i;
      const sprite = sc > 0.45 && info?.sprite ? thumb(info.sprite) : null;
      ctx.globalAlpha = S.mode === "things" ? 1 : 0.55;
      if (sprite) {
        const k = (r * 2) / Math.max(sprite.width, sprite.height);
        ctx.imageSmoothingEnabled = false;
        ctx.drawImage(sprite, x - sprite.width * k / 2, y - sprite.height * k / 2, sprite.width * k, sprite.height * k);
      }
      ctx.strokeStyle = sel ? "#ffcc33" : hov ? "#ff9a4a" : color;
      ctx.lineWidth = sel ? 2.5 : 1.5;
      ctx.fillStyle = sprite ? "transparent" : `${color}55`;
      ctx.beginPath(); ctx.arc(x, y, r, 0, Math.PI * 2);
      if (!sprite) ctx.fill();
      ctx.stroke();
      const ang = (t.angle * Math.PI) / 180;
      ctx.beginPath(); ctx.moveTo(x, y); ctx.lineTo(x + Math.cos(ang) * r * 1.3, y - Math.sin(ang) * r * 1.3); ctx.stroke();
      ctx.globalAlpha = 1;
    });
  }
  // Draw-mode polyline.
  if (S.mode === "draw" && S.drawPts.length) {
    ctx.strokeStyle = "#4cc38a";
    ctx.lineWidth = 2;
    ctx.beginPath();
    S.drawPts.forEach((p, i) => { const [x, y] = toScreen(p.x, p.y); if (i) ctx.lineTo(x, y); else ctx.moveTo(x, y); });
    if (mouse.world) { const [x, y] = toScreen(snap(mouse.world[0]), snap(mouse.world[1])); ctx.lineTo(x, y); }
    ctx.stroke();
    S.drawPts.forEach((p) => { const [x, y] = toScreen(p.x, p.y); ctx.fillStyle = "#4cc38a"; ctx.fillRect(x - 3, y - 3, 6, 6); });
  }
  // Selection box.
  if (mouse.box) {
    const [x0, y0, x1, y1] = mouse.box;
    ctx.strokeStyle = "#ffcc33"; ctx.setLineDash([4, 3]); ctx.lineWidth = 1;
    ctx.strokeRect(Math.min(x0, x1), Math.min(y0, y1), Math.abs(x1 - x0), Math.abs(y1 - y0));
    ctx.setLineDash([]);
  }
  // Camera / player.
  if (S.cam) {
    const [x, y] = toScreen(S.cam.x, S.cam.y);
    const a = S.cam.yaw;
    ctx.fillStyle = S.cam.playing ? "#4cc38a" : "#3b9cff";
    ctx.beginPath();
    ctx.moveTo(x + Math.cos(a) * 14, y - Math.sin(a) * 14);
    ctx.lineTo(x + Math.cos(a + 2.5) * 9, y - Math.sin(a + 2.5) * 9);
    ctx.lineTo(x + Math.cos(a - 2.5) * 9, y - Math.sin(a - 2.5) * 9);
    ctx.closePath(); ctx.fill();
  }
}

const mouse = { down: -1, start: null, world: null, last: null, box: null, drag: null, pan: null, moved: false };

function setHint() {
  if (S.dukeMap) {
    const mode = { vertices: "vertices", lines: "walls", sectors: "sectors", things: "sprites" }[S.mode] || S.mode;
    $("#hint2d").textContent = S.mode === "draw"
      ? "Build sector drawing · click points · click first point or press Enter to close · Esc cancels · right-drag pan"
      : `Build ${mode} · click to select · drag vertices / sprites · wheel zoom · right-drag pan · Insert adds sprite · F5 native test`;
    return;
  }
  const hints = {
    vertices: "Click/drag vertices · Box-select on empty space · Del removes attached lines",
    lines: "Click/drag lines · X split · F flip · Del delete",
    sectors: "Click sectors · PgUp/PgDn floor ±8 (Shift: ceiling) · +/- light · Del delete",
    things: "Double-click/Insert to place the type picked in Things · drag to move · Del delete",
    draw: "Click to add points · click the first point (or Enter) to close · Esc cancels",
  };
  $("#hint2d").textContent = `${hints[S.mode]} · Wheel zoom · Right/middle-drag pan · Double-click: move 3D camera`;
}

function dukePointSegmentDistance(x, y, ax, ay, bx, by) {
  const dx = bx - ax, dy = by - ay;
  const t = Math.max(0, Math.min(1, ((x - ax) * dx + (y - ay) * dy) / (dx * dx + dy * dy || 1)));
  return Math.hypot(x - ax - t * dx, y - ay - t * dy);
}

function dukeSectorAt(x, y) {
  for (let si = S.dukeMap.sectors.length - 1; si >= 0; si--) {
    let inside = false;
    for (const loop of dukeSectorLoops(S.dukeMap.sectors[si])) {
      let hit = false;
      for (let i = 0, j = loop.length - 1; i < loop.length; j = i++) {
        const a = loop[i], b = loop[j];
        if ((a.y > y) !== (b.y > y) && x < ((b.x - a.x) * (y - a.y)) / (b.y - a.y) + a.x) hit = !hit;
      }
      if (hit) inside = !inside;
    }
    if (inside) return si;
  }
  return -1;
}

function finishDukeDraw() {
  const points = S.drawPts;
  S.drawPts = [];
  if (points.length < 3) { render2d(); return; }
  const pts = points.filter((p, i) => i === 0 || p.x !== points[i - 1].x || p.y !== points[i - 1].y);
  if (pts.length > 2 && pts[0].x === pts.at(-1).x && pts[0].y === pts.at(-1).y) pts.pop();
  if (pts.length < 3 || new Set(pts.map((p) => `${p.x},${p.y}`)).size < 3) {
    toast("A Build sector needs at least three different points", true); render2d(); return;
  }
  const m = S.dukeMap;
  if (pts.length > 512) { toast("A drawn sector is limited to 512 points", true); render2d(); return; }
  if (m.walls.length + pts.length > 32767 || m.sectors.length >= 32767) {
    toast("This map has reached the Build v7 wall or sector limit", true); render2d(); return;
  }
  let area = 0n;
  pts.forEach((p, i) => {
    const q = pts[(i + 1) % pts.length];
    area += BigInt(p.x) * BigInt(q.y) - BigInt(q.x) * BigInt(p.y);
  });
  if (area === 0n) { toast("Sector outline is too small", true); render2d(); return; }
  const cross = (a, b, c) => BigInt(b.x - a.x) * BigInt(c.y - a.y) - BigInt(b.y - a.y) * BigInt(c.x - a.x);
  const intersects = (a, b, c, d) => {
    const abC = cross(a, b, c), abD = cross(a, b, d);
    const cdA = cross(c, d, a), cdB = cross(c, d, b);
    const onSegment = (p, u, v) => p.x >= Math.min(u.x, v.x) && p.x <= Math.max(u.x, v.x)
      && p.y >= Math.min(u.y, v.y) && p.y <= Math.max(u.y, v.y);
    const opposite = (x, y) => (x < 0n && y > 0n) || (x > 0n && y < 0n);
    return (opposite(abC, abD) && opposite(cdA, cdB))
      || (abC === 0n && onSegment(c, a, b)) || (abD === 0n && onSegment(d, a, b))
      || (cdA === 0n && onSegment(a, c, d)) || (cdB === 0n && onSegment(b, c, d));
  };
  for (let i = 0; i < pts.length; i++) {
    const a = pts[i], b = pts[(i + 1) % pts.length];
    for (let j = i + 1; j < pts.length; j++) {
      if (j === i || j === (i + 1) % pts.length || i === (j + 1) % pts.length) continue;
      if (intersects(a, b, pts[j], pts[(j + 1) % pts.length])) {
        toast("Build sector outline cannot cross itself", true); render2d(); return;
      }
    }
  }
  // Build sectors use the same clockwise screen-space winding as the starter map.
  if (area < 0n) pts.reverse();
  const cx = pts.reduce((sum, p) => sum + p.x, 0) / pts.length;
  const cy = pts.reduce((sum, p) => sum + p.y, 0) / pts.length;
  const source = m.sectors[dukeSectorAt(cx, cy)] || m.sectors.at(-1);
  const sector = source ? { ...source } : {
    ceilingz: -16384, floorz: 16384, ceilingstat: 0, floorstat: 0,
    ceilingpicnum: 1, floorpicnum: 1, ceilingheinum: 0, floorheinum: 0,
    ceilingshade: 0, ceilingpal: 0, ceilingxpanning: 0, ceilingypanning: 0,
    floorshade: 0, floorpal: 0, floorxpanning: 0, floorypanning: 0,
    visibility: 0, lotag: 0, hitag: 0, extra: -1,
  };
  dukeSnapshot();
  const first = m.walls.length;
  const sectorIndex = m.sectors.length;
  m.sectors.push({ ...sector, wallptr: first, wallnum: pts.length });
  pts.forEach((p, i) => m.walls.push({
    x: p.x, y: p.y, point2: first + (i + 1) % pts.length,
    nextwall: -1, nextsector: -1, cstat: 0, picnum: 1, overpicnum: 0,
    shade: 0, pal: 0, xrepeat: 8, yrepeat: 8, xpanning: 0, ypanning: 0,
    lotag: 0, hitag: 0, extra: -1,
  }));
  for (let i = 0; i < pts.length; i++) {
    const a = pts[i], b = pts[(i + 1) % pts.length];
    const oldIndex = m.walls.slice(0, first).findIndex((wall) => {
      const end = m.walls[wall.point2];
      return end && wall.x === b.x && wall.y === b.y && end.x === a.x && end.y === a.y && wall.nextwall < 0;
    });
    if (oldIndex < 0) continue;
    const owner = m.sectors.findIndex((s, sector) => sector !== sectorIndex
      && oldIndex >= s.wallptr && oldIndex < s.wallptr + s.wallnum);
    if (owner < 0) continue;
    const newIndex = first + i;
    m.walls[newIndex].nextwall = oldIndex;
    m.walls[newIndex].nextsector = owner;
    m.walls[oldIndex].nextwall = newIndex;
    m.walls[oldIndex].nextsector = sectorIndex;
  }
  S.dukeSelection = { kind: "sector", index: sectorIndex };
  dukePersist();
  renderInspector();
  render2d();
  toast(`Build sector ${sectorIndex} created · edit heights and ART tiles in the inspector`);
}

function dukeNearestSector(x, y) {
  let best = -1, bestDistance = Infinity;
  S.dukeMap.sectors.forEach((sector, index) => {
    const walls = dukeSectorLoops(sector).flat();
    if (!walls.length) return;
    const cx = walls.reduce((sum, wall) => sum + wall.x, 0) / walls.length;
    const cy = walls.reduce((sum, wall) => sum + wall.y, 0) / walls.length;
    const distance = (cx - x) ** 2 + (cy - y) ** 2;
    if (distance < bestDistance) { best = index; bestDistance = distance; }
  });
  return best >= 0 ? best : 0;
}

function deleteDukeSector(index) {
  const m = S.dukeMap;
  if (!m || index < 0 || index >= m.sectors.length) return;
  if (m.sectors.length <= 1) { toast("A Build map must keep at least one sector", true); return; }
  const removed = m.sectors[index];
  const first = removed.wallptr, count = removed.wallnum, end = first + count;
  dukeSnapshot();
  m.walls.splice(first, count);
  m.sectors.splice(index, 1);
  for (const wall of m.walls) {
    if (wall.point2 >= end) wall.point2 -= count;
    else if (wall.point2 >= first) wall.point2 = first;
    if (wall.nextwall >= end) wall.nextwall -= count;
    else if (wall.nextwall >= first) { wall.nextwall = -1; wall.nextsector = -1; }
    if (wall.nextsector === index) { wall.nextsector = -1; wall.nextwall = -1; }
    else if (wall.nextsector > index) wall.nextsector--;
  }
  for (const sector of m.sectors) if (sector.wallptr >= end) sector.wallptr -= count;
  for (const sprite of m.sprites) {
    if (sprite.sectnum > index) sprite.sectnum--;
    else if (sprite.sectnum === index) {
      const contained = dukeSectorAt(sprite.x, sprite.y);
      sprite.sectnum = contained >= 0 ? contained : dukeNearestSector(sprite.x, sprite.y);
    }
  }
  if (m.start_sector > index) m.start_sector--;
  else if (m.start_sector === index) {
    let contained = dukeSectorAt(m.start[0], m.start[1]);
    if (contained < 0) {
      contained = dukeNearestSector(m.start[0], m.start[1]);
      const sector = m.sectors[contained];
      const walls = dukeSectorLoops(sector).flat();
      if (walls.length) {
        m.start[0] = Math.round(walls.reduce((sum, wall) => sum + wall.x, 0) / walls.length);
        m.start[1] = Math.round(walls.reduce((sum, wall) => sum + wall.y, 0) / walls.length);
        m.start[2] = sector.floorz;
      }
    }
    m.start_sector = contained;
  }
  S.dukeSelection = null;
  dukePersist();
  renderInspector();
  render2d();
}

function dukeHit(wx, wy) {
  const m = S.dukeMap;
  const max = Math.max(6, 10 / S.view.scale);
  if (S.mode === "vertices") {
    let at = -1, best = max;
    m.walls.forEach((w, i) => { const d = Math.hypot(wx - w.x, wy - w.y); if (d < best) { at = i; best = d; } });
    if (at >= 0) { const w = m.walls[at]; return { kind: "vertex", index: at, x: w.x, y: w.y }; }
  } else if (S.mode === "lines") {
    let at = -1, best = max;
    m.walls.forEach((w, i) => { const end = m.walls[w.point2]; if (!end) return; const d = dukePointSegmentDistance(wx, wy, w.x, w.y, end.x, end.y); if (d < best) { at = i; best = d; } });
    if (at >= 0) return { kind: "wall", index: at };
  } else if (S.mode === "sectors") {
    const at = dukeSectorAt(wx, wy);
    if (at >= 0) return { kind: "sector", index: at };
  } else if (S.mode === "things") {
    let at = -1, best = max;
    m.sprites.forEach((s, i) => { const d = Math.hypot(wx - s.x, wy - s.y); if (d < best) { at = i; best = d; } });
    if (at >= 0) return { kind: "sprite", index: at };
  }
  return null;
}

function dukeMouseDown(e) {
  cv.focus();
  const r = cv.getBoundingClientRect();
  const sx = e.clientX - r.left, sy = e.clientY - r.top;
  const [wx, wy] = dukeToWorld(sx, sy);
  mouse.down = e.button; mouse.start = [sx, sy]; mouse.moved = false;
  if (e.button === 1 || e.button === 2 || (e.button === 0 && keys.has("Space"))) {
    mouse.pan = [sx, sy, S.view.x, S.view.y]; return;
  }
  if (e.button !== 0) return;
  if (S.mode === "draw") {
    const near = S.dukeMap.walls.find((wall) => Math.hypot(wall.x - wx, wall.y - wy) < 8 / S.view.scale);
    const p = near ? { x: near.x, y: near.y } : { x: snap(wx), y: snap(wy) };
    const first = S.drawPts[0];
    if (first && S.drawPts.length >= 3 && Math.hypot(first.x - wx, first.y - wy) <= 8 / S.view.scale) {
      finishDukeDraw();
      return;
    }
    if (!S.drawPts.some((point) => point.x === p.x && point.y === p.y)) S.drawPts.push(p);
    render2d();
    return;
  }
  const hit = dukeHit(wx, wy);
  S.dukeSelection = hit;
  if (hit?.kind === "sprite") {
    const s = S.dukeMap.sprites[hit.index];
    S.dukeDragging = { kind: "sprite", index: hit.index, x: s.x, y: s.y, originX: wx, originY: wy };
  } else if (hit?.kind === "vertex") {
    S.dukeDragging = { kind: "vertex", x: hit.x, y: hit.y, originX: wx, originY: wy };
  } else S.dukeDragging = null;
  renderInspector(); render2d();
}

function dukeMouseMove(e) {
  const r = cv.getBoundingClientRect();
  const sx = e.clientX - r.left, sy = e.clientY - r.top;
  const over = sx >= 0 && sy >= 0 && sx <= r.width && sy <= r.height;
  const [wx, wy] = dukeToWorld(sx, sy);
  if (over || mouse.down >= 0) {
    mouse.world = [wx, wy];
    $("#st-pos").textContent = `${Math.round(wx)}, ${Math.round(wy)} · Build units · zoom ${(S.view.scale * 100).toFixed(0)}%`;
  }
  if (mouse.start && Math.hypot(sx - mouse.start[0], sy - mouse.start[1]) > 3) mouse.moved = true;
  if (mouse.pan) {
    S.view.x = mouse.pan[2] - (sx - mouse.pan[0]) / S.view.scale;
    S.view.y = mouse.pan[3] - (sy - mouse.pan[1]) / S.view.scale;
    render2d(); return;
  }
  if (S.mode === "draw" && over) { render2d(); return; }
  if (!mouse.moved || !S.dukeDragging) return;
  const d = S.dukeDragging;
  if (!d.undo) { dukeSnapshot(); d.undo = true; }
  const x = Math.round(d.x + wx - d.originX), y = Math.round(d.y + wy - d.originY);
  if (d.kind === "sprite") {
    const s = S.dukeMap.sprites[d.index];
    if (s) {
      s.x = x; s.y = y;
      const sector = dukeSectorAt(x, y);
      if (sector >= 0) s.sectnum = sector;
    }
  } else {
    for (const w of S.dukeMap.walls) if (w.x === d.x && w.y === d.y) { w.x = x; w.y = y; }
    S.dukeSelection.x = x; S.dukeSelection.y = y;
  }
  render2d();
}

function dukeMouseUp() {
  if (S.dukeDragging?.undo) { dukePersist(); renderInspector(); }
  S.dukeDragging = null;
  mouse.pan = null; mouse.down = -1; mouse.start = null;
}

cv.addEventListener("contextmenu", (e) => e.preventDefault());
cv.addEventListener("wheel", (e) => {
  e.preventDefault();
  const r = cv.getBoundingClientRect();
  const mapToWorld = S.dukeMap ? dukeToWorld : toWorld;
  const [wx, wy] = mapToWorld(e.clientX - r.left, e.clientY - r.top);
  const k = Math.exp(-e.deltaY * 0.0015);
  S.view.scale = Math.max(0.01, Math.min(16, S.view.scale * k));
  const [nx, ny] = mapToWorld(e.clientX - r.left, e.clientY - r.top);
  S.view.x += wx - nx; S.view.y += wy - ny;
  render2d();
}, { passive: false });

cv.addEventListener("mousedown", (e) => {
  if (S.dukeMap) { dukeMouseDown(e); return; }
  if (!S.doc) return;
  cv.focus();
  const r = cv.getBoundingClientRect();
  const sx = e.clientX - r.left, sy = e.clientY - r.top;
  const [wx, wy] = toWorld(sx, sy);
  mouse.down = e.button; mouse.start = [sx, sy]; mouse.moved = false;
  if (e.button === 1 || e.button === 2 || (e.button === 0 && keys.has("Space"))) {
    mouse.pan = [sx, sy, S.view.x, S.view.y];
    return;
  }
  if (e.button !== 0) return;
  if (S.mode === "draw") {
    let p = { x: clampI16(snap(wx)), y: clampI16(snap(wy)) };
    const near = S.doc.vertices.find((v) => Math.hypot(v.x - wx, v.y - wy) < 8 / S.view.scale);
    if (near) p = { x: near.x, y: near.y };
    const first = S.drawPts[0];
    if (first && S.drawPts.length >= 3 && p.x === first.x && p.y === first.y) { finishDraw(); return; }
    S.drawPts.push(p);
    render2d();
    return;
  }
  const hit = hitTest(wx, wy);
  if (hit >= 0) {
    if (e.shiftKey || e.ctrlKey || e.metaKey) {
      if (S.sel.has(hit) && (e.ctrlKey || e.metaKey)) S.sel.delete(hit); else S.sel.add(hit);
    } else if (!S.sel.has(hit)) {
      S.sel = new Set([hit]);
    }
    S.pick3d = null;
    renderInspector();
    // Prepare a move drag.
    const verts = S.mode === "things" ? null : selectionVertices();
    mouse.drag = {
      origin: [snap(wx), snap(wy)],
      verts: verts ? [...verts].map((i) => [i, vtx(i).x, vtx(i).y]) : null,
      things: S.mode === "things" ? [...S.sel].map((i) => [i, S.doc.things[i].x, S.doc.things[i].y]) : null,
      snapped: false,
    };
  } else {
    if (!e.shiftKey) S.sel.clear();
    mouse.box = [sx, sy, sx, sy];
    renderInspector();
  }
  render2d();
});

window.addEventListener("mousemove", (e) => {
  if (S.dukeMap) { dukeMouseMove(e); return; }
  const r = cv.getBoundingClientRect();
  const sx = e.clientX - r.left, sy = e.clientY - r.top;
  const over = sx >= 0 && sy >= 0 && sx <= r.width && sy <= r.height;
  if (!S.doc) return;
  const [wx, wy] = toWorld(sx, sy);
  if (over || mouse.down >= 0) {
    mouse.world = [wx, wy];
    $("#st-pos").textContent = `${Math.round(wx)}, ${Math.round(wy)}  ·  grid ${S.grid}  ·  zoom ${(S.view.scale * 100).toFixed(0)}%`;
  }
  if (mouse.start && Math.hypot(sx - mouse.start[0], sy - mouse.start[1]) > 3) mouse.moved = true;
  if (mouse.pan) {
    S.view.x = mouse.pan[2] - (sx - mouse.pan[0]) / S.view.scale;
    S.view.y = mouse.pan[3] + (sy - mouse.pan[1]) / S.view.scale;
    render2d();
    return;
  }
  if (mouse.box) {
    mouse.box[2] = sx; mouse.box[3] = sy;
    render2d();
    return;
  }
  if (mouse.drag && mouse.moved) {
    const dg = mouse.drag;
    if (!dg.snapped) { snapshot(); dg.snapped = true; }
    const dx = snap(wx) - dg.origin[0], dy = snap(wy) - dg.origin[1];
    if (dg.verts) for (const [i, x, y] of dg.verts) { S.doc.vertices[i].x = clampI16(x + dx); S.doc.vertices[i].y = clampI16(y + dy); }
    if (dg.things) for (const [i, x, y] of dg.things) { S.doc.things[i].x = clampI16(x + dx); S.doc.things[i].y = clampI16(y + dy); }
    S.version++;
    render2d();
    return;
  }
  if (over) {
    const h = S.mode === "draw" ? -1 : hitTest(wx, wy);
    if (h !== S.hover) { S.hover = h; render2d(); }
    if (S.mode === "draw") render2d();
  }
});

window.addEventListener("mouseup", () => {
  if (S.dukeMap) { dukeMouseUp(); return; }
  if (mouse.box && S.doc) {
    const [x0, y0, x1, y1] = mouse.box;
    const [ax, ay] = toWorld(Math.min(x0, x1), Math.max(y0, y1));
    const [bx, by] = toWorld(Math.max(x0, x1), Math.min(y0, y1));
    const inside = (p) => p.x >= ax && p.x <= bx && p.y >= ay && p.y <= by;
    const d = S.doc;
    if (S.mode === "vertices") d.vertices.forEach((v, i) => { if (inside(v)) S.sel.add(i); });
    if (S.mode === "lines") d.linedefs.forEach((l, i) => { if (inside(vtx(l.v1)) && inside(vtx(l.v2))) S.sel.add(i); });
    if (S.mode === "things") d.things.forEach((t, i) => { if (inside(t)) S.sel.add(i); });
    if (S.mode === "sectors") {
      const g = geometry();
      g.bbox.forEach((bb, i) => { if (bb[0] >= ax && bb[2] <= bx && bb[1] >= ay && bb[3] <= by) S.sel.add(i); });
    }
    mouse.box = null;
    renderInspector();
    render2d();
  }
  if (mouse.drag?.snapped) commit();
  mouse.drag = null; mouse.pan = null; mouse.down = -1; mouse.start = null;
});

cv.addEventListener("dblclick", (e) => {
  if (S.dukeMap) {
    const r = cv.getBoundingClientRect();
    const [wx, wy] = dukeToWorld(e.clientX - r.left, e.clientY - r.top);
    if (S.mode === "things") addDukeSprite(snap(wx), snap(wy));
    return;
  }
  if (!S.doc) return;
  const r = cv.getBoundingClientRect();
  const [wx, wy] = toWorld(e.clientX - r.left, e.clientY - r.top);
  if (S.mode === "things" && hitTest(wx, wy) < 0) { insertThing(wx, wy); return; }
  if (S.mode === "draw") return;
  S.mod.roomEditorSetCamera(wx, wy, NaN, S.cam?.yaw ?? Math.PI / 2);
  if (S.playing) setPlaying(false);
});

// ---------------------------------------------------------------- 3D view

const room = $("#room");
let look = false;
let lastClickHit = false;
room.addEventListener("contextmenu", (e) => e.preventDefault());
room.addEventListener("mousedown", (e) => {
  room.focus();
  if (S.playing) return;
  if (e.button === 2) {
    look = true;
    room.requestPointerLock?.();
  } else if (e.button === 0) {
    const r = room.getBoundingClientRect();
    const nx = ((e.clientX - r.left) / r.width) * 2 - 1;
    const ny = 1 - ((e.clientY - r.top) / r.height) * 2;
    const hit = S.mod.roomEditorPick(nx, ny);
    lastClickHit = !!hit;
    if (hit) applyPick(JSON.parse(hit), e.shiftKey);
  }
});
room.addEventListener("dblclick", (e) => {
  if (S.playing || e.button !== 0 || !lastClickHit) return;
  e.preventDefault();
  editPicked();
});
window.addEventListener("mouseup", (e) => {
  if (e.button === 2 && look) { look = false; if (document.pointerLockElement === room && !S.playing) document.exitPointerLock(); }
});
window.addEventListener("mousemove", (e) => {
  if (look && !S.playing) S.mod.roomEditorLook(e.movementX, e.movementY);
});

// 3D overlay: outline the selected things (solid) and the one under the mouse (dashed).
const overlay = $("#room-overlay");
const octx = overlay.getContext("2d");
let hover3d = -1;
let hoverEvent = null;
room.addEventListener("mousemove", (e) => { hoverEvent = look ? null : e; });
room.addEventListener("mouseleave", () => { hoverEvent = null; hover3d = -1; room.style.cursor = ""; });

function updateHover3d() {
  const e = hoverEvent;
  hoverEvent = null;
  const r = room.getBoundingClientRect();
  const hit = S.mod.roomEditorPick(((e.clientX - r.left) / r.width) * 2 - 1, 1 - ((e.clientY - r.top) / r.height) * 2);
  const h = hit ? JSON.parse(hit) : null;
  hover3d = h?.kind === "thing" ? h.index : -1;
  room.style.cursor = hover3d >= 0 ? "pointer" : "";
}

function drawOverlay() {
  requestAnimationFrame(drawOverlay);
  const dpr = window.devicePixelRatio || 1;
  const w = overlay.clientWidth, h = overlay.clientHeight;
  if (overlay.width !== Math.round(w * dpr) || overlay.height !== Math.round(h * dpr)) {
    overlay.width = Math.round(w * dpr); overlay.height = Math.round(h * dpr);
  }
  octx.setTransform(dpr, 0, 0, dpr, 0, 0);
  octx.clearRect(0, 0, w, h);
  if (!S.mod || !S.doc || S.playing || !w) return;
  if (look) hover3d = -1;
  else if (hoverEvent) updateHover3d();
  const sel = S.mode === "things" ? [...S.sel] : [];
  const ids = new Set(sel);
  if (hover3d >= 0) ids.add(hover3d);
  if (!ids.size) return;
  const rects = S.mod.roomEditorThingRects(Uint32Array.from(ids));
  octx.font = "600 11px ui-monospace, Menlo, monospace";
  octx.textBaseline = "bottom";
  for (let k = 0; k < rects.length; k += 6) {
    const i = rects[k];
    octx.globalAlpha = rects[k + 5] ? 1 : 0.35;
    const x0 = ((rects[k + 1] + 1) / 2) * w - 4, x1 = ((rects[k + 3] + 1) / 2) * w + 4;
    const y0 = ((1 - rects[k + 4]) / 2) * h - 4, y1 = ((1 - rects[k + 2]) / 2) * h + 4;
    const selected = S.mode === "things" && S.sel.has(i);
    const color = selected ? "#ffcc33" : "#7fd4ff";
    octx.lineWidth = selected ? 2 : 1.5;
    octx.strokeStyle = color;
    octx.setLineDash(selected ? [] : [5, 4]);
    octx.fillStyle = selected ? "rgba(255,204,51,0.10)" : "rgba(127,212,255,0.08)";
    octx.fillRect(x0, y0, x1 - x0, y1 - y0);
    octx.strokeRect(x0, y0, x1 - x0, y1 - y0);
    const t = S.doc.things[i];
    const label = `${i} · ${S.thingInfo.get(t?.type_id)?.title || t?.type_id}`;
    const tw = octx.measureText(label).width + 8;
    const ly = y0 > 18 ? y0 - 2 : y1 + 16;
    octx.fillStyle = "rgba(0,0,0,0.7)";
    octx.fillRect(x0, ly - 15, tw, 15);
    octx.fillStyle = color;
    octx.fillText(label, x0 + 4, ly - 2);
  }
  octx.globalAlpha = 1;
  octx.setLineDash([]);
}
requestAnimationFrame(drawOverlay);

function applyPick(hit, add) {
  const mode = { thing: "things", wall: "lines", floor: "sectors", ceiling: "sectors" }[hit.kind];
  if (S.mode !== mode) { S.mode = mode; S.sel.clear(); syncModeButtons(); }
  if (add) S.sel.add(hit.index); else S.sel = new Set([hit.index]);
  S.pick3d = hit;
  renderInspector();
  ensureVisible();
  render2d();
  status(`Picked ${hit.kind} ${hit.index}${hit.part ? ` (${hit.side ? "back" : "front"} ${hit.part})` : ""}`);
}

function ensureVisible() {
  const d = S.doc;
  let p = null;
  const i = [...S.sel][0];
  if (S.mode === "things") p = d.things[i];
  else if (S.mode === "lines") { const l = d.linedefs[i]; const a = vtx(l.v1), b = vtx(l.v2); p = { x: (a.x + b.x) / 2, y: (a.y + b.y) / 2 }; }
  else if (S.mode === "sectors") { const bb = geometry().bbox[i]; if (bb) p = { x: (bb[0] + bb[2]) / 2, y: (bb[1] + bb[3]) / 2 }; }
  if (!p) return;
  const [sx, sy] = toScreen(p.x, p.y);
  if (sx < 0 || sy < 0 || sx > cv.clientWidth || sy > cv.clientHeight) { S.view.x = p.x; S.view.y = p.y; }
}

/** Picked 3D surface → the sidedef / sector field it refers to. */
function pickTarget() {
  const h = S.pick3d;
  if (!h || !S.doc) return null;
  if (h.kind === "wall") {
    const l = S.doc.linedefs[h.index];
    const s = side(h.side ? l.side_back : l.side_front);
    return s ? { obj: s, key: h.part, sector: s.sector } : null;
  }
  if (h.kind === "floor" || h.kind === "ceiling") {
    return { obj: S.doc.sectors[h.index], key: h.kind === "floor" ? "floor_pic" : "ceiling_pic", sector: h.index, plane: h.kind };
  }
  return null;
}

/** Open the picked 3D object in the pixel editor (thing sprite, wall texture, flat). */
function editPicked() {
  const h = S.pick3d;
  if (h?.kind === "thing") {
    const num = S.doc.things[h.index]?.type_id;
    const sprite = S.thingInfo.get(num)?.sprite;
    if (!sprite) { toast("This thing has no sprite to edit", true); return false; }
    paint.open("sprites", sprite, { num });
    return true;
  }
  const t = pickTarget();
  if (!t) return false;
  paint.open(t.plane ? "flats" : "textures", t.obj[t.key]);
  return true;
}

function key3d(e) {
  if (e.code === "KeyP") return editPicked();
  const t = pickTarget();
  if (!t) return false;
  const sec = S.doc.sectors[t.sector];
  const step = e.shiftKey ? 1 : 8;
  if (e.code === "PageUp" || e.code === "PageDown") {
    const delta = e.code === "PageUp" ? step : -step;
    snapshot();
    if (t.plane) sec[t.plane === "floor" ? "floor_height" : "ceiling_height"] += delta;
    else t.obj.y_offset = clampI16(t.obj.y_offset - delta);
    commit();
    return true;
  }
  if (e.code === "Equal" || e.code === "Minus" || e.code === "NumpadAdd" || e.code === "NumpadSubtract") {
    snapshot();
    sec.light = Math.max(0, Math.min(255, sec.light + (e.code === "Equal" || e.code === "NumpadAdd" ? 16 : -16)));
    commit();
    return true;
  }
  if (e.code === "KeyT") {
    snapshot();
    t.obj[t.key] = t.plane ? S.flat : S.texture;
    commit();
    return true;
  }
  if (e.code === "BracketLeft" || e.code === "BracketRight") {
    if (t.plane) return false;
    snapshot();
    t.obj.x_offset = clampI16(t.obj.x_offset + (e.code === "BracketRight" ? step : -step));
    commit();
    return true;
  }
  return false;
}

function setPlaying(on, mode) {
  if (on) {
    try {
      S.mod.roomEditorSetSkill(Number($("#skill").value));
      if (S.dukeMap) {
        S.dukeMapBytes = S.mod.roomEditorDukeMapPut(S.doc.name, JSON.stringify(S.dukeMap));
        const overlayGrp = S.mod.roomEditorDukeTestOverlay();
        S.mod.roomSetDukeEditorGrp(overlayGrp);
        S.mod.roomEditorDukePlay(S.dukeGrp, S.dukeMapBytes, Number($("#skill").value));
        S.dukeTesting = true;
        document.body.classList.add("duke-testing");
      } else {
        S.mod.roomEditorPlay(mode || "start");
      }
    } catch (e) { toast(errText(e), true); return; }
    S.playing = true;
    room.focus();
    status(S.dukeTesting
      ? "Native Duke test · click the 3D view to capture the mouse · F7 returns to Build editing"
      : "Play test — click the 3D view to capture the mouse · Esc releases · F7 back to edit");
  } else {
    if (S.playing && S.mod) {
      try { S.mod.roomEditorPlay("edit"); } catch (e) { toast(errText(e), true); }
    }
    S.playing = false;
    S.dukeTesting = false;
    document.body.classList.remove("duke-testing");
    if (document.pointerLockElement === room) document.exitPointerLock();
  }
  syncPlayUi();
}

function syncPlayUi() {
  $("#btn-stop").disabled = !S.playing;
  const b = $("#badge3d");
  b.textContent = S.playing ? "PLAYING" : (S.visual ? "VISUAL" : "EDIT");
  b.classList.toggle("playing", S.playing);
  $("#hint3d").textContent = S.playing
    ? S.dukeTesting
      ? "Native Duke test — WASD move · mouse look · Ctrl/LMB fire · Space jump · F7 returns to Build editing"
      : "Playing — WASD move · mouse look · wheel weapon · Ctrl/LMB fire · E/Space use · edits apply live"
    : S.visual
      ? "Visual mode (M exits) — Click: select · Double-click / P: edit pixels · Right-drag: look · WASD/Q/Z: fly · PgUp/PgDn height · T paint · +/- light"
      : "Click: select · Double-click / P: edit pixels · Right-drag: look · WASD/Q/Z: fly · Shift: fast · PgUp/PgDn height · T paint · +/- light · M visual";
}

function currentLayout() {
  const active = document.querySelector(".layout .btn.active[data-layout]");
  return active?.dataset.layout || "split";
}

function applyLayout(layout) {
  const mv = $("#map-view");
  document.querySelectorAll(".layout .btn[data-layout]").forEach((x) => {
    x.classList.toggle("active", x.dataset.layout === layout);
  });
  mv.className = { "2d": "only2d", split: "split", "3d": "only3d" }[layout] || "split";
  render2d();
}

/** Doom Builder–style visual mode: full-bleed 3D edit (toggle with M). */
function setVisualMode(on) {
  const next = !!on;
  if (S.visual === next) return;
  if (next) {
    S.layoutBeforeVisual = currentLayout();
    S.visual = true;
    document.body.classList.add("visual-mode");
    applyLayout("3d");
    room.focus();
    status("Visual mode — fly and edit in 3D · M or Visual to exit");
  } else {
    S.visual = false;
    document.body.classList.remove("visual-mode");
    applyLayout(S.layoutBeforeVisual || "split");
    status("Classic edit");
  }
  syncPlayUi();
}

async function addPackFiles(files) {
  await importPk3Files(files);
}

function pollCamera() {
  if (S.mod && S.doc) {
    const c = S.mod.roomEditorCamera();
    const cam = c ? JSON.parse(c) : null;
    const prev = S.cam;
    S.cam = cam;
    if (cam && cam.playing !== S.playing) { S.playing = cam.playing; syncPlayUi(); }
    if (cam && (!prev || Math.abs(prev.x - cam.x) > 0.5 || Math.abs(prev.y - cam.y) > 0.5 || Math.abs(prev.yaw - cam.yaw) > 0.01)) render2d();
  }
  setTimeout(pollCamera, 80);
}

// ---------------------------------------------------------------- inspector

function rowEl(label, input, extra) {
  return el("div", { class: "row" }, el("label", { title: label }, label), extra ? el("div", { class: "tex-field" }, input, extra) : input);
}

/** Bind an input to `key` on every object in `objs`. */
function fieldFor(objs, key, opts = {}) {
  const v0 = objs[0][key];
  const mixed = objs.some((o) => JSON.stringify(o[key]) !== JSON.stringify(v0));
  let input;
  let snapped = false;
  const apply = (val, live = true) => {
    if (!snapped) { snapshot(); snapped = true; }
    for (const o of objs) o[key] = val;
    commit({ inspector: false });
    if (!live) renderInspector();
  };
  const onBlur = () => { snapped = false; };
  if (opts.flags) {
    const wrap = el("div", { class: "flags" });
    for (const [bit, name] of opts.flags) {
      const on = (v0 & bit) !== 0;
      const cb = el("input", { type: "checkbox", checked: on });
      cb.addEventListener("change", () => {
        snapshot();
        for (const o of objs) o[key] = cb.checked ? o[key] | bit : o[key] & ~bit;
        commit({ inspector: false });
      });
      wrap.append(el("label", {}, cb, name));
    }
    return wrap;
  }
  if (opts.options) {
    input = el("select", {});
    const known = Object.keys(opts.options).map(Number);
    if (!known.includes(v0)) input.append(el("option", { value: v0 }, `${v0} (custom)`));
    for (const [k, name] of Object.entries(opts.options)) input.append(el("option", { value: k, selected: Number(k) === v0 }, `${k} — ${name}`));
    input.addEventListener("change", () => { apply(Number(input.value), false); snapped = false; });
    const num = el("input", { type: "number", value: v0, style: "width:70px" });
    num.addEventListener("change", () => { apply(Math.round(Number(num.value) || 0), false); snapped = false; });
    return el("div", { class: "tex-field" }, input, num);
  }
  if (typeof v0 === "boolean") {
    input = el("input", { type: "checkbox", checked: v0 });
    input.addEventListener("change", () => { apply(input.checked); snapped = false; });
    return input;
  }
  if (typeof v0 === "number") {
    input = el("input", { type: "number", value: mixed ? "" : v0, placeholder: mixed ? "mixed" : "", step: FLOAT_KEYS.has(key) ? "any" : opts.step || 1 });
    input.addEventListener("input", () => {
      if (input.value === "") return;
      let n = Number(input.value);
      if (!Number.isFinite(n)) return;
      if (!FLOAT_KEYS.has(key)) n = Math.round(n);
      if (opts.min != null) n = Math.max(opts.min, n);
      if (opts.max != null) n = Math.min(opts.max, n);
      apply(n);
    });
    input.addEventListener("blur", onBlur);
    if (opts.range) {
      const rng = el("input", { type: "range", min: opts.min, max: opts.max, value: v0, style: "flex:1" });
      rng.addEventListener("input", () => { input.value = rng.value; apply(Number(rng.value)); });
      rng.addEventListener("change", onBlur);
      input.addEventListener("input", () => { rng.value = input.value; });
      return el("div", { class: "tex-field" }, rng, input);
    }
    return input;
  }
  if (typeof v0 === "string") {
    input = el("input", { type: "text", value: mixed ? "" : v0, placeholder: mixed ? "mixed" : "", spellcheck: "false" });
    if (opts.tex || opts.flat) {
      input.style.textTransform = "uppercase";
      const thumbC = el("canvas", { title: "Pick…" });
      drawThumb(thumbC, v0, 40);
      thumbC.addEventListener("click", () => openPicker(opts.flat ? "flats" : "textures", (name) => { input.value = name; drawThumb(thumbC, name, 40); apply(name); snapped = false; }));
      input.addEventListener("change", () => { const v = input.value.trim().toUpperCase() || "-"; drawThumb(thumbC, v, 40); apply(v); snapped = false; });
      const edit = el("button", { class: "btn small icon", title: "Edit pixels", onclick: () => paint.open(opts.flat ? "flats" : "textures", input.value) }, "✎");
      const wrap = el("div", { class: "tex-field" }, thumbC, input, edit);
      return wrap;
    }
    input.addEventListener("change", () => { apply(input.value); snapped = false; });
    return input;
  }
  if (Array.isArray(v0) && v0.every((x) => typeof x === "number")) {
    const wrap = el("div", { class: "args" });
    v0.forEach((x, j) => {
      const inp = el("input", { type: "number", value: x });
      inp.addEventListener("change", () => {
        snapshot();
        for (const o of objs) { o[key] = [...o[key]]; o[key][j] = Math.round(Number(inp.value) || 0); }
        commit({ inspector: false });
      });
      wrap.append(inp);
    });
    return wrap;
  }
  input = el("input", { type: "text", value: JSON.stringify(v0), spellcheck: "false" });
  input.addEventListener("change", () => {
    try { apply(JSON.parse(input.value)); } catch (_) { toast(`${key}: invalid JSON`, true); }
    snapped = false;
  });
  return input;
}

function fieldset(title, objs, keys, opts = {}) {
  const fs = el("fieldset", {}, el("legend", {}, title));
  for (const k of keys) {
    if (!(k in objs[0])) continue;
    fs.append(rowEl(opts.labels?.[k] || k, fieldFor(objs, k, opts.fields?.[k] || {})));
  }
  return fs;
}

function advanced(title, objs, skip) {
  const keys = Object.keys(objs[0]).filter((k) => !skip.includes(k));
  if (!keys.length) return null;
  const det = el("details", {}, el("summary", { class: "muted", style: "cursor:pointer;margin:4px 0 8px" }, `${title} (${keys.length} fields)`));
  const fs = el("fieldset", {});
  for (const k of keys) fs.append(rowEl(k, fieldFor(objs, k, { tex: TEX_KEYS.has(k), flat: FLAT_KEYS.has(k) })));
  det.append(fs);
  return det;
}

function dukeSnapshot() {
  if (!S.dukeMap) return;
  const snapshot = JSON.stringify(S.dukeMap);
  S.dukeUndo.push(snapshot);
  if (S.dukeUndo.length > 120) S.dukeUndo.shift();
  S.dukeRedo = [];
  S.dukePersistBefore = snapshot;
  return snapshot;
}

function dukePersist() {
  const rollback = S.dukePersistBefore;
  S.dukePersistBefore = null;
  try {
    S.dukeMapBytes = S.mod.roomEditorDukeMapPut(S.doc.name, JSON.stringify(S.dukeMap));
    syncPill("live", "Build map saved in editor");
    status(`${S.dukeMapFile} · native Build map · ${S.dukeMap.walls.length} walls · ${S.dukeMap.sprites.length} sprites`);
    render2d();
  } catch (e) {
    if (rollback) {
      S.dukeMap = JSON.parse(rollback);
      S.dukeUndo.pop();
      S.dukeSelection = null;
    }
    syncPill("error", "map error");
    toast(`Build map: ${errText(e)}`, true);
    renderInspector();
    render2d();
  }
}

function dukeUndoAction(redo = false) {
  const from = redo ? S.dukeRedo : S.dukeUndo;
  const to = redo ? S.dukeUndo : S.dukeRedo;
  if (!from.length) return false;
  to.push(JSON.stringify(S.dukeMap));
  S.dukeMap = JSON.parse(from.pop());
  dukePersist();
  renderInspector();
  return true;
}

function dukeEditField(obj, key, value, selection = S.dukeSelection) {
  if (!obj || !selection) return;
  const n = Number(value);
  if (!Number.isFinite(n)) { renderInspector(); return; }
  dukeSnapshot();
  const v = Math.trunc(n);
  if (selection.kind === "vertex" && (key === "x" || key === "y")) {
    const old = obj[key];
    for (const wall of S.dukeMap.walls) if (wall[key] === old && wall["xy"[key === "x" ? 0 : 1]] === selection[key === "x" ? "y" : "x"]) wall[key] = v;
    selection[key] = v;
  } else {
    obj[key] = v;
    if (selection.kind === "start") S.dukeMap[key] = v;
  }
  dukePersist();
  renderInspector();
}

function dukeField(label, obj, key, selection, bounds = {}) {
  const field = el("input", {
    type: "number", value: obj[key], step: 1,
    ...(bounds.min == null ? {} : { min: bounds.min }),
    ...(bounds.max == null ? {} : { max: bounds.max }),
    onchange: (e) => dukeEditField(obj, key, e.target.value, selection),
  });
  return rowEl(label, field);
}

function dukeTileField(label, obj, key, selection) {
  const value = obj[key];
  const name = value >= 0 ? `BT${String(value).padStart(4, "0")}` : "-";
  const thumbCanvas = el("canvas", { width: 40, height: 40, title: value >= 0 ? `Pick ART tile ${value}` : "No tile" });
  drawThumb(thumbCanvas, name, 40);
  thumbCanvas.addEventListener("click", () => openPicker("textures", (tile) => {
    const match = /^BT(\d{1,5})$/.exec(tile);
    if (match) dukeEditField(obj, key, Number(match[1]), selection);
  }));
  const numeric = el("input", {
    type: "number", value, step: 1,
    onchange: (e) => dukeEditField(obj, key, e.target.value, selection),
  });
  const editButton = el("button", {
    class: "btn small icon", title: value >= 0 ? `Edit ART tile ${value}` : "Choose a tile first",
    disabled: value < 0, onclick: () => paint.open("textures", name),
  }, "✎");
  return rowEl(label, el("div", { class: "tex-field" }, thumbCanvas, numeric, editButton));
}

function renderDukeInspector(body, title) {
  const m = S.dukeMap;
  const sel = S.dukeSelection;
  const fieldset = (legend, obj, keys, selection = sel) => {
    const fs = el("fieldset", {}, el("legend", {}, legend));
    for (const key of keys) {
      if (!(key in obj)) continue;
      if (["picnum", "overpicnum", "floorpicnum", "ceilingpicnum"].includes(key)) {
        fs.append(dukeTileField(key, obj, key, selection));
      } else fs.append(dukeField(key, obj, key, selection));
    }
    return fs;
  };
  const floorHeights = m.sectors.map((sector) => sector.floorz).filter(Number.isFinite);
  const floorRange = floorHeights.length ? (() => {
    const [min, max] = floorHeights.reduce(([lo, hi], value) => [Math.min(lo, value), Math.max(hi, value)], [Infinity, -Infinity]);
    return `${min} to ${max} Build units`;
  })() : "—";
  body.append(
    el("fieldset", {}, el("legend", {}, "Native Build map"),
      rowEl("File", el("span", {}, S.dukeMapFile || "—")),
      rowEl("Version", el("span", {}, m.version)),
      rowEl("Sectors", el("span", {}, m.sectors.length)),
      rowEl("Floor range", el("span", {}, floorRange)),
      rowEl("Walls", el("span", {}, m.walls.length)),
      rowEl("Sprites", el("span", {}, m.sprites.length)),
      el("div", { class: "actions" },
        el("button", { class: "btn small", onclick: () => setMode("draw") }, "+ Sector"),
        el("button", { class: "btn small", onclick: () => addDukeSprite() }, "+ Sprite at start"),
        el("button", { class: "btn small", onclick: () => download(S.dukeMapBytes, S.dukeMapFile || `${m.name || "MAP"}.MAP`) }, "Save .MAP"))),
  );
  if (!sel) {
    title.textContent = "Build map";
    const start = { x: m.start[0], y: m.start[1], z: m.start[2], ang: m.start_ang, sector: m.start_sector };
    const fs = el("fieldset", {}, el("legend", {}, "Player start"));
    for (const [key, label] of [["x", "X"], ["y", "Y"], ["z", "Z"], ["ang", "Angle"], ["sector", "Sector"]]) {
      const input = el("input", { type: "number", value: start[key], step: 1, onchange: (e) => {
        const v = Math.trunc(Number(e.target.value));
        if (!Number.isFinite(v)) return;
        dukeSnapshot();
        if (key === "ang") m.start_ang = v; else if (key === "sector") m.start_sector = v; else m.start[({ x: 0, y: 1, z: 2 })[key]] = v;
        dukePersist();
      }});
      fs.append(rowEl(label, input));
    }
    const floors = el("fieldset", { class: "duke-floor-overview" },
      el("legend", {}, `Sector floors (${m.sectors.length})`),
      el("p", { class: "muted" }, "Build Z is shown in native units. Click a row to edit that sector's floor and tile."),
      el("div", { class: "duke-floor-list" }, ...m.sectors.map((sector, index) => {
        const tile = sector.floorpicnum >= 0 ? `BT${String(sector.floorpicnum).padStart(4, "0")}` : "No tile";
        const button = el("button", { class: "duke-floor-row", type: "button", title: `Edit sector ${index}`,
          onclick: () => {
            S.dukeSelection = { kind: "sector", index };
            renderInspector();
            render2d();
          } },
        el("strong", {}, `Sector ${index}`),
        el("span", { class: "duke-floor-z" }, `Floor Z ${sector.floorz}`),
        el("span", { class: "duke-floor-tile" }, tile));
        return button;
      })));
    body.append(fs, floors,
      el("p", { class: "muted" }, "This document is the native Build .MAP. Changes export into the original GRP format."));
    return;
  }
  let obj;
  let keys;
  if (sel.kind === "vertex") {
    obj = m.walls[sel.index];
    title.textContent = `Build vertex (${sel.x}, ${sel.y})`;
    keys = ["x", "y"];
  } else if (sel.kind === "wall") {
    obj = m.walls[sel.index];
    title.textContent = `Build wall ${sel.index}`;
    keys = ["point2", "nextwall", "nextsector", "cstat", "picnum", "overpicnum", "shade", "pal", "xrepeat", "yrepeat", "xpanning", "ypanning", "lotag", "hitag", "extra"];
  } else if (sel.kind === "sector") {
    obj = m.sectors[sel.index];
    title.textContent = `Build sector ${sel.index}`;
    keys = ["wallptr", "wallnum", "ceilingz", "floorz", "ceilingstat", "floorstat", "ceilingpicnum", "ceilingheinum", "ceilingshade", "ceilingpal", "ceilingxpanning", "ceilingypanning", "floorpicnum", "floorheinum", "floorshade", "floorpal", "floorxpanning", "floorypanning", "visibility", "lotag", "hitag", "extra"];
  } else {
    obj = m.sprites[sel.index];
    title.textContent = `Build sprite ${sel.index}`;
    keys = ["x", "y", "z", "cstat", "picnum", "shade", "pal", "clipdist", "xrepeat", "yrepeat", "xoffset", "yoffset", "sectnum", "statnum", "ang", "owner", "xvel", "yvel", "zvel", "lotag", "hitag", "extra"];
  }
  if (!obj) { S.dukeSelection = null; renderDukeInspector(body, title); return; }
  const fs = fieldset(sel.kind[0].toUpperCase() + sel.kind.slice(1), obj, keys);
  if (sel.kind === "sector") {
    fs.append(el("button", { class: "btn small danger", onclick: () => deleteDukeSector(sel.index) }, "Delete sector"));
  }
  if (sel.kind === "sprite") {
    fs.append(el("button", { class: "btn small danger", onclick: () => deleteDukeSprite(sel.index) }, "Delete sprite"));
  }
  body.append(fs);
}

function addDukeSprite(x = S.dukeMap.start[0], y = S.dukeMap.start[1]) {
  dukeSnapshot();
  const sector = dukeSectorAt(x, y);
  const sectnum = sector >= 0 ? sector : S.dukeMap.start_sector;
  const z = S.dukeMap.sectors[sectnum]?.floorz ?? S.dukeMap.start[2];
  const sprite = { x: Math.round(x), y: Math.round(y), z, cstat: 0, picnum: 1405,
    shade: 0, pal: 0, clipdist: 32, xrepeat: 32, yrepeat: 32, xoffset: 0, yoffset: 0,
    sectnum, statnum: 0, ang: 0, owner: -1, xvel: 0, yvel: 0, zvel: 0, lotag: 0, hitag: 0, extra: -1 };
  S.dukeMap.sprites.push(sprite);
  S.dukeSelection = { kind: "sprite", index: S.dukeMap.sprites.length - 1 };
  dukePersist();
  renderInspector();
}

function deleteDukeSprite(index) {
  dukeSnapshot();
  S.dukeMap.sprites.splice(index, 1);
  S.dukeSelection = null;
  dukePersist();
  renderInspector();
}

function renderInspector() {
  const body = $("#inspector-body");
  const title = $("#inspector-title");
  body.replaceChildren();
  if (S.dukeMap) { renderDukeInspector(body, title); return; }
  if (S.actorSel) { renderActorInspector(body, title); return; }
  const d = S.doc;
  if (!d) { title.textContent = "Inspector"; return; }
  const idx = [...S.sel];
  if (!idx.length) {
    title.textContent = `Map ${d.name}`;
    body.append(
      el("fieldset", {}, el("legend", {}, "Map"),
        rowEl("Name", el("input", { type: "text", value: d.name, onchange: (e) => renameMap(e.target.value) })),
        rowEl("Format", el("span", {}, d.format)),
        rowEl("Vertices", el("span", {}, d.vertices.length)),
        rowEl("Lines", el("span", {}, d.linedefs.length)),
        rowEl("Sides", el("span", {}, d.sidedefs.length)),
        rowEl("Sectors", el("span", {}, d.sectors.length)),
        rowEl("Things", el("span", {}, d.things.length))),
      el("p", { class: "muted" }, "Pick a mode (V/L/S/T), then click in the 2D view or the 3D view. Everything you change is pushed to the running level immediately."),
      el("fieldset", {}, el("legend", {}, "Current paint"),
        rowEl("Wall texture", fieldTexPick("textures")),
        rowEl("Flat", fieldTexPick("flats"))),
    );
    updateCounts();
    return;
  }
  const n = idx.length;
  if (S.mode === "vertices") {
    title.textContent = n > 1 ? `${n} vertices` : `Vertex ${idx[0]}`;
    body.append(fieldset("Position", idx.map((i) => d.vertices[i]), ["x", "y"]));
  } else if (S.mode === "things") {
    const objs = idx.map((i) => d.things[i]);
    const info = S.thingInfo.get(objs[0].type_id);
    title.textContent = n > 1 ? `${n} things` : `Thing ${idx[0]} — ${info?.title || objs[0].type_id}`;
    const typeSel = el("select", {});
    for (const cat of CAT_ORDER) {
      const og = el("optgroup", { label: cat });
      S.summary.things.filter((t) => t.cat === cat).sort((a, b) => a.title.localeCompare(b.title))
        .forEach((t) => og.append(el("option", { value: t.num, selected: t.num === objs[0].type_id }, `${t.title} (${t.num})`)));
      typeSel.append(og);
    }
    typeSel.addEventListener("change", () => { snapshot(); objs.forEach((o) => { o.type_id = Number(typeSel.value); }); commit(); });
    const icon = el("canvas", { class: "thing-icon" });
    if (info?.sprite) {
      drawThumb(icon, info.sprite, 34);
      icon.title = "Edit sprite & animations";
      icon.style.cursor = "pointer";
      icon.addEventListener("click", () => paint.open("sprites", info.sprite, { num: objs[0].type_id }));
    }
    const faceBtn = info?.sprite
      ? el("button", {
        class: "btn small",
        title: "Open the sprite painter and capture a webcam face onto this enemy",
        onclick: () => paint.open("sprites", info.sprite, { num: objs[0].type_id, camera: true }),
      }, "Camera face…")
      : null;
    const angles = { 0: "East", 45: "NE", 90: "North", 135: "NW", 180: "West", 225: "SW", 270: "South", 315: "SE" };
    body.append(
      el("fieldset", {}, el("legend", {}, "Type"),
        el("div", { class: "tex-field" }, icon, typeSel),
        ...(faceBtn ? [faceBtn] : []),
        rowEl("type_id", fieldFor(objs, "type_id"))),
      fieldset("Placement", objs, ["x", "y", "z", "angle"], { fields: { angle: { options: angles } } }),
      el("fieldset", {}, el("legend", {}, "Flags"), fieldFor(objs, "flags", { flags: isHexen() ? HEXEN_THING_FLAGS : DOOM_THING_FLAGS })),
      ...((isHexen() || isUdmf()) ? [fieldset("Action", objs, ["tid", "special", "args"], { fields: { special: { options: HEXEN_LINE_SPECIALS } } })] : []),
      advanced("All fields", objs, ["x", "y", "z", "angle", "type_id", "flags", "tid", "special", "args"]),
    );
  } else if (S.mode === "lines") {
    const objs = idx.map((i) => d.linedefs[i]);
    title.textContent = n > 1 ? `${n} lines` : `Line ${idx[0]}`;
    const actions = el("div", { class: "actions" },
      el("button", { class: "btn small", onclick: flipLines }, "Flip (F)"),
      el("button", { class: "btn small", onclick: splitLines }, "Split (X)"),
      el("button", { class: "btn small", title: "Align x offsets along connected walls with the same texture", onclick: () => tools.autoAlignSelection() }, "Auto-align (J)"),
      el("button", { class: "btn small danger", onclick: deleteSelection }, "Delete"));
    body.append(actions);
    body.append(el("fieldset", {}, el("legend", {}, "Flags"), fieldFor(objs, "flags", { flags: isHexen() || isUdmf() ? HEXEN_LINE_FLAGS : DOOM_LINE_FLAGS })));
    const act = el("fieldset", {}, el("legend", {}, "Action"));
    act.append(rowEl("special", fieldFor(objs, "special", { options: isHexen() || isUdmf() ? HEXEN_LINE_SPECIALS : DOOM_LINE_SPECIALS })));
    act.append(rowEl("tag", fieldFor(objs, "tag")));
    if (isHexen() || isUdmf()) act.append(rowEl("args", fieldFor(objs, "args")));
    if (isUdmf()) act.append(el("div", { style: "margin-top:4px" }, fieldFor(objs, "activation", { flags: SPAC_BITS })));
    if (isHexen()) {
      const actSel = el("select", {});
      const cur = (objs[0].flags >> 10) & 7;
      HEXEN_ACTIVATION.forEach((name, i) => actSel.append(el("option", { value: i, selected: i === cur }, name)));
      actSel.addEventListener("change", () => { snapshot(); objs.forEach((o) => { o.flags = (o.flags & ~0x1c00) | (Number(actSel.value) << 10); }); commit(); });
      act.append(rowEl("activation", actSel));
    }
    body.append(act);
    const pick = S.pick3d?.kind === "wall" ? S.pick3d : null;
    for (const [sk, label] of [["side_front", "Front side"], ["side_back", "Back side"]]) {
      const sides = objs.map((l) => side(l[sk])).filter(Boolean);
      if (!sides.length) {
        if (n === 1 && sk === "side_back") {
          body.append(el("div", { class: "actions" }, el("button", { class: "btn small", onclick: () => addBackSide(objs[0]) }, "Add back side")));
        }
        continue;
      }
      const fs = fieldset(`${label}${n === 1 ? ` (#${objs[0][sk]})` : ""}`, sides, ["upper", "middle", "lower", "x_offset", "y_offset", "sector"],
        { fields: { upper: { tex: true }, middle: { tex: true }, lower: { tex: true } } });
      if (pick && ((pick.side === 1) === (sk === "side_back"))) fs.style.borderColor = "#ffcc33";
      body.append(fs);
      const adv = advanced(`${label} — all fields`, sides, ["upper", "middle", "lower", "x_offset", "y_offset", "sector"]);
      if (adv) body.append(adv);
    }
    body.append(advanced("Line — all fields", objs, ["v1", "v2", "flags", "special", "tag", "args", "activation", "side_front", "side_back", "hexen"]));
  } else if (S.mode === "sectors") {
    const objs = idx.map((i) => d.sectors[i]);
    title.textContent = n > 1 ? `${n} sectors` : `Sector ${idx[0]}`;
    body.append(el("div", { class: "actions" },
      el("button", { class: "btn small", onclick: makeDoor }, "Make door"),
      el("button", { class: "btn small", onclick: () => adjustSectors("floor_height", 8) }, "Floor +8"),
      el("button", { class: "btn small", onclick: () => adjustSectors("floor_height", -8) }, "Floor −8"),
      el("button", { class: "btn small", onclick: () => adjustSectors("ceiling_height", 8) }, "Ceil +8"),
      el("button", { class: "btn small", onclick: () => adjustSectors("ceiling_height", -8) }, "Ceil −8"),
      el("button", { class: "btn small danger", onclick: deleteSelection }, "Delete")));
    body.append(fieldset("Heights", objs, ["floor_height", "ceiling_height"]));
    const flats = fieldset("Flats", objs, ["floor_pic", "ceiling_pic"], { fields: { floor_pic: { flat: true }, ceiling_pic: { flat: true } } });
    if (S.pick3d?.kind === "floor" || S.pick3d?.kind === "ceiling") flats.style.borderColor = "#ffcc33";
    body.append(flats);
    body.append(fieldset("Light & action", objs, ["light", "special", "tag"], {
      fields: { light: { range: true, min: 0, max: 255 }, special: isHexen() || isUdmf() ? {} : { options: DOOM_SECTOR_SPECIALS } },
    }));
    body.append(advanced("All fields", objs, ["floor_height", "ceiling_height", "floor_pic", "ceiling_pic", "light", "special", "tag"]));
  }
  updateCounts();
}

function fieldTexPick(kind) {
  const c = el("canvas", { title: "Pick…" });
  const name = kind === "flats" ? S.flat : S.texture;
  drawThumb(c, name, 40);
  const lbl = el("span", { class: "name" }, name);
  c.addEventListener("click", () => openPicker(kind, (n) => {
    if (kind === "flats") S.flat = n; else S.texture = n;
    renderInspector();
  }));
  const edit = el("button", { class: "btn small icon", title: "Edit pixels", onclick: () => paint.open(kind, name) }, "✎");
  return el("div", { class: "tex-field" }, c, lbl, edit);
}

function addBackSide(line) {
  snapshot();
  const f = side(line.side_front);
  const a = vtx(line.v1), b = vtx(line.v2);
  // Sample just left of the line (back side).
  const mx = (a.x + b.x) / 2, my = (a.y + b.y) / 2;
  const len = Math.hypot(b.x - a.x, b.y - a.y) || 1;
  const sec = sectorAt(mx - (b.y - a.y) / len * 4, my + (b.x - a.x) / len * 4);
  S.doc.sidedefs.push(tpl("sidedefs", { sector: sec >= 0 ? sec : f.sector }));
  line.side_back = S.doc.sidedefs.length - 1;
  line.flags = (line.flags | 4) & ~1;
  f.upper = f.upper === "-" ? f.middle : f.upper;
  f.lower = f.lower === "-" ? f.middle : f.lower;
  f.middle = "-";
  commit();
}

function renameMap(name) {
  const n = name.trim().toUpperCase().slice(0, 8);
  if (!n || n === S.doc.name) return;
  delete S.docs[S.doc.name];
  S.doc.name = n;
  commit();
  setSummary(S.mod.roomEditorSummary());
  $("#map-select").value = n;
}

function updateCounts() {
  const d = S.doc;
  $("#st-counts").textContent = d ? `${d.vertices.length} V · ${d.linedefs.length} L · ${d.sidedefs.length} SD · ${d.sectors.length} S · ${d.things.length} T` : "";
}

// ---------------------------------------------------------------- texture picker

let pickerCb = null;
function openPicker(kind, cb) {
  pickerCb = cb;
  const names = kind === "flats" ? S.summary.flats : S.summary.textures;
  const grid = el("div", { class: "thumbs" });
  const notice = el("p", { class: "muted", style: "padding:6px 10px;margin:0" });
  const filter = $("#picker-filter");
  filter.value = "";
  const build = () => {
    const q = filter.value.trim().toUpperCase();
    const filtered = names.filter((n) => !q || n.includes(q));
    const shown = S.summary.duke && kind === "textures" ? filtered.slice(0, 300) : filtered;
    grid.replaceChildren(...shown.map((n) => thumbTile(n, () => { $("#picker").hidden = true; pickerCb?.(n); })));
    notice.hidden = shown.length === filtered.length;
    notice.textContent = `Showing ${shown.length} of ${filtered.length}. Filter by BT tile number to narrow the list.`;
  };
  filter.oninput = build;
  build();
  $("#picker-grid").replaceChildren(notice, grid);
  $("#picker").hidden = false;
  filter.focus();
}
$("#picker-close").addEventListener("click", () => { $("#picker").hidden = true; });

const lazy = new IntersectionObserver((entries) => {
  for (const en of entries) {
    if (!en.isIntersecting) continue;
    const c = en.target;
    lazy.unobserve(c);
    drawThumb(c, c.dataset.name, 72);
  }
}, { rootMargin: "200px" });
function thumbTile(name, onclick, active = false) {
  const c = el("canvas", { "data-name": name, width: 72, height: 72 });
  lazy.observe(c);
  return el("div", { class: `thumb${active ? " active" : ""}`, title: name, onclick }, c, el("div", {}, name));
}

// ---------------------------------------------------------------- resource browser

function renderBrowser() {
  const body = $("#browser-body");
  const q = $("#browser-filter").value.trim().toUpperCase();
  body.replaceChildren();
  S.actorSel = S.tab === "actors" ? S.actorSel : null;
  if (!S.summary) { body.append(el("p", { class: "muted", style: "padding:10px" }, "No game archive open.")); return; }
  const sm = S.summary;
  if (S.tab === "iwad") {
    const det = sm.iwadDetected;
    const title = sm.duke ? sm.duke.grp : det?.name || sm.iwad || "IWAD";
    const subtitle = sm.duke ? "Duke Nukem 3D · Native Build editor" : det
      ? `${det.game} · ${det.autoname || "—"}`
      : "Undetected / custom";
    body.append(
      el("p", { class: "muted", style: "padding:6px 10px;margin:0" }, sm.duke
        ? "Native Duke GRP and Build map details."
        : "Game archive currently loaded. Open the details page for counts, MD5, and maps."),
      el("div", { class: "list-item active", onclick: openIwadPage, title: "Open IWAD details" },
        el("div", {},
          el("div", {}, title),
          el("div", { class: "sub" }, subtitle)),
        el("span", { class: "meta" }, sm.duke ? "Build" : fmtSize(sm.iwadBytes || 0))),
      el("div", { class: "actions", style: "padding:8px 10px" },
        el("button", { class: "btn small primary", onclick: openIwadPage }, "Open details…"),
        el("button", { class: "btn small", onclick: () => { S.tab = "maps"; syncBrowserTab(); renderBrowser(); } }, "Maps")));
    if (sm.duke) {
      body.append(
        el("div", { class: "section-head" }, "Native Build content"),
        el("div", { class: "list-item" }, el("span", {}, "Build maps"), el("span", { class: "meta" }, sm.maps.length)),
        el("div", { class: "list-item" }, el("span", {}, "ART tiles"), el("span", { class: "meta" }, sm.textures.length)));
    } else {
      const kinds = Object.entries(sm.kindCounts || {}).sort((a, b) => b[1] - a[1]).slice(0, 8);
      if (kinds.length) {
        body.append(el("div", { class: "section-head" }, "Lump kinds"));
        for (const [k, n] of kinds) {
          body.append(el("div", { class: "list-item", onclick: () => { S.tab = "lumps"; $("#browser-filter").value = k; syncBrowserTab(); renderBrowser(); } },
            el("span", { class: `kind ${k}` }, k), el("span", { class: "meta" }, n)));
        }
      }
    }
  } else if (S.tab === "maps") {
    body.append(el("div", { class: "section-head" }, "Maps"));
    const names = [...new Set([...sm.maps, ...Object.keys(S.docs)])];
    for (const m of names.filter((m) => !q || displayMapName(m).toUpperCase().includes(q))) {
      const src = sm.lumps.find((l) => l.name === m && l.kind === "map")?.src || "new";
      body.append(el("div", { class: `list-item${S.doc?.name === m ? " active" : ""}`, onclick: () => loadMapOrDoc(m) },
        el("span", { class: "name" }, displayMapName(m)), S.docs[m] ? el("span", { class: "kind map" }, "edited") : null,
        el("span", { class: "meta" }, sm.duke ? "Build .MAP" : src)));
    }
  } else if (S.tab === "things") {
    body.append(el("p", { class: "muted", style: "padding:6px 10px;margin:0" },
      "Click: pick type · Double-click / right-click / icon: edit sprite + sounds"));
    for (const cat of CAT_ORDER) {
      const list = sm.things.filter((t) => t.cat === cat && (!q || t.title.toUpperCase().includes(q) || String(t.num).includes(q)));
      if (!list.length) continue;
      body.append(el("div", { class: "section-head" }, `${cat} (${list.length})`));
      for (const t of list.sort((a, b) => a.title.localeCompare(b.title))) {
        const openSprite = () => {
          if (!t.sprite) { toast(`${t.title} has no sprite to edit`); return; }
          paint.open("sprites", t.sprite, { num: t.num, className: t.name });
        };
        const icon = el("canvas", {
          class: "thing-icon",
          "data-name": t.sprite || "",
          title: t.sprite ? `Edit ${t.sprite} sprite & sounds` : "No sprite",
          style: t.sprite ? "cursor:pointer" : "",
        });
        if (t.sprite) {
          lazy.observe(icon);
          icon.addEventListener("click", (e) => { e.stopPropagation(); openSprite(); });
        }
        const row = el("div", {
          class: `list-item${S.thingType === t.num ? " active" : ""}`,
          title: t.sprite ? `${t.title} — edit sprite & sounds` : t.title,
          onclick: () => {
            S.thingType = t.num;
            if (S.mode === "things" && S.sel.size) { snapshot(); S.sel.forEach((i) => { S.doc.things[i].type_id = t.num; }); commit(); }
            renderBrowser();
          },
        }, icon, el("div", {}, el("div", {}, t.title), el("div", { class: "sub" }, t.name)), el("span", { class: "meta" }, t.num));
        row.addEventListener("dblclick", (e) => { e.preventDefault(); openSprite(); });
        row.addEventListener("contextmenu", (e) => { e.preventDefault(); openSprite(); });
        body.append(row);
      }
    }
  } else if (S.tab === "textures" || S.tab === "flats") {
    const flats = S.tab === "flats";
    const allNames = (flats ? sm.flats : sm.textures).filter((n) => !q || n.includes(q));
    const names = sm.duke && !flats ? allNames.slice(0, 300) : allNames;
    body.append(el("p", { class: "muted", style: "padding:6px 10px;margin:0" },
      sm.duke && !flats ? "Build ART tiles · Right-click or use the tile button in the map inspector to edit pixels · the next Native test uses saved ART edits"
        : flats ? "Click: current flat · Double-click: apply to selected sectors' floor (Shift: ceiling) · Right-click: edit pixels · T / P in 3D paint / edit the picked plane"
        : "Click: current texture · Double-click: apply to selected lines · Right-click: edit pixels · T / P in 3D paint / edit the picked wall"));
    const grid = el("div", { class: "thumbs" });
    for (const n of names) {
      const tile = thumbTile(n, () => {
        if (flats) S.flat = n; else S.texture = n;
        grid.querySelectorAll(".thumb.active").forEach((x) => x.classList.remove("active"));
        tile.classList.add("active");
        if (!S.sel.size) renderInspector();
      }, (flats ? S.flat : S.texture) === n);
      tile.addEventListener("dblclick", (e) => applyPaint(flats, n, e.shiftKey));
      tile.addEventListener("contextmenu", (e) => { e.preventDefault(); paint.open(flats ? "flats" : "textures", n); });
      grid.append(tile);
    }
    body.append(grid);
    if (names.length < allNames.length) {
      body.append(el("p", { class: "muted", style: "padding:6px 10px;margin:0" },
        `Showing ${names.length} of ${allNames.length} ART tiles. Filter by BT tile number to find more.`));
    }
  } else if (S.tab === "actors") {
    if (!S.actors) {
      try { S.actors = JSON.parse(S.mod.roomEditorActors()); } catch (e) { toast(errText(e), true); S.actors = []; }
    }
    body.append(el("p", { class: "muted", style: "padding:6px 10px;margin:0" }, "Edit any actor's values. New spawns use them immediately; Restart level updates placed ones. Exported as DECORATE."));
    const list = S.actors.filter((a) => !q || a.name.toUpperCase().includes(q) || String(a.num ?? "").includes(q));
    for (const a of list) {
      body.append(el("div", { class: `list-item${S.actorSel === a.name ? " active" : ""}`, onclick: () => { S.actorSel = a.name; renderBrowser(); renderInspector(); } },
        el("span", { class: "name" }, a.name), a.edited ? el("span", { class: "kind map" }, "edited") : null,
        el("span", { class: "meta" }, a.num ?? "")));
    }
  } else if (S.tab === "scripts") {
    scripts.renderBrowser(body);
  } else if (S.tab === "lumps") {
    body.append(el("div", { class: "actions", style: "padding:6px 10px;margin:0" },
      el("button", { class: "btn small", onclick: newLump }, "+ New text lump"),
      el("button", { class: "btn small", onclick: newLumpFromFile }, "+ Import file as lump")));
    body.lastChild.append(el("button", { class: "btn small", title: "Add a file by path (zscript/…, actors/…, maps/…) — saved in Export PK3", onclick: newPackFile }, "+ New file in pack…"));
    const row = (l, label) => el("div", { class: "list-item", onclick: () => openLump(l), title: l.path },
      el("span", { class: `kind ${l.kind}` }, l.kind), el("span", { class: "name" }, label),
      isOverride(l) ? el("span", { class: "kind map" }, "override") : null,
      el("span", { class: "meta" }, l.ns === "global" ? fmtSize(l.size) : `${l.ns} · ${fmtSize(l.size)}`));
    // Every loaded ZScript file in one place (roots first), whatever its folder.
    const zsRows = q ? [] : zscriptLumps();
    if (zsRows.length) {
      const det = el("details", { class: "tree-dir" }, el("summary", {}, "ZScript", el("span", { class: "meta" }, String(zsRows.length))));
      const box = el("div", { class: "tree-kids" });
      det.append(box);
      const fill = () => { if (!box.childElementCount) for (const l of zsRows) box.append(row(l, `${l.path || l.name}${l.src !== S.workPk3 ? `  (${l.src})` : ""}`)); };
      if (!S.openDirs.has("zs:") && !S.zsClosedOnce) S.openDirs.add("zs:");
      if (S.openDirs.has("zs:")) { det.open = true; fill(); }
      det.addEventListener("toggle", () => {
        if (det.open) { S.openDirs.add("zs:"); fill(); } else { S.openDirs.delete("zs:"); S.zsClosedOnce = true; }
      });
      body.append(el("div", { class: "section-head" }, "ZScript files"), det);
    }
    // Group by container (load order); PK3 / PK7 / folders show as a tree.
    const groups = [];
    for (const l of sm.lumps) {
      if (q && !l.name.includes(q) && !l.path.toUpperCase().includes(q) && !l.kind.toUpperCase().includes(q)) continue;
      if (!groups.length || groups.at(-1).src !== l.src) groups.push({ src: l.src, items: [] });
      groups.at(-1).items.push(l);
    }
    let shown = 0;
    for (const g of groups) {
      body.append(el("div", { class: "section-head" }, g.src));
      if (!q && g.items.some((l) => l.arch)) { body.append(lumpTree(g.src, g.items, row)); continue; }
      for (const l of g.items) {
        if (++shown > 4000) break;
        body.append(row(l, l.name));
      }
      if (shown > 4000) { body.append(el("p", { class: "muted", style: "padding:8px" }, "…filter to see more")); break; }
    }
  }
}

/** Folder tree of one archive's files; folders render their rows when opened. */
function lumpTree(src, items, row) {
  const root = { dirs: new Map(), files: [] };
  for (const l of items) {
    const parts = (l.path || l.name).split("/");
    let node = root;
    for (const d of parts.slice(0, -1)) {
      if (!node.dirs.has(d)) node.dirs.set(d, { dirs: new Map(), files: [] });
      node = node.dirs.get(d);
    }
    node.files.push(l);
  }
  const count = (n) => n.files.length + [...n.dirs.values()].reduce((a, d) => a + count(d), 0);
  const render = (node, prefix, into) => {
    for (const [name, d] of [...node.dirs].sort((a, b) => a[0].localeCompare(b[0]))) {
      const key = `${src}:${prefix}${name}/`;
      const det = el("details", { class: "tree-dir" }, el("summary", {}, `${name}/`, el("span", { class: "meta" }, String(count(d)))));
      const box = el("div", { class: "tree-kids" });
      det.append(box);
      const fill = () => { if (!box.childElementCount) render(d, `${prefix}${name}/`, box); };
      if (S.openDirs.has(key)) { det.open = true; fill(); }
      det.addEventListener("toggle", () => {
        if (det.open) { S.openDirs.add(key); fill(); } else S.openDirs.delete(key);
      });
      into.append(det);
    }
    for (const l of node.files) into.append(row(l, (l.path || l.name).split("/").pop()));
  };
  const wrap = el("div", { class: "tree" });
  render(root, "", wrap);
  return wrap;
}

const ZS_FILE_TEMPLATE = `// New ZScript file. Include it from your ZSCRIPT lump:
//   #include "PATH"

class MyActor : Actor
{
  Default
  {
    Health 100;
    Radius 20;
    Height 56;
    Speed 8;
    Monster;
    +FLOORCLIP
  }
  States
  {
  Spawn:
    TROO AB 10 A_Look;
    Loop;
  See:
    TROO AABBCCDD 3 A_Chase;
    Loop;
  Death:
    TROO I 8 A_Scream;
    TROO J 8 A_NoBlocking;
    TROO K -1;
    Stop;
  }
}
`;

/** Ask for a path and open an empty editor for a new file in the edited PK3. */
function newPackFile() {
  if (!S.summary) return;
  const path = prompt("New file path in the pack (e.g. zscript/monsters.zs, zscript.txt, actors/imp.dec, mapinfo.txt):", "zscript/myactor.zs");
  if (!path) return;
  const p = path.trim().replace(/\\/g, "/").replace(/^\/+/, "").toLowerCase();
  const base = p.split("/").pop();
  const name = base.replace(/\.[^.]*$/, "").toUpperCase().slice(0, 8);
  const lang = textLang(name, p);
  const text = lang === "zs" ? (name === "ZSCRIPT" ? 'version "4.10"\n\n#include "zscript/myactor.zs"\n' : ZS_FILE_TEMPLATE.replace("PATH", p))
    : lang === "decorate" ? "// actor FastImp : DoomImp replaces DoomImp { Speed 16 Health 90 }\n" : "";
  const fake = { id: null, name, ns: "global", size: 0, src: PATH_OVERRIDES, path: p, kind: "text", arch: true };
  const view = el("div", {}, el("div", { class: "doc-head" }, el("h2", {}, p), el("span", { class: "kind text" }, "new")),
    textEditor(fake, new TextEncoder().encode(text)));
  const id = `newpath:${p}`;
  S.lumpTabs.set(id, { title: base, view });
  selectDocTab(id);
}

function applyPaint(flats, name, ceiling) {
  if (!S.doc || !S.sel.size) return;
  snapshot();
  if (flats && S.mode === "sectors") S.sel.forEach((i) => { S.doc.sectors[i][ceiling ? "ceiling_pic" : "floor_pic"] = name; });
  if (!flats && S.mode === "lines") {
    S.sel.forEach((i) => {
      const l = S.doc.linedefs[i];
      const f = side(l.side_front), b = side(l.side_back);
      if (!b) { if (f) f.middle = name; return; }
      for (const s of [f, b]) { if (s.upper !== "-") s.upper = name; if (s.lower !== "-") s.lower = name; }
    });
  }
  commit();
}

const fmtSize = (n) => (n >= 1048576 ? `${(n / 1048576).toFixed(1)}M` : n >= 1024 ? `${(n / 1024).toFixed(1)}K` : `${n}B`);

function loadMapOrDoc(name) {
  if (S.summary.maps.includes(name)) { loadMap(name); renderBrowser(); return; }
  S.doc = S.docs[name];
  S.undo = []; S.redo = []; S.sel.clear(); S.version++;
  pushNow();
  fitView(); renderInspector(); renderBrowser();
}

// ---------------------------------------------------------------- actors / thing sounds

const ACTOR_SOUND_KEYS = ["see_sound", "attack_sound", "pain_sound", "death_sound", "active_sound"];

function resolveSoundLogical(logical) {
  if (!logical || typeof S.mod.roomEditorResolveSound !== "function") {
    return { lump: null, id: null };
  }
  try {
    return S.mod.roomEditorResolveSound(String(logical)) || { lump: null, id: null };
  } catch {
    return { lump: null, id: null };
  }
}

function actorSoundSlots(actorName) {
  if (!S.actors) {
    try { S.actors = JSON.parse(S.mod.roomEditorActors()); } catch { S.actors = []; }
  }
  const a = S.actors.find((x) => x.name === actorName);
  if (!a) return [];
  const slots = [];
  for (const key of ACTOR_SOUND_KEYS) {
    const logical = a.props?.[key];
    if (logical == null || logical === "") continue;
    const resolved = resolveSoundLogical(logical);
    slots.push({ key, logical: String(logical), lump: resolved.lump || null, id: resolved.id ?? null });
  }
  return slots;
}

async function renderActorInspector(body, title) {
  const sel = S.actorSel;
  const a = S.actors?.find((x) => x.name === sel);
  if (!a) { S.actorSel = null; renderInspector(); return; }
  title.textContent = `Actor ${a.name}`;
  const edited = new Set(a.edited || []);
  body.append(el("div", { class: "actions" },
    el("button", { class: "btn small", onclick: () => { S.actorSel = null; renderInspector(); renderBrowser(); } }, "← Map"),
    el("button", { class: "btn small", onclick: restartLevel }, "Restart level")));
  body.append(el("p", { class: "muted" }, `Parent: ${a.parent || "—"} · DoomEdNum: ${a.num ?? "—"}`));
  const { mountThingSoundsPanel } = await import("./sound-record.js");
  if (S.actorSel !== sel) return;
  const sounds = mountThingSoundsPanel({
    el,
    getSlots: () => actorSoundSlots(a.name),
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
      } catch (e) { toast(errText(e), true); }
    },
    replace: (slot, bytes) => {
      if (!slot.lump) { toast("No lump mapped for that sound", true); return; }
      try {
        const err = overrideLump({ name: slot.lump, ns: "global" }, bytes);
        toast(err ? `Replaced ${slot.lump} — ${err}` : `${slot.lump} replaced`, !!err);
        sounds.refresh();
      } catch (e) { toast(errText(e), true); }
    },
  });
  body.append(sounds.box);
  const fs = el("fieldset", {}, el("legend", {}, "Properties"));
  const flags = el("fieldset", {}, el("legend", {}, "Flags"));
  const fl = el("div", { class: "flags" });
  flags.append(fl);
  for (const [k, v] of Object.entries(a.props)) {
    if (ACTOR_SOUND_KEYS.includes(k)) continue; // shown in Sounds panel above
    const setVal = (val) => {
      try {
        S.mod.roomEditorSetActor(a.name, k, JSON.stringify(val));
        a.props[k] = val ?? a.props[k];
        a.edited = [...new Set([...(a.edited || []), k])];
      } catch (e) { toast(errText(e), true); }
    };
    if (k.startsWith("+")) {
      const cb = el("input", { type: "checkbox", checked: !!v });
      cb.addEventListener("change", () => setVal(cb.checked));
      fl.append(el("label", { class: edited.has(k) ? "edited" : "" }, cb, k.slice(1)));
      continue;
    }
    const input = typeof v === "number"
      ? el("input", { type: "number", value: v, step: "any" })
      : el("input", { type: "text", value: v ?? "" });
    input.addEventListener("change", () => setVal(typeof v === "number" ? Number(input.value) : input.value));
    const reset = el("button", { class: "btn small", title: "Revert", onclick: () => {
      try { S.mod.roomEditorSetActor(a.name, k, "null"); S.actors = null; renderBrowser(); renderInspector(); } catch (e) { toast(errText(e), true); }
    } }, "↺");
    const row = rowEl(k, input, edited.has(k) ? reset : null);
    if (edited.has(k)) row.querySelector("label").classList.add("edited");
    fs.append(row);
  }
  body.append(fs, flags);
}

function restartLevel() {
  try {
    setSummary(S.mod.roomEditorApplyLumps());
    setPlaying(false);
    toast("Level restarted with current definitions");
  } catch (e) { toast(errText(e), true); }
}

// ---------------------------------------------------------------- lump documents

function selectDocTab(id) {
  S.docTab = id;
  $("#doc-view").hidden = id === "map";
  renderDocTabs();
  if (id !== "map") {
    const tab = S.lumpTabs.get(id);
    $("#doc-view").replaceChildren(tab.view);
  } else {
    render2d();
  }
}

function renderDocTabs() {
  const bar = $("#doc-tabs");
  bar.replaceChildren(el("button", { class: S.docTab === "map" ? "active" : "", onclick: () => selectDocTab("map") }, `🗺 ${displayMapName(S.doc?.name || "Map")}`));
  for (const [id, t] of S.lumpTabs) {
    bar.append(el("button", { class: S.docTab === id ? "active" : "", onclick: () => selectDocTab(id) }, t.title,
      el("span", { class: "x", onclick: (e) => { e.stopPropagation(); S.lumpTabs.delete(id); if (S.docTab === id) selectDocTab("map"); else renderDocTabs(); } }, "✕")));
  }
}

function lumpBytes(l) { return l.id == null ? new Uint8Array() : S.mod.roomEditorReadLump(l.id); }

/** Identity of a file across summaries: container + full path (or lump name). */
const lumpKey = (l) => `${l.src}|${String(l.path || l.name).toLowerCase()}`;

/**
 * Keys of every ZScript file loaded (ZSCRIPT roots + their #includes), cached
 * per summary. A file is edited as ZScript when it is in here, whatever its
 * name — `actors/pistol.txt` pulled in by `#include` included.
 */
function zscriptKeys() {
  if (S.zsKeys) return S.zsKeys;
  const lumps = (S.summary?.lumps || []).filter((l) => l.kind !== "map" && l.kind !== "mapwad");
  const rows = zscriptClosure(lumps, (l) => new TextDecoder("utf-8").decode(lumpBytes(l)), (path, from) => findInclude(path, from));
  // Also by path alone, so the edited copy of a PK3 file (held in the
  // overrides pack) stays ZScript.
  S.zsKeys = new Set([...rows].flatMap((l) => [lumpKey(l), `*|${String(l.path || l.name).toLowerCase()}`]));
  return S.zsKeys;
}
const isZscriptLump = (l) => {
  try {
    const k = zscriptKeys();
    return k.has(lumpKey(l)) || (!!l.path && k.has(`*|${String(l.path).toLowerCase()}`));
  } catch (_) { return false; }
};

/** ZScript files of the loaded archives (optionally of one container), roots first. */
function zscriptLumps(src = null) {
  const keys = zscriptKeys();
  const rows = (S.summary?.lumps || []).filter((l) => keys.has(lumpKey(l)) && (!src || l.src === src));
  // The working archive first (its ZSCRIPT root on top), engine built-ins last.
  const rank = (l) => (l.src === S.workPk3 ? 0 : l.src === "room.pk3" ? 2 : 1) * 2
    + (l.name === "ZSCRIPT" && !String(l.path || "").includes("/") ? 0 : 1);
  return rows.sort((a, b) => rank(a) - rank(b) || String(a.path || a.name).localeCompare(String(b.path || b.name)));
}

/** Open the ZSCRIPT root of `src` (the archive just loaded) in the text editor. */
function openZscriptOf(src) {
  const rows = zscriptLumps(src);
  if (!rows.length) return 0;
  openLump(rows[0]);
  return rows.length;
}

/** A ZScript file picked on its own (.zs/.zc/.zsc): add it to the pack under zscript/ and open it. */
function openLooseZscript(name, bytes) {
  if (!S.summary) { toast("Open an IWAD or PK3 first, then add the ZScript file", true); return; }
  const base = String(name).split(/[\\/]/).pop().toLowerCase();
  const p = /^zscript\./.test(base) ? base : `zscript/${base}`;
  try {
    S.mod.roomEditorSetPath(p, bytes);
    setSummary(S.mod.roomEditorApplyLumps());
    setPlaying(false);
  } catch (e) { toast(errText(e), true); return; }
  const row = S.summary.lumps.find((l) => l.src === PATH_OVERRIDES && String(l.path).toLowerCase() === p);
  if (row) openLump(row);
  // Room loads every .zs/.zc/.zsc file; GZDoom only what a ZSCRIPT root #includes.
  const included = p === base || zscriptLumps().some((l) => l !== row && includesOf(new TextDecoder().decode(lumpBytes(l)))
    .some((x) => x.path.replace(/\\/g, "/").replace(/^\.?\//, "").toLowerCase() === p));
  toast(included ? `${p} added and loaded`
    : `${p} added and loaded — for GZDoom, also add #include "${p}" to your ZSCRIPT lump`);
}

function download(bytes, name) {
  const url = URL.createObjectURL(new Blob([bytes]));
  const a = el("a", { href: url, download: name });
  document.body.append(a); a.click(); a.remove();
  setTimeout(() => URL.revokeObjectURL(url), 2000);
}

function pickFile(accept) {
  return new Promise((res) => {
    const inp = el("input", { type: "file", accept });
    inp.addEventListener("change", async () => { const f = inp.files[0]; res(f ? new Uint8Array(await f.arrayBuffer()) : null); });
    inp.click();
  });
}

const overrideNs = (l) => (["flats", "sprites", "patches", "textures"].includes(l.ns) ? l.ns : "global");
const IMAGE_KINDS = ["flat", "patch", "sprite", "graphic", "texture"];

const PATH_OVERRIDES = "editor-overrides.pk3";
const isOverride = (l) => l.src === "editor-overrides.wad" || l.src === PATH_OVERRIDES;

/** Replace a lump (`bytes`) or drop its override (`null`): files inside a PK3 by full path, WAD lumps by name. */
function setLumpData(l, bytes) {
  if (l.arch && l.path) S.mod.roomEditorSetPath(l.path, bytes);
  else S.mod.roomEditorSetLump(overrideNs(l), l.name, bytes);
}

function overrideLump(l, bytes) {
  setLumpData(l, bytes);
  const json = S.mod.roomEditorApplyLumps();
  thumbs.clear();
  setSummary(json);
  setPlaying(false);
  return S.summary.decorateError;
}

function openLump(l) {
  // A PK3 file edited in this session: open the edited copy.
  if (l.arch && l.path && !isOverride(l)) {
    const p = l.path.toLowerCase();
    const edited = S.summary?.lumps.find((x) => x.src === PATH_OVERRIDES && x.path.toLowerCase() === p);
    if (edited) l = edited;
  }
  const id = `lump:${l.src}:${l.name}:${l.id}`;
  if (S.lumpTabs.has(id)) { selectDocTab(id); return; }
  if (l.kind === "map") { loadMap(l.name); return; }
  if (l.kind === "mapwad") {
    // maps/*.wad inside a PK3 is mounted as "<pk3>:<path>"; open its map.
    const m = S.summary.lumps.find((x) => x.kind === "map" && x.src === `${l.src}:${l.path}`);
    if (m) { loadMap(m.name); return; }
  }
  // Files in PK3 subfolders the VFS does not map to a namespace have no
  // 8-char name; show their file name instead of an empty title.
  const title = l.name || String(l.path || "").split("/").pop() || "(file)";
  const view = el("div", {});
  const head = el("div", { class: "doc-head" }, el("h2", {}, title),
    el("span", { class: `kind ${l.kind}` }, l.kind), el("span", { class: "muted" }, `${l.ns} · ${fmtSize(l.size)} · ${l.src}${l.path && l.path !== l.name ? ` · ${l.path}` : ""}`));
  const actions = el("div", { class: "actions" },
    el("button", { class: "btn small", onclick: () => download(lumpBytes(l), l.path?.split("/").pop() || `${l.name}.lmp`) }, "Download"),
    el("button", { class: "btn small", onclick: async () => {
      const b = await pickFile("*/*");
      if (!b) return;
      try { const err = overrideLump(l, b); toast(err ? `Replaced — DECORATE: ${err}` : `${l.name} replaced and reloaded`, !!err); } catch (e) { toast(errText(e), true); }
    } }, "Replace…"),
    el("button", { class: "btn small", onclick: () => {
      try { setLumpData(l, null); setSummary(S.mod.roomEditorApplyLumps()); thumbs.clear(); toast(`${l.name} override removed`); } catch (e) { toast(errText(e), true); }
    } }, "Revert override"));
  view.append(head, actions);
  const bytes = lumpBytes(l);
  const textish = l.kind === "text" || (l.kind === "data" && looksText(bytes));
  const picture = !textish && (IMAGE_KINDS.includes(l.kind) || (l.kind === "data" && S.mod.roomEditorPreviewLump(l.id)));
  if (picture) actions.append(el("button", { class: "btn small go", onclick: () => paint.open(paintNs(l), l.name) }, "✎ Edit pixels"));
  if (textish) view.append(textEditor(l, bytes));
  else if (picture) view.append(imageView(l));
  else if (l.kind === "sound") view.append(soundView(l));
  else if (l.kind === "music") view.append(el("div", { class: "actions" },
    el("button", { class: "btn go", onclick: () => S.mod.roomEditorPlayMusic(l.name) }, "▶ Play in engine"),
    el("button", { class: "btn", onclick: () => S.mod.roomEditorPlayMusic("") }, "■ Stop")), hexView(bytes));
  else if (l.kind === "palette") view.append(paletteView(l.name, bytes));
  else if (l.kind === "texdefs") view.append(el("p", { class: "muted" }, "Composite textures defined here appear in the Textures tab. Add new ones with a TEXTURES text lump (+ New text lump)."), hexView(bytes));
  else view.append(hexView(bytes));
  S.lumpTabs.set(id, { title, view });
  selectDocTab(id);
}

function looksText(b) {
  const n = Math.min(b.length, 2048);
  if (!n) return false;
  let bad = 0;
  for (let i = 0; i < n; i++) { const c = b[i]; if (c < 9 || (c > 13 && c < 32 && c !== 27)) bad++; }
  return bad / n < 0.02;
}

const DOOM_LANG_LABEL = { zs: "ZScript", decorate: "DECORATE", acs: "ACS", mapinfo: "MAPINFO", text: "text" };
const escHtml = (s) => String(s).replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");

/** Lump for an `#include "path"`: same full path (last in load order wins, like the engine), else the 8-char stem. */
function findInclude(path, from) {
  const lumps = S.summary?.lumps || [];
  const norm = (p) => p.replace(/\\/g, "/").replace(/^\.?\//, "").toLowerCase();
  const p = norm(path);
  const dir = (from?.path || "").includes("/") ? from.path.slice(0, from.path.lastIndexOf("/") + 1).toLowerCase() : "";
  let hit = null;
  for (const l of lumps) if (l.kind !== "map" && norm(l.path || l.name) === p) hit = l;
  if (!hit && dir) for (const l of lumps) if (l.kind !== "map" && norm(l.path || l.name) === dir + p) hit = l;
  if (!hit) {
    const stem = p.split("/").pop().replace(/\.[^.]*$/, "").toUpperCase().slice(0, 8);
    for (const l of lumps) if (l.kind !== "map" && l.name === stem) hit = l;
  }
  return hit;
}

function textEditor(l, bytes) {
  const wrap = el("div", {});
  const text0 = new TextDecoder("utf-8").decode(bytes);
  const lang = isZscriptLump(l) ? "zs" : textLang(l.name, l.path || l.name, text0);
  const zscript = lang === "zs";
  const ta = el("textarea", { class: "code hl-input", spellcheck: "false" });
  ta.value = text0;
  const gutter = el("div", { class: "gutter" });
  const hl = el("pre", { class: "code-hl", "aria-hidden": "true" });
  const incBar = el("div", { class: "inc-bar", hidden: true });
  const errBox = el("div", { class: "err-box", hidden: true });
  let marks = new Map(); // line → message (exact diagnostics of this file)
  const renderGutter = () => {
    const n = ta.value.split("\n").length;
    const rows = [];
    for (let i = 1; i <= n; i++) {
      const m = marks.get(i);
      rows.push(m ? `<span class="gl-err" title="${escHtml(m)}">${i}</span>` : String(i));
    }
    gutter.innerHTML = rows.join("\n");
  };
  const openInclude = (path) => {
    const hit = findInclude(path, l);
    if (hit) openLump(hit); else toast(`#include "${path}" not found in the loaded files`, true);
  };
  let incKey = "";
  const renderIncludes = () => {
    if (lang === "mapinfo" || lang === "text") return;
    const inc = includesOf(ta.value);
    const key = inc.map((x) => x.path).join("\n");
    if (key === incKey) return;
    incKey = key;
    incBar.replaceChildren();
    incBar.hidden = !inc.length;
    if (!inc.length) return;
    incBar.append(el("span", { class: "muted" }, "#include:"));
    for (const x of inc) {
      const found = !!findInclude(x.path, l);
      incBar.append(el("button", { class: `btn small${found ? "" : " missing"}`, title: found ? `Open ${x.path} (or Ctrl/⌘+click the line)` : "Not found in the loaded files", onclick: () => openInclude(x.path) }, x.path));
    }
  };
  const syncScroll = () => { gutter.scrollTop = ta.scrollTop; hl.scrollTop = ta.scrollTop; hl.scrollLeft = ta.scrollLeft; };
  const lines = () => { renderGutter(); hl.innerHTML = highlight(ta.value, lang); renderIncludes(); syncScroll(); };
  const goToLine = (n, col = 1) => {
    const rows = ta.value.split("\n");
    const k = Math.max(1, Math.min(n, rows.length));
    let start = 0;
    for (let i = 0; i < k - 1; i++) start += rows[i].length + 1;
    ta.focus();
    ta.setSelectionRange(start + Math.max(0, col - 1), start + rows[k - 1].length);
    ta.scrollTop = Math.max(0, (k - 6) * 18);
    syncScroll();
  };
  const showDiags = (r, decorateErr) => {
    errBox.replaceChildren();
    marks = new Map();
    const me = (l.path || l.name || "").toLowerCase();
    if (decorateErr) errBox.append(el("div", {}, `DECORATE error (engine definitions kept): ${decorateErr}`));
    if (r) {
      const errs = r.diagnostics.filter((d) => d.severity === "error").length;
      const where = r.scope === "file" ? "this file" : `${r.files} ZScript file(s)`;
      errBox.append(el("div", { class: r.ok ? "diag-ok" : "" }, r.ok ? `ZScript OK — ${where} compiled${r.diagnostics.length ? `, ${r.diagnostics.length} warning(s)` : ""}` : `ZScript: ${errs} error(s) in ${where}`));
      for (const d of r.diagnostics) {
        const exact = d.exact && (!d.path || d.path.toLowerCase() === me || r.scope === "file");
        if (exact && d.severity === "error" && !marks.has(d.line)) marks.set(d.line, d.message);
        errBox.append(el("div", { class: `diag ${d.severity}${exact ? " jump" : ""}`, title: exact ? "Go to line" : "",
          onclick: exact ? () => goToLine(d.line, d.col) : null }, exact ? `${d.line}:${d.col}  ${d.severity}: ${d.message}` : d.text));
      }
    }
    errBox.hidden = !errBox.childElementCount;
    renderGutter();
  };
  const check = () => {
    try {
      const r = JSON.parse(S.mod.roomEditorCheckZscript(l.path || l.name, ta.value));
      showDiags(r, null);
      return r;
    } catch (e) { errBox.hidden = false; errBox.textContent = errText(e); return null; }
  };
  lines();
  ta.addEventListener("input", lines);
  ta.addEventListener("scroll", syncScroll);
  ta.addEventListener("keydown", (e) => {
    if (e.key === "Tab") { e.preventDefault(); const s = ta.selectionStart; ta.setRangeText("    ", s, ta.selectionEnd, "end"); lines(); }
    if ((e.ctrlKey || e.metaKey) && e.key === "s") { e.preventDefault(); save(); }
  });
  // Ctrl/⌘+click on an #include line opens the included file.
  ta.addEventListener("click", (e) => {
    if (!(e.ctrlKey || e.metaKey)) return;
    const line = ta.value.slice(0, ta.selectionStart).split("\n").length;
    const inc = includesOf(ta.value).find((x) => x.line === line);
    if (inc) { e.preventDefault(); openInclude(inc.path); }
  });
  const save = () => {
    try {
      const err = overrideLump(l, new TextEncoder().encode(ta.value));
      const r = zscript ? check() : null;
      showDiags(r, err);
      const bad = !!err || (r && !r.ok);
      toast(bad ? "Applied with errors" : `${l.name} applied — level reloaded`, bad);
    } catch (e) { errBox.hidden = false; errBox.textContent = errText(e); }
  };
  const where = l.arch && l.path ? ` Saved by path into the PK3 (${l.path}); Export PK3 writes it back.` : "";
  const note = zscript
    ? `ZScript — syntax colors as you type, Check finds errors (click one to jump), Ctrl/⌘+click an #include to open it.${where}`
    : `${DOOM_LANG_LABEL[lang]} — the level reloads with your changes.${where}`;
  const actions = el("div", { class: "actions" }, el("button", { class: "btn primary", onclick: save }, "Apply & reload (Ctrl+S)"));
  if (zscript) actions.append(el("button", { class: "btn", title: "Parse this file, then compile the whole ZScript program with it", onclick: check }, "Check ZScript"));
  actions.append(el("span", { class: `kind text` }, DOOM_LANG_LABEL[lang]), el("span", { class: "muted" }, note));
  wrap.append(actions, incBar, el("div", { class: "code-wrap" }, gutter, el("div", { class: "code-area" }, hl, ta)), errBox);
  return wrap;
}

function imageView(l) {
  const img = S.mod.roomEditorPreviewLump(l.id);
  if (!img) return el("p", { class: "muted" }, "Not a decodable picture.");
  const c = el("canvas", { class: "preview-canvas", width: img.w, height: img.h });
  c.getContext("2d").putImageData(new ImageData(img.rgba, img.w, img.h), 0, 0);
  const zoom = el("input", { type: "range", min: 1, max: 12, value: Math.max(1, Math.min(8, Math.floor(512 / Math.max(img.w, img.h)))) });
  const setZoom = () => { c.style.width = `${img.w * zoom.value}px`; c.style.height = `${img.h * zoom.value}px`; };
  zoom.addEventListener("input", setZoom);
  setZoom();
  return el("div", {}, el("div", { class: "actions" }, el("span", { class: "muted" }, `${img.w}×${img.h} · zoom`), zoom,
    el("span", { class: "muted" }, "Replace accepts Doom patches/flats or PNG.")), c);
}

let audioCtx = null;
function soundView(l) {
  let snd = null;
  try { snd = S.mod.roomEditorDecodeSound(l.id); } catch (e) { return el("p", { class: "muted" }, `Cannot decode: ${errText(e)}`); }
  const c = el("canvas", { class: "preview-canvas", width: 900, height: 160, style: "width:100%;max-width:900px;height:160px" });
  const g = c.getContext("2d");
  g.fillStyle = "#0f1114"; g.fillRect(0, 0, 900, 160);
  g.strokeStyle = "#7cc3ff"; g.beginPath();
  const s = snd.samples, ch = snd.channels || 1, frames = Math.floor(s.length / ch);
  for (let x = 0; x < 900; x++) {
    const i0 = Math.floor((x / 900) * frames), i1 = Math.max(i0 + 1, Math.floor(((x + 1) / 900) * frames));
    let mn = 1, mx = -1;
    for (let i = i0; i < i1; i++) { const v = s[i * ch]; mn = Math.min(mn, v); mx = Math.max(mx, v); }
    g.moveTo(x + 0.5, 80 - mx * 76); g.lineTo(x + 0.5, 80 - mn * 76);
  }
  g.stroke();
  const play = () => {
    audioCtx = audioCtx || new AudioContext();
    const buf = audioCtx.createBuffer(ch, frames, snd.rate);
    for (let k = 0; k < ch; k++) { const d = buf.getChannelData(k); for (let i = 0; i < frames; i++) d[i] = s[i * ch + k]; }
    const src = audioCtx.createBufferSource(); src.buffer = buf; src.connect(audioCtx.destination); src.start();
  };
  const record = el("button", {
    class: "btn go",
    onclick: async () => {
      const { openSoundRecord } = await import("./sound-record.js");
      openSoundRecord({
        title: `Record · ${l.name}`,
        onDone: (bytes) => {
          try {
            const err = overrideLump(l, bytes);
            toast(err ? `Replaced — DECORATE: ${err}` : `${l.name} replaced with recording`, !!err);
            openLump(l);
          } catch (e) { toast(errText(e), true); }
        },
      });
    },
  }, "🎤 Record…");
  return el("div", {}, el("div", { class: "actions" }, el("button", { class: "btn go", onclick: play }, "▶ Play"), record,
    el("span", { class: "muted" }, `${snd.rate} Hz · ${ch} ch · ${(frames / snd.rate).toFixed(2)} s · Replace accepts WAV or DMX`)), c);
}

function paletteView(name, b) {
  const wrap = el("div", {});
  if (name === "PLAYPAL") {
    const n = Math.floor(b.length / 768);
    for (let p = 0; p < n; p++) {
      const c = el("canvas", { width: 16, height: 16, class: "preview-canvas", style: "width:128px;height:128px;margin:4px" });
      const img = new ImageData(16, 16);
      for (let i = 0; i < 256; i++) { img.data.set([b[p * 768 + i * 3], b[p * 768 + i * 3 + 1], b[p * 768 + i * 3 + 2], 255], i * 4); }
      c.getContext("2d").putImageData(img, 0, 0);
      wrap.append(c);
    }
  } else wrap.append(hexView(b));
  return wrap;
}

function hexView(b) {
  const n = Math.min(b.length, 65536);
  const out = [];
  for (let o = 0; o < n; o += 16) {
    const row = b.slice(o, Math.min(o + 16, n));
    const hex = [...row].map((x) => x.toString(16).padStart(2, "0")).join(" ");
    const asc = [...row].map((x) => (x >= 32 && x < 127 ? String.fromCharCode(x) : ".")).join("");
    out.push(`${o.toString(16).padStart(8, "0")}  ${hex.padEnd(48)}  ${asc}`);
  }
  if (b.length > n) out.push(`… ${b.length - n} more bytes`);
  return el("pre", { class: "hex" }, out.join("\n"));
}

function newLump() {
  const name = prompt("New lump name (e.g. DECORATE, MAPINFO, SNDINFO, TEXTURES):", "DECORATE");
  if (!name) return;
  const up = name.trim().toUpperCase().slice(0, 8);
  const templates = {
    DECORATE: "// Add or replace actors. Example:\n// actor FastImp : DoomImp replaces DoomImp { Speed 16 Health 90 }\n",
    MAPINFO: "// map E1M1 \"My level\" { next = \"E1M2\" sky1 = \"SKY1\" music = \"D_E1M1\" }\n",
    TEXTURES: "// texture MYWALL, 64, 128 { patch WALL00_3, 0, 0 }\n",
  };
  const fake = { id: null, name: up, ns: "global", size: 0, src: "editor-overrides.wad", path: up, kind: "text" };
  const view = el("div", {}, el("div", { class: "doc-head" }, el("h2", {}, up), el("span", { class: "kind text" }, "new")),
    textEditor(fake, new TextEncoder().encode(templates[up] || "")));
  const id = `new:${up}`;
  S.lumpTabs.set(id, { title: up, view });
  selectDocTab(id);
}

async function newLumpFromFile() {
  const name = prompt("Lump name (≤ 8 chars):");
  if (!name) return;
  const ns = prompt("Namespace: global, flats, sprites or patches", "global") || "global";
  const b = await pickFile("*/*");
  if (!b) return;
  try {
    S.mod.roomEditorSetLump(ns, name, b);
    thumbs.clear();
    setSummary(S.mod.roomEditorApplyLumps());
    toast(`${name.toUpperCase()} added`);
  } catch (e) { toast(errText(e), true); }
}

// ---------------------------------------------------------------- export / new map

function exportWad() {
  if (!S.summary) return;
  if (S.summary.duke) {
    try {
      const bytes = S.mod.roomEditorDukeExport();
      const base = String(S.summary.duke.grp || "duke3d").replace(/\.[^.]+$/, "");
      download(bytes, `${base}-room-edit.grp`);
      toast(`Exported native GRP with ${S.dukeMapFile || "Build maps"} and ART tile edits`);
    } catch (e) { toast(`Export GRP: ${errText(e)}`, true); }
    return;
  }
  if (S.doc) S.docs[S.doc.name] = S.doc;
  try {
    const bytes = S.mod.roomEditorExport(JSON.stringify(S.docs));
    const base = (S.summary.pwads.at(-1) || S.summary.iwad).replace(/\.(wad|pk3|zip)$/i, "");
    download(bytes, `${base}-room-edit.wad`);
    const paths = S.summary.overrides.filter((o) => o.ns === "path").length;
    toast(`Exported ${Object.keys(S.docs).length} map(s) + ${S.summary.overrides.length - paths} lump(s)${paths ? ` — ${paths} PK3 file edit(s) need Export PK3` : ""}`, paths > 0);
  } catch (e) { toast(`Export: ${errText(e)}`, true); }
}

/** Export a PK3: the opened PK3 (or a new one) with every edit applied. */
function exportPk3() {
  if (!S.summary) return;
  if (S.doc) S.docs[S.doc.name] = S.doc;
  const archives = new Set((S.summary.containers || []).filter((c) => ["pk3", "pk7", "dir"].includes(c.kind)).map((c) => c.name));
  const opened = [S.summary.iwad, ...S.summary.pwads].filter((n) => archives.has(n));
  // The imported (working) archive by default, else the last opened pack.
  let target = opened.includes(S.workPk3) ? S.workPk3 : opened.at(-1) || "";
  if (opened.length > 1) {
    const pick = prompt(`Rebuild which pack? (${opened.join(", ")})\nLeave empty for a new PK3 with only your changes.`, target);
    if (pick === null) return;
    target = pick.trim();
  }
  try {
    const bytes = S.mod.roomEditorExportPk3(JSON.stringify(S.docs), target);
    const base = (target || S.summary.pwads.at(-1) || S.summary.iwad).replace(/^.*[\/]/, "").replace(/\.(wad|pk3|pk7|zip|ipk3)$/i, "");
    download(bytes, `${base}-room-edit.pk3`);
    toast(`Exported ${target ? `${target} + ` : ""}${Object.keys(S.docs).length} map(s) + ${S.summary.overrides.length} edit(s) as PK3`);
  } catch (e) { toast(`Export PK3: ${errText(e)}`, true); }
}

function newMap() {
  if (!S.summary) return;
  if (S.summary.duke) {
    let suffix = 1;
    let stem = `ROOM${String(suffix).padStart(2, "0")}`;
    while (S.summary.maps.includes(stem)) {
      suffix++;
      stem = `ROOM${String(suffix).padStart(2, "0")}`;
    }
    const requested = prompt("Native Build .MAP name (1–8 letters, digits, or underscores):", stem);
    if (!requested) return;
    const mapName = requested.trim().replace(/\.map$/i, "").toUpperCase();
    const editorName = /^E([0-9])L([0-9]+)$/.test(mapName)
      ? mapName.replace(/^E([0-9])L([0-9]+)$/, "E$1M$2")
      : mapName;
    try {
      setSummary(S.mod.roomEditorDukeMapCreate(mapName));
      S.mode = "sectors";
      S.drawPts = [];
      syncModeButtons();
      loadMap(editorName);
      toast(`${mapName}.MAP created in the working Duke GRP`);
    } catch (e) { toast(errText(e), true); }
    return;
  }
  const name = prompt("Map name:", S.summary.maps.includes("MAP01") ? "MAP33" : "E1M9");
  if (!name) return;
  const fmt = (prompt("Format: doom, hexen or udmf", isUdmf() ? "udmf" : "doom") || "doom").toLowerCase();
  try {
    S.doc = JSON.parse(S.mod.roomEditorNewMap(name, fmt));
    S.docs[S.doc.name] = S.doc;
    S.undo = []; S.redo = []; S.sel.clear(); S.version++;
    setSummary(S.mod.roomEditorSummary());
    $("#map-select").value = S.doc.name;
    fitView(); renderInspector();
    toast(`${S.doc.name} created — draw sectors with D`);
  } catch (e) { toast(errText(e), true); }
}

// ---------------------------------------------------------------- toolbar & keys

function syncModeButtons() {
  document.querySelectorAll("#modes .btn").forEach((b) => b.classList.toggle("active", b.dataset.mode === S.mode));
  setHint();
}
function setMode(m) {
  if (S.mode === m) return;
  if (S.mode === "draw" && S.drawPts.length) S.drawPts = [];
  S.mode = m;
  S.sel.clear();
  S.pick3d = null;
  S.actorSel = null;
  syncModeButtons();
  renderInspector();
  render2d();
}

document.querySelectorAll("#modes .btn").forEach((b) => b.addEventListener("click", () => setMode(b.dataset.mode)));
document.querySelectorAll(".layout .btn[data-layout]").forEach((b) => b.addEventListener("click", () => {
  if (S.visual) setVisualMode(false);
  applyLayout(b.dataset.layout);
}));
$("#btn-visual")?.addEventListener("click", () => setVisualMode(!S.visual));
$("#btn-add-pack")?.addEventListener("click", () => $("#file-pack")?.click());
$("#file-pk3")?.addEventListener("change", (e) => {
  const files = [...e.target.files];
  e.target.value = "";
  if (files.length) withEditorLoading("Loading content pack…", files.map((f) => f.name).join(", "), () => importPk3Files(files));
});
const GAME_ARCHIVE_ACCEPT = ".wad,.iwad,.grp,.pk3,.pk7,.zip,.ipk3,.ipk7,application/octet-stream,application/zip";
function chooseStartArchive(game) {
  const input = $("#file-iwad");
  input.accept = game === "duke" ? ".grp,application/octet-stream" : ".wad,.iwad,application/octet-stream";
  input.dataset.startPicker = "1";
  input.click();
}
$("#start-open-doom").addEventListener("click", () => chooseStartArchive("doom"));
$("#start-open-duke").addEventListener("click", () => chooseStartArchive("duke"));
$("#start-open-pk3").addEventListener("click", () => $("#file-pk3").click());
$("#file-pack")?.addEventListener("change", (e) => {
  const files = [...e.target.files];
  e.target.value = "";
  if (files.length) withEditorLoading("Loading content pack…", files.map((f) => f.name).join(", "), () => addPackFiles(files));
});
document.querySelectorAll("#browser-tabs button").forEach((b) => b.addEventListener("click", () => {
  S.tab = b.dataset.tab;
  syncBrowserTab();
  if (S.tab !== "actors" && S.actorSel) { S.actorSel = null; renderInspector(); }
  renderBrowser();
  if (S.tab === "iwad") openIwadPage();
}));
let filterTimer = 0;
$("#browser-filter").addEventListener("input", () => { clearTimeout(filterTimer); filterTimer = setTimeout(renderBrowser, 120); });
$("#file-iwad").addEventListener("change", (e) => {
  const f = e.target.files && e.target.files[0];
  e.target.value = "";
  e.target.accept = GAME_ARCHIVE_ACCEPT;
  if (f) withEditorLoading("Loading game archive…", f.name, () => openIwadFile(f));
});
$("#file-iwad").addEventListener("click", (e) => {
  if (e.currentTarget.dataset.startPicker) {
    delete e.currentTarget.dataset.startPicker;
    return;
  }
  e.currentTarget.accept = GAME_ARCHIVE_ACCEPT;
});
$("#file-open").addEventListener("change", (e) => {
  const files = [...e.target.files];
  e.target.value = "";
  if (files.length) withEditorLoading("Opening selected files…", files.map((f) => f.name).join(", "), () => openFiles(files));
});
$("#map-select").addEventListener("change", (e) => { if (e.target.value) loadMapOrDoc(e.target.value); });
$("#btn-new-map").addEventListener("click", newMap);
$("#btn-export").addEventListener("click", exportWad);
$("#btn-export-pk3").addEventListener("click", exportPk3);
$("#btn-undo").addEventListener("click", undo);
$("#btn-redo").addEventListener("click", redo);
$("#btn-play").addEventListener("click", () => setPlaying(true, "start"));
$("#btn-play-here").addEventListener("click", () => setPlaying(true, "here"));
$("#btn-stop").addEventListener("click", () => setPlaying(false));
function applyMute(muted) {
  try { localStorage.setItem("room/editor-muted", muted ? "1" : "0"); } catch (_) { /* private mode */ }
  if (S.mod) S.mod.roomSetMuted(muted);
  const b = $("#btn-mute");
  b.textContent = muted ? "🔇" : "🔊";
  b.title = muted ? "Unmute game sound and music" : "Mute game sound and music";
  b.classList.toggle("active", muted);
}
$("#btn-mute").addEventListener("click", () => applyMute(!(S.mod && S.mod.roomMuted())));
$("#grid").addEventListener("change", (e) => { S.grid = Number(e.target.value); render2d(); });
$("#snap").addEventListener("change", (e) => { S.snap = e.target.checked; });
$("#textured").addEventListener("change", (e) => { S.textured = e.target.checked; render2d(); });
$("#duke-floors").addEventListener("change", (e) => { S.dukeFloors = e.target.checked; render2d(); });
$("#skill").addEventListener("change", (e) => { try { S.mod.roomEditorSetSkill(Number(e.target.value)); } catch (_) {} });

const splitter = $("#splitter");
splitter.addEventListener("mousedown", (e) => {
  e.preventDefault();
  const mv = $("#map-view");
  const r = mv.getBoundingClientRect();
  const move = (ev) => {
    const f = Math.max(0.15, Math.min(0.85, (ev.clientX - r.left) / r.width));
    $("#view2d-wrap").style.flex = `0 0 ${f * 100}%`;
    $("#view3d-wrap").style.flex = "1 1 0";
    render2d();
  };
  const up = () => { window.removeEventListener("mousemove", move); window.removeEventListener("mouseup", up); };
  window.addEventListener("mousemove", move);
  window.addEventListener("mouseup", up);
});

const keys = new Set();
window.addEventListener("keyup", (e) => keys.delete(e.code));
window.addEventListener("keydown", (e) => {
  keys.add(e.code);
  const t = e.target;
  const typing = t instanceof HTMLInputElement || t instanceof HTMLTextAreaElement || t instanceof HTMLSelectElement;
  const mod = e.ctrlKey || e.metaKey;
  if (mod && e.code === "KeyS") { e.preventDefault(); if (!typing || t.tagName !== "TEXTAREA") exportWad(); return; }
  if (e.code === "F5") { e.preventDefault(); setPlaying(true, "start"); return; }
  if (e.code === "F6") { e.preventDefault(); setPlaying(true, "here"); return; }
  if (e.code === "F7") { e.preventDefault(); setPlaying(false); return; }
  if (typing) return;
  if (mod && e.code === "KeyZ") { e.preventDefault(); if (e.shiftKey) redo(); else undo(); return; }
  if (mod && e.code === "KeyY") { e.preventDefault(); redo(); return; }
  // Visual mode (M) works from 2D or 3D focus — same idea as Doom Builder W / GZDB visual.
  if (!mod && e.code === "KeyM" && S.doc) {
    e.preventDefault();
    setVisualMode(!S.visual);
    return;
  }
  if (!S.doc) return;
  if (S.dukeMap) {
    if (!mod && (e.code === "Enter" || e.code === "NumpadEnter") && S.mode === "draw") {
      e.preventDefault(); finishDukeDraw(); return;
    }
    if (!mod && (e.code === "Delete" || e.code === "Backspace") && S.dukeSelection?.kind === "sector") {
      deleteDukeSector(S.dukeSelection.index); e.preventDefault(); return;
    }
    if (!mod && e.code === "Insert" && mouse.world) { addDukeSprite(Math.round(mouse.world[0]), Math.round(mouse.world[1])); e.preventDefault(); return; }
    if (!mod && (e.code === "Delete" || e.code === "Backspace") && S.dukeSelection?.kind === "sprite") {
      deleteDukeSprite(S.dukeSelection.index); e.preventDefault(); return;
    }
    if (!mod && e.code === "Escape") {
      if (S.drawPts.length) { S.drawPts = []; render2d(); }
      else { S.dukeSelection = null; renderInspector(); render2d(); }
      return;
    }
    if (!mod && e.code === "Home") { fitDukeView(); return; }
  }
  if (tools.onKey(e, { mod, in3d: document.activeElement === room })) return;
  if (document.activeElement === room) {
    if (!S.playing && key3d(e)) e.preventDefault();
    if (!S.playing && (e.code === "Delete" || e.code === "Backspace") && S.mode === "things") { deleteSelection(); e.preventDefault(); }
    return;
  }
  if (mod && e.code === "KeyA") {
    e.preventDefault();
    const n = { vertices: S.doc.vertices, lines: S.doc.linedefs, sectors: S.doc.sectors, things: S.doc.things }[S.mode]?.length || 0;
    S.sel = new Set(Array.from({ length: n }, (_, i) => i));
    renderInspector(); render2d();
    return;
  }
  const modeKeys = { KeyV: "vertices", KeyL: "lines", KeyS: "sectors", KeyT: "things", KeyD: "draw" };
  if (!mod && modeKeys[e.code]) { setMode(modeKeys[e.code]); return; }
  switch (e.code) {
    case "Delete": case "Backspace": deleteSelection(); e.preventDefault(); break;
    case "Escape":
      if (S.drawPts.length) { S.drawPts = []; render2d(); } else { S.sel.clear(); S.actorSel = null; renderInspector(); render2d(); }
      break;
    case "Enter": case "NumpadEnter": if (S.mode === "draw") finishDraw(); break;
    case "Insert": if (mouse.world) insertThing(mouse.world[0], mouse.world[1]); break;
    case "KeyX": splitLines(); break;
    case "KeyF": flipLines(); break;
    case "KeyG": S.snap = !S.snap; $("#snap").checked = S.snap; break;
    case "Home": fitView(); break;
    case "BracketLeft": case "BracketRight": {
      const opts = [4, 8, 16, 32, 64, 128, 256];
      const i = Math.max(0, Math.min(opts.length - 1, opts.indexOf(S.grid) + (e.code === "BracketRight" ? 1 : -1)));
      S.grid = opts[i]; $("#grid").value = String(S.grid); render2d();
      break;
    }
    case "PageUp": case "PageDown":
      if (adjustSectors(e.shiftKey ? "ceiling_height" : "floor_height", e.code === "PageUp" ? 8 : -8)) e.preventDefault();
      break;
    case "Equal": case "NumpadAdd": adjustSectors("light", 16); break;
    case "Minus": case "NumpadSubtract": adjustSectors("light", -16); break;
    default: break;
  }
});

window.addEventListener("resize", render2d);
new ResizeObserver(render2d).observe($("#view2d-wrap"));
window.addEventListener("beforeunload", (e) => { if (Object.keys(S.docs).length) { e.preventDefault(); e.returnValue = ""; } });
document.addEventListener("pointerlockchange", () => {
  if (document.pointerLockElement !== room) look = false;
});

let dragDepth = 0;
window.addEventListener("dragenter", (e) => { e.preventDefault(); dragDepth++; document.body.classList.add("dragging"); });
window.addEventListener("dragleave", () => { if (--dragDepth <= 0) { dragDepth = 0; document.body.classList.remove("dragging"); } });
window.addEventListener("dragover", (e) => e.preventDefault());
window.addEventListener("drop", (e) => {
  e.preventDefault();
  dragDepth = 0;
  document.body.classList.remove("dragging");
  const files = [...e.dataTransfer.files];
  if (files.length) withEditorLoading("Opening dropped files…", files.map((f) => f.name).join(", "), () => openFiles(files));
});

const tools = installTools({
  S, $, el, NO_SIDE, clampI16, DOOM_LINE_SPECIALS, cv, room, mouse,
  toast, status, errText, snapshot, commit, render2d, renderInspector, side, vtx, tpl, geometry, sectorAt,
  selectionVertices, snap, compact, deleteSelection, syncModeButtons, isHexen, isUdmf, thumb, lumpBytes,
  setSummary, selectDocTab, setPlaying, clearThumbs: () => thumbs.clear(),
  openPaint: (ns, name) => paint.open(ns, name),
});
const paint = installPaint({
  S, el, toast, errText, selectDocTab, setSummary, render2d, renderInspector, thumb,
  clearThumbs: () => thumbs.clear(),
  actorSoundSlots, overrideLump,
});
const scripts = installScripts({
  S, el, toast, errText, selectDocTab, setSummary, setPlaying, renderBrowser, rowEl, download, pickFile, addPwad,
});
// Console scripting hook: edit `roomEditor.state.doc`, then `roomEditor.commit()`.
window.roomEditor = { state: S, tools, paint, scripts, snapshot, commit, undo, redo, loadMap };
$("#btn-check").addEventListener("click", () => tools.toggleChecker());
$("#btn-composer").addEventListener("click", () => tools.openComposer());
$("#btn-rotate").addEventListener("click", (e) => tools.transform(e.shiftKey ? "ccw" : "cw"));
$("#btn-mirror").addEventListener("click", (e) => tools.transform(e.shiftKey ? "mirrorY" : "mirrorX"));
$("#btn-copy").addEventListener("click", () => tools.copy() && status(`Copied ${S.sel.size} ${S.mode}`));
$("#btn-paste").addEventListener("click", () => tools.paste());

setHint();
setPlaying(false);
renderDocTabs();
renderInspector();
boot().catch((e) => { status(`Engine failed: ${errText(e)}`); console.error(e); });
pollCamera();
