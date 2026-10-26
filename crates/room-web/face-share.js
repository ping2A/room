/**
 * Peer face-share UI + ControlMsg helpers (docs/26 §8 / F6).
 * Server must have `face_share_enabled`; never persists blobs.
 */

import { mergeFaceSettings } from "./settings-faces.js";

/** Encode raw RGBA bytes to base64 (browser). */
export function rgbaToB64(bytes) {
  let s = "";
  const u8 = bytes instanceof Uint8Array ? bytes : new Uint8Array(bytes);
  for (let i = 0; i < u8.length; i++) s += String.fromCharCode(u8[i]);
  return btoa(s);
}

/** Decode base64 to Uint8Array. */
export function b64ToRgba(b64) {
  const bin = atob(b64);
  const out = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i);
  return out;
}

/**
 * Build a v2 FaceShare control object (64×64 wire size).
 * @param {{ name: string, faceId: string|bigint|number, rgba: Uint8Array }} opts
 */
export function buildFaceShareMsg(opts) {
  return {
    t: "face_share",
    name: String(opts.name || "face").slice(0, 64),
    face_id: String(opts.faceId),
    rgba_b64: rgbaToB64(opts.rgba),
  };
}

/**
 * Prompt the local user to accept a peer face (Ask policy).
 * @returns {Promise<boolean>}
 */
export function promptAcceptPeerFace({ from, name }) {
  return new Promise((resolve) => {
    const overlay = document.createElement("div");
    overlay.className = "room-face-share-prompt";
    overlay.style.cssText =
      "position:fixed;inset:0;z-index:10000;background:rgba(0,0,0,.6);display:flex;align-items:center;justify-content:center;font-family:system-ui,sans-serif;color:#eee";
    overlay.innerHTML = `
      <div style="background:#1a1a1a;padding:1rem 1.25rem;border:1px solid #555;max-width:360px;width:90%">
        <p style="margin:0 0 .75rem">Seat ${from ?? "?"} shared face “${escapeHtml(name || "face")}”. Show it?</p>
        <div style="display:flex;gap:.5rem;justify-content:flex-end">
          <button type="button" data-a="no">Ignore</button>
          <button type="button" data-a="yes">Accept</button>
        </div>
      </div>`;
    document.body.appendChild(overlay);
    overlay.addEventListener("click", (e) => {
      const a = e.target?.getAttribute?.("data-a");
      if (a === "yes") {
        overlay.remove();
        resolve(true);
      } else if (a === "no") {
        overlay.remove();
        resolve(false);
      }
    });
  });
}

function escapeHtml(s) {
  return String(s)
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");
}

/**
 * Handle an inbound FaceShare control message.
 * @param {object} msg
 * @param {ReturnType<typeof mergeFaceSettings>} settings
 * @param {(blob: {faceId:string,name:string,rgba:Uint8Array,from?:number}) => void} onAccept
 */
export async function handleInboundFaceShare(msg, settings, onAccept) {
  const cfg = mergeFaceSettings(settings || {});
  if (cfg.hideSharedFaces) return false;
  const ok = await promptAcceptPeerFace({ from: msg.from, name: msg.name });
  if (!ok) return false;
  const rgba = b64ToRgba(msg.rgba_b64 || "");
  onAccept({
    faceId: msg.face_id,
    name: msg.name,
    rgba,
    from: msg.from,
  });
  return true;
}
