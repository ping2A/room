/* Room operator console — talks only to same-origin admin API. */
(() => {
  "use strict";

  const $ = (id) => document.getElementById(id);
  const state = {
    view: "dashboard",
    summary: null,
    rooms: [],
    bans: [],
    audit: [],
    metrics: {},
    modalResolve: null,
    selectedRoomId: null,
    roomDetail: null,
  };

  const TITLES = {
    dashboard: ["Dashboard", "Cluster pulse for this node"],
    rooms: ["Rooms", "Live rooms on this node"],
    bans: ["Bans", "IP · pseudo · fingerprint · ticket · TURN · auth keys"],
    audit: ["Audit", "Append-only operator actions"],
    settings: ["Settings", "Drain and session controls"],
  };

  function token() {
    return ($("token").value || "").trim();
  }

  function headers(extra = {}) {
    const h = { Accept: "application/json", ...extra };
    const t = token();
    if (t) h.Authorization = "Bearer " + t;
    return h;
  }

  async function api(path, opts = {}) {
    const init = {
      ...opts,
      headers: headers({
        ...(opts.body ? { "Content-Type": "application/json" } : {}),
        ...(opts.headers || {}),
      }),
    };
    const res = await fetch(path, init);
    if (res.status === 401) {
      const err = new Error("unauthorized");
      err.code = 401;
      throw err;
    }
    if (!res.ok) {
      const err = new Error(res.status + " " + path);
      err.code = res.status;
      throw err;
    }
    if (res.status === 204) return null;
    const ct = res.headers.get("content-type") || "";
    return ct.includes("json") ? res.json() : res.text();
  }

  function flash(msg, kind = "ok") {
    const el = $("flash");
    el.textContent = msg;
    el.className = "flash " + kind;
    el.classList.remove("hidden");
    clearTimeout(flash._t);
    flash._t = setTimeout(() => el.classList.add("hidden"), 3200);
  }

  function fmtTs(ts) {
    if (!ts) return "—";
    const d = new Date(Number(ts) * 1000);
    if (Number.isNaN(d.getTime())) return String(ts);
    return d.toLocaleString(undefined, {
      month: "short",
      day: "2-digit",
      hour: "2-digit",
      minute: "2-digit",
      second: "2-digit",
    });
  }

  function shortId(id) {
    if (!id) return "—";
    return id.length > 12 ? id.slice(0, 8) + "…" : id;
  }

  function parseMetrics(text) {
    const out = {};
    for (const line of String(text || "").split("\n")) {
      if (!line || line.startsWith("#")) continue;
      const sp = line.indexOf(" ");
      if (sp < 0) continue;
      const key = line.slice(0, sp);
      const val = Number(line.slice(sp + 1));
      if (!Number.isNaN(val)) out[key] = val;
    }
    return out;
  }

  function setView(name) {
    state.view = name;
    for (const btn of document.querySelectorAll(".nav-item")) {
      btn.classList.toggle("active", btn.dataset.view === name);
    }
    for (const [key, pair] of Object.entries(TITLES)) {
      const el = $("view-" + key);
      if (el) el.classList.toggle("hidden", key !== name);
    }
    $("view-title").textContent = TITLES[name][0];
    $("view-sub").textContent = TITLES[name][1];
    render();
  }

  function openModal({ title, bodyHtml, confirmLabel = "Confirm", danger = false }) {
    return new Promise((resolve) => {
      state.modalResolve = resolve;
      $("modal-title").textContent = title;
      $("modal-body").innerHTML = bodyHtml;
      const conf = $("modal-confirm");
      conf.textContent = confirmLabel;
      conf.className = "btn " + (danger ? "danger" : "primary");
      $("modal").showModal();
    });
  }

  $("modal-form").addEventListener("submit", (ev) => {
    const submitter = ev.submitter;
    const value = submitter && submitter.value ? submitter.value : "cancel";
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

  async function noticeRoom(room) {
    const data = await openModal({
      title: "Broadcast notice",
      confirmLabel: "Send",
      bodyHtml:
        '<p class="muted mono">' +
        escapeHtml(room.id) +
        "</p>" +
        '<label class="field"><span>Message</span>' +
        '<input name="text" required maxlength="500" placeholder="Server message to all seats"/></label>',
    });
    if (!data || !data.text) return;
    await api("/admin/api/v1/rooms/" + encodeURIComponent(room.id) + "/notice", {
      method: "POST",
      body: JSON.stringify({ text: data.text }),
    });
    flash("Notice sent");
    await refresh();
  }

  async function closeRoom(room) {
    const data = await openModal({
      title: "Close room",
      confirmLabel: "Close room",
      danger: true,
      bodyHtml:
        "<p>Close <span class='mono'>" +
        escapeHtml(shortId(room.id)) +
        "</span> (" +
        escapeHtml(room.map) +
        ")? Players will be disconnected.</p>",
    });
    if (!data) return;
    await api("/admin/api/v1/rooms/" + encodeURIComponent(room.id) + "/close", {
      method: "POST",
      body: "{}",
    });
    flash("Room closed");
    await refresh();
  }

  async function seatAction(room, kind) {
    const data = await openModal({
      title: kind === "kick" ? "Kick seat" : "Mute seat",
      confirmLabel: kind === "kick" ? "Kick" : "Apply mute",
      danger: kind === "kick",
      bodyHtml:
        '<label class="field"><span>Seat (0–7)</span>' +
        '<input name="seat" type="number" min="0" max="7" value="0" required/></label>' +
        (kind === "kick"
          ? '<label class="field"><span>Reason</span><input name="reason" placeholder="kicked"/></label>'
          : '<label class="field"><span><input name="audio" type="checkbox" checked/> Mute audio</span></label>' +
            '<label class="field"><span><input name="video" type="checkbox" checked/> Mute video</span></label>' +
            '<label class="field"><span><input name="chat" type="checkbox"/> Freeze chat</span></label>'),
    });
    if (!data) return;
    const seat = Number(data.seat);
    if (Number.isNaN(seat) || seat < 0 || seat > 7) {
      flash("Invalid seat", "error");
      return;
    }
    if (kind === "kick") {
      await api(
        "/admin/api/v1/rooms/" +
          encodeURIComponent(room.id) +
          "/seats/" +
          seat +
          "/kick",
        { method: "POST", body: JSON.stringify({ reason: data.reason || "kicked" }) }
      );
      flash("Seat " + seat + " kicked");
    } else {
      await api(
        "/admin/api/v1/rooms/" +
          encodeURIComponent(room.id) +
          "/seats/" +
          seat +
          "/mute",
        {
          method: "POST",
          body: JSON.stringify({
            audio: !!data.audio,
            video: !!data.video,
            chat: !!data.chat,
          }),
        }
      );
      flash("Mute updated for seat " + seat);
    }
    await refresh();
  }

  async function createRoom() {
    const data = await openModal({
      title: "Create room",
      confirmLabel: "Create",
      bodyHtml:
        '<label class="field"><span>Map</span><input name="map" value="E1M1" required/></label>' +
        '<label class="field"><span>Skill (1–5)</span><input name="skill" type="number" min="1" max="5" value="3"/></label>' +
        '<label class="field"><span>Max players</span><input name="max_players" type="number" min="2" max="8" value="2"/></label>' +
        '<label class="field"><span><input name="enable_verifier" type="checkbox"/> Enable verifier</span></label>',
    });
    if (!data) return;
    const body = {
      map: data.map || "E1M1",
      skill: Number(data.skill) || 3,
      max_players: Number(data.max_players) || 2,
      enable_verifier: !!data.enable_verifier,
    };
    const created = await api("/admin/api/v1/rooms", {
      method: "POST",
      body: JSON.stringify(body),
    });
    flash("Created " + shortId(created.id));
    setView("rooms");
    await refresh();
  }

  function escapeHtml(s) {
    return String(s)
      .replace(/&/g, "&amp;")
      .replace(/</g, "&lt;")
      .replace(/>/g, "&gt;")
      .replace(/"/g, "&quot;");
  }

  function renderDashboard() {
    const s = state.summary || {};
    const m = state.metrics;
    $("stat-rooms").textContent = s.rooms ?? "—";
    $("stat-verified").textContent = s.verified_rooms ?? "—";
    $("stat-drain").textContent = s.draining ? "ON" : "off";
    $("stat-drain").style.color = s.draining ? "var(--warn)" : "";
    $("stat-msgs").textContent = fmtNum(m.room_server_ws_msgs_total);
    $("stat-frames").textContent = fmtNum(m.room_server_tic_frames_total);
    $("stat-violations").textContent = fmtNum(m.room_server_violations_total);
    $("node-chip").textContent = "node " + (s.node_id || "—");
    $("drain-status").textContent = s.draining
      ? "Draining — new rooms blocked"
      : "Node accepting rooms";

    const count = m.room_server_relay_latency_us_count || 0;
    const sum = m.room_server_relay_latency_us_sum || 0;
    const avg = count ? Math.round(sum / count) : 0;
    $("relay-summary").textContent = count
      ? "n=" + count + " · avg " + avg + " µs"
      : "no samples yet";

    const buckets = [
      ["≤100µs", m['room_server_relay_latency_us_bucket{le="100"}'] || 0],
      ["≤300µs", m['room_server_relay_latency_us_bucket{le="300"}'] || 0],
      ["≤1ms", m['room_server_relay_latency_us_bucket{le="1000"}'] || 0],
      ["≤2ms", m['room_server_relay_latency_us_bucket{le="2000"}'] || 0],
      ["≤5ms", m['room_server_relay_latency_us_bucket{le="5000"}'] || 0],
      ["+Inf", m['room_server_relay_latency_us_bucket{le="+Inf"}'] || 0],
    ];
    // Convert cumulative histogram to approx per-bucket for display.
    const deltas = buckets.map((b, i) => {
      const prev = i === 0 ? 0 : buckets[i - 1][1];
      return [b[0], Math.max(0, b[1] - prev)];
    });
    const max = Math.max(1, ...deltas.map((d) => d[1]));
    $("relay-bars").innerHTML = deltas
      .map(([label, v]) => {
        const h = Math.max(4, Math.round((v / max) * 100));
        return (
          '<div class="bar"><div class="bar-fill" style="height:' +
          h +
          '%"></div><div class="bar-label">' +
          label +
          "<br/>" +
          v +
          "</div></div>"
        );
      })
      .join("");
  }

  function fmtNum(n) {
    if (n == null || Number.isNaN(n)) return "—";
    return Number(n).toLocaleString();
  }

  function drawMinimap(detail) {
    const canvas = $("room-minimap");
    if (!canvas || !canvas.getContext) return;
    const ctx = canvas.getContext("2d");
    const w = canvas.width;
    const h = canvas.height;
    ctx.fillStyle = "#05080c";
    ctx.fillRect(0, 0, w, h);
    const segs = detail.outline || [];
    const poses = (detail.seats || [])
      .filter((s) => s.pose)
      .map((s) => ({ seat: s.seat, ...s.pose }));
    let minX = Infinity,
      minY = Infinity,
      maxX = -Infinity,
      maxY = -Infinity;
    for (let i = 0; i + 3 < segs.length; i += 4) {
      minX = Math.min(minX, segs[i], segs[i + 2]);
      minY = Math.min(minY, segs[i + 1], segs[i + 3]);
      maxX = Math.max(maxX, segs[i], segs[i + 2]);
      maxY = Math.max(maxY, segs[i + 1], segs[i + 3]);
    }
    for (const p of poses) {
      minX = Math.min(minX, p.x);
      minY = Math.min(minY, p.y);
      maxX = Math.max(maxX, p.x);
      maxY = Math.max(maxY, p.y);
    }
    if (!Number.isFinite(minX)) {
      ctx.fillStyle = "#8492a0";
      ctx.font = "12px sans-serif";
      ctx.fillText("no map outline yet", 16, h / 2);
      return;
    }
    const pad = 12;
    const spanX = Math.max(64, maxX - minX);
    const spanY = Math.max(64, maxY - minY);
    const scale = Math.min((w - pad * 2) / spanX, (h - pad * 2) / spanY);
    const ox = (w - spanX * scale) / 2;
    const oy = (h - spanY * scale) / 2;
    const tx = (x) => ox + (x - minX) * scale;
    const ty = (y) => h - (oy + (y - minY) * scale);
    ctx.strokeStyle = "#3a4a5a";
    ctx.lineWidth = 1;
    ctx.beginPath();
    for (let i = 0; i + 3 < segs.length; i += 4) {
      ctx.moveTo(tx(segs[i]), ty(segs[i + 1]));
      ctx.lineTo(tx(segs[i + 2]), ty(segs[i + 3]));
    }
    ctx.stroke();
    const colors = ["#2fbfa5", "#d9a441", "#6f9fff", "#e080a0", "#c0c070", "#a080ff"];
    for (const p of poses) {
      const c = colors[p.seat % colors.length];
      const x = tx(p.x);
      const y = ty(p.y);
      const rad = ((p.angle || 0) * Math.PI) / 180;
      // Doom map angle: 0 = east; canvas Y is flipped already via ty.
      const dx = Math.cos(rad) * 8;
      const dy = -Math.sin(rad) * 8;
      ctx.strokeStyle = c;
      ctx.fillStyle = c;
      ctx.lineWidth = 2;
      ctx.beginPath();
      ctx.moveTo(x + dx, y + dy);
      ctx.lineTo(x - dy * 0.5 - dx * 0.4, y + dx * 0.5 - dy * 0.4);
      ctx.lineTo(x + dy * 0.5 - dx * 0.4, y - dx * 0.5 - dy * 0.4);
      ctx.closePath();
      ctx.fill();
      ctx.font = "10px monospace";
      ctx.fillText(String(p.seat), x + 6, y - 6);
    }
  }

  function renderRoomDetail() {
    const wrap = $("room-detail");
    if (!state.selectedRoomId || !state.roomDetail) {
      wrap.classList.add("hidden");
      return;
    }
    wrap.classList.remove("hidden");
    const d = state.roomDetail;
    $("room-detail-title").textContent =
      shortId(d.id) + " · " + (d.map || "?");
    $("room-detail-meta").textContent =
      "skill " +
      d.skill +
      " · " +
      (d.occupied || 0) +
      "/" +
      d.max_players +
      " · seed " +
      d.rng_seed +
      (d.started ? " · playing" : " · lobby") +
      (d.iwad_fingerprint ? " · iwad " + d.iwad_fingerprint : "");
    drawMinimap(d);
    const tb = $("room-seats-body");
    tb.innerHTML = "";
    const seats = d.seats || [];
    $("room-seats-empty").classList.toggle("hidden", seats.length > 0);
    for (const seat of seats) {
      const tr = document.createElement("tr");
      tr.innerHTML =
        "<td>" +
        escapeHtml(seat.seat) +
        "</td><td>" +
        escapeHtml(seat.name) +
        "</td><td>" +
        escapeHtml(seat.state) +
        '</td><td class="mono">' +
        escapeHtml(seat.remote_ip) +
        '</td><td class="key" title="' +
        escapeHtml(seat.ticket_id) +
        '">' +
        escapeHtml(seat.ticket_id) +
        '</td><td class="key full" title="' +
        escapeHtml(seat.resume_token) +
        '">' +
        escapeHtml(seat.resume_token) +
        '</td><td class="key" title="' +
        escapeHtml(seat.iwad_fingerprint) +
        '">' +
        escapeHtml(seat.iwad_fingerprint) +
        '</td><td class="row-actions"></td>';
      const actions = tr.querySelector(".row-actions");
      const kick = document.createElement("button");
      kick.type = "button";
      kick.className = "btn small danger";
      kick.textContent = "Kick";
      kick.addEventListener("click", () =>
        api(
          "/admin/api/v1/rooms/" +
            encodeURIComponent(d.id) +
            "/seats/" +
            seat.seat +
            "/kick",
          { method: "POST", body: JSON.stringify({ reason: "kicked" }) }
        )
          .then(() => {
            flash("Seat " + seat.seat + " kicked");
            return refresh();
          })
          .catch(onError)
      );
      const ban = document.createElement("button");
      ban.type = "button";
      ban.className = "btn small danger";
      ban.textContent = "Ban";
      ban.title = "Ban IP + name and kick";
      ban.addEventListener("click", () => banSeat(d.id, seat).catch(onError));
      actions.appendChild(kick);
      actions.appendChild(ban);
      tb.appendChild(tr);
    }
  }

  async function banSeat(roomId, seat) {
    const data = await openModal({
      title: "Ban seat " + seat.seat + " (" + (seat.name || "?") + ")",
      confirmLabel: "Ban & kick",
      danger: true,
      bodyHtml:
        "<p>Create bans from this seat and kick them. Hold Ctrl/Cmd to multi-select scopes.</p>" +
        '<label class="field"><span>Scopes</span>' +
        '<select name="scopes" multiple size="6" style="min-height:8rem">' +
        '<option value="ip" selected>ip (' +
        escapeHtml(seat.remote_ip || "—") +
        ")</option>" +
        '<option value="name" selected>name (' +
        escapeHtml(seat.name || "—") +
        ")</option>" +
        '<option value="name_regex">name pattern (= exact name)</option>' +
        '<option value="fingerprint">fingerprint</option>' +
        '<option value="ticket_id">ticket id</option>' +
        '<option value="turn_user">TURN user</option>' +
        '<option value="auth_key">auth key (ticket/TURN/fp)</option>' +
        '<option value="resume_token">resume token</option>' +
        "</select></label>" +
        '<label class="field"><span>Reason</span>' +
        '<input name="reason" value="banned by operator"/></label>',
    });
    if (!data) return;
    // Multi-select: re-read selected options while modal body still holds them.
    const sel = $("modal-body").querySelector('select[name="scopes"]');
    const scopes = sel
      ? Array.from(sel.selectedOptions).map((o) => o.value)
      : ["ip", "name"];
    const reason = (data.reason || "banned").trim() || "banned";
    await api(
      "/admin/api/v1/rooms/" +
        encodeURIComponent(roomId) +
        "/seats/" +
        seat.seat +
        "/ban",
      {
        method: "POST",
        body: JSON.stringify({ scopes, reason, kick: true, exp: 0 }),
      }
    );
    flash("Banned seat " + seat.seat);
    await refresh();
  }

  async function selectRoom(room) {
    state.selectedRoomId = room.id;
    state.roomDetail = await api(
      "/admin/api/v1/rooms/" + encodeURIComponent(room.id)
    );
    renderRoomDetail();
  }

  function renderRooms() {
    const q = ($("room-filter").value || "").trim().toLowerCase();
    const rows = state.rooms.filter((r) => {
      if (!q) return true;
      return (
        String(r.id).toLowerCase().includes(q) ||
        String(r.map).toLowerCase().includes(q) ||
        String(r.node_id || "").toLowerCase().includes(q)
      );
    });
    const tb = $("rooms-body");
    tb.innerHTML = "";
    $("rooms-empty").classList.toggle("hidden", rows.length > 0);
    for (const room of rows) {
      const tr = document.createElement("tr");
      if (room.id === state.selectedRoomId) tr.classList.add("selected");
      const flags = [];
      if (room.enable_verifier) flags.push('<span class="pill on">verified</span>');
      if (room.started) flags.push('<span class="pill on">live</span>');
      if (!flags.length) flags.push('<span class="pill">—</span>');
      const occ =
        (room.occupied != null ? room.occupied : "?") + "/" + room.max_players;
      tr.innerHTML =
        '<td class="mono" title="' +
        escapeHtml(room.id) +
        '">' +
        escapeHtml(shortId(room.id)) +
        "</td>" +
        "<td>" +
        escapeHtml(room.map) +
        "</td>" +
        "<td>" +
        escapeHtml(room.skill) +
        "</td>" +
        "<td>" +
        escapeHtml(occ) +
        "</td>" +
        '<td class="mono">' +
        escapeHtml(room.node_id || "—") +
        "</td>" +
        "<td>" +
        flags.join(" ") +
        '</td><td class="row-actions"></td>';
      const actions = tr.querySelector(".row-actions");
      const mk = (label, cls, fn) => {
        const b = document.createElement("button");
        b.type = "button";
        b.className = "btn small " + (cls || "");
        b.textContent = label;
        b.addEventListener("click", () => fn().catch(onError));
        return b;
      };
      actions.append(
        mk("Detail", "primary", () => selectRoom(room)),
        mk("Notice", "ghost", () => noticeRoom(room)),
        mk("Kick", "ghost", () => seatAction(room, "kick")),
        mk("Mute", "ghost", () => seatAction(room, "mute")),
        mk("Close", "danger", () => closeRoom(room))
      );
      tb.appendChild(tr);
    }
    renderRoomDetail();
  }

  function banExpiryLabel(exp) {
    if (!exp) return "never";
    const t = Number(exp) * 1000;
    if (!t) return "never";
    try {
      return new Date(t).toLocaleString();
    } catch (_) {
      return String(exp);
    }
  }

  function renderBans() {
    const tb = $("bans-body");
    tb.innerHTML = "";
    $("bans-empty").classList.toggle("hidden", state.bans.length > 0);
    for (const ban of state.bans) {
      const tr = document.createElement("tr");
      tr.innerHTML =
        "<td>" +
        escapeHtml(ban.scope) +
        "</td><td class='mono'>" +
        escapeHtml(ban.value) +
        "</td><td>" +
        escapeHtml(ban.reason) +
        "</td><td class='mono'>" +
        escapeHtml(banExpiryLabel(ban.exp)) +
        "</td><td class='mono' title='" +
        escapeHtml(ban.id) +
        "'>" +
        escapeHtml(shortId(ban.id)) +
        '</td><td class="row-actions"></td>';
      const actions = tr.querySelector(".row-actions");
      const del = document.createElement("button");
      del.type = "button";
      del.className = "btn small";
      del.textContent = "Remove";
      del.addEventListener("click", () =>
        api("/admin/api/v1/bans/" + encodeURIComponent(ban.id), {
          method: "DELETE",
        })
          .then(() => {
            flash("Ban removed");
            return refresh();
          })
          .catch(onError)
      );
      actions.appendChild(del);
      tb.appendChild(tr);
    }
  }

  function renderAudit() {
    const q = ($("audit-filter").value || "").trim().toLowerCase();
    const rows = [...state.audit].reverse().filter((e) => {
      if (!q) return true;
      return (
        String(e.action).toLowerCase().includes(q) ||
        String(e.target).toLowerCase().includes(q) ||
        String(e.detail).toLowerCase().includes(q) ||
        String(e.actor).toLowerCase().includes(q)
      );
    });
    const tb = $("audit-body");
    tb.innerHTML = "";
    $("audit-empty").classList.toggle("hidden", rows.length > 0);
    for (const e of rows.slice(0, 200)) {
      const tr = document.createElement("tr");
      tr.innerHTML =
        "<td>" +
        escapeHtml(fmtTs(e.ts)) +
        "</td><td>" +
        escapeHtml(e.action) +
        "</td><td>" +
        escapeHtml(e.target) +
        "</td><td>" +
        escapeHtml(e.detail) +
        "</td><td>" +
        escapeHtml(e.actor) +
        "</td>";
      tb.appendChild(tr);
    }
  }

  function render() {
    if (state.view === "dashboard") renderDashboard();
    if (state.view === "rooms") renderRooms();
    if (state.view === "bans") renderBans();
    if (state.view === "audit") renderAudit();
    if (state.view === "settings") renderDashboard();
  }

  function onError(err) {
    if (err && err.code === 401) {
      showGate("Unauthorized — check the bearer token.");
      return;
    }
    flash(err.message || String(err), "error");
    $("live-chip").textContent = "stale";
    $("live-chip").classList.add("stale");
    $("live-chip").classList.remove("live");
  }

  function showGate(msg) {
    $("shell").classList.add("hidden");
    $("auth-gate").classList.remove("hidden");
    if (msg) {
      $("gate-error").textContent = msg;
      $("gate-error").classList.remove("hidden");
    } else {
      $("gate-error").classList.add("hidden");
    }
  }

  function showShell() {
    $("auth-gate").classList.add("hidden");
    $("shell").classList.remove("hidden");
  }

  async function refresh() {
    try {
      const [summary, rooms, bans, audit, metricsText] = await Promise.all([
        api("/admin/api/v1/summary"),
        api("/admin/api/v1/rooms"),
        api("/admin/api/v1/bans"),
        api("/admin/api/v1/audit"),
        fetch("/metrics", { headers: headers() })
          .then((r) => (r.ok ? r.text() : ""))
          .catch(() => ""),
      ]);
      state.summary = summary;
      state.rooms = rooms || [];
      state.bans = bans || [];
      state.audit = audit || [];
      state.metrics = parseMetrics(metricsText);
      if (
        state.selectedRoomId &&
        !state.rooms.some((r) => r.id === state.selectedRoomId)
      ) {
        state.selectedRoomId = null;
        state.roomDetail = null;
      } else if (state.selectedRoomId) {
        try {
          state.roomDetail = await api(
            "/admin/api/v1/rooms/" + encodeURIComponent(state.selectedRoomId)
          );
        } catch (_) {
          /* keep last snapshot */
        }
      }
      $("live-chip").textContent = "live";
      $("live-chip").classList.add("live");
      $("live-chip").classList.remove("stale");
      showShell();
      render();
    } catch (err) {
      onError(err);
      throw err;
    }
  }

  // Wire UI
  for (const btn of document.querySelectorAll(".nav-item")) {
    btn.addEventListener("click", () => setView(btn.dataset.view));
  }
  $("refresh").addEventListener("click", () => refresh().catch(() => {}));
  $("room-filter").addEventListener("input", renderRooms);
  $("audit-filter").addEventListener("input", renderAudit);
  $("room-detail-close").addEventListener("click", () => {
    state.selectedRoomId = null;
    state.roomDetail = null;
    renderRoomDetail();
  });
  $("room-create-open").addEventListener("click", () => createRoom().catch(onError));
  $("ban-form").addEventListener("submit", async (ev) => {
    ev.preventDefault();
    try {
      const ttl = Number(($("ban-exp") && $("ban-exp").value) || 0);
      const exp = ttl > 0 ? Math.floor(Date.now() / 1000) + ttl : 0;
      await api("/admin/api/v1/bans", {
        method: "POST",
        body: JSON.stringify({
          scope: $("ban-scope").value,
          value: $("ban-value").value,
          reason: $("ban-reason").value,
          exp,
        }),
      });
      $("ban-value").value = "";
      $("ban-reason").value = "";
      flash("Ban added");
      await refresh();
    } catch (err) {
      onError(err);
    }
  });
  $("drain-form").addEventListener("submit", async (ev) => {
    ev.preventDefault();
    const data = await openModal({
      title: "Begin drain?",
      confirmLabel: "Drain node",
      danger: true,
      bodyHtml:
        "<p>The node will stop accepting new rooms and warn connected players.</p>",
    });
    if (!data) return;
    try {
      await api("/admin/api/v1/maintenance", {
        method: "POST",
        body: JSON.stringify({
          enabled: true,
          message: $("drain-message").value || "Node draining",
        }),
      });
      flash("Drain started");
      await refresh();
    } catch (err) {
      onError(err);
    }
  });
  $("lock-console").addEventListener("click", () => {
    localStorage.removeItem("roomAdminToken");
    $("token").value = "";
    $("gate-token").value = "";
    showGate("");
  });
  $("token").addEventListener("change", () => {
    localStorage.setItem("roomAdminToken", $("token").value);
  });
  $("gate-enter").addEventListener("click", async () => {
    $("token").value = $("gate-token").value;
    localStorage.setItem("roomAdminToken", $("token").value);
    try {
      await refresh();
    } catch (_) {
      /* gate message set in onError */
    }
  });
  $("gate-token").addEventListener("keydown", (ev) => {
    if (ev.key === "Enter") $("gate-enter").click();
  });

  // Boot
  const saved = localStorage.getItem("roomAdminToken") || "";
  $("token").value = saved;
  $("gate-token").value = saved;
  refresh()
    .then(() => showShell())
    .catch(() => {
      if (!saved) showGate("");
    });
  setInterval(() => {
    if ($("shell").classList.contains("hidden")) return;
    refresh().catch(() => {});
  }, 5000);
})();
