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
    lbMode: "all",
    brTop: [],
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
      flash("Enter a player name before joining, then press Enter.", "error");
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
    flash("Joining game " + shortId(id) + "…");
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

  /** Match length for people: "45 s", "3 min 30 s", "20 min". */
  function fmtDuration(secs) {
    const n = Math.max(0, Math.round(Number(secs) || 0));
    const m = Math.floor(n / 60);
    const r = n % 60;
    if (m === 0) return r + " s";
    return r ? m + " min " + r + " s" : m + " min";
  }

  /** Room time limit in seconds (older servers send minutes). */
  function roomLimitSecs(room) {
    if (room.time_limit_secs) return room.time_limit_secs;
    return room.time_limit ? room.time_limit * 60 : 0;
  }

  function shortId(id) {
    return id && id.length > 12 ? id.slice(0, 8) + "…" : id || "—";
  }

  function wsOrigin() {
    const proto = location.protocol === "https:" ? "wss:" : "ws:";
    return proto + "//" + location.host;
  }

  function isLoopbackHost(host) {
    return host === "localhost" || host === "127.0.0.1" || host === "::1";
  }

  /**
   * Game-client origin for links and the reachability probe.
   * `play_base` is often `http://127.0.0.1:8765/play` while the lobby was
   * opened as `http://localhost:8765`. Those are different origins, and the
   * lobby CSP (`connect-src 'self'`) blocks the probe even when `/play/` is
   * served by this process. Stay on the host the page was opened with.
   */
  function clientBase() {
    let base = (state.config.play_base || "").replace(/\/$/, "");
    // Same-origin `/play` must keep the trailing directory slash so relative
    // wasm/js assets resolve under `/play/…` (not the site root).
    if (base.endsWith("/play")) base += "/";
    else if (base && !base.endsWith("/")) base += "/";
    try {
      const configured = new URL(base, location.href);
      const here = new URL(location.href);
      const samePort =
        (configured.port || (configured.protocol === "https:" ? "443" : "80")) ===
        (here.port || (here.protocol === "https:" ? "443" : "80"));
      const sameLoopback =
        isLoopbackHost(configured.hostname) && isLoopbackHost(here.hostname);
      if (
        (configured.pathname === "/play" || configured.pathname === "/play/") &&
        samePort &&
        (configured.hostname === here.hostname || sameLoopback)
      ) {
        return here.origin + "/play/";
      }
      return configured.href;
    } catch (_) {
      return base;
    }
  }

  function playUrl(room, ticket, opts = {}) {
    const base = clientBase();
    const path = room.ws_v2.startsWith("/") ? room.ws_v2 : "/" + room.ws_v2;
    const ws = room.ws_v2.startsWith("ws") ? room.ws_v2 : wsOrigin() + path;
    const q = new URLSearchParams();
    q.set("ws", ws);
    q.set("players", String(room.max_players || 2));
    q.set("name", opts.name || playerName());
    q.set("map", room.map || "E1M1");
    if (room.game === "duke3d") q.set("iwad", "duke3d.grp");
    if (ticket) q.set("ticket", ticket);
    if (opts.spectate) q.set("spectate", "1");
    return base + "?" + q.toString();
  }

  async function ensurePlayBase() {
    const probe = clientBase();
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
      let detail = "";
      try {
        detail = (await res.text()).trim();
      } catch (_) {}
      const err = new Error(detail && detail.length < 300 ? detail : res.status + " " + path);
      err.code = res.status;
      throw err;
    }
    if (res.status === 204) return null;
    const ct = res.headers.get("content-type") || "";
    return ct.includes("json") ? res.json() : res.text();
  }

  function openModal({ title, bodyHtml, confirmLabel = "Confirm", onReady }) {
    return new Promise((resolve) => {
      state.modalResolve = resolve;
      $("modal-title").textContent = title;
      $("modal-body").innerHTML = bodyHtml;
      $("modal-confirm").textContent = confirmLabel;
      $("modal").showModal();
      if (typeof onReady === "function") {
        try {
          onReady($("modal-body"));
        } catch (_) {
          /* ignore UI wiring errors */
        }
      }
    });
  }

  const VANILLA_MOD_INFO = {
    title: "None (vanilla)",
    description: "Classic Doom / Freedoom — no gameplay mod.",
  };
  /** Lookup table for mod blurbs (API rows). */
  function modInfoByName(mods) {
    const map = { "": VANILLA_MOD_INFO };
    for (const m of mods) {
      map[m.name] = {
        title: m.title || m.name,
        description: m.description || "",
        solo: m.solo !== false,
      };
    }
    return map;
  }

  function paintModBlurb(el, info) {
    if (!el) return;
    if (!info || (!info.description && !info.title)) {
      el.hidden = true;
      el.textContent = "";
      return;
    }
    el.hidden = false;
    const title = info.title ? escapeHtml(info.title) : "";
    const desc = info.description ? escapeHtml(info.description) : "";
    el.innerHTML =
      (title ? '<strong class="mod-blurb-title">' + title + "</strong>" : "") +
      (desc ? '<p class="mod-blurb-text">' + desc + "</p>" : "");
  }

  /** Mod `<select>` + live blurb + optional Voxel pack checkbox. */
  function modPickerHtml(mods, { name = "mod", voxelPack = true } = {}) {
    const options =
      '<option value="">None (vanilla)</option>' +
      mods
        .map(
          (m) =>
            '<option value="' +
            escapeHtml(m.name) +
            '">' +
            escapeHtml(m.title || m.name) +
            "</option>"
        )
        .join("");
    const packHtml = voxelPack
      ? '<label class="field check"><span><input type="checkbox" name="voxel" value="1"/> Voxel sprites</span>' +
        '<span class="muted" style="font-weight:400;text-transform:none;letter-spacing:0.02em">Cheello voxel meshes — stacks with the gameplay mod above</span></label>'
      : "";
    const html =
      '<label class="field"><span>Mod</span><select name="' +
      escapeHtml(name) +
      '">' +
      options +
      "</select></label>" +
      '<div class="mod-blurb" data-mod-blurb hidden></div>' +
      packHtml;
    const info = modInfoByName(mods);
    function wire(body) {
      const sel = body.querySelector('select[name="' + name + '"]');
      const blurb = body.querySelector("[data-mod-blurb]");
      const game = body.querySelector('select[name="game"]');
      const voxel = body.querySelector('input[name="voxel"]');
      const refresh = () => {
        if (game && game.value === "duke3d") {
          paintModBlurb(blurb, {
            title: "Duke Nukem 3D",
            description: "Build engine — mods are Doom only; this run is vanilla Duke.",
          });
          if (sel) sel.disabled = true;
          if (voxel) {
            voxel.checked = false;
            voxel.disabled = true;
          }
          return;
        }
        if (sel) sel.disabled = false;
        if (voxel) voxel.disabled = false;
        let blurbInfo = info[sel ? sel.value : ""] || VANILLA_MOD_INFO;
        if (voxel && voxel.checked) {
          const packNote = " + Voxel sprites (Cheello meshes).";
          blurbInfo = {
            title: blurbInfo.title,
            description: (blurbInfo.description || "") + packNote,
          };
        }
        paintModBlurb(blurb, blurbInfo);
      };
      if (sel) sel.addEventListener("change", refresh);
      if (game) game.addEventListener("change", refresh);
      if (voxel) voxel.addEventListener("change", refresh);
      refresh();
    }
    return { html, wire };
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

  /** Always ask (pre-filled) — joining someone else's game must not reuse a stale name silently. */
  async function confirmCallsign({ title, confirmLabel, prefill, remember }) {
    const data = await openModal({
      title,
      confirmLabel,
      bodyHtml:
        "<p>Other players see this name on the scoreboard and in the kill feed.</p>" +
        '<label class="field"><span>Player name</span>' +
        '<input name="name" required maxlength="24" autocomplete="nickname" autofocus value="' +
        escapeHtml(prefill || "") +
        '"/></label>',
    });
    if (!data) return null;
    const n = cleanCallsign(data.name);
    if (!n) {
      flash("Player name required.", "error");
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

  async function playRoom(room, { newTab = false, confirm = false, ticket = null, callsign = null } = {}) {
    try {
      if (room.occupied >= room.max_players) {
        flash("Game full — spectate it, or host another game.", "error");
        return;
      }
      let callsign;
      if (newTab) {
        // Second local seat must pick its own pseudo (not reuse the host's).
        callsign = await confirmCallsign({
          title: "Name for this player",
          confirmLabel: "Join",
          prefill: "",
          remember: false,
        });
      } else if (confirm || (room.occupied || 0) > 0 || room.started) {
        callsign = await confirmCallsign({
          title: "Your player name",
          confirmLabel: "Join game",
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
      let join;
      let name = callsign;
      if (ticket && callsign) {
        join = { ticket, ws_v2: room.ws_v2 };
      } else {
        const minted = await mintTicket(room.id, callsign);
        join = minted.join;
        name = minted.name;
      }
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

  /** Anyone can watch: no seat, no ticket, no name prompt. */
  function watchUrl(room, name) {
    return playUrl(room, null, { name: name || "Spectator", spectate: true });
  }

  /** Live multi-view spectator client (no seat, no ticket). */
  async function spectateRoom(room) {
    const name = playerName() || "Spectator";
    const pending = openPendingTab();
    if (!(await ensurePlayBase())) {
      if (pending) pending.close();
      return;
    }
    const url = watchUrl(room, name);
    if (pending) {
      pending.location.href = url;
      flash("Watching in a new tab");
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
      body.innerHTML = "<p class='muted'>Game over or no longer listed.</p>";
      $("watch-play").disabled = true;
      return;
    }
    const openSeats = Math.max(0, room.max_players - room.occupied);
    body.innerHTML =
      '<div class="row"><span class="k">Game</span><span>' +
      escapeHtml(gameLabel(room)) +
      "</span></div>" +
      '<div class="row"><span class="k">Map</span><span>' +
      escapeHtml(room.map) +
      "</span></div>" +
      '<div class="row"><span class="k">Mode</span><span>' +
      escapeHtml(modeLabel(room)) +
      "</span></div>" +
      '<div class="row"><span class="k">Skill</span><span>' +
      escapeHtml(skillLabel(room.game, room.skill)) +
      "</span></div>" +
      (room.mod
        ? '<div class="row"><span class="k">Mod</span><span>' + escapeHtml(modLabel(room)) + "</span></div>"
        : "") +
      (room.packs && room.packs.length
        ? '<div class="row"><span class="k">Packs</span><span>' +
          escapeHtml(room.packs.join(", ")) +
          "</span></div>"
        : "") +
      '<div class="row"><span class="k">Players</span><span>' +
      room.occupied +
      " / " +
      room.max_players +
      " (" +
      openSeats +
      " free)</span></div>" +
      (roomLimitSecs(room)
        ? '<div class="row"><span class="k">Time limit</span><span>' + escapeHtml(fmtDuration(roomLimitSecs(room))) + "</span></div>"
        : "") +
      '<div class="row"><span class="k">Status</span><span>' +
      (room.started ? "in progress" : "waiting for players") +
      "</span></div>" +
      '<div class="row"><span class="k">Game ID</span><span class="id">' +
      escapeHtml(room.id) +
      "</span></div>" +
      '<p class="muted">Watch opens the game in a new tab without taking a slot: it catches up from the start, then shows every player side by side. G switches between the grid and one player, 1-4 or a click picks a player, Tab shows the scores. Copy watch link to send the live game to anyone.</p>';
    $("watch-play").disabled = openSeats === 0;
  }

  /** Mods admins allowed on this server (`GET /api/mods`); every player downloads the room's mod. */
  function loadMods() {
    return api("/api/mods").catch(() => []);
  }

  const GAME_LABELS = { doom: "Doom", duke3d: "Duke Nukem 3D" };
  const MODE_LABELS = {
    doom: { coop: "Cooperative", deathmatch: "Deathmatch" },
    duke3d: { coop: "Cooperative", deathmatch: "DukeMatch", dukematch_nospawn: "DukeMatch (no spawn)" },
  };
  // In-game skill names. Duke menu skills are `ud.player_skill` 1–4, which
  // Room's skill 2–5 maps to.
  const SKILL_LABELS = {
    doom: ["I'm too young to die", "Hey, not too rough", "Hurt me plenty", "Ultra-Violence", "Nightmare!"],
    duke3d: [null, "Piece of Cake", "Let's Rock", "Come Get Some", "Damn I'm Good"],
  };

  // Duke `ud.coop` value → lobby mode name.
  const DUKE_MODE_VALUES = ["deathmatch", "coop", "dukematch_nospawn"];

  function modeLabel(room) {
    const game = room.game === "duke3d" ? "duke3d" : "doom";
    const mode = MODE_LABELS[game][room.mode || "coop"] || MODE_LABELS[game].coop;
    return room.friendly_fire ? mode + ", friendly fire" : mode;
  }

  function skillLabel(game, skill) {
    const list = SKILL_LABELS[game === "duke3d" ? "duke3d" : "doom"];
    return list[(Number(skill) || 3) - 1] || "skill " + skill;
  }

  function skillOptions(game, selected) {
    const list = SKILL_LABELS[game === "duke3d" ? "duke3d" : "doom"];
    return list
      .map((label, i) =>
        label ? '<option value="' + (i + 1) + '"' + (i + 1 === selected ? " selected" : "") + ">" + escapeHtml(label) + "</option>" : ""
      )
      .join("");
  }

  function gameLabel(room) {
    return GAME_LABELS[room.game] || GAME_LABELS.doom;
  }

  function modLabel(room) {
    return room.mod ? room.mod.title || room.mod.name : "";
  }

  /** Admin presets (`GET /api/presets`): IWAD + PK3/WAD files + allowed maps. */
  function loadPresets() {
    return api("/api/presets").catch(() => ({ require_preset: false, presets: [] }));
  }

  function fmtBytes(n) {
    if (!n) return "";
    if (n >= 1 << 30) return (n / (1 << 30)).toFixed(1) + " GB";
    if (n >= 1 << 20) return (n / (1 << 20)).toFixed(1) + " MB";
    return Math.max(1, Math.round(n / 1024)) + " KB";
  }

  /** Preset `<select>` + map picker that follows it; hides the free fields for presets. */
  function presetPickerHtml(pp) {
    const presets = pp.presets || [];
    if (!presets.length) return { html: "", wire() {} };
    const opts =
      (pp.require_preset ? "" : '<option value="">Custom (type a map)</option>') +
      presets
        .map((p) => '<option value="' + escapeHtml(p.id) + '">' + escapeHtml(p.title) + ' · ' + (p.game === "duke3d" ? "Duke Nukem 3D" : "Doom") + "</option>")
        .join("");
    const html =
      '<label class="field"><span>Preset</span><select name="preset">' + opts + "</select></label>" +
      '<div class="mod-blurb" data-preset-blurb hidden></div>' +
      '<label class="field" data-preset-map style="display:none"><span>Map</span><select name="preset_map"></select></label>';
    function wire(body) {
      const sel = body.querySelector('select[name="preset"]');
      const blurb = body.querySelector("[data-preset-blurb]");
      const mapRow = body.querySelector("[data-preset-map]");
      const mapSel = body.querySelector('select[name="preset_map"]');
      const custom = body.querySelectorAll("[data-custom]");
      const skill = body.querySelector('[name="skill"]');
      const game = body.querySelector('select[name="game"]');
      const modeSel = body.querySelector('select[name="mode"]');
      const friendlyFire = body.querySelector('input[name="duke_friendly_fire"]');
      const refresh = () => {
        const p = presets.find((x) => x.id === sel.value);
        // `.field` sets display, which beats the `hidden` attribute: use style.
        const show = (el, on) => {
          el.hidden = !on;
          el.style.display = on ? "" : "none";
        };
        for (const el of custom) show(el, !p);
        if (game) {
          game.value = p?.game || game.value || "doom";
          game.dispatchEvent(new Event("change"));
        }
        // The preset may fix the game mode (Duke presets always do); then
        // the mode list shows it and cannot be changed.
        const fixedMode = !p ? null : p.game === "duke3d" ? DUKE_MODE_VALUES[p.duke_game_mode ?? 1] || "coop" : p.doom_mode || null;
        if (modeSel) {
          if (fixedMode) modeSel.value = fixedMode;
          modeSel.disabled = !!fixedMode;
          modeSel.title = fixedMode ? "Set by the preset" : "";
          modeSel.dispatchEvent(new Event("change"));
        }
        if (friendlyFire && p) {
          friendlyFire.checked = !!p.duke_friendly_fire;
          friendlyFire.disabled = p.game === "duke3d";
        } else if (friendlyFire) {
          friendlyFire.disabled = false;
        }
        const free = body.querySelector('input[name="map"]');
        if (free) free.required = !p;
        if (!p) {
          blurb.hidden = true;
          show(mapRow, false);
          return;
        }
        const files = [p.iwad && p.iwad.file, ...(p.content || []).map((c) => c.file)].filter(Boolean);
        blurb.hidden = false;
        blurb.innerHTML =
          '<strong class="mod-blurb-title">' + escapeHtml(p.title) + "</strong>" +
          (p.description ? '<p class="mod-blurb-text">' + escapeHtml(p.description) + "</p>" : "") +
          '<p class="mod-blurb-text muted">' + escapeHtml(files.join(" + ") || "default game") +
          (p.download_bytes ? " · download " + escapeHtml(fmtBytes(p.download_bytes)) : "") +
          (p.mod ? " · mod " + escapeHtml(p.mod) : "") + "</p>";
        show(mapRow, true);
        if (p.maps && p.maps.length) {
          mapSel.innerHTML = p.maps
            .map((m) => '<option value="' + escapeHtml(m) + '"' + (m === p.default_map ? " selected" : "") + ">" + escapeHtml(m) + "</option>")
            .join("");
          mapSel.disabled = false;
        } else {
          mapSel.innerHTML = '<option value="">' + escapeHtml(p.default_map || "default") + "</option>";
          mapSel.disabled = false;
          show(mapRow, false);
          for (const el of body.querySelectorAll("[data-free-map]")) show(el, true);
          if (free) {
            free.required = true;
            if (p.default_map) free.value = p.default_map;
          }
        }
        if (skill && p.skill) skill.value = String(p.skill);
      };
      sel.addEventListener("change", refresh);
      refresh();
    }
    return { html, wire };
  }

  async function createRoom() {
    if (!requireCallsign()) return;
    const [mods, pp] = await Promise.all([loadMods(), loadPresets()]);
    const picker = modPickerHtml(mods, { voxelPack: true });
    const presetPicker = presetPickerHtml(pp);
    const data = await openModal({
      title: "Host a netgame",
      confirmLabel: "Start game",
      bodyHtml:
        presetPicker.html +
        '<label class="field" data-custom><span>Game</span><select name="game"><option value="doom">Doom</option><option value="duke3d">Duke Nukem 3D (no mods)</option></select></label>' +
        '<label class="field"><span>Game mode</span><select name="mode"></select></label>' +
        '<label class="field check" data-duke-coop><span><input name="duke_friendly_fire" type="checkbox"/> Friendly fire</span></label>' +
        '<label class="field" data-custom data-free-map><span>Map</span><input name="map" value="E1M1" autocomplete="off"/></label>' +
        '<label class="field"><span>Skill</span><select name="skill"></select></label>' +
        '<label class="field"><span>Players</span><input name="max_players" type="number" min="2" max="64" value="4"/></label>' +
        '<label class="field"><span>Time limit (minutes)</span><input name="time_limit" type="number" min="1" max="180" value="' +
        escapeHtml(state.config.room_time_limit_mins || 20) +
        '"/></label>' +
        '<p class="muted time-limit-note" data-royale-time hidden>Battle Royale: under 8 minutes, the zone speeds up so it closes before time runs out.</p>' +
        '<label class="field"><span>Listing</span><select name="visibility"><option value="public">Public — shown on this page</option><option value="private">Private — join by game ID only</option></select></label>' +
        '<div data-custom>' + picker.html + "</div>" +
        '<div class="field start-delay" data-bots hidden>' +
        '<label class="check"><span><input type="checkbox" name="fill_bots" checked/> Fill empty slots with bots</span></label>' +
        '<label class="field" data-bot-skill><span>Bot skill</span><select name="bot_skill">' +
        '<option value="1">Easy — slow to aim, misses often</option>' +
        '<option value="2" selected>Normal</option>' +
        '<option value="3">Hard — quick aim, hunts you down</option>' +
        '<option value="4">Nightmare — near-instant aim</option>' +
        "</select></label>" +
        '<label for="host-start-delay"><span>Starts in (seconds)</span></label>' +
        '<div class="time-limit-row"><input id="host-start-delay" name="start_delay_secs" type="number" min="5" max="600" step="5" value="' +
        escapeHtml(state.config.bot_fill_secs || 60) +
        '"/><output data-delay-readout></output></div>' +
        '<div class="time-limit-picks" role="group" aria-label="Quick picks">' +
        [10, 30, 60, 120]
          .map((v) => '<button type="button" class="btn small ghost" data-secs="' + v + '">' + fmtDuration(v) + "</button>")
          .join("") +
        "</div>" +
        '<p class="muted time-limit-note" data-delay-note></p></div>',
      onReady: (body) => {
        // Bot-fill start delay: seconds from the first join to the start.
        const delay = body.querySelector('input[name="start_delay_secs"]');
        const delayOut = body.querySelector("[data-delay-readout]");
        const delayNote = body.querySelector("[data-delay-note]");
        const fillBox = body.querySelector('input[name="fill_bots"]');
        const syncDelay = () => {
          const v = Number(delay.value) || 0;
          const ok = v >= 5 && v <= 600;
          delayOut.textContent = ok ? "= " + fmtDuration(v) : "5 s to 10 min";
          for (const b of body.querySelectorAll("[data-secs]")) {
            b.setAttribute("aria-pressed", String(Number(b.dataset.secs) === v));
          }
          delayNote.textContent = fillBox.checked
            ? "The match starts " + (ok ? fmtDuration(v) : "…") + " after the first player joins; bots take the slots nobody filled."
            : "The match starts " + (ok ? fmtDuration(v) : "…") + " after the first player joins.";
        };
        delay.addEventListener("input", syncDelay);
        fillBox.addEventListener("change", () => {
          show(body.querySelector("[data-bot-skill]"), fillBox.checked);
        });
        fillBox.addEventListener("change", syncDelay);
        for (const b of body.querySelectorAll("[data-secs]")) {
          b.addEventListener("click", () => {
            delay.value = b.dataset.secs;
            syncDelay();
          });
        }
        syncDelay();
        const game = body.querySelector('select[name="game"]');
        const mode = body.querySelector('select[name="mode"]');
        const skill = body.querySelector('select[name="skill"]');
        const map = body.querySelector('input[name="map"]');
        const ffRow = body.querySelector("[data-duke-coop]");
        const show = (el, on) => {
          if (!el) return;
          el.hidden = !on;
          el.style.display = on ? "" : "none";
        };
        let lastGame = "";
        const updateGame = () => {
          const g = game && game.value === "duke3d" ? "duke3d" : "doom";
          if (g !== lastGame) {
            const keepMode = mode.value;
            mode.innerHTML = Object.entries(MODE_LABELS[g])
              .map(([v, label]) => '<option value="' + v + '">' + escapeHtml(label) + "</option>")
              .join("");
            if (MODE_LABELS[g][keepMode]) mode.value = keepMode;
            const keepSkill = Number(skill.value) || 3;
            skill.innerHTML = skillOptions(g, g === "duke3d" ? Math.max(2, keepSkill) : keepSkill);
            // Swap the default start map between the games.
            if (map && lastGame && (map.value === "" || /^(E1M1|MAP01|E1L1)$/i.test(map.value))) {
              map.value = g === "duke3d" ? "E1L1" : "E1M1";
            }
            lastGame = g;
          }
          show(ffRow, g === "duke3d" && mode.value === "coop");
        };
        if (game) game.addEventListener("change", updateGame);
        mode.addEventListener("change", updateGame);
        updateGame();
        picker.wire(body);
        presetPicker.wire(body);
        // Battle Royale: offer bot fill (the deterministic room-sim bot brain).
        const modSel = body.querySelector('select[name="mod"]');
        const botsRow = body.querySelector("[data-bots]");
        const presetSel = body.querySelector('select[name="preset"]');
        const updateBots = () => {
          const preset = presetSel ? pp.presets.find((x) => x.id === presetSel.value) : null;
          const modName = preset ? preset.mod || "" : modSel ? modSel.value : "";
          const g = preset ? preset.game : game ? game.value : "doom";
          // Sim bots play Battle Royale and Doom deathmatch (they hunt players).
          show(botsRow, g !== "duke3d" && (modName === "royale" || mode.value === "deathmatch"));
          show(body.querySelector("[data-royale-time]"), g !== "duke3d" && modName === "royale");
        };
        if (modSel) modSel.addEventListener("change", updateBots);
        mode.addEventListener("change", updateBots);
        if (game) game.addEventListener("change", updateBots);
        if (presetSel) presetSel.addEventListener("change", updateBots);
        updateBots();
      },
    });
    if (!data) return;
    try {
      const preset = data.preset || "";
      const selectedPreset = pp.presets.find((p) => p.id === preset);
      const game = selectedPreset?.game || data.game || "doom";
      const doom = game !== "duke3d";
      if (selectedPreset) {
        // A mode fixed by the preset (disabled select) is not in the form data.
        const fixed = selectedPreset.game === "duke3d"
          ? DUKE_MODE_VALUES[selectedPreset.duke_game_mode ?? 1]
          : selectedPreset.doom_mode;
        if (fixed) data.mode = fixed;
        if (selectedPreset.game === "duke3d") data.duke_friendly_fire = !!selectedPreset.duke_friendly_fire;
      }
      const royale = doom && (selectedPreset ? selectedPreset.mod === "royale" : data.mod === "royale");
      const botGame = royale || (doom && data.mode === "deathmatch");
      const created = await api("/api/rooms", {
        method: "POST",
        body: JSON.stringify({
          preset: preset || undefined,
          map: preset ? data.preset_map || data.map || "" : data.map || "E1M1",
          skill: Number(data.skill) || 3,
          max_players: Number(data.max_players) || 2,
          visibility: data.visibility || "public",
          time_limit: Number(data.time_limit) || undefined,
          start_delay_secs: botGame && data.fill_bots === true ? Number(data.start_delay_secs) || undefined : undefined,
          bot_skill: botGame && data.fill_bots === true ? Number(data.bot_skill) || undefined : undefined,
          game,
          duke_game_mode: Math.max(0, DUKE_MODE_VALUES.indexOf(data.mode || "coop")),
          duke_friendly_fire: data.mode === "coop" && !!data.duke_friendly_fire,
          doom_deathmatch: doom && data.mode === "deathmatch",
          // Royale bot fill: start on a timer after the first join, bots in empty slots.
          fill_bots: botGame && data.fill_bots === true,
          quick_play: botGame && data.fill_bots === true,
          mod: doom && !preset ? data.mod || undefined : undefined,
          packs: doom && !preset && data.voxel ? ["voxel"] : [],
        }),
      });
      flash("Game " + shortId(created.id) + " created");
      await refresh();
      const room = {
        id: created.id,
        map: created.settings.map,
        skill: created.settings.skill,
        max_players: created.settings.max_players,
        mod: created.settings.mod,
        packs: created.settings.packs || [],
        game: created.settings.game,
        time_limit_secs: created.settings.time_limit_secs,
        preset: created.settings.preset || "",
        occupied: 0,
        started: false,
        ws_v2: created.ws_v2,
        ws_legacy: created.ws_legacy,
      };
      await playRoom(room);
    } catch (err) {
      if (err.code === 403 && data.mod) {
        flash("Mod " + data.mod + " is no longer allowed on this server", "error");
      } else {
        flash(err.message || String(err), "error");
      }
    }
  }

  function soloFileMaps(bytes, game, fileName) {
    const view = new DataView(bytes);
    const readName = (start, length) => {
      const data = new Uint8Array(bytes, start, length);
      let end = data.indexOf(0);
      if (end < 0) end = data.length;
      return String.fromCharCode(...data.subarray(0, end)).toUpperCase();
    };
    if (game === "duke3d") {
      if (bytes.byteLength < 16 || readName(0, 12) !== "KENSILVERMAN") throw new Error("Choose a Duke Nukem 3D GRP file");
      const count = view.getUint32(12, true);
      const dirEnd = 16 + count * 16;
      if (count > 65536 || dirEnd > bytes.byteLength) throw new Error("This GRP directory is invalid or truncated");
      let bodyAt = dirEnd;
      const maps = [];
      for (let i = 0; i < count; i++) {
        const at = 16 + i * 16;
        const name = readName(at, 12);
        const size = view.getUint32(at + 12, true);
        bodyAt += size;
        if (bodyAt > bytes.byteLength) throw new Error("This GRP file is truncated");
        const stem = name.endsWith(".MAP") ? name.slice(0, -4) : "";
        if (stem && stem.length <= 8 && /^[A-Z0-9_]+$/.test(stem) && !maps.includes(stem)) maps.push(stem);
      }
      if (!maps.length) throw new Error("No Build .MAP files were found in this GRP");
      console.info("[Room solo] GRP map scan", { file: fileName, bytes: bytes.byteLength, entries: count, maps });
      return maps;
    }

    const wadMagic = bytes.byteLength >= 4 ? readName(0, 4) : "";
    console.info("[Room solo] WAD header", { file: fileName, game, bytes: bytes.byteLength, magic: wadMagic });
    // Some complete local Doom data sets are packaged with a PWAD header even
    // though they contain the base maps and resources needed for a solo run.
    // The engine mounts either WAD kind as the selected base archive.
    if (bytes.byteLength < 12 || (wadMagic !== "IWAD" && wadMagic !== "PWAD")) {
      throw new Error(game === "doom-addon" ? "This is not a Doom WAD add-on" : "Choose a Doom WAD file");
    }
    const count = view.getUint32(4, true);
    const directory = view.getUint32(8, true);
    console.info("[Room solo] WAD directory", { file: fileName, magic: wadMagic, bytes: bytes.byteLength, lumps: count, directory });
    if (count < 0 || count > 65536 || directory < 12 || directory + count * 16 > bytes.byteLength) {
      throw new Error("This WAD directory is invalid or truncated");
    }
    const lumps = [];
    for (let i = 0; i < count; i++) lumps.push(readName(directory + i * 16 + 8, 8));
    const maps = [];
    for (let i = 0; i + 1 < lumps.length; i++) {
      const map = lumps[i];
      const conventionalName = /^(?:E\dM\d+|MAP\d+)$/i.test(map);
      const hasMapLumps = lumps[i + 1] === "THINGS" || lumps[i + 1] === "TEXTMAP";
      if (!conventionalName && !hasMapLumps) continue;
      if (map && !maps.includes(map)) maps.push(map);
    }
    // The stock shareware Doom 1 IWAD has exactly episode 1, maps 1–9. Some
    // repacked copies omit or reorder marker lumps, so keep those selectable
    // when its otherwise-valid IWAD directory has no recognizable markers.
    if (!maps.length && game === "doom" && /^doom1\.wad$/i.test(fileName || "")) {
      maps.push(...Array.from({ length: 9 }, (_, i) => `E1M${i + 1}`));
      console.warn("[Room solo] no map markers found in doom1.wad; using the stock shareware episode map list");
    }
    if (!maps.length && game === "doom-addon") return [];
    if (!maps.length) throw new Error("No Doom maps were found in this IWAD directory");
    console.info("[Room solo] WAD map scan", { file: fileName, bytes: bytes.byteLength, lumps: count, directory, maps });
    return maps;
  }

  function soloAddonMaps(bytes, fileName) {
    const magic = String.fromCharCode(...new Uint8Array(bytes, 0, Math.min(4, bytes.byteLength)));
    if (magic === "PWAD" || magic === "IWAD") return soloFileMaps(bytes, "doom-addon", fileName);
    if (magic.startsWith("PK")) {
      const data = new Uint8Array(bytes);
      const view = new DataView(bytes);
      const min = Math.max(0, data.length - 65_557);
      let eocd = -1;
      for (let at = data.length - 22; at >= min; at--) {
        if (view.getUint32(at, true) === 0x06054b50) { eocd = at; break; }
      }
      if (eocd < 0) return [];
      const entries = view.getUint16(eocd + 10, true);
      let at = view.getUint32(eocd + 16, true);
      const maps = [];
      for (let i = 0; i < entries && at + 46 <= data.length; i++) {
        if (view.getUint32(at, true) !== 0x02014b50) break;
        const nameLen = view.getUint16(at + 28, true);
        const extraLen = view.getUint16(at + 30, true);
        const commentLen = view.getUint16(at + 32, true);
        const end = at + 46 + nameLen + extraLen + commentLen;
        if (end > data.length) break;
        const path = new TextDecoder().decode(data.subarray(at + 46, at + 46 + nameLen)).replace(/\\/g, "/");
        const match = /(?:^|\/)maps\/([a-z0-9_]+)\.wad$/i.exec(path);
        if (match && !maps.includes(match[1].toUpperCase())) maps.push(match[1].toUpperCase());
        at = end;
      }
      console.info("[Room solo] ZIP add-on map scan", { file: fileName, bytes: bytes.byteLength, entries, maps });
      return maps;
    }
    return [];
  }

  /** Solo URL: the game client without `ws` runs a local single-player game. */
  function soloUrl(opts) {
    const base = clientBase();
    const q = new URLSearchParams();
    q.set("map", opts.map);
    q.set("skill", String(opts.skill || 3));
    q.set("game", opts.game);
    if (opts.bundled) {
      // Built-in content served by this server (play page fetches + caches it).
      q.set("iwad", opts.bundled.iwad.url);
      if (opts.bundled.addons.length) q.set("addon", opts.bundled.addons.map((a) => a.url).join(","));
      if (opts.bundled.duke_mod?.url) q.set("duke_mod", opts.bundled.duke_mod.url);
    } else {
      q.set("local", "1");
      q.set("iwad", opts.game === "duke3d" ? "solo-local.grp" : "solo-local.wad");
    }
    if (opts.game !== "duke3d" && opts.gen) q.set("gen", JSON.stringify(opts.gen));
    if (opts.game !== "duke3d" && opts.mod) q.set("mod", opts.mod);
    if (opts.game !== "duke3d" && opts.packs && opts.packs.length) q.set("pack", opts.packs.join(","));
    // The play page turns voxels on when the URL says nothing, so an
    // unchecked "Voxel sprites" box must be spelled out.
    if (opts.game !== "duke3d") q.set("voxel", opts.packs && opts.packs.includes("voxel") ? "1" : "0");
    return base + "?" + q.toString();
  }

  /** Built-in solo presets (`/play/bundled/manifest.json`, written by bundle-solo-content.sh). */
  async function loadBundledPresets() {
    try {
      const r = await fetch(clientBase() + "bundled/manifest.json", { cache: "no-store" });
      if (!r.ok) return [];
      const presets = (await r.json()).presets;
      return Array.isArray(presets) ? presets : [];
    } catch (_) {
      return [];
    }
  }

  async function saveSoloGameFile(fileName, bytes, addons = []) {
    if (!window.indexedDB) throw new Error("This browser does not support local game storage");
    const db = await new Promise((resolve, reject) => {
      const request = indexedDB.open("room-solo-game", 1);
      request.onupgradeneeded = () => request.result.createObjectStore("game", { keyPath: "id" });
      request.onsuccess = () => resolve(request.result);
      request.onerror = () => reject(request.error || new Error("Could not open local game storage"));
    });
    await new Promise((resolve, reject) => {
      const tx = db.transaction("game", "readwrite");
      // Blobs, not ArrayBuffers: Chrome rejects one IndexedDB value over
      // ~128 MiB ("serialized keys and/or value are too large"), and a large
      // PK3 such as Brutal Doom (~160 MB) exceeds that alone. Blob contents are
      // stored outside the serialized value, so their size is not limited.
      const asBlob = (data) =>
        data instanceof Blob ? data : new Blob([data], { type: "application/octet-stream" });
      tx.objectStore("game").put({
        id: "current",
        fileName,
        bytes: asBlob(bytes),
        addons: addons.map((addon) => ({ ...addon, bytes: asBlob(addon.bytes) })),
      });
      tx.oncomplete = resolve;
      tx.onerror = () => reject(tx.error || new Error("Could not store local game file"));
      tx.onabort = () => reject(tx.error || new Error("Local game storage was interrupted"));
    });
    db.close();
  }

  /**
   * `pre` opens the dialog already pointed somewhere: `source` (a built-in
   * preset id, or "own"), `level` ("game" / "gen"), `mod` (a mod name).
   */
  async function playSolo(pre = {}) {
    const mods = await loadMods();
    // Multiplayer-only: `[mod] solo = false`, or known names if an old server omits the field.
    const soloMods = mods.filter(
      (m) => m.solo !== false && m.name !== "royale"
    );
    const picker = modPickerHtml(soloMods, { voxelPack: true });
    const bundled = await loadBundledPresets();
    let selectedGameFile = null;
    let selectedPreset = null;
    let soloMaps = [];
    const data = await openModal({
      title: "Play solo",
      confirmLabel: "Start",
      bodyHtml:
        '<label class="field"><span>Game files</span><select name="source" id="solo-source">' +
        bundled.map((p) => '<option value="' + escapeHtml(p.id) + '"' + (p.default ? " selected" : "") + ">" + escapeHtml(p.label) + " (built-in)</option>").join("") +
        '<option value="own"' + (bundled.length ? "" : " selected") + ">My own files…</option></select></label>" +
        '<p class="muted" id="solo-source-note"></p>' +
        '<div id="solo-own-files">' +
        '<p class="muted">Choose a local game file. It stays in this browser and is never uploaded.</p>' +
        '<label class="field"><span>Game</span><select name="game"><option value="doom">Doom</option><option value="duke3d">Duke Nukem 3D (no mods)</option></select></label>' +
        '<label class="field"><span>WAD / GRP</span><input id="solo-game-file" type="file" accept=".wad,.grp,application/octet-stream" required/></label>' +
        '<label class="field" id="solo-addons-field"><span>Additional Doom WAD / PK3 mods</span><input id="solo-addons" type="file" accept=".wad,.pk3,.pk7,.zip,.pke,.ipk3,.ipk7,application/octet-stream" multiple/></label>' +
        "</div>" +
        '<label class="field" id="solo-level-field"><span>Level</span><select name="level" id="solo-level">' +
        '<option value="game">A map from the game</option><option value="gen">🎲 Generate a new map</option></select></label>' +
        '<div id="solo-gen" class="solo-gen" hidden>' +
        '<p class="muted">A new level built from a seed: rooms, corridors, stairs, caves, swamps, keeps… with monsters, weapons and an exit switch. Same seed, same map.</p>' +
        '<div class="solo-gen-grid">' +
        '<label class="field"><span>Theme</span><select name="gen_theme"><option value="mixed">Mixed districts</option><option value="tech">Tech base</option><option value="base">Brown industrial</option><option value="hell">Hell keep</option><option value="castle">Stone castle</option><option value="ruins">Ruins under the sky</option></select></label>' +
        '<label class="field"><span>Places</span><select name="gen_biomes"><option value="mixed">A bit of everything</option><option value="indoor">Built interiors</option><option value="caves">Caverns</option><option value="wilderness">Wilderness</option><option value="urban">Plazas and keeps</option></select></label>' +
        '<label class="field"><span>Size</span><select name="gen_size"><option value="small">Small</option><option value="medium" selected>Medium</option><option value="large">Large</option><option value="huge">Huge</option></select></label>' +
        '<label class="field"><span>Monsters</span><select name="gen_monsters"><option value="1">A few</option><option value="2" selected>Normal</option><option value="3">Hordes</option></select></label>' +
        '<label class="field"><span>Length</span><select name="gen_maps"><option value="1">One map</option><option value="3" selected>3-map episode</option><option value="5">5-map episode</option></select></label>' +
        '<label class="field"><span>Seed</span><span class="solo-seed"><input name="gen_seed" id="solo-gen-seed" type="number" min="0" step="1"/><button type="button" id="solo-gen-dice" class="btn ghost" title="Random seed">🎲</button></span></label>' +
        "</div></div>" +
        '<div id="solo-map-field">' +
        '<label class="field"><span>Map <small id="solo-map-count" class="muted">Choose a game file to list maps</small></span><select name="map" id="solo-map" required disabled><option value="">Choose a game file first</option></select></label>' +
        "</div>" +
        '<label class="field"><span>How challenging should this run be?</span><select name="skill" id="solo-skill"></select></label>' +
        '<div id="solo-mod-picker">' + picker.html + "</div>" +
        '<p class="muted">Doom maps are detected from the selected WAD; Duke uses the GRP’s Build map names. Gameplay mods and Voxel are Doom only.</p>',
      onReady: (body) => {
        picker.wire(body);
        const game = body.querySelector('select[name="game"]');
        const file = body.querySelector("#solo-game-file");
        const addons = body.querySelector("#solo-addons");
        const addonsField = body.querySelector("#solo-addons-field");
        const map = body.querySelector("#solo-map");
        const count = body.querySelector("#solo-map-count");
        const skill = body.querySelector("#solo-skill");
        const level = body.querySelector("#solo-level");
        const levelField = body.querySelector("#solo-level-field");
        const genBox = body.querySelector("#solo-gen");
        const mapField = body.querySelector("#solo-map-field");
        const seedInput = body.querySelector("#solo-gen-seed");
        const newSeed = () => { seedInput.value = String(Math.floor(Math.random() * 1e9)); };
        newSeed();
        body.querySelector("#solo-gen-dice").addEventListener("click", newSeed);
        const updateLevel = () => {
          const duke = game.value === "duke3d";
          levelField.hidden = duke;
          if (duke) level.value = "game";
          const gen = level.value === "gen";
          genBox.hidden = !gen;
          mapField.hidden = gen;
          map.required = !gen;
        };
        level.addEventListener("change", updateLevel);
        game.addEventListener("change", updateLevel);
        updateLevel();
        let scanGeneration = 0;
        // The game's own skill names (menu order). Duke has four menu skills,
        // `ud.player_skill` 1–4; Room passes skill-1, so they are values 2–5.
        const SKILLS = {
          doom: ["I’m too young to die", "Hey, not too rough", "Hurt me plenty", "Ultra-Violence", "Nightmare!"],
          freedoom: ["Please don’t kill me!", "Will this hurt?", "Bring on the pain.", "Extreme carnage.", "MAYHEM!"],
          duke3d: ["Piece of Cake", "Let’s Rock", "Come Get Some", "Damn I’m Good"],
        };
        const skillSet = () => {
          if (game.value === "duke3d") return "duke3d";
          const name = selectedPreset ? selectedPreset.iwad.name : file.files?.[0]?.name || "";
          return /freedoom|freedm/i.test(name) ? "freedoom" : "doom";
        };
        const skillNames = () => SKILLS[skillSet()];
        // Relabel in place, keeping the chosen level when it still exists.
        const relabelSkill = (fallback) => {
          const keep = Number(skill.value) || fallback;
          const names = skillNames();
          const first = skillSet() === "duke3d" ? 2 : 1;
          skill.replaceChildren(...names.map((label, index) => new Option(label, String(index + first))));
          skill.value = String(Math.min(Math.max(keep, first), first + names.length - 1));
        };
        const paintMapOptions = (maps, sourceLabel) => {
          const available = [...new Set(maps)];
          map.replaceChildren(new Option("Select a map…", ""));
          let group = "";
          for (const name of available) {
            const episode = /^E(\d)(?:M|L)\d+$/i.exec(name);
            const nextGroup = episode ? `Episode ${episode[1]}` : /^MAP\d+$/i.test(name) ? "Maps" : "Other";
            if (nextGroup !== group) {
              const optgroup = document.createElement("optgroup");
              optgroup.label = nextGroup;
              map.appendChild(optgroup);
              group = nextGroup;
            }
            map.lastElementChild.appendChild(new Option(name, name));
          }
          const initial = game.value === "doom"
            ? available.find((name) => name === "E1M1") || available.find((name) => name === "MAP01") || available[0]
            : available[0];
          if (!initial) throw new Error("No maps were found in the selected game files");
          map.value = initial;
          map.disabled = false;
          soloMaps = available;
          count.textContent = `${available.length} maps ready · ${sourceLabel}`;
          return available;
        };
        const scanSelectedFiles = async () => {
          relabelSkill(3);
          const picked = file.files?.[0];
          if (!picked) return;
          const generation = ++scanGeneration;
          const confirm = document.getElementById("modal-confirm");
          selectedGameFile = null;
          map.disabled = true;
          file.disabled = true;
          addons.disabled = true;
          game.disabled = true;
          skill.disabled = true;
          if (confirm) confirm.disabled = true;
          count.textContent = `Loading ${picked.name}…`;
          map.replaceChildren(new Option("Loading map list…", ""));
          console.info("[Room solo] game file selected", { game: game.value, file: picked.name, bytes: picked.size });
          await new Promise((resolve) => requestAnimationFrame(() => resolve()));
          try {
            const buffer = await picked.arrayBuffer();
            count.textContent = `Scanning maps in ${picked.name}…`;
            await new Promise((resolve) => requestAnimationFrame(() => resolve()));
            const maps = soloFileMaps(buffer, game.value, picked.name);
            const selectedAddons = game.value === "doom" ? [...(addons.files || [])] : [];
            const addonData = [];
            for (const addon of selectedAddons) {
              count.textContent = `Reading ${addon.name}…`;
              await new Promise((resolve) => requestAnimationFrame(() => resolve()));
              const addonBytes = await addon.arrayBuffer();
              const magic = String.fromCharCode(...new Uint8Array(addonBytes, 0, Math.min(4, addonBytes.byteLength)));
              const zip = magic.startsWith("PK");
              const sevenZip = addonBytes.byteLength >= 6 && Array.from(new Uint8Array(addonBytes, 0, 6)).join(",") === "55,122,188,175,39,28";
              if (!zip && !sevenZip && magic !== "PWAD" && magic !== "IWAD") {
                throw new Error(`${addon.name} is not a WAD, PK3, PK7, or ZIP archive`);
              }
              const extraMaps = soloAddonMaps(addonBytes, addon.name);
              maps.push(...extraMaps);
              addonData.push({ name: addon.name, bytes: addonBytes });
              console.info("[Room solo] add-on scanned", { file: addon.name, bytes: addon.size, maps: extraMaps });
            }
            if (generation !== scanGeneration) return;
            const available = paintMapOptions(maps, [picked.name, ...selectedAddons.map((addon) => addon.name)].join(" + "));
            selectedGameFile = { name: picked.name, buffer, addons: addonData };
            console.info("[Room solo] map list ready", { game: game.value, base: picked.name, addons: selectedAddons.map((addon) => addon.name), count: available.length, maps: available });
          } catch (err) {
            if (generation !== scanGeneration) return;
            selectedGameFile = null;
            map.replaceChildren(new Option("No maps available", ""));
            map.disabled = true;
            count.textContent = err.message || "Could not read game maps";
            console.warn("[Room solo] map scan failed", { game: game.value, file: picked.name, bytes: picked.size, error: err });
            flash(count.textContent, "error");
          } finally {
            if (generation === scanGeneration) {
              file.disabled = false;
              addons.disabled = false;
              game.disabled = false;
              skill.disabled = false;
              if (confirm) confirm.disabled = false;
            }
          }
        };
        const updateGame = () => {
          scanGeneration++;
          selectedGameFile = null;
          file.value = "";
          addons.value = "";
          file.accept = game.value === "duke3d" ? ".grp,application/octet-stream" : ".wad,application/octet-stream";
          addonsField.hidden = game.value === "duke3d";
          skill.value = "";
          relabelSkill(3);
          map.replaceChildren(new Option("Choose a matching game file", ""));
          map.disabled = true;
          count.textContent = "Choose a game file to list maps";
          body.querySelector("#solo-mod-picker").hidden = game.value === "duke3d";
        };
        game.addEventListener("change", updateGame);
        updateGame();
        file.addEventListener("change", scanSelectedFiles);
        addons.addEventListener("change", scanSelectedFiles);
        const source = body.querySelector("#solo-source");
        const own = body.querySelector("#solo-own-files");
        const note = body.querySelector("#solo-source-note");
        const updateSource = () => {
          selectedPreset = bundled.find((p) => p.id === source.value) || null;
          own.hidden = !!selectedPreset;
          file.required = !selectedPreset;
          if (selectedPreset) {
            game.value = selectedPreset.game || "doom";
            updateGame();
            const mb = (selectedPreset.bytes / 1048576).toFixed(0);
            note.textContent = `${selectedPreset.note} Served by this Room server (${mb} MB, downloaded once and cached in this browser).`;
            try {
              paintMapOptions(selectedPreset.maps, selectedPreset.label);
            } catch (err) {
              count.textContent = err.message || String(err);
            }
          } else {
            note.textContent = bundled.length ? "Use your own Doom WAD or Duke GRP instead of the built-in files." : "";
            updateGame();
          }
        };
        source.addEventListener("change", () => {
          updateSource();
          updateLevel();
        });
        updateSource();
        updateLevel();
        // Shortcuts from the Solo section's cards.
        const pick = (el, value) => {
          if (!el || value == null || ![...el.options].some((o) => o.value === value)) return;
          el.value = value;
          el.dispatchEvent(new Event("change"));
        };
        pick(source, pre.source);
        pick(level, pre.level);
        pick(body.querySelector('select[name="mod"]'), pre.mod);
        if (pre.source === "own") file.focus();
      },
    });
    if (!data) return;
    // Generated level: name it like the game's own maps (MAP01 or E1M1).
    let gen = null;
    if (data.level === "gen" && (selectedPreset || data.game !== "duke3d")) {
      const doom2 = soloMaps.some((m) => /^MAP\d+$/i.test(m)) || !soloMaps.length;
      const first = doom2 ? "MAP01" : soloMaps.find((m) => /^E\dM1$/i.test(m)) || "E1M1";
      gen = {
        seed: Math.max(0, Math.floor(Number(data.gen_seed) || 0)),
        name: first,
        players: 4,
        size: data.gen_size || "medium",
        theme: data.gen_theme || "mixed",
        biomes: data.gen_biomes || "mixed",
        monsters: Number(data.gen_monsters) || 2,
        maps: Number(data.gen_maps) || 1,
        verticality: 1,
      };
      data.map = first;
    }
    if (selectedPreset) {
      if (!data.map) {
        flash("Choose a map before starting", "error");
        return;
      }
      if (!(await ensurePlayBase())) return;
      goPlay(
        soloUrl({
          game: selectedPreset.game || "doom",
          map: data.map.trim().toUpperCase(),
          skill: Number(data.skill) || 3,
          mod: selectedPreset.game === "duke3d" ? "" : data.mod,
          packs: selectedPreset.game === "duke3d" ? [] : data.voxel ? ["voxel"] : [],
          bundled: selectedPreset,
          gen,
        })
      );
      return;
    }
    if (!selectedGameFile || !data.map) {
      flash("Choose a WAD or GRP and a map before starting", "error");
      return;
    }
    if (!(await ensurePlayBase())) return;
    const doom = data.game !== "duke3d";
    try {
      await saveSoloGameFile(selectedGameFile.name, selectedGameFile.buffer, selectedGameFile.addons || []);
      console.info("[Room solo] prepared local run", {
        game: data.game,
        base: selectedGameFile.name,
        addons: (selectedGameFile.addons || []).map((addon) => addon.name),
        map: data.map,
      });
    } catch (err) {
      flash("Could not prepare the local game file: " + (err.message || String(err)), "error");
      return;
    }
    goPlay(
      soloUrl({
        game: data.game,
        map: data.map.trim().toUpperCase(),
        skill: Number(data.skill) || 3,
        mod: doom ? data.mod : "",
        packs: doom && data.voxel ? ["voxel"] : [],
        gen: doom ? gen : null,
      })
    );
  }

  async function joinById() {
    const data = await openModal({
      title: "Join by game ID",
      confirmLabel: "Join game",
      bodyHtml:
        '<label class="field"><span>Game ID</span><input name="id" required placeholder="hex game ID"/></label>',
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
    $("room-count").textContent = state.rooms.length === 1 ? "1 game" : state.rooms.length + " games";
    $("rooms-empty").classList.toggle("hidden", state.rooms.length > 0);
    for (const room of state.rooms) {
      const el = document.createElement("article");
      el.className = "room";
      const full = room.occupied >= room.max_players;
      el.innerHTML =
        "<div><h3>" +
        escapeHtml(room.map) +
        '</h3><p class="meta">' +
        escapeHtml(modeLabel(room)) +
        " · " +
        escapeHtml(skillLabel(room.game, room.skill)) +
        " · " +
        room.occupied +
        "/" +
        room.max_players +
        " players" +
        (roomLimitSecs(room) ? " · " + escapeHtml(fmtDuration(roomLimitSecs(room))) : "") +
        '</p><p class="id" title="' +
        escapeHtml(room.id) +
        '">' +
        escapeHtml(shortId(room.id)) +
        '</p><div class="badges">' +
        (room.started
          ? '<span class="badge live">in progress</span>'
          : '<span class="badge open">waiting</span>') +
        (full ? '<span class="badge">full</span>' : "") +
        (room.game === "duke3d" ? '<span class="badge mod">' + escapeHtml(gameLabel(room)) + "</span>" : "") +
        (room.mod ? '<span class="badge mod">' + escapeHtml(modLabel(room)) + "</span>" : "") +
        (room.packs && room.packs.length
          ? '<span class="badge mod">' + escapeHtml(room.packs.join("+")) + "</span>"
          : "") +
        (room.preset
          ? '<span class="badge mod" title="Download ' + escapeHtml(fmtBytes(room.download_bytes)) + '">' +
            escapeHtml(room.preset) + "</span>"
          : "") +
        "</div></div>";
      const actions = document.createElement("div");
      actions.className = "room-actions";
      const play = document.createElement("button");
      play.type = "button";
      play.className = "btn primary small";
      play.textContent = "Join";
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
      // A game in progress: watching is the main action.
      if (room.started) play.className = "btn small";
      const watch = document.createElement("button");
      watch.type = "button";
      watch.className = room.started ? "btn primary small" : "btn small";
      watch.textContent = "Watch";
      watch.title = "Watch the game live (no account, no seat)";
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

  function renderTop(players) {
    const list = $("top-list");
    const mode = state.lbMode;
    const rows = mode === "br" ? state.brTop || [] : players || [];
    $("top-empty").classList.toggle("hidden", rows.length > 0);
    const label = $("lb-mode-label");
    if (label) label.textContent = mode === "br" ? "rating · BR stats" : "wins · frags";
    document.querySelectorAll(".lb-tab").forEach((btn) => {
      btn.classList.toggle("on", btn.getAttribute("data-lb") === mode);
    });
    if (mode === "br") {
      list.innerHTML = rows
        .map(
          (p) =>
            '<li><span class="who" title="' +
            escapeHtml(p.name) +
            '">' +
            escapeHtml(p.name) +
            '</span><span class="stats"><b>' +
            p.rating +
            "</b> rating · " +
            p.wins +
            " W · avg #" +
            (p.matches ? (p.place_sum / p.matches).toFixed(1) : "—") +
            " · " +
            p.kills +
            " K</span></li>"
        )
        .join("");
      return;
    }
    list.innerHTML = rows
      .map(
        (p) =>
          '<li><span class="who" title="' +
          escapeHtml(p.name) +
          '">' +
          escapeHtml(p.name) +
          '</span><span class="stats"><b>' +
          p.wins +
          "</b> W · " +
          p.frags +
          " frags · " +
          p.matches +
          " played</span></li>"
      )
      .join("");
  }

  async function refresh() {
    try {
      const [cfg, rooms, top, brTop] = await Promise.all([
        api("/api/lobby"),
        api("/api/rooms"),
        api("/api/leaderboard").catch(() => null),
        api("/api/br/leaderboard?limit=15").catch(() => null),
      ]);
      state.config = cfg;
      state.rooms = rooms || [];
      state.brTop = brTop || [];
      if (top) renderTop(top);
      $("node-pill").textContent = "node " + (cfg.node_id || "—");
      $("drain-pill").classList.toggle("hidden", !cfg.draining);
      $("drain-pill").classList.toggle("warn", !!cfg.draining);
      $("play-base-label").textContent = cfg.play_base || "—";
      if (cfg.play_base) $("btn-editor").href = `${clientBase()}editor.html`;
      renderRooms();
    } catch (err) {
      flash(err.message || String(err), "error");
    }
  }

  async function quickPlayBr() {
    const name = requireCallsign();
    if (!name) return;
    try {
      const join = await api("/api/quick-play/br", {
        method: "POST",
        body: JSON.stringify({ name }),
      });
      flash("Quick play BR — joining queue…");
      const roomId = (join.ws_v2 || "").replace(/.*\/v2\/rooms\//, "").replace(/\/ws.*/, "");
      if (roomId) {
        await playRoom(
          {
            id: roomId,
            ws_v2: join.ws_v2,
            map: "E1M1",
            skill: 3,
            max_players: 8,
            mod: { name: "royale", title: "Battle Royale" },
            game: "doom",
            time_limit_secs: (state.config.room_time_limit_mins || 20) * 60,
            occupied: 0,
            started: false,
          },
          { ticket: join.ticket, callsign: name }
        );
      }
    } catch (err) {
      flash(err.message || String(err), "error");
    }
  }

  document.querySelectorAll(".lb-tab").forEach((btn) => {
    btn.addEventListener("click", () => {
      state.lbMode = btn.getAttribute("data-lb") || "all";
      renderTop(null);
    });
  });

  // ---- Sections: Multiplayer · Solo · Editor -------------------------------
  const MODES = ["multiplayer", "solo", "editor"];

  function showMode(mode, { remember = true } = {}) {
    if (!MODES.includes(mode)) mode = "multiplayer";
    for (const m of MODES) {
      const on = m === mode;
      $("mode-" + m).hidden = !on;
      const tab = $("tab-" + m);
      tab.setAttribute("aria-selected", on ? "true" : "false");
      tab.tabIndex = on ? 0 : -1;
    }
    if (remember) {
      try { localStorage.setItem("roomLobbyMode", mode); } catch (_) { /* private mode */ }
      if (location.hash !== "#" + mode) history.replaceState(null, "", "#" + mode);
    }
  }

  function initialMode() {
    const hash = location.hash.replace(/^#/, "");
    if (MODES.includes(hash)) return hash;
    // A join or watch link is about a netgame.
    const q = new URLSearchParams(location.search);
    if (q.get("join") || q.get("watch")) return "multiplayer";
    try {
      const saved = localStorage.getItem("roomLobbyMode");
      if (MODES.includes(saved)) return saved;
    } catch (_) { /* private mode */ }
    return "multiplayer";
  }

  for (const m of MODES) {
    const tab = $("tab-" + m);
    tab.addEventListener("click", () => showMode(m));
    // Arrow keys move between the three tabs (ARIA tabs pattern).
    tab.addEventListener("keydown", (ev) => {
      const step = ev.key === "ArrowRight" ? 1 : ev.key === "ArrowLeft" ? -1 : 0;
      if (!step) return;
      ev.preventDefault();
      const next = MODES[(MODES.indexOf(m) + step + MODES.length) % MODES.length];
      showMode(next);
      $("tab-" + next).focus();
    });
  }
  window.addEventListener("hashchange", () => {
    const hash = location.hash.replace(/^#/, "");
    if (MODES.includes(hash)) showMode(hash);
  });
  showMode(initialMode(), { remember: false });

  function cardHtml({ title, text, meta, action, attrs = "", href = "" }) {
    const button = href
      ? '<a class="btn" href="' + escapeHtml(href) + '" target="_blank" rel="noopener">' + escapeHtml(action) + "</a>"
      : '<button type="button" class="btn" ' + attrs + ">" + escapeHtml(action) + "</button>";
    return (
      '<article class="card"><h3>' + escapeHtml(title) + "</h3>" +
      (meta ? '<p class="card-meta">' + escapeHtml(meta) + "</p>" : "") +
      "<p>" + escapeHtml(text || "") + "</p>" + button + "</article>"
    );
  }

  /** Solo section: the server's game files and its mods, one click each. */
  async function renderSoloHub(presets) {
    const grid = $("solo-presets");
    grid.innerHTML = presets
      .map((p) =>
        cardHtml({
          title: p.label,
          meta: [(p.game === "duke3d" ? "Duke Nukem 3D" : "Doom engine"), p.maps?.length ? p.maps.length + (p.maps.length === 1 ? " map" : " maps") : "", p.bytes ? Math.round(p.bytes / 1048576) + " MB" : ""].filter(Boolean).join(" · "),
          text: p.note || "Built-in game files served by this server.",
          action: "Play",
          attrs: 'data-solo-preset="' + escapeHtml(p.id) + '"',
        })
      )
      .join("");
    $("solo-presets-empty").classList.toggle("hidden", presets.length > 0);
    $("solo-preset-count").textContent = presets.length ? presets.length + " built in" : "";
    const mods = (await loadMods()).filter((m) => m.solo !== false && m.name !== "royale");
    $("solo-mods").innerHTML = mods
      .map((m) =>
        cardHtml({
          title: m.title || m.name,
          text: m.description || "",
          action: "Play with this mod",
          attrs: 'data-solo-mod="' + escapeHtml(m.name) + '"',
        })
      )
      .join("");
    $("solo-mods-empty").classList.toggle("hidden", mods.length > 0);
  }

  /** Editor section: open the editor empty or on a built-in game. */
  function renderEditorHub(presets) {
    const editor = clientBase() + "editor.html";
    // The editor opens one game archive (plus at most one PWAD): skip
    // presets that stack mods on the game.
    presets = presets.filter((p) => !(p.addons && p.addons.length));
    const doomPreset = presets.find((p) => p.game !== "duke3d") || null;
    const cards = [
      cardHtml({
        title: "Your own files",
        text: "Open any WAD, PK3 or Duke GRP from your computer. Files stay in this browser.",
        action: "Open the editor",
        href: editor,
      }),
    ];
    for (const p of presets) {
      cards.push(
        cardHtml({
          title: p.label,
          meta: p.game === "duke3d" ? "Duke Nukem 3D maps" : "Doom maps",
          text: "Open the built-in game files with its first map loaded.",
          action: "Edit its maps",
          href: editor + "?iwad=" + encodeURIComponent(p.iwad.url) + (p.maps?.[0] ? "&map=" + encodeURIComponent(p.maps[0]) : ""),
        })
      );
    }
    if (doomPreset) {
      cards.push(
        cardHtml({
          title: "Script a mod",
          meta: "AssemblyScript · runs live",
          text: "Start in the Scripts tab: write gameplay code, compile it in the browser and play it on " + doomPreset.label + ".",
          action: "Open Scripts",
          href: editor + "?iwad=" + encodeURIComponent(doomPreset.iwad.url) + "&tab=scripts",
        })
      );
    }
    $("editor-starts").innerHTML = cards.join("");
  }

  let hubsReady = false;
  async function renderHubs() {
    if (hubsReady) return;
    hubsReady = true;
    const presets = await loadBundledPresets();
    renderEditorHub(presets);
    await renderSoloHub(presets).catch(() => {});
  }

  $("mode-solo").addEventListener("click", (ev) => {
    const preset = ev.target.closest("[data-solo-preset]");
    if (preset) return void playSolo({ source: preset.dataset.soloPreset, level: "game" });
    const mod = ev.target.closest("[data-solo-mod]");
    if (mod) return void playSolo({ mod: mod.dataset.soloMod });
  });
  $("btn-solo-gen").addEventListener("click", () => playSolo({ level: "gen" }));
  $("btn-solo-own").addEventListener("click", () => playSolo({ source: "own" }));

  $("btn-create").addEventListener("click", () => createRoom());
  $("btn-quick-br").addEventListener("click", () => quickPlayBr());
  $("btn-join-id").addEventListener("click", () => joinById());
  $("btn-solo").addEventListener("click", () => playSolo());
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
  if ($("watch-link")) {
    $("watch-link").addEventListener("click", async () => {
      const room = state.rooms.find((r) => r.id === state.watching);
      if (!room) return;
      const url = watchUrl(room, "Spectator");
      try {
        await navigator.clipboard.writeText(url);
        flash("Watch link copied");
      } catch (_) {
        window.prompt("Watch link", url);
      }
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
    .then(() => {
      consumeJoinQuery();
      return renderHubs();
    })
    .catch(() => {});
  setInterval(refresh, 4000);
})();


// Background "Doom fire" strip (160×64 palette automaton, scaled up).
(() => {
  const canvas = document.getElementById("bg-fire");
  if (!canvas) return;
  const g = canvas.getContext("2d");
  const W = canvas.width, H = canvas.height;
  const PAL = [[7,7,7],[31,7,7],[47,15,7],[71,15,7],[87,23,7],[103,31,7],[119,31,7],[143,39,7],[159,47,7],[175,63,7],
    [191,71,7],[199,71,7],[223,79,7],[223,87,7],[223,87,7],[215,95,7],[215,95,7],[215,103,15],[207,111,15],[207,119,15],
    [207,127,15],[207,135,23],[199,135,23],[199,143,23],[199,151,31],[191,159,31],[191,159,31],[191,167,39],[191,167,39],
    [191,175,47],[183,175,47],[183,183,47],[183,183,55],[207,207,111],[223,223,159],[239,239,199],[255,255,255]];
  const fire = new Uint8Array(W * H);
  for (let x = 0; x < W; x++) fire[(H - 1) * W + x] = PAL.length - 1;
  const img = g.createImageData(W, H);
  const step = () => {
    for (let x = 0; x < W; x++) {
      for (let y = 1; y < H; y++) {
        const src = y * W + x;
        const heat = fire[src];
        if (!heat) { fire[src - W] = 0; continue; }
        const r = (Math.random() * 3.6) | 0;
        fire[Math.max(0, Math.min(W * H - 1, src - W - (r - 1)))] = heat - (r & 1);
      }
    }
  };
  const paint = () => {
    const d = img.data;
    for (let i = 0; i < fire.length; i++) {
      const c = PAL[fire[i]], o = i * 4;
      d[o] = c[0]; d[o + 1] = c[1]; d[o + 2] = c[2]; d[o + 3] = fire[i] ? 255 : 0;
    }
    g.putImageData(img, 0, 0);
  };
  if (window.matchMedia && matchMedia("(prefers-reduced-motion: reduce)").matches) {
    for (let i = 0; i < H * 2; i++) step();
    paint();
    return;
  }
  let last = 0;
  const frame = (t) => {
    if (!document.hidden && t - last > 33) { last = t; step(); paint(); }
    requestAnimationFrame(frame);
  };
  requestAnimationFrame(frame);
})();
