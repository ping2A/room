// Map-document operations for the editor's shaping and workflow tools.
//
// Everything here works on the engine's `RawMap` JSON (`S.doc`) and nothing
// else: no DOM, no engine calls, so `tests/editor_geom.test.mjs` runs it in
// Node. Indices are array positions; `NO_SIDE` (0xffff) is "no sidedef".

export const NO_SIDE = 0xffff;

const clampI16 = (v) => Math.max(-32768, Math.min(32767, Math.round(v)));
const sideOf = (doc, i) => (i === NO_SIDE || i == null ? null : doc.sidedefs[i] || null);
const isHexenLike = (doc) => doc.format === "Hexen" || doc.format === "Udmf";

// ------------------------------------------------------------------ sectors

/** Sector `sec`'s boundary as directed edges `[from, to, line, sideKey]`, sector on the right. */
export function sectorEdges(doc, sec) {
  const out = [];
  doc.linedefs.forEach((l, li) => {
    const f = sideOf(doc, l.side_front), b = sideOf(doc, l.side_back);
    if (f && f.sector === sec) out.push([l.v1, l.v2, li, "side_front"]);
    if (b && b.sector === sec) out.push([l.v2, l.v1, li, "side_back"]);
  });
  return out;
}

/** Sectors on both sides of a two-sided line: neighbour lists per sector. */
export function sectorNeighbours(doc) {
  const n = new Map();
  for (const l of doc.linedefs) {
    const f = sideOf(doc, l.side_front), b = sideOf(doc, l.side_back);
    if (!f || !b || f.sector === b.sector) continue;
    for (const [x, y] of [[f.sector, b.sector], [b.sector, f.sector]]) {
      if (!n.has(x)) n.set(x, new Set());
      n.get(x).add(y);
    }
  }
  return n;
}

/** Centre of a sector's boundary vertices (good enough for labels and arrows). */
export function sectorCenter(doc, sec) {
  let x = 0, y = 0, k = 0;
  for (const [a] of sectorEdges(doc, sec)) {
    const v = doc.vertices[a];
    if (v) { x += v.x; y += v.y; k++; }
  }
  return k ? { x: x / k, y: y / k } : null;
}

/**
 * The selected sectors in walking order: from the first one picked, each
 * next sector is an unvisited selected neighbour of the last (a corridor of
 * steps in order, however they were clicked); a gap falls back to pick order.
 */
export function orderSectors(doc, picked) {
  const want = [...picked];
  const left = new Set(want);
  const nb = sectorNeighbours(doc);
  const out = [];
  let cur = want[0];
  while (left.size) {
    if (cur == null || !left.has(cur)) cur = want.find((s) => left.has(s));
    out.push(cur);
    left.delete(cur);
    const next = [...(nb.get(cur) || [])].filter((s) => left.has(s));
    // Several open neighbours: the one picked earliest.
    next.sort((a, b) => want.indexOf(a) - want.indexOf(b));
    cur = next[0];
  }
  return out;
}

/**
 * Stairs: each sector in `order` is `step` above the previous one, starting
 * from the first sector's height. `what` is "floor", "ceiling" or "both".
 */
export function buildStairs(doc, order, step, what = "floor") {
  if (order.length < 2) return 0;
  const first = doc.sectors[order[0]];
  const f0 = first.floor_height, c0 = first.ceiling_height;
  order.forEach((si, i) => {
    const s = doc.sectors[si];
    if (what === "floor" || what === "both") s.floor_height = clampI16(f0 + step * i);
    if (what === "ceiling" || what === "both") s.ceiling_height = clampI16(c0 + step * i);
  });
  return order.length;
}

/** Linear blend of `key` (light, floor_height, ceiling_height) from the first sector to the last. */
export function gradient(doc, order, key) {
  if (order.length < 3) return 0;
  const a = doc.sectors[order[0]][key], b = doc.sectors[order.at(-1)][key];
  order.forEach((si, i) => {
    const v = a + ((b - a) * i) / (order.length - 1);
    doc.sectors[si][key] = key === "light" ? Math.max(0, Math.min(255, Math.round(v))) : clampI16(v);
  });
  return order.length;
}

/**
 * Slope the floor or ceiling of `secs` as one plane: level with the sector's
 * height along the hinge line, `rise` units higher at the selection's point
 * farthest from the hinge (on the side away from it). UDMF `floorplane` /
 * `ceilingplane`: `a·x + b·y + c·z + d = 0`, floors facing up, ceilings down.
 */
export function slopeSectors(doc, secs, plane, hingeLine, rise) {
  const l = doc.linedefs[hingeLine];
  if (!l || !secs.length) return false;
  const a = doc.vertices[l.v1], b = doc.vertices[l.v2];
  const len = Math.hypot(b.x - a.x, b.y - a.y);
  if (!len) return false;
  // Unit normal of the hinge pointing into the selection.
  let ux = -(b.y - a.y) / len, uy = (b.x - a.x) / len;
  let far = 0, nearSum = 0, nPts = 0;
  for (const s of secs) {
    for (const [v] of sectorEdges(doc, s)) {
      const p = doc.vertices[v];
      const t = (p.x - a.x) * ux + (p.y - a.y) * uy;
      nearSum += t; nPts++;
      if (Math.abs(t) > Math.abs(far)) far = t;
    }
  }
  if (!nPts || Math.abs(far) < 1e-9) return false;
  if (far < 0) { ux = -ux; uy = -uy; far = -far; }
  const key = plane === "ceiling" ? "ceiling_height" : "floor_height";
  const h0 = doc.sectors[secs[0]][key];
  const k = rise / far;
  // z = h0 + k·((x − ax)·ux + (y − ay)·uy)
  let pa = -k * ux, pb = -k * uy, pc = 1, pd = -(h0 - k * (a.x * ux + a.y * uy));
  const norm = Math.hypot(pa, pb, pc);
  [pa, pb, pc, pd] = [pa / norm, pb / norm, pc / norm, pd / norm];
  if (plane === "ceiling") [pa, pb, pc, pd] = [-pa, -pb, -pc, -pd];
  for (const s of secs) {
    doc.sectors[s][plane === "ceiling" ? "ceiling_plane" : "floor_plane"] = [pa, pb, pc, pd];
    doc.sectors[s][key] = clampI16(h0);
  }
  return true;
}

/** Height of a sloped (or flat) plane at `x, y`. */
export function planeZ(sector, plane, x, y) {
  const p = sector[plane === "ceiling" ? "ceiling_plane" : "floor_plane"];
  if (!p || !p[2]) return sector[plane === "ceiling" ? "ceiling_height" : "floor_height"];
  return -(p[0] * x + p[1] * y + p[3]) / p[2];
}

export function clearSlope(doc, secs, plane) {
  for (const s of secs) doc.sectors[s][plane === "ceiling" ? "ceiling_plane" : "floor_plane"] = null;
}

/** Drop unused sidedefs and vertices (renumbering what lines use). */
export function compact(doc) {
  const sideMap = new Map(), sides = [];
  const vMap = new Map(), verts = [];
  for (const l of doc.linedefs) {
    for (const k of ["side_front", "side_back"]) {
      const s = l[k];
      if (s === NO_SIDE || s >= doc.sidedefs.length) { l[k] = NO_SIDE; continue; }
      if (!sideMap.has(s)) { sideMap.set(s, sides.length); sides.push(doc.sidedefs[s]); }
      l[k] = sideMap.get(s);
    }
    for (const k of ["v1", "v2"]) {
      const v = l[k];
      if (!vMap.has(v)) { vMap.set(v, verts.length); verts.push(doc.vertices[v]); }
      l[k] = vMap.get(v);
    }
  }
  doc.sidedefs = sides;
  doc.vertices = verts;
}

/** Remove sectors `gone` (no sidedef may still use them) and renumber the rest. */
function dropSectors(doc, gone) {
  const remap = [];
  let n = 0;
  doc.sectors.forEach((_, i) => { remap[i] = gone.has(i) ? -1 : n++; });
  doc.sectors = doc.sectors.filter((_, i) => !gone.has(i));
  for (const s of doc.sidedefs) s.sector = Math.max(0, remap[s.sector] ?? 0);
  return remap;
}

/**
 * Merge `secs` into the first of them: lines between two merged sectors go,
 * every other side of a merged sector joins the kept one. Returns the kept
 * sector's new index.
 */
export function mergeSectors(doc, secs) {
  if (secs.length < 2) return secs[0] ?? -1;
  const keep = secs[0];
  const set = new Set(secs);
  doc.linedefs = doc.linedefs.filter((l) => {
    const f = sideOf(doc, l.side_front), b = sideOf(doc, l.side_back);
    return !(f && b && set.has(f.sector) && set.has(b.sector));
  });
  compact(doc);
  for (const s of doc.sidedefs) if (set.has(s.sector)) s.sector = keep;
  const gone = new Set(secs.slice(1));
  const remap = dropSectors(doc, gone);
  return remap[keep];
}

// ------------------------------------------------------------------ lines

/** Split line `li` at `(x, y)`; returns the new vertex index. */
export function splitLineAt(doc, li, x, y) {
  const l = doc.linedefs[li];
  const v = doc.vertices.length;
  doc.vertices.push({ x: clampI16(x), y: clampI16(y) });
  const copy = structuredClone(l);
  copy.v1 = v;
  for (const k of ["side_front", "side_back"]) {
    if (l[k] !== NO_SIDE) { copy[k] = doc.sidedefs.length; doc.sidedefs.push(structuredClone(doc.sidedefs[l[k]])); }
  }
  l.v2 = v;
  doc.linedefs.splice(li + 1, 0, copy);
  return v;
}

/** Vertex at `p` (exact), or the line `p` lies on (within `tol`), or nothing. */
export function snapToBoundary(doc, p, tol = 0.5) {
  const v = doc.vertices.findIndex((q) => Math.abs(q.x - p.x) <= tol && Math.abs(q.y - p.y) <= tol);
  if (v >= 0) return { vertex: v };
  for (let li = 0; li < doc.linedefs.length; li++) {
    const l = doc.linedefs[li];
    const a = doc.vertices[l.v1], b = doc.vertices[l.v2];
    const dx = b.x - a.x, dy = b.y - a.y, len2 = dx * dx + dy * dy;
    if (!len2) continue;
    const t = ((p.x - a.x) * dx + (p.y - a.y) * dy) / len2;
    if (t <= 0 || t >= 1) continue;
    const qx = a.x + t * dx, qy = a.y + t * dy;
    if (Math.hypot(qx - p.x, qy - p.y) <= tol) return { line: li, x: qx, y: qy };
  }
  return null;
}

/**
 * Faces of sector `sec` (with `extra` directed edges added): closed loops of
 * edges `[from, to, ...]`, each keeping the sector on its right.
 */
function traceLoops(doc, edges) {
  const out = new Map();
  edges.forEach((e, i) => {
    if (!out.has(e[0])) out.set(e[0], []);
    out.get(e[0]).push(i);
  });
  const ang = (e) => {
    const a = doc.vertices[e[0]], b = doc.vertices[e[1]];
    return Math.atan2(b.y - a.y, b.x - a.x);
  };
  const used = new Uint8Array(edges.length);
  const loops = [];
  for (let s = 0; s < edges.length; s++) {
    if (used[s]) continue;
    const loop = [];
    let i = s;
    for (let guard = 0; guard <= edges.length && !used[i]; guard++) {
      used[i] = 1;
      loop.push(i);
      const e = edges[i];
      // Face on the right: take the sharpest right turn at the next vertex,
      // the outgoing edge nearest counter-clockwise from the way back (going
      // straight back comes last).
      const back = ang(e) + Math.PI;
      let best = -1, bestTurn = Infinity;
      for (const j of out.get(e[1]) || []) {
        let turn = ang(edges[j]) - back;
        while (turn <= 1e-9) turn += 2 * Math.PI;
        while (turn > 2 * Math.PI + 1e-9) turn -= 2 * Math.PI;
        if (turn < bestTurn) { bestTurn = turn; best = j; }
      }
      if (best < 0) break;
      i = best;
    }
    loops.push(loop);
  }
  return loops;
}

const loopArea = (doc, edges, loop) => {
  let a = 0;
  for (const i of loop) {
    const p = doc.vertices[edges[i][0]], q = doc.vertices[edges[i][1]];
    a += p.x * q.y - q.x * p.y;
  }
  return a / 2;
};

const insideLoop = (doc, edges, loop, pt) => {
  let inside = false;
  for (const i of loop) {
    const a = doc.vertices[edges[i][0]], b = doc.vertices[edges[i][1]];
    if ((a.y > pt.y) !== (b.y > pt.y) && pt.x < ((b.x - a.x) * (pt.y - a.y)) / (b.y - a.y) + a.x) inside = !inside;
  }
  return inside;
};

/**
 * Split a sector with a straight cut from `p` to `q`, both on its boundary
 * (a vertex, or a point on one of its lines, which is split there). The part
 * on the right of `p→q` becomes a new sector (a copy of the old one).
 * Returns `{ sector, newSector, line }` or an error string.
 */
export function splitSector(doc, p, q, texture = "-") {
  const at = (pt) => {
    const hit = snapToBoundary(doc, pt);
    if (!hit) return null;
    if (hit.vertex != null) return hit.vertex;
    return splitLineAt(doc, hit.line, hit.x, hit.y);
  };
  const before = structuredClone(doc);
  const restore = (msg) => { Object.assign(doc, before); return msg; };
  const va = at(p);
  if (va == null) return restore("The cut must start on a sector's edge or corner");
  const vb = at(q);
  if (vb == null) return restore("The cut must end on a sector's edge or corner");
  if (va === vb) return restore("The cut needs two different points");
  // The sector both ends touch whose inside the cut's midpoint lies in.
  const mid = { x: (doc.vertices[va].x + doc.vertices[vb].x) / 2, y: (doc.vertices[va].y + doc.vertices[vb].y) / 2 };
  let sec = -1;
  for (let s = 0; s < doc.sectors.length && sec < 0; s++) {
    const edges = sectorEdges(doc, s);
    const touches = (v) => edges.some((e) => e[0] === v || e[1] === v);
    if (!touches(va) || !touches(vb)) continue;
    const loops = traceLoops(doc, edges);
    let inside = false;
    for (const lp of loops) if (insideLoop(doc, edges, lp, mid)) inside = !inside;
    if (inside) sec = s;
  }
  if (sec < 0) return restore("Both ends must be on the same sector, with the cut inside it");
  if (doc.linedefs.some((l) => (l.v1 === va && l.v2 === vb) || (l.v1 === vb && l.v2 === va))) {
    return restore("There is already a line there");
  }
  const edges = sectorEdges(doc, sec);
  const cutAB = edges.length, cutBA = edges.length + 1;
  edges.push([va, vb, -1, "cut"], [vb, va, -1, "cut"]);
  const loops = traceLoops(doc, edges);
  const withAB = loops.find((lp) => lp.includes(cutAB));
  const withBA = loops.find((lp) => lp.includes(cutBA));
  if (!withAB || !withBA || withAB === withBA) return restore("The cut does not divide the sector in two");
  // Holes (loops without the cut) go with the part they sit in.
  const newSec = doc.sectors.length;
  doc.sectors.push(structuredClone(doc.sectors[sec]));
  const toNew = new Set(withAB);
  for (const lp of loops) {
    if (lp === withAB || lp === withBA) continue;
    // Outer loops run clockwise (negative area); holes counter-clockwise.
    if (loopArea(doc, edges, lp) < 0) continue;
    const v = doc.vertices[edges[lp[0]][0]];
    if (insideLoop(doc, edges, withAB, v)) lp.forEach((i) => toNew.add(i));
  }
  for (const i of toNew) {
    const [, , li, key] = edges[i];
    if (li < 0) continue;
    const sd = sideOf(doc, doc.linedefs[li][key]);
    // A line with this sector on both sides needs its own sidedef per side.
    if (sd) {
      const other = key === "side_front" ? "side_back" : "side_front";
      if (doc.linedefs[li][other] === doc.linedefs[li][key]) {
        doc.sidedefs.push(structuredClone(sd));
        doc.linedefs[li][key] = doc.sidedefs.length - 1;
      }
      doc.sidedefs[doc.linedefs[li][key]].sector = newSec;
    }
  }
  // The cut: front faces the new sector (its right side is the new part).
  const tplLine = structuredClone(doc.linedefs[0]);
  const sideTpl = structuredClone(doc.sidedefs[0]);
  const mkSide = (s) => {
    doc.sidedefs.push(Object.assign(structuredClone(sideTpl), { x_offset: 0, y_offset: 0, upper: texture, lower: texture, middle: "-", sector: s }));
    return doc.sidedefs.length - 1;
  };
  Object.assign(tplLine, {
    v1: va, v2: vb, flags: 4, special: 0, tag: 0, args: [0, 0, 0, 0, 0],
    side_front: mkSide(newSec), side_back: mkSide(sec),
  });
  if ("activation" in tplLine) tplLine.activation = 0;
  if ("extra_ids" in tplLine) tplLine.extra_ids = [];
  doc.linedefs.push(tplLine);
  return { sector: sec, newSector: newSec, line: doc.linedefs.length - 1 };
}

/**
 * Bend line `li` into an arc of `segments` pieces bulging `bulge` units to
 * its left (negative: right), like Doom Builder's curve-linedefs.
 */
export function curveLine(doc, li, segments, bulge) {
  const n = Math.max(2, Math.min(64, Math.round(segments)));
  const l = doc.linedefs[li];
  const a = { ...doc.vertices[l.v1] }, b = { ...doc.vertices[l.v2] };
  const len = Math.hypot(b.x - a.x, b.y - a.y);
  if (!len || !bulge) return [li];
  // Arc through a, b and the point `bulge` off the middle.
  const nx = -(b.y - a.y) / len, ny = (b.x - a.x) / len;
  const h = bulge, c = len / 2;
  const r = (c * c + h * h) / (2 * h);
  const cx = (a.x + b.x) / 2 + nx * (h - r), cy = (a.y + b.y) / 2 + ny * (h - r);
  const a0 = Math.atan2(a.y - cy, a.x - cx), a1 = Math.atan2(b.y - cy, b.x - cx);
  const TAU = 2 * Math.PI;
  const norm = (x) => ((x % TAU) + TAU) % TAU;
  // Counter-clockwise from a to b, unless the bulge point is on the other way.
  const ccw = norm(a1 - a0);
  const midAng = Math.atan2((a.y + b.y) / 2 + ny * h - cy, (a.x + b.x) / 2 + nx * h - cx);
  const sweep = norm(midAng - a0) < ccw ? ccw : ccw - TAU;
  const rAbs = Math.abs(r);
  const lines = [li];
  let cur = li;
  for (let k = 1; k < n; k++) {
    const t = a0 + (sweep * k) / n;
    splitLineAt(doc, cur, cx + Math.cos(t) * rAbs, cy + Math.sin(t) * rAbs);
    cur += 1;
    lines.push(cur);
  }
  return lines;
}

// ------------------------------------------------------------------ shapes

/** Rectangle corners from two opposite corners (clockwise, so the inside is on the right). */
export function rectPoints(p, q) {
  const x0 = Math.min(p.x, q.x), x1 = Math.max(p.x, q.x), y0 = Math.min(p.y, q.y), y1 = Math.max(p.y, q.y);
  if (x0 === x1 || y0 === y1) return [];
  return [{ x: x0, y: y0 }, { x: x0, y: y1 }, { x: x1, y: y1 }, { x: x1, y: y0 }];
}

/** Ellipse inscribed in the box `p`–`q`, `n` sides (clockwise), integer points. */
export function ellipsePoints(p, q, n) {
  const sides = Math.max(3, Math.min(128, Math.round(n)));
  const cx = (p.x + q.x) / 2, cy = (p.y + q.y) / 2, rx = Math.abs(q.x - p.x) / 2, ry = Math.abs(q.y - p.y) / 2;
  if (rx < 1 || ry < 1) return [];
  const out = [];
  for (let i = 0; i < sides; i++) {
    const t = -(i / sides) * 2 * Math.PI + Math.PI / 2;
    const pt = { x: Math.round(cx + Math.cos(t) * rx), y: Math.round(cy + Math.sin(t) * ry) };
    if (!out.length || out.at(-1).x !== pt.x || out.at(-1).y !== pt.y) out.push(pt);
  }
  if (out.length > 1 && out[0].x === out.at(-1).x && out[0].y === out.at(-1).y) out.pop();
  return out;
}

// ------------------------------------------------------------------ tag links

/** Doom-format teleporters (to a TeleportDest in the tagged sector). */
const DOOM_TELEPORTS = new Set([39, 97, 125, 126, 174, 195, 207, 208, 209, 210, 268, 269]);
/** Boom line-to-line teleporters (to the line with the same tag). */
const DOOM_LINE_TELEPORTS = new Set([243, 244, 262, 263, 264, 265, 266, 267]);
const TELEPORT_DEST = new Set([14, 9044]);

/**
 * What each argument of a Hexen / ZDoom special points at: `s` sector tag,
 * `t` thing TID, `l` line id, `p` polyobject id (ZDoom `zspecial.acs`).
 */
const ARG_KINDS = (() => {
  const k = {};
  const set = (nums, kinds) => { for (const n of nums) k[n] = kinds; };
  set([1, 2, 3, 4, 5, 6, 7, 8, 86, 87, 88, 89, 90, 91, 92, 93], "p");
  set([10, 11, 12, 13, 14, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32, 35, 36, 37, 38, 40, 41, 42, 43,
    44, 45, 46, 47, 60, 61, 62, 63, 64, 65, 66, 67, 68, 69, 94, 95, 96, 97, 99, 104, 105, 106, 110, 111,
    112, 113, 114, 115, 116, 117, 138, 139, 140, 168, 169, 172, 185, 186, 187, 188, 189, 192, 193, 194, 195,
    196, 197, 198, 199, 200, 201, 202, 203, 204, 205, 206, 207, 212, 213, 214, 216, 217, 218, 219, 220, 228,
    230, 231, 232, 233, 234, 235, 236, 237, 238, 239, 240, 241, 242, 245, 246, 247, 249, 250, 251, 252, 253,
    254, 255, 256, 257, 258, 259, 260, 261, 262], "s");
  set([17, 19, 119, 127, 128, 130, 131, 132, 133, 134, 135, 136, 137, 175, 176, 178, 180], "t");
  k[177] = "tt"; // Thing_Hate(hater, hatee)
  k[229] = "tt"; // Thing_SetGoal(tid, goal)
  k[59] = "p-t"; // Polyobj_OR_MoveToSpot(po, speed, target)
  k[34] = "s"; // ClearForceField(tag)
  k[48] = "ls"; // Sector_Attach3dMidtex(lineid, tag, …)
  k[51] = "ss"; // Sector_SetLink(controltag, tag, …)
  k[52] = "l"; k[53] = "l"; k[55] = "l"; k[56] = "l";
  k[57] = "s";
  k[70] = "ts"; // Teleport(tid, tag)
  k[71] = "t-s"; // Teleport_NoFog(tid, useangle, tag)
  k[74] = "";
  k[76] = "tt"; // TeleportOther(tid, desttid)
  k[77] = "ttt"; // TeleportGroup(group, source, dest)
  k[78] = "stt"; // TeleportInSector(tag, source, dest)
  k[107] = "ll"; k[121] = ""; k[156] = "l";
  k[125] = "tt"; // Thing_Move(tid, destid)
  k[160] = "s"; // Sector_Set3dFloor(tag, …)
  k[209] = "s"; k[215] = "-l"; // Teleport_Line(thisid, destid)
  k[222] = "l"; k[223] = "s"; k[224] = "s"; // Scroll_Texture_Model(lineid), Scroll_Floor/Ceiling(tag)
  return k;
})();

/** Sector tags of a sector (`tag` plus UDMF `moreids`). */
const sectorTags = (s) => [s.tag, ...(s.extra_ids || [])].filter((t) => t);
/** Line ids: UDMF `id` (+ `moreids`), Hexen `Line_SetIdentification`, `Line_SetPortal`'s own id. */
function lineIds(doc, l) {
  const ids = [];
  if (doc.format === "Udmf") ids.push(l.tag, ...(l.extra_ids || []));
  else if (doc.format === "Hexen") {
    if (l.special === 121) ids.push(l.args[0] + 256 * (l.args[4] || 0));
    if (l.special === 156) ids.push(l.args[1]);
  }
  return ids.filter((t) => t);
}

/** What an object's special refers to: `[{ kind: "s"|"t"|"l"|"p", value }]`. */
/** Argument kinds of line special `n` (`s` tag, `t` tid, `l` line id, `p` polyobj, `-` other), or null. */
export function argKindsOf(n) {
  return ARG_KINDS[n] ?? null;
}

export function specialRefs(doc, obj, isThing = false) {
  if (!obj.special) return [];
  if (!isThing && !isHexenLike(doc)) {
    if (!obj.tag) return [];
    if (DOOM_LINE_TELEPORTS.has(obj.special)) return [{ kind: "l", value: obj.tag, teleport: true }];
    return [{ kind: "s", value: obj.tag, teleport: DOOM_TELEPORTS.has(obj.special) }];
  }
  const kinds = ARG_KINDS[obj.special];
  if (kinds == null) return [];
  const out = [];
  [...kinds].forEach((c, i) => {
    const v = obj.args?.[i] || 0;
    if (c === "-" || !v) return;
    out.push({ kind: c, value: v, teleport: [70, 71, 76, 77, 215].includes(obj.special) });
  });
  return out;
}

/**
 * Everything the selection is tied to by tags, TIDs, line ids and
 * polyobject ids, both ways: what it triggers and what triggers it.
 * `mode` is the editor's selection mode, `sel` the selected indices.
 * Returns `{ sectors, lines, things }` (sets of indices) and `arrows`
 * (`{ from: {x,y}, to: {x,y}, teleport }`, source → target).
 */
export function tagLinks(doc, mode, sel) {
  const res = { sectors: new Set(), lines: new Set(), things: new Set(), arrows: [] };
  if (!doc || !sel?.size) return res;
  const centers = new Map();
  const center = (s) => {
    if (!centers.has(s)) centers.set(s, sectorCenter(doc, s));
    return centers.get(s);
  };
  const lineMid = (li) => {
    const l = doc.linedefs[li], a = doc.vertices[l.v1], b = doc.vertices[l.v2];
    return { x: (a.x + b.x) / 2, y: (a.y + b.y) / 2 };
  };
  const thingPos = (ti) => ({ x: doc.things[ti].x, y: doc.things[ti].y });
  const thingSector = (t) => {
    // Sector whose boundary loop encloses the thing (smallest bbox wins).
    let best = -1, bestArea = Infinity;
    for (let s = 0; s < doc.sectors.length; s++) {
      const edges = sectorEdges(doc, s);
      let inside = false;
      let x0 = Infinity, y0 = Infinity, x1 = -Infinity, y1 = -Infinity;
      for (const [a, b] of edges) {
        const p = doc.vertices[a], q = doc.vertices[b];
        x0 = Math.min(x0, p.x); y0 = Math.min(y0, p.y); x1 = Math.max(x1, p.x); y1 = Math.max(y1, p.y);
        if ((p.y > t.y) !== (q.y > t.y) && t.x < ((q.x - p.x) * (t.y - p.y)) / (q.y - p.y) + p.x) inside = !inside;
      }
      const area = (x1 - x0) * (y1 - y0);
      if (inside && area < bestArea) { bestArea = area; best = s; }
    }
    return best;
  };
  const polyLines = (id) => doc.linedefs.flatMap((l, i) => ((l.special === 1 || l.special === 5) && l.args[0] === id ? [i] : []));
  /** Map objects a reference points at. */
  const targets = (ref) => {
    const out = [];
    if (ref.kind === "s") {
      doc.sectors.forEach((s, i) => { if (sectorTags(s).includes(ref.value)) out.push(["sectors", i, center(i)]); });
      if (ref.teleport && !isHexenLike(doc)) {
        doc.things.forEach((t, i) => {
          if (TELEPORT_DEST.has(t.type_id) && out.some(([k, s]) => k === "sectors" && s === thingSector(t))) out.push(["things", i, thingPos(i)]);
        });
      }
    } else if (ref.kind === "t") {
      doc.things.forEach((t, i) => { if (t.tid === ref.value) out.push(["things", i, thingPos(i)]); });
    } else if (ref.kind === "l") {
      if (!isHexenLike(doc)) {
        doc.linedefs.forEach((l, i) => { if (l.tag === ref.value) out.push(["lines", i, lineMid(i)]); });
      } else {
        doc.linedefs.forEach((l, i) => { if (lineIds(doc, l).includes(ref.value)) out.push(["lines", i, lineMid(i)]); });
      }
    } else if (ref.kind === "p") {
      for (const i of polyLines(ref.value)) out.push(["lines", i, lineMid(i)]);
      doc.things.forEach((t, i) => { if (t.type_id >= 9300 && t.type_id <= 9303 && t.angle === ref.value) out.push(["things", i, thingPos(i)]); });
    }
    return out;
  };
  /** Every special source in the map: `[kind, index, pos, refs]`. */
  const sources = [];
  doc.linedefs.forEach((l, i) => { const r = specialRefs(doc, l); if (r.length) sources.push(["lines", i, () => lineMid(i), r]); });
  doc.things.forEach((t, i) => { const r = specialRefs(doc, t, true); if (r.length) sources.push(["things", i, () => thingPos(i), r]); });
  const add = (k, i) => res[k].add(i);
  const key = { lines: "lines", sectors: "sectors", things: "things", vertices: null }[mode];
  if (!key) return res;
  const selected = (k, i) => k === key && sel.has(i);
  // Forward: selected sources → their targets.
  for (const [k, i, pos, refs] of sources) {
    if (!selected(k, i)) continue;
    for (const ref of refs) {
      for (const [tk, ti, tp] of targets(ref)) {
        if (selected(tk, ti)) continue;
        add(tk, ti);
        if (tp) res.arrows.push({ from: pos(), to: tp, teleport: !!ref.teleport });
      }
    }
  }
  // Backward: sources whose targets include the selection.
  for (const [k, i, pos, refs] of sources) {
    if (selected(k, i)) continue;
    for (const ref of refs) {
      for (const [tk, ti, tp] of targets(ref)) {
        if (!selected(tk, ti)) continue;
        add(k, i);
        if (tp) res.arrows.push({ from: pos(), to: tp, teleport: !!ref.teleport });
      }
    }
  }
  // A selected sector also lights up things in it that teleporters aim at.
  return res;
}

// ------------------------------------------------------------------ find / filter

/** What Find can look for, by selection mode. */
export const FIND_KINDS = {
  texture: { mode: "lines", label: "Wall texture" },
  flat: { mode: "sectors", label: "Floor / ceiling flat" },
  thing: { mode: "things", label: "Thing type" },
  lineSpecial: { mode: "lines", label: "Line special" },
  sectorSpecial: { mode: "sectors", label: "Sector special" },
  lineTag: { mode: "lines", label: "Line tag / id" },
  sectorTag: { mode: "sectors", label: "Sector tag" },
  tid: { mode: "things", label: "Thing TID" },
  light: { mode: "sectors", label: "Light level" },
};

const texEq = (a, b) => String(a).toUpperCase() === String(b).toUpperCase();
const numEq = (a, b) => Number(a) === Number(b);

/** Does object `i` (of `FIND_KINDS[kind].mode`) match `value`? */
export function matches(doc, kind, i, value) {
  const v = String(value).trim();
  if (!v) return false;
  switch (kind) {
    case "texture": {
      const l = doc.linedefs[i];
      return [l.side_front, l.side_back].some((s) => {
        const sd = sideOf(doc, s);
        return sd && ["upper", "middle", "lower"].some((k) => texEq(sd[k], v));
      });
    }
    case "flat": return texEq(doc.sectors[i].floor_pic, v) || texEq(doc.sectors[i].ceiling_pic, v);
    case "thing": return numEq(doc.things[i].type_id, v);
    case "lineSpecial": return numEq(doc.linedefs[i].special, v);
    case "sectorSpecial": return numEq(doc.sectors[i].special, v);
    case "lineTag": return numEq(doc.linedefs[i].tag, v) || lineIds(doc, doc.linedefs[i]).includes(Number(v));
    case "sectorTag": return sectorTags(doc.sectors[i]).includes(Number(v));
    case "tid": return numEq(doc.things[i].tid, v);
    case "light": return numEq(doc.sectors[i].light, v);
    default: return false;
  }
}

const listOf = (doc, mode) => ({ lines: doc.linedefs, sectors: doc.sectors, things: doc.things }[mode] || []);

/** Indices matching `value` for `kind` (in its mode). */
export function findAll(doc, kind, value) {
  const mode = FIND_KINDS[kind]?.mode;
  return listOf(doc, mode).flatMap((_, i) => (matches(doc, kind, i, value) ? [i] : []));
}

/** Replace `from` by `to` everywhere `kind` applies; returns how many fields changed. */
export function replaceAll(doc, kind, from, to) {
  let n = 0;
  const f = String(from).trim(), t = String(to).trim();
  if (!f || !t) return 0;
  const num = Number(t);
  for (const i of findAll(doc, kind, f)) {
    switch (kind) {
      case "texture": {
        const l = doc.linedefs[i];
        for (const s of [l.side_front, l.side_back]) {
          const sd = sideOf(doc, s);
          if (!sd) continue;
          for (const k of ["upper", "middle", "lower"]) if (texEq(sd[k], f)) { sd[k] = t.toUpperCase(); n++; }
        }
        break;
      }
      case "flat":
        for (const k of ["floor_pic", "ceiling_pic"]) if (texEq(doc.sectors[i][k], f)) { doc.sectors[i][k] = t.toUpperCase(); n++; }
        break;
      case "thing": doc.things[i].type_id = num; n++; break;
      case "lineSpecial": doc.linedefs[i].special = num; n++; break;
      case "sectorSpecial": doc.sectors[i].special = num; n++; break;
      case "lineTag": if (numEq(doc.linedefs[i].tag, f)) { doc.linedefs[i].tag = num; n++; } break;
      case "sectorTag": if (numEq(doc.sectors[i].tag, f)) { doc.sectors[i].tag = num; n++; } break;
      case "tid": doc.things[i].tid = num; n++; break;
      case "light": doc.sectors[i].light = Math.max(0, Math.min(255, num)); n++; break;
      default: break;
    }
  }
  return n;
}

// ------------------------------------------------------------------ prefabs

/** A prefab must look like a clipboard the editor can paste. */
export function isPrefab(p) {
  return !!p && typeof p.name === "string" && p.clip && ["sectors", "lines", "things"].includes(p.clip.kind)
    && Array.isArray(p.clip.verts) && Array.isArray(p.clip.lines) && Array.isArray(p.clip.things);
}

/** Size of a prefab's clip in map units (`[w, h]`). */
export function prefabSize(clip) {
  const pts = [...clip.verts, ...clip.things];
  if (!pts.length) return [0, 0];
  const xs = pts.map((p) => p.x), ys = pts.map((p) => p.y);
  return [Math.max(...xs) - Math.min(...xs), Math.max(...ys) - Math.min(...ys)];
}

// ------------------------------------------------------------------ measure

/** Distance, components and angle (degrees, map convention) from `a` to `b`. */
export function measure(a, b) {
  const dx = b.x - a.x, dy = b.y - a.y;
  const ang = ((Math.atan2(dy, dx) * 180) / Math.PI + 360) % 360;
  return { dist: Math.hypot(dx, dy), dx, dy, angle: ang };
}

/**
 * A prefab's outline fitted in a `size`×`size` box (y up → screen y down):
 * `{segs: [[x1, y1, x2, y2, twoSided]], dots: [[x, y, type]]}`.
 */
export function prefabOutline(clip, size = 56, pad = 4) {
  const pts = [...clip.verts, ...clip.things];
  if (!pts.length) return { segs: [], dots: [] };
  const xs = pts.map((p) => p.x), ys = pts.map((p) => p.y);
  const x0 = Math.min(...xs), y1 = Math.max(...ys);
  const span = Math.max(Math.max(...xs) - x0, y1 - Math.min(...ys), 1);
  const k = (size - 2 * pad) / span;
  const P = (p) => [pad + (p.x - x0) * k, pad + (y1 - p.y) * k];
  const segs = clip.lines
    .map(({ line, back }) => {
      const a = clip.verts[line.v1], b = clip.verts[line.v2];
      return a && b ? [...P(a), ...P(b), !!back] : null;
    })
    .filter(Boolean);
  const dots = clip.things.map((t) => [...P(t), t.type_id]);
  return { segs, dots };
}

/** File format for sharing prefabs. */
export const PREFAB_FILE = "room-prefabs";

/** Prefabs as a downloadable JSON text. */
export function prefabsToFile(list) {
  return JSON.stringify({ format: PREFAB_FILE, version: 1, prefabs: list }, null, 1);
}

/**
 * Add the prefabs of a file (`text`) to `existing`: invalid entries are
 * skipped, a name already in use gets " (2)", " (3)"…
 * @returns {{list: object[], added: string[], skipped: number}}
 */
export function mergePrefabs(existing, text) {
  let data;
  try { data = JSON.parse(text); } catch (_) { throw new Error("not a prefab file (invalid JSON)"); }
  const incoming = Array.isArray(data) ? data : data?.format === PREFAB_FILE ? data.prefabs : isPrefab(data) ? [data] : null;
  if (!Array.isArray(incoming)) throw new Error("not a prefab file");
  const list = [...existing];
  const added = [];
  let skipped = 0;
  for (const p of incoming) {
    if (!isPrefab(p)) { skipped++; continue; }
    let name = p.name.trim() || "prefab";
    const taken = (n) => list.some((q) => q.name === n);
    if (taken(name)) {
      let i = 2;
      while (taken(`${name} (${i})`)) i++;
      name = `${name} (${i})`;
    }
    list.unshift({ ...p, name, time: p.time || Date.now() });
    added.push(name);
  }
  return { list, added, skipped };
}
