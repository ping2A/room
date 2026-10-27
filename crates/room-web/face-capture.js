/**
 * Face capture / import dialog (docs/26 §3.2–3.3, F5 multi-angle).
 * Keeps getUserMedia lifecycle aligned with voice.js (stop tracks on close).
 */

/** @typedef {{ leftEye:[number,number], rightEye:[number,number], chin:[number,number] }} ManualPoints */

function ensureDlgStyles() {
  if (!document.getElementById("room-dlg-css")) {
    const s = document.createElement("style");
    s.id = "room-dlg-css";
    s.textContent = `
.room-dlg-back{position:fixed;inset:0;z-index:10000;background:rgba(0,0,0,.82);display:flex;align-items:center;justify-content:center;font:13px/1.35 system-ui,sans-serif;color:#d7dbe0}
.room-dlg{background:#1a1d22;border:1px solid #3a414c;border-radius:8px;padding:1.1rem 1.2rem;max-width:min(440px,94vw);width:100%;box-shadow:0 18px 48px rgba(0,0,0,.55);box-sizing:border-box}
.room-dlg h2{margin:0 0 .45rem;font-size:1.05rem;color:#e8eaed}
.room-dlg p,.room-dlg .hint{margin:0 0 .75rem;font-size:.82rem;color:#9aa3ad}
.room-dlg .row-btns{display:flex;flex-wrap:wrap;gap:.45rem;margin:.65rem 0}
.room-dlg .foot{display:flex;gap:.45rem;justify-content:flex-end;margin-top:.85rem;flex-wrap:wrap}
.room-dlg button{appearance:none;-webkit-appearance:none;font:inherit;cursor:pointer;background:#2a2f36;border:1px solid #454c57;border-radius:5px;color:#e8eaed;padding:.4rem .85rem;line-height:1.2}
.room-dlg button:hover:not(:disabled){background:#343a43;border-color:#5a6270}
.room-dlg button:disabled{opacity:.4;cursor:default}
.room-dlg button.primary{background:#2d5ea8;border-color:#3b74c7;color:#fff}
.room-dlg button.go{background:#243d2e;border-color:#3a6b4a;color:#8fd9a8}
.room-dlg label.check{display:flex;align-items:center;gap:.4rem;font-size:.8rem;color:#c9ced6}
`;
    document.head.appendChild(s);
  }
  if (document.getElementById("room-face-capture-css")) return;
  const fc = document.createElement("style");
  fc.id = "room-face-capture-css";
  fc.textContent = `
.room-face-capture .room-dlg{max-width:min(420px,94vw)}
.room-face-capture .fc-preview{position:relative;display:none;background:#0a0a0a;border-radius:6px;overflow:hidden;border:1px solid #2e333b;margin:0 0 .75rem}
.room-face-capture .fc-preview video,.room-face-capture .fc-still{display:block;width:100%;max-height:260px;background:#000}
.room-face-capture .fc-still{display:none;border-radius:6px;border:1px solid #2e333b;margin:0 0 .75rem}
.room-face-capture .fc-guide{pointer-events:none;position:absolute;left:50%;top:42%;width:52%;aspect-ratio:1;transform:translate(-50%,-50%);border:2px solid rgba(232,196,110,.9);border-radius:50%;box-shadow:0 0 0 9999px rgba(0,0,0,.38)}
.room-face-capture .fc-actions{display:grid;grid-template-columns:1fr 1fr;gap:.45rem;margin:0 0 .65rem}
.room-face-capture .fc-actions button{width:100%}
.room-face-capture .fc-consent{margin:0 0 .55rem}
.room-face-capture .fc-status{min-height:1.2em;margin:0}
`;
  document.head.appendChild(fc);
}

const YAW_STEPS = [
  { id: "front", label: "Look straight at the camera" },
  { id: "three_quarter_left", label: "Turn slightly left (¾)" },
  { id: "three_quarter_right", label: "Turn slightly right (¾)" },
  { id: "profile_left", label: "Full left profile" },
  { id: "profile_right", label: "Full right profile" },
];

/**
 * @param {object} opts
 * @param {(img: HTMLCanvasElement, points: ManualPoints, meta: object) => void} opts.onCapture
 * @param {(views: Array<{yaw:string, canvas:HTMLCanvasElement, points:ManualPoints}>, meta: object) => void} [opts.onMultiAngle]
 * @param {boolean} [opts.guidedMultiAngle]
 * @param {() => void} [opts.onCancel]
 */
export function openFaceCapture(opts) {
  ensureDlgStyles();
  const { onCapture, onMultiAngle, onCancel } = opts;
  const guided = !!opts.guidedMultiAngle && typeof onMultiAngle === "function";
  let step = 0;
  /** @type {Array<{yaw:string, canvas:HTMLCanvasElement, points:ManualPoints}>} */
  const captured = [];

  const overlay = document.createElement("div");
  overlay.className = "room-dlg-back room-face-capture";
  overlay.innerHTML = `
    <div class="room-dlg" role="dialog" aria-label="Face capture">
      <h2>Face capture</h2>
      <p data-guide class="hint"></p>
      <div data-preview-wrap class="fc-preview">
        <video playsinline muted style="transform:scaleX(-1)"></video>
        <div data-head-guide class="fc-guide" aria-hidden="true"></div>
      </div>
      <canvas class="fc-still"></canvas>
      <div class="fc-actions">
        <button type="button" data-act="camera">Camera</button>
        <button type="button" data-act="file">Import photo</button>
      </div>
      <label class="check fc-consent">
        <input type="checkbox" data-consent disabled /> I am this person, or I have permission
      </label>
      <p data-status class="hint fc-status"></p>
      <div class="foot">
        <button type="button" data-act="cancel">Cancel</button>
        <button type="button" class="primary" data-act="snap" disabled>Capture</button>
      </div>
      <input type="file" accept="image/*" hidden data-file />
    </div>`;
  document.body.appendChild(overlay);

  const video = overlay.querySelector("video");
  const previewWrap = overlay.querySelector("[data-preview-wrap]");
  const canvas = overlay.querySelector("canvas.fc-still");
  const status = overlay.querySelector("[data-status]");
  const guideEl = overlay.querySelector("[data-guide]");
  const consent = overlay.querySelector("[data-consent]");
  const snapBtn = overlay.querySelector("[data-act=snap]");
  const fileInput = overlay.querySelector("[data-file]");
  /** @type {MediaStream | null} */
  let stream = null;
  let sourceKind = "none";

  function setStatus(t) {
    status.textContent = t || "";
  }

  function updateGuide() {
    if (!guided) {
      guideEl.textContent = "Fit your head in the circle, then Capture.";
      return;
    }
    const s = YAW_STEPS[step] || YAW_STEPS[0];
    guideEl.textContent = `Step ${step + 1}/${YAW_STEPS.length}: ${s.label}`;
    snapBtn.textContent =
      step + 1 < YAW_STEPS.length ? "Capture & next" : "Finish";
  }
  updateGuide();

  function stopCam() {
    if (stream) {
      for (const t of stream.getTracks()) t.stop();
      stream = null;
    }
    previewWrap.style.display = "none";
    video.srcObject = null;
  }

  function close() {
    stopCam();
    document.removeEventListener("visibilitychange", onVisibility);
    overlay.remove();
    window.removeEventListener("keydown", onKey);
  }

  function onKey(e) {
    if (e.key === "Escape") {
      close();
      onCancel && onCancel();
    }
  }
  window.addEventListener("keydown", onKey);

  function showStill(bitmap) {
    stopCam();
    canvas.style.display = "block";
    canvas.width = bitmap.width;
    canvas.height = bitmap.height;
    const ctx = canvas.getContext("2d");
    ctx.drawImage(bitmap, 0, 0);
    sourceKind = "file";
    consent.disabled = false;
    snapBtn.disabled = false;
    setStatus("Tick permission, Capture, then drag the face onto the sprite.");
  }

  function defaultPoints(w, h) {
    return {
      leftEye: [w * 0.35, h * 0.38],
      rightEye: [w * 0.65, h * 0.38],
      chin: [w * 0.5, h * 0.78],
    };
  }

  function grabFrame() {
    let w, h, ctx;
    if (sourceKind === "camera") {
      w = video.videoWidth;
      h = video.videoHeight;
      canvas.width = w;
      canvas.height = h;
      ctx = canvas.getContext("2d");
      ctx.save();
      ctx.translate(w, 0);
      ctx.scale(-1, 1);
      ctx.drawImage(video, 0, 0);
      ctx.restore();
    } else {
      w = canvas.width;
      h = canvas.height;
    }
    const out = document.createElement("canvas");
    out.width = w;
    out.height = h;
    out.getContext("2d").drawImage(canvas, 0, 0);
    return { out, points: defaultPoints(w, h) };
  }

  overlay.addEventListener("click", async (e) => {
    const act = e.target?.getAttribute?.("data-act");
    if (act === "cancel") {
      close();
      onCancel && onCancel();
      return;
    }
    if (act === "file") {
      fileInput.click();
      return;
    }
    if (act === "camera") {
      try {
        stopCam();
        stream = await navigator.mediaDevices.getUserMedia({
          video: { facingMode: "user", width: 720, height: 720 },
          audio: false,
        });
        video.srcObject = stream;
        previewWrap.style.display = "block";
        canvas.style.display = "none";
        await video.play();
        sourceKind = "camera";
        consent.checked = true;
        consent.disabled = true;
        snapBtn.disabled = false;
        setStatus("Move so your head fills the circle, then Capture.");
      } catch (err) {
        setStatus(
          `Camera unavailable (${err?.name || err}). Use Import a photo instead.`,
        );
        snapBtn.disabled = true;
      }
      return;
    }
    if (act === "snap") {
      if (sourceKind === "file" && !consent.checked) {
        setStatus("Tick the permission checkbox to import a photo.");
        return;
      }
      const { out, points } = grabFrame();
      const meta = {
        consent: sourceKind === "camera" ? "self-camera" : "attested",
        source: sourceKind,
        yaw: guided ? YAW_STEPS[step].id : "front",
      };
      if (guided) {
        captured.push({ yaw: YAW_STEPS[step].id, canvas: out, points });
        if (step + 1 < YAW_STEPS.length) {
          step += 1;
          updateGuide();
          setStatus(`Saved ${captured.length}. Next pose…`);
          return;
        }
        close();
        onMultiAngle(captured, meta);
        return;
      }
      close();
      onCapture(out, points, meta);
    }
  });

  fileInput.addEventListener("change", async () => {
    const f = fileInput.files?.[0];
    if (!f) return;
    try {
      const bmp = await createImageBitmap(f, { imageOrientation: "from-image" });
      showStill(bmp);
    } catch (err) {
      setStatus(`Could not decode image: ${err}`);
    }
  });

  overlay.addEventListener("paste", async (e) => {
    const item = [...(e.clipboardData?.items || [])].find((i) =>
      i.type.startsWith("image/"),
    );
    if (!item) return;
    const f = item.getAsFile();
    if (!f) return;
    e.preventDefault();
    const bmp = await createImageBitmap(f, { imageOrientation: "from-image" });
    showStill(bmp);
  });

  function onVisibility() {
    if (document.hidden) stopCam();
  }
  document.addEventListener("visibilitychange", onVisibility);

  return { close };
}

/** Yaw step ids for hosts that drive multi-angle outside the dialog. */
export { YAW_STEPS };
