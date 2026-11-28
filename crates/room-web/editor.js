// Room web editor: 2D map editing + live 3D / play test through the Room engine (WASM).
// The map document is the engine's `RawMap` as JSON; every change is pushed back with
// `roomEditorUpdateMap`, which patches the running level in place when it can.

import {
  install as installTools, HEXEN_LINE_SPECIALS, SPECIAL_ARGS, LINE_EXT_FLAGS, SECTOR_EXT_FLAGS,
  SECTOR_PORTAL_FLAGS, THING_EXT_FLAGS, THING_TYPE_HINTS,
} from "./editor-tools.js";
import { install as installPaint, paintNs } from "./editor-paint.js";
import { install as installScripts, highlight, textLang, includesOf, zscriptClosure, acsAssist, acsLocalNames } from "./editor-scripts.js";
import { argKindsOf } from "./editor-geom.js";
import { install as installExtras } from "./editor-extras.js";

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
  obstacle: "#b48cff", decoration: "#9aa3ad", special: "#ffd24a", portal: "#e07bff" };
const CAT_ORDER = ["player", "monster", "weapon", "pickup", "obstacle", "decoration", "special", "portal"];
/** Portal overlay colours: line portals, plane portals (floor / ceiling). */
const PORTAL_COLORS = { line: "#e07bff", floor: "#3fd6c6", ceiling: "#ff8fb8" };

const S = {
  mod: null,
  summary: null,
  dukeGrp: null,
  dukeMap: null,
  dukeMapBytes: null,
  dukeMapFile: null,
  dukeSelection: null,
  dukeClipboard: null,
  dukeUndo: [],
  dukeRedo: [],
  dukePersistBefore: null,
  dukeDragging: null,
  dukeTesting: false,
  thingInfo: new Map(),
  doc: null,
  docs: {},
  mapAcs: {}, // map → ACS source compiled into its BEHAVIOR in the editor
  acsGen: 0,
  mode: "sectors",
  sel: new Set(),
  hover: -1,
  pick3d: null,
  view: { x: 0, y: 0, scale: 0.5 },
  grid: 32,
  snap: true,
  textured: true,
  showPortals: true,
  /** Engine portal report for the open map (`roomEditorPortals`), refreshed on every sync. */
  portals: null,
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
/** Autosave, tag links, shaping and workflow tools (`editor-extras.js`), installed at the end. */
let extras = null;

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
      // `?tab=scripts` (the lobby's "Script a mod"): open that browser tab.
      const tab = params.get("tab");
      if (tab) {
        const button = document.querySelector(`#browser-tabs [data-tab="${CSS.escape(tab)}"]`);
        if (button && !button.hidden) button.click();
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
  extras?.onSummary();
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
  if (S.summary.duke && !["iwad", "maps", "things", "textures"].includes(S.tab)) S.tab = "maps";
  for (const tab of document.querySelectorAll("#browser-tabs [data-tab]")) {
    tab.hidden = !!S.summary.duke && !["iwad", "maps", "things", "textures"].includes(tab.dataset.tab);
  }
  if (S.summary.duke) {
    applyLayout("split");
    $("#btn-export").textContent = "Export GRP";
    $("#btn-export").title = "Download a native Duke GRP with Build map and ART tile edits (Ctrl+S)";
    $("#btn-play").textContent = "▶ Native test";
    $("#btn-play").title = "Play the selected Build map in the native Duke runtime (F5)";
    $("#btn-play-here").hidden = true;
    $("#btn-new-map").hidden = false;
    $("#btn-generate").hidden = true;
    $("#gen-panel").hidden = true;
    $("#btn-new-map").textContent = "New Build map";
    $("#btn-new-map").title = "Create an editable native Build .MAP in the working Duke GRP";
    $("#btn-visual").hidden = true;
    $("#btn-check").hidden = false;
    $("#btn-check").textContent = "✔ Check";
    $("#btn-check").title = "Check Build map structure (F8)";
    $("#btn-composer").hidden = true;
    $("#btn-export-pk3").hidden = true;
    $("#btn-open-pk3").hidden = true;
    document.querySelectorAll(".packs").forEach((e) => { e.hidden = true; });
    document.querySelector(".layout").hidden = false;
    $("#textured").parentElement.hidden = true;
    $("#portals-toggle").hidden = true;
    $("#duke-floor-toggle").hidden = false;
    ["#btn-copy", "#btn-paste", "#btn-rotate", "#btn-mirror"].forEach((id) => { $(id).hidden = false; $(id).disabled = false; });
    const labels = { vertices: "Vertices", lines: "Walls", sectors: "Sectors", things: "Sprites", draw: "Draw" };
    document.querySelectorAll("#modes .btn").forEach((b) => {
      b.textContent = labels[b.dataset.mode] || b.textContent;
      b.hidden = !labels[b.dataset.mode];
    });
    $("#doom-tools").hidden = true;
    $("#thing-filter").hidden = true;
  } else {
    $("#btn-export").textContent = "Export WAD";
    $("#btn-export").title = "Download edited maps and resources as a PWAD (Ctrl+S)";
    $("#btn-play").textContent = "▶ Play";
    $("#btn-play").title = "Play from the player start (F5)";
    $("#btn-play-here").hidden = false;
    $("#btn-new-map").hidden = false;
    $("#btn-generate").hidden = false;
    $("#btn-new-map").textContent = "New map";
    $("#btn-new-map").title = "New map";
    $("#btn-visual").hidden = false;
    $("#btn-check").hidden = false;
    $("#btn-check").textContent = "✔ Check";
    $("#btn-composer").hidden = false;
    $("#btn-export-pk3").hidden = false;
    $("#btn-open-pk3").hidden = false;
    document.querySelectorAll(".packs").forEach((e) => { e.hidden = false; });
    document.querySelector(".layout").hidden = false;
    $("#textured").parentElement.hidden = false;
    $("#portals-toggle").hidden = false;
    $("#duke-floor-toggle").hidden = true;
    ["#btn-copy", "#btn-paste", "#btn-rotate", "#btn-mirror"].forEach((id) => { $(id).hidden = false; });
    const labels = { vertices: "Vertices", lines: "Lines", sectors: "Sectors", things: "Things", draw: "Draw" };
    document.querySelectorAll("#modes .btn").forEach((b) => {
      b.textContent = labels[b.dataset.mode] || b.textContent;
      b.hidden = false;
    });
    $("#doom-tools").hidden = false;
    $("#thing-filter").hidden = false;
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
    S.mapAcs = {};
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
    if (!S.summary.duke) extras?.offerRestore();
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
    // Nothing mounted: the chip shows only the game (empty labels are hidden).
    label.title = "No extra archives — File › Mount content pack… or Add files on top…";
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
    // A map made in the editor (New map, Generate, a restored autosave) is
    // not in the game files: open its document and push it to the engine.
    const onlyHere = !!S.docs[name] && !S.summary?.duke && !S.summary?.maps?.includes(name);
    const json = onlyHere ? null : S.mod.roomEditorLoadMap(name);
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
    if (S.docs[name]) pushNow(); else refreshPortals();
    S.undo = []; S.redo = []; S.sel.clear(); S.pick3d = null; S.drawPts = [];
    S.version++;
    $("#map-select").value = name;
    setPlaying(false);
    if (S.dukeMap) { fitDukeView(); dukePreviewMapLoaded(); } else fitView();
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
    refreshPortals();
  } catch (e) {
    syncPill("error", "error");
    status(`Map error: ${errText(e)}`);
  }
}

/** Re-read the engine's portal report; repaint only when it changed. */
function refreshPortals() {
  const clear = () => { S.portals = null; S.portalsJson = null; };
  if (S.dukeMap || typeof S.mod?.roomEditorPortals !== "function") { clear(); return; }
  let json;
  try { json = S.mod.roomEditorPortals(); } catch (_) { clear(); return; }
  if (json === S.portalsJson) return;
  S.portalsJson = json;
  S.portals = JSON.parse(json);
  render2d();
  updatePortalInfo();
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
      if (!thingVisible(t)) return;
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

// ---------------------------------------------------------------- thing filter

const TF_KEY = "room/editor-thing-filter";
/** View › Things on the map: hidden categories, skill, game mode. */
const thingFilter = (() => {
  try {
    const f = JSON.parse(localStorage.getItem(TF_KEY) || "null");
    if (f) return { hide: new Set(f.hide || []), skill: f.skill || "all", mode: f.mode || "all" };
  } catch (_) { /* fresh */ }
  return { hide: new Set(), skill: "all", mode: "all" };
})();
const thingFilterOn = () => thingFilter.hide.size > 0 || thingFilter.skill !== "all" || thingFilter.mode !== "all";

/** Whether thing `t` passes the filter (category, skill bits, game-mode flags per map format). */
function thingVisible(t) {
  if (!thingFilterOn()) return true;
  const cat = S.thingInfo.get(t.type_id)?.cat || "special";
  if (thingFilter.hide.has(cat)) return false;
  const f = t.flags || 0;
  if (thingFilter.skill !== "all" && !(f & Number(thingFilter.skill))) return false;
  const m = thingFilter.mode;
  if (m === "all") return true;
  if (isHexen()) {
    // Hexen: explicit single / coop / deathmatch bits.
    return !!(f & { single: 256, coop: 512, dm: 1024 }[m]);
  }
  // Doom / Boom (and Room's UDMF bits): 16 multiplayer only, 32 not DM, 64 not coop.
  if (m === "single") return !(f & 16);
  if (m === "dm") return isUdmf() || !(f & 32);
  return isUdmf() || !(f & 64);
}

function saveThingFilter() {
  try { localStorage.setItem(TF_KEY, JSON.stringify({ hide: [...thingFilter.hide], skill: thingFilter.skill, mode: thingFilter.mode })); } catch (_) { /* private mode */ }
}

function renderThingFilter() {
  const box = $("#tf-cats");
  if (!box) return;
  box.replaceChildren(...CAT_ORDER.map((cat) => {
    const cb = el("input", { type: "checkbox", checked: !thingFilter.hide.has(cat) });
    cb.addEventListener("change", () => {
      if (cb.checked) thingFilter.hide.delete(cat); else thingFilter.hide.add(cat);
      thingFilterChanged();
    });
    return el("label", { class: "check" }, cb, el("span", { class: "tf-dot", style: `background:${CAT_COLORS[cat]}` }), cat[0].toUpperCase() + cat.slice(1));
  }));
  $("#tf-skill").value = thingFilter.skill;
  $("#tf-mode").value = thingFilter.mode;
}

function thingFilterChanged() {
  saveThingFilter();
  // Hidden things leave the selection, so edits never touch what you can't see.
  if (S.mode === "things" && S.doc) for (const i of [...S.sel]) if (!thingVisible(S.doc.things[i])) S.sel.delete(i);
  render2d();
  renderInspector();
  updateCounts();
}

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


// Wall / vertex / sprite "kind" colours for the Build 2D view (legend in the corner).
const DUKE_WALL_KINDS = {
  solid: { label: "One-sided wall", color: "#d6d9de", width: 1.35 },
  portal: { label: "Two-sided (portal) wall", color: "#6f8db8", width: 1.2 },
  blocking: { label: "Blocking portal (cstat 1)", color: "#e0904a", width: 1.8 },
  masked: { label: "Masked / see-through (cstat 16/32)", color: "#57c7d4", width: 1.8 },
  mirror: { label: "Mirror (MIRROR tile, one-way)", color: "#d77bff", width: 2.4 },
  tagged: { label: "Tagged wall (lotag/hitag)", color: "#ffd75e", width: 2 },
  door: { label: "Door / switch tile", color: "#ff6b6b", width: 2.2 },
};
const dukeIsDoorTile = (n) => n >= 0 && /SWITCH|DOOR|SLOT|HANDPRINT|ACCESS/.test(dukeTileName(n) || "");
function dukeWallKind(m, i) {
  const w = m.walls[i];
  const K = DUKE_WALL_KINDS;
  if (!w) return K.solid;
  if (w.picnum === 560 || w.overpicnum === 560) return K.mirror;
  if (w.lotag || w.hitag) return K.tagged;
  if (dukeIsDoorTile(w.picnum) || dukeIsDoorTile(w.overpicnum)) return K.door;
  if (w.nextwall >= 0) {
    if (w.cstat & (16 | 32)) return K.masked;
    if (w.cstat & 1) return K.blocking;
    return K.portal;
  }
  return K.solid;
}
const DUKE_VERTEX_KINDS = {
  shared: { label: "Shared vertex (joins sectors)", color: "#7fd3ff", shape: "square" },
  single: { label: "Single-sector vertex", color: "#9fb4ff", shape: "square" },
  tagged: { label: "Vertex on a tagged / special wall", color: "#ffd75e", shape: "diamond" },
  loop: { label: "Vertex of an inner loop (pillar/hole)", color: "#ff9fd0", shape: "circle" },
  problem: { label: "Overlap: >2 walls start here in one sector", color: "#ff5050", shape: "diamond" },
};
function dukeVertexIndex(m) {
  if (m._vertexIndex && m._vertexIndex.walls === m.walls && m._vertexIndex.n === m.walls.length && m._vertexIndex.gen === S.dukeGen) return m._vertexIndex.map;
  const map = new Map();
  const wallSector = new Int32Array(m.walls.length).fill(-1);
  const innerLoop = new Uint8Array(m.walls.length);
  m.sectors.forEach((sec, si) => {
    let first = true;
    let loopStart = sec.wallptr;
    for (let w = sec.wallptr; w < sec.wallptr + sec.wallnum && w < m.walls.length; w++) {
      wallSector[w] = si;
      if (!first) innerLoop[w] = 1;
      if (m.walls[w].point2 <= w && m.walls[w].point2 >= loopStart) { first = false; loopStart = w + 1; }
    }
  });
  m.walls.forEach((w, i) => {
    const key = `${w.x},${w.y}`;
    let e = map.get(key);
    if (!e) { e = { walls: [], sectors: new Set(), tagged: false, inner: false, perSector: new Map() }; map.set(key, e); }
    e.walls.push(i);
    const si = wallSector[i];
    if (si >= 0) { e.sectors.add(si); e.perSector.set(si, (e.perSector.get(si) || 0) + 1); }
    if (innerLoop[i]) e.inner = true;
    const k = dukeWallKind(m, i);
    if (k === DUKE_WALL_KINDS.tagged || k === DUKE_WALL_KINDS.door || k === DUKE_WALL_KINDS.mirror) e.tagged = true;
  });
  m._vertexIndex = { walls: m.walls, n: m.walls.length, gen: S.dukeGen, map };
  return map;
}
function dukeVertexKind(m, x, y) {
  const e = dukeVertexIndex(m).get(`${x},${y}`);
  const K = DUKE_VERTEX_KINDS;
  if (!e) return K.single;
  for (const n of e.perSector.values()) if (n > 1) return { ...K.problem, label: `${K.problem.label}` };
  if (e.tagged) return K.tagged;
  if (e.inner) return K.loop;
  return e.sectors.size > 1 ? { ...K.shared, label: `Shared vertex · ${e.sectors.size} sectors` } : K.single;
}
function dukeSpriteKind(sprite) {
  const name = dukeTileName(sprite.picnum) || "";
  if (sprite.picnum === 1) return { label: "Sector effector (SE)", color: "#c07bff" };
  if (sprite.picnum >= 2 && sprite.picnum <= 10) return { label: "Activator / locator / touchplate", color: "#ff9fd0" };
  if (sprite.picnum === 1405) return { label: "Player start", color: "#59aaff" };
  if (sprite.statnum === 1 || /TROOP|PIGCOP|LIZ|BOSS|OCTA|COMMANDER|DRONE|RECON|SHARK|ROTATEGUN|EGG|NEWBEAST|GREENSLIME|TURRET|TANK/.test(name)) return { label: "Enemy", color: "#ff5a5a" };
  if (/AMMO|WEAPON|GUN|SHOTGUN|CHAINGUN|RPG|DEVISTATOR|FREEZE|SHRINK|TRIPBOMB|HEAVYHB|BATTERY|SIXPAK|ATOMIC|COLA|MEDKIT|STEROIDS|SHIELD|AIRTANK|JETPACK|HOLODUKE|BOOTS|ACCESSCARD|HEATSENSOR|FIRSTAID/.test(name)) return { label: "Item / pickup", color: "#ffd75e" };
  if (sprite.cstat & (16 | 32)) return { label: "Wall/floor-aligned decoration", color: "#57c7d4" };
  return { label: "Decoration", color: "#4cc38a" };
}
function dukeDrawLegend(W, H) {
  if (S.dukeLegend === false) return;
  const rows = [];
  if (S.mode === "vertices") rows.push(...Object.values(DUKE_VERTEX_KINDS).map((k) => [k.color, k.label, k.shape]));
  else if (S.mode === "things") rows.push(["#ff5a5a", "Enemy"], ["#ffd75e", "Item / pickup"], ["#c07bff", "Sector effector (SE)"], ["#ff9fd0", "Activator / locator"], ["#57c7d4", "Aligned decoration"], ["#4cc38a", "Decoration"]);
  else rows.push(...Object.values(DUKE_WALL_KINDS).map((k) => [k.color, k.label, "line"]));
  ctx.font = "11px ui-monospace, monospace";
  const w = Math.max(...rows.map((r) => ctx.measureText(r[1]).width)) + 34;
  const h = rows.length * 16 + 10;
  const x0 = W - w - 10, y0 = H - h - 34;
  ctx.fillStyle = "rgba(8,10,13,.86)"; ctx.fillRect(x0, y0, w, h);
  ctx.strokeStyle = "rgba(190,205,220,.3)"; ctx.lineWidth = 1; ctx.strokeRect(x0 + .5, y0 + .5, w - 1, h - 1);
  ctx.textBaseline = "middle";
  rows.forEach(([color, label, shape], i) => {
    const cy = y0 + 13 + i * 16, cx = x0 + 12;
    ctx.fillStyle = color; ctx.strokeStyle = color;
    if (shape === "line") { ctx.lineWidth = 2.5; ctx.beginPath(); ctx.moveTo(cx - 6, cy); ctx.lineTo(cx + 8, cy); ctx.stroke(); }
    else if (shape === "diamond") { ctx.beginPath(); ctx.moveTo(cx, cy - 5); ctx.lineTo(cx + 5, cy); ctx.lineTo(cx, cy + 5); ctx.lineTo(cx - 5, cy); ctx.closePath(); ctx.fill(); }
    else if (shape === "square") ctx.fillRect(cx - 4, cy - 4, 8, 8);
    else { ctx.beginPath(); ctx.arc(cx, cy, 4, 0, Math.PI * 2); ctx.fill(); }
    ctx.fillStyle = "#dfe6ee"; ctx.fillText(label, x0 + 26, cy);
  });
  ctx.textBaseline = "alphabetic";
}

// Animation viewer: CON actions (rotations × frames) and the ART picanm cycle.
function dukeAnimations(picnum) {
  if (!S.mod?.roomEditorDukeAnimations || picnum == null || picnum < 0) { status("Animation data is not available for this tile"); return; }
  let data;
  try { data = JSON.parse(S.mod.roomEditorDukeAnimations(Number(picnum))); } catch (e) { status(`Animations: ${e}`); return; }
  document.querySelector("dialog.duke-anim")?.remove();
  const dlg = el("dialog", { class: "duke-anim" });
  const bt = (n) => `BT${String(n).padStart(4, "0")}`;
  const name = dukeTileName(picnum);
  const stage = el("canvas", { class: "duke-anim-stage", width: 192, height: 192 });
  const info = el("div", { class: "muted" });
  const grid = el("div", { class: "duke-anim-grid" });
  const list = el("div", { class: "duke-anim-list" });
  let current = null, timer = 0, playing = true, tic = 0, rot = 0;
  const frameTile = (a, frame, view) => picnum + a.start + view + a.views * frame * a.inc;
  const clips = [];
  if (data.picanm.length > 1) clips.push({ name: "ART cycle (picanm)", art: data.picanm });
  for (const a of data.actions) clips.push(a);
  if (!clips.length) clips.push({ name: "Static tile", art: [[picnum, 1]] });
  const views = (a) => (a.art ? 1 : Math.max(1, [0, 1, 2, 4, 4, 5, 0, 7, 8][a.views] ?? 1));
  function select(clip, btn) {
    current = clip; tic = 0;
    list.querySelectorAll("button").forEach((b) => b.classList.toggle("active", b === btn));
    grid.replaceChildren();
    if (clip.art) {
      info.textContent = `${clip.art.length} frames · ART speed from picanm · click a frame to paint`;
      const row = el("div", { class: "duke-anim-row" }, el("span", {}, "—"));
      clip.art.forEach(([t]) => row.append(frameCell(t)));
      grid.append(row);
    } else {
      info.textContent = `start ${clip.start} · ${clip.frames} frame(s) · views ${clip.views} · inc ${clip.inc} · delay ${clip.delay}${clip.default ? " · default action" : ""}`;
      for (let v = 0; v < views(clip); v++) {
        const row = el("div", { class: "duke-anim-row" }, el("span", {}, `rot ${v}`));
        for (let f = 0; f < Math.max(1, Math.abs(clip.frames)); f++) row.append(frameCell(frameTile(clip, f, v), () => { rot = v; }));
        grid.append(row);
      }
    }
  }
  function frameCell(tile, extra) {
    const c = el("canvas", { width: 56, height: 56 });
    drawThumb(c, bt(tile), 56);
    return el("div", { class: "thumb", title: `${bt(tile)}${dukeTileName(tile) ? ` · ${dukeTileName(tile)}` : ""} — click to paint`, onclick: () => { extra?.(); dlg.close(); paint.open("textures", bt(tile)); } }, c, el("div", {}, String(tile)));
  }
  function step() {
    if (!current || !dlg.open) return;
    let tile;
    if (current.art) {
      const total = current.art.reduce((s, [, d]) => s + Math.max(1, d), 0);
      let t = tic % Math.max(1, total);
      tile = current.art[0][0];
      for (const [n, d] of current.art) { if (t < Math.max(1, d)) { tile = n; break; } t -= Math.max(1, d); }
    } else {
      const period = Math.floor(current.delay / 4) + 1;
      const frames = Math.max(1, Math.abs(current.frames));
      const f = Math.floor(tic / period) % frames;
      tile = frameTile(current, f, Math.min(rot, views(current) - 1));
    }
    drawThumb(stage, bt(tile), 192);
    stage.title = bt(tile);
    if (playing) tic++;
    grid.querySelectorAll(".thumb").forEach((t) => t.classList.toggle("active", t.title.startsWith(bt(tile))));
  }
  clips.forEach((clip, i) => {
    const b = el("button", { class: "btn small" }, `${clip.name}${clip.default ? " ★" : ""}`);
    b.addEventListener("click", () => select(clip, b));
    list.append(b);
    if (i === 0 || clip.default) queueMicrotask(() => { if (!current || clip.default) select(clip, b); });
  });
  const play = el("button", { class: "btn small" }, "Pause");
  play.addEventListener("click", () => { playing = !playing; play.textContent = playing ? "Pause" : "Play"; });
  const prevRot = el("button", { class: "btn small", title: "Previous rotation" }, "⟲");
  const nextRot = el("button", { class: "btn small", title: "Next rotation" }, "⟳");
  prevRot.addEventListener("click", () => { if (current && !current.art) rot = (rot + views(current) - 1) % views(current); });
  nextRot.addEventListener("click", () => { if (current && !current.art) rot = (rot + 1) % views(current); });
  const close = el("button", { class: "btn small", onclick: () => dlg.close() }, "Close");
  dlg.append(
    el("h3", {}, `Animations · ${bt(picnum)}${name ? ` · ${name}` : ""}`),
    el("div", { class: "duke-anim-body" },
      el("div", { class: "duke-anim-side" }, stage, el("div", { class: "actions" }, play, prevRot, nextRot), info),
      el("div", { class: "duke-anim-main" }, list, grid)),
    el("div", { class: "actions" }, close));
  document.body.append(dlg);
  dlg.addEventListener("close", () => { clearInterval(timer); dlg.remove(); });
  dlg.showModal();
  timer = setInterval(step, 1000 / 30);
}

function drawDuke2d(W, H, dpr) {
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  ctx.fillStyle = "#0b0c0e";
  ctx.fillRect(0, 0, W, H);
  const m = S.dukeMap;
  if (!m) return;
  S.dukeGen = (S.dukeGen || 0) + 1;
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
    const kind = dukeWallKind(m, i);
    ctx.strokeStyle = selected ? "#ffcc33" : kind.color;
    ctx.lineWidth = selected ? 2.5 : kind.width;
    ctx.stroke();
  });
  if (S.mode === "vertices" || S.mode === "lines") {
    const seen = new Set();
    m.walls.forEach((wall) => {
      const key = `${wall.x},${wall.y}`;
      if (seen.has(key)) return;
      seen.add(key);
      const [ax, ay] = dukeToScreen(wall.x, wall.y);
      if (ax < -8 || ay < -8 || ax > W + 8 || ay > H + 8) return;
      const vertexSelected = S.dukeSelection?.kind === "vertex" && S.dukeSelection.x === wall.x && S.dukeSelection.y === wall.y;
      const r = vertexSelected ? 4 : (S.view.scale > 0.12 ? 2.6 : S.view.scale > 0.05 ? 1.6 : 0);
      if (!r) return;
      const kind = dukeVertexKind(m, wall.x, wall.y);
      ctx.fillStyle = vertexSelected ? "#ffcc33" : kind.color;
      if (kind.shape === "diamond") {
        ctx.beginPath(); ctx.moveTo(ax, ay - r - 1); ctx.lineTo(ax + r + 1, ay); ctx.lineTo(ax, ay + r + 1); ctx.lineTo(ax - r - 1, ay); ctx.closePath(); ctx.fill();
      } else if (kind.shape === "circle") {
        ctx.beginPath(); ctx.arc(ax, ay, r, 0, Math.PI * 2); ctx.fill();
      } else {
        ctx.fillRect(ax - r, ay - r, r * 2, r * 2);
      }
    });
  }
  dukeDrawLegend(W, H);
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
    ctx.fillStyle = selected ? "#ffcc33" : dukeSpriteKind(sprite).color;
    ctx.strokeStyle = "#111";
    ctx.beginPath(); ctx.arc(x, y, r, 0, Math.PI * 2); ctx.fill(); ctx.stroke();
    const a = sprite.ang * Math.PI / 1024;
    ctx.beginPath(); ctx.moveTo(x, y); ctx.lineTo(x + Math.cos(a) * 10, y + Math.sin(a) * 10); ctx.stroke();
  });
  const [sx, sy] = dukeToScreen(m.start[0], m.start[1]);
  ctx.strokeStyle = "#59aaff"; ctx.lineWidth = 2;
  ctx.beginPath(); ctx.moveTo(sx - 6, sy); ctx.lineTo(sx + 6, sy); ctx.moveTo(sx, sy - 6); ctx.lineTo(sx, sy + 6); ctx.stroke();
  dukeDrawPreviewCamera(ctx);
  const mode = { vertices: "vertices", lines: "walls", sectors: "sectors", things: "sprites" }[S.mode] || S.mode;
  const extra = { lines: " · double-click: split wall", things: " · double-click/Insert: add sprite" }[S.mode] || "";
  $("#hint2d").textContent = `Build ${mode} · click to select · drag to move${extra} · Alt+double-click: put 3D camera · wheel zoom · right-drag pan · F5 native test`;
}

let renderQueued = false;
function render2d() {
  if (renderQueued) return;
  renderQueued = true;
  requestAnimationFrame(() => { renderQueued = false; draw2d(); });
}

/** Portal overlay: plane portals as dashed sector outlines, line portals as
 * thick lines with an arrow to their destination, linked group numbers. */
function drawPortalOverlay(ctx, geo, sc, dpr, W, H) {
  const d = S.doc, rep = S.portals;
  if (!rep.lines.length && !rep.planes.length && !rep.line_copies.length) {
    ctx.font = "12px system-ui, sans-serif";
    ctx.fillStyle = "rgba(10,12,16,0.75)";
    const msg = isUdmf() || isHexen()
      ? "No portals in this map — select two lines and press Make portal"
      : "No portals in this map — Doom-format maps have none; convert to UDMF from the inspector";
    const w = ctx.measureText(msg).width + 16;
    ctx.fillRect(8, H - 30, w, 22);
    ctx.fillStyle = PORTAL_COLORS.line;
    ctx.fillText(msg, 16, H - 15);
    return;
  }
  ctx.save();
  ctx.setTransform(dpr * sc, 0, 0, -dpr * sc, dpr * (W / 2 - S.view.x * sc), dpr * (H / 2 + S.view.y * sc));
  ctx.lineWidth = 2 / sc;
  for (const p of rep.planes) {
    const path = geo.paths[p.sector];
    if (!path) continue;
    const color = p.disabled ? "#8a8f98" : PORTAL_COLORS[p.plane];
    ctx.setLineDash(p.plane === "floor" ? [6 / sc, 4 / sc] : [2 / sc, 4 / sc]);
    ctx.lineDashOffset = p.plane === "floor" ? 0 : 3 / sc;
    ctx.strokeStyle = color;
    ctx.stroke(path);
    if (!p.disabled && p.kind !== "sky") {
      ctx.fillStyle = `${color}${p.linked ? "2e" : "1a"}`;
      ctx.fill(path, "evenodd");
    }
  }
  ctx.restore();
  const mid = (li) => {
    const l = d.linedefs[li];
    const a = l && vtx(l.v1), b = l && vtx(l.v2);
    return a && b ? toScreen((a.x + b.x) / 2, (a.y + b.y) / 2) : null;
  };
  const selLine = (li) => S.mode === "lines" && (S.sel.has(li) || S.hover === li);
  ctx.lineCap = "round";
  for (const [li] of rep.line_copies) {
    const l = d.linedefs[li];
    const a = l && vtx(l.v1), b = l && vtx(l.v2);
    if (!a || !b) continue;
    const [ax, ay] = toScreen(a.x, a.y), [bx, by] = toScreen(b.x, b.y);
    ctx.strokeStyle = PORTAL_COLORS.floor; ctx.lineWidth = 3; ctx.setLineDash([4, 3]);
    ctx.beginPath(); ctx.moveTo(ax, ay); ctx.lineTo(bx, by); ctx.stroke();
  }
  ctx.setLineDash([]);
  for (const p of rep.lines) {
    const l = d.linedefs[p.line];
    const a = l && vtx(l.v1), b = l && vtx(l.v2);
    if (!a || !b) continue;
    const [ax, ay] = toScreen(a.x, a.y), [bx, by] = toScreen(b.x, b.y);
    const hot = selLine(p.line) || (p.target != null && selLine(p.target));
    ctx.globalAlpha = hot ? 1 : 0.8;
    ctx.strokeStyle = p.target == null && p.kind !== "horizon" && p.kind !== "mirror" ? "#ff5a4a" : PORTAL_COLORS.line;
    ctx.lineWidth = p.passable ? 4 : 3;
    ctx.beginPath(); ctx.moveTo(ax, ay); ctx.lineTo(bx, by); ctx.stroke();
    const m0 = mid(p.line), m1 = p.target != null ? mid(p.target) : null;
    if (m0 && m1) {
      ctx.globalAlpha = hot ? 1 : 0.45;
      ctx.strokeStyle = PORTAL_COLORS.line;
      ctx.lineWidth = hot ? 2 : 1.2;
      ctx.setLineDash([6, 4]);
      ctx.beginPath(); ctx.moveTo(m0[0], m0[1]); ctx.lineTo(m1[0], m1[1]); ctx.stroke();
      ctx.setLineDash([]);
      const ang = Math.atan2(m1[1] - m0[1], m1[0] - m0[0]);
      const k = hot ? 10 : 7;
      ctx.fillStyle = PORTAL_COLORS.line;
      ctx.beginPath();
      ctx.moveTo(m1[0], m1[1]);
      ctx.lineTo(m1[0] - Math.cos(ang - 0.4) * k, m1[1] - Math.sin(ang - 0.4) * k);
      ctx.lineTo(m1[0] - Math.cos(ang + 0.4) * k, m1[1] - Math.sin(ang + 0.4) * k);
      ctx.closePath(); ctx.fill();
    }
  }
  ctx.globalAlpha = 1;
  ctx.lineCap = "butt";
  if (rep.groups.length && sc > 0.2) {
    ctx.font = "11px system-ui, sans-serif";
    ctx.textAlign = "center";
    ctx.textBaseline = "middle";
    rep.groups.forEach((g, si) => {
      const bb = geo.bbox[si];
      if (!g || !bb || !Number.isFinite(bb[0])) return;
      const [x, y] = toScreen((bb[0] + bb[2]) / 2, (bb[1] + bb[3]) / 2);
      if (x < 0 || y < 0 || x > W || y > H) return;
      ctx.fillStyle = "rgba(10,12,16,0.7)";
      ctx.fillRect(x - 13, y - 8, 26, 16);
      ctx.fillStyle = PORTAL_COLORS.floor;
      ctx.fillText(`G${g}`, x, y);
    });
    ctx.textAlign = "start";
    ctx.textBaseline = "alphabetic";
  }
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
  extras?.drawBackground(ctx);
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
  extras?.drawLinks(ctx, geo, dpr, W, H);
  if (S.showPortals && S.portals) drawPortalOverlay(ctx, geo, sc, dpr, W, H);
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
      if (!thingVisible(t)) return;
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
  extras?.drawTool(ctx, W, H);
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
    draw: "Click to add points · click the first point (or Enter) to close · Esc cancels · Rectangle / Ellipse: drag a box",
    ...(extras?.HINTS || {}),
  };
  $("#hint2d").textContent = `${hints[S.mode] || ""} · Wheel zoom · Right/middle-drag pan · Double-click: move 3D camera`;
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
  } else if (hit?.kind === "wall" || hit?.kind === "sector") {
    // Move the wall's two vertices, or every vertex (and sprite) of the sector.
    const m = S.dukeMap;
    const points = new Map();
    const add = (w) => { if (w) points.set(`${w.x},${w.y}`, [w.x, w.y]); };
    let sprites = [];
    if (hit.kind === "wall") { add(m.walls[hit.index]); add(m.walls[m.walls[hit.index]?.point2]); }
    else {
      const sec = m.sectors[hit.index];
      for (let i = sec.wallptr; i < sec.wallptr + sec.wallnum; i++) add(m.walls[i]);
      sprites = m.sprites.map((sp, i) => (sp.sectnum === hit.index ? [i, sp.x, sp.y] : null)).filter(Boolean);
    }
    S.dukeDragging = { kind: "group", points: [...points.values()], sprites, originX: wx, originY: wy, dx: 0, dy: 0 };
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
  if (d.kind === "group") {
    // Snap the delta, then move from the previous delta to the new one.
    const ndx = snap(wx - d.originX), ndy = snap(wy - d.originY);
    if (ndx === d.dx && ndy === d.dy) return;
    const moves = new Map(d.points.map(([px, py]) => [`${px + d.dx},${py + d.dy}`, [px + ndx, py + ndy]]));
    for (const w of S.dukeMap.walls) {
      const to = moves.get(`${w.x},${w.y}`);
      if (to) [w.x, w.y] = to;
    }
    for (const [i, sx0, sy0] of d.sprites) {
      const sp = S.dukeMap.sprites[i];
      if (sp) { sp.x = sx0 + ndx; sp.y = sy0 + ndy; }
    }
    d.dx = ndx; d.dy = ndy;
    render2d();
    return;
  }
  const x = snap(d.x + wx - d.originX), y = snap(d.y + wy - d.originY);
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
  if (extras?.mouseDown(wx, wy, e)) { render2d(); return; }
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
  if (extras?.mouseMove(wx, wy)) { render2d(); return; }
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
  if (extras?.mouseUp()) render2d();
  if (mouse.box && S.doc) {
    const [x0, y0, x1, y1] = mouse.box;
    const [ax, ay] = toWorld(Math.min(x0, x1), Math.max(y0, y1));
    const [bx, by] = toWorld(Math.max(x0, x1), Math.min(y0, y1));
    const inside = (p) => p.x >= ax && p.x <= bx && p.y >= ay && p.y <= by;
    const d = S.doc;
    // With a Find filter on, only matching objects are picked.
    const pick = (i) => { if (!extras || extras.allowBox(S.mode, i)) S.sel.add(i); };
    if (S.mode === "vertices") d.vertices.forEach((v, i) => { if (inside(v)) pick(i); });
    if (S.mode === "lines") d.linedefs.forEach((l, i) => { if (inside(vtx(l.v1)) && inside(vtx(l.v2))) pick(i); });
    if (S.mode === "things") d.things.forEach((t, i) => { if (inside(t) && thingVisible(t)) pick(i); });
    if (S.mode === "sectors") {
      const g = geometry();
      g.bbox.forEach((bb, i) => { if (bb[0] >= ax && bb[2] <= bx && bb[1] >= ay && bb[3] <= by) pick(i); });
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
    if (e.altKey || (S.mode !== "things" && S.mode !== "lines" && S.mode !== "draw")) { dukePlaceCamera(wx, wy); return; }
    if (S.mode === "things") addDukeSprite(snap(wx), snap(wy));
    else if (S.mode === "lines") {
      const hit = dukeHit(wx, wy);
      if (hit?.kind === "wall") dukeSplitWall(hit.index, snap(wx), snap(wy));
    }
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
    if (hit) {
      const h = JSON.parse(hit);
      applyPick(h, e.shiftKey);
      if (h.kind === "wall" && !e.shiftKey) startOffsetDrag(h, e);
    }
  }
});

// Dragging a picked wall in 3D moves its texture with the mouse (x / y
// offsets); a plain click without moving just picks it. One undo step.
let offDrag = null;
function startOffsetDrag(h, e) {
  const t = pickTarget();
  if (!t || t.plane) return;
  const l = S.doc.linedefs[h.index];
  const a = vtx(l.v1), b = vtx(l.v2);
  // Texels per screen pixel at the wall's distance (90° horizontal field of view).
  let dist = 256;
  if (a && b && S.cam) {
    const dx = b.x - a.x, dy = b.y - a.y, len2 = dx * dx + dy * dy || 1;
    const k = Math.max(0, Math.min(1, ((S.cam.x - a.x) * dx + (S.cam.y - a.y) * dy) / len2));
    dist = Math.max(32, Math.hypot(a.x + dx * k - S.cam.x, a.y + dy * k - S.cam.y));
  }
  offDrag = { t, sx: e.clientX, sy: e.clientY, ox: t.obj.x_offset, oy: t.obj.y_offset, scale: (dist * 2) / Math.max(1, room.clientWidth), moved: false, queued: false };
}
window.addEventListener("mousemove", (e) => {
  if (!offDrag || S.playing) return;
  const dx = e.clientX - offDrag.sx, dy = e.clientY - offDrag.sy;
  if (!offDrag.moved && Math.hypot(dx, dy) < 4) return;
  if (!offDrag.moved) { snapshot(); offDrag.moved = true; room.style.cursor = "move"; }
  const step = e.shiftKey ? 8 : 1;
  const q = (v) => Math.round(v / step) * step;
  // Texture follows the mouse: right → smaller x offset, down → smaller y offset.
  offDrag.t.obj.x_offset = clampI16(offDrag.ox - q(dx * offDrag.scale));
  offDrag.t.obj.y_offset = clampI16(offDrag.oy - q(dy * offDrag.scale));
  status(`Offsets ${offDrag.t.obj.x_offset}, ${offDrag.t.obj.y_offset}${e.shiftKey ? " (steps of 8)" : " · Shift: steps of 8"}`);
  if (!offDrag.queued) {
    offDrag.queued = true;
    requestAnimationFrame(() => { if (offDrag) offDrag.queued = false; commit({ inspector: false }); });
  }
});
window.addEventListener("mouseup", (e) => {
  if (e.button !== 0 || !offDrag) return;
  const moved = offDrag.moved;
  offDrag = null;
  room.style.cursor = "";
  if (moved) { commit(); renderInspector(); }
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
    if (S.dukeMap) dukePreviewInvalidate();
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
    : S.dukeMap
      ? "Native Build preview — Click: select · Right-drag: look · WASD move · Q/Z up/down · arrows turn · Shift fast · Home: player start · Alt+double-click in 2D: move camera"
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
  if (S.dukeMap) dukePreviewInvalidate();
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

/** Doom-format maps: portals and UDMF flags need a conversion first. */
function udmfNotice() {
  return el("fieldset", {}, el("legend", {}, "Portals & UDMF"),
    el("p", { class: "muted", style: "margin:0 0 6px" },
      "This is a Doom-format map, which has no portal specials, line ids or UDMF flags. Convert it to UDMF to use them (Undo reverts)."),
    el("button", { class: "btn small", onclick: convertToUdmf }, "Convert map to UDMF"));
}

function convertToUdmf() {
  if (!S.doc || isUdmf()) return;
  if (typeof S.mod?.roomEditorConvertToUdmf !== "function") { toast("Rebuild the editor (wasm) to convert maps", true); return; }
  try {
    const next = JSON.parse(S.mod.roomEditorConvertToUdmf(JSON.stringify(S.doc)));
    snapshot();
    S.doc = next;
    commit();
    toast(`${S.doc.name} is now a UDMF map — exports save it as TEXTMAP`);
  } catch (e) {
    toast(`Convert: ${errText(e)}`, true);
  }
}

/** Highest line id in use (UDMF `id` / `moreids`, Hexen id-carrying specials). */
function maxLineId() {
  let m = 0;
  for (const l of S.doc.linedefs) {
    m = Math.max(m, l.tag || 0, ...(l.extra_ids || []));
    if (isHexen() && l.special === 121) m = Math.max(m, l.args[0] + 256 * (l.args[4] || 0));
    if (l.special === 156) m = Math.max(m, l.args[0], l.args[1]);
  }
  return m;
}

/** Two selected lines become a pair of `Line_SetPortal`s looking into each other. */
function makePortalPair() {
  const idx = [...S.sel];
  if (S.mode !== "lines" || idx.length !== 2) { toast("Select exactly two lines (Shift+click), then Make portal", true); return; }
  if (!isUdmf() && !isHexen()) { toast("Convert the map to UDMF first", true); return; }
  const [a, b] = idx.map((i) => S.doc.linedefs[i]);
  const two = a.side_back !== NO_SIDE && b.side_back !== NO_SIDE;
  const type = two ? 2 : 0; // interactive needs a back sector; else visual
  const ida = maxLineId() + 1, idb = ida + 1;
  snapshot();
  for (const [l, self, other] of [[a, ida, idb], [b, idb, ida]]) {
    l.special = 156;
    l.args = [other, self, type, 0, 0];
    if (isUdmf()) l.tag = self;
  }
  commit();
  toast(`Lines ${idx[0]} ↔ ${idx[1]}: ${two ? "interactive" : "visual"} portal (ids ${ida} / ${idb}); change the type in arg2`);
}

/** Thing TID / special / args (args named when the special is known). */
function thingAction(objs) {
  const fs = fieldset("Action", objs, ["tid", "special"], { fields: { special: { options: HEXEN_LINE_SPECIALS } } });
  fs.append(namedArgs(objs, objs[0].special) || rowEl("args", fieldFor(objs, "args")));
  return fs;
}

/** Arguments with names for the specials `SPECIAL_ARGS` explains. */
function namedArgs(objs, special) {
  const spec = SPECIAL_ARGS[special];
  if (!spec || objs.some((o) => o.special !== special)) return null;
  const wrap = el("div", { class: "named-args" });
  spec.forEach((arg, j) => {
    if (!arg) return;
    const [label, options] = arg;
    const v0 = objs[0].args?.[j] ?? 0;
    const set = (v) => {
      snapshot();
      for (const o of objs) { o.args = [...(o.args || [0, 0, 0, 0, 0])]; o.args[j] = v; }
      commit({ inspector: false });
    };
    let input;
    if (options) {
      input = el("select", {});
      if (!(v0 in options)) input.append(el("option", { value: v0 }, `${v0} (custom)`));
      for (const [k, name] of Object.entries(options)) input.append(el("option", { value: k, selected: Number(k) === v0 }, `${k} — ${name}`));
      input.addEventListener("change", () => set(Number(input.value)));
    } else {
      input = el("input", { type: "number", value: v0 });
      input.addEventListener("change", () => set(Math.round(Number(input.value) || 0)));
    }
    wrap.append(rowEl(`arg${j}: ${label}`, input));
  });
  return wrap;
}

/** Specials that make or change portals (show the Portal panel for them). */
const PORTAL_SPECIALS = new Set([1, 5, 9, 57, 107, 156, 182, 301]);

const LINE_PORTAL_NAMES = { visual: "Visual (see-through only)", teleport: "Teleport", interactive: "Interactive",
  linked: "Linked (seamless)", horizon: "Horizon", mirror: "Mirror", other: "Other" };
const PLANE_PORTAL_NAMES = { sky: "Sky", skybox: "Skybox viewpoint", stacked: "Stacked sectors", portal: "Copy portal",
  linked: "Linked portal", plane: "Plane portal", horizon: "Horizon", other: "Other" };

/** Engine view of the selected line's / sector's portals (refreshed live). */
function portalInfo() {
  const box = el("div", { id: "insp-portal-info", class: "portal-info" });
  fillPortalInfo(box);
  return box;
}

function updatePortalInfo() {
  const box = document.getElementById("insp-portal-info");
  if (box) { box.replaceChildren(); fillPortalInfo(box); }
}

function fillPortalInfo(box) {
  const rep = S.portals;
  const idx = [...S.sel];
  if (!rep) { box.append(el("p", { class: "muted" }, "Portal info appears once the map is synced.")); return; }
  if (idx.length !== 1) return;
  const i = idx[0];
  const go = (mode, j, label) => el("button", { class: "btn small", onclick: () => tools.show(mode, [j]) }, label);
  if (S.mode === "lines") {
    const p = rep.lines.find((x) => x.line === i);
    const into = rep.lines.filter((x) => x.target === i);
    const copy = rep.line_copies.find(([l]) => l === i);
    const po = rep.polyobjects.find((x) => x.lines.includes(i));
    if (p) {
      box.append(el("div", { class: "row" }, el("label", {}, "portal"), el("span", {}, LINE_PORTAL_NAMES[p.kind] || p.kind)));
      const flags = [p.visible && "visible", p.passable && "passable", p.interactive && "interactive", p.sound && "sound"].filter(Boolean);
      box.append(el("div", { class: "row" }, el("label", {}, "behaviour"), el("span", { class: "muted" }, flags.join(" · ") || "none")));
      if (p.kind === "linked" || p.align !== "absolute") box.append(el("div", { class: "row" }, el("label", {}, "anchor"), el("span", {}, p.align)));
      if (p.target != null) box.append(el("div", { class: "row" }, el("label", {}, "destination"), el("div", { class: "tex-field" }, el("span", {}, `line ${p.target}`), go("lines", p.target, "Go to"))));
      else if (p.kind !== "horizon" && p.kind !== "mirror") box.append(el("p", { class: "warn-text" }, "No destination line — set arg0 to a line id that exists."));
    }
    for (const q of into) box.append(el("div", { class: "row" }, el("label", {}, "seen from"), el("div", { class: "tex-field" }, el("span", {}, `line ${q.line} (${q.kind})`), go("lines", q.line, "Go to"))));
    if (copy) box.append(el("div", { class: "row" }, el("label", {}, "shows"), el("span", {}, `sector portal #${copy[1]}`)));
    if (po) box.append(el("div", { class: "row" }, el("label", {}, "polyobject"), el("span", {}, `#${po.id}${po.portal_lines.includes(i) ? " — portal moves with it" : ""}`)));
    if (!p && !into.length && !copy && !po) box.append(el("p", { class: "muted" }, "Not a portal."));
  } else if (S.mode === "sectors") {
    const planes = rep.planes.filter((x) => x.sector === i);
    for (const p of planes) {
      const parts = [PLANE_PORTAL_NAMES[p.kind] || p.kind];
      if (p.linked) parts.push("linked");
      if (p.disabled) parts.push("disabled");
      if (p.norender) parts.push("not drawn");
      if (p.alpha < 1) parts.push(`alpha ${p.alpha.toFixed(2)}`);
      const row = el("div", { class: "tex-field" }, el("span", {}, parts.join(" · ")));
      if (p.destination != null && p.destination !== i) row.append(go("sectors", p.destination, `→ sector ${p.destination}`));
      if (p.thing != null) row.append(go("things", p.thing, `thing ${p.thing}`));
      box.append(el("div", { class: "row" }, el("label", { style: `color:${PORTAL_COLORS[p.plane]}` }, p.plane), row));
    }
    const g = rep.groups[i];
    if (g) box.append(el("div", { class: "row" }, el("label", {}, "group"), el("span", {}, `linked group ${g} of ${rep.group_count - 1}`)));
    if (!planes.length && !g) box.append(el("p", { class: "muted" }, "No portal planes."));
  }
}

/** Editable `user_*` fields (UDMF), one `user_name = value` per line. */
function userFields(objs) {
  if (objs.length !== 1) return null;
  const o = objs[0];
  const bag = o.user || { ints: {}, fixed: {} };
  const lines = [
    ...Object.entries(bag.ints || {}).map(([k, v]) => `${k} = ${v}`),
    ...Object.entries(bag.fixed || {}).map(([k, v]) => {
      const t = String(Math.round((v / 65536) * 10000) / 10000);
      return `${k} = ${t.includes(".") ? t : `${t}.0`}`;
    }),
  ];
  const ta = el("textarea", { rows: Math.max(2, lines.length + 1), spellcheck: "false", placeholder: "user_score = 10\nuser_speed = 1.5" });
  ta.value = lines.join("\n");
  ta.addEventListener("change", () => {
    const next = { ints: {}, fixed: {} };
    for (const raw of ta.value.split("\n")) {
      const line = raw.trim();
      if (!line) continue;
      const m = /^(user_[a-z0-9_]+)\s*=\s*(true|false|-?\d+(?:\.\d*)?)\s*;?$/i.exec(line);
      if (!m) { toast(`User field: "${line}" — use user_name = number`, true); return; }
      const key = m[1].toLowerCase(), v = m[2].toLowerCase();
      if (v === "true" || v === "false") next.ints[key] = v === "true" ? 1 : 0;
      else if (v.includes(".")) next.fixed[key] = Math.round(Number(v) * 65536);
      else next.ints[key] = Number(v) | 0;
    }
    snapshot();
    if (Object.keys(next.ints).length || Object.keys(next.fixed).length) o.user = next; else delete o.user;
    commit({ inspector: false });
  });
  return el("fieldset", {}, el("legend", {}, "User fields (user_*)"), ta,
    el("p", { class: "muted", style: "margin:4px 0 0" }, "Read by ZScript and ACS (GetUserVariable / GetUDMFInt). Decimals are stored as fixed point."));
}

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
  const keys = Object.keys(objs[0]).filter((k) => !skip.includes(k) && k !== "ext" && k !== "user");
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
    dukePreviewInvalidate();
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

function dukeCheckMap() {
  const m = S.dukeMap;
  if (!m) return;
  const errors = [];
  if (!m.sectors.length) errors.push("map has no sectors");
  m.sectors.forEach((sector, index) => {
    const start = sector.wallptr, end = start + sector.wallnum;
    if (start < 0 || sector.wallnum < 3 || end > m.walls.length) errors.push(`sector ${index}: invalid wall range`);
    else for (let i = start; i < end; i++) {
      const next = m.walls[i]?.point2;
      if (!Number.isInteger(next) || next < start || next >= end) { errors.push(`sector ${index}: wall ${i} points outside its sector`); break; }
    }
  });
  m.walls.forEach((wall, index) => {
    if (!Number.isInteger(wall.point2) || wall.point2 < 0 || wall.point2 >= m.walls.length) errors.push(`wall ${index}: invalid point2`);
    if (wall.nextwall >= 0 && (wall.nextwall >= m.walls.length || m.walls[wall.nextwall]?.nextwall !== index)) errors.push(`wall ${index}: invalid nextwall link`);
  });
  m.sprites.forEach((sprite, index) => {
    if (!m.sectors[sprite.sectnum]) errors.push(`sprite ${index}: invalid sector ${sprite.sectnum}`);
  });
  if (!m.sectors[m.start_sector]) errors.push(`player start: invalid sector ${m.start_sector}`);
  if (errors.length) toast(`Build check found ${errors.length} issue(s): ${errors.slice(0, 3).join("; ")}`, true);
  else toast(`Build map looks valid · ${m.sectors.length} sectors · ${m.walls.length} walls · ${m.sprites.length} sprites`);
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
  const tileName = dukeTileName(value);
  const thumbCanvas = el("canvas", { width: 40, height: 40, title: value >= 0 ? `Pick ART tile ${value}${tileName ? ` (${tileName})` : ""}` : "No tile" });
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
  const field = el("div", { class: "tex-field" }, thumbCanvas, numeric, editButton);
  if (tileName) field.append(el("span", { class: "duke-tile-name" }, tileName));
  return rowEl(label, field);
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
      } else if (DUKE_FLAGS[`${selection?.kind}.${key}`]) {
        fs.append(dukeField(key, obj, key, selection), dukeFlagsField(DUKE_FLAGS[`${selection.kind}.${key}`], obj, key, selection));
      } else {
        fs.append(dukeField(key, obj, key, selection));
        const hint = dukeTagHint(selection, obj, key);
        if (hint) fs.append(rowEl("", el("span", { class: "muted duke-hint" }, hint)));
      }
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
    title.textContent = `Build sprite ${sel.index}${dukeTileName(m.sprites[sel.index]?.picnum) ? ` · ${dukeTileName(m.sprites[sel.index].picnum)}` : ""}`;
    keys = ["x", "y", "z", "cstat", "picnum", "shade", "pal", "clipdist", "xrepeat", "yrepeat", "xoffset", "yoffset", "sectnum", "statnum", "ang", "owner", "xvel", "yvel", "zvel", "lotag", "hitag", "extra"];
  }
  if (!obj) { S.dukeSelection = null; renderDukeInspector(body, title); return; }
  const fs = fieldset(sel.kind[0].toUpperCase() + sel.kind.slice(1), obj, keys);
  if (sel.kind === "sector") {
    fs.append(el("button", { class: "btn small danger", onclick: () => deleteDukeSector(sel.index) }, "Delete sector"));
  }
  if (sel.kind === "sprite") {
    fs.append(el("div", { class: "actions" },
      el("button", { class: "btn small", onclick: () => dukeLookAt(obj.x, obj.y, obj.z) }, "View in 3D"),
      el("button", { class: "btn small", title: "Play this actor's CON actions and ART cycle; click a frame to paint it", onclick: () => dukeAnimations(obj.picnum) }, "Animations…"),
      el("button", { class: "btn small danger", onclick: () => deleteDukeSprite(sel.index) }, "Delete sprite")));
  }
  if (sel.kind === "wall") {
    const end = m.walls[obj.point2];
    fs.append(el("div", { class: "actions" },
      el("button", { class: "btn small", onclick: () => end && dukeSplitWall(sel.index, Math.round((obj.x + end.x) / 2), Math.round((obj.y + end.y) / 2)) }, "Split wall"),
      el("button", { class: "btn small", onclick: () => end && dukeLookAt((obj.x + end.x) / 2, (obj.y + end.y) / 2) }, "View in 3D"),
      el("button", { class: "btn small", onclick: () => dukeAnimations(obj.picnum) }, "Animations…")));
    const kind = dukeWallKind(m, sel.index);
    fs.prepend(el("p", { class: "duke-kind" }, el("span", { class: "swatch", style: `background:${kind.color}` }), kind.label));
  }
  if (sel.kind === "vertex") {
    const kind = dukeVertexKind(m, sel.x, sel.y);
    fs.prepend(el("p", { class: "duke-kind" }, el("span", { class: "swatch", style: `background:${kind.color}` }), kind.label));
  }
  if (sel.kind === "sector") {
    fs.append(el("div", { class: "actions" },
      el("button", { class: "btn small", onclick: () => dukeAnimations(obj.floorpicnum) }, "Floor animation…"),
      el("button", { class: "btn small", onclick: () => dukeAnimations(obj.ceilingpicnum) }, "Ceiling animation…")));
  }
  body.append(fs);
}

function addDukeSprite(x = S.dukeMap.start[0], y = S.dukeMap.start[1]) {
  if (!S.dukeMap) return;
  dukeSnapshot();
  const sector = dukeSectorAt(x, y);
  const sectnum = sector >= 0 ? sector : S.dukeMap.start_sector;
  const z = S.dukeMap.sectors[sectnum]?.floorz ?? S.dukeMap.start[2];
  const sprite = { x: snap(x), y: snap(y), z, cstat: 0, picnum: 1405,
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

function dukeCopySelection() {
  const selection = S.dukeSelection;
  if (selection?.kind !== "sprite") return false;
  const sprite = S.dukeMap?.sprites[selection.index];
  if (!sprite) return false;
  S.dukeClipboard = structuredClone(sprite);
  status("Copied Build sprite");
  return true;
}

function dukePasteSprite(atMouse = true) {
  if (!S.dukeMap || !S.dukeClipboard) return false;
  const old = S.dukeClipboard;
  const x = snap(atMouse && mouse.world ? mouse.world[0] : old.x + S.grid);
  const y = snap(atMouse && mouse.world ? mouse.world[1] : old.y + S.grid);
  dukeSnapshot();
  const sprite = structuredClone(old);
  sprite.x = x; sprite.y = y;
  const sector = dukeSectorAt(x, y);
  if (sector >= 0) { sprite.sectnum = sector; sprite.z = S.dukeMap.sectors[sector]?.floorz ?? sprite.z; }
  S.dukeMap.sprites.push(sprite);
  S.dukeSelection = { kind: "sprite", index: S.dukeMap.sprites.length - 1 };
  dukePersist(); renderInspector();
  return true;
}

function dukeTransformSelection(operation) {
  const selection = S.dukeSelection;
  if (!S.dukeMap || !selection) return false;
  if (selection.kind === "sprite") {
    const sprite = S.dukeMap.sprites[selection.index];
    if (!sprite) return false;
    dukeSnapshot();
    if (operation === "rotate-cw") sprite.ang = (sprite.ang + 512) & 2047;
    else if (operation === "rotate-ccw") sprite.ang = (sprite.ang + 1536) & 2047;
    else if (operation === "mirror-x") sprite.cstat ^= 4;
    else if (operation === "mirror-y") sprite.cstat ^= 8;
  } else if (selection.kind === "sector" && (operation === "rotate-cw" || operation === "rotate-ccw")) {
    const sector = S.dukeMap.sectors[selection.index];
    if (!sector) return false;
    const points = new Map();
    for (let i = sector.wallptr; i < sector.wallptr + sector.wallnum; i++) {
      const wall = S.dukeMap.walls[i];
      if (wall) points.set(`${wall.x},${wall.y}`, [wall.x, wall.y]);
    }
    if (!points.size) return false;
    const values = [...points.values()];
    const cx = values.reduce((sum, p) => sum + p[0], 0) / values.length;
    const cy = values.reduce((sum, p) => sum + p[1], 0) / values.length;
    const transformed = new Map();
    for (const [key, [x, y]] of points) {
      const dx = x - cx, dy = y - cy;
      transformed.set(key, operation === "rotate-cw" ? [Math.round(cx + dy), Math.round(cy - dx)] : [Math.round(cx - dy), Math.round(cy + dx)]);
    }
    dukeSnapshot();
    for (const wall of S.dukeMap.walls) {
      const point = transformed.get(`${wall.x},${wall.y}`);
      if (point) [wall.x, wall.y] = point;
    }
    for (const sprite of S.dukeMap.sprites) if (sprite.sectnum === selection.index) {
      const dx = sprite.x - cx, dy = sprite.y - cy;
      if (operation === "rotate-cw") { sprite.x = Math.round(cx + dy); sprite.y = Math.round(cy - dx); sprite.ang = (sprite.ang + 512) & 2047; }
      else { sprite.x = Math.round(cx - dy); sprite.y = Math.round(cy + dx); sprite.ang = (sprite.ang + 1536) & 2047; }
    }
  } else {
    return false;
  }
  dukePersist(); renderInspector();
  return true;
}

function syncDukeToolButtons() {
  if (!S.dukeMap) return;
  const kind = S.dukeSelection?.kind;
  $("#btn-copy").disabled = kind !== "sprite";
  $("#btn-paste").disabled = !S.dukeClipboard;
  $("#btn-rotate").disabled = kind !== "sprite" && kind !== "sector";
  $("#btn-mirror").disabled = kind !== "sprite";
}

function renderInspector() {
  const body = $("#inspector-body");
  const title = $("#inspector-title");
  body.replaceChildren();
  if (S.dukeMap) { renderDukeInspector(body, title); syncDukeToolButtons(); return; }
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
      ...((isHexen() || isUdmf()) ? [thingAction(objs)] : []),
      ...(isUdmf() ? [el("fieldset", {}, el("legend", {}, "UDMF flags"), fieldFor(objs, "ext", { flags: THING_EXT_FLAGS }))] : []),
      ...(isUdmf() ? [userFields(objs)].filter(Boolean) : []),
      advanced("All fields", objs, ["x", "y", "z", "angle", "type_id", "flags", "tid", "special", "args"]),
    );
    const hint = THING_TYPE_HINTS[objs[0].type_id];
    if (hint && objs.every((o) => o.type_id === objs[0].type_id)) body.querySelector("fieldset")?.append(el("p", { class: "muted", style: "margin:6px 0 0" }, hint));
  } else if (S.mode === "lines") {
    const objs = idx.map((i) => d.linedefs[i]);
    title.textContent = n > 1 ? `${n} lines` : `Line ${idx[0]}`;
    const actions = el("div", { class: "actions" },
      el("button", { class: "btn small", onclick: flipLines }, "Flip (F)"),
      el("button", { class: "btn small", onclick: splitLines }, "Split (X)"),
      el("button", { class: "btn small", title: "Align x offsets along connected walls with the same texture", onclick: () => tools.autoAlignSelection() }, "Auto-align (J)"),
      el("button", { class: "btn small", title: "Align x and y offsets: the texture runs on and its rows stay at the same heights", onclick: () => tools.autoAlignSelection(true) }, "Align X+Y (⇧J)"),
      ...(n === 2 ? [el("button", { class: "btn small", title: "Make the two selected lines a portal pair (Line_SetPortal)", onclick: makePortalPair }, "Make portal")] : []),
      el("button", { class: "btn small danger", onclick: deleteSelection }, "Delete"));
    body.append(actions);
    body.append(el("fieldset", {}, el("legend", {}, "Flags"), fieldFor(objs, "flags", { flags: isHexen() || isUdmf() ? HEXEN_LINE_FLAGS : DOOM_LINE_FLAGS })));
    const act = el("fieldset", {}, el("legend", {}, "Action"));
    act.append(rowEl("special", fieldFor(objs, "special", { options: isHexen() || isUdmf() ? HEXEN_LINE_SPECIALS : DOOM_LINE_SPECIALS })));
    act.append(rowEl("tag", fieldFor(objs, "tag")));
    if (isHexen() || isUdmf()) act.append(namedArgs(objs, objs[0].special) || rowEl("args", fieldFor(objs, "args")));
    if (isUdmf()) act.append(el("div", { style: "margin-top:4px" }, fieldFor(objs, "activation", { flags: SPAC_BITS })));
    if (isHexen()) {
      const actSel = el("select", {});
      const cur = (objs[0].flags >> 10) & 7;
      HEXEN_ACTIVATION.forEach((name, i) => actSel.append(el("option", { value: i, selected: i === cur }, name)));
      actSel.addEventListener("change", () => { snapshot(); objs.forEach((o) => { o.flags = (o.flags & ~0x1c00) | (Number(actSel.value) << 10); }); commit(); });
      act.append(rowEl("activation", actSel));
    }
    body.append(act);
    const portalLine = objs.some((l) => PORTAL_SPECIALS.has(l.special))
      || (n === 1 && S.portals && (S.portals.lines.some((p) => p.line === idx[0] || p.target === idx[0])
        || S.portals.polyobjects.some((po) => po.lines.includes(idx[0]))));
    if ((isHexen() || isUdmf()) && portalLine) body.append(el("fieldset", {}, el("legend", {}, "Portal"), portalInfo()));
    if (!isHexen() && !isUdmf()) body.append(udmfNotice());
    if (isUdmf()) {
      body.append(el("fieldset", {}, el("legend", {}, "UDMF flags"), fieldFor(objs, "ext", { flags: LINE_EXT_FLAGS })));
      const uf = userFields(objs);
      if (uf) body.append(uf);
    }
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
    if (extras) body.append(extras.linePanel(idx));
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
    if (extras) body.append(extras.sectorPanel(idx));
    body.append(fieldset("Light & action", objs, ["light", "special", "tag"], {
      fields: { light: { range: true, min: 0, max: 255 }, special: isHexen() || isUdmf() ? {} : { options: DOOM_SECTOR_SPECIALS } },
    }));
    const portalSector = n === 1 && S.portals && (S.portals.planes.some((p) => p.sector === idx[0]) || S.portals.groups[idx[0]]);
    if (isUdmf() || (isHexen() && portalSector)) {
      const fs = el("fieldset", {}, el("legend", {}, "Portals"), portalInfo());
      if (isUdmf()) {
        for (const plane of ["floor", "ceiling"]) {
          fs.append(rowEl(`${plane} portal`, fieldFor(objs, "ext", { flags: SECTOR_PORTAL_FLAGS[plane] })));
          fs.append(rowEl(`${plane} alpha`, fieldFor(objs, `${plane}_alpha`, { min: 0, max: 1 })));
        }
      }
      body.append(fs);
    }
    if (!isHexen() && !isUdmf()) body.append(udmfNotice());
    if (isUdmf()) {
      body.append(el("fieldset", {}, el("legend", {}, "UDMF flags"), fieldFor(objs, "ext", { flags: SECTOR_EXT_FLAGS })));
      const uf = userFields(objs);
      if (uf) body.append(uf);
    }
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
  const hidden = d && thingFilterOn() ? d.things.filter((t) => !thingVisible(t)).length : 0;
  $("#st-counts").textContent = d ? `${d.vertices.length} V · ${d.linedefs.length} L · ${d.sidedefs.length} SD · ${d.sectors.length} S · ${d.things.length} T${hidden ? ` (${hidden} hidden)` : ""}` : "";
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
    if (sm.duke) {
      const sprites = S.dukeMap?.sprites || [];
      body.append(
        el("p", { class: "muted", style: "padding:6px 10px;margin:0" },
          "Build sprites in this map. Select one to edit its position, ART tile, angle, tags, and flags; Insert adds one at the cursor."),
        el("div", { class: "actions", style: "padding:6px 10px" },
          el("button", { class: "btn small primary", onclick: () => addDukeSprite(mouse.world?.[0] ?? S.dukeMap?.start[0], mouse.world?.[1] ?? S.dukeMap?.start[1]) }, "+ Sprite")),
      );
      sprites.forEach((sprite, index) => {
        const tile = `BT${String(sprite.picnum).padStart(4, "0")}`;
        const text = `SPRITE ${String(index).padStart(3, "0")} · ${tile} · LOTAG ${sprite.lotag} · HITAG ${sprite.hitag}`;
        if (q && !text.includes(q) && !String(sprite.sectnum).includes(q)) return;
        body.append(el("div", {
          class: `list-item${S.dukeSelection?.kind === "sprite" && S.dukeSelection.index === index ? " active" : ""}`,
          onclick: () => {
            S.mode = "things";
            S.dukeSelection = { kind: "sprite", index };
            syncModeButtons(); renderInspector(); render2d();
          },
        }, el("span", { class: "name" }, `#${index} ${tile}`),
        el("span", { class: "meta" }, `sector ${sprite.sectnum}`)));
      });
      if (!sprites.length) body.append(el("p", { class: "muted", style: "padding:6px 10px" }, "No sprites in this map yet."));
      return;
    }
    // The picked type, shown large: what it looks like and what clicking does.
    const picked = sm.things.find((t) => t.num === S.thingType);
    if (picked) {
      const big = el("canvas", { class: "thing-preview-img", width: 128, height: 128 });
      if (picked.sprite) drawThumb(big, picked.sprite, 128);
      const edit = el("button", { class: "btn small", disabled: !picked.sprite,
        onclick: () => paint.open("sprites", picked.sprite, { num: picked.num, className: picked.name }) }, "✎ Edit sprite & sounds");
      body.append(el("div", { class: "thing-preview" }, big,
        el("div", { class: "thing-preview-info" },
          el("strong", {}, picked.title),
          el("div", { class: "sub" }, `${picked.name} · #${picked.num} · ${picked.cat}`),
          el("div", { class: "sub" }, picked.sprite ? `Sprite ${picked.sprite}` : "No sprite"),
          el("div", { class: "muted thing-preview-hint" }, S.mode === "things" ? "Click the map to place one; selected things change to this type." : "Things mode (T): click the map to place one."),
          edit)));
    } else {
      body.append(el("p", { class: "muted", style: "padding:6px 10px;margin:0" }, "Click a thing to see it and use it for placing."));
    }
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
          title: t.sprite ? `${t.title} — double-click to edit its sprite & sounds` : t.title,
          onclick: () => {
            S.thingType = t.num;
            if (S.mode === "things" && S.sel.size) { snapshot(); S.sel.forEach((i) => { S.doc.things[i].type_id = t.num; }); commit(); }
            renderBrowser();
          },
        }, icon, el("div", {}, el("div", {}, t.title), el("div", { class: "sub" }, t.name)), el("span", { class: "meta" }, t.num));
        // Double-click is the shortcut to the sprite editor.
        row.addEventListener("dblclick", (e) => { e.preventDefault(); openSprite(); });
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

const overrideNs = (l) => (["flats", "sprites", "patches", "textures", "acs"].includes(l.ns) ? l.ns : "global");
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

/** Argument names of line special `n` for ACS hints (inspector labels, else kinds). */
function specialArgNames(n) {
  const spec = SPECIAL_ARGS[n];
  if (spec) return spec.map((a) => (a ? String(a[0]).toLowerCase() : "arg"));
  const kinds = argKindsOf(n);
  return kinds ? [...kinds].map((c) => ({ s: "tag", t: "tid", l: "line id", p: "polyobj" }[c] || "arg")) : null;
}

/** Open an #include / #import file (path as written) and put the caret on `line`. */
function openAtLine(path, line, from) {
  const hit = findInclude(path, from);
  if (!hit) { toast(`${path} is not in the loaded files`, true); return; }
  openLump(hit);
  setTimeout(() => document.querySelector(`[data-lump-id="${hit.id}"]`)?._goTo?.(line), 0);
}

/**
 * Code editor for a text lump. `opts.map`: edit that map's ACS (`SCRIPTS`),
 * where saving compiles it into the map's `BEHAVIOR` instead of writing a lump.
 */
function textEditor(l, bytes, opts = {}) {
  const wrap = el("div", {});
  const text0 = new TextDecoder("utf-8").decode(bytes);
  const lang = opts.map ? "acs" : isZscriptLump(l) ? "zs" : textLang(l.name, l.path || l.name, text0);
  const acs = lang === "acs";
  const acsName = opts.map ? "SCRIPTS" : (l.path || l.name || "SCRIPTS");
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
      const builtin = acs && /(^|[\\/])z(?:common|defs|special|wvars)\.acs$/i.test(x.path) && !findInclude(x.path, l);
      if (builtin) {
        incBar.append(el("span", { class: "btn small", title: "Standard header: built into the ACS compiler" }, x.path));
        continue;
      }
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
  /** ACS compile result → status line, gutter mark, click-to-jump. */
  const showAcs = (r) => {
    errBox.replaceChildren();
    marks = new Map();
    if (r.ok) {
      const n = (k, w) => `${k} ${w}${k === 1 ? "" : "s"}`;
      errBox.append(el("div", { class: "diag-ok" }, `ACS OK — ${n(r.scripts, "script")}, ${n(r.functions, "function")}, ${r.bytes} bytes`));
    } else {
      const e = r.error || {};
      const here = !e.file || e.file.toLowerCase() === acsName.toLowerCase();
      if (here && e.line) marks.set(e.line, e.message);
      // An error in an #include / #import opens that file at the line.
      const jump = !e.line ? null : here ? () => goToLine(e.line) : () => openAtLine(e.file, e.line, l);
      errBox.append(el("div", { class: `diag error${jump ? " jump" : ""}`, title: here ? "Go to line" : `Open ${e.file} at line ${e.line}`,
        onclick: jump }, here ? `${e.line}  error: ${e.message}` : `${e.file}:${e.line}  error: ${e.message}`));
    }
    errBox.hidden = false;
    renderGutter();
  };
  const compileAcs = () => {
    try {
      const r = JSON.parse(S.mod.roomEditorCompileAcs(acsName, ta.value));
      showAcs(r);
      return r;
    } catch (e) { errBox.hidden = false; errBox.textContent = errText(e); return null; }
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
    if (opts.map) {
      try {
        const r = JSON.parse(S.mod.roomEditorSetMapAcs(opts.map, ta.value));
        showAcs(r);
        if (r.ok) {
          S.mapAcs[opts.map] = ta.value;
          S.acsGen++;
          // The map is now edited: Export writes it with its new BEHAVIOR.
          if (S.doc?.name === opts.map && !S.docs[opts.map]) S.docs[opts.map] = S.doc;
        }
        toast(r.ok ? `${opts.map} scripts compiled — level restarted` : `ACS error on line ${r.error?.line} — the map keeps its old scripts`, !r.ok);
      } catch (e) { errBox.hidden = false; errBox.textContent = errText(e); }
      return;
    }
    if (acs) {
      // Lumps are saved as typed; the check only reports.
      try {
        const err = overrideLump(l, new TextEncoder().encode(ta.value));
        const r = compileAcs();
        toast(err ? `Applied — ${err}` : r?.ok ? `${l.name} applied — ACS compiles` : `${l.name} applied — ACS has errors`, !!err || !r?.ok);
      } catch (e) { errBox.hidden = false; errBox.textContent = errText(e); }
      return;
    }
    try {
      const err = overrideLump(l, new TextEncoder().encode(ta.value));
      const r = zscript ? check() : null;
      showDiags(r, err);
      const bad = !!err || (r && !r.ok);
      toast(bad ? "Applied with errors" : `${l.name} applied — level reloaded`, bad);
    } catch (e) { errBox.hidden = false; errBox.textContent = errText(e); }
  };
  const where = l.arch && l.path ? ` Saved by path into the PK3 (${l.path}); Export PK3 writes it back.` : "";
  const note = opts.map
    ? `Compile & run builds the map's BEHAVIOR from this source and restarts the level; errors leave the old scripts in place. Exported with the map.`
    : zscript
      ? `ZScript — syntax colors as you type, Check finds errors (click one to jump), Ctrl/⌘+click an #include to open it.${where}`
      : acs
        ? `ACS — Check compiles it (click an error to jump). Map scripts are edited from the map's 📜 Scripts button.${where}`
        : `${DOOM_LANG_LABEL[lang]} — the level reloads with your changes.${where}`;
  const actions = el("div", { class: "actions" }, el("button", { class: "btn primary", onclick: save }, opts.map ? "Compile & run (Ctrl+S)" : "Apply & reload (Ctrl+S)"));
  if (zscript) actions.append(el("button", { class: "btn", title: "Parse this file, then compile the whole ZScript program with it", onclick: check }, "Check ZScript"));
  if (acs) actions.append(el("button", { class: "btn", title: "Compile without applying; errors are marked in the margin", onclick: compileAcs }, "Check ACS"));
  actions.append(el("span", { class: `kind text` }, DOOM_LANG_LABEL[lang]), el("span", { class: "muted" }, note));
  const area = el("div", { class: "code-area" }, hl, ta);
  wrap.dataset.lumpId = String(l.id ?? "");
  wrap._goTo = goToLine;
  let assistHint = null;
  let libRow = null;
  if (acs) {
    let builtin = null;
    const names = () => {
      if (!builtin) { try { builtin = JSON.parse(S.mod.roomEditorAcsNames()); } catch (_) { builtin = []; } }
      return [...acsLocalNames(ta.value), ...builtin];
    };
    assistHint = acsAssist(ta, area, names, specialArgNames).hint;
  }
  if (acs && !opts.map) {
    // `#library "NAME"` sources compile into acs/NAME (+ LOADACS).
    const auto = el("input", { type: "checkbox", checked: true });
    const libBtn = el("button", { class: "btn go", title: "Compile into the acs namespace as this library's lump" }, "Compile library");
    libRow = el("div", { class: "actions acs-lib" }, libBtn,
      el("label", { title: "Add the library to LOADACS so every map loads it" }, auto, " Load on every map (LOADACS)"));
    const libName = () => /^\s*#library\s+"([^"]+)"/im.exec(ta.value)?.[1] || null;
    const syncLib = () => {
      const n = libName();
      libRow.hidden = !n;
      if (n) libBtn.textContent = `Compile library ${n.toUpperCase()}`;
    };
    libBtn.addEventListener("click", () => {
      try {
        const r = JSON.parse(S.mod.roomEditorInstallAcsLibrary(acsName, ta.value, auto.checked));
        showAcs(r);
        if (!r.ok) { toast(`ACS error on line ${r.error?.line} — library not installed`, true); return; }
        setSummary(S.mod.roomEditorApplyLumps());
        S.acsGen++;
        toast(`Library ${r.library.toUpperCase()} compiled into acs/${r.library.toUpperCase()}${auto.checked ? " and added to LOADACS" : ""} — level restarted`);
      } catch (e) { errBox.hidden = false; errBox.textContent = errText(e); }
    });
    ta.addEventListener("input", syncLib);
    syncLib();
  }
  wrap.append(actions, ...(libRow ? [libRow] : []), incBar, el("div", { class: "code-wrap" }, gutter, area));
  if (assistHint) wrap.append(assistHint);
  wrap.append(errBox);
  return wrap;
}

/** The open map's ACS in a document tab: edit, compile, play-test. */
function openMapAcs() {
  if (!S.doc || S.dukeMap || S.summary?.duke) { toast("Open a Doom-engine map first", true); return; }
  const name = S.doc.name;
  const id = `acs:${name}`;
  if (S.lumpTabs.has(id)) { selectDocTab(id); return; }
  let info;
  try { info = JSON.parse(S.mod.roomEditorMapAcs()); } catch (e) { toast(errText(e), true); return; }
  const src = S.mapAcs[name] ?? (info.source || info.starter);
  const view = el("div", {});
  const status = info.scripts ? `${info.scripts} compiled script${info.scripts === 1 ? "" : "s"} in BEHAVIOR` : "No compiled scripts yet";
  view.append(el("div", { class: "doc-head" }, el("h2", {}, `${name} scripts`), el("span", { class: "kind text" }, "ACS"), el("span", { class: "muted" }, status)));
  // The document's own format: a conversion may not have reached the engine yet.
  if (String(S.doc.format || info.format).toLowerCase() === "doom") {
    view.append(el("div", { class: "acs-convert" },
      el("p", { class: "muted" }, "This is a Doom-format map: lines can't start scripts and the exported map has no BEHAVIOR. OPEN scripts still run while you play-test here."),
      el("button", { class: "btn go", onclick: () => {
        convertToUdmf();
        if (!isUdmf()) return;
        S.lumpTabs.delete(id);
        openMapAcs();
      } }, "Convert the map to UDMF (keeps the scripts)")));
  }
  view.append(textEditor({ name: "SCRIPTS", path: "SCRIPTS", kind: "text" }, new TextEncoder().encode(src), { map: name }));
  S.lumpTabs.set(id, { title: `${name} ACS`, view });
  selectDocTab(id);
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
    extras?.onExported();
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
    extras?.onExported();
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

// ---------------------------------------------------------------- map generator

function genOptions() {
  return {
    seed: Math.max(0, Math.floor(Number($("#gen-seed").value) || 0)),
    name: ($("#gen-name").value || "MAP01").trim().toUpperCase().slice(0, 8),
    players: Number($("#gen-players").value) || 8,
    size: $("#gen-size").value,
    theme: $("#gen-theme").value,
    biomes: $("#gen-biomes").value,
    verticality: Number($("#gen-vert").value),
    cover: Number($("#gen-cover").value),
    outdoor: Number($("#gen-outdoor").value),
    teleporters: $("#gen-tele").checked,
    liquids: $("#gen-liquids").checked,
    power_items: $("#gen-power").checked,
    monsters: Number($("#gen-monsters").value),
  };
}

function genReport(r) {
  const row = (k, v) => `<dt>${k}</dt><dd>${v}</dd>`;
  const extent = `${(r.width / 1024).toFixed(1)} × ${(r.height / 1024).toFixed(1)}k units`;
  return `<dl>${row("Rooms", r.rooms)}${row("Corridors", `${r.corridors} (${r.loops} loops)`)}` +
    `${row("Teleporters", r.teleporters ? `${r.teleporters} pairs` : "none")}${row("DM starts", r.dm_starts)}` +
    `${row("Weapons", r.weapons)}${row("Items", r.items)}` +
    (r.monsters ? row("Monsters", r.monsters) : "") + (r.secrets ? row("Secrets", r.secrets) : "") +
    `${row("Size", extent)}` +
    `${row("Biomes", genBiomes(r.biomes || []))}` +
    `${row("Geometry", `${r.sectors} sectors · ${r.lines} lines`)}</dl>` +
    (r.attempts > 1 ? `<div>Seed ${r.seed} (attempt ${r.attempts})</div>` : "");
}

function genBiomes(list) {
  const names = { indoor: "interior", yard: "yard", cave: "cave", canyon: "canyon", swamp: "swamp", courtyard: "keep", plaza: "plaza" };
  const n = {};
  for (const b of list) n[b] = (n[b] || 0) + 1;
  return Object.entries(n).sort((a, b) => b[1] - a[1]).map(([b, c]) => `${c} ${names[b] || b}`).join(", ");
}

let genBusy = false;
function generateMap() {
  if (!S.summary || S.summary.duke || genBusy) return;
  const opts = genOptions();
  if (!/^[A-Z0-9_]{1,8}$/.test(opts.name)) { toast("Map name: 1–8 letters, digits or _", true); return; }
  const mine = S.docs[opts.name] && S.docs[opts.name].generated;
  if (!mine && (S.summary.maps.includes(opts.name) || S.docs[opts.name])
      && !confirm(`${opts.name} already exists. Replace it with a generated map?`)) return;
  genBusy = true;
  $("#gen-go").disabled = true;
  $("#gen-report").textContent = "Generating…";
  // Let the panel repaint before the (synchronous) generation.
  requestAnimationFrame(() => setTimeout(() => {
    try {
      if (S.doc) S.docs[S.doc.name] = S.doc;
      const out = JSON.parse(S.mod.roomEditorGenerateMap(JSON.stringify(opts)));
      S.doc = out.doc;
      S.doc.generated = opts;
      S.docs[S.doc.name] = S.doc;
      S.undo = []; S.redo = []; S.sel.clear(); S.version++;
      setSummary(S.mod.roomEditorSummary());
      $("#map-select").value = S.doc.name;
      fitView(); renderInspector(); renderBrowser();
      $("#gen-report").innerHTML = genReport(out.report);
      toast(`${S.doc.name} generated: ${out.report.rooms} rooms, ${out.report.dm_starts} starts`);
    } catch (e) {
      $("#gen-report").innerHTML = `<span class="gen-err"></span>`;
      $("#gen-report .gen-err").textContent = errText(e);
    } finally {
      genBusy = false;
      $("#gen-go").disabled = false;
    }
  }, 0));
}

function openGenerator(open = true) {
  $("#gen-panel").hidden = !open;
  if (open) {
    const cur = S.doc && S.doc.generated;
    if (cur) {
      $("#gen-name").value = S.doc.name;
    } else if (S.summary) {
      let n = 1;
      while (S.summary.maps.includes(`MAP${String(n).padStart(2, "0")}`) || S.docs[`MAP${String(n).padStart(2, "0")}`]) n++;
      $("#gen-name").value = n <= 99 ? `MAP${String(n).padStart(2, "0")}` : "GEN01";
    }
    $("#gen-seed").focus();
  }
}

$("#btn-generate").addEventListener("click", () => openGenerator($("#gen-panel").hidden));
$("#gen-close").addEventListener("click", () => openGenerator(false));
$("#gen-form").addEventListener("submit", (e) => { e.preventDefault(); generateMap(); });
$("#gen-dice").addEventListener("click", () => { $("#gen-seed").value = Math.floor(Math.random() * 1e9); });
$("#gen-reroll").addEventListener("click", () => {
  $("#gen-seed").value = Math.floor(Math.random() * 1e9);
  generateMap();
});
$("#gen-players").addEventListener("input", (e) => { $("#gen-players-out").textContent = e.target.value; });
$("#gen-panel").addEventListener("keydown", (e) => {
  e.stopPropagation();
  if (e.key === "Escape") openGenerator(false);
});
$("#gen-seed").value = Math.floor(Math.random() * 1e6);

// ---------------------------------------------------------------- toolbar & keys

// View › Things on the map.
renderThingFilter();
$("#tf-skill").addEventListener("change", (e) => { thingFilter.skill = e.target.value; thingFilterChanged(); });
$("#tf-mode").addEventListener("change", (e) => { thingFilter.mode = e.target.value; thingFilterChanged(); });
$("#tf-reset").addEventListener("click", () => {
  thingFilter.hide.clear(); thingFilter.skill = "all"; thingFilter.mode = "all";
  renderThingFilter(); thingFilterChanged();
});

// Header menus (File / Tools / View): one open at a time; choosing an item,
// clicking elsewhere or Escape closes them; hovering another title while one
// is open switches to it. Checkboxes keep the menu open.
const menus = [...document.querySelectorAll("#toolbar details.menu")];
const closeMenus = (except) => menus.forEach((m) => { if (m !== except) m.open = false; });
for (const m of menus) {
  m.addEventListener("toggle", () => { if (m.open) closeMenus(m); });
  m.querySelector("summary").addEventListener("pointerenter", () => {
    if (!m.open && menus.some((o) => o.open)) m.open = true;
  });
  m.querySelector(".menu-pop").addEventListener("click", (e) => {
    const item = e.target.closest(".btn");
    // Later, so a label still opens its file picker.
    if (item && !item.closest(".menu-row")) setTimeout(() => { m.open = false; });
  });
}
document.addEventListener("pointerdown", (e) => { if (!e.target.closest("#toolbar details.menu")) closeMenus(); });
document.addEventListener("keydown", (e) => {
  if (e.key === "Escape" && menus.some((m) => m.open)) { closeMenus(); e.stopPropagation(); }
}, true);

function syncModeButtons() {
  document.querySelectorAll("#modes .btn").forEach((b) => b.classList.toggle("active", b.dataset.mode === S.mode));
  $("#draw-opts").hidden = S.mode !== "draw" || !!S.dukeMap;
  setHint();
}
function setMode(m) {
  if (S.mode === m) return;
  if (S.mode === "draw" && S.drawPts.length) S.drawPts = [];
  S.mode = m;
  S.sel.clear();
  extras?.reset();
  if (S.dukeMap) S.dukeSelection = null;
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
$("#show-portals").addEventListener("change", (e) => { S.showPortals = e.target.checked; render2d(); });
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
  if (mod && e.code === "KeyG" && S.summary && !S.summary.duke) { e.preventDefault(); openGenerator($("#gen-panel").hidden); return; }
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
    if (dukePreviewKey(e)) return;
    if (mod && e.code === "KeyC") { e.preventDefault(); dukeCopySelection(); return; }
    if (mod && e.code === "KeyV") { e.preventDefault(); dukePasteSprite(); return; }
    if (mod && e.code === "KeyD") { e.preventDefault(); if (dukeCopySelection()) dukePasteSprite(false); return; }
    if (!mod && e.code === "KeyR") { e.preventDefault(); dukeTransformSelection(e.shiftKey ? "rotate-ccw" : "rotate-cw"); return; }
    if (!mod && e.code === "KeyH") { e.preventDefault(); dukeTransformSelection(e.shiftKey ? "mirror-y" : "mirror-x"); return; }
    if (!mod && e.code === "F8") { e.preventDefault(); dukeCheckMap(); return; }
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
  if (extras?.onKey(e, { mod })) return;
  if (document.activeElement === room) {
    if (!S.playing && key3d(e)) e.preventDefault();
    if (!S.playing && (e.code === "Delete" || e.code === "Backspace") && S.mode === "things") { deleteSelection(); e.preventDefault(); }
    return;
  }
  if (mod && e.code === "KeyA") {
    e.preventDefault();
    const n = { vertices: S.doc.vertices, lines: S.doc.linedefs, sectors: S.doc.sectors, things: S.doc.things }[S.mode]?.length || 0;
    S.sel = new Set(Array.from({ length: n }, (_, i) => i).filter((i) => S.mode !== "things" || thingVisible(S.doc.things[i])));
    renderInspector(); render2d();
    return;
  }
  const modeKeys = { KeyV: "vertices", KeyL: "lines", KeyS: "sectors", KeyT: "things", KeyD: "draw", KeyK: "cut", KeyQ: "measure" };
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
// Leaving with unexported work asks first: see editor-extras.js (it knows what was exported).
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
extras = installExtras({
  S, $, el, toast, status, errText, snapshot, commit, render2d, renderInspector, snap, clampI16, toScreen, mouse,
  tools, finishDraw, loadMap, setSummary, setLumpData,
  thingColor: (type) => CAT_COLORS[S.thingInfo.get(type)?.cat] || "#ff00ff",
});
for (const [id, fn] of [["#btn-find", extras.openFind], ["#btn-prefabs", extras.openPrefabs], ["#btn-trace", extras.openTrace], ["#btn-acs", openMapAcs]]) {
  $(id).addEventListener("click", () => { if (S.doc) fn(); else toast("Open a map first"); });
}
$("#draw-shape").addEventListener("change", setHint);
$("#btn-snapshots").addEventListener("click", () => { if (S.summary && !S.summary.duke) extras.openSnapshots(); else toast("Open a Doom game first"); });
// Console scripting hook: edit `roomEditor.state.doc`, then `roomEditor.commit()`.
window.roomEditor = { state: S, tools, paint, scripts, extras, snapshot, commit, undo, redo, loadMap, openMapAcs };
$("#btn-check").addEventListener("click", () => S.dukeMap ? dukeCheckMap() : tools.toggleChecker());
$("#btn-composer").addEventListener("click", () => tools.openComposer());
$("#btn-rotate").addEventListener("click", (e) => S.dukeMap
  ? dukeTransformSelection(e.shiftKey ? "rotate-ccw" : "rotate-cw")
  : tools.transform(e.shiftKey ? "ccw" : "cw"));
$("#btn-mirror").addEventListener("click", (e) => S.dukeMap
  ? dukeTransformSelection(e.shiftKey ? "mirror-y" : "mirror-x")
  : tools.transform(e.shiftKey ? "mirrorY" : "mirrorX"));
$("#btn-copy").addEventListener("click", () => S.dukeMap
  ? dukeCopySelection()
  : tools.copy() && status(`Copied ${S.sel.size} ${S.mode}`));
$("#btn-paste").addEventListener("click", () => S.dukeMap ? dukePasteSprite() : tools.paste());

setHint();
setPlaying(false);
renderDocTabs();
renderInspector();
boot().then(() => extras.noteAutosave()).catch((e) => { status(`Engine failed: ${errText(e)}`); console.error(e); });
pollCamera();

// ------------------------------------------------ native Build helpers

/** Bit labels for Build `cstat` / plane stat fields. */
const DUKE_FLAGS = {
  "wall.cstat": [[1, "Blocking"], [2, "Swap bottoms"], [4, "Align bottom"], [8, "X-flip"], [16, "Masked"], [32, "One-way"],
    [64, "Hitscan"], [128, "Translucent"], [256, "Y-flip"], [512, "Reverse transl."]],
  "sprite.cstat": [[1, "Blocking"], [2, "Translucent"], [4, "X-flip"], [8, "Y-flip"], [16, "Wall-aligned"], [32, "Floor-aligned"],
    [64, "One-sided"], [128, "Centered"], [256, "Hitscan"], [512, "Reverse transl."], [32768, "Invisible"]],
  "sector.ceilingstat": [[1, "Parallax sky"], [2, "Sloped"], [4, "Swap XY"], [8, "Double smoosh"], [16, "X-flip"], [32, "Y-flip"],
    [64, "Align to wall"], [128, "Masked"], [256, "Translucent"]],
  "sector.floorstat": [[1, "Parallax sky"], [2, "Sloped"], [4, "Swap XY"], [8, "Double smoosh"], [16, "X-flip"], [32, "Y-flip"],
    [64, "Align to wall"], [128, "Masked"], [256, "Translucent"]],
};

/** Atomic sector-effector (SE) lotags. */
const DUKE_SE_TYPES = {
  0: "Rotating sector", 1: "Rotation pivot", 2: "Earthquake", 3: "Lights on after shot out", 4: "Random flashing lights",
  6: "Subway engine", 7: "Teleporter", 8: "Door lights (up-open)", 9: "Door lights (down-open)", 10: "Door auto-close timer",
  11: "Swinging door", 12: "Light switch", 13: "C-9 explosive", 14: "Subway car", 15: "Sliding door (ST 25)",
  16: "Reactor", 17: "Transport elevator", 18: "Incremental rise/fall", 19: "Shields (explosion lowers ceiling)",
  20: "Stretch / bridge (ST 27)", 21: "Drop floor (ST 28)", 22: "Teeth door prong (ST 29)", 24: "Conveyor belt",
  25: "Piston", 26: "Escalator", 27: "Demo camera", 28: "Lightning", 29: "Wave floor", 30: "Two-way train (ST 31)",
  31: "True-drop floor", 32: "True-drop ceiling", 33: "Earthquake debris", 34: "Wall/floor scroller", 35: "Ceiling crusher",
  36: "Shooter", 128: "Glass breakage", 130: "Explosion", 131: "Big explosion",
};

/** Atomic sector lotags (low byte). */
const DUKE_SECTOR_TYPES = {
  1: "Above water (dive down)", 2: "Underwater", 9: "Star Trek sliding door", 15: "Elevator transport",
  16: "Platform down", 17: "Platform up", 18: "Elevator down", 19: "Elevator up", 20: "Ceiling door", 21: "Floor door",
  22: "Splitting door", 23: "Swinging door", 25: "Sliding door", 26: "Splitting Star Trek door", 27: "Stretch / bridge",
  28: "Drop floor", 29: "Teeth door", 30: "Rotate-rise bridge", 31: "Two-way train", 32767: "Secret place", 65535: "End of level",
};

function dukeTileName(picnum) {
  if (!(picnum >= 0) || !S.dukeTileNames) return "";
  return S.dukeTileNames[picnum] || "";
}

function dukeFlagsField(bits, obj, key, selection) {
  const value = obj[key] | 0;
  const box = el("div", { class: "duke-flags" });
  for (const [bit, label] of bits) {
    const input = el("input", { type: "checkbox" });
    input.checked = (value & bit) !== 0;
    input.addEventListener("change", () => {
      const next = input.checked ? ((obj[key] | 0) | bit) : ((obj[key] | 0) & ~bit);
      dukeEditField(obj, key, next, selection);
    });
    box.append(el("label", { title: `${key} & ${bit}` }, input, label));
  }
  return rowEl("", box);
}

function dukeTagHint(selection, obj, key) {
  if (!selection) return "";
  if (selection.kind === "sprite") {
    if (obj.picnum === 1 && key === "lotag") return DUKE_SE_TYPES[obj.lotag] ? `SE ${obj.lotag}: ${DUKE_SE_TYPES[obj.lotag]}` : "";
    if (obj.picnum === 1 && key === "hitag") return "SE channel (links effectors, switches and activators)";
    if (key === "lotag" && [2, 4].includes(obj.picnum)) return "Activator channel (matches switch/SE lotag)";
    if (key === "lotag" && obj.picnum === 5) return "MUSICANDSFX: sound number";
    if (key === "picnum") return dukeTileName(obj.picnum);
    return "";
  }
  if (selection.kind === "sector" && key === "lotag") {
    const low = obj.lotag & 0xffff;
    return DUKE_SECTOR_TYPES[low] || DUKE_SECTOR_TYPES[low & 0xff] || "";
  }
  if (selection.kind === "wall" && key === "lotag" && obj.lotag) return "Switch / door channel";
  return "";
}

/** Insert a copy of wall `after` at `after + 1`, starting at (x, y). */
function dukeInsertWall(m, after, x, y) {
  const at = after + 1;
  for (const w of m.walls) {
    if (w.point2 >= at) w.point2++;
    if (w.nextwall >= at) w.nextwall++;
  }
  for (const sec of m.sectors) {
    if (sec.wallptr >= at) sec.wallptr++;
    else if (sec.wallptr <= after && after < sec.wallptr + sec.wallnum) sec.wallnum++;
  }
  const src = m.walls[after];
  const added = { ...src, x, y, point2: src.point2 };
  src.point2 = at;
  m.walls.splice(at, 0, added);
  return at;
}

/** Split wall `index` at the point nearest (x, y); both sides of a portal split together. */
function dukeSplitWall(index, x, y) {
  const m = S.dukeMap;
  const w = m?.walls[index];
  const end = m?.walls[w?.point2];
  if (!w || !end) return;
  const dx = end.x - w.x, dy = end.y - w.y;
  const len2 = dx * dx + dy * dy;
  if (!len2) return;
  const t = Math.max(0, Math.min(1, ((x - w.x) * dx + (y - w.y) * dy) / len2));
  const px = Math.round(w.x + dx * t), py = Math.round(w.y + dy * t);
  if ((px === w.x && py === w.y) || (px === end.x && py === end.y)) { toast("Pick a point inside the wall to split it"); return; }
  dukeSnapshot();
  let first = index;
  let firstNew = dukeInsertWall(m, first, px, py);
  const other = m.walls[first].nextwall;
  if (other >= 0) {
    const otherNew = dukeInsertWall(m, other, px, py);
    if (first >= otherNew) first++;
    if (firstNew >= otherNew) firstNew++;
    // A→P pairs with P→A, P→B pairs with B→P.
    m.walls[first].nextwall = otherNew; m.walls[otherNew].nextwall = first;
    m.walls[firstNew].nextwall = other; m.walls[other].nextwall = firstNew;
  }
  S.mode !== "vertices" && setMode("vertices");
  S.dukeSelection = { kind: "vertex", index: firstNew, x: px, y: py };
  dukePersist();
  renderInspector();
  status(`Split wall ${index} at (${px}, ${py})`);
}

// ------------------------------------------------ native Duke 3D preview

const dukePreview = $("#duke-preview");
const dukePv = { cam: null, dirty: true, raf: 0, look: false, keys: new Set(), lastT: 0, broken: false };

function dukePreviewActive() {
  return !!(S.dukeMap && S.mod?.roomEditorDukePreview && !S.dukeTesting && !$("#map-view")?.classList.contains("only2d"));
}

function dukeCamFloor(x, y, fallback) {
  const sector = dukeSectorAt(x, y);
  const sec = S.dukeMap?.sectors[sector];
  return sec ? sec.floorz - (40 << 8) : fallback;
}

function dukePreviewMapLoaded() {
  if (!S.dukeTileNames && S.mod?.roomEditorDukeTileNames) {
    try { S.dukeTileNames = JSON.parse(S.mod.roomEditorDukeTileNames()); } catch { S.dukeTileNames = {}; }
  }
  const m = S.dukeMap;
  dukePv.cam = { x: m.start[0], y: m.start[1], z: dukeCamFloor(m.start[0], m.start[1], m.start[2]), ang: m.start_ang & 2047, horiz: 100 };
  dukePv.broken = false;
  dukePreviewInvalidate();
}

function dukePreviewInvalidate() {
  dukePv.dirty = true;
  dukePreviewKick();
}

function dukePreviewKick() {
  if (!dukePv.raf) dukePv.raf = requestAnimationFrame(dukePreviewFrame);
}

function dukePreviewFrame(t) {
  dukePv.raf = 0;
  if (!dukePreviewActive() || dukePv.broken) return;
  if (!dukePv.cam) dukePreviewMapLoaded();
  const dt = dukePv.lastT ? Math.min(0.1, (t - dukePv.lastT) / 1000) : 0;
  dukePv.lastT = t;
  const moved = dukePreviewMove(dt);
  if (moved || dukePv.dirty) {
    dukePv.dirty = false;
    dukePreviewDraw();
    if (moved) render2d();
  }
  if (dukePv.keys.size || dukePv.look) dukePreviewKick();
  else dukePv.lastT = 0;
}

function dukePreviewMove(dt) {
  if (!dt || !dukePv.keys.size || document.activeElement !== dukePreview) return false;
  const c = dukePv.cam, k = dukePv.keys;
  const fast = k.has("ShiftLeft") || k.has("ShiftRight") ? 3 : 1;
  const a = c.ang * Math.PI / 1024, cos = Math.cos(a), sin = Math.sin(a);
  const step = 1400 * fast * dt;
  let fx = 0, sx = 0;
  if (k.has("KeyW") || k.has("ArrowUp")) fx += 1;
  if (k.has("KeyS") || k.has("ArrowDown")) fx -= 1;
  if (k.has("KeyD")) sx += 1;
  if (k.has("KeyA")) sx -= 1;
  c.x += (cos * fx - sin * sx) * step;
  c.y += (sin * fx + cos * sx) * step;
  if (k.has("KeyQ") || k.has("PageUp")) c.z -= 14000 * fast * dt;
  if (k.has("KeyZ") || k.has("PageDown")) c.z += 14000 * fast * dt;
  if (k.has("ArrowLeft")) c.ang = (c.ang - Math.round(900 * dt) + 2048) & 2047;
  if (k.has("ArrowRight")) c.ang = (c.ang + Math.round(900 * dt)) & 2047;
  return true;
}

function dukePreviewSize() {
  const wrap = $("#view3d-wrap");
  const W = Math.max(16, wrap.clientWidth), H = Math.max(16, wrap.clientHeight);
  // The software renderer runs on the CPU: cap the internal resolution.
  const scale = Math.min(1, 720 / W, 480 / H);
  return [Math.max(16, Math.round(W * scale)), Math.max(16, Math.round(H * scale))];
}

function dukePreviewDraw() {
  const [w, h] = dukePreviewSize();
  if (dukePreview.width !== w || dukePreview.height !== h) { dukePreview.width = w; dukePreview.height = h; }
  const c = dukePv.cam;
  const tic = Math.floor(performance.now() / (1000 / 30));
  let rgba;
  try {
    rgba = S.mod.roomEditorDukePreview(S.doc.name, Math.round(c.x), Math.round(c.y), Math.round(c.z), c.ang, Math.round(c.horiz), w, h, tic);
  } catch (e) {
    dukePv.broken = true;
    status(`3D preview: ${errText(e)}`);
    return;
  }
  if (rgba.length !== w * h * 4) return;
  const g = dukePreview.getContext("2d");
  g.putImageData(new ImageData(new Uint8ClampedArray(rgba.buffer, rgba.byteOffset, rgba.length), w, h), 0, 0);
  // Center cross for aiming picks.
  g.strokeStyle = "rgba(255,204,51,.7)"; g.lineWidth = 1;
  g.beginPath(); g.moveTo(w / 2 - 4, h / 2); g.lineTo(w / 2 + 4, h / 2); g.moveTo(w / 2, h / 2 - 4); g.lineTo(w / 2, h / 2 + 4); g.stroke();
}

function dukeDrawPreviewCamera(g) {
  const c = dukePv.cam;
  if (!c || !dukePreviewActive()) return;
  const [x, y] = dukeToScreen(c.x, c.y);
  const a = c.ang * Math.PI / 1024;
  const fov = Math.PI / 4;
  g.fillStyle = "rgba(89,214,255,.18)"; g.strokeStyle = "#59d6ff"; g.lineWidth = 1.5;
  g.beginPath(); g.moveTo(x, y);
  g.lineTo(x + Math.cos(a - fov) * 34, y + Math.sin(a - fov) * 34);
  g.lineTo(x + Math.cos(a + fov) * 34, y + Math.sin(a + fov) * 34);
  g.closePath(); g.fill(); g.stroke();
  g.beginPath(); g.arc(x, y, 4, 0, Math.PI * 2); g.fillStyle = "#59d6ff"; g.fill();
}

function dukePlaceCamera(wx, wy) {
  if (!dukePv.cam) return;
  if (dukeSectorAt(wx, wy) < 0) { toast("Put the 3D camera inside a sector"); return; }
  dukePv.cam.x = wx; dukePv.cam.y = wy;
  dukePv.cam.z = dukeCamFloor(wx, wy, dukePv.cam.z);
  dukePreviewInvalidate(); render2d();
}

/** Aim the 3D camera at a point from a nearby spot inside the map. */
function dukeLookAt(tx, ty, tz) {
  const c = dukePv.cam;
  if (!c) return;
  if ($("#map-view").classList.contains("only2d")) applyLayout("split");
  for (const turn of [0, 512, 1024, 1536, 256, 768, 1280, 1792]) {
    const ang = (c.ang + turn) & 2047;
    const a = ang * Math.PI / 1024;
    for (const dist of [900, 600, 320]) {
      const x = tx - Math.cos(a) * dist, y = ty - Math.sin(a) * dist;
      if (dukeSectorAt(x, y) < 0) continue;
      c.x = x; c.y = y; c.ang = ang; c.horiz = 100;
      c.z = Number.isFinite(tz) ? tz - (24 << 8) : dukeCamFloor(x, y, c.z);
      dukePreviewInvalidate(); render2d();
      return;
    }
  }
  dukePlaceCamera(tx, ty);
}

function dukePreviewPick(e) {
  const r = dukePreview.getBoundingClientRect();
  const nx = ((e.clientX - r.left) / r.width) * 2 - 1;
  const ny = 1 - ((e.clientY - r.top) / r.height) * 2;
  const c = dukePv.cam;
  let hit;
  try {
    const raw = S.mod.roomEditorDukePick(S.doc.name, Math.round(c.x), Math.round(c.y), Math.round(c.z), c.ang, Math.round(c.horiz), nx, ny);
    hit = raw ? JSON.parse(raw) : null;
  } catch (err) { toast(errText(err), true); return; }
  if (!hit) { S.dukeSelection = null; renderInspector(); render2d(); return; }
  const mode = { wall: "lines", sector: "sectors", sprite: "things" }[hit.kind];
  if (mode && S.mode !== mode) setMode(mode);
  S.dukeSelection = { kind: hit.kind, index: hit.index };
  // Keep the picked object visible in the 2D view.
  const [sx, sy] = dukeToScreen(hit.x, hit.y);
  if (sx < 0 || sy < 0 || sx > cv.clientWidth || sy > cv.clientHeight) { S.view.x = hit.x; S.view.y = hit.y; }
  renderInspector(); render2d();
  status(`Picked ${hit.kind} ${hit.index}${hit.kind === "sprite" && dukeTileName(S.dukeMap.sprites[hit.index]?.picnum) ? ` (${dukeTileName(S.dukeMap.sprites[hit.index].picnum)})` : ""}`);
}

dukePreview.addEventListener("contextmenu", (e) => e.preventDefault());
dukePreview.addEventListener("mousedown", (e) => {
  dukePreview.focus();
  if (e.button === 2) {
    dukePv.look = true;
    dukePreview.requestPointerLock?.();
  } else if (e.button === 0) dukePreviewPick(e);
});
window.addEventListener("mouseup", (e) => {
  if (e.button === 2 && dukePv.look) {
    dukePv.look = false;
    if (document.pointerLockElement === dukePreview) document.exitPointerLock();
  }
});
window.addEventListener("mousemove", (e) => {
  if (!dukePv.look || !dukePv.cam) return;
  dukePv.cam.ang = (dukePv.cam.ang + Math.round(e.movementX * 2) + 2048) & 2047;
  dukePv.cam.horiz = Math.max(-99, Math.min(299, dukePv.cam.horiz - e.movementY * 0.6));
  dukePreviewInvalidate();
  render2d();
});
dukePreview.addEventListener("wheel", (e) => {
  e.preventDefault();
  if (!dukePv.cam) return;
  const a = dukePv.cam.ang * Math.PI / 1024, d = -Math.sign(e.deltaY) * 256;
  dukePv.cam.x += Math.cos(a) * d; dukePv.cam.y += Math.sin(a) * d;
  dukePreviewInvalidate(); render2d();
}, { passive: false });
// Touch: one finger looks around, two fingers move forward/back.
{
  let touch = null;
  dukePreview.addEventListener("touchstart", (e) => {
    const t = e.touches;
    touch = { x: t[0].clientX, y: t[0].clientY, n: t.length, moved: false, d: t.length > 1 ? Math.hypot(t[0].clientX - t[1].clientX, t[0].clientY - t[1].clientY) : 0 };
  }, { passive: true });
  dukePreview.addEventListener("touchmove", (e) => {
    if (!touch || !dukePv.cam) return;
    e.preventDefault();
    const t = e.touches;
    if (t.length > 1) {
      const d = Math.hypot(t[0].clientX - t[1].clientX, t[0].clientY - t[1].clientY);
      const a = dukePv.cam.ang * Math.PI / 1024, step = (d - touch.d) * 6;
      dukePv.cam.x += Math.cos(a) * step; dukePv.cam.y += Math.sin(a) * step;
      touch.d = d;
    } else {
      dukePv.cam.ang = (dukePv.cam.ang + Math.round((t[0].clientX - touch.x) * 3) + 2048) & 2047;
      dukePv.cam.horiz = Math.max(-99, Math.min(299, dukePv.cam.horiz - (t[0].clientY - touch.y)));
      touch.x = t[0].clientX; touch.y = t[0].clientY;
    }
    touch.moved = true;
    dukePreviewInvalidate(); render2d();
  }, { passive: false });
  dukePreview.addEventListener("touchend", (e) => {
    if (touch && !touch.moved && touch.n === 1 && e.changedTouches[0]) dukePreviewPick(e.changedTouches[0]);
    touch = null;
  });
}
window.addEventListener("keyup", (e) => { dukePv.keys.delete(e.code); });
window.addEventListener("blur", () => dukePv.keys.clear());

/** Keys while the native preview has focus; returns true when consumed. */
function dukePreviewKey(e) {
  if (document.activeElement !== dukePreview || !S.dukeMap) return false;
  const moveKeys = ["KeyW", "KeyA", "KeyS", "KeyD", "KeyQ", "KeyZ", "ArrowUp", "ArrowDown", "ArrowLeft", "ArrowRight", "PageUp", "PageDown", "ShiftLeft", "ShiftRight"];
  if (moveKeys.includes(e.code) && !e.ctrlKey && !e.metaKey) {
    dukePv.keys.add(e.code);
    dukePreviewKick();
    e.preventDefault();
    return true;
  }
  if (e.code === "Home") { dukePreviewMapLoaded(); e.preventDefault(); return true; }
  return false;
}

new ResizeObserver(() => { if (dukePreviewActive()) dukePreviewInvalidate(); }).observe($("#view3d-wrap"));
// Animated ART tiles (screens, water, lights) keep moving while idle.
setInterval(() => {
  if (dukePreviewActive() && !document.hidden && !dukePv.keys.size && !dukePv.look) dukePreviewInvalidate();
}, 125);
