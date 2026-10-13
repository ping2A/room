# 15 — Input, console, commands, CVars, configuration

Crates: `room-input`, `room-console`. GZDoom references: `src/common/console/`
(`c_cvars.cpp`, `c_dispatch.cpp`, `c_bind.cpp`, `c_console.cpp`), `src/g_game.cpp` (`G_BuildTiccmd`),
`src/d_main.cpp` (`D_ProcessEvents`, responder chain), `src/m_misc.cpp` (config), ZDoom wiki
*Console*, *CVARs*, *Console commands*, *KEYCONF*.

## 1. Input events and responder chain

Platform shells translate native events into:

```rust
pub enum InputEvent {
    KeyDown(Key, Modifiers), KeyUp(Key), Char(char),
    MouseMove { dx: f32, dy: f32 }, MouseButton(MouseButton, bool), MouseWheel(f32, f32), MousePos(f32, f32),
    GamepadButton(u32, GamepadButton, bool), GamepadAxis(u32, Axis, f32), GamepadConnected(u32, bool),
    Touch(..) /* web/mobile, S */, FocusLost, FocusGained,
}
```

Key codes use GZDoom's `KEY_*` scheme for compatibility with config files (`KEY_MOUSE1`, `KEY_PAD_A`, `KEY_MWHEELUP`, …) and keep names for binds (`"mouse1"`, `"pad_a"`, `"kp5"`, `"lshift"`…).

Responder order (port `D_ProcessEvents`): console (when open / toggle key) → menu → ui-scope module `InputProcess` handlers (if any claims) → chat → automap → game bindings (`G_Responder`). Each may consume.

## 2. Bindings

- Three bind tables like GZDoom: `bind` (normal), `doublebind`, `mapbind` (automap); plus per-gamepad axis config (`joy_*` CVars).
- Bound strings are console command lines, e.g. `+forward`, `+attack`, `weapnext`, `use ArtiTeleport`, `+user1`.
- `+cmd`/`-cmd` button commands set button state; `room-input` keeps `ButtonState` (down, went-down-this-tic, went-up) — port `FButtonStatus`.
- Defaults: `DEFBINDS` lump in `room.pk3` (per game sections) + `KEYCONF` `defaultbind`, `addkeysection`, `addmenukey`.

## 3. TicCmd building (`G_BuildTiccmd`)

Per tic: combine button states, `cl_run`/`+speed`, `freelook`, `lookstrafe`, `m_forward`, `m_side`, `m_yaw`, `m_pitch`, `m_sensitivity`, `invertmouse`, gamepad axes with dead zones and response curves, turn speeds (`turnspeeds` slow-turn acceleration for keyboard), `forwardmove`/`sidemove` scaling by `ForwardMove1/2` (player class) and `cl_` options, `+jump`, `+crouch`, `crouch` toggle, `+mlook`, `centerview`, `turn180`, weapon selection (`slot n`, `weapnext`, `weapprev`, `use <weapon>`) and inventory (`invnext`, `invprev`, `invuse`, `invdrop`) produce `NetCommand`s. Mouse look accumulates fractional input in high precision and quantizes to the `i16` deltas.

The renderer may apply not-yet-sent mouse deltas for zero-latency view (see `01 §6`).

## 4. Console

- Text console (toggle with `~`), scrollback (`con_buffersize`), command history (persisted), tab completion (commands, CVars, aliases, map names after `map`), colour codes (`\c` + letter/`[name]`), notify lines (`con_notifylines`, `con_notifytime`), centered messages (`C_MidPrint` used by ACS `Print`), pickup messages, obituaries, chat.
- Console font: GZDoom-style console font (bundled), scalable (`con_scale`).
- Output API: `room_console::printf(level, args)` where level is `PRINT_LOW, MEDIUM, HIGH, CHAT, TEAMCHAT, LOG, BOLD, NONOTIFY, NOLOG` (`msglevel` filter).
- Log file (`logfile` command).

## 5. Command system

```rust
pub struct CommandDef { pub name: Name, pub flags: CmdFlags /* CHEAT, SERVER, UNSAFE, NOSET… */, pub handler: CmdHandler, pub help: &'static str }
pub enum CmdHandler { Native(fn(&mut CmdCtx, &[&str])), Module(ModuleId, CallbackId) }
```

- Command lines: `;`-separated, with quoting. Aliases take positional arguments (`%1`, `%2`, …). Port `FConsoleAlias` and `C_DoCommand` parsing exactly rather than inventing syntax.
- Built-ins to implement (≥ 250 in GZDoom; the essential set): `map`, `changemap`, `restart`, `quit`, `exit`, `echo`, `alias`, `unalias`, `bind`, `unbind`, `unbindall`, `binddefaults`, `doublebind`, `mapbind`, `exec`, `writeini`, `set`, `seta`, `get`, `toggle`, `reset`, `resetcvar`, `cvarlist`, `cmdlist`, `wait`, `god`, `buddha`, `noclip`, `noclip2`, `notarget`, `fly`, `give`, `take`, `summon`, `summonfriend`, `summonfoe`, `kill`, `resurrect`, `morphme`, `unmorph`, `iddqd`…(cheat strings handled by cheat parser), `freeze`, `puke`, `pukename`, `netevent`, `screenshot`, `vid_setmode`, `menu_*`, `openmenu`, `closemenu`, `save`, `load`, `quicksave`, `quickload`, `playdemo`, `record`, `stop`, `timedemo`, `stat`, `idmus`/`changemus`, `playsound`, `stopsound`, `snd_reset`, `snd_status`, `dumpmapinfo`, `dumpclasses`, `dumpactors`, `dumpmapthings`, `linetarget`, `info`, `myinfo`, `whereami`, `warp`, `idclev`, `skill`, `gameversion`, `version`, `dir`, `logfile`, `pings`, `kick`, `say`, `say_team`, `weapnext`, `weapprev`, `slot`, `use`, `drop`, `invnext`, `invprev`, `invuse`, `invuseall`, `invdrop`, `centerview`, `crouch`, `spynext`, `spyprev`, `chase`, `togglemap`, `am_*`, `sizeup`, `sizedown`, `togglemessages`, `togglehud`, `bumpgamma`, `soundlist`, `soundlinks`, `stopmus`, `exec`, `printinv`, `scriptstat`, `acsprofile`, `fov`, `r_*` debug commands.

## 6. CVars

### 6.1 Model

```rust
pub struct CvarDef {
    pub name: Name, pub ty: CvarType /* Bool, Int, Float, String, Color, Flag(of base cvar, bit), Mask */,
    pub default: CvarValue, pub flags: CvarFlags, pub callback: Option<CvarCallback>, pub origin: CvarOrigin, pub range: Option<(f64, f64)>,
}
bitflags! { CvarFlags: ARCHIVE, USERINFO, SERVERINFO, NOSET, LATCH, UNSETTABLE, DEMOSAVE, NOINITCALL, GLOBALCONFIG,
            VIDEOCONFIG, AUDIOCONFIG, CHEAT, UNSAFECONTEXT, VIRTUAL, CONFIG_ONLY, MOD /* declared by a mod */, NOSAVE, USER, SERVER, SYSTEM_ONLY }
```

- Scopes: **user** (per-player, synced in netplay as userinfo: `name`, `team`, `color`, `colorset`, `skin`, `gender`, `neverswitchonpickup`, `movebob`, `stillbob`, `fviewbob`, `playerclass`, `autoaim`, `wi_noautostartmap`, `classicflight`, `fov`…), **server** (`sv_*`, `dmflags`, `dmflags2`, `compatflags`, `compatflags2`, `skill`, `timelimit`, `fraglimit`, `teamplay`, `deathmatch`… — synced to all peers, changeable only by the arbitrator), **client/nosave** (local).
- Sim code reads server CVars and *synced* user CVars only. Each read goes through `World::cvars` (a snapshot synced via net commands), never the live console table (to guarantee determinism).
- `dmflags`/`dmflags2`/`compatflags`/`compatflags2` are bitfields with derived flag CVars (`sv_nomonsters`, `compat_shorttex`, …) — port `FFlagCVar` and `FMaskCVar`.
- Latched CVars apply on next map/game.
- CVar callbacks (e.g. `vid_*` → renderer reconfigure, `snd_*` → audio reset) run on the main thread after the change.

### 6.2 Declared by content

- `CVARINFO` (see `05`) and module manifests/registration produce `MOD`-flagged CVars; archived ones persist in `[<game>.ModCVars]`-equivalent config section (`room.toml` `[mod_cvars."<mod id>"]`).
- `DEFCVARS` lump can change defaults of engine CVars that aren't `NOSET`/`CHEAT`.

## 7. Configuration file

`room.toml` (platform config dir; `-config <path>` override):

```toml
[paths]
iwad = ["~/games/doom"]
file = []
soundfonts = ["~/sf2"]

[autoload]
global = []
"doom" = []
"doom.id.doom2" = ["~/mods/music.pk3"]

[cvars]               # global archived CVars
vid_fullscreen = true
snd_musicvolume = 0.5

[cvars.game.doom]     # per-game archived CVars (GZDoom has per-game sections)
"crosshair" = 2

[bindings.doom]
"w" = "+forward"
"mouse1" = "+attack"
[doublebindings.doom]
[automapbindings.doom]

[mod_cvars."org.example.betterimps"]
betterimps_fireball_speed = 14.0

[last]
iwad = "doom2.wad"
```

- **Import from GZDoom**: on first run, if no `room.toml` exists and a `gzdoom.ini` is found (platform-specific locations), offer to import: paths, autoload sections, bindings, archived CVars (mapping table for renamed CVars; unknown ones preserved under `[cvars.unmapped]`).
- Writes are atomic (write temp + rename). Web: stored in OPFS / IndexedDB (`18`).
- `autoexec.cfg` (per game) executed after config load; `-exec file` and `+cmd args` on the command line.

## 8. Command line

Support GZDoom's common parameters: `-iwad`, `-file`, `-deh`, `-bex`, `-warp`, `+map`, `-skill`, `-config`, `-nomonsters`, `-fast`, `-respawn`, `-record`, `-playdemo`, `-timedemo`, `-loadgame`, `-savedir`, `-width`, `-height`, `-nosound`, `-nomusic`, `-nosfx`, `-host n`, `-join addr`, `-netmode`, `-dup`, `-extratic`, `-port`, `-norun`, `-stdout`, `-noautoload`, `+<cvar> value`, `+<command>`. Parse with a hand-rolled parser matching GZDoom's `FArgs` semantics (response files `@file`, `-file` accepting multiple values until next `-`/`+`).

## 9. Cheats

Classic cheat strings typed in game (`iddqd`, `idkfa`, `idclev`, `idbehold`, Heretic `quicken`, `ponce`, `kitty`, Hexen `satan`, `casper`, `indiana`, Strife `omnipotent`, …) via a per-game cheat sequence table (port `st_stuff.cpp` / `ct_chat`), producing `NetCommand::Cheat`. Disabled in netplay unless `sv_cheats`.

## 10. Tests

- Binding parse/serialize round trip, gzdoom.ini import fixture.
- Alias expansion & command splitting tests (quotes, `;` inside quotes).
- CVar flag semantics (latch, noset, cheat) and determinism: changing a client CVar mid-game must not alter world checksum.
