// On-screen controls for phones / tablets (play page).
//
// Left half: floating analog stick. It appears where the thumb lands, follows
// the thumb past its rim, and moves at any angle and speed (a full push runs).
// Right half: drag to look, with optional acceleration; double-tap and hold to
// fire while aiming. Buttons: FIRE (drag on it to aim while firing), USE,
// JUMP and DUCK (can be hidden in Settings for Doom), previous / next weapon (hold either one for the weapon picker),
// scores, fullscreen, game menu; Duke adds KICK and ITEM.
// Optional gyro aiming (turn the phone to aim) and vibration on presses.
//
// Everything feeds the same input path as keyboard + mouse (`roomInputKey`,
// `roomInputLook`, `roomInputWeaponCycle`, `roomInputMove`), so solo, coop and
// mods behave the same. Solo Doom uses the analog stick directly; netplay and
// Duke get the matching movement keys.
//
// Enabled on coarse-pointer devices or at the first touch; `?touch=1` forces
// it on, `?touch=0` off. Settings live in `localStorage.roomTouch` and are
// edited from the Settings menu (`window.roomTouch`).

const STICK_RADIUS = 56;
const ANALOG_DEAD = 0.12;
const KEY_DEAD = 0.35;
const RUN_AT = 0.85;
// Mouse counts per CSS pixel of drag at speed 1 (a count is ≈1.4° of turn).
const LOOK_COUNTS_PER_PX = 0.3;
const DEG_PER_COUNT = (256 * 360) / 65536;
const LONG_PRESS_MS = 420;
const DOUBLE_TAP_MS = 300;

const DEFAULTS = {
  look: 1, // drag look speed
  // Up/down drag speed relative to left/right. Landscape screens are short,
  // so a full-height swipe should still tilt the view most of its range.
  lookY: 1.6,
  accel: true, // fast swipes turn further
  tapFire: true, // double-tap and hold the look side to fire
  gyro: false,
  gyroSpeed: 1,
  size: 1, // button scale
  opacity: 1,
  lefty: false, // stick on the right, buttons on the left
  haptics: true,
  // Jump / crouch work GZDoom-style unless a map turns them off. Duke always
  // shows them.
  jumpDuck: true,
};

const STORE_KEY = "roomTouch";
/** Shown in Settings so a stale cached copy is easy to spot. */
const VERSION = "2026-11-16c jump + crouch";

function loadSettings() {
  let s = { ...DEFAULTS };
  try {
    const raw = localStorage.getItem(STORE_KEY);
    if (raw) s = { ...s, ...JSON.parse(raw) };
    else {
      const old = Number(localStorage.getItem("roomTouchLook"));
      if (old > 0.05 && old < 10) s.look = old;
    }
  } catch (_) {
    /* private mode / bad JSON */
  }
  return s;
}

function saveSettings(s) {
  try {
    localStorage.setItem(STORE_KEY, JSON.stringify(s));
  } catch (_) {
    /* private mode */
  }
}

// Sizes and offsets scale with --s. The button cluster sits bottom right, or
// bottom left for left-handed play (`.lefty`).
const px = (n) => `calc(${n}px * var(--s))`;
const at = (end, bottom) =>
  `right: calc(${px(end)} + env(safe-area-inset-right)); bottom: calc(${px(bottom)} + env(safe-area-inset-bottom));`;
// Left-handed: the same spot measured from the left edge.
const atLeft = (end) => `right: auto; left: calc(${px(end)} + env(safe-area-inset-left));`;
const CLUSTER = [
  [".fire", 24, 28],
  [".use", 128, 22],
  [".wnext", 36, 136],
  [".wprev", 100, 112],
  [".jump", 204, 86],
  [".duck", 206, 18],
  [".kick", 166, 158],
  [".item", 230, 156],
];
const LEFTY_CSS = CLUSTER.map(([sel, end]) => `#touch.lefty ${sel} { ${atLeft(end)} }`).join("\n");
const size = (n, font) => `width: ${px(n)}; height: ${px(n)};${font ? ` font-size: ${px(font)};` : ""}`;

const CSS = `
body.touch #touch { display: block; }
body.touch, body.touch #stage { -webkit-user-select: none; user-select: none; -webkit-touch-callout: none; }
body.touch #gamebar { display: none; top: calc(52px + env(safe-area-inset-top)); left: auto; right: calc(8px + env(safe-area-inset-right)); flex-wrap: wrap; max-width: 90vw; }
body.touch.menu-open #gamebar { display: flex; }
body.touch #voiceBar { bottom: auto; top: calc(52px + env(safe-area-inset-top)); left: calc(8px + env(safe-area-inset-left)); right: auto; }
body.touch #netGraph { top: calc(8px + env(safe-area-inset-top)); left: calc(8px + env(safe-area-inset-left)); transform: scale(0.8); transform-origin: top left; }
body.touch #status { left: 50%; transform: translateX(-50%); bottom: auto; top: calc(8px + env(safe-area-inset-top)); }
body:not(.touch) #settings-touch { display: none; }
#touch {
  --s: 1; --o: 1;
  display: none; position: fixed; inset: 0; z-index: 1;
  touch-action: none; -webkit-tap-highlight-color: transparent;
}
#touch .zone { position: absolute; top: 0; bottom: 0; }
#touch .zone.move { left: 0; width: 45%; }
#touch .zone.look { right: 0; width: 55%; }
#touch.lefty .zone.move { left: auto; right: 0; }
#touch.lefty .zone.look { right: auto; left: 0; }
#touch .stick {
  position: absolute; width: ${STICK_RADIUS * 2}px; height: ${STICK_RADIUS * 2}px;
  margin: -${STICK_RADIUS}px 0 0 -${STICK_RADIUS}px; border-radius: 50%;
  border: 2px solid rgba(220, 225, 235, 0.35); background: rgba(20, 24, 32, 0.25);
  pointer-events: none; display: none;
}
#touch .stick.idle { display: block; opacity: calc(0.45 * var(--o)); }
#touch .stick.on { display: block; opacity: var(--o); }
#touch .knob {
  position: absolute; left: 50%; top: 50%; width: 46px; height: 46px; margin: -23px 0 0 -23px;
  border-radius: 50%; background: rgba(230, 235, 245, 0.45);
}
#touch .stick.run .knob { background: rgba(255, 190, 110, 0.7); }
#touch button {
  position: absolute; display: flex; align-items: center; justify-content: center;
  border-radius: 50%; border: 2px solid rgba(220, 225, 235, 0.4);
  background: rgba(20, 24, 32, 0.35); color: rgba(240, 242, 248, 0.9);
  font: 600 13px/1 system-ui, sans-serif; letter-spacing: 0.04em; padding: 0;
  touch-action: none; -webkit-user-select: none; user-select: none;
  opacity: var(--o);
}
#touch button.on { background: rgba(200, 70, 50, 0.55); border-color: rgba(255, 150, 130, 0.8); }
#touch .fire { ${size(92, 15)} ${at(24, 28)} }
#touch .use { ${size(64, 13)} ${at(128, 22)} }
#touch .wprev, #touch .wnext { ${size(48, 18)} }
#touch .wnext { ${at(36, 136)} }
#touch .wprev { ${at(100, 112)} }
#touch .jump { ${size(58, 12)} ${at(204, 86)} }
#touch .duck { ${size(52, 11)} ${at(206, 18)} }
#touch .kick { ${size(48, 11)} ${at(166, 158)} }
#touch .item { ${size(48, 11)} ${at(230, 156)} }
#touch .duke-only { display: none; }
#touch:not(.duke):not(.jd) .jump, #touch:not(.duke):not(.jd) .duck { display: none; }
#touch.duke .duke-only { display: flex; }
/* Short landscape phones: tighten the cluster so it stays clear of the stick. */
@media (max-height: 380px) {
  #touch .fire { ${size(78)} }
  #touch .jump { bottom: calc(${px(76)} + env(safe-area-inset-bottom)); }
  #touch .kick, #touch .item { bottom: calc(${px(138)} + env(safe-area-inset-bottom)); }
}
${LEFTY_CSS}
#touch .top {
  top: calc(8px + env(safe-area-inset-top)); width: 40px; height: 40px; border-radius: 8px; font-size: 16px;
}
#touch .menu { right: calc(8px + env(safe-area-inset-right)); }
#touch .full { right: calc(56px + env(safe-area-inset-right)); }
#touch .scores { right: calc(104px + env(safe-area-inset-right)); font-size: 12px; }
#touch .wslots {
  position: absolute; left: 50%; top: calc(60px + env(safe-area-inset-top)); transform: translateX(-50%);
  display: none; gap: 8px; padding: 8px 10px; border-radius: 14px;
  background: rgba(10, 12, 18, 0.72);
}
#touch .wslots.open { display: flex; }
#touch .wslots button { position: static; width: 46px; height: 46px; border-radius: 10px; font-size: 18px; }
#touch:not(.duke) .wslots .duke-only { display: none; }
#touch .rotate {
  position: absolute; left: 50%; top: calc(56px + env(safe-area-inset-top)); transform: translateX(-50%);
  padding: 6px 12px; border-radius: 6px; background: rgba(10, 12, 18, 0.75);
  font: 12px system-ui, sans-serif; color: #aab; pointer-events: none; display: none; white-space: nowrap;
}
@media (orientation: portrait) { #touch .rotate { display: block; } }
`;

function wantTouch(params) {
  const forced = params.get("touch");
  if (forced === "0") return false;
  if (forced === "1") return true;
  return window.matchMedia && window.matchMedia("(pointer: coarse)").matches;
}

/** Install the controls. `mod` is the `room_web.js` module namespace. */
export function initTouchControls(mod, params = new URLSearchParams(location.search)) {
  if (params.get("touch") === "0") return;
  const style = document.createElement("style");
  style.textContent = CSS;
  document.head.appendChild(style);

  const slots = [1, 2, 3, 4, 5, 6, 7, 8, 9]
    .map((n) => `<button type="button" data-slot="${n}" aria-label="Weapon ${n}">${n}</button>`)
    .join("");
  const root = document.createElement("div");
  root.id = "touch";
  root.innerHTML = `
    <div class="zone move"></div>
    <div class="zone look"></div>
    <div class="stick"><div class="knob"></div></div>
    <button type="button" class="fire" aria-label="Fire">FIRE</button>
    <button type="button" class="use" aria-label="Use / open">USE</button>
    <button type="button" class="jump" aria-label="Jump">JUMP</button>
    <button type="button" class="duck" aria-label="Crouch">DUCK</button>
    <button type="button" class="kick duke-only" aria-label="Quick kick">KICK</button>
    <button type="button" class="item duke-only" aria-label="Use inventory item">ITEM</button>
    <button type="button" class="wprev" aria-label="Previous weapon (hold to pick)">‹</button>
    <button type="button" class="wnext" aria-label="Next weapon (hold to pick)">›</button>
    <div class="wslots" role="group" aria-label="Pick a weapon">${slots}<button type="button" class="duke-only" data-slot="0" aria-label="Weapon 10">0</button></div>
    <button type="button" class="top scores" aria-label="Scores">TAB</button>
    <button type="button" class="top full" aria-label="Fullscreen">⛶</button>
    <button type="button" class="top menu" aria-label="Game menu">☰</button>
    <p class="rotate">Turn your device sideways for a bigger view</p>`;
  document.body.appendChild(root);
  const $ = (sel) => root.querySelector(sel);
  // Capture keeps a finger's moves on its control; never let it block the press.
  const capture = (el, ev) => {
    try {
      el.setPointerCapture(ev.pointerId);
    } catch (_) {
      /* pointer already gone */
    }
  };
  const stick = $(".stick");
  const knob = $(".knob");
  const hasAnalog = typeof mod.roomInputMove === "function";

  // --- settings ---------------------------------------------------------------
  const cfg = loadSettings();
  const buzz = (ms) => {
    if (cfg.haptics && navigator.vibrate) {
      try {
        navigator.vibrate(ms);
      } catch (_) {
        /* not allowed yet */
      }
    }
  };

  // --- keys ------------------------------------------------------------------
  const held = new Map(); // code → reference count (several fingers may hold one key)
  const press = (code) => {
    const n = held.get(code) || 0;
    held.set(code, n + 1);
    if (n === 0) mod.roomInputKey(code, true);
  };
  const release = (code) => {
    const n = held.get(code) || 0;
    if (n <= 1) {
      held.delete(code);
      if (n === 1) mod.roomInputKey(code, false);
    } else {
      held.set(code, n - 1);
    }
  };
  const tapKey = (code) => {
    mod.roomInputKey(code, true);
    setTimeout(() => mod.roomInputKey(code, false), 60);
  };
  const setAnalog = (x, y) => {
    if (hasAnalog) mod.roomInputMove(x, y);
  };
  const releaseAll = () => {
    for (const code of held.keys()) mod.roomInputKey(code, false);
    held.clear();
    stickState.keys.clear();
    stickState.id = null;
    setAnalog(0, 0);
    stick.classList.remove("on", "run");
    placeIdleStick();
    for (const b of root.querySelectorAll("button.on")) b.classList.remove("on");
    looks.clear();
    lookFire.clear();
    closeSlots();
  };

  // --- look (fractional counts accumulate so slow drags still turn) -----------
  let accX = 0;
  let accY = 0;
  const lookCounts = (cx, cy) => {
    accX += cx;
    accY += cy;
    const ix = Math.trunc(accX);
    const iy = Math.trunc(accY);
    if (ix !== 0 || iy !== 0) {
      accX -= ix;
      accY -= iy;
      mod.roomInputLook(ix, iy);
    }
  };
  const looks = new Map(); // pointerId → last {x, y, t}
  const startLook = (ev) => looks.set(ev.pointerId, { x: ev.clientX, y: ev.clientY, t: ev.timeStamp });
  const moveLook = (ev) => {
    const last = looks.get(ev.pointerId);
    if (!last) return;
    const dx = ev.clientX - last.x;
    const dy = ev.clientY - last.y;
    const dt = Math.max(1, ev.timeStamp - last.t);
    // Acceleration: slow drags stay precise, quick flicks turn further.
    let gain = 1;
    if (cfg.accel) {
      const speed = Math.hypot(dx, dy) / dt; // CSS px per ms
      gain += Math.min(1, Math.max(0, (speed - 0.25) / 1.5));
    }
    const k = LOOK_COUNTS_PER_PX * cfg.look * gain;
    lookCounts(dx * k, dy * k * cfg.lookY);
    last.x = ev.clientX;
    last.y = ev.clientY;
    last.t = ev.timeStamp;
  };
  const endLook = (ev) => looks.delete(ev.pointerId);

  // --- stick ------------------------------------------------------------------
  const stickState = { id: null, cx: 0, cy: 0, keys: new Set() };
  const setStickKeys = (want) => {
    for (const code of stickState.keys) if (!want.has(code)) release(code);
    for (const code of want) if (!stickState.keys.has(code)) press(code);
    stickState.keys = want;
  };
  const idleOffset = STICK_RADIUS + 36;
  function placeIdleStick() {
    stick.style.left = cfg.lefty
      ? `calc(100% - ${idleOffset}px - env(safe-area-inset-right))`
      : `calc(${idleOffset}px + env(safe-area-inset-left))`;
    stick.style.top = `calc(100% - ${idleOffset}px - env(safe-area-inset-bottom))`;
    knob.style.transform = "";
    stick.classList.add("idle");
  }
  const moveZone = $(".zone.move");
  moveZone.addEventListener("pointerdown", (ev) => {
    ev.preventDefault();
    if (stickState.id !== null) return;
    capture(moveZone, ev);
    stickState.id = ev.pointerId;
    stickState.cx = ev.clientX;
    stickState.cy = ev.clientY;
    stick.style.left = `${ev.clientX}px`;
    stick.style.top = `${ev.clientY}px`;
    stick.classList.remove("idle");
    stick.classList.add("on");
  });
  moveZone.addEventListener("pointermove", (ev) => {
    if (ev.pointerId !== stickState.id) return;
    let dx = (ev.clientX - stickState.cx) / STICK_RADIUS;
    let dy = (ev.clientY - stickState.cy) / STICK_RADIUS;
    const mag = Math.hypot(dx, dy);
    if (mag > 1) {
      // Past the rim: drag the stick along so turning back responds at once.
      dx /= mag;
      dy /= mag;
      stickState.cx = ev.clientX - dx * STICK_RADIUS;
      stickState.cy = ev.clientY - dy * STICK_RADIUS;
      stick.style.left = `${stickState.cx}px`;
      stick.style.top = `${stickState.cy}px`;
    }
    // (dx, dy) is now inside the unit circle; m is how far the knob is pushed.
    const m = Math.min(1, mag);
    knob.style.transform = `translate(${dx * STICK_RADIUS}px, ${dy * STICK_RADIUS}px)`;
    // Analog: rescale past the dead zone so a light push creeps.
    if (m > ANALOG_DEAD) {
      const out = (m - ANALOG_DEAD) / (1 - ANALOG_DEAD);
      setAnalog((dx / m) * out, (-dy / m) * out);
    } else {
      setAnalog(0, 0);
    }
    // Keys for netplay / Duke (and older builds without the analog export).
    const want = new Set();
    if (dy < -KEY_DEAD) want.add("KeyW");
    if (dy > KEY_DEAD) want.add("KeyS");
    if (dx < -KEY_DEAD) want.add("KeyA");
    if (dx > KEY_DEAD) want.add("KeyD");
    const run = m >= RUN_AT;
    if (run) want.add("ShiftLeft");
    stick.classList.toggle("run", run);
    setStickKeys(want);
  });
  const endStick = (ev) => {
    if (ev.pointerId !== stickState.id) return;
    stickState.id = null;
    setStickKeys(new Set());
    setAnalog(0, 0);
    stick.classList.remove("on", "run");
    placeIdleStick();
  };
  moveZone.addEventListener("pointerup", endStick);
  moveZone.addEventListener("pointercancel", endStick);
  placeIdleStick();

  // --- look zone (double-tap and hold fires) ---------------------------------
  const lookZone = $(".zone.look");
  const lookFire = new Set(); // pointers firing from the look side
  let lastTap = { t: -1e9, x: 0, y: 0 };
  const downAt = new Map(); // pointerId → {t, x, y}
  lookZone.addEventListener("pointerdown", (ev) => {
    ev.preventDefault();
    capture(lookZone, ev);
    startLook(ev);
    downAt.set(ev.pointerId, { t: ev.timeStamp, x: ev.clientX, y: ev.clientY });
    if (
      cfg.tapFire &&
      ev.timeStamp - lastTap.t < DOUBLE_TAP_MS &&
      Math.hypot(ev.clientX - lastTap.x, ev.clientY - lastTap.y) < 60
    ) {
      lookFire.add(ev.pointerId);
      press("ControlLeft");
      $(".fire").classList.add("on");
      buzz(12);
    }
  });
  lookZone.addEventListener("pointermove", moveLook);
  const endLookZone = (ev) => {
    endLook(ev);
    const d = downAt.get(ev.pointerId);
    downAt.delete(ev.pointerId);
    if (lookFire.delete(ev.pointerId)) {
      release("ControlLeft");
      if (!held.has("ControlLeft")) $(".fire").classList.remove("on");
      lastTap.t = -1e9;
      return;
    }
    // A short, still touch counts as a tap for the double-tap.
    if (d && ev.timeStamp - d.t < 220 && Math.hypot(ev.clientX - d.x, ev.clientY - d.y) < 14) {
      lastTap = { t: ev.timeStamp, x: ev.clientX, y: ev.clientY };
    }
  };
  lookZone.addEventListener("pointerup", endLookZone);
  lookZone.addEventListener("pointercancel", endLookZone);

  // --- buttons ----------------------------------------------------------------
  const holdButton = (btn, code, { aim = false, haptic = 8 } = {}) => {
    btn.addEventListener("pointerdown", (ev) => {
      ev.preventDefault();
      ev.stopPropagation();
      capture(btn, ev);
      btn.classList.add("on");
      press(code);
      buzz(haptic);
      if (aim) startLook(ev);
    });
    if (aim) btn.addEventListener("pointermove", moveLook);
    const up = (ev) => {
      if (!btn.classList.contains("on")) return;
      btn.classList.remove("on");
      release(code);
      if (aim) endLook(ev);
    };
    btn.addEventListener("pointerup", up);
    btn.addEventListener("pointercancel", up);
  };
  holdButton($(".fire"), "ControlLeft", { aim: true, haptic: 12 });
  holdButton($(".use"), "KeyE");
  holdButton($(".scores"), "Tab", { haptic: 0 });
  holdButton($(".jump"), "Space");
  holdButton($(".duck"), "KeyC");
  holdButton($(".kick"), "Slash");
  holdButton($(".item"), "Quote");
  // Duke-only buttons follow the running game.
  if (typeof mod.roomInputDukeMode === "function") {
    setInterval(() => {
      try {
        root.classList.toggle("duke", !!mod.roomInputDukeMode());
      } catch (_) {
        /* module not ready */
      }
    }, 1000);
  }

  // Weapon arrows: tap cycles, hold opens the weapon picker.
  const slotBar = $(".wslots");
  let slotTimer = 0;
  function closeSlots() {
    slotBar.classList.remove("open");
    clearTimeout(slotTimer);
  }
  const openSlots = () => {
    slotBar.classList.add("open");
    buzz(20);
    clearTimeout(slotTimer);
    slotTimer = setTimeout(closeSlots, 4000);
  };
  const weaponButton = (btn, steps) => {
    let timer = 0;
    let long = false;
    btn.addEventListener("pointerdown", (ev) => {
      ev.preventDefault();
      ev.stopPropagation();
      capture(btn, ev);
      btn.classList.add("on");
      long = false;
      timer = setTimeout(() => {
        long = true;
        openSlots();
      }, LONG_PRESS_MS);
    });
    const up = (ev) => {
      if (!btn.classList.contains("on")) return;
      btn.classList.remove("on");
      clearTimeout(timer);
      if (!long && ev.type === "pointerup") {
        mod.roomInputWeaponCycle(steps);
        buzz(8);
      }
    };
    btn.addEventListener("pointerup", up);
    btn.addEventListener("pointercancel", up);
  };
  weaponButton($(".wnext"), 1);
  weaponButton($(".wprev"), -1);
  for (const b of slotBar.querySelectorAll("button")) {
    b.addEventListener("pointerdown", (ev) => {
      ev.preventDefault();
      ev.stopPropagation();
      tapKey(`Digit${b.dataset.slot}`);
      buzz(10);
      closeSlots();
    });
  }
  slotBar.addEventListener("pointerdown", (ev) => ev.stopPropagation());

  const tapButton = (btn, fn) => {
    btn.addEventListener("pointerdown", (ev) => {
      ev.preventDefault();
      ev.stopPropagation();
      btn.classList.add("on");
      fn();
    });
    const off = () => btn.classList.remove("on");
    btn.addEventListener("pointerup", off);
    btn.addEventListener("pointercancel", off);
  };
  tapButton($(".menu"), () => {
    document.body.classList.toggle("menu-open");
    // Drop mouse lock if a desktop hybrid still has it, so the bar is usable.
    if (document.body.classList.contains("menu-open") && document.pointerLockElement) {
      try {
        document.exitPointerLock();
      } catch (_) {
        /* ignore */
      }
    }
  });

  const fullBtn = $(".full");
  const canFullscreen = !!(document.documentElement.requestFullscreen || document.documentElement.webkitRequestFullscreen);
  if (!canFullscreen) fullBtn.hidden = true;
  fullBtn.addEventListener("click", async () => {
    const el = document.documentElement;
    try {
      if (document.fullscreenElement || document.webkitFullscreenElement) {
        await (document.exitFullscreen || document.webkitExitFullscreen).call(document);
        return;
      }
      await (el.requestFullscreen || el.webkitRequestFullscreen).call(el, { navigationUI: "hide" });
      if (screen.orientation && screen.orientation.lock) {
        await screen.orientation.lock("landscape").catch(() => {});
      }
    } catch (_) {
      /* not allowed here (iOS iPhone): the page still plays windowed */
    }
  });

  // --- gyro aiming --------------------------------------------------------------
  let gyroOn = false;
  let gyroT = 0;
  const screenAngle = () => {
    const a = screen.orientation && typeof screen.orientation.angle === "number"
      ? screen.orientation.angle
      : Number(window.orientation) || 0;
    return ((a % 360) + 360) % 360;
  };
  const onMotion = (ev) => {
    const r = ev.rotationRate;
    const now = ev.timeStamp || performance.now();
    const dt = gyroT ? Math.min(0.1, (now - gyroT) / 1000) : 0;
    gyroT = now;
    if (!r || !dt || !document.body.classList.contains("touch")) return;
    const beta = r.beta || 0; // around the device x axis, deg/s
    const gamma = r.gamma || 0; // around the device y axis
    let yawRate;
    let pitchRate;
    switch (screenAngle()) {
      case 90:
        yawRate = -beta;
        pitchRate = gamma;
        break;
      case 270:
        yawRate = beta;
        pitchRate = -gamma;
        break;
      case 180:
        yawRate = gamma;
        pitchRate = beta;
        break;
      default:
        yawRate = -gamma;
        pitchRate = -beta;
    }
    // Ignore sensor noise when the phone is held still.
    if (Math.abs(yawRate) < 1.5) yawRate = 0;
    if (Math.abs(pitchRate) < 1.5) pitchRate = 0;
    const k = (dt * cfg.gyroSpeed) / DEG_PER_COUNT;
    lookCounts(yawRate * k, pitchRate * k);
  };
  const setGyro = (on) => {
    if (on === gyroOn) return;
    gyroOn = on;
    gyroT = 0;
    if (on) window.addEventListener("devicemotion", onMotion);
    else window.removeEventListener("devicemotion", onMotion);
  };
  // iOS asks for motion access, and only from a tap.
  const requestGyro = async () => {
    const DME = window.DeviceMotionEvent;
    if (!DME) return false;
    if (typeof DME.requestPermission === "function") {
      try {
        return (await DME.requestPermission()) === "granted";
      } catch (_) {
        return false;
      }
    }
    return true;
  };

  // --- apply settings -----------------------------------------------------------
  const applyCfg = () => {
    root.style.setProperty("--s", String(Math.min(1.6, Math.max(0.6, Number(cfg.size) || 1))));
    root.style.setProperty("--o", String(Math.min(1, Math.max(0.15, Number(cfg.opacity) || 1))));
    root.classList.toggle("lefty", !!cfg.lefty);
    root.classList.toggle("jd", !!cfg.jumpDuck);
    if (stickState.id === null) placeIdleStick();
    setGyro(!!cfg.gyro);
  };
  applyCfg();

  window.roomTouch = {
    version: VERSION,
    defaults: { ...DEFAULTS },
    get: () => ({ ...cfg }),
    /** Change one setting; returns the stored value (gyro may be refused). */
    async set(key, value) {
      if (!(key in DEFAULTS)) return undefined;
      if (key === "gyro" && value && !(await requestGyro())) value = false;
      cfg[key] = value;
      saveSettings(cfg);
      applyCfg();
      return cfg[key];
    },
    reset() {
      Object.assign(cfg, DEFAULTS);
      saveSettings(cfg);
      applyCfg();
    },
    gyroAvailable: () => "DeviceMotionEvent" in window,
  };

  // Never leave a key stuck when the page loses focus or a call / alert interrupts.
  window.addEventListener("blur", releaseAll);
  document.addEventListener("visibilitychange", () => {
    if (document.hidden) releaseAll();
  });
  // Long-press menus / pinch zoom / double-tap zoom over the controls.
  root.addEventListener("contextmenu", (ev) => ev.preventDefault());
  document.addEventListener("gesturestart", (ev) => ev.preventDefault());

  const enable = () => document.body.classList.add("touch");
  if (wantTouch(params)) enable();
  else window.addEventListener("touchstart", enable, { once: true, passive: true });
}
