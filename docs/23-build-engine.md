# 23 — Build engine games (Duke Nukem 3D)

> For what "full" support requires and the plan to get there, see
> [`25-duke3d-full-support.md`](25-duke3d-full-support.md). Architecture ADR:
> [`decisions/0004-native-build-runtime.md`](decisions/0004-native-build-runtime.md)
> (native `room-build-engine` / `room-duke` track; this converter path stays
> for play until D11, then remains as an export tool).

Room plays Duke Nukem 3D content in the same way [Raze](https://github.com/ZDoom/Raze)
brings Build games to the GZDoom backend. The crate `room-build` reads the
Build formats and converts a `DUKE3D.GRP` into an in-memory **UDMF PWAD**, so
every shell (desktop `room-app`, browser `room-web`, `room-server` rooms and
the verifier) loads Duke maps through the normal VFS → map → playsim → renderer
path. No commercial content is committed: `testdata/duke/duke3d.grp` is
git-ignored and the tests that use it pass trivially when it is absent.

## 1. Stage 1 (landed): walk the maps

| Area | Status |
|------|--------|
| GRP container (`KenSilverman`) | yes — `room_build::Grp` |
| ART tiles (`TILES*.ART`) | yes — every tile used by a map becomes a Doom patch in `TX_START`…`TX_END` (`BTnnnn`) |
| `PALETTE.DAT` | yes — `PLAYPAL` (14 palettes with Doom pain/bonus tints) + 34-map `COLORMAP` |
| `.MAP` v7/v8 | yes — sectors, walls, sprites → `TEXTMAP` |
| `USER.CON` level table | names, par times, music → old-style `MAPINFO` (`E1L1` → `E1M1`, `next` within an episode) |
| Slopes | yes — `heinum` → UDMF `floorplane_*` / `ceilingplane_*` |
| Parallax sky | yes — `F_SKY1`; the most common parallax ceiling tile is the `sky1` |
| Doors | sector lotag 20/22/26 → `Door_Raise` that toggles (a second use closes it), 21 → `Floor_LowerToLowest` (use on any wall of the sector) |
| Exit | the `NUKEBUTTON` sprite puts `Exit_Normal` on the nearest wall |
| Player starts | the map header start (`type 1`), up to three more `APLAYER` sprites (`2`–`4`), every `APLAYER` as a deathmatch start (`11`) |
| Music | `.MID` lumps named by their stem |
| Title / cutscenes (`.ANM`) | yes — held DeluxePaint LPF stills remapped onto `PLAYPAL`: `TITLEPIC` from `LOGO.ANM`, `INTERPIC` / `CREDIT` from `DUKETEAM.ANM`, `WIMAP0`–`2` from `CINEOV2` / `CINEOV3` / `VOL4E1`. The title plays `RADL00`… then loops `TITL00`… and `GRABBAG`. Episode ends play `E1AN`…`E4AN` once. MENUDEF `MainMenu` is New Game (episode + skill) / Options / Quit. |
| Shade | per-wall and per-sector light from Build shade (`255 − 6·shade`, clamped to 48–255) |

### Conversion rules

- Units follow Raze: `x / 16`, `−y / 16`, `−z / 256`. The y flip makes every
  wall's own sector the front (right) side.
- A wall and its `nextwall` become one two-sided linedef. Vertices are merged
  by exact position and kept as exact floats (`UdmfMap::vertex_pos`), so Build
  geometry is not snapped to integers.
- Texture scale and alignment: `scalex = 8 / xrepeat`,
  `scaley = 8 / yrepeat` (repeat 8 is one texel per map unit; the scale does
  not grow with the wall), panning → offsets; wall `cstat & 4` pegs to the
  bottom. Two-sided walls take the upper tile from `picnum`, the lower from the
  next wall when `cstat & 2`, and a masked mid texture from `overpicnum` when
  `cstat & (16|32)`, translucent for `cstat & (128|512)`.
- Floors and ceilings follow Raze's `UVCalculator`, written as UDMF
  `rotation*`, `xscale*`/`yscale*` and `xpanning*`/`ypanning*`. A flat
  repeats at its power-of-two tile size, halved for stat 8. Stat 64 aligns
  it to the first wall, stretched along a slope. Stat 4 swaps the axes and
  16/32 flip them. Panning is in 1/256 of a tile.
- Walls between sloped sectors follow both slopes at each end. Actors whose
  frame is `TNT1` or has no sprite lump draw nothing (GZDoom) instead of a
  white quad.
- Sprites are placed in the sector Room finds at their rounded position. A
  sprite within 1.5 units of a sector edge is nudged inside, and its height is
  taken from the (sloped) floor under it. Fences on roof edges no longer float
  in the sky. Billboard sprites move 0.35 units toward the camera so a
  wall cannot bury them; a larger shove floated the dumpster tag in front of
  the fire. Wall and floor sprites stay on their plane (a tag on the
  dumpster, the cinema marquee) and are only lifted a fraction of a unit
  so they do not z-fight.
- A bullet that hits a wall above a sky ceiling or below a sky floor hits the
  sky: no spark, no hole (Doom and Duke). This covers the invisible upper wall
  between two parallax ceilings.

### Using it

```sh
# Desktop (E1L1; ROOM_MAP=E2M1, ROOM_SKILL=4, extra room-app flags pass through)
./scripts/play-duke3d.sh
cargo run -p room-app --release -- --iwad testdata/duke/duke3d.grp --map E1M1 --play
# Browser (copies the GRP next to the client when present)
./scripts/play-web.sh   # then open /?iwad=duke3d.grp, or pick the file
# Multiplayer: lobby "Open a pit" → Game: Duke Nukem 3D
./scripts/room-server.sh start
```

### Native web editor

Open `editor.html` to a game chooser; no IWAD or map is loaded by default.
Choose **Doom** to open a Doom WAD (IWAD or standalone PWAD), or **Duke Nukem 3D** to open
`DUKE3D.GRP`. A PWAD selected as the base opens on its own; with a base already loaded, **Open…** mounts it as an add-on. The map list and resource tabs then show only the selected game's
editor data. Duke maps use a native Build 2D view: select and
move vertices or sprites, inspect/edit wall, sector and sprite records, and add
or remove sprites. **New Build map** creates a native `.MAP`; **Draw** creates
sector polygons, links reversed shared edges as portals, and selected sectors
can be removed. Floor labels show each sector's native Z and ART tile; turn
them on or off in the toolbar, or use the per-map sector-floor list in the
inspector to jump to a floor and edit it. The texture browser lists
all tiles from loaded ART archives; map wall and plane properties can pick a
tile and open it in the indexed pixel painter. **Native test** starts the
selected edited map through `room-duke`; **Edit** / F7 returns to the map view.
**Export GRP** writes the original GRP with edited native maps and ART archives.
**Save .MAP** downloads the current map on its own.

A native 3D visual editor and preserving add-on GRP layers are still open.

A room's `game` (`doom` or `duke3d`, `POST /api/rooms` `"game"`) is listed in
`GET /api/rooms`; the lobby adds `iwad=duke3d.grp` to the play URL. Doom mods
cannot be combined with `duke3d`. Peers still compare content fingerprints, so
a seat that loads other content is refused.

Build lets an inner loop (a bridge strip) lie along its sector's own outer
wall. Doom node building cannot take collinear overlapping lines, so the
shared span becomes a one-sided line of the inner sector and the outer wall
is cut around it.

## 2. Stages 2–8: playing the game

The conversion also writes `DECORATE`, `KEYCONF`, `SNDINFO`, the sound lumps,
sprites (`S_START`…`S_END`) and HUD graphics. It reads the CON files for the
data they hold (`room_build::ConData`). Classes are named `Duke*`. Editor
numbers are `20000 + tile` for CON actors and items, and `27000+` for
generated decorations. Shells layer the PWAD `DECORATE` over the engine's.

| Stage | Status |
|-------|--------|
| 2 Sprites | Every visible sprite (except `cstat & 32768`) spawns as an actor with ART frames. Face sprites are billboards; wall (`cstat & 16`) and floor (`cstat & 32`) sprites use `+WALLSPRITE` / `+FLATSPRITE`. Blocking (`cstat & 1`) sprites are solid, `cstat & 2` ones are translucent. `picanm` animations become state loops. `xrepeat`/`yrepeat` set `scalex`/`scaley` (a face sprite is `xrepeat/80` wide and `yrepeat/64` tall; wall and floor sprites stay `repeat/64`). Palette swaps are baked from `LOOKUP.DAT`. 5-, 7- and 8-view rotations map to Doom rotations, mirrored ones to Doom's `A2A8`-style pairs. Actions that only ship front views show those from every side. Build clips sprites by their centre, so a monster whose Doom box would cross a blocking wall or solid decoration is moved to the nearest free spot (up to 48 units, in its sector or a neighbour within a step). Animated wall and floor tiles (`picanm`) become `ANIMDEFS` texture animations. |
| 3 Items, weapons, HUD | Health, armor, ammo and power-ups subclass the Doom item they behave like, with CON quotes as pickup messages. Weapons, on Duke's slots: kick 1 (`/` quick-kicks from any gun and returns to it), pistol 2, shotgun 3, ripper 4, RPG 5, pipebombs 6, shrinker 7 (scales a monster to 25% for ~8 s, then it grows back; a stomp kills it; slot 7 also has the expander, which un-shrinks or swells a monster until it pops), devastator 8, tripbombs 9, freeze thrower 0 (holds, then a second hit shatters). The medkit (`FIRSTAID`) and steroids stay in inventory: `'` (`+invuse`) heals 1 HP per tic from the kit (taking 1 per tic, cap 100) and, when the kit cannot heal, starts a 420-tic steroid burst that doubles move. The same key starts night vision (`HEATSENSOR`, PLAYPAL 13 green), scuba (`AIRTANK`, also sets an air timer) or protective boots, and drops a `DukeHolodukeDecoy` that wanders and draws fire. The jetpack still starts on pickup. Pipebombs: fire throws one (`DukeThrownPipebomb`, whose master is the player). Holding fire winds the throw up for up to 6 tics (`DukeThrowPower`, Raze's 140 + 32 per tic held), and the throw follows the view pitch. Fire again with the remote to set off every bomb out (`A_KillChildren`). A floor `HEAVYHBOMB` is one bomb to pick up, and an `HBOMBAMMO` box is a `DukePipebombWeapon` subclass in the same slot. Tripbombs (`TRIPBOMBSPRITE` pickup) place a `DukeTripbomb` in front of the player. A map `TRIPBOMB` is one already armed. A tripbomb arms after 2 s and goes off when a shootable actor crosses its facing laser (`A_JumpIfLaserHit`, drawn with `DukeLaserLine` / `LASERLINE`). The jetpack is an `ArtiFly` and flies for 60 s from pickup: move up/down (`up_move`) climbs or dives, and jump climbs. `KEYCONF` replaces the player class with `DukePlayer` (pistol + kick, 48 bullets) and sets slots 0–9. Access cards are the Doom keys by palette (0 blue, 21 red, 23 yellow), and access switches get the matching lock. The status bar (`DukeStatusBar`, chosen when the WAD has `DUKEBAR`) draws health, armor, ammo, owned weapon slots and cards, with a fullscreen variant. Bullets hit with Duke effects: `DukePuff` (`SHOTSPARK1`) and `DukeBlood` (`BLOOD`) replace `BulletPuff` / `Blood`, and DECALDEF `BulletChip` is the `BULLETHOLE` tile. `M_SKULL1/2` are the spinning nuke icon (menu cursor). MAPINFO `skill` blocks carry the `defineskillname` names. The E1L1 opening car (`DUKECAR` / `HELECOPT`) flies down trailing fire and blows up. Guns follow Duke's `kickback_pic` timing at 35 Hz. The shotgun kicks, settles and pumps (`SHOTGUN_COCK` at kickback 15), has a two-frame muzzle flash and ejects a `DukeShotgunShell`. The pistol ejects a `DukeShell` per shot and reloads (`EJECT_CLIP` / `INSERT_CLIP`) after 12 shots (counted with `DukePistolShots`). The ripper fires and ejects a shell every 3 Duke tics. Thrown pipebombs bounce off floors and walls (`PIPEBOMB_BOUNCE`), and pipebombs, barrels and charges explode with `PIPEBOMB_EXPLODE` and scrap. Raze's destructible sprites (`BREAKABLES` in `duke.rs`: the vent `FANSPRITE`, grates, panels, chairs, cameras, bottles, plates, pipes, fetus jars, satellites, …) break on the first hit into their broken picture, or vanish, with their sound and `DukeScrap` / `DukeGlassPiece` / `DukeSmallSmoke` debris thrown at random (`frandom`), and stop blocking. Force-field sprites (`W_FORCEFIELD`, `FORCERIPPLE`) also hurt anyone who overlaps them (`ContactDamage`). `BIGFORCE` walls hurt on contact, block hitscans and projectiles, and break when shot or blasted. Breakable walls (Raze `checkhitwall`) break when shot or caught in a blast: masked glass, stained glass and force-field walls vanish and stop blocking, the masked fan grate turns `FANSPRITEBROKE` and opens, screens, vending machines, tech walls, the milk shelf, ATMs and wall lights take their broken picture. Each such line has 1 health and a death special running an ACS script (`SetLineTexture`, `Line_SetBlocking`, and `SetActorState` on a dormant `DukeWallBreak*` spot at the wall that plays the sounds and throws glass). A shot-out wall light also starts the SE3 flicker of its lotag. A fan sprite on a `FANSHADOW` floor leaves `FANSHADOWBROKE`. C-9 charges (`SEENINE`, `OOZFILTER`) share a thing id per hitag: the first to blow runs its group's script once, which moves the same-hitag effectors and lights every other charge's fuse (Duke's lotag countdown; hidden charges count to −99). A shot charge blinks 10 tics before it blows. A C-9 blast also lights same-hitag `SE 8` sectors to the effector's shade. A `MASTERSWITCH` makes the switch or touchplate with its lotag set off, after its hitag delay, the charges in its sector and that sector's other effectors (`SE 2` / `18` / `21` / `31` / `32`). Glass panes that share a wall lotag or sit in an `SE 128` sector share a UDMF `healthgroup` and break together. An ATM throws `DukeMoney`, which stays as a pickup. Babes (`FEM*`, `NAKED1`, `TOUGHGAL`, `PODFEM1`) are `+USESPECIAL`: Use with a bill takes one `DukeMoney`, prints a tip and plays `DUKE_TIP1` / `DUKE_TIP2`. A kill plays `DUKE_HIT_STRIPPER1` / `2`, drops the CON `money` bills, leaves `BARBROKE` / `BLOODYPOLE` / `FEM6PAD` where CON `cactor`s them, and the rest scream (`LADY_SCREAM`) and vanish. Water fountains (`WATERFOUNTAIN`) heal 1 HP per Use up to `MAXWATERFOUNTAINHEALTH` (50), and toilets / stalls play `FLUSH_TOILET`. Shooting one swaps in `WATERFOUNTAINBROKE` / `TOILETBROKE` / `STALLBROKE` and stops Use. A pipebomb thrown while holding move-down on the ground is Duke's crouch roll (lower spawn, no standing lift, Raze's crouched speed). |
| 4 Enemies | Trooper, pig cop, enforcer, octabrain, commander, drone, recon car, turret, slimer, shark, tank and the bosses. Frames and tic lengths come from the CON `action`s; health from the CON `actor` strength. Attacks map to Doom codepointers: lasers / spit / balls / rockets, hitscan, the drone's charge, melee. Sounds come from the CON sound names. Sprite `lotag` sets the lowest skill an enemy appears on. Palette-21 troopers are `DukeTrooperCaptain`: they chase like a trooper and occasionally warp nearby (`A_RandomTeleport`). Palette-21 non-boss enemies are `*Mini` with double health. |
| 5 CON | **Data + enter/use/fire/weapon/hurt/exit events.** `DEFS.CON`, `USER.CON` and `GAME.CON` give defines, sounds, quotes, actions and actor strengths. `onevent` `quote` / `sound` run on map enter (`ENTERLEVEL`), Use (`USE`), fire (`FIRE`), weapon select (`CHANGEWEAPON`), holster (`HOLSTER`), when hurt (`GETSHOT` / `INCURDAMAGE`), on death (`KILLIT`), on a fresh start (`RESETWEAPONS` / `RESETINVENTORY`), on respawn (`RESPAWN`), and at intermission (`EGS` / `ENDLEVEL`). There is no full CON VM: `ai`, `move` and most `ifxxx` logic is replaced by Doom AI; CONEVT `state NAME` jumps the pawn to that label; `ifrnd`, `ifphealthl` / `ifphealthg`, `ifinwater` / `ifonwater`, `ifdead`, `ifmultiplayer`, `ifoutside`, `ifhitweapon`, `ifpalswap`, `ifsomethingonplayer`, `ifsquished`, `ifpinventory`, `ifcansee`, `ifpdistl` / `ifpdistg`, `ifhitspace`, `ifgotweaponce`, `ifnotmoving`, `ifrespawn`, `iffloordistl` / `ifceilingdistl`, `ifawayfromwall`, `ifinspace`, `ifinouterspace`, `ifgapzl`, `ifangdiffl`, `ifbulletnear`, `ifwasweapon`, `ifnosounds`, `ifsound`, `ifcanshoottarget`, `ifcanseetarget`, `ifspritepal`, `ifactor`, `ifcount`, `ifactioncount`, `ifstrength`, `ifspawnedby`, `ifactornotstayput`, `ifaction`, `ifai`, `ifmove`, and `ifp palive` / `ponground` / `pfalling` / `pducking` / `pjetpack` / `ponsteroids` / `pshrunk` / `pkicking` / `phigher` / `pwalkingback` / `paiming` / `pweapon` (and kin) gate CONEVT quotes. Jetpack, diving and sitting variants are separate classes; palette behaviour and recolouring are not. |
| 6 Sector effects | Doors 20/22/26 (`Door_Raise` toggles: open until used again, or an SE10 auto-close after 150 tics; they open to the lowest neighbouring ceiling), 21 floor doors (an ACS toggle: a shut floor drops to the next floor below the ceiling around it, an open one rises back), 29 teeth doors (the ceiling opens; `SE 22` prongs retract along the effector angle), 16/17 platforms, 18/19 elevators. Activators, touchplates, switches (wall or sprite, with `SW1D`/`SW2D` textures), access switches, secret sectors (32767), end-level sectors (−1 and −2), the nuke button. SE4 flicker becomes light special 17. A `CYCLER` pulses the sector light (`Light_Glow` from an `OPEN` script). SE7 transporters standing on their sector floor and SE23 one-way exits become teleport lines. An SE7 floating above its floor (the E1L1 rooftop vent shaft) becomes a Room fall-link plane portal (`Sector_SetPortal` kind 201, with both effector heights packed in arg 2): a player whose eye comes within 24 units of the source effector moves by the effector offset to 24 units below the destination one (above it when rising), as in Raze `movetransports`. SE17 transport elevators teleport between the two shafts. Hurt floors use the tile: slime 5, rail 10, plasma 20, lava 10, every 32 tics. An `OPEN` script starts the effects that run on their own: SE24/34/26 scroll the floor and carry actors standing on it (the sector geometry stays put), SE6/14/30 follow `LOCATORS` (`user_subway`): anyone near the car is carried and the car walls move as a polyobject (`user_subpo` / `Polyobj_MoveTo`), SE25/35 are perpetual lifts, SE29 waggles the floor, SE28 blinks the light. Activators run SE18 (a step per use), SE21 and SE31/32 (floor or ceiling toward the effector). SE2 quakes when the sector is entered; `SE 33` drops `DukeScrap` from the ceiling while a quake is felt (`A_JumpIfQuake`). SE3 flickers once a wall light with its hitag as lotag is shot out, SE12 switches set the light. `SE 8` / `SE 9` of the same hitag as a ceiling door (20/22/26/29) light those rooms to the effector's shade when the door opens (`SE 8`) or closes (`SE 9`), and restore the sector shade the other way. SE13 sectors start shut at the effector's height (both planes, or only the nearer one at angle 512, with a parallax ceiling hidden until then) and go back when a C-9 of the same hitag blows, which also runs same-hitag SE8 lights, SE18 and SE21. SE19 drops the ceiling when one of its walls is shot. SE36 spawns a shooter, and SE5 spawns a turret. Moving sectors become polyobjects drawn in a void east of the map and moved into place at load. Sliding doors (25 + SE15) and star doors (9) become `Polyobj_DoorSlide` groups, and swinging doors (23 + SE11) become `Polyobj_DoorSwing`. Each group is an ACS script that its walls (or the matching activators) start with `ACS_Execute`. SE10 makes a door close again by itself. Solid SE0 rotating sectors around their SE1 pivot spin forever from an `OPEN` script (`Polyobj_RotateLeft`/`Right`, angle 255). Lotag-30 walkable gears (`SE 0` + `SE 1`) write `user_spin` / `user_spinx` / `user_spiny` and carry on-floor actors around the pivot while spinning the floor texture (`plane_angles`; Hexen-byte speed 27, `GPSPEED` scaled). Lotag-27 stretch bridges (`SE 20`) that start paper-thin open to a 64-unit walkable height (`user_bridge`; always extended). An activator (or `Sector_Stretch` 284) slides that sector's vertices along the effector angle by `GPSPEED`/16 (`user_stretch`) and carries whoever is on the floor. `SE 22` teeth write `user_teeth` and retract along the effector angle when the matching lotag-29 door ceiling leaves its rest height (Atomic has none). `SE 16` reactor floors write `user_skew` and oscillate their vertices along the effector angle (Atomic has none). Mirror walls (`cstat & 32`, `MIRROR` overpic) become one-sided `Line_Mirror` lines. SE7 water links in water sectors (lotag 1 and 2) become plane portals of the Room water kind: players dive by moving down or falling fast, and surface by moving up. Water sectors (lotag 2) are `waterzone`. Sector `visibility` darkens the light. |
| 7 Sounds | `.VOC` and `.WAV` play through `SNDINFO` (`duke/<name>`), and Doom's logical sounds are remapped to Duke sounds. Looping `MUSICANDSFX` ambients retrigger on the clip length (none inside moving sectors). A `MUSICANDSFX` in a door, lift or elevator sector becomes its UDMF `soundsequence`: a SNDSEQ choice list `DukeMover<lo>_<hi>` that plays the lotag sound on opening and the hitag one (or lotag again) on closing. Mover sectors without one are `silent`, as in Duke. Level music comes from MAPINFO. |
| 8 Skies | Multi-tile parallax skies are composed into one `BSKYnnnn` texture from the eight `pskyoff` slots Duke uses for each sky tile (moon, orbit, LA, the rest flat). The texture wraps once around the horizon. Above and below it, the sky's edge row blurs and fades into the average colour of its top and bottom rows. |

What the conversion does not do yet is listed in §3.

### Room extensions used by the conversion

These are documented deviations from GZDoom, all opt-in by content:

- **Water portal kind 200.** `Sector_SetPortal(tag, 200, plane, dx, dy)`
  (`room_map::PORTAL_KIND_WATER`) links a water surface to the sector
  below with an explicit offset. Only players cross it: down when
  `up_move < 0` or falling faster than 8, up when `up_move > 0` or rising.
- **`ContactDamage`.** An actor with `ContactDamage N` hurts each
  overlapping shootable actor by `N` every 8 tics. Duke force-field
  sprites use this.
- **`A_HealTarget(amount [, max])`.** Adds hit points to the actor's
  target, capped at `max` (or the class default health). Duke fountains
  use this.
- **`A_JumpIfQuake("Label")`.** Jumps when a tremor reaches the actor
  (`World::quake_intensity_at`). `SE 33` uses this to drop scrap.
- **`A_JumpIfLaserHit("Label" [, range])`.** Jumps when a shootable actor
  (not the caller or its master) crosses a beam along the caller's
  facing. Spawns `DukeLaserLine` along the ray when that class exists.
  Tripbombs use this.
- **`Player.ForwardMove`.** Scales player XY thrust from ticcmd (default
  1). `DukePlayer` uses 1.25. Steroids still double the result.
- **`A_RandomTeleport([range])`.** Warps to a free spot within `range`
  (default 192), using `pr_rndtele`. Fog at both ends. Trooper captains
  use this.
- **`Sector_Stretch` (284).** Toggles a Duke `SE 20` stretch
  (`user_stretch`): the sector's vertices slide along the effector
  angle by `user_stdist` and carry on-floor actors. Activators fire it.
- **`A_ChangeCameraToNearest("Class" [, revert])`.** Sets the target
  player's `ChangeCamera` view to the nearest actor of `Class`. Move
  input restores the player eye when `revert` is set (default). Duke
  viewscreens use this.
- **`KillTaunt` / `IdleTaunt`.** Repeatable DECORATE properties: lists of
  logical sounds. A player kill of a `COUNTKILL` plays a `KillTaunt`
  (`pr_killtaunt`). Standing still for 140 tics (~4 s) plays an
  `IdleTaunt` (`pr_idletaunt`). `DukePlayer` kill lines are `DUKE_YES`,
  `DUKE_GETWEAPON2` and `DUKE_CRACK2`; idle lines are `DUKE_CRACK_FIRST`,
  `DUKE_CRACK` and `DUKE_CRACK2`.
- **Flight.** `ArtiFly` (engine `DECORATE`, no editor number) grants
  `PowerFlight` for 60 s on pickup. GZDoom needs an inventory use first.
  While flying, `up_move` sets vertical speed (`upmove / 128`), jump climbs
  at 3, and vertical drag is `FRICTION_FLY`, as in `P_MovePlayer`.
- **Polyobjects.** `Polyobj_DoorSwing` with a negative angle swings
  clockwise. `Polyobj_RotateLeft`/`Right` angle 255 spins forever and 0 is
  one full turn (GZDoom `po_man`). The E1L1 cinema doors are the swing
  doors under the marquee. Walking into them opens them, including the edge that only marks the polyobject (E still works);
  the panel pushes Duke through, and a balcony overlapping the lobby does
  not seal the floor.   The closed door's picture is drawn on the street
  side of the slab (the step face used to point into the lobby, so the
  opening was the clear color). When that tile is the dark glass, the
  face uses the brighter picture from the other side of the same wall.
  Finding a secret prints the player sector, the wall in front, any
  sector that has moved, and nearby doors to the console (`look` prints
  the same dump). A thrown pipebomb uses Duke's repeat of 9,
  so the 86-wide tile is not drawn at full size. A gas can dies on the
  first shot and is not knocked into the air. Wall sprites face their
  Build angle, so a sign such as the police tape reads from the front.
  The dumpster fire cycles `FIRE_FRAMES` and is drawn
  additively. The wall crack is not an entrance.
- **Bounce subset.** `+BOUNCEONFLOORS` / `+BOUNCEONCEILINGS`,
  `+BOUNCEONWALLS` (`+DOOMBOUNCE` / `+HEXENBOUNCE` set both) and
  `BounceFactor` reflect the velocity, playing `BounceSound` /
  `WallBounceSound`. A floor bounce slower than 1 unit per tic settles.
  There is no bounce count, actor bounce or `Bounce` state.
- **Sector sound sequences.** UDMF sector `soundsequence` names a SNDSEQ
  sequence or choice list (`[Name 0 Open 1 Close ]`). Door, lift and floor
  movers start choice 0 when opening or starting and 1 when closing, and
  their stop sounds come from the sequence.
- **`A_CheckProximity`.** Room reads GZDoom's order (`jump, class, distance,
  count, flags`) and the `CPXF_*` flags. It still accepts its older
  `class, distance, count, flags, jump` order.
- **`DamageType "Shrink"` / `"Grow"` / `"Ice"`.** A shrink hit scales a
  `COUNTKILL` to 25% for ~8 s (`user_dukeshink`), then it grows back; a
  stomp kills it. A grow hit un-shrinks or swells a monster (~2 s,
  `user_dukegrow`) until it pops. An ice hit freezes (~8 s); when that
  wears off a monster stays as an ice statue (`user_dukeicecorpse`)
  until a further hit shatters it. Duke shrinker / expander / freezer
  missiles use these.
- **Blast damage on lines.** `A_Explode` / radius attacks damage lines with
  health (GZDoom `P_GeometryRadiusAttack`) by distance to the line, without
  GZDoom's sight check.
- **Shot lines.** Room never raises impact activations, so the converter
  gives lines that run when shot (SE19 walls, breakable walls) 1 health and
  a death special.

Input: move up/down (`+moveup`/`+movedown`; desktop C = down, web Q/C) fill
`up_move`, and jump also swims up. Weapon keys 8, 9 and 0 select slots 8, 9
and 0 (`weapon_select` 10). `/` is a quick kick (`+quickkick`). `'` uses inventory (`+invuse`).

Tests: `crates/room-game/tests/duke3d_play.rs` covers E1M1 classes, spawns
(95%+ of monsters standing free), a trooper fight, `give all`, polyobject
doors and rotators, pipebombs, tripbombs and the jetpack, the vent fan
breaking when shot, the shotgun pump and shell, the pistol reload, door
sound sequences, a C-9 group opening the rooftop vent and lighting same-hitag `SE 8`s, glass
breaking when shot and spreading to the next pane, a masterswitch running
its sector's effectors, ATMs dropping money you can pick up, floor doors closing again, and
the pipebomb wind-up, pitch and crouch roll, a ceiling door lighting its `SE 8` rooms, a force field hurting on touch, tipping a babe with `DukeMoney` (CON tip line), shooting a babe for money or a leftover picture, drinking from a fountain, a kill taunt when a trooper dies, an idle knuckle-crack if you stand still, `SE 33` scrap during a quake, shooting a fountain swapping its broken picture, a `/` quick kick that returns to the pistol, `'` using a stored medkit then steroids, `'` starting night vision (green PLAYPAL 13), scuba and a holoduke decoy, an underwater blue tint, a shrinker / freezer changing a trooper (shrink still then grow-back; freeze holds the ice pose), a force-field wall breaking when shot, a C-9 blast breaking a nearby force-field wall (walk-through, mid gone), a pal-21 trooper captain warping nearby, a pal-21 enforcer mini-boss with double health, a viewscreen looking through a security camera (and binding a live `VCAMA0` canvas to that camera), standing on an E1M2 lotag-30 gear turning you around its pivot and spinning the floor texture, riding the E2M1 subway along its locators (car walls included), E1M1 lotag-27 bridges starting walkable and stretching when toggled, a `CYCLER` pulsing its sector light, a `RESPAWN` marker that does not block doors, water-bubble makers, a lightning flash, a pulsing force sphere, a frame-effect afterimage on a trooper, `SE 22` teeth retracting with a lotag-29 door, an `SE 16` reactor floor skewing, unfinished `SE 40`–`45` becoming a 3D floor, stacked rooms becoming a 3D floor, a nested loft becoming a 3D floor, an overlapping strip becoming a 3D floor, a shared-wall loft becoming a 3D floor, a grazing overlap becoming a 3D floor, a shared-corner pair becoming a 3D floor, an E1M1 Room `.fsave` restoring Duke's health, an E1M1 Room `.fdem` that verifies, a `.DMO` that looks through a security camera, holding move-down to crouch, a long fall taking half health, airborne coast keeping XY speed, airborne hold adding to XY speed, CON enter-level quotes, CON EGS quotes on intermission, CON GETSHOT / KILLIT quotes, CON USE / FIRE quotes, and CON CHANGEWEAPON / HOLSTER quotes, CON ifpalswap quotes, CON ifsomethingonplayer quotes, CON ifsquished quotes, CON ifpinventory quotes, CON ifcansee quotes, CON ifpdistl quotes, CON ifhitspace quotes, CON ifgotweaponce quotes, CON ifnotmoving quotes, CON ifrespawn / RESPAWN quotes, CON iffloordistl / ifceilingdistl quotes, CON iffloordistg / ifceilingdistg quotes, CON ifawayfromwall quotes, CON ifinspace quotes, CON ifgapzl quotes, CON ifgapzg quotes, CON ifangdiffl quotes, CON ifangdiffg quotes, CON ifbulletnear quotes, CON ifinouterspace quotes, CON ifwasweapon quotes, CON ifnosounds quotes, CON ifsound quotes, CON ifcanshoottarget quotes, CON ifspritepal quotes, CON ifactor quotes, CON ifcount quotes, CON ifp pducking quotes, CON ifp pshrunk quotes, CON ifcanseetarget quotes, CON ifactioncount quotes, CON ifstrength quotes, CON ifspawnedby quotes, CON ifp phigher quotes, CON ifp pwalkingback quotes, CON ifactornotstayput quotes, CON ifp paiming quotes, CON ifaction quotes, CON ifai quotes, CON ifmove quotes, CON ifp pweapon quotes, CON ifp pweaponout quotes, CON else quotes, CON break, CON palfrom flash, CON addphealth, CON addammo, CON addweapon, CON addinventory, CON spawn, CON guts, CON shoot, CON killit, CON wackplayer, CON pkick, CON operate, CON operateactivators, CON operatesectors, CON money, CON lotsofglass, CON spritepal, CON sizeat, CON cstat, CON cstator, CON addstrength, CON strength, CON clipdist, CON cactor, CON sizeto, CON count / resetcount, CON actioncount / resetactioncount, CON addkills, CON hitradius, CON fall, CON sleeptime, CON getlastpal, CON debris, CON mail, CON paper, CON tip, CON mikesnd, CON resetplayer, CON endofgame, CON tossweapon, CON resetweapons, CON movesprite, CON setsprite, CON lockplayer, CON setplayerangle, CON setactorangle, CON getplayerangle, CON getactorangle, CON lotsofcolour, CON zshoot, CON eshoot, CON ezshoot, CON pstomp, CON findplayer, CON findotherplayer, CON getzrange, CON updatesector, CON clipmove, CON changespritestat, CON wackmove, CON respawnhitag, CON getflorzofslope, CON getceilzofslope, CON useractor, CON useractor hitradius, CON useractor fall, CON useractor operate, CON useractor operateactivators, CON useractor operatesectors, CON useractor lotsofglass, CON useractor money, CON useractor mail, CON useractor paper, CON useractor ifactioncount, CON useractor ifrnd, CON useractor lotsofcolour, CON useractor zshoot, CON useractor ifactor, CON useractor ifspawnedby, CON useractor ifactornotstayput, CON useractor ifaction, CON useractor addkills, CON useractor ifinwater, CON useractor tip, CON useractor mikesnd, CON useractor ifoutside, CON useractor ifinspace, CON useractor iffloordistl, CON useractor ifgapzl, CON useractor ifawayfromwall, CON useractor ifangdiffl, CON useractor ifbulletnear, CON useractor ifcanshoottarget, CON useractor ifsound, CON useractor quake, CON useractor endofgame, CON useractor debug, CON useractor pstomp, CON useractor ifwasweapon, CON useractor ifhitweapon, CON useractor ifmultiplayer, CON useractor ifsquished, CON useractor ifrespawn, CON useractor ifdead, CON useractor ifnotmoving, CON useractor wackplayer, CON useractor pkick, CON useractor lockplayer, CON useractor palfrom, CON useractor addphealth, CON useractor resetplayer, CON useractor tossweapon, CON useractor addweapon, CON useractor addinventory, CON useractor addammo, CON useractor movesprite, CON useractor setsprite, CON useractor clipmove, CON useractor changespritestat, CON useractor wackmove, CON useractor getzrange, CON useractor updatesector, CON useractor getflorzofslope, CON useractor getceilzofslope, CON useractor respawnhitag, CON useractor findotherplayer, CON useractor emit, CON useractor emit count, CON useractor emit lotsofglass, CON useractor emit ifinwater, CON useractor emit ifdead, CON useractor emit ifp, CON useractor emit ifhitspace, CON useractor emit ifrespawn, CON useractor emit ifinspace, CON useractor emit ifbulletnear, CON useractor emit ifsound, CON useractor emit ifactioncount, CON useractor emit ifai, CON useractor emit state, CON useractor emit addphealth, CON useractor emit addammo, CON useractor emit resetplayer, CON useractor emit clipmove, CON useractor emit setsprite, CON useractor emit zshoot, CON useractor emit eshoot, CON useractor emit debug, CON useractor emit quake, CON useractor emit sleeptime, CON useractor ifp, CON useractor break, CON useractor movevel, CON debug, CON quake, CON soundonce, CON spritesound, CON stopsound, and CON state / ai / move / action jumps. `crates/room-build/tests/duke3d.rs` checks that
all maps convert, and `room-build` unit tests cover the converter and content.

Every stage keeps the playsim determinism rules (`docs/02-conventions.md`).

## 3. Not supported (Duke Nukem 3D)

All four episodes (Atomic Edition `E1L1`–`E4L11`) convert and load. This is
what still plays differently from Duke. Approximations that did land are in
§2 (a lotag-30 gear carries you around the pivot and spins the floor texture).
Remove an entry when it lands, and log it in
`docs/21-milestone-progress.md`.

### Scripting

- **CON events only.** `onevent` `quote` / `sound` / `globalsound` run
  on map enter (`EVENT_ENTERLEVEL`), Use (`EVENT_USE`), fire
  (`EVENT_FIRE`), weapon select (`EVENT_CHANGEWEAPON`), holster
  (`EVENT_HOLSTER`), when the player is hurt (`EVENT_GETSHOT` /
  `EVENT_INCURDAMAGE`),   on death (`EVENT_KILLIT`), on a fresh map start   (`EVENT_RESETWEAPONS` /
  `EVENT_RESETINVENTORY`), on a player respawn (`EVENT_RESPAWN`),
  and at intermission (`EVENT_EGS` / `EVENT_ENDLEVEL`) via `CONEVT`.
  `event USERACTOR` runs `count` / `addstrength` / `strength` /
  `sleeptime` / `ai` / `move` / `state` / `action` / `findplayer` /
  `shoot` / `ifcansee` / `ifpdistl` / `ifpdistg` /   `ifstrength` /
  `ifcount` / `killit` / `guts` / `sound` / `print` /   `spritepal` /
  `spawn` / `ifspritepal` / `soundonce` / `spritesound` / `cstat` / `cstator` /
  `sizeat` / `sizeto` / `clipdist` /   `ifcanseetarget` / `cactor` /
  `getlastpal` / `stopsound` / `hitradius` / `fall` / `operate` / `operateactivators` / `operatesectors` /
  `lotsofglass` / `money` / `mail` / `paper` / `actioncount` /
  `resetactioncount` / `ifactioncount` / `ifrnd` / `lotsofcolour` / `zshoot` / `ifactor` / `ifspawnedby` / `ifactornotstayput` / `ifaction` / `addkills` / `ifinwater` / `ifonwater` / `tip` / `mikesnd` / `ifoutside` / `ifinspace` / `ifinouterspace` / `iffloordistl` / `ifceilingdistl` / `ifgapzl` / `ifgapzg` / `ifawayfromwall` / `ifangdiffl` / `ifbulletnear` / `ifcanshoottarget` / `ifsound` / `ifnosounds` / `quake` / `endofgame` / `debug` / `pstomp` / `ifwasweapon` / `ifhitweapon` / `ifpalswap` / `ifsomethingonplayer` / `ifmultiplayer` / `ifsquished` / `ifrespawn` / `ifdead` / `ifnotmoving` / `ifp` / `ifhitspace` / `ifgotweaponce` / `ifpinventory` / `ifhealthg` / `break` / `setplayerangle` / `setactorangle` / `getplayerangle` / `getactorangle` / `movevel` / `wackplayer` / `pkick` / `lockplayer` / `palfrom` / `addphealth` / `resetplayer` / `tossweapon` / `resetweapons` / `resetinventory` / `addweapon` / `addinventory` / `addammo` / `movesprite` / `setsprite` / `clipmove` / `changespritestat` / `wackmove` / `getzrange` / `updatesector` / `getflorzofslope` / `getceilzofslope` / `respawnhitag` / `findotherplayer` on living `COUNTKILL`s each tic.
  `ai`, `move`, `state`, `action` inside `useractor` (converter emits a subset of the body as `event USERACTOR`; CONEVT `ai` /
  `move` / `state` / `action NAME` jump the pawn; CONEVT `else` flips
  a failed `if`; CONEVT `break` stops the rest of the event; CONEVT
  `palfrom N` flashes the HUD red (`damage_count`); CONEVT `addphealth N`
  heals the pawn; CONEVT `addammo NAME N` gives ammo; CONEVT
  `addweapon NAME N` gives the weapon and N ammo; CONEVT
  `addinventory NAME N` gives an inventory item; CONEVT `spawn NAME`
  drops that actor at the pawn; CONEVT `guts NAME N` throws N pieces;
  CONEVT `shoot NAME` fires that actor along the pawn's facing;
  CONEVT `killit` drops the pawn's health to 0; CONEVT `wackplayer`
  hurts the pawn and flashes the HUD; CONEVT `pkick` starts the
  pawn's `/` quick kick; CONEVT `operate` uses the
  special in front of the pawn; CONEVT `operateactivators N` uses
  every special with that tag; CONEVT `operatesectors` uses specials
  tagged to the pawn's sector; CONEVT `money N` drops N bills;
  CONEVT `lotsofglass N` throws N glass pieces; CONEVT `spritepal N`
  sets the pawn's palette; CONEVT `sizeat X Y` sets Build xrepeat /
  yrepeat (`scale / 64`); CONEVT `sizeto X Y` steps those repeats by 1;
  CONEVT `cstat N` sets blocking / translucent /
  invisible; CONEVT `cstator N` ORs those bits; CONEVT `addstrength N`
  adds hit points; CONEVT `strength N` sets hit points; CONEVT
  `clipdist N` sets collision radius (`/16`);   CONEVT `cactor NAME`
  changes the pawn's class; CONEVT `count N` sets the action counter
  (`ifcount` reads it); CONEVT `resetcount` zeros it; CONEVT
  `actioncount N` sets the action-frame counter (`ifactioncount`
  reads it); CONEVT `resetactioncount` zeros it; CONEVT `addkills N`
  adds to the map kill count; CONEVT `hitradius R D` blasts D
  damage within Build radius R (`/16`); CONEVT `fall` snaps the
  pawn to the floor; CONEVT `sleeptime N` sets the reaction timer;
  CONEVT `getlastpal` restores the previous palette; CONEVT
  `debris NAME N` throws N pieces; CONEVT `mail N` drops N letters;
  CONEVT `paper N` drops N sheets;   CONEVT `tip` prints the tip
  line and plays `duke/duke_tip1`; CONEVT `mikesnd` plays the pawn's
  use chatter (`duke/duke_use1`); CONEVT `resetplayer` restores
  health and the spawn pose; CONEVT `endofgame` finishes the map;
  CONEVT `tossweapon` drops the ready gun; CONEVT `resetweapons` keeps kick/pistol and strips the rest; CONEVT `resetinventory` strips inventory items; CONEVT `movesprite X Y Z`
  shifts the pawn by Build units; CONEVT `setsprite X Y Z` snaps
  the pawn to Build units; CONEVT `lockplayer N` freezes the pawn
  (`0` unlocks); CONEVT `setplayerangle N` sets yaw from a Build
  angle; CONEVT `quake N` starts a map-wide rumble; CONEVT
  `soundonce NAME` plays that sound only if it is not already queued;
  CONEVT `spritesound NAME` plays it from the pawn;
  CONEVT `stopsound NAME` drops it from the queue; CONEVT
  `setactorangle N` sets yaw from a Build angle; CONEVT
  `getplayerangle` stores that angle as `user_playerangle`; CONEVT
  `getactorangle` stores it as `user_actorangle`; CONEVT
  `lotsofcolour N` throws N tinted glass pieces; CONEVT
  `zshoot NAME Z` fires along facing with extra Build Z velocity;
  CONEVT `eshoot NAME` fires along facing; CONEVT `ezshoot NAME Z`
  fires with extra Build Z velocity; CONEVT `pstomp` stomps a
  shootable actor underfoot; CONEVT `findplayer` stores Build XY
  distance to the nearest `COUNTKILL` as `user_findplayer`; CONEVT
  `debug` prints `DEBUG`; CONEVT `findotherplayer` stores Build XY
  distance to another player as `user_findotherplayer` (`-1` if none);
  CONEVT `getzrange` stores floor / ceiling Build Z as `user_floorz` /
  `user_ceilz`;   CONEVT `updatesector` stores the pawn's sector index
  as `user_sector`;   CONEVT `clipmove X Y Z` try-moves the pawn by
  Build units and stores `user_clipmove` 1/0;   CONEVT `changespritestat N`
  stores the pawn's sprite stat as `user_spritestat`;   CONEVT `wackmove X Y Z`
  try-moves and wacks a solid that blocked;   CONEVT `respawnhitag N`
  revives dead actors whose tid is N;   CONEVT `getflorzofslope` stores
  floor Build Z as `user_florz`; CONEVT `getceilzofslope` stores
  ceiling Build Z as `user_ceilzofslope`;   CONEVT `event USERACTOR`
  runs `count` / `addstrength` / `strength` / `sleeptime` / `ai` /
  `move` / `state` / `action` / `findplayer` / `shoot` / `ifcansee` /
  `ifpdistl` / `ifpdistg` / `ifstrength` / `ifcount` / `killit` /
  `guts` / `sound` / `print` / `spritepal` / `spawn` / `ifspritepal` /
  `soundonce` / `spritesound` / `cstat` / `cstator` / `sizeat` / `sizeto` / `clipdist` /
  `ifcanseetarget` / `cactor` / `getlastpal` / `stopsound` /
  `hitradius` / `fall` / `operate` / `operateactivators` / `operatesectors` /   `lotsofglass` / `money` / `mail` / `paper` / `actioncount` /
  `resetactioncount` / `ifactioncount` / `ifrnd` / `lotsofcolour` / `zshoot` / `ifactor` / `ifspawnedby` / `ifactornotstayput` / `ifaction` / `addkills` / `ifinwater` / `ifonwater` / `tip` / `mikesnd` / `ifoutside` / `ifinspace` / `ifinouterspace` / `iffloordistl` / `ifceilingdistl` / `ifgapzl` / `ifgapzg` / `ifawayfromwall` / `ifangdiffl` / `ifbulletnear` / `ifcanshoottarget` / `ifsound` / `ifnosounds` / `quake` / `endofgame` / `debug` / `pstomp` / `ifwasweapon` / `ifhitweapon` / `ifpalswap` / `ifsomethingonplayer` / `ifmultiplayer` / `ifsquished` / `ifrespawn` / `ifdead` / `ifnotmoving` / `ifp` / `ifhitspace` / `ifgotweaponce` / `ifpinventory` / `ifhealthg` / `break` / `setplayerangle` / `setactorangle` / `getplayerangle` / `getactorangle` / `movevel` / `wackplayer` / `pkick` / `lockplayer` / `palfrom` / `addphealth` / `resetplayer` / `tossweapon` / `resetweapons` / `resetinventory` / `addweapon` / `addinventory` / `addammo` / `movesprite` / `setsprite` / `clipmove` / `changespritestat` / `wackmove` / `getzrange` / `updatesector` / `getflorzofslope` / `getceilzofslope` / `respawnhitag` / `findotherplayer` on living `COUNTKILL`s each tic),
  `ifxxx` (except `ifrnd`, `ifphealthl` /
  `ifphealthg`, `ifinwater` / `ifonwater`, `ifdead`,
  `ifmultiplayer`, `ifoutside`, `ifhitweapon`, `ifpalswap`, `ifsomethingonplayer`, `ifsquished`,
  `ifpinventory`, `ifcansee`, `ifpdistl` / `ifpdistg`,
  `ifhitspace`, `ifgotweaponce`, `ifnotmoving`, `ifrespawn`,
  `iffloordistl` / `ifceilingdistl` / `iffloordistg` / `ifceilingdistg`, `ifawayfromwall`,
  `ifinspace`, `ifinouterspace`, `ifgapzl` / `ifgapzg`, `ifangdiffl` /
  `ifangdiffg`, `ifbulletnear`,
  `ifwasweapon`, `ifnosounds`, `ifsound`,   `ifcanshoottarget`, `ifcanseetarget`, `ifspritepal`,
  `ifactor`, `ifcount`, `ifactioncount`, `ifstrength`, `ifspawnedby`,
  `ifactornotstayput`, `ifaction`, `ifai`, `ifmove`, and `ifp palive` / `ponground` / `pfalling` /
  `pjumping` / `pstanding` / `pwalking` / `prunning` / `pducking` /
  `pjetpack` / `ponsteroids` / `pshrunk` / `pkicking` / `phigher` /
  `pwalkingback` / `prunningback` / `paiming` / `plooking` / `pweapon` /
  `pweaponout`, which gate
  the following quote / sound) and `useractor` chase uses the
  same wall-hug when CON did not emit `movevel` this tic
  (`A_Chase` on a `Duke*` class slides left, then right, along
  the wall instead of Doom `P_NewChaseDir`; CON `move NAME vel angvel` is stored; `event USERACTOR`
  `movevel` walks that vel along facing then applies angvel
  and sets `user_conchase` so `A_Chase` keeps melee/missile but
  does not also walk; a blocked `movevel` step slides left, then right,
  along the wall;
  `faceplayer` / `seekplayer` snap to the player, `fleeenemy` snaps
  away, `faceplayerslow` turns 22.5° per tic, `spin` adds 22.5°,
  `dodgebullet` faces away from a nearby missile, `jumptoplayer`
  hops from the floor, `geth` steps XY toward the player, `getv`
  steps Z toward the player, `randomangle` sets yaw from the RNG,
  `faceplayersmart` leads the player's velocity, `furthestdir`
  picks the 8-way heading furthest from the player).
  User CON mods still get their data (defines, sounds, quotes, actions,
  strengths).
- **Palette variants are the base enemy.** Jetpack troopers and diving pig
  cops fly, and troopers that sit or stand guard do not walk. Palette-21
  troopers teleport as captains. Palette-21 non-boss enemies (enforcers,
  octabrains, …) are `*Mini` classes with double health.   Frozen actors
  hold on their `Ice` still, then become an ice statue that shatters;
  shrunk ones hold `Shrink` and grow back to `See`.

### Sector effects that need moving geometry

Unfinished Duke `SE 40`–`45` floor-over-floor sprites become a Room
`Sector_Set3DFloor` (solid / swimmable / see-through) on the matching
hitag sector. Two sectors that sit one above the other become the
same solid 3D floor when one centroid is inside the other (matching
loops or a loft over a hall, even if they share a wall) or their 2D
boxes overlap at all (including a graze) or only share an edge
or a corner.

`RESPAWN` markers spawn as non-solid `DukeRespawn` things (`+NOBLOCKMAP`):
after the hitag delay (or ~30 s) they spawn the lotag actor once
(`A_DukeRespawn`). `CYCLER` sprites pulse their sector light
(`Light_Glow`) between the sector shade and the cycler's lotag shade.
`LOCATORS` order subway / train carry. `GPSPEED` is read for the
effects above.

### Map sprites

- **Cameras and viewscreens.** Use a `VIEWSCREEN` to look through the
  nearest `CAMERA1` or `SE 27` demo camera (`A_ChangeCameraToNearest`).
  Move to drop back to the player eye. The screen draws `VCAMA0`, bound
  to that camera (`SetCameraToTexture` / ANIMDEFS `cameratexture`).
  Desktop and the browser refresh that canvas (GPU copy on the web, no
  CPU readback). Playing a `.DMO` (`World::set_demo_cameras`) looks
  through those cameras: a camera that can see the player, else the
  nearest, cycling every 70 tics. Move does not drop the view.
- **Force fields** block when the sprite is blocking, a shot removes them,
  and touching one hurts (`ContactDamage`). `BIGFORCE` / `W_FORCEFIELD`
  walls hurt on contact, block shots, and break when shot or blasted
  (texture gone, no more contact or projectile block).
- **Eggs** hatch a slimer on a random delay or when shot. The shuttle is a
  decoration. `FORCESPHERE` is a translucent pulsing ball
  (`DukeForceSphere` / `A_SetScale`); the Overlord emits one when it fires.
  `FRAMEEFFECT1` is a translucent afterimage (`DukeFrameEffect` /
  `A_DukeFrameEffect`) that copies the nearest monster. `WATERBUBBLEMAKER` pops `DukeWaterBubble`
  sprites that rise and vanish (`A_SpawnItemEx`). `NATURALLIGHTNING` rolls
  the dice every couple of seconds and flashes the map
  (`A_CallSpecial(109)` / `Light_ForceLightning`).
- **Breakable walls**: a C-9 blast (or a shot) zeros health walls (force
  fields, glass): the mid texture is cleared and the line stops blocking
  walk, shots and sight. Linedefs stay in the BSP.
- Barrels, cans, extinguishers, `SEENINE` and OOZ filters explode when shot.

### Weapons, items, player

- **Expander** is slot 7 with the shrinker (`DukeExpander`, `GROWAMMO`).
  A grow hit un-shrinks or swells a `COUNTKILL` for ~2 s
  (`user_dukegrow`) until it pops. The freezer holds an actor for ~8 s
  (`user_dukefreeze`); when that wears off a monster stays as an ice
  statue until a further hit shatters it.
- **Crouch** is hold move-down on the ground (`+movedown` / C): height and
  view drop to 40%, walk halves, and jump is blocked until you stand. A
  low ceiling keeps you down. Pipebombs still crouch-roll when thrown
  while holding move-down.
- **Inventory use** is `'` / `+invuse`: medkit (held), then a press for
  steroids, night vision, scuba (radsuit + air timer) / boots, or a
  holoduke decoy. A `game#.sav` `inven_icon` (2 steroids … 7 boots)
  uses that item first. Night vision uses Doom PLAYPAL 13 (green), the same
  tint as the radiation suit. Fully underwater uses a blue tint (HUD
  palette 14). The jetpack still starts on pickup. A holoduke decoy
  wanders (`A_Wander`) and monsters hunt it like a player.
- Movement is Doom's on the ground, with `JumpZ 10`,
  `Player.ForwardMove 1.25`, and `MaxStepHeight 48` on `DukePlayer`
  (steroids still double the walk). A 32-unit roof curb is a step, not a wall. Crouch halves the walk. In the air, releasing move keeps the
  last XY speed (Duke coast); holding move adds a quarter of wish
  thrust instead of replacing XY speed. A long
  fall (~2 s / 72 tics, Duke's `falling_counter >= 62`) takes half
  health on landing. Jetpack and water reset the count.

### Rendering and the front end

- A 3D floor only blocks the actor when its target sector is the one under the actor, or a line the actor is actually touching. A floor in the same blockmap cell no longer drops the E1L1 spawn ceiling onto the player's head. A Duke player ducks on their own when the gap overhead is shorter than standing height, so a low overhang stays walkable. The map start Z is the eye: the pawn's feet are written 36 units below it instead of being snapped to the sector floor.
- **Overlapping sectors** (rooms over rooms) become a 3D floor when
  the 2D boxes overlap, share an edge, or only share a corner.
  Matching stacked rooms, nested lofts (including a loft that shares
  a wall with the hall), overlapping strips, grazing overlaps,
  shared-edge rooms, and corner-touch rooms are covered (see sector
  effects). A `COUNTKILL`
  that does not fit at its map spot is nudged up to 48 units (not in
  vanilla-fixed demo mode).
- Night vision is Doom PLAYPAL 13 (green) plus the light-amp colormap.
  Underwater is a blue HUD tint (palette 14). Rendering otherwise uses
  Doom colormaps. Sector visibility darkens the light and writes UDMF
  `fogdensity` (visibility × 4).
- The title screen plays `RADLOGO.ANM` once (`RADL00`…), then loops
  `LOGO.ANM` (`TITL00`… at the file's fps) and plays `music 0`
  (`GRABBAG`). The menu is hidden during the splash; a key or click
  skips it. The browser shows the same title when a GRP is loaded
  without `?map=`. Intermission uses `INTERPIC` / `WIMAP*`
  stills and falls back to `GRABBAG` when Doom intermission lumps are
  missing. The finale plays the matching episode ANM once
  (`CINEOV2` / `CINEOV3` / `DUKETEAM` / `VOL4E1` as `E1AN`…`E4AN`), or
  `CREDIT` when there is no sequence. The title uses a MENUDEF
  `MainMenu` (New Game opens episode then skill, Load Game, Options, Quit).
  Pause can Save Game / Load Game. Room `.fsave` (console `save` / `load`,
  default slot `quick` under the config `saves/` dir) restores the player
  and sectors on the saved map. Options include Auto Aim (hitscans follow
  pitch when off), Mouse Aiming, invert mouse, always run, and show FPS.
  Room `.fdem` (console `record` / `stop` / `playdemo`, default slot
  `quick` under the config `demos/` dir) records and verifies a map.
  Original Duke `.DMO` headers parse (1.0 / 1.3D / 1.5). Ken LZW bodies
  (`kdfread`) unpack to `TicCmd`s — Atomic `DEMO1`–`3` included. An
  uncompressed 1.3D stream also works.   Original `game#.sav` headers parse
  (`BYTEVERSION`, slot name, volume / level / skill). Load Game / `load`
  accepts `saves/<slot>.sav` as well as `.fsave`: it names the map and
  starts it at the saved skill. The dump's `numwalls` / `numsectors` /
  `numsprites` / `numcyclers` / `numanimwalls` / `spriteqamount` / `numclouds` / `randomseed` / `lockclock` / `pskybits` / `pskyoff` / `animatecnt` / `animategoal` / `animatevel` / `animateptr` / `earthquaketime` / `from_bonus` / `secretlevel` / `respawn_monsters` / `respawn_items` / `respawn_inventory` / `god` / `auto_run` / `crosshair` / `monsters_off` / `last_level` / `eog` / `coop` / `marker` / `ffire` / `camsprite` / `connecthead` / `connectpoint2` / `numplayersprites` / `frags` / `global_random` / `parallaxyscale` prefix is read (plus the first `numwalls` `walltype`, `numsectors` `sectortype`, and `numsprites` `spritetype` records when those blobs are long enough, and the first `ps[]` pose); `randomseed` becomes the Room RNG seed, `lockclock` (30 Hz) becomes map time (35 Hz), `earthquaketime` starts a map-wide Room quake (`A_Quake(4, …, 0, 2048)`), `respawn_monsters` sets `World::respawn_monsters`, `respawn_items` / `respawn_inventory` set those World flags, `monsters_off` kills `COUNTKILL`s, `from_bonus` / `secretlevel` / `eog` / `global_random` set those World fields, `coop` / `ffire` / `marker` / `last_level` / `camsprite` / `pskybits` / `pskyoff` / `parallaxyscale` set those World fields (`camsprite >= 0` also starts demo cameras), `connecthead` / `connectpoint2` / `numplayersprites` / `frags` / `animatecnt` / `animategoal` / `animatevel` / `animateptr` / `numanimwalls` / `spriteqamount` / `numclouds` / `numcyclers` set those World fields (the first frag row also writes player 0 `frags`), `god` sets player god mode, `auto_run` sets `cl_run`, `crosshair` sets `cl_crosshair`, and the `ps[]` pose moves the pawn (`x/16`, `−y/16`, `−z/256`, converter yaw plus `look_ang`, plus `weapon_ang` look-aside, `horiz+horizoff−100` pitch, plus `ohoriz`/`ohorizoff` previous look, copies `footprintshade` as the footprint shade, copies `footprintcount` as remaining footprints, copies `footprintpal` as the footprint palette), sets health from `last_extra` plus `extra_extra8 / 256` overheal, marks the pawn dead when `dead_flag` is set, sets the attacker from `frag_ps`, selects inventory from `inven_icon` (1 kit … 7 boots), copies `invdisptime` (30 Hz → 35 Hz) as the HUD inventory-name timer, copies `toggle_key_flag` as inventory-key still held, shows the inventory name when `refresh_inventory` is set (HUD `MEDKIT` / `STEROIDS` / … while `invdisptime` runs), gives access cards from `got_access` (1 blue, 2 red, 4 yellow), respawns a holoduke decoy when `holoduke_on >= 0`, starts flight when `jetpack_on` is set, starts scuba (radsuit + air) when `scuba_on` is set, copies `airleft` (30 Hz → 35 Hz) onto remaining air, copies `on_ground` onto the pawn stance, copies `transporter_hold` (30 Hz → 35 Hz) as a move lock, gives `DukeBombsOut` when `hbomb_on` is set, copies `weapon_pos` onto HUD gun Y (`0` ready, `|n|·8` down), starts a jump when `jumping_counter` is mid-air (`1`..=`1279`), starts night vision when `heat_on` is set,, copies `falling_counter` (30 Hz → 35 Hz) onto mid-air fall tics, copies `hard_landing` as a view dip, copies `pycount` as the view-bob phase, looks through dump sprite `newowner` when that index is set, copies `hurt_delay` (30 Hz → 35 Hz) as a pain-flash cooldown, copies `hbomb_hold_delay` as pipebomb wind-up (`DukeThrowPower`), copies `rotscrnang` as a decaying view roll, copies `jumping_toggle` as jump still held, plays the idle knuckle-crack when `knuckle_incs` is set, copies `return_to_center` (30 Hz → 35 Hz) as a look-recenter, copies `tipincs` (30 Hz → 35 Hz) as a tip pose, jumps the ready gun to Fire when `show_empty_weapon` is set, keeps fire held when `rapid_fire_hold` is set, copies `pals` / `pals_time` as a HUD flash, writes dump `palookup` as `DukePalN` on the pawn, copies `walking_snd_toggle` as the footstep phase, holsters the ready gun when `holster_weapon` is set, sets `Clip` from the larger of `ammo_amount[PISTOL_WEAPON]` / `ammo_amount[CHAINGUN_WEAPON]`, sets `Shell` from `ammo_amount[SHOTGUN_WEAPON]`, sets `RocketAmmo` from `ammo_amount[RPG_WEAPON]`, sets `Cell` from `ammo_amount[SHRINKER_WEAPON]`, sets `DukePipebombAmmo` from `ammo_amount[HANDBOMB_WEAPON]`, sets `DukeDevastatorAmmo` from `ammo_amount[DEVISTATOR_WEAPON]`, sets `DukeFreezeAmmo` from `ammo_amount[FREEZE_WEAPON]`, sets `DukeTripbombAmmo` from `ammo_amount[TRIPBOMB_WEAPON]`, sets `DukeGrowAmmo` from `ammo_amount[GROW_WEAPON]`, gives `gotweapon` guns, gives `last_weapon` without changing the ready gun, gives `last_full_weapon` without changing the ready gun, equips `curr_weapon`, fires `wantweaponfire` when that slot is `>= 0`, starts a `/` quick kick when `quick_kick` is set, equips the kick and jumps to Fire when `knee_incs` is set, uses the special in front when `access_incs` is set, uses dump wall `access_wallnum` when that index is set, uses dump sprite `access_spritenum` when that index is set, copies `fta` / `ftq` (30 Hz → 35 Hz) as the on-screen quote, jumps the ready gun to Fire when `kickback_pic` is set, sets armor from `shield_amount`, sets `DukeFirstAid` from `firstaid_amount`, sets `DukeSteroids` from `steroids_amount`, sets `DukeJetpack` from `jetpack_amount`, sets `DukeScubaGear` from `scuba_amount`, sets `DukeHoloduke` from `holoduke_amount`, sets `DukeBoots` from `boot_amount`, sets `DukeNightVision` from `heat_amount`, applies dump `sector[]` floor / ceiling Z in map order, writes dump `sector[]` floor / ceiling tiles (`BT####`) onto those sectors, writes dump `sector[]` light from `floorshade` / `visibility`, writes dump `sector[]` fog density (`visibility × 4`), moves living `COUNTKILL`s onto dump `sprite[]` XY (extra > 0, think order), writes dump `sprite[]` extra onto actors at those XY, writes dump `sprite[]` `cstat` (blocking / translucent / invisible) onto those actors, writes dump `sprite[]` `xrepeat` / `yrepeat` / `clipdist` as scale and radius, writes dump `sprite[]` `pal` as `DukePalN`, writes dump `sprite[]` `ang` as converter yaw, writes dump `sprite[]` Z (`−z/256`), and writes dump `wall[]` extra / blocking / mid tiles (`overpicnum`, else `picnum`, else `-`) onto linedefs at those XY. `gotpic` tile-seen bits (any set bit reveals the automap) and `playerswhenstarted` (coop seats) are applied after that prefix. Anything further is not restored (use
  Room `.fsave`). DukeMatch
  (`GameOptions.deathmatch`) is Room deathmatch plus no `COUNTKILL` and
  spawn on a type-11 start.
- Other Build games are refused at convert time: a GRP with `BLOOD.INI` /
  `SW.CON` / `REDNECK.CON` / `NAM.CON` returns
  `BuildError::Unsupported`. Only Duke Nukem 3D GRPs convert.
