/**
 * Portal / UDMF tables of the map editor stay in step with the engine.
 * Run: node --test crates/room-web/tests/editor_portals.test.mjs
 */
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import {
  HEXEN_LINE_SPECIALS, LINE_EXT_FLAGS, SECTOR_EXT_FLAGS, SECTOR_PORTAL_FLAGS, SPECIAL_ARGS,
  THING_EXT_FLAGS, THING_TYPE_HINTS,
} from "../editor-tools.js";

const udmf = readFileSync(new URL("../../room-map/src/udmf.rs", import.meta.url), "utf8");
const portals = readFileSync(new URL("../../room-map/src/portals.rs", import.meta.url), "utf8");
const rustConst = (src, name) => {
  const m = new RegExp(`pub const ${name}: \\w+ = (\\d+);`).exec(src);
  assert.ok(m, `missing ${name}`);
  return Number(m[1]);
};

test("ext flag bits match room-map udmf::ext", () => {
  const lineNames = ["BLOCKUSE", "PASSUSE", "BLOCKLANDMONSTERS", "MONSTERACTIVATE", "JUMPOVER", "DRAWFULLHEIGHT", "NOSKYWALLS", "ZONEBOUNDARY"];
  assert.deepEqual(LINE_EXT_FLAGS.map(([b]) => b), lineNames.map((n) => rustConst(udmf, `LINE_${n}`)));
  const secNames = ["NOATTACK", "NORESPAWN", "HURTMONSTERS", "HARMINAIR", "DAMAGEHAZARD", "HIDDEN", "NOSKYWALLS"];
  assert.deepEqual(SECTOR_EXT_FLAGS.map(([b]) => b), secNames.map((n) => rustConst(udmf, `SECTOR_${n}`)));
  for (const [plane, tag] of [["floor", "FLOOR"], ["ceiling", "CEIL"]]) {
    const want = ["DISABLED", "NORENDER", "NOPASS", "BLOCKSOUND"].map((n) => rustConst(udmf, `SECTOR_PORTAL_${tag}_${n}`));
    assert.deepEqual(SECTOR_PORTAL_FLAGS[plane].map(([b]) => b), want);
  }
  const thingNames = ["STANDING", "STRIFEALLY", "INVISIBLE", "TRANSLUCENT", "NOTCOOP", "NOTDM"];
  assert.deepEqual(THING_EXT_FLAGS.map(([b]) => b), thingNames.map((n) => rustConst(udmf, `THING_${n}`)));
});

test("portal specials are named and have five argument slots", () => {
  for (const n of ["LINE_SET_PORTAL", "LINE_QUICK_PORTAL", "SECTOR_SET_PORTAL", "LINE_HORIZON", "LINE_MIRROR"]) {
    const num = rustConst(portals, n);
    assert.ok(HEXEN_LINE_SPECIALS[num], `special ${num} unnamed`);
    assert.equal(SPECIAL_ARGS[num]?.length, 5, `special ${num} args`);
  }
  for (const [num, args] of Object.entries(SPECIAL_ARGS)) {
    assert.ok(HEXEN_LINE_SPECIALS[num], `SPECIAL_ARGS ${num} is not a known special`);
    assert.equal(args.length, 5);
    for (const a of args) if (a) assert.equal(typeof a[0], "string");
  }
  // Line_SetPortal: arg0 destination id, arg2 type with the engine's PORTT_* values.
  const types = SPECIAL_ARGS[156][2][1];
  for (const [k, name] of [["PORTT_VISUAL", "Visual"], ["PORTT_TELEPORT", "Teleport"], ["PORTT_INTERACTIVE", "Interactive"]]) {
    assert.equal(types[rustConst(portals, k)], name);
  }
});

test("portal and skybox things have hints", () => {
  for (const n of ["THING_UPPER_STACK", "THING_LOWER_STACK", "THING_SKY_VIEWPOINT", "THING_SKY_PICKER", "THING_SKY_CAM_COMPAT", "THING_SKY_CAM_COMPAT_EE"]) {
    assert.ok(THING_TYPE_HINTS[rustConst(portals, n)], n);
  }
  for (const n of [9300, 9301, 9302, 9303]) assert.ok(THING_TYPE_HINTS[n]);
});
