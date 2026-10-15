/**
 * WebRTC audio for Room v2 — Lightcall ICE patterns.
 *
 * Signalling goes through the game WebSocket as JSON `rtc` control messages
 * (handled in wasm). ICE servers come from `welcome.media`.
 *
 * Voice chrome is shown as soon as this module loads (do not wait for
 * getUserMedia). Mic is acquired on first PTT / user gesture.
 *
 * Exposed as `window.roomRtc`.
 */
(function () {
  "use strict";

  /** @type {RTCPeerConnection | null} */
  let pc = null;
  /** @type {MediaStream | null} */
  let localStream = null;
  /** Seat 0 is impolite; others polite (perfect negotiation). */
  let polite = true;
  let makingOffer = false;
  let ignoreOffer = false;
  /** @type {(msg: object) => void} */
  let sendSignal = () => {};
  /** @type {object[]} */
  let outbound = [];
  /** Signals that arrived before the PeerConnection was ready. */
  /** @type {object[]} */
  let inboundPending = [];
  /** @type {Record<string, HTMLAudioElement>} */
  let remoteAudio = {};
  let speakerMuted = false;
  let pttKeyDown = false;
  let pttPadDown = false;
  let talking = false;
  let micUnavailable = false;
  let micRequested = false;
  let started = false;
  /** @type {object | null} */
  let pendingMedia = null;
  const PTT_CODE = "KeyV";

  function normalizeIceServers(list) {
    const servers = (list || []).map((s) => {
      let urls = s.urls || s.url || [];
      if (typeof urls === "string") urls = [urls];
      const entry = { urls };
      if (s.username) entry.username = s.username;
      if (s.credential) entry.credential = s.credential;
      return entry;
    });
    const summary = servers
      .map((s) => {
        const u = (Array.isArray(s.urls) ? s.urls[0] : s.urls) || "";
        if (String(u).toLowerCase().startsWith("turn")) return "TURN";
        if (String(u).toLowerCase().startsWith("stun")) return "STUN";
        return "?";
      })
      .join(", ");
    console.log("[roomRtc] ICE servers:", servers.length, summary || "none");
    return servers;
  }

  function iceConfig(servers) {
    return {
      iceServers: normalizeIceServers(servers),
      bundlePolicy: "max-bundle",
    };
  }

  function queueSignal(msg) {
    outbound.push(msg);
    sendSignal(msg);
  }

  function iceState() {
    return pc ? pc.iceConnectionState : "none";
  }

  async function ensurePc(iceServers) {
    if (pc) return;
    pc = new RTCPeerConnection(iceConfig(iceServers));
    pc.onicecandidate = (ev) => {
      if (ev.candidate) {
        queueSignal({
          t: "rtc",
          kind: "ice",
          candidate: ev.candidate.candidate,
          mid: ev.candidate.sdpMid,
        });
      }
    };
    pc.ontrack = (ev) => {
      const mid = ev.transceiver && ev.transceiver.mid;
      const key = mid || String(Object.keys(remoteAudio).length);
      let el = remoteAudio[key];
      if (!el) {
        el = document.createElement("audio");
        el.autoplay = true;
        el.playsInline = true;
        el.style.display = "none";
        document.body.appendChild(el);
        remoteAudio[key] = el;
      }
      el.muted = speakerMuted;
      el.srcObject = ev.streams[0];
      const p = el.play();
      if (p && typeof p.catch === "function") {
        p.catch((err) => console.warn("[roomRtc] audio play blocked:", err));
      }
    };
    pc.oniceconnectionstatechange = () => {
      console.log("[roomRtc] iceConnectionState", pc && pc.iceConnectionState);
      if (typeof window.roomNetGraphNote === "function") {
        window.roomNetGraphNote({ ice: iceState() });
      }
    };
  }

  async function acquireMic(enableVideo) {
    if (localStream || micUnavailable || micRequested) return;
    micRequested = true;
    try {
      const constraints = {
        audio: {
          echoCancellation: true,
          noiseSuppression: true,
          autoGainControl: true,
        },
        video: enableVideo
          ? { width: { ideal: 640 }, height: { ideal: 360 }, frameRate: { ideal: 24 } }
          : false,
      };
      localStream = await navigator.mediaDevices.getUserMedia(constraints);
      for (const track of localStream.getAudioTracks()) {
        track.enabled = talking;
        if (pc) pc.addTrack(track, localStream);
      }
      for (const track of localStream.getVideoTracks()) {
        if (pc) pc.addTrack(track, localStream);
      }
      micUnavailable = false;
      console.log("[roomRtc] mic acquired");
    } catch (e) {
      console.warn("[roomRtc] mic/camera unavailable", e);
      micUnavailable = true;
      markMicUnavailable("Mic unavailable — hear only");
    } finally {
      syncVoiceUi();
    }
  }

  async function start(opts) {
    const { iceServers, onSignal, enableMic, enableVideo } = opts || {};
    sendSignal = typeof onSignal === "function" ? onSignal : () => {};
    if (pc) await stop();
    outbound = [];
    ensureUi();
    bindPtt();
    await ensurePc(iceServers || []);

    // Defer getUserMedia until a gesture so lobby invite / focus stay usable.
    // Receive-only path: add recv transceiver + create offer immediately.
    if (pc.getTransceivers().length === 0) {
      try {
        pc.addTransceiver("audio", { direction: "sendrecv" });
      } catch (_) {
        /* older browsers */
      }
    }

    makingOffer = true;
    try {
      const offer = await pc.createOffer();
      await pc.setLocalDescription(offer);
      queueSignal({ t: "rtc", kind: "offer", sdp: offer.sdp });
    } finally {
      makingOffer = false;
    }
    started = true;
    pendingMedia = {
      enableMic: enableMic !== false,
      enableVideo: !!enableVideo,
      iceServers: iceServers || [],
    };
    syncVoiceUi();
    // Apply offers/ICE that arrived while start() was in flight.
    const pending = inboundPending.splice(0, inboundPending.length);
    for (const m of pending) {
      await handleSignal(m);
    }
    return { ok: true };
  }

  /** Start from welcome.media JSON (called from wasm). */
  async function startFromMedia(media) {
    ensureUi();
    bindPtt();
    if (!media || media.enabled === false) {
      console.log("[roomRtc] media disabled in welcome");
      syncVoiceUi();
      return { ok: false };
    }
    if (typeof media.polite === "boolean") {
      polite = media.polite;
    } else if (typeof media.seat === "number") {
      // Host (seat 0) wins glare; joiners are polite.
      polite = media.seat !== 0;
    }
    console.log("[roomRtc] polite=", polite, "seat=", media.seat);
    const iceServers = media.ice_servers || media.iceServers || [];
    return start({
      iceServers,
      onSignal: () => {},
      enableMic: true,
      enableVideo: !!(media.enable_video || media.enableVideo),
    });
  }

  function sdpLooksValid(sdp) {
    if (!sdp || typeof sdp !== "string") return false;
    // Reject the old room-sfu stub and any SDP missing the required timing line.
    if (sdp.indexOf("room-sfu-stub") >= 0 || sdp.indexOf("# offer-len=") >= 0) {
      return false;
    }
    return /(^|\r?\n)t=/m.test(sdp);
  }

  async function ensureMicForTalk() {
    if (!pendingMedia || !pendingMedia.enableMic) return;
    await acquireMic(pendingMedia.enableVideo);
    // Renegotiate after adding tracks so the remote gets our send.
    if (pc && localStream && pc.signalingState === "stable") {
      makingOffer = true;
      try {
        const offer = await pc.createOffer();
        await pc.setLocalDescription(offer);
        queueSignal({ t: "rtc", kind: "offer", sdp: offer.sdp });
      } catch (e) {
        console.warn("[roomRtc] renegotiate after mic", e);
      } finally {
        makingOffer = false;
      }
    }
  }

  async function handleSignal(msg) {
    if (!msg || !msg.kind) return;
    if (!pc) {
      if (inboundPending.length < 32) inboundPending.push(msg);
      return;
    }
    try {
      if (msg.kind === "offer" && msg.sdp) {
        if (!sdpLooksValid(msg.sdp)) {
          console.warn(
            "[roomRtc] ignoring invalid offer sdp len=",
            msg.sdp.length,
            msg.sdp.slice(0, 80)
          );
          return;
        }
        const offerCollision = makingOffer || pc.signalingState !== "stable";
        ignoreOffer = !polite && offerCollision;
        if (ignoreOffer) {
          console.log("[roomRtc] glare: ignoring offer (impolite)");
          return;
        }
        await pc.setRemoteDescription({ type: "offer", sdp: msg.sdp });
        const answer = await pc.createAnswer();
        await pc.setLocalDescription(answer);
        queueSignal({ t: "rtc", kind: "answer", sdp: answer.sdp });
      } else if (msg.kind === "answer" && msg.sdp) {
        if (!sdpLooksValid(msg.sdp)) {
          console.warn(
            "[roomRtc] ignoring invalid answer sdp len=",
            msg.sdp.length,
            msg.sdp.slice(0, 80)
          );
          return;
        }
        await pc.setRemoteDescription({ type: "answer", sdp: msg.sdp });
      } else if (msg.kind === "ice" && msg.candidate) {
        try {
          await pc.addIceCandidate({
            candidate: msg.candidate,
            sdpMid: msg.mid || undefined,
          });
        } catch (e) {
          if (!ignoreOffer) console.warn("[roomRtc] ice", e);
        }
      }
    } catch (e) {
      console.warn(
        "[roomRtc] signal",
        e,
        "kind=",
        msg.kind,
        "sdp-len=",
        msg.sdp ? msg.sdp.length : 0
      );
    }
  }

  function takeOutboundSignals() {
    const out = outbound;
    outbound = [];
    return out;
  }

  function setTalking(on) {
    talking = !!on;
    if (localStream) {
      for (const t of localStream.getAudioTracks()) t.enabled = talking;
    }
    syncVoiceUi();
  }

  async function stop() {
    started = false;
    pendingMedia = null;
    micRequested = false;
    if (localStream) {
      for (const t of localStream.getTracks()) t.stop();
      localStream = null;
    }
    if (pc) {
      pc.close();
      pc = null;
    }
    for (const el of Object.values(remoteAudio)) el.remove();
    remoteAudio = {};
    talking = false;
    outbound = [];
    inboundPending = [];
  }

  function supported() {
    return typeof RTCPeerConnection !== "undefined";
  }

  function focusGameCanvas() {
    const canvas = document.getElementById("room");
    if (canvas && typeof canvas.focus === "function") {
      try {
        canvas.focus({ preventScroll: true });
      } catch (_) {
        canvas.focus();
      }
    }
  }

  function markMicUnavailable(hint) {
    micUnavailable = true;
    const el = document.getElementById("voiceHint");
    if (el) el.textContent = hint || "Mic unavailable";
    syncVoiceUi();
  }

  function syncVoiceUi() {
    const bar = document.getElementById("voiceBar");
    const pad = document.getElementById("voicePtt");
    const spk = document.getElementById("voiceSpeaker");
    if (!bar) return;
    bar.hidden = false;
    bar.classList.toggle("is-talking", talking);
    bar.classList.toggle("no-mic", micUnavailable);
    if (pad) {
      pad.classList.toggle("active", talking);
      pad.classList.toggle("disabled", micUnavailable);
      pad.textContent = micUnavailable
        ? "No mic"
        : talking
          ? "Talking…"
          : "Hold V · talk";
    }
    if (spk) {
      spk.classList.toggle("is-off", speakerMuted);
      spk.textContent = speakerMuted ? "Speaker off" : "Speaker on";
    }
    const hint = document.getElementById("voiceHint");
    if (hint && !micUnavailable) {
      hint.textContent = talking
        ? "Transmit (WebRTC)"
        : localStream
          ? "Hold V to talk (WebRTC)"
          : "Hold V to enable mic";
    }
  }

  function bindNoFocus(el, onDown, onUp) {
    const down = (ev) => {
      ev.preventDefault();
      ev.stopPropagation();
      onDown(ev);
    };
    const up = (ev) => {
      ev.preventDefault();
      ev.stopPropagation();
      onUp(ev);
    };
    el.addEventListener("pointerdown", down);
    el.addEventListener("pointerup", up);
    el.addEventListener("pointerleave", up);
    el.addEventListener("pointercancel", up);
    el.addEventListener("mousedown", (ev) => ev.preventDefault());
  }

  function ensureUi() {
    if (document.getElementById("voiceBar")) {
      document.getElementById("voiceBar").hidden = false;
      return;
    }
    const bar = document.createElement("div");
    bar.id = "voiceBar";
    bar.setAttribute("role", "group");
    bar.setAttribute("aria-label", "Coop voice");
    bar.innerHTML =
      '<div id="voicePtt" class="voice-ptt" tabindex="-1">Hold V · talk</div>' +
      '<div id="voiceSpeaker" class="voice-spk" tabindex="-1">Speaker on</div>' +
      '<span id="voiceHint">Hold V to talk (WebRTC)</span>';
    document.body.appendChild(bar);
    const pad = document.getElementById("voicePtt");
    bindNoFocus(
      pad,
      () => {
        if (micUnavailable) {
          focusGameCanvas();
          return;
        }
        pttPadDown = true;
        void ensureMicForTalk().then(() => {
          setTalking(pttKeyDown || pttPadDown);
          focusGameCanvas();
        });
      },
      () => {
        pttPadDown = false;
        setTalking(pttKeyDown || pttPadDown);
        focusGameCanvas();
      },
    );
    const spk = document.getElementById("voiceSpeaker");
    bindNoFocus(
      spk,
      () => {
        speakerMuted = !speakerMuted;
        for (const el of Object.values(remoteAudio)) el.muted = speakerMuted;
        syncVoiceUi();
        focusGameCanvas();
      },
      () => {},
    );
    syncVoiceUi();
  }

  let pttBound = false;
  function bindPtt() {
    if (pttBound) return;
    pttBound = true;
    window.addEventListener(
      "keydown",
      (ev) => {
        if (ev.code !== PTT_CODE || ev.repeat) return;
        const t = ev.target;
        if (t && (t.tagName === "INPUT" || t.tagName === "TEXTAREA" || t.isContentEditable)) {
          return;
        }
        if (micUnavailable) return;
        pttKeyDown = true;
        void ensureMicForTalk().then(() => setTalking(true));
      },
      true,
    );
    window.addEventListener(
      "keyup",
      (ev) => {
        if (ev.code !== PTT_CODE) return;
        pttKeyDown = false;
        setTalking(pttPadDown);
      },
      true,
    );
    window.addEventListener("blur", () => {
      pttKeyDown = false;
      pttPadDown = false;
      setTalking(false);
    });
  }

  // Show chrome immediately when the module loads (coop page).
  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", () => {
      ensureUi();
      bindPtt();
    });
  } else {
    ensureUi();
    bindPtt();
  }

  window.roomRtc = {
    start,
    startFromMedia,
    handleSignal,
    takeOutboundSignals,
    setTalking,
    stop,
    supported,
    isStarted: () => started,
    iceState,
    ensureUi,
  };
})();
