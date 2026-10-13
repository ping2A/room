# 05 — Text definition lumps (`room-sc`, `room-defs`)

All ZDoom text lumps share one tokenizer (GZDoom `FScanner`, `sc_man.cpp`).
`room-defs` contains one parser module per lump type and a pipeline that
compiles everything into `Definitions` (see `01-architecture.md §3`).

## 1. The scanner (`room-sc`)

### 1.1 Tokens

```rust
pub enum Tok<'a> {
    Ident(&'a str),          // [A-Za-z_][A-Za-z0-9_]* ; in non-C mode, also allows '.', '-', etc. per FScanner rules
    String(Cow<'a, str>),    // "..." with C escapes (\n \t \" \\ \x hex), and line continuations
    Int(i64),                // parsed like C strtol(base 0) as FScanner::GetNumber does: 0x = hex, leading 0 = octal.
                             // Verify against sc_man.cpp and add a regression test; DECORATE colour/flag values depend on it.
    Float(f64),              // 1.0, .5, 1e3, 1.0f suffix tolerated
    Punct(Punct),            // { } ( ) [ ] ; , = : + - * / % < > ! ~ & | ^ ? . and multi-char ops (==, !=, <=, >=, &&, ||, <<, >>, >>>, ++, --, +=, …, ::, ..)
    Eof,
}
```

### 1.2 Modes

- **C mode** (`set_c_mode(true)`): punctuation is always a separate token; used by DECORATE, MAPINFO (new), GLDEFS, MODELDEF, TEXTURES, LOCKDEFS, UDMF, CVARINFO, MENUDEF, SBARINFO.
- **Non-C mode** (default, Hexen-era lumps such as SNDINFO, old MAPINFO, ANIMDEFS, TERRAIN, SNDSEQ, KEYCONF): a token is a run of non-whitespace characters, except that `{ } ( ) = , ;` may still split depending on the parser. Replicate `FScanner::GetString` rules precisely: `sc_man_scanner.re` is the reference (re2c grammar) — port its rules table rather than guessing.
- Case-insensitive keyword comparison everywhere (`sc.Compare`).
- Comments: `//` and `/* */` always. Some Hexen-era lumps (SNDSEQ, old-format MAPINFO, SNDINFO) also accept `;` as a line comment. Expose this as `ScanOptions::semicolon_comments` and set it per parser to match GZDoom.

### 1.3 Includes

`#include "path"` is resolved through the VFS (`find_full`, then short name in Global). Includes are recursive with a depth limit (64) and cycle detection. GZDoom resolves include paths relative to the archive root, not the including file — do the same. Lumps that support `#include`: DECORATE, MAPINFO, GLDEFS, MENUDEF, SBARINFO, TEXTURES, MODELDEF, LANGUAGE (no), TERRAIN (no). The list is per parser (table in §3).

### 1.4 API

```rust
let mut sc = Scanner::new(&vfs, lump_id, ScanOptions { c_mode: true, ..Default::default() });
sc.must_get_ident()?; sc.check_punct(Punct::LBrace); sc.must_get_number()?;
sc.unget(); sc.span(); // SourceSpan for diagnostics
```

Errors: `ScanError { span, kind }`. Higher-level parsers attach context.

## 2. Compilation pipeline

Phases run in this order (dependencies flow downward). Within a phase,
lumps are processed in VFS load order (`find_all`) unless "last wins".

| Phase | Lumps | Output |
|---|---|---|
| 0 | `IWADINFO` (engine), `GAMEINFO` (PWAD) | `GameInfo`, extra load list, startup screen params |
| 1 | `LANGUAGE` (text or `.csv`), `X11R6RGB`, `TEXTCOLO` | string tables, colour names, text colours |
| 2 | `CVARINFO`, `DEFCVARS` (engine & mod defaults), `KEYCONF` (aliases, weapon sections, player classes) | CVar declarations, default overrides |
| 3 | `PLAYPAL`, `COLORMAP`, `TRNSLATE`, `PALVERS` | palette, colormaps, named translations |
| 4 | `PNAMES` + `TEXTURE1/2`, `TEXTURES`, flats/patches/sprites/hires namespaces, `ANIMATED`, `SWITCHES`, `ANIMDEFS` | `TextureRegistry`, `AnimDefs` |
| 5 | `FONTDEFS`, font directories | `FontDefs` |
| 6 | `MAPINFO`/`ZMAPINFO`/`UMAPINFO` (all formats, §4) including `DoomEdNums`, `SpawnNums`, `DamageType`, `GameInfo` section | `MapInfo` (class names unresolved) |
| 7 | **Actor classes**: built-in DECORATE (`room.pk3`), mod DECORATE, WASM module class registrations, then `DEHACKED` lumps / `-deh` files (applied last, in load order) | `ClassRegistry`, `StateTable`, `SpriteRegistry` |
| 8 | Resolve MAPINFO class references (DoomEdNums, `PlayerClasses`, intermission actors, `EventHandlers`) | |
| 9 | `SNDINFO`, `SNDSEQ`, `REVERBS`, `MUSINFO` | `SoundRegistry`, sequences, environments |
| 10 | `TERRAIN`, `DECALDEF`, `LOCKDEFS`, `SECRETS`, `ALTHUDCF`, `TEAMINFO`, `S_SKIN` | |
| 11 | `GLDEFS` (and `DOOMDEFS`/`HTICDEFS`/`HEXNDEFS`/`STRFDEFS` game-specific variants), `MODELDEF`, `VOXELDEF` | render defs |
| 12 | `MENUDEF`, `SBARINFO`, `LOADACS`, `XLAT` | UI defs, ACS library list, special translators |
| 13 | Validation pass (dangling state jumps, missing sprites → warnings) and freezing into `Arc<Definitions>` | |

Parsing of independent lumps inside a phase may be parallel (rayon) **but merging is sequential in load order** so IDs are deterministic.

## 3. Lump reference

For each lump: syntax reference (ZDoom wiki page), merge policy, error policy.
Implement every keyword listed on the wiki page; the tables below are the
minimum checklist and the non-obvious semantics.

| Lump | Wiki | Merge | Errors | `#include` |
|---|---|---|---|---|
| MAPINFO / ZMAPINFO | `MAPINFO` | additive; ZMAPINFO in a container hides MAPINFO of the same container | fatal on syntax | yes |
| UMAPINFO | `UMAPINFO` | additive, applied after (Z)MAPINFO of the same container | warn | no |
| DECORATE | `DECORATE` | additive | fatal (report all, then abort) | yes |
| DEHACKED | `DeHackEd` | sequential patches | warn | `include` (BEX) |
| SNDINFO | `SNDINFO` | additive, later overrides | warn | no |
| SNDSEQ | `SNDSEQ` | additive by name | warn | no |
| LANGUAGE | `LANGUAGE` | additive per locale, later wins | warn | no |
| TEXTURES | `TEXTURES` | additive, later overrides by name+usetype | fatal | yes |
| ANIMDEFS | `ANIMDEFS` | additive | warn | no |
| TERRAIN | `TERRAIN` | additive | warn | no |
| DECALDEF | `DECALDEF` | additive | warn | yes |
| GLDEFS | `GLDEFS` | additive | warn | yes |
| MODELDEF | `MODELDEF` | additive (later model for same class/frame wins) | warn | yes |
| VOXELDEF | `VOXELDEF` | additive | warn | no |
| KEYCONF | `KEYCONF` | additive, executed as console commands in a restricted context | warn | no |
| CVARINFO | `CVARINFO` | additive; redeclaring an engine CVar is an error | warn | no |
| DEFCVARS | `DEFCVARS` | additive (Room and GZDoom ≥4.11 semantics: `set`/`seta` defaults for existing CVars; mods may not change `noset` ones) | warn | no |
| MENUDEF | `MENUDEF` | additive; `DefaultListMenu`, `DefaultOptionMenu`, `OptionValue` etc. | fatal | yes |
| SBARINFO | `SBARINFO` | last wins (only one status bar) — `base` loads a built-in | fatal | yes |
| LOCKDEFS | `LOCKDEFS` | additive; `ClearLocks` resets | warn | no |
| FONTDEFS | `FONTDEFS` | additive | warn | no |
| TRNSLATE | `TRNSLATE` | additive | warn | no |
| REVERBS | `REVERBS` | additive | warn | no |
| SECRETS | `SECRETS` | additive | warn | no |
| ALTHUDCF | `ALTHUDCF` | additive | warn | no |
| X11R6RGB | `X11R6RGB` | additive | warn | no |
| TEXTCOLO | `TEXTCOLO` | additive | warn | no |
| LOADACS | `LOADACS` | all instances | warn | no |
| XLAT | (engine-internal, `xlat/*.txt`) | per game; MAPINFO `translator` selects | fatal | yes |
| GAMEINFO | `GAMEINFO` | first PWAD only | warn | no |
| MUSINFO | `MUSINFO` | additive | warn | no |
| TEAMINFO | `TEAMINFO` | additive | warn | no |
| S_SKIN | Legacy skins | each instance defines a skin | warn | no |
| PALVERS | `PALVERS` | additive | warn | no |
| ZSCRIPT | — | **not supported**: emit one clear error per container ("this mod requires ZScript; see migration guide") and continue loading unless `strict_zscript_error` | warn/fatal | — |

### 3.1 MAPINFO

Support all four syntaxes:

1. **Old Hexen MAPINFO** (non-C mode, `map 1 "Winnowing Hall"` followed by lines).
2. **New ZDoom MAPINFO** (braces, `key = value`). Detected by `{` after the map header (GZDoom's `ParseMapDefinition` "format detection").
3. **ZMAPINFO** (same as new, explicit).
4. **UMAPINFO** (separate parser; translated into the same IR, with its specific semantics for `bossaction`, `endpic`, `endbunny`, `endgame`, `intertext`, `episode = clear`).

Blocks (new format):

| Block | Notes |
|---|---|
| `map <lump> [lookup] "<title>"` | 100+ properties — see list below |
| `defaultmap { }`, `adddefaultmap { }` | defaults applied to subsequent maps |
| `gamedefaults { }` | global defaults |
| `episode <map> [teaser <map>] { name, lookup, picname, key, remove, noskillmenu, optional, extended }` | |
| `clearepisodes` | |
| `cluster <n> { entertext [lookup], exittext, music, flat|pic, hub, allowintermission, name }` | |
| `skill <name> { AmmoFactor, DropAmmoFactor, DoubleAmmoFactor, DamageFactor, HealthFactor, FastMonsters, SlowMonsters, RespawnTime, RespawnLimit, Aggressiveness, SpawnFilter, SpawnMulti, ACSReturn, Name, PicName, PlayerClassName, MustConfirm, Key, TextColor, EasyBossBrain, EasyKey, NoMenu, NoPain, ArmorFactor, MonsterHealth, FriendlyHealth, NoInfighting, TotalInfighting, DefaultSkill, Instantreaction, ReplaceActor, SpawnMultiCoopOnly, ... }` | |
| `clearskills` | |
| `gameinfo { ... }` | ~100 keys (titlepage, titlemusic, titletime, advisorytime, pagetime, chatsound, finalemusic, finaleflat, finalepage, quitsound, borderflat, border, telefogheight, defkickback, skyflatname, translator, weaponslot?, playerclasses, pausesign, gibfactor, cursorpic, statusbar, statusbarclass, messageboxclass, menufontcolor_*, dimcolor, dimamount, definventorymaxamount, defaultrespawntime, defaultdropstyle, endoom, player5start, drawreadthis, pickupcolor, quitmessages, mapinfo/textscreenx/y, intermissioncounter, menubackbutton, nightmarefast, forcetextinmenus, precachetextures, EventHandlers, AddEventHandlers, normforwardmove, normsidemove, …) | |
| `intermission <name> { Image, Scroller, Fader, Wiper, TextScreen, GotoTitle, Cast, Link, ... }` | finale/intermission sequences |
| `automap { base, showlocks, background, ... colors }` / `automap_overlay` | |
| `doomednums { 3004 = ZombieMan [, special args]* }` | class name strings resolved in phase 8 |
| `spawnnums { 1 = ShotgunGuy }` | for Thing_Spawn |
| `conversationids { 1 = ... }` | Strife |
| `damagetype <name> { factor, replacefactor, noarmor, obituary }` | |
| `include "<lump>"` | |

Map properties checklist (implement all; group them in `MapInfoEntry` sub-structs):
`levelnum, next, secretnext, next = endgame/EndTitle/EndSequence…, sky1 [speed], sky2, skybox, doublesky, forcenoskystretch, skystretch, music [order], intermusic, exitpic, enterpic, interpic, titlepatch, cluster, par, sucktime, author, label, fade, outsidefog, fogdensity, outsidefogdensity, skyfog, gravity, aircontrol, airsupply, lightning, evenlighting, smoothlighting, lightmode, nointermission, intermission, nojump/allowjump, nocrouch/allowcrouch, nofreelook/allowfreelook, infiniteflightpowerup, noinfiniteflightpowerup, allowrespawn, teamdamage, teamplayon/off, checkswitchrange, fallingdamage, oldfallingdamage, strifefallingdamage, forcefallingdamage, monsterfallingdamage, nofallingdamage, noautosequences, autosequences, specialaction "Class", "special", args, map07special, baronspecial, cyberdemonspecial, spidermastermindspecial, ironlichspecial, minotaurspecial, dsparilspecial, specialaction_exitlevel, specialaction_opendoor, specialaction_lowerfloor, specialaction_lowerfloortohighest, specialaction_killmonsters, resethealth, resetinventory, resetitems, keepfullinventory, unfreezesingleplayerconversations, filterstarts, activateowndeathspecials, killeractivatesdeathspecials, missilesactivateimpactlines, missileshootersactivetimpactlines, spawnwithweaponraised, laxmonsteractivation, staticmonsteractivation, translator, compat_* (every compat flag), vertwallshade, horizwallshade, pixelratio, precachetextures, precachesounds, precacheclasses, redirect, slideshow, deathsequence, endofgame, nosoundclipping, clipmidtextures, wrapmidtextures, totalinfighting, normalinfighting, noinfighting, grinding_polyobj, disableskyboxao, attenuatelights, lightblendmode, tonemap, avoidmelee, EventHandlers, ...`.
Unknown keys: warning and skip the value(s) (GZDoom's `ParseMapDefinition` default case — skips tokens until next line/key).

### 3.2 SNDINFO

Commands (non-C mode): `logicalname lumpname`, `$alias`, `$ambient index logical [type] [mode] volume` (types: point [attenuation], surround, world; modes: continuous, random min max, periodic secs), `$archivepath`, `$attenuation`, `$edfoverride`, `$ifdoom/$ifheretic/$ifhexen/$ifstrife/$endif`, `$limit name n [distance]`, `$map mapnum music`, `$mididevice music device [arg]`, `$musicalias`, `$musicvolume`, `$pitchshift`, `$pitchshiftrange`, `$playeralias`, `$playercompat`, `$playersound class gender name lump`, `$playersounddup`, `$random name { a b c }`, `$rawsound`, `$registered`, `$rolloff name|* type min max` (types: custom, linear, log), `$singular`, `$volume`.
Player sounds (`*pain100`, `*death`) resolve through player class + gender + skin at runtime.

### 3.3 TEXTURES

```
Texture|Sprite|Graphic|WallTexture|Flat [optional] "Name", width, height
{
  XScale f; YScale f; Offset x, y; WorldPanning; NoDecals; NullTexture; NoTrim;
  Patch|Graphic|Sprite "lump", x, y
  {
    FlipX; FlipY; UseOffsets; Rotate 0|90|180|270;
    Translation "a:b=c:d", ... | Translation Ice|Inverse|Gold|Red|Green|Blue|Desaturate n;
    Blend "color"[, alpha] | Blend r,g,b[,a];  Alpha f;
    Style Copy|Translucent|Add|Subtract|ReverseSubtract|Modulate|CopyAlpha|CopyNewAlpha|Overlay;
  }
}
Define "Name" width height   // treat lump as given size (legacy)
```

Usetype matters: textures, flats, sprites, graphics, wall textures and overrides
form separate lookup spaces with fallback rules (`FTextureManager::CheckForTexture` with `TEXMAN_Overridable`, `TEXMAN_TryAny` flags). Port those flags 1:1 as `TexLookupFlags` in `TextureRegistry::find`.

### 3.4 ANIMDEFS

`flat|texture name [optional] [allowdecals] [oscillate]` followed by `pic n|name tics t|rand a b` entries or `range last tics t|rand`, `warp|warp2 name [speed f] [allowdecals]`, `cameratexture name w h [fit fw fh] [worldpanning]`, `switch [doom|heretic|hexen|strife] name on [sound s] pic n tics t ... off ...`, `animateddoor name [opensound s] [closesound s] pic ...`, `skyoffset name n`, `canvastexture` (S).

### 3.5 GLDEFS

Lights: `pointlight|pulselight|flickerlight|flickerlight2|sectorlight name { color r g b; size n; secondarysize n; interval f; chance f; scale f; offset x y z; subtractive 0|1; additive 0|1; dontlightself; dontlightactors; dontlightmap; attenuate 0|1; spot innerangle outerangle; noshadowmap; lightsizefactor; }`.
Bindings: `object Class { frame XXXXa { light name } }` (frame prefix matching, as in GZDoom).
Others: `skybox name [fliptop] { up, north… }` (6 or 3 textures), `brightmap texture|sprite|flat name { map lump; iwad; thiswad; disablefullbright }`, `glow { flats {…} walls {…} texture name, color, height, fullbright }`, `hardwareshader|material texture name { shader "file" speed f; texture uniformname "file"; define name = value; normal/specular/metallic/roughness/ao/brightmap maps; specularlevel/glossiness }`, `lightsizefactor`, `disable_fullbright`, `#include`.

Custom shaders are **GLSL** in GZDoom; see `13-renderer.md §Custom shaders` for the translation plan.

### 3.6 Other lumps — key semantics

- **LANGUAGE**: `[enu default]` headers (multiple locales per header; `~` excludes), `KEY = "string" "concatenated";`, `$ifgame(doom)` prefixes. CSV variant: first row = locale codes; GZDoom's `language.csv` layout. Lookup with `$KEY` from other lumps (`ParseStringOrLookup`). Locale fallback: requested → `enu` → `default`.
- **KEYCONF**: runs as console commands: `weaponsection`, `setslot`, `addslotdefault`, `addplayerclass Class [nomenu]`, `clearplayerclasses`, `alias`, `defaultbind`, `addkeysection`, `addmenukey`. Only this whitelist is allowed.
- **CVARINFO**: `[server|user|nosave] [noarchive] [cheat] [latch] [handlerclass(...)?] int|float|bool|color|string name [= default];`.
- **LOCKDEFS**: `Lock n [Doom|Heretic|Hexen|Strife|Chex] { KeyClass; Any { KeyA KeyB }; Message "s"; RemoteMessage "s"; Mapcolor r g b; LockedSound "s" }`, `ClearLocks`.
- **TERRAIN**: `splash name { smallclass, smallclip, smallsound, baseclass, chunkclass, chunkxvelshift, chunkyvelshift, chunkzvelshift, chunkbasezvel, sound, noalert }`, `terrain name { splash, footclip, liquid, friction, damageamount, damagetype, damagetimemask, allowprotection, leftstepsounds, rightstepsounds, stepvolume, walkingsteptime, runningsteptime, stepdistance, stepdistanceminvel, damageonland }`, `floor flatname terrainname`, `ifdoom/ifheretic/ifhexen/ifstrife/endif`, `defaultterrain`.
- **DECALDEF**: `decal name [id] { pic, add, shade, translucent, solid, x-scale, y-scale, randomflipx, randomflipy, fullbright, fuzzy, lowerdecal, animator, opaqueblood }`, `decalgroup name { decal weight }`, `fader`, `stretcher`, `slider`, `colorchanger`, `combiner`, `generator class decal`.
- **SNDSEQ**: `:Name` … `end`, `[Name ... ]` slot syntax, commands `play, playuntildone, playtime, playrepeat, playloop, delay, delayrand, volume, volumerel, volumerand, attenuation, stopsound, nostopcutoff, door n, platform n, environment n, slot`.
- **REVERBS**: `Name id1 id2 { property value ... }` (EAX 2.0-style fields: `Environment, EnvironmentSize, EnvironmentDiffusion, Room, RoomHF, RoomLF, DecayTime, DecayHFRatio, DecayLFRatio, Reflections, ReflectionsDelay, Reverb, ReverbDelay, EchoTime, EchoDepth, ModulationTime, ModulationDepth, AirAbsorptionHF, HFReference, LFReference, RoomRolloffFactor, …, bDecayTimeScale…`). Mapped to Room's reverb model (`14-audio.md`).
- **TRNSLATE**: `Name = "range", "range", …` with GZDoom translation-string syntax (`a:b=c:d`, `a:b=[r,g,b]:[r,g,b]`, `%[…]` desaturated, `#[r,g,b]` colourise, `@amount[r,g,b]` tint).
- **XLAT**: line-special translation for Doom/Heretic/Strife-format maps (see `09-specials.md §Translation`). Syntax: `define`, `enum`, `<num> = <flags>, <special> (<args>)`, `[num1, num2] = …` for generalized ranges (`lineid`/`tag` placeholders), `sector bitmask`, `lineflag`, `maxlinespecial`.

## 4. Diagnostics and dumping

Every compiled registry implements `Dump` (RON output) used by golden tests and by `room-cli defs dump <wads…> --registry mapinfo`. This is the main tool for comparing against GZDoom behaviour (GZDoom has `dumpmapinfo`, `dumpclasses`, `dumpactions` console commands — implement the same names).
