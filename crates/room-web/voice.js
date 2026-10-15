/**
 * Live voice for Room web coop — capture/playback adapted from Lightcall
 * (AudioWorklet PCM over the game WebSocket as MSG_VOICE).
 *
 * Push-to-talk: hold **V** (or the on-screen pad). UI never takes keyboard
 * focus so WASD / pointer-lock keep working.
 *
 * Wire payload (Lightcall PCM layout, no E2E):
 *   [0]=1 PCM  [1..5]=seq u32le  [5..9]=rate u32le  [9]=channels  [10..]=i16 LE
 */

const AUDIO_FRAME_SIZE = 960;
const WIRE_SAMPLE_RATE = 48000;
const TYPE_PCM = 1;
/** KeyboardEvent.code for push-to-talk. */
const PTT_CODE = "KeyV";

const AUDIO_WORKLET_CODE = `
class CaptureProcessor extends AudioWorkletProcessor {
  constructor(options) {
    super();
    const p = (options && options.processorOptions) || {};
    this.gateOn = false;
    this.gate = 1;
    this.gateThresh = p.gateThresh || 0.008;
    this.gateAttack = p.gateAttack || 0.15;
    this.gateDecay = p.gateDecay || 0.01;
    this.port.onmessage = (e) => {
      if (e.data && e.data.noiseGate !== undefined) this.gateOn = e.data.noiseGate;
    };
  }
  process(inputs, outputs) {
    const input = inputs[0]?.[0];
    const output = outputs[0]?.[0];
    if (!input || input.length === 0) return true;
    if (!this.gateOn) {
      if (output && output.length >= input.length) output.set(input);
      const copy = new Float32Array(input);
      this.port.postMessage(copy, [copy.buffer]);
      return true;
    }
    let sum = 0;
    for (let i = 0; i < input.length; i++) sum += input[i] * input[i];
    const rms = Math.sqrt(sum / input.length);
    if (rms > this.gateThresh) this.gate = Math.min(1, this.gate + this.gateAttack);
    else this.gate = Math.max(0, this.gate - this.gateDecay);
    const out = new Float32Array(input.length);
    for (let i = 0; i < input.length; i++) out[i] = input[i] * this.gate;
    if (output && output.length >= input.length) output.set(out);
    this.port.postMessage(out, [out.buffer]);
    return true;
  }
}
registerProcessor('capture-processor', CaptureProcessor);

class PlaybackProcessor extends AudioWorkletProcessor {
  constructor(options) {
    super();
    const p = (options && options.processorOptions) || {};
    const sr = sampleRate || 48000;
    const bufSec = p.bufferSec || 0.3;
    const prebufSec = p.prebufSec || 0.04;
    this.cap = Math.round(sr * bufSec);
    this.buf = new Float32Array(this.cap);
    this.rd = 0;
    this.rdFloat = 0;
    this.wr = 0;
    this.cnt = 0;
    this.playing = false;
    this.prebuf = Math.round(sr * prebufSec);
    this.hiWM = (this.cap * (p.hiWM || 0.88)) | 0;
    this.loWM = (this.cap * (p.loWM || 0.72)) | 0;
    this.targetWM = (this.hiWM + this.loWM) >> 1;
    this.slewRate = 0.00015;
    this.silenceFrames = 0;
    this.silenceMax = p.silenceMax || 10;
    this.fadeLen = Math.min(64, Math.round(sr * 0.001));
    this.lastSample = 0;
    this.comfortNoise = 0.0003;
    this.port.onmessage = (e) => {
      const s = e.data;
      const n = s.length;
      this.reanchorCnt();
      const overflow = (this.cnt + n) - this.cap;
      if (overflow > 0) {
        this.rd = (this.rd + overflow) % this.cap;
        this.rdFloat = (this.rdFloat + overflow) % this.cap;
        if (this.rdFloat < 0) this.rdFloat += this.cap;
      }
      const first = Math.min(n, this.cap - this.wr);
      this.buf.set(s.subarray(0, first), this.wr);
      if (first < n) this.buf.set(s.subarray(first), 0);
      this.wr = (this.wr + n) % this.cap;
      this.reanchorCnt();
      this.silenceFrames = 0;
    };
  }
  fillComfort(out, start, len) {
    const amp = this.comfortNoise;
    let last = this.lastSample;
    for (let i = start; i < start + len; i++) {
      last = last * 0.98 + (Math.random() - 0.5) * amp;
      out[i] = last;
    }
    this.lastSample = last;
  }
  reanchorCnt() {
    this.cnt = (this.wr - this.rd + this.cap) % this.cap;
  }
  readBufSlew(out, offset, count, speed) {
    const cap = this.cap;
    for (let i = 0; i < count; i++) {
      const idx = this.rdFloat % cap;
      const idx0 = (idx | 0) % cap;
      const idx1 = (idx0 + 1) % cap;
      const frac = idx - (idx | 0);
      out[offset + i] = this.buf[idx0] * (1 - frac) + this.buf[idx1] * frac;
      this.rdFloat += speed;
      if (this.rdFloat >= cap) this.rdFloat -= cap;
      if (this.rdFloat < 0) this.rdFloat += cap;
    }
    this.rd = this.rdFloat | 0;
    this.rdFloat = this.rdFloat % cap;
    if (this.rdFloat < 0) this.rdFloat += cap;
  }
  process(inputs, outputs) {
    const out = outputs[0]?.[0];
    if (!out) return true;
    this.reanchorCnt();
    const n = out.length;
    if (!this.playing) {
      if (this.cnt >= this.prebuf) {
        this.playing = true;
        this.rdFloat = this.rd;
      } else {
        this.fillComfort(out, 0, n);
        return true;
      }
    }
    if (this.cnt >= n) {
      let speed = 1.0;
      if (this.cnt > this.targetWM) {
        const excess = Math.min(this.cnt - this.targetWM, this.hiWM - this.targetWM);
        speed = 1.0 + this.slewRate * excess;
      }
      this.readBufSlew(out, 0, n, speed);
      this.lastSample = out[n - 1];
      this.silenceFrames = 0;
    } else if (this.cnt > 0) {
      const avail = this.cnt;
      this.readBufSlew(out, 0, avail, 1.0);
      this.lastSample = out[avail - 1];
      const fl = Math.min(this.fadeLen, n - avail);
      for (let i = 0; i < fl; i++) {
        const t = 1 - i / fl;
        out[avail - fl + i] *= t;
      }
      this.fillComfort(out, avail, n - avail);
      this.silenceFrames++;
      if (this.silenceFrames > this.silenceMax) this.playing = false;
    } else {
      this.fillComfort(out, 0, n);
      this.silenceFrames++;
      if (this.silenceFrames > this.silenceMax) this.playing = false;
    }
    return true;
  }
}
registerProcessor('playback-processor', PlaybackProcessor);
`;

function detectDeviceType() {
  const ua = navigator.userAgent || "";
  const mobile = /Android|iPhone|iPod/i.test(ua);
  const tablet =
    /iPad/i.test(ua) ||
    (navigator.platform === "MacIntel" && navigator.maxTouchPoints > 1) ||
    (/Android/i.test(ua) && !/Mobile/i.test(ua));
  if (tablet) return "tablet";
  if (mobile) return "phone";
  return "desktop";
}

const AUDIO_PROFILES = {
  desktop: { bufferSec: 0.3, prebufSec: 0.04, hiWM: 0.88, loWM: 0.72, gateThresh: 0.008, gateAttack: 0.15, gateDecay: 0.01, silenceMax: 10 },
  tablet: { bufferSec: 0.5, prebufSec: 0.08, hiWM: 0.85, loWM: 0.68, gateThresh: 0.006, gateAttack: 0.12, gateDecay: 0.008, silenceMax: 8 },
  phone: { bufferSec: 0.8, prebufSec: 0.15, hiWM: 0.82, loWM: 0.62, gateThresh: 0.005, gateAttack: 0.1, gateDecay: 0.006, silenceMax: 6 },
};

function getAudioProfile() {
  return AUDIO_PROFILES[detectDeviceType()] || AUDIO_PROFILES.desktop;
}

function resampleLinear(samples, fromRate, toRate) {
  if (fromRate === toRate || !fromRate || !toRate) return samples;
  const ratio = fromRate / toRate;
  const outLen = Math.round(samples.length / ratio);
  if (outLen === samples.length) return samples;
  const out = new Float32Array(outLen);
  for (let i = 0; i < outLen; i++) {
    const srcIdx = i * ratio;
    const lo = srcIdx | 0;
    const hi = Math.min(lo + 1, samples.length - 1);
    const frac = srcIdx - lo;
    out[i] = samples[lo] * (1 - frac) + samples[hi] * frac;
  }
  return out;
}

function softLimitInPlace(buf) {
  for (let i = 0; i < buf.length; i++) buf[i] = Math.tanh(buf[i]);
}

/** @type {import('./room_web.js') | null} */
let wasm = null;
let audioCtx = null;
let workletPromise = null;
let micStream = null;
let captureSource = null;
let captureHpf = null;
let captureGain = null;
let captureNode = null;
let audioCaptureBuffer = null;
let audioCaptureOffset = 0;
let audioSeq = 0;
/** True while V / PTT pad is held. */
let talking = false;
let speakerMuted = false;
let pollId = null;
let pttKeyDown = false;
let pttPadDown = false;
let micBusy = false;
/** Permanent for this page load — do not retry getUserMedia (it steals pointer lock / focus). */
let micUnavailable = false;
let micFailLogged = false;
let lastRemoteSpeakAt = 0;
/** @type {Map<number, AudioWorkletNode>} */
const playbackNodes = new Map();

function getAudioCtx() {
  if (!audioCtx) {
    audioCtx = new (window.AudioContext || window.webkitAudioContext)({
      sampleRate: WIRE_SAMPLE_RATE,
      latencyHint: "interactive",
    });
  }
  return audioCtx;
}

function ensureWorklet() {
  if (workletPromise) return workletPromise;
  workletPromise = (async () => {
    getAudioCtx();
    if (audioCtx.state === "suspended") await audioCtx.resume();
    const blob = new Blob([AUDIO_WORKLET_CODE], { type: "application/javascript" });
    const url = URL.createObjectURL(blob);
    await audioCtx.audioWorklet.addModule(url);
    URL.revokeObjectURL(url);
  })().catch((e) => {
    workletPromise = null;
    throw e;
  });
  return workletPromise;
}

function packPcmFrame(int16, seq, sampleRate) {
  const pcmLen = int16.byteLength;
  const payload = new Uint8Array(10 + pcmLen);
  const dv = new DataView(payload.buffer);
  payload[0] = TYPE_PCM;
  dv.setUint32(1, seq, true);
  dv.setUint32(5, sampleRate, true);
  payload[9] = 1;
  payload.set(new Uint8Array(int16.buffer, int16.byteOffset, pcmLen), 10);
  return payload;
}

function flushCaptureBatch(samples) {
  if (!wasm || !talking || !wasm.roomVoiceActive() || samples.length === 0) return;
  const seq = audioSeq++;
  const actualRate = audioCtx ? audioCtx.sampleRate : WIRE_SAMPLE_RATE;
  let out = resampleLinear(samples, actualRate, WIRE_SAMPLE_RATE);
  softLimitInPlace(out);
  const int16 = new Int16Array(out.length);
  for (let i = 0; i < out.length; i++) {
    int16[i] = Math.max(-32768, Math.min(32767, Math.round(out[i] * 32767)));
  }
  wasm.roomVoiceSend(packPcmFrame(int16, seq, WIRE_SAMPLE_RATE));
}

function getPlaybackNode(seat) {
  if (playbackNodes.has(seat)) return playbackNodes.get(seat);
  if (!audioCtx) return null;
  try {
    const prof = getAudioProfile();
    const node = new AudioWorkletNode(audioCtx, "playback-processor", {
      processorOptions: {
        bufferSec: prof.bufferSec,
        prebufSec: prof.prebufSec,
        hiWM: prof.hiWM,
        loWM: prof.loWM,
        silenceMax: prof.silenceMax,
      },
    });
    node.connect(audioCtx.destination);
    playbackNodes.set(seat, node);
    return node;
  } catch (e) {
    console.warn("room voice: playback node failed", e);
    return null;
  }
}

function playPcmFrame(seat, payload) {
  if (speakerMuted || payload.length < 10 || payload[0] !== TYPE_PCM) return;
  getAudioCtx();
  if (audioCtx.state === "suspended") audioCtx.resume();
  const dv = new DataView(payload.buffer, payload.byteOffset, payload.byteLength);
  const sr = dv.getUint32(5, true) || WIRE_SAMPLE_RATE;
  const audioBytes = payload.subarray(10);
  const aligned = new Uint8Array(audioBytes.length);
  aligned.set(audioBytes);
  const int16 = new Int16Array(aligned.buffer);
  let float32 = new Float32Array(int16.length);
  for (let i = 0; i < int16.length; i++) float32[i] = int16[i] / 32768;
  float32 = resampleLinear(float32, sr, audioCtx.sampleRate || WIRE_SAMPLE_RATE);
  const node = getPlaybackNode(seat);
  if (node) node.port.postMessage(float32);
  lastRemoteSpeakAt = performance.now();
  syncVoiceUi();
}

function pollInbound() {
  if (!wasm || typeof wasm.roomVoicePoll !== "function") return;
  const frames = wasm.roomVoicePoll();
  for (let i = 0; i < frames.length; i++) {
    const row = frames[i];
    const from = row[0];
    const bytes = row[1];
    if (!(bytes instanceof Uint8Array)) continue;
    playPcmFrame(from | 0, bytes);
  }
  // Clear "hearing" indicator shortly after last packet.
  if (lastRemoteSpeakAt && performance.now() - lastRemoteSpeakAt > 400) {
    lastRemoteSpeakAt = 0;
    syncVoiceUi();
  }
}

async function acquireMicStream() {
  const attempts = [
    {
      audio: {
        echoCancellation: true,
        noiseSuppression: true,
        autoGainControl: true,
      },
    },
    { audio: true },
  ];
  let lastErr = null;
  for (const constraints of attempts) {
    try {
      return await navigator.mediaDevices.getUserMedia(constraints);
    } catch (e) {
      lastErr = e;
      const name = e && e.name ? e.name : "";
      if (
        name !== "NotFoundError" &&
        name !== "OverconstrainedError" &&
        name !== "ConstraintNotSatisfiedError"
      ) {
        throw e;
      }
    }
  }
  throw lastErr || new Error("getUserMedia failed");
}

function markMicUnavailable(reason) {
  micUnavailable = true;
  micStream = null;
  talking = false;
  pttKeyDown = false;
  pttPadDown = false;
  setVoiceHint(reason);
  syncVoiceUi();
  // Restore game focus — getUserMedia often drops pointer lock.
  focusGameCanvas();
}

async function startCapture() {
  if (micUnavailable) return false;
  if (!navigator.mediaDevices?.getUserMedia) {
    markMicUnavailable("No mic — hear only");
    return false;
  }
  if (micBusy) return false;
  micBusy = true;
  try {
    await ensureWorklet();
    stopCapture();
    micStream = await acquireMicStream();
    captureSource = audioCtx.createMediaStreamSource(micStream);
    captureHpf = audioCtx.createBiquadFilter();
    captureHpf.type = "highpass";
    captureHpf.frequency.value = 80;
    captureGain = audioCtx.createGain();
    captureGain.gain.value = 1.0;
    const prof = getAudioProfile();
    captureNode = new AudioWorkletNode(audioCtx, "capture-processor", {
      numberOfInputs: 1,
      numberOfOutputs: 1,
      processorOptions: {
        gateThresh: prof.gateThresh,
        gateAttack: prof.gateAttack,
        gateDecay: prof.gateDecay,
      },
    });
    captureSource.connect(captureHpf);
    captureHpf.connect(captureGain);
    captureGain.connect(captureNode);
    captureNode.port.postMessage({ noiseGate: true });
    audioCaptureBuffer = new Float32Array(AUDIO_FRAME_SIZE);
    audioCaptureOffset = 0;
    captureNode.port.onmessage = (e) => {
      const chunk = e.data;
      if (!chunk || !(chunk instanceof Float32Array)) return;
      if (!talking) return;
      for (let i = 0; i < chunk.length; i++) {
        audioCaptureBuffer[audioCaptureOffset++] = chunk[i];
        if (audioCaptureOffset >= AUDIO_FRAME_SIZE) {
          flushCaptureBatch(audioCaptureBuffer);
          audioCaptureOffset = 0;
        }
      }
    };
    // Mic stays gated until PTT.
    for (const t of micStream.getAudioTracks()) t.enabled = false;
    setVoiceHint("Hold V to talk");
    syncVoiceUi();
    focusGameCanvas();
    return true;
  } catch (e) {
    const name = e && e.name ? e.name : "error";
    if (!micFailLogged) {
      micFailLogged = true;
      console.info("room voice: mic unavailable (%s) — playback only", name);
    }
    const hint =
      name === "NotFoundError" || name === "OverconstrainedError"
        ? "No mic — hear only"
        : name === "NotAllowedError"
          ? "Mic blocked — hear only"
          : "Mic unavailable — hear only";
    // Never retry: repeating getUserMedia exits pointer-lock and eats input.
    markMicUnavailable(hint);
    return false;
  } finally {
    micBusy = false;
  }
}

function stopCapture() {
  if (captureNode) {
    try {
      captureNode.disconnect();
      captureNode.port.onmessage = null;
    } catch (_) {}
    captureNode = null;
  }
  if (captureGain) {
    try {
      captureGain.disconnect();
    } catch (_) {}
    captureGain = null;
  }
  if (captureHpf) {
    try {
      captureHpf.disconnect();
    } catch (_) {}
    captureHpf = null;
  }
  if (captureSource) {
    try {
      captureSource.disconnect();
    } catch (_) {}
    captureSource = null;
  }
  if (micStream) {
    micStream.getTracks().forEach((t) => t.stop());
    micStream = null;
  }
  audioCaptureBuffer = null;
  audioCaptureOffset = 0;
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

function setTalking(next) {
  talking = !!next;
  if (micStream) {
    for (const t of micStream.getAudioTracks()) t.enabled = talking;
  }
  if (!talking) {
    audioCaptureOffset = 0;
  }
  syncVoiceUi();
}

function updateTalkFromSources() {
  const want = pttKeyDown || pttPadDown;
  if (want === talking) {
    syncVoiceUi();
    return;
  }
  setTalking(want);
}

async function beginPtt() {
  // No mic: ignore completely so V/pad never touch focus or async device APIs.
  if (micUnavailable) {
    return;
  }
  if (!micStream) {
    const ok = await startCapture();
    if (!ok) {
      return;
    }
  }
  if (audioCtx && audioCtx.state === "suspended") {
    try {
      await audioCtx.resume();
    } catch (_) {}
  }
  updateTalkFromSources();
  focusGameCanvas();
}

function endPtt() {
  if (micUnavailable) {
    talking = false;
    return;
  }
  updateTalkFromSources();
  focusGameCanvas();
}

function setVoiceHint(text) {
  const el = document.getElementById("voiceHint");
  if (el) el.textContent = text;
}

function syncVoiceUi() {
  const bar = document.getElementById("voiceBar");
  const pad = document.getElementById("voicePtt");
  const hear = document.getElementById("voiceHear");
  const spk = document.getElementById("voiceSpeaker");
  if (!bar) return;

  bar.classList.toggle("is-talking", talking);
  bar.classList.toggle("is-hearing", !talking && lastRemoteSpeakAt > 0);
  bar.classList.toggle("no-mic", micUnavailable || !micStream);

  if (pad) {
    pad.classList.toggle("active", talking);
    pad.classList.toggle("disabled", micUnavailable);
    if (micUnavailable) {
      pad.textContent = "No mic";
    } else {
      pad.textContent = talking ? "Talking…" : "Hold V · talk";
    }
  }
  if (hear) {
    hear.hidden = !(lastRemoteSpeakAt > 0 && !talking);
  }
  if (spk) {
    spk.classList.toggle("is-off", speakerMuted);
    spk.textContent = speakerMuted ? "Speaker off" : "Speaker on";
    spk.setAttribute("aria-pressed", speakerMuted ? "true" : "false");
  }
  if (micUnavailable) {
    // Hint already set by markMicUnavailable; keep hear-only messaging.
  } else if (talking) {
    setVoiceHint("Transmit");
  } else if (lastRemoteSpeakAt > 0) {
    setVoiceHint("Teammate speaking");
  } else if (micStream) {
    setVoiceHint("Hold V to talk");
  }
}

/** Avoid <button> focus steal (Space would activate it and break game keys). */
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
  // Never become the document activeElement.
  el.addEventListener("mousedown", (ev) => ev.preventDefault());
}

function ensureUi() {
  if (document.getElementById("voiceBar")) return;
  const bar = document.createElement("div");
  bar.id = "voiceBar";
  bar.setAttribute("role", "group");
  bar.setAttribute("aria-label", "Coop voice");
  bar.innerHTML =
    '<div id="voicePtt" class="voice-ptt" tabindex="-1">Hold V · talk</div>' +
    '<div id="voiceHear" class="voice-hear" hidden>Hearing…</div>' +
    '<div id="voiceSpeaker" class="voice-spk" tabindex="-1" aria-pressed="false">Speaker on</div>' +
    '<span id="voiceHint">Hold V to talk</span>';
  document.body.appendChild(bar);

  const pad = document.getElementById("voicePtt");
  bindNoFocus(
    pad,
    async () => {
      if (micUnavailable) {
        focusGameCanvas();
        return;
      }
      pttPadDown = true;
      await beginPtt();
    },
    () => {
      pttPadDown = false;
      endPtt();
    },
  );

  const spk = document.getElementById("voiceSpeaker");
  bindNoFocus(
    spk,
    () => {
      speakerMuted = !speakerMuted;
      syncVoiceUi();
      focusGameCanvas();
    },
    () => {},
  );

  syncVoiceUi();
}

function onKeyDown(ev) {
  if (ev.code !== PTT_CODE || ev.repeat) return;
  if (micUnavailable) return;
  // Don't hijack when typing in a real field (we have none, but be safe).
  const t = ev.target;
  if (t && (t.tagName === "INPUT" || t.tagName === "TEXTAREA" || t.isContentEditable)) {
    return;
  }
  pttKeyDown = true;
  // Fire-and-forget; never leave an unhandled rejection on the game page.
  beginPtt().catch(() => {
    markMicUnavailable("Mic unavailable — hear only");
  });
}

function onKeyUp(ev) {
  if (ev.code !== PTT_CODE) return;
  pttKeyDown = false;
  endPtt();
}

/**
 * @param {import('./room_web.js')} mod wasm-bindgen exports
 */
export async function initRoomVoice(mod) {
  wasm = mod;
  const params = new URLSearchParams(location.search);
  if (!params.get("ws")) return;
  ensureUi();
  if (!pollId) {
    pollId = setInterval(pollInbound, 20);
  }
  window.addEventListener("keydown", onKeyDown, true);
  window.addEventListener("keyup", onKeyUp, true);
  window.addEventListener("blur", () => {
    pttKeyDown = false;
    pttPadDown = false;
    endPtt();
  });

  // Warm AudioContext on first game gesture (autoplay policy).
  const kick = async () => {
    try {
      await ensureWorklet();
      if (audioCtx && audioCtx.state === "suspended") await audioCtx.resume();
    } catch (_) {}
    focusGameCanvas();
  };
  const canvas = document.getElementById("room");
  if (canvas) canvas.addEventListener("pointerdown", kick, { once: true });
}

export function teardownRoomVoice() {
  if (pollId) {
    clearInterval(pollId);
    pollId = null;
  }
  window.removeEventListener("keydown", onKeyDown, true);
  window.removeEventListener("keyup", onKeyUp, true);
  stopCapture();
  for (const node of playbackNodes.values()) {
    try {
      node.disconnect();
    } catch (_) {}
  }
  playbackNodes.clear();
  talking = false;
  pttKeyDown = false;
  pttPadDown = false;
}
