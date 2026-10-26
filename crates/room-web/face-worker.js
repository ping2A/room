/**
 * Worker host for face detect / bake (docs/26 F3).
 * Ships a skin-heuristic detector matching `room_faceswap::detect` until
 * YuNet/tract wasm is linked (owner decision §12).
 */
/* eslint-disable no-restricted-globals */

function isSkin(r, g, b) {
  if (r < 60 || g < 30 || b < 15) return false;
  if (r < g || r < b) return false;
  if (Math.abs(r - g) < 12 && b > g * 0.85) return false;
  return r - g > 8 || r - b > 15;
}

function skinDensity(skin, w, x0, y0, bw, bh) {
  let n = 0;
  let hit = 0;
  for (let y = y0; y < y0 + bh; y++) {
    for (let x = x0; x < x0 + bw; x++) {
      n++;
      hit += skin[y * w + x];
    }
  }
  return n ? hit / n : 0;
}

/** @param {Uint8ClampedArray|Uint8Array} pixels @param {number} width @param {number} height */
function detectHeuristic(pixels, width, height) {
  if (width < 16 || height < 16 || pixels.length < 64) return [];
  const scale = Math.min(1, 128 / Math.max(width, height));
  const dw = Math.max(8, Math.round(width * scale));
  const dh = Math.max(8, Math.round(height * scale));
  const skin = new Uint8Array(dw * dh);
  for (let y = 0; y < dh; y++) {
    for (let x = 0; x < dw; x++) {
      const sx = Math.min(width - 1, Math.floor((x + 0.5) * width / dw));
      const sy = Math.min(height - 1, Math.floor((y + 0.5) * height / dh));
      const i = (sy * width + sx) * 4;
      skin[y * dw + x] = isSkin(pixels[i], pixels[i + 1], pixels[i + 2]) ? 1 : 0;
    }
  }
  let best = null;
  let minS = Math.max(12, Math.floor(Math.min(dw, dh) / 4));
  let maxS = Math.floor((Math.min(dw, dh) * 9) / 10);
  for (let s = minS; s <= maxS; s = s + Math.floor(s / 4) + 4) {
    const step = Math.max(2, Math.floor(s / 6));
    for (let y = 0; y + s <= dh; y += step) {
      for (let x = 0; x + s <= dw; x += step) {
        const dens = skinDensity(skin, dw, x, y, s, s);
        if (dens <= 0.35) continue;
        const upper = skinDensity(skin, dw, x, y, s, Math.floor(s / 2));
        const score = dens * 0.6 + upper * 0.4;
        if (!best || score > best.score) best = { score, x, y, s };
      }
    }
  }
  if (!best) return [];
  const inv = 1 / scale;
  const bx = best.x * inv;
  const by = best.y * inv;
  const bs = best.s * inv;
  const landmarks = [
    { x: bx + bs * 0.32, y: by + bs * 0.38 },
    { x: bx + bs * 0.68, y: by + bs * 0.38 },
    { x: bx + bs * 0.5, y: by + bs * 0.52 },
    { x: bx + bs * 0.38, y: by + bs * 0.72 },
    { x: bx + bs * 0.62, y: by + bs * 0.72 },
  ];
  return [
    {
      bbox: [bx, by, bs, bs],
      landmarks,
      score: best.score,
    },
  ];
}

self.onmessage = (ev) => {
  const { id, cmd } = ev.data || {};
  if (cmd === "ping") {
    self.postMessage({ id, ok: true, cmd: "pong" });
    return;
  }
  if (cmd === "detect") {
    const { width, height, rgba } = ev.data;
    try {
      const pixels =
        rgba instanceof Uint8Array || rgba instanceof Uint8ClampedArray
          ? rgba
          : new Uint8Array(rgba || []);
      const faces = detectHeuristic(pixels, width | 0, height | 0);
      self.postMessage({ id, ok: true, cmd: "detect", faces });
    } catch (e) {
      self.postMessage({ id, ok: false, cmd: "detect", error: String(e) });
    }
    return;
  }
  if (cmd === "bake") {
    self.postMessage({
      id,
      ok: false,
      cmd: "bake",
      error: "wasm bake not wired; use room-cli faceswap bake",
    });
    return;
  }
  self.postMessage({ id, ok: false, error: `unknown cmd ${cmd}` });
};
