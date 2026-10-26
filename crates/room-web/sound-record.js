/**
 * Mic / file → WAV for Doom sound lump overrides (thing See/Attack/…).
 */

/** Inject once: dark dialog chrome (browser default buttons are unreadable). */
export function ensureDlgStyles() {
  if (document.getElementById("room-dlg-css")) return;
  const s = document.createElement("style");
  s.id = "room-dlg-css";
  s.textContent = `
.room-dlg-back{position:fixed;inset:0;z-index:10000;background:rgba(0,0,0,.82);display:flex;align-items:center;justify-content:center;font:13px/1.35 system-ui,sans-serif;color:#d7dbe0}
.room-dlg{background:#1a1d22;border:1px solid #3a414c;border-radius:8px;padding:1.1rem 1.2rem;max-width:min(440px,94vw);width:100%;box-shadow:0 18px 48px rgba(0,0,0,.55);box-sizing:border-box}
.room-dlg h2{margin:0 0 .45rem;font-size:1.05rem;color:#e8eaed}
.room-dlg p,.room-dlg .hint{margin:0 0 .75rem;font-size:.82rem;color:#9aa3ad}
.room-dlg .row-btns{display:flex;flex-wrap:wrap;gap:.45rem;margin:.65rem 0}
.room-dlg .foot{display:flex;gap:.45rem;justify-content:flex-end;margin-top:.85rem;flex-wrap:wrap}
.room-dlg button,.room-dlg .rdbtn{
  appearance:none;-webkit-appearance:none;font:inherit;cursor:pointer;
  background:#2a2f36;border:1px solid #454c57;border-radius:5px;color:#e8eaed;
  padding:.4rem .85rem;line-height:1.2}
.room-dlg button:hover:not(:disabled),.room-dlg .rdbtn:hover:not(:disabled){background:#343a43;border-color:#5a6270}
.room-dlg button:disabled{opacity:.4;cursor:default}
.room-dlg button.primary{background:#2d5ea8;border-color:#3b74c7;color:#fff}
.room-dlg button.primary:hover:not(:disabled){background:#3670c4}
.room-dlg button.go{background:#243d2e;border-color:#3a6b4a;color:#8fd9a8}
.room-dlg button.go:hover:not(:disabled){background:#2d4e3a}
.room-dlg button.danger{background:#3a2424;border-color:#6b3a3a;color:#ffb3a8}
.room-dlg button.rec-on{background:#6b2222;border-color:#a6463a;color:#fff;animation:room-rec-pulse 1s ease-in-out infinite}
@keyframes room-rec-pulse{50%{opacity:.75}}
.room-dlg label.check{display:flex;align-items:center;gap:.4rem;font-size:.8rem;color:#c9ced6}
.room-dlg canvas,.room-dlg video{display:block;width:100%;background:#0a0a0a;border:1px solid #2e333b;border-radius:4px}
.room-dlg .slist{display:flex;flex-direction:column;gap:4px;max-height:min(360px,55vh);overflow:auto;margin:.5rem 0}
.room-dlg .srow{display:flex;align-items:center;gap:8px;padding:6px 8px;background:#22262c;border:1px solid #2e333b;border-radius:5px}
.room-dlg .srow .meta{margin-left:auto;font:11px ui-monospace,monospace;color:#8a919b}
.room-dlg .srow .name{font-weight:600;font-size:12px}
.room-dlg .srow .sub{font:11px ui-monospace,monospace;color:#8a919b}
.room-ctx{position:fixed;z-index:10001;min-width:160px;background:#1a1d22;border:1px solid #454c57;border-radius:6px;padding:4px;box-shadow:0 12px 32px rgba(0,0,0,.55)}
.room-ctx button{display:block;width:100%;text-align:left;appearance:none;font:13px system-ui,sans-serif;background:transparent;border:0;color:#d7dbe0;padding:7px 10px;border-radius:4px;cursor:pointer}
.room-ctx button:hover{background:#2a2f36}
`;
  document.head.appendChild(s);
}

/**
 * Encode mono/stereo float PCM as 16-bit little-endian WAV.
 * @param {Float32Array} samples interleaved
 * @param {number} sampleRate
 * @param {number} [channels=1]
 */
export function encodeWav(samples, sampleRate, channels = 1) {
  const ch = Math.max(1, channels | 0);
  const n = samples.length;
  const dataBytes = n * 2;
  const buf = new ArrayBuffer(44 + dataBytes);
  const view = new DataView(buf);
  const w = (o, s) => {
    for (let i = 0; i < s.length; i++) view.setUint8(o + i, s.charCodeAt(i));
  };
  w(0, "RIFF");
  view.setUint32(4, 36 + dataBytes, true);
  w(8, "WAVE");
  w(12, "fmt ");
  view.setUint32(16, 16, true);
  view.setUint16(20, 1, true);
  view.setUint16(22, ch, true);
  view.setUint32(24, sampleRate, true);
  view.setUint32(28, sampleRate * ch * 2, true);
  view.setUint16(32, ch * 2, true);
  view.setUint16(34, 16, true);
  w(36, "data");
  view.setUint32(40, dataBytes, true);
  let o = 44;
  for (let i = 0; i < n; i++) {
    const x = Math.max(-1, Math.min(1, samples[i]));
    view.setInt16(o, (x < 0 ? x * 0x8000 : x * 0x7fff) | 0, true);
    o += 2;
  }
  return new Uint8Array(buf);
}

/**
 * Record from the mic (or import a file) → WAV bytes.
 * @param {{ title?: string, onDone: (wav: Uint8Array, meta: object) => void, onCancel?: () => void }} opts
 */
export function openSoundRecord(opts) {
  ensureDlgStyles();
  const { onDone, onCancel } = opts;
  const overlay = document.createElement("div");
  overlay.className = "room-dlg-back";
  overlay.innerHTML = `
    <div class="room-dlg" role="dialog" aria-label="Record sound">
      <h2>${opts.title || "Record sound"}</h2>
      <p class="hint">Hold Record, speak or make the noise, then Stop. Or import a WAV/OGG/MP3 file.</p>
      <canvas data-wave width="400" height="72"></canvas>
      <p data-status class="hint" style="min-height:1.2em;margin:.5rem 0 0"></p>
      <div class="row-btns">
        <button type="button" data-act="rec">● Record</button>
        <button type="button" data-act="stop" disabled>■ Stop</button>
        <button type="button" data-act="play" disabled>▶ Preview</button>
        <button type="button" data-act="file">Import file…</button>
      </div>
      <div class="foot">
        <button type="button" data-act="cancel">Cancel</button>
        <button type="button" class="primary" data-act="apply" disabled>Use recording</button>
      </div>
      <input type="file" accept="audio/*,.wav,.ogg,.mp3,.flac" hidden data-file />
    </div>`;
  document.body.appendChild(overlay);

  const status = overlay.querySelector("[data-status]");
  const wave = overlay.querySelector("[data-wave]");
  const wg = wave.getContext("2d");
  const fileInput = overlay.querySelector("[data-file]");
  const btn = (act) => overlay.querySelector(`[data-act="${act}"]`);

  /** @type {MediaStream | null} */
  let stream = null;
  /** @type {AudioContext | null} */
  let actx = null;
  /** @type {ScriptProcessorNode | null} */
  let proc = null;
  /** @type {MediaStreamAudioSourceNode | null} */
  let source = null;
  /** @type {Float32Array[]} */
  let chunks = [];
  let sampleRate = 22050;
  /** @type {Float32Array | null} */
  let pcm = null;
  let recording = false;
  /** @type {AudioBufferSourceNode | null} */
  let previewSrc = null;

  function setStatus(t) {
    status.textContent = t || "";
  }

  function drawWave(samples) {
    const w = wave.width;
    const h = wave.height;
    wg.fillStyle = "#0f1114";
    wg.fillRect(0, 0, w, h);
    if (!samples || !samples.length) {
      wg.strokeStyle = "#3a414c";
      wg.beginPath();
      wg.moveTo(0, h / 2);
      wg.lineTo(w, h / 2);
      wg.stroke();
      return;
    }
    wg.strokeStyle = "#7cc3ff";
    wg.beginPath();
    const step = Math.max(1, Math.floor(samples.length / w));
    for (let x = 0; x < w; x++) {
      let mn = 1;
      let mx = -1;
      const i0 = x * step;
      for (let i = i0; i < i0 + step && i < samples.length; i++) {
        const v = samples[i];
        mn = Math.min(mn, v);
        mx = Math.max(mx, v);
      }
      const y0 = h / 2 - mx * (h * 0.45);
      const y1 = h / 2 - mn * (h * 0.45);
      wg.moveTo(x + 0.5, y0);
      wg.lineTo(x + 0.5, y1);
    }
    wg.stroke();
  }
  drawWave(null);

  function mergeChunks() {
    let n = 0;
    for (const c of chunks) n += c.length;
    const out = new Float32Array(n);
    let o = 0;
    for (const c of chunks) {
      out.set(c, o);
      o += c.length;
    }
    return out;
  }

  function stopTracks() {
    if (proc) {
      try {
        proc.disconnect();
      } catch (_) { /* */ }
      proc.onaudioprocess = null;
      proc = null;
    }
    if (source) {
      try {
        source.disconnect();
      } catch (_) { /* */ }
      source = null;
    }
    if (stream) {
      for (const t of stream.getTracks()) t.stop();
      stream = null;
    }
    recording = false;
    btn("rec").disabled = false;
    btn("rec").classList.remove("rec-on");
    btn("rec").textContent = "● Record";
    btn("stop").disabled = true;
  }

  function finishRecording() {
    stopTracks();
    pcm = mergeChunks();
    chunks = [];
    drawWave(pcm);
    btn("play").disabled = !pcm?.length;
    btn("apply").disabled = !pcm?.length;
    setStatus(
      pcm?.length
        ? `Recorded ${(pcm.length / sampleRate).toFixed(2)} s @ ${sampleRate} Hz`
        : "Nothing captured",
    );
  }

  async function startRec() {
    try {
      if (previewSrc) {
        try {
          previewSrc.stop();
        } catch (_) { /* */ }
        previewSrc = null;
      }
      stream = await navigator.mediaDevices.getUserMedia({
        audio: {
          echoCancellation: false,
          noiseSuppression: false,
          autoGainControl: true,
        },
        video: false,
      });
      actx = actx || new AudioContext();
      if (actx.state === "suspended") await actx.resume();
      sampleRate = actx.sampleRate;
      source = actx.createMediaStreamSource(stream);
      // ScriptProcessor is deprecated but widely available; fine for short editor clips.
      const bufSize = 4096;
      proc = actx.createScriptProcessor(bufSize, 1, 1);
      chunks = [];
      proc.onaudioprocess = (ev) => {
        if (!recording) return;
        chunks.push(new Float32Array(ev.inputBuffer.getChannelData(0)));
        drawWave(chunks[chunks.length - 1]);
      };
      source.connect(proc);
      // ScriptProcessor only runs while connected to the graph; mute so the mic
      // is not monitored through the speakers.
      const silent = actx.createGain();
      silent.gain.value = 0;
      proc.connect(silent);
      silent.connect(actx.destination);
      recording = true;
      btn("rec").disabled = true;
      btn("rec").classList.add("rec-on");
      btn("rec").textContent = "● Recording…";
      btn("stop").disabled = false;
      btn("play").disabled = true;
      btn("apply").disabled = true;
      setStatus("Recording — click Stop when done.");
    } catch (err) {
      stopTracks();
      setStatus(`Mic unavailable (${err?.name || err}). Try Import file.`);
    }
  }

  function playPreview() {
    if (!pcm?.length) return;
    actx = actx || new AudioContext();
    if (actx.state === "suspended") actx.resume();
    if (previewSrc) {
      try {
        previewSrc.stop();
      } catch (_) { /* */ }
    }
    const buf = actx.createBuffer(1, pcm.length, sampleRate);
    buf.copyToChannel(pcm, 0);
    previewSrc = actx.createBufferSource();
    previewSrc.buffer = buf;
    previewSrc.connect(actx.destination);
    previewSrc.start();
  }

  function close() {
    stopTracks();
    if (previewSrc) {
      try {
        previewSrc.stop();
      } catch (_) { /* */ }
    }
    if (actx) {
      actx.close().catch(() => {});
      actx = null;
    }
    overlay.remove();
    window.removeEventListener("keydown", onKey);
  }

  function apply() {
    if (!pcm?.length) return;
    const wav = encodeWav(pcm, sampleRate, 1);
    close();
    onDone(wav, { rate: sampleRate, samples: pcm.length, source: "mic" });
  }

  function onKey(e) {
    if (e.key === "Escape") {
      close();
      onCancel && onCancel();
    }
  }
  window.addEventListener("keydown", onKey);

  overlay.addEventListener("click", (e) => {
    const act = e.target?.getAttribute?.("data-act");
    if (act === "cancel") {
      close();
      onCancel && onCancel();
    } else if (act === "rec") startRec();
    else if (act === "stop") finishRecording();
    else if (act === "play") playPreview();
    else if (act === "apply") apply();
    else if (act === "file") fileInput.click();
  });

  fileInput.addEventListener("change", async () => {
    const f = fileInput.files?.[0];
    if (!f) return;
    try {
      const bytes = new Uint8Array(await f.arrayBuffer());
      // Pass-through: engine decode_sound accepts WAV/OGG/FLAC/MP3.
      pcm = null;
      drawWave(null);
      close();
      onDone(bytes, { source: "file", name: f.name });
    } catch (err) {
      setStatus(`Could not read file: ${err}`);
    }
  });

  return { close };
}

export const SOUND_PROP_LABELS = {
  see_sound: "See (alert)",
  attack_sound: "Attack",
  pain_sound: "Pain",
  death_sound: "Death",
  active_sound: "Active (idle)",
};

/**
 * Draw a waveform into a canvas (mono or first channel of interleaved samples).
 * @param {HTMLCanvasElement} canvas
 * @param {Float32Array | null | undefined} samples
 * @param {number} [channels=1]
 */
export function drawWaveform(canvas, samples, channels = 1) {
  const w = canvas.width;
  const h = canvas.height;
  const g = canvas.getContext("2d");
  g.fillStyle = "#0f1114";
  g.fillRect(0, 0, w, h);
  if (!samples || !samples.length) {
    g.strokeStyle = "#3a414c";
    g.beginPath();
    g.moveTo(0, h / 2);
    g.lineTo(w, h / 2);
    g.stroke();
    return;
  }
  const ch = Math.max(1, channels | 0);
  const frames = Math.floor(samples.length / ch);
  g.strokeStyle = "#7cc3ff";
  g.beginPath();
  for (let x = 0; x < w; x++) {
    const i0 = Math.floor((x / w) * frames);
    const i1 = Math.max(i0 + 1, Math.floor(((x + 1) / w) * frames));
    let mn = 1;
    let mx = -1;
    for (let i = i0; i < i1 && i < frames; i++) {
      const v = samples[i * ch];
      mn = Math.min(mn, v);
      mx = Math.max(mx, v);
    }
    g.moveTo(x + 0.5, h / 2 - mx * (h * 0.42));
    g.lineTo(x + 0.5, h / 2 - mn * (h * 0.42));
  }
  g.stroke();
}

function fmtDur(sec) {
  if (!(sec > 0) || !Number.isFinite(sec)) return "—";
  if (sec < 10) return `${sec.toFixed(2)} s`;
  if (sec < 60) return `${sec.toFixed(1)} s`;
  const m = Math.floor(sec / 60);
  const s = sec - m * 60;
  return `${m}:${s.toFixed(1).padStart(4, "0")}`;
}

/**
 * Inline Sounds panel for the thing sprite editor (or actor inspector).
 * @param {object} opts
 * @param {(tag:string, attrs?:object, ...kids:any) => HTMLElement} opts.el
 * @param {() => Array<{key:string, logical:string, lump?:string|null, id?:number|null}>} opts.getSlots
 * @param {(id:number) => {rate:number, channels:number, samples:Float32Array}} opts.decode
 * @param {(snd: object) => void} opts.play
 * @param {(slot: object, bytes: Uint8Array, meta: object) => void} opts.replace
 * @param {string} [opts.className]
 */
export function mountThingSoundsPanel(opts) {
  const { el, getSlots, decode, play, replace } = opts;
  const box = el("fieldset", { class: "px-sounds" }, el("legend", {}, "Sounds"));
  const body = el("div", { class: "px-sounds-body" });
  box.append(
    el("p", { class: "muted" }, "Play or record over this thing’s See / Attack / Pain / Death / Active lumps."),
    body,
  );

  function refresh() {
    const slots = getSlots() || [];
    body.replaceChildren();
    if (!slots.length) {
      body.append(
        el("p", { class: "muted" }, "No See/Attack/Pain/Death/Active sounds on this actor."),
      );
      return;
    }
    for (const slot of slots) {
      const label = SOUND_PROP_LABELS[slot.key] || slot.key;
      const wave = el("canvas", {
        class: "px-sound-wave",
        width: 280,
        height: 44,
        title: slot.lump || slot.logical || "",
      });
      const meta = el("div", { class: "px-sound-meta muted" }, "—");
      const head = el("div", { class: "px-sound-head" },
        el("div", {},
          el("div", { class: "px-sound-title" }, label),
          el("div", { class: "px-sound-sub muted" },
            `${slot.logical || "—"}${slot.lump ? ` → ${slot.lump}` : ""}`)),
        el("div", { class: "px-sound-actions" }));
      const actions = head.querySelector(".px-sound-actions");
      const playBtn = el("button", {
        class: "btn small",
        type: "button",
        title: "Play",
        disabled: true,
      }, "▶");
      const recBtn = el("button", {
        class: "btn small go",
        type: "button",
        title: slot.lump ? `Record over ${slot.lump}` : "No lump mapped",
        disabled: !slot.lump,
      }, "🎤");
      actions.append(playBtn, recBtn);

      let snd = null;
      if (slot.id != null) {
        try {
          snd = decode(slot.id);
        } catch (_) {
          snd = null;
        }
      }
      if (snd) {
        const ch = snd.channels || 1;
        const frames = Math.floor(snd.samples.length / ch);
        const sec = frames / Math.max(1, snd.rate);
        drawWaveform(wave, snd.samples, ch);
        meta.textContent = `${fmtDur(sec)} · ${snd.rate} Hz · ${ch === 1 ? "mono" : `${ch} ch`} · ${frames.toLocaleString()} samples`;
        playBtn.disabled = false;
        playBtn.onclick = () => play(snd);
      } else {
        drawWaveform(wave, null);
        meta.textContent = slot.lump ? "Could not decode lump" : "Unresolved (no SNDINFO lump)";
      }
      recBtn.onclick = () => {
        openSoundRecord({
          title: `Record · ${label}`,
          onDone: (bytes, metaInfo) => {
            replace(slot, bytes, metaInfo);
            refresh();
          },
        });
      };
      body.append(el("div", { class: "px-sound-row" }, head, wave, meta));
    }
  }

  refresh();
  return { box, refresh };
}
