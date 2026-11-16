/**
 * Global Settings menu for the web client (solo + multiplayer).
 * Persists to localStorage (`room/web-settings`) and applies via roomApplySettings.
 */
const STORAGE_KEY = "room/web-settings";

const ACTIONS = [
  { id: "w", label: "Forward" },
  { id: "s", label: "Back" },
  { id: "a", label: "Strafe left" },
  { id: "d", label: "Strafe right" },
  { id: "left", label: "Turn left" },
  { id: "right", label: "Turn right" },
  { id: "shift", label: "Run" },
  { id: "use", label: "Use / open" },
  { id: "attack", label: "Attack" },
  { id: "up", label: "Swim / fly up" },
  { id: "down", label: "Swim / fly down" },
  { id: "kick", label: "Quick kick" },
  { id: "invuse", label: "Use inventory" },
  { id: "scores", label: "Scoreboard" },
  { id: "enter", label: "Confirm / chat" },
  { id: "grid", label: "Spectate grid" },
];

const RES_PRESETS = [
  { label: "Native", cap: 0 },
  { label: "High (1920)", cap: 1920 },
  { label: "Medium (1280)", cap: 1280 },
  { label: "Low (960)", cap: 960 },
  { label: "Pixel (640)", cap: 640 },
];

/** CSS playfield size as a fraction of the fitted browser viewport. */
const WINDOW_PRESETS = [
  { label: "Full", scale: 1 },
  { label: "Large (90%)", scale: 0.9 },
  { label: "Medium (75%)", scale: 0.75 },
  { label: "Small (60%)", scale: 0.6 },
  { label: "Compact (50%)", scale: 0.5 },
  { label: "Tiny (35%)", scale: 0.35 },
];

const CODE_LABELS = {
  Space: "Space",
  ArrowUp: "↑",
  ArrowDown: "↓",
  ArrowLeft: "←",
  ArrowRight: "→",
  ShiftLeft: "L.Shift",
  ShiftRight: "R.Shift",
  ControlLeft: "L.Ctrl",
  ControlRight: "R.Ctrl",
  Quote: "'",
  Slash: "/",
  Enter: "Enter",
  NumpadEnter: "NumEnter",
  Tab: "Tab",
};

function codeLabel(code) {
  if (CODE_LABELS[code]) return CODE_LABELS[code];
  if (code.startsWith("Key") && code.length === 4) return code.slice(3);
  if (code.startsWith("Digit")) return code.slice(5);
  if (code.startsWith("Numpad")) return "Num" + code.slice(6);
  return code;
}

function loadStored() {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return null;
    return JSON.parse(raw);
  } catch (_) {
    return null;
  }
}

function saveStored(s) {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(s));
  } catch (_) {
    /* private mode */
  }
}

function defaultsFromMod(mod) {
  let binds = {};
  try {
    binds = JSON.parse(mod.roomDefaultBindsJson());
  } catch (_) {
    binds = {};
  }
  return {
    mouseSens: 1,
    invertY: false,
    renderCap: 0,
    windowScale: 1,
    binds,
    /** @type {Record<string, string>} */
    shaders: shaderDefaults(),
  };
}

/** Faithful / classic look — every modern FX off (`docs/29`). */
function shaderDefaults() {
  return {
    r_quality: "0",
    gl_bloom: "0",
    gl_ssao: "0",
    gl_tonemap: "0",
    gl_fxaa: "0",
    r_aa: "0",
    r_crt: "0",
    r_dither: "0",
    r_banding: "0",
    r_vignette: "0",
    r_grain: "0",
    r_chromatic: "0",
    r_softparticles: "0",
    r_damagefx: "0",
    r_colorgrade: "0",
    r_gore: "1",
    r_bloodfx: "0",
    r_blooddrips: "0",
    r_screenblood: "0",
    r_muzzlefx: "0",
    r_tracers: "0",
    r_impactfx: "0",
    r_explosionfx: "0",
    r_viewkick: "0",
    r_trails: "0",
    r_casings: "0",
    r_weaponsmoke: "0",
    r_hitconfirm: "0",
    r_corpsepools: "0",
    r_gibs: "0",
    r_reduce_flashing: "0",
    r_sharpen: "0",
    r_exposure: "1",
    gl_bloom_amount: "0.8",
  };
}

/** Graphics toggles shown in Settings (maps to roomSetShaderCvar). */
const SHADER_TOGGLES = [
  { key: "gl_bloom", label: "Bloom", kind: "bool" },
  { key: "gl_ssao", label: "SSAO", kind: "int", min: 0, max: 3 },
  { key: "gl_fxaa", label: "Anti-alias", kind: "aa" },
  { key: "r_crt", label: "CRT", kind: "bool" },
  { key: "r_dither", label: "Dither", kind: "bool" },
  { key: "r_vignette", label: "Vignette", kind: "float", on: "0.35" },
  { key: "r_grain", label: "Film grain", kind: "float", on: "0.15" },
  { key: "r_chromatic", label: "Chromatic aberration", kind: "float", on: "0.2" },
  { key: "r_softparticles", label: "Soft particles", kind: "bool" },
  { key: "r_damagefx", label: "Damage FX", kind: "bool" },
  { key: "r_muzzlefx", label: "Muzzle FX", kind: "bool" },
  { key: "r_tracers", label: "Tracers", kind: "bool" },
  { key: "r_impactfx", label: "Impact FX", kind: "bool" },
  { key: "r_explosionfx", label: "Explosion FX", kind: "bool" },
  { key: "r_viewkick", label: "View kick", kind: "bool" },
  { key: "r_bloodfx", label: "Blood FX", kind: "bool" },
  { key: "r_screenblood", label: "Screen blood", kind: "bool" },
  { key: "r_reduce_flashing", label: "Reduce flashing", kind: "bool" },
];

const QUALITY_LABELS = ["Faithful", "Low", "Medium", "Ultra"];
const TONEMAP_LABELS = [
  "None",
  "Reinhard",
  "Linear",
  "Hejl",
  "Palette",
  "Uncharted2",
  "AgX",
  "Khronos",
  "RoomSoft",
];
const GORE_LABELS = ["Off", "Vanilla", "Enhanced", "Full"];
const AA_LABELS = ["Off", "FXAA", "SMAA", "TAA"];

function codesForAction(binds, action) {
  return Object.keys(binds)
    .filter((c) => binds[c] === action)
    .sort();
}

/**
 * @param {object} mod room_web wasm exports
 * @param {{ openGamebar?: () => void }} [opts]
 */
export function initSettings(mod, opts = {}) {
  if (!mod || typeof mod.roomApplySettings !== "function") {
    console.warn("room-web: settings unavailable (old wasm)");
    return { open() {}, apply() {} };
  }

  const factoryAll = defaultsFromMod(mod);
  let state = { ...factoryAll, ...(loadStored() || {}) };
  // Merge missing default binds / shader keys so new toggles appear after updates.
  state.binds = { ...factoryAll.binds, ...(state.binds || {}) };
  state.shaders = { ...factoryAll.shaders, ...(state.shaders || {}) };

  const root = document.createElement("div");
  root.id = "settings";
  root.hidden = true;
  root.setAttribute("role", "dialog");
  root.setAttribute("aria-modal", "true");
  root.setAttribute("aria-labelledby", "settings-title");
  root.innerHTML = `
    <div class="settings-panel">
      <header class="settings-head">
        <h2 id="settings-title">Settings</h2>
        <button type="button" class="settings-close" aria-label="Close">×</button>
      </header>
      <div class="settings-body">
        <section>
          <h3>Display</h3>
          <label class="settings-row">Window size
            <select id="settings-window"></select>
          </label>
          <p class="settings-hint">On-screen playfield size (letterboxed in the browser).</p>
          <label class="settings-row">Resolution
            <select id="settings-res"></select>
          </label>
          <p class="settings-hint">Caps the longest edge of the render buffer (internal pixels).</p>
          <button type="button" id="settings-fs" class="settings-btn">Toggle fullscreen</button>
        </section>
        <section>
          <h3>Minimap</h3>
          <label class="settings-check"><input id="settings-mm-show" type="checkbox" /> Show minimap</label>
          <label class="settings-row">Size
            <select id="settings-mm-size">
              <option value="s">Small</option>
              <option value="m">Medium</option>
              <option value="l">Large</option>
            </select>
          </label>
          <label class="settings-row">Zoom
            <input id="settings-mm-zoom" type="range" min="0" max="5" step="1" />
          </label>
          <label class="settings-check"><input id="settings-mm-rotate" type="checkbox" /> Turn with the view</label>
          <p class="settings-hint">Top right, explored areas only (a computer map shows the rest). Alt+M shows or hides it, Alt+= and Alt+- zoom.</p>
        </section>
        <section>
          <h3>Graphics</h3>
          <p class="settings-hint">All effects default off (faithful look). Quality preset fills the toggles below.</p>
          <label class="settings-row">Quality
            <select id="settings-quality"></select>
          </label>
          <label class="settings-row">Tonemap
            <select id="settings-tonemap"></select>
          </label>
          <label class="settings-row">Gore
            <select id="settings-gore"></select>
          </label>
          <div id="settings-shaders" class="settings-shaders"></div>
          <button type="button" id="settings-reset-shaders" class="settings-btn ghost">Reset shaders to defaults</button>
          <p class="settings-hint">Turns off bloom, blood FX, muzzle, CRT, etc. (faithful look). Does not change mouse/keys.</p>
        </section>
        <section>
          <h3>Messages</h3>
          <label class="settings-row">Style
            <select id="settings-msg-style">
              <option value="toast">Cards (fade out on their own)</option>
              <option value="classic">Classic (in-game text)</option>
            </select>
          </label>
          <div id="settings-msg-kinds" class="settings-shaders"></div>
          <p class="settings-hint">Choose what the game tells you about. Tab shows the level stats.</p>
        </section>
        <section id="settings-modopts">
          <h3>Mod options</h3>
          <p class="settings-hint" id="modopt-note"></p>
          <input id="modopt-search" class="modopt-search" type="search" placeholder="Find a setting" aria-label="Find a mod setting" />
          <div id="modopt-groups" class="modopt-groups"></div>
          <button type="button" id="modopt-reset" class="settings-btn ghost">Reset mod options</button>
        </section>
        <section>
          <h3>Mouse</h3>
          <label class="settings-row">Sensitivity
            <input id="settings-sens" type="range" min="10" max="300" step="5" />
            <span id="settings-sens-val">1.00</span>
          </label>
          <label class="settings-check"><input id="settings-invert" type="checkbox" /> Invert look Y</label>
        </section>
        <section>
          <h3>Keyboard</h3>
          <p class="settings-hint">Click a binding, then press a key. Esc cancels. Backspace clears.</p>
          <div id="settings-binds" class="settings-binds"></div>
        </section>
      </div>
      <footer class="settings-foot">
        <button type="button" id="settings-reset" class="settings-btn ghost">Reset defaults</button>
        <button type="button" id="settings-done" class="settings-btn primary">Done</button>
      </footer>
    </div>`;
  document.body.appendChild(root);

  const resSel = root.querySelector("#settings-res");
  for (const p of RES_PRESETS) {
    const o = document.createElement("option");
    o.value = String(p.cap);
    o.textContent = p.label;
    resSel.appendChild(o);
  }
  const winSel = root.querySelector("#settings-window");
  for (const p of WINDOW_PRESETS) {
    const o = document.createElement("option");
    o.value = String(p.scale);
    o.textContent = p.label;
    winSel.appendChild(o);
  }
  const qualitySel = root.querySelector("#settings-quality");
  QUALITY_LABELS.forEach((label, i) => {
    const o = document.createElement("option");
    o.value = String(i);
    o.textContent = label;
    qualitySel.appendChild(o);
  });
  const tonemapSel = root.querySelector("#settings-tonemap");
  TONEMAP_LABELS.forEach((label, i) => {
    const o = document.createElement("option");
    o.value = String(i);
    o.textContent = label;
    tonemapSel.appendChild(o);
  });
  const goreSel = root.querySelector("#settings-gore");
  GORE_LABELS.forEach((label, i) => {
    const o = document.createElement("option");
    o.value = String(i);
    o.textContent = label;
    goreSel.appendChild(o);
  });

  let listening = null;

  function shaderOn(key, kind, onVal) {
    const v = state.shaders[key] ?? "0";
    if (kind === "float") return Number(v) > 0.01;
    if (kind === "aa" || kind === "int") return Number(v) !== 0;
    return v === "1" || v === "true" || v === "on";
  }

  function renderShaderToggles() {
    const box = root.querySelector("#settings-shaders");
    box.innerHTML = "";
    for (const t of SHADER_TOGGLES) {
      if (t.kind === "aa") {
        const row = document.createElement("label");
        row.className = "settings-row";
        row.textContent = t.label + " ";
        const sel = document.createElement("select");
        AA_LABELS.forEach((label, i) => {
          const o = document.createElement("option");
          o.value = String(i);
          o.textContent = label;
          sel.appendChild(o);
        });
        sel.value = String(Number(state.shaders[t.key] || 0));
        sel.addEventListener("change", () => {
          state.shaders[t.key] = sel.value;
          state.shaders.r_aa = sel.value;
          applyShaders();
        });
        row.appendChild(sel);
        box.appendChild(row);
        continue;
      }
      if (t.kind === "int") {
        const row = document.createElement("label");
        row.className = "settings-row";
        row.textContent = t.label + " ";
        const sel = document.createElement("select");
        for (let i = t.min; i <= t.max; i++) {
          const o = document.createElement("option");
          o.value = String(i);
          o.textContent = String(i);
          sel.appendChild(o);
        }
        sel.value = String(Number(state.shaders[t.key] || 0));
        sel.addEventListener("change", () => {
          state.shaders[t.key] = sel.value;
          applyShaders();
        });
        row.appendChild(sel);
        box.appendChild(row);
        continue;
      }
      const row = document.createElement("label");
      row.className = "settings-check";
      const cb = document.createElement("input");
      cb.type = "checkbox";
      cb.checked = shaderOn(t.key, t.kind, t.on);
      cb.addEventListener("change", () => {
        if (t.kind === "float") {
          state.shaders[t.key] = cb.checked ? t.on || "1" : "0";
        } else {
          state.shaders[t.key] = cb.checked ? "1" : "0";
        }
        applyShaders();
      });
      row.appendChild(cb);
      row.appendChild(document.createTextNode(" " + t.label));
      box.appendChild(row);
    }
  }

  function applyShaders() {
    if (typeof mod.roomSetShaderCvar !== "function") return;
    try {
      for (const [k, v] of Object.entries(state.shaders || {})) {
        mod.roomSetShaderCvar(k, String(v));
      }
    } catch (e) {
      console.warn("room-web: shader cvars failed", e);
    }
    saveStored(state);
  }

  function nearestWindowScale(v) {
    const n = Number(v) || 1;
    let best = WINDOW_PRESETS[0].scale;
    let bestD = Infinity;
    for (const p of WINDOW_PRESETS) {
      const d = Math.abs(p.scale - n);
      if (d < bestD) {
        bestD = d;
        best = p.scale;
      }
    }
    return best;
  }

  function syncForm() {
    resSel.value = String(state.renderCap || 0);
    winSel.value = String(nearestWindowScale(state.windowScale));
    const sens = root.querySelector("#settings-sens");
    sens.value = String(Math.round((state.mouseSens || 1) * 100));
    root.querySelector("#settings-sens-val").textContent = (state.mouseSens || 1).toFixed(2);
    root.querySelector("#settings-invert").checked = !!state.invertY;
    qualitySel.value = String(Number(state.shaders.r_quality || 0));
    tonemapSel.value = String(Number(state.shaders.gl_tonemap || 0));
    goreSel.value = String(Number(state.shaders.r_gore || 1));
    syncMinimap();
    syncMessages();
    renderModOptions();
    renderShaderToggles();
    renderBinds();
  }

  // ---- Messages: which kinds of in-game messages show, and how.
  const MSG_LABELS = {
    kill: "Monster kills",
    frag: "Frags (players and bots)",
    death: "Deaths",
    pickup: "Pickups",
    power: "Power-ups (berserk and others)",
    secret: "Secrets found",
    lock: "Locked doors and keys",
    script: "Map and mod messages",
    system: "Game and connection notices",
  };
  const messages = () => window.roomMessages;
  function syncMessages() {
    const api = messages();
    const box = root.querySelector("#settings-msg-kinds");
    const style = root.querySelector("#settings-msg-style");
    style.disabled = !api;
    box.innerHTML = "";
    if (!api) return;
    const c = api.get();
    style.value = c.style || "toast";
    const hidden = new Set(c.hidden || []);
    for (const kind of api.kinds) {
      const row = document.createElement("label");
      row.className = "settings-check";
      const cb = document.createElement("input");
      cb.type = "checkbox";
      cb.checked = !hidden.has(kind);
      cb.addEventListener("change", () => {
        const h = new Set(api.get().hidden || []);
        if (cb.checked) h.delete(kind);
        else h.add(kind);
        api.configure({ hidden: [...h] });
      });
      row.append(cb, document.createTextNode(" " + (MSG_LABELS[kind] || kind)));
      box.appendChild(row);
    }
  }
  root.querySelector("#settings-msg-style").addEventListener("change", (ev) => {
    messages()?.configure({ style: ev.target.value });
  });

  // ---- Mod options: the loaded mods' CVARINFO, as their own menus label it.
  const MOD_KEY = "room/mod-cvars";
  const loadModCvars = () => {
    try {
      return JSON.parse(localStorage.getItem(MOD_KEY) || "{}") || {};
    } catch (_) {
      return {};
    }
  };
  const saveModCvars = (o) => {
    try {
      localStorage.setItem(MOD_KEY, JSON.stringify(o));
    } catch (_) {
      /* private mode */
    }
  };
  let modGroups = [];
  const openGroups = new Set();

  function colorToHex(v) {
    const t = String(v || "").trim();
    if (/^#[0-9a-f]{6}$/i.test(t)) return t;
    const m = t.match(/^([0-9a-f]{1,2})\s+([0-9a-f]{1,2})\s+([0-9a-f]{1,2})$/i);
    if (m) return "#" + m.slice(1).map((x) => x.padStart(2, "0")).join("");
    return "#000000";
  }
  const hexToColor = (h) => h.replace("#", "").match(/../g).join(" ");

  function setModCvar(item, value) {
    const store = loadModCvars();
    const key = item.cvar.toLowerCase();
    if (value === item.default) delete store[key];
    else store[key] = value;
    saveModCvars(store);
    item.value = value;
    try {
      mod.roomSetModCvar(item.cvar, value);
    } catch (e) {
      console.warn("room-web: mod cvar failed", e);
    }
  }

  function modControl(item, onChange) {
    if (item.kind === "choice") {
      const sel = document.createElement("select");
      let found = false;
      for (const [v, label] of item.values) {
        const o = document.createElement("option");
        o.value = v;
        o.textContent = label;
        if (Number(v) === Number(item.value) || v === item.value) {
          o.selected = true;
          found = true;
        }
        sel.appendChild(o);
      }
      if (!found) {
        const o = document.createElement("option");
        o.value = item.value;
        o.textContent = item.value;
        o.selected = true;
        sel.appendChild(o);
      }
      sel.addEventListener("change", () => onChange(sel.value));
      return sel;
    }
    if (item.kind === "range") {
      const wrap = document.createElement("span");
      wrap.className = "modopt-range";
      const r = document.createElement("input");
      r.type = "range";
      r.min = String(Math.min(item.min, item.max));
      r.max = String(Math.max(item.min, item.max));
      r.step = String(item.step || 1);
      r.value = String(item.value);
      const out = document.createElement("output");
      out.textContent = String(item.value);
      r.addEventListener("input", () => (out.textContent = r.value));
      r.addEventListener("change", () => onChange(r.value));
      wrap.append(r, out);
      return wrap;
    }
    if (item.kind === "color") {
      const c = document.createElement("input");
      c.type = "color";
      c.value = colorToHex(item.value);
      c.addEventListener("change", () => onChange(hexToColor(c.value)));
      return c;
    }
    const f = document.createElement("input");
    f.type = item.type === "int" || item.type === "float" ? "number" : "text";
    if (item.type === "float") f.step = "any";
    f.value = String(item.value);
    f.addEventListener("change", () => onChange(f.value.trim()));
    return f;
  }

  function renderModOptions() {
    const box = root.querySelector("#modopt-groups");
    const note = root.querySelector("#modopt-note");
    const query = root.querySelector("#modopt-search").value.trim().toLowerCase();
    try {
      modGroups = typeof mod.roomModOptions === "function" ? JSON.parse(mod.roomModOptions()) : [];
    } catch (_) {
      modGroups = [];
    }
    box.innerHTML = "";
    const any = modGroups.length > 0;
    root.querySelector("#modopt-search").hidden = !any;
    root.querySelector("#modopt-reset").hidden = !any;
    if (!any) {
      note.textContent = "Start a game with a mod that has settings (a PK3 with CVARINFO) and its options appear here.";
      return;
    }
    const count = modGroups.reduce((n, g) => n + g.items.length, 0);
    note.textContent = `${count} settings from the loaded mods, grouped as their own menus show them. Changes apply now; some only show on things spawned afterwards or on the next map.`;
    for (const g of modGroups) {
      const items = g.items.filter(
        (it) =>
          !query ||
          it.label.toLowerCase().includes(query) ||
          it.cvar.toLowerCase().includes(query) ||
          (it.hint || "").toLowerCase().includes(query)
      );
      if (!items.length) continue;
      const det = document.createElement("details");
      det.className = "modopt-group";
      det.open = !!query || openGroups.has(g.title);
      det.addEventListener("toggle", () => {
        if (query) return;
        if (det.open) openGroups.add(g.title);
        else openGroups.delete(g.title);
      });
      const sum = document.createElement("summary");
      const changed = items.filter((it) => String(it.value) !== String(it.default)).length;
      sum.innerHTML = `<span class="modopt-title"></span><span class="modopt-count"></span>`;
      sum.querySelector(".modopt-title").textContent = g.title;
      sum.querySelector(".modopt-count").textContent = changed ? `${changed} changed · ${items.length}` : String(items.length);
      det.appendChild(sum);
      for (const it of items) {
        const row = document.createElement("div");
        row.className = "modopt-row";
        const isChanged = String(it.value) !== String(it.default);
        row.classList.toggle("changed", isChanged);
        const label = document.createElement("label");
        label.className = "modopt-label";
        label.textContent = it.label;
        label.title = it.cvar;
        const ctl = modControl(it, (v) => {
          setModCvar(it, v);
          renderModOptions();
        });
        const reset = document.createElement("button");
        reset.type = "button";
        reset.className = "modopt-undo";
        reset.textContent = "↺";
        reset.title = "Back to the mod default";
        reset.setAttribute("aria-label", `Reset ${it.label}`);
        reset.hidden = !isChanged;
        reset.addEventListener("click", () => {
          setModCvar(it, it.default);
          renderModOptions();
        });
        row.append(label, ctl, reset);
        if (it.hint) {
          const h = document.createElement("p");
          h.className = "modopt-hint";
          h.textContent = it.hint;
          row.appendChild(h);
        }
        det.appendChild(row);
      }
      box.appendChild(det);
    }
    if (!box.children.length) {
      note.textContent = `No setting matches "${query}".`;
    }
  }
  root.querySelector("#modopt-search").addEventListener("input", renderModOptions);
  root.querySelector("#modopt-reset").addEventListener("click", () => {
    for (const g of modGroups) {
      for (const it of g.items) {
        if (String(it.value) !== String(it.default)) setModCvar(it, it.default);
      }
    }
    saveModCvars({});
    renderModOptions();
  });

  const minimap = () => window.roomMinimap;
  function syncMinimap() {
    const mm = minimap();
    const on = !!mm;
    for (const id of ["#settings-mm-show", "#settings-mm-size", "#settings-mm-zoom", "#settings-mm-rotate"]) {
      root.querySelector(id).disabled = !on;
    }
    if (!on) return;
    const c = mm.get();
    root.querySelector("#settings-mm-show").checked = !!c.show;
    root.querySelector("#settings-mm-size").value = c.size || "m";
    root.querySelector("#settings-mm-zoom").max = String((mm.zoomLevels || 6) - 1);
    root.querySelector("#settings-mm-zoom").value = String(c.zoom);
    root.querySelector("#settings-mm-rotate").checked = c.rotate !== false;
  }
  root.querySelector("#settings-mm-show").addEventListener("change", (ev) => {
    minimap()?.configure({ show: ev.target.checked });
  });
  root.querySelector("#settings-mm-size").addEventListener("change", (ev) => {
    minimap()?.configure({ size: ev.target.value });
  });
  root.querySelector("#settings-mm-zoom").addEventListener("input", (ev) => {
    minimap()?.configure({ zoom: Number(ev.target.value) });
  });
  root.querySelector("#settings-mm-rotate").addEventListener("change", (ev) => {
    minimap()?.configure({ rotate: ev.target.checked });
  });

  function renderBinds() {
    const box = root.querySelector("#settings-binds");
    box.innerHTML = "";
    for (const act of ACTIONS) {
      const row = document.createElement("div");
      row.className = "settings-bind-row";
      const codes = codesForAction(state.binds, act.id);
      const btn = document.createElement("button");
      btn.type = "button";
      btn.className = "settings-bind-btn" + (listening === act.id ? " listening" : "");
      btn.textContent = codes.length ? codes.map(codeLabel).join(" · ") : "—";
      btn.title = codes.join(", ") || "Click to bind";
      btn.addEventListener("click", () => {
        listening = act.id;
        renderBinds();
      });
      row.innerHTML = `<span class="settings-bind-label">${act.label}</span>`;
      row.appendChild(btn);
      box.appendChild(row);
    }
  }

  function apply() {
    const bindsJson = JSON.stringify(state.binds || {});
    try {
      mod.roomApplySettings(
        Number(state.mouseSens) || 1,
        !!state.invertY,
        Number(state.renderCap) || 0,
        Number(state.windowScale) || 1,
        bindsJson
      );
    } catch (e) {
      console.warn("room-web: apply settings failed", e);
    }
    applyShaders();
    saveStored(state);
  }

  function close() {
    listening = null;
    root.hidden = true;
    document.body.classList.remove("settings-open");
  }

  function open() {
    if (opts.openGamebar) opts.openGamebar();
    if (document.pointerLockElement) {
      try {
        document.exitPointerLock();
      } catch (_) {
        /* ignore */
      }
    }
    syncForm();
    root.hidden = false;
    document.body.classList.add("settings-open");
  }

  root.querySelector(".settings-close").addEventListener("click", () => {
    apply();
    close();
  });
  root.querySelector("#settings-done").addEventListener("click", () => {
    apply();
    close();
  });
  root.querySelector("#settings-reset").addEventListener("click", () => {
    state = defaultsFromMod(mod);
    syncForm();
    apply();
  });
  root.querySelector("#settings-reset-shaders").addEventListener("click", () => {
    state.shaders = shaderDefaults();
    qualitySel.value = "0";
    tonemapSel.value = "0";
    goreSel.value = "1";
    renderShaderToggles();
    applyShaders();
    saveStored(state);
  });
  root.querySelector("#settings-sens").addEventListener("input", (ev) => {
    state.mouseSens = Number(ev.target.value) / 100;
    root.querySelector("#settings-sens-val").textContent = state.mouseSens.toFixed(2);
  });
  root.querySelector("#settings-sens").addEventListener("change", apply);
  root.querySelector("#settings-invert").addEventListener("change", (ev) => {
    state.invertY = ev.target.checked;
    apply();
  });
  resSel.addEventListener("change", () => {
    state.renderCap = Number(resSel.value) || 0;
    apply();
  });
  winSel.addEventListener("change", () => {
    state.windowScale = Number(winSel.value) || 1;
    apply();
  });
  qualitySel.addEventListener("change", () => {
    state.shaders.r_quality = qualitySel.value;
    // Mirror desktop preset writes into local shader map for the form.
    const presets = {
      0: {
        gl_tonemap: "0",
        gl_fxaa: "0",
        gl_bloom: "0",
        gl_ssao: "0",
        r_crt: "0",
        r_softparticles: "0",
        r_damagefx: "0",
      },
      1: {
        gl_tonemap: "8",
        gl_fxaa: "1",
        gl_bloom: "1",
        gl_ssao: "0",
        r_softparticles: "1",
      },
      2: {
        gl_tonemap: "6",
        gl_fxaa: "2",
        gl_bloom: "1",
        gl_ssao: "1",
        r_softparticles: "1",
        r_damagefx: "1",
      },
      3: {
        gl_tonemap: "7",
        gl_fxaa: "3",
        gl_bloom: "1",
        gl_ssao: "3",
        r_softparticles: "1",
        r_damagefx: "1",
        r_gore: "2",
      },
    };
    Object.assign(state.shaders, presets[qualitySel.value] || {});
    tonemapSel.value = String(Number(state.shaders.gl_tonemap || 0));
    goreSel.value = String(Number(state.shaders.r_gore || 1));
    renderShaderToggles();
    applyShaders();
  });
  tonemapSel.addEventListener("change", () => {
    state.shaders.gl_tonemap = tonemapSel.value;
    applyShaders();
  });
  goreSel.addEventListener("change", () => {
    state.shaders.r_gore = goreSel.value;
    applyShaders();
  });
  root.querySelector("#settings-fs").addEventListener("click", () => {
    const el = document.documentElement;
    if (!document.fullscreenElement) {
      el.requestFullscreen?.().catch(() => {});
    } else {
      document.exitFullscreen?.().catch(() => {});
    }
  });

  root.addEventListener("click", (ev) => {
    if (ev.target === root) {
      apply();
      close();
    }
  });

  window.addEventListener(
    "keydown",
    (ev) => {
      if (root.hidden) return;
      if (listening) {
        ev.preventDefault();
        ev.stopPropagation();
        const act = listening;
        if (ev.code === "Escape") {
          listening = null;
          renderBinds();
          return;
        }
        if (ev.code === "Backspace" || ev.code === "Delete") {
          for (const [c, a] of Object.entries(state.binds)) {
            if (a === act) delete state.binds[c];
          }
          listening = null;
          renderBinds();
          apply();
          return;
        }
        // One primary code per action: drop prior codes for this action, then set.
        for (const [c, a] of Object.entries(state.binds)) {
          if (a === act) delete state.binds[c];
        }
        state.binds[ev.code] = act;
        listening = null;
        renderBinds();
        apply();
        return;
      }
      if (ev.code === "Escape") {
        apply();
        close();
      }
    },
    true
  );

  // GPU init is async (`room-ready`); applying before then no-ops shader CVars.
  apply();
  window.addEventListener("room-ready", () => {
    applyShaders();
  });

  return { open, close, apply, isOpen: () => !root.hidden };
}
