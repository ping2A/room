/* Public multiplayer lobby. */
(() => {
  "use strict";

  const $ = (id) => document.getElementById(id);
  const state = {
    rooms: [],
    config: { play_base: "http://127.0.0.1:8080", node_id: "local", draining: false },
    watching: null,
    modalResolve: null,
    /** Room id from `?join=` waiting for a callsign + Enter. */
    pendingJoinId: null,
  };

  function playerName() {
    const el = $("player-name");
    let n = (el && el.value ? el.value : "").trim();
    // Strip controls / zero-widths client-side (server re-sanitizes).
    n = n.replace(/[\u0000-\u001f\u007f\u200b-\u200d\u2060\ufeff\u202a-\u202e\u2066-\u2069]/g, "");
    n = n.replace(/\s+/g, " ").trim();
    return n.slice(0, 24);
  }

  function requireCallsign() {
    const el = $("player-name");
    const n = playerName();
    if (!n) {
      flash("Pick a callsign before joining — type a name, then Enter.", "error");
      if (el) {
        el.classList.add("need-name");
        el.focus();
      }
      return null;
    }
    if (el) {
      el.classList.remove("need-name");
      el.value = n;
      try {
        localStorage.setItem("roomLobbyName", n);
      } catch (_) {
        /* ignore */
      }
    }
    return n;
  }

  function roomStub(id) {
    const listed = state.rooms.find((r) => r.id === id);
    return (
      listed || {
        id,
        map: "E1M1",
        max_players: 2,
        occupied: 0,
        ws_v2: "/v2/rooms/" + id + "/ws",
      }
    );
  }

  async function tryPendingJoin() {
    if (!state.pendingJoinId) return;
    const id = state.pendingJoinId;
    state.pendingJoinId = null;
    try {
      history.replaceState({}, "", location.pathname);
    } catch (_) {
      /* ignore */
    }
    flash("Joining room " + shortId(id) + "…");
    try {
      await playRoom(roomStub(id), { newTab: false, confirm: true });
    } catch (err) {
      flash(err.message || String(err), "error");
      state.pendingJoinId = id;
    }
  }

  function flash(msg, kind = "ok") {
    const el = $("flash");
    el.textContent = msg;
    el.className = "flash " + kind;
    el.classList.remove("hidden");
    clearTimeout(flash._t);
    flash._t = setTimeout(() => el.classList.add("hidden"), 3500);
  }

  function escapeHtml(s) {
    return String(s)
      .replace(/&/g, "&amp;")
      .replace(/</g, "&lt;")
      .replace(/>/g, "&gt;")
      .replace(/"/g, "&quot;");
  }

  function shortId(id) {
    return id && id.length > 12 ? id.slice(0, 8) + "…" : id || "—";
  }

  function wsOrigin() {
    const proto = location.protocol === "https:" ? "wss:" : "ws:";
    return proto + "//" + location.host;
  }

  function playUrl(room, ticket, opts = {}) {
    let base = (state.config.play_base || "").replace(/\/$/, "");
    // Same-origin `/play` must keep the trailing directory slash so relative
    // wasm/js assets resolve under `/play/…` (not the site root).
    if (base.endsWith("/play")) base += "/";
    else if (!base.endsWith("/")) base += "/";
    const path = room.ws_v2.startsWith("/") ? room.ws_v2 : "/" + room.ws_v2;
    const ws = room.ws_v2.startsWith("ws") ? room.ws_v2 : wsOrigin() + path;
    const q = new URLSearchParams();
    q.set("ws", ws);
    q.set("players", String(room.max_players || 2));
    q.set("name", opts.name || playerName());
    q.set("map", room.map || "E1M1");
    if (ticket) q.set("ticket", ticket);
    if (opts.spectate) q.set("spectate", "1");
    return base + "?" + q.toString();
  }

  async function ensurePlayBase() {
    const base = (state.config.play_base || "").replace(/\/$/, "");
    const probe = base.endsWith("/play") ? base + "/" : base + "/";
    try {
      const r = await fetch(probe, { method: "GET", cache: "no-store" });
      if (!r.ok) throw new Error("HTTP " + r.status);
      return true;
    } catch (_) {
      flash(
        "Game client not reachable at " +
          probe +
          " — build it with ./scripts/play-web.sh --no-open, or start the server with --web-root target/room-web",
        "error"
      );
      return false;
    }
  }

  /** Navigate to the game client without losing the user-gesture (no popup block). */
  function goPlay(url, { newTab } = { newTab: false }) {
    if (!newTab) {
      location.assign(url);
      return true;
    }
    // Open synchronously, then navigate after mint — `noopener` would return null.
    const w = window.open("about:blank", "_blank");
    if (!w) {
      flash("Popup blocked — use Play (this tab), or allow popups. Link: " + url, "error");
      return false;
    }
    try {
      w.opener = null;
    } catch (_) {
      /* ignore */
    }
    w.location.href = url;
    return true;
  }

  async function api(path, opts = {}) {
    const res = await fetch(path, {
      ...opts,
      headers: {
        Accept: "application/json",
        ...(opts.body ? { "Content-Type": "application/json" } : {}),
        ...(opts.headers || {}),
      },
    });
    if (!res.ok) {
      const err = new Error(res.status + " " + path);
      err.code = res.status;
      throw err;
    }
    if (res.status === 204) return null;
    const ct = res.headers.get("content-type") || "";
    return ct.includes("json") ? res.json() : res.text();
  }

  function openModal({ title, bodyHtml, confirmLabel = "Confirm" }) {
    return new Promise((resolve) => {
      state.modalResolve = resolve;
      $("modal-title").textContent = title;
      $("modal-body").innerHTML = bodyHtml;
      $("modal-confirm").textContent = confirmLabel;
      $("modal").showModal();
    });
  }

  $("modal-form").addEventListener("submit", (ev) => {
    const value = ev.submitter && ev.submitter.value ? ev.submitter.value : "cancel";
    const resolve = state.modalResolve;
    state.modalResolve = null;
    if (!resolve) return;
    if (value === "confirm") {
      const data = {};
      for (const el of $("modal-body").querySelectorAll("[name]")) {
        data[el.name] = el.type === "checkbox" ? el.checked : el.value;
      }
      resolve(data);
    } else {
      resolve(null);
    }
  });

  function cleanCallsign(raw) {
    return String(raw || "")
      .replace(/[\u0000-\u001f\u007f\u200b-\u200d\u2060\ufeff\u202a-\u202e\u2066-\u2069]/g, "")
      .replace(/\s+/g, " ")
      .trim()
      .slice(0, 24);
  }

  /** Always ask (pre-filled) — joining someone else's pit must not reuse a stale name silently. */
  async function confirmCallsign({ title, confirmLabel, prefill, remember }) {
    const data = await openModal({
      title,
      confirmLabel,
      bodyHtml:
        "<p>Other players see this name on the scoreboard and in the kill feed.</p>" +
        '<label class="field"><span>Callsign</span>' +
        '<input name="name" required maxlength="24" autocomplete="nickname" autofocus value="' +
        escapeHtml(prefill || "") +
        '"/></label>',
    });
    if (!data) return null;
    const n = cleanCallsign(data.name);
    if (!n) {
      flash("Callsign required.", "error");
      return null;
    }
    if (remember) {
      const el = $("player-name");
      if (el) {
        el.value = n;
        el.classList.remove("need-name");
      }
      try {
        localStorage.setItem("roomLobbyName", n);
      } catch (_) {
        /* ignore */
      }
    }
    return n;
  }

  /** Open a tab right after a confirm click (still inside the user activation window). */
  function openPendingTab() {
    const w = window.open("about:blank", "_blank");
    if (w) {
      try {
        w.opener = null;
      } catch (_) {
        /* ignore */
      }
    }
    return w;
  }

  async function mintTicket(roomId, name) {
    const callsign = name || requireCallsign();
    if (!callsign) throw new Error("callsign required");
    const join = await api("/api/rooms/" + encodeURIComponent(roomId) + "/join", {
      method: "POST",
      body: JSON.stringify({ name: callsign }),
    });
    return { join, name: callsign };
  }

  async function playRoom(room, { newTab = false, confirm = false } = {}) {
    try {
      if (room.occupied >= room.max_players) {
        flash("Pit full — Spectate the slaughter, or open another pit", "error");
        return;
      }
      let callsign;
      if (newTab) {
        // Second local seat must pick its own pseudo (not reuse the host's).
        callsign = await confirmCallsign({
          title: "Callsign for this seat",
          confirmLabel: "Join",
          prefill: "",
          remember: false,
        });
      } else if (confirm || (room.occupied || 0) > 0 || room.started) {
        callsign = await confirmCallsign({
          title: "Pick your callsign",
          confirmLabel: "Enter the pit",
          prefill: playerName(),
          remember: true,
        });
      } else {
        callsign = requireCallsign();
      }
      if (!callsign) return;
      // Open the tab before any await so the confirm click still counts as a gesture.
      let pending = null;
      if (newTab) {
        pending = openPendingTab();
        if (!pending) {
          flash("Popup blocked — trying this tab instead…", "error");
          newTab = false;
        }
      }
      if (!(await ensurePlayBase())) {
        if (pending) pending.close();
        return;
      }
      const { join, name } = await mintTicket(room.id, callsign);
      const url = playUrl(
        {
          ...room,
          ws_v2: join.ws_v2 || room.ws_v2,
        },
        join.ticket,
        { name }
      );
      if (newTab && pending) {
        pending.location.href = url;
        flash("Opened game client in a new tab");
      } else {
        flash("Joining…");
        goPlay(url, { newTab: false });
      }
    } catch (err) {
      flash(err.message || String(err), "error");
    }
  }

  /** Live multi-view spectator client (no seat, no ticket). */
  async function spectateRoom(room) {
    const name = await confirmCallsign({
      title: "Spectate as…",
      confirmLabel: "Watch live",
      prefill: playerName(),
      remember: true,
    });
    if (!name) return;
    const pending = openPendingTab();
    if (!(await ensurePlayBase())) {
      if (pending) pending.close();
      return;
    }
    const url = playUrl(room, null, { name, spectate: true });
    if (pending) {
      pending.location.href = url;
      flash("Spectator view opened in a new tab");
    } else {
      goPlay(url, { newTab: false });
    }
  }

  function watchRoom(room) {
    state.watching = room.id;
    $("watch-panel").classList.remove("hidden");
    renderWatch();
  }

  function renderWatch() {
    const room = state.rooms.find((r) => r.id === state.watching);
    const body = $("watch-body");
    if (!room) {
      body.innerHTML = "<p class='muted'>Room closed or no longer public.</p>";
      $("watch-play").disabled = true;
      return;
    }
    const openSeats = Math.max(0, room.max_players - room.occupied);
    body.innerHTML =
      '<div class="row"><span class="k">Map</span><span>' +
      escapeHtml(room.map) +
      "</span></div>" +
      '<div class="row"><span class="k">Skill</span><span>' +
      escapeHtml(room.skill) +
      "</span></div>" +
      '<div class="row"><span class="k">Seats</span><span>' +
      room.occupied +
      " / " +
      room.max_players +
      " (" +
      openSeats +
      " free)</span></div>" +
      '<div class="row"><span class="k">State</span><span>' +
      (room.started ? "in game" : "lobby") +
      "</span></div>" +
      '<div class="row"><span class="k">Room</span><span class="id">' +
      escapeHtml(room.id) +
      "</span></div>" +
      '<p class="muted">Watch live opens a spectator client: the whole match is replayed from tic 0, then every player is shown side by side (G toggles grid / follow, 1-4 or click picks a player, Tab shows scores). Join &amp; play takes a free seat.</p>';
    $("watch-play").disabled = openSeats === 0;
  }

  async function createRoom() {
    if (!requireCallsign()) return;
    const data = await openModal({
      title: "Open a pit",
      confirmLabel: "Spill blood",
      bodyHtml:
        '<label class="field"><span>Map</span><input name="map" value="E1M1" required/></label>' +
        '<label class="field"><span>Skill</span><input name="skill" type="number" min="1" max="5" value="3"/></label>' +
        '<label class="field"><span>Max players</span><input name="max_players" type="number" min="2" max="8" value="2"/></label>' +
        '<label class="field"><span>Visibility</span><select name="visibility"><option value="public">public</option><option value="private">private</option></select></label>',
    });
    if (!data) return;
    try {
      const created = await api("/api/rooms", {
        method: "POST",
        body: JSON.stringify({
          map: data.map || "E1M1",
          skill: Number(data.skill) || 3,
          max_players: Number(data.max_players) || 2,
          visibility: data.visibility || "public",
        }),
      });
      flash("Created " + shortId(created.id));
      await refresh();
      const room = {
        id: created.id,
        map: created.settings.map,
        skill: created.settings.skill,
        max_players: created.settings.max_players,
        occupied: 0,
        started: false,
        ws_v2: created.ws_v2,
        ws_legacy: created.ws_legacy,
      };
      await playRoom(room);
    } catch (err) {
      flash(err.message || String(err), "error");
    }
  }

  async function joinById() {
    const data = await openModal({
      title: "Join by pit id",
      confirmLabel: "Enter the pit",
      bodyHtml:
        '<label class="field"><span>Pit id</span><input name="id" required placeholder="hex room id"/></label>',
    });
    if (!data || !data.id) return;
    const id = data.id.trim();
    try {
      await playRoom(
        {
          id,
          map: "E1M1",
          max_players: 2,
          occupied: 0,
          ws_v2: "/v2/rooms/" + id + "/ws",
        },
        { confirm: true }
      );
    } catch (err) {
      flash(err.message || String(err), "error");
    }
  }

  function renderRooms() {
    const grid = $("room-grid");
    grid.innerHTML = "";
    $("room-count").textContent = state.rooms.length + " open";
    $("rooms-empty").classList.toggle("hidden", state.rooms.length > 0);
    for (const room of state.rooms) {
      const el = document.createElement("article");
      el.className = "room";
      const full = room.occupied >= room.max_players;
      el.innerHTML =
        "<div><h3>" +
        escapeHtml(room.map) +
        '</h3><p class="meta">skill ' +
        escapeHtml(room.skill) +
        " · " +
        room.occupied +
        "/" +
        room.max_players +
        ' seats</p><p class="id" title="' +
        escapeHtml(room.id) +
        '">' +
        escapeHtml(shortId(room.id)) +
        '</p><div class="badges">' +
        (room.started
          ? '<span class="badge live">in game</span>'
          : '<span class="badge open">lobby</span>') +
        (full ? '<span class="badge">full</span>' : "") +
        "</div></div>";
      const actions = document.createElement("div");
      actions.className = "room-actions";
      const play = document.createElement("button");
      play.type = "button";
      play.className = "btn primary small";
      play.textContent = "Enter";
      play.title = "Join in this tab";
      play.disabled = full;
      play.addEventListener("click", () => playRoom(room, { newTab: false }));
      const playTab = document.createElement("button");
      playTab.type = "button";
      playTab.className = "btn small";
      playTab.textContent = "New tab";
      playTab.title = "Join in a new tab";
      playTab.disabled = full;
      playTab.addEventListener("click", () => playRoom(room, { newTab: true }));
      const watch = document.createElement("button");
      watch.type = "button";
      watch.className = "btn small";
      watch.textContent = "Spectate";
      watch.title = "Watch every player live (multi-view)";
      watch.addEventListener("click", () => {
        watchRoom(room);
        void spectateRoom(room);
      });
      actions.append(play, playTab, watch);
      el.appendChild(actions);
      grid.appendChild(el);
    }
    if (state.watching) renderWatch();
  }

  async function refresh() {
    try {
      const [cfg, rooms] = await Promise.all([api("/api/lobby"), api("/api/rooms")]);
      state.config = cfg;
      state.rooms = rooms || [];
      $("node-pill").textContent = "node " + (cfg.node_id || "—");
      $("drain-pill").classList.toggle("hidden", !cfg.draining);
      $("drain-pill").classList.toggle("warn", !!cfg.draining);
      $("play-base-label").textContent = cfg.play_base || "—";
      if (cfg.play_base) $("btn-editor").href = `${cfg.play_base.replace(/\/$/, "")}/editor.html`;
      renderRooms();
    } catch (err) {
      flash(err.message || String(err), "error");
    }
  }

  $("btn-create").addEventListener("click", () => createRoom());
  $("btn-join-id").addEventListener("click", () => joinById());
  $("btn-refresh").addEventListener("click", () => refresh());
  $("watch-close").addEventListener("click", () => {
    state.watching = null;
    $("watch-panel").classList.add("hidden");
  });
  $("watch-play").addEventListener("click", () => {
    const room = state.rooms.find((r) => r.id === state.watching);
    if (room) playRoom(room, { confirm: true });
  });
  if ($("watch-live")) {
    $("watch-live").addEventListener("click", () => {
      const room = state.rooms.find((r) => r.id === state.watching);
      if (room) void spectateRoom(room);
    });
  }

  const saved = localStorage.getItem("roomLobbyName") || "";
  if (saved) $("player-name").value = saved;
  $("player-name").addEventListener("change", () => {
    localStorage.setItem("roomLobbyName", $("player-name").value);
  });

  function consumeJoinQuery() {
    const params = new URLSearchParams(location.search);
    const joinId = (params.get("join") || "").trim();
    if (!joinId) return;
    state.pendingJoinId = joinId;
    // The callsign modal always opens (pre-filled) before a ticket is minted.
    void tryPendingJoin();
  }

  $("player-name").addEventListener("keydown", (ev) => {
    if (ev.key === "Enter") {
      ev.preventDefault();
      if (state.pendingJoinId) {
        void tryPendingJoin();
      } else {
        requireCallsign();
      }
    }
  });

  refresh()
    .then(() => consumeJoinQuery())
    .catch(() => {});
  setInterval(refresh, 4000);
})();
