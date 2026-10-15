/**
 * On-screen coop network graph for room-web.
 * Polls `roomNetStats()` / ICE state and draws a wait-frame sparkline.
 */
(function () {
  "use strict";

  const MAX_BARS = 48;
  /** @type {number[]} */
  const waitHist = [];
  /** @type {number[]} */
  const rxHist = [];
  let lastRx = 0;
  let ice = "—";
  let timer = null;

  function ensureDom() {
    if (document.getElementById("netGraph")) return;
    const el = document.createElement("div");
    el.id = "netGraph";
    el.innerHTML =
      '<div class="ng-title">Net</div>' +
      '<div class="ng-row"><span class="ng-k">tic</span><span id="ngTic">—</span>' +
      '<span class="ng-k">wait</span><span id="ngWait">—</span></div>' +
      '<div class="ng-row"><span class="ng-k">rx/tx</span><span id="ngRxTx">—</span>' +
      '<span class="ng-k">ice</span><span id="ngIce">—</span></div>' +
      '<div class="ng-row"><span class="ng-k">held</span><span id="ngHeld">—</span>' +
      '<span class="ng-k">pos</span><span id="ngPos">—</span></div>' +
      '<div id="ngBars" class="ng-bars" aria-hidden="true"></div>' +
      '<div class="ng-hint">wait frames / tic (lower = better)</div>';
    document.body.appendChild(el);
  }

  function pushHist(arr, v) {
    arr.push(v);
    while (arr.length > MAX_BARS) arr.shift();
  }

  function renderBars() {
    const container = document.getElementById("ngBars");
    if (!container) return;
    const max = Math.max(8, ...waitHist, 1);
    container.innerHTML = waitHist
      .map((w) => {
        const pct = Math.max(6, Math.min(100, (w / max) * 100));
        const c = w < 4 ? "ok" : w < 20 ? "warn" : "bad";
        return `<i class="${c}" style="height:${pct}%" title="${w}f"></i>`;
      })
      .join("");
  }

  function tick() {
    ensureDom();
    const panel = document.getElementById("netGraph");
    if (!panel) return;

    let stats = null;
    try {
      if (typeof window.roomNetStats === "function") {
        stats = window.roomNetStats();
      }
    } catch (_) {
      stats = null;
    }

    if (!stats) {
      panel.classList.add("is-idle");
      return;
    }
    panel.classList.remove("is-idle");

    if (stats.mode === "lobby") {
      const held = document.getElementById("ngHeld");
      if (held) held.textContent = stats.held || "—";
      const tic = document.getElementById("ngTic");
      if (tic) tic.textContent = "lobby";
      return;
    }

    const wait = Number(stats.waitFrames) || 0;
    pushHist(waitHist, wait);
    const rx = Number(stats.rx) || 0;
    pushHist(rxHist, Math.max(0, rx - lastRx));
    lastRx = rx;

    const set = (id, text) => {
      const el = document.getElementById(id);
      if (el) el.textContent = text;
    };
    set("ngTic", String(stats.tic ?? "—"));
    const rtt = typeof stats.rttMs === "number" ? ` · ${Math.round(stats.rttMs)}ms` : "";
    const backlog = Number(stats.backlog) > 0 ? ` · +${stats.backlog}` : "";
    set("ngWait", `${wait}f${stats.pending ? " · pend" : ""}${rtt}${backlog}`);
    set("ngRxTx", `${stats.rx ?? 0}/${stats.tx ?? 0}`);
    if (typeof window.roomRtc?.iceState === "function") {
      try {
        ice = window.roomRtc.iceState() || ice;
      } catch (_) {
        /* ignore */
      }
    }
    set("ngIce", ice);
    set("ngHeld", stats.held || "—");
    set("ngPos", stats.pos || "—");

    panel.classList.toggle("is-stalling", wait >= 45);
    renderBars();
  }

  window.roomNetGraphNote = (note) => {
    if (note && note.ice) ice = note.ice;
  };

  function start() {
    ensureDom();
    if (timer) return;
    timer = setInterval(tick, 200);
    tick();
  }

  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", start);
  } else {
    start();
  }
})();
