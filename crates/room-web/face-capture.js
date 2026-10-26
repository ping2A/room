/**
 * Face capture / import dialog (docs/26 §3.2–3.3, F5 multi-angle).
 * Keeps getUserMedia lifecycle aligned with voice.js (stop tracks on close).
 */

/** @typedef {{ leftEye:[number,number], rightEye:[number,number], chin:[number,number] }} ManualPoints */

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
  const { onCapture, onMultiAngle, onCancel } = opts;
  const guided = !!opts.guidedMultiAngle && typeof onMultiAngle === "function";
  let step = 0;
  /** @type {Array<{yaw:string, canvas:HTMLCanvasElement, points:ManualPoints}>} */
  const captured = [];

  const overlay = document.createElement("div");
  overlay.className = "room-face-capture";
  overlay.style.cssText =
    "position:fixed;inset:0;z-index:9999;background:rgba(0,0,0,.75);display:flex;align-items:center;justify-content:center;font-family:system-ui,sans-serif;color:#eee";
  overlay.innerHTML = `
    <div style="background:#1a1a1a;padding:1.25rem;max-width:420px;width:92%;border:1px solid #444">
      <h2 style="margin:0 0 .75rem;font-size:1.1rem">Face capture</h2>
      <p data-guide style="margin:0 0 .75rem;font-size:.85rem;opacity:.85"></p>
      <video playsinline muted style="width:100%;max-height:240px;background:#000;display:none"></video>
      <canvas style="width:100%;max-height:240px;background:#000;display:none"></canvas>
      <div style="display:flex;flex-wrap:wrap;gap:.5rem;margin:.75rem 0">
        <button type="button" data-act="camera">Camera</button>
        <button type="button" data-act="file">Import photo</button>
        <label style="font-size:.8rem;display:flex;align-items:center;gap:.35rem">
          <input type="checkbox" data-consent disabled /> I am this person, or I have permission
        </label>
      </div>
      <p data-status style="font-size:.8rem;min-height:1.2em;opacity:.9"></p>
      <div style="display:flex;gap:.5rem;justify-content:flex-end;margin-top:.75rem">
        <button type="button" data-act="cancel">Cancel</button>
        <button type="button" data-act="snap" disabled>Capture</button>
      </div>
      <input type="file" accept="image/*" hidden data-file />
    </div>`;
  document.body.appendChild(overlay);

  const video = overlay.querySelector("video");
  const canvas = overlay.querySelector("canvas");
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
      guideEl.textContent =
        "Camera, file, or paste. Manual eye/chin refine after capture.";
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
    video.style.display = "none";
    video.srcObject = null;
  }

  function close() {
    stopCam();
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
    setStatus("Adjust consent, then Capture. Eyes default to upper third.");
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
        video.style.display = "block";
        canvas.style.display = "none";
        await video.play();
        sourceKind = "camera";
        consent.checked = true;
        consent.disabled = true;
        snapBtn.disabled = false;
        setStatus("Ready — hold still, then Capture.");
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

  document.addEventListener(
    "visibilitychange",
    () => {
      if (document.hidden) stopCam();
    },
    { once: false },
  );

  return { close };
}

/** Yaw step ids for hosts that drive multi-angle outside the dialog. */
export { YAW_STEPS };
