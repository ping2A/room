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
  };
}

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

  let state = { ...defaultsFromMod(mod), ...(loadStored() || {}) };
  // Merge missing default binds so new actions appear after updates.
  const factory = defaultsFromMod(mod).binds;
  state.binds = { ...factory, ...(state.binds || {}) };

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

  let listening = null;

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
    renderBinds();
  }

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

  // Apply saved settings as soon as wasm is ready.
  apply();

  return { open, close, apply, isOpen: () => !root.hidden };
}
