// Scripts panel: WASM mods that control the game (spawn, damage, give items,
// change states, react to kills / pickups / map start, end the level…).
// Modules are written in AssemblyScript or WAT (both compiled in the browser;
// saving during a play test hot-swaps them) or imported as .wasm
// (e.g. built with room-script-sdk), and saved as ROOMMOD + module lumps in
// the override PWAD, so play tests and "Export WAD" carry them.

const EXAMPLES = {
  survival: { file: "survival.wad", label: "Survival Director", note: "waves start after ~20 s on HMP" },
  bounty: { file: "bounty.wad", label: "Bounty Hunter", note: "first bounty after 5 s · \"Copy into editor for editing\" opens its AssemblyScript source" },
};
const LOG_CAP = 400;

// ------------------------------------------------------------ highlighting

const words = (s) => new Set(s.split(" "));
const AS_KEYWORDS = words("abstract as break case catch class const constructor continue declare default delete do else enum export extends false finally for from function get if implements import in instanceof interface let namespace new null private protected public readonly return set static super switch this throw true try type typeof var while");
const AS_TYPES = words("i8 i16 i32 i64 isize u8 u16 u32 u64 usize f32 f64 v128 bool void string number boolean any never unknown Array StaticArray ArrayBuffer Map Set String Math");
const WAT_KEYWORDS = words("module func param result local global import export memory data table elem type mut start offset then if else end loop block br br_if br_table return call call_indirect drop select unreachable nop");

// Order matters: comments and strings first so their contents are not re-tokenized.
const AS_TOKENS = /(\/\/[^\n]*|\/\*[\s\S]*?(?:\*\/|$))|("(?:[^"\\\n]|\\.)*"?|'(?:[^'\\\n]|\\.)*'?|`(?:[^`\\]|\\[\s\S])*`?)|(@[A-Za-z_]\w*)|(\b(?:0[xX][0-9a-fA-F_]+|0[bB][01_]+|\d[\d_]*(?:\.\d+)?(?:[eE][+-]?\d+)?)\b)|([A-Za-z_$][\w$]*)/g;
const WAT_TOKENS = /(;;[^\n]*|\(;[\s\S]*?(?:;\)|$))|("(?:[^"\\\n]|\\.)*"?)|(\$[\w.$!#%&*+\-/:<=>?@\\^`|~']+)|((?:[if](?:32|64)|v128)\.[\w.]+|(?:local|global|memory|table|ref)\.[\w.]+)|(\b[-+]?(?:0x[0-9a-fA-F_]+|\d[\d_]*(?:\.\d+)?(?:e[+-]?\d+)?)\b)|([a-z_][\w.]*)/g;
const CALL_AHEAD = /\s*\(/y;

const escapeHtml = (s) => s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");

function asClass(m, src, end) {
  if (m[1]) return "c";
  if (m[2]) return "s";
  if (m[3]) return "d";
  if (m[4]) return "n";
  const w = m[5];
  if (AS_KEYWORDS.has(w)) return "k";
  if (AS_TYPES.has(w)) return "t";
  CALL_AHEAD.lastIndex = end;
  if (CALL_AHEAD.test(src)) return "f";
  if (/^[A-Z][A-Z0-9_]+$/.test(w)) return "n";
  return /^[A-Z]/.test(w) ? "t" : null;
}

function watClass(m) {
  if (m[1]) return "c";
  if (m[2]) return "s";
  if (m[3]) return "v";
  if (m[4]) return "f";
  if (m[5]) return "n";
  if (WAT_KEYWORDS.has(m[6])) return "k";
  return /^(?:i32|i64|f32|f64|v128|funcref|externref|anyfunc)$/.test(m[6]) ? "t" : null;
}

const ZS_KEYWORDS = words("abstract action break case class clearscope const continue default deprecated do else enum extend fail false final for goto if in include internal let loop meta mixin native null out override play private property protected public readonly return states static stop struct super switch this transient true ui version virtual void wait while");
const ZS_TYPES = words("int int8 int16 uint uint8 uint16 uint32 double float bool string Name color Vector2 Vector3 State StateLabel Sound TextureID TranslationID Class Actor Thinker void");
const ZS_TOKENS = /(#[A-Za-z_]\w*)|(\/\/[^\n]*|\/\*[\s\S]*?(?:\*\/|$))|("(?:[^"\\\n]|\\.)*"?|'(?:[^'\\\n]|\\.)*'?)|(\b(?:0[xX][0-9a-fA-F_]+|\d[\d_]*(?:\.\d+)?(?:[eE][+-]?\d+)?)\b)|([A-Za-z_][\w]*)/g;

function zsClass(m, src, end) {
  if (m[1]) return "d";
  if (m[2]) return "c";
  if (m[3]) return "s";
  if (m[4]) return "n";
  const w = m[5];
  if (!w) return null;
  const folded = w.toLowerCase();
  const allCaps = w === w.toUpperCase() && /[A-Z]/.test(w);
  if (ZS_KEYWORDS.has(w) || (!allCaps && ZS_KEYWORDS.has(folded))) return "k";
  if (ZS_TYPES.has(w) || ZS_TYPES.has(folded)) return "t";
  if (/^A_/.test(w)) return "f";
  CALL_AHEAD.lastIndex = end;
  if (CALL_AHEAD.test(src)) return "f";
  const colon = /\s*:/y;
  colon.lastIndex = end;
  const lineStart = src.lastIndexOf("\n", m.index) + 1;
  const prefix = src.slice(lineStart, m.index);
  if (colon.test(src) && !/\b(?:class|extend|mixin)\b/i.test(prefix)) return "f";
  return /^[A-Z]/.test(w) ? "t" : null;
}

/** Source → HTML with `<span class="hl-*">` tokens (AssemblyScript, WAT, or ZScript). */
export function highlight(src, lang) {
  const re = lang === "wat" ? WAT_TOKENS : lang === "zs" ? ZS_TOKENS : AS_TOKENS;
  re.lastIndex = 0;
  let out = "";
  let last = 0;
  for (let m; (m = re.exec(src)); ) {
    const end = re.lastIndex;
    const cls = lang === "wat" ? watClass(m) : lang === "zs" ? zsClass(m, src, end) : asClass(m, src, end);
    if (!cls) continue;
    out += escapeHtml(src.slice(last, m.index)) + `<span class="hl-${cls}">${escapeHtml(m[0])}</span>`;
    last = end;
  }
  // A trailing newline needs content after it or the last line has no height.
  return out + escapeHtml(src.slice(last)) + "\n ";
}

const TEMPLATE = `(module
  ;; Room WASM mod (play scope). Callbacks: room_dispatch(callback_base + event bit).
  ;; Event bits: 0 Tick [i32 maptime] · 1 ThingDied [u64 thing][u64 source]
  ;;   3 Loaded · 4 ThingSpawned [u64] · 5 ThingDamaged [u64 thing][u64 src][u64 inflictor][i32 dmg][i32 hp]
  ;;   6 PlayerEntered / 7 PlayerDied [u64 actor][i32 player] · 9 ItemPickup [u64 toucher][i32 class]
  ;; actor_get_i32 / actor_set_i32: 0 HEALTH · 4 IS_MONSTER · 5 PLAYER (slot+1) · 7 IS_DEAD
  (import "room" "reg_event_handler" (func $reg_handler (param i32 i32 i32 i32 i32 i32 i32) (result i32)))
  (import "room" "player_print" (func $print (param i32 i32 i32)))
  (import "room" "actor_get_i32" (func $get_i32 (param i64 i32) (result i32)))
  (import "room" "actor_set_i32" (func $set_i32 (param i64 i32 i32)))
  (memory (export "memory") 1)
  (global $heap (mut i32) (i32.const 8192))
  (data (i32.const 0) "HelloMod")
  (data (i32.const 16) "Hello from a WASM mod!")
  (data (i32.const 48) "Monster down: +10 health")
  (func (export "room_abi_version") (result i32) (i32.const 1))
  (func (export "room_alloc") (param $size i32) (param $align i32) (result i32)
    (local $p i32)
    (local.set $p (global.get $heap))
    (global.set $heap (i32.add (local.get $p) (local.get $size)))
    (local.get $p))
  (func (export "room_free") (param i32 i32 i32))
  ;; Subscribe to Loaded (bit 3) + ThingDied (bit 1): mask 0b1010 = 10, callbacks 100 + bit.
  (func (export "room_register") (result i32)
    (drop (call $reg_handler (i32.const 0) (i32.const 8) (i32.const 0) (i32.const 0)
      (i32.const 10) (i32.const 0) (i32.const 100)))
    (i32.const 0))
  (func (export "room_init") (param i32) (result i32) (i32.const 0))
  (func (export "room_dispatch") (param $id i32) (param $ptr i32) (param $len i32) (result i64)
    (local $killer i64)
    (if (i32.eq (local.get $id) (i32.const 103))
      (then (call $print (i32.const 0) (i32.const 16) (i32.const 22))))
    (if (i32.eq (local.get $id) (i32.const 101))
      (then
        (local.set $killer (i64.load offset=8 (local.get $ptr)))
        (if (i32.and
              (i32.ne (call $get_i32 (i64.load (local.get $ptr)) (i32.const 4)) (i32.const 0))
              (i32.ne (call $get_i32 (local.get $killer) (i32.const 5)) (i32.const 0)))
          (then
            (call $set_i32 (local.get $killer) (i32.const 0)
              (i32.add (call $get_i32 (local.get $killer) (i32.const 0)) (i32.const 10)))
            (call $print (i32.const 0) (i32.const 48) (i32.const 24))))))
    (i64.const 0))
)
`;

const ASC_VERSION = "0.28.20";
const ASC_URLS = ["./vendor/asc/asc.js", `https://cdn.jsdelivr.net/npm/assemblyscript@${ASC_VERSION}/dist/asc.js/+esm`];

const AS_TEMPLATE = `// AssemblyScript mod (TypeScript syntax, compiled to WASM in the browser).
// Ctrl+S while playing reloads it live. Full API: see "Game API (room.ts)" below.
import { RoomMod, Actor, register, print, player, spawn, skill, random, Spawn, TICRATE } from "./room";

class MyMod extends RoomMod {
  kills: i32 = 0;

  onLoad(maptime: i32): void {
    print("Hello from AssemblyScript! Skill " + skill().toString());
  }

  onTick(maptime: i32): void {
    // Every 20 seconds, teleport an imp in front of the player.
    if (maptime > 0 && maptime % (20 * TICRATE) == 0) {
      const p = player(0);
      if (p == null) return;
      const pos = p.pos;
      const a = p.angle * Math.PI / 180;
      spawn("DoomImp", pos.x + Math.cos(a) * 320, pos.y + Math.sin(a) * 320, NaN, p.angle + 180,
        Spawn.TELEFOG | Spawn.CHECK_POSITION);
    }
  }

  onThingDied(thing: Actor, killer: Actor | null): void {
    if (killer == null || !killer.isPlayer || !thing.isMonster) return;
    this.kills++;
    killer.health = min(killer.health + 5, 200);
    if (this.kills % 5 == 0) {
      killer.give("Shell", 4);
      print(this.kills.toString() + " kills - have some shells!");
    }
  }
}

register(new MyMod("MyMod"));
`;

let ascPromise = null;
let libPromise = null;

/** The AssemblyScript compiler: vendored copy first, then the CDN. */
function loadAsc() {
  if (!ascPromise) {
    ascPromise = (async () => {
      let last = null;
      for (const url of ASC_URLS) {
        try { return (await import(url)).default; } catch (e) { last = e; }
      }
      ascPromise = null;
      throw new Error(`AssemblyScript compiler unavailable (run ./scripts/fetch-assemblyscript.sh or go online): ${last && last.message}`);
    })();
  }
  return ascPromise;
}

function loadLib() {
  if (!libPromise) {
    libPromise = Promise.all(["room.ts", "room_abi.ts"].map(async (f) => {
      const r = await fetch(`./as/${f}`, { cache: "no-cache" });
      if (!r.ok) throw new Error(`./as/${f}: HTTP ${r.status}`);
      return [f, await r.text()];
    })).then(Object.fromEntries).catch((e) => { libPromise = null; throw e; });
  }
  return libPromise;
}

/** Compile an AssemblyScript mod to WASM bytes (throws the compiler's messages). */
async function compileAs(source) {
  const [asc, lib] = await Promise.all([loadAsc(), loadLib()]);
  const files = { ...lib, "input.ts": /export\s*\*\s*from\s*["']\.\/room_abi["']/.test(source) ? source : `${source}\nexport * from "./room_abi";\n` };
  const out = {};
  const res = await asc.main(["input.ts", "--runtime", "incremental", "--use", "abort=room/roomAbort", "-O2", "--noColors", "-o", "out.wasm"], {
    readFile: (name) => (name in files ? files[name] : null),
    writeFile: (name, data) => { out[name] = data; },
    listFiles: () => [],
  });
  const msg = res.stderr.toString().replace(/\nFAILURE[^\n]*\n*$/, "").trim();
  if (res.error || !out["out.wasm"]) throw new Error(msg || (res.error && res.error.message) || "compile failed");
  const bytes = out["out.wasm"];
  const foreign = WebAssembly.Module.imports(new WebAssembly.Module(bytes)).filter((i) => i.module !== "room");
  if (foreign.length) {
    const names = foreign.map((i) => `${i.module}.${i.name}`).join(", ");
    const hint = foreign.some((i) => i.name === "seed") ? " — use random(lo, hi) instead of Math.random()" : "";
    throw new Error(`the module imports ${names}, which mods can't use${hint}`);
  }
  return bytes;
}

const langOf = (src) => (src == null ? "wasm" : /^(\s*;;[^\n]*\n)*\s*\(module/.test(src) ? "wat" : "as");
const LANG_LABEL = { wat: "WAT", as: "AS", wasm: "wasm" };

export function install(ctx) {
  const { S, el } = ctx;
  S.scriptLog = S.scriptLog || [];
  const logViews = new Set();
  let api = null;

  const state = () => JSON.parse(S.mod.roomEditorScripts());

  function pushLog(lines) {
    if (!lines.length) return;
    S.scriptLog.push(...lines);
    if (S.scriptLog.length > LOG_CAP) S.scriptLog.splice(0, S.scriptLog.length - LOG_CAP);
    for (const v of logViews) {
      if (!v.isConnected) { logViews.delete(v); continue; }
      v.textContent = S.scriptLog.join("\n");
      v.scrollTop = v.scrollHeight;
    }
  }

  function pollLog() {
    if (S.mod) { try { pushLog(S.mod.roomEditorScriptLog()); } catch (_) {} }
    setTimeout(pollLog, 300);
  }
  pollLog();

  function logView() {
    const pre = el("pre", { class: "script-log" }, S.scriptLog.join("\n") || "(mod log — info lines, warnings and errors appear here)");
    logViews.add(pre);
    return el("fieldset", {}, el("legend", {}, "Mod log"), pre,
      el("button", { class: "btn small", onclick: () => { S.scriptLog.length = 0; pre.textContent = ""; } }, "Clear"));
  }

  /** Restart the level so the mod set is reloaded (fresh mod session). */
  function apply(msg) {
    ctx.setSummary(S.mod.roomEditorApplyLumps());
    ctx.setPlaying(false);
    ctx.toast(msg || "Mods reloaded — press ▶ Play to test");
    ctx.renderBrowser();
  }

  /** Store the module; while a play test runs, swap it in live (no restart). */
  function save(lump, name, wasm, source) {
    const res = JSON.parse(S.mod.roomEditorSetScript(lump, name, wasm, source));
    pushLog(res.log || []);
    if (S.playing) {
      const live = JSON.parse(S.mod.roomEditorLiveReloadScripts());
      if (live.live) {
        ctx.toast(`${lump.toUpperCase()} reloaded live (${live.loaded.join(", ") || "no mods"})`);
        ctx.renderBrowser();
        return;
      }
    }
    apply(`${lump.toUpperCase()} compiled & registered — press ▶ Play to test`);
  }

  let asApi = null;
  function asApiView() {
    const pre = el("pre", {}, "Loading…");
    if (!asApi) asApi = loadLib().then((l) => l["room.ts"]);
    asApi.then((t) => { pre.innerHTML = highlight(t, "as"); }, (e) => { pre.textContent = ctx.errText(e); });
    return el("details", { class: "script-api" }, el("summary", {}, "Game API (room.ts) · import { … } from \"./room\""), pre);
  }

  function apiView() {
    if (!api) api = JSON.parse(S.mod.roomEditorScriptApi());
    const rows = api.map((f) => `(import "room" "${f.name}" (func${f.params ? ` (param ${f.params})` : ""}${f.result ? ` (result ${f.result})` : ""}))`);
    return el("details", { class: "script-api" }, el("summary", {}, `Host API (${api.length} imports) · ActorRef = i64 · ClassRef = class_find(ptr, len) · player = slot + 1`),
      Object.assign(el("pre"), { innerHTML: highlight(rows.join("\n"), "wat") }));
  }

  function moduleView(lump) {
    let mod;
    try { mod = S.mod.roomEditorScriptGet(lump); } catch (e) { return el("p", { class: "err-box" }, ctx.errText(e)); }
    const info = state().modules.find((m) => m.lump === lump) || { name: lump.toLowerCase() };
    const nameIn = el("input", { type: "text", value: info.name, title: "Module name (log prefix / identity)" });
    const errBox = el("div", { class: "err-box", hidden: true });
    const showErr = (e) => { errBox.hidden = false; errBox.textContent = ctx.errText(e); };
    const lang = langOf(mod.source);
    const head = el("div", { class: "doc-head" }, el("h2", {}, `Script ${lump}`),
      el("span", { class: "kind text" }, { wat: "WAT", as: "AssemblyScript", wasm: "wasm" }[lang]),
      el("label", {}, "Name ", nameIn));
    const actions = el("div", { class: "actions" });
    const wrap = el("div", { class: "script-doc" }, head, actions);
    if (mod.source != null) {
      const COMPILE_LABEL = "Compile & apply (Ctrl+S)";
      const compileBtn = el("button", { class: "btn primary", title: "While playing, the mod is swapped in live", onclick: () => compile() }, COMPILE_LABEL);
      const ta = el("textarea", { class: "code hl-input", spellcheck: "false" });
      ta.value = mod.source;
      const gutter = el("div", { class: "gutter" });
      const hl = el("pre", { class: "code-hl", "aria-hidden": "true" });
      const syncScroll = () => { gutter.scrollTop = ta.scrollTop; hl.scrollTop = ta.scrollTop; hl.scrollLeft = ta.scrollLeft; };
      const lines = () => {
        gutter.textContent = Array.from({ length: ta.value.split("\n").length }, (_, i) => i + 1).join("\n");
        hl.innerHTML = highlight(ta.value, lang);
        syncScroll();
      };
      lines();
      let busy = false;
      const compile = async () => {
        if (busy) return;
        busy = true;
        compileBtn.disabled = true;
        compileBtn.textContent = lang === "as" ? "Compiling…" : compileBtn.textContent;
        try {
          const src = ta.value;
          const wasm = lang === "as" ? await compileAs(src) : S.mod.roomEditorCompileWat(src);
          save(lump, nameIn.value, wasm, src);
          errBox.hidden = true;
        } catch (e) { showErr(e); } finally {
          busy = false;
          compileBtn.disabled = false;
          compileBtn.textContent = COMPILE_LABEL;
        }
      };
      ta.addEventListener("input", lines);
      ta.addEventListener("scroll", syncScroll);
      ta.addEventListener("keydown", (e) => {
        if (e.key === "Tab") { e.preventDefault(); ta.setRangeText("  ", ta.selectionStart, ta.selectionEnd, "end"); lines(); }
        if ((e.ctrlKey || e.metaKey) && e.key === "s") { e.preventDefault(); compile(); }
      });
      actions.append(compileBtn);
      wrap.append(el("div", { class: "code-wrap" }, gutter, el("div", { class: "code-area" }, hl, ta)));
    } else {
      actions.append(el("button", { class: "btn primary", onclick: async () => {
        const b = await ctx.pickFile(".wasm");
        if (!b) return;
        try { save(lump, nameIn.value, b, null); errBox.hidden = true; } catch (e) { showErr(e); }
      } }, "Replace .wasm…"));
      wrap.append(el("p", { class: "muted" }, `Binary module, ${mod.wasm.length} bytes. Build guest modules in Rust with room-script-sdk (see mods/doom1-survival), or write AssemblyScript / WAT here.`));
    }
    actions.append(
      el("button", { class: "btn", onclick: () => ctx.download(mod.wasm, `${lump.toLowerCase()}.wasm`) }, "Download .wasm"),
      el("button", { class: "btn", onclick: () => {
        if (!confirm(`Remove script ${lump}?`)) return;
        try { S.mod.roomEditorRemoveScript(lump); S.lumpTabs.delete(`script:${lump}`); ctx.selectDocTab("map"); apply(`${lump} removed`); } catch (e) { ctx.toast(ctx.errText(e), true); }
      } }, "Remove"));
    wrap.append(errBox, lang === "as" ? asApiView() : apiView(), logView());
    return wrap;
  }

  function open(lump) {
    const id = `script:${lump}`;
    S.lumpTabs.set(id, { title: `⚙ ${lump}`, view: moduleView(lump) });
    ctx.selectDocTab(id);
  }

  function askLump(def) {
    const n = prompt("Module lump name (1–8 letters / digits):", def);
    return n ? n.trim().toUpperCase().slice(0, 8) : null;
  }

  function newWat() {
    const lump = askLump("HELLO");
    if (!lump) return;
    try {
      S.mod.roomEditorSetScript(lump, lump.toLowerCase(), S.mod.roomEditorCompileWat(TEMPLATE), TEMPLATE);
      apply(`${lump} created`);
      open(lump);
    } catch (e) { ctx.toast(ctx.errText(e), true); }
  }

  async function newAs() {
    const lump = askLump("MYMOD");
    if (!lump) return;
    ctx.toast("Loading the AssemblyScript compiler…");
    try {
      S.mod.roomEditorSetScript(lump, lump.toLowerCase(), await compileAs(AS_TEMPLATE), AS_TEMPLATE);
      apply(`${lump} created — press ▶ Play, then edit and Ctrl+S to reload it live`);
      open(lump);
    } catch (e) { ctx.toast(ctx.errText(e), true); }
  }

  async function importWasm() {
    const b = await ctx.pickFile(".wasm");
    if (!b) return;
    const lump = askLump("MYMOD");
    if (!lump) return;
    try { save(lump, lump.toLowerCase(), b, null); open(lump); } catch (e) { ctx.toast(ctx.errText(e), true); }
  }

  async function loadExample(key) {
    const ex = EXAMPLES[key];
    try {
      const url = `./mods/${ex.file}`;
      const r = await fetch(url, { cache: "no-cache" });
      if (!r.ok) throw new Error(`${url}: HTTP ${r.status} (run ./scripts/play-web.sh to publish it)`);
      ctx.addPwad(new Uint8Array(await r.arrayBuffer()), ex.file);
      apply(`${ex.label} loaded — press ▶ Play (${ex.note})`);
    } catch (e) { ctx.toast(ctx.errText(e), true); }
  }

  function renderBrowser(body) {
    let st;
    try { st = state(); } catch (e) { body.append(el("p", { class: "err-box" }, ctx.errText(e))); return; }
    body.append(el("p", { class: "muted", style: "padding:6px 10px;margin:0" },
      "WASM mods run every tic with full game control (spawn, damage, inventory, states, level exit…). Write them in AssemblyScript or WAT; Ctrl+S during a play test reloads them live. Saved in the exported PWAD as ROOMMOD + module lumps."));
    body.append(el("div", { class: "actions", style: "padding:0 10px 6px;margin:0;flex-wrap:wrap" },
      el("button", { class: "btn small", onclick: newAs, title: "TypeScript-like language compiled to WASM in the browser" }, "+ New AssemblyScript mod"),
      el("button", { class: "btn small", onclick: newWat }, "+ New WAT script"),
      el("button", { class: "btn small", onclick: importWasm }, "+ Import .wasm"),
      el("button", { class: "btn small", onclick: () => loadExample("survival"), title: "Rust example (mods/doom1-survival): skill-scaled monster waves" }, "Example: Survival (Rust)"),
      el("button", { class: "btn small", onclick: () => loadExample("bounty"), title: "AssemblyScript example (mods/doom1-bounty): bounties, rewards, combos, a final boss — editable source" }, "Example: Bounty Hunter (AS)")));

    const meta = el("fieldset", { style: "margin:4px 10px" }, el("legend", {}, "Mod (ROOMMOD)"));
    const idIn = el("input", { type: "text", value: st.id });
    const nmIn = el("input", { type: "text", value: st.name });
    const vrIn = el("input", { type: "text", value: st.version });
    const setInfo = () => { try { S.mod.roomEditorSetModInfo(idIn.value, nmIn.value, vrIn.value); } catch (e) { ctx.toast(ctx.errText(e), true); } };
    for (const i of [idIn, nmIn, vrIn]) i.addEventListener("change", setInfo);
    meta.append(ctx.rowEl("Id", idIn), ctx.rowEl("Name", nmIn), ctx.rowEl("Version", vrIn));
    body.append(meta);

    body.append(el("div", { class: "section-head" }, `Editor scripts (${st.modules.length})`));
    if (!st.modules.length) body.append(el("p", { class: "muted", style: "padding:4px 10px" }, "None yet."));
    for (const m of st.modules) {
      const live = st.loaded.includes(m.name);
      body.append(el("div", { class: `list-item${S.docTab === `script:${m.lump}` ? " active" : ""}`, onclick: () => open(m.lump) },
        el("span", { class: "kind text" }, LANG_LABEL[m.lang || "wasm"]), el("span", { class: "name" }, m.lump),
        live ? el("span", { class: "kind map" }, "running") : null,
        el("span", { class: "meta" }, `${m.name} · ${(m.size / 1024).toFixed(1)}K`)));
    }
    if (st.mounted.length) {
      body.append(el("div", { class: "section-head" }, "From loaded PWADs"));
      for (const m of st.mounted) {
        body.append(el("div", { class: "list-item" }, el("span", { class: "kind data" }, "mod"), el("span", { class: "name" }, m.name),
          st.loaded.includes(m.name) ? el("span", { class: "kind map" }, "running") : null,
          el("span", { class: "meta" }, m.mod)));
      }
      body.append(el("div", { class: "actions", style: "padding:4px 10px" }, el("button", { class: "btn small", onclick: () => {
        try { S.mod.roomEditorImportScripts(); apply("PWAD mod copied into the editor — it now overrides the PWAD copy"); } catch (e) { ctx.toast(ctx.errText(e), true); }
      } }, "Copy into editor for editing")));
    }
    for (const e of st.errors) body.append(el("p", { class: "err-box", style: "margin:4px 10px" }, e));
    const logBox = el("div", { style: "padding:0 10px" }, logView());
    body.append(logBox);
  }

  return { renderBrowser, open, newWat, newAs };
}
