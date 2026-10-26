// On-screen controls for phones / tablets (play page).
//
// Left half: floating stick (forward / back / strafe; push to the rim to run).
// Right half: drag to look. Buttons: FIRE (drag on it to aim while firing),
// USE, previous / next weapon, scores, fullscreen, game menu.
//
// Everything feeds the same input path as keyboard + mouse (`roomInputKey`,
// `roomInputLook`, `roomInputWeaponCycle`), so solo, coop and mods behave the same.
//
// Enabled on coarse-pointer devices or at the first touch; `?touch=1` forces
// it on, `?touch=0` off. Look speed: `localStorage.roomTouchLook` (default 1).

const STICK_RADIUS = 56;
const DEAD_ZONE = 0.28;
const RUN_AT = 0.85;
// Mouse counts per CSS pixel of drag at speed 1 (a count is ≈1.4° of turn).
const LOOK_COUNTS_PER_PX = 0.3;
const PITCH_FACTOR = 0.6;

const CSS = `
body.touch #touch { display: block; }
body.touch, body.touch #stage { -webkit-user-select: none; user-select: none; -webkit-touch-callout: none; }
body.touch #gamebar { display: none; top: calc(52px + env(safe-area-inset-top)); left: auto; right: calc(8px + env(safe-area-inset-right)); flex-wrap: wrap; max-width: 90vw; }
body.touch.menu-open #gamebar { display: flex; }
body.touch #voiceBar { bottom: auto; top: calc(52px + env(safe-area-inset-top)); left: calc(8px + env(safe-area-inset-left)); right: auto; }
body.touch #netGraph { top: calc(8px + env(safe-area-inset-top)); left: calc(8px + env(safe-area-inset-left)); transform: scale(0.8); transform-origin: top left; }
body.touch #status { left: 50%; transform: translateX(-50%); bottom: auto; top: calc(8px + env(safe-area-inset-top)); }
#touch {
  display: none; position: fixed; inset: 0; z-index: 1;
  touch-action: none; -webkit-tap-highlight-color: transparent;
}
#touch .zone { position: absolute; top: 0; bottom: 0; }
#touch .zone.move { left: 0; width: 45%; }
#touch .zone.look { right: 0; width: 55%; }
#touch .stick {
  position: absolute; width: ${STICK_RADIUS * 2}px; height: ${STICK_RADIUS * 2}px;
  margin: -${STICK_RADIUS}px 0 0 -${STICK_RADIUS}px; border-radius: 50%;
  border: 2px solid rgba(220, 225, 235, 0.35); background: rgba(20, 24, 32, 0.25);
  pointer-events: none; display: none;
}
#touch .stick.idle { display: block; opacity: 0.45; }
#touch .stick.on { display: block; opacity: 1; }
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
}
#touch button.on { background: rgba(200, 70, 50, 0.55); border-color: rgba(255, 150, 130, 0.8); }
#touch .fire {
  width: 92px; height: 92px; font-size: 15px;
  right: calc(24px + env(safe-area-inset-right)); bottom: calc(28px + env(safe-area-inset-bottom));
}
#touch .use {
  width: 64px; height: 64px;
  right: calc(128px + env(safe-area-inset-right)); bottom: calc(22px + env(safe-area-inset-bottom));
}
#touch .wprev, #touch .wnext { width: 48px; height: 48px; font-size: 18px; }
#touch .wnext { right: calc(36px + env(safe-area-inset-right)); bottom: calc(136px + env(safe-area-inset-bottom)); }
#touch .wprev { right: calc(100px + env(safe-area-inset-right)); bottom: calc(112px + env(safe-area-inset-bottom)); }
#touch .top {
  top: calc(8px + env(safe-area-inset-top)); width: 40px; height: 40px; border-radius: 8px; font-size: 16px;
}
#touch .menu { right: calc(8px + env(safe-area-inset-right)); }
#touch .full { right: calc(56px + env(safe-area-inset-right)); }
#touch .scores { right: calc(104px + env(safe-area-inset-right)); font-size: 12px; }
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

  const root = document.createElement("div");
  root.id = "touch";
  root.innerHTML = `
    <div class="zone move"></div>
    <div class="zone look"></div>
    <div class="stick"><div class="knob"></div></div>
    <button type="button" class="fire" aria-label="Fire">FIRE</button>
    <button type="button" class="use" aria-label="Use / open">USE</button>
    <button type="button" class="wprev" aria-label="Previous weapon">‹</button>
    <button type="button" class="wnext" aria-label="Next weapon">›</button>
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

  let lookSpeed = Number(localStorage.getItem("roomTouchLook") || "1");
  if (!(lookSpeed > 0.05 && lookSpeed < 10)) lookSpeed = 1;

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
  const releaseAll = () => {
    for (const code of held.keys()) mod.roomInputKey(code, false);
    held.clear();
    stickState.keys.clear();
    stickState.id = null;
    stick.classList.remove("on", "run");
    for (const b of root.querySelectorAll("button.on")) b.classList.remove("on");
    looks.clear();
  };

  // --- look (fractional counts accumulate so slow drags still turn) -----------
  let accX = 0;
  let accY = 0;
  const look = (dxPx, dyPx) => {
    accX += dxPx * LOOK_COUNTS_PER_PX * lookSpeed;
    accY += dyPx * LOOK_COUNTS_PER_PX * lookSpeed * PITCH_FACTOR;
    const ix = Math.trunc(accX);
    const iy = Math.trunc(accY);
    if (ix !== 0 || iy !== 0) {
      accX -= ix;
      accY -= iy;
      mod.roomInputLook(ix, iy);
    }
  };
  const looks = new Map(); // pointerId → last {x, y}
  const startLook = (ev) => looks.set(ev.pointerId, { x: ev.clientX, y: ev.clientY });
  const moveLook = (ev) => {
    const last = looks.get(ev.pointerId);
    if (!last) return;
    look(ev.clientX - last.x, ev.clientY - last.y);
    last.x = ev.clientX;
    last.y = ev.clientY;
  };
  const endLook = (ev) => looks.delete(ev.pointerId);

  // --- stick ------------------------------------------------------------------
  const stickState = { id: null, cx: 0, cy: 0, keys: new Set() };
  const setStickKeys = (want) => {
    for (const code of stickState.keys) if (!want.has(code)) release(code);
    for (const code of want) if (!stickState.keys.has(code)) press(code);
    stickState.keys = want;
  };
  const placeIdleStick = () => {
    stick.style.left = `calc(${STICK_RADIUS + 36}px + env(safe-area-inset-left))`;
    stick.style.top = `calc(100% - ${STICK_RADIUS + 36}px - env(safe-area-inset-bottom))`;
    knob.style.transform = "";
    stick.classList.add("idle");
  };
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
      dx /= mag;
      dy /= mag;
    }
    knob.style.transform = `translate(${dx * STICK_RADIUS}px, ${dy * STICK_RADIUS}px)`;
    const want = new Set();
    if (dy < -DEAD_ZONE) want.add("KeyW");
    if (dy > DEAD_ZONE) want.add("KeyS");
    if (dx < -DEAD_ZONE) want.add("KeyA");
    if (dx > DEAD_ZONE) want.add("KeyD");
    const run = mag >= RUN_AT;
    if (run) want.add("ShiftLeft");
    stick.classList.toggle("run", run);
    setStickKeys(want);
  });
  const endStick = (ev) => {
    if (ev.pointerId !== stickState.id) return;
    stickState.id = null;
    setStickKeys(new Set());
    stick.classList.remove("on", "run");
    placeIdleStick();
  };
  moveZone.addEventListener("pointerup", endStick);
  moveZone.addEventListener("pointercancel", endStick);
  placeIdleStick();

  // --- look zone --------------------------------------------------------------
  const lookZone = $(".zone.look");
  lookZone.addEventListener("pointerdown", (ev) => {
    ev.preventDefault();
    capture(lookZone, ev);
    startLook(ev);
  });
  lookZone.addEventListener("pointermove", moveLook);
  lookZone.addEventListener("pointerup", endLook);
  lookZone.addEventListener("pointercancel", endLook);

  // --- buttons ----------------------------------------------------------------
  const holdButton = (btn, code, { aim = false } = {}) => {
    btn.addEventListener("pointerdown", (ev) => {
      ev.preventDefault();
      ev.stopPropagation();
      capture(btn, ev);
      btn.classList.add("on");
      press(code);
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
  holdButton($(".fire"), "ControlLeft", { aim: true });
  holdButton($(".use"), "KeyE");
  holdButton($(".scores"), "Tab");
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
  tapButton($(".wnext"), () => mod.roomInputWeaponCycle(1));
  tapButton($(".wprev"), () => mod.roomInputWeaponCycle(-1));
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
