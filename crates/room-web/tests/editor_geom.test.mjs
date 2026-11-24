/**
 * Map-shaping and workflow operations of the editor (`editor-geom.js`).
 * Run: node --test crates/room-web/tests/editor_geom.test.mjs
 */
import assert from "node:assert/strict";
import test from "node:test";
import {
  NO_SIDE, orderSectors, buildStairs, gradient, slopeSectors, planeZ, clearSlope, mergeSectors, splitSector,
  curveLine, rectPoints, ellipsePoints, tagLinks, findAll, replaceAll, measure, sectorEdges, isPrefab, prefabSize,
} from "../editor-geom.js";

/**
 * A map from sector polygons (clockwise, so each sector is on the right of
 * its edges). Edges two polygons share become one two-sided line.
 */
function build(polys, format = "Doom") {
  const doc = { name: "MAP01", format, vertices: [], linedefs: [], sidedefs: [], sectors: [], things: [] };
  const vid = (p) => {
    let i = doc.vertices.findIndex((v) => v.x === p[0] && v.y === p[1]);
    if (i < 0) { doc.vertices.push({ x: p[0], y: p[1] }); i = doc.vertices.length - 1; }
    return i;
  };
  polys.forEach((poly, si) => {
    doc.sectors.push({ floor_height: 0, ceiling_height: 128, floor_pic: "FLAT1", ceiling_pic: "CEIL1", light: 160, special: 0, tag: 0, floor_plane: null, ceiling_plane: null });
    poly.forEach((p, i) => {
      const a = vid(p), b = vid(poly[(i + 1) % poly.length]);
      const sd = doc.sidedefs.length;
      doc.sidedefs.push({ x_offset: 0, y_offset: 0, upper: "-", middle: "WALL", lower: "-", sector: si });
      const rev = doc.linedefs.find((l) => l.v1 === b && l.v2 === a && l.side_back === NO_SIDE);
      if (rev) {
        rev.side_back = sd;
        rev.flags = 4;
        doc.sidedefs[rev.side_front].middle = "-";
        doc.sidedefs[sd].middle = "-";
      } else {
        doc.linedefs.push({ v1: a, v2: b, flags: 1, special: 0, tag: 0, args: [0, 0, 0, 0, 0], side_front: sd, side_back: NO_SIDE, hexen: format !== "Doom" });
      }
    });
  });
  return doc;
}
const sq = (x, y, w, h = w) => [[x, y], [x, y + h], [x + w, y + h], [x + w, y]];

/** Each sector's edges must close into loops (every vertex in = out). */
function assertClosed(doc) {
  doc.sectors.forEach((_, s) => {
    const deg = new Map();
    for (const [a, b] of sectorEdges(doc, s)) {
      deg.set(a, (deg.get(a) || 0) + 1);
      deg.set(b, (deg.get(b) || 0) - 1);
    }
    for (const [v, d] of deg) assert.equal(d, 0, `sector ${s} open at vertex ${v}`);
    assert.ok(sectorEdges(doc, s).length >= 3, `sector ${s} has a boundary`);
  });
  for (const l of doc.linedefs) {
    assert.ok(l.side_front !== NO_SIDE, "every line has a front side");
    assert.ok(doc.vertices[l.v1] && doc.vertices[l.v2]);
    if (l.side_back !== NO_SIDE) assert.notEqual(l.side_front, l.side_back);
  }
}

/** Signed area of a sector (negative: clockwise, the Doom winding). */
function area(doc, s) {
  let a = 0;
  for (const [i, j] of sectorEdges(doc, s)) {
    const p = doc.vertices[i], q = doc.vertices[j];
    a += p.x * q.y - q.x * p.y;
  }
  return a / 2;
}

test("sectors are walked in neighbour order however they were picked", () => {
  const doc = build([sq(0, 0, 64), sq(64, 0, 64), sq(128, 0, 64), sq(192, 0, 64)]);
  assert.deepEqual(orderSectors(doc, [0, 3, 1, 2]), [0, 1, 2, 3]);
  // From the middle: the neighbour picked earlier first, then the rest.
  assert.deepEqual(orderSectors(doc, [2, 0, 3, 1]), [2, 3, 0, 1]);
  assert.deepEqual(orderSectors(doc, [3, 1, 0]), [3, 1, 0], "a gap falls back to pick order");
});

test("stairs step each sector from the first", () => {
  const doc = build([sq(0, 0, 64), sq(64, 0, 64), sq(128, 0, 64)]);
  doc.sectors[0].floor_height = 16;
  buildStairs(doc, [0, 1, 2], 8, "both");
  assert.deepEqual(doc.sectors.map((s) => s.floor_height), [16, 24, 32]);
  assert.deepEqual(doc.sectors.map((s) => s.ceiling_height), [128, 136, 144]);
  buildStairs(doc, [2, 1, 0], -24, "floor");
  assert.deepEqual(doc.sectors.map((s) => s.floor_height), [-16, 8, 32]);
});

test("gradients blend from the first sector to the last", () => {
  const doc = build([sq(0, 0, 64), sq(64, 0, 64), sq(128, 0, 64), sq(192, 0, 64), sq(256, 0, 64)]);
  doc.sectors[0].light = 96;
  doc.sectors[4].light = 255;
  doc.sectors[4].ceiling_height = 256;
  gradient(doc, [0, 1, 2, 3, 4], "light");
  gradient(doc, [0, 1, 2, 3, 4], "ceiling_height");
  assert.deepEqual(doc.sectors.map((s) => s.light), [96, 136, 176, 215, 255]);
  assert.deepEqual(doc.sectors.map((s) => s.ceiling_height), [128, 160, 192, 224, 256]);
  assert.equal(gradient(doc, [0, 1], "light"), 0, "two sectors have nothing in between");
});

test("a slope is level on its hinge and rises to the far side", () => {
  const doc = build([sq(0, 0, 128), sq(128, 0, 128)]);
  doc.sectors[0].floor_height = 32;
  // Hinge: the left wall of sector 0 (x = 0); slope both sectors as one ramp.
  const hinge = doc.linedefs.findIndex((l) => doc.vertices[l.v1].x === 0 && doc.vertices[l.v2].x === 0);
  assert.ok(slopeSectors(doc, [0, 1], "floor", hinge, 64));
  for (const s of [0, 1]) {
    assert.ok(Math.abs(planeZ(doc.sectors[s], "floor", 0, 50) - 32) < 1e-9);
    assert.ok(Math.abs(planeZ(doc.sectors[s], "floor", 256, 10) - 96) < 1e-9);
    assert.ok(Math.abs(planeZ(doc.sectors[s], "floor", 128, 99) - 64) < 1e-9);
    assert.ok(doc.sectors[s].floor_plane[2] > 0, "floor faces up");
    assert.ok(Math.abs(Math.hypot(...doc.sectors[s].floor_plane.slice(0, 3)) - 1) < 1e-9, "unit normal");
  }
  assert.ok(slopeSectors(doc, [0], "ceiling", hinge, -32));
  assert.ok(doc.sectors[0].ceiling_plane[2] < 0, "ceiling faces down");
  assert.ok(Math.abs(planeZ(doc.sectors[0], "ceiling", 0, 0) - 128) < 1e-9);
  assert.ok(Math.abs(planeZ(doc.sectors[0], "ceiling", 128, 0) - 96) < 1e-9);
  clearSlope(doc, [0], "ceiling");
  assert.equal(doc.sectors[0].ceiling_plane, null);
  assert.equal(planeZ(doc.sectors[0], "ceiling", 77, 5), 128);
});

test("merging sectors removes the lines between them", () => {
  const doc = build([sq(0, 0, 64), sq(64, 0, 64), sq(0, 64, 128, 64)]);
  const lines = doc.linedefs.length;
  const kept = mergeSectors(doc, [1, 0]);
  assert.equal(doc.sectors.length, 2);
  assert.equal(kept, 0, "sector 1 kept, renumbered to 0");
  assert.equal(doc.linedefs.length, lines - 1, "the shared wall went");
  assertClosed(doc);
  assert.equal(area(doc, kept), -128 * 64);
});

test("a cut corner to corner splits a sector in two", () => {
  const doc = build([sq(0, 0, 128)]);
  const r = splitSector(doc, { x: 0, y: 0 }, { x: 128, y: 128 });
  assert.equal(typeof r, "object", String(r));
  assert.equal(doc.sectors.length, 2);
  assertClosed(doc);
  assert.equal(area(doc, 0), -8192);
  assert.equal(area(doc, 1), -8192);
  const cut = doc.linedefs[r.line];
  assert.equal(doc.sidedefs[cut.side_front].sector, r.newSector, "new part on the cut's right");
  assert.equal(doc.sidedefs[cut.side_back].sector, r.sector);
  assert.equal(cut.flags, 4, "two-sided");
});

test("a cut between two walls splits them and the sector", () => {
  const doc = build([sq(0, 0, 128), sq(128, 0, 128)]);
  const r = splitSector(doc, { x: 0, y: 32 }, { x: 128, y: 32 });
  assert.equal(typeof r, "object", String(r));
  assert.equal(doc.sectors.length, 3);
  assertClosed(doc);
  assert.equal(area(doc, 0) + area(doc, r.newSector), -128 * 128);
  assert.deepEqual([area(doc, 0), area(doc, r.newSector)].sort((a, b) => a - b), [-96 * 128, -32 * 128]);
  // Sector 1 gained a vertex on its shared wall but kept its area.
  assert.equal(area(doc, 1), -128 * 128);
});

test("a hole stays with the part it is in", () => {
  // Room with a pillar (a hole: an inner loop run the other way).
  const doc = build([sq(0, 0, 256)]);
  const pillar = [[160, 32], [192, 32], [192, 64], [160, 64]]; // counter-clockwise: room on the right
  const ids = pillar.map(([x, y]) => { doc.vertices.push({ x, y }); return doc.vertices.length - 1; });
  ids.forEach((a, i) => {
    doc.sidedefs.push({ x_offset: 0, y_offset: 0, upper: "-", middle: "WALL", lower: "-", sector: 0 });
    doc.linedefs.push({ v1: a, v2: ids[(i + 1) % 4], flags: 1, special: 0, tag: 0, args: [0, 0, 0, 0, 0], side_front: doc.sidedefs.length - 1, side_back: NO_SIDE });
  });
  const r = splitSector(doc, { x: 128, y: 0 }, { x: 128, y: 256 });
  assert.equal(typeof r, "object", String(r));
  assertClosed(doc);
  // The cut runs north at x = 128: its right (east) is the new sector, with the pillar.
  const pillarLine = doc.linedefs.find((l) => doc.vertices[l.v1].x === 160 && doc.vertices[l.v1].y === 32);
  assert.equal(doc.sidedefs[pillarLine.side_front].sector, r.newSector);
  assert.equal(area(doc, r.newSector), -128 * 256 + 32 * 32);
  assert.equal(area(doc, r.sector), -128 * 256);
});

test("cuts that cannot split are refused and change nothing", () => {
  const doc = build([sq(0, 0, 128)]);
  const before = JSON.stringify(doc);
  assert.equal(typeof splitSector(doc, { x: 30, y: 30 }, { x: 128, y: 128 }), "string");
  assert.equal(JSON.stringify(doc), before);
  assert.equal(typeof splitSector(doc, { x: 0, y: 0 }, { x: 0, y: 128 }), "string", "along an existing wall");
  assert.equal(JSON.stringify(doc), before);
});

test("a curved line keeps its ends and bends to the chosen side", () => {
  const doc = build([sq(0, 0, 128)]);
  const li = doc.linedefs.findIndex((l) => doc.vertices[l.v1].x === 0 && doc.vertices[l.v2].x === 0); // (0,0)→(0,128)
  const lines = curveLine(doc, li, 8, 32);
  assert.equal(lines.length, 8);
  const chain = lines.map((i) => doc.linedefs[i]);
  assert.deepEqual(doc.vertices[chain[0].v1], { x: 0, y: 0 });
  assert.deepEqual(doc.vertices[chain.at(-1).v2], { x: 0, y: 128 });
  for (let i = 1; i < chain.length; i++) assert.equal(chain[i].v1, chain[i - 1].v2, "a connected chain");
  // Going north, left is west: the bulge is at x ≈ −32 in the middle.
  const mid = doc.vertices[chain[3].v2];
  assert.ok(mid.x <= -30 && mid.x >= -33, `middle point ${JSON.stringify(mid)}`);
  assertClosed(doc);
  const other = build([sq(0, 0, 128)]);
  const li2 = other.linedefs.findIndex((l) => other.vertices[l.v1].x === 0 && other.vertices[l.v2].x === 0);
  curveLine(other, li2, 4, -16);
  assert.ok(other.vertices.some((v) => v.x >= 15 && v.x <= 17 && v.y === 64), "negative bulge goes right");
});

test("shapes come out clockwise", () => {
  const signed = (pts) => pts.reduce((a, p, i) => { const q = pts[(i + 1) % pts.length]; return a + p.x * q.y - q.x * p.y; }, 0);
  const r = rectPoints({ x: 64, y: 0 }, { x: 0, y: 32 });
  assert.equal(r.length, 4);
  assert.ok(signed(r) < 0);
  const c = ellipsePoints({ x: 0, y: 0 }, { x: 256, y: 128 }, 16);
  assert.equal(c.length, 16);
  assert.ok(signed(c) < 0);
  for (const p of c) assert.ok(Math.abs(((p.x - 128) / 128) ** 2 + ((p.y - 64) / 64) ** 2 - 1) < 0.05);
  assert.deepEqual(rectPoints({ x: 0, y: 0 }, { x: 0, y: 9 }), []);
});

test("Doom tags link switches, sectors and teleport destinations both ways", () => {
  const doc = build([sq(0, 0, 128), sq(128, 0, 64), sq(512, 0, 64)]);
  const sw = doc.linedefs.findIndex((l) => l.side_back === NO_SIDE);
  doc.linedefs[sw].special = 103; // S1 Door open stay
  doc.linedefs[sw].tag = 7;
  doc.sectors[1].tag = 7;
  let r = tagLinks(doc, "lines", new Set([sw]));
  assert.deepEqual([...r.sectors], [1]);
  assert.equal(r.arrows.length, 1);
  r = tagLinks(doc, "sectors", new Set([1]));
  assert.deepEqual([...r.lines], [sw], "the sector shows what opens it");
  // A teleporter into sector 2 with its destination thing.
  const tele = doc.linedefs.findIndex((l, i) => i !== sw && l.side_back === NO_SIDE);
  doc.linedefs[tele].special = 97;
  doc.linedefs[tele].tag = 9;
  doc.sectors[2].tag = 9;
  doc.things.push({ x: 540, y: 30, type_id: 14, angle: 0, tid: 0, flags: 7, special: 0, args: [0, 0, 0, 0, 0] });
  r = tagLinks(doc, "lines", new Set([tele]));
  assert.deepEqual([...r.things], [0]);
  assert.ok(r.arrows.some((a) => a.teleport && a.to.x === 540));
  r = tagLinks(doc, "things", new Set([0]));
  assert.deepEqual([...r.lines], [tele], "the destination shows its teleporter");
  assert.equal(tagLinks(doc, "lines", new Set()).arrows.length, 0);
});

test("Hexen specials link by tag, TID, line id and polyobject id", () => {
  const doc = build([sq(0, 0, 128), sq(128, 0, 64)], "Hexen");
  const walls = doc.linedefs.map((l, i) => i).filter((i) => doc.linedefs[i].side_back === NO_SIDE);
  const [door, tele, scroll, poly] = walls;
  Object.assign(doc.linedefs[door], { special: 12, args: [3, 16, 150, 0, 0] }); // Door_Raise(tag 3)
  doc.sectors[1].tag = 3;
  Object.assign(doc.linedefs[tele], { special: 70, args: [44, 0, 0, 0, 0] }); // Teleport(tid 44)
  doc.things.push({ x: 150, y: 20, type_id: 9044, angle: 0, tid: 44, flags: 7, special: 0, args: [0, 0, 0, 0, 0] });
  Object.assign(doc.linedefs[scroll], { special: 121, args: [12, 0, 0, 0, 0] }); // Line_SetIdentification(12)
  doc.things.push({ x: 10, y: 10, type_id: 9001, angle: 0, tid: 0, flags: 7, special: 52, args: [12, 64, 0, 0, 0] }); // Scroll_Wall(line 12)
  Object.assign(doc.linedefs[poly], { special: 1, args: [5, 0, 0, 0, 0] }); // Polyobj_StartLine(5)
  doc.things.push({ x: 60, y: 60, type_id: 9301, angle: 5, tid: 0, flags: 7, special: 0, args: [0, 0, 0, 0, 0] });
  assert.deepEqual([...tagLinks(doc, "lines", new Set([door])).sectors], [1]);
  assert.deepEqual([...tagLinks(doc, "lines", new Set([tele])).things], [0]);
  assert.deepEqual([...tagLinks(doc, "things", new Set([0])).lines], [tele]);
  assert.deepEqual([...tagLinks(doc, "things", new Set([1])).lines], [scroll], "a thing's special names a line id");
  assert.deepEqual([...tagLinks(doc, "lines", new Set([poly])).things].sort(), [2], "start line ↔ start spot");
});

test("find and replace textures, flats, things and specials", () => {
  const doc = build([sq(0, 0, 64), sq(64, 0, 64)]);
  doc.sectors[1].floor_pic = "NUKAGE1";
  doc.things.push({ x: 1, y: 1, type_id: 3004, tid: 0 }, { x: 2, y: 2, type_id: 9, tid: 0 }, { x: 3, y: 3, type_id: 3004, tid: 0 });
  assert.equal(findAll(doc, "texture", "wall").length, 6, "case-insensitive");
  assert.equal(replaceAll(doc, "texture", "WALL", "brick1"), 6);
  assert.equal(findAll(doc, "texture", "BRICK1").length, 6);
  assert.deepEqual(findAll(doc, "flat", "nukage1"), [1]);
  assert.equal(replaceAll(doc, "flat", "FLAT1", "FLOOR4_8"), 1, "only the fields that match");
  assert.deepEqual(doc.sectors.map((s) => [s.floor_pic, s.ceiling_pic]), [["FLOOR4_8", "CEIL1"], ["NUKAGE1", "CEIL1"]]);
  assert.deepEqual(findAll(doc, "thing", "3004"), [0, 2]);
  assert.equal(replaceAll(doc, "thing", "3004", "3001"), 2);
  assert.deepEqual(doc.things.map((t) => t.type_id), [3001, 9, 3001]);
  assert.equal(replaceAll(doc, "thing", "", "1"), 0);
});

test("prefab and measure helpers", () => {
  assert.ok(!isPrefab({ name: "x" }));
  const clip = { kind: "sectors", verts: [{ x: 0, y: 0 }, { x: 64, y: 32 }], lines: [], things: [{ x: 80, y: 10 }], sectors: [] };
  assert.ok(isPrefab({ name: "door", clip }));
  assert.deepEqual(prefabSize(clip), [80, 32]);
  const m = measure({ x: 0, y: 0 }, { x: 30, y: 40 });
  assert.equal(m.dist, 50);
  assert.ok(Math.abs(m.angle - 53.13) < 0.01);
  assert.equal(measure({ x: 0, y: 0 }, { x: 0, y: -5 }).angle, 270);
});

test("prefab files round-trip and merge without clobbering names", async () => {
  const { prefabsToFile, mergePrefabs, prefabOutline } = await import("../editor-geom.js");
  const clip = {
    kind: "sectors", sectors: [{}], things: [{ x: 32, y: 32, type_id: 11 }],
    verts: [{ x: 0, y: 0 }, { x: 0, y: 64 }, { x: 128, y: 64 }, { x: 128, y: 0 }],
    lines: [0, 1, 2, 3].map((i) => ({ line: { v1: i, v2: (i + 1) % 4 }, front: {}, back: i === 0 ? {} : null })),
  };
  const door = { name: "door", clip, time: 1 };
  const text = prefabsToFile([door, { name: "bad" }]);
  const r = mergePrefabs([{ name: "door", clip, time: 5 }], text);
  assert.deepEqual(r.added, ["door (2)"]);
  assert.equal(r.skipped, 1);
  assert.equal(r.list.length, 2);
  assert.equal(r.list[0].name, "door (2)");
  // A single prefab object and a bare array are accepted too.
  assert.deepEqual(mergePrefabs([], JSON.stringify(door)).added, ["door"]);
  assert.deepEqual(mergePrefabs([], JSON.stringify([door, door])).added, ["door", "door (2)"]);
  assert.throws(() => mergePrefabs([], "{nope"), /invalid JSON/);
  assert.throws(() => mergePrefabs([], '{"format":"other"}'), /not a prefab file/);
  // Outline: fitted in the box, y flipped, two-sided flag and things kept.
  const o = prefabOutline(clip, 56, 4);
  assert.equal(o.segs.length, 4);
  // Box 0..128 × 0..64 at 48/128 per unit: (0,0) → (4,28), (0,64) → (4,4).
  assert.deepEqual(o.segs[0], [4, 28, 4, 4, true]);
  assert.deepEqual(o.dots, [[16, 16, 11]]);
  assert.ok(o.segs.flat().filter((v) => typeof v === "number").every((v) => v >= 4 && v <= 52));
});
