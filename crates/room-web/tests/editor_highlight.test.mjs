/**
 * Editor text-lump highlighting (ZScript / DECORATE / ACS / MAPINFO) and
 * language detection. Run: node --test crates/room-web/tests/editor_highlight.test.mjs
 */
import assert from "node:assert/strict";
import test from "node:test";
import { highlight, includesOf, looksZscript, textLang, zscriptClosure } from "../editor-scripts.js";

const spans = (html, cls) => [...html.matchAll(new RegExp(`<span class="hl-${cls}">([^<]*)</span>`, "g"))].map((m) => m[1]);
const strip = (html) => html.replace(/<[^>]+>/g, "").replace(/&lt;/g, "<").replace(/&gt;/g, ">").replace(/&quot;/g, '"').replace(/&amp;/g, "&");

const ZS = `version "4.10"
#include "zscript/weapons.zs"
// a comment
class FastImp : DoomImp replaces DoomImp
{
  Default
  {
    Health 90;
    Speed 16;
    +FLOAT -SOLID
    Obituary "%o was burned";
  }
  override void Tick() { super.Tick(); double x = 0x1F + 2.5; /* c */ }
  States
  {
  Spawn:
    TROO AB 10 A_Look;
    Loop;
  See:
    "####" "#" 0 Bright A_Chase("Melee", null);
    goto Super::See+1;
  Death: TROO I 8 A_Scream; Stop;
  }
}`;

test("ZScript: keywords, types, strings, numbers, comments, directives", () => {
  const h = highlight(ZS, "zs");
  assert.equal(strip(h), `${ZS}\n `, "highlighting must not change the text");
  assert.deepEqual(spans(h, "d"), ["version", "#include"]);
  assert.ok(spans(h, "k").includes("class") && spans(h, "k").includes("override") && spans(h, "k").includes("States"));
  assert.ok(spans(h, "t").includes("double") && spans(h, "t").includes("DoomImp"));
  assert.ok(spans(h, "s").includes('"zscript/weapons.zs"'));
  assert.deepEqual(spans(h, "n").slice(0, 4), ["90", "16", "0x1F", "2.5"]);
  assert.deepEqual(spans(h, "c"), ["// a comment", "/* c */"]);
});

test("ZScript: Default properties and flags, States labels and frames", () => {
  const h = highlight(ZS, "zs");
  assert.deepEqual(spans(h, "p"), ["Health", "Speed", "Obituary"]);
  assert.deepEqual(spans(h, "fl"), ["+FLOAT", "-SOLID"]);
  assert.deepEqual(spans(h, "l"), ["Spawn", "See", "Super::See", "Death"]);
  assert.deepEqual(spans(h, "sp"), ["TROO", '"####"', "TROO"]);
  assert.deepEqual(spans(h, "fr"), ["AB", '"#"', "I"]);
  assert.ok(["A_Look", "A_Chase", "A_Scream", "Tick"].every((f) => spans(h, "f").includes(f)));
  assert.ok(spans(h, "k").includes("Bright") && spans(h, "k").includes("Loop") && spans(h, "k").includes("goto"));
});

test("ZScript: - outside Default is an operator, not a flag", () => {
  const h = highlight("class A : Actor { void F() { int x = a -b; } }", "zs");
  assert.deepEqual(spans(h, "fl"), []);
});

test("DECORATE actor body: properties, flags, states", () => {
  const h = highlight("actor Foo : Bar 1234\n{\n  Health 100\n  +SHOOTABLE\n  states\n  {\n  Spawn:\n    POSS A 1 A_Look\n    loop\n  }\n}", "decorate");
  assert.deepEqual(spans(h, "p"), ["Health"]);
  assert.deepEqual(spans(h, "fl"), ["+SHOOTABLE"]);
  assert.deepEqual(spans(h, "l"), ["Spawn"]);
  assert.deepEqual(spans(h, "sp"), ["POSS"]);
  assert.deepEqual(spans(h, "t"), ["Foo", "Bar"]);
});

test("ACS and MAPINFO", () => {
  const acs = highlight('#include "zcommon.acs"\nscript 1 OPEN { Print(s:"hi"); int x = 2; }', "acs");
  assert.deepEqual(spans(acs, "d"), ["#include"]);
  assert.ok(spans(acs, "k").includes("script") && spans(acs, "k").includes("OPEN"));
  assert.deepEqual(spans(acs, "f"), ["Print"]);
  const mi = highlight('map MAP01 "Hangar" // c\n{\n  next = "MAP02"\n  sky1 = "SKY1", 0\n}', "mapinfo");
  assert.deepEqual(spans(mi, "k"), ["map"]);
  assert.deepEqual(spans(mi, "p"), ["next", "sky1"]);
  assert.deepEqual(spans(mi, "c"), ["// c"]);
});

test("HTML is escaped; unterminated strings and comments do not hang", () => {
  const h = highlight('a < b && "x<y" /* open', "zs");
  assert.ok(!h.includes("<y"));
  assert.equal(strip(h), 'a < b && "x<y" /* open\n ');
  assert.equal(strip(highlight('"never closed\nnext', "decorate")), '"never closed\nnext\n ');
});

test("language detection and #include lines", () => {
  assert.equal(textLang("ZSCRIPT", "zscript.txt"), "zs");
  assert.equal(textLang("WEAPONS", "zscript/weapons.zs"), "zs");
  assert.equal(textLang("DECORATE", "decorate.txt"), "decorate");
  assert.equal(textLang("IMP", "actors/imp.dec"), "decorate");
  assert.equal(textLang("FOO", "foo.txt", "// x\nactor Foo : Bar {}"), "decorate");
  assert.equal(textLang("ZMAPINFO", "zmapinfo.txt"), "mapinfo");
  assert.equal(textLang("SCRIPTS", "acs/scripts.acs"), "acs");
  assert.equal(textLang("SNDINFO", "sndinfo.txt"), "text");
  assert.deepEqual(includesOf(ZS), [{ line: 2, path: "zscript/weapons.zs" }]);
});

test("ZScript is recognised by content, not only by name", () => {
  assert.equal(textLang("PISTOL", "actors/pistol.txt", "// gun\nclass BDPistol : Weapon\n{\n  Default { Weapon.SlotNumber 2; }\n}"), "zs");
  assert.equal(textLang("PISTOL", "actors/pistol.txt", "actor BDPistol : Pistol {}"), "decorate");
  assert.equal(textLang("ROOT", "root.txt", 'version "4.10"\n'), "zs");
  assert.equal(textLang("SNDINFO", "sndinfo.txt", "misc/x dsx\n"), "text");
  assert.ok(looksZscript("extend class DoomImp\n{\n  void Hi() {}\n}"));
  assert.ok(!looksZscript("/* class X { */\nactor X {}"));
});

test("zscriptClosure follows #include from the ZSCRIPT root, whatever the file names", () => {
  const files = {
    "zscript.zc": 'version "3.2"\n#include "src/handlers.txt"\n',
    "src/handlers.txt": '#include "src/more.txt"\nclass H : EventHandler {}\n',
    "src/more.txt": "class M : Actor {}\n",
    "src/unrelated.txt": "not included\n",
    "decorate.txt": "actor A {}\n",
  };
  const lumps = Object.keys(files).map((path) => ({ path, src: "mod.pk3", kind: "text",
    name: path.split("/").pop().replace(/\.[^.]*$/, "").toUpperCase().slice(0, 8) }));
  const resolve = (p) => lumps.find((l) => l.path === p.toLowerCase());
  const got = [...zscriptClosure(lumps, (l) => files[l.path], resolve)].map((l) => l.path).sort();
  assert.deepEqual(got, ["src/handlers.txt", "src/more.txt", "zscript.zc"]);
});
