/**
 * F3 no-network smoke: face modules must not fetch remote URLs at import time.
 * Run: node --test crates/room-web/tests/face_no_network.test.mjs
 */
import { createRequire } from "node:module";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";

const here = dirname(fileURLToPath(import.meta.url));
const webRoot = join(here, "..");

test("face JS sources have no http(s) URLs", () => {
  for (const name of [
    "face-capture.js",
    "face-gallery.js",
    "face-worker.js",
    "face-share.js",
    "settings-faces.js",
  ]) {
    const src = readFileSync(join(webRoot, name), "utf8");
    assert.equal(
      /https?:\/\//i.test(src),
      false,
      `${name} must not embed network URLs`,
    );
  }
});

test("settings defaults hide share on public lobbies", async () => {
  // Dynamic import works under node for ESM without browser APIs used at top level.
  const { DEFAULT_FACE_SETTINGS, mergeFaceSettings } = await import(
    "../settings-faces.js"
  );
  assert.equal(DEFAULT_FACE_SETTINGS.shareMyFaces, false);
  assert.equal(DEFAULT_FACE_SETTINGS.liveFaces, false);
  const m = mergeFaceSettings({ liveFaces: true });
  assert.equal(m.liveFaces, true);
  assert.equal(m.shareMyFaces, false);
});

test("face-share b64 roundtrip", async () => {
  const { rgbaToB64, b64ToRgba, buildFaceShareMsg } = await import(
    "../face-share.js"
  );
  const rgba = new Uint8Array(64 * 64 * 4);
  rgba[0] = 1;
  rgba[1] = 2;
  rgba[2] = 3;
  rgba[3] = 255;
  const back = b64ToRgba(rgbaToB64(rgba));
  assert.deepEqual(back, rgba);
  const msg = buildFaceShareMsg({ name: "x", faceId: 9n, rgba });
  assert.equal(msg.t, "face_share");
  assert.ok(msg.rgba_b64.length > 0);
});

// silence unused
void createRequire;
