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
    selectedPresetId: null,
    roomDetail: null,
    authMode: "open",
    csrf: "",
    username: "",
    methods: [],
  };

  const TITLES = {
    dashboard: ["Dashboard", "Cluster pulse for this node"],
    rooms: ["Rooms", "Live rooms on this node"],
    presets: ["Presets", "Set up game files, map access, and room defaults"],
    bans: ["Bans", "IP · pseudo · fingerprint · ticket · TURN · auth keys"],
    audit: ["Audit", "Append-only operator actions"],
    settings: ["Settings", "Drain and session controls"],
  };

  function token() {
    return (localStorage.getItem("roomAdminToken") || "").trim();
  }

  function headers(extra = {}, mutating = false) {
    const h = { Accept: "application/json", ...extra };
    const t = token();
    if (t) h.Authorization = "Bearer " + t;
    if (mutating && state.csrf) h["X-CSRF-Token"] = state.csrf;
    return h;
  }

  async function api(path, opts = {}) {
    const method = (opts.method || "GET").toUpperCase();
    const mutating = method !== "GET" && method !== "HEAD";
    const init = {
      credentials: "same-origin",
      ...opts,
      headers: headers(
        {
          ...(opts.body ? { "Content-Type": "application/json" } : {}),
          ...(opts.headers || {}),
        },
        mutating
      ),
    };
    const res = await fetch(path, init);
    if (res.status === 401 || res.status === 403) {
      const err = new Error(res.status === 403 ? "csrf/forbidden" : "unauthorized");
      err.code = res.status;
      throw err;
    }
    if (!res.ok) {
      let detail = res.status + " " + path;
      try {
        const j = await res.clone().json();
        if (typeof j === "string") detail = j;
        else if (j && j.message) detail = j.message;
      } catch (_) {
        try {
          const t = await res.clone().text();
          if (t) detail = t;
        } catch (_) {}
      }
      const err = new Error(detail);
      err.code = res.status;
      throw err;
    }
    if (res.status === 204) return null;
    const ct = res.headers.get("content-type") || "";
    return ct.includes("json") ? res.json() : res.text();
  }

  function b64urlToBuf(b64) {
    const pad = "=".repeat((4 - (b64.length % 4)) % 4);
    const s = (b64 + pad).replace(/-/g, "+").replace(/_/g, "/");
    const bin = atob(s);
    const out = new Uint8Array(bin.length);
    for (let i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i);
    return out.buffer;
  }

  function bufToB64url(buf) {
    const bytes = new Uint8Array(buf);
    let s = "";
    for (const b of bytes) s += String.fromCharCode(b);
    return btoa(s).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
  }

  function revivePublicKey(opts) {
    const o = structuredClone(opts);
    o.challenge = b64urlToBuf(o.challenge);
    if (o.user && o.user.id) o.user.id = b64urlToBuf(o.user.id);
    if (o.excludeCredentials) {
      o.excludeCredentials = o.excludeCredentials.map((c) => ({
        ...c,
        id: b64urlToBuf(c.id),
      }));
    }
    if (o.allowCredentials) {
      o.allowCredentials = o.allowCredentials.map((c) => ({
        ...c,
        id: b64urlToBuf(c.id),
      }));
    }
    return o;
  }

  function credentialToJson(cred) {
    const r = cred.response;
    const out = {
      id: cred.id,
      rawId: bufToB64url(cred.rawId),
      type: cred.type,
      response: {
        clientDataJSON: bufToB64url(r.clientDataJSON),
      },
    };
    if (r.attestationObject) {
      out.response.attestationObject = bufToB64url(r.attestationObject);
    }
    if (r.authenticatorData) {
      out.response.authenticatorData = bufToB64url(r.authenticatorData);
    }
    if (r.signature) out.response.signature = bufToB64url(r.signature);
    if (r.userHandle) out.response.userHandle = bufToB64url(r.userHandle);
    return out;
  }

  function showGateStep(name) {
    for (const el of document.querySelectorAll(".gate-step")) {
      el.classList.toggle("hidden", el.id !== "gate-step-" + name);
    }
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
    if (name === "settings") {
      refreshSecurity().catch(() => {});
      refreshMods().catch(() => {});
      refreshContent().catch(() => {});
    }
    if (name === "presets") refreshContent().catch(() => {});
  }

  function openModal({ title, bodyHtml, confirmLabel = "Confirm", danger = false, onReady }) {
    return new Promise((resolve) => {
      state.modalResolve = resolve;
      $("modal-title").textContent = title;
      $("modal-body").innerHTML = bodyHtml;
      const conf = $("modal-confirm");
      conf.textContent = confirmLabel;
      conf.className = "btn " + (danger ? "danger" : "primary");
      $("modal").showModal();
      if (onReady) onReady($("modal-body"));
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
        : '<label class="inline-check"><input name="audio" type="checkbox" checked/> Mute audio</label>' +
            '<label class="inline-check"><input name="video" type="checkbox" checked/> Mute video</label>' +
            '<label class="inline-check"><input name="chat" type="checkbox"/> Freeze chat</label>'),
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
    const presetConfig = await api("/admin/api/v1/presets");
    const presets = presetConfig.presets || [];
    const requirePreset = !!presetConfig.require_preset && presets.length > 0;
    const presetOptions = [
      ...(!requirePreset ? ['<option value="">Custom room</option>'] : []),
      ...presets.map((p) => '<option value="' + escapeHtml(p.id) + '">' + escapeHtml(p.title || p.id) + "</option>"),
    ].join("");
    const data = await openModal({
      title: "Create room",
      confirmLabel: "Create",
      bodyHtml:
        '<div class="room-create-grid">' +
        '<label class="field"><span>Game</span><select name="game" id="create-game"><option value="doom">Doom</option><option value="duke3d">Duke Nukem 3D</option></select></label>' +
        '<label class="field" id="create-preset-field"><span>Room setup</span><select name="preset" id="create-preset">' +
        (presetOptions || '<option value="">No presets configured</option>') + '</select></label>' +
        '<div class="field" id="create-preset-summary" aria-live="polite"></div>' +
        '<div class="field" id="create-map-field"></div>' +
        '<label class="field"><span>Difficulty (1–5)</span><select name="skill" id="create-skill">' +
        '<option value="1">1 · Easiest</option><option value="2">2 · Easy</option><option value="3" selected>3 · Normal</option>' +
        '<option value="4">4 · Hard</option><option value="5">5 · Hardest</option></select></label>' +
        '<label class="field"><span>Max players</span><input name="max_players" type="number" min="2" max="64" value="2"/></label>' +
        '<label class="field"><span>Visibility</span><select name="visibility"><option value="public">Public · listed in lobby</option><option value="private">Private · join by room id</option></select></label>' +
        '<label class="field"><span>Time limit (minutes)</span><input name="time_limit" type="number" min="1" max="180" value="20"/></label>' +
        '<div class="field" id="create-duke-options"><span>Duke match</span><select name="duke_game_mode"><option value="0">DukeMatch</option><option value="1">Co-op</option><option value="2">No enemies</option></select>' +
        '<label class="inline-check"><input name="duke_friendly_fire" type="checkbox"/> Friendly fire</label></div>' +
        '<label class="inline-check" id="create-verifier-field"><input name="enable_verifier" type="checkbox"/> Enable verifier</label>' +
        '</div>',
      onReady: (body) => {
        const game = body.querySelector("#create-game");
        const preset = body.querySelector("#create-preset");
        const presetField = body.querySelector("#create-preset-field");
        const gameField = game.closest(".field");
        const summary = body.querySelector("#create-preset-summary");
        const mapField = body.querySelector("#create-map-field");
        const skill = body.querySelector("#create-skill");
        const dukeOptions = body.querySelector("#create-duke-options");
        const verifierField = body.querySelector("#create-verifier-field");
        if (requirePreset && !preset.value) preset.value = presets[0]?.id || "";
        const findPreset = () => presets.find((p) => p.id === preset.value);
        const update = () => {
          const selected = findPreset();
          if (selected) game.value = selected.game || "doom";
          const isDuke = selected ? selected.game === "duke3d" : game.value === "duke3d";
          preset.disabled = !presets.length;
          presetField.classList.toggle("hidden", !presets.length && !requirePreset);
          gameField.classList.toggle("hidden", !!selected || requirePreset);
          summary.classList.toggle("hidden", !presets.length);
          dukeOptions.classList.toggle("hidden", !isDuke || !!selected);
          verifierField.classList.toggle("hidden", isDuke);
          mapField.replaceChildren();
          const label = document.createElement("label");
          label.className = "field";
          const labelText = document.createElement("span");
          labelText.textContent = selected ? "Map in this preset" : "Map";
          label.append(labelText);
          const allowedMaps = selected && Array.isArray(selected.maps) ? selected.maps : [];
          if (selected && allowedMaps.length) {
            const select = document.createElement("select");
            select.name = "map";
            select.required = true;
            const initial = allowedMaps.includes(selected.default_map) ? selected.default_map : allowedMaps[0];
            for (const name of allowedMaps) {
              const option = document.createElement("option");
              option.value = name;
              option.textContent = name;
              option.selected = name === initial;
              select.append(option);
            }
            label.append(select);
          } else {
            const input = document.createElement("input");
            input.name = "map";
            input.value = selected?.default_map || (isDuke ? "E1L1" : "E1M1");
            input.placeholder = isDuke ? "E1L1" : "E1M1 or MAP01";
            input.required = true;
            label.append(input);
          }
          mapField.append(label);
          if (selected) {
            const files = [selected.iwad?.file, ...(selected.content || []).map((f) => f.file)].filter(Boolean).join(" + ") || "server default game files";
            const details = [files, allowedMaps.length ? `${allowedMaps.length} maps` : "any map", selected.mod ? `mod: ${selected.mod}` : ""].filter(Boolean).join(" · ");
            summary.replaceChildren();
            if (selected.description) {
              const description = document.createElement("p");
              description.className = "preset-blurb";
              description.textContent = selected.description;
              summary.append(description);
            }
            const meta = document.createElement("p");
            meta.className = "preset-meta";
            meta.textContent = details;
            summary.append(meta);
            skill.value = selected.skill >= 1 && selected.skill <= 5 ? String(selected.skill) : "3";
            if (selected.game === "duke3d") {
              body.querySelector('select[name="duke_game_mode"]').value = String(selected.duke_game_mode ?? 1);
              body.querySelector('input[name="duke_friendly_fire"]').checked = !!selected.duke_friendly_fire;
            }
          } else if (presets.length) {
            skill.value = "3";
            summary.textContent = isDuke
              ? "Custom Duke room using the server’s Duke GRP."
              : "Custom room using the server’s default game files.";
          }
        };
        game.addEventListener("change", () => {
          const selected = findPreset();
          if (selected && selected.game !== game.value) {
            const matching = presets.find((p) => p.game === game.value);
            preset.value = requirePreset ? (matching?.id || "") : "";
          }
          update();
        });
        preset.addEventListener("change", update);
        update();
      },
    });
    if (!data) return;
    const selectedPreset = presets.find((p) => p.id === data.preset);
    const roomGame = selectedPreset?.game || data.game || "doom";
    const body = {
      game: roomGame,
      preset: data.preset || null,
      map: data.map || (roomGame === "duke3d" ? "E1L1" : "E1M1"),
      skill: Number(data.skill) || selectedPreset?.skill || 3,
      duke_game_mode: selectedPreset?.duke_game_mode ?? Number(data.duke_game_mode) ?? 1,
      duke_friendly_fire: selectedPreset ? !!selectedPreset.duke_friendly_fire : data.duke_friendly_fire === true,
      max_players: Number(data.max_players) || 2,
      visibility: data.visibility || "public",
      time_limit: Number(data.time_limit) || 20,
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
    $("node-chip").textContent = s.node_id || "—";
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
    if (state.view === "presets") renderPresets();
    if (state.view === "bans") renderBans();
    if (state.view === "audit") renderAudit();
    if (state.view === "settings") renderDashboard();
  }

  function onError(err) {
    if (err && (err.code === 401 || err.code === 403)) {
      state.csrf = "";
      state.username = "";
      showGate(err.code === 403 ? "Session/CSRF rejected — sign in again." : "Sign-in required.");
      routeGate().catch(() => {});
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
    $("whoami").textContent = state.username || (token() ? "token" : "open");
  }

  async function routeGate() {
    const st = await fetch("/admin/api/v1/auth/status", { credentials: "same-origin" }).then((r) =>
      r.json()
    );
    state.authMode = st.mode;
    if (st.user && st.user.csrf) {
      state.csrf = st.user.csrf;
      state.username = st.user.username || "";
      showShell();
      return true;
    }
    showGate("");
    if (st.bootstrap || st.mode === "mfa") {
      $("gate-lede").textContent =
        "Sign in with your admin password. An authenticator or passkey is asked for only after you set one up.";
      if (st.bootstrap) showGateStep("bootstrap");
      else showGateStep("password");
    } else {
      $("gate-lede").textContent =
        "Dev mode — bearer token (optional if the node is open). Prefer --admin-bootstrap for MFA.";
      showGateStep("token");
    }
    return false;
  }

  async function afterPassword(step) {
    if (step.csrf) {
      await finishSession(step);
      return;
    }
    state.methods = step.methods || [];
    $("gate-use-key").classList.toggle("hidden", !state.methods.includes("webauthn"));
    if (step.need === "enroll") {
      showGateStep("enroll");
      return;
    }
    if (step.need === "webauthn") {
      await doWebAuthnLogin(step.publicKey);
      return;
    }
    showGateStep("totp");
    $("gate-totp").focus();
  }

  async function finishSession(body) {
    state.csrf = body.csrf || "";
    const me = await api("/admin/api/v1/auth/me");
    state.username = (me.user && me.user.username) || "";
    if (me.user && me.user.csrf) state.csrf = me.user.csrf;
    await refresh();
  }

  async function doWebAuthnLogin(publicKey) {
    if (!publicKey) {
      const begun = await api("/admin/api/v1/auth/webauthn/login/begin", {
        method: "POST",
        body: "{}",
      });
      publicKey = begun.publicKey;
    }
    if (!window.PublicKeyCredential) {
      throw new Error("This browser has no WebAuthn support");
    }
    const cred = await navigator.credentials.get({
      publicKey: revivePublicKey(publicKey),
    });
    const body = await api("/admin/api/v1/auth/webauthn/login/finish", {
      method: "POST",
      body: JSON.stringify({ credential: credentialToJson(cred) }),
    });
    await finishSession(body);
  }

  async function doWebAuthnRegister(name) {
    const begun = await api("/admin/api/v1/auth/webauthn/register/begin", {
      method: "POST",
      body: "{}",
    });
    const cred = await navigator.credentials.create({
      publicKey: revivePublicKey(begun.publicKey),
    });
    const body = await api("/admin/api/v1/auth/webauthn/register/finish", {
      method: "POST",
      body: JSON.stringify({
        credential: credentialToJson(cred),
        name: name || "YubiKey",
      }),
    });
    if (body && body.csrf) await finishSession(body);
    else flash("Security key registered");
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
  $("lock-console").addEventListener("click", async () => {
    try {
      await api("/admin/api/v1/auth/logout", { method: "POST", body: "{}" });
    } catch (_) {}
    localStorage.removeItem("roomAdminToken");
    state.csrf = "";
    state.username = "";
    showGate("");
    routeGate().catch(() => {});
  });
  $("gate-enter").addEventListener("click", async () => {
    localStorage.setItem("roomAdminToken", ($("gate-token").value || "").trim());
    try {
      await refresh();
    } catch (_) {}
  });
  $("gate-token").addEventListener("keydown", (ev) => {
    if (ev.key === "Enter") $("gate-enter").click();
  });
  $("boot-submit").addEventListener("click", async () => {
    try {
      await api("/admin/api/v1/auth/bootstrap", {
        method: "POST",
        body: JSON.stringify({
          username: $("boot-user").value,
          password: $("boot-pass").value,
        }),
      });
      $("gate-user").value = $("boot-user").value;
      $("gate-pass").value = $("boot-pass").value;
      showGateStep("password");
      flash("Admin created — sign in");
    } catch (err) {
      showGate(err.message || String(err));
    }
  });
  $("gate-login").addEventListener("click", async () => {
    try {
      const step = await api("/admin/api/v1/auth/login", {
        method: "POST",
        body: JSON.stringify({
          username: $("gate-user").value,
          password: $("gate-pass").value,
        }),
      });
      $("gate-error").classList.add("hidden");
      await afterPassword(step);
    } catch (err) {
      showGate(err.message || String(err));
    }
  });
  $("gate-pass").addEventListener("keydown", (ev) => {
    if (ev.key === "Enter") $("gate-login").click();
  });
  $("gate-totp-submit").addEventListener("click", async () => {
    try {
      const body = await api("/admin/api/v1/auth/totp", {
        method: "POST",
        body: JSON.stringify({ code: $("gate-totp").value }),
      });
      await finishSession(body);
    } catch (err) {
      showGate(err.message || String(err));
    }
  });
  $("gate-totp").addEventListener("keydown", (ev) => {
    if (ev.key === "Enter") $("gate-totp-submit").click();
  });
  $("gate-use-key").addEventListener("click", () => {
    doWebAuthnLogin(null).catch((err) => showGate(err.message || String(err)));
  });
  $("enroll-totp").addEventListener("click", async () => {
    try {
      const body = await api("/admin/api/v1/auth/totp/enroll", {
        method: "POST",
        body: "{}",
      });
      $("enroll-totp-box").classList.remove("hidden");
      $("enroll-secret").textContent = body.secret;
      $("enroll-otpauth").href = body.otpauth;
    } catch (err) {
      showGate(err.message || String(err));
    }
  });
  $("enroll-totp-confirm").addEventListener("click", async () => {
    try {
      const body = await api("/admin/api/v1/auth/totp/confirm", {
        method: "POST",
        body: JSON.stringify({ code: $("enroll-totp-code").value }),
      });
      await finishSession(body);
    } catch (err) {
      showGate(err.message || String(err));
    }
  });
  $("enroll-key").addEventListener("click", () => {
    doWebAuthnRegister("YubiKey").catch((err) => showGate(err.message || String(err)));
  });
  $("add-passkey").addEventListener("click", async () => {
    try {
      await doWebAuthnRegister("YubiKey");
      await refreshSecurity();
    } catch (err) {
      onError(err);
    }
  });

  async function refreshSecurity() {
    try {
      const users = await api("/admin/api/v1/auth/users");
      const me = users.find((u) => u.username === state.username) || users[0];
      const keys = (me && me.keys) || [];
      $("keys-status").textContent = keys.length
        ? keys.length + " key(s)"
        : "No security keys yet";
      $("keys-list").replaceChildren(
        ...keys.map((k) => {
          const li = document.createElement("li");
          li.innerHTML =
            "<span>" +
            (k.name || "key") +
            '</span><span class="muted">' +
            shortId(k.id) +
            "</span>";
          return li;
        })
      );
    } catch (_) {
      $("keys-status").textContent = "—";
    }
  }

  async function refreshMods() {
    let mods;
    try {
      mods = await api("/admin/api/v1/mods");
    } catch (_) {
      $("mods-status").textContent = "—";
      return;
    }
    const allowed = mods.filter((m) => m.allowed).length;
    $("mods-status").textContent = mods.length
      ? allowed + " of " + mods.length + " allowed · new mods start disallowed"
      : "No mods on this node (--mods-dir)";
    $("mods-list").replaceChildren(
      ...mods.map((m) => {
        const li = document.createElement("li");
        li.innerHTML =
          "<span><strong>" +
          escapeHtml(m.title) +
          '</strong> <span class="muted">' +
          escapeHtml(m.name) +
          " · " +
          fmtNum(Math.round(m.size / 1024)) +
          " KB · " +
          escapeHtml(String(m.fingerprint).slice(0, 8)) +
          "</span></span>";
        const btn = document.createElement("button");
        btn.type = "button";
        btn.className = m.allowed ? "btn" : "btn primary";
        btn.textContent = m.allowed ? "Disallow" : "Allow";
        btn.addEventListener("click", async () => {
          btn.disabled = true;
          try {
            await api("/admin/api/v1/mods/" + encodeURIComponent(m.name), {
              method: "POST",
              body: JSON.stringify({ allowed: !m.allowed }),
            });
            flash((m.allowed ? "Disallowed " : "Allowed ") + m.title);
          } catch (e) {
            flash("Mod update failed: " + e.message, "error");
          }
          await refreshMods();
        });
        li.appendChild(btn);
        return li;
      })
    );
  }


  // ------------------------------------------------------------ load plan
  // What each file of a preset brings and how the order combines them
  // (POST /admin/api/v1/presets/plan, server load_plan.rs).
  const PLAN_ROLES = { engine: "Engine", base: "Base game", addon: "Add-on", mod: "Gameplay mod" };
  const planCache = new Map();

  function planKey(iwad, files, mod) {
    return JSON.stringify([iwad || "", files || [], mod || ""]);
  }

  async function fetchPlan(iwad, files, mod) {
    const key = planKey(iwad, files, mod);
    if (planCache.has(key)) return planCache.get(key);
    const plan = await api("/admin/api/v1/presets/plan", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ iwad: iwad || null, content: files || [], mod: mod || null }),
    });
    planCache.set(key, plan);
    return plan;
  }

  const n0 = (n) => Number(n || 0).toLocaleString("en-US");
  const plural = (n, one, many) => n0(n) + " " + (Number(n) === 1 ? one : many || one + "s");

  function renderLoadPlan(plan) {
    const steps = plan.steps || [];
    const rules = new Map();
    const stepHtml = steps.map((s) => {
      const warns = s.warnings || [];
      const parts = [];
      if (warns.length) parts.push('<ul class="plan-warn">' + warns.map((w) => "<li>" + escapeHtml(w) + "</li>").join("") + "</ul>");
      if ((s.provides || []).length) {
        parts.push('<div class="plan-block"><h5>Contains</h5><div class="plan-chips">' +
          s.provides.map((c) => '<span class="plan-chip">' + escapeHtml(c.label) + " <b>" + n0(c.count) + "</b></span>").join("") + "</div></div>");
      }
      const overrideList = (list, dir) => '<ul class="plan-list">' + list.map((o) =>
        "<li><b>" + n0(o.count) + "</b> " + escapeHtml(o.label.charAt(0).toLowerCase() + o.label.slice(1)) + " " + dir + " <code>" + escapeHtml(o.file) + "</code>" +
        (o.examples && o.examples.length ? ' <span class="muted">(' + escapeHtml(o.examples.slice(0, 6).join(", ")) + (o.count > 6 ? ", …" : "") + ")</span>" : "") +
        "</li>").join("") + "</ul>";
      if ((s.overrides || []).length) {
        parts.push('<div class="plan-block"><h5>Takes over from files above</h5><p class="plan-help">Same names: this file’s version is the one used.</p>' + overrideList(s.overrides, "replacing those of") + "</div>");
      }
      if ((s.overridden_by || []).length) {
        parts.push('<div class="plan-block"><h5>Taken over by files below</h5><p class="plan-help">Same names: the later file’s version is used instead of this one.</p>' + overrideList(s.overridden_by, "replaced by") + "</div>");
      }
      const c = s.classes || {};
      if (c.zscript || c.decorate || c.extends) {
        const counts = [
          c.decorate ? plural(c.decorate, "DECORATE actor") : "",
          c.zscript ? plural(c.zscript, "ZScript class", "ZScript classes") : "",
          c.extends ? plural(c.extends, "class extension") : "",
        ].filter(Boolean).join(", ");
        let html = '<div class="plan-block"><h5>Actors and scripts</h5><p>' + escapeHtml(counts) + ".</p>";
        if (s.replacements_total) {
          html += "<p>" + escapeHtml(plural(s.replacements_total, "actor")) + " replace" + (s.replacements_total === 1 ? "s" : "") +
            " an actor of the game: whenever a map or the game spawns the old one, this one appears instead.</p>" +
            '<ul class="plan-replace">' + (s.replacements || []).map((r) => "<li><code>" + escapeHtml(r.actor) + "</code> → <code>" + escapeHtml(r.by) + "</code></li>").join("") +
            (s.replacements_total > (s.replacements || []).length ? '<li class="muted">+' + n0(s.replacements_total - s.replacements.length) + " more</li>" : "") + "</ul>";
        }
        parts.push(html + "</div>");
      }
      if ((s.definitions || []).length) {
        for (const d of s.definitions) rules.set(d.combine, d.rule);
        parts.push('<div class="plan-block"><h5>Definition lumps</h5><table class="plan-defs"><thead><tr><th>Lump</th><th>Files</th><th>What it holds</th><th>With other files</th></tr></thead><tbody>' +
          s.definitions.map((d) => "<tr><td><code>" + escapeHtml(d.lump) + "</code></td><td>" + n0(d.count) + "</td><td>" + escapeHtml(d.what) +
            '</td><td title="' + escapeHtml(d.rule) + '">' + escapeHtml(d.combine) + "</td></tr>").join("") + "</tbody></table></div>");
      }
      if (s.room_mod) {
        const m = s.room_mod;
        parts.push('<div class="plan-block"><h5>Room mod</h5><dl class="plan-mod">' +
          "<dt>Name</dt><dd>" + escapeHtml(m.name || m.id || "—") + (m.id ? ' <code>' + escapeHtml(m.id) + "</code>" : "") + "</dd>" +
          (m.description ? "<dt>About</dt><dd>" + escapeHtml(m.description) + "</dd>" : "") +
          ((m.modules || []).length ? "<dt>Code</dt><dd>WebAssembly module " + m.modules.map((x) => "<code>" + escapeHtml(x) + "</code>").join(", ") + ", run by Room’s mod host</dd>" : "") +
          ((m.replaces_zscript || []).length ? "<dt>Replaces</dt><dd>" + m.replaces_zscript.map((x) => "<code>" + escapeHtml(x) + "</code>").join(", ") + " — Room leaves these ZScript files out of the compile</dd>" : "") +
          "</dl></div>");
      }
      if ((s.notes || []).length) parts.push('<ul class="plan-notes">' + s.notes.map((x) => "<li>" + escapeHtml(x) + "</li>").join("") + "</ul>");
      const meta = [PLAN_ROLES[s.role] || s.role, s.size ? fmtBytes(s.size) : ""].filter(Boolean).join(" · ");
      return '<li class="plan-step role-' + escapeHtml(s.role) + (warns.length ? " has-warn" : "") + '"><details' + (warns.length ? " open" : "") + ">" +
        '<summary><span class="plan-order">' + s.order + '</span><span class="plan-head"><span class="plan-file"><code>' + escapeHtml(s.file) + '</code><span class="plan-meta">' + escapeHtml(meta) + "</span></span>" +
        '<span class="plan-summary">' + escapeHtml(s.summary) + "</span></span>" +
        (warns.length ? '<span class="plan-badge">' + plural(warns.length, "problem") + "</span>" : "") + "</summary>" +
        '<div class="plan-body">' + (parts.join("") || '<p class="muted">Nothing more to show.</p>') + "</div></details></li>";
    }).join("");
    const warnings = plan.warnings || [];
    const status = warnings.length
      ? '<div class="plan-alert"><strong>This order needs a change</strong><ul>' + warnings.map((w) => "<li>" + escapeHtml(w) + "</li>").join("") + "</ul></div>"
      : '<div class="plan-ok">No load-order problems found.</div>';
    const legend = rules.size
      ? '<dl class="plan-legend">' + [...rules].map(([k, v]) => "<dt>" + escapeHtml(k) + "</dt><dd>" + escapeHtml(v) + "</dd>").join("") + "</dl>"
      : "";
    return '<div class="load-plan">' +
      '<p class="plan-help">Files load from top to bottom. A file can take over the maps, sprites, sounds, music and graphics of the same name from any file above it; definition lumps (MAPINFO, SNDINFO…) add up; actors come from ZScript, compiled together, or DECORATE, read in order. Open a file to see what it brings.</p>' +
      status + '<ol class="plan-steps">' + stepHtml + "</ol>" + legend + "</div>";
  }

  // ---------------------------------------------------------------- content + presets
  const content = { files: [], presets: { require_preset: false, presets: [] }, mods: [], enabled: false };

  function fmtBytes(n) {
    if (n >= 1 << 30) return (n / (1 << 30)).toFixed(1) + " GB";
    if (n >= 1 << 20) return (n / (1 << 20)).toFixed(1) + " MB";
    return Math.max(1, Math.round(n / 1024)) + " KB";
  }

  // Skill names in each game's own words (Room skill 1–5; Duke menu skills
  // are Room 2–5).
  const PRESET_SKILLS = {
    doom: ["I'm too young to die", "Hey, not too rough", "Hurt me plenty", "Ultra-Violence", "Nightmare!"],
    duke3d: [null, "Piece of Cake", "Let's Rock", "Come Get Some", "Damn I'm Good"],
  };
  const DUKE_MODES = ["DukeMatch", "Cooperative", "DukeMatch (no spawn)"];

  function presetSkillLabel(p) {
    const n = Number(p.skill) || 0;
    if (!n) return "Player's choice";
    return PRESET_SKILLS[p.game === "duke3d" ? "duke3d" : "doom"][n - 1] || "Skill " + n;
  }

  function presetModeLabel(p) {
    if (p.game !== "duke3d") {
      return p.doom_mode === "deathmatch" ? "Deathmatch" : p.doom_mode === "coop" ? "Cooperative" : "Player's choice";
    }
    const mode = DUKE_MODES[Number(p.duke_game_mode ?? 1)] || "Cooperative";
    return Number(p.duke_game_mode ?? 1) === 1 && p.duke_friendly_fire ? mode + ", friendly fire" : mode;
  }

  function renderPresets() {
    const ps = content.presets.presets || [];
    const list = $("preset-list");
    if (!list) return;
    const selected = ps.find((p) => p.id === state.selectedPresetId) || ps[0] || null;
    state.selectedPresetId = selected ? selected.id : null;
    $("preset-require").checked = !!content.presets.require_preset;
    $("presets-status").textContent = ps.length
      ? `${ps.length} saved · ${content.presets.require_preset ? "required for new games" : "optional"}`
      : "None yet";
    list.replaceChildren(
      ...ps.map((p) => {
        const li = document.createElement("li");
        const btn = document.createElement("button");
        btn.type = "button";
        btn.className = "preset-item" + (p.id === state.selectedPresetId ? " on" : "");
        btn.setAttribute("role", "option");
        btn.setAttribute("aria-selected", String(p.id === state.selectedPresetId));
        const maps = p.maps && p.maps.length ? `${p.maps.length} map${p.maps.length === 1 ? "" : "s"}` : "any map";
        btn.innerHTML =
          '<span class="preset-item-title">' + escapeHtml(p.title || p.id) + "</span>" +
          '<span class="preset-item-meta"><span class="game-tag ' + (p.game === "duke3d" ? "duke" : "doom") + '">' +
          (p.game === "duke3d" ? "Duke 3D" : "Doom") + "</span>" +
          escapeHtml([p.mod, maps].filter(Boolean).join(" · ")) + "</span>";
        btn.addEventListener("click", () => {
          state.selectedPresetId = p.id;
          renderPresets();
        });
        li.appendChild(btn);
        return li;
      })
    );
    if (!ps.length) {
      list.innerHTML = '<li class="preset-list-empty muted">Create a preset to offer players a ready-made game: base game, add-ons, maps and defaults.</li>';
    }

    const preview = $("preset-preview");
    if (!selected) {
      preview.innerHTML =
        '<div class="preset-empty"><strong>No preset selected</strong>' +
        "<span>A preset bundles a base game (IWAD or GRP), optional WAD / PK3 add-ons, the maps players may pick and default settings. " +
        "Players see its name and description in <em>Host game</em>.</span>" +
        '<button type="button" class="btn primary" data-preset-new>New preset</button></div>';
      preview.querySelector("[data-preset-new]").addEventListener("click", () => editPreset(-1));
      return;
    }
    const idx = ps.indexOf(selected);
    const files = [selected.iwad ? selected.iwad + " (base game)" : "Server default game", ...(selected.content || [])];
    const sizeOf = (name) => content.files.find((f) => f.file === name)?.size || 0;
    const total = [selected.iwad, ...(selected.content || [])].filter(Boolean).reduce((t, f) => t + sizeOf(f), 0);
    const maps = selected.maps || [];
    const defaultMap = selected.default_map || maps[0] || "Server default";
    const shownMaps = maps.slice(0, 48);
    const defaults = [
      ["Default map", defaultMap],
      ["Skill", presetSkillLabel(selected)],
      ["Game mode", presetModeLabel(selected)],
    ];
    if (selected.game !== "duke3d") defaults.push(["Gameplay mod", selected.mod || "None"]);
    preview.innerHTML =
      '<div class="preset-preview-heading">' +
      '<div><span class="game-tag ' + (selected.game === "duke3d" ? "duke" : "doom") + '">' + (selected.game === "duke3d" ? "Duke Nukem 3D" : "Doom") + "</span>" +
      "<h3>" + escapeHtml(selected.title || selected.id) + "</h3>" +
      '<p class="muted">' + escapeHtml(selected.description || "No description — players only see the name.") + "</p></div>" +
      '<div class="preset-preview-actions">' +
      '<button type="button" class="btn primary small" data-act="edit">Edit</button>' +
      '<button type="button" class="btn small" data-act="dup">Duplicate</button>' +
      '<button type="button" class="btn danger small" data-act="del">Delete</button></div></div>' +
      '<div class="preset-cards">' +
      '<section class="preset-card' + (selected.game === "duke3d" ? "" : " wide") + '" id="preset-load-card"><h4>Load order' + (total ? ' <span class="muted">' + fmtBytes(total) + " download</span>" : "") + "</h4><ol>" +
      files.map((f) => "<li><code>" + escapeHtml(f) + "</code></li>").join("") + "</ol></section>" +
      '<section class="preset-card"><h4>Defaults</h4><dl class="preset-detail-grid">' +
      defaults.map(([k, v]) => "<div><dt>" + escapeHtml(k) + "</dt><dd>" + escapeHtml(v) + "</dd></div>").join("") + "</dl></section>" +
      '<section class="preset-card wide"><h4>Allowed maps <span class="muted">' + (maps.length ? maps.length : "any") + "</span></h4>" +
      (maps.length
        ? '<div class="map-chips">' + shownMaps.map((m) => '<span class="map-chip' + (m === defaultMap ? " default" : "") + '">' + escapeHtml(m) + "</span>").join("") +
          (maps.length > shownMaps.length ? '<span class="muted">+' + (maps.length - shownMaps.length) + " more</span>" : "") + "</div>"
        : '<p class="muted">Players may type any map name.</p>') +
      "</section></div>" +
      '<p class="preset-id muted">Preset ID <code>' + escapeHtml(selected.id) + "</code></p>";
    if (selected.game !== "duke3d") {
      const card = preview.querySelector("#preset-load-card");
      const shownId = selected.id;
      fetchPlan(selected.iwad, selected.content || [], selected.mod)
        .then((plan) => {
          if (state.selectedPresetId !== shownId || !card.isConnected) return;
          card.querySelector("ol").outerHTML = renderLoadPlan(plan);
        })
        .catch(() => {});
    }
    preview.querySelector('[data-act="edit"]').addEventListener("click", () => editPreset(idx));
    preview.querySelector('[data-act="dup"]').addEventListener("click", () => editPreset(-1, selected));
    preview.querySelector('[data-act="del"]').addEventListener("click", () => deletePreset(idx));
  }

  async function deletePreset(idx) {
    const ps = content.presets.presets || [];
    const preset = ps[idx];
    if (!preset) return;
    if (!confirm(`Delete preset “${preset.title || preset.id}”? Games already running keep their files.`)) return;
    const remaining = ps.filter((_, i) => i !== idx);
    state.selectedPresetId = remaining[Math.min(idx, remaining.length - 1)]?.id || null;
    await savePresets({ ...content.presets, presets: remaining });
  }

  async function refreshContent() {
    planCache.clear();
    let lib;
    try {
      lib = await api("/admin/api/v1/content");
      content.presets = await api("/admin/api/v1/presets");
      content.mods = await api("/admin/api/v1/mods").catch(() => []);
    } catch (err) {
      $("content-status").textContent = "Content library unavailable";
      $("presets-status").textContent = "Could not load presets: " + (err.message || String(err));
      $("preset-list").replaceChildren();
      $("preset-preview").innerHTML = '<p class="muted">Reload the page or check the admin API connection.</p>';
      return;
    }
    content.files = lib.files || [];
    content.enabled = !!lib.enabled;
    $("content-status").textContent = !lib.enabled
      ? "Library disabled: start the server with --data-dir (or --content-dir)"
      : content.files.length + " file(s) · max upload " + fmtBytes(lib.max_bytes);
    $("content-file").disabled = !lib.enabled;
    $("content-list").replaceChildren(
      ...content.files.map((f) => {
        const li = document.createElement("li");
        const maps = f.maps && f.maps.length ? f.maps.length + " map(s): " + f.maps.slice(0, 8).join(" ") + (f.maps.length > 8 ? " …" : "") : "no maps";
        li.innerHTML =
          "<span><strong>" + escapeHtml(f.file) + '</strong> <span class="muted">' +
          escapeHtml(f.kind) + " · " + fmtBytes(f.size) + " · " + escapeHtml(String(f.fingerprint).slice(0, 8)) +
          " · " + escapeHtml(maps) + "</span></span>";
        const del = document.createElement("button");
        del.type = "button";
        del.className = "btn";
        del.textContent = "Delete";
        del.addEventListener("click", async () => {
          if (!confirm("Delete " + f.file + "?")) return;
          try {
            await api("/admin/api/v1/content/" + encodeURIComponent(f.file), { method: "DELETE" });
            flash("Deleted " + f.file);
          } catch (e) {
            flash("Delete failed: " + e.message, "error");
          }
          await refreshContent();
        });
        li.appendChild(del);
        return li;
      })
    );
    renderPresets();
  }

  async function savePresets(cfg) {
    try {
      content.presets = await api("/admin/api/v1/presets", { method: "PUT", body: JSON.stringify(cfg) });
      flash("Presets saved");
    } catch (e) {
      flash("Presets not saved: " + e.message, "error");
    }
    await refreshContent();
  }

  async function editPreset(idx, copyFrom = null) {
    const ps = content.presets.presets || [];
    const blank = { id: "", title: "", description: "", game: "doom", iwad: null, content: [], maps: [], default_map: "", mod: null, skill: 0, duke_game_mode: 1, duke_friendly_fire: false };
    const p = idx >= 0
      ? ps[idx]
      : copyFrom
        ? { ...copyFrom, id: (copyFrom.id + "_copy").slice(0, 32), title: (copyFrom.title || copyFrom.id) + " (copy)" }
        : blank;
    const baseFiles = (game) => content.files.filter((f) => f.kind === (game === "duke3d" ? "duke" : "iwad"));
    const others = content.files.filter((f) => f.kind !== "iwad" && f.kind !== "duke");
    const opt = (v, label, sel) => '<option value="' + escapeHtml(v) + '"' + (sel ? " selected" : "") + ">" + escapeHtml(label) + "</option>";
    const orderedOthers = [
      ...(p.content || []).map((file) => others.find((entry) => entry.file === file)).filter(Boolean),
      ...others.filter((entry) => !(p.content || []).includes(entry.file)),
    ];
    const fileRows = orderedOthers
      .map((f) => {
        const pos = (p.content || []).indexOf(f.file);
        const initialOrder = pos >= 0 ? pos + 1 : orderedOthers.indexOf(f) + 1;
        return '<div class="preset-file-row" data-file="' + escapeHtml(f.file) + '">' +
          '<label class="preset-file-choice"><input type="checkbox" name="content:' + escapeHtml(f.file) + '"' +
          (pos >= 0 ? " checked" : "") + '/><span><strong>' + escapeHtml(f.file) + '</strong><small>' + fmtBytes(f.size) + '</small></span></label>' +
          '<input type="hidden" name="order:' + escapeHtml(f.file) + '" value="' + initialOrder + '"/>' +
          '<span class="preset-position">' + (pos >= 0 ? "Load " + initialOrder : "") + '</span>' +
          '<div class="preset-order-controls"><button type="button" class="btn small" data-move="up" title="Move earlier">↑</button>' +
          '<button type="button" class="btn small" data-move="down" title="Move later">↓</button></div></div>';
      })
      .join("");
    const getMapSources = () => {
      const sources = new Map();
      for (const f of content.files) for (const m of f.maps || []) {
        if (!sources.has(m)) sources.set(m, []);
        sources.get(m).push(f.file);
      }
      return sources;
    };
    const mapRowsHtml = (selected) => [...getMapSources()]
      .map(([m, sources]) => '<label class="preset-map-option" data-map="' + escapeHtml(m.toLowerCase()) + '" data-sources="' + escapeHtml(JSON.stringify(sources)) + '"><input type="checkbox" name="map:' + escapeHtml(m) + '"' +
        (selected.has(m) ? " checked" : "") + '/><span>' + escapeHtml(m) + "</span></label>")
      .join("");
    const allMaps = [...getMapSources().keys()];
    const mapRows = mapRowsHtml(new Set(p.maps || []));
    const extra = (p.maps || []).filter((m) => !allMaps.includes(m)).join(" ");
    const data = await openModal({
      title: idx >= 0 ? "Edit preset" : copyFrom ? "Duplicate preset" : "New preset",
      confirmLabel: "Save preset",
      bodyHtml:
        '<div class="preset-editor"><div class="preset-col">' +
        '<section class="preset-section"><h3>1 · Name</h3><p class="preset-help">What players see in <em>Host game</em>.</p>' +
        '<div class="preset-grid">' +
        '<label class="field"><span>Display name</span><input name="title" maxlength="80" value="' + escapeHtml(p.title) + '" placeholder="Brutal Doom co-op"/></label>' +
        '<label class="field"><span>Preset ID</span><input name="id" required maxlength="32" pattern="[A-Za-z0-9_-]+" value="' + escapeHtml(p.id) + '" placeholder="brutal_doom"/></label>' +
        '<label class="field"><span>Game</span><select name="game">' + opt("doom", "Doom", (p.game || "doom") === "doom") + opt("duke3d", "Duke Nukem 3D", p.game === "duke3d") + "</select></label>" +
        '<label class="field preset-span"><span>Player description</span><input name="description" maxlength="300" value="' + escapeHtml(p.description || "") + '" placeholder="One sentence shown before creating a room"/></label></div></section>' +
        '<section class="preset-section"><h3>2 · Game files</h3><p class="preset-help" id="preset-files-help">Doom presets use an IWAD and optional WAD / PK3 add-ons.</p>' +
        '<label class="field"><span id="preset-base-label">Base game IWAD</span><select name="iwad" id="preset-iwad">' + opt("", "Server default game", !p.iwad) +
        baseFiles(p.game || "doom").map((f) => opt(f.file, f.file, p.iwad === f.file)).join("") + "</select></label>" +
        '<label class="field"><span>Add a WAD / GRP from your computer</span><input id="preset-upload" type="file" accept=".wad,.grp,.pk3,.pk7,.zip,.ipk3,.ipk7"/><small class="muted">Uploads to this server’s content library, then adds the file to these choices.</small></label>' +
        '<div class="preset-file-list" id="preset-content-files">' + (fileRows || '<p class="muted">Upload game files to the content library first.</p>') + "</div></section>" +
        '<section class="preset-section" id="preset-plan-section"><h3>Load order, file by file</h3><div id="preset-plan"><p class="muted">Reading the files…</p></div></section>' +
        '</div><div class="preset-col">' +
        '<section class="preset-section"><h3>3 · Maps</h3><p class="preset-help" id="preset-map-help">Map choices follow the selected base game and checked add-ons. Add map names if they are not detected in the library.</p>' +
        '<label class="inline-check"><input type="checkbox" name="any_map" id="preset-any-map"' + (!(p.maps || []).length ? " checked" : "") + '/> Allow any map</label>' +
        '<div class="preset-map-tools"><input id="preset-map-filter" type="search" placeholder="Find a map…"/><span class="muted">' + allMaps.length + ' detected</span></div>' +
        '<div class="preset-map-list" id="preset-map-list">' + (mapRows || '<p class="muted">No maps detected. You can still type a map name below.</p>') + "</div>" +
        '<label class="field"><span>Additional map names</span><input name="extra_maps" value="' + escapeHtml(extra) + '" placeholder="E1M1 MAP07"/></label>' +
        '<label class="field"><span>Default map</span><input name="default_map" value="' + escapeHtml(p.default_map || "") + '" placeholder="first selected map"/></label></section>' +
        '<section class="preset-section"><h3>4 · Defaults</h3><div class="preset-grid">' +
        '<label class="field"><span>Gameplay mod</span><select name="mod">' + opt("", "No gameplay mod", !p.mod) +
        content.mods.filter((m) => m.allowed).map((m) => opt(m.name, m.title || m.name, p.mod === m.name)).join("") + "</select></label>" +
        '<label class="field"><span>Skill</span><select name="skill" id="preset-skill"></select></label>' +
        '<label class="field" id="preset-doom-mode-field"><span>Game mode</span><select name="doom_mode">' +
        opt("", "Player's choice", !p.doom_mode) + opt("coop", "Cooperative", p.doom_mode === "coop") + opt("deathmatch", "Deathmatch", p.doom_mode === "deathmatch") +
        "</select></label>" +
        '<label class="field" id="preset-duke-mode-field"><span>Game mode</span><select name="duke_game_mode">' +
        opt("1", "Cooperative", (p.duke_game_mode ?? 1) === 1) + opt("0", "DukeMatch", p.duke_game_mode === 0) + opt("2", "DukeMatch (no spawn)", p.duke_game_mode === 2) +
        '</select></label><label class="inline-check" id="preset-duke-ff-field"><input name="duke_friendly_fire" type="checkbox"' + (p.duke_friendly_fire ? " checked" : "") + '/> Friendly fire</label></div></section>' +
        "</div></div>",
      onReady: (body) => {
        const modal = $("modal");
        modal.classList.add("modal-wide");
        modal.addEventListener("close", () => modal.classList.remove("modal-wide"), { once: true });
        const gameSelect = body.querySelector('select[name="game"]');
        const iwadSelect = body.querySelector("#preset-iwad");
        const fileList = body.querySelector("#preset-content-files");
        const skillSelect = body.querySelector("#preset-skill");
        const fillSkills = () => {
          const keep = Number(skillSelect.value || p.skill || 0);
          const names = PRESET_SKILLS[gameSelect.value === "duke3d" ? "duke3d" : "doom"];
          skillSelect.replaceChildren(new Option("Player's choice", "0"));
          names.forEach((name, i) => { if (name) skillSelect.add(new Option(name, String(i + 1))); });
          skillSelect.value = [...skillSelect.options].some((o) => Number(o.value) === keep) ? String(keep) : "0";
        };
        const updateGameFields = () => {
          fillSkills();
          const duke = gameSelect.value === "duke3d";
          const selectedBase = iwadSelect.value;
          const bases = baseFiles(gameSelect.value);
          iwadSelect.replaceChildren(new Option(duke ? "Server default (duke3d.grp)" : "Server default IWAD", ""));
          for (const f of bases) iwadSelect.add(new Option(f.file, f.file));
          if (bases.some((f) => f.file === selectedBase)) iwadSelect.value = selectedBase;
          else if (p.iwad && bases.some((f) => f.file === p.iwad)) iwadSelect.value = p.iwad;
          body.querySelector("#preset-base-label").textContent = duke ? "Duke base GRP" : "Base game IWAD";
          body.querySelector("#preset-files-help").textContent = duke
            ? "Duke multiplayer presets use a Duke GRP. Maps are detected from that GRP; add map names below if needed."
            : "Doom presets use an IWAD and optional WAD / PK3 add-ons.";
          body.querySelector("#preset-content-files").classList.toggle("hidden", duke);
          body.querySelector("#preset-duke-mode-field").classList.toggle("hidden", !duke);
          body.querySelector("#preset-doom-mode-field").classList.toggle("hidden", duke);
          body.querySelector("#preset-duke-ff-field").classList.toggle("hidden", !duke);
          body.querySelector('select[name="mod"]').closest("label").classList.toggle("hidden", duke);
          body.querySelector("#preset-map-help").textContent = duke
            ? "Duke map choices follow the selected GRP. Doom add-ons and gameplay mods do not apply."
            : "Map choices follow the selected Doom IWAD and checked WAD / PK3 add-ons. Gameplay mods apply to the Doom base game.";
        };
        gameSelect.addEventListener("change", updateGameFields);
        updateGameFields();
        const mapList = body.querySelector("#preset-map-list");
        const mapFilter = body.querySelector("#preset-map-filter");
        const anyMap = body.querySelector("#preset-any-map");
        const selectedMapNames = () => new Set([...mapList.querySelectorAll('input[name^="map:"]:checked')].map((input) => input.name.slice(4)));
        const syncMapChoices = () => {
          const compatible = new Set(iwadSelect.value ? [iwadSelect.value] : []);
          if (gameSelect.value === "doom") {
            for (const row of fileList.querySelectorAll(".preset-file-row")) {
              if (!row.classList.contains("hidden") && row.querySelector('input[name^="content:"]').checked) {
                compatible.add(row.dataset.file);
              }
            }
          }
          let available = 0;
          for (const row of mapList.querySelectorAll(".preset-map-option")) {
            let sources = [];
            try { sources = JSON.parse(row.dataset.sources || "[]"); } catch (_) { sources = []; }
            const shown = sources.some((file) => compatible.has(file));
            row.dataset.available = shown ? "1" : "0";
            const filter = mapFilter.value.trim().toLowerCase();
            row.classList.toggle("hidden", !shown || !row.dataset.map.includes(filter));
            const input = row.querySelector("input");
            if (!shown) input.checked = false;
            input.disabled = anyMap.checked || !shown;
            if (shown) available += 1;
          }
          mapList.classList.toggle("disabled", anyMap.checked);
          body.querySelector("#preset-map-filter").parentElement.querySelector(".muted").textContent = `${available} available`;
          const defaultInput = body.querySelector('input[name="default_map"]');
          const base = content.files.find((file) => file.file === iwadSelect.value);
          const availableNames = [...mapList.querySelectorAll('input[name^="map:"]')].filter((input) => !input.disabled).map((input) => input.name.slice(4));
          const currentDefault = defaultInput.value.trim().toUpperCase();
          if (!currentDefault || (!anyMap.checked && !availableNames.includes(currentDefault))) {
            defaultInput.value = availableNames[0] || base?.maps?.[0] || (gameSelect.value === "duke3d" ? "E1L1" : "E1M1");
          }
        };
        let planSeq = 0;
        let planTimer = 0;
        function schedulePlan() {
          clearTimeout(planTimer);
          planTimer = setTimeout(async () => {
            const section = body.querySelector("#preset-plan-section");
            const duke = gameSelect.value === "duke3d";
            section.classList.toggle("hidden", duke);
            if (duke) return;
            const files = [...fileList.querySelectorAll(".preset-file-row")]
              .filter((row) => row.querySelector('input[name^="content:"]').checked)
              .map((row) => row.dataset.file);
            const seq = ++planSeq;
            const target = body.querySelector("#preset-plan");
            try {
              const plan = await fetchPlan(iwadSelect.value, files, body.querySelector('select[name="mod"]').value);
              if (seq === planSeq) target.innerHTML = renderLoadPlan(plan);
            } catch (err) {
              if (seq === planSeq) target.innerHTML = '<p class="muted">Could not read the files: ' + escapeHtml(err.message || String(err)) + "</p>";
            }
          }, 200);
        }
        gameSelect.addEventListener("change", schedulePlan);
        iwadSelect.addEventListener("change", schedulePlan);
        body.querySelector('select[name="mod"]').addEventListener("change", schedulePlan);
        const updateOrder = () => {
          schedulePlan();
          const rows = [...fileList.querySelectorAll(".preset-file-row")];
          for (const [index, row] of rows.entries()) {
            row.querySelector('input[name^="order:"]').value = String(index + 1);
            const selected = row.querySelector('input[name^="content:"]').checked;
            row.classList.toggle("selected", selected);
            row.querySelector(".preset-position").textContent = selected ? `Load ${index + 1}` : "";
            row.querySelector('[data-move="up"]').disabled = index === 0;
            row.querySelector('[data-move="down"]').disabled = index === rows.length - 1;
          }
        };
        fileList.addEventListener("click", (event) => {
          const button = event.target.closest("button[data-move]");
          if (!button) return;
          const row = button.closest(".preset-file-row");
          if (button.dataset.move === "up" && row.previousElementSibling) fileList.insertBefore(row, row.previousElementSibling);
          if (button.dataset.move === "down" && row.nextElementSibling) fileList.insertBefore(row.nextElementSibling, row);
          updateOrder();
        });
        fileList.addEventListener("change", (event) => {
          if (event.target.matches('input[name^="content:"]')) {
            updateOrder();
            syncMapChoices();
          }
        });
        updateOrder();

        mapFilter.addEventListener("input", () => {
          const filter = mapFilter.value.trim().toLowerCase();
          for (const row of mapList.querySelectorAll(".preset-map-option")) {
            row.classList.toggle("hidden", row.dataset.available !== "1" || !row.dataset.map.includes(filter));
          }
        });
        body.querySelector("#preset-upload").addEventListener("change", async (event) => {
          const files = [...event.target.files];
          event.target.value = "";
          for (const file of files) {
            const name = file.name.replace(/[^A-Za-z0-9._-]/g, "_");
            try {
              const entry = await api("/admin/api/v1/content/" + encodeURIComponent(name), {
                method: "POST",
                headers: { "Content-Type": "application/octet-stream" },
                body: file,
              });
              const selectedMaps = selectedMapNames();
              await refreshContent();
              const baseMatches = (gameSelect.value === "duke3d" && entry.kind === "duke") || (gameSelect.value === "doom" && entry.kind === "iwad");
              updateGameFields();
              if (baseMatches) {
                iwadSelect.value = entry.file;
                flash(`Uploaded and selected ${entry.file} as the base game`);
              } else if (gameSelect.value === "doom" && ["pwad", "archive"].includes(entry.kind)) {
                if (!fileList.querySelectorAll(".preset-file-row").length) fileList.replaceChildren();
                const exists = [...fileList.querySelectorAll(".preset-file-row")].some((row) => row.dataset.file === entry.file);
                if (!exists) {
                  const order = fileList.querySelectorAll(".preset-file-row").length + 1;
                  fileList.insertAdjacentHTML("beforeend", '<div class="preset-file-row" data-file="' + escapeHtml(entry.file) + '">' +
                    '<label class="preset-file-choice"><input type="checkbox" name="content:' + escapeHtml(entry.file) + '" checked/><span><strong>' + escapeHtml(entry.file) + '</strong><small>' + fmtBytes(entry.size) + '</small></span></label>' +
                    '<input type="hidden" name="order:' + escapeHtml(entry.file) + '" value="' + order + '"/><span class="preset-position"></span>' +
                    '<div class="preset-order-controls"><button type="button" class="btn small" data-move="up" title="Move earlier">↑</button><button type="button" class="btn small" data-move="down" title="Move later">↓</button></div></div>');
                }
                flash(`Uploaded ${entry.file} as a Doom add-on`);
              } else {
                flash(`${entry.file} uploaded. Choose Doom for WAD add-ons or Duke for a GRP base.`, "error");
              }
              mapList.innerHTML = mapRowsHtml(selectedMaps);
              updateOrder();
              syncMapChoice();
            } catch (err) {
              flash(`Upload of ${name} failed: ${err.message}`, "error");
            }
          }
        });
        const syncMapChoice = () => {
          syncMapChoices();
          mapList.classList.toggle("disabled", anyMap.checked);
          body.querySelector('input[name="extra_maps"]').disabled = anyMap.checked;
        };
        gameSelect.addEventListener("change", syncMapChoice);
        iwadSelect.addEventListener("change", syncMapChoice);
        anyMap.addEventListener("change", syncMapChoice);
        syncMapChoice();

        const form = body.closest("form");
        const validate = (event) => {
          if (event.submitter?.value !== "confirm") return;
          const id = body.querySelector('input[name="id"]');
          if (!/^[A-Za-z0-9_-]{1,32}$/.test(id.value.trim())) {
            event.preventDefault();
            event.stopImmediatePropagation();
            flash("Preset ID: letters, digits, _ and - only", "error");
            id.focus();
            return;
          }
          if (idx < 0 && ps.some((x) => x.id === id.value.trim())) {
            event.preventDefault();
            event.stopImmediatePropagation();
            flash("A preset with this ID already exists", "error");
            id.focus();
            return;
          }
          if (anyMap.checked) return;
          const chosen = [...mapList.querySelectorAll('input[name^="map:"]:checked')].map((input) => input.name.slice(4));
          const extraMaps = body.querySelector('input[name="extra_maps"]').value.split(/[\s,]+/).filter(Boolean).map((name) => name.toUpperCase());
          const allowed = [...new Set([...chosen, ...extraMaps])];
          if (!allowed.length) {
            event.preventDefault();
            event.stopImmediatePropagation();
            flash("Select at least one map or allow any map", "error");
            return;
          }
          const defaultMap = body.querySelector('input[name="default_map"]');
          if (!allowed.includes(defaultMap.value.trim().toUpperCase())) defaultMap.value = allowed[0];
        };
        // The modal form is shared: drop this check when the editor closes.
        form.addEventListener("submit", validate, true);
        modal.addEventListener("close", () => form.removeEventListener("submit", validate, true), { once: true });
      },
    });
    if (!data) return;
    const picked = [];
    const maps = [];
    for (const [k, v] of Object.entries(data)) {
      if (k.startsWith("content:") && v) picked.push({ file: k.slice(8), order: Number(data["order:" + k.slice(8)]) || 50 });
      if (!data.any_map && k.startsWith("map:") && v) maps.push(k.slice(4));
    }
    picked.sort((a, b) => a.order - b.order);
    if (!data.any_map) for (const m of String(data.extra_maps || "").split(/[\s,]+/)) if (m) maps.push(m.toUpperCase());
    const selectedBase = content.files.find((f) => f.file === data.iwad);
    const defaultMap = String(data.default_map || "").trim().toUpperCase() || maps[0] || selectedBase?.maps?.[0] || (data.game === "duke3d" ? "E1L1" : "E1M1");
    if (!data.any_map && maps.length && !maps.includes(defaultMap)) {
      flash("Choose a default map that is in the allowed map list", "error");
      return;
    }
    const next = {
      id: String(data.id || "").trim(),
      title: data.title,
      description: data.description,
      game: data.game || "doom",
      iwad: data.iwad || null,
      content: data.game === "duke3d" ? [] : picked.map((x) => x.file),
      maps,
      default_map: defaultMap,
      mod: data.mod || null,
      skill: Number(data.skill) || 0,
      duke_game_mode: Number(data.duke_game_mode ?? 1),
      duke_friendly_fire: data.duke_friendly_fire === true,
      doom_mode: data.game === "duke3d" ? null : data.doom_mode || null,
    };
    const list = ps.slice();
    if (idx >= 0) list[idx] = next;
    else list.push(next);
    state.selectedPresetId = next.id;
    await savePresets({ ...content.presets, presets: list });
  }

  $("preset-add").addEventListener("click", () => editPreset(-1));
  $("preset-require").addEventListener("change", (ev) =>
    savePresets({ ...content.presets, require_preset: ev.target.checked })
  );
  document.querySelector('[data-go-settings="content"]').addEventListener("click", () => setView("settings"));
  $("content-rescan").addEventListener("click", async () => {
    try {
      await api("/admin/api/v1/content/rescan", { method: "POST" });
    } catch (e) {
      flash("Rescan failed: " + e.message, "error");
    }
    await refreshContent();
  });
  $("content-file").addEventListener("change", async (ev) => {
    const files = [...ev.target.files];
    ev.target.value = "";
    for (const f of files) {
      const name = f.name.replace(/[^A-Za-z0-9._-]/g, "_");
      $("content-status").textContent = "Uploading " + name + " (" + fmtBytes(f.size) + ")…";
      try {
        const e = await api("/admin/api/v1/content/" + encodeURIComponent(name), {
          method: "POST",
          headers: { "Content-Type": "application/octet-stream" },
          body: f,
        });
        flash("Uploaded " + e.file + (e.maps && e.maps.length ? " · " + e.maps.length + " map(s)" : ""));
      } catch (err) {
        flash("Upload of " + name + " failed: " + err.message, "error");
      }
    }
    await refreshContent();
  });

  // Boot
  routeGate()
    .then((ok) => {
      if (ok) return refresh();
      if (state.authMode !== "mfa") {
        return refresh().catch(() => {});
      }
    })
    .catch(() => showGate(""));
  setInterval(() => {
    if ($("shell").classList.contains("hidden")) return;
    refresh().catch(() => {});
  }, 5000);
})();
