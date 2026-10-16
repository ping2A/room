// Room web editor — pro tools: clipboard & transforms, map checker, auto-align,
// TEXTURE1/PNAMES composer and the live play-test panel. `install(ctx)` receives the
// editor's state and helpers (see the bottom of editor.js).

/** ZDoom / Hexen action specials (line + thing `special`). Unlisted numbers stay editable. */
export const HEXEN_LINE_SPECIALS = {
  0: "None", 1: "Polyobj_StartLine", 2: "Polyobj_RotateLeft", 3: "Polyobj_RotateRight", 4: "Polyobj_Move",
  5: "Polyobj_ExplicitLine", 6: "Polyobj_MoveTimes8", 7: "Polyobj_DoorSwing", 8: "Polyobj_DoorSlide",
  9: "Line_Horizon", 10: "Door_Close", 11: "Door_Open", 12: "Door_Raise", 13: "Door_LockedRaise",
  14: "Door_Animated", 15: "Autosave", 16: "Transfer_WallLight", 17: "Thing_Raise", 18: "StartConversation",
  19: "Thing_Stop", 20: "Floor_LowerByValue", 21: "Floor_LowerToLowest", 22: "Floor_LowerToNearest",
  23: "Floor_RaiseByValue", 24: "Floor_RaiseToHighest", 25: "Floor_RaiseToNearest", 26: "Stairs_BuildDown",
  27: "Stairs_BuildUp", 28: "Floor_RaiseAndCrush", 29: "Pillar_Build", 30: "Pillar_Open",
  31: "Stairs_BuildDownSync", 32: "Stairs_BuildUpSync", 33: "ForceField", 34: "ClearForceField",
  35: "Floor_RaiseByValueTimes8", 36: "Floor_LowerByValueTimes8", 37: "Floor_MoveToValue", 38: "Ceiling_Waggle",
  39: "Teleport_ZombieChanger", 40: "Ceiling_LowerByValue", 41: "Ceiling_RaiseByValue", 42: "Ceiling_CrushAndRaise",
  43: "Ceiling_LowerAndCrush", 44: "Ceiling_CrushStop", 45: "Ceiling_CrushRaiseAndStay", 46: "Floor_CrushStop",
  47: "Ceiling_MoveToValue", 48: "Sector_Attach3dMidtex", 49: "GlassBreak", 50: "ExtraFloor_LightOnly",
  51: "Sector_SetLink", 52: "Scroll_Wall", 53: "Line_SetTextureOffset", 54: "Sector_ChangeFlags",
  55: "Line_SetBlocking", 56: "Line_SetTextureScale", 57: "Sector_SetPortal", 58: "Sector_CopyScroller",
  59: "Polyobj_OR_MoveToSpot", 60: "Plat_PerpetualRaise", 61: "Plat_Stop", 62: "Plat_DownWaitUpStay",
  63: "Plat_DownByValue", 64: "Plat_UpWaitDownStay", 65: "Plat_UpByValue", 66: "Floor_LowerInstant",
  67: "Floor_RaiseInstant", 68: "Floor_MoveToValueTimes8", 69: "Ceiling_MoveToValueTimes8", 70: "Teleport",
  71: "Teleport_NoFog", 72: "ThrustThing", 73: "DamageThing", 74: "Teleport_NewMap", 75: "Teleport_EndGame",
  76: "TeleportOther", 77: "TeleportGroup", 78: "TeleportInSector", 79: "Thing_SetConversation",
  80: "ACS_Execute", 81: "ACS_Suspend", 82: "ACS_Terminate", 83: "ACS_LockedExecute",
  84: "ACS_ExecuteWithResult", 85: "ACS_LockedExecuteDoor", 86: "Polyobj_MoveToSpot", 87: "Polyobj_Stop",
  88: "Polyobj_MoveTo", 89: "Polyobj_OR_MoveTo", 90: "Polyobj_OR_RotateLeft", 91: "Polyobj_OR_RotateRight",
  92: "Polyobj_OR_Move", 93: "Polyobj_OR_MoveTimes8", 94: "Pillar_BuildAndCrush",
  95: "FloorAndCeiling_LowerByValue", 96: "FloorAndCeiling_RaiseByValue", 97: "Ceiling_LowerAndCrushDist",
  98: "Sector_SetTranslucent", 99: "Floor_RaiseAndCrushDoom", 100: "Scroll_Texture_Left",
  101: "Scroll_Texture_Right", 102: "Scroll_Texture_Up", 103: "Scroll_Texture_Down",
  104: "Ceiling_CrushAndRaiseSilentDist", 105: "Door_WaitRaise", 106: "Door_WaitClose",
  107: "Line_SetPortalTarget", 109: "Light_ForceLightning", 110: "Light_RaiseByValue",
  111: "Light_LowerByValue", 112: "Light_ChangeToValue", 113: "Light_Fade", 114: "Light_Glow",
  115: "Light_Flicker", 116: "Light_Strobe", 117: "Light_Stop", 118: "Plane_Copy", 119: "Thing_Damage",
  120: "Radius_Quake", 121: "Line_SetIdentification", 125: "Thing_Move", 127: "Thing_SetSpecial",
  128: "ThrustThingZ", 129: "UsePuzzleItem", 130: "Thing_Activate", 131: "Thing_Deactivate",
  132: "Thing_Remove", 133: "Thing_Destroy", 134: "Thing_Projectile", 135: "Thing_Spawn",
  136: "Thing_ProjectileGravity", 137: "Thing_SpawnNoFog", 138: "Floor_Waggle", 139: "Thing_SpawnFacing",
  140: "Sector_ChangeSound", 160: "Sector_Set3dFloor", 168: "Ceiling_CrushAndRaiseDist",
  172: "Plat_UpNearestWaitDownStay", 173: "NoiseAlert", 176: "Thing_ChangeTID", 179: "ChangeSkill",
  181: "Plane_Align", 182: "Line_Mirror", 183: "Line_AlignCeiling", 184: "Line_AlignFloor",
  185: "Sector_SetRotation", 186: "Sector_SetCeilingPanning", 187: "Sector_SetFloorPanning",
  190: "Static_Init", 200: "Generic_Floor", 201: "Generic_Ceiling", 202: "Generic_Door", 203: "Generic_Lift",
  204: "Generic_Stairs", 205: "Generic_Crusher", 206: "Plat_DownWaitUpStayLip", 207: "Plat_PerpetualRaiseLip",
  208: "TranslucentLine", 209: "Transfer_Heights", 210: "Transfer_FloorLight", 211: "Transfer_CeilingLight",
  212: "Sector_SetColor", 213: "Sector_SetFade", 214: "Sector_SetDamage", 215: "Teleport_Line",
  216: "Sector_SetGravity", 217: "Stairs_BuildUpDoom", 218: "Sector_SetWind", 219: "Sector_SetFriction",
  220: "Sector_SetCurrent", 221: "Scroll_Texture_Both", 222: "Scroll_Texture_Model", 223: "Scroll_Floor",
  224: "Scroll_Ceiling", 225: "Scroll_Texture_Offsets", 226: "ACS_ExecuteAlways", 227: "PointPush_SetForce",
  228: "Plat_RaiseAndStayTx0", 229: "Thing_SetGoal", 230: "Plat_UpByValueStayTx", 231: "Plat_ToggleCeiling",
  232: "Light_StrobeDoom", 233: "Light_MinNeighbor", 234: "Light_MaxNeighbor", 235: "Floor_TransferTrigger",
  236: "Floor_TransferNumeric", 237: "ChangeCamera", 238: "Floor_RaiseToLowestCeiling",
  239: "Floor_RaiseByValueTxTy", 240: "Floor_RaiseByTexture", 241: "Floor_LowerToLowestTxTy",
  242: "Floor_LowerToHighest", 243: "Exit_Normal", 244: "Exit_Secret", 245: "Elevator_RaiseToNearest",
  246: "Elevator_MoveToFloor", 247: "Elevator_LowerToNearest", 248: "HealThing", 249: "Door_CloseWaitOpen",
  250: "Floor_Donut", 251: "FloorAndCeiling_LowerRaise", 252: "Ceiling_RaiseToNearest",
  253: "Ceiling_LowerToLowest", 254: "Ceiling_LowerToFloor", 255: "Ceiling_CrushRaiseAndStaySilA",
};

export function install(ctx) {
  const { S, $, el, NO_SIDE, clampI16 } = ctx;
  const side = (i) => ctx.side(i);
  const sectorOf = (s) => (s ? S.doc.sectors[s.sector] : null);
  const norm360 = (a) => ((Math.round(a) % 360) + 360) % 360;

  let overMap = false;
  ctx.cv.addEventListener("mouseenter", () => { overMap = true; });
  ctx.cv.addEventListener("mouseleave", () => { overMap = false; });

  /** Bring a selection into view and the inspector. */
  function show(mode, idx) {
    const d = S.doc;
    if (S.docTab !== "map") ctx.selectDocTab("map");
    if (S.mode !== mode) { S.mode = mode; ctx.syncModeButtons(); }
    S.sel = new Set(idx.filter((i) => i >= 0));
    S.pick3d = null;
    S.actorSel = null;
    const i = idx[0];
    let p = null;
    if (mode === "vertices") p = d.vertices[i];
    else if (mode === "things") p = d.things[i];
    else if (mode === "lines") {
      const l = d.linedefs[i];
      const a = l && ctx.vtx(l.v1), b = l && ctx.vtx(l.v2);
      if (a && b) p = { x: (a.x + b.x) / 2, y: (a.y + b.y) / 2 };
    } else if (mode === "sectors") {
      const bb = ctx.geometry().bbox[i];
      if (bb && Number.isFinite(bb[0])) p = { x: (bb[0] + bb[2]) / 2, y: (bb[1] + bb[3]) / 2 };
    }
    if (p) { S.view.x = p.x; S.view.y = p.y; }
    ctx.renderInspector();
    ctx.render2d();
  }

  // ------------------------------------------------------------ clipboard & transforms

  let clip = null;
  const centerOf = (pts) => {
    let x0 = Infinity, y0 = Infinity, x1 = -Infinity, y1 = -Infinity;
    for (const p of pts) { x0 = Math.min(x0, p.x); y0 = Math.min(y0, p.y); x1 = Math.max(x1, p.x); y1 = Math.max(y1, p.y); }
    return pts.length ? [(x0 + x1) / 2, (y0 + y1) / 2] : [0, 0];
  };

  function thingsInSectors(secs) {
    return S.doc.things.map((t, i) => [t, i]).filter(([t]) => secs.has(ctx.sectorAt(t.x, t.y)));
  }

  function copySelection() {
    const d = S.doc;
    if (!d || !S.sel.size) return false;
    if (S.mode === "things") {
      const things = [...S.sel].map((i) => structuredClone(d.things[i]));
      clip = { kind: "things", things, lines: [], verts: [], sectors: [], center: centerOf(things) };
      return true;
    }
    if (S.mode !== "sectors" && S.mode !== "lines") return false;
    const vMap = new Map(), verts = [], lines = [];
    const addV = (i) => {
      if (!vMap.has(i)) { vMap.set(i, verts.length); verts.push({ ...d.vertices[i] }); }
      return vMap.get(i);
    };
    if (S.mode === "sectors") {
      const secList = [...S.sel].sort((a, b) => a - b);
      const secMap = new Map(secList.map((s, i) => [s, i]));
      for (const l of d.linedefs) {
        const f = side(l.side_front), b = side(l.side_back);
        const fin = !!f && secMap.has(f.sector), bin = !!b && secMap.has(b.sector);
        if (!fin && !bin) continue;
        const line = structuredClone(l);
        let front = fin ? { ...structuredClone(f), sector: secMap.get(f.sector) } : null;
        let back = bin ? { ...structuredClone(b), sector: secMap.get(b.sector) } : null;
        let v1 = l.v1, v2 = l.v2;
        if (!front) { front = back; back = null; [v1, v2] = [v2, v1]; }
        if (f && b && !(fin && bin)) {
          line.flags = (line.flags & ~4) | 1;
          if (front.middle === "-") front.middle = front.lower !== "-" ? front.lower : front.upper !== "-" ? front.upper : S.texture;
        }
        line.v1 = addV(v1); line.v2 = addV(v2);
        lines.push({ line, front, back });
      }
      const things = thingsInSectors(S.sel).map(([t]) => structuredClone(t));
      clip = { kind: "sectors", verts, lines, sectors: secList.map((i) => structuredClone(d.sectors[i])), things, center: centerOf(verts) };
    } else {
      for (const i of [...S.sel].sort((a, b) => a - b)) {
        const l = d.linedefs[i];
        const line = structuredClone(l);
        line.v1 = addV(l.v1); line.v2 = addV(l.v2);
        lines.push({ line, front: structuredClone(side(l.side_front)), back: structuredClone(side(l.side_back)) });
      }
      clip = { kind: "lines", verts, lines, sectors: [], things: [], center: centerOf(verts) };
    }
    return true;
  }

  function paste(at) {
    if (!clip || !S.doc) { ctx.toast("Clipboard is empty — Ctrl+C a selection first"); return; }
    const d = S.doc;
    const dx = ctx.snap(at[0] - clip.center[0]), dy = ctx.snap(at[1] - clip.center[1]);
    ctx.snapshot();
    const sel = [];
    const v0 = d.vertices.length, s0 = d.sectors.length;
    for (const v of clip.verts) d.vertices.push({ ...v, x: clampI16(v.x + dx), y: clampI16(v.y + dy) });
    for (const s of clip.sectors) { d.sectors.push(structuredClone(s)); sel.push(d.sectors.length - 1); }
    const addSide = (s) => {
      if (!s) return NO_SIDE;
      const c = structuredClone(s);
      if (clip.kind === "sectors") c.sector += s0;
      else if (c.sector >= d.sectors.length) c.sector = 0;
      d.sidedefs.push(c);
      return d.sidedefs.length - 1;
    };
    for (const { line, front, back } of clip.lines) {
      const l = structuredClone(line);
      l.v1 += v0; l.v2 += v0;
      l.side_front = addSide(front);
      l.side_back = addSide(back);
      d.linedefs.push(l);
      if (clip.kind === "lines") sel.push(d.linedefs.length - 1);
    }
    for (const t of clip.things) {
      d.things.push({ ...structuredClone(t), x: clampI16(t.x + dx), y: clampI16(t.y + dy) });
      if (clip.kind === "things") sel.push(d.things.length - 1);
    }
    S.mode = clip.kind;
    ctx.syncModeButtons();
    S.sel = new Set(sel);
    ctx.commit();
    ctx.status(`Pasted ${sel.length} ${clip.kind}${clip.kind !== "things" && clip.things.length ? ` + ${clip.things.length} things` : ""}`);
  }

  function pastePoint(duplicate) {
    if (!duplicate && overMap && ctx.mouse.world) return ctx.mouse.world;
    const step = Math.max(S.grid, 32);
    return [clip.center[0] + step, clip.center[1] - step];
  }

  function transform(kind) {
    const d = S.doc;
    if (!d || !S.sel.size) { ctx.toast("Select something to transform"); return; }
    const vs = [...ctx.selectionVertices()];
    let things = [];
    if (S.mode === "things") things = [...S.sel].map((i) => d.things[i]);
    else if (S.mode === "sectors") things = thingsInSectors(S.sel).map(([t]) => t);
    const pts = [...vs.map((i) => d.vertices[i]), ...things];
    if (!pts.length) return;
    let [cx, cy] = centerOf(pts);
    cx = ctx.snap(cx); cy = ctx.snap(cy);
    const f = {
      cw: (x, y) => [cx + (y - cy), cy - (x - cx)],
      ccw: (x, y) => [cx - (y - cy), cy + (x - cx)],
      mirrorX: (x, y) => [2 * cx - x, y],
      mirrorY: (x, y) => [x, 2 * cy - y],
    }[kind];
    const turn = { cw: (a) => a - 90, ccw: (a) => a + 90, mirrorX: (a) => 180 - a, mirrorY: (a) => -a }[kind];
    ctx.snapshot();
    for (const p of pts) { const [x, y] = f(p.x, p.y); p.x = clampI16(x); p.y = clampI16(y); }
    for (const t of things) if (typeof t.angle === "number") t.angle = norm360(turn(t.angle));
    if (kind.startsWith("mirror")) {
      const vset = new Set(vs);
      for (const l of d.linedefs) if (vset.has(l.v1) && vset.has(l.v2)) [l.v1, l.v2] = [l.v2, l.v1];
    }
    ctx.commit();
    ctx.status({ cw: "Rotated 90° clockwise", ccw: "Rotated 90° counter-clockwise", mirrorX: "Mirrored horizontally", mirrorY: "Mirrored vertically" }[kind]);
  }

  // ------------------------------------------------------------ auto-align

  /** Align texture x offsets along walls connected to `lineIdx`'s side that share its texture. */
  function autoAlign(lineIdx, sideKey, part) {
    const d = S.doc;
    const l0 = d.linedefs[lineIdx];
    const s0 = l0 && side(l0[sideKey]);
    if (!s0) return;
    const tex = (part && s0[part] !== "-" ? s0[part] : null) || [s0.middle, s0.upper, s0.lower].find((t) => t && t !== "-");
    if (!tex) { ctx.toast("This wall has no texture to align", true); return; }
    const tw = Math.max(1, ctx.thumb(tex)?.width || 64);
    const wrap = (v) => ((Math.round(v) % tw) + tw) % tw;
    const uses = (s) => s && (s.upper === tex || s.middle === tex || s.lower === tex);
    const startV = (l, sk) => (sk === "side_front" ? l.v1 : l.v2);
    const endV = (l, sk) => (sk === "side_front" ? l.v2 : l.v1);
    const len = (l) => { const a = ctx.vtx(l.v1), b = ctx.vtx(l.v2); return a && b ? Math.hypot(b.x - a.x, b.y - a.y) : 0; };
    const starts = new Map(), ends = new Map();
    const push = (m, k, v) => { if (!m.has(k)) m.set(k, []); m.get(k).push(v); };
    d.linedefs.forEach((l, i) => {
      for (const sk of ["side_front", "side_back"]) {
        if (!uses(side(l[sk]))) continue;
        push(starts, startV(l, sk), [i, sk]);
        push(ends, endV(l, sk), [i, sk]);
      }
    });
    ctx.snapshot();
    const seen = new Set([`${lineIdx}:${sideKey}`]);
    const unseen = ([i, sk], from) => i !== from && !seen.has(`${i}:${sk}`);
    let n = 0;
    for (const dir of [1, -1]) {
      let cur = [lineIdx, sideKey], off = s0.x_offset;
      for (let guard = 0; guard < 4096; guard++) {
        const l = d.linedefs[cur[0]];
        const cands = (dir > 0 ? starts.get(endV(l, cur[1])) : ends.get(startV(l, cur[1]))) || [];
        const next = cands.find((c) => unseen(c, cur[0]));
        if (!next) break;
        seen.add(`${next[0]}:${next[1]}`);
        off = dir > 0 ? off + len(l) : off - len(d.linedefs[next[0]]);
        off = wrap(off);
        side(d.linedefs[next[0]][next[1]]).x_offset = clampI16(off);
        cur = next;
        n++;
      }
    }
    ctx.commit();
    ctx.status(`Auto-aligned ${n} wall${n === 1 ? "" : "s"} to ${tex} (width ${tw})`);
  }

  function autoAlignSelection() {
    const h = S.pick3d;
    if (h?.kind === "wall") { autoAlign(h.index, h.side ? "side_back" : "side_front", h.part); return; }
    if (S.mode === "lines" && S.sel.size) { autoAlign([...S.sel][0], "side_front", null); return; }
    ctx.toast("Pick a wall in 3D or select a line, then press J");
  }

  // ------------------------------------------------------------ map checker

  const checker = { panel: null, version: -1 };

  function runChecks() {
    const d = S.doc;
    const out = [];
    const add = (sev, msg, mode, idx, fix, opts = {}) => out.push({ sev, msg, mode, idx: [].concat(idx), fix, ...opts });
    const nv = d.vertices.length, ns = d.sidedefs.length, nsec = d.sectors.length;
    const hexLike = ctx.isHexen() || ctx.isUdmf();
    const texSet = new Set([...(S.summary?.textures || []), ...(S.summary?.flats || [])].map((t) => t.toUpperCase()));
    const known = (name) => !name || name === "-" || texSet.has(name.toUpperCase());

    // Things.
    const starts = d.things.filter((t) => t.type_id === 1);
    if (!starts.length) {
      add("error", "No player 1 start — the map cannot be played", "things", [], () => {
        const [x, y] = ctx.sectorAt(S.view.x, S.view.y) >= 0 ? [S.view.x, S.view.y] : (() => {
          const bb = ctx.geometry().bbox.find((b) => Number.isFinite(b[0]));
          return bb ? [(bb[0] + bb[2]) / 2, (bb[1] + bb[3]) / 2] : [0, 0];
        })();
        d.things.push(ctx.tpl("things", { x: clampI16(x), y: clampI16(y), z: 0, angle: 90, type_id: 1, flags: ctx.isHexen() ? 0x707 : 7, hexen: hexLike }));
      }, { fixLabel: "Place at view" });
    } else if (starts.length > 1) {
      add("info", `${starts.length} player 1 starts (extras become voodoo dolls)`, "things", d.things.map((t, i) => (t.type_id === 1 ? i : -1)).filter((i) => i >= 0));
    }
    const coop = [2, 3, 4].filter((n) => !d.things.some((t) => t.type_id === n));
    if (coop.length) add("info", `Missing co-op start${coop.length > 1 ? "s" : ""} for player ${coop.join(", ")}`, "things", []);
    const dm = d.things.filter((t) => t.type_id === 11).length;
    if (dm < 4) add("info", `${dm} deathmatch start${dm === 1 ? "" : "s"} (4+ recommended)`, "things", []);
    const unknown = new Map();
    d.things.forEach((t, i) => {
      if (!S.thingInfo.has(t.type_id)) { if (!unknown.has(t.type_id)) unknown.set(t.type_id, []); unknown.get(t.type_id).push(i); }
      if (ctx.sectorAt(t.x, t.y) < 0) add("warn", `Thing ${i} (${S.thingInfo.get(t.type_id)?.title || t.type_id}) is outside the map`, "things", i);
      if (!ctx.isUdmf() && typeof t.flags === "number") {
        const skills = (t.flags & 7) === 0;
        const classes = ctx.isHexen() && (t.flags & 0xe0) === 0 && (t.flags & 0x700) === 0;
        if (skills) add("warn", `Thing ${i} has no skill flags — never spawns`, "things", i, () => { t.flags |= 7; }, { safe: true, fixLabel: "All skills" });
        else if (classes) add("warn", `Thing ${i} spawns for no player class or mode`, "things", i, () => { t.flags |= 0x7e0; }, { safe: true, fixLabel: "All classes" });
      }
    });
    for (const [type, idx] of unknown) add("warn", `Unknown thing type ${type} ×${idx.length}`, "things", idx);

    // Vertices.
    const byPos = new Map();
    d.vertices.forEach((v, i) => { const k = `${v.x},${v.y}`; if (!byPos.has(k)) byPos.set(k, []); byPos.get(k).push(i); });
    const dupV = [...byPos.values()].filter((g) => g.length > 1);
    if (dupV.length) {
      add("warn", `${dupV.reduce((s, g) => s + g.length - 1, 0)} duplicate vertices (same position)`, "vertices", dupV.flat(), () => {
        const to = new Map();
        for (const g of dupV) for (const i of g.slice(1)) to.set(i, g[0]);
        for (const l of d.linedefs) { l.v1 = to.get(l.v1) ?? l.v1; l.v2 = to.get(l.v2) ?? l.v2; }
        d.linedefs = d.linedefs.filter((l) => l.v1 !== l.v2);
        ctx.compact();
      }, { fixLabel: "Merge" });
    }

    // Lines.
    const pairs = new Map();
    const zero = [], badRef = [];
    d.linedefs.forEach((l, i) => {
      const a = d.vertices[l.v1], b = d.vertices[l.v2];
      if (!a || !b) { badRef.push(i); return; }
      if (a.x === b.x && a.y === b.y) zero.push(i);
      const k = l.v1 < l.v2 ? `${l.v1}:${l.v2}` : `${l.v2}:${l.v1}`;
      if (!pairs.has(k)) pairs.set(k, []); pairs.get(k).push(i);
      for (const sk of ["side_front", "side_back"]) {
        if (l[sk] !== NO_SIDE && l[sk] >= ns) add("error", `Line ${i} ${sk} references missing sidedef ${l[sk]}`, "lines", i, () => { l[sk] = NO_SIDE; }, { safe: true, fixLabel: "Clear" });
      }
      const f = side(l.side_front), bk = side(l.side_back);
      if (!f) {
        if (bk) add("error", `Line ${i} has only a back side`, "lines", i, () => { [l.v1, l.v2] = [l.v2, l.v1]; [l.side_front, l.side_back] = [l.side_back, l.side_front]; }, { safe: true, fixLabel: "Flip" });
        else add("error", `Line ${i} has no sides`, "lines", i, () => { d.linedefs.splice(i, 1); ctx.compact(); }, { fixLabel: "Delete" });
        return;
      }
      for (const [s, nm] of [[f, "front"], [bk, "back"]]) {
        if (s && s.sector >= nsec) add("error", `Line ${i} ${nm} side points at missing sector ${s.sector}`, "lines", i, () => { s.sector = 0; }, { safe: true, fixLabel: "Sector 0" });
      }
      const twoFlag = (l.flags & 4) !== 0;
      if (!!bk !== twoFlag) {
        add("warn", `Line ${i} ${bk ? "has a back side but no Two-sided flag" : "is flagged Two-sided without a back side"}`, "lines", i,
          () => { l.flags = bk ? (l.flags | 4) : (l.flags & ~4) | 1; }, { safe: true, fixLabel: "Fix flags" });
      }
      const fs = sectorOf(f), bs = sectorOf(bk);
      const need = [];
      if (!bk) { if (f.middle === "-") need.push([f, "middle", "front"]); }
      else if (fs && bs) {
        const sky = (s) => String(s.ceiling_pic).toUpperCase().startsWith("F_SKY");
        const bothSky = sky(fs) && sky(bs);
        if (fs.ceiling_height > bs.ceiling_height && !bothSky && f.upper === "-") need.push([f, "upper", "front"]);
        if (bs.ceiling_height > fs.ceiling_height && !bothSky && bk.upper === "-") need.push([bk, "upper", "back"]);
        if (bs.floor_height > fs.floor_height && f.lower === "-") need.push([f, "lower", "front"]);
        if (fs.floor_height > bs.floor_height && bk.lower === "-") need.push([bk, "lower", "back"]);
      }
      for (const [s, part, nm] of need) {
        add("warn", `Line ${i}: missing ${nm} ${part} texture`, "lines", i, () => {
          s[part] = [s.middle, s.upper, s.lower].find((t) => t && t !== "-") || S.texture;
        }, { safe: true, fixLabel: "Fill" });
      }
      if (!hexLike && l.special) {
        const name = ctx.DOOM_LINE_SPECIALS[l.special];
        if (name && l.tag === 0 && !/^D[R1S]|Exit|Scroll|Translucent/i.test(name)) add("warn", `Line ${i}: "${name}" has tag 0 (acts on no sector)`, "lines", i);
        if (l.tag && !d.sectors.some((s) => s.tag === l.tag)) add("warn", `Line ${i}: no sector has tag ${l.tag}`, "lines", i);
      }
    });
    if (badRef.length) add("error", `${badRef.length} line(s) reference missing vertices`, "lines", badRef, () => {
      const bad = new Set(badRef); d.linedefs = d.linedefs.filter((_, i) => !bad.has(i)); ctx.compact();
    }, { fixLabel: "Delete" });
    if (zero.length) add("error", `${zero.length} zero-length line(s)`, "lines", zero, () => {
      const bad = new Set(zero); d.linedefs = d.linedefs.filter((_, i) => !bad.has(i)); ctx.compact();
    }, { fixLabel: "Delete" });
    const dupL = [...pairs.values()].filter((g) => g.length > 1);
    if (dupL.length) add("warn", `${dupL.length} overlapping line pair(s) (same vertices)`, "lines", dupL.flat());

    // Textures in use.
    const badTex = new Map();
    const note = (name, kind, i) => { const k = `${kind}:${name.toUpperCase()}`; if (!badTex.has(k)) badTex.set(k, []); badTex.get(k).push(i); };
    d.linedefs.forEach((l, i) => {
      for (const sk of ["side_front", "side_back"]) {
        const s = side(l[sk]);
        if (s) for (const p of ["upper", "middle", "lower"]) if (!known(s[p])) note(s[p], "tex", i);
      }
    });
    d.sectors.forEach((s, i) => { for (const p of ["floor_pic", "ceiling_pic"]) if (!known(s[p])) note(s[p], "flat", i); });
    for (const [k, idx] of badTex) {
      const [kind, name] = k.split(":");
      add("warn", `Unknown ${kind === "tex" ? "texture" : "flat"} ${name} ×${idx.length}`, kind === "tex" ? "lines" : "sectors", [...new Set(idx)], () => {
        if (kind === "tex") {
          for (const s of d.sidedefs) for (const p of ["upper", "middle", "lower"]) if (String(s[p]).toUpperCase() === name) s[p] = S.texture;
        } else {
          for (const s of d.sectors) for (const p of ["floor_pic", "ceiling_pic"]) if (String(s[p]).toUpperCase() === name) s[p] = S.flat;
        }
      }, { safe: true, fixLabel: kind === "tex" ? `→ ${S.texture}` : `→ ${S.flat}` });
    }

    // Sectors.
    const refs = new Uint32Array(nsec);
    const balance = Array.from({ length: nsec }, () => new Map());
    const bump = (sec, v, n) => { if (sec < nsec) balance[sec].set(v, (balance[sec].get(v) || 0) + n); };
    for (const l of d.linedefs) {
      const f = side(l.side_front), b = side(l.side_back);
      if (f && f.sector < nsec) { refs[f.sector]++; bump(f.sector, l.v1, 1); bump(f.sector, l.v2, -1); }
      if (b && b.sector < nsec) { refs[b.sector]++; bump(b.sector, l.v2, 1); bump(b.sector, l.v1, -1); }
    }
    const unused = [];
    d.sectors.forEach((s, i) => {
      if (!refs[i]) { unused.push(i); return; }
      if ([...balance[i].values()].some((n) => n !== 0)) add("error", `Sector ${i} is not closed`, "sectors", i);
      if (s.ceiling_height < s.floor_height) add("error", `Sector ${i}: ceiling (${s.ceiling_height}) below floor (${s.floor_height})`, "sectors", i,
        () => { [s.floor_height, s.ceiling_height] = [s.ceiling_height, s.floor_height]; }, { safe: true, fixLabel: "Swap" });
      if (s.light < 0 || s.light > 255) add("warn", `Sector ${i}: light ${s.light} out of 0–255`, "sectors", i, () => { s.light = Math.max(0, Math.min(255, s.light)); }, { safe: true, fixLabel: "Clamp" });
    });
    if (unused.length) add("warn", `${unused.length} sector(s) not used by any sidedef`, "sectors", unused, () => {
      const gone = new Set(unused);
      const remap = []; let n = 0;
      d.sectors.forEach((_, i) => { remap[i] = gone.has(i) ? 0 : n++; });
      d.sectors = d.sectors.filter((_, i) => !gone.has(i));
      for (const s of d.sidedefs) s.sector = remap[s.sector] ?? 0;
    }, { fixLabel: "Remove" });
    const usedSides = new Set(), usedV = new Set();
    for (const l of d.linedefs) { usedSides.add(l.side_front); usedSides.add(l.side_back); usedV.add(l.v1); usedV.add(l.v2); }
    const orphanS = d.sidedefs.length - [...usedSides].filter((s) => s !== NO_SIDE && s < ns).length;
    const orphanV = nv - [...usedV].filter((v) => v < nv).length;
    if (orphanS > 0 || orphanV > 0) add("info", `${orphanV} unused vertices, ${orphanS} unused sidedefs`, "vertices", [], () => ctx.compact(), { fixLabel: "Clean up" });
    return out;
  }

  function applyFix(issue) {
    ctx.snapshot();
    try { issue.fix(); } catch (e) { ctx.toast(ctx.errText(e), true); }
    ctx.commit();
    renderChecker();
  }

  function renderChecker() {
    if (!checker.panel || !S.doc) return;
    const issues = runChecks();
    checker.version = S.version;
    const counts = { error: 0, warn: 0, info: 0 };
    issues.forEach((i) => counts[i.sev]++);
    const safe = issues.filter((i) => i.fix && i.safe);
    const list = el("div", { class: "issues" });
    const order = { error: 0, warn: 1, info: 2 };
    issues.sort((a, b) => order[a.sev] - order[b.sev]);
    for (const it of issues.slice(0, 600)) {
      list.append(el("div", { class: `issue ${it.sev}`, onclick: () => { if (it.idx.length) show(it.mode, it.idx); } },
        el("span", { class: "sev" }, { error: "✖", warn: "▲", info: "ℹ" }[it.sev]),
        el("span", { class: "msg" }, it.msg),
        it.fix ? el("button", { class: "btn small", onclick: (e) => { e.stopPropagation(); applyFix(it); } }, it.fixLabel || "Fix") : null));
    }
    if (issues.length > 600) list.append(el("div", { class: "muted", style: "padding:6px" }, `…and ${issues.length - 600} more`));
    if (!issues.length) list.append(el("div", { class: "issue ok" }, el("span", { class: "sev" }, "✔"), el("span", { class: "msg" }, "No problems found")));
    checker.panel.querySelector(".tp-body").replaceChildren(
      el("div", { class: "tp-actions" },
        el("span", { class: "pill error" }, `${counts.error} errors`),
        el("span", { class: "pill warn" }, `${counts.warn} warnings`),
        el("span", { class: "pill" }, `${counts.info} notes`),
        el("button", { class: "btn small", onclick: renderChecker }, "Re-check"),
        el("button", { class: "btn small go", disabled: !safe.length, onclick: () => {
          ctx.snapshot();
          for (const it of safe) { try { it.fix(); } catch (_) { /* keep going */ } }
          ctx.commit();
          ctx.toast(`Applied ${safe.length} fixes`);
          renderChecker();
        } }, `Fix all safe (${safe.length})`)),
      list);
  }

  function panel(title, cls, onClose) {
    const p = el("div", { class: `tool-panel ${cls}` },
      el("div", { class: "tp-head" }, el("span", {}, title), el("button", { class: "x", title: "Close", onclick: onClose }, "✕")),
      el("div", { class: "tp-body" }));
    const head = p.querySelector(".tp-head");
    head.addEventListener("mousedown", (e) => {
      if (e.target.closest("button")) return;
      const r = p.getBoundingClientRect();
      const ox = e.clientX - r.left, oy = e.clientY - r.top;
      const move = (ev) => { p.style.left = `${Math.max(0, ev.clientX - ox)}px`; p.style.top = `${Math.max(0, ev.clientY - oy)}px`; p.style.right = "auto"; p.style.bottom = "auto"; };
      const up = () => { window.removeEventListener("mousemove", move); window.removeEventListener("mouseup", up); };
      window.addEventListener("mousemove", move);
      window.addEventListener("mouseup", up);
    });
    document.body.append(p);
    return p;
  }

  function toggleChecker() {
    if (!S.doc) return;
    if (checker.panel) { checker.panel.remove(); checker.panel = null; return; }
    checker.panel = panel(`Map check — ${S.doc.name}`, "checker", toggleChecker);
    renderChecker();
  }
  setInterval(() => {
    if (!checker.panel || !S.doc || checker.version === S.version) return;
    checker.panel.querySelector(".tp-head span").textContent = `Map check — ${S.doc.name}`;
    renderChecker();
  }, 700);

  // ------------------------------------------------------------ texture composer (TEXTURE1/2 + PNAMES)

  const u8 = (s, n = 8) => { const b = new Uint8Array(n); for (let i = 0; i < Math.min(n, s.length); i++) b[i] = s.charCodeAt(i) & 0x7f; return b; };
  const name8 = (b, o) => { let s = ""; for (let i = 0; i < 8 && b[o + i]; i++) s += String.fromCharCode(b[o + i]); return s.toUpperCase(); };

  function parsePnames(b) {
    if (b.length < 4) return [];
    const dv = new DataView(b.buffer, b.byteOffset, b.byteLength);
    const n = Math.min(dv.getInt32(0, true), Math.floor((b.length - 4) / 8));
    return Array.from({ length: Math.max(0, n) }, (_, i) => name8(b, 4 + i * 8));
  }

  function parseTextures(b, pnames, src) {
    const dv = new DataView(b.buffer, b.byteOffset, b.byteLength);
    if (b.length < 4) return { list: [], strife: false };
    const n = dv.getInt32(0, true);
    if (n < 0 || 4 + n * 4 > b.length) throw new Error(`${src}: bad header`);
    const offs = Array.from({ length: n }, (_, i) => dv.getInt32(4 + i * 4, true));
    const attempt = (strife) => {
      const head = strife ? 18 : 22, psz = strife ? 6 : 10;
      return offs.map((o) => {
        if (o < 0 || o + head > b.length) throw new Error("offset");
        const count = dv.getInt16(o + head - 2, true);
        if (count < 0 || o + head + count * psz > b.length) throw new Error("patches");
        const patches = [];
        for (let k = 0; k < count; k++) {
          const p = o + head + k * psz;
          const idx = dv.getInt16(p + 4, true);
          patches.push({ name: pnames[idx] ?? `#${idx}`, x: dv.getInt16(p, true), y: dv.getInt16(p + 2, true) });
        }
        return { name: name8(b, o), masked: dv.getInt32(o + 8, true), width: dv.getInt16(o + 12, true), height: dv.getInt16(o + 14, true), patches, src };
      });
    };
    try { return { list: attempt(false), strife: false }; } catch (_) { return { list: attempt(true), strife: true }; }
  }

  function writeTextures(list, pnames, strife) {
    const head = strife ? 18 : 22, psz = strife ? 6 : 10;
    const size = 4 + list.length * 4 + list.reduce((s, t) => s + head + t.patches.length * psz, 0);
    const b = new Uint8Array(size);
    const dv = new DataView(b.buffer);
    dv.setInt32(0, list.length, true);
    let o = 4 + list.length * 4;
    list.forEach((t, i) => {
      dv.setInt32(4 + i * 4, o, true);
      b.set(u8(t.name), o);
      dv.setInt32(o + 8, t.masked | 0, true);
      dv.setInt16(o + 12, t.width, true);
      dv.setInt16(o + 14, t.height, true);
      dv.setInt16(o + head - 2, t.patches.length, true);
      t.patches.forEach((p, k) => {
        const q = o + head + k * psz;
        dv.setInt16(q, p.x, true);
        dv.setInt16(q + 2, p.y, true);
        dv.setInt16(q + 4, pnames.indexOf(p.name.toUpperCase()), true);
        if (!strife) { dv.setInt16(q + 6, 1, true); dv.setInt16(q + 8, 0, true); }
      });
      o += head + t.patches.length * psz;
    });
    return b;
  }

  function writePnames(names) {
    const b = new Uint8Array(4 + names.length * 8);
    new DataView(b.buffer).setInt32(0, names.length, true);
    names.forEach((n, i) => b.set(u8(n), 4 + i * 8));
    return b;
  }

  const patchCache = new Map();
  function patchImage(name) {
    const key = name.toUpperCase();
    if (patchCache.has(key)) return patchCache.get(key);
    let out = null;
    try {
      const img = S.mod.roomEditorPreviewPatch(key);
      if (img) {
        out = document.createElement("canvas");
        out.width = img.w; out.height = img.h;
        out.getContext("2d").putImageData(new ImageData(img.rgba, img.w, img.h), 0, 0);
      }
    } catch (_) { out = null; }
    patchCache.set(key, out);
    return out;
  }

  const lastLump = (name) => [...(S.summary?.lumps || [])].reverse().find((l) => l.name === name && l.ns === "global");

  function openComposer() {
    if (!S.summary) { ctx.toast("Open a WAD first", true); return; }
    const id = "tool:composer";
    if (S.lumpTabs.has(id)) { ctx.selectDocTab(id); return; }
    const C = { list: [], pnames: [], strife: false, sel: 0, patch: -1, zoom: 2, dirty: false, sources: [] };
    try {
      const pl = lastLump("PNAMES");
      C.pnames = pl ? parsePnames(ctx.lumpBytes(pl)) : [];
      for (const nm of ["TEXTURE1", "TEXTURE2"]) {
        const l = lastLump(nm);
        if (!l) continue;
        const r = parseTextures(ctx.lumpBytes(l), C.pnames, nm);
        C.strife ||= r.strife;
        C.list.push(...r.list);
        C.sources.push(`${nm} (${l.src}, ${r.list.length})`);
      }
    } catch (e) { ctx.toast(`Texture lumps: ${ctx.errText(e)}`, true); }
    const allPatches = [...new Set([...C.pnames, ...(S.summary.patches || [])])].sort();

    const view = el("div", { class: "composer" });
    const filter = el("input", { type: "search", placeholder: "Filter textures…" });
    const listEl = el("div", { class: "cp-list" });
    const canvas = el("canvas", { class: "preview-canvas cp-canvas", tabindex: "0" });
    const props = el("div", { class: "cp-props" });
    const dirtyPill = el("span", { class: "pill" }, "saved");
    const zoomSel = el("select", {}, ...[1, 2, 3, 4, 6].map((z) => el("option", { value: z, selected: z === C.zoom }, `${z}×`)));
    zoomSel.addEventListener("change", () => { C.zoom = Number(zoomSel.value); draw(); });
    const cur = () => C.list[C.sel];
    const markDirty = () => { C.dirty = true; dirtyPill.textContent = "unsaved"; dirtyPill.className = "pill warn"; };

    function renderList() {
      const q = filter.value.trim().toUpperCase();
      const rows = [];
      C.list.forEach((t, i) => {
        if (q && !t.name.includes(q)) return;
        if (rows.length > 800) return;
        rows.push(el("div", { class: `list-item${i === C.sel ? " active" : ""}`, onclick: () => { C.sel = i; C.patch = -1; renderList(); renderProps(); draw(); } },
          el("span", {}, t.name), el("span", { class: "meta" }, `${t.width}×${t.height} · ${t.patches.length}p · ${t.src}`)));
      });
      listEl.replaceChildren(...rows);
    }

    function draw() {
      const t = cur();
      const c = canvas.getContext("2d");
      if (!t) { canvas.width = 1; canvas.height = 1; return; }
      const z = C.zoom, pad = 16;
      canvas.width = t.width * z + pad * 2; canvas.height = t.height * z + pad * 2;
      c.imageSmoothingEnabled = false;
      c.save();
      c.beginPath(); c.rect(pad, pad, t.width * z, t.height * z); c.clip();
      t.patches.forEach((p) => {
        const img = patchImage(p.name);
        if (img) c.drawImage(img, pad + p.x * z, pad + p.y * z, img.width * z, img.height * z);
        else { c.fillStyle = "rgba(255,60,60,0.35)"; c.fillRect(pad + p.x * z, pad + p.y * z, 32 * z, 32 * z); }
      });
      c.restore();
      c.strokeStyle = "rgba(255,255,255,0.6)"; c.setLineDash([4, 3]);
      c.strokeRect(pad - 0.5, pad - 0.5, t.width * z + 1, t.height * z + 1);
      c.setLineDash([]);
      t.patches.forEach((p, k) => {
        const img = patchImage(p.name);
        const w = (img?.width || 32) * z, h = (img?.height || 32) * z;
        c.strokeStyle = k === C.patch ? "#ffcc33" : "rgba(90,176,255,0.45)";
        c.lineWidth = k === C.patch ? 2 : 1;
        c.strokeRect(pad + p.x * z + 0.5, pad + p.y * z + 0.5, w - 1, h - 1);
      });
    }

    function patchAt(mx, my) {
      const t = cur();
      const z = C.zoom, pad = 16;
      for (let k = t.patches.length - 1; k >= 0; k--) {
        const p = t.patches[k];
        const img = patchImage(p.name);
        const w = img?.width || 32, h = img?.height || 32;
        const x = (mx - pad) / z, y = (my - pad) / z;
        if (x >= p.x && y >= p.y && x < p.x + w && y < p.y + h) return k;
      }
      return -1;
    }

    let drag = null;
    canvas.addEventListener("mousedown", (e) => {
      if (!cur()) return;
      canvas.focus();
      const r = canvas.getBoundingClientRect();
      const k = patchAt(e.clientX - r.left, e.clientY - r.top);
      C.patch = k;
      if (k >= 0) drag = { x: e.clientX, y: e.clientY, px: cur().patches[k].x, py: cur().patches[k].y };
      renderProps(); draw();
    });
    window.addEventListener("mousemove", (e) => {
      if (!drag || !cur() || C.patch < 0) return;
      const p = cur().patches[C.patch];
      const g = e.shiftKey ? 8 : 1;
      p.x = clampI16(Math.round((drag.px + (e.clientX - drag.x) / C.zoom) / g) * g);
      p.y = clampI16(Math.round((drag.py + (e.clientY - drag.y) / C.zoom) / g) * g);
      markDirty(); draw();
    });
    window.addEventListener("mouseup", () => { if (drag) { drag = null; renderProps(); } });
    canvas.addEventListener("keydown", (e) => {
      const t = cur();
      if (!t || C.patch < 0) return;
      const step = e.shiftKey ? 8 : 1;
      const mv = { ArrowLeft: [-step, 0], ArrowRight: [step, 0], ArrowUp: [0, -step], ArrowDown: [0, step] }[e.key];
      if (mv) { const p = t.patches[C.patch]; p.x += mv[0]; p.y += mv[1]; markDirty(); draw(); renderProps(); e.preventDefault(); e.stopPropagation(); }
      if (e.key === "Delete") { t.patches.splice(C.patch, 1); C.patch = -1; markDirty(); draw(); renderProps(); e.preventDefault(); e.stopPropagation(); }
    });

    const dl = el("datalist", { id: "cp-patches" }, ...allPatches.map((p) => el("option", { value: p })));
    function num(val, onset, w = 64) {
      const i = el("input", { type: "number", value: val, style: `width:${w}px` });
      i.addEventListener("change", () => { onset(clampI16(Number(i.value) || 0)); markDirty(); draw(); renderList(); });
      return i;
    }
    function renderProps() {
      const t = cur();
      if (!t) { props.replaceChildren(el("p", { class: "muted" }, "No texture selected")); return; }
      const nameIn = el("input", { type: "text", value: t.name, maxlength: 8, style: "text-transform:uppercase;width:110px", spellcheck: "false" });
      nameIn.addEventListener("change", () => { t.name = nameIn.value.trim().toUpperCase().slice(0, 8) || t.name; markDirty(); renderList(); });
      const rows = t.patches.map((p, k) => {
        const pn = el("input", { type: "text", value: p.name, list: "cp-patches", style: "width:96px;text-transform:uppercase", spellcheck: "false" });
        pn.addEventListener("change", () => { p.name = pn.value.trim().toUpperCase().slice(0, 8); markDirty(); draw(); });
        const move = (dir) => { const j = k + dir; if (j < 0 || j >= t.patches.length) return; [t.patches[k], t.patches[j]] = [t.patches[j], t.patches[k]]; C.patch = j; markDirty(); renderProps(); draw(); };
        return el("tr", { class: k === C.patch ? "active" : "", onclick: (e) => { if (e.target.tagName === "TD") { C.patch = k; renderProps(); draw(); } } },
          el("td", {}, k), el("td", {}, pn),
          el("td", {}, num(p.x, (v) => { p.x = v; }, 58)), el("td", {}, num(p.y, (v) => { p.y = v; }, 58)),
          el("td", {}, el("button", { class: "btn small icon", title: "Draw earlier", onclick: () => move(-1) }, "↑"),
            el("button", { class: "btn small icon", title: "Draw later", onclick: () => move(1) }, "↓"),
            el("button", { class: "btn small icon danger", title: "Remove", onclick: () => { t.patches.splice(k, 1); C.patch = -1; markDirty(); renderProps(); draw(); } }, "✕")));
      });
      const addIn = el("input", { type: "text", list: "cp-patches", placeholder: "patch name", style: "width:110px;text-transform:uppercase", spellcheck: "false" });
      const addBtn = el("button", { class: "btn small", onclick: () => {
        const n = addIn.value.trim().toUpperCase();
        if (!n) return;
        if (!patchImage(n)) ctx.toast(`No patch named ${n} (it will draw as a red box)`, true);
        t.patches.push({ name: n, x: 0, y: 0 });
        C.patch = t.patches.length - 1;
        markDirty(); renderProps(); draw(); renderList();
      } }, "Add patch");
      props.replaceChildren(
        el("fieldset", {}, el("legend", {}, "Texture"),
          el("div", { class: "row" }, el("label", {}, "Name"), nameIn),
          el("div", { class: "row" }, el("label", {}, "Width"), num(t.width, (v) => { t.width = Math.max(1, v); })),
          el("div", { class: "row" }, el("label", {}, "Height"), num(t.height, (v) => { t.height = Math.max(1, v); })),
          el("div", { class: "row" }, el("label", {}, "Lump"), el("span", {}, t.src))),
        el("fieldset", {}, el("legend", {}, `Patches (${t.patches.length})`),
          el("table", { class: "cp-patches" }, el("thead", {}, el("tr", {}, el("th", {}, "#"), el("th", {}, "Patch"), el("th", {}, "X"), el("th", {}, "Y"), el("th", {}, ""))), el("tbody", {}, ...rows)),
          el("div", { class: "tex-field", style: "margin-top:6px" }, addIn, addBtn)),
        el("p", { class: "muted" }, "Drag patches on the canvas (Shift = 8px snap) · arrows nudge · Del removes · Apply pushes TEXTURE1/2 + PNAMES into the running game."),
        dl);
    }

    function apply() {
      const names = [...C.pnames];
      for (const t of C.list) for (const p of t.patches) { const n = p.name.toUpperCase(); if (!names.includes(n)) names.push(n); }
      try {
        S.mod.roomEditorSetLump("global", "PNAMES", writePnames(names));
        for (const src of ["TEXTURE1", "TEXTURE2"]) {
          const list = C.list.filter((t) => t.src === src);
          if (list.length || lastLump(src)) S.mod.roomEditorSetLump("global", src, writeTextures(list, names, C.strife));
        }
        ctx.clearThumbs();
        ctx.setSummary(S.mod.roomEditorApplyLumps());
        ctx.setPlaying(false);
        C.pnames = names;
        C.dirty = false; dirtyPill.textContent = "applied"; dirtyPill.className = "pill live";
        ctx.toast(`Textures applied — ${C.list.length} composites live in the 3D view`);
      } catch (e) { ctx.toast(`Apply failed: ${ctx.errText(e)}`, true); }
    }

    const newTex = (base) => {
      const t = base ? structuredClone(base) : { name: "NEWTEX", masked: 0, width: 64, height: 128, patches: [], src: "TEXTURE1" };
      let n = 1;
      const stem = t.name.slice(0, 6);
      while (C.list.some((x) => x.name === t.name)) t.name = `${stem}${n++}`.slice(0, 8);
      C.list.push(t);
      C.sel = C.list.length - 1; C.patch = -1;
      markDirty(); renderList(); renderProps(); draw();
      listEl.querySelector(".active")?.scrollIntoView({ block: "nearest" });
    };

    filter.addEventListener("input", renderList);
    view.append(
      el("div", { class: "doc-head" }, el("h2", {}, "Texture composer"),
        el("span", { class: "muted" }, C.sources.length ? C.sources.join(" · ") : "no TEXTURE1 in this WAD"), dirtyPill),
      el("div", { class: "actions" },
        el("button", { class: "btn small", onclick: () => newTex(null) }, "+ New texture"),
        el("button", { class: "btn small", onclick: () => cur() && newTex(cur()) }, "Duplicate"),
        el("button", { class: "btn small danger", onclick: () => { if (!cur()) return; C.list.splice(C.sel, 1); C.sel = Math.max(0, C.sel - 1); markDirty(); renderList(); renderProps(); draw(); } }, "Delete"),
        el("button", { class: "btn small", onclick: () => { if (cur()) { S.texture = cur().name; ctx.renderInspector(); ctx.toast(`Paint texture: ${cur().name}`); } } }, "Use as paint"),
        el("button", { class: "btn small", title: "Paint the composed pixels (saved as a TX_START texture)", onclick: () => { if (cur()) { if (C.dirty) apply(); ctx.openPaint("textures", cur().name); } } }, "✎ Edit pixels"),
        el("label", { class: "muted" }, "Zoom ", zoomSel),
        el("button", { class: "btn small go", onclick: apply }, "Apply to game")),
      el("div", { class: "cp-grid" }, el("div", { class: "cp-left" }, filter, listEl), el("div", { class: "cp-center" }, canvas), props));
    renderList(); renderProps(); draw();
    S.lumpTabs.set(id, { title: "🧱 Textures", view });
    ctx.selectDocTab(id);
  }

  // ------------------------------------------------------------ live play-test panel

  const play = el("div", { class: "playtools", hidden: true });
  ctx.room.parentElement.append(play);
  const statEl = el("div", { class: "pt-stats" });
  const cheat = (cmd, arg = "") => {
    try { ctx.status(S.mod.roomEditorCheat(cmd, arg)); } catch (e) { ctx.toast(ctx.errText(e), true); }
    ctx.room.focus();
    poll(true);
  };
  const toggles = {};
  const tbtn = (cmd, label, title) => (toggles[cmd] = el("button", { class: "btn small", title, onclick: () => cheat(cmd) }, label));
  const summonIn = el("input", { type: "text", list: "pt-classes", placeholder: "Actor class", spellcheck: "false" });
  const classes = el("datalist", { id: "pt-classes" });
  const hpIn = el("input", { type: "number", value: 100, min: 1, max: 999, style: "width:58px" });
  const arIn = el("input", { type: "number", value: 200, min: 1, max: 200, style: "width:58px" });
  for (const i of [summonIn, hpIn, arIn]) i.addEventListener("keydown", (e) => e.stopPropagation());
  play.append(
    el("div", { class: "pt-head" }, el("strong", {}, "Live test"), statEl),
    el("div", { class: "pt-row" },
      tbtn("god", "God", "Invulnerable (iddqd)"), tbtn("noclip", "Noclip", "Walk through walls (idclip)"),
      tbtn("freeze", "Freeze", "Freeze monsters & movers"),
      el("button", { class: "btn small", title: "Weapons, ammo, keys, armor", onclick: () => cheat("give", "all") }, "Give all"),
      el("button", { class: "btn small", onclick: () => cheat("give", "keys") }, "Keys"),
      el("button", { class: "btn small", onclick: () => cheat("give", "ammo") }, "Ammo"),
      el("button", { class: "btn small danger", title: "Kill every monster", onclick: () => cheat("kill") }, "Kill monsters")),
    el("div", { class: "pt-row" },
      el("label", {}, "HP ", hpIn), el("button", { class: "btn small", onclick: () => cheat("health", hpIn.value) }, "Set"),
      el("label", {}, "Armor ", arIn), el("button", { class: "btn small", onclick: () => cheat("armor", arIn.value) }, "Set"),
      el("button", { class: "btn small", title: "Die and test respawn", onclick: () => cheat("suicide") }, "Die")),
    el("div", { class: "pt-row" }, summonIn, classes,
      el("button", { class: "btn small", title: "Spawn in front of the player", onclick: () => summonIn.value.trim() && cheat("summon", summonIn.value.trim()) }, "Summon")));
  let classesFor = null;
  function poll(force) {
    let st = null;
    if (S.playing && S.mod) { try { const j = S.mod.roomEditorPlayerState(); st = j ? JSON.parse(j) : null; } catch (_) { st = null; } }
    const visible = !!(st && st.playing);
    if (play.hidden === visible) play.hidden = !visible;
    if (!visible) return;
    if (classesFor !== S.summary) {
      classesFor = S.summary;
      classes.replaceChildren(...(S.summary?.things || []).filter((t) => t.cat === "monster" || t.cat === "weapon" || t.cat === "pickup")
        .map((t) => el("option", { value: t.name }, t.title)));
    }
    const t = st.time, mmss = `${Math.floor(t / 60)}:${String(t % 60).padStart(2, "0")}`;
    statEl.textContent = `HP ${st.health} · AR ${st.armor} · ammo ${st.ammo} · kills ${st.kills}/${st.totalKills} · secrets ${st.secrets}/${st.totalSecrets} · ${mmss} · (${Math.round(st.x)}, ${Math.round(st.y)}, ${Math.round(st.z)})`;
    toggles.god.classList.toggle("active", st.god);
    toggles.noclip.classList.toggle("active", st.noclip);
    toggles.freeze.classList.toggle("active", st.frozen);
  }
  setInterval(poll, 250);

  // ------------------------------------------------------------ keys

  /** Handle a tool shortcut; returns true when consumed. */
  function onKey(e, { mod, in3d }) {
    if (S.playing) return false;
    if (mod && e.code === "KeyC") { if (copySelection()) ctx.status(`Copied ${S.sel.size} ${S.mode}`); e.preventDefault(); return true; }
    if (mod && e.code === "KeyX") { if (copySelection()) ctx.deleteSelection(); e.preventDefault(); return true; }
    if (mod && e.code === "KeyV") { e.preventDefault(); if (clip) paste(pastePoint(false)); else ctx.toast("Clipboard is empty"); return true; }
    if (mod && e.code === "KeyD") { e.preventDefault(); if (copySelection()) paste(pastePoint(true)); return true; }
    if (mod) return false;
    if (e.code === "F8") { e.preventDefault(); toggleChecker(); return true; }
    if (e.code === "KeyJ") { autoAlignSelection(); return true; }
    if (in3d) return false;
    if (e.code === "KeyR") { transform(e.shiftKey ? "ccw" : "cw"); return true; }
    if (e.code === "KeyH") { transform(e.shiftKey ? "mirrorY" : "mirrorX"); return true; }
    return false;
  }

  return { onKey, toggleChecker, openComposer, transform, autoAlignSelection, copy: copySelection, paste: () => clip && paste(pastePoint(false)) };
}
