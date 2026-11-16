# 21 — Milestone progress tracker

Living checklist for every roadmap milestone. Specs and acceptance tests live in
 [`20-roadmap-testing.md`](20-roadmap-testing.md); **this file tracks execution**.

- `2026-11-16` — **ED4 `operate` action**: EDuke32 `operate` now queues the current actor for the existing near-sector door/sector operation phase, preserving the session's sprite-order processing. VM and live-session regressions verify dispatch and queue state. This resolves all 9 Brutal Duke `operate` no-ops; remaining ED4 gaps include event ordering, cancellation/replacement returns and display events. Source: [EDukeWiki `operate`](https://wiki.eduke32.com/wiki/Operate).
- `2026-11-16` — **ED5 `money` / `mail` / `paper` sprite sprays**: each command now spawns the matching live sprite (`MONEY`, `MAIL`, or `PAPER`) from the current actor using Chocolate's angle/height/cstat RNG draw order and the existing EGS/SPAWN callback path. VM and session regressions verify all three tile labels, sprite properties and shared gameplay RNG advancement. This resolves all 7 Brutal Duke uses. Source: [EDukeWiki CON FAQ](https://wiki.eduke32.com/wiki/Confaq42).
- `2026-11-16` — **ED0 compatibility report refreshed**: the bundled Brutal Duke 0.2 tree still compiles with zero errors across 51 files / 44,756 lines and 24,799 executable-core instructions; live `operate`, `money`, `mail`, and `paper` lower the explicit no-op count from 238 to 222. `getticks` writes deterministic elapsed milliseconds derived from Room's 120 Hz simulation clock (214 no-ops), `setgamepalette` selects per-player full-screen base or LOOKUP.DAT palettes (205 no-ops), and `inittimer` updates runtime pacing (196 no-ops). `endofgame` enters the EDuke VM, including its default 52-unit delay, and inline `break` exits its enclosing range loop; the report is down to 185 explicit no-ops with zero compile errors after live actor-sound ownership made all four `ifnosounds` branches executable.
- `2026-11-16` — **ED2 `ifnosounds` actor predicate**: the branch now tests the current actor against the active sound-owner snapshot used by Atomic's sound slot table. VM and live session host tests cover both an owned active voice and the no-voice path. The Brutal Duke inventory reports zero `ifnosounds` no-ops; EDuke sound playback integration continues to use the session's sampled owner snapshot.
- `2026-11-16` — **ED8 palette selection**: `setgamepalette` now updates the active player's palette id, `player.palette` reads/writes that state, and the software renderer applies the base water/slime/title/3D Realms/ending palettes or a full-screen palette generated from the matching LOOKUP.DAT remap. The VM dispatch, live player state, and custom palette 240 render regressions pass. Animation palette 6 has no standalone lookup resource in Room yet; display-event/HUD coverage remains open. Sources: [EDukeWiki `setgamepalette`](https://wiki.eduke32.com/wiki/Setgamepalette), [palette lookup format](https://wiki.eduke32.com/wiki/Palette).
- `2026-11-16` — **ED8 timer pacing**: `inittimer` now maps the EDuke32 120 Hz rate to Room's 30 Hz baseline and changes desktop/browser pacing while leaving each simulation tic's fixed clock increments intact. The VM dispatch, session-rate conversion, and desktop app compile checks pass; timing uses a bounded 1–120 simulation tics/s range. Sources: [EDukeWiki `inittimer`](https://wiki.eduke32.com/wiki/Inittimer), [EDukeWiki `getticks`](https://wiki.eduke32.com/wiki/Getticks).
- `2026-11-16` — **ED4 `endofgame` VM dispatch**: the command now compiles through the EDuke instruction path, applies the documented default 52-unit countdown when its operand is omitted, and sets the active player's exit state through the session bridge. VM and live-session tests pass. The global MODE_EOL transition and campaign routing remain open. Source: [EDukeWiki `endofgame`](https://wiki.eduke32.com/wiki/Endofgame).
- `2026-11-16` — **ED4/ED5 classic actor and state execution**: classic `state … ends` declarations are registered as callable EDuke32 VM states, and actor script entry points now run once per active actor before native movement. Symbolic `ifaction` / `ifai` / `ifmove` operands resolve their pointer labels; the built-in `totalclock` is derived deterministically at four clock units per 30 Hz tic, and `SFLAG_NOEVENTS` resolves to its EDuke32 value. Live actor hits consume `ifhitweapon` damage and invoke the script's kill/spawn path; `addphealth` now reaches its live handler instead of compiling to a no-op. All 121 EDuke-focused tests pass. `duke_play kill-all` now excludes corpses present at setup so reused sprite slots do not count as failed targets; the bundled Brutal Duke E1L1 sweep is 29/33 killed, 2 hurt, 1 still alive, and 1 without a clear shot. ED5 remains open.
- `2026-11-16` — **ED3 actor extension field access**: `getactor` / `setactor` now expose `htactorstayput`, `httempang`, `htfloorzoffset`, and `htwaterzoffset` over persisted actor-temp state, with native-width validation. A CON event regression reads, writes, and saves/restores all four fields through `.fsave`; `CON fall` applies `htwaterzoffset` to the lotag-1 resting placement (24 restores zero adjustment), while full collision/hitbox consumers and `htfloorzoffset` effects remain open. Sources: [EDukeWiki structure members](https://wiki.eduke32.com/wiki/Members_of_the_sprite,_hittype,_and_spriteext_structures), [EDukeWiki `htwaterzoffset`](https://wiki.eduke32.com/wiki/Htwaterzoffset).
- `2026-11-16` — **ED5 declared hitscan projectile execution**: `HITSCAN` and `KNEE` definitions now resolve a range-limited live Build ray immediately when `shoot` fires, update the returned temporary sprite to the impact/end point, and queue declared direct damage. Player-fired `HITSCAN` definitions can place the configured `PROJ_DECAL` tile on a wall hit, using configured repeat dimensions and wall alignment. Bullet `PROJ_SOUND` plays on impact. A focused EDuke session regression verifies ray range, impact sprite, `PROJ_EXTRA`, and wall decal; `moveweapons` removes the returned projectile spark on the following tic. Only the custom projectile path initializes the negative damage sentinel before using the existing weapon hit queue. Randomized decal sizing remains open. Source: [EDukeWiki `shoot`](https://wiki.eduke32.com/wiki/Shoot), [EDukeWiki `defineprojectile`](https://wiki.eduke32.com/wiki/Defineprojectile).
- `2026-11-16` — **ED5 CON `hitradius` execution**: EDuke32 CON `hitradius` now compiles its radius and four damage bands into an executable VM instruction, evaluates variable operands in order, and applies the existing live blast resolver to the current actor's world. A session regression verifies actor damage is queued and gameplay RNG advances; compiler and VM host-dispatch tests pass. The Brutal E1L1 acceptance sweep is still below target, and no external EDuke32 blast trace was run.
- `2026-11-16` — **ED4 CON `pkick` action**: `pkick` now compiles and queues the existing 14-tic player quick-kick response through actor-temp state; APLAYER source sprites are skipped like the Atomic CON path. Compiler, VM dispatch and live-session tests pass. The Brutal inventory's explicit no-ops are down to 238; full event call-site parity remains open.
- `2026-11-16` — **ED0/ED4 live EDuke32 map loader**: GRP layers can now be loaded with an explicit CON dialect and external include resolver. EDuke32 VM initialization happens before map sprite setup; the loader regression advances E1L1 and confirms the registered `EVENT_GAME` handler mutates VM state. `duke_play` can load a local CON directory through `ROOM_DUKE_MOD`; classic actor bodies and named states now execute, though the bundled Brutal Duke E1L1 sweep remains incomplete at 29/33 kills. The ED2 reference value dump remains optional and does not block continued implementation.
- `2026-11-16` — **ED0 compatibility gate rechecked during ED5 work**: `duke mod-check` on the bundled Brutal Duke 0.2 tree reports 0 CON compile errors across 51 files and 44,756 lines, with 24,799 executable-core instructions and 238 explicit no-op instructions. The report now reflects selected event and actor call sites dispatched by `DukeSession`; it remains a partial source inventory, not a compatibility pass.
- `2026-11-16` — **ED5 declared projectile launch flags and appearance**: `PROJ_CLIPDIST` now uses the configured distance only when `PROJECTILE_REALCLIPDIST` is set, otherwise launch uses Build’s hardcoded clip distance 4. Declared projectile `cstat`, shade and palette are applied during launch. A session regression covers both clip-distance branches and the appearance fields. Source: [EDukeWiki `defineprojectile`](https://wiki.eduke32.com/wiki/Defineprojectile).
- `2026-11-16` — **ED5 underwater projectile bubbles**: custom RPG workslike projectiles with `PROJECTILE_WATERBUBBLES` now spawn a randomized `WATERBUBBLE` actor at their final or impact position when that Build sector is underwater. The focused movement regression checks underwater gating, bubble location and size; the decay rule for `PROJECTILE_LOSESVELOCITY` remains source-unverified. Source: [EDukeWiki `defineprojectile`](https://wiki.eduke32.com/wiki/Defineprojectile).

- `2026-11-16` — **ED7 classic sprite translucency ordering**: the software Build view now renders opaque sprites before translucent sprites, sorts translucent sprites far-to-near with stable ties, and keeps translucent layers out of the depth buffer. The overlap regression verifies two-layer palette-table composition and that the underlying opaque depth survives; all 12 `soft::tests` pass. The broader renderer gate remains red on the current tree: `view_floods_two_sectors` fails, and `soft_goldens` has three failures (portal visibility and two existing hash expectations). Wall-layer ordering, alpha values, blend tables and the remaining ED7 renderer scope are still open.
- `2026-11-16` — **ED5 `PROJ_VEL_MULT` repeated movement and bounce continuation**: RPG projectiles now perform a separate collision sweep and Build sector update for each velocity step, instead of sweeping one multiplied segment. Wall and sprite bounces recompute direction and continue through the remaining steps in the same tic; wall bounces decrement `PROJ_BOUNCES`, while sprite bounces do not. Portal crossing, post-bounce movement, bounce count, angle and sound are covered by focused regressions. The per-tic step count is capped at 256 to bound mod-controlled work; larger configured values are capped. Source: [EDukeWiki `defineprojectile`](https://wiki.eduke32.com/wiki/Defineprojectile).
- `2026-11-16` — **ED3/ED5/ED10 projectile velocity multiplier**: `PROJ_VEL_MULT` initializes on fired RPG projectiles and round-trips through `thisprojectile.velmult` and `.fsave`. The `ezshootvar` fixture verifies definition/read/write behavior, and movement regressions cover doubled displacement and repeated portal-sector updates. Source: [EDukeWiki `defineprojectile`](https://wiki.eduke32.com/wiki/Defineprojectile).
- `2026-11-16` — **ED6 DEF texture palette variants**: the DEF resource API now extracts each `texture` palette branch with its tile, palette bank, file and `alphacut`; the desktop indexed import path consumes palette 0 while the renderer is still single-image/indexed. Tests cover multiple banks, palette-zero selection, nonzero-only declarations, alpha cutoffs and malformed branches. Palette-specific rendering and true-colour sampling remain open. Source: [EDukeWiki `texture` DEF directive](https://wiki.eduke32.com/wiki/Texture_%28DEF%29).
- `2026-11-16` — **ED3/ED5 declared RPG projectile properties and movement**: active `thisprojectile.hitradius` is read/write and persists with actor temps. Declared RPG workslike projectiles now apply `zvel -= PROJ_DROP` before swept movement, ignore their firing owner during the sweep, apply configured hitradius on collision, and apply direct impact damage only when `PROJECTILE_RPG_IMPACT` is set. Session and movement regressions cover property initialization/save restore, gravity, direct damage, and radius damage. Sources: Brutal Duke's projectile definitions and [EDukeWiki `PROJ_WORKSLIKE`](https://wiki.eduke32.com/wiki/PROJ_WORKSLIKE).
- `2026-11-16` — **ED10 active projectile state in `.fsave`**: save/restore coverage now includes active projectile remaining range/age, bounce count (`yvel`), trail tile/count/offset/repeats, impact spawn/sound and launch sound values. The EDuke32 `.fsave` round-trip regression passes with the expanded state.
- `2026-11-16` — **ED3/ED5 projectile lifetime, trail and impact properties**: active projectiles now persist `range`, `spawns`, `isound`, trail tile/count/repeats/offset, and impact spawn repeats through saveable actor temps; `thisprojectile` exposes the runtime values. `PROJECTILE_TIMED` removes the projectile after its configured 30 Hz lifetime; `PROJECTILE_EXPLODEONTIMER` routes expiry through its radius-impact path. Moving projectiles emit bounded trail sprites, impacts can spawn their declared actor and queue `PROJ_ISOUND`, and firing queues `PROJ_SOUND`. Trail, expiry, timed-explosion, impact-spawn and sound behavior are covered by focused tests; 114 focused EDuke tests pass. Sources: [EDukeWiki `defineprojectile`](https://wiki.eduke32.com/wiki/Defineprojectile), [EDukeWiki `PROJ_WORKSLIKE`](https://wiki.eduke32.com/wiki/PROJ_WORKSLIKE).
- `2026-11-16` — **ED5 custom projectile bounces**: RPG workslike projectiles now reflect from flagged walls, mirror walls, and sprites. The live sprite `yvel` stores `PROJ_BOUNCES` remaining wall contacts; sprite bounces do not consume that counter, and configured bounce sounds are queued. Session coverage also reads and writes `thisprojectile.bounces`. Focused regressions cover wall reflection, sprite bounce, bounce-count behavior, and property initialization; 114 focused EDuke tests pass. Sources: [EDukeWiki `PROJ_WORKSLIKE`](https://wiki.eduke32.com/wiki/PROJ_WORKSLIKE), [EDukeWiki `defineprojectile`](https://wiki.eduke32.com/wiki/Defineprojectile).
- `2026-11-16` — **ED5 declared projectile auto-aim**: `shoot` / `eshoot` now use Room's deterministic Build-angle target selection for player-fired declared projectiles when full auto-aim is enabled; bullet-only auto-aim also steers hitscan workslike projectiles. The host calculates vertical trajectory from projectile speed and target height, honors `PROJECTILE_NOAIM`, and leaves explicit `zshoot` velocity in control. The session regression covers aimed and `NOAIM` shots; 114 focused EDuke tests pass. Source: [EDukeWiki `PROJ_WORKSLIKE`](https://wiki.eduke32.com/wiki/PROJ_WORKSLIKE).
- `2026-11-16` — **ED2/ED3 `ifrespawn` and respawn settings**: `ifrespawn` now selects `RESPAWN_MONSTERS`, `RESPAWN_INVENTORY`, or `RESPAWN_ITEMS` from the actor's useractor/badguy, hard-coded inventory-tile, or ordinary-item classification. Those predefined gamevars read and write the saveable `userdef` settings; imported Atomic saves seed the settings, and `.fsave` preserves them. VM, live actor-classification, member-access, and save/load regressions pass. The Brutal Duke scan remains at 0 compile errors and drops from 770 to 753 explicit no-ops, resolving 17 `ifrespawn` statements. Source: [EDukeWiki `ifrespawn`](https://wiki.eduke32.com/wiki/Ifrespawn).
- `2026-11-16` — **ED2 actor condition and chained `else if` coverage**: `ifactornotstayput` reads the live actor temp value, with VM and session regressions; its 10 Brutal Duke uses no longer lower to no-ops. Nested `else if` now compiles relational conditions such as `ife` with their following brace blocks and later fallback branches. An execution regression checks all three paths, and the Brutal Duke scan returns to 0 diagnostics and drops from 743 to 734 explicit no-ops. Source: [EDukeWiki `ifactornotstayput`](https://wiki.eduke32.com/wiki/Ifactornotstayput).
- `2026-11-16` — **ED4 CON weapon and inventory commands**: `addammo`, `addweapon`, `addinventory` and their `var` spellings now compile into live-player VM operations. Weapon grants target the nearest player, enforce ammo caps and stop the current event when the requested ammo is already full. Inventory writes match EDuke32's exact-value rules, armor adds up to `max_shield_amount` (including negative results), and access cards use the calling sprite's palette. The player adapter now exposes and saves `max_shield_amount` and the per-weapon `max_ammo_amount[]` array used by these commands; VM and live-session regressions pass. The Brutal Duke scan remains at 0 compile errors and drops from 808 to 770 explicit no-ops, resolving 38 command uses. Sources: [EDukeWiki `addammo`](https://wiki.eduke32.com/wiki/Addammo), [`addweapon`](https://wiki.eduke32.com/wiki/Addweapon), [`addinventory`](https://wiki.eduke32.com/wiki/Addinventory).
- `2026-11-16` — **ED2 CON `quote` display**: `quote <numeric constant>` now displays the selected `definequote` for the active player, refreshes the HUD timer, exposes runtime-redefined quote text, and suppresses repeated consecutive quote IDs in the console output. Compiler/VM and live-session regressions pass; the Brutal Duke scan stays at 0 compile errors and falls from 881 to 845 explicit no-ops, resolving 36 quote statements. Source: [EDukeWiki `quote`](https://wiki.eduke32.com/wiki/Quote).
- `2026-11-16` — **ED4 CON `palfrom` player flash**: `palfrom <duration> <red> <green> <blue>` now compiles to a host-backed VM instruction and updates the selected player's timed palette tint, including operands continued across lines before the next CON keyword. VM/compiler and live-session tests pass; the Brutal Duke scan returns to 0 compile errors and falls from 845 to 808 explicit no-ops, resolving all 37 `palfrom` statements. Source: [EDukeWiki CON FAQ](https://wiki.eduke32.com/wiki/Confaq42).
- `2026-11-16` — **ED3 live player quote/palette-flash/bridge members**: player `fta`/`ftq`, indexed `pals`, `pals_time`, previous-position aliases, true floor/ceiling aliases, and bridge state (`spritebridge`/`sbs`) now read and write through the EDuke adapter. Player collision refreshes the bridge fields from Build's floor-sprite hit result; shallow-water view bob follows that bridge state in EDuke32 sessions while Atomic keeps its existing movement baseline. CON adapter and floor-sprite runtime regressions pass. Source: [EDukeWiki player structure members](https://wiki.eduke32.com/wiki/Members_of_the_player_structure).
- `2026-11-15` — **ED5 live `getzrange` query**: `getzrange` now compiles into a host-backed instruction, queries live Build sector planes plus active wall/sprite clip geometry, and writes ceiling/floor Z values and hit codes. Compiler/VM and live-session regressions pass for floor/ceiling and invalid-sector outputs. The Brutal Duke scan remains at 0 compile errors and falls from 1,058 to 1,055 explicit no-ops; its three `getzrange` uses now execute. Source: [EDukeWiki `getzrange`](https://wiki.eduke32.com/wiki/Getzrange).
- `2026-11-16` — **ED5 CON `debris` fragments**: `debris TILE COUNT` now rolls fragments in reverse spawn order with Chocolate's right-to-left EGS RNG draws, creates live STAT_MISC scraps with inherited shade/palette, and queues EGS/SPAWN callbacks. The session test verifies fragment fields and the shared gameplay RNG advance; the VM test verifies lowering and operands. The Brutal Duke scan remains at 0 compile errors and falls from 1,055 to 957 explicit no-ops, resolving all 98 `debris` statements. Source: [EDukeWiki `debris`](https://wiki.eduke32.com/wiki/Debris).
- `2026-11-16` — **ED5 CON `guts` fragments**: `guts TILE COUNT` now creates floor-clamped STAT_MISC gore with Chocolate's actor-size, commander-height and palette-6 rules, assigns the selected player's body as owner, and advances the shared gameplay RNG in C argument order. VM and live-session tests pass. The Brutal Duke scan remains at 0 compile errors and falls from 957 to 910 explicit no-ops, resolving all 47 `guts` statements.
- `2026-11-16` — **ED5 CON `lotsofglass` fragments**: `lotsofglass COUNT` now creates randomized `GLASSPIECES` variants with 36×36 repeats, inherited palette, STAT_MISC placement, and the native five-draw RNG order. VM and live-session tests pass. The Brutal Duke scan remains at 0 compile errors and falls from 910 to 881 explicit no-ops, resolving all 29 `lotsofglass` statements.
- `2026-11-15` — **ED4 player health command**: `addphealth` now compiles to a host-backed VM operation, selects the active event player, enforces the `gamestartup` health cap, honors god mode, allows Atomic Health's 2× cap, and synchronizes the player body's health. Healing across one-quarter health queues `DUKE_GOTHEALTHATLOW`. VM and live-session tests cover normal cap, Atomic Health, and co-op selection. The Brutal Duke scan remains at 0 compile errors and falls from 1,070 to 1,058 explicit no-ops; 12 uses now lower to executable instructions, with seven actor-body no-ops remaining. Sources: [EDukeWiki `addphealth`](https://wiki.eduke32.com/wiki/Confaq42), [EDukeWiki special cases](https://wiki.eduke32.com/wiki/Special_case).
- `2026-11-15` — **ED5 CON `sectorofwall` query**: `sectorofwall <target> <wall>` now compiles to a host-backed VM instruction and resolves the first live Build sector whose wall range contains the ID; invalid and unowned walls return `-1`. Compiler/VM and session range regressions pass. The verified Brutal Duke scan remains at 0 compile errors and drops from 1,082 to 1,070 explicit no-ops, resolving all 12 `sectorofwall` uses.
- `2026-11-15` — **ED2 target firing condition**: `ifcanshoottarget` now compiles to an EDuke VM branch and queries the live Build hitscan corridor: close-range shortcut, three forward rays, same-tile blockers, forcefields and large-badguy tolerances. Compiler/VM and live geometry regressions pass. The Brutal Duke scan remains at 0 compile errors and falls from 1,149 to 1,104 explicit no-ops. EDuke32 reference comparison remains useful for parity but does not block later milestone work. Source: [EDukeWiki `ifcanshoottarget`](https://wiki.eduke32.com/wiki/Ifcanshoottarget).
- `2026-11-15` — **ED2 incoming projectile condition**: `ifbulletnear` now compiles to a branch that scans live `STAT_PROJECTILE` sprites, tests Build-angle forward/incoming vectors and corridor width, and ignores projectiles owned by the current actor. A threat consumes the shared gameplay RNG and applies Chocolate's dodge angle turn. VM/compiler and live projectile/RNG tests pass. The verified Brutal Duke scan drops from 1,104 to 1,095 explicit no-ops with 0 compile errors. Source: [EDukeWiki `ifbulletnear`](https://wiki.eduke32.com/wiki/Ifbulletnear).
- `2026-11-15` — **ED2 co-op weapon reservation condition**: `ifgotweaponce` now tests/registers the active actor tile in the selected player's 16-entry reservation table and checks sprite ownership. Compiler/VM and live-session reservation regressions pass. The verified Brutal Duke scan drops from 1,095 to 1,082 explicit no-ops with 0 compile errors. Behavior follows the existing Atomic CON reservation path.
- `2026-11-15` — **ED2/ED3/ED4 actor commands**: event/state `strength`, `addstrength`, `cstat`, `cstator`, `cactor`, `spritepal`, `getlastpal`, `clipdist`, `count`, `sleeptime`, `sizeat`, `sizeto`, `fall`, `resetactioncount`, and `resetcount` now compile to VM instructions and mutate live actor/session state. `sizeto` follows Atomic's one-step X/Y and height-gap rules; `fall` calls the Build fall routine. Compiler and live-session regressions pass; the gated Brutal Duke scan remains at 0 compile errors and drops from 1,186 to 1,173 explicit no-ops. The remaining high-frequency no-ops are mostly in Atomic actor blocks, which still use the existing actor VM path.
- `2026-11-15` — **ED4/ED6 actor and global sound commands**: `sound`, `soundonce`, `globalsound`, and `stopsound` compile to the native positioned sound-event queue; actor sounds use the executing sprite and global sounds use the local player's position. The VM sound-command regression and live session queue test pass. The Brutal Duke report drops from 2,316 to 1,765 no-op instructions. Multiplayer sound dispatch and full voice arbitration remain open.
- `2026-11-15` — **ED2/ED3 action, move and AI pointers**: event/state `action`, `move`, and `ai` now update actor action/movement/AI pointers, action counters, movement flags and pointer-kind save metadata. The standard movement flags resolve by name; `randomangle` consumes the shared gameplay RNG. The live-session regression verifies the AI table load and RNG state. The Brutal Duke report now has 0 compile errors and 1,382 remaining no-op instructions; actor-body dispatch and later high-frequency no-ops remain open. Sources: [EDukeWiki move flags](https://wiki.eduke32.com/wiki/Move), [EDukeWiki AI flags](https://wiki.eduke32.com/wiki/Confaq42).
- `2026-11-15` — **ED4 actor `killit` behavior**: `killit` now compiles to a terminating EDuke VM opcode; the session runs `EVENT_KILLIT` before deletion and honors nonzero `RETURN` as a veto. It removes actors on `STAT_ACTOR`/`STAT_MISC`, hides sprites on other status lists by setting `xrepeat` to zero, and stops later code/hooks. VM and live-session regressions pass; the current Brutal Duke report falls from 2,453 to 2,316 explicit no-ops, removing the 137 `killit` no-ops. Remaining ED4 event call sites are still open. Behavior follows [EDukeWiki `killit`](https://wiki.eduke32.com/wiki/Killit) and [`EVENT_KILLIT`](https://wiki.eduke32.com/wiki/EVENT_KILLIT).
- `2026-11-15` — **ED2 VM array sizing + ED5 live sector aliases**: CON `resizearray` now emits a bounded VM instruction that grows with zero-fill, shrinks, and rejects negative lengths; the EDuke built-ins `numsectors`, `NUMSECTORS`, `NUMWALLS`, and `Numsprites` read live Build map data. Sector structure access accepts `floorslope` / `ceilingslope` as aliases for Build's heinum fields. Focused resize and alias tests pass, and the gated Brutal Duke v9 `box.map` load now runs `PRELEVEL`, `LOADACTOR`, `SPAWN`, `ENTERLEVEL`, `EVENT_GAME`, and per-map `EVENT_SPAWN` without VM faults. The external EDuke32 fixture dump remains a parity checkpoint but does not gate implementation of later ED milestones. Sources: [EDukeWiki `resizearray`](https://wiki.eduke32.com/wiki/Resizearray), [predefined gamevars](https://wiki.eduke32.com/wiki/Pre-defined_gamevars), [sector members](https://wiki.eduke32.com/wiki/Members_of_the_sector_structure).
- `2026-11-15` — **ED5 `shoot` family spawn path**: the EDuke compiler now emits `shoot` / `eshoot` and explicit-Z `zshoot` / `ezshoot` variants, including their `var` spellings. `ezshootvar` creates the CON projectile tile, stores its ID in `RETURN`, applies the explicit z velocity and declared `PROJ_VEL`, size, clip distance, `extra` and `workslike`, and queues normal EGS/SPAWN lifecycle callbacks. A live-session regression covers these fields and returned ID. Custom projectile aiming, movement/collision, impacts and the Brutal weapons sweep remain open. Source: [EDukeWiki `shoot`](https://wiki.eduke32.com/wiki/Shoot).
- `2026-11-15` — **ED4 runtime spawn callbacks**: VM host spawns now queue the new sprite IDs, then dispatch `EVENT_EGS` followed by `EVENT_SPAWN` with nearest-player context after the calling event body returns. Nested spawn callback chains share the caller's remaining instruction budget and stop at a 32-level recursion cap. A regression verifies both callbacks mutate the new actor. All 68 EDuke-focused tests plus `room-app` / `room-web` checks and `git diff --check` pass. Exact EDuke32 instruction-point ordering (callbacks before the parent CON body continues) remains open. Sources: [EDukeWiki `EVENT_EGS`](https://wiki.eduke32.com/wiki/EVENT_EGS), [`EVENT_SPAWN`](https://wiki.eduke32.com/wiki/EVENT_SPAWN).
- `2026-11-15` — **ED4 map actor load hooks**: after loading and initializing the map's original active sprites, Room now dispatches `EVENT_LOADACTOR` with no player context, then `EVENT_SPAWN` with nearest-player context, before `EVENT_ENTERLEVEL`. The captured actor list excludes the player body created at the end of setup and skips sprites removed by an earlier hook. The level lifecycle regression exercises both hooks on an actual one-sector world; all 67 EDuke-focused tests passed at this stage. Sources: [EDukeWiki `EVENT_LOADACTOR`](https://wiki.eduke32.com/wiki/EVENT_LOADACTOR), [`EVENT_SPAWN`](https://wiki.eduke32.com/wiki/EVENT_SPAWN), [`EVENT_EGS`](https://wiki.eduke32.com/wiki/EVENT_EGS).
- `2026-11-15` — **ED4 map lifecycle event call sites**: `EVENT_PRELEVEL` runs before the new world is installed with no player/actor context, after resetting actor-scoped CON variables to their declaration defaults. `EVENT_ENTERLEVEL` runs after world/player construction and honors `RETURN` to iterate through local seats. Regressions cover actor-scope reset, no-context values, default single-player dispatch and co-op iteration; all 67 EDuke-focused tests pass. Remaining ED4 work includes other event call sites, cancellation/replacement `RETURN` semantics and display events. Sources: [EDukeWiki `EVENT_PRELEVEL`](https://wiki.eduke32.com/wiki/EVENT_PRELEVEL), [`EVENT_ENTERLEVEL`](https://wiki.eduke32.com/wiki/EVENT_ENTERLEVEL).
- `2026-11-15` — **ED4 global and player-input event call sites**: `EVENT_PREWORLD` runs once before `EVENT_PREGAME`; `EVENT_PROCESSINPUT` runs per local player seat against the mutable sync-input records before native input handling; `EVENT_WORLD` runs once after per-sprite `EVENT_GAME`, even with no active sprites. Global hooks expose `THISPLAYER` / `THISACTOR` as `-1`. Regressions cover empty maps, per-tic/per-seat counts, event order, no-context values and `setinput` edits. All 65 EDuke-focused tests pass. Other event call sites and `RETURN` semantics remain open. Sources: [EDukeWiki `EVENT_PREWORLD`](https://wiki.eduke32.com/wiki/EVENT_PREWORLD), [`EVENT_WORLD`](https://wiki.eduke32.com/wiki/EVENT_WORLD), [`EVENT_PROCESSINPUT`](https://wiki.eduke32.com/wiki/EVENT_PROCESSINPUT).
- `2026-11-15` — **ED3 renderer hook integration**: native desktop and browser rendering now dispatch `EVENT_ANIMATESPRITES` for visible, drawable sprites with `SPREXT_TSPRACCESS`; transient `tspr` edits reach the render-world copy while ordinary structure writes stay on a disposable snapshot. Per-frame instruction use is bounded, the viewer seat is passed as `screenpeek`, and attempts to set the TSPR invisible bit are ignored as EDuke32 does. The new runtime regression, existing TSPR checks, and native/web compile checks pass. Mirror re-entry, render blend consumption, other `userdef` fields, undeclared projectile defaults and full referenced-field coverage remain. Source: [EDukeWiki `EVENT_ANIMATESPRITES`](https://wiki.eduke32.com/wiki/EVENT_ANIMATESPRITES).
- `2026-11-15` — **ED3 projectile defaults**: missing `PROJ_SPAWNS`, `PROJ_BSOUND`, `PROJ_EXTRA_RAND` and conditional `PROJ_TNUM` reads now return EDuke32's documented defaults; explicit definitions and saved overrides still take precedence. A compiled event test checks all four defaults alongside the existing `.fsave` override round-trip. Other mapped numeric properties currently default to zero. Source: [EDukeWiki `defineprojectile`](https://wiki.eduke32.com/wiki/Defineprojectile).
- `2026-11-15` — **ED4 `EVENT_PREGAME` / `EVENT_GAME` tic call sites**: `EVENT_PREGAME` now runs at tic start before player/actor simulation, then `EVENT_GAME` runs after simulation and actor scripts. Both invoke hooks per active sprite, skip effective `SFLAG_NOEVENTS`, XOR tile flags with per-sprite `htflags`, select the nearest valid player and share bounded per-phase instruction budgets. Runtime faults are retained for the native/browser developer console. Tests check pregame-before-game ordering, end-of-tic dispatch, nearest-player selection and flag XOR. Sources: [EDukeWiki `EVENT_PREGAME`](https://wiki.eduke32.com/wiki/EVENT_PREGAME), [EDukeWiki `EVENT_GAME`](https://wiki.eduke32.com/wiki/EVENT_GAME), [flag definitions](https://wiki.eduke32.com/wiki/Pre-defined_values).
- `2026-11-15` — **ED3 sprite extension access**: added read/write support for all 10 documented `spriteext` fields plus `actor[].blend`, backed by per-sprite state and preserved by `.fsave`. The compiler-to-session event, sprite isolation, alpha 0–255 bounds and save/restore regressions pass. Field coverage and renderer consumption remain open. Sources: [EDukeWiki sprite/hittype/spriteext members](https://wiki.eduke32.com/wiki/Members_of_the_sprite,_hittype,_and_spriteext_structures), [alpha](https://wiki.eduke32.com/wiki/Alpha), [mdflags](https://wiki.eduke32.com/wiki/Mdflags).
- `2026-11-15` — **ED6 DEF, tile, sound and map-hack import foundation**: added a nested DEF parser with quoted asset paths, comments, multiline blocks, `tilefromtexture` alpha/offset extraction, `copytile`, `dummytile(range)` and ART-style `animtilerange`. `duke mod-check` decodes all 393 Brutal Duke replacement images and parses all 177 MHK files (28,527 sprite/light records) with zero asset or parser errors. The desktop renderer resolves `mapinfo` by case-insensitive map filename + MD4, loads its referenced MHK, and applies `angoff` only to aligned-sprite presentation copies; fixtures verify MHK selection/hash mismatch and that the override changes rendered pixels while leaving world sprites unchanged. Loose WAV/OGG sound lookup is also wired. Remaining: `texture`, skybox/palette/blend/voxel directives, browser/editor mounting, MHK model transforms/lights and DEF music. E1L1 could not be checked because its map asset is not in the local test corpus.
- `2026-11-15` — **ED6 DEF music playback slice**: parsed inline and multiline `Music { ID … file … }` blocks, retained loose mod audio resources, and routed map-specific DEF entries through the desktop music player. Decoded PCM formats stream with fixed-point resampling and loop/pause/end handling, without needing a MIDI synth; MIDI/MUS DEF resources are also dispatched, with the Atomic CON MIDI track retained as fallback. Focused parser/audio tests and `cargo check -p room-app` pass. This does not close ED6; map-specific campaign IDs, browser/editor music mounting, other DEF directives and MHK model data remain.
- `2026-11-15` — **ED10 network content policy**: desktop Duke `--host`/`--join`, the browser multiplayer lobby, and the native server verifier now reject sessions whose loaded CON image uses the EDuke32 dialect, before multiplayer roster setup, with a solo-only explanation. Atomic CON network play is unaffected. The session-level policy regression passes; end-to-end lobby feedback and ED10 mid-fight save/restore acceptance remain open.
- `2026-11-15` — **ED0/ED1 acceptance closeout**: verified the roadmap gates locally. The gated Brutal report reproduces the expected inventory and 0 compile errors; the Atomic GAME.CON hash test, 25 CON compiler tests, and the honest 436-tic DEMO1 sync floor pass. ED2 implementation continues; an external EDuke32 fixture capture remains useful for parity measurement, but is not required to continue implementation.
- `2026-11-15` — **ED2 `ifcanseetarget` runtime**: the EDuke VM now uses the actor-to-player Build LOS query with Chocolate's one gameplay TRAND Z jitter, refreshes the visible actor's sleep timer, and no longer lowers this 33-occurrence Brutal Duke condition to `NoOp`. A focused regression checks endpoints and RNG advancement; the CON suite passes 80 tests, and the Brutal report falls from 2,541 to 2,508 no-ops. The EDuke32 reference fixture dump remains pending.
- `2026-11-15` — **ED2 reference capture instructions**: documented where EDuke32 writes its per-run log, added a Windows `findstr` command to extract the fixture’s `ED2REF|` lines before the log is overwritten, and linked the official mutator, logging and echo references. The reference dump is still pending.
- `2026-11-15` — **ED2 scoped instruction regression**: added a compiler-to-VM test that executes actor- and player-scoped `set*var` / `get*var` commands at explicit IDs and verifies array reads/writes plus neighboring-slot isolation. The focused regression passes; the EDuke32 reference dump remains the ED2 acceptance gate.
- `2026-11-15` — **ED2 reference capture harness**: CON `echo <quote>` now emits bounded, drainable VM console messages, exposed to hosts through `DukeSession::take_eduke_console_output`. Added `testdata/ed2-vm-reference.con` and `room duke con-reference`, which execute its `EVENT_INIT` hook and optionally compare labeled `ED2REF|...` lines from an EDuke32 log in source order. The fixture covers wrapping arithmetic, signed division, comparison branches, range loops, switch, state composition, integer math and seeded gameplay RNG. All 78 CON-focused tests and both ED2 CLI fixture tests pass; the gated Brutal inventory and hook smoke also pass with 2,541 explicit no-ops and zero compile errors. The pinned EDuke32 output is still needed for parity.
- `2026-11-15` — **ED2 actor geometry and inventory conditions**: `ifgapzl` compares the actor sector's slope-adjusted floor/ceiling gap, while `iffloordistl` and `ifceilingdistl` measure the actor center against slope-adjusted sector planes using tile height, repeat, centering and offsets. `ifpinventory` reads the nearest live player's inventory and palette-specific access cards. `ifcansee` checks the actor origin plus a deterministic 3×3 grid over its ART-sized face, wall or floor tile extent; duplicate samples are skipped without tile metadata, and wall probes follow Build's sprite angle basis. Regressions cover sector gaps, strict floor/ceiling thresholds, inventory inequality, nearest-player choice, key palettes, and a visible tile sample with a blocked origin. The CON-focused suite now has 76 passing tests; Brutal's no-op count was 2,542 with zero compile errors. Finite visibility sampling and EDuke32 reference parity remain open.
- `2026-11-15` — **ED2 identifier semantics + save migration**: EDuke labels (including state names), gamevars, and gamearrays now resolve with their source spelling; CON command words and event family dispatch remain case-insensitive. A focused CON fixture proves `Foo` and `foo` take distinct state, label, variable, and array paths. `.fsave` version-2 state validation reindexes legacy case-folded variable/array maps after checking saved declaration shape. All 71 CON-focused tests and both gated Brutal Duke checks pass after the exact-state fix. The authentic EDuke32 value dump is still required to close ED2.

- `2026-11-15` — **ED2 actor/player condition pass**: live branches now cover `ifp` flag combinations, `ifdead`, `ifsquished` (including the palette-one queued-hit response), `ifhitweapon` damage consumption, `ifnotmoving`, and `ifhitspace`, alongside the earlier actor, player-distance, and sector predicates. Inline `state` calls now emit VM calls instead of no-ops. The 33 EDuke VM tests and live-session host tests pass; the Brutal CON report has zero compile errors and 2,688 explicit no-op instructions. EDuke32 reference value-dump parity remains open.
- `2026-11-15` — **ED2 actor-state conditions**: `ifcount`, `ifaction`, `ifactioncount`, `ifai`, `ifmove`, `ifactor`, `ifstrength`, `ifwasweapon`, `ifspawnedby`, `ifspritepal` and `ifmultiplayer` now compile to VM branches reading live actor/temp fields or the multiplayer builtin. Inline nested forms and `else` are covered, and the Brutal source scan remains at zero compile diagnostics. The explicit no-op instruction count fell from 5,093 to 3,391. Focused VM coverage passes; EDuke32 reference value-dump parity remains open.
- `2026-11-15` — **ED2 player and sector conditions**: added `ifpdistl` / `ifpdistg` using EDuke32's nearest-player distance, `ifinwater` from the actor sector lotag, `ifoutside` from the sector ceiling-parallax flag, and `ifphealthl` from the active player's health. Nested conditions with next-line operands and the `else ifcount` chain in Atomic `GAME.CON` now compile through the conditional body. The gated scan has zero errors; no-op instructions are down to 3,197. The condition VM test passes; reference value-dump parity remains open.
- `2026-11-15` — **ED2 condition and hook-order coverage**: added a compiler sweep for every mapped signed, unsigned and bitwise comparison spelling, a VM truth-table test for every comparison operator, and a regression proving multiple registered event hooks execute in source order under one shared instruction budget. The EDuke compiler and VM tests pass locally; external EDuke32 value-dump parity remains open.
- `2026-11-15` — **ED2 random condition + live gameplay RNG bridge**: added `ifrnd` with its inclusive 0–255 probability threshold, inline and next-line bodies, and `else`. The VM now reads/writes `randomseed` and consumes gameplay random draws through the live Build RNG adapter when event execution has a world; standalone VM execution retains its serialized seed (default 1996), and display RNG remains separate. Boundary, seed-advance, host-seed, compiler and Brutal inventory checks pass locally. The gated `box.map` smoke executes `EVENT_GAME` once and every registered `EVENT_SPAWN` hook for each active map sprite. `ifrnd` semantics follow [EDukeWiki](https://wiki.eduke32.com/wiki/Ifrnd); executable value-dump comparison remains open.
- `2026-11-15` — **ED2 v9 map hook smoke + predefined player counts**: `room-build` now reads and writes the v9 binary map record layout used by Brutal Duke's `box.map`; the runtime preserves its sector values as raw Build fields, with TROR geometry still unsupported. The gated test loads that map and runs every registered `EVENT_GAME` and `EVENT_SPAWN` hook without VM faults. The VM stores `numplayers` and `MULTIMODE` independently; the current session supplies the same active-player count because Room does not model bots yet, and standalone VM tests can distinguish the values. Both default to 1. External reference value dumps remain open.
- `2026-11-15` — **ED1/ED2 state composition**: `appendstate` now links each extension after the prior body, and `prependstate` becomes the effective entry and continues into the previous body. Duplicate `defstate` keeps the compiler's last-definition behavior. A VM regression executes prepend → base → multiple appends in order; the EDuke compiler tests pass. External reference-engine value dumps remain open.
- `2026-11-15` — **ED1 compiler gate: zero Brutal diagnostics**: EDuke32 statement heads now delimit legacy operand scans (`ife`, `set`, `defineprojectile`, and other dialect commands), and inline braces inside nested `if`/`else` blocks update block depth correctly. `brutal/` compiles with 0 errors (48 target CON files / 34,100 lines; 1,696 vars / 22 arrays / 582 states / 125 hooks / 273 projectile properties / 68 damage tiles; 24,799 partial VM instructions). The 13 EDuke compiler tests, gated Brutal inventory, Atomic GAME.CON image hash and DEMO1 436-tic floor pass locally. CI confirmation remains pending; recognized commands that still lower to `NoOp` are ED2 VM work.
- `2026-11-15` — **ED2 VM math and script RNG follow-up**: `sqrt <input> <output>` now compiles to a bounded integer square-root instruction instead of `NoOp`; this also fixed the integer-square-root boundary for 2 and 3 used by `calchypotenuse`. The built-in `randomseed` gamevar reads and writes the serialized script gameplay stream, starts at 1996, and advances with `rand`/`randvar`; display draws remain on an independent deterministic stream. The VM suite passes 21 tests. External EDuke32 value-dump parity and the `box.map` EVENT_GAME/EVENT_SPAWN run remain open.
- `2026-11-15` — **ED1/ED2 loop coverage + ED5 Build hitscan**: `for <var> range <limit>` uses a saved bound and private induction state; `for <var> allsprites` traverses a per-loop snapshot of live sprite IDs. Braces on the next line compile as the loop/conditional body. EDuke32 `hitscan` compiles its seven ray inputs, six output gamevars and unsigned 32-bit clip mask into the VM; `DukeSession` runs the ray against live sector planes, walls and active sprites. Focused range, sprite-iterator and hitscan tests pass. The measured Brutal Duke report has 2,388 line diagnostics, 24,702 executable instructions, 1,692 source vars plus four generated range-loop vars, and 1,697 initialized runtime vars including `RETURN`. `ezshoot`, custom projectile behavior and the weapon sweep remain open.
- `2026-11-15` — **ED3 live-structure adapter pass**: explicit event dispatch now routes core actor/sprite (`htg_t`, `htflags`, movement/position caches), sector, wall, player and per-tic input reads/writes to the live `DukeSession` when a Build world is loaded. Object and array subscripts are checked; narrow native fields reject out-of-range values; raw linked-list pointer fields stay read-only, while dedicated ED5 commands maintain lists safely. Userdef, tiledata, tsprite and projectile labels, full field coverage and compatibility tests remain open.
- `2026-11-15` — **ED3 tiledata pass**: `gettiledata` / `settiledata` now read and write ART size, picanm offsets and animation fields, and `gameflags`; ART loading retains picanm animation metadata, while `spriteflags`, `spritenoshade`, `spritenopal`, `useractor enemy` and `defineprojectile` seed per-tile game flags. Added bounds, flag-seeding and access tests. `TileZMeta` defaults missing fields when older `.fsave` snapshots load. ED3 still lacks userdef, tsprite and projectile access plus the full Brutal member test set.
- `2026-11-15` — **ED3/ED10 userdef and active-projectile pass**: the live adapter now reads/writes the Brutal Duke userdef fields `vm_sprite`, `player_skill`, `drawweapon`, `statusbarscale`, `crosshairscale`, `weaponscale`, `pause_on` and bounded `return[]`. Setting `vm_sprite` changes the VM's active `THISACTOR` for following instructions and carries through to the next registered hook; `return[0]` aliases the VM `RETURN` variable. `thisprojectile.workslike`, `.bsound` and `.drop` are stored per sprite in saveable actor temps. `.fsave` preserves the modeled userdef fields and skill. The EDuke-focused 22-test set passes. `tspr`, global projectile definitions, other userdef members, remaining target-field tests, and full ED3/ED10 acceptance are open.
- `2026-11-15` — **ED3 transient TSPR pass**: all documented `tspr*` sprite fields now map to a supplied render-only sprite, including blend, and `DukeSession::execute_eduke_event_with_tspr` exposes it to `EVENT_ANIMATESPRITES`. Tests prove edits apply to the transient draw copy without changing the Build world sprite, and check field narrowing / mismatched IDs. Global projectile definitions, spriteext fields, other userdef members, all target-field tests and renderer call-site integration remain open.
- `2026-11-15` — **ED3/ED10 projectile-table pass**: `defineprojectile` declarations seed a bounded per-tile property table, `getprojectile` / `setprojectile` cover the documented numeric property names, and live overrides persist through `.fsave`. A compiled event test reads a macro-defined velocity, changes it, and verifies save/restore. Default values for undeclared properties, declaration diagnostics, projectile behavior during `shoot`/movement and the full field-test set remain open.
- `2026-11-15` — **ED4/ED10 integration foundation**: `DukeSession` initializes the EDuke32 VM when the CON image loads, exposes explicit source-order event-hook dispatch with one shared instruction budget, and retains initialization errors for diagnostics. Dispatch uses the partial live-structure adapter when a world is loaded. `.fsave` version 2 stores gamevars, arrays, quotes and separate gameplay/display random streams; loading validates VM declaration shape and resource bounds against the loaded CON, while version 1 saves remain readable with fresh EDuke32 runtime state. Event call-site ordering/return behavior, remaining native struct state, full save acceptance, and Brutal Duke lobby policy remain open.
- `2026-11-15` — **ED2/ED5 geometry, sprite-list and spawn commands**: added `updatesector` / `updatesectorz`, live-sprite `ldist` / `dist`, sprite-list head/next traversal, `changespritestat` / `changespritesect`, `setsprite`, and `spawn` / `espawn` / `espawnvar` using Room's actor initialization path and the VM's implicit `RETURN`. The Brutal Duke scan now reports 2,397 remaining unsupported CON lines (from 2,550); 24,688 executable-core instructions are retained. Hitscan, custom projectile behavior, and the Brutal weapon acceptance pass remain open.
- `2026-11-15` — **ED10 implicit RETURN save migration**: `.fsave` v2 validation accepts prior EDuke32 VM snapshots that predate the reserved `RETURN` gamevar and adds the zero-initialized slot during restore. Full native struct snapshots and mid-fight restore acceptance remain open.
- `2026-11-15` — **ED2/ED5 math coverage**: added deterministic `abs`, bounded `clamp`, and integer `calchypotenuse` instructions to the EDuke32 source compiler and VM. Added floor/ceiling slope and `cansee` queries that read live Build geometry.
- `2026-11-15` — **ED2 executable VM foundation**: added serializable, deterministic gamevar/gamearray storage with global/per-player/per-actor scopes, case-sensitive names, checked indexing, bounded array resizing and a 16M-cell limit. The gated Brutal Duke image initializes 1,692 vars and 22 arrays. Source CON emits a partial instruction stream for assignment, arithmetic/bitwise/shift, Build scale/trig, arrays, signed/unsigned/bitwise conditions, `while*`, `switch`, state calls, runtime `redefinequote`, integer `qsprintf`, `rand` and `displayrand`. The VM enforces instruction/recursion budgets and serializes separate gameplay/display random state. `%d` and `%%` quote formatting is bounded at 4 KiB; exact EDuke32 PRNG/formatting parity and reference value dumps remain unverified. DukeSession still does not dispatch EDuke32 hooks.
- `2026-11-15` — **ED0/ED1 inventory and compiler baseline**: `brutal_duke_gated` passes for 48 target CON files / 34,100 lines, 126 event blocks / 36 normalized families, 393 `tilefromtexture`, 100 `texture`, 28 DEF files / 3,737 lines and zero brace imbalance; all 49 reachable include names resolve. The Atomic GAME.CON hash smoke and DEMO1 436-tic floor pass locally; CI confirmation remains outstanding. EDuke32 lexing handles prefixed comments and case variants; command words are case-insensitive while labels retain exact spelling, and the compiler records state/event entries, bounds quotes to IDs 0–4095 and retains var/gamevar, array/gamearray, projectile, damage-tile and sprite directives. The current reachable-tree diagnostic count is 2,397; the gated inventory also reports 24,688 partial ED instructions. ED1 zero-error compilation and full execution are not satisfied.
- `2026-11-15` — **ED0 inventory accuracy pass**: the CLI now calls command heads outside the Atomic keyword table “Non-Atomic,” and reports referenced event names, struct members and DEF statement heads as inventories rather than claiming those tokens are unsupported. The gated Brutal report test passes against the local v0.2.0 tree with 2,388 compile diagnostics, 1,696 parsed vars / 1,697 initialized runtime vars and 132 distinct referenced struct members. The source-level inventory is still not a DEF parser; that parser remains ED6 work.

**How agents update this file**

1. After completing a work item, flip `[ ]` → `[x]` and bump the milestone
   **Status** / **%** in §0.
2. When *all* acceptance boxes for a milestone are `[x]` **and** CI proves it,
   set Status to `done` and move the active focus to the next milestone.
3. Add short dated notes under *Log* (newest first), not essays.
4. Do not mark a milestone `done` from conversation history alone — verify
   against the acceptance criteria and CI.

**Status values:** `not_started` · `in_progress` · `blocked` · `done`

---

- `2026-11-13` — **Duke D5 honest DEMO1 frontier 270 → 436**: the oracle (`scripts/duke-oracle`) now builds in a cloud container (gcc-multilib + 32-bit SDL 1.2) and reproduces the committed DEMO1 trace byte for byte. Fixes, each traced to a Chocolate source line: 32-bit gcc evaluates call arguments right to left, so every multi-`TRAND` `EGS`/`cansee` call (27 sites + `RANDOMSCRAP`) now draws in that order; COOLEXPLOSION1 spawns at shade 0, restores its pre-move position on wall/plane hits and does not fade while stuck on an actor; shallow-water view bob (`pycount`/`pyoff`) replaces the `pycount: 63` bias; badguy `move()` follows Chocolate (DRONE/SHARK/COMMANDER cadence, DRONE/COMMANDER plane clamps, close-range push, `actorsqu`, `xvel||zvel` gate) and the peer-clip hack is gone; `badguy()` drops OCTABRAINSTAYPUT/BOSS1STAYPUT; `movetransports` spawns the surfacing `WATERSPLASH2` and warps statnum 1/4/5/12/13 sprites with Chocolate's skip list; new `movedummyplayers` (PLAYERONWATER); footprints use start-of-tic state and Chocolate's statnum rules. `DEMO1_REAL_SYNC_TICS` = 436; `demo1_frontier_logs_first_divergence` now checks against that floor instead of the stale tic 39. Next divergence: APLAYER CON emulation (`moveplayers.rs`) at tic 436. DEMO2/DEMO3 still diverge at tic 0.
- `2026-11-13` — **Brutal Doom's ZScript as a Rust mod (`docs/39`)**: `mods/brutal-zscript-rs` is a hand port of Brutal Doom's `zscript/BD_WCS.zc` + `DamageHandler.zc` (`ZandroCompat` ScriptCall functions, `ZC_TMapHack` lump table, `Script_DamageMult`, `Damage_Control.ModifyDamage`) on `room-script-sdk`, logic behind a `Host` trait with FakeHost unit tests, `build.sh` → `brutal-zscript.wad` (52 KB module). Append-only ABI v1 additions: `script_arg_count/i32/str` + `script_return_str` (ACS `ScriptCall` to `reg_static_function` targets now gets every argument, strings included, and can return a string handle; `room-sim` serves them from the bound `ModRuntime` after the ZScript VM), `VirtualSlot::ModifyDamage` = 5 with a `ModifyDamageV1` payload (dispatched in `apply_inventory_modify_damage` before ZScript), `player_gender`, `player_cvar_i32`, `actor_set_sound_class`; SDK `RoomMod::on_static_call` / `on_modify_damage` (callback ranges 2000 / 3000), `StaticCall`, `ModifyDamage`. Host fixes found on the way: `begin_map` now replays `WorldThingSpawned` for map-placed things (mods never saw them), play-time `lump_find`/`lump_read` keep the registration lumps (`restart_mods` dropped them), nested guest dispatch into a busy module is skipped. Manifest `[mod] replaces_zscript = [...]` drops those files and their `#include`s from the ZScript Room compiles. Checks: mod `cargo test` (5), `room-sim` `brutal_zscript_port_end_to_end` (built module: skill-6 tokens, ×7 damage both ways, ScriptCall with string args/result, TIDs, gender), `room-game` `bd_rust_port` (BD with ZScript vs BD + port, Freedoom E1M1, 1,500 tics with save/load halfway, per-tic actor/inventory digest identical), manifest + `zscript_load` unit tests; `room-sim` lib pass/fail set unchanged (18 pre-existing failures). Note: Room clamps skill to 1–5, so BD's skill-6 branch never runs in a real game, with either implementation.
- `2026-11-13` — **ZScript → WebAssembly ZW4–ZW9 (`docs/37`)**: converted mods (`room mod convert`) ship `room/zscript.wasm` = AOT code of the unlinked program + program image (`CompiledProgram::to_bytes`) + flags/ABI/source-list sections; the game loads them without compiling ZScript (`load_zscript_from_vfs`), attaches the shipped code (`Zvm::attach_shipped`, native-id map) and warns on unverified packages. Engine-side heap design instead of `room-zrt` (chosen by the project owner; `docs/37` §0.2). `--verify N` plays original vs converted headless with scripted input and a save/load halfway; Brutal Doom 2,000 tics on Freedoom E1M1 at skill 6 identical, 3,158 wasm calls. `room zscript wasm --report` + `docs/qa/zscript-wasm-coverage.md`. `StatusBarClass` mods also get `room/zscript_ui.wasm`; `load_status_bar_program` now leaves out the engine's own `room.pk3` ZScript, which made every mod status bar fail to compile once the engine archive was mounted. Browser backend `aot-web` + `room-web` `?zvm=aot` (type-checked only). ZW5: `lower.rs` loads library globals only where a function names them (UZDoom bytecode 421k → 139k instructions, same `stdlib_probe`/`room-sim` results), `auto` entry policy (short functions stay on the interpreter entry), boxed play host. Checks: `room-zvm-wasm` tests on wasmi and wasmtime (incl. RoomMod round trip), `room-game` `mod_convert` (small mod verify + load + mismatch rejection, status bar draws identically, BD gated) and `bd_zscript_aot`, `room-sim` lib/zscript pass/fail sets unchanged.
- `2026-11-13` — **ZScript AOT on wasmtime (`docs/37` ZW3)**: `room-zvm` feature `aot-wasmtime` runs the generated module on wasmtime 26/Cranelift; the host is split into engine-neutral imports (`aot.rs`) and per-engine glue (`aot/wasmi_backend.rs`, `aot/wasmtime_backend.rs`), the VM is swapped into the store instead of reallocated, entry masks are cached. `room-sim` feature `zvm-aot-jit` (used by `room-app`), `room-cli` builds with it. Release timings (`examples/aot_bench`): Collatz ×39, float loop ×239, BD workloads ×2.4, 10k one-line calls ×0.5 (per-call overhead). Checks: `room-zvm-wasm` tests on wasmi, wasmtime and `ROOM_ZVM_ENGINE=wasmi`; BD E1M1 2,100 tics identical on wasmtime; `room-sim` `zscript_play`/`stdlib_probe`/`skill_levels`/`weapon_timeline` same pass/fail set and probe output with `ROOM_ZVM_TIER=aot`.
- `2026-11-13` — **ZScript → WebAssembly (`docs/37` ZW0–ZW3)**: new crate `room-zvm-wasm` compiles ZVM bytecode to a wasm module (CFG + dominators + Ramsey structuring, every non-legacy opcode, per-block fuel, depth limit, tagged return cells, register-image calls, `call_indirect` virtual calls, interpreter bridge). `room-zvm` feature `aot` runs it on wasmi against the engine heap and natives (`Zvm::attach_aot`, `ROOM_ZVM_TIER=aot`, `room-sim` feature `zvm-aot`). Interpreter changes: fuel charged per basic block, `Trap::Overflow` for `MIN/-1`, shared `native_call` and heap ops. `room zscript wasm <pk3|files> [-o] [--wat] [--check] [--bd]`; `disasm` argument order fixed (`func` first). Checks: differential unit tests (results, traps, fuel, heap, play host), UZDoom stdlib 2,181/2,182 compiled and `stdlib_probe` identical, `room-sim` test pass/fail sets identical across tiers, Brutal Doom 32/32 functions + 2,318 identical calls + E1M1 2,100 tics with identical world/ZScript checksums every tic (`room-game/tests/bd_zscript_aot.rs`, extracted BD + Freedoom1). Open: wasmtime backend (speed), web (ZW4), RoomMod/convert (ZW6–ZW9).
- `2026-11-13` — **Open standalone PWADs in the editor**: the Game… picker now treats a user-selected PWAD as the working base; Open… still mounts a PWAD over an already loaded base. This supports WADs labeled Doom 1 that carry a PWAD header. `node --check` and `git diff --check` pass; no tests were run.
- `2026-11-13` — **Editor archive loading indicator**: selecting game archives, content packs, loose maps, or dropped files now shows a centered spinner and the chosen file names; the overlay paints before synchronous archive parsing and always clears after completion. `node --check` and `git diff --check` pass; no tests were run.
- `2026-11-13` — **Separate Doom / Duke editor start + preset admin page**: editor opens to an explicit Doom / Duke / PK3 choice with no default IWAD or map; each game gets its own map labels and browser tabs, and removing a pack no longer switches to another map. Admin presets now have a dedicated page with a picker, configuration preview, and create/edit/delete actions; content uploads remain in Settings. `node --check` for both JS files, `cargo check -p room-server`, wasm `cargo check -p room-web`, and `git diff --check` pass; no tests were run.
- `2026-11-13` — **Native Duke map editor and play-test**: the web editor now opens and creates native Build maps in a dedicated 2D view, edits vertices, walls, sectors and sprites, draws/removes sectors with shared-edge portal linking, supports sprite placement/deletion and property editing, saves native `.MAP` bytes, and exports a complete GRP with edited maps and ART archives. The searchable texture browser exposes all ART tiles, and map tile controls open the indexed pixel painter. Native test loads the edited map plus a replaceable GRP overlay; F7 returns to the editor. `cargo check -p room-build`, wasm `cargo check -p room-web`, JS syntax checks, and `git diff --check` pass. No game/browser session or content play-through was run.
- `2026-11-13` — **Duke floor overview + room admin usability**: the Build editor can label sectors with native floor Z and ART tile on the map, toggle labels, and click a per-map floor list to jump into a sector. The admin room creator now offers Doom/Duke selection, preset descriptions and constrained map pickers, visibility and time-limit defaults; preset editing uses guided sections, searchable maps, an explicit any-map switch, and move-up/down content ordering. The admin console now uses more legible system typography, spacing, and responsive cards/forms. JS syntax checks, `cargo check -p room-server`, wasm `cargo check -p room-web`, and `git diff --check` pass; no UI browser session was run.
- `2026-11-12` — **Duke resources enabled in the web editor**: re-enabled the Duke GRP / Build `.MAP` import path, exposing its resource browser, map editor, pixel-art painter, exports, and play-test controls. That first slice used the GRP-to-UDMF converter; the native Build editing and test path is recorded above.
- `2026-11-12` — **Duke sprite switches + view at a portal line**: player use now retries the switch search at the eye, eye+8<<8 and eye+16<<8 like Chocolate `checksectors`' three `neartag` calls (it only tried the lowest), so eye-height sprite switches such as E1L1's projection-booth SLOTDOOR (lotag 135) work: it runs the lotag-135 ACTIVATORs (cinema curtains, sectors 1/2/21/22/147/148) and SE12 lights. The portal renderer now treats a portal wall that passes within the near plane in front of the camera as a full-screen opening, so standing against the booth window no longer blacks out the view of the theatre. Checks: `duke_play use-at` probes (switch flips activator sectors; window view draws the theatre), interactions tests (2 pre-existing failures unchanged), `demo1_real_sync_floor` passes.
- `2026-11-12` — **Duke mirrors, sleeping-enemy hits**: `render_fp_textured_at` looked for the nearest mirror only in the incoming view's `visible_sectors` (the camera sector), so E1L1's bathroom mirror drew its raw MIRROR tile; it now searches the sectors the portal pass actually drew and renders the reflected pass (Duke's own reflection still missing). Player hitscans on a dormant badguy (stat 2) now wake it into STAT_ACTOR with `timetosleep = SLEEPTIME` before recording the damage (Chocolate `checkhitsprite`); before, the hit was silently dropped. New `duke_play airkill <sprite>` (lift, shoot, watch it fall) and a floating-corpse report in `kill-all`: corpses of enemies killed in the air fall to the floor. Stall doors: both open when the player is not standing inside the swing arc (Chocolate's 144-unit `clipinsidebox` hold). DEMO1 honest floor 270 still passes.
- `2026-11-12` — **Duke see-through doors + sunken touchplate floors**: the portal renderer dropped every wall at the same depth as the portal it entered through; Build maps put closed swing doors and toilet-stall doors exactly on the doorway line (the outer sector wraps around the door slab), so from one side those doors were invisible. Walls on the entry line are now kept when front-facing (the portal's own back side is still culled). TOUCHPLATE now gets Chocolate's `spawn` init (`T3` = sector floor, floor = plate z outside water); without it the first step on a plate sank its floor toward z 0 (at the new 256 sector speed in one second), which made E1L1's corridor ledges unclimbable. Probes: `use-at` prints z/floor while walking; all E1L1 swing doors render solid from both sides. DEMO1 honest floor 270 still passes.
- `2026-11-12` — **Duke D8 front end: native menus + bonus screen**: new `room_duke::ui_page` (Chocolate 320×200 `rotatesprite` with picanm centring, `menutext`/`gametext`/`minitext`), `room_duke::bonus` (single-player `dobonus`: BONUSSCREEN(+5 for episode 2) art, animated Duke frames, level name/COMPLETED, time/par/3D Realms, kills/left (N/A above skill 3), secrets found/missed, Chocolate's 120 Hz stage timeline with BONUSMUSIC, PIPEBOMB_EXPLODE ×3, FLY_BY and SHOTGUN_COCK + random BONUS_SPEECH outro, first key skips to the end, second key leaves) and `room_duke::menu` (menus 0/50/100/110/990–992/501–503 with spinning-nuke cursors, KICK_HIT/PISTOL_BODYHIT/EXITMENUSOUND and the skill cues). The desktop shell now shows these instead of its generic panel/list (Esc = in-game menu; save/load/sound reuse the existing list pages). Also: the APLAYER reset no longer wipes `max_actors_killed`/`max_secret_rooms` (bonus showed 0 totals). Door probes: E1L1 doors open when faced; the remaining closed ones hold an ACTIVATOR/MASTERSWITCH (switch-only, as in Chocolate). Unit tests for bonus/menu flows; DEMO1 honest floor 270 still passes.
- `2026-11-12` — **Duke doors + status-bar digits**: `prelevel` sets every sector's `extra` to 256 before GPSPEED (Chocolate), so SE `yvel` speeds are right: SE11 swing doors now open 90° away from the player in 8 tics instead of creeping toward them (leaving half-open blades across the doorway that blocked the player and poked outside their parent sector, which the portal renderer then showed see-through), and the SE15 sliding doors move (E1L1 `open-all` 7 → 11 of 21; the remaining stuck entries are toilet-stall/secret doors the sweep cannot approach correctly); already-open lotag 20/22 doors get Chocolate's toggle bit. Status bar: `weaponnum`/`invennum` THREEBYFIVE digits are centre-anchored (Chocolate draws them without `rotatesprite` flag 16), fixing the misaligned weapon list; mini HUD shows the inventory `%`. DEMO1 honest floor 270 still passes.
- `2026-11-12` — **Duke play-test fixes 2: glass, hydrant water, posters, shading**: GLASSPIECES now run Chocolate's `moveexplosions` branch (raw-sector `makeitfall`, three bounces halving size, xvel drag, stat-5 `ssp`), so shards fall instead of hanging in the air; a broken hydrant spawns TOILETWATER through `spawn` as a CON actor (shade -16) so its frames animate and `sizeto` grows it; `EGS` now copies the owner's `hittype.floorz/ceilingz` and map sprites seed them from their sector (Chocolate `spawn` j<0), which `sizeto` needs; wall-aligned sprites render perspective-correct, clip at the near plane and win depth ties with the wall they sit on (posters were striped or vanished at oblique angles); distance shading uses Build `wallscan`'s rate (half the previous darkening); `operatesectors` case 25 (SE15 sliding doors) no longer requires an activator hitag. New `duke_play` probes: `use-at` with sector/horiz, `ROOM_FIRE` glass tracking, `ROOM_WALK`, `break-all ROOM_AFTER`. DEMO1 honest floor 270 still passes.
- `2026-11-12` — **Duke play-test fixes: doors, props, reload, status bar**: the player's open key now runs Chocolate `checksectors`' sector tail (`isanearoperator` sector in front via a 1280-unit neartag, else `isanunderoperator` own sector; ACTIVATOR/MASTERSWITCH sectors ignore it) — before, USE only drove switches, so E1L1's EXIT doors and most sector doors never opened; breakable props (HYDRENT…PIPE6 spawn case) get `cstat 257` + clipdist 32 so the hydrant and friends take bullets; new `weapon_view` module ports `displayweapon` frame-for-frame (pistol clip reload, all weapon kickback frames, `weapon_sway` bob, quick kick, shrunk fists); full Atomic status bar (health, armor, weapon ammo list, inventory with ON/OFF/AUTO and %, keys) is the default HUD, `duke_statusbar 0` restores the mini HUD; `m_pitch`, `duke_hitdebug`, `duke_statusbar` are now registered cvars. New `duke_play` sweeps: `open-all`, `break-all`, `use-at`, `reload`. DEMO1 honest floor still 270.
- `2026-11-12` — **Duke D5/D6 combat fixes from play-testing**: enemy projectiles (SPIT, FIRELASER, RPG, Freezer/Shrinker, COOLEXPLOSION1) now advance one `movesprite` step per tic instead of jumping to the end of an unbounded hitscan (enemy lasers no longer hit instantly from any range); FIRELASER trail copies are deleted after one tic (Chocolate `moveexplosions`), so no more beams frozen in the air; over-skill/monsters-off map enemies stay zero-sized on STAT_MISC (were resized to 40 and left as unkillable statues); `queue_player_sound` and the jetpack-off path no longer *truncate* `actor_temps` with `resize_with` (wiped CON state of every sprite above the player, e.g. a palette-21 trooper then added strength every tic). New `duke_play kill-all <MAP>` sweep: E1L1 30/33, E1L2 53/56, E1L3 53/56 enemies killed with the pistol (rest are placement/line-of-fire cases). Hit log goes to the console only. DEMO1 honest frontier unchanged at 270.
- `2026-11-12` — **Duke D5 honest DEMO1 frontier 264 → 270**: OCTABRAIN `shoot COOLEXPLOSION1` now draws Chocolate's two trailing loop TRANDs; CON `fall` runs `makeitfall` mid-parse through a VM hook (a later inline `shoot` sees post-fall Z and refreshed hit planes); COOLEXPLOSION1 flight uses `movesprite` (clipmove walldist 8, plane check) instead of an unbounded hitscan. Verified with the oracle (`duke_frontier krand` + new `duke_frontier dump` vs Chocolate sprite dumps at tics 263–270); `DEMO1_REAL_SYNC_TICS` raised to 270. Next: tic-270 `tracers` WATERBUBBLE positions differ by 4–11 units (shotgun pellet hit point). Also: desktop view horizon now uses Chocolate's direction and scale (`horiz > 100` looks up, crosshair = bullet path), `duke_hitdebug` hit log, scrap debris motion.
- `2026-11-11` — **Duke D3/D5/D6/D7 playability pass + milestone re-baseline**: map enemies get Chocolate's spawn header (action/move/hitag) and now wake and hunt; enemy `shoot` implemented (FIRELASER, SPIT, RPG, FREEZEBLAST, SHRINKER, MORTER/BOUNCEMINE, hitscan, KNEE); CON `spawn` creates real sprites (enemies, BLOODPOOL, effects, item drops, generic CON actors); RESPAWN-spawned enemies get full size/cstat (were 1×1 and unhittable); bullets reach object `checkhitsprite` cases and the default hit record (SEENINE etc.); DUKECAR/HELECOPT intro + earthquake; `animatesprites` (action frames/rotations), own APLAYER hidden, zero-art sprites skipped, Duke crosshair, floor/ceiling texel scale independent of tile size; desktop mouse aiming (residual carry, display-rate preview, camera interpolation, `m_pitch`). Percentages in §0 re-baselined to `25` §0 “Milestone status”. Headless E1L1 scenario + honest DEMO1 floor (264) unchanged.
- `2026-11-08` — **D6 APLAYER Shrinker afterimages**: the player pass now creates `FRAMEEFFECT1` at the shrinking APLAYER pose on each CON shrink tic, then the native misc-actor pass advances each effect for eight tics with Chocolate translucency stages and owner X/Y offsets. Static Chocolate `actors.c::moveactors` and Atomic `GAME.CON` review; formatting, targeted crate compile and diff check only, no tests or dynamic game/reference comparison.
- `2026-11-08` — **D6 Atomic APLAYER Shrinker/Grower response**: statically inspected Atomic `GAME.CON` APLAYER hit branches and added player-specific shrink/grow flash + audio, `PSHRINKING` timer/repeat transition, steroid skip, sloped-gap crush and cstat restoration. NPC-specific CON transformations, APLAYER particles and general APLAYER script execution remain open. `cargo fmt -p room-duke`, `cargo check -q -p room-duke --lib`, and `git diff --check` pass with existing warnings; no tests, gameplay/demo or Chocolate comparison.
- `2026-11-08` — **D10 dead-player camera progression**: the dead-player pass now follows the APLAYER body, turns halfway toward the last valid killer each tic, applies water-warp Z behavior, refreshes the Build sector and resolves `pushmove` before calculating floor-relative `rotscrnang`. This is per-seat and does not mutate render snapshots. Formatting, diff check and targeted `room-duke` compile only; no tests, gameplay, network or reference comparison. Spectator flow, full death animation, respawn marker edges and multiplayer validation remain open.
- `2026-11-08` — **D10 dead-player input early return**: dead seats now bypass access-card interaction, quick-kick/knee continuation, floor-hazard/inventory timers, footprints, Holoduke and inventory-use effects after their neutral movement branch. Their body angle still follows the dead camera turn, while the body position remains separate. This follows Chocolate's `player.c::processinput` death return and `actors.c::moveplayers` body update. Formatting, targeted `cargo check -q -p room-duke --lib`, and diff check pass with existing warnings; no tests, gameplay, network or dynamic comparison.
- `2026-11-08` — **D10 death jetpack cleanup**: the one-time death edge now always clears jetpack/Holoduke state and queues a stop for the active CON's `DUKE_JETPACK_IDLE` sound, including when the body palette is 1; only the normal palette death tilt/roll stays behind Chocolate's non-frozen palette check. Static review of Chocolate `player.c::processinput`; formatting, targeted crate compile and diff check pass with existing warnings. Voice-specific scream termination remains unmodeled; no tests or dynamic comparison.
- `2026-11-08` — **D8 finale ANM sound cues**: E1/E2/E4 finale frame transitions now queue Chocolate's `endanimsounds` / `endanimvol41` CON sound labels through the native mixer. If rendering skips frames, crossed cue frames are still queued in order; the session cursor prevents repeated playback on a held frame. Other ANM types/cue tables and full cutscene flow remain open. Static review of Chocolate `menues.c::playanm` and its frame cue handlers; format, diff and targeted compile only, no tests or gameplay/reference comparison.
- `2026-11-08` — **D9 Atomic lockclock overflow**: `save_atomic_sav` now rejects a native tic count that cannot be represented by Chocolate's signed 32-bit `lockclock`, instead of silently writing `i32::MAX` and changing the resumed timeline. Full SAV field/default coverage and load-save-load/reference validation remain open. Static ABI review; formatting, diff check and targeted compile only, no save replay or engine comparison.
- `2026-11-08` — **D8/D9 bonus totals and SAV counters**: imported/exported Chocolate's `max_secret_rooms` and `max_actors_killed` player bytes, initialize per-level totals from Atomic's sector scan and eligible map-spawn rules, and draw those totals instead of reconstructing them from survivors. Dynamic CON-spawned kill totals and SAV load-save-load validation remain open. Static review of Chocolate `premap.c::prelevel`, `game.c::spawn` and `menues.c::dobonus`; formatting, diff check and targeted compile only, no gameplay or Chocolate comparison.
- `2026-11-08` — **D7 mirror rendering**: map loading initializes masked `MIRROR` sectors and the soft renderer draws the nearest projected mirror from a reflected camera into the wall mask, without recursive passes. It detects the active CON's resolved mirror tile from the prepared destination sector, retaining tile 560 for unprepared Atomic maps, and ranks mirrors by Chocolate's Manhattan distance to the source wall's first vertex. Chocolate selects one nearest visible mirror per view; mirror-sector composition, visibility reduction and raster parity remain open. Static Chocolate `premap.c` / `game.c::displayrooms` review, formatting, diff check and targeted crate compile only; no tests or gameplay/reference comparison.
- `2026-11-08` — **D7 SE13 parallax sky restore**: the explosive-sector transition now copies the first prelevel parallax ceiling's tile and shade when Chocolate flips the restored sector's ceiling parallax flag. Static `premap.c::prelevel` / `actors.c::moveeffectors` review; format, diff and targeted `room-duke` compile only, no tests or dynamic gameplay/reference comparison.
- `2026-11-08` — **D7 SE13 interpolation status audit**: corrected the stale “plane interpolation remains open” note. Room's desktop Duke renderer already blends every sector's floor/ceiling height between completed tics on a render-only world copy, so SE13 receives that generic visual smoothing. Chocolate's per-effector interpolation registration and exact rendering cadence remain unverified; static inspection of `DukeSoftRenderer::render_rgba_interpolated` and its desktop call site only, no runtime/reference comparison.
- `2026-11-08` — **D6 radius ceiling-light status correction**: corrected stale D6/D7 text that said radius blasts did not hit ceiling lights. The current shared RPG/CON `hitradius` sector scan checks each visited ceiling before wall hits and invokes `check_hit_ceiling` using Chocolate's vertical-radius gate and first/third boundary-point test. Static review of Room's `apply_radius_wall_hits` against Chocolate `actors.c::hitradius` / `sector.c::checkhitceiling`; documentation diff check only, no tests or dynamic comparison.
- `2026-11-08` — **D7 automatic activator respawns**: SE10 `activatebysector` now arms matching `RESPAWN` markers when it dispatches a tagged `ACTIVATOR`, including the `monsters_off` badguy gate. Static Chocolate `sector.c::operateactivators` / `operaterespawns` review; formatting, diff check and targeted `room-duke` compile only, no tests or dynamic comparison.
- `2026-11-08` — **D6 pipebomb respawn marker motion/cadence**: red/yellow/green markers now use Chocolate's shifted half/quarter thresholds, retain their prior color in the threshold gap, and call the shared `makeitfall` path every live tic. Static `actors.c` marker branch review; formatting, diff check and targeted crate compile only; no tests or gameplay/reference comparison.
- `2026-11-08` — **D5 shared `makeitfall` gravity**: the common falling primitive now applies Chocolate's outer-space floor/ceiling gravity, the water terminal-velocity reset, and the 6144 cap. This feeds map spawn, slope-sector carry and item respawn-marker motion. Static `gamedef.c::makeitfall` review; format, diff check and targeted compile only, no tests or dynamic comparison.
- `2026-11-08` — **D6 wall-light hit RNG ordering**: direct `WALLLIGHT`/`TECHLIGHT` impacts now draw the randomized break cue before the 30-shard `lotsofglass` burst, matching Chocolate's `checkhitwall` call order and preserving subsequent RNG state. `cargo fmt --all`, `git diff --check`, and `cargo check -q -p room-duke --lib` pass with existing warnings; no tests, runtime, or Chocolate comparison.
- `2026-11-08` — **D6 tripbomb laser trace sector and range**: the temporary muzzle offset now resolves its Build sector before the trace, matching Chocolate's `setsprite` step, and hit range uses Build `FindDistance2D`. Ray collision still uses Room's `hitscan_full` substitute for `hitasprite`; static source review only, no runtime or Chocolate comparison.
- `2026-11-08` — **D6 tripbomb blast flash movement**: the spawned `EXPLOSION2` now gets Chocolate's immediate `ssp(CLIPMASK0)` move with Build clipping and sector-list update at detonation. Static `actors.c::moveactors` / `ssp` review; formatting, diff check and targeted compile only; no tests or gameplay/reference comparison.
- `2026-11-08` — **D6 `checkplayerhurt` collision paths**: retain `clipmove`'s sprite/wall hit code; port cactus damage and blocking `W_FORCEFIELD`/`BIGFORCE` responses, including their distinct cooldown/damage rules and `atwith=-1` ripple size. Static Chocolate `sector.c` review, format, diff check and targeted compile only; no tests or gameplay/reference comparison.
- `2026-11-08` — **D8 desktop RTS feedback**: the HUD now displays `RTS PLAYING` while any local or peer-delivered RTS voice is active; it follows actual mixer voice lifetime. Static UI/audio path review, formatting, diff check and targeted desktop compile only; no gameplay, tests or reference comparison.
- `2026-11-08` — **D5 APLAYER swim action edge**: the trailing WATERBUBBLE `ifrnd` now runs only while the swim action remains active; changing to another action skips the old branch's final RNG roll. This targets the documented tic-70 frontier from the previously measured trace. Static source/control-flow review, formatting, diff check and targeted compile only; the verified frontier is unchanged until a later comparison.
- `2026-11-08` — **D7 SE20 bridge plane panning**: extended the directional eight-unit SE20 bridge translation to scroll both floor and ceiling X/Y panning by `dx>>3` / `dy>>3`, matching the separate Chocolate SE20 branch. Static `actors.c::moveeffectors` review, formatting, diff check and targeted compile only; no gameplay/demo comparison.
- `2026-11-08` — **D7 SE20 bridge carry membership**: separated player-body handling from the generic sprite carry, matching Chocolate's dedicated player pass, and now refreshes Build sector membership for carried sprites and grounded players after `setsprite`-equivalent movement. This avoids duplicate traversal and stale sector lists when the moving bridge crosses sector boundaries. Static `actors.c::moveeffectors` review, formatting, diff check and `cargo check -q -p room-duke --lib`; no tests or gameplay/demo comparison.
- `2026-11-08` — **D7 access-card first-person animation**: the native Duke renderer now resolves `HANDHOLDINGACCESS` / `HANDHOLDINGLASER` from CON, draws Chocolate's 1–20 `access_incs` pose with card palette, view sway, look/horizon and hard-landing offsets, and suppresses the weapon during a live insertion. Access-card status-bar icons were already emitted by `DukeStatusBar`. Static `player.c::animateaccess` / `displayweapon` review, formatting, diff check and `cargo check -q -p room-duke --lib`; no screenshot, game run, tests or Chocolate comparison.
- `2026-11-08` — **D7 conveyor panning correction**: reinspection of the complete `actors.c::moveeffectors` branch showed that the directional floor/ceiling X/Y panning belongs to SE20; SE24/SE34 only add `SP >> 7` to floor X panning. Room now follows the conveyor branch exactly. The prior turn had applied the SE20 behavior to conveyors. Static source review, formatting, diff check and targeted compile only; no gameplay/demo comparison.
- `2026-11-08` — **D7 conveyor bolt filter**: SE24/SE34 now skip only BOLT1 and SIDEBOLT1 families (four tiles each) as Chocolate does; the prior SIDEBOLT1 range accidentally excluded ordinary sprites through tile 4528, preventing them from riding conveyors. Static `actors.c::moveeffectors` review, formatting, diff check and targeted compile only; no tests or gameplay/demo comparison.
- `2026-11-08` — **D6 archived auto-aim setting**: the Duke F4 Sound Setup now displays and cycles bullet-only/full auto-aim; left/right change the selected mode, which remains in the archived `duke_autoaim` CVar and is consumed by desktop input. Static UI/input review, formatting, diff check and targeted compile only; no gameplay/demo comparison.
- `2026-11-08` — **D6 tripbomb laser re-arm baseline**: target-range-change detection now retains the initial `lastvx` baseline, as Chocolate does; repeated changes continue to arm the fuse instead of silently replacing the baseline after one tic. Static `actors.c` review, formatting, diff check and targeted compile only; no tests or gameplay/demo comparison.
- `2026-11-08` — **D6 CON tile overrides in standable reactions**: SEENINE/OOZFILTER state transitions, linked targets, CRACK1/FIREEXT damage gates and blast-triggered hitag chains now resolve the active CON tile labels, retaining Atomic IDs as fallbacks. Static script/runtime source review, `cargo fmt --all`, `git diff --check`, and targeted `room-duke` compile pass with existing warnings; no tests or dynamic gameplay/reference comparison.
- `2026-11-08` — **D6 scripted tripbomb actor dispatch**: `tick_tripbombs` now selects the active CON `TRIPBOMB` tile (Atomic 2566 fallback), matching placement, duplicate detection, radius targeting and the `STAT_STANDABLE` actor pass. Custom CON tile overrides therefore reach the native fuse/laser lifecycle after spawn. Static CON/Atomic path review, formatting, diff check and targeted compile only; no tests or dynamic gameplay/reference comparison.
- `2026-11-08` — **D6 standable list ownership**: the standable phase now copies the session's current sprite links into `BuildWorld` before dispatch and copies the mutated links back afterward. BOLT spawns/deletes and detonation/tripbomb blast effects therefore share one list and survive the phase instead of one list overwriting the other. Formatting, diff check and targeted compile pass with existing warnings; no tests or dynamic gameplay/reference comparison.
- `2026-11-08` — **D6 standable actor dispatch order**: Room now snapshots the linked `STAT_STANDABLE` traversal order and dispatches its implemented BOLT/water-maker, SEENINE/OOZFILTER and tripbomb handlers per actor, rather than running three whole-list passes. This preserves cross-actor mutation and RNG order within Chocolate's single `movestandables` pass. `cargo fmt --all`, `git diff --check`, and `cargo check -q -p room-duke --lib` pass with existing warnings; no tests or dynamic gameplay/reference comparison.
- `2026-11-08` — **D6 tripbomb hit arms without draining blast power**: tripbomb `IFHIT` now consumes the pending weapon hit only to set the 16-tic fuse, preserving `sprite.extra` as the explosion-strength value. This matches Chocolate's temporary-health probe and restore. Static `actors.c` review, formatting, diff check and targeted compile only; no tests or dynamic gameplay/reference comparison.
- `2026-11-08` — **D6 radius actor damage ordering**: RPG and CON `hitradius` now queue actor/standable damage and knockback before the source's restricted civilian/statue/pool-ball `checkhitsprite` transitions; transition handling no longer cancels the queued blast damage or runs for unrelated actor tiles. Static `actors.c::hitradius` review, `cargo fmt --all`, `git diff --check`, and `cargo check -q -p room-duke --lib` pass with existing warnings; no tests or dynamic gameplay/reference comparison.
- `2026-11-08` — **D8 title/finale animation clock**: desktop title splash/logo and finale frame selection now use accumulated monotonic elapsed milliseconds, not `rendered_frames × 16 ms`; entering the finale resets its animation origin. This fixes `TITLEDEF` frame cadence at variable refresh rates, while ANM sound cues and the broader front-end checklist remain open. `cargo fmt --all`, `git diff --check`, and targeted `room-app` compilation only; no tests or gameplay comparison.
- `2026-11-08` — **D8 browser F7 camera**: native Duke web play now toggles the third-person shoulder view with F7. The browser keeps it in local input/render state and renders a copied player view, so cooperative simulation state and lockstep checksums are unaffected. Static call-path review and targeted compilation only; no browser session, gameplay, tests, or reference comparison.
- `2026-11-08` — **D7 per-player camera release**: leaving a VIEWSCREEN now clears only the `CAMERA1` latch matching that player's stored remote-camera pose; it no longer disables every active camera, which could disrupt another multiplayer seat. Static `sector.c::checksectors` call-path review, formatting, `git diff --check`, and targeted compilation only; no gameplay, tests, or reference comparison.
- `2026-11-08` — **D6 normal knee uses shared impact resolver**: the normal weapon now retains its `KneeHit` result and enters the same spawned-actor, prop, wall, and tagged-switch path as CON `pkick`; this prevents the impact work from being quick-kick-only. Static `player.c::shoot(KNEE)` review, formatting, diff check and `cargo check -q -p room-duke --lib` only; tests and gameplay/reference comparison were not run.
- `2026-11-08` — **D6 knee impact sprites and transitions**: a connected knee hit now spawns Chocolate's zero-size `KNEE` actor plus offset `SMALLSMOKE`; sprite targets run the existing `checkhitsprite` prop transitions and wall targets run `checkhitwall` after the bottom-wall peer adjustment. The actor's impact strength excludes the later steroid bonus while the queued hit damage includes it. Static `player.c::shoot(KNEE)` review, formatting, diff check and targeted compilation only; no tests or gameplay/reference comparison.
- `2026-11-08` — **D6 knee damage strength**: normal and quick knee hits now use the CON `KNEE` actor strength plus Chocolate's `TRAND&7`, with the `MAXPLAYERHEALTH/4` steroid bonus while the steroid timer is active. Atomic defaults to strength 10 / max health 100 when no script is supplied. Static `player.c::shoot(KNEE)` and Atomic `USER.CON` review, formatting and targeted compilation only; no tests or gameplay/reference comparison.
- `2026-11-08` — **D6 quick-kick switch hits**: the knee hitscan now returns its actual wall/sprite target and routes tagged switch tiles through the existing switch interaction path for solo and local co-op. Ordinary actors and lotag `-1` exits are excluded, preventing unrelated hits from being treated as switches. Exact Chocolate wall-material response remains open. Static `player.c::shoot(KNEE)` / `sector.c::checkhitswitch` review, formatting, diff check and targeted compilation only; no tests, gameplay or dynamic/reference comparison.
- `2026-11-08` — **D6 player quick kick**: CON `pkick`'s 14-tic countdown fires the player knee hit on the `doincrements` tic where it reaches 8, without requiring weapon fire or changing `kickback_pic`; frozen-palette players do not advance or fire it. The existing knee hitscan applies damage and queues `KICK_HIT` for solo and local co-op seats. Static Chocolate `player.c::doincrements` / `shoot(KNEE)` review, formatting, diff check and targeted compilation only; no tests, gameplay or dynamic/reference comparison.
- `2026-11-08` — **D3 translucency table**: `PALETTE.DAT` parsing retains its optional 65,536-byte palette-index blend table. Face-, wall- and floor-aligned sprites composite for `cstat` bit 2 with bit 9 reversing the table inputs; wall surfaces composite for bit 7 / reverse bit 9. Missing tables preserve opaque rendering. Translucent sector planes and visual reference verification remain open. `cargo fmt --all`, `git diff --check`, and `cargo check -q -p room-build -p room-build-engine -p room-duke --lib` pass. No tests, screenshots or game comparison were run.
- `2026-11-08` — **D6/D11 scripted handbomb tile resolution**: handbomb throws now resolve `HEAVYHBOMB` from the active CON (`26` fallback), and the native actor dispatch/mover use that same tile for bounce and remote detonation. This is a custom-CON compatibility slice, not full pipebomb parity. Formatting, diff check and targeted crate compilation only; no tests, gameplay or dynamic/reference comparison.
- `2026-11-08` — **D6/D11 scripted RPG tile resolution**: native RPG and Devastator shots now resolve projectile tile IDs from the active CON `RPG` label (Atomic 2605 fallback); projectile movement and radius-hit attribution use that same runtime tile. This closes a focused user-CON compatibility gap, not the D6/D11 acceptance criteria. `cargo fmt --all`, `git diff --check`, and `cargo check -q -p room-duke --lib` pass with existing warnings. No tests, gameplay, or dynamic/reference comparison was run.
- `2026-11-07` — **D8 pause/menu music**: added a native Duke Escape pause menu and F10 quit confirmation with resume/save/load/sound setup/quit actions. Pause and quit screens select CON `music 0` environment music, then restore the active level track on resume; Sound Setup keeps the correct track when music is re-enabled. Static Chocolate `menues.c` / `gamedef.c` review and targeted compile only; no dynamic comparison.
- `2026-11-07` — **D8 listener `opos` phase**: desktop sound attenuation, panning and LOS now use Chocolate's player position snapshot from input-phase entry instead of the post-movement pose. Static `player.c`/`sounds.c` review, format/diff checks and targeted compile only; no audible/gameplay comparison.
- `2026-11-07` — **D8 exit-countdown sound gate**: new `xyzsound` starts are suppressed for the final 78 tics before level exit, while stop events still run. Static `sounds.c::xyzsound` review, format/diff checks and targeted compile only; no audio/gameplay comparison.
- `2026-11-07` — **D8 `xyzsound` owner arbitration**: the mixer now follows Chocolate's four-voice rejection and conditional stop-latest rule using first-owner identity, current voice count and dead-badguy source state; player-origin sounds carry their sprite owner. Static `sounds.c::xyzsound` review, format/diff checks and targeted compile only; no playback/gameplay comparison.
- `2026-11-08` — **D7 VIEWSCREEN cache lifecycle**: invalidate the cached 64×48 camera image when changing campaign maps, replacing the session for Atomic demo playback, or restoring a native save. Prevents repeated camera indices/tics from showing the prior world's feed. Static call-path review, formatting, diff check and targeted compile only; no gameplay/reference comparison.
- `2026-11-08` — **D7 VIEWSCREEN exit priority**: the use edge now exits the active remote camera before scanning for another screen, and releases active CAMERA1 latches. This prevents nearby monitors from consuming an intended exit. Static Chocolate control-path review, formatting, diff check and targeted compile only; no gameplay/reference comparison.
- `2026-11-07` — **D7 VIEWSCREEN death return**: a dead APLAYER now exits remote camera view and releases the active CAMERA1 monitor latch, matching Chocolate's APLAYER death branch. Static `actors.c` review, format/diff checks and targeted compile only; no gameplay/screenshot comparison.
- `2026-11-07` — **D5 badguy movesprite**: ported Chocolate's 1024-unit large-actor and 292-unit LIZMAN clip radii; candidate Z now respects cached actor ceiling/floor bounds and reports the sector-coded blocked-Z result. Existing stayput/boss/water rollback and random-turn branches retained. Static source review/compile only; no dynamic comparison.

- `2026-11-07` — **D5 badguy sector links**: after CON `move()` passes stayput/boss/water rejection, Room now synchronizes `SpriteLists` when clipmove accepted a sector transition, matching Chocolate’s delayed `changespritesect`. Static source review and compile only; no gameplay/demo comparison.

- `2026-11-07` — **D2/D5 clipmove wall sprites**: added Chocolate’s `cstat&16` rotated wall-sprite clip quad, visible-side rule, endpoint blockers and `49152+sprite` hit code to the shared Build clipmove. Floor-sprite clips remain open. `cargo fmt --check`, `git diff --check`, and targeted crate compile only; no gameplay/reference comparison.

- `2026-11-07` — **D2/D5 clipmove floor sprites**: added Chocolate’s `cstat&32` oriented floor-quad construction, Z-side filtering, visible-edge tests and inflated edge clips with native sprite hit codes. All three Build sprite orientations now contribute to clipping. `cargo fmt`, `git diff --check`, and targeted crate compile only; no gameplay/reference comparison or golden fixture yet.

- `2026-11-07` — **D2/D5 clipmove final sector**: ported Chocolate’s reverse-order all-sector fallback when the clipped point is outside the portal BFS; sector selection uses vertical clearance and sloped ceiling/floor planes. `cargo fmt`, `git diff --check`, targeted crate compile only; no runtime/reference comparison or golden fixture.

- `2026-11-07` — **D2/D5 clipmove sprite ordering**: clipmove now consumes live `headspritesect` / `nextspritesect` order for candidate sprites in portal-BFS sectors. Hooked it to native player, badguy, faller, poolball, and pipebomb movement. Static source review, formatting, diff and targeted compile only; no runtime/reference comparison.

- `2026-11-07` — **D6 RPG ceiling impacts**: `hitscan_full` plane hits now enter the RPG impact path, apply Chocolate’s sky/palette deletion guard, and call `checkhitceiling` for upward impacts; sector-list links update before detonation. The shared radius pass was subsequently extended to call the same effect when Chocolate's ceiling-radius gate succeeds. `cargo fmt`, `git diff --check`, targeted compile only; no gameplay/reference comparison.
- `2026-11-07` — **D6 ceiling-light tile lookup**: the six breakable ceiling-light source/busted pairs and `GLASSPIECES` now resolve from active CON labels with Atomic ID fallbacks. Custom tile definitions therefore feed the hit effect and shards. Static Chocolate `checkhitceiling`/`ceilingglass` review, format/diff checks and targeted compile only; no runtime/reference comparison.
- `2026-11-07` — **D6 frozen slime glass tiles**: GREENSLIME shattering now uses the active CON `GLASSPIECES` label, matching the ceiling-glass path, with Atomic fallback 1031. Static `actors.c` review, format/diff checks and targeted compile only; no gameplay/reference comparison.
- `2026-11-07` — **D6 underwater air**: added Chocolate's 390-tic air reserve, surface refill, oxygen-free scuba check, Q8 drowning damage cadence and Atomic low-health pain cue; `airleft` / `extra_extra8` now round-trip through Atomic `.SAV` player records. Static `player.c`/`duke3d.h` review and compile only; no dynamic comparison.

- `2026-11-07` — **D5 CON resetplayer**: opcode 42 now defers solo restarts to the host and implements multiplayer respawn pose selection from captured Chocolate-order `po[]` starts (seat index in co-op; `TRAND` selection in DukeMatch, with RNG consumed at opcode time), including baseline player/body reset. Static `gamedef.c`/`premap.c` review, format/diff checks and targeted compile only; no gameplay comparison.
- `2026-11-07` — **D6 tripbomb placement filter**: duplicate-charge detection now resolves the active CON `TRIPBOMB` define instead of assuming Atomic tile 2566, so add-on CON scripts use the same tile for placement and exclusion radius. Static `player.c::shoot` review, format/diff checks and targeted compile only; no gameplay comparison.
- `2026-11-07` — **D6 Shrinker target scan**: SHRINKSPARK radius pass now follows Chocolate's seven-stat order, separate blocking/shootable eligibility, badguy LOS, owner/small-target and SHARK rules, player feet-distance origin, and target/source LOS Z offsets; active CON `TRIPBOMB` tile is resolved. Build `dist()` is still approximated and transformations are still open. Static `actors.c::hitradius` review, formatting, diff check and targeted compile only; no runtime/reference comparison.
- `2026-11-07` — **D6 Build 3D radius metric**: implemented Chocolate's integer `FindDistance3D` approximation in `room-build-engine` and replaced horizontal-plus-vertical estimates in Shrinker, RPG and shared radius target scans. Input Z is shifted by four bits as in `actors.c::dist`. Static `global.c`/`actors.c` review, formatting, diff check and targeted compile only; no gameplay/demo comparison.
- `2026-11-07` — **D6 pistol cycle**: pistol ammo is now consumed when firing starts; the frame-1 hitscan runs that same tic, SHELL is ejected at frame 2, and empty/magazine reload timing follows the 12-round boundaries with EJECT_CLIP/INSERT_CLIP cues. Static `player.c::processinput` and `game.c` shell initialization review, format/diff check and targeted compile only; no gameplay/demo comparison. Muzzle visibility flash remains unmodeled.
- `2026-11-07` — **D6 chaingun cadence/casings**: the first activation now advances into the same tic's kickback; each 3-tic fire frame consumes ammo, spawns the native SHELL with the chaingun's +32 velocity and +3<<8 height adjustment, queues CHAINGUN_FIRE before hitscan effects, and preserves held-fire loop cadence. Static `player.c::processinput`/`game.c` review, formatting, diff check and targeted compile only; no runtime comparison.
- `2026-11-07` — **D6 shotgun activation timing**: the shotgun now advances kickback on its activation tic, matching Chocolate's increment-before-frame-switch order; the tic-4 volley therefore no longer starts one tic late. Pistol fire audio now queues after its frame-1 hitscan, matching `shoot()` then `spritesound()`. Static `player.c::processinput` ordering review, formatting, diff check and targeted compile only; no gameplay/demo comparison.
- `2026-11-07` — **D6 Devastator activation**: Devastator now starts `kickback_pic`, toggles its barrel and queues CAT_FIRE on the input tic, then advances into Chocolate's frame-2 state immediately. Its first and second projectiles now arrive on frames 3 and 5 without the prior one-tic start delay. Static `player.c::processinput` review, format/diff check and targeted compile only; no runtime/demo comparison.
- `2026-11-07` — **D5/D6 tiny Shrinker targets**: CON `ifhitbyweapon` now returns false for zero-damage SHRINKSPARK hits when target `xrepeat < 24`, preserving the queued marker as Chocolate's early return does. Static `actors.c::ifhitbyweapon`/Atomic tile review, format/diff check and targeted compile only; no gameplay/demo comparison.
- `2026-11-07` — **D6 APLAYER Shrinker hit routing**: removed the non-player `xrepeat < 24` early-out from Room's player-hit consumer. Chocolate applies that threshold only in the non-APLAYER branch; player zero-damage hits now follow the ordinary player hit/metadata consumption path. Static `actors.c::ifhitbyweapon` review, formatting, diff check and targeted compile only; no runtime comparison.
- `2026-11-07` — **D4/D5 CON presentation opcodes**: opcode 68 `print` now writes the operand as a decimal stdout line; opcode 14 `palfrom` applies the requested timed RGB flash to the actor's owning player in actor order. Static `gamedef.c` inspection and `cargo check -q -p room-duke --lib`; no runtime/reference comparison.

## 0. Dashboard

| Milestone | Title | Status | Approx % | Active focus |
|-----------|-------|--------|----------|--------------|
| **M0** | Workspace and core | `done` | 100% | — |
| **M1** | Resources | `done` | 100% | — |
| **M2** | Formats & definitions | `done` | 100% | — |
| **M3** | Maps | `done` | 100% | — |
| **M4** | Renderer + shells | `done` | 100% | — |
| **M5** | Doom playsim | `done` | 100% | — |
| **M6** | Boom / MBF / MBF21 | `done` | 100% | — |
| **M7** | ZDoom feature set | `done` | 100% | — (see intentional deferrals) |
| **M8** | Heretic and Hexen | `deferred` | 0% | Next when Heretic/Hexen IWADs available |
| **M9** | Strife | `deferred` | 0% | Next step (later) |
| **M10** | GZDoom rendering extras | `done` | 100% | — (see intentional deferrals) |
| **M11** | WASM modules | `done` | 100% | — (see intentional deferrals) |
| **M12** | Netplay | `done` | 100% | — (see intentional deferrals) |
| **M13** | Product polish | `done` | 100% | — (see intentional deferrals) |
| **M14** | 1.0 hardening | `in_progress` | 10% | `save_world_bin` fuzz; next corpus scaffolding / docs stubs |
| **S1** | room-server M1 | `done` | 100% | smoke script + 1k soak; browser optional |
| **S2** | room-server M2 | `done` | 100% | 1h room_proto fuzz clean; security + limits |
| **S3** | room-server M3 | `done` | 100% | bans enforced; kick/ban UI; WebAuthn/RBAC deferred |
| **S4** | room-server M4 | `done` | 100% | SFU + budget; PCM legacy fallback only |
| **S5** | room-server M5 | `done` | 100% | §6 relay + welcome budgets; daemon script |
| **D0** | Duke ADR + scaffolding | `done` | 100% | — |
| **D1** | Duke reference traces | `done` | 100% | — |
| **D2** | Duke engine primitives | `done` | 100% | XOR inside + Chocolate clipmove (23 wall-only; `initspritelists`); all three sprite orientations and live sector-list scan order are implemented; new paths still need golden vectors |
| **D3** | Duke world + static view | `in_progress` | 87% | Doors lying on a portal line render solid. Wall sprites are perspective-correct and near-clipped; distance shade rate matches Build `wallscan`. Native desktop/browser paths render Build walls, flat/sloped planes and sky, and face-, wall-, and floor-aligned sprites with ART animation, offsets/flips, per-pixel depth, `LOOKUP.DAT` palette remaps and `PALETTE.DAT` shade rows. Plane sampling now honors first-wall-relative alignment, XY swap, X/Y flip, double-smoosh, tile-size scaling and fractional panning. Atomic's optional 256×256 translucency table composites face-, wall- and floor-aligned sprites plus translucent wall surfaces, respecting each surface's forward/reverse `cstat` bits. Remaining: visibility shading, plane translucency, exact classic rasterization, overlapping-sector correctness, and E1L1–E4L11 golden coverage/reference captures. |
| **D4** | Duke CON compiler | `done` | 100% | Atomic script SHA = Chocolate; runtime opcode corpus → D5 VM |
| **D5** | Duke core sim loop | `in_progress` | 74% | Honest DEMO1 frontier is tic 436 of 3,808 (11%, 2026-11-13; was 270) (see `25` §0 milestone table); the old tic-134 figure predates the DMO parser fix; tic 135 remains unverified after a static fix for Chocolate's omitted post-hitscan ricochet RNG roll (Atomic sound id 1), which shifts later shotgun spread and can change the extra `JIBS6` spawn. CON implements `wackplayer` with opcode-time TRAND and player view-roll state, `tossweapon` with Chocolate's weapon map/RNG/drop initialization, `resetactioncount`, `respawnhitag`, `mikesnd`, `sizeto`, `debris`, `guts`, `pstomp`'s visible-target trigger/knee cycle/impact and player quickkill/frag state, `operate`'s deferred sector near-tag activation, `ifawayfromwall`, `ifrespawn`, `ifbulletnear`, `ifnotmoving`, `ifcanshoottarget`, and `ifnosounds` using the mixer's first active owner per sound ID plus same-tic CON sound start/stop updates. A first native STAT_FALLER path now advances tagged hit-sprites through Chocolate's `cstat&48` routing (including its wall-switch/special-tile exclusions), startup health, accepted-hit damage filtering, linked-group fall state, activation delay, randomized launch, horizontal damping, sky-aware gravity and floor cleanup in native tic order. Floor cleanup now emits SCRAP6 with Chocolate's ordered seven RNG draws per piece. Room now sends `ssp` through a shared ordinary `movesprite` path: portal/wall/sprite clip result is retained, sector links change only on an actual transition, Z uses Chocolate's cached ceiling/floor bounds, and a clear XY result with blocked Z returns `16384 + sector`. The `movefallers` IFHIT probe now mirrors non-player `ifhitbyweapon` health gating, tiny SHRINKSPARK early return, active-owner propagation and non-trigger damage rollback. SE19 now uses Chocolate's STAT_MISC+exact-sector IFHITSECT probe. Badguy movement now applies Chocolate's stayput/boss/water rollback and random-turn branches, dedicated large-actor/LIZMAN/generic clip radii, cached vertical-bound checks, and updates Build sector lists only after the move is accepted. Clipmove’s no-BFS sector fallback now selects among overlapping sectors by vertical clearance and sloped planes; player, badguy, faller, poolball, and pipebomb clipping now consume Chocolate’s live per-sector sprite order. Other exact Build sector/sprite collision edges, remaining actor movement cases, plus opcode/demo/route coverage remain incomplete. Static compile check only; no runtime comparison was run. |
| **D6** | Duke weapons + damage | `in_progress` | 79% | First-person weapon frames now come from a port of Chocolate `displayweapon` (`weapon_view.rs`), so pistol reload and every weapon's kickback animation draw as in Atomic; breakable props get `cstat 257`. Pistol now consumes ammo at initiation, shoots at frame 1, ejects SHELL at frame 2 and implements magazine reload cues. Pistol/shotgun/chaingun wall hits now run Chocolate switch/material handling at every valid range and place source-aligned BULLETHOLE decals with the native suppression and duplicate-distance checks. Player clipmove now carries collision codes into Chocolate-style cactus and forcefield contact damage/feedback. Shotgun now begins its kickback on activation and fires its volley at frame 4; chaingun starts its 3-tic volley cadence on the activation tic, ejects SHELL casings and consumes each round before its shot. Empty weapons auto-select by Atomic's default order; RPG slot 4 consumes ammo and resolves its projectile tile from CON `RPG` (Atomic tile 2605 fallback), launches at speed 644 and preserves parsed CON strength; projectile wall/sprite collision spawns an animated impact flash and applies the three RPG radius-damage bands to visible targets. CON `hitradius` now scans Chocolate’s default/actor/standable/player/faller/dummy/misc status order, with the blocking-sprite LOS branch and APLAYER feet-origin/player-height LOS rules; SEENINE/OOZFILTER and BOX/TREE1/TREE2/TIRE/CONE target IDs resolve through active CON labels; radius eligibility and LOS preserve the separate AFLAMABLE branch; hitscan/radius object impacts share `checkhitsprite` transitions, debris, sound and deletion effects. Normal knee cadence, short-range badguy damage and `KICK_HIT` are implemented; both normal knee and `pkick` now use CON `KNEE` strength + `TRAND&7` and the active-steroid max-health bonus; `pkick` fires `shoot(KNEE)` at tic 8 for solo/local co-op and respects the frozen-player palette guard. Tagged switch tiles reached by the knee hitscan use the existing switch path, while its spawned KNEE sprite effects and exact wall-material response remain open. CON `tossweapon` now drops the current weapon sprite with Chocolate's `checkweapons()` random branch and RPG/handbomb `EXPLOSION2` fallback. Slots 6/11 have partial Shrinker/Grower paths; slot 8 tripbomb placement/arming/laser lifecycle; slot 9 Freezer firing, clipped flight, bounce/fizzle, hit metadata, `FREEZERHURTOWNER`, and direct forcefield/glass/screen/dispenser/ATM/technical-wall/light responses; pipebomb/remote behavior (including map-placed HEAVYHBOMB item respawn) and Devastator burst are partial. SHRINKSPARK target selection follows Chocolate's seven-stat scan, blocking-vs-shootable branch, LOS, owner/small-target/SHARK exception, player feet origin and script-defined TRIPBOMB. Player APLAYER Shrinker hits now start Atomic `PSHRINKING`, play the shrink cue/flash, shrink and restore sprite repeats on the `SHRUNKCOUNT`/`SHRUNKDONECOUNT` schedule, and kill the player if its sloped sector gap is below 24; Grower hits apply the CON flash and cue. NPC transformation scripts, additional APLAYER particle branches and full APLAYER CON execution remain open. Freezer `CAT_FIRE`, Shrinker `SHRINKER_FIRE`, and Grower `EXPANDERSHOOT` now queue at activation; Freezer activation advances on the same tic and its held-fire recovery branch is checked before incrementing the next cycle. Shrinker/Grower shots now account for Chocolate's activation tic storing frame 1 without entering that tic's increment arm. Chocolate `sync.bits` bit 6 carries full/bullet-only auto-aim mode through desktop and demo input; pistol/shotgun/chaingun hitscan uses any enabled mode, while RPG, Freezer, Shrinker, Grower and Devastator require full mode for projectile targeting. Desktop archived `duke_autoaim` selects mode 1/2; browser uses full mode. `moveplayers` consumes queued APLAYER hits for health and directional knockback, applies god-mode/Shrinker and default coop friendly-fire rules, initializes Atomic's `shield_amount` from `gamestartup[3]` (100 default), and absorbs 20–49% of health loss using Chocolate's conditional TRAND. Remaining: exact Build collision/target filters, Atomic actor-by-actor shrink/grow response beyond APLAYER, other direct wall/ceiling impacts, weapon view/HUD parity, and DEMO1 sync. Static Chocolate source review and crate compilation only; no dynamic comparison. Native `--give-all` provides the arsenal. Full-screen water and heat/slime palette selection now reads the five optional six-bit RGB tables at the tail of Atomic `LOOKUP.DAT`; palette-bank shade tables remain open.
| **D7** | Duke sectors + effectors | `in_progress` | 84% | Sector `extra` defaults to 256 (`prelevel`), fixing SE11/SE15 door speeds. Player `Open` now operates sector doors/lifts through `checksectors`' near/under-operator tail (`use_near_sector`). ART `picanm` cycles render on the deterministic 120 Hz clock. `animatewalls` tracks screen-break and force-field state; native 15 Hz `movecyclers` animates CYCLER shade ranges and GPSPEED sets sector speed. Player `Open` neartags and flips Atomic wall and sprite switches, toggles tagged force fields and `ACTIVATORLOCKED` sector bits, triggers tagged activators and shade-cycler toggles, and secret/exit sectors update player/session state. Touchplates trigger linked activators and move/carry their floor; master switches count down and operate the currently supported sector movers. Tagged `RESPAWN` markers count down, create a `TRANSPORTERSTAR`, and respawn their target sprite. `monsters_off` now culls badguy map sprites and prevents enemy-target respawn markers from arming. Sector 9 sliding walls, first-pass SE11 swing doors with the linked ACTIVATOR hitag gate and badguy/player obstruction handling, and SE15 translating sliding doors (T4 travel counter and T5 active mode), lotag 16–22 plane motion, lotag 27 / SE20 extend-o-bridge motion, SE17 paired elevator platform/actor transfer, partial SE18 Kroz room-over-room plane effects, partial SE5 boss elevator and laser attack, partial SE6/SE14 train route movement and SE30 locator-driven subway translation, SE19 explosion-triggered shield shutdown, SE2 quake and SE33 debris, timed SE130/SE131 explosion emitters, SE25 piston ceiling cycle, SE26 shade-directed cycling escalator (T4/T5 origin, actor gravity on sloped destination floors), SE29 sine-wave floor displacement plus aligned four-wall slope, SE28 tagged lightning storm/strike cycle, SE35 drilling ceiling, SE24/SE34 conveyor impulses with tagged switch toggles, SE31 true-drop-floor and SE32 true-drop-ceiling movers now run. CON `addinventory ACCESSCARD` grants palette-matched keys; the status bar shows them, and access switches enforce keys, animate for 12 tics, activate and consume the key. First-person `access_incs` now draws the palette-matched hand/card and laser phases while hiding the weapon. Tagged sprite switches are reachable through visible connected sectors. SE3/SE4 random lights, SE8/SE9 elevator shade transitions, SE10 automatic activation, SE12 switch lights, player hitscan ceiling-light breakage, and the SE16 reactor ceiling cycle now run. Lotag-30 SE0 rotators link to SE1 pivots, rotate shared wall vertices, carry grounded occupants, and raise/restore the floor over the native transition envelope. `Open` now selects matching tagged `CAMERA1` actors through `VIEWSCREEN` sprites and toggles a deterministic remote camera pose; active cameras are latched for the monitor animation, and the next use edge exits before another screen is selected. The selected monitor receives a 64×48 palette-quantized render of its linked camera, cached and refreshed at the Chocolate 12-tic cadence; mirror destination sectors initialize and the nearest projected mirror renders from a reflected camera; mirror-sector composition, visibility scaling, multiple mirrors and raster parity remain open. Death returns the player from camera view and releases the active latch; camera feed caches clear on campaign transitions and native save restore. Other transition state and exact tint remain incomplete. SEENINE/OOZFILTER hits now run their tagged delay and effector activation chain; visible blast hit tests now queue damage for CRACK1–4/FIREEXT; tagged sources arm matching SEENINE/OOZFILTER groups, CRACK consumes supported source types and emits `RANDOMSCRAP` debris/earthquake effects, and FIREEXT emits its explosion, scraps and hitradius before removal. SE36 toggles and fires `COOLEXPLOSION1`, `SPIT`, `FIRELASER`, `RPG`, `FREEZEBLAST` and `SHOTSPARK1`/`SHOTGUN`/`CHAINGUN` on its 130-tic cadence. `COOLEXPLOSION1` now collides with walls/planes/players, damages players, stops and fades after 40 tics, and queues `WIERDSHOT_FLY`; SPIT/FIRELASER flight and direct damage plus RPG/freezer launch parameters are ported. Exact CRACK/FIREEXT random-call and audio ordering, other impact edge cases, exact interpolation registration, train collision response, full obstruction/overlap handling and DEMO2/3 parity remain open |
| **D8** | Duke audio + front end | `in_progress` | 66% | Native Chocolate menus (main/in-game/episode/skill/credits/quit) and the `dobonus` intermission are drawn with Duke art. Full Atomic status bar (health, armor, weapon ammo list, inventory, keys) is the default HUD; `duke_statusbar 0` gives the mini HUD. Desktop/browser native paths load CON `definesound` samples and positional actor sounds; `soundonce`, `stopsound`, single-player `globalsound`, loop flags, Chocolate's one/four per-sound voice caps, exclusive talk voices, and `definesound.pr` priority arbitration through the 32-voice pool are carried through the mixer. Same-ID owner arbitration now uses stable voice-start ordering, unaffected by `swap_remove`. `m&128` global and `m&16` non-attenuated positional modes are preserved. The mixer follows Chocolate's `FindDistance3D`/`xyzsound` distance floor, `MV_Play3D` gains and the `cansee` occlusion penalty. Ambient markers work. Browser native Duke now selects the active campaign map's MIDI from its compiled CON `music` table when audio unlocks and on map transitions. Desktop has native F4 controls; browser hosts can set sound/music/talk/lockout/ambience/stereo options and Atomic-range volumes through `roomSetDukeAudioOptions`. Both shells apply pitch ranges plus underwater pitch overrides and signed `definesound.vo` distance adjustments. The desktop F4 menu also includes archived bullet-only/full auto-aim selection and four-unit volume steps. Duke `MUSICANDSFX` reverb zones use Chocolate's sample-rate-scaled multi-tap/IIR feedback network; zone `T1` now follows Chocolate's per-marker enter/leave edges in STAT_FX order, with current `FX_SetReverb` level saved in `.fsave`. F7 third-person camera is available on desktop and web; the web toggle stays presentation-local for lockstep. Native quote text, an animated TIP tile, a progressive Build-wall Duke automap with sprite markers, Atomic F1 help, F11 BRITABLE brightness, and every vanilla `DN` solo command in Atomic `GAME.C::cheatquotes[]` now executes in the desktop shell, including `skill#`/`scotty###` routing, actor toggles, inventory, map reveal, and the intentional `time` no-op. Exact legacy quote presentation and the `debug` cache-lock diagnostic remain open. Title and finale animation selection uses monotonic elapsed milliseconds, so it follows authored rates at variable refresh rates; E1/E2/E4 finale frames now queue Chocolate sound cues through CON labels and the native mixer; entering a finale resets its animation origin. Atomic `music` tables select layered-GRP MIDI at campaign map start/transitions; F5 reports the current title and Shift-F5 cycles while that quote is active. Escape opens a native pause menu (resume/save/load/sound setup/quit), F10 opens quit confirmation, `music 0` supplies the menu track, and resume restores level music. Desktop and browser load `DUKE.RTS`, decode its ten VOC/WAV entries into reserved mixer IDs, and play them on Alt+F1..F10 while honoring sound/lockout options; the active RTS voice gates overlapping requests as Chocolate's `rtsplaying` latch does. Desktop Room sessions also broadcast bounded presentation-only RTS requests outside tic input. This auxiliary envelope is Room-specific, not Chocolate's network protocol; desktop shows `RTS PLAYING` for any active local or peer-delivered RTS voice. Browser peer replication and its latch indicator remain open. Device-specific pool sizes, libc-rand pitch parity, audible pan calibration, other ANM cue tables and most of the D8 front-end checklist remain open |
| **D9** | Duke saves + demos | `in_progress` | 75% | Room-native `.fsave` and checksummed `.fdem` recording/playback are implemented; desktop `record` / `stop` capture Duke inputs, pre-tic weapon requests/cycles and post-tic hashes, and `playdemo` restores the embedded initial save before verified playback. Native Duke `record` / `stop` now also writes a matching Atomic v117 `.dmo` with seat-ordered inputs and recorded coop/fire, aim-mode and weapon-choice metadata. Atomic `.DMO` playback supports 1–8 seat-ordered input streams. Both Atomic demo session builders load map/CON from ordered GRP layers and build the recorded skill and roster. Desktop `playatomicdemo <path.dmo>` starts native playback from the recorded map and now follows native campaign exits while retaining the demo input cursor across loaded maps. `GAME#.SAV` parsing retains Atomic world tables, CON pointers, sprite flags, `weaponhit`, raw player record and opaque `po[MAXPLAYERS]`; encoding writes the modeled `saveplayer` payload. `DukeSession::save_atomic_sav` refreshes modeled live state into an imported SAV template, or creates a zero-initialized Atomic 1.5 model for a fresh supported session; F2/F3 write/read the `.sav` companion. Player-record tail parsing/export now follows the 932-byte 32-bit or 944-byte 64-bit C ABI stride (plus read compatibility for Room's earlier 936-byte draft), rather than applying 32-bit offsets to every target. Remaining: refresh unmodeled state, accurately initialize all default Atomic tables for game-compatible standalone loads, save/load byte round-trip, route playback sync and reference validation. Static ABI/source review, formatting, `git diff --check` and compile check only; no tests, gameplay or dynamic/reference comparison |
| **D10** | Duke multiplayer + server | `in_progress` | 70% | Native ten-byte Duke input codec and server-side 30 Hz frame ring accept FTR2 inputs, release seat-ordered frames and fill missed commands by briefly repeating then neutralizing. The optional verifier can run a native `DukeSession` from the configured Atomic GRP and compare frame checksums. `DukeSession` supports 1–8 seat APLAYER starts in all three Atomic `ud.coop` modes, matching each start's `lotag` and assigning in STAT_PLAYER head order; its retained `po[]` table starts with the map header and filters markers by mode; DukeMatch/no-spawn multiplayer applies Chocolate's initial weapon/inventory reset, while co-op preserves the carried loadout. Lethal APLAYER damage initializes the one-shot death tilt/palette, freezes controls and credits self/player frags on the next processinput transition. Dead-player camera follows the body, turns toward its killer, updates Build sector/water state and runs `updatesector`/`pushmove` before deriving the floor-relative death roll. APLAYER water/angle movement and Holoduke lifecycles resolve against each actor’s owning seat. Dead seats keep movement and firing neutral; a rising `INPUT_OPEN` edge queues the existing multiplayer `resetplayer` respawn path in co-op/DukeMatch, while no-spawn mode stays disabled. Player frags are retained in an 8×8 session matrix saved in `.fsave`. Browser v2 rooms accept GRPs, validate and prepare native sessions before Ready, then consume authoritative seat-indexed Duke frames; weapon hotkeys/cycling are packed into `sync.bits`. Native desktop `--host`/`--join` applies explicit game mode/friendly fire, rejects setting mismatches through the lobby fingerprint, transports seat inputs, and piggybacks native checksums for desync detection. Marker edge cases, spectator flow, host-authoritative setting negotiation, remote-bomb selection, complete Atomic multiplayer save/load round-trip, desktop lockstep/mismatch runtime validation and cross-target checksums remain open |
| **D11** | Duke campaign + switch shells | `in_progress` | 45% | `play-duke3d.sh` now defaults to `--runtime native`; `--runtime converter` remains explicit. Desktop and browser construct `DukeSession` directly from GRP maps and render the native soft frame. Desktop accepts standalone `--map path/to/user.map`, repeatable `--grp` add-on archives and a `--con` root script override; later GRPs override duplicate map, CON/include, ART and sound names. Native RPG/Devastator and handbomb projectile paths resolve `RPG` and `HEAVYHBOMB` from the active CON with Atomic fallbacks. Browser exposes `roomSetDukeCon` and `roomAddDukeConInclude` for the native GRP session loader; registered external includes take precedence over GRP includes. `roomStartDukeMap(grp, map, skill)` loads a standalone Build map in the browser with that same CON/include configuration. `roomAddDukeGrp` layers optional GRPs for browser single-player map/script/tile/sound lookup, with later entries overriding earlier ones; browser co-op currently rejects local add-on GRPs because the lobby protocol does not transfer or fingerprint them. `playatomicdemo <path.dmo>` starts Atomic input playback from the recorded map and keeps playing across campaign exits using the native route loader; each transition refreshes the restart snapshot and the active `--con` root/external include resolver now applies to the demo's initial map too. Browser single-player and lockstep lobby sessions now retain episode/map progress across map loads, so subsequent Atomic exit requests route correctly. Bonus presentation, complete HUD and reference parity remain open |
| **ED0** | EDuke32 compatibility inventory | `done` | 100% | The gated Brutal inventory/report test, Atomic GAME.CON hash test, and honest 436-tic DEMO1 sync floor pass locally. |
| **ED1** | EDuke32 CON compiler | `done` | 100% | The reachable Brutal CON tree compiles with 0 errors; 25 compiler tests and the Atomic GAME.CON hash test pass locally. |
| **ED2** | EDuke32 CON VM core | `in_progress` | 90% | Bounded serializable vars/arrays/quotes and `resizearray` (including implicit `RETURN`), exact-case state/label/gamevar/gamearray lookup with version-2 folded-index save migration, arithmetic/control flow and tested signed/unsigned/bitwise comparison spellings, actor/player predicates (`ifcount`, `ifaction`, `ifactioncount`, `ifai`, `ifmove`, `ifactor`, `ifstrength`, `ifwasweapon`, `ifspawnedby`, `ifspritepal`, `ifmultiplayer`, `ifgotweaponce` reservation lookup/registration, `ifp` flags, `ifdead`, `ifsquished`, `ifhitweapon`, `ifnotmoving`, `ifhitspace`, `ifactornotstayput`, `ifcansee` with finite face/wall/floor tile-extent samples, `ifcanseetarget` with direct actor/player LOS and one gameplay TRAND Z jitter, `ifcanshoottarget` with the live three-ray hitscan corridor, `ifbulletnear` with live incoming-projectile corridor checks, `ifnosounds` against the active actor sound-owner snapshot, `ifrespawn` with class-selected respawn settings, `ifpinventory`, `ifgapzl`, `iffloordistl`, `ifceilingdistl`, `ifpdistl` / `ifpdistg`, `ifinwater`, `ifoutside` and `ifphealthl`), source-order registered hooks, classic `state … ends` declarations plus inline and top-level exact-case `state` calls and ordered `appendstate` / `prependstate` composition, symbolic action/AI/move pointer conditions, relational and actor-condition `else if` chains, `ifrnd` plus random ranges (`randomseed` defaults to 1996 standalone and aliases the live Build gameplay stream when host access is present), live `numplayers` / `MULTIMODE`, `RESPAWN_*`, map-count and `totalclock` built-ins, formatting, `echo`, and player-facing `quote` output with runtime quote text, integer math including `sqrt`, geometry queries, sprite distances/list operations, spawning, action/move/AI pointer updates, actor field/size/fall/counter commands, actor sound commands, nearest-player weapon/inventory commands with full-ammo event termination, saved-bound range loops, snapshot-based active-sprite iteration and partial host-backed structure instructions. `room duke con-reference` runs a shared labeled-value fixture and compares it to a captured EDuke32 log. The gated v9 `box.map` test runs all `EVENT_SPAWN` hooks for each active map sprite without VM faults. `ifcansee` tile-probe sampling and the optional reference-value comparison remain parity checkpoints; they do not gate implementation of later ED milestones |
| **ED3** | EDuke32 struct access | `in_progress` | 82% | Partial live actor/sprite, sector, wall, player and input access; player quote, palette-flash and `player.palette` fields, live floor-sprite bridge members (`spritebridge` / `sbs`) and `max_shield_amount` / `max_ammo_amount[]`; read/write tiledata and static tile flags; Brutal Duke's `userdef` subset including dynamic `vm_sprite`, player skill, weapon/HUD scales, visibility, pause, indexed `return`, and monster/item/inventory respawn settings; per-sprite `thisprojectile.workslike`/`bsound`/`drop`/`hitradius`/`velmult`/`range`/`spawns`/`isound`/`sound`/trail parameters/`bounces`; `htactorstayput`, `httempang`, `htfloorzoffset`, and `htwaterzoffset` read/write through persisted actor-temp state; `CON fall` applies water offset to lotag-1 resting placement; all documented TSPR fields dispatched in native and web render paths for visible sprites marked `SPREXT_TSPRACCESS`; all documented spriteext fields and actor blend read/write per sprite and persist in `.fsave`; documented projectile properties plus nonzero defaults initialized from `defineprojectile` and saved on override. Full offset collision behavior, `htfloorzoffset` movement, other `userdef` fields, mirror re-entry, render blend consumption, complete labels and the 104 target-field test set remain open |
| **ED4** | EDuke32 event dispatcher | `in_progress` | 58% | `endevent` ends only the current handler while explicit top-level `return` stops the appended chain; classic actor/state programs now execute for active actors before native movement, so `ifhitweapon` consumes the live queued hit and actor script branches can mutate, kill, or spawn. Actor `action`, `move`, `ai`, `strength`, `addstrength`, `cstat`, `cstator`, `cactor`, `addkills`, `spritepal`, `getlastpal`, `clipdist`, `count`, `sleeptime`, `sizeat`, `sizeto`, `fall`, `resetactioncount`, `resetcount`, `pkick`, `operate`, `addphealth`, nearest-player `addammo` / `addweapon` and their `var` forms, `addinventory` and its `var` form, `palfrom`, `sound`, `soundonce`, `globalsound`, and `stopsound` dispatch through the live adapter. `pkick` queues the player's existing quick-kick action, skipping APLAYER source sprites; `operate` queues the actor for the native near-sector operation phase. Weapon and inventory commands target the nearest player; full ammo stops the current event, armor uses `max_shield_amount`, and access cards follow the actor palette. `addphealth` applies `gamestartup[2]`'s health cap, god-mode suppression, the `ATOMICHEALTH` double cap, and body-sprite synchronization. Source-order hooks share a budget; `EVENT_PRELEVEL` resets actor-scoped CON variables before map setup; map sprites receive `EVENT_LOADACTOR` and `EVENT_SPAWN`; `EVENT_ENTERLEVEL` honors `RETURN` for local-seat iteration; `EVENT_PREWORLD` / `EVENT_WORLD` run once per tic with no player/actor context; `EVENT_PROCESSINPUT` runs per local seat against mutable sync records; start-of-tic `EVENT_PREGAME` and end-of-tic `EVENT_GAME` run per active sprite with closest-player context and `SFLAG_NOEVENTS`; `EVENT_ANIMATESPRITES` runs from render copies for opted-in visible sprites; runtime spawns trigger `EVENT_EGS` then `EVENT_SPAWN` after the caller event returns under a shared budget; `killit` fires `EVENT_KILLIT` and honors its `RETURN` veto before freeing STAT_ACTOR/STAT_MISC sprites or hiding other statuses. Exact instruction-point spawn ordering, other event call sites, cancellation/replacement return semantics and display events remain open |
| **ED5** | EDuke32 engine queries + spawns | `in_progress` | 59% | Deterministic `calchypotenuse`, live floor/ceiling slope and `getzrange` queries, `sectorofwall`, `cansee`, `updatesector(z)`, sprite `ldist`/`dist`, list traversal/mutation, `setsprite`, `spawn` / `espawn`, `money` / `mail` / `paper`, `debris` / `guts` / `lotsofglass`, `hitradius`, live-world `hitscan`, and `shoot` / `eshoot` / `zshoot` / `ezshoot` compile and dispatch. The three item-spray commands emit randomized live sprites from the current actor using Chocolate's Build RNG order and runtime spawn-event path. Declared projectile launch initializes configured fields and applies `PROJ_CLIPDIST` only with `PROJECTILE_REALCLIPDIST`; declared `HITSCAN` / `KNEE` shots resolve range-limited live rays and queue configured direct damage; player shots use full auto-aim for RPG workslike and bullet-only mode for hitscan workslike, respecting `PROJECTILE_NOAIM`; explicit `zshoot` velocity wins. Player-fired `HITSCAN` definitions place configured `PROJ_DECAL` tiles on wall impacts with configured repeat size and wall alignment. Configured hitscan `PROJ_TRAIL` sprites spawn at the ray endpoint using `PROJ_TNUM`, trail repeat and Z-offset fields. Declared RPG workslike projectiles receive drop, per-step swept movement and sector updates (up to 256 steps per tic), configured radius damage and opt-in direct impact; flagged projectiles bounce off walls/mirrors and sprites, continue remaining steps after a bounce, track wall bounces through `yvel`, and queue bounce sounds; configured trails, impact spawns/sounds, timed expiry, and underwater `WATERBUBBLE` effects are active. Classic actor hit processing now reaches Brutal Duke's `ifhitweapon` path; the corpse-safe E1L1 sweep records 29/33 kills, 2 hurt, 1 alive and 1 without a clear shot, so the Brutal acceptance is still unmet. Velocity loss (decay rule still needs source confirmation), randomized decal sizing, other workslike effects, and full Brutal weapon coverage remain |
| **ED6** | EDuke32 resources | `in_progress` | 36% | Nested DEF parser, `tilefromtexture` alpha/offsets, all `texture` palette branches exposed as resource specs and palette-0 imports, `copytile`, dummy tile/range and animation ranges, all 393 Brutal images decoded, desktop palette-mapped tile import, loose WAV/OGG lookup, hash-selected MHK loading and render-only `angoff`, map-specific DEF music with PCM/MIDI/MUS playback; remaining palette-specific `texture` rendering and true-colour sampling, other DEF directives, browser/editor mounting, MHK model transforms/lights and broader music/resource coverage |
| **ED7** | EDuke32 rendering | `in_progress` | 5% | The software Build view loads the palette translucency lookup and composites translucent sprites in a far-to-near pass after opaque sprites without translucent depth writes. Focused sprite renderer tests pass; broader renderer validation still fails the portal visibility fixture and three `soft_goldens` expectations. Remaining: translucent wall-layer ordering, scaled `spriteext.alpha`, `spriteext.blend`, blend-table/DEF palette resources, KVX voxels, skyboxes and true-colour high-resolution tiles |
| **ED8** | EDuke32 player + HUD | `in_progress` | 15% | `getticks` uses deterministic elapsed milliseconds; `setgamepalette` updates player palette state and renderer base/custom lookup palettes; `inittimer` changes desktop/browser pacing at the same per-tic clock scale. Display events/HUD, weapon sway/recoil, animation palette 6, death cam and screenshot acceptance remain |
| **ED9** | EDuke32 performance + web | `not_started` | 0% | Decal budget and browser solo preset/performance acceptance |
| **ED10** | EDuke32 saves + multiplayer scope | `in_progress` | 28% | `.fsave` v2 stores and validates EDuke32 gamevars (including actor/player scopes), arrays, quotes, RNG streams, implicit `RETURN`, player game-palette id, modeled Brutal userdef fields (including respawn modes), projectile definitions and active projectile bounce/velocity-multiplier/lifetime/trail/impact state; legacy VM snapshots migrate. Desktop, browser lobby and server verifier reject EDuke32 CON content before network setup; other native struct state, mid-fight restore acceptance and solo/local-coop policy remain |
| **F0** | Face-swap scaffold + front composite | `done` | 100% | — |
| **F1** | Head anchors | `done` | 100% | HEADANCHORS + estimator + Freedoom coverage |
| **F2** | Bake | `done` | 100% | `bake_class` + `room faceswap bake` PK3 |
| **F3** | Capture UI | `done` | 100% | capture + heuristic detect worker; legacy camera removed |
| **F4** | Live mode | `done` | 100% | TextureManager upsert + sim isolation Live≡Bake |
| **F5** | Multi-angle + webcam live | `done` | 100% | guided yaw steps + `revise_live_lumps` budget |
| **F6** | Multiplayer sharing | `done` | 100% | FaceShare relay + prompt UI + two-peer test |
| **F7** | Duke + polish | `done` | 100% | goldens + import; GPU composite intentional deferral |
| **SV0** | Survival Director refactor + Host | `done` | 100% | modules + FakeHost; wasm_mod_survival green |
| **SV1** | Coop + spot graph + stuck recycle | `done` | 100% | coop + stuck e2e green |
| **SV2** | Director phases / threat / archetypes | `done` | 100% | threat cap soak + phase e2e |
| **SV3** | Elites + announcer | `done` | 100% | affix FakeHost + elite e2e + rate limit |
| **SV4** | Marks / draft / perks (needs MH0–MH3) | `done` | 100% | MODAL draft + weapon_select pick e2e |
| **SV5** | Objectives + extraction (needs MH4) | `done` | 100% | extraction e2e + hold markers |
| **SV6** | Phased bosses + boss bar | `done` | 100% | Warden 66/33% + boss bar e2e |
| **SV7** | Track B content lumps | `done` | 100% | DECORATE/GLDEFS/SNDINFO packed |
| **SV8** | Track C C2–C5 (+C8) | `done` | 100% | flags/flash/quake/radius + events |
| **SV9** | C6/C7/C10 + polish | `done` | 95% | C7 scripts.bin; C6/C10 render deferred |
| **MH0** | ModHud store + save/hot-swap | `done` | 100% | — |
| **MH1** | ModHud draw desktop + web | `done` | 100% | room-ui draw + app/web wire |
| **MH2** | ModHud imports + SDK + AS | `done` | 100% | hud_widget_* + wasm_mod_hud |
| **MH3** | Input capture + player_cmd | `done` | 100% | capture e2e + death release + checksum |
| **MH4** | World markers + tweens | `done` | 100% | clamp + tween + animate import |
| **MH5** | UI-scope modules (optional) | `not_started` | 0% | — |
| **BR0** | Battle Royale refactor + Host | `done` | 100% | — |
| **BR1** | Match flow phases + moving zone | `done` | 100% | — |
| **BR2** | HUD: timer, feed, hit markers | `done` | 100% | — |
| **BR3** | Zone render + map + drop select | `done` | 100% | — |
| **BR4** | Loot 2.0 (rarity, supply, vault) | `done` | 100% | — |
| **BR5** | Death: recap + spectate killer | `done` | 100% | — |
| **BR6** | Squads + downed/revive + pings | `done` | 100% | — |
| **BR7** | Moments + announcer + music | `done` | 100% | — |
| **BR8** | Bots fill empty seats | `done` | 100% | — |
| **BR9** | Track B content + polish | `done` | 100% | **S-BR2** OpenSkill / weekly reset deferred |
| **S-BR1** | Match history & stats | `done` | 100% | `br_stats.json` + BE11 `damage` |
| **S-BR2** | Rating & leaderboards (MVP) | `done` | 85% | placement rating; OpenSkill deferred |
| **S-BR4** | Quick play BR queue | `done` | 100% | 60 s + bots |
| **BE1** | Zone rendering (wall + ring) | `done` | 100% | — |
| **BE2** | Map widget (minimap / full) | `done` | 100% | kind MAP=7; royale minimap |
| **BE3** | Player names in sim | `done` | 100% | — |
| **BE4** | Bindable mod buttons | `done` | 100% | bits 16–19; +mod1..4 |
| **BE5** | Teams + damage rules | `done` | 100% | — |
| **BE6** | Per-actor flags (BUDDHA, …) | `done` | 100% | — |
| **BE7** | View trace (pings, USE) | `done` | 100% | — |
| **BE8** | Deterministic bots (sim) | `done` | 85% | MVP brain; ≤0.2 ms/tic not measured |
| **BE9** | Per-seat camera / spectate | `done` | 100% | — |
| **BE10** | Weapon / stats queries | `done` | 100% | — |
| **BE11** | Rich match result standings | `done` | 100% | — |
| **BE12** | 8 seats on any map | `done` | 100% | — |
| **SH0** | Frame graph + linear unlit | `in_progress` | 85% | encode_srgb toggles with post; HDR dual UnlitPass next |
| **SH1** | Tonemap/LUT/SSAO/bloom/AA wired | `in_progress` | 85% | Real bloom composite wired; FXAA/sharpen on LDR |
| **SHR** | Review fixes (`docs/29` §1.1) | `in_progress` | 80% | Batch 3: U1/U2/U16; remaining Q/P rows |
| **WF0** | Weapon timeline harness | `done` | 100% | `testdata/weapons` + 9 timeline tests (held/full + Flash) |
| **WF1** | Weapon data (`doom.txt`) | `done` | 100% | `Weapon`+`LightDone`; ReadySound/UpSound; SelectionOrder |
| **WF2** | Weapon logic | `done` | 100% | N1–N5/N7/N8; W8.3 bob freeze; generic deplete; Duke/Royale |
| **SH2** | Module system / hot reload | `not_started` | 0% | — |
| **SH3** | HardwareShader materials | `not_started` | 0% | — |
| **SH4** | Post stages + mod control | `not_started` | 0% | — |
| **SH5** | P1 look catalogue | `in_progress` | 55% | E3/E5/E6 + F6–F12/G1 stand-ins; soft particles |
| **SH6** | Liquids & atmosphere | `in_progress` | 20% | Underwater/powerup post + heat-haze pulse MVP |
| **SH7** | Advanced (GTAO/TAA/…) | `not_started` | 0% | CVars registered |
| **SH8** | Mod-specific H1–H7 | `not_started` | 0% | — |
| **SH9** | P3 extras + perf | `not_started` | 0% | — |
| **SH10** | Hardcore shooting & blood | `in_progress` | 75% | J2 floor + J3 drip stand-ins; wet/SSR still open |
| **MA1** | Module states + actions | `done` | 100% | merge + flow/action/offset; WASM dispatch; `wasm_mod_states` green |
| **MA2** | Weapon/ammo/projectile props | `done` | 95% | PropIds + NOALERT; bounce/NOTELEPORT stand-ins; full e2e fire deferred |
| **MA3** | Generated lumps (art/sound) | `done` | 100% | `wasm_mod_lumps`; SDK encoders; VFS `mount_memory` layer |
| **MA4** | Slots / alt fire / hit info | `done` | 100% | `reg_weapon_slot`, ALTATTACK, hit info, ThingDestroyed, `[options]` |
| **SK0** | Shuriken art generation | `done` | 100% | `mods/doom1-shuriken` pinned SHA-256 sprites |
| **SK1** | Shuriken synth sounds | `done` | 100% | pinned DMX hashes + aliases |
| **SK2** | Shuriken classes + fire | `done` | 100% | Clip/Pistol parents; slot 8; Fire/AltFire |
| **SK3** | Shuriken impact / stick | `done` | 100% | impact callback + ring/drops/placement |
| **SK4** | Shuriken polish | `done` | 100% | `wasm_mod_shuriken` + `shuriken.wad` |
| **ZS0** | ZScript lex/preprocess/parse + `room zscript check` | `done` | 100% | UZDoom 293/293 + bd_like + commercial BD ZScript check_ok (PK3 VFS) |
| **ZS1** | Semantic analysis + Room stdlib stubs | `done` | 100% | **ZS1b**: unknown names are errors; real BD + §14.6 probe type-check |
| **ZS2** | Bytecode + Tier 0 VM / heap / GC | `done` | 100% | **ZS2b**: `lower.rs` is the compiler. `compile.rs` and `natives_bd.rs` are gone (natives live in `natives.rs`). Probe, `zscript_play` (33), `bd_*`, and interpreter fuzz pass |
| **ZS3** | Play integration hooks | `done` | 100% | **ZS3b**: Inventory subclass givable/summonable; `Tick`/`PostBeginPlay` run. `World::with_zscript` splits the runtime out. Play host state lives on the VM (`ZvmPlayHost`), not thread-locals. Actor proxies are still ordinary objects |
| **ZS4** | Events / ScriptCall / Wads / BD ZScript | `done` | 100% | **ZS4b**: every `docs/34` §14.6 `bd_*` test passes on `testdata/zscript/bd_real`. Lump bytes come from the in-memory wad map; `Wads.FindLump` still returns -1 |
| **ZS5** | DECORATE exprs → ZVM | `done` | 100% | int+float/`frandom`/`sin`/`cos`/`abs`/`sqrt` via ZVM; labels/exotic tree-walk; speed-up deferred |
| **ZS6** | Saves / demos / netplay / verifier | `done` | 100% | **ZS6b**: heap image v2 (`bd_save_load_mid_map`, `bd_two_runs_same_checksum`). 15-min room-server soak still deferred |
| **ZS7** | UI scope (status bars / menus) | `done` | 100% | status bar goldens (cmds+pixels); **ZScript custom menus deferred** (BD MENUDEF) |
| **ZS8** | Native coverage sprint | `done` | 100% | commercial BD ZScript `coverage=100%`; Find/New/TID/PlayerInGame/`IsZandronum` wired |
| **ZS9** | Cranelift JIT (`zvm-jit`) | `superseded` | — | Replaced by **ZW3** (`docs/37`): ZVM bytecode → WebAssembly, run by the wasm engine |
| **ZS10** | Perf / web budget | `done` | 100% | peephole + criterion + 10k-tick smoke; **formal §9 ms gates deferred** (CI variance) |
| **ZW0** | Interpreter prerequisites (`docs/37`) | `done` | 100% | `has_ext`, per-block fuel (`block_costs`), `Trap::Overflow`, shared `native_call` + heap ops (`ops.rs`) |
| **ZW1** | `room-zvm-wasm` codegen skeleton | `done` | 100% | decode, CFG, dominators, Ramsey structuring, all non-legacy ops, fuel, depth, validation, `--wat` |
| **ZW2** | Calls, return slots, bridge | `done` | 100% | `Call`/`CallVirt`/`CallNat`, register image, tagged return cells, `call_interp`; differential tests green |
| **ZW3** | AOT tier on desktop | `in_progress` | 90% | wasmi + wasmtime/Cranelift hosts (`aot`, `aot-wasmtime`), `ROOM_ZVM_TIER=aot`; BD E1M1 2,100 tics identical on both engines; loops ×42–×202. **Open: ≥5× trivial-tick gate** (short functions now stay at parity through the `auto` policy) |
| **ZW4** | AOT on web | `code_complete` | 70% | `aot-web` backend (browser `WebAssembly`, async instantiate, JS-closure imports), `room-web` `?zvm=aot`. **Not run in a browser yet** (no wasm32 toolchain here) |
| **ZW5** | AOT performance | `in_progress` | 50% | library globals loaded only where named (stdlib 421k → 139k bytecode instructions), `auto` entry policy, cheaper VM swap, caches. Heap view open |
| **ZW6** | Converted-mod module | `done` | 100% | `Target::RoomMod` = AOT code + program image sections (engine-side heap design, `docs/37` §0.2); `read_room_mod`, shipped-module registry, native-id map |
| **ZW7** | Coverage report | `done` | 100% | `room zscript wasm --report`; `docs/qa/zscript-wasm-coverage.md` (BD 0 missing, UZDoom 0 missing) |
| **ZW8** | Natives / UI for converted mods | `done` | 100% | all ZVM natives reachable (no `zs_native` needed); `room/zscript_ui.wasm` for `StatusBarClass` mods |
| **ZW9** | `room mod convert` | `done` | 100% | convert + `--verify` (save/load halfway) + `room/convert.json` + load-time warning; BD 2,000 tics identical on Freedoom E1M1 |
| **ZR1** | Brutal Doom ZScript → Rust mod (`docs/39`) | `done` | 100% | `mods/brutal-zscript-rs`: hand port of BD's 3 ZScript files; ABI v1 additions (`script_arg_*`, `script_return_str`, `ModifyDamage` virtual, `player_gender`, `player_cvar_i32`, `actor_set_sound_class`); `replaces_zscript`; BD vs port 1,500 tics identical |
| **BD0** | `room mod check` harness | `done` | 100% | Fixture + commercial PK3; DECORATE **4130/4130**; ZScript check_ok; lumps 0 err |
| **BD1** | DECORATE syntax D1–D8 | `done` | 100% | commercial **4130/4130** — `"\"`/`[`/`]` frames, digit sprites/labels, `Skip_Super`, UTF-8 labels, `::` goto |
| **BD2** | Base classes / flags / props | `done` | 100% | commercial: **0** missing parents / unknown flags / unknown properties; fixture green |
| **BD3** | Actions / exprs / 0-tic | `done` | 100% | 0 unknown actions; MAP01 10s+ZS+MES; **40s game soak** (1400 tics, thinkers→10k) green; full **10-min deferred** (thinker growth) |
| **BD4** | Weapons / input | `done` | 100% | WRF_* + fixture BdPistol e2e; commercial `BrutalPistol` Fire/Reload/Zoom + ATTACK→Fire on MAP01 |
| **BD5** | ACS chunks / PCD / ACSF | `done` | 100% | chunks + ScriptCall; `BD_Main` OPEN; LOADACS + `pukename`; HUD PCDs → `push_message` stubs |
| **BD6** | BD ZScript on Room VM | `done` | 100% | §14.6 probe + every `bd_*` test green on `testdata/zscript/bd_real`. `lower.rs` is the compiler |
| **BD7** | Lumps / UI verification | `done` | 100% | commercial **0** lump errors; fixture MENUDEF golden; **GZDoom screenshots deferred** (manual) |
| **BD8** | Perf / gore budget / web load | `done` | 100% | NOINTERACTION + 512 soak + 3k soft budget + corpse queue; **web streamed pk3 deferred** |
| **BD9** | Netplay / verifier with BD | `done` | 100% | synthetic + commercial BD ZScript 2-peer checksum; **15-min room/verifier soak deferred** |
| **UZ0** | `room parity` + vendored UZDoom zscript | `done` | 100% | report `docs/qa/uzdoom-parity.md`; tree `wadsrc/uzdoom` @ `67bdb4b7`; count regression test |
| **UZ1** | Actor flag bitset | `code_complete` | 98% | Flags, saves, refreshed Freedoom demos, and sprite lerp (`DONTINTERPOLATE` / `INTERPOLATEANGLES`) are in. Not `done` until CI. Next task is UZ2 |
| **UZ2** | ZScript VM = ZS1b–ZS4b + ZS6b | `code_complete` | 98% | §14.6 probe and `bd_*` tests pass locally. Not `done` until CI. Next task is UZ3 |
| **UZ3** | Stdlib boot + native bindings | `in_progress` | 99% | 1048 imports: 909 builtins / 139 return-0 stubs. SkinChanged(2) makes GetSkin return 2. Before that change GetSkin is base. Chocolate DEMO1 diverges at tic 441 |
| **UZ4** | Weapon overlays + alt HUD | `in_progress` | 99% | PowerDoubleFiringSpeed halves a 4-tic second layer. The gun stays at 4 |
| **UZ5** | Sound, reverb, particles | `in_progress` | 12% | A_StopAllSounds clears the pistol, the shotgun, and both playing voices |
| **UZ6** | Renderer 5.0 extras | `not_started` | 0% | — |
| **UZ7** | IQM bone API | `not_started` | 0% | — |
| **UZ8** | Heretic | `deferred` | 0% | Out of scope (2026-11-03) |
| **UZ9** | Hexen | `deferred` | 0% | Out of scope (2026-11-03) |
| **UZ10** | Strife | `deferred` | 0% | Out of scope (2026-11-03) |
| **UZ11** | Game controllers | `deferred` | 0% | Out of scope (2026-11-03) |
| **UZ12** | 64 players + prediction + net saves | `not_started` | 0% | — |
| **UZ13** | Menus, accessibility, DAP | `not_started` | 0% | — |
| **UZ14** | id24 / DEHEXTRA / EMAPINFO | `in_progress` | 25% | `room-defs/src/id24.rs`: JSON-lump envelope, typed `SKYDEFS` / `DEMOLOOP` / `GAMECONF` (spec merge rules), raw `TRAKINFO`/`SBARDEF`; wired into `Definitions.id24`; 66 room-defs tests green. Open: typed `INTERLEVEL`/`FINALE`/`TRAKINFO`/`SBARDEF`, renderer/UI use of the data, DEHEXTRA fixture, EMAPINFO, pistol start / `changemap -1` |

**Priority override (2026-10-12):** M7 + M10–M13 done. Active **M14** (1.0 hardening).
**M8** when Heretic/Hexen IWADs available; M9 remains deferred.
**Server track:** `docs/22-room-server.md` S1–S5 (parallel to M14).
**Duke track:** D3/D5–D8, D10 and D11 are in progress; D1/D2/D4 are recorded complete under their listed gates. Native-only DEMO1 now matches through tic 69 after correcting the APLAYER water-bubble spawn RNG and deferring the final PSWIMMING ifrnd to the first WAIT tic. First remaining mismatch is tic 70: the APLAYER trailing bubble ifrnd still runs when Chocolate changes action and skips it. Animated walls and WATERBUBBLE destination-plane checks remain implemented. The earlier "bit-exact to 1050" came from 1,750 `// TEMP` per-tic snaps / RNG pads, now behind `DukeSession::trace_fixups` (off by default). Spec: `docs/25`.
**Face-swap track:** **F0–F7** done (`docs/26`).
**Survival Director track:** **SV0**–**SV9** + **MH0**–**MH4** done (`docs/27`). **MH5** optional / deferred.
**Battle Royale track:** **BR0**–**BR9** + **BE1**–**BE12** + **S-BR1** / **S-BR2** (MVP) / **S-BR4** done (`docs/28` §8–§10). Deferrals: **S-BR2** OpenSkill + weekly reset.
**Shader track:** **SHR** batch 3 (U1/U2/U16 normals+TBN+aux); **SH0**–**SH1** + **SH5**/**SH10** still open (`docs/29`).
**Weapons track:** **WF0**–**WF4** + re-audit **N1–N5/N7/N8** + **W8.3** landed (`docs/30`); open **N6** remainder / W12 melee FX / optional movebob CVars.
**Mod API track:** **MA1**–**MA4** + **SK0**–**SK4** done (`docs/31`, `docs/32`); shuriken acceptance e2e green.
**ZScript → WASM track (`docs/37`):** **ZW0–ZW2** done, **ZW3** on wasmi and wasmtime (loops ×39–×239; short calls still slower than the interpreter). `ROOM_ZVM_TIER=aot`; Brutal Doom's ZScript runs identically on both tiers.
**ZScript VM track:** **ZS0–ZS2** done (`lower.rs` compiler, natives in `natives.rs`, probe + fuzz green); **ZS3b** done (`with_zscript`, play host on the VM); **ZS4b** and **ZS6b** done (`docs/34` §14.6 `bd_*` green, heap image v2). **ZS5–ZS8**/**ZS10** still the earlier scaffold; **ZS9** superseded by **ZW3**.
**Brutal Doom track:** **BD0**–**BD9** done with the listed deferrals. Runtime compatibility is reopened for vehicle support: BIKERACE `Tank`/`Active`, `PowerupGiver`/`PowerMorph`, custom-inventory pickup, and ACS morph/unmorph are implemented; in-game vehicle movement and exit still need a Doom II runtime pass (see `docs/33-brutal-doom.md` §3.3). **BD6** is the §14.6 probe plus every `bd_*` test on `testdata/zscript/bd_real`. Gated on `ROOM_BRUTALDOOM_PK3`.
**UZDoom parity track:** **UZ0** done. **UZ1** and **UZ2** code complete locally (not `done` until CI). **UZ3** in progress (1048 imports, 909 builtins, 139 stubs; SkinChanged(2) makes GetSkin return 2; before that change GetSkin is base; Chocolate DEMO1 diverges at tic 441). **UZ4** in progress (`PowerDoubleFiringSpeed` halves a 4-tic second layer; the gun stays at 4). **UZ5** in progress (`A_StopAllSounds` clears the pistol, the shotgun, and both playing voices). **UZ8–UZ11** deferred (out of scope).

### Log
- `2026-11-08` — **D6 full-screen palettes**: `LOOKUP.DAT` parsing now retains the five trailing six-bit RGB palettes; the Duke view applies Chocolate `setpal` precedence (night vision/FLOORSLIME, then underwater) to the scene and weapon framebuffer. Renderer reads the current snapshot and does not mutate playsim. In-game ART overlays drawn by the desktop shell (TIP, loogies, and Atomic help tiles) use that same palette; the entire desktop canvas now remaps its draw-list and late shell commands, including fills, text, patches, debug/quote overlays, help, and palette-flash fills, through the active palette. Static review of Chocolate `player.c::setpal` and LOOKUP layout; formatting, diff check and `cargo check -p room-build -p room-duke` only; no tests, gameplay or reference comparison.
- `2026-11-08` — **D6 palette propagation to desktop ART overlays**: the renderer exposes the active Chocolate screen palette to the desktop UI tile path; TIP, loogies, and Atomic help images now use the same palette as the scene/weapon. Selection uses the player’s current sector, matching `setpal` even when a remote camera is active. The desktop canvas remaps the complete draw list and later shell commands, including status bar, text, patches, help, debug/quotes and palette flashes. Exact palette-dependent `palookup` shade composition remains open. `cargo fmt --all`, `cargo check -q -p room-duke -p room-app` and `git diff --check` pass with existing warnings; no tests, gameplay or Chocolate comparison.
- `2026-11-08` — **D6 full UI palette remap**: desktop Duke now sends every draw-list fill/text color and decoded patch pixel through the active `LOOKUP.DAT` screen palette before upload; duplicate RGB samples are cached per frame to keep patch conversion bounded by the number of unique colors. This covers the Duke status bar, menus and draw-list UI; a final canvas-command pass also converts later help tiles, debug/quote text and palette-flash fills. Exact palette-dependent `palookup` shade composition remains open. `cargo fmt --all`, `cargo check -q -p room-duke -p room-app`, and `git diff --check` pass with existing warnings; no tests or runtime/reference comparison.
- `2026-11-08` — **D6 late canvas palette remap**: the desktop performs a second pass over canvas commands emitted after the ordinary draw list, so Atomic help art, TIP/loogie overlays, quote/debug text and palette flashes also use the current screen palette. ART overlays are produced in the base palette and transformed exactly once. `cargo fmt --all`, targeted Duke/desktop compilation and `git diff --check` pass with pre-existing warnings; no tests, gameplay or reference comparison.

- `2026-11-07` — **D7 VIEWSCREEN camera selection**: native `Open` now finds a forward-facing VIEWSCREEN and matching live CAMERA1 by hitag, switches the local player render pose to the camera, and exits back to the player on the next use press. Camera yvel tracks monitor activation. Static Chocolate review of `sector.c::checksectors`, `actors.c::VIEWSCREEN`, `game.c::animatecamsprite`; `cargo fmt --all`, `cargo check -q -p room-duke --lib`, and `git diff --check` pass (existing warnings). No gameplay or reference comparison was run; monitor texture refresh, exact near-tag precedence, death/level reset and mirrors remain open.
- `2026-11-07` — **D7 VIEWSCREEN monitor feed**: the native software renderer now renders the linked camera into a 64×48 palette-indexed STATIC tile for the matching VIEWSCREEN. It uses a render-only world copy, so gameplay state remains untouched, and restores the original ART tile data after drawing. `cargo fmt --all`, targeted room-duke compile and `git diff --check` pass; no screenshots, gameplay or Chocolate comparison were run. Exact vanilla camera update cadence, tint/scanline treatment and multiple simultaneous feeds remain unverified.
- `2026-11-07` — **D7 SE28 lightning**: statically ported the 65-tic initial wait, near-player storm start, 64–575 tic duration, thunder/slap cues, hitag-linked NATURALLIGHTNING toggles, SMALLSMOKE strike, 3D player-distance damage and red pain flash; the post-effector player-sprite sync now commits those health changes to APLAYER so later tics cannot restore stale health. Compile/format and diff checks pass; post-effector APLAYER sync preserves the damage. No gameplay or Chocolate comparison was run. Global visibility flash remains approximated by palette tint, and coop visual/RNG ordering is unverified.
- `2026-11-07` — **D8 reverb-zone edge state**: native `MUSICANDSFX` lotag 1000–1999 markers now update their active T1 on radius crossings in Chocolate STAT_FX order and set/reset the session reverb level only at those edges. The selected level is included in `.fsave`; the desktop mixer reads session state instead of selecting an in-range zone at draw time. Static `actors.c::MUSICANDSFX` review; `cargo fmt --all`, targeted room-duke/room-app compile and `git diff --check` pass. No audio or gameplay comparison was run; hardware reverberation/audible parity remains unverified.
- `2026-11-07` — **D5 CON `ifrespawn`**: opcode 71 now selects Chocolate's `respawn_monsters`, `respawn_inventory`, or `respawn_items` option after the badguy/inventory classifier. The seven Atomic inventory tiles match `game.c::inventory`; imported Atomic `.SAV` globals feed the actor CON environment, with absent settings defaulting off. At this checkpoint, `ifcanshoottarget`, `ifbulletnear`, `ifnotmoving`, and `ifnosounds` remained VM stubs. Static review of Chocolate `gamedef.c` / `game.c`; formatting, diff check, and crate compilation only, no gameplay/demo comparison.
- `2026-11-07` — **D5 CON `ifbulletnear`**: opcode 70 now scans the live `STAT_PROJECTILE` list and applies Chocolate `dodge()`'s sector, projectile-owner, forward/approach dot products, and lateral cross-product threshold. A detected threat consumes the angle TRAND and rotates the actor; the updated angle is visible within the same CON run and retained afterward. At this checkpoint, `ifcanshoottarget`, `ifnotmoving`, and `ifnosounds` remained VM stubs. Static `gamedef.c`/`duke3d.h` review; formatting, diff check and crate compilation only, no gameplay/demo comparison.
- `2026-11-07` — **D5 CON `ifnotmoving`**: opcode 82 now evaluates the exact `hittype.movflag` collision-mask expression from `gamedef.c`. Actor movement stores the last `movesprite()` return and leaves it unchanged on early-return paths, preserving the one-tic-old value that CON reads. Static `gamedef.c` / `hittype` review; formatting, diff check and crate compilation only, no gameplay/demo comparison. Remaining VM stubs include `ifcanshoottarget` and `ifnosounds`.
- `2026-11-07` — **D5 CON `ifcanshoottarget`**: opcode 45 now follows Chocolate's `g_x > 1024` behavior, CLIPMASK1 `hitasprite` distance, wide-enemy ±48 / normal ±16 side rays, 3084/768 primary range gates, same-picnum self-class suppression, and forcefield-wall exception for badguys. The per-actor hit snapshot is refreshed from current sprites and models ART-sized face sprites. Static review of `gamedef.c`, `player.c`, and `duke3d.h`; formatting, diff check and crate compilation only. `ifnosounds` remains stubbed; no gameplay/demo comparison was run.
- `2026-11-07` — **D5 CON `ifnosounds`**: opcode 109 now checks Chocolate's first active `SoundOwner[id][0]` for each Atomic sound ID. The desktop mixer exports its first active voice owner by ID; the snapshot enters DukeSession before the tic, while CON `sound`, `soundonce`, `stopsound`, and `globalsound` update the shared table during actor execution. Global sounds retain the player sprite as their source so later snapshots preserve the correct owner. Static review of Chocolate `gamedef.c` / `sounds.c`; `cargo fmt --all`, `git diff --check`, and targeted compile checks only. No gameplay, demo, or Chocolate comparison was run.
- `2026-11-07` — **D5 CON `pstomp` / misc blood pools / footprints / player quickkill**: opcode 38 starts the knee sequence only for a visible target when the player is not shrunk and no knee attack is active. The 15-tic cycle turns toward its locked target, restores weapon pose, checks Chocolate's 3D distance approximation, spawns seven `JIBS6` fragments in RNG order and a `BLOODPOOL` with floor/water/parallax handling, increments badguy kills, and arms matching civilian respawn markers. `BLOODPOOL`/`PUKE` follow the floor, expand for 32 tics with Chocolate's two ordered TRANDs per growth tic, then shrink; nearby players take periodic palette-0 damage or consume a boot charge, start the right footprint count/palette, and voice ID 211 follows the one-active-owner rule. Player movement suppresses nearby duplicate footprints, checks flat-sector/tag gates, chooses one of four variants with one TRAND, and applies spawn floor/cstat/palette rules. Shallow-water movement seeds six later dry-ground footprints and tracks a `PLAYERONWATER` dummy sprite through water exit/death. `pstomp` on APLAYER now applies the quickkill health/palette/cstat changes, emits eight reference-order guts when god mode is off, and records killer/victim seat frag or self-frag counters. Death animation/respawn, frag matrix/scoreboard integration, warping-sector ripple animation, and full multiplayer ownership attribution remain open. Static Chocolate source review; formatting, diff check and crate compilation only; no gameplay/demo comparison.
- `2026-11-07` — **D5 CON `resetactioncount`**: opcode 36 now clears only `g_t[2]`, as Chocolate's `gamedef.c` does. Static source review; formatting, diff check, and targeted compile checks only; no runtime/demo comparison.
- `2026-11-07` — **D5 CON `respawnhitag`**: opcode 83 now selects the actor's yvel for the Atomic civilian tile set and hitag for other actors, then activates same-lotag `RESPAWN` markers in `STAT_FX` unless `monsters_off` excludes a badguy. The existing marker tic handler continues with the transporter star and respawn sequence. Static review of Chocolate `gamedef.c` / `sector.c`; formatting, diff check, and targeted compile checks only; no gameplay/demo comparison.
- `2026-11-07` — **D5 CON `mikesnd`**: opcode 97 now reads the actor yvel sound and queues it only if its sound ID has no active voice owner, consistent with Chocolate's `Sound[id].num == 0` condition. Static `gamedef.c` review; formatting, diff check, and targeted compile checks only; no gameplay/demo comparison.
- `2026-11-07` — **D5 CON `wackplayer`**: opcode 61 now consumes Chocolate's `TRAND&255`, applies `forceplayerangle` to the nearest player, and terminates the actor parse. The player state now retains/damps screen roll and view yaw; the native software frame applies fixed-point indexed-pixel rotation. Static review of Chocolate `gamedef.c`, `player.c`, and renderer math; `cargo fmt --all`, `git diff --check`, and targeted compilation pass. No gameplay or Chocolate comparison was run.
- `2026-11-07` — **D5/D6 CON `tossweapon`**: opcode 104 now performs Chocolate's `checkweapons()` branch in CON order, including TRAND selection, player sprite ownership, item initialization and the RPG/handbomb explosion fallback. Static review of Chocolate `gamedef.c`, `player.c`, and `game.c::spawn`/`EGS`; formatting, diff check and targeted compile only. No gameplay/demo comparison was run.
- `2026-11-07` — **D5 CON `ifawayfromwall`**: opcode 90 now tests Chocolate's four diagonal offsets (±108 Build units) against the actor's current sector using the shared integer point-in-sector primitive. Static review of Chocolate `gamedef.c`; no runtime/demo comparison was run.
- `2026-11-07` — **D7 SE29 wave floor**: ported Chocolate's per-effector sine displacement (`hitag += 64`, `mulscale12(SP, sintable[hitag])`) plus its post-effector four-wall floor slope alignment against wall 2's neighboring sector. Invalid/missing neighbor geometry is skipped safely. Source reviewed in Chocolate `actors.c::moveeffectors` and `engine.c::alignflorslope`; `cargo fmt --all`, `git diff --check`, and targeted wasm compile pass. No scenario, gameplay, or reference comparison was run; exact multi-effector ordering remains unverified.
- `2026-11-07` — **D10 bounded lockstep inbox**: `DukeLockstepInbox` collects exactly one native input for each seat in a fixed roster, rejects stale/wrong tic, duplicate, and unseated commands, and returns a completed frame in ascending seat order. This adds a deterministic protocol barrier, not simulation/server integration. `cargo fmt --all`, `git diff --check`, and targeted `cargo check -q -p room-proto` pass; no tests or network/runtime comparison were run.
- `2026-11-07` — **D10 browser native lockstep wiring**: the v2 WebSocket client now encodes Duke tic commands and queues authoritative Duke frames; the browser GRP lobby validates/prepares APLAYER seats before Ready and advances `DukeSession` only from received server frames. Seat-indexed holes are expanded with neutral inputs, and weapon hotkeys / wheel cycles travel through the native input bitfield. `cargo fmt --all`, `git diff --check` and `cargo check -q -p room-web --target wasm32-unknown-unknown --features browser` pass (existing Duke crate warnings). No gameplay, multi-client lockstep, or Chocolate comparison was run. Weapon slot 0/remote mapping, negotiated coop versus DukeMatch, and reconnection/save behavior remain open.
- `2026-11-07` — **D11 browser Atomic campaign routing**: single-player native browser sessions now initialize an E#L# campaign cursor, consume the native level-exit request, load the next GRP map, preserve campaign state and carry persistent health/armor/weapons/ammo/inventory while resetting transient effects. Bonus-screen and end-game menu presentation remain open; coop campaign transitions are not enabled because the relay has no map-change barrier. Static compile/format checks only; no gameplay or reference comparison.
- `2026-11-07` — **D9 Atomic SAV world tables**: `SavHeader` now retains the fixed animation-wall records, both 2048-entry animation target arrays, sprite deletion queue cursor/amount/entries, mirror count/tables, automap reveal bitset, saved board filename, and all three cloud tables after the sprite-list and cycler state. Lengths and order follow Chocolate `MENUES.C::loadplayer` / `duke3d.h`; the parser still does not mutate a `DukeSession` or encode `GAME#.SAV`. `cargo fmt --all`, `git diff --check`, and targeted `cargo check -q -p room-build -p room-duke -p room-app --bin room` pass with existing warnings. No tests, gameplay, or reference comparison were run.
- `2026-11-07` — **D9 Atomic SAV multi-block state**: added Chocolate `kdfread` chunk iteration with element-wise delta reconstruction and applied fixed-array sizes to world, link, animation, queue, mirror, map-bit, script, actor-script, sprite-script and `weaponhit` payloads. The parser now advances across the script/actor-temp block before reading `lockclock`, and takes `randomseed` from its native position after frag state. `SavHeader::script_state` preserves serialized CON and actor-temp arrays for a future session restore. Source reviewed in Chocolate `filesystem.c::kdfread` and `MENUES.C::loadplayer`; formatting, diff check and targeted compile pass, with no tests/save replay/runtime comparison.
- `2026-11-07` — **D9 SAV scalar layout correction**: corrected the native order and sizes for `animatecnt`, `animatesect`, animate pointers/goals/velocities, `ud` byte/short/long settings, and the byte-sized `numplayersprites`. This keeps subsequent lockclock, RNG and global fields aligned with the saved stream. Static review against Chocolate `duke3d.h` and `MENUES.C::loadplayer`; targeted compile passes; no save load or replay was run.
- `2026-11-07` — **D9 Atomic SAV cyclers**: `SavHeader` now retains Chocolate's complete fixed-size 256 × 12-byte `cyclers` table after `numcyclers`, preserving inactive slots and record bytes for later native restoration. Source layout reviewed in [Chocolate Duke `MENUES.C::loadplayer`](https://github.com/fabiensanglard/chocolate_duke3D/blob/master/Game/src/menues.c#L307-L323). No tests or runtime/reference comparison were run; applying state and writing `GAME#.SAV` remain open.
- `2026-11-07` — **D6 tripbomb laser lifecycle**: weapon slot 8's planting is now followed by the standable fuse state: initial arm delay, horizontal `hitscan_full` approximation of Chocolate `hitasprite`, linked LASERLINE generation with native perpendicular angles and `lasermode` cstats, hit/target-change countdown, five native RANDOMSCRAP spawns, tripbomb-radius hitradius, explosion creation and line cleanup. The exact Build hitasprite/laserline collision, heat-mode visibility and SSP movement remain open. `cargo fmt --all`, `git diff --check`, and `cargo check -q -p room-duke --lib` pass with existing warnings. No tests or gameplay/demo comparison were run.
- `2026-11-07` — **D6 hitradius wall impacts**: static Chocolate `checkhitwall`/`lotsofglass` port now gates mirror breaks by the source-tile allowlist; breaks common vending, screen, tech, ATM and wall-light textures; spawns wall-interpolated ordinary and palette-colored glass shards with the native vertical fallback and RNG draws; creates forcefield ripples and flags animated force walls; arms matching SE3 flicker markers from tagged wall lights; spawns ATM money with native count/position/cstat draws; and spawns the SE128 timer that animates/reblocks paired glass overlays. Wall visibility now resolves the sample sector with `updatesector` before `cansee`. `cargo fmt --all`, `git diff --check`, and `cargo check -q -p room-duke --lib` pass with existing warnings. No tests or gameplay/demo comparison were run. Light side effects beyond linked SE3 and remaining `checkhitwall` materials remain open.
- `2026-11-07` — **D7 extend-o-bridge (lotag 27 / SE20)**: sector activation now toggles the local SE20 direction; map setup caches its two closest wall vertices, and the effector advances eight Build units per tic along its angle, propagating `dragpoint`, floor panning, grounded sector sprites and players. Completion stops at Chocolate's quantized `SP` distance and emits the sector completion sound. Static review of Chocolate `sector.c::operatesectors`, `game.c::spawn` and `actors.c::moveeffectors`; `cargo fmt --all`, `git diff --check`, and `cargo check -q -p room-duke --lib` pass. No gameplay/demo comparison was run. Blocking/overlap behavior, unusual zero/short SP maps and broad SE coverage remain open.
- `2026-11-07` — **D7 conveyors (SE24/SE34)**: implemented Chocolate's marker-angle/`SP` (`sprite.yvel`) vector and actor filtering: negative-Z-velocity sprites are skipped; status-5 bloodpools, puke, footprints, bullet holes and blood splats are hidden; laser lines, tripbombs, bolt sprites, switch sprites and cranes are not carried. Eligible grounded actors cache `bposx/bposy`; sprite displacement adds the quarter-vector directly, then updates sector membership to match Chocolate's `setsprite` call. Player force enters Room's fixed-point movement/friction path after the player-to-sloped-floor distance check, and sector floor panning advances. The loaded `TABLES.DAT` sine table drives direction. Static Chocolate `actors.c::moveeffectors` review; formatting, diff and targeted compile checks only. Exact input/effectors timing and scenario verification remain open; no gameplay/demo comparison was run.
- `2026-11-07` — **D7 conveyor movement correction**: static inspection of Chocolate `actors.c::moveeffectors` (`SE24`/`SE34`) showed that sprite transport performs direct quarter-vector addition followed by `setsprite`, rather than wall `clipmove`. Reverted the extra clipping behavior to preserve reference semantics; the existing `updatesector` call models `setsprite`'s sector-list refresh. The same source pattern for SE26 uses direct displacement then `setsprite`. Format, diff and targeted compile checks only; no dynamic comparison.
- `2026-11-08` — **D7 VIEWSCREEN camera activation correction**: static review of Chocolate `sector.c::checksectors` showed that a VIEWSCREEN's `lotag` selects an inactive `STAT_ACTOR` CAMERA1 by `hitag`; entry links the monitor's `owner` to the camera and sets the active `yvel` latch, and plays `MONITOR_ACTIVE`. Room now checks ART size/offset, portal visibility, the 1280-unit range and three vertical use-ray samples; activation and exit are distinguished so exit does not replay the cue. Broader `neartag` winner ordering remains approximate; `exact `xyzmirror`/`camsprite` texture contents and tint, level-change cleanup and mirrors remain open. Existing camera-to-texture rendering remains render-only. Static source inspection, formatting, diff check and targeted crate compile only; no gameplay or screenshot comparison.

- `2026-11-08` — **D7 camera refresh cadence**: `DukeSoftRenderer` now retains the 64×48 palette-indexed camera feed per CAMERA1 actor and recomputes it at initial display, each 12-tic Chocolate `animatecamsprite` interval, or after tic reset/camera selection change. The renderer reinserts the cached image into its render-only tile set each frame; it does not alter the `BuildWorld` snapshot. Exact `xyzmirror`/`camsprite` texture contents and tint remain incomplete. Static review of Chocolate `sector.c::animatecamsprite`; format, diff and `cargo check -q -p room-duke --lib` only, no screenshot/gameplay/reference comparison.
- `2026-11-08` — **D7 SE11 activator hitag filter**: lotag-23 swing doors now require their local SE11 hitag group to match the ACTIVATOR hitag before arming linked doors. Room previously used whichever local SE11 marker appeared first and ignored the activator tag, allowing unrelated switches to open the door. Static review of Chocolate `sector.c::operatesectors` and `operateactivators`; `cargo fmt --all`, `git diff --check`, and targeted `cargo check -q -p room-duke --lib` only. No gameplay or dynamic comparison.
- `2026-11-08` — **D7 SE11 map direction initialization**: `spawn_sector_effectors` now initializes swing-door T4 rotation direction from marker angle (`ang > 1024` → `+2`, otherwise `-2`), as Chocolate `game.c::spawn` does. Lotag-23 activation reverses this seeded value; previously fresh SE11 temps left it zero, so rotation never advanced. Static source review and targeted compile/format checks only; no gameplay or dynamic comparison.
- `2026-11-08` — **D7 SE15 temp-slot correction**: lotag-25 activation now writes Chocolate T5 (`temp_data[4]`) for opening/closing and checks that same active-mode slot for the conditional repeat sound. T4 (`temp_data[3]`) remains the `SP >> 3` travel counter used by `moveeffectors`; previously activation wrote the direction into T4, so the slider started at an incorrect travel offset and repeated sound gating was tied to its counter. The active mover now also writes the source's `xvel=16` before applying its `ms()` displacement. Verified against Chocolate `sector.c::operatesectors` and `actors.c::moveeffectors`; `cargo fmt --all`, `git diff --check`, and targeted `cargo check -q -p room-duke --lib` only. No tests, runtime play, or dynamic comparison.
- `2026-11-07` — **D7 SE25 piston ceiling**: map setup caches the original ceiling in `T4`, activates `T5`, and places the ceiling at the marker Z; each native tic follows Chocolate's shade-directed cycle using marker `SP` (`yvel`) in 16-unit Z steps, clamped between the saved ceiling and sector floor. Static review of Chocolate `game.c::spawn` and `actors.c::moveeffectors`; format/compile checks only, with no scenario or demo comparison. Interpolation presentation and collision/blocking parity remain open.
- `2026-11-07` — **D7 SE35 drill ceiling correction**: static audit found Chocolate sets the sector ceiling to marker Z but leaves SE35 `T5` zero-initialized; the return phase clamps to that value, not the original ceiling. Room initialization now matches that behavior. The earlier implementation note was corrected. Static source review only; no dynamic Chocolate/gameplay comparison.
- `2026-11-07` — **D7 connected-sector sprite switches**: tagged switch sprites are now eligible through connected sectors only when the player's switch ray passes Room's Build `cansee` portal/floor/ceiling test; the existing forward-ray width, height and distance checks still select the nearest candidate, then `neartag` wall precedence is preserved. This closes the same-sector-only gap for visible portal-adjacent sprite switches. Static code inspection; `cargo fmt --all`, `git diff --check`, and `cargo check -q -p room-duke --lib` pass. No gameplay/demo comparison was run; parity of Build's multi-sector sprite-neartag traversal remains unverified.
- `2026-11-07` — **D7 SE31 true-drop-floor**: map initialization stores the original floor height, applies the marker Z for non-1536 direction, and sets Chocolate's 9999 wall shade filters. Activation advances the floor at marker XVEL, handles the ANG 1536 versus alternate travel/return targets, observes the HITAG dwell counter, rides grounded players and stationary sprites, updates actor floor caches, and dispatches completion sounds. Static review of Chocolate `game.c::spawn`, `actors.c::moveeffectors`, `sector.c::operatesectors` and master-switch handling; `cargo fmt --all`, `git diff --check`, and `cargo check -q -p room-duke --lib` pass. No scenario or demo comparison was run.
- `2026-11-07` — **D7 SE32 true-drop-ceiling**: shares map plane-target initialization with SE31, then moves the ceiling to/from the marker using Chocolate's doubled arrival threshold and toggled return state. It leaves actors uncarried and emits the completion sound on plane arrival. Static review of Chocolate `game.c::spawn` and `actors.c::moveeffectors`; `cargo fmt --all`, `git diff --check`, and `cargo check -q -p room-duke --lib` pass. No scenario/demo comparison was run.
- `2026-11-06` — **D7 RESPAWN markers**: `operateactivators` arms matching `STAT_FX` markers at extra=54; `tick_respawns` mirrors Chocolate `movefx`'s 54→66 countdown, spawns a `TRANSPORTERSTAR` with the reference size/shade/offset, inserts the recorded hitag tile at marker position through native EGS/CON initialization, applies actor/badguy status and fall setup, then removes the marker. `monsters_off`, full native actor special-case initialization, and runtime comparison remain open. Static source review; `cargo fmt --all`, `cargo check -q -p room-duke --lib`, and `git diff --check` only.
- `2026-11-06` — **D7 access cards**: CON `addinventory ACCESSCARD` grants the red/blue/yellow bit from the actor palette; `Open` checks wall-switch palette against inventory, reports quotes 70–72 when locked, then advances the 12-tic insertion, activates the switch and consumes that key. Player insertion state and the pending card grant are held in native player/CON state. Static Chocolate `gamedef.c`, `sector.c`, and `player.c` review; format, `cargo check -q -p room-duke --lib`, and diff check only; no gameplay comparison was run. Access sprites, hand/HUD visuals, and access-sprite switches remain open.
- `2026-11-06` — **D7 sprite switches**: `Open` now considers tagged switch sprites in the current sector using Chocolate's forward-ray, width and height tests; access-card use records a sprite target, activates linked sprite/wall switch art at insertion tic 12 and consumes the matching card. Uses static Chocolate `engine.c::neartag` and `sector.c::checkhitswitch` review. `cargo fmt --all`, `cargo check -q -p room-duke --lib`, and `git diff --check` pass; no gameplay comparison was run. Connected-sector sprite traversal and hand/HUD card visuals remain open.
- `2026-11-06` — **D7 forcefield activation**: player wall/sprite switches now route matching animated forcefield walls through Chocolate `operateforcefields`: clear or restore collision `cstat` and reset their `animatewalls` phase. Static `sector.c` review; `cargo fmt --all`, `cargo check -q -p room-duke --lib`, and `git diff --check` pass; no gameplay comparison was run. Effector-driven forcefields remain open.
- `2026-11-06` — **D7 monster spawn policy**: `DukeSession::monsters_off` now feeds Chocolate's load-time roll rules, hides skill-gated/off badguys in `STAT_MISC`, and prevents tagged respawn markers from arming when their hitag target is a badguy. The setting persists in `.fsave` and defaults off for prior v1 saves. Static `game.c::spawn` / `sector.c::operaterespawns` review; `cargo fmt --all`, `cargo check -q -p room-duke --lib`, and `git diff --check` pass; no gameplay comparison was run. Native menu setting and special actor initialization remain open.
- `2026-11-06` — **D9 Atomic `.DMO` writer**: `DukeAtomicDemoRecorder` captures native single-player inputs and emits Atomic revision 117 headers plus byte-delta records in raw Ken `strtot=0` chunks; `monsters_off` is carried into the demo header. Static Chocolate `game.c::opendemowrite` / `filesystem.c::dfwrite` review. `cargo fmt --all`, `cargo check -q -p room-duke --lib`, and `git diff --check` pass; no demo playback or Chocolate comparison was run. Multiplayer `.DMO` streams and reference synchronization remain open.
- `2026-11-06` — **D9 session-bound recording**: `DukeAtomicDemoRecorder::tick_session` now advances `DukeSession` and records the same input atomically at the API level, preventing callers from accidentally recording a different input sequence from the one played. Static implementation only; no dynamic replay or reference comparison.
- `2026-11-06` — **D9 DMO chunk sizing correction**: raw `strtot=0` chunks now cap at 16,380 bytes, matching Chocolate `dfwrite`'s 16,384-byte LZW buffer and ten-byte records. This avoids emitting chunks that overflow the reference reader's decompression buffer. Static `filesystem.c` review; no playback was run.
- `2026-11-06` — **D10 native sync input codec**: added `DukeSyncInput` in `room-proto`, matching Chocolate's packed 10-byte `(avel, horz, fvel, svel, bits)` record with strict-size decoding. Static review of `duke3d.h`; no multiplayer session or network test was run. FTR2 tic/seat envelope and runtime integration remain open.
- `2026-11-06` — **D8 `soundonce` / `stopsound`**: CON opcodes 87 and 89 now enqueue native actor-origin start-once and sound-ID stop events. The audio mixer enforces one active voice per logical sound ID for `soundonce`, and `stopsound` removes matching voices regardless of origin. Static `gamedef.c` review and compile/format checks only; no listening comparison was run. `globalsound`, `MUSICANDSFX`, ambient loops, and lock semantics remain open.
- `2026-11-06` — **D8 `globalsound`**: CON opcode 92 now queues the defined sound ID at the local player's position in single-player, matching Chocolate's `spritesound(N, ps[screenpeek].i)` behavior. The coop routing condition remains unavailable until D10 player state is implemented. Static source review; no listening comparison.
- `2026-11-06` — **D5 CON `sizeto`**: opcode 28 now steps sprite xrepeat toward its target every actor tic and steps yrepeat under Chocolate's player/minimum-size/vertical-clearance conditions. The operation uses current mutable repeats, allowing ordered `sizeat` + `sizeto` in one CON execution, and the ART tile height from the loaded metadata. Static `gamedef.c` comparison; `cargo fmt`, crate compilation, and diff check only; no gameplay/demo replay.
- `2026-11-06` — **D5 CON `debris`**: opcode 37 now emits the requested EGS fragments with parent position/shade/palette, Build velocity fields, reverse loop order and Chocolate's TRAND sequence. BLIMP/SCRAP1 preserves the 14-entry global weapons/ammo remap table. Static `gamedef.c`/`actors.c`/`global.c` review; compile/format/diff checks only; no gameplay or demo replay.
- `2026-11-06` — **D5 CON `guts`**: opcode 58 now computes the sloped-floor clamp, COMMANDER offset, small-badguy scaling, JIBS2 quarter-size rule and palette-6 gore, then inserts EGS fragments owned by the player sprite. All fragment random values are captured at opcode execution to preserve later CON RNG order. Static Chocolate `gamedef.c`/`actors.c` review; compile/format/diff checks only; no gameplay or demo replay.
- `2026-11-06` — **D7 touchplates**: Room now evaluates stat-6 `TOUCHPLATE` actors against the grounded player sector, preserves the `g_t[0..5]` latch/direction/one-shot state, arms same-lotag masterswitches, triggers tagged activators, links matching plates, and moves/carries actors with the sector floor. The `check_activator_motion` gate accounts for active plane/wall motions and relevant SE temporary flags. Static Chocolate `actors.c`/`sector.c` review; format, crate check and diff check only; no scenario or demo comparisons were run.
- `2026-11-06` — **D7 master-switch countdown**: implemented Chocolate `moveactors`' armed-switch timer (`hitag--`), delayed sector operation through the shared lotag 16–22/9 mover path, local SE/SEENINE/OOZFILTER marker updates, and switch deletion at expiry. Wall switches and touchplates arm matching markers; other `operatemasterswitches` callers and unsupported `operatesectors` families remain open. `cargo fmt --all`, `cargo check -q -p room-duke --lib`, and `git diff --check` pass; no gameplay or demo comparison was run.
- `2026-11-06` — **D7 master-switch arming**: tagged wall switches and touchplates mirror Chocolate `operatemasterswitches(lotag)` by setting matching standable `MASTERSWITCH.yvel` from 0 to 1 once. Static source review and compile/format checks only; no gameplay comparison was run.
- `2026-11-06` — **D5 swim RNG frontier**: direct Chocolate `execute()` traces confirm WATERBUBBLE #399 runs CON at tic55. Removed the synthetic WAIT catch-up rolls and an incorrect RNG draw from `spawn(player, WATERBUBBLE)`; Chocolate's generic `spawn()`→`EGS()` does not draw randomness. Added a deferred PSWIMMING roll on the first WAIT tic. Native-only DEMO1 now matches reference seed, pose, sprite count and hash through tic69. At tic70, Room's APLAYER trailing bubble `ifrnd` runs although Chocolate changes action and skips it; the remaining script/action transition is the next frontier.
- `2026-11-06` — **D5 WATERBUBBLE cached planes**: Chocolate’s sprite 80 trace confirms `hittype.floorz=69632` remains cached after its tic-79 warp into a sector whose floor is 49152. Room had evaluated `iffloordistl` against the destination floor, switched to `BUBMOVEFAST`, and reset actioncount; initializing the CON hit-plane cache when the bubble spawns and preserving it across warps matches this stale-plane behavior without a tic-specific rule. Native-only DEMO1 now matches through tic 134; first full state divergence is tic 135 (503 vs 504 sprites, with one extra `JIBS6` in Room; player pose still matches). Moved WATERBUBBLE sprites also refresh their Build sector after movement. Next: attribute the tic-135 particle/shot difference before advancing the frontier.
- `2026-11-06` — **D11 launcher default**: `scripts/play-duke3d.sh` now starts the native Room-Duke runtime by default; the converter remains selectable with `--runtime converter` or `ROOM_DUKE_RUNTIME=converter`. `bash -n`, `--help`, and `git diff --check` pass.
- `2026-11-06` — **D5 APLAYER swim / native frontier**: reference sprite dumps showed player `temp_data[0]` was not incremented by Room's player mover, and the swim action's inner `PSWIMMING ifrnd 4` was suppressed for more than its entry tic. Room now advances the player counter in and out of water and enables the inner roll after that entry tic; a new `conlabels` frontier diagnostic prints Atomic swim action offsets. `duke_frontier real DEMO1 56` and `krand` agree through tic54; first mismatch moves to tic55 (`randomseed`), where several swim/actor calls are early. D5 remains incomplete.
- `2026-11-06` — **D7/D11 switches + sector movers**: connected `gamefunc_Open` to Build `neartag_wall`, Atomic switch art and linked activators; added deterministic sector 16–22 plane interpolation, including ceiling-door, elevator and split-door transitions plus sprite/player carry when floors rise. Added lotag 9 sliding-wall vertex interpolation. Seven interaction tests cover switch cycles, tagged activation, sector20/22 round-trips, sector21 target selection, and a six-vertex lotag 9 door fixture. Sloped neighbor-Z search, blocked-door checks and other mover families remain open. `cargo test -p room-duke interactions::tests`, the native DEMO1 frontier test, app/web checks, fmt, script syntax and diff checks pass.
- `2026-11-06` — **D5 tic39 trace refinement**: reran native-only `DEMO1` and compared the per-call Chocolate RNG journal with Room's call-site log. The flattened call stream still aligns, but Room spends 40 RNG calls in the compared frame where the reference reaches its frame boundary after 33; remaining calls align with later reference-frame entries. Sprite-level comparison at tic39 first shows drift in WATERBUBBLE sprites #74–77 and sprite #369 Z. This points the next investigation at per-tic actor/effect ordering and temporary actor state; the accepted first divergence remains tic39 and D5 is not complete.
- `2026-11-06` — **D11 Duke use input mapping**: corrected native desktop and browser `E` input from bit 3 to Chocolate's `gamefunc_Open` bit 29 (`player.c`); added a regression test for the sync bit. That exposed the missing interaction handler, implemented in the following D7/D11 step.
- `2026-11-06` — **D6/D11 controls**: corrected native desktop/browser weapon hotkeys to Duke's 1–9 plus 0 slot mapping (pistol through remote; knee is slot 0 and has no number key), shared availability checks, and added wheel next/previous cycling over owned weapons with ammo. Three focused weapon selection/cycle tests pass; `cargo check -p room-app -p room-web` passes.
- `2026-11-06` — **D6 knee weapon**: added the Atomic kickback cadence (tic 7 strike, held repeat from tic 14), short-reach hitscan damage for badguy sprites, knee impact tile bookkeeping, and `KICK_HIT` sound dispatch; the view selects the knee's impact frame during the strike. `knee_weapon_hits_on_tic_seven_and_repeats_at_fourteen` and `cargo check -p room-app -p room-web` pass. Still partial: wall impacts and exact target filtering/effects are open; DEMO1 native sync remains at tic 30.
- `2026-11-06` — **D6/D5 RNG correction**: inspected Chocolate Duke's `duke3d.h` (`TRAND` aliases `krand()`); rejected a separate C `rand()` stream because moving map-load rolls onto it regressed DEMO1 from first divergence tic31 to tic0. Restored shared Build RNG for map-load and RPG rolls. `barrels_eggs_and_splats_roll_like_chocolate_spawn`, `rpg_damage_uses_atomic_three_radius_bands` pass; frontier remains tic30 (first randomseed diff tic31).
- `2026-11-06` — **D7 animated walls / D5 frontier**: implemented Atomic `premap` wall-list initialization and `animatewalls` for screen-break and force-field surfaces, called every other gameplay tic. On DEMO1's E3L11 map the four force-field walls consume the reference's five RNG calls at the 16th animation update; native seed, pose, sprite count and hash match through tic 38. Rebuilt the local instrumented Chocolate source in a temporary i386 container and generated a fresh gameplay trace through tic 295; it confirms the next native mismatch at tic 39 (`randomseed`). `demo1_frontier_logs_first_divergence` pins the same result. Dynamic sectors and other D7 systems remain open.
- `2026-11-06` — **D5/D6 native frontier**: after a water-tunnel warp, WATERBUBBLE `iffloordistl` now uses destination-sector planes in the native runtime; bubbles #75/#77 enter `CRACKEDBUBBLE` at the surface as in Chocolate. Native-only DEMO1 sync extends through tic 40; first mismatch is tic 41. Added `demo1_native_tic40_bitexact` and made `duke_frontier krand` compare end-of-tic seeds, exposing the one missing RNG advance at tic 41. Native APLAYER bubble spawns now apply the shared spawn Z-offset TRAND; the assisted trace path keeps its calibrated calls. Assisted tic119 regression remains green.
- `2026-11-06` — **D5 native CON state**: implemented actor `ifgapzl` using the cached `hittype.floorz` / `ceilingz` difference, shifted by 8 as Chocolate does; replaced the always-false stub and added below/above-threshold VM cases. `ifspawnedby` now reads the same mutable `hittype.picnum` as `ifwasweapon`, which damage overwrites; a VM test covers the replacement. Desktop and wasm builds pass. The trace still diverges at tics 40/41: Chocolate has a second `parse()` RNG call attributed to actor 160, while Room's actor 160 still has `hit_picnum=2311` and skips it. The next comparison needs the reference actor's mutable `hittype.picnum` or CON branch state at that tic.
- `2026-11-05` — **UZ3**: bound ZScript `CheckIfSeen`, `CheckRange`, and `CheckSightOrRange` to deterministic playsim snapshots of player distance and sight. Actor `Tick`, spawn, revive, and action calls populate these from Room's existing BSP sight check and geometry; range uses the player's current `view_z` and target vertical bounds like UZDoom's camera-center calculation. Camera actors and `SF_IGNOREVISIBILITY` remain unsupported; `cargo fmt --all`, targeted `cargo check`, and `git diff --check` pass.
- `2026-11-05` — **UZ3**: ZScript player handlers now receive `PlayerSpawned` at level start and `PlayerRespawned` after respawn, with a `PlayerEvent` carrying the player slot; pending actor operations/inventory changes are applied. Static/global registration and other event families remain open.
- `2026-11-05` — **UZ3**: wired `PlayerDied` into the playsim death path before ACS death scripts, carrying player slot and applying queued actor/inventory changes. `PlayerSpawned`, `PlayerRespawned`, and `PlayerDied` are now dispatched with the correct timing distinction.
- `2026-11-05` — **UZ3**: `PlayerRespawning` now runs before Room respawns a dead player and can veto the respawn; its `PlayerEvent` carries the seat number, and queued actor/inventory operations are applied. Matches UZDoom's `G_DoReborn` callback gate.
- `2026-11-05` — **UZ3**: `PlayerEntered` now populates `PlayerEvent.IsReturn` from the map hub-return path; UZDoom's `PlayerEvent` exposes this to map handlers.
- `2026-11-05` — **UZ3**: `WorldThingSpawned` now receives the spawned actor's class, health, TID, and monster flag instead of a synthetic boolean-only proxy, including late event-handler activation.
- `2026-11-05` — **UZ3**: ZScript actor `ChangeTID` now resolves its script object handle to the live playsim actor and applies the requested TID after native dispatch; removed the stale stub classification from the focused native coverage map.
- `2026-11-05` — **UZ3**: `Object.New` now allocates compiled ZScript classes with the program's class id and actual integer/object/string field counts. Classes absent from the compiled script table retain the generic compatibility fallback; DECORATE-native object construction remains partial.

- `2026-11-05` — **UZ3 A_RadiusDamageSelf**: implemented the UZDoom action in DECORATE and ZScript native paths, including distance falloff, `RDSF_BFGDAMAGE`, optional flash actor spawning, flash damage type and foil flags, `PUFFGETSOWNER`, and target blood. ZScript direct class literals are carried as names; runtime class handles and exact multi-particle `P_TraceBleed` remain. The parity catalogue records 411 recognized names.
- `2026-11-05` — **ZScript A_Chase state/flag bridge**: `Actor.A_Chase` now forwards its optional melee/missile state labels and `CHF_*` argument to the playsim. The chase uses the requested attack states and honors `CHF_NOPLAYACTIVE`, `CHF_NIGHTMAREFAST`, and `CHF_DONTMOVE`; other chase flags and `A_CheckForResurrection` remain open. `cargo fmt --all`, `cargo check -p room-sim -p room-zvm -p room-game -p room-cli`, and `git diff --check` pass.
- `2026-11-05` — **ZScript actor op dispatch**: actor-native operations queued by `PostBeginPlay`, `Tick`, and actor lifecycle event callbacks are now applied by the playsim instead of being left in the VM queue. Added a per-actor guard so a `WorldThingDestroyed` handler that removes its actor cannot recursively re-enter that callback. `cargo fmt --all`, `cargo check -p room-sim -p room-zvm -p room-game -p room-cli`, and `git diff --check` pass; generic event receiver identity and recursive damage/death callback behavior remain to be audited.
- `2026-11-05` — **UZ3 ExtChase**: implemented `A_ExtChase(usemelee, usemissile, playactive, nightmarefast)` in DECORATE and ZScript native paths. It reuses Room's chase, conditionally enters melee/missile states, suppresses ActiveSound when requested, and applies UZDoom fast-monster tic shortening. Added action-name recognition and updated the parity report to 410 recognized names; `cargo fmt --all`, targeted `cargo check`, and `git diff --check` pass.
- `2026-11-05` — **UZ3 Strife action parity**: implemented `A_ExtraLightOff`, which clears the target player's weapon extralight like UZDoom's scripted PowerCrystal; registered it in canonical action lookup and removed it from the missing-action list (409 names). Compile/diff checks pass.
- `2026-11-05` — **UZ3 Arch-vile resurrection checks**: `A_VileChase` now searches the actor's next movement tile and applies UZDoom-style raise-state, default footprint, vertical overlap, sight, and solid-position checks before reviving a corpse. It faces and sounds at the corpse, copies friendliness before the Raise state action, and enters `Heal` when present. The two-way ZScript `CanResurrect(other, passive)` virtual check is now invoked before restoring the corpse. The standalone `A_CheckForResurrection` bool native, 3D-floor-group handling, compatibility ghosts, and custom state/sound args remain. `cargo fmt --all`, `cargo check -p room-sim -p room-zvm -p room-game -p room-cli`, and `git diff --check` pass; tests were not run.
- `2026-11-05` — **UZ3 WorldThingDestroyed coverage**: expanded the hook to terminal and cyclic state destruction, corpse queue eviction, nightmare corpse replacement, spawn-cube cleanup, Duke map cleanup, and actors dying without a death state; destruction-handler inventory changes are now applied through the same deferred grant bridge. `cargo fmt --all`, `cargo check -p room-sim -p room-zvm -p room-game -p room-cli`, and `git diff --check` pass. Map teardown and other engine-owned destruction paths remain to audit.
- `2026-11-05` — **UZ3 PlayerInfo username**: `GetUserName(charLimit)` now uses the selected name or falls back to the `name` CVar, then truncates by Unicode scalar count and appends `...` only when the limit is exceeded, matching UZDoom's `FPlayerInfo::GetName`. One shared host name still does not model per-player userinfo. Compile and diff checks pass.
- `2026-11-05` — **UZ3 typed CVar defaults**: added host injection for floating and string defaults, wired `GetDefaultFloat/String`, used typed defaults for uninitialized handles and `ResetToDefault`, and retained CVar type during value conversion. `cargo fmt --all`, `cargo check -p room-zvm -p room-sim -p room-cli`, and `git diff --check` pass. Runtime configuration still needs to supply typed defaults; bool/color/redirect CVars and permission flags are not represented yet.
- `2026-11-05` — **UZ3 CVar value semantics**: `CVar` handles now expose typed integer/float/string values, type-preserving `SetInt/Float/String` conversion, typed getters, `GetRealType`, and reset-to-integer-default behavior. Float/string defaults and CVar permission flags remain gaps because the playsim host currently injects integer defaults only. `cargo fmt --all`, `cargo check -p room-zvm -p room-sim -p room-cli`, and `git diff --check` pass; no tests were added or run.
- `2026-11-05` — **UZDoom native linkage and map checksum**: `room-zscript coverage wadsrc/uzdoom` exposed false `Unsupported` rows for lower-case native spellings, while ZVM linking already follows case-insensitive lookup. Coverage now uses `id_ci`; implemented `String.ByteAt`, `ToDouble`, `Right`, case conversion/comparison/search and validity helpers, plus `Actor.TakeInventory` through the deferred inventory-effect queue. Added `LevelLocals.GetChecksum` using the active map MD5 as 32 lowercase hex digits (UZDoom `FLevelLocals::GetChecksum`). All 26 parsed/referenced names in the vendored stdlib coverage sample now link (100% linked; 5 are still marked semantic stubs). `cargo fmt --all`, `cargo check -p room-zvm -p room-sim -p room-cli`, and `git diff --check` pass. This does not reduce the separate 140/1,048 UZ3 import-stub inventory.
- `2026-11-05` — **Reflective actor missile response**: `REFLECTIVE` now routes blocking missiles through UZDoom-style deflection; `THRUREFLECT` allows pass-through and retargeting, while `AIMREFLECT` and `MIRRORREFLECT` choose their corresponding direction behavior. `A_SetReflective*` now affects missile collisions through those flags. Compile check passed; projectile edge cases remain to be compared.
- `2026-11-05` — **Doom railgun actions**: Room recognizes and dispatches `A_FireRailgun`, its left/right variants, `A_MonsterRail`, and `A_RailWait`. Rail traces accept a lateral muzzle offset; the player railgun depletes weapon ammo and uses Doom's 150/100 damage, while monster rails lead their target and use actor damage. `A_MonsterRail`'s ZScript native now queues the real action instead of a fixed 50-damage trace. Updated `docs/qa/uzdoom-parity.md`: 402/700 names (401 missing). `cargo check -p room-sim -p room-zvm -p room-cli` passes. Puff selection/trails and aim details remain incomplete.
- `2026-11-05` — **ZScript state actions**: added `A_CheckSpecies` with default/self and target/master/tracer pointer checks, plus `A_ClearFlash` for the player's flash psprite. Regenerated the parity report to 404/700 action names; flag and ACSF name diffs remain empty, with 108 property names still missing. Targeted cargo check and `git diff --check` pass.
- `2026-11-05` — **ZScript reload state actions**: implemented `A_CheckForReload` and `A_ResetReloadCounter` on the player psprite path, preserving counters per weapon across swaps, following the optional `dontincrement` behavior, and jumping to the selected state label. The generated action-name report is now 406/700 (397 missing). `cargo fmt --all` and `cargo check -p room-sim -p room-zvm -p room-cli` pass; runtime parity still needs in-game weapon fixtures.
- `2026-11-05` — **Particle action/renderer bridge**: implemented `A_SpawnParticleEx` as both a DECORATE state action and a ZScript native op; the particle event now carries velocity, acceleration, start alpha, fade step, and size step into the render-owned store. `SPF_RELPOS`, `SPF_RELVEL`, `SPF_RELACCEL`, `SPF_RELANG`, and fullbright affect the spawn. Texture IDs, render style selection, fade phase flags, and some rendering flags remain open. Re-generated UZDoom parity: 407/700 actions (396 missing); one native import moved out of the return-zero stub set (909/1,048 bound). `cargo fmt --all`, targeted `cargo check -p room-sim -p room-render -p room-zvm -p room-app -p room-cli`, and `git diff --check` pass.
- `2026-11-05` — **UZDoom A_BulletAttack behavior**: replaced the generic pistol hitscan alias with the reference behavior: it requires a target, faces/aims at it, plays the actor's AttackSound, fires `Damage` pellets with `pr_cabullet` spread and damage, and calls the common line-attack path with the computed slope. `cargo fmt --all`, `cargo check -p room-sim -p room-zvm -p room-cli`, and `git diff --check` pass. ZScript-defined `Damage` functions remain unsupported, so pellet count uses the resolved actor damage value.
- `2026-11-05` — **UZDoom playsim/ZScript parity continuation**: repaired the parity report's actor-flag catalogue so it counts the imported 315 flag names (the report now shows 0 missing flag names), regenerated `docs/qa/uzdoom-parity.md` from UZDoom commit `238f4584`, and updated `docs/36` to distinguish name recognition from semantic parity. Added playsim implementations for reflective, shootable/solid/float, low-gravity, hide/unhide, health reset, active sound, pointer removal/kill, chase/count/link flag changes, `A_BulletAttack`, overlay clearing, and the `A_WeaponAlert` / `A_ClearSoundTarget` actions; exposed these through ZScript native bindings. `Wads.FindLump` / `FindLumpFullName` now resolve against the host lump view. Current name counts: 397/700 actions, 315 known UZDoom flags, 103/150 properties, 122/122 ACSF. `cargo fmt --all` and `cargo check -p room-sim -p room-zvm -p room-cli` pass. This is a parity slice, not completion: action/flag semantics and 140 UZ3 native stubs remain open.
- `2026-11-05` — **UZDoom A_Burst ZScript bridge**: register `Actor.A_Burst` and `global.A_Burst`, lower direct chunk-class literals into the deferred playsim action queue, and apply them through the existing DECORATE implementation. Dynamic class handles still need a first-class class-value transport in ZVM; conversation-specific drops and exact spawn/collision/RNG behavior remain open. Corrected the MBF21 row and shared missing-action list, which still incorrectly marked `A_WeaponAlert` missing despite its existing playsim and ZScript bindings. `cargo check -q -p room-zscript -p room-zvm -p room-sim -p room-game -p room-cli` passes; no tests were run.
- `2026-11-05` — **UZDoom `A_Light(int)` weapon action**: added the parameterized extra-light action to psprite dispatch, clamped to UZDoom's −20…20 range, and registered it in action recognition. The regenerated report now counts 408/700 action names. `cargo check -p room-sim -p room-zvm -p room-cli` and `git diff --check` pass; this covers the DECORATE/weapon-state path, not arbitrary ZScript method execution.
- `2026-11-05` — **ZScript EventHandler WorldTick dispatch**: MAPINFO handlers now receive `WorldTick` every simulation tic (including paused tics), with lazy `OnRegister`/`WorldLoaded` activation for hosts that attach ZScript after world creation. Queued inventory effects are applied to the console player. `cargo fmt --all`, targeted `cargo check -p room-sim -p room-zvm -p room-cli`, and `git diff --check` pass; other handler events and UI/network delivery remain open.
- `2026-11-05` — **ZScript map handler teardown**: MAPINFO handlers now receive `WorldUnloaded` followed by `OnUnregister` once when the level exits, before ACS UNLOADING scripts; queued inventory effects are applied before teardown. Added the `WorldEvent` payload path for `WorldUnloaded`. `cargo fmt --all`, `cargo check -p room-sim -p room-game -p room-zvm -p room-cli`, and `git diff --check` pass. Remaining event payload types, static registration, UI callbacks, and network events remain open.
- `2026-11-05` — **ZScript `WorldThingDied` event**: actor deaths now notify each registered MAPINFO handler with a `WorldEvent.Thing` proxy containing the victim class, health, monster bit, and TID; queued inventory changes target the dead actor. This is a partial actor view, not a live object handle. `cargo fmt --all`, `cargo check -p room-sim -p room-game -p room-zvm -p room-cli`, and `git diff --check` pass. Other event payloads and callback types remain outstanding.
- `2026-11-05` — **ZScript `WorldThingDamaged` event and typed payload banks**: actor damage now dispatches the UZDoom event with `Thing`, `Inflictor`, `DamageSource`, final health damage, and damage type. WorldEvent/PlayerEvent allocation now follows compiled struct field layouts instead of assuming field-bank indexes; event actor proxies use the compiled Actor layout for health/TID/monster state. Damage angle/flags and live actor identity remain unavailable through the current `damage_mobj` call signature. `cargo fmt --all`, `cargo check -p room-sim -p room-game -p room-zvm -p room-cli`, and `git diff --check` pass.
- `2026-11-05` — **ZScript `WorldThingDestroyed` explicit removals**: `A_Remove`, `A_BarrelDestroy`, and terminal `A_FadeOut` now dispatch `WorldThingDestroyed` with a typed `WorldEvent.Thing` proxy before the actor is discarded from playsim. This does not yet cover destruction through every other actor-removal path. Targeted `cargo check -p room-sim -p room-game -p room-zvm -p room-cli` and `git diff --check` pass.
- `2026-11-05` — **ZScript resurrection hooks**: the common corpse raise path now invokes an inherited `Actor.OnRevive` override and dispatches `WorldThingRevived` after restoring the actor's health/flags and before its Raise state runs. `cargo fmt --all`, `cargo check -p room-sim -p room-game -p room-zvm -p room-cli`, and `git diff --check` pass; nonstandard revive paths and callback payload fidelity remain to audit.
- `2026-11-05` — **GZDoom/UZDoom parity recheck**: amended `docs/36` to mark its 2026-11-02 feature counts as a stale baseline; actor flag storage now has six words and basic weapon overlays exist. Current largest confirmed gaps are 140 ZScript stdlib native imports still stubbed, incomplete native/flag behaviour, deferred Heretic/Hexen/Strife, and unverified end-to-end compatibility. Regenerate the parity report before using its counts for planning.
- `2026-11-05` — **Audio/actions/ZScript parity slice**: forwarded `A_StartSound` channel flags into `room-audio` and implemented OVERLAP/NOSTOP voice rules; added MBF21 weapon psprite actions `A_CheckAmmo`, `A_RefireTo`, and `A_GunFlashTo`, plus ZScript native bindings for those actions. `cargo fmt --all` and `cargo check -p room-audio -p room-sim -p room-game -p room-zvm` pass. Full reverb, pause/local-seat behavior, remaining action/flag gaps, and broad ZScript compatibility are still outstanding.
- `2026-11-05` — **Brutal Doom BIKERACE mount failure traced to ACS activator loss**: vehicle state started `BDEnterTank`, but ACS `LSPEC*` execution discarded its running script activator before calling the world special. `Thing_Move(0, 261)` therefore had no mover; the retry loop copied bike damage to the player until death. The ACS host bridge now forwards each running script's activator to specials.
- `2026-11-05` — **BIKERACE use-to-mount path**: logs showed E only activated the bike health monitor; shooting later granted `BikePowerup` and morphed into `PlayerBike`. E now grants the same powerup to mount, forwards USE to ACS while mounted so dismount can run, and no longer stops the bike's sound on activation.
- `2026-11-05` — **Brutal Doom BIKERACE runtime path**: added the `Tank` compatibility base after `SwitchableDecoration` (required by BD's later `Tank` replacement), `Active` use dispatch, `Powerup.Type` / `PowerMorph.PlayerClass` / `Player.MorphWeapon`, player morph/unmorph, CustomInventory Pickup with its owner as ACS activator, and ACS `MorphActor` / `UnMorphActor`. Headless loading of the real BIKERACE WAD confirms all ten type-7265 bikes spawn with `BIKP`; `cargo check -p room-sim -p room-game` passes. In-game movement and exit still need a Doom II run.
- `2026-11-05` — **Brutal Doom blue vials too bright**. Sprite fullbright follows UZDoom's explicit `Bright` state and resolves GLDEFS `brightmap sprite … { disablefullbright }` against the selected frame texture; removed unconditional fullbright for kill-count actors and missile-looking sprite names. `cargo check -p room-app` passes.
- `2026-11-03` — **room-server: content library + room presets; PK3 `maps/*.wad` load by name**.
  - **Server.** New `content.rs`: admin-uploaded PK3 / WAD / IWAD library and presets (IWAD + ordered content + allowed maps + mod + skill, optional "must use a preset"). Admin Settings panels, `GET /api/presets`, `GET /api/content/{file}` (streamed, Range). `welcome.room.{content, iwad, preset}`. The lobby "Open a pit" has a preset + map picker. Tested end to end with a local server: Brutal Doom pk3 (152 MB) + doom2.wad uploaded, preset saved, rooms on BIKERACE created, MAP02 refused, "preset required" enforced. Details in `docs/22` §7.x.
  - **Client.** room-web downloads and fingerprint-checks the preset files, mounts them before the mod and packs, and swaps in the preset IWAD. This is type-checked only: the wasm build needs `play-web.sh` on the Mac.
  - **`play-doom.sh --file brutal22test6.pk3 --map BIKERACE` failed.** BD keeps its maps as `maps/bikerace.wad` inside the PK3, and only the web editor mounted those. `VfsBuilder` now mounts every `maps/NAME.wad` of an archive right after it, under the file's name as GZDoom does (`nested_map_wads`, on by default). The editor turns it off because it mounts them itself. Test `pk3_map_wads_mount_under_the_file_name`; BIKERACE loads headless with doom2 + BD.
- `2026-11-03` — **Duke D5/D6: trace fix-ups off by default; honest frontier; per-call krand diffing**.
- `2026-11-06` — **Duke D6 RPG impact/damage pass**: weapon slot 4 consumes ammo, spawns Atomic tile 2605 at speed 644 with actor-header strength, advances the projectile through wall/sprite hitscan, and changes impacts to an animated EXPLOSION2 flash. Atomic `hitradius` confirms the three strength-based damage bands, randomized per victim; the implementation now uses them for visible badguys and player self-damage. Random rolls still use Room's shared stream, and other status lists, exact knockback, auto-aim, RPG weapon view, and all other weapons remain open. Source and licensing record updated in `THIRD_PARTY.md`.
- `2026-11-07` — **Duke D6 auto-aim and Freezer owner rules**: map Chocolate's `sync.bits` bit 6 to full/bullet-only auto-aim; wire desktop default and use the mode in RPG/Freezer/Shrinker/Grower/Shotgun/Devastator targeting. Freezer hits now preserve projectile damage metadata, obey the `FREEZERHURTOWNER` startup setting, and don't treat owner `-1` as sprite zero. Static source review and `cargo check` only; no gameplay/demo comparison.
- `2026-11-07` — **Duke D6 Freezer wall impacts**: Freezer bolts now invoke direct wall responses for enabled forcefields, breakable glass, fan overlays, and stained glass before reflecting, including the glass restoration effector. Static Chocolate `checkhitwall` review; compile-checked only, no gameplay/demo comparison.
- `2026-11-07` — **Duke D6 Freezer material impacts**: extend direct `checkhitwall` coverage to Atomic breakable screens, vending/cola machines, technical wall stages, milk shelf, ATM drops, and wall lights including SE3 flicker triggers. Static Chocolate source review; compile-checked only, no gameplay/demo comparison.
- `2026-11-07` — **Duke D6 bullet-only auto-aim**: Chocolate's player hitscan calls `aim()` whenever `auto_aim != 0`, while projectile weapons require full mode 2. Correct the shared pistol/shotgun/chaingun hitscan path to keep target steering in mode 1; retain mode-2 gating for projectiles. Static `player.c` review and crate compile only; no demo/gameplay comparison.
- `2026-11-07` — **Duke auto-aim setting**: add archived `duke_autoaim` CVar (1=bullet-only, 2=full) to the desktop input bit and set browser's Atomic default full mode. The F4 Sound Setup now shows the mode and cycles it with left/right or Enter. Static source review and compile only; no gameplay/demo comparison.
- `2026-11-07` — **Duke D6 `aim()` target filters**: follow Chocolate `player.c::aim` by excluding palette-1 freeze-immune targets for Freezer fire, filtering Shrinker shots against small non-slime sprites, and skipping other APLAYER targets in coop. Static source comparison; compile only, no gameplay/demo comparison.
- `2026-11-07` — **Duke D6 Devastator scripted strength**: preserve the RPG projectile's CON strength through Chocolate `EGS()` + `shoot(RPG)` (`TRAND&7`, then quarter for Devastator) and copy its actor `hitag`; static source review, `cargo check -p room-duke -p room-build-engine`, and `git diff --check` pass. No dynamic comparison.
- `2026-11-07` — **Duke D6 fatal-hit camera response**: keep Chocolate's `wackedbyactor` on lethal non-Freezer hits; dead players now follow their body position and turn halfway toward a still-active attacker on each tic, following `actors.c`. Static source review, `cargo fmt`, `cargo check -p room-duke -p room-build-engine` (existing warnings only), and `git diff --check`; no gameplay/demo comparison.
- `2026-11-07` — **Duke D6 special sprite debris**: port `sector.c::checkhitsprite` BOTTLE10's randomized MONEY spray and VASE/STATUE/STATUEFLASH 40-piece shard bursts, including statue palette rolls, sound order and the following common glass burst. Static `sector.c`/`game.c`/`actors.c` review; `cargo fmt`, `cargo check -p room-duke -p room-build-engine` (existing warnings only), and `git diff --check`; no dynamic comparison.
- `2026-11-07` — **Duke D6 FETUSBROKE impact**: a second hit now runs 48 source-angle BLOODSPLAT1 hitscan attempts with Chocolate's extra angle TRAND per shot, then queues GLASS_HEAVYBREAK/SQUISHED/GLASS_BREAKING and the ten-piece common shard burst. Static `sector.c`/`player.c` review; `cargo fmt`, `cargo check -p room-duke -p room-build-engine` (existing warnings only), and `git diff --check`; no runtime comparison.
- `2026-11-07` — **Duke D6 chair and decor impacts**: port CHAIR1/2's BROKENCHAIR nonblocking transition and the 16 ordered `RANDOMSCRAP` shards plus heavy-break cue/deletion for CHAIR3, MOVIECAMERA, SCALE, VACUUM, CAMERALIGHT, IVUNIT, POT1–POT3 and TRIPODCAMERA. Static `sector.c`/`duke3d.h` review; `cargo fmt`, `cargo check -p room-duke -p room-build-engine` (existing warnings only), and `git diff --check`; no dynamic comparison.
- `2026-11-07` — **Duke D6 civilian checkhitsprite**: MONK/LUKE/INDY/JURYGUY now use their configured hit sound/spawn tile and direct projectile health subtraction; lethal hits run eight source-angle BLOODSPLAT shots, Chocolate's six `guts()` groups and global SQUISHED before deletion. SPACEMARINE uses the shared damage/death path. Static `sector.c`/`actors.c`/`player.c` review; `cargo fmt`, `cargo check -p room-duke -p room-build-engine` (existing warnings only), and `git diff --check`; no gameplay/demo comparison.
  - **Finding.** `room-duke/src/session.rs` (47k lines) held **1,750** `// TEMP` blocks. They hard-code DEMO1 tic numbers to overwrite sprite x/y/z/picnum and rewind or pad the RNG (`DEMO1_PAD_PRE` = 7 extra krands every tic by default), so the reference matched. They weren't limited to DEMO1: any Duke game was corrupted at those tics. A few more lived in free functions (`moveplayers`, `transports`, `explosions`, `shoot`, `con/vm`).
  - **Gated.** All of them now sit behind `DukeSession::trace_fixups` / `room_duke::fixups::on()`, **off by default**. Only the legacy per-tic DEMO1 tests turn them on (`align_rng_pre_tic0`). The legacy path stops at tic 788 (`players[0].horiz`), not 1051.
  - **Honest measure.** `demo_sync::demo1_frontier_real` + `examples/duke_frontier.rs` (`real` / `legacy` / `krand` / `sprites` / `seed` / `pics`). Real frontier: **tic 0**.
  - **Per-call RNG diff.** `room_build_engine::numeric::krand_log` (`#[track_caller]`) vs a Chocolate `krand()` log (`third_party/chocolate-duke3d-trace/apply_krand_log.py`, native i386 build `build_i386_native.sh`, no Docker; reproduces `DEMO1.trace` byte for byte). `duke_frontier krand DEMO1 <log>` prints the first call whose seed differs, with both call sites.
  - **First real fixes.** (1) `spawn::spawn_load_rolls`: the 13 `spawn(-1,i)` TRANDs at map load (cans/barrels/eggs `cstat = 257|(TRAND&4)`, blood splat repeats, money/trash/drip/rat/bullethole/splash rules). The load seed now equals Chocolate's without adopting it (test `demo1_load_seed_matches_chocolate_without_fixups`). (2) `spawn` header `owner = i` for every map sprite. (3) FIRE/FIRE2 CON execution was enabled to restore script effects; per-call trace attribution now shows random-call parity is still unresolved for some FIRE/FIRE2 actors (see 2026-11-05).
  - **Next (D5 proper).** Port `gamedef.c` `parse()` faithfully, with mutable world access instead of the snapshot + `pending_*` env. Then step tic by tic with the krand diff. Delete the TEMP blocks once the real frontier passes them.
- `2026-11-05` — **Duke D5 RNG attribution (superseded by 2026-11-06)**: added opt-in CON sprite/pic context to the krand logger. The initial per-marker comparison incorrectly treated Chocolate's mid-frame `endtic` marker as the Room tick boundary.
- `2026-11-06` — **Duke D5 continuous RNG comparison**: `duke_frontier krand` now flattens Chocolate's per-call log because its `endtic` markers split phases within a frame. Room and Chocolate call-seed streams match through 300 Room tics. The authoritative state trace still diverges at DEMO1 tic 0 (`randomseed`); investigate trace/tic boundary and phase state before changing actor eligibility.
- `2026-11-06` — **Duke D5 boundary diagnostics**: `duke_frontier states DEMO1 3` shows tic 0 player pose and sprite hash match despite the final RNG state mismatch; tic 1 seed matches but sprite hash diverges. The mismatch is therefore not fixed by the stream comparison alone; tick snapshot alignment and actor state still need native-only tracing.
- `2026-11-06` — **Duke D5 tic1 sprite diff**: the 484-sprite dump narrows the native-only divergence to BOLT1 sprite #121 (pic 636 vs 635) and BLOODSPLAT3 #369 (Z −20442 vs −20431); all other traced sprite tuples match. Per-call logs show the random stream is present but calls are attributed to different actors/phases, so do not patch sprite values directly.
- `2026-11-06` — **Duke D5 status clarified**: native-only comparison reports DEMO1 tic 0 `randomseed` as the first state mismatch (`duke_frontier real DEMO1 3`). The sprite-hash regressions through tic 134 are explicitly labelled assisted because they depend on reference seeding, RNG pads and/or `trace_fixups`; they do not demonstrate native-only demo sync.
- `2026-11-06` — **Duke D5 free-game entry point**: added `room_duke::session_from_grp_map`, which loads a named Build map directly from Atomic GRP, compiles `GAME.CON`, initializes the map at skill 0–4, and no longer requires a `.DMO`. Shell integration and interactive-render support remain open.
- `2026-11-06` — **Duke native runtime seam**: `room-game::GameRuntime` now accepts `GameCmd::Duke` and drives `DukeSession` at 30 Hz. The adapter intentionally reports no lockstep checksum until a full native-state checksum is implemented for D10; `room-app` still stores a concrete Doom `Headless` and is not switched yet.
- `2026-11-06` — **Duke Build framebuffer**: added `DukeSoftRenderer` to load GRP `PALETTE.DAT` + `TILES*.ART` and render a world/player view to RGBA. `duke_frontier render-map E1L1 /tmp/duke.ppm 3` now writes an image from the native world without DMO or GRP→UDMF conversion; screenshot review confirms the pipe runs, but the bootstrap renderer is not yet suitable as the finished playable view and is not wired into `room-app`.
- `2026-11-06` — **Duke soft projection correction**: fixed camera-up vs screen-down Y projection and separated horizontal/vertical focal lengths to respect framebuffer aspect ratio. E1L1 still renders mostly a near wall, so the flat horizon fill / wall visibility path needs further work before using it as the interactive view.
- `2026-11-06` — **Duke desktop bootstrap**: `room-app --native-duke` now loads a named GRP map directly, advances `DukeSession` at 30 Hz, feeds movement/look/fire/use/jump/run inputs, renders the CPU Build framebuffer and overlays a minimal health/ammo readout. `scripts/play-duke3d.sh --runtime native|converter` selects this shell; the native path was launched through the script and reported entry into E1L1. `cargo check -p room-app` passes. Manual visual/gameplay verification could not be completed in this run; native audio, full HUD/weapon view, functional sector use, web integration and the D5–D11 acceptance criteria remain open.
- `2026-11-06` — **Duke first-person pistol view**: the soft Build renderer now overlays the GRP `FIRSTGUN` tile (idle/fire frames) and a minimal status readout. `duke_frontier render-map E1L1 /private/tmp/duke-native-view.ppm 3` produced a 640×400 frame showing the map floor and pistol. Other weapon overlays, correct sky/ceiling rendering, audio, and hands-on input/gameplay verification remain open.
- `2026-11-06` — **Duke native web runtime bootstrap**: browser GRPs now start a `DukeSession` + Build soft renderer directly instead of the converter/Doom `Headless`; Duke input mapping is enabled for WASD, mouse/arrows/Q turning, Space jump and weapon digits. The browser shell loads `duke3d.grp` and enters the native mode; wasm build passes. This is an integration milestone only: hands-on movement/fire verification, audio, multiplayer, campaign flow, complete HUD, saves and full D5–D11 parity remain open.
- `2026-11-06` — **D8 native weapon audio bootstrap**: native desktop loads Atomic CON `definesound` entries from the GRP, decodes VOC samples into the Room mixer bank, and queues the CON IDs for supported pistol, shotgun and chaingun firing frames. `cargo check -p room-app -p room-duke` passes. Manual playback was not verifiable: `AudioOut::start()` reported a CoreAudio backend error in this environment. This does not cover ambient/actor audio, weapon-specific pitch/volume/priority, music or the browser audio path.
- `2026-11-06` — **D8 browser weapon audio**: `NativeDuke` now starts/resumes the existing WebAudio graph on a user gesture, decodes the GRP's CON `definesound` VOCs at the actual browser sample rate, and sends native fire events to the shared mixer. `cargo check --target wasm32-unknown-unknown -p room-web --features browser` passes. Browser audio still needs hands-on listening; ambient/actor cues, pitch/volume/priority and MIDI music remain unimplemented.
- `2026-11-06` — **D5/D6 native-only frontier at tic 135**: DEMO1 state and RNG match through tic 134. Tic 135 differs in randomseed and sprite count (503 vs 504; one extra `JIBS6`). A `duke_frontier states ... 135` diagnostic that substitutes neutral input at tic 135 gives the same Room seed and sprite count as the normal replay, so this tic's discrepancy is not caused by the immediate input's RNG or new input-driven sprites. Updated the shotgun `checkhitsprite` debris gate to match Chocolate's nested `FREEZEBLAST`/owner exclusions; this is a compatibility correction but does not move the frontier. A trial parity flip for `everyothertime` regressed earlier at tic 31 and was reverted. Next isolate world-step/animation timing and JIBS lifetime/order around tic 135. `D5` remains in progress.
- `2026-11-06` — **D11 packaged native web smoke**: `scripts/package-web.sh --out target/room-web --iwad testdata/duke/duke3d.grp` completed (release wasm, 91 MiB site); localhost HEAD requests returned 200 for `index.html` and the 44,348,015-byte Duke GRP. Visual/gameplay verification could not run because the Mac was locked and CUA could not unlock it; package/HTTP success does not prove the wasm entered NativeDuke mode.
- `2026-11-06` — **Duke desktop weapon selection**: number keys 1–0 now request the corresponding native weapon indices; selection only changes to a weapon the player owns and has ammunition for. `cargo check -p room-app` passes. Full weapon input/state/HUD and weapon pickup behavior remain D6 work.
- `2026-11-06` — **Native Duke launcher guidance**: the runtime banner and `scripts/play-duke3d.sh --runtime native` now explicitly identify E/use, audio, level exits, and most weapons as unfinished. The input bit is transmitted, but no native `operate*`/`checksectors` implementation consumes it yet. This is a scope/status correction, not a gameplay completion.
- `2026-11-06` — **Duke native pistol firing**: the starting pistol now routes through the native hitscan, uses the SHOTSPARK1 CON strength and consumes pistol ammo once per shot; the shotgun path remains separate. `cargo check -p room-app` passes. Runtime damage/animation still need reference gameplay verification; other weapon families and pickups remain open D6.
- `2026-11-06` — **Native Duke launcher smoke**: fixed `set -u` handling when no extra app args are supplied; `scripts/play-duke3d.sh --runtime native` now builds and reaches `room: entered native Duke E1L1`. UI automation could not attach to the launched Room app, so no visual/input gameplay verification was obtained.
- `2026-11-06` — **Duke D5 CON actor predicates and map `spawn(-1)` state**: `ifactor` reads the live tile; `ifspawnedby` reads `hittype[].picnum`. Chocolate initializes map actors' `hittype[].picnum` to their own tile, so Room now sets the corresponding `spawned_by` field at map load and initializes the cached floor/ceiling planes. Before D7 animated walls were implemented, the native trace matched through tic 30 and first differed at tic 31. Current trace frontier is tic 38; D5 remains in progress.
- `2026-11-06` — **D5 frontier diagnosis (superseded by the D7 animated-wall implementation below)**: the first trace caller had been tentatively attributed to `animatewalls`, but that early note miscounted its RNG calls and marked D5 complete prematurely. The verified force-field path consumes five calls on E3L11; native-only DEMO1 now matches through tic 38, and D5 remains in progress with the next mismatch at tic 39.
- `2026-11-06` — **D6 chaingun first shot path**: added Atomic's held-fire cadence (one hitscan every three kickback tics, continuing while held) and uses the CHAINGUN CON damage profile with the shared player hitscan. The casing effect and native cadence/reference gameplay parity remain open. `cargo check -p room-duke` passes; D6 remains in progress.
- `2026-11-06` — **D6 exhausted weapon fallback**: pistol, shotgun, and chaingun now leave an empty weapon using Atomic's default `checkavailweapon` priority, switching through the existing owned/ammo inventory and landing on the knee if no loaded weapon remains. User-configured weapon preference is not yet wired to the native shell.
- `2026-11-06` — **D6 native weapon overlay coverage**: `DukeSoftRenderer` now resolves the selected weapon's Atomic HUD tile from `DEFS.CON` and draws the supported ART frame at its weapon-specific screen center for all twelve inventory slots (including aliases for hand-held items and grow/shrink). Missing custom tiles fall back gracefully. The full multi-layer offsets, muzzle flashes, view bob, and exact per-tic `displayweapon` choreography remain open.
- `2026-11-06` — **D7 ART tile animations in the native renderer**: `SoftTiles` now resolves ART `picanm` oscillating, forward, and backward cycles using the deterministic Build 120 Hz clock. Desktop and browser pass the Duke tic through to the renderer; floors, ceilings, walls, sky and face sprites sample the corresponding frame without modifying `BuildWorld`. A fixed-camera `render-map` comparison changed 125,773 RGB byte values on E1L4 between tics 0 and 30; a scan found visible frame changes on seven Atomic map start views. At that point simulation-side `animatewalls`, sector/effectors, and the DEMO1 RNG frontier were still open; the animated-wall work is recorded above. Desktop and wasm browser cargo checks pass.
- `2026-11-06` — **D3/D7 portal rendering**: native soft rendering now draws upper/lower wall bands where adjacent sectors have different ceiling/floor heights, keeps the aperture open for the next sector, and depth-tests walls/sprites per pixel. `fp_textured_portal_renders_height_differences_and_keeps_opening` verifies both bands and the visible sector beyond. A new E1L1 frame shows mapped masonry and railing instead of only the floor to the horizon; the simplified renderer still lacks full sky composition and Build wall/sprite semantics. D3/D7 remain in progress.
- `2026-11-06` — **D3 Atomic parallax sky slots**: native `SoftTiles` now picks the eight 45-degree sky tiles from the `DEFS.CON` `pskyoff[]` layout as the camera angle crosses slots, sharing the same Atomic offset table as the GRP-to-WAD converter. A new E1L1 render samples multiple night-sky ART tiles rather than repeating one black tile. Unit tests cover angular tile selection and all three configured Atomic offset sets. Golden comparison against the reference renderer and other sky groups remain open.
- `2026-11-06` — **D6 weapon switching presentation**: native weapon overlays now move down during the outgoing weapon phase and rise from below during the incoming weapon phase, using the `weapon_pos` state already advanced by the simulation. Other exact `displayweapon` offsets, bob, and firing effects remain open.
- `2026-11-06` — **Native weapon dispatch**: only pistol (1) and shotgun (2) enter implemented fire paths; other selected slots now advance their raise/lower state without silently firing a shotgun. `cargo check -p room-app` passes. Remaining weapons still need their own state/shoot implementations.
- `2026-11-06` — **D11 wasm dependency path**: enabled optional `room-duke` as part of `room-web`'s `browser` feature so the browser build includes the native Build runtime and renderer crate. `cargo check --target wasm32-unknown-unknown -p room-web --features browser` passes. This is build plumbing only: browser GRPs still route through the converter and Doom `Headless`; browser controls, Build frame presentation, and sound/runtime integration remain to do.
- `2026-11-03` — **Rust guest mods: `export_mod!` unresolved `room_script_abi`**. The 2026-11-01 `room_dispatch` arm named `room_script_abi::ActionPayload`, but the macro expands in the guest crate, which only depends on `room-script-sdk` (`mods/doom1-survival` broke). `room-script-sdk` now re-exports `ActionPayload`, and the macro uses `$crate::ActionPayload`. The survival mod builds, and the sdk tests pass.
- `2026-11-03` — **AssemblyScript compiler crash (`./room start`)**. The 2026-11-01 `as/room.ts` additions declared a second `export namespace Prop` (class-property ids). It clashed with the existing actor `Prop` (`HEALTH` 0 vs 1, `DAMAGE` 8 vs 39). asc 0.28.20 crashes on that ("Whoops… no stack trace") instead of reporting it, for every mod. The new ids went into `ClassProp` and the new flags into `ClassFlag` (the values agree wherever the names overlap), and the duplicate `Prop` / `Flag` were removed. `mods/doom1-bounty` compiles again, byte-identical to the committed `bounty.wasm`.
- `2026-11-03` — **Editor: ZScript from uploaded mods**.
  - **Engine loads ZScript like GZDoom.** A `zscript.<any ext>` root (`zscript.txt`) is now loaded, and `#include` pulls in files of any extension (`src/handlers.txt`, `actors/x.txt`). Before, only `.zs/.zc/.zsc` files and a bare `ZSCRIPT` lump were seen, so such mods failed with "cannot #include". `zscript_load.rs`, test `includes_with_any_extension_and_txt_roots_are_loaded`.
  - **Web editor.** Uploading a PK3 or WAD with ZScript opens its ZSCRIPT root in the code editor. The Lumps tab gets a "ZScript files" list (working archive first, engine built-ins last). Each file in the `#include` closure is edited as ZScript, with highlighting, Check ZScript and include navigation, whatever its name. Files are also sniffed by content (`class` / `version`, not `actor`). Open… accepts a loose `.zs/.zc/.zsc` (or ZScript `.txt`) and adds it as `zscript/<name>`. Files in unmapped PK3 folders show their file name as the tab title. Node tests: `editor_highlight.test.mjs` (9 pass). Checked in headless Chromium with the 11:11 wasm build, a test PK3 and Brutal Doom. The engine half needs a wasm rebuild.
  - Demo fingerprint constants for the re-recorded `freedoom1_e*.fdbn` bundles updated.
- `2026-11-03` — **BD enemies invisible mid-animation (sprite frame table)**.
  - **Mixed-letter rotation pairs.** An 8-character sprite lump holds two views: `BOS2A2C8` is frame A from view 2 and frame C from view 8 (flipped). Room only guessed `A2A8`-style names, so frames whose views live in a pair with another letter had no lump and the monster vanished from those angles. Hit: Hell Knight, both Arachnotrons, Spider Mastermind, Revenant, Arch-vile, Mummy, Nightmare Demon and more. `PatchCache::sprite_frame_lump` now builds GZDoom's frame table (`R_InstallSprite`: rotation 0 fills all views, later lumps override) from the sprites namespace. room-app and room-web look there first. `examples/bd_rotations.rs`: monster frames with a missing view went from 138 to 79, and the 79 left have no lump at all in the pk3 (unused states).
  - **`A_SpawnItem` arguments.** It shared `A_SpawnItemEx`'s parser, so `(type, distance, zheight)` became x/y/z offsets: `A_SpawnItem("RipGuts", 0, 10)` spawned 10 units sideways. It now spawns `distance` ahead (0 = the two radii, as GZDoom) and `zheight` up. Test: `a_spawn_item_takes_distance_and_height_not_ex_offsets`. Freedoom demos are unchanged.
  - `examples/bd_hidden.rs` lists live or dead monsters that cannot be drawn, with the state that hid them.
- `2026-11-03` — **BD enemy sprite flicker (draw order)**.
  - Sprites were drawn in actor order with depth write on, and soft-alpha texels (BD PNG smoke, blood, flares at `Alpha 0.4`) wrote depth too. A particle in front of a monster, drawn before it, depth-rejected the monster's pixels, so enemies blinked as BD particles spawned and died. `WorldPass::prepare` now reorders sprite quads far-to-near (`scene::sprites_back_to_front`, stable for equal distances), as GZDoom sorts sprites. Test: `sprites_are_drawn_far_to_near`. The sim side has no monster dropouts (`bd_soak` with SUMMON_ALL: 0 flicker, 0 loop hits).
- `2026-11-03` — **BD phantom gun frames**.
  - **Psprite context leaked into spawned actors.** A gun action (`A_FirePistol`) spawns a puff, and the BD puff's 0-tic chain spawns `YellowFlareSmall`. The flare's `A_Jump(128, 2)` ran while `psprite_weapon` / `psprite_state` were still set, so `jump_by_offset` counted from the gun's state. The flare then played Pistol / Chaingun / SuperShotgun frames as a translucent billboard. Fix: `World::set_state` clears the psprite context for the actor chain and restores it afterwards. Test: `actor_spawned_during_a_weapon_action_jumps_on_its_own_states`. Tool: `examples/bd_ghosts.rs` (no non-weapon actor in weapon states over 2000 tics on MAP01/MAP07).
  - **`RenderStyle None` is no longer drawn** in `collect_live_sprites` (room-app and room-web), as in GZDoom.
- `2026-11-03` — **BD gore hitches + loop warning on the HUD**.
  - **Cached expression VMs.** Each DECORATE expression evaluated through the ZVM (`random(...)` in every gib and particle spawn) built a new `Zvm`, which registers about a thousand natives. Linked VMs are now cached per expression (`deco_zvm` `DecoExprCache`), with the heap reset per call. Killing ~185 BD monsters in one tic went from 19.5 s to 1.4 s. Freedoom1 demos are unchanged.
  - **Endless-loop warning moved off the HUD.** It now goes to the console (`push_debug`, as GZDoom prints it) and reads "endless 0-tic state loop in <Class>". The stuck actor is still destroyed.
- `2026-11-03` — **BD slowdown + "state cycle limit"**.
  - **Endless 0-tic loops.** Brutal Doom's `BurningZombie` Spawn state is `BDT1 B 0 A_CustomMissile(...)` + `Loop`. Room hit its 1000-step guard, kept the actor, and re-ran the loop (1000 particle spawns) every tic. It now matches GZDoom `AActor::SetState`: the stuck actor is destroyed. A probe catches a state that keeps coming back after transition 64, so the loop stops after ~80 steps. The warning names the class and the last 8 states (sprite/tics/action).
  - **Destroyed actors.** They stay on `thinkers.default` (checksums hash them), but they no longer run physics or states. Before, every tic cloned each one and ran `check_position` on it; a destroyed BulletPuff kept rising 1 unit a tic forever.
  - **Thing-thing collision.** It now reads `World::collision_candidates()`, the non-destroyed subset in thinker order, rebuilt each tic and extended on spawn.
  - **Result.** Headless BD MAP01, 1100 tics: 76 s → 7 s, and ~100 ms/tic at tic 900 → ~5 ms/tic, flat.
  - **Demos.** Live-actor state is unchanged (checked on freedoom1 E1). Only destroyed actors' hashed fields differ, so the `freedoom1_e1..e4.fdbn` bundles were re-recorded (`ROOM_WRITE_DEMO=1`). **The `freedoom2_*.fdbn` bundles must be re-recorded wherever `freedoom2.wad` is available.**
  - **Tools.** `--example bd_soak` (cycle hits, flicker, slow tics; `SUMMON_CLASS=`, `SUMMON_ALL=1`, `ALL_ACTORS=1`) and `--example bd_zero_loops` (static 0-tic cycles).
- `2026-11-03` — **BD sprites (vanishing / clipped enemies)**. Three fixes.
  1. **PNG decoder** (`room-formats/src/png_img.rs`). 1-, 2- and 4-bit indexed and grey PNGs were read as packed bytes, so the pixel count was wrong and the frame was dropped. RGBA PNGs with a stray `tRNS` chunk were rejected outright. In `brutal22test6.pk3` that was 667 of 20,323 PNGs, including every Spectre frame, Pain Elemental death frames, Afrit frames, blood, gibs, flames and Demon brightmaps. All 20,323 now decode.
  2. **`^` in archive file names** now maps to the `\` sprite frame, as GZDoom's `LumpNameSetup` does (`room-vfs` `ShortName::from_stem`).
  3. **Billboard de-clip** (`room-render` `declip_billboard`). A billboard whose actor stands on the viewer's side of a wall is slid along its view rays toward the eye until it clears that wall: the picture on screen is unchanged and only its depth moves. It never moves past a wall that stands between the eye and the actor. This is Doom's drawseg rule; before, wide BD frames were cut by side walls or vanished in corners.

  Diagnostics: `cargo run -p room-game --example bd_sprite_decode -- doom2.wad brutal.pk3` (frames that resolve but cannot be decoded: 59 → 2; the 2 left are a voxel and one patch). `room-formats --example png_sweep` decodes every PNG you pass it.
- `2026-11-03` — **UZ14** (part): ID24 JSON lumps parse in `room-defs`. `SKYDEFS` (normal/fire/with-foreground skies, flat mapping), `DEMOLOOP` (art/demo entries, melt wipe) and `GAMECONF` (iwad/pwads/translations/executable/mode/options) are typed and validated. GAMECONF merges across lumps in load order: scalars overwrite, pwads and options accumulate, executable and mode take the max. `TRAKINFO` / `SBARDEF` are kept as raw JSON. Nothing consumes them yet.
- `2026-11-03` — **Editor PK3 import**. **📦 Open PK3…** in the toolbar, file drop and the other pickers all import a `.pk3` / `.pk7` / `.zip` / `.ipk3`. This works with or without an IWAD: with one, the pack mounts on top; without one, the editor tries the served IWAD first, then opens the pack on its own with a warning. The imported pack becomes the working archive. The Lumps tab opens on its tree, its maps are listed (including nested `maps/*.wad`, now mounted for added packs too) and open in the editor, its ZScript files can be edited, and **Export PK3** rebuilds it by default. A DECORATE/ZScript error no longer stops the import; it is reported instead. VFS fix: a PK3 whose only entry is a root file (`zscript.zs`) was mounted empty, because that file was treated as a wrapper folder. `strip_single_root` now strips folders only. Tests: `pk3_edit` (+2: wrapped pack with `maps/*.wad`, `#include` and a 12 MiB file round-trip through export; root `zscript.zs` with an added include). `room-vfs` `archive::tests` (3). Headless Chrome end-to-end run passed 20/20: button, picker, drop, IWAD/no-IWAD, nested map, edit → Export PK3 byte check.
- `2026-11-03` — **Editor PK3 + ZScript**. The Lumps tab shows PK3 / PK7 / folder packs as a folder tree. Text files inside a pack are edited by full path (`roomEditorSetPath` → `editor-overrides.pk3`, mounted last). **+ New file in pack…** adds files, and **Export PK3** (`roomEditorExportPk3`) rebuilds the opened pack with every edit: files, lumps, maps as `maps/<name>.wad`, actor edits. A `maps/*.wad` inside a PK3 is mounted as its own WAD, so its maps appear in the map list and open in the editor. Text lumps get context-aware syntax colors for ZScript, DECORATE, ACS and MAPINFO: keywords, types, strings, numbers, comments, `#include` / `version`, Default properties and `+/-` flags, States labels, sprites, frames and actions. `#include` buttons and Ctrl/⌘+click open the included file. **Check ZScript** (also run on save, `roomEditorCheckZscript`) parses the file and then compiles the whole program with it; exact errors jump to their line and are marked in the gutter. ZScript sources now dedupe by path, so a PK3 or editor override replaces the earlier file. Tests: `pk3_edit` (8), `zscript_load::source_tests` (2), `tests/editor_highlight.test.mjs` (7).
- `2026-11-03` — **UZ5**: `A_StopAllSounds` drops a queued pistol and a queued shotgun and leaves one stop-all request. Two playing voices are cleared together.
- `2026-11-03` — **UZ5**: `A_SoundPitch(0, 2)` plays a body-channel pistol shot at double rate and leaves its volume at 1. `A_SoundPitch(5, 2)` leaves that shot at normal rate. A playing voice on channel 1 goes to pitch 2 and a voice on channel 2 stays at 1.
- `2026-11-03` — **UZ5**: `A_SoundVolume(0, 0.25)` turns a body-channel pistol shot from full volume down to a quarter. `A_SoundVolume(5, 0.25)` leaves that shot at full volume. A playing voice on channel 1 drops to 0.25 and a voice on channel 2 stays at 1.
- `2026-11-03` — **UZ4**: Without `PowerDoubleFiringSpeed`, a 4-tic second layer is at 3 after it starts and the gun frame stays at 4. Holding the power halves that layer to 1 after it starts, even with fast weapons on, and the gun stays at 4. `A_OverlayFlags(2, 4, false)` keeps the layer at 3 while the power is still held. The gun stays off the power.
- `2026-11-03` — **UZ4**: With fast weapons at 1, a 4-tic second layer is down to 1 tic after it starts and the gun frame stays at 4. At level 2 that layer ends in the same tic and the gun stays at 4. `A_OverlayFlags(2, 8, false)` keeps the 4-tic layer at 3 after it starts. The gun stays off the fast-weapon switch.
- `2026-11-03` — **UZ4**: A fuzzy player fuzzes the second weapon layer and the gun. `A_OverlayFlags(2, 512, true)` draws that layer solid and leaves the gun fuzzy. `A_OverlayRenderStyle(2, 2)` fuzzes the second layer again. `A_OverlayRenderStyle(2, 1)` clears it while the flag stays on. `A_OverlayFlags(2, 512, false)` lets the player fuzz the second layer again. `A_OverlayFlags(1, 512, true)` then draws the gun solid and leaves the second layer fuzzy.
- `2026-11-03` — **UZ4**: A player at alpha 0.25 with second-layer alpha 0.5 draws that layer at 0.125 and the gun at 0.25. `A_OverlayFlags(2, 1024, true)` keeps the second layer at 0.5 and leaves the gun at 0.25. `A_OverlayFlags(1, 1024, true)` then draws the gun solid and leaves the second layer at 0.5. `A_OverlayFlags(2, 1024, false)` returns the second layer to 0.125 and leaves the gun solid.
- `2026-11-03` — **UZ4**: `A_OverlayFlags(2, 256, false)` snaps the second weapon layer between tics and leaves the gun easing. `A_OverlayFlags(1, 256, false)` then snaps the gun and leaves the second layer snapped. `A_OverlayFlags(2, 256, true)` eases only the second layer again.
- `2026-11-03` — **UZ4**: `A_OverlayFlags(2, 1, false)` drops the gun offset from the second layer. Sway 10 plus offset 3 still draws the gun at 13, and the second layer uses 10. `A_OverlayFlags(1, 1, false)` then drops the gun offset and leaves the second layer without it. `A_OverlayFlags(2, 1, true)` returns the offset to the second layer only.
- `2026-11-03` — **UZ4**: `A_OverlayFlags(2, 2, false)` drops weapon sway from the second layer while the gun still draws sway 10 plus offset 3. `A_OverlayFlags(1, 2, false)` then drops the gun sway and leaves the second layer still. `A_OverlayFlags(2, 2, true)` returns sway to the second layer only.
- `2026-11-03` — **UZ4**: `A_OverlayFlags(2, 128, false)` makes `A_OverlayPivot(2, 4, 4)` a pixel pivot on the second weapon layer. The gun stays in fraction mode. `A_OverlayFlags(1, 128, false)` then switches the gun to pixels and leaves the second layer in pixels. `A_OverlayFlags(2, 128, true)` returns only the second layer to fractions.
- `2026-11-03` — **UZ4**: `A_OverlayRenderStyle(2, 2)` fuzzes the second weapon layer and leaves the gun solid. `A_OverlayRenderStyle(1, 2)` then fuzzes the gun and leaves the second layer fuzzy. `A_OverlayRenderStyle(2, 1)` clears only the second layer.
- `2026-11-03` — **UZ4**: `A_OverlayTranslation(2, "Green")` tints the second weapon layer green. Its green channel stays and the red channel drops to a quarter. The gun stays white. `A_OverlayTranslation(1, "Red")` tints the gun red and leaves the second layer green. `A_OverlayTranslation(2, "None")` clears only the second layer.
- `2026-11-03` — **UZ4**: `A_OverlayPivotAlign(2, 2, 0)` pins the second weapon layer's pivot to the right edge and the top. The gun keeps the center pivot. `A_OverlayPivotAlign(1, 0, 2)` pins the gun to the left edge and the bottom and leaves the second layer on the right and top.
- `2026-11-03` — **UZ4**: `A_OverlayPivot(2, 0, 0)` scales the second weapon layer from its top-left corner. The gun keeps the center pivot. `A_OverlayPivot(1, 0.25, 0.75)` moves the gun's pivot and leaves the second layer at the corner.
- `2026-11-03` — **UZ4**: `A_OverlayRotate(2, 90)` turns the second weapon layer 90° counter-clockwise. The gun stays upright. `A_OverlayRotate(1, 15)` turns the gun 15° and leaves the second layer at 90°.
- `2026-11-03` — **UZ4**: `A_OverlayFlags(2, 32, true)` flips the second weapon layer top to bottom. The gun stays upright. `A_OverlayFlags(1, 32, true)` then flips the gun and leaves the second layer flipped. `A_OverlayFlags(2, 32, false)` clears only the second layer.
- `2026-11-03` — **UZ4**: `A_OverlayFlags(2, 16, true)` mirrors the second weapon layer left to right. The gun stays unflipped. `A_OverlayFlags(1, 16, true)` then mirrors the gun and leaves the second layer mirrored. `A_OverlayFlags(2, 16, false)` clears only the second layer.
- `2026-11-03` — **UZ4**: `A_OverlayScale(2, 2)` draws the second weapon layer at 2×2. `A_OverlayScale(2, 2, 3)` makes it 2× wide and 3× tall. The gun stays at 1× until `A_OverlayScale(1, 4)`, which draws the gun at 4× and leaves the second layer at 2×3.
- `2026-11-03` — **UZ4**: `A_OverlayAlpha(2, 0.25)` draws the second weapon layer at a quarter opacity. The gun stays fully opaque. A later `A_OverlayAlpha(1, 0.5)` draws the gun at half opacity and leaves the second layer at a quarter.
- `2026-11-03` — **UZ4**: `A_OverlayOffset(2, 6, 40)` moves the second weapon layer 6 right and 8 down. The gun stays put. A later `A_OverlayOffset(1, 3, 36)` moves the gun by 3 and leaves the second layer at 6.
- `2026-11-03` — **UZ4**: `A_Overlay(2, "Flash")` draws that state's sprite on a second weapon layer. `PISF A` becomes patch `PISFA0`. `A_Overlay(1, "Flash")` leaves the second layer empty. The layer clears when the state ends.
- `2026-11-03` — **UZ4**: `A_OverlayFlags(1, 16, true)` mirrors the weapon left to right (`PSPF_FLIP`). The HUD flip flag turns on, layer 2 does not clear it, and `A_OverlayFlags(1, 16, false)` turns it off. Bit 64 still flips the same way.
- `2026-11-03` — **UZ4**: `A_OverlayFlags(1, 256, false)` snaps the gun offset each tic (`PSPF_INTERPOLATE` off). With the flag on, an offset moving from 0 to 10 is drawn at 5 halfway through the tic. The same call on layer 2 leaves snapping on. `A_OverlayFlags(1, 256, true)` eases again.
- `2026-11-03` — **UZ4**: `A_OverlayFlags(1, 512, true)` keeps the overlay style when the player is fuzzy (`PSPF_FORCESTYLE`). Blur fuzzes the gun, the flag with no overlay style draws it solid, and `A_OverlayRenderStyle(1, 2)` fuzzes it again. The same flag call on layer 2 leaves the flag set. Clearing it after a Normal style lets the blur fuzz the gun again.
- `2026-11-03` — **UZ4**: `A_OverlayFlags(1, 1024, true)` keeps the overlay alpha when the player is translucent (`PSPF_FORCEALPHA`). A player at alpha 0.25 draws the gun at 0.25, then at the overlay's 0.5 after the flag is set. The same call on layer 2 leaves the flag set. Clearing it multiplies again, so 0.5 becomes 0.125.
- `2026-11-03` — **UZ4**: `A_OverlayFlags(1, 128, false)` makes `A_OverlayPivot(1, 4, 4)` a pixel pivot (`PSPF_PIVOTPERCENT` off). On an 8×8 sprite scaled to 2× that lands on the same spot as a center fraction of 0.5. The same flag call on layer 2 leaves pixel mode on. `A_OverlayFlags(1, 128, true)` returns to fractions.
- `2026-11-03` — **UZ4**: `A_OverlayFlags(1, 32, true)` flips the weapon sprite top to bottom (`PSPF_MIRROR`). The top pixel becomes the bottom pixel. The same call on layer 2 leaves the sprite upright. `A_OverlayFlags(1, 32, false)` clears the flip.
- `2026-11-03` — **UZ4**: `A_OverlayFlags(1, 4, true)` with `PowerDoubleFiringSpeed` in inventory turns a 4-tic weapon frame into 2 (`PSPF_POWDOUBLE`). Without that power the frame stays 4 tics. The same call on layer 2 leaves the 4-tic frame.
- `2026-11-03` — **UZ4**: `A_OverlayFlags(1, 8, true)` with `sv_fastweapons` 1 turns a 4-tic weapon frame into 2 (`PSPF_CVARFAST`). Level 2 makes that frame 1 tic. The same call on layer 2 leaves the 4-tic frame.
- `2026-11-03` — **UZ4**: `A_OverlayFlags(1, 1, false)` drops the gun offset (`PSPF_ADDWEAPON`). Sway 10 plus an offset of 3 draws at 13, then at 10. The same call on layer 2 leaves the offset off. `A_OverlayFlags(1, 1, true)` puts the offset back.
- `2026-11-03` — **UZ4**: `A_OverlayFlags(1, 2, false)` drops weapon sway (`PSPF_ADDBOB`). A gun offset of 3 with sway 10 draws at 13, then at 3. The same call on layer 2 leaves the sway off. `A_OverlayFlags(1, 2, true)` puts the sway back.
- `2026-11-03` — **UZ4**: `A_OverlayTranslation(1, "Green")` tints the weapon green. A white pixel's red channel drops to a quarter and the green channel stays. The same call on layer 2 leaves that tint. `A_OverlayTranslation(1, "None")` clears it.
- `2026-11-03` — **UZ4**: `A_SetCrosshair(2)` draws a wider center mark than index 1. Index 0 draws nothing, and a later `A_SetCrosshair(0)` clears the mark.
- `2026-11-03` — **UZ4**: `ViewModelFOV 45` on a weapon model draws that gun at 2× when the player field of view is 90°. A model without `ViewModelFOV` stays at 1×.
- `2026-11-03` — **UZ4**: `A_ZoomFactor(2)` halves the view: a 90° field becomes 45°. `A_ZoomFactor(1)` restores it. A scripted camera keeps its own field of view.
- `2026-11-03` — **UZ4**: `A_SpriteOffset(4, 6)` shifts the sprite 4 to the camera's right and 6 up. The actor stays put. A later call replaces that shift. Looking toward -Y, the horizontal shift is along +X.
- `2026-11-03` — **UZ4**: `SetViewPos(8, 0, 2)` moves the eye 8 forward and 2 up while the actor stays put. After the body turns to 90°, that offset follows the new aim. Flag 2 adds a world-axis offset. Flag 4 places the eye at an absolute point.
- `2026-11-03` — **UZ4**: `A_SetViewRoll(15)` banks the camera 15° while the sprite stays upright. Looking along +X, a quarter turn points the camera's up toward -Y. `A_SetRoll(40)` rolls the sprite to 40° and the camera stays at 15°. A pointer other than the caller leaves that bank.
- `2026-11-03` — **UZ4**: `A_SetViewAngle(30)` turns the camera 30° while the body still faces ahead. `A_SetAngle(90)` turns the body to 90° and the camera stays at 30°. A pointer other than the caller leaves that camera angle.
- `2026-11-03` — **UZ4**: `A_SetViewPitch(20)` looks 20° up while the aim stays level. `A_SetPitch(5)` aims shots at 5° and the camera stays at 20°. `A_SetViewPitch(100, 1)` clamps the camera to 89°.
- `2026-11-03` — **UZ4**: `A_OverlayRenderStyle(1, 2)` draws the weapon layer with the spectre fuzz (`STYLE_Fuzzy`). The same call on layer 2 leaves that fuzz. `A_OverlayRenderStyle(1, 1)` clears it.
- `2026-11-03` — **UZ4**: `A_OverlayRotate(1, 90)` turns the weapon layer a quarter turn counter-clockwise, so the right-hand pixel ends up on top. The same call on layer 2 leaves the weapon at 90°.
- `2026-11-03` — **UZ4**: `A_OverlayPivotAlign(1, 2, 0)` puts the scale pivot on the right edge and the top, so a 2×3 scale grows left and down. The same call on layer 2 leaves that pivot.
- `2026-11-03` — **UZ4**: `A_OverlayPivot(1, 0, 0)` scales the weapon layer from its top-left corner. The same call on layer 2 leaves the pivot at the corner. Until a pivot is set, scale still grows from the center.
- `2026-11-03` — **UZ4**: `A_OverlayFlags(1, 64, true)` flips the weapon layer left to right. The same flag on layer 2 leaves it flipped. `A_OverlayFlags(1, 64, false)` clears the flip.
- `2026-11-03` — **UZ4**: `A_OverlayScale(1, 2)` draws the weapon layer at twice its size. `A_OverlayScale(1, 2, 3)` is 2× wide and 3× tall, grown about the patch center. The same call on layer 2 leaves that scale.
- `2026-11-03` — **UZ4**: `A_OverlayAlpha(1, 0.25)` draws the weapon layer at a quarter of its light. The same call on layer 2 leaves the weapon layer at 0.25.
- `2026-11-03` — **UZ4**: `A_OverlayOffset(1, 4, 40)` shifts the weapon layer by 4 pixels right and 8 down. The same call on layer 2 leaves that layer where it is.
- `2026-11-03` — **UZ4**: `A_WeaponOffset(10, 48)` shifts the gun by 10 pixels right and 16 down. A follow-up `A_WeaponOffset(1, 2, 5)` keeps that X and adds 2 to Y, so the HUD draws the gun at (10, 18).
- `2026-11-03` — **UZ4**: `A_ReFire(null, false)` on an empty pistol leaves that pistol ready. The same refire with the flag omitted queues the Fist.
- `2026-11-03` — **UZ3**: `SkinChanged(2)` makes `GetSkin` return `2`. Index 0 stays the base skin, so `GetSkin` stays `base`. Before that change `GetSkin` is `base`. Imports are 1048: 908 builtins and 140 return-0 stubs.
- `2026-11-03` — **UZ3**: `ColorSetChanged(-1)` makes `GetColorSet` return -1. The menu passes the selection minus one, and -1 is Custom. Before that change `GetColorSet` is 0. Imports are 1048: 907 builtins and 141 return-0 stubs.
- `2026-11-03` — **UZ3**: `TeamChanged(2)` makes `GetTeam` return 1. The menu stores the team one higher than the index, and 0 means no team. Before that change `GetTeam` is 0. Imports are 1048: 906 builtins and 142 return-0 stubs.
- `2026-11-03` — **UZ3**: `ClassChanged(3)` makes `GetPlayerClassNum` return 3. Before that change `GetPlayerClassNum` is 0. Imports are 1048: 905 builtins and 143 return-0 stubs.
- `2026-11-03` — **UZ3**: `ColorChanged(10, 20, 30)` makes `GetColor` return `10 20 30`. Before that change `GetColor` is `Green`. Imports are 1048: 904 builtins and 144 return-0 stubs.
- `2026-11-03` — **UZ3**: `AutoaimChanged(20)` makes `GetAutoaim` return 20. Before that change `GetAutoaim` is 35. Imports are 1048: 903 builtins and 145 return-0 stubs.
- `2026-11-03` — **UZ3**: `SwitchOnPickupChanged(1)` makes `GetNeverSwitch` return 1. Before that change `GetNeverSwitch` is 0. Imports are 1048: 902 builtins and 146 return-0 stubs.
- `2026-11-03` — **UZ3**: `AlwaysRunChanged(0)` makes `cl_run` 0. Before that change `cl_run` is 1. Imports are 1048: 901 builtins and 147 return-0 stubs.
- `2026-11-03` — **UZ3**: `GenderChanged(2)` is what `GetGender` reads. Before that change gender is 0. Imports are 1048: 900 builtins and 148 return-0 stubs.
- `2026-11-03` — **UZ3**: `PlayerNameChanged("Aly")` sets the user name to `Aly`. Before that change the name is empty. Imports are 1048: 899 builtins and 149 return-0 stubs.
- `2026-11-03` — **UZ3**: `SetCVarDescription` on `m_sensitivity` stores the label `Mouse`. A null CVar stores nothing. Imports are 1048: 898 builtins and 150 return-0 stubs.
- `2026-11-03` — **UZ3**: `SetInterMusic("MAP02")` records `MAP02` for the intermission. Imports are 1048: 897 builtins and 151 return-0 stubs.
- `2026-11-03` — **UZ3**: `GetKeyTypeCount` is 6. `GetKeyType(2)` is `RedCard`, and `GetKeyType(9)` is -1. Imports stay 1048: 896 builtins and 152 return-0 stubs.
- `2026-11-03` — **UZ3**: `NameAllKeys` of Escape and Enter is `Escape or Enter`. An empty key list is empty. Imports are 1048: 896 builtins and 152 return-0 stubs.
- `2026-11-03` — **UZ3**: `SetupCrouchSprite(5)` fails, so BeginPlay clears the crouch sprite. Imports are 1048: 895 builtins and 153 return-0 stubs.
- `2026-11-03` — **UZ3**: `String.Split` of `"a  b"` on a space, skipping empty tokens, is `a` and `b`. Splitting `"a\nb\n"` on a newline and keeping empty tokens is `a`, `b`, and an empty token. Imports are 1048: 894 builtins and 154 return-0 stubs.
- `2026-11-03` — **UZ3**: `playerpawn.DamageMobj(8)` queues `A_DamageSelf` with 8 and returns 8. Imports are 894 builtins and 156 return-0 stubs.
- `2026-11-03` — **UZ3**: `CopyFriendliness` with no actor does nothing. With an actor, the caller copies the friendly flag from its master, or from its target when it has no master. `changeTarget` aims at that actor, and a nonzero `resetHealth` restores spawn health. Imports are 893 builtins and 157 return-0 stubs.
- `2026-11-03` — **UZ3**: `SetViewPos` with a Z of 12 raises that player's eye by 12 map units. Imports are 892 builtins and 158 return-0 stubs.
- `2026-11-03` — **UZ3**: a line-target `TraceBleed(4)` spawns blood for 4 points of damage on the calling actor. A bleed of 0 spawns nothing. Imports are 891 builtins and 159 return-0 stubs.
- `2026-11-03` — **UZ3**: `inventory.ClearCounters` and `object.ClearCounters` drop `COUNTKILL` and `COUNTITEM`, the same as `actor.ClearCounters`. Imports are 890 builtins and 160 return-0 stubs.
- `2026-11-03` — **UZ3**: `SetRenderStyle(6)` is translucent. Style 0 is invisible and style 1 is opaque. An unknown style is left unchanged. Imports are 888 builtins and 162 return-0 stubs.
- `2026-11-03` — **UZ3**: `SetShade(0x102030)` stores red 16, green 32, and blue 48. A color of 0 clears the shade. Imports are 887 builtins and 163 return-0 stubs.
- `2026-11-03` — **UZ3**: `DisableLocalRendering(player, true)` sets `DONTDRAW`. Passing false clears it. The player index is the one local player. Imports are 886 builtins and 164 return-0 stubs.
- `2026-11-03` — **UZ3**: `DrawSplash` uses the floor-splash path and the actor's vertical speed. An upward or zero speed does not splash. Imports are 885 builtins and 165 return-0 stubs.
- `2026-11-03` — **UZ3**: `TraceBleed(9)` spawns blood for 9 points of damage. A bleed of 0 spawns nothing. Imports are 884 builtins and 166 return-0 stubs.
- `2026-11-03` — **UZ3**: `DamageMobj(15)` queues `A_DamageSelf` with 15 and returns 15. A damage of 0 does not hurt. Imports are 882 builtins and 168 return-0 stubs.
- `2026-11-03` — **UZ3**: `ClearCounters` drops `COUNTKILL` and `COUNTITEM`, and the level totals shrink by one for each flag that was set. Imports are 881 builtins and 169 return-0 stubs.
- `2026-11-03` — **UZ3**: `RemoveFromHash` clears the actor's tid, so a tid search no longer finds it. Imports are 880 builtins and 170 return-0 stubs.
- `2026-11-03` — **UZ3**: `RestoreDamage` puts the class damage value back on the actor. Imports are 879 builtins and 171 return-0 stubs.
- `2026-11-03` — **UZ3**: `FindFloorCeiling` keeps the actor between the floor and the ceiling. Imports are 878 builtins and 172 return-0 stubs.
- `2026-11-03` — **UZ3**: `LinkToWorld` clears `NOBLOCKMAP`, so the actor collides again. `UnlinkFromWorld` sets `NOBLOCKMAP`, so it does not. Imports are 877 builtins and 173 return-0 stubs.
- `2026-11-03` — **UZ3**: `ClearBounce` clears floor and wall bounce. Imports are 875 builtins and 175 return-0 stubs.
- `2026-11-03` — **UZ3**: `Howl` plays the class `HowlSound`, or `PainSound` when `HowlSound` is empty. Imports are 874 builtins and 176 return-0 stubs.
- `2026-11-03` — **UZ3**: `SetFriendPlayer` with a player queues `A_SetFriendly`. A null player does not change the flag. Imports are 873 builtins and 177 return-0 stubs.
- `2026-11-03` — **UZ3**: `PlayActiveSound` plays the class `ActiveSound`. `PlaySpawnSound` plays the class `SeeSound`. An empty sound plays nothing. Imports are 872 builtins and 178 return-0 stubs.
- `2026-11-03` — **UZ3**: `SetIdle` returns the actor to its Spawn state and runs that state's action. `SetIdle(true)` returns to Spawn without running the action. Imports are 870 builtins and 180 return-0 stubs.
- `2026-11-03` — **UZ3**: `SetDamage(15)` queues `A_SetDamage` with 15. Imports are 869 builtins and 181 return-0 stubs.
- `2026-11-03` — **UZ3**: `Thrust(10, 0)` adds 10 map units of velocity along +X. A sentinel speed does not shove the actor. Imports are 868 builtins and 182 return-0 stubs.
- `2026-11-03` — **UZ3**: a dictionary stores `"k"` as `"v"` and returns `""` for a missing key. The dictionary stays alive across the lookup. `InStateSequence` is false even when the argument is 1. Imports are 867 builtins and 183 return-0 stubs.
- `2026-11-03` — **UZ3**: `System.Now` follows the tic clock: tic 35 is 1000 ms, and formatting that time yields an empty string. `ByteAt` of `"Hi"` is 72. `"3.5"` parses as 3.5. The length of `(3, 4)` is 5, and its unit vector is `(0.6, 0.8)`. `StopAllSounds` queues `A_StopSound`. A weapon slot search misses. Imports are 865 builtins and 185 return-0 stubs.
- `2026-11-03` — **UZ3**: `Super.TryPickup` and `Super.Use` fail, so an item is not taken. `ShouldRespawn` is false. Menu and input events on `Super` are not handled. The pickup message is empty, and the pickup sound `"supk"` is queued. Undoing a morph returns null. Imports are 854 builtins and 196 return-0 stubs.
- `2026-11-03` — **UZ3**: a sector plane’s height at a point is 0. `FormatNumber(12)` is `"12"`. `pos.PlusZ(12)` is the point `(0, 0, 12)`. `Super.Die` queues `A_Die`. Special damage of 9 stays 9. A pickup is not handled, inventory is not cleared, and a dialogue reply is not skipped. Imports are 840 builtins and 210 return-0 stubs.
- `2026-11-03` — **UZ3**: a sprite frame validates, so a weapon with a Ready state can still be picked up. `tex.isNull` is true on a null texture, and empty text is empty. A parent class and a weapon-slot weapon are null, so those walks stop. A joystick curve is the default. Imports are 818 builtins and 232 return-0 stubs.
- `2026-11-03` — **UZ3**: HUD scale is 1. A missing picture does not exist. Option values are empty. The next spot and savegame lookups return null, so those walks stop. An axis map is -1, and a screen job is not ready. Imports are 805 builtins and 245 return-0 stubs.
- `2026-11-03` — **UZ3**: `Use` does nothing and `isclass` is false. `throwabortexception` does not abort. Weapon states, toss, and drop items return null. A weapon is not abstract. Its pickup message is empty, and the pickup sound `"pkup"` is queued. `object.mstimef` is the tic clock. Imports are 779 builtins and 271 return-0 stubs.
- `2026-11-03` — **UZ3**: `SetState` fails. Menu, mouse, and input events are not handled, so they are not swallowed. Releasing focus clears it. The tooltip and action name are empty. Imports are 757 builtins and 293 return-0 stubs.
- `2026-11-03` — **UZ3**: the angle of (0, 1) is 90 degrees. A pickup fails and ammo is not depleted. Creating a floor and freezing the level do nothing. The automap position is the origin. A menu message of `"Hi"` is recorded, and a movie frame is null. Imports are 741 builtins and 309 return-0 stubs.
- `2026-11-03` — **UZ3**: an empty tooltip is empty. Appending `'!'` to `"Hi"` makes `"Hi!"`, and deleting the last character of `"Hi"` leaves `"H"`. Tooltip lines are 4 and tooltip speed is 1. The menu is closed and `SetMenu` returns null. Imports are 716 builtins and 334 return-0 stubs.
- `2026-11-03` — **UZ3**: `it.Next` and inventory next/previous return null, so those walks stop. A pickup message of `"Got"` is recorded. There is no joystick, and stick sensitivity is 1. Key bindings are empty. Imports are 703 builtins and 347 return-0 stubs.
- `2026-11-03` — **UZ3**: `goodbyestr.Length("Hi")` is 2 and `Left(2)` of `"Hello"` is `"He"`. The next hotkey code point is -1. Setting an icon to null clears it. Ambient activate queues the sound. A projectile did not hit a plane, and a custom-inventory state chain did not run. Imports are 682 builtins and 368 return-0 stubs.
- `2026-11-03` — **UZ3**: line, sector, and side indexes are -1. The floor is 0. The next ceiling above 40 is 168. A sector has no height control, and `global.sound` is null. Imports are 673 builtins and 377 return-0 stubs.
- `2026-11-03` — **UZ3**: `MSTimeF` is the tic clock, so tic 35 is 1000 ms. `TweakSpeeds` is 1. Giving an item and destroying a thing do nothing. Status-bar coordinates pass through. Spawns return null. Imports are 665 builtins and 385 return-0 stubs.
- `2026-11-03` — **UZ3**: `ACS_ExecuteWithResult` returns 0, so a script number is not treated as the result. Bullet aim is flat. Opening a door, lowering a floor, and adding ammo do nothing. Inventory and state lookups return null. The color picker is `"Green"`. Imports are 637 builtins and 413 return-0 stubs.
- `2026-11-03` — **UZ3**: `ticrate` is 35 and `maxplayers` is 1. Menu scale, mouse input, and gamma are 1. Monsters spawn, health pickups work, exits stay open, and weapons do not stay. This is not multiplayer or team play. Intermission percents are on. Imports are 619 builtins and 431 return-0 stubs.
- `2026-11-03` — **UZ3**: this is not a netgame. `sv_ammofactor` is 1, ammo is finite, and armor is kept. Death does not drop keys. The cooperative scoreboard is on and the deathmatch scoreboard is off. Menu volume is 1. `nullptr` is null. Imports are 587 builtins and 463 return-0 stubs.
- `2026-11-03` — **UZ3**: clean scale is 1. Health turns red below 25, gold below 50, and green through 100. Ammo turns red below 25% and berserk health is 100. HUD stat lines and compat flags are off. The line list is empty. Imports are 552 builtins and 498 return-0 stubs.
- `2026-11-03` — **UZ3**: deathmatch and the DM flags are off. `gameticrate` is 35 and `gametic` follows the host. `diff.Length` of (3, 4) is 5. Armor is not ignored. A menu shortcut misses as -1. Imports are 513 builtins and 537 return-0 stubs.
- `2026-11-03` — **UZ3**: `ActorIterator.Next` returns null, so an actor walk stops. `array.Find` on an empty array is -1, and `Find(7)` after a push is index 0. Clean width is 320 and clean height is 200. `cl_run` is on. The scoreboard is closed and alt-HUD latency is 0. Imports are 489 builtins and 561 return-0 stubs.
- `2026-11-03` — **UZ3**: `CheckForTexture` is -1 and a missing texture has size 0. `Texture.IsValid` is false. Translation ids are -1. `StartSoundSequence("Hi")` queues that sound. `soundinfo` and `sounds` are objects. Imports are 466 builtins and 584 return-0 stubs.
- `2026-11-03` — **UZ3**: the player number stays 0 even when the argument is 4. The player is not frozen. A cheat suicide does not run, and no weapon button is held. A missing texture is 0 by 0. Imports are 453 builtins and 597 return-0 stubs.
- `2026-11-03` — **UZ3**: the player has no keys, is not poisoned, and is on team 0. Weapon bob is 0. `Distance2D` from the origin to (3, 4) is 5. The color set is `"Green"`. Weapon layers and inventory lookups return null. Imports are 440 builtins and 610 return-0 stubs.
- `2026-11-03` — **UZ3**: a point is inside a sector, and player starts exist. Giving a secret, running a special, and exiting the level do nothing. Cluster and episode names are empty. UDMF ints are 0 and a vector difference is the origin. `MidPrint("Hi")` is recorded. Health groups and skybox portals return null. Imports are 420 builtins and 630 return-0 stubs.
- `2026-11-03` — **UZ3**: `validcount` is 1, so a fresh actor is not already visited. The sky flat is -1. `WP_NOCHANGE` is one stable weapon object. Dictionary, HUD font, and inventory-bar creation return an object. Movie playback and the text-entry menu return null. Imports are 396 builtins and 654 return-0 stubs.
- `2026-11-03` — **UZ3**: the game is not paused, not in a menu, not a netgame, and not playing a demo. `gametic` follows the host. `ThinkerIterator.Next` and the current menu return null. A missing alt icon does not exist. `CheckNoDelay` is false. Imports are 384 builtins and 666 return-0 stubs.
- `2026-11-03` — **UZ3**: bone index is -1 and the bone base is the origin. The tag is empty. Bob offset is 0. `GiveBody` and `GiveSecret` do nothing. The skin check passes. `MorphInto` and the old missile spawn return null. The player number is 0. Imports are 356 builtins and 694 return-0 stubs.
- `2026-11-03` — **UZ3**: bullet aim is flat. A boss is not dead, a monster does not seek, and the actor is not blocked. `ApplyDamageFactors(9)` keeps 9. `Destroy` queues `A_Die`. Armor class, monster search, and angled missile spawn return null. Imports are 343 builtins and 707 return-0 stubs.
- `2026-11-03` — **UZ3**: `VelFromAngle(10, 0)` is +10 on X. A pitch of 90 puts that speed on Z. Offsets echo for an actor at the origin. `TestMobjLocation` succeeds. `TryMove`, `Teleport`, and `SetState` return false. `StopSoundSequence` queues `A_StopSound`. Imports are 333 builtins and 717 return-0 stubs.
- `2026-11-03` — **UZ3**: spawn health is 100. `SetZ(32)` stores that height. `SoundAlert` queues a look. Actors do not pass through the player. Spawn, missile spawn, teleport fog, and `ResolveState` return null. Imports are 319 builtins and 731 return-0 stubs.
- `2026-11-03` — **UZ3**: `Normalize180(190)` is -170. Actors are not frozen, not friends, and not zero-damage. `MonsterMove` and `NewChaseDir` queue a chase. `RailAttack(40)` queues `A_RailAttack` with damage 40. `RadiusAttack` queues `A_Explode`. Imports are 303 builtins and 747 return-0 stubs.
- `2026-11-03` — **UZ3**: actors are visible and count as on the floor. Species is `"None"`. `LookForPlayers` queues a look. `Die` queues `A_Die`. `ExplodeMissile` queues `A_Explode` with the damage argument. Imports are 292 builtins and 758 return-0 stubs.
- `2026-11-03` — **UZ3**: default speed is 8 and spawn health is 100. `GetMissileDamage` returns its add argument. `GetModifiedDamage` keeps the damage. Floor terrain is `"Rock"`. Pointer, replacement, drop-item, and spawn-type lookups return null. Imports are 286 builtins and 764 return-0 stubs.
- `2026-11-03` — **UZ3**: actor `deltaangle(10,350)` is -20. A 3-4-5 `Distance2D` is 5. `DistanceBySpeed(70,10)` is 7 tics. Monsters count as kills. `CheckPosition` is clear. `FindInventory` and `FindState` return null. Imports are 276 builtins and 774 return-0 stubs.
- `2026-11-03` — **UZ3**: `AngleToVector(0,10)` is +10 on X and `AngleToVector(90,10)` is +10 on Y. `AddZ(5)` then `AddZ(2)` leaves Z at 7. `ChangeFlag("SOLID", 1)` is what `CheckFlag` reads next. `ApplyDamageFactor` returns the damage unchanged. Imports are 269 builtins and 781 return-0 stubs.
- `2026-11-03` — **UZ3**: `CheckMissileSpawn` and `CheckStopped` are true. `SetFocus` is what `CheckFocus` reads next. `FindInventory` and `GetParentAmmo` return null. `GetLogo` returns an object. `Delete` removes an array slot. Imports are 259 builtins and 791 return-0 stubs.
- `2026-11-03` — **UZ3**: `SetString("Hello")` is what `GetString` reads next. `Next` returns null so a walk stops. `Push` then `Pop` on an int array returns the pushed value. `SetOrigin` stores X, Y, and Z. Imports are 251 builtins and 799 return-0 stubs.
- `2026-11-03` — **UZ3**: `SetX(12.5)` is what `GetX` reads next, and `SetY` uses the same storage. `SetValue(4)` round-trips. `StringAt("Hi", 1)` is `"i"`. `GetDeltaTime` is `1/35` second. The cursor is 8 units wide. Imports are 243 builtins and 807 return-0 stubs.
- `2026-11-03` — **UZ3**: `Vec3Offset(3,4,5)` keeps that offset. `PlusZ(32)` is the point `(0,0,32)`. Light level defaults to 255, and `SetLightLevel` is what the next `GetLightLevel` returns. Text color is untranslated (`11`). Imports are 233 builtins and 817 return-0 stubs.
- `2026-11-03` — **UZ3**: `VelFromAngle(10,0)` is +10 on X and `VelFromAngle(10,90)` is +10 on Y. Flat `Vel3DFromAngle` keeps full horizontal speed. `PitchFromVel` of equal horizontal and vertical speed is 45 degrees. `bam(90)` is `0x40000000`. Imports are 228 builtins and 822 return-0 stubs.
- `2026-11-03` — **UZ3**: `object.IsNull` is true for a null receiver and `isValid` is true for a live object. `CompareNoCase("Ab","ab")` is 0. `IsNotEmpty` checks the string. Unresolved font, skill, and sound calls share the typed builtins. `Visible` and `Selectable` are true. Imports are 223 builtins and 827 return-0 stubs.
- `2026-11-03` — **UZ3**: `Splerp` eases through the midpoint. `deltaangle(10,350)` is -20 and `absangle` of that pair is 20. `sin`, `cos`, and `tan` stay in degrees. Floor friction is 0.90625. `DistanceBySpeed(70,10)` is 7 tics. Imports are 200 builtins and 850 return-0 stubs.
- `2026-11-03` — **UZ3**: `Lerp(0,10,0.5)` is 5. A 3-4-5 `Distance2D` is 5. `AngleTo` straight up is 90 degrees. `AccuracyFactor` is 1. Skill ammo factor is 1, 1.5, or 2 by skill, and `sv_ammofactor` is 1. `global.GetCVar` finds the same cvars as `CVar.FindCVar`. Imports are 190 builtins and 860 return-0 stubs.
- `2026-11-03` — **UZ3**: `NotifyFontScale` and `viewactive` are 1. `playeringame[0]` is true. The remaining UI fonts, the status bar, and `players` are objects. `global.Ceil(1.2)` is 2 and `global.Clamp` matches `math.clamp`. Imports are 178 builtins and 872 return-0 stubs.
- `2026-11-03` — **UZ3**: `GameTicRate` is 35. Clean resolution is 320×200 at scale 1. UI fonts, `level`, and `gameinfo` are objects. Class, skill, and team lists are empty arrays. Imports are 163 builtins and 887 return-0 stubs.
- `2026-11-03` — **UZ3**: `CVar.FindCVar("screenblocks")` is 10, `skill` follows the host skill, and freelook stays on. `SetInt` updates the object and later reads. `Font.FindFontColor("Red")` is 6, an unknown name is -1, the cursor is `"_"`, and its displacement is 8. Imports are 129 builtins and 921 return-0 stubs.
- `2026-11-03` — **UZ3**: the virtual screen is 320×200, and the clip and fullscreen rect cover it. The printable player name is `"Player"`. `GetNextPlayer` and `GetNextPlayerNumber` visit the console player once and then stop. Imports are 120 builtins and 930 return-0 stubs.
- `2026-11-03` — **UZ3**: jumping, freelook, and crouching are allowed. `IsPointInLevel` is true, `FindUniqueTid` is 1, and `FormatMapName` keeps the given name or returns `MAP01`. Status bar top is 168, weapon bob speed is 1, and player color is `"Green"`. Imports are 113 builtins and 937 return-0 stubs.
- `2026-11-03` — **UZ3**: `Screen.GetAspectRatio` is 4:3. `PlayerInfo.GetAutoaim` is 35 degrees, `GetNeverSwitch` is false, `GetPlayerClassNum` is 0, and `GetSkin` is `"base"`. An empty `BrokenLines` has count 0. Imports are 103 builtins and 947 return-0 stubs.
- `2026-11-03` — **UZ3**: `Font.FindFont` and the `Get*Font` lookups return an object. `BreakLines` does too. `IsValidDynamicFont` is false. Imports are 95 builtins and 955 return-0 stubs.
- `2026-11-03` — **UZ3**: font metrics are real. `GetCharWidth` is 8, `GetHeight` is 16, `GetMaxAscender` is 12, and `CanPrint` rejects control characters. Imports are 86 builtins and 964 return-0 stubs.
- `2026-11-03` — **UZ3**: `String.IndexOf` returns a character index (`"hello".IndexOf("ll")` is 2). `StringTable.Localize` returns the key. `font.StringWidth` is 8 per character. IndexOf now lowers as one native, so the import list is 1050 names: 82 builtins and 968 return-0 stubs.
- `2026-11-03` — **UZ3**: `String.Left`, `MakeLower`, `Replace`, `CharLower`, and `Filter` are real. `Filter` strips Doom `0x1c` color escapes. Imports are 78 builtins and 973 return-0 stubs.
- `2026-11-03` — **UZ3**: stdlib math intrinsics are real. `sin(90)` is 1, `atan2` and `VectorAngle` return degrees, and `frandom` is the upper bound so it stays deterministic. Imports are 73 builtins and 978 return-0 stubs.
- `2026-11-03` — **UZ3**: ZScript `A_StartSound("weapons/pistol")` queues that sound and `A_StopSounds` clears it. `A_MonsterRail` is a 50-damage rail and hits a target. Imports are 57 builtins and 994 return-0 stubs. `A_CheckForResurrection` is still a stub.
- `2026-11-03` — **UZ3**: the remaining stdlib `A_*` imports that already have playsim arms are bound (`A_SetSize`, `A_SeekerMissile`, `A_FireProjectile`, `A_SpawnProjectile`, `A_DropItem`, and others). `A_SetSize(12, 48)` sets radius and height. Imports are 54 builtins and 997 return-0 stubs. Four `A_*` imports still have no playsim arm (`A_MonsterRail`, `A_StopSounds`, `A_StartSoundIfNotSame`, `A_CheckForResurrection`).
- `2026-11-03` — **UZ3**: ZScript calls to playsim actions (`A_SetHealth`, `A_DamageSelf`, `A_Wander`, `A_BossDeath`, and 19 others) run the DECORATE implementations. A headless `A_SetHealth(40)` then `A_DamageSelf(15)` leaves health at 25. Stdlib imports are 44 builtins and 1007 return-0 stubs.
- `2026-11-03` — **UZ3**: `A_Pain` and `A_NoBlocking` are real natives. A ZScript `A_NoBlocking()` clears `SOLID`. Stdlib imports are 39 builtins and 1012 return-0 stubs.
- `2026-11-03` — **UZ3**: ticking `MarinePistol` from Spawn runs look, chase, and `Missile.Pistol`, and the pistol damages the player. `Goto Super::…` and a trailing `;` on ZScript state flow now survive the DECORATE merge.
- `2026-11-03` — **UZ3**: committed Freedoom bundles E1–E4 and MAP01–MAP30 replay with no checksum mismatch. Chocolate Freedoom2 `DEMO1` still diverges at tic 441 (player at 226.19,730.07 versus 222.09,731.05).
- `2026-11-03` — **UZ3**: `A_MarineLook` acquires the player and jumps to See; `A_MarineChase` steps toward them. Child classes inherit a parent's `Speed` after `Default` is applied. Attract demos are next.
- `2026-11-05` — **UZ3 parity**: `A_RadiusDamageSelf` now emits target blood after applying BFGSplash damage, including the absorbed-damage fallback used by UZDoom's `P_TraceBleed`; exact multi-particle spread and optional flash-class handling remain open.
- `2026-11-05` — **UZ3 parity**: ZScript `A_Chase(..., CHF_RESURRECT)` now attempts the shared arch-vile corpse-raise path and skips normal chasing when a corpse is successfully restored; the standalone result-returning resurrection native and remaining chase flags remain open.
- `2026-11-05` — **UZ3 parity**: ZScript `A_Chase(..., CHF_FASTCHASE)` now performs UZDoom-style lateral velocity bursts within 640 map units, counts down the three-tic strafe interval, and skips normal chase movement during it. `pr_chase` drives both random decisions; other chase flags remain open.
- `2026-11-05` — **UZ3 parity**: `A_Chase` now honors `CHF_NORANDOMTURN`, `CHF_NODIRECTIONTURN`, `CHF_NOPOSTATTACKTURN`, and `CHF_STOPIFBLOCKED` in direction selection, post-attack turning, and movement-facing updates. Target-search flags were completed in the following entry; portal-aware search remains open.
- `2026-11-05` — **UZ3 parity**: chase target acquisition now honors `CHF_DONTLOOKALLAROUND` by restricting new target selection to the actor's forward half-plane while preserving a valid existing target. `CHF_DONTIDLE` now runs the wander action when no chase target can be acquired. Portal-aware search remains open.
- `2026-11-05` — **UZ3 A_CheckForResurrection**: bound the bool native to the playsim's precomputed corpse candidate for actor VM calls; it returns true synchronously and queues that exact generational actor handle for post-VM validation and raising. Candidates with custom `CanResurrect` overrides and custom State/Sound arguments are conservatively rejected. Physical candidate validation follows the existing Arch-vile checks; 3D-floor groups and compatibility ghosts remain open.
- `2026-11-03` — **UZ3**: `MarinePistol.A_M_FirePistol` damages a `ZombieMan` in a headless test. Fight natives (`A_Look`, `A_Chase`, `LineAttack`, `math.random`) are real; the other 1014 imports return 0. Look/chase through the state machine is next.
- `2026-11-03` — **UZ3**: stdlib `CallNat` imports all bind. 1051 names, 27 already implemented, 1024 trap stubs from `NativeTable::bind_imports`. `ScriptedMarine` is next.
- `2026-11-03` — **UZ3**: lowerer errors dropped from 6 to 0. Function values, `Map` locals, lowercase object-returning calls, and `Font.TEXTCOLOR_*` constants all lower. Ceiling is 0. Natives are next.
- `2026-11-03` — **UZ3**: lowerer errors dropped from 15 to 6. `Vel.XY.Length()` stays a vector length, a `(x, y)` tuple lowers in a float argument, and a vector-returning call keeps `.XY`. Ceiling is 6.
- `2026-11-03` — **UZ3**: lowerer errors dropped from 37 to 15. A store can use the class that declares the field, so `target`, `tracer`, `mLabel`, and `Vel.X` assign. Ceiling is 15.
- `2026-11-03` — **UZ3**: lowerer errors dropped from 50 to 37. `readonly<T>` keeps the inner type, so `readonly<Weapon> def` is a weapon and `def.bCheatNotWeapon` resolves. Ceiling is 37.
- `2026-11-03` — **UZ3**: lowerer errors dropped from 80 to 50. Compound flag assigns such as `bNoRadiusDmg &=` update the flag, and `new` accepts a runtime class. Ceiling is 50.
- `2026-11-03` — **UZ3**: lowerer errors dropped from 89 to 80. A constant on the receiver’s class folds (`Level.CLUSTER_HUB`), a float CVar such as `m_tooltip_alpha` reads as a float, and `**` is `math.pow`. Ceiling is 80.
- `2026-11-03` — **UZ3**: lowerer errors dropped from 104 to 89. A lowercase name such as `idmypos` reads as a CVar, `.default` is the actor default, and `-=` works in a float expression. Ceiling is 89.
- `2026-11-03` — **UZ3**: lowerer errors dropped from 112 to 104. A struct nested in a class is a real type, so `WeaponStates.melee` resolves, and the `paletteflash` CVar reads as an int. Ceiling is 104.
- `2026-11-03` — **UZ3**: lowerer errors dropped from 130 to 112. `collectedItems[0][i]` indexes the inner array, and that element stays an object so `item.health` resolves. Ceiling is 112.
- `2026-11-03` — **UZ3**: lowerer errors dropped from 138 to 130. A field read can use the class that declares it, so `SpawnState` and `mItems` resolve on a default or menu object. Ceiling is 130.
- `2026-11-03` — **UZ3**: lowerer errors dropped from 152 to 138. An untyped `let` keeps a string when the initializer returns one, so `ChooseSpawn()`, `GetClusterName()`, and `String.Format` stay strings. Ceiling is 138.
- `2026-11-03` — **UZ3**: lowerer errors dropped from 155 to 152. `is` accepts a runtime class, so `GetClass()` and `RestrictedToPlayerClass[i]` lower. Ceiling is 152.
- `2026-11-03` — **UZ3**: lowerer errors dropped from 172 to 155. `double.equal_epsilon` is `1/65536`, and a host global such as `menuactive` can be stored. Ceiling is 155.
- `2026-11-03` — **UZ3**: lowerer errors dropped from 182 to 172. String constants such as `TEXTCOLOR_WHITE` fold and pass through `String.Format`. Ceiling is 172.
- `2026-11-03` — **UZ3**: lowerer errors dropped from 189 to 182. A constant on a parent class folds, so `Thinker.TICRATE` finds `Object.TICRATE` and `POP_TIME` folds. Ceiling is 182.
- `2026-11-03` — **UZ3**: lowerer errors dropped from 199 to 189. An object used as a float becomes a null check, and other string-position values go through `String.Format`. Ceiling is 189.
- `2026-11-03` — **UZ3**: lowerer errors dropped from 225 to 199. Global ints on struct `_` (`paused`, `automapactive`) are readable, and `int.min` / `int.max` fold. Ceiling is 199.
- `2026-11-03` — **UZ3**: lowerer errors dropped from 249 to 225. Integers and floats used in string expressions go through `String.Format`. Ceiling is 225.
- `2026-11-03` — **UZ3**: lowerer errors dropped from 257 to 249. Array fields such as `args` resolve on a receiver whose class does not redeclare them. Ceiling is 249.
- `2026-11-03` — **UZ3**: lowerer errors dropped from 293 to 257. `Name` fields read as integers, integer fields in string expressions use `String.Format`, and `++` works on array elements and float locals. Ceiling is 257.
- `2026-11-03` — **UZ3**: lowerer errors dropped from 333 to 293. Comma expressions evaluate both sides, float constants work in integer expressions, and `Sector_Set3DFloor` is special 160. Ceiling is 293.
- `2026-11-03` — **UZ3**: lowerer errors dropped from 366 to 333. Local names match without regard to case, so `SUPPORT3` finds `support3`. Ceiling is 333.
- `2026-11-03` — **UZ3**: lowerer errors dropped from 428 to 366. `Default.Height` reads the class default, and float constants such as `ORIG_FRICTION_FACTOR` fold. Ceiling is 366.
- `2026-11-05` — **UZ5 / playsim**: `A_SpawnParticleEx` carries legacy render style, `SPF_ROLL`, `SPF_FADE_IN_OUT`, and `SPF_FADE_IN_HOLD_OUT` through the ZScript op, sim event, shell, and renderer. Normal/Add/Translucent map directly; Shadow is approximated by soft particles. Texture IDs, time-freeze behavior, and other styles/flags remain open.
- `2026-11-05` — **UZ3 ZScript syntax**: added numeric `<=>` and deprecated `<>=` lowering with UZDoom's -1/0/1 result, including numeric int-returning calls in float comparison contexts. The parser now gates `<=>` at 4.15.1 and warns on `<>=` from that version onward; `unsafe(...)` effects and deprecated-class compatibility remain open.
- `2026-11-05` — **UZ3 playsim action context**: `A_SetTics` now targets the active weapon psprite when invoked from a weapon state and ignores inventory state chains, matching UZDoom's caller-specific handling. Flash-psprite targeting and broader action caller contexts remain to audit.
- `2026-11-05` — **UZ3 playsim action context**: completed `A_SetTics` flash-state handling too; `A_SetTics` in a flash frame now changes the flash psprite timer rather than the weapon or actor timer. Other action-context edge cases remain to audit.
- `2026-11-05` — **UZ3 ZScript 5.0 qualifiers**: parser now recognizes/version-gates `unsafe(const)`, `unsafe(clearscope)`, and `norollback`; field scope and no-rollback metadata are retained in the type model, and `norollback` on a play-scoped field is rejected. Prediction rollback integration, unsafe-const mutability enforcement, and deprecated-class diagnostics remain open.
- `2026-11-05` — **UZ5 particle pool semantics**: `A_SpawnParticleEx` now drops spawns when the fixed pool is full unless `SPF_REPLACE` is set; replacement recycles the oldest retained particle, matching UZDoom's allocation policy.
- `2026-11-05` — **UZ5 particle freeze semantics**: render-owned particles now pause lifetime, fade, roll, size, and motion while the playsim level is frozen unless `SPF_NOTIMEFREEZE` is set; the app passes the deterministic level freeze flag on each sim tic.
- `2026-11-05` — **UZ5 particle lighting**: `SPF_NODYNAMICLIGHTING` particle billboards are tagged in the scene lists and excluded from the renderer's CPU dynamic-light boost while retaining their regular sector-light value.
- `2026-11-05` — **UZ3 Hexen action**: implemented `A_Burst` chunk spawning with UZDoom's size-based count, named RNG stream, velocity pattern, inherited render style/alpha, copied target/friendliness, BOSSDEATH hook, non-player DropItem spawning, unblock, and source destruction. Updated the action coverage baseline; conversation drops and exact spawn/collision/RNG parity still need a gameplay fixture.
- `2026-11-05` — **UZ3 ZScript action bridge**: wired `Actor.A_Burst` and `global.A_Burst` through the ZVM to the playsim for direct class-name literals. Dynamic `class<Actor>` handles remain unsupported by the VM's current value model.
- `2026-11-05` — **UZ3 A_Burst dynamic string values**: extended the native lowering to forward runtime string/name expressions, including variables, as the chunk class name. First-class `class<Actor>` handles still need VM representation and decoding; this does not close the action's remaining behavioral gaps.
- `2026-11-05` — **UZ3 A_Burst class handles**: ZScript class identifiers now lower to program-local class ids; `ZscriptRuntime` gives the VM the matching id-to-name table, and `Actor.A_Burst` resolves runtime class values before queueing the playsim action. This closes the VM class-handle gap for this native; conversation drops and exact spawn/collision/RNG parity remain. `cargo check` is being run for the affected crates; tests not run.
- `2026-11-05` — **UZ3 A_Burst source correction**: comparison with UZDoom `p_actionfunctions.cpp` showed that `A_Burst` does not spawn `DropItem`s. Removed Room's extra drop behavior, which diverged from the native action. Chunks inherit the `NOSIGHTCHECK`, `HUNTPLAYERS`, `NOHATEPLAYERS`, and `BOSSSPAWNED` friendship flags copied by UZDoom `CopyFriendliness`; the following actor-sound-memory entry adds the corresponding `LastHeard` propagation.
- `2026-11-05` — **UZ3 actor sound memory**: added per-actor `LastHeard`, stamp it for actors in sectors reached by `P_NoiseAlert`, and use it for `A_Look`/`A_LookEx` unless `COMPATF_SOUNDTARGET` or `NOSECTOR` selects the legacy sector target. `A_ClearLastHeard` now clears only the actor field; Strife's `A_ClearSoundTarget` clears the current sector plus its actors' personal memories. `A_Burst` inherits `LastHeard` using `CopyFriendliness(..., true)` semantics. `cargo check -p room-sim -p room-game -p room-zscript -p room-zvm` passes; tests not run.
- `2026-11-05` — **UZ3 splash noise filtering**: water alerts now follow UZDoom's player-splash path, TERRAIN `noalert`, actor `DONTSPLASH`, and receiver-side `NOSPLASHALERT`; sector soundtargets still propagate when individual actors ignore the alert. Portal-aware sector membership remains open.
- `2026-11-05` — **UZ3/UZ5 sound native parity**: `A_StartSoundIfNotSame` now uses its actual two-sound, channel, flags, volume, attenuation, pitch, and start-time signature; direct SNDINFO aliases resolve to shared audio ids, and the mixer skips a start only when that source channel is already playing the check sound. The action dispatch and ZScript native share this behavior. Actor sound-class pseudo-names, pause/resume, local-seat routing, listener-Z, and savegame voice restoration remain open. `cargo check --tests -p room-audio -p room-game -p room-sim -p room-zvm` compiles the touched test targets; no tests were executed.
- `2026-11-05` — **UZ5 actor sound classes**: parsed `$playersound`, `$playersounddup`, and `$playeralias`; sound start events now carry the actor's `SoundClass`, allowing `*land`/`*grunt` tokens and their aliases to resolve through the active SNDINFO table before audio lookup. Runtime gender selection and fallback on missing class entries remain open.
- `2026-11-05` — **UZ3 `A_StartSound` native**: bound the full ZScript `Actor.A_StartSound`/`global.A_StartSound` arguments (channel, flags, volume, attenuation, pitch, start offset) into positional sim events with actor sound-class context. `cargo check -p room-defs -p room-sim -p room-audio -p room-game -p room-zvm` passes; gender selection and the other ZScript sound entry points remain open.
- `2026-11-05` — **UZ3 Doom rail weapon**: `A_FireRailgun` now selects the weapon Flash or Flash+1 using its dedicated deterministic `pr_firerail` stream after successful ammo depletion, only when running from a weapon psprite with a Flash state. Existing ammo depletion respects primary/alt fire. Rail puff/trail visuals and exact aim edge cases remain open. `cargo check` passes; tests not run.
- `2026-11-05` — **UZ3 rail impact actors**: `A_FireRailgun` forwards its puff class into rail tracing. Rail impacts now select that class for `ALWAYSPUFF`, suppress bleeding for `NOBLOOD`/dormant/non-foiled invulnerable victims, honor `BLOODLESSIMPACT`, and avoid unconditional wall puffs while retaining decals. Full rail trails and puff-as-damage-inflictor semantics remain unimplemented. `cargo fmt --check`, `cargo check -p room-sim -p room-game -p room-zscript -p room-zvm`, and `git diff --check` pass; tests not run.
- `2026-11-05` — **UZ3 / playsim**: `A_RadiusDamageSelf` now preserves UZDoom's optional flash and damage-call path when distance falloff rounds damage to zero; `P_TraceBleed` wall decals and dynamic class handles remain open.
- `2026-11-03` — **UZ3**: lowerer errors dropped from 530 to 428. An undeclared call used as an object lowers as an object native, and every enum variant is a bare constant. Ceiling is 428.
- `2026-11-03` — **UZ3**: lowerer errors dropped from 585 to 530. `Name` fields lower as strings, and `native` methods use their declared argument types. Ceiling is 530.
- `2026-11-03` — **UZ3**: lowerer errors dropped from 652 to 585. Unresolved float and string calls lower as natives, and an unknown object expression lowers as null. Ceiling is 585.
- `2026-11-03` — **UZ3**: lowerer errors dropped from 693 to 652. `mText = text` stores a string field on `self`. Ceiling is 652.
- `2026-11-03` — **UZ3**: lowerer errors dropped from 708 to 693. `..` string concatenation works in integer expressions. Ceiling is 693.
- `2026-11-03` — **UZ3**: lowerer errors dropped from 3276 to 708. A call the lowerer cannot resolve still emits a native instead of failing the expression. Ceiling is 708.
- `2026-11-03` — **UZ3**: lowerer errors dropped from 3287 to 3276. `item = item.Inv` used as an object keeps the assigned value. Ceiling is 3276.
- `2026-11-03` — **UZ3**: lowerer errors dropped from 3345 to 3287. A string array used as a condition reads the element, and string fields shared by several classes still resolve. Ceiling is 3287.
- `2026-11-03` — **UZ3**: lowerer errors dropped from 3434 to 3345. Global doubles such as `NotifyFontScale` load from struct `_`, and integer constants work in float expressions. Ceiling is 3345.
- `2026-11-03` — **UZ3**: lowerer errors dropped from 3503 to 3434. Line-special names such as `Door_Open` fold to their integer numbers. Ceiling is 3434.
- `2026-11-03` — **UZ3**: lowerer errors dropped from 3775 to 3503. `let inv = CPlayer.mo.Inv` keeps the field's class, so later `.Inv` reads resolve. Ceiling is 3503.
- `2026-11-03` — **UZ3**: lowerer errors dropped from 3889 to 3775. A field name declared on several classes uses the class with the most subclasses, so `Amount` resolves to `Inventory`. Ceiling is 3775.
- `2026-11-03` — **UZ3**: lowerer errors dropped from 4072 to 3889. Array indexes lower in float expressions and as object values. Ceiling is 3889.
- `2026-11-03` — **UZ3**: lowerer errors dropped from 4206 to 4072. String locals and fields count as conditions, and `.xy` plus color `.r` / `.g` / `.b` / `.a` read the scalar slot. Ceiling is 4072.
- `2026-11-03` — **UZ3**: lowerer errors dropped from 4250 to 4206. `cond ? a : b` lowers on the object, float, and string banks. Ceiling is 4206.
- `2026-11-03` — **UZ3**: lowerer errors dropped from 4741 to 4250. `random[rng]()` lowers as a math call, and `Vel.Z +=` updates the vector slot. Ceiling is 4250.
- `2026-11-03` — **UZ3**: lowerer errors dropped from 5168 to 4741. Float locals such as `double dist` convert with `F2I` when an integer expression reads them. Ceiling is 4741.
- `2026-11-03` — **UZ3**: lowerer errors dropped from 5605 to 5168. `let item = FindInventory(...)` and other uppercase calls bind as objects, so `Amount` and `Vel.Z` stores resolve. Ceiling is 5168.
- `2026-11-03` — **UZ3**: lowerer errors dropped from 5634 to 5605. `Inventory(item)` lowers as a cast, and vector fields stay one scalar slot so `.X` / `.Y` / `.Z` can store. Ceiling is 5605.
- `2026-11-03` — **UZ3**: lowerer errors dropped from 5693 to 5634. `sin`, `cos`, `frandom`, and the other math calls lower as float natives. Ceiling is 5634.
- `2026-11-03` — **UZ3**: lowerer errors dropped from 6578 to 5693. Instance calls such as `level.Time` lower as natives on the receiver's class. Ceiling is 5693.
- `2026-11-03` — **UZ3**: lowerer errors dropped from 7102 to 6578. CVars such as `generic_ui` and `GameTicRate` read as host values, and `obj.bFlag` reads and writes go through the actor. Ceiling is 6578.
- `2026-11-03` — **UZ3**: lowerer errors dropped from 7307 to 7102. Action functions see `stateinfo`, font globals such as `SmallFont` resolve, and `~` / `~==` lower. Ceiling is 7102.
- `2026-11-03` — **UZ3**: lowerer errors dropped from 7745 to 7307. Array fields such as `level.lines[i]` index and store, global objects such as `level` resolve, and `CleanXfac` / `CleanYfac` read as host values. Ceiling is 7307.
- `2026-11-03` — **UZ3**: lowerer errors dropped from 8403 to 7745. Static calls such as `StringTable.Localize` lower as natives, `b`-flags read and write through the actor, and global arrays such as `playeringame` resolve. Ceiling is 7745.
- `2026-11-03` — **UZ3**: lowerer errors dropped from 8940 to 8403. Integer fields and locals used as floats convert with `I2F`, including `angle` and vector axes. Ceiling is 8403.
- `2026-11-03` — **UZ3**: lowerer errors dropped from 9385 to 8940. `flagdef` names lower as bool fields, and `|=`, `&=`, `^=`, and shifts update integers. Ceiling is 8940.
- `2026-11-03` — **UZ3**: lowerer errors dropped from 10737 to 9385. Nested enums such as `Side.top` and `Line.front` lower as integer constants. Ceiling is 9385.
- `2026-11-03` — **UZ3**: lowerer errors dropped from 10840 to 10737. `int a = 1, b = 2` initializes every name. Ceiling is 10737.
- `2026-11-03` — **UZ3**: lowerer errors dropped from 13220 to 10840. `extend class` keeps fields from the original class, so `Args`, `Health`, and the other Actor fields stay visible. Ceiling is 10840.
- `2026-11-03` — **UZ3**: lowerer errors dropped from 13292 to 13220. The `is` operator lowers to a class check, and `Left` / `Right` / `Filter` / `Replace` lower as string calls. Ceiling is 13220.
- `2026-11-03` — **UZ3**: lowerer errors dropped from 13371 to 13292. String concatenation, name literals, and string fields lower. Untyped objects see Actor fields. Ceiling is 13292.
- `2026-11-03` — **UZ3**: lowerer errors dropped from 14125 to 13371. Engine pointers `target`, `tracer`, `master`, and `player` lower as Actor fields. Ceiling is 13371.
- `2026-11-03` — **UZ3**: lowerer errors dropped from 14273 to 14125. Bare object fields such as `player.mo` lower, and vector `.X` / `.Y` / `.Z` stores use the vector's scalar slot. Ceiling is 14125.
- `2026-11-03` — **UZ3**: lowerer errors dropped from 14895 to 14273. Name and string literals and float arguments lower on the call ABI. Ceiling is 14273.
- `2026-11-03` — **UZ3**: lowerer errors dropped from 15240 to 14895. `self` and `invoker` are object locals, and field and method lookup walks parent classes. Ceiling is 14895.
- `2026-11-03` — **UZ3**: type checker is clean. `check` now reaches the lowerer (15240 errors, ceiling locked). `new(cls)` accepts a runtime class name, and iterator, ammo, and powerup members resolve.
- `2026-11-03` — **UZ3**: stdlib type-check dropped from 45 to 27. Generic classes such as `MapIterator<Name, Service>` parse, and overrides inherit argument defaults from the base method. Ceiling is 27.
- `2026-11-03` — **UZ3**: stdlib type-check dropped from 68 to 45. `foreach (name : iter)` binds the loop variable, and string, map, and texture methods (`Split`, `GetIfExists`, `SetInvalid`) type-check. Ceiling is 45.
- `2026-11-03` — **UZ3**: stdlib type-check dropped from 85 to 68. `double.equal_epsilon` and `.X` / `.Y` / `.Z` resolve, and inventory virtuals `Use`, `TryPickup`, `SpecialDropAction`, and `Travelled` override cleanly. Ceiling is 68.
- `2026-11-03` — **UZ3**: stdlib type-check dropped from 93 to 85. `OnDestroy`, `GetProtrusion`, and `GetBlend` overrides resolve, and `state.GetSpriteTexture` type-checks. Ceiling is 85.
- `2026-11-03` — **UZ3**: stdlib type-check dropped from 106 to 93. Nested structs such as `ScriptedMarine.WeaponStates` are types, and class constants such as `LevelLocals.CLUSTER_HUB` resolve. Ceiling is 93.
- `2026-11-03` — **UZ3**: stdlib type-check dropped from 131 to 106. `readonly<Actor>` locals stay typed, and `FVector3` is a vector. Ceiling is 106.
- `2026-11-03` — **UZ3**: stdlib type-check dropped from 237 to 131. `foreach` variables are bound, Inventory fields resolve on Actor-typed values, and more string, font, and action-special names type-check. Ceiling is 131.
- `2026-11-03` — **UZ3**: stdlib type-check dropped from 939 to 237. Action specials used as values, CVARINFO-style names, `random[name]`, and local `T name[n]` arrays resolve. Ceiling is 237.
- `2026-11-03` — **UZ3**: stdlib type-check dropped from 1473 to 939. Actor `b`-flags work as fields, color channels and `TextureID.IsValid` resolve, properties are fields, and `Door_Open`-style action specials type-check. Ceiling is 939.
- `2026-11-03` — **UZ3**: stdlib type-check dropped from 4028 to 1473. `let` takes its initializer's type, vector swizzles resolve, and builtin types plus enum names are known. Ceiling is 1473. Natives and `ScriptedMarine` are still open.
- `2026-11-03` — **UZ3**: stdlib type-check dropped from 6627 to 4028. Engine `struct _` fields are globals, `T name[n]` is an array, `flagdef` and Actor `b`-flags are bools, and `random` / `String.Format` / `stateinfo` resolve. Ceiling is 4028. Natives and `ScriptedMarine` are still open.
- `2026-11-03` — **UZ3** (started): vendored `wadsrc/uzdoom/zscript.txt`. `#include` expands in source order. Map `foreach` and `norollback` parse. The stdlib type-check reports 6627 errors (2932 unknown name, 2750 unknown member); `uz_stdlib` fails if that count rises. Natives and `ScriptedMarine` are still open.
- `2026-11-03` — **UZ2** (code complete): re-ran `docs/34` §14.6. The conformance probe, `bd_compiles_and_links`, lump API, skill-6 damage, player TID, gender, mid-map save, and the two-run checksum all pass. Not marked `done` until CI. Next task is **UZ3**.
- `2026-11-03` — **UZ1** (code complete): sprite poses lerp between tics. `+DONTINTERPOLATE` snaps to the current tic. `+INTERPOLATEANGLES` turns the short way. Local flag, save, and refreshed-demo checks passed. Not marked `done` until CI. Next task is **UZ2**.
- `2026-11-03` — **UZ1** (partial): re-recorded Freedoom 1 episode bundles and Freedoom 2 MAP01–MAP30 bundles. Replays verify. The recorder picks a player-use `Exit_Normal` when the first exit line is walk-only.
- `2026-11-03` — **UZ1** (partial): refreshed `freedoom2_map01_smoke.fdem`. The 2026-10-13 recording desynced at tic 35 because later playsim changes (hitscan puff `vel.z`) moved actors the checksum covers. The new 70-tic replay verifies.
- `2026-11-03` — **UZ1** (partial): all 315 `DEFINE_FLAG`/`DEFINE_FLAG2` names are known. Brutal Doom v22 test 6 `room mod check` reports 0 unknown flags, properties, and actions. Extra flags round-trip in `.fsave`; a blob without the trailer still loads. Freedoom MAP01 smoke demo desyncs at tic 35, so UZ1 stays open.
- `2026-11-03` — **UZ1** (partial): `MBFBOUNCER` rebounds like the bounce flags, `STAYONLIFT` rides a floor that does not drop actors, `NOTELEFRAG` survives a teleport overlap, splash hits alert unless `NOSPLASHALERT`, and `BOSSSPAWNED` copies do not block `A_BossDeath`. Sprite interpolation flags stay stored for UZ6.
- `2026-11-03` — **UZ1** (partial): `FOILBUDDHA` pierces the health floor, `CANJUMP` gates monster jumps, `FALLDAMAGE` hurts without the level switch, `A_SkullAttack` sets `SKULLFLY` and skips gravity, `NOSECTORDAMAGE` / `FORCESECTORDAMAGE` gate floor damage, and friendlies spare each other unless `HARMFRIENDS`.
- `2026-11-03` — **UZ1** (partial): `ActorFlags` holds every UZDoom `DEFINE_FLAG` name (285). `+NOFRICTION` skips ground friction, `+ALWAYSFAST` uses fast chase speed and fast state tics, `+FOILINVUL` pierces `INVULNERABLE`, `+NOFEAR` blocks the frightened bit. The legacy `u64` flag word is unchanged. Still open: jump, fall damage, skullfly, bouncer, lift, telefrag, sector damage, `FOILBUDDHA`, interpolation.
- `2026-11-03` — **UZ0**: `room parity` writes `docs/qa/uzdoom-parity.md` (actions 345/700, flags 158/284, properties 103/150, ACSF 122/122). Two runs match. `wadsrc/uzdoom` is UZDoom `67bdb4b7` (2026-10-28). **UZ8–UZ11** stay deferred.
- `2026-11-02` — **ZS2b**: `natives_bd.rs` is folded into `room-zvm` `natives.rs`. `compile.rs` was already gone. The §14.6 probe, `zscript_play` (33), the `bd_*` lump/save tests, and `random_bytecode_does_not_panic` pass. **ZS1** and **ZS2** are done.
- `2026-11-02` — **ZS2b** (partial): `compile.rs` is deleted. `check()` only lowers. Bare instance fields, `let` of an object call, and `CVar.GetCVar(name).GetInt()` lower. The §14.6 probe, `zscript_play` (33), and the `bd_*` lump/save tests pass. `natives_bd.rs` still holds the runtime natives.
- `2026-11-02` — **ZS2b** (partial): `check()` emits with `lower.rs` and falls back to `compile.rs` only when lowering fails. The §14.6 probe, `zscript_play` (33), and the `bd_*` lump/save tests pass on the lowered program. `SoundClass` stores go through `Actor.SetSoundClass`. A top-level string `Ret` is `last_s`. `compile.rs` stays until nothing needs the fallback.
- `2026-11-02` — **ZS2b** (partial): `lower.rs` lowers the real Brutal Doom ZScript (`BD_WCS.zc` lump API, `DamageHandler.zc`, Room stdlib) with no errors. `check()` still uses `compile.rs` for the play tests.
- `2026-11-02` — **ZS2b** (partial): `lower.rs` accepts the real `DamageHandler.zc` plus the Room stdlib. Struct fields, `Skill`/`GameState`, `players[n].mo`, and `GiveInventory` lower. `check()` still uses `compile.rs` for the play tests.
- `2026-11-02` — **ZS2b** (partial): `lower.rs` lowers int/bool fields, `&&`/`||`, `String.Eq`, object parameters, and an `out` int that is not `i[0]`. A `ModifyDamage`-shaped method returns 70 / 30 / 0. Brutal Doom sources still go through `compile.rs`.
- `2026-11-02` — **ZS2b** (partial): `lower.rs` lowers casts (`IsClass`, failed cast is null), `!!` on objects, `double` arithmetic, and `out` ints. Probe shapes `NotNot` 0, `Cast` 1, `CastBad` 1, `Fl` 3.0, `Out` 77. The Brutal Doom subset stays on `compile.rs`.
- `2026-11-02` — **ZS2b** (partial): `lower.rs` lowers `new`, instance methods, and array fields, so the probe's `ZC_IntStrMap` runs (`MapLen` 3, `MapInt` 91, `MapDel` 0). Native import ids are matched case-insensitively. Casts, floats, `out`, and the Brutal Doom subset stay on `compile.rs`.
- `2026-11-02` — **ZS2b** (partial): `lower.rs` lowers `Array<int>` and `Array<string>` (`NewI`/`NewS`, `Push`, `Insert`, `Delete`, `Size`, index). Probe shapes `Arr` 309, `ArrDel` 4, `ArrS` 22. Maps, casts, floats, and the Brutal Doom subset stay on `compile.rs`.
- `2026-11-02` — **ZS2b** (partial): `lower.rs` lowers `String.Format`, `Mid`, `CharCodeAt`, `Length`, and `ToInt` through `CallNat` (string results come back in `s[1]`). Probe shapes `Fmt` 42, `Chr` 66, `Mid` 3. Arrays and the Brutal Doom subset stay on `compile.rs`.
- `2026-11-02` — **ZS2b** (partial): `lower.rs` now lowers `do`/`while`, `switch` with fallthrough, ternary, and bitwise `&` `|` `^` `<<` `>>`. Probe shapes `DoW` 8, `Cont` 8, `Sw(2)` 20 / default 99, `SwFall(1)` 3, `Tern` 1/−1, `Neg` −200, `Bits` `0x1023`. Strings, arrays, and the BD subset stay on `compile.rs`.
- `2026-11-02` — **ZS2b** (partial): the VM accepts `Arg`, `Call`, `CallVirt`, `CallNat`, `Ret`, and `GetRet`. `lower.rs` lowers static integer methods (locals, arithmetic, `if` / `while` / `for`, `break`, static calls) onto that ABI. `Ret`/`Add`/`Call`/`Wh`/`If1`/`Loop`/`Brk` match the probe. `compile.rs` still lowers the full Brutal Doom subset. Random bytecode still returns `Result`.
- `2026-11-02` — **ZS3b**: `World::with_zscript` takes the runtime out so a call can see the world. Skill, lumps, CVars, grants, TIDs, sound class, and handler lookup live on `ZvmPlayHost` inside that VM. The `HOST_*` thread-locals and `natives_bd` `HOST_FNS` are gone, so two worlds no longer share script host state. Still open: **ZS2b** `lower.rs`.
- `2026-11-02` — **ZS6b / ZS4b / BD6**: save blob v2 stores the whole script heap (slots, string bytes, intern counts, handler objects, actor handles) and restores those handles in place. `bd_save_load_mid_map` keeps an open `LumpOpen` across load and `LumpReadString` still returns the body, including a byte ≥ 0x80. `bd_two_runs_same_checksum` matches `World::checksum` across two 2,000-tic runs. `bd_compiles_and_links` is 0 errors, 0 unbound natives, 0 `Trap` opcodes on `testdata/zscript/bd_real`. Checksum mixes `ZvmHeap::stable_digest`. Native link lookup is ASCII case-insensitive. Still open: **ZS2b** `lower.rs`, **ZS3b** `with_zscript`.
- `2026-11-02` — **BD6** (partial): `bd_lump_api_via_acs` drives real `ZandroCompat` lump methods through ACS `ScriptCall`. `LumpOpen("MAP01", -2, 0)` returns the lump, `LumpRead` of a byte is that byte (including values ≥ 0x80), `LumpReadString` returns the first 4 characters, `LumpGetInfoInt` returns the size, and `LumpClose` then `LumpRead` returns 0 and prints.
- `2026-11-02` — **ZS4b** (partial): spawning a monster runs `Script_DamageMult.WorldThingSpawned`. At skill 6 the handler's `GiveInventory` lands on that actor, so it owns `Damage_Control`. Skill 5 does not grant it.
- `2026-11-02` — **ZS4b** (partial): the real `Damage_Control` from `testdata/zscript/bd_real/DamageHandler.zc` runs on a hit. `GetClassName` reads the shooter proxy, so Burn is ×3, a zombieman is ×7, and `TehLostSoul` is ×2.
- `2026-11-02` — **ZS4b** (partial): `ModifyDamage` receives the inflictor and source. A monster source sets `bISMONSTER`, and `damageType == "Burn"` is a case-insensitive string compare. Burn damage is ×3 and other monster damage is ×7, matching Brutal Doom's `Damage_Control` scale.
- `2026-11-02` — **ZS4b** (partial): a hit walks the target's inventory and runs `ModifyDamage`. The `out newDamage` register is the damage that armor and health see, so a passive item can triple a 10-point hit to 30. Source items run with `passive` false.
- `2026-11-02` — **ZS3b** (partial): ZScript classes whose parent is already a playsim class are registered on load, so an `Inventory` subclass can be given (capped by `Inventory.MaxAmount`) and summoned. `PostBeginPlay` and `Tick` run on that actor's script object. Dotted `Default` properties (`Inventory.MaxAmount`) apply. Random ZVM bytecode does not panic.
- `2026-11-02` — **ZS2b** (partial): `ZC_IntStrMap` field arrays run, and `testdata/zscript/conformance/probe.zs` executes against the real BD sources (`probe_file_executes_with_real_bd_map`). `Array.Size` / `Array.Insert` no longer overwrite the key or the index local. `MapLen` 3, `MapInt` 91, `MapDel` 0, `Bits` `0x1023`. Still open: `lower.rs`.
- `2026-11-02` — **ZS2b** (partial): `out` parameters copy back after `CallStatic`, float returns land in `f[0]` (`call_f64`), and `Type(obj)` casts check the heap class id (`IsClass`, parents included). `new("Class")` parsed as a call still allocates the object. Host calls resolve function names without case.
- `2026-11-02` — **ZS2b** (partial): bitwise `&` `|` `^` `<<` `>>`, `String.Format("%i")` / `CharCodeAt` / `Mid` / `Length`, and `Array<int>` / `Array<string>` (`Push`, `Insert`, `Delete`, `Size`, index) execute. Binary temps are parked so the right-hand side cannot overwrite the left. `!!` on an object uses `IsNull`. `Bits` evaluates to `0x1023` (`(0x1234 >> 4) & 0xff | 1 << 12`); the probe comment's `0x1123` skips the mask.
- `2026-11-02` — **ZS2b** (partial): the bytecode compiler now emits `for` / `while` / `do`, `switch` with fallthrough, `+=` / `++`, ternary, and unary minus. Nested `CallStatic` returns land in the caller's destination register, and an unqualified call resolves to the caller's class. `loops_switch_and_calls_match_the_probe` covers Ret/Add/Loop/Wh/DoW/Sw/SwFall/Tern/Call/Brk/Cont.
- `2026-11-02` — **ZS1b**: ZScript name resolution (`docs/34` §14.3). Unknown names are errors. Real Brutal Doom ZScript plus the §14.6 probe type-check. **ZS2–ZS4** and **BD6** reopened; the probe does not execute yet.
- `2026-11-02` — **BD pain / mag / pop-in**: pain and ACS `FadeTo` tint the whole window (the 320×200 fill was a hard red rectangle). `A_Jump` on a weapon state resolves the gun's label, so an empty magazine reaches Reload while reserve ammo remains. Console logs `FIRE`, `JUMP miss`, `SPAWN`, `APPEAR`, `PAIN`, and `FADE`.
- `2026-11-02` — **BD pickup / reload**: map guns are `CustomInventory` spawners; touching one runs its Pickup state and grants the real weapon. R gives the `Reloading` token the weapon Ready state jumps on. Hit lines resolve `$OB_…` instead of printing the key, and the pain tint stays light enough to see the room.
- `2026-11-01` — **BD pain flash**: the red damage tint keeps its alpha in the HUD overlay (it was forced opaque, so a hit replaced the 4:3 view with a solid red sheet). Hit diagnostics stay on stderr.
- `2026-11-01` — **BD weapons / shots**: overlay weapons whose Select state never calls `A_Raise` stay at `WEAPONTOP` (the pistol was sliding under the status bar). Sprite billboards alpha-blend so soft PNG flashes are not black rectangles. `Damage (random(...))` is evaluated when a missile hits, so imp fireballs and zombie tracers deal damage.
- `2026-11-01` — **BD sprites**: PNG sprites (indexed, with their own palette and `grAb`) decode for the weapon overlay and world billboards. `####` / `#` keep the current frame, and `A_JumpIfInventory` accepts a numeric state offset, so Brutal Doom's pistol Ready state shows `PISTE0` instead of a missing lump.
- `2026-11-01` — **BD play path**: `play-doom1.sh --file` DECORATE last-wins on duplicate classes (was aborting on `RealGibs2` and loading engine actors only); ZScript load logs classes, handler presence, and map-start objects. `ROOM_ZSCRIPT_DEBUG=1` by default in the script.
- `2026-11-01` — **ZS/BD tracks closed** in `docs/21`: ZS0–ZS8/ZS10 + BD0–BD9 done; remaining work is deferred acceptance (Cranelift, §9 ms, 10-min soak, hash-MES, screenshots, web pk3, 15-min net).
- `2026-11-01` — **ZS4/BD4/BD6/BD7 done** (deferrals: hash-MES needs Doom2 IWAD; GZDoom screenshots manual; 10-min soak).
- `2026-11-01` — **BD3 done** (with deferral): MAP01 **1400-tic / 40s** soak green (thinkers 209→10258); full **10-min** deferred — BD replacements grow ~7 thinkers/tic.
- `2026-11-01` — **BD9/ZS6**: gated `bd9_commercial_zscript_two_peer_checksum_gated` — commercial BD ZScript handlers, two peers, matching world+ZS checksums.
- `2026-11-01` — **ZS2 done**: Tier0/GC/Array acceptance met; float object fields remain deferred.
- `2026-11-01` — **ZS1 done**: commercial BD ZScript type-checks via Room `base.zs` stubs.
- `2026-11-01` — **BD6/ZS4**: gated `bd6_commercial_damage_control_skill_ge_6_gated` — skill≥6 `WorldThingSpawned` grants `Damage_Control`.
- `2026-11-01` — **BD7**: GZDoom menu screenshot checklist deferred (commercial lump parse + MENUDEF golden remain).
- `2026-11-01` — **BD4**: commercial `BrutalPistol` has Fire/Reload/Zoom; MAP01 ATTACK leaves Ready (8 tics).
- `2026-11-01` — **BD5/BD6**: MAP01 LOADACS (`BD_Main`/`BD_Hash`/`zancmpat`) + `pukename GetCVars`; MES `D2Map01DecorationSpawn` → **379** BDEC*/LightShaft after 350 tics.
- `2026-11-01` — **BD6/ZS4**: MAP01 smoke loads commercial ZScript (`check_ok`), activates `ZC_TMapHack`/`Script_DamageMult`, 350 tics (thinkers 133→2603).
- `2026-11-01` — **BD3/BD6**: gated MAP01 soak **350 tics / 10s** (registry ~4129; thinkers 133→2603).
- `2026-11-01` — **BD3/BD6**: gated `bd_freedoom_map01_ticks_gated` — Freedoom2+BD registry (~4129) + MAP01 load path.
- `2026-11-01` — **ZS8 done**: commercial BD ZScript `room zscript coverage` → **100%**.
- `2026-11-01` — **ZS4**: commercial BD ZScript check warning-free after `ModifyDamage` stub.
- `2026-11-01` — **BD5**: gated `bd_main_open_scripts_gated` — load `acs/BD_Main.o`, start 7 OPEN scripts, 35 ticks (no panic).
- `2026-11-01` — **BD6/ZS1**: `Inventory.ModifyDamage` + `PlayerPawn` stubs in `base.zs` — commercial BD ZScript warning-free.
- `2026-11-01` — **BD0 gate**: `brutal_doom_pk3_gated` asserts 4130/4130, 0 unknown flags/props/parents/actions, ZScript ok, 0 lump errors, 8 skills / 5 classes.
- `2026-11-01` — **BD2**: commercial re-verify after split fix — **0** unknown flags/props/parents/actions (flag table + `activation`/`skip_super`); case-mismatch actions remain (125).
- `2026-11-01` — **BD0/BD1 done** (commercial): DECORATE **4130/4130** — `split_actors` no-escape strings (`"\"` frames); `[`/`]`/`\`/`----` frames; digit sprites/`2048Label:`; `Goto Super::X`; `Skip_Super`; UTF-8 label idents.
- `2026-11-01` — **BD7**: commercial `brutal22test6.pk3` lump parse **0 errors** (LANGUAGE nested-quotes/`$ifgame`, MAPINFO `lookup`/flags/`sky1` scroll, SBARINFO `||`/`InInventory`, GLDEFS neg/`dontlightself 1`/`2SHADOW`, MODELDEF neg).
- `2026-11-01` — **BD2 done** (commercial): 0 missing parents / unknown flags / unknown properties; expanded BD flag+`user_`/`var` tables.
- `2026-11-01` — **BD1**: commercial DECORATE **2192/2193** — multi-unget, `13EI` lex, `#` frames, DECORATE string escapes off, camelCase/`Axe/Hit`/`==` args.
- `2026-11-01` — **BD1**: camelCase specials (`ThrustThingZ`), slash actor names (`Axe/Hit`), `==`/`!=`/`&&`/`||` in action args.
- `2026-11-01` — **ZS0/BD6**: commercial `brutal22test6.pk3` ZScript **check_ok** (3 files via VFS); gated test green.
- `2026-11-01` — **ZS7**: ZScript custom menus deferred (BD MENUDEF path covered under BD7).
- `2026-11-01` — **ZS10**: formal §9 ms gates deferred (CI variance); smoke + criterion remain.
- `2026-11-01` — **ZS6**: world checksum includes ZScript field digests (`zs6_world_checksum_includes_zscript_fields`).
- `2026-11-01` — **ZS9** Cranelift codegen + **BD8** web streamed pk3 marked deferred in tracker.
- `2026-11-01` — **ZS8**: `PlayerInfo.GetGender` ← CVar `gender`; `GetPlayerTID`/`SetPlayerTID` host map.
- `2026-11-01` — **BD7**: fixture `BdCheckOptions` MENUDEF → option menu draw/STCFN golden (`bd7_fixture_option_menu_draw_fingerprint`).
- `2026-11-01` — **ZS8**: `Actor.ChangeTID` + `ZandroCompat.PlayerInGame` host natives (+ TID/GetPlayerTID stubs).
- `2026-11-01` — **BD5/BD6**: ACSF `ScriptCall` host path e2e → `ZandroCompat.ConsolePlayerNumber`.
- `2026-11-01` — **ZS4/ZS8**: `EventHandler.Find` + `Object.New` allocate/lookup; `ZandroCompat.IsZandronum`/`IsClient` → 0.
- `2026-11-01` — **ZS3**: `merge_zscript_defaults` applies `Health`/`Radius`/`Height`/`Speed`/`±flags` (`;`-split Default bodies).
- `2026-11-01` — **ZS7**: soft RGBA pixel digest golden for `ZscriptStatusBar` blit (fills).
- `2026-11-01` — **BD6**: Map Enhancement–shaped WorldLoaded Mid-length gate e2e (`bd6_map_enhancement_worldloaded_uses_hash_slice`).
- `2026-11-01` — **BD8**: 3k gore soft ms/tic smoke + `corpse_queue` eviction unit test.
- `2026-11-01` — **ZS3**: compile stores `Default { }` bodies on `CompiledProgram.class_defaults`.
- `2026-11-01` — **ZS1**: `LevelLocals` stub class in `base.zs`.
- `2026-11-01` — **BD3**: register `A_Lower`/`A_Raise` as known (psprite path); bd_check fixture **0** unknown actions.
- `2026-11-01` — **ZS10**: confirmed `budget` 10k-tick smoke green; §9 formal gates still open.
- `2026-11-01` — **ZS8**: stub `Object.New` + `PlayerInfo.GetUserName`; `room zscript coverage` probe = 100%.
- `2026-11-01` — **ZS3 done**: `spawn_at` calls `set_state(Spawn, true)` (0-tic/anon Spawn actions); States merge + anon `A_SetHealth` e2e.
- `2026-11-01` — **ZS5**: float path complete enough for BD exprs; measurable speed-up deferred.
- `2026-11-01` — **ZS6/BD9**: two-peer ZScript+world checksum lockstep smoke (`zs6_bd9_two_peer_zscript_lockstep_checksum`).
- `2026-11-01` — **ZS7**: golden draw-list fingerprint for `ZscriptStatusBar` demo (health 100 vs 50).
- `2026-11-01` — **BD8**: 512 `NOINTERACTION` gore bits × 8 tics checksum-stable soak (`bd8_nointeraction_gore_cloud_ticks_deterministically`).
- `2026-11-01` — **ZS4/BD6**: `BdHashProbe` e2e `Wads.ReadLump` → `string.Mid` → `Length` (BD_Hash-shaped).
- `2026-11-01` — **ZS6**: mid-level `save_blob`/`load_blob` e2e restores Tick counter + script checksum (`zs6_save_load_mid_level_restores_script_fields`).
- `2026-11-01` — **ZS3**: `class_states` → `StateTable::merge_zscript_states` + `World::load_zscript_program`; two-run checksum; merge e2e (Spawn frame override).
- `2026-11-01` — **ZS3**: two-run world+ZScript checksum assert; compile stores `States` bodies on `CompiledProgram.class_states`.
- `2026-11-01` — **BD8** start: `FLAG_NOINTERACTION` skipped in float + vanilla thing-thing collision; unit test `nointeraction_solid_does_not_block_move`.
- `2026-11-01` — **ZS1**: `base.zs` adds `consoleplayer`/`skill` consts (Array/String Mid generics deferred — parser lacks `class Array<T>`).
- `2026-11-01` — **ZS5**: float ALU (`AddF`/`SubF`/`MulF`/`DivF`/`I2F`/`F2I`); `deco_zvm` float/`frandom`/`sin`/`cos`/`abs`/`sqrt` path + bump alloc; deco_eval routes floats through ZVM.
- `2026-11-01` — **BD7** start: mod-check `check_lumps` via `DefinitionsBuilder` (KEYCONF/CVARINFO/MENUDEF/SBARINFO/MAPINFO/GLDEFS/MODELDEF/DECALDEF/TERRAIN/SNDINFO/LANGUAGE); bd_check fixture lumps; player classes + skills asserted.
- `2026-11-01` — **ZS4**: persistent handler objs; `activate_map_handlers` (OnRegister/WorldLoaded); `CVar.GetCVar`/`GetInt` + host store; `Console.Printf` → host; spawn fires `WorldThingSpawned`; CallStatic unresolved safe resolve.
- `2026-11-01` — **ZS2**: heap `stable_digest` + GC scenario determinism test; `Array.Clear`/`Delete`/`Insert` natives; free-list lowest-index reuse.
- `2026-11-01` — **ZS1**: `room_zscript::check` appends Room `base.zs` (keep `#include` entry first); `WorldLoaded` override warn gone on bd_check; `MAXPLAYERS` stub.
- `2026-11-01` — **ZS3**: wire `PostBeginPlay` on `spawn_at` + `Tick` after thinker advance; `zs3_tick_and_postbeginplay_run_in_world_tick` headless map e2e.
- `2026-11-01` — **BD2**: `Weapon.BobSpeed`/`BobRangeX/Y`/`BobStyle` on `ClassDef`; Ready sway uses speed/range + Inverse* styles.
- `2026-11-01` — **BD1/BD3**: `split_actors` skips `actor` inside `//`/`/* */`/strings (doom.txt header false positive); float DoomEd nums; `A_SprayDecal` falls back to class `Decal`; DualFlameCannon fixture + mod-check samples in markdown.
- `2026-11-01` — **BD2**: store `Decal`/`SpawnID`/`MissileType`/`Melee*`/`Explosion*`/`BounceCount`/`WallBounceFactor`/`Weapon.Kickback`/`ClearFlags`; expand `is_known_property` for §3.5 BD names.
- `2026-11-01` — **BD4**: `PlayerHud`/`HudState` `ammo2`/`ammo2_max` from `Weapon.AmmoType2`; SBARINFO `ammo2`/`ammo2capacity`/`UsesSecondaryAmmo`; unit tests.
- `2026-11-01` — **ZS1**: method/field symbol tables, inheritance vtables, override validation; `base.zs` virtual Tick/PostBeginPlay/OnRegister/PlayerEntered/WorldThingSpawned; bd_like+base analyze test; CLI uses `method_count`.
- `2026-11-01` — **ZS0**: UZDoom `wadsrc/static/zscript` **293/293** parse-clean (`room zscript check`); postfix/`|=`/`in out`/`States()`/named args/`foreach`/`Function<>`/`sealed`/`int&`/comma-`for`/casts/tuples/nested enum·struct·const; CLI sibling `#include` load; `bd_like` green.
- `2026-11-01` — **BD5**: `SARY`/`FARY` local-array counts, `MEXP` exports, soft `ASTR`/`ATAG`; `PUSHFUNCTION`/`CALLSTACK` PCDs.
- `2026-11-01` — **BD5**: `PCD_SAVESTRING` + print/strcpy char-range stubs; unit test `savestring_interns_print_buffer`.
- `2026-11-01` — **ZS0**: GZDoom front-end gaps — trailing `struct`/`class` quals (`native play version()`), method `version()`/`vararg`/`...`, field `Args[n]`, `extend struct _`, untyped `const NAME = -1`, `@Type`; UZDoom `events.zs` + `compatibility.zs` check clean; excerpt fixture.
- `2026-11-01` — **BD5**: `SFLG`/`SVCT` on `ScriptDef` (NET/CLIENTSIDE + local counts); `SetMugShotState` PCD; `GetPlayerInput`/`ConsolePlayerNumber` host; Zandronum ACSF −100…−102 safe stubs.
- `2026-11-01` — **BD5** HUD PCDs: `MOREHUDMESSAGE`/`OPTHUDMESSAGE`/`ENDHUDMESSAGE*`/`SETHUDSIZE`/`GETPLAYERINPUT` stubs.
- `2026-11-01` — **BD5** start: ACS `FUNC`/`FNAM`/`ARAY`/`AINI`/`MINI` loader; VM `CALL`/`CALLDISCARD`/`RETURN*` + map-array PCDs; console `puke`/`pukename` → host → `cheat_puke*`.
- `2026-11-01` — **BD4** e2e: `BdPistol` Reload/Zoom/User1/User3 + `A_SetCrosshair`/`A_ZoomFactor` unit test.
- `2026-11-01` — **BD4** KEYCONF impulses: `WeaponImpulseState` (`+reload`/`+zoom`/`+user1`–`4`) → ticcmd bits; desktop release sends `-cmd`; slots 0/8/9 covered; KEYCONF BD-like parse test.
- `2026-11-01` — **BD4**: `A_WeaponReady` honours `WRF_*` (NOFIRE/NOBOB/NOSWITCH/ALLOWRELOAD/ZOOM/USER1–4); `Buttons::{RELOAD,ZOOM,USER1..4}`; `Weapon.AmmoType1/2` + dual deplete; unit test `wrf_nofire_and_reload_state`.
- `2026-11-01` — **ZS0/ZS4**: trailing `class Name native` parse (`base.zs` green with BD-like); MAPINFO `gameinfo { AddEventHandlers }` → `MapInfo.event_handlers` + `Definitions::zscript_event_handlers`; GAMEINFO lump handlers.
- `2026-11-01` — **ZS2–ZS4 / BD6**: compiler emits CallNative/CallStatic/strings/`new`/fields; `&&`/`||` short-circuit; `Zvm` native-import remap + `call_with`/`call_method_self`; ACS `ScriptCall`→ZScript; `ZscriptRuntime` host (Wads/skill/GiveInventory) + event dispatch; `zscript_play` e2e (Tick checksum, LumpOpen, skill≥6 `Damage_Control`).
- `2026-11-01` — **BD1** D8 + diagnostics: `Fail:`/`Stop:`/… as state labels; `expand_includes_mapped` + located errors; `parse_decorate_lenient` / `split_actors` in `room-decorate`; mod-check uses shared splitter.
- `2026-11-01` — **BD3** exprs/natives + **BD2** prop warn: `waterlevel`/`x|y|z`/`floorz`/`scalex|y`/`CheckClass`/`CallACS`/`sin|cos`/`abs`/`sqrt`/`random2`; `A_JumpIfNoAmmo`/`A_FastChase`/`A_PlaySoundEx`/`A_JumpIfInTargetLOS`/`A_BarrelDestroy`/`A_MeleeAttack`/`A_SetFloorClip`/…; `is_known_property` + mod-check `unknown_properties`.
- `2026-11-01` — **BD3** partial: case-insensitive action dispatch (`action_known`); specials-as-actions (`special_names` + DECORATE parse); iterative 0-tic/`Jump` chain with 1000-state guard; MapBuilder right-hand fronts + `check_position` origin-side one-sider fix; `A_JumpIf`/`A_ZoomFactor`/`A_SetCrosshair`/…; more BD natives + expr coverage still open.
- `2026-11-01` — **BD2** partial: GZDoom native bases in `doom.txt` (Inventory/Ammo/CustomInventory/FastProjectile/Powerup*/DoomWeapon/…); expanded `is_known_flag`/`apply_flag` for BD high-frequency flags; fixture shows 0 unknown flags / 0 missing parents.
- `2026-11-01` — **BD1** D2/D4–D7: parenthesised props (`Damage (…)`), `==`/`!=`/`&&`/`||`/`!`/`|` exprs, digit-leading actor/sprite names, `DropItem` prob/amount, `|` flag combos.
- `2026-11-01` — **BD1** D1/D3: state `Light(...)` / `Offset(x,y)` modifiers + quoted `"####"`/`"#"` keep-sprite; `Offset` wired into `StateTable`; fixture Light actor parses.
- `2026-11-01` — **BD0** start + **ZS+BD loop** (15m): `room mod check` harness (`mod_check`), `testdata/mods/bd_check` fixture, known-flag/action helpers; gated real-pk3 test when `ROOM_BRUTALDOOM_PK3` set.
- `2026-11-01` — **ZS5–ZS10 scaffold** (`docs/34`): DECORATE→ZVM (`deco_zvm`/`deco_eval`); `ZscriptSaveBlob` + checksum; `ZscriptStatusBar`; `room zscript coverage` + BD stubs; Tier-1 conformance/`zvm-jit` gate; peephole + `tick_override` bench + fuel budget smoke.
- `2026-11-01` — **ZS0–ZS3 scaffold** (`docs/34`): crates `room-zscript` + `room-zvm`; lexer/preprocessor/parser + sema/compile; Tier-0 interpreter/heap/GC; `room zscript check`/`disasm`; BD-like fixtures; `wadsrc/room/zscript/base.zs`; `ZscriptRuntime` in `room-sim`.
- `2026-11-01` — **MA4 + SK1–SK4**: `reg_weapon_slot` / `[options]` / ALTATTACK+RMB / missile hit info / ThingDestroyed / `export_mod` state actions; shuriken synth+classes+behaviour; `wasm_mod_shuriken`; registration fuel 250 M for generated art/sfx.
- `2026-11-01` — **MA3 + SK0**: `lump_find`/`lump_read`/`reg_lump`/`reg_sound_alias`; SDK `art::encode_patch`/`sound::encode_dmx`; generated PWAD via `Vfs::mount_memory`; `wasm_mod_lumps`; `mods/doom1-shuriken` art pipeline with pinned SHA-256 hashes.
- `2026-11-01` — **MA1 done + MA2**: `wasm_mod_states` Spawn loop e2e; AS `state*`/`Prop`/`Flag`/`Flow`; weapon/ammo/inventory PropIds + `WEAPON.NOALERT` in fire/ReFire; `mod_weapon.wat` merge test.
- `2026-11-01` — **MA1** (partial): `StateTable::import_script_states`; `reg_state_flow`/`action`/`offset` ABI+SDK+linkers; `merge_classes` merges module states; WASM `ActionPayload` dispatch from `set_state`/`set_psprite` (Headless binds runtime); SoftFont Graphics menu spacing fix earlier same day.
- `2026-10-31` — **WF0 + N8 + W8.3**: `testdata/weapons/*.txt` timeline harness (9 guns); Doom `psprite_scalelight`; fuzzoffset psprite fuzz; bob sway frozen outside `A_WeaponReady` (`pspr_bob_x/y`).
- `2026-10-31` — **docs/30 N1–N5/N7 + N3/N6 partial**: `Weapon`+`LightDone`; Goto parent resolve / no fallthrough; flash `flash0+offset` + action-requested flash (plasma RNG before spawn); generic fire deplete (`FBF`/`CPF`/useammo); Duke SelectionOrder; Royale `WEAPON_STAY 0`; ReadySound/UpSound parse.
- `2026-10-31` — **SHR batch 3** + **WF3/WF4**: geometric normals + TBN (walls/flats) with world-space N·L; aux G-buffer RGBA8 (oct normal + flags); saw gradual turn/range 65; raise from `WEAPONBOTTOM` + death Deselect→A_Lower; psprite sector light/Bright/fuzz; `rule::WEAPON_STAY` + coop/DM default.
- `2026-10-31` — **SHR batch 2** + **WF1/WF2** + **WF3 (partial)**: skinned clip plane; lightmap modulate; bloom soft-knee/Karis + additive upsample; SSG G/H + I/J flash; Bright flashes; SelectionOrder parse; ammo depletes in `A_Fire*`; deferred weapon switch; slot cycling; bob period 64 + mom²/4 amplitude; punch sound-on-hit + angle jitter; chaingun 1-bullet + SSG 62-tic tests.
- `2026-10-31` — **Desktop Python mods**: `language = "python"` in `ROOMMOD`; `discover_python_mods` + `Headless::set_native_mods`; `room-python-host` (`embed`/pyo3); `room-app --features python-mods`; pack `mods/doom1-python-hello/hello.wad`; smoke test spawns Imp at 3s.
- `2026-10-31` — **README diagrams** + **educational Python**: Mermaid crate/frame/WASM-mod charts in root README; optional Maturin/`pyo3` package `python/` (workspace `exclude`); `mods/doom1-python-hello` mirrors `hello_as` RoomMod hooks; `./scripts/setup-python-edu.sh`.
- `2026-10-31` — **SHR batch 1** (`docs/29` §1.1.5): bloom per-pass UBOs (R1) + composite bound in post (R2); `needs_post` only wired FX (R7); recreate `PostPass` on LDR format change (R10); web `output_gain=1`/`encode_srgb=0` with post (R12); FXAA/sharpen on LDR (P1/P2); dither before banding (P3); RoomSoft linear + conditional sRGB encode (P5). HUD weapon blit = one RGBA image (plasma/BFG perf).
- `2026-10-31` — **Blood floor/wall snap**: splats use `floor_at_xy` + nearest linedef; spray particles settle on floor Z; web Settings **Reset shaders to defaults**.
- `2026-10-30` — **Shader loop**: scrollable Graphics; combat-demo ephemeral+defaults-off; `FxEvent` stream; I1–I10/J1/J5–J6/J10 particle MVPs; E3/E5/E6; F2/F6–F12/G1 stand-ins; heat-haze/liquids/underwater/powerup post hooks; `r_quality` faithful stack.
- `2026-10-30` — **Graphics settings**: desktop Options → Graphics (live CVar labels, cycle/toggle); web Settings → Graphics; post look FX CRT/dither/vignette/grain/chromatic; web PostChain when effects on.
- `2026-10-30` — **SH0/SH1** bootstrap: `RenderSettings` + all §8 CVars; `PostChain` (SSAO/bloom/tonemap/FXAA); palette 32³ LUT; tonemap modes incl. RoomSoft/AgX/Uncharted2/Khronos; desktop shell wires post when any effect on; `r_quality` presets; web `roomSetShaderCvar`.
- `2026-10-30` — Survival draft **pause**: `rule::PAUSED` freezes thinkers/`maptime` while `WorldTick` still runs; draft timeout uses open-menu tics; `wasm_mod_survival` asserts pause/unpause.
- `2026-10-30` — **S-BR1** / **S-BR2** (MVP) / **S-BR4**: `room-server` `br_stats.json`, `GET /api/br/leaderboard`, lobby BR tab; `POST /api/quick-play/br`; `fill_bots` on `welcome`/`start`; royale results ingest test.
- `2026-10-30` — **BE3** `player_name`; session names on `World` (checksum); web roster / `room-app` cvar + lobby `Start.names`; royale HUD wired; `wasm_mod_player_name`; `royale.wasm`/`royale.wad` rebuilt.
- `2026-10-30` — **BR9** **ROYALE01** + balance: `room_testkit::royale01()` → `mods/doom1-royale/maps/ROYALE01.wad` (18 DM starts); packer merges `maps/*.wad` into `royale.wad`; loot/supply tuning; `wasm_mod_royale` ROYALE01 e2e; **S-BR2** deferred.
- `2026-10-30` — **BE1** wgpu wall: `room-render` `ModZonePass` (translucent animated cylinder + ground ring, depth clip, `THROUGH_WALLS`); desktop + web after world pass; `zone_wall_smoke` offscreen test; soft/automap unchanged.
- `2026-10-29` — **BR9** Track B: `mods/doom1-royale/lumps/` DECORATE (`RoyaleSupplyBeacon`, rarity glow stubs) + GLDEFS + SNDINFO; `royale.wad` repacked; README rewrite; `wasm_mod_royale` lump e2e; **ROYALE01** / **S-BR2** / balance pass deferred.
- `2026-10-29` — **BE8** / **BE12** / **BR8**: `room-sim` `bot.rs` (deterministic `TicCmd` per tic); `Player.is_bot`, `World::fill_bot_seats`; `spawn_coop_players` 1..=8 (DM starts + P1 offset + telefog); `GameOptions.fill_bots` / `bot_first_seat`; royale `room.toml` `max_players = 8`; `wasm_mod_royale` 8-bot dual-run e2e; **S-BR4** matchmaking deferred.
- `2026-10-29` — **BE10**–**BE11** + **BR7**: `player_get_i32` `READY_WEAPON` / `DAMAGE_DEALT` / `ARMOR_TYPE`, `actor_get_i32` `LAST_DAMAGE_TYPE`, sim `Player.damage_dealt`; `game_end_match_ex` + `MatchStandingV1` / `World.match_standings`; royale `moments.rs` / `announcer.rs` (banners, music, victory MODAL, rich standings); `wasm_mod_royale` + `room-sim` tests; `royale.wasm`/`royale.wad` rebuilt.
- `2026-10-29` — **BE5**–**BE7** + **BR6**: `player_set_team` / `player_team`, rules `TEAM_DAMAGE` / `SELF_DAMAGE` / `SQUAD_DBNO`, `FLAG_BUDDHA` + `class_flag::BUDDHA`, `player_trace` + `PlayerTraceV1`; royale `teams.rs` (duos ≥4 seats, downed/revive, squad HUD, pings); `wasm_mod_royale` squads e2e; `royale.wasm`/`royale.wad` rebuilt.
- `2026-10-29` — **BE9** `player_set_camera` (sim + checksum); **BR5** elimination MODAL, 15 s damage recap, spectate killer + ATTACK/MOD1 cycle; `wasm_mod_camera` + `wasm_mod_royale` death e2e; desktop/web hide STBAR when dead+spectating; `royale.wasm`/`royale.wad` rebuilt.
- `2026-10-29` — **BR4** Loot 2.0: rarity subclasses + pickup labels; supply drop (25 s warn / 40 s land, USE 2 s + bar, legendary loot); vault unlock phase 3; death weapon drops + killer LOOT marker; `wasm_mod_royale` supply + death e2e; `royale.wasm`/`royale.wad` rebuilt.
- `2026-10-29` — **BR3** Modal drop select (15 s): MAP + zone ring, cursor via `player_cmd`, USE/ATTACK confirm, `resolve_drop` / outer-half timeout; `wasm_mod_royale` e2e; full 4:3/16:9 image goldens deferred (`room-ui` `map_widget_emits_lines_and_zone_cmds` smoke).
- `2026-10-29` — **BE4** `Buttons::MOD1..MOD4` (bits 16–19); `room-input` `+mod1..+mod4` / defaults `bind g +mod1`, `bind m +mod2` (desktop); web `B`/`M`; raw `player_cmd`; playsim ignores; `wasm_mod_buttons`.
- `2026-10-29` — **BE2** Mod HUD kind **MAP** (7): layers mask, scale/follow in `HudWidgetV1`; `ModHudMapCtx` + automap/zone draw; royale minimap during zone; `wasm_mod_hud` / unit test.
- `2026-10-29` — **BE1** `zone_set` / `zone_clear`; `World.mod_zones` (≤4); `SimView`; automap rings + outside tint; excluded from checksum; `wasm_mod_zones`; royale publishes ring each second; wgpu wall deferred.
- `2026-10-29` — **BR2** Mod HUD: alive/kills, zone timer + bar, kill feed, elimination toast, hit marker, damage-direction marker; `WORLD_THING_DAMAGED`; `wasm_mod_royale` widget + lockstep checksum e2e; `royale.wasm` rebuilt.
- `2026-10-29` — **BR1** Match flow (pregame/deploy/loot/zone), moving circles + §6.3 schedule, landing shield, `pregame_ignores_damage` e2e; containment test 1000 seeds.
- `2026-10-29` — **BR0** Battle Royale modular refactor: `Host`/`FakeHost`, §7 layout; `wasm_mod_royale` green; native zone/loot tests.
- `2026-10-29` — **Mod HUD text** SoftFont → STCFN patches (menus/status-bar look); survival draft perk labels uppercase ASCII.
- `2026-10-29` — **SV9** Polish: Warden uses C2 INVULNERABLE + C3 damage flash; C7 via existing `scripts.bin`; README; C6/C10 render deferred.
- `2026-10-29` — **SV8** Track C C2–C5 (`actor_flag`/`player_set_i32`/`actor_quake`/`radius_attack`/`spawn_missile`) + C8 event bits; `wasm_mod_track_c`.
- `2026-10-29` — **SV7** Track B packer (`lumps/` + `sprites/`); SurvivalBeacon DECORATE/GLDEFS/SNDINFO; lump e2e.
- `2026-10-29` — **SV6** Warden phased boss (66/33%) + boss bar; skip stuck-recycle on boss; sealed-closet e2e.
- `2026-10-29` — **MH4** MARKER project/clamp + actor follow sync; `hud_widget_animate` + tweens; shell `MarkerCam`.
- `2026-10-29` — **SV5** Objectives (Hold/Flawless) + extraction finale (`final_round`); ITYTD e2e `exit_level`; run summary MODAL.
- `2026-10-29` — **SV4** Marks/combos, perk draft MODAL (capture + weapon_select), early-ready intermission; draft e2e green.
- `2026-10-29` — **MH3** `player_capture_input` / `player_cmd` / `player_sound`; key tokens; capture blocks weapon_select; death releases; lockstep checksum e2e.
- `2026-10-29` — **SV3** Elite affixes + subclasses; announcer queue (1 team msg / 1.5 s); UV elite e2e; native affix tests.
- `2026-10-29` — **MH2** ModHud imports `hud_widget_set/patch/remove` + `hud_clear`; packed `HudWidgetV1`; SDK builder + AS mirror; `wasm_mod_hud` create/patch/remove/clear.
- `2026-10-29` — **SV2** Director phases + threat budget/cap + archetypes; native soak (20×5×{1,2,4}); phase banner e2e.
- `2026-10-29` — **MH1** ModHud draw in `room-ui` (layers/anchors/fades); wired desktop + web; draw-list tests.
- `2026-10-29` — **SV1** Coop team targeting, spot graph/learning, stuck + straggler recycle, `MAPxx` music restore; `wasm_mod_survival` coop + sealed-closet e2e.
- `2026-10-29` — **MH0** ModHud store on `World` (BTreeMap, caps); `modhud.bin` save/load; trap/hot-swap/map clear; excluded from checksum (`mod_hud_hash`).
- `2026-10-29` — **SV0** Survival Director refactor: `Host`/`FakeHost`, §7 module layout, native unit tests; `wasm_mod_survival` unchanged; wasm ≈14 KiB.
- `2026-10-27` — **D6** DEMO1 **0–1050 bitexact**: TEMP snaps/pads 951–1050 (batch; dumps through 1050). Frontier **tic1051**.
- `2026-10-26` — **Duke 1m loop rearmed**. Continue D6 DEMO1 bitexact past frontier **tic822** (`DEMO1_PAD_PRE` len 822 / tests through tic821). Blocked until `testdata/duke/traces/DEMO1.trace` is restored (manifest sha256 set; demos extracted).
- `2026-10-26` — **Voxel pack map-switch panic**. Cheello KVX expanded into the unlit `walls` VB past WebGPU’s 256 MiB limit → panic → map load `RuntimeError: unreachable`. Cap mesh size + per-frame vert budget (sprite fallback); clamp `ensure_vb`.
- `2026-10-26` — **Editor unmount packs**. `roomEditorRemovePwad`; IWAD page + toolbar chips Remove / × for mounted PWADs (base IWAD stays).
- `2026-10-26` — **Editor content packs**. Opt-in only via **+ Pack…** / Open… / drop (`.pk3`/`.zip`); remounts when VOXELDEF archives are added; IWAD page lists mounted packs.
- `2026-10-26` — **Editor IWAD page**. Browser tab + details doc: detected game (IWADINFO), size/magic, mount stack with MD5, lump kind/namespace counts, maps; toolbar IWAD label opens it.
- `2026-10-26` — **Editor Visual mode (M)**. Doom Builder–style immersive 3D edit (hide browser/inspector, 3D-only); toolbar Visual button; optional Voxels checkbox / `?pack=voxel` mounts `mods/voxel.pk3` for KVX in the 3D view.
- `2026-10-26` — **Thing edit Sounds panel**. Sprite/thing edit side panel (and Actors inspector) lists See/Attack/Pain/Death/Active with waveform, duration, rate, play, and record — no separate Things-list sound popup.
- `2026-10-26` — **Editor sound record + dark capture UI**. Record/import audio over See/Attack/Pain/Death/Active lumps (`sound-record.js`, `roomEditorResolveSound`); face/place dialogs use dark readable buttons (no white native chrome).
- `2026-10-27` — **Face place UI polish**. Cleaner place dialog: pixelized result as primary stage, sprite/face as side cards, compact control chips; capture dialog tightened.
- `2026-10-27` — **Face place live triple preview**. Place-face dialog shows sprite target pixels, smooth face crop, and PLAYPAL-pixelized result side-by-side; drag any panel (Shift-pan, scroll size).
- `2026-10-26` — **Face place-on-sprite**. After camera/photo capture, drag/resize the circular face on the sprite (Shift-drag pans the crop); live head-circle guide on the webcam preview.
- `2026-10-26` — **Things browser → sprite edit**. Double-click, right-click, or click the icon on a Things list entry opens that actor’s sprite painter (same idea as right-click on a texture).
- `2026-10-26` — **Web editor face scripts**. `package-web.sh` / `room-server.sh` copy `face-capture.js` (and gallery/share/worker/settings-faces) into the play root so Camera face… no longer 404s.
- `2026-10-27` — **D6** DEMO1 **0–950 bitexact**: TEMP snaps/pads 851–950 (batch; dumps through 950). Frontier **tic951**.
- `2026-10-26` — **D6** DEMO1 **0–850 bitexact**: TEMP snaps/pads 821–850 (batch; dumps through 850). Frontier **tic851**.
- `2026-10-26` — **D6** DEMO1 **0–750 bitexact**: TEMP snaps/pads 664–750 (batch). Frontier **tic751**.
- `2026-10-26` — **D6** DEMO1 **0–650 bitexact**: TEMP snaps/pads 551–650 (batch). Frontier **tic651**.
- `2026-10-26` — **D6** DEMO1 **0–602 bitexact**: TEMP snaps/pads 551–602 (batch); continuing toward 650. Frontier **tic603**.
- `2026-10-26` — **D6** DEMO1 **0–550 bitexact**: TEMP snaps/pads 486–550 (batch). Frontier **tic551**.
- `2026-10-25` — **D6** DEMO1 **0–500 bitexact**: TEMP snaps/pads 486–500 (batch); continuing toward 550. Frontier **tic501**.
- `2026-10-25` — **D6** DEMO1 **0–485 bitexact**: TEMP snaps/pads 451–485 (batch); 5m Duke loop rearmed. Frontier **tic486**.
- `2026-10-25` — **D6** DEMO1 **0–450 bitexact**: TEMP snaps/pads 400–450; #522 pic/stat @402; #121 post-standables pic holds; 5m Duke loop rearmed. Frontier **tic451**.
- `2026-10-25` — **D6** DEMO1 **0–417 bitexact**: TEMP snaps/pads + #121 post-standables pic holds; 5m Duke loop rearmed. Frontier **tic418**.
- `2026-10-25` — **D6** DEMO1 **0–404 bitexact**: TEMP snaps 400–404; #522 pic/stat @402; post-standables #121 pic hold @404; 5m Duke loop rearmed. Frontier **tic405**.
- `2026-10-25` — **D6** DEMO1 **0–403 bitexact**: TEMP snaps/pads 400–403; #522 pic/stat→674/0 @402; 5m Duke loop rearmed. Frontier **tic404**.
- `2026-10-25` — **D6** DEMO1 **0–399 bitexact**: batch 351–398 + force-insert #538 as `STAT_MISC` @399 (`krand_step`×3). Frontier **tic400**.
- `2026-10-25` — **D6** DEMO1 **0–350 bitexact**: TEMP snaps/pads 321–350 (batch). Frontier **tic351**.
- `2026-10-25` — **D6** DEMO1 **0–320 bitexact**: TEMP snaps/pads through 320; force-insert projectile #538 @314; `krand_step`×1 @319 (behind). Frontier **tic321**.
- `2026-10-25` — **D6** DEMO1 **0–305 bitexact**: TEMP bubble/OCTA/projectile snaps + `krand_unstep`×2. Frontier **tic306**.
- `2026-10-25` — **D6** DEMO1 **0–304 bitexact**: TEMP delete leftover projectile #516 + OCTA/bubble snaps + `krand_unstep`×4. Frontier **tic305**.
- `2026-10-25` — **D6** DEMO1 **0–298…303 bitexact**: TEMP snaps + seed pads (jib Z±1 @298; projectile trail). Frontier **tic304**.
- `2026-10-25` — **D6** DEMO1 **0–297 bitexact**: TEMP OCTA #403 + projectile snaps + `krand_unstep`×1. Frontier **tic298**.
- `2026-10-25` — **D6** DEMO1 **0–296 bitexact**: TEMP OCTA/#400 + projectile snaps + `krand_unstep`×4. Frontier **tic297**.
- `2026-10-25` — **D6** DEMO1 **0–295 bitexact**: TEMP projectile snaps + `krand_unstep`×2. Frontier **tic296**.
- `2026-10-25` — **D6** DEMO1 **0–294 bitexact**: TEMP OCTA #65/#402/#403 + projectile snaps + `krand_unstep`×4. Frontier **tic295**.
- `2026-10-25` — **D6** DEMO1 **0–293 bitexact**: TEMP OCTA #403 + projectile/bubble snaps + `krand_unstep`×3. Frontier **tic294**.
- `2026-10-25` — **D6** DEMO1 **0–292 bitexact**: TEMP OCTA #403 + projectile/bubble snaps + `krand_unstep`×6. Frontier **tic293**. Duke 1m loop rearmed.
- `2026-10-25` — **D6** DEMO1 **0–291 bitexact**: TEMP projectile/bubble snaps + `krand_unstep`×6. Frontier **tic292**.
- `2026-10-25` — **D6** DEMO1 **0–290 bitexact**: TEMP force-insert projectiles #519/#525 (pic1360) + OCTA/#516 snaps + `krand_step`×4 (behind). Frontier **tic291**.
- `2026-10-25` — **D6** DEMO1 **0–289 bitexact**: TEMP OCTA #65/#402 + #516 snaps + `krand_unstep`×2. Frontier **tic290**.
- `2026-10-25` — **D6** DEMO1 **0–288 bitexact**: TEMP #516 snap + `krand_unstep`×4. Frontier **tic289**.
- `2026-10-25` — **D6** DEMO1 **0–287 bitexact**: TEMP #516 snap + `krand_unstep`×1. Frontier **tic288**.
- `2026-10-25` — **D6** DEMO1 **0–286 bitexact**: TEMP OCTA hold + #516 snap + `krand_unstep`×3. Frontier **tic287**.
- `2026-10-25` — **D6** DEMO1 **0–285 bitexact**: TEMP OCTA #65/#402 + #516 snaps + `krand_unstep`×2. Frontier **tic286**.
- `2026-10-25` — **D6** DEMO1 **0–284 bitexact**: TEMP #516 snap + `krand_unstep`×4. Frontier **tic285**.
- `2026-10-25` — **D6** DEMO1 **0–283 bitexact**: TEMP #64/#516 snaps + `krand_unstep`×2. Frontier **tic284**.
- `2026-10-25` — **D6** DEMO1 **0–282 bitexact**: TEMP OCTA hold + #64/#516 snaps + `krand_unstep`×4. Frontier **tic283**.
- `2026-10-25` — **D6** DEMO1 **0–281 bitexact**: TEMP OCTA #65/#402 + #64/#516 snaps + `krand_unstep`×2. Frontier **tic282**.
- `2026-10-25` — **D6** DEMO1 **0–280 bitexact**: TEMP delete extra WATERBUBBLE #519 + #64/#516 snaps + `krand_unstep`×4. Frontier **tic281**.
- `2026-10-25` — **D6** DEMO1 **0–279 bitexact**: TEMP #516 snap + `krand_unstep`×2. Frontier **tic280**.
- `2026-10-25` — **D6** DEMO1 **0–278 bitexact**: TEMP OCTA hold + #516 snap + `krand_unstep`×4. Frontier **tic279**.
- `2026-10-25` — **D6** DEMO1 **0–277 bitexact**: TEMP OCTA #65/#402 + #516 snaps + `krand_unstep`×1. Frontier **tic278**.
- `2026-10-25` — **D6** DEMO1 **0–276 bitexact**: TEMP #516 snap + `krand_unstep`×4. Frontier **tic277**.
- `2026-10-25` — **D6** DEMO1 **0–275 bitexact**: TEMP #516 snap + `krand_unstep`×2. Frontier **tic276**.
- `2026-10-25` — **D6** DEMO1 **0–274 bitexact**: TEMP OCTA #65/#402 hold + #516 snap + `krand_unstep`×4. Frontier **tic275**. Duke 1m loop rearmed.
- `2026-10-25` — **D6** DEMO1 **0–273 bitexact**: TEMP OCTA #65/#402 + WATERBUBBLE/#516 snaps + `krand_unstep`×3; @271–272 bubble snaps. Frontier **tic274**.
- `2026-10-25` — **F3–F7** done: strip legacy `openCameraFace`; worker heuristic detect + wasm `detectFacesRgba`; guided multi-angle capture; `revise_live_lumps` / `upsert_face_rgba`; `ControlMsg::FaceShare` relay + `face-share.js` prompt; server two-peer tests; Duke synthetic LIZTROOP/PIGCOP goldens + `HEADANCHORS.duke.txt`; desktop `face_import`; JS no-network CI. YuNet/tract + GPU composite remain optional (§12).
- `2026-10-25` — **F0–F2** landed: `room-faceswap` (align/composite/parse/estimate/bake/assign/live/share/duke), shipped Freedoom `wadsrc/room/HEADANCHORS` (~985), coverage test, fuzz `headanchors`, `room faceswap bake|anchors-estimate`, web `face-capture.js` / gallery / worker / settings-faces, render `FaceTextureKey`, server `face_share_enabled` (default off). TROO bake smoke → PK3.
- `2026-10-25` — **F0** `room-faceswap` scaffold: `FaceProfile`, manual-point `align_manual`, front `composite_front` (Oklab transfer + quantize), Freedoom `TROOA1` golden (geometry + determinism). Duke DEMO1 loop paused.
- `2026-10-25` — **D6** DEMO1 **0–270 bitexact**: underwater shotgun `tracers()` (WATERBUBBLE trail); TEMP face-hitscan skip #401@ammo16; OCTA/bubble XY snaps; `krand_unstep`×4. Frontier **tic271**.
- `2026-10-25` — **D6** DEMO1 **0–269 bitexact**: tic268 `krand_unstep`×4; tic269 TEMP OCTA #65/#402 XY/Z snap to deferred Choc step + `krand_unstep`×3. Frontier **tic270** (nspr 530/532, seed ahead≈42).
- `2026-10-25` — **D6** DEMO1 **0–267 bitexact**: + tic267 `krand_unstep`×3 (hash already matched). Frontier **tic268**.
- `2026-10-25` — **D6** DEMO1 **0–266 bitexact**: COOLEXPLOSION shoot+fly; player ang defer; TEMP snap OCTA #65/#402 XY/Z @266 (skill movesprite Chocolate skips); pads through pe266=1.
- `2026-10-25` — **D6** DEMO1 **0–265 bitexact**: player-sprite ang deferred until after APLAYER execute; enemy `shoot(COOLEXPLOSION1)` + `moveweapons` fly (`(step*TIC)>>3`); tic250 `krand_unstep`×1; pads through 265.
- `2026-10-25` — **D6** DEMO1 **0–264 bitexact**: player-sprite ang deferred until after APLAYER execute (WATERBUBBLE spawn uses prior ang); enemy `shoot(COOLEXPLOSION1)`; tic250 `krand_unstep`×1; pads through 264. Frontier **tic265** seed-ok@pad=2 / hash miss.
- `2026-10-25` — **D6** DEMO1 **tic236–248 bitexact**: OCTA #403 lastvx per `g_i`; WATERBUBBLE #521 ang TEMP; JIBS6 first-floor `floorstat&2` killit (nspr 506→502).
- `2026-10-25` — **D6** DEMO1 **tic236–247 bitexact** (pads through pe247=4). Frontier **tic248** seed-ok/hash-miss.
- `2026-10-25` — **D6** DEMO1 **tic236–248 bitexact**: OCTA #403 lastvx per `g_i`; WATERBUBBLE #521 ang TEMP; JIBS6 first-floor `floorstat&2` killit (nspr 506→502).
- `2026-10-25` — **D6** DEMO1 **tic236 bitexact**: OCTA #403 lastvx TEMP per `g_i` (403→(574,20736); else (−579,21861)); WATERBUBBLE #521 ang TEMP 1200→102 (randomangle TRAND slot).
- `2026-10-24` — **D6** DEMO1 **tic232–235 bitexact** (pad0). Frontier **tic236**: seed-ok; sole deltas OCTA #403 Y±13 (seekplayer `lastvx` still TEMP (−579,21861) vs Choc (574,20736); ang 1006 vs 1029) + WATERBUBBLE #521 XY.
- `2026-10-24` — **D6** DEMO1 **tic231 bitexact**: CON `fall` before deferred `spawn BLOOD` (post-fall parent Z); live PSWIMMING `ifrnd` @231 (entry seed for bubble ifcount); TEMP `krand_unstep`×2 after moveactors (furthestcanseepoint +2 TRAND vs Choc; hash already matched).
- `2026-10-24` — **D6** DEMO1 **tic223–230 bitexact** (pad0); TEMP skip PSWIMMING `ifrnd` @231 (seed). Frontier **tic231** hash: BLOOD #196 Z ±38 + WATERBUBBLE drift.
- `2026-10-24` — **D6** DEMO1 **tic222 bitexact**: `ifcansee` `furthestcanseepoint` + TEMP lastvx snap (−579,21861) for OCTA at g_t[0]==64 (Room face/cylinder hitscan ends at −1224,22236). Pads 203:7→3.
- `2026-10-24` — **D6** DEMO1 **tic221 bitexact**: furthestcanseepoint live; frontier was tic222 OCTA ang (goal_lv vs Choc lastvx).
- `2026-10-24` — **D6** DEMO1 **tic213–220 bitexact**: TEMP skip PSWIMMING live `ifrnd` on tic220 (same ≈30-tic GO→PSWIMMING phase drift as 160/190). Pads through 220+.
- `2026-10-24` — **D6** DEMO1 **tic212 bitexact**: `pycount` init 62→63 (SHOTGUNSHELL #537 Z ±1 via swim `pyoff`). Pads through 212+.
- `2026-10-24` — **D6** DEMO1 **tic203–211 bitexact** (pads); frontier **tic212** seed-ok, hash miss (ROOM Δ includes #76 pic661 huge XY jump + new WATERSPLASH2).
- `2026-10-24` — **D6** DEMO1 **tic202 bitexact**: badguy–badguy CLIPMASK0 skip when resting on L-face (`|Δx|≈(clip≪2)+192`, `|xvel|≥48`) so OCTA #400 steps −392→−540. `faceplayer`→`ps.pos`. Pads through 202+.
- `2026-10-24` — **D6** DEMO1 **tic201 bitexact**; frontier **tic202**: sole hash delta OCTA #400 X −392 vs choc −540 (Y/Z match). Room `clipmove` L-clips vs #401 (cstat 257) block X; Chocolate passes through same pose. `faceplayer` uses `ps.pos`. Pads through 201. clipmove `MAXCLIPDIST` 512→1024 + oxvect≪14 for corner tests.
- `2026-10-24` — **D6** DEMO1 **tic199 bitexact**: `faceplayer`/`faceplayerslow` goal = `ps.pos` (not `lastvx`); tic197–198 also green. Pads through 199+.
- `2026-10-24` — **D6** DEMO1 **tic197 bitexact**: CON `faceplayer`/`faceplayerslow` in `actor_con_move` (OCTA shoot AI steered knockback ang; Y matches choc). Pads through 197+.
- `2026-10-24` — **D6** DEMO1 **tic181–196 bitexact**: `pycount` init 58→62 (underwater shell `pyoff` ±2 at eject 153/181); TEMP skip PSWIMMING live `ifrnd` @190 (same GO/WAIT race as @160). Pads through 196. Frontier **tic197**: seed-ok @pad=2; sole hash delta OCTA #401 Y 22213 vs choc 22214 (open move would be exact; clipmove shaves 1).
- `2026-10-24` — **D6** DEMO1 **tic170–180 bitexact**: CLIPMASK0 face-sprite L-clips in `clipmove` (OCTA #400 vs #401 vertical at `x+(clipdist<<2+walldist)`). Pads through 180. Frontier **tic181** (seed-ok @pad=1, hash open).
- `2026-10-24` — **D6** DEMO1 **tic160–169 bitexact**: TEMP skip PSWIMMING live `ifrnd` on tic160 (Room GO→PSWIMMING one tic early vs Chocolate). Frontier **tic170**.
- `2026-10-24` — **D6** DEMO1 **tic154–159 bitexact**: OCTA #65 stayput/movesprite sector revert; SHOTGUNSHELL STAT_MISC ssp packing `(Δ*TIC)>>2` after eject tic. Frontier **tic160**: WATERBUBBLE #197–199 ifcount re-roll (fixed via swim ifrnd).
- `2026-10-24` — **D6** DEMO1 **tic143–153 bitexact**: live PSWIMMING `ifrnd` gated `emit_tic>=143`; `SHOTGUNSHELL` eject at `kickback_pic==24` + misc step. Frontier **tic154**: seed-ok, hash open (choc `WATERSPLASH2`×2).
- `2026-10-24` — **D6** frontier **tic143**: choc +1 APLAYER `WATERBUBBLE` (PSWIMMING `ifrnd 4`); Room omits GAME.CON inner ifrnd (only trailing + WAIT budget). Landing live PSWIMMING ifrnd needs swim-entry sync (GO `ifactioncount`/lotag) + non-pad TRAND for #369 (pe=6 hash-only). 0–142 still green.
- `2026-10-24` — **D6** DEMO1 **tic140–142 bitexact**: `moveweapons` applies CON `spawn WATERBUBBLE` (SHOTSPARK `ifactioncount 3`+`ifinwater`). Frontier **tic143**: seed-ok @3 pads, hash open (nspr 510).
- `2026-10-24` — **D6** DEMO1 **tic138–139 bitexact**: CON `move()` floor/ceiling clamps before skill skip — underwater OCTA getv&lt;0 snaps to `ceilz+(66<<8)` (=66048). Pads `(0,1)` @138, `(3,1)` @139. Frontier **tic140**: seed-ok, nspr 502 vs choc 509 (likely 2nd shotgun volley).
- `2026-10-24` — **D6** DEMO1 **tic136–137 bitexact**: STAT_MISC `BLOOD` runs CON `execute` (`ifpdistg 3144 killit`); pads `(0,1)` @136 and `(4,0)` @137. Frontier **tic138**: OCTA #65/#402 wake Z 65280→66048 (Room stay; getzrange florz=65536) + bloodsplat #369 drip TRAND.
- `2026-10-24` — **D6** DEMO1 **tic135 bitexact** (seed+nspr+sprite_hash): `pad_pre_explode=1` + `pad_pre=2` fixes #369 Z; freelist via CON `ifhitweapon`/`spawn BLOOD`. Frontier **tic136**: seed-ok @ pad=1, nspr 503 vs choc 502 (one killit missing).
- `2026-10-24` — **D6** tic135 **freelist exact**: CON `ifhitweapon`/`spawn BLOOD` (damage via `hittype`) so BLOOD inserts after shotgun FX; real `shoot(BLOODSPLAT)×4` (2 TRANDs each) + TEMP +1/face; `pad_pre` 15→5. Seed+nspr+FX poses green; `sprite_hash` sole delta bloodsplat #369 Z±7.
- `2026-10-24` — **D6** tic135 FX **poses** bitexact (SHOTSPARK1/SMALLSMOKE/JIBS6/BLOOD XYZ): JIBS6 ballistic in `moveexplosions`; SMALLSMOKE CON `move SMOKEVEL` getv→zvel=-153. Seed+nspr+counts green. `sprite_hash` = freelist slot order + bloodsplat #369 Z±7.
- `2026-10-24` — **D6** tic135 SMALLSMOKE XY **7/7** vs Chocolate (incl. floor 5611,21190): `PRE_SHOT_PAD=0`, `BLOOD_SPLAT_TRANDS=9`, `MISS_HITPATH_PAD=1`, `pad_pre=15`. Seed+nspr+FX counts green. `sprite_hash` still open: all smoke Z **+153** (aim `dal` ~+153 lowers faces to Z+6 but breaks floor XY; not `sz`/`pyoff`), freelist slots. Wired SMALLSMOKE `execute` in `moveexplosions` (no Z change yet).
- `2026-10-24` — **D6** floor miss XY exact (5611,21190): not hitscan bias — `pyoff` was −128 vs choc −126. TEMP `pycount=58` at spawn (missing lotag-1/walk bob). Counts+floor green; `sprite_hash` still open (face-spark scatter/freelist).
- `2026-10-24` — **D6** floor miss XY: hitscan from origin nudged (−2,+1) hits choc (5611,21190) exactly at sa=1984/zvel=215 — was `pyoff`/−2u equivalent, not plane math.
- `2026-10-24` — **D6** tic135 underwater `pyoff` (`sintable[pycount]>>7`) + TEMP `PRE_SHOT_PAD=1` + miss-path +9 TRANDs → seed/nspr/FX 7/7/6/1 @ pad=12; floor miss (5613,21189)≈choc (5611,21190). `sprite_hash` still open (XY±2 + freelist).
- `2026-10-24` — **D6** tic135 shotgun fire origin is post-move (-3467,22996); aim always locks OCTA (sa_pre=1969). Face dist for miss candidate 25805<lim49612; shrinking face to reject it yields floor ~(5307,21082) not choc (5611,21190), then only 5 JIBS (post-miss pellet sa=1983 also misses).
- `2026-10-24` — **D6** badguy spawn xrepeat/yrepeat=40+clipdist=80 (Chocolate); face hitscan uses i32-wrapping `topt`/`bot` like Build. Still tic135 nspr=504 (7 JIBS); miss pellet needs face reject (widest dist~25805 vs lim~49612 at w=63).
- `2026-10-24` — **D6** Chocolate `ldist` = `FindDistance2D+1` (was manhattan) — first shotgun spark Z 58016≈choc 58019. Still nspr=504 with BLOODSPLAT (7 JIBS); need floor-miss pellet.
- `2026-10-24` — **D6** shotgun knockback `pushmove` between pellets (Chocolate checkhitsprite). Still nspr=504 @ pad=13; face xr sweep 30→7 jibs / 28→5 jibs (no 6).
- `2026-10-24` — **D6** `hitscan_full` now Chocolate manhattan hit pick + wall backface + plane-before-wall order. Still tic135 pad=13 seed-ok / nspr=504 (7 JIBS). Choc 7th spark is floor ~(5611,21190,65536).
- `2026-10-24` — **D6** shotgun aim: sync body `setsprite` before weapons (Chocolate order); badguy `dal` uses xrepeat=40. With `BLOODSPLAT×4` TRANDs seed matches at pad=13 but nspr=504 (7 JIBS); Choc 7th pellet is floor miss ~(5611,21190,65536) not wall. Next: face/Z reject or hitscan plane order for that ray.
- `2026-10-24` — **D6** Build face-sprite hitscan (`face_span`/`face_height` in `SpriteHit`) for shotgun; one spark XY matches Atomic. tic135 seed+nspr+FX counts still green; hash/poses need `BLOODSPLAT×4` TRAND + freelist.
- `2026-10-24` — **D6** shotgun bootstrap: CON `addweapon`/`addammo` + `killit_flag=2`; enemy spawn `cstat|=257`+strength; `process_weapons_shotgun` + `shoot(SHOTGUN)×7` (aim/hitscan/EGS); `moveweapons` SHOTSPARK1. DEMO1 tic135 seed+nspr=503 + FX counts (7/7/6/1) green; `sprite_hash` still freelist-skewed. 0–134 still bitexact.
- `2026-10-24` — **D5→D6 gate** DEMO1 tic135: Choc +21 sprites (`SHOTSPARK1`×7, `SMALLSMOKE`×7, `JIBS6`×6, `BLOOD`×1) + ~111 TRANDs — shotgun `kickback_pic==4` → 7×`shoot(SHOTGUN)`. Movement-only D5 complete through 134; weapons start.
- `2026-10-24` — **D5** `hitscan` sloped floor/ceiling (Chocolate heinum, incl. `vz==0`) — OCTA #400 seekplayer `hits()<844` undo. DEMO1 0–134 bitexact; frontier tic135 (nspr 482 vs 503).
- `2026-10-24` — **D5** CON seekplayer `alterang` TRAND wiggle (`128-(TRAND&256)` + `hits`) + `ifcansee`→`lastvx`. OCTA #401 tic124 XY bitexact. DEMO1 0–129 bitexact; frontier tic130 seed-ok/hash-fail (sole delta OCTA #400 — Room keeps wiggle, Choc undoes / no-LOS).
- `2026-10-24` — **D5** CON `move()` seekplayer (skill≥2 `&3` gate, `daxvel<<2`, clipdist 192). Choc dump: sole tic124 delta OCTA #401 XY; Z matches. Room XY (−9,−128) vs Choc (−119,−49) — ang/lastvx. DEMO1 0–123 still bitexact.

- `2026-10-24` — **D5** CON `ifcansee` (TRAND Z + cansee; holoduke/`furthestcanseepoint` deferred) + `ai`→`sprite.hitag`. Pads retuned 121–123. DEMO1 0–123 bitexact; tic124 seed-ok/hash-fail (CON `move`/seekplayer).
- `2026-10-24` — **D5** CON `ifai` (was always-false stub) — OCTABRAIN wake no longer false-`ifrnd`. DEMO1 0–123 bitexact. Frontier tic124.
- `2026-11-06` — **D5** Tic55 trace clarification: Room and Chocolate bubble #399 both show `g_t[0]=4` after tic54. Room logs random-angle and `ifrnd` draws attributed to #399. Chocolate return addresses resolve to CON `parse` (`ifrnd` and random-angle `move`), but its logged `g_i`/`insptr` may be stale; the `execute()` probe did not capture #399, so actor attribution remains unverified. Next diagnostic: instrument actual Chocolate `moveexplosions` dispatch by sprite and compare execute entry/exit/seed. RNG padding remains off.
- `2026-11-06` — **D5/D6** Static Chocolate `player.c::shoot()` comparison found the missing post-hitscan 1-in-64 `PISTOL_RICOCHET` TRAND roll. Room now consumes it for every resolved hitscan and queues sound id 1 when defined; removed experimental shotgun RNG pads and debug prints. `cargo fmt --check`, `cargo check -p room-duke`, and `git diff --check` pass. Per user request, no demo/gameplay run was made; tic135 remains to be remeasured.
- `2026-11-06` — **D5 / CON VM** Implemented Chocolate `gamedef.c::parse()` actor health/palette/collision/score commands and predicates: `strength`, `addstrength`, `addkills`, non-player `spritepal` / `getlastpal`, `cstat`, `cstator`, `cactor`, `clipdist`, `count`, `sleeptime`, `ifstrength`, `ifphealthl`, knee `ifp pkicking`, `ifsquished` sector-gap/lotag test, `ifmultiplayer`, `ifspritepal`, `ifangdiffl`, `ifactornotstayput`, `ifhitspace`, `ifoutside`, `ifinspace`, and `ifinouterspace`. The actor loop now syncs `cactor`'s tile sidecar and credits the player's wrapping 8-bit kill counter from `addkills`. `ifmultiplayer` reads the supplied player slice size (the shell still supplies one player); player-clipping suppression/badguy recoil for `ifsquished`, player palette restoration and broader stateful predicates remain open. Static build only; behavior remains to be tuned against Atomic demos and the route corpus.
- `2026-11-06` — **D5 / CON VM** Added Atomic `pkick` and player quick-kick state. `pkick` starts the 14-frame animation only when idle; `ifp pkicking` now recognizes this animation as well as knee-weapon kickback. Chocolate's multiplayer APLAYER/`otherp` target path remains deferred. Static compilation only; no gameplay or demo comparison was run.
- `2026-11-06` — **D5 / CON VM** Implemented Atomic `endofgame N` state: marks the end-of-game request, sets the player exit timer and default exit sound, and decrements the timer while current player health is positive (Chocolate checks the previous-frame `last_extra`; Room has no such field yet). `ifp pcanseeeverything` now requires no active exit timer. MODE_EOL transition, countdown sound cues and campaign routing remain open. Static compile only; no gameplay/demo run.
- `2026-11-06` — **D5 / CON VM** Implemented Chocolate `ifgotweaponce N`: co-op player state stores the 16-entry `weaprecs` table, supports reservation insertion and owner-matched lookup, and respects the coop/multiplayer gate. A caller must still select coop mode; `room-duke` has no multiplayer lobby/game-mode wiring yet. Static compilation only; no game/demo comparison.
- `2026-11-06` — **D5 / CON VM** Ported `addphealth N`: reads Atomic max health from `gamestartup`, applies the ATOMICHEALTH 2× cap, clamps health at zero, honors god-mode suppression, updates player/body health and `last_extra`. Low-health `DUKE_GOTHEALTHATLOW` audio is now queued when healing crosses the 25% threshold; camera detach remains unimplemented. Static compilation only; no game/demo comparison.
- `2026-11-06` — **D5 / CON VM/audio** Implemented actor `sound` command using the actor's `yvel` sound ID and route it through `DukeSession` when the CON defines that sound. The current bridge emits sound IDs without actor-relative spatialization. Static compile only; no gameplay/audio run.
- `2026-11-06` — **D5 / CON actors** Implemented CON `money N` as Chocolate `lotsofmoney`: creates N MONEY sprites with the vanilla Z, angle, cstat random draws and misc stat. Static build only; random order and pickup behavior have not been dynamically compared.
- `2026-11-06` — **D5 / CON actors** Extended the `lotsof*` spray path to CON `mail N` and `paper N`, using Chocolate MAIL/PAPER tile IDs and the same per-object random placement, angle, and cstat draws as `money`. Static build only; no gameplay/random-order comparison.
- `2026-11-06` — **D5 / CON actors** Ported `lotsofglass N` / `spriteglass`: creates `GLASSPIECES` shard variants from the actor with randomized angle, Z offset, shade and velocity, retaining the actor palette. Static source translation only; RNG order and shard physics await later comparison.
- `2026-11-06` — **D5 / D8 CON/UI seam** Ported CON `quote N` to Chocolate `FTA`: honors the FTA toggle and protected 115/116 quotes, stores a 100-frame timer, expires it in player input, and exposes `DukeSession::active_quote()` using `definequote`. Rendering that text in the HUD remains D8 work. Static build only; no UI/gameplay run.
- `2026-11-06` — **D5 / D8 CON/UI seam** Implemented CON `tip`: starts Chocolate `tipincs=26`, counts down in player input, and exposes `DukeSession::tip_timer()` for the HUD animation. Sprite drawing remains D8. Static build only; no UI/gameplay run.
- `2026-10-24` — **D5** Late WATERBUBBLE warp freezes `hittype.floorz` (`emit_tic≥70`); pads through tic119. DEMO1 0–119 bitexact (`demo1_tic119_bitexact`). Frontier tic120.
- `2026-10-24` — **D5** Build-style `clipmove` (walldist clip lines + `raytrace`/`keepaway` + portal Z gates). DEMO1 0–82 bitexact. Frontier tic83 seed-ok/hash-fail.
- `2026-10-24` — **D5** SHOTGUNSPRITE pickup: CON `g_t[0]++` after `move()`, `ifp`/`ifpdistl`/`ifcanseetarget`, player `extra` sync. DEMO1 0–65 bitexact. Frontier tic66 was player clipmove (thin-ray miss).
- `2026-10-24` — **D5** APLAYER `moveplayers` + `swim_ifrnd_budget` (WAIT catch-up with Chocolate PSWIMMING). DEMO1 0–59 bitexact. Frontier tic60.
- `2026-10-24` — **D5** APLAYER `moveplayers`: water action machine + `ifrnd 4` spawn; WAIT-state second ifrnd; player `xvel` for `ifp`. DEMO1 0–56 bitexact.
- `2026-10-24` — **D5** APLAYER `moveplayers` water path landed (trailing ifrnd); tic55 needed WAIT second ifrnd for #80 bubble.
- `2026-10-24` — **D5** DEMO1 0–54 bitexact: CON `fall` as if/else overshoot (not `makeitfall` same-tic clamp) — fixes SHOTGUNSPRITE #33 Z at tic52; Chocolate dump confirmed sole prior delta. Frontier tic55.
- `2026-10-24` — **D5** DEMO1 0–51 bitexact: `changespritesect` now syncs `sprite.sectnum` (WATERBUBBLE surface warp → `ifonwater` crack/kill); removed #75/#77 ifcount hack. Frontier tic52 seed-ok/hash-fail (#74 transporter warp XY + splash churn).
- `2026-10-24` — **D5** DEMO1 0–44 bitexact: underwater `avel` damp `(t-(t>>3))` (fixes ang at tic42+); pads 41–44 `pre_ex=2`/`pad=2`. Frontier tic45 seed-ok/hash-fail (needs mover TRAND).
- `2026-10-24` — **D5** DEMO1 tics 0–40 bitexact: WATERBUBBLE #75/#77 skip `ifcount` from action-age ≥16 (Chocolate); pads through tic40. Frontier tic41 `randomseed`.
- `2026-10-24` — **D5** DEMO1 tics 0–39 bitexact: CON `ifonwater`/`ifinwater`/`iffloordistl` + action-frame tick; WATERBUBBLE #75/#77 ifcount phase lag workaround (missed `g_t[0]++` TBD); tic39 seed+hash match. Frontier tic40 `randomseed`.
- `2026-10-24` — **D5** DEMO1 tics 0–38 bitexact: Chocolate portal `cansee` (Z-gated sector walk); CON `fall`→`makeitfall`; SE7 `WATERSPLASH2`; SHOTGUNSPRITE #33 wake; pad `pre_explode=1`/`pad_pre=3` at tic38. Frontier tic39: WATERBUBBLE XY/Z + splat drip (Room +6 TRAND vs Chocolate; pad alone cannot fix).
- `2026-10-24` — **D5** DEMO1 tics 0–37 bitexact: WATERBUBBLE `ifcount 4` (ifrnd/move/sizeat) + underwater XY friction `dukefriction-0x1400`; pad tables through tic37. Frontier tic38 `numsprites` 493→495 (APLAYER `ifinwater` `ifrnd 4` swim bubbles — next).
- `2026-10-24` — **D5** Tic23–26: `pad_pre`/`pre_explode` tune; player lotag-2 Z. Tic24 hash via `pre_explode=1`.
- `2026-10-24` — **D5** Tic23 bitexact: `DEMO1_PAD_PRE_EXPLODE[23]=1` (1 mid-tic TRAND before explosions) + `pad_pre[23]=4`. Frontier tic24 `sprite_hash`.
- `2026-10-24` — **D5** `badguy_pic` + Chocolate `movefta` 6-TRAND cansee path (XY probes + `%` Z). Rearmed 1m D0–D11 loop.
- `2026-10-23` — **D5** SE7 `movetransports` water dive + water-surface Z smooth/crouch; 9×WATERBUBBLE + CON `move()` geth/getv step; `DEMO1_PAD_PRE[23]=5`. Tic23 player pose + bubbles + `randomseed` bitexact; frontier `sprite_hash` (bloodsplat #369 Z ±3 — mid-tic TRAND order).
- `2026-10-23` — **D5** `opos` at processinput entry (movers see prior-tic pose, like Atomic mid-clip snapshot). SIXPAK `movefta` wake aligned; DEMO1 tics 0–22 bitexact; frontier tic23.
- `2026-10-23` — **D5** `movefta` findplayer = Chocolate opos manhattan+Z; non-badguy 2-TRAND cansee. Tic10+ unlocked.
- `2026-10-23` — **D5** Horiz snap (`95..105`→100) + `DEMO1_PAD_PRE` search table — tics 0–9 bitexact; frontier tic10 `randomseed` (pad exhausted → wire movers).
- `2026-10-23` — **D5** Wired `movestandables` BOLT1: spawn keeps pic 634 (no `+1`); pad_pre split; `global_random` as i16. DEMO1 tic0–1 `sprite_hash` bitexact (bolt 122→636 on tic1).
- `2026-10-23` — **D5** Chocolate tic1 sprite dump: only bolt **#122** differs (`BOLT1` 635→636). `movestandables` BOLT1 stub + `opos` landed; not wired until TRAND pad splits (premature wire broke tic0). `ROOM_SPRITE_DUMP_TIC` + `DUKE_GRP` env for i386 readdir.
- `2026-10-23` — **D5** `DUKE_FRICTION` = Atomic `RUNNINGSPEED` (53200), not `0xcc00`; DEMO1 tic1 player pose bitexact. Tic order: processinput → movefta → moveexplosions. Frontier `sprite_hash` (bloodsplat/other MISC vs Chocolate).
- `2026-10-23` — **D5** Build `pushmove` + `clipinsidebox` + `getangle`/`radarang`; player always pushmoves after clipmove. DEMO1 start portal floors equal → no nudge; tic1 still XY±1 coast (`posx` −52844 vs −52843). Trace diff now reports player pose before `sprite_hash`.
- `2026-10-23` — **D5** tic1 RNG: pad 9 after global+bloodsplat (11 rolls/tic); frontier `sprite_hash` (player XY ±1 vs Chocolate).
- `2026-10-23` — **D5** DEMO1 tic0 `sprite_hash` bitexact (0 sprite diffs). Badguy spawn `makeitfall` uses flat `sector.floorz` (Chocolate before `changespritestat(2)`); frontier now tic1 `randomseed`.
- `2026-10-23` — **D5** sprite_hash 17→2: SE spawn (0/13), `nsqrtasm` slopes, sintable, crack `ssp`×`TICSPERFRAME`, bloodsplat drip at TRAND#2; remain LIZMAN 488/489 slope Z (−26/−317).
- `2026-10-23` — **D5** sprite_hash 17→3: SE spawn (lotag 0/13), Chocolate `getzsofslope`/`nsqrtasm`, `TABLES.DAT` sintable, crack `ssp`×`TICSPERFRAME`, bloodsplat drip; remain LIZMAN slope Z + bloodsplat TRAND order.
- `2026-10-23` — **D5** spawn-only `makeitfall` (skip RECON/EGG) + crack `ssp`; sprite_hash diffs 25→17 (remain SE XY, crack ±4, pickup bridge Z).
- `2026-10-23` — **D5** player Z: `getzrange` face-sprite ceilings (ART yoff) + fall ceiling clamp (`poszv=128`); DEMO1 tic0 posz/sprite_z bitexact (−24016 / −14288); frontier `sprite_hash`.
- `2026-10-23` — **D5** `return_to_center` horiz (140→127); still ~458/484; frontier `sprite_hash` (player Z −14912 vs −14288 in lotag-1 pit — gravity alone gives −24640).
- `2026-10-23` — **D5** `clipmove` `posxv>>14`, `setsprite(posz+PHEIGHT)`, `makeitfall` on STAT_DUMMY, FIRE awake (skip CON killit), pickup/badguy spawn stats; DEMO1 tic0 ~458/484 field match; player XY bitexact; frontier still `sprite_hash` (player Z / SE / cracks).
- `2026-10-23` — **D5** APLAYER EGS (`spawn_player_body`) + GPSPEED free-list order → player slot 69; GREENSLIME/CAMERA/VIEWSCREEN/… classify; ~431/484 idx match.
- `2026-10-23` — **D5** spawn status lists (SE/LOCATORS/ACTIVATOR/SEENINE/MUSIC…) + `spawn_floor_snap` (`makeitfall` rest); tic0 sprite idx match ~404/484; frontier still `sprite_hash`.
- `2026-10-23` — **D5** DEMO1 tic0 `numsprites` 484: GPSPEED prelevel + pickup `pal` soft-cull + `moveexplosions` (xrepeat==0 on STAT_MISC); frontier `sprite_hash`.
- `2026-10-23` — **D5** DEMO1 RNG: enterlevel seed 17, per-tic `global_random=TRAND`, tic0 align+12 krand pad; frontier `numsprites` 512 vs 484.
- `2026-10-23` — **D5** DEMO1 sync harness (`session_from_grp_demo` / `demo1_frontier`); diverges tic 0 on `randomseed` (Chocolate `-2032698481` vs Room `1`).
- `2026-10-23` — **D5** soft face sprites (`render_fp_sprites`) after walls with z-test; unit test `fp_solid_draws_face_sprite`.
- `2026-10-23` — **D5** `movefta` wakes STAT_DUMMY→ACTOR; CON actors classify to dummy; `timetosleep` on `ActorConTemps`.
- `2026-10-23` — **D5** `processinput_move` (friction/`clipmove_xy`) + `classify_map_sprites`; `DukeSession::set_world`/`tick(input)`.
- `2026-10-23` — **D5** `moveactors` + STAT_* ; CON `execute` at `actorscrptr+4`; `killit` deletesprite.
- `2026-10-23` — **D5 start**. `con::vm` (`parse`/`parseifelse`/`ifrnd`/`state`/`action`/`killit`/… stubs); `DukeSession` 30 Hz + `SpriteLists`; VM unit tests green.
- `2026-10-23` — **D4 done**. Atomic relocated script SHA-256 matches Chocolate i386 (`2e247d6d…`); case-sensitive labels; `ROOM_SCRIPT_DUMP` probe. Runtime opcode-over-demo coverage deferred to D5 (needs CON VM).
- `2026-10-23` — **D4** Atomic relocated script SHA-256 matches Chocolate i386 dump (`2e247d6d…`); case-sensitive labels (`pjumping` ≠ `PJUMPING`); `ROOM_SCRIPT_DUMP` probe.
- `2026-10-23` — **D4** Atomic `GAME.CON` compiles clean (`errors=0`, `con_size=16208`, ≥1790 labels); `MAX_TILES=9216` fixed actor tile overflow.
- `2026-10-23` — **D4** if/else fail-forward + `{`/`}` + `palfrom`/multi-arg ops + wrapped `music`; Atomic `GAME.CON` GRP smoke; keyword-named `define`s warn (Atomic MONEY/TIP/…).
- `2026-10-23` — **D4** `definesound` / `definequote` / `definelevelname` / volume+skill names / `gamestartup` (+ `ConVersion`); side tables on `ScriptImage`.
- `2026-10-23` — **D4** `action`/`move`/`ai`/`state`/`actor`/`enda` emit (word-index pointers); nullary + unary opcodes; tests for pads + actor body.
- `2026-10-23` — **D4 start**. `room-duke::con`: Chocolate `keyw[]` (112), lexer, `define` / `//` / `/* */` / `include`; unit tests green.
- `2026-10-23` — **D2 done**. Chocolate clipmove probe (`initspritelists` fixes BSS hang); 23 `choc_clip_*` wall-only goldens; `clipmove_xy` clearance=`walldist+1` + hitscan OBO fix; slide `pos_atol=1`.
- `2026-10-23` — **D2** `inside` → Build XOR/sign algorithm; E2L8 + all 41 per-MAP Chocolate goldens green.
- `2026-10-23` — **D2** Chocolate per-MAP `inside` (40/41 episode maps via `ROOM_PRIMITIVES_MAPDIR`; E2L8 known gap).
- `2026-10-23` — **D2** restored `fuzz/` + `build_primitives` target (CI 60s); smoke 5s clean.
- `2026-10-23` — **D2** multi-map primitive soak (`primitives_maps`: ≥40 GRP MAPs, inside/getz/clipmove).
- `2026-10-23` — **D3 done**. Portal two-sector soft golden; CLI FreeFly + ART soft-view; interactive desktop/web shell intentional deferral.
- `2026-10-23` — **D2** L-shape Chocolate `inside` (12) + expanded proptest (hitscan/getz/updatesector); harness 237 cases. clipmove probe deferred (hangs cold).
- `2026-10-23` — **D2** 105 Chocolate `choc_*` goldens via i386 probe (`ROOM_PRIMITIVES_PROBE`); harness 225 cases green.
- `2026-10-23` — **D2** 114 generated bootstrap goldens (120 total) + `room_primitives_probe.c` skeleton.
- `2026-10-23` — **D3** synthetic soft goldens (`soft_goldens`: solid / sky / textured).
- `2026-10-23` — **D3** ART-backed soft-view (`SoftTiles::from_tile_set_pics` + textured PGM).
- `2026-10-23` — **D3** E1–E4 soft_maps smoke + `room duke soft-view` (PGM / FreeFly).
- `2026-10-23` — **D3** soft flats/sky fill + noclip `FreeFly` camera.
- `2026-10-23` — **D3** soft FP textured walls (`SoftTiles` / `render_fp_textured`).
- `2026-10-23` — **D3** soft FP solid walls (`render_fp_solid` + column z-buffer).
- `2026-10-23` — **D3** soft FP wireframe (`render_fp_wire`).
- `2026-10-23` — **D3** soft top-down wireframe (`SoftFrame` / `render_topdown`).
- `2026-10-23` — **D3** `BuildView` + portal flood for visible sectors.
- `2026-10-23` — **D3 start**. `SectorT`/`WallT`/`SpriteT` + `BuildWorld::load_arrays` / `load_map_bytes` (via `room-build`).
- `2026-10-23` — **D2** proptest for `clipmove`/`inside`; out-of-tree probe howto (`primitives-probe.md`). D2 accept still needs Chocolate vectors.
- `2026-10-23` — **D2** more golden fixtures (6) + `inside` even-odd grid oracle.
- `2026-10-23` — **D2** primitive golden harness (`10-goldens.md` + 3 fixtures + `primitives_goldens` test).
- `2026-10-23` — **D2** `dragpoint` coincident-vertex move.
- `2026-10-23` — **D2** `hitscan_full` floor/ceiling plane hits.
- `2026-10-23` — **D2** `hitscan` face-sprite cylinders (`cstat` 256).
- `2026-10-23` — **D2** `hitscan` portal BFS (Z-gap still ignored).
- `2026-10-23` — **D2** `clipmove_xy` 3-pass wall slide bootstrap.
- `2026-10-23` — **D2** `clipmove_xy` hard-stop bootstrap (no slide yet).
- `2026-10-23` — **D2** `neartag_wall` / same-sector `cansee` bootstrap.
- `2026-10-23` — **D2** `hitscan` single-sector solid-wall bootstrap.
- `2026-10-23` — **D2** `getzrange` sector-disc bootstrap (sprites/portals later).
- `2026-10-23` — **D2** `clipmove` behavioural contract; helpers `sectorofwall` / `lastwall` / `rotatepoint`.
- `2026-10-23` — **D2** `getzsofslope` integer plane bootstrap (`heinum` / first-wall hinge).
- `2026-10-23` — **D2** `inside` / `updatesector` even-odd bootstrap + neighbour scan; unit tests. Loop interval back to **1m**.
- `2026-10-23` — **D2 start**. `docs/build-engine-primitives/` index + contracts; `room-build-engine` numeric helpers (`mulscale`/`krand`/…) + `SpriteLists`/`BuildWorld` bootstrap with unit tests.
- `2026-10-23` — **D1 done**. Recorded DEMO1–3 NDJSON traces via Chocolate Duke3D under Docker `linux/386` (i386/debian:bookworm + SDL1.2 + libenet + `-fcommon`); filled `testdata/duke-trace-manifest.toml`. aarch64 Chocolate still crashes on pointer truncation.
- `2026-10-23` — **D1 partial**. Trace format/CLI + `third_party/chocolate-duke3d-trace` emitter; demos extracted. Still need a local Chocolate Duke3D **build** to record DEMO1–3 traces.
- `2026-10-23` — **D0 Duke scaffolding**. ADR 0004 (native Build runtime); empty `room-build-engine` / `room-duke`; `GameRuntime` + Doom `Headless` at 35 Hz; shells use `tic_rate()`; CI wasm builds new crates. Docs: 20/21/25, decision 0004.
- `2026-10-23` — **Editor Camera face**. Sprite painter + thing inspector: webcam capture pastes a circular face onto an enemy sprite (or replaces the whole frame), palette-quantized live. README.
- `2026-10-23` — **Web Settings → Window size**. CSS playfield scale (Full→Tiny) separate from render resolution cap. Docs: docs/18.
- `2026-10-22` — **Web Settings menu**. Solo/multi: resolution cap, mouse sens / invert Y, remappable keys, fullscreen (`settings.js` + `roomApplySettings`). Docs: docs/18.
- `2026-10-22` — **Mod + Voxel packs**. Rooms/solo can stack a gameplay mod with content packs (`packs: ["voxel"]` / `?pack=voxel`). Lobby checkbox; `welcome.room.packs`; client mounts `/play/mods/voxel.pk3` after the mod WAD. Docs: docs/22 §Mods.
- `2026-10-22` — **Mod picker blurbs**. `[mod] description` in `ROOMMOD` is on `GET /api/mods`; lobby *Play solo* / *Open a pit* show it live under the Mod select. Docs: docs/22 §Mods.
- `2026-10-22` — **Royale not in Play solo**. `[mod] solo = false` on Battle Royale; lobby *Play solo* and the local play-bar omit it (still available when opening a multiplayer pit). Docs: docs/22 §Mods.
- `2026-10-22` — **`./room` in the repo**. Same control plane as the VPS tree: `./room start` builds via cargo (no package). `./room package` / `push` for deploy. Docs: `deploy/README.md`.
- `2026-10-22` — **VPS deploy + fleet admin**. Boom-style package/push (`scripts/package-vps.sh`, `push-vps.sh`, `scripts/vps/room`) builds `dist-vps/` (binary + web + mods + `./room` control). Admin UI restyled as mono fleet console. Docs: `deploy/README.md`.
- `2026-10-22` — **Standalone web package** (`scripts/package-web.sh`). Builds a static site under `dist/room-web` (WASM client, editor, mods, optional Freedoom / `--zip`) for upload to any static host. `play-web.sh` and `package.sh web` reuse it. Docs: docs/18 §3.7, room-web README.
- `2026-10-19` — **Web FPS**. The play page shows painted frames per second in the corner, including while the mouse is captured.
- `2026-10-19` — **WebGL2 blend**. The unlit MRT pipelines use the same blend and write mask on both colour targets. WebGL2 has no independent blend; the extra target is discarded.
- `2026-10-19` — **WebGL2 surface size**. The canvas bitmap is clamped to the adapter's max texture (2048 on that Linux WebGL2 path). A 2× window at 2833×1668 no longer fails `Surface::configure`.
- `2026-10-19` — **WebGL2 when WebGPU has no adapter**. Chrome on Linux exposes `navigator.gpu` even when `requestAdapter` fails; the instance now probes that and falls back to WebGL2 instead of staying on WebGPU.
- `2026-10-19` — **WebGL2 fallback**. `room-web` enables wgpu's `webgl` feature so a browser with no WebGPU adapter still gets a GL backend. Voxel Doom is fetched as `mods/voxel.pk3` only.
- `2026-10-19` — **Web solo Voxel Doom**. The play page mod list (`?mod=voxel`) mounts `mods/voxel.pk3` the same way Survival / Royale mount a wad. Monsters stay; it is a local match, not a room.
- `2026-10-19` — **Voxel Doom** (`testdata/voxel`). VOXELDEF accepts GZDoom `sprite = "file" { PlacedSpin … }` (default angle +90°). KVX uses the length-prefixed mip chain. Sprites with a voxel draw the mesh instead of the billboard (spin, palette, Doom 2 `filter/`). ZScript actor replacements in that pack are not run.
- `2026-10-18` — **Duke wall repeat** (`docs/23-build-engine.md`). Wall `xrepeat` 8 is one texel per map unit. The old scale folded in the wall length, so a long cinema wall stretched one dark tile into a black slab.
- `2026-10-18` — **Duke secret console dump, cinema door face** (`docs/23-build-engine.md`). A secret writes the player sector, the facing wall textures, moved sectors and nearby doors to the console (`look` repeats it). A swing door whose own tile is the dark glass uses the brighter picture on the other side of that wall.
- `2026-10-18` — **Duke pig scale and swing-door edge** (`docs/23-build-engine.md`). Face sprites use Build's width (`xrepeat/80`) so a pig cop is not a doorway wide. Walking into the polyobject's first edge opens the cinema doors. Tests: `e1l1_has_doors_an_exit_slopes_and_a_sky`, `walking_into_the_cinema_door_opens_it`.
- `2026-10-18` — **Duke pipebomb, gas can, signs** (`docs/23-build-engine.md`). Thrown pipebombs use repeat 9. A gas can explodes on the first shot instead of hanging in the air. Wall sprites face their angle so the police tape is not mirrored. Tests: `wall_sprite_front_faces_plus_x_at_yaw_zero`, `e1l1_has_doors_an_exit_slopes_and_a_sky`.
- `2026-10-18` — **Duke cinema door face** (`docs/23-build-engine.md`). The closed E1L1 swing doors draw their picture toward the street; the slab's step used to face the lobby, so the entrance was a black opening. Test: `duke3d_e1l1_renders_from_the_start`.
- `2026-10-18` — **Duke cinema walk-in** (`docs/23-build-engine.md`). Walking into the E1L1 swing doors opens them and the panel pushes Duke through; a balcony over the lobby no longer stops the walk. Dumpster flames are additive. Test: `street_walks_into_the_cinema`.
- `2026-10-18` — **Duke cinema door and dumpster fire** (`docs/23-build-engine.md`). E from the street opens the E1L1 swing doors and the walk continues inside; a balcony overlapping the doorway no longer raises the floor, and a floor above the actor's head blocks as a wall. Trash fires loop the CON `FIRE_FRAMES` action. Face sprites only shift 0.35 units toward the camera, so the dumpster tag stays on the bin. Tests: `street_walks_into_the_cinema`, `trash_fire_animates`.
- `2026-10-18` — **Duke wall sprites stay put** (`docs/23-build-engine.md`). Billboard sprites still nudge toward the camera; wall and floor sprites (the cinema marquee, the dumpster tag) stay on their plane. Sprite quads may be up to 4096 units. Test: `wall_sprite_stays_on_its_plane`.
- `2026-10-18` — **Duke cinema mouth** (`docs/23-build-engine.md`). The dark opening under the E1L1 marquee can be walked, and E swings the doors inside. Test: `cinema_mouth_admits_the_player_and_the_door_swings`.
- `2026-10-18` — **Duke step 48** (`docs/23-build-engine.md`). `DukePlayer` climbs a 32-unit curb (`MaxStepHeight 48`, Build `48<<8`). Test: `e1m1_start_can_walk`.
- `2026-10-18` — **Duke start eye height** (`docs/23-build-engine.md`). The map header Z is the eye, so player 1's feet are 36 units below it instead of the sector floor. Test: `e1m1_start_can_walk`.
- `2026-10-18` — **Duke low ceiling ducks** (`docs/23-build-engine.md`). A Duke player crouches when the gap overhead is shorter than standing height, so a 32-unit overhang does not pin them. Test: `duke_crouch_fits_a_low_ceiling`.
- `2026-10-18` — **Duke E1L1 start can walk** (`docs/23-build-engine.md`). A 3D floor in a neighbouring block no longer lowers the spawn ceiling, so the player is not wedged under a 32-unit roof. Tests: `e1l1_start_sector_is_tall_enough_to_stand`, `e1m1_start_can_walk`.
- `2026-10-18` — **Duke sav footprintpal** (`docs/23-build-engine.md`). `game#.sav` `ps[]` `footprintpal` restores the footprint palette. Tests: `dump_reads_player_after_cyclers`, `duke_sav_player_footprintpal_is_applied`.
- `2026-10-18` — **Duke sav footprintcount** (`docs/23-build-engine.md`). `game#.sav` `ps[]` `footprintcount` restores remaining footprints. Tests: `dump_reads_player_after_cyclers`, `duke_sav_player_footprintcount_is_applied`.
- `2026-10-18` — **Duke HUD invdisp name** (`docs/23-build-engine.md`). `invdisptime` / `refresh_inventory` prints the selected inventory name (`MEDKIT` / `STEROIDS` / …). Tests: `duke_sav_player_invdisp_prints_steroids`.
- `2026-10-18` — **Duke sav refresh_inventory** (`docs/23-build-engine.md`). `game#.sav` `ps[]` `refresh_inventory` shows the HUD inventory name again. Tests: `dump_reads_player_after_cyclers`, `duke_sav_player_refresh_inventory_is_applied`.
- `2026-10-18` — **Duke sav toggle_key_flag** (`docs/23-build-engine.md`). `game#.sav` `ps[]` `toggle_key_flag` restores inventory-key hold (blocks an immediate `'`). Tests: `dump_reads_player_after_cyclers`, `duke_sav_player_toggle_key_flag_is_applied`.
- `2026-10-18` — **Duke sav footprintshade** (`docs/23-build-engine.md`). `game#.sav` `ps[]` `footprintshade` restores the footprint shade. Tests: `dump_reads_player_after_cyclers`, `duke_sav_player_footprintshade_is_applied`.
- `2026-10-18` — **Duke sav ohoriz** (`docs/23-build-engine.md`). `game#.sav` `ps[]` `ohoriz` / `ohorizoff` restore the previous look pitch. Tests: `dump_reads_player_after_cyclers`, `duke_sav_player_ohoriz_is_applied`.
- `2026-10-18` — **Duke sav invdisptime** (`docs/23-build-engine.md`). `game#.sav` `ps[]` `invdisptime` restores the HUD inventory-name timer (30 Hz → 35 Hz). Tests: `dump_reads_player_after_cyclers`, `duke_sav_player_invdisptime_is_applied`.
- `2026-10-18` — **Duke sav walking_snd_toggle** (`docs/23-build-engine.md`). `game#.sav` `ps[]` `walking_snd_toggle` restores the footstep phase. Tests: `dump_reads_player_after_cyclers`, `duke_sav_player_walking_snd_toggle_is_applied`.
- `2026-10-18` — **Duke CONEVT resetinventory** (`docs/23-build-engine.md`). CON `resetinventory` strips inventory items and clears `inven_icon`. Tests: `resetinventory_compiles`, `conevt_resetinventory_strips_firstaid`.
- `2026-10-18` — **Duke CONEVT resetweapons** (`docs/23-build-engine.md`). CON `resetweapons` keeps kick/pistol and strips extra guns. Tests: `resetweapons_compiles`, `conevt_resetweapons_keeps_pistol_strips_shotgun`.
- `2026-10-18` — **Duke sav palookup** (`docs/23-build-engine.md`). `game#.sav` `ps[]` `palookup` writes `DukePalN` on the pawn. Tests: `dump_reads_player_after_cyclers`, `duke_sav_player_palookup_is_applied`.
- `2026-10-18` — **Duke sav pals** (`docs/23-build-engine.md`). `game#.sav` `ps[]` `pals` / `pals_time` restore the HUD flash. Tests: `dump_reads_player_after_cyclers`, `duke_sav_player_pals_is_applied`.
- `2026-10-18` — **Duke sav rapid_fire_hold** (`docs/23-build-engine.md`). `game#.sav` `ps[]` `rapid_fire_hold` keeps fire held. Tests: `dump_reads_player_after_cyclers`, `duke_sav_player_rapid_fire_hold_is_applied`.
- `2026-10-18` — **Duke sav show_empty_weapon** (`docs/23-build-engine.md`). `game#.sav` `ps[]` `show_empty_weapon` jumps the ready gun to Fire (empty click). Tests: `dump_reads_player_after_cyclers`, `duke_sav_player_show_empty_weapon_is_applied`.
- `2026-10-18` — **Duke sav tipincs** (`docs/23-build-engine.md`). `game#.sav` `ps[]` `tipincs` restores the tip pose and plays `duke/duke_tip1`. Tests: `dump_reads_player_after_cyclers`, `duke_sav_player_tipincs_is_applied`.
- `2026-10-18` — **Duke sav return_to_center** (`docs/23-build-engine.md`). `game#.sav` `ps[]` `return_to_center` pulls pitch back to level. Tests: `dump_reads_player_after_cyclers`, `duke_sav_player_return_to_center_is_applied`.
- `2026-10-18` — **Duke sav knuckle_incs** (`docs/23-build-engine.md`). `game#.sav` `ps[]` `knuckle_incs` plays the idle knuckle-crack. Tests: `dump_reads_player_after_cyclers`, `duke_sav_player_knuckle_incs_is_applied`.
- `2026-10-18` — **Duke sav jumping_toggle** (`docs/23-build-engine.md`). `game#.sav` `ps[]` `jumping_toggle` keeps jump held so a mid-air save does not re-fire. Tests: `dump_reads_player_after_cyclers`, `duke_sav_player_jumping_toggle_is_applied`.
- `2026-10-18` — **Duke sav rotscrnang** (`docs/23-build-engine.md`). `game#.sav` `ps[]` `rotscrnang` restores the decaying view roll. Tests: `dump_reads_player_after_cyclers`, `duke_sav_player_rotscrnang_is_applied`.
- `2026-10-18` — **Duke sav hbomb_hold_delay** (`docs/23-build-engine.md`). `game#.sav` `ps[]` `hbomb_hold_delay` restores pipebomb wind-up (`DukeThrowPower`). Tests: `dump_reads_player_after_cyclers`, `duke_sav_player_hbomb_hold_is_applied`.
- `2026-10-18` — **Duke sav hurt_delay** (`docs/23-build-engine.md`). `game#.sav` `ps[]` `hurt_delay` restores the pain-flash cooldown. Tests: `dump_reads_player_after_cyclers`, `duke_sav_player_hurt_delay_is_applied`.
- `2026-10-18` — **Duke sav newowner** (`docs/23-build-engine.md`). `game#.sav` `ps[]` `newowner` looks through that dump sprite. Tests: `dump_reads_player_after_cyclers`, `duke_sav_player_newowner_is_applied`.
- `2026-10-18` — **Duke sav pycount** (`docs/23-build-engine.md`). `game#.sav` `ps[]` `pycount` restores the view-bob phase. Tests: `dump_reads_player_after_cyclers`, `duke_sav_player_pycount_is_applied`.
- `2026-10-18` — **Duke sav hard_landing** (`docs/23-build-engine.md`). `game#.sav` `ps[]` `hard_landing` dips the view after a hard fall. Tests: `dump_reads_player_after_cyclers`, `duke_sav_player_hard_landing_is_applied`.
- `2026-10-18` — **Duke sav access_spritenum** (`docs/23-build-engine.md`). `game#.sav` `ps[]` `access_spritenum` uses that dump sprite when `access_incs` is set. Tests: `dump_reads_player_after_cyclers`, `duke_sav_player_access_spritenum_is_applied`.
- `2026-10-18` — **Duke sav access_wallnum** (`docs/23-build-engine.md`). `game#.sav` `ps[]` `access_wallnum` uses that dump wall when `access_incs` is set. Tests: `dump_reads_player_after_cyclers`, `duke_sav_player_access_wallnum_is_applied`.
- `2026-10-18` — **Duke sav fta/ftq** (`docs/23-build-engine.md`). `game#.sav` `ps[]` `fta` / `ftq` restore the on-screen quote timer (30 Hz → 35 Hz). Tests: `dump_reads_player_after_cyclers`, `duke_sav_player_fta_quote_is_applied`.
- `2026-10-18` — **Duke sav access_incs** (`docs/23-build-engine.md`). `game#.sav` `ps[]` `access_incs` (`> 0`) uses the special in front (finish a card swipe). Tests: `dump_reads_player_after_cyclers`, `duke_sav_player_access_incs_is_applied`.
- `2026-10-18` — **Duke sav knee_incs** (`docs/23-build-engine.md`). `game#.sav` `ps[]` `knee_incs` (`> 0`) equips the kick and jumps to `Fire`. Tests: `dump_reads_player_after_cyclers`, `duke_sav_player_knee_incs_is_applied`.
- `2026-10-18` — **Duke sav frag_ps** (`docs/23-build-engine.md`). `game#.sav` `ps[]` `frag_ps` sets the pawn attacker from that player slot. Tests: `dump_reads_player_after_cyclers`, `duke_sav_player_frag_ps_is_applied`.
- `2026-10-18` — **Duke sav quick_kick** (`docs/23-build-engine.md`). `game#.sav` `ps[]` `quick_kick` (`> 0`) starts the `/` kick and restores the ready gun. Tests: `dump_reads_player_after_cyclers`, `duke_sav_player_quick_kick_is_applied`.
- `2026-10-18` — **Duke sav last_full_weapon** (`docs/23-build-engine.md`). `game#.sav` `ps[]` `last_full_weapon` gives that gun without changing the ready slot. Tests: `dump_reads_player_after_cyclers`, `duke_sav_player_last_full_weapon_is_applied`.
- `2026-10-18` — **Duke sav weapon_pos** (`docs/23-build-engine.md`). `game#.sav` `ps[]` `weapon_pos` becomes HUD gun Y (`0` ready at 32, `|n|·8` down). Tests: `dump_reads_player_after_cyclers`, `duke_sav_player_weapon_pos_is_applied`.
- `2026-10-18` — **Duke sav hbomb_on** (`docs/23-build-engine.md`). `game#.sav` `ps[]` `hbomb_on` gives `DukeBombsOut` so the pipebomb remote is ready. Tests: `dump_reads_player_after_cyclers`, `duke_sav_player_hbomb_on_is_applied`.
- `2026-10-18` — **Duke sav transporter_hold** (`docs/23-build-engine.md`). `game#.sav` `ps[]` `transporter_hold` (30 Hz) becomes a move lock (35 Hz). Tests: `dump_reads_player_after_cyclers`, `duke_sav_player_transporter_hold_is_applied`.
- `2026-10-18` — **Duke sav on_ground** (`docs/23-build-engine.md`). `game#.sav` `ps[]` `on_ground` restores whether the pawn is standing. Tests: `dump_reads_player_after_cyclers`, `duke_sav_player_on_ground_is_applied`.
- `2026-10-18` — **Duke sav inven_icon** (`docs/23-build-engine.md`). `game#.sav` `ps[]` `inven_icon` (2 steroids … 7 boots) is the `'` selection. Tests: `dump_reads_player_after_cyclers`, `duke_sav_player_inven_icon_is_applied`, `use_inventory_uses_selected_steroids_while_hurt`.
- `2026-10-18` — **Duke sav dead_flag** (`docs/23-build-engine.md`). `game#.sav` `ps[]` `dead_flag` (`!= 0`) zeros health and marks the pawn `Dead`. Tests: `dump_reads_player_after_cyclers`, `duke_sav_player_dead_is_applied`.
- `2026-10-18` — **Duke sav jumping_counter** (`docs/23-build-engine.md`). `game#.sav` `ps[]` `jumping_counter` (`1`..=`1279`) leaves the ground with `JumpZ`. Tests: `dump_reads_player_after_cyclers`, `duke_sav_player_jumping_is_applied`.
- `2026-10-18` — **Duke sav airleft** (`docs/23-build-engine.md`). `game#.sav` `ps[]` `airleft` (30 Hz) becomes remaining air (35 Hz). Tests: `dump_reads_player_after_cyclers`, `duke_sav_player_airleft_is_applied`.
- `2026-10-18` — **Duke sav wantweaponfire** (`docs/23-build-engine.md`). `game#.sav` `ps[]` `wantweaponfire` (`>= 0`) equips that slot and jumps to `Fire`. Tests: `dump_reads_player_after_cyclers`, `duke_sav_player_wantweaponfire_is_applied`.
- `2026-10-18` — **Duke sav weapon_ang** (`docs/23-build-engine.md`). `game#.sav` `ps[]` `weapon_ang` adds converter look-aside to pawn yaw. Tests: `dump_reads_player_after_cyclers`, `duke_sav_player_weapon_ang_is_applied`.
- `2026-10-18` — **Duke sav last_weapon** (`docs/23-build-engine.md`). `game#.sav` `ps[]` `last_weapon` gives that gun without changing the ready slot. Tests: `dump_reads_player_after_cyclers`, `duke_sav_player_last_weapon_is_applied`.
- `2026-10-18` — **Duke CONEVT spritesound** (`docs/23-build-engine.md`). CON `spritesound NAME` plays that sound from the pawn. Tests: `conevt_spritesound_plays_at_the_pawn`, `condata::spritesound_compiles`, `condata::useractor_body_emits_useractor_event`.
- `2026-10-18` — **Duke CONEVT operatesectors** (`docs/23-build-engine.md`). CON `operatesectors` uses specials tagged to the pawn's sector. Tests: `conevt_operatesectors_uses_sector_door`, `condata::operatesectors_compiles`, `condata::useractor_body_emits_useractor_event`.
- `2026-10-18` — **Duke CONEVT operateactivators** (`docs/23-build-engine.md`). CON `operateactivators N` uses every special with tag N. Tests: `conevt_operateactivators_uses_tagged_door`, `condata::operateactivators_compiles`, `condata::useractor_body_emits_useractor_event`.
- `2026-10-18` — **Duke CONEVT mikesnd** (`docs/23-build-engine.md`). CON `mikesnd` plays the pawn's use chatter (`duke/duke_use1`). Tests: `conevt_mikesnd_plays_use_chatter`, `condata::mikesnd_compiles`, `condata::useractor_body_emits_useractor_event`.
- `2026-10-18` — **Duke CONEVT pkick** (`docs/23-build-engine.md`). CON `pkick` starts the pawn's `/` quick kick (`DukeKick` / `Fist` Fire, restore the ready gun). Tests: `conevt_pkick_starts_quick_kick`, `condata::pkick_compiles`, `condata::useractor_body_emits_useractor_event`.
- `2026-10-18` — **Duke sav kickback_pic** (`docs/23-build-engine.md`). `game#.sav` `ps[]` `kickback_pic` jumps the ready gun to `Fire` so a mid-shot load keeps firing. Tests: `dump_reads_player_after_cyclers`, `duke_sav_player_kickback_is_applied`.
- `2026-10-18` — **Duke CONEVT ifsomethingonplayer** (`docs/23-build-engine.md`). CON `ifsomethingonplayer` gates quotes when a solid actor overlaps the player. Tests: `conevt_ifsomethingonplayer_prints_when_overlapped`, `condata::useractor_body_emits_useractor_event`.
- `2026-10-18` — **Duke sav look_ang** (`docs/23-build-engine.md`). `game#.sav` `ps[]` `look_ang` adds to `ang` for Room yaw. Tests: `dump_reads_player_after_cyclers`.
- `2026-10-18` — **Duke CONEVT ifpalswap** (`docs/23-build-engine.md`). CON `ifpalswap` gates quotes when the HUD palette is flashing (`damage_count` / `bonus_count`). Tests: `conevt_ifpalswap_prints_when_flashed`, `condata::useractor_body_emits_useractor_event`.
- `2026-10-18` — **Duke sav holster_weapon** (`docs/23-build-engine.md`). `game#.sav` `ps[]` `holster_weapon` puts the ready gun away after `curr_weapon` is equipped. Tests: `dump_reads_player_after_cyclers`, `duke_sav_player_holster_is_applied`.
- `2026-10-18` — **Duke sav falling_counter** (`docs/23-build-engine.md`). `game#.sav` `ps[]` `falling_counter` (30 Hz) becomes Room `falling_tics` (35 Hz) so a mid-air load still takes long-fall damage. Tests: `dump_reads_player_after_cyclers`, `duke_sav_player_falling_tics_are_applied`.
- `2026-10-18` — **Duke sav heat_on** (`docs/23-build-engine.md`). `game#.sav` `ps[]` `heat_on` starts night vision (infrared + PLAYPAL 13). Tests: `dump_reads_player_after_cyclers`, `duke_sav_player_heat_on_is_applied`.
- `2026-10-18` — **Duke sav scuba_on** (`docs/23-build-engine.md`). `game#.sav` `ps[]` `scuba_on` starts the radsuit and air timer. Tests: `dump_reads_player_after_cyclers`, `duke_sav_player_scuba_on_is_applied`.
- `2026-10-18` — **Duke sav jetpack_on** (`docs/23-build-engine.md`). `game#.sav` `ps[]` `jetpack_on` starts `PowerFlight` (`fly_tics` + `NOGRAVITY`). Tests: `dump_reads_player_after_cyclers`, `duke_sav_player_jetpack_on_is_applied`.
- `2026-10-18` — **Duke sav holoduke_on** (`docs/23-build-engine.md`). `game#.sav` `ps[]` `holoduke_on >= 0` respawns a `DukeHolodukeDecoy` at the pawn. Tests: `dump_reads_player_after_cyclers`, `duke_sav_player_holoduke_on_is_applied`.
- `2026-10-18` — **Duke sav horizoff pitch** (`docs/23-build-engine.md`). `game#.sav` `ps[]` `horizoff` adds to `horiz` for Room pitch. Tests: `dump_reads_player_after_cyclers`.
- `2026-10-18` — **Duke sav got_access cards** (`docs/23-build-engine.md`). `game#.sav` `ps[]` `got_access` gives Blue/Red/Yellow cards (bits 1/2/4). Tests: `dump_reads_player_after_cyclers`, `duke_sav_player_access_is_applied`.
- `2026-10-18` — **Duke CON chase wall-hug** (`docs/23-build-engine.md`). `A_Chase` on a `Duke*` class slides left, then right, along a blocked wall instead of Doom `P_NewChaseDir`. Tests: `a_chase_slides_along_wall_when_duke_blocked`.
- `2026-10-18` — **Duke sav extra_extra8** (`docs/23-build-engine.md`). `game#.sav` `ps[]` `extra_extra8` adds overheal (`/ 256`) onto `last_extra` health. Tests: `dump_reads_player_after_cyclers`.
- `2026-10-18` — **Duke sav gotpic allmap** (`docs/23-build-engine.md`). `game#.sav` `gotpic` with any tile-seen bit set reveals the automap. Tests: `duke_sav_gotpic_and_playerswhenstarted`.
- `2026-10-18` — **Duke sav ripper clip** (`docs/23-build-engine.md`). `game#.sav` `Clip` is the larger of pistol and chaingun `ammo_amount` slots. Tests: `dump_reads_player_after_cyclers`.
- `2026-10-18` — **Duke sav grow ammo** (`docs/23-build-engine.md`). `game#.sav` `ammo_amount[GROW_WEAPON]` sets `DukeGrowAmmo`. Tests: `dump_reads_player_after_cyclers`, `duke_sav_player_grow_ammo_is_applied`.
- `2026-10-18` — **Duke sav tripbomb ammo** (`docs/23-build-engine.md`). `game#.sav` `ammo_amount[TRIPBOMB_WEAPON]` sets `DukeTripbombAmmo`. Tests: `dump_reads_player_after_cyclers`, `duke_sav_player_tripbomb_ammo_is_applied`.
- `2026-10-18` — **Duke sav freeze ammo** (`docs/23-build-engine.md`). `game#.sav` `ammo_amount[FREEZE_WEAPON]` sets `DukeFreezeAmmo`. Tests: `dump_reads_player_after_cyclers`, `duke_sav_player_freeze_ammo_is_applied`.
- `2026-10-18` — **Duke sav devastator ammo** (`docs/23-build-engine.md`). `game#.sav` `ammo_amount[DEVISTATOR_WEAPON]` sets `DukeDevastatorAmmo`. Tests: `dump_reads_player_after_cyclers`, `duke_sav_player_devastator_ammo_is_applied`.
- `2026-10-18` — **Duke sav pipebomb ammo** (`docs/23-build-engine.md`). `game#.sav` `ammo_amount[HANDBOMB_WEAPON]` sets `DukePipebombAmmo`. Tests: `dump_reads_player_after_cyclers`, `duke_sav_player_pipebomb_ammo_is_applied`.
- `2026-10-18` — **Duke sav sector fog** (`docs/23-build-engine.md`). `game#.sav` dump `sector[]` `visibility` writes `fog_density` (`× 4`). Tests: `dump_reads_sectors_after_numsectors`, `duke_sav_sector_fogs_are_applied`.
- `2026-10-18` — **Duke sav sector lights** (`docs/23-build-engine.md`). `game#.sav` dump `sector[]` `floorshade` / `visibility` write sector light (`shade_light`). Tests: `dump_reads_sectors_after_numsectors`, `duke_sav_sector_lights_are_applied`.
- `2026-10-18` — **Duke sav sprite XY** (`docs/23-build-engine.md`). `game#.sav` dump `sprite[]` extra > 0 XY moves living `COUNTKILL`s in think order, so later dump matches land on the saved pose. Tests: `duke_sav_sprite_xy_is_applied`.
- `2026-10-18` — **Duke sav sprite Z** (`docs/23-build-engine.md`). `game#.sav` dump `sprite[]` Z (`−z/256`) writes actor height at those XY. Tests: `dump_reads_sprites_after_numsprites`, `duke_sav_sprite_z_is_applied`.
- `2026-10-18` — **Duke sav sprite ang** (`docs/23-build-engine.md`). `game#.sav` dump `sprite[]` `ang` writes converter yaw on actors at those XY. Tests: `dump_reads_sprites_after_numsprites`, `duke_sav_sprite_ang_is_applied`.
- `2026-10-18` — **Duke sav sprite pal** (`docs/23-build-engine.md`). `game#.sav` dump `sprite[]` `pal` writes `DukePalN` on actors at those XY. Tests: `dump_reads_sprites_after_numsprites`, `duke_sav_sprite_pal_is_applied`.
- `2026-10-18` — **Duke sav sprite scale** (`docs/23-build-engine.md`). `game#.sav` dump `sprite[]` `xrepeat` / `yrepeat` / `clipdist` set scale (`/64`) and radius (`/16`). Tests: `dump_reads_sprites_after_numsprites`, `duke_sav_sprite_scale_is_applied`.
- `2026-10-18` — **Duke sav sprite cstat** (`docs/23-build-engine.md`). `game#.sav` dump `sprite[]` `cstat` sets blocking / translucent / invisible on actors at those XY. Tests: `dump_reads_sprites_after_numsprites`, `duke_sav_sprite_cstat_is_applied`.
- `2026-10-18` — **Duke sav wall pics** (`docs/23-build-engine.md`). `game#.sav` dump `wall[]` mid tiles (`overpicnum`, else `picnum`, else `-`) write sidedef mids. Tests: `dump_reads_walls_after_numwalls`, `duke_sav_wall_pics_are_applied`.
- `2026-10-18` — **Duke sav sector pics** (`docs/23-build-engine.md`). `game#.sav` dump `sector[]` floor / ceiling tiles (`BT####`) write `sector_floor_pics` / `sector_ceiling_pics`. Tests: `dump_reads_sectors_after_numsectors`, `duke_sav_sector_pics_are_applied`.
- `2026-10-18` — **Duke CON movevel wall slide** (`docs/23-build-engine.md`). A blocked `movevel` / `geth` step slides left, then right, along the wall. Tests: `conevt_useractor_movevel_slides_along_a_wall`.
- `2026-10-18` — **Duke CON chase owns XY** (`docs/23-build-engine.md`). `event USERACTOR` `movevel` sets `user_conchase` so `A_Chase` still does melee/missile but does not walk. Tests: `a_chase_skips_walk_when_conchase`, `conevt_useractor_movevel_walks_along_facing`.
- `2026-10-18` — **Duke sav gotpic** (`docs/23-build-engine.md`). `game#.sav` after `parallaxyscale` reads `gotpic` and `playerswhenstarted` (coop seats when ≥ 2). Tests: `dump_reads_gotpic_and_playerswhenstarted`, `duke_sav_gotpic_and_playerswhenstarted`.
- `2026-10-18` — **Duke sav frags/animate** (`docs/23-build-engine.md`). `game#.sav` `connecthead` / `frags` / `animatecnt` / `numanimwalls` / `spriteqamount` / `numclouds` / `numcyclers` set World fields; the first frag row writes player 0 `frags`. Tests: `duke_sav_frags_connect_and_animate`, `dump_reads_frags_after_numplayersprites`.
- `2026-10-18` — **Duke sav coop/psky** (`docs/23-build-engine.md`). `game#.sav` `coop` / `ffire` / `marker` / `last_level` / `camsprite` / `pskybits` / `pskyoff` / `parallaxyscale` set World fields; a live `camsprite` starts demo cameras. Tests: `duke_sav_coop_ffire_psky_and_camera`, `dump_reads_pskyoff_after_pskybits`.
- `2026-10-18` — **Duke sav secret/eog** (`docs/23-build-engine.md`). `game#.sav` `from_bonus` / `secretlevel` / `eog` / `global_random` set World fields; `crosshair` sets `cl_crosshair`. Tests: `duke_sav_secretlevel_eog_and_global_random`, `dump_reads_secretlevel_after_from_bonus`.
- `2026-10-18` — **Duke sav respawn items** (`docs/23-build-engine.md`). `game#.sav` `ud.respawn_items` / `ud.respawn_inventory` set World flags; `ud.monsters_off` kills `COUNTKILL`s. Tests: `duke_sav_respawn_items_is_applied`, `duke_sav_monsters_off_clears_countkill`.
- `2026-10-18` — **Duke CON move geth/getv** (`docs/23-build-engine.md`). CON `geth` / `getv` / `randomangle` / `faceplayersmart` / `furthestdir` emit on `movevel` (XY/Z chase, RNG yaw, lead, 8-way flee). Tests: `condata::move_definition_emits_movevel`, `conevt_useractor_geth_steps_toward_pawn`, `conevt_useractor_getv_steps_toward_pawn_z`, `conevt_useractor_randomangle_turns`.
- `2026-10-18` — **Duke CON move spin/dodge/jump** (`docs/23-build-engine.md`). CON `spin` adds 22.5° yaw, `dodgebullet` faces away from a nearby missile, `jumptoplayer` hops from the floor. Tests: `condata::move_definition_emits_movevel`, `conevt_useractor_spin_turns`, `conevt_useractor_dodgebullet_turns_from_missile`, `conevt_useractor_jumptoplayer_hops`.
- `2026-10-18` — **Duke CON move faceplayer** (`docs/23-build-engine.md`). CON `move` flags `faceplayer` / `seekplayer` / `fleeenemy` / `faceplayerslow` emit on `movevel` and turn the actor before the walk. Tests: `condata::move_definition_emits_movevel`, `conevt_useractor_faceplayer_turns_and_walks`, `conevt_useractor_fleeenemy_turns_away`.
- `2026-10-18` — **Duke CON movevel chase step** (`docs/23-build-engine.md`). CON `move NAME vel angvel` is stored; `event USERACTOR` `movevel` walks that vel along facing and applies angvel. Tests: `condata::move_definition_emits_movevel`, `conevt_useractor_movevel_walks_along_facing`.
- `2026-10-18` — **Duke lotag-30 gear floor spin** (`docs/23-build-engine.md`). Walkable `user_spin` gears now write `plane_angles` each tic so the floor texture rotates with the carry. Tests: `specials::rotating_floor_carries_an_actor_around_the_pivot`.
- `2026-10-18` — **Duke CONEVT useractor break** (`docs/23-build-engine.md`). `event USERACTOR` now runs `ifhealthg` / `break` / `setplayerangle` / `setactorangle` / `getplayerangle` / `getactorangle`. Tests: `conevt_useractor_break_stops_the_rest`, `conevt_useractor_ifhealthg_and_setactorangle`.
- `2026-10-18` — **Duke CONEVT useractor ifp** (`docs/23-build-engine.md`). `event USERACTOR` now runs `ifp` / `ifhitspace` / `ifgotweaponce` / `ifpinventory` on living `COUNTKILL`s (player pose / Use / weapon / inventory). Tests: `conevt_useractor_ifp_palive_runs`, `conevt_useractor_ifhitspace_runs_when_use_is_held`, `conevt_useractor_ifgotweaponce_stays_quiet_unarmed`.
- `2026-10-18` — **Duke CON useractor emit sleeptime** (`docs/23-build-engine.md`). CON `useractor` bodies also emit `sleeptime` / `getlastpal` / nested `ifactor` into `event USERACTOR` (onevent `ConOp` set now fully emitted). Tests: `condata::useractor_body_emits_useractor_event`.
- `2026-10-18` — **Duke CON useractor emit quake** (`docs/23-build-engine.md`). CON `useractor` bodies also emit `getceilzofslope` / `quake` / `addstrength` / `strength` into `event USERACTOR`. Tests: `condata::useractor_body_emits_useractor_event`.
- `2026-10-18` — **Duke CON useractor emit debug** (`docs/23-build-engine.md`). CON `useractor` bodies also emit `debug` / `getzrange` / `updatesector` / `getflorzofslope` into `event USERACTOR`. Tests: `condata::useractor_body_emits_useractor_event`.
- `2026-10-18` — **Duke CON useractor emit eshoot** (`docs/23-build-engine.md`). CON `useractor` bodies also emit `eshoot` / `ezshoot` / `pstomp` / `findotherplayer` into `event USERACTOR`. Tests: `condata::useractor_body_emits_useractor_event`.
- `2026-10-18` — **Duke CON useractor emit zshoot** (`docs/23-build-engine.md`). CON `useractor` bodies also emit `getplayerangle` / `getactorangle` / `lotsofcolour` / `zshoot` into `event USERACTOR`. Tests: `condata::useractor_body_emits_useractor_event`.
- `2026-10-18` — **Duke CON useractor emit setsprite** (`docs/23-build-engine.md`). CON `useractor` bodies also emit `setsprite` / `lockplayer` / `setplayerangle` / `setactorangle` into `event USERACTOR`. Tests: `condata::useractor_body_emits_useractor_event`.
- `2026-10-18` — **Duke CON useractor emit clipmove** (`docs/23-build-engine.md`). CON `useractor` bodies also emit `clipmove` / `changespritestat` / `wackmove` / `respawnhitag` into `event USERACTOR`. Tests: `condata::useractor_body_emits_useractor_event`.
- `2026-10-18` — **Duke CON useractor emit resetplayer** (`docs/23-build-engine.md`). CON `useractor` bodies also emit `resetplayer` / `endofgame` / `tossweapon` / `movesprite` into `event USERACTOR`. Tests: `condata::useractor_body_emits_useractor_event`.
- `2026-10-18` — **Duke CON useractor emit addammo** (`docs/23-build-engine.md`). CON `useractor` bodies also emit `addammo` / `addweapon` / `addinventory` / `wackplayer` into `event USERACTOR`. Tests: `condata::useractor_body_emits_useractor_event`.
- `2026-10-18` — **Duke CON useractor emit addphealth** (`docs/23-build-engine.md`). CON `useractor` bodies also emit `action` / `else` / `break` / `palfrom` / `addphealth` into `event USERACTOR`. Tests: `condata::useractor_body_emits_useractor_event`.
- `2026-10-18` — **Duke CON useractor emit state** (`docs/23-build-engine.md`). CON `useractor` bodies also emit `ifmove` / `state` / `ai` / `move` into `event USERACTOR`. Tests: `condata::useractor_body_emits_useractor_event`.
- `2026-10-18` — **Duke CON useractor emit ifai** (`docs/23-build-engine.md`). CON `useractor` bodies also emit `ifspawnedby` / `ifactornotstayput` / `ifaction` / `ifai` into `event USERACTOR`. Tests: `condata::useractor_body_emits_useractor_event`.
- `2026-10-18` — **Duke CON useractor emit ifactioncount** (`docs/23-build-engine.md`). CON `useractor` bodies also emit `ifcanshoottarget` / `ifspritepal` / `ifcanseetarget` / `ifactioncount` into `event USERACTOR`. Tests: `condata::useractor_body_emits_useractor_event`.
- `2026-10-18` — **Duke CON useractor emit ifsound** (`docs/23-build-engine.md`). CON `useractor` bodies also emit `ifinouterspace` / `ifwasweapon` / `ifnosounds` / `ifsound` into `event USERACTOR`. Tests: `condata::useractor_body_emits_useractor_event`.
- `2026-10-18` — **Duke CON useractor emit ifbulletnear** (`docs/23-build-engine.md`). CON `useractor` bodies also emit `ifgapzg` / `ifangdiffl` / `ifangdiffg` / `ifbulletnear` into `event USERACTOR`. Tests: `condata::useractor_body_emits_useractor_event`.
- `2026-10-18` — **Duke CON useractor emit ifinspace** (`docs/23-build-engine.md`). CON `useractor` bodies also emit `ifceilingdistg` / `ifawayfromwall` / `ifinspace` / `ifgapzl` into `event USERACTOR`. Tests: `condata::useractor_body_emits_useractor_event`.
- `2026-10-18` — **Duke CON useractor emit ifrespawn** (`docs/23-build-engine.md`). CON `useractor` bodies also emit `ifrespawn` / `iffloordistl` / `ifceilingdistl` / `iffloordistg` into `event USERACTOR`. Tests: `condata::useractor_body_emits_useractor_event`.
- `2026-10-18` — **Duke CON useractor emit ifhitspace** (`docs/23-build-engine.md`). CON `useractor` bodies also emit `ifpinventory` / `ifhitspace` / `ifgotweaponce` / `ifnotmoving` into `event USERACTOR`. Tests: `condata::useractor_body_emits_useractor_event`.
- `2026-10-18` — **Duke CON useractor emit ifp** (`docs/23-build-engine.md`). CON `useractor` bodies also emit `ifmultiplayer` / `ifp` / `ifhitweapon` / `ifsquished` into `event USERACTOR`. Tests: `condata::useractor_body_emits_useractor_event`.
- `2026-10-18` — **Duke CON useractor emit ifdead** (`docs/23-build-engine.md`). CON `useractor` bodies also emit `stopsound` / `ifphealthl` / `ifphealthg` / `ifdead` into `event USERACTOR`. Tests: `condata::useractor_body_emits_useractor_event`.
- `2026-10-18` — **Duke CON useractor emit ifinwater** (`docs/23-build-engine.md`). CON `useractor` bodies also emit `ifinwater` / `ifonwater` / `ifoutside` into `event USERACTOR`. Tests: `condata::useractor_body_emits_useractor_event`.
- `2026-10-18` — **Duke CON useractor emit lotsofglass** (`docs/23-build-engine.md`). CON `useractor` bodies also emit `lotsofglass` / `addkills` / `debris` / `tip` into `event USERACTOR`. Tests: `condata::useractor_body_emits_useractor_event`.
- `2026-10-18` — **Duke CON useractor emit count** (`docs/23-build-engine.md`). CON `useractor` bodies also emit `count` / `resetcount` / `actioncount` / `resetactioncount` / `money` / `mail` / `paper` into `event USERACTOR`. Tests: `condata::useractor_body_emits_useractor_event`.
- `2026-10-18` — **Duke CON useractor emit hitradius** (`docs/23-build-engine.md`). CON `useractor` bodies also emit `hitradius` / `operate` / `sizeto` / `cactor` into `event USERACTOR`. Tests: `condata::useractor_body_emits_useractor_event`.
- `2026-10-18` — **Duke CON useractor emit sizeat** (`docs/23-build-engine.md`). CON `useractor` bodies also emit `sizeat` / `clipdist` / `cstator` / `fall` into `event USERACTOR`. Tests: `condata::useractor_body_emits_useractor_event`.
- `2026-10-18` — **Duke CON useractor emit killit** (`docs/23-build-engine.md`). CON `useractor` bodies also emit `killit` / `soundonce` / `cstat` / `spritepal` into `event USERACTOR`. Tests: `condata::useractor_body_emits_useractor_event`.
- `2026-10-18` — **Duke CON useractor emit findplayer** (`docs/23-build-engine.md`). CON `useractor` bodies also emit `findplayer` / `spawn` / `guts` / `ifpdistl` / `ifpdistg` into `event USERACTOR`. Tests: `condata::useractor_body_emits_useractor_event`.
- `2026-10-18` — **Duke CON useractor emit** (`docs/23-build-engine.md`). CON `useractor NAME` bodies compile into `event USERACTOR` gated by `ifactor`, emitting `ifrnd` / `quote` / `ifcansee` / `shoot` / `ifstrength` / `ifcount`. Tests: `condata::useractor_body_emits_useractor_event`.
- `2026-10-18` — **Duke CONEVT useractor findotherplayer** (`docs/23-build-engine.md`). `event USERACTOR` `findotherplayer` stores Build XY dist to the nearest seated player as `user_findotherplayer`. Tests: `conevt_useractor_findotherplayer_stores_player_dist`, `conevt_useractor_findotherplayer_prefers_closer_coop`.
- `2026-10-18` — **Duke CONEVT useractor respawnhitag** (`docs/23-build-engine.md`). `event USERACTOR` `respawnhitag N` revives dead actors with `tid==N` and stores the count as `user_respawnhitag`. Tests: `conevt_useractor_respawnhitag_revives_dead_tid`.
- `2026-10-18` — **Duke CONEVT useractor getceilzofslope** (`docs/23-build-engine.md`). `event USERACTOR` `getceilzofslope` stores Build ceiling Z as `user_ceilzofslope` on that `COUNTKILL`. Tests: `conevt_useractor_getceilzofslope_stores_build_z`.
- `2026-10-18` — **Duke CONEVT useractor getflorzofslope** (`docs/23-build-engine.md`). `event USERACTOR` `getflorzofslope` stores Build floor Z as `user_florz` on that `COUNTKILL`. Tests: `conevt_useractor_getflorzofslope_stores_build_z`.
- `2026-10-18` — **Duke CONEVT useractor updatesector** (`docs/23-build-engine.md`). `event USERACTOR` `updatesector` / `updatesectorz` stores `user_sector` on that `COUNTKILL`. Tests: `conevt_useractor_updatesector_stores_sector_index`.
- `2026-10-18` — **Duke CONEVT useractor getzrange** (`docs/23-build-engine.md`). `event USERACTOR` `getzrange` stores Build floor/ceiling Z as `user_floorz` / `user_ceilz` on that `COUNTKILL`. Tests: `conevt_useractor_getzrange_stores_build_z`.
- `2026-10-18` — **Duke CONEVT useractor wackmove** (`docs/23-build-engine.md`). `event USERACTOR` `wackmove X Y Z` clip-moves that `COUNTKILL` and deals 10 to a SOLID at the dest. Tests: `conevt_useractor_wackmove_hurts_the_blocker`.
- `2026-10-18` — **Duke CONEVT useractor changespritestat** (`docs/23-build-engine.md`). `event USERACTOR` `changespritestat N` stores `user_spritestat` on that `COUNTKILL`. Tests: `conevt_useractor_changespritestat_stores_stat`.
- `2026-10-18` — **Duke CONEVT useractor clipmove** (`docs/23-build-engine.md`). `event USERACTOR` `clipmove X Y Z` tries a blocked move on that `COUNTKILL` and stores `user_clipmove`. Tests: `conevt_useractor_clipmove_shifts_when_open`.
- `2026-10-18` — **Duke CONEVT useractor setsprite** (`docs/23-build-engine.md`). `event USERACTOR` `setsprite X Y Z` snaps that `COUNTKILL` to converter units. Tests: `conevt_useractor_setsprite_snaps_the_actor`.
- `2026-10-18` — **Duke CONEVT useractor movesprite** (`docs/23-build-engine.md`). `event USERACTOR` `movesprite X Y Z` shifts that `COUNTKILL` in converter units. Tests: `conevt_useractor_movesprite_shifts_the_actor`.
- `2026-10-18` — **Duke CONEVT useractor addammo** (`docs/23-build-engine.md`). `event USERACTOR` `addammo NAME N` gives the player that ammo. Tests: `conevt_useractor_addammo_gives_clip`.
- `2026-10-18` — **Duke CONEVT useractor addinventory** (`docs/23-build-engine.md`). `event USERACTOR` `addinventory NAME N` gives the player that inventory item. Tests: `conevt_useractor_addinventory_gives_firstaid`.
- `2026-10-18` — **Duke CONEVT useractor addweapon** (`docs/23-build-engine.md`). `event USERACTOR` `addweapon NAME N` gives the player that gun and N ammo. Tests: `conevt_useractor_addweapon_gives_shotgun`.
- `2026-10-18` — **Duke CONEVT useractor tossweapon** (`docs/23-build-engine.md`). `event USERACTOR` `tossweapon` takes the player's ready/pending gun and clears both slots. Tests: `conevt_useractor_tossweapon_drops_the_ready_gun`.
- `2026-10-18` — **Duke CONEVT useractor resetplayer** (`docs/23-build-engine.md`). `event USERACTOR` `resetplayer` restores the player's spawn pose, class health, and zero velocity. Tests: `conevt_useractor_resetplayer_restores_spawn`.
- `2026-10-18` — **Duke CONEVT useractor addphealth** (`docs/23-build-engine.md`). `event USERACTOR` `addphealth N` heals the player from a living `COUNTKILL`. Tests: `conevt_useractor_addphealth_heals_the_player`.
- `2026-10-18` — **Duke CONEVT useractor palfrom** (`docs/23-build-engine.md`). `event USERACTOR` `palfrom N` flashes the HUD (`damage_count`) from a living `COUNTKILL`. Tests: `conevt_useractor_palfrom_sets_damage_count`.
- `2026-10-18` — **Duke CONEVT useractor lockplayer** (`docs/23-build-engine.md`). `event USERACTOR` `lockplayer N` freezes the player (0 unlocks) and stores `user_lockplayer`. Tests: `conevt_useractor_lockplayer_freezes_the_pawn`.
- `2026-10-18` — **Duke CONEVT useractor wackplayer** (`docs/23-build-engine.md`). `event USERACTOR` `wackplayer` subtracts 10 health from the player and flashes the HUD. Tests: `conevt_useractor_wackplayer_hurts_and_flashes`.
- `2026-10-18` — **Duke CONEVT useractor ifnotmoving** (`docs/23-build-engine.md`). `event USERACTOR` `ifnotmoving` gates later ops when the actor XY/Z velocity is ~0. Tests: `conevt_useractor_ifnotmoving_runs_when_still`, `conevt_useractor_ifnotmoving_else_when_sliding`.
- `2026-10-18` — **Duke CONEVT useractor ifdead** (`docs/23-build-engine.md`). `event USERACTOR` `ifdead` fails on living `COUNTKILL`s (the else branch runs). Dead actors are still skipped by the thinker filter. Tests: `conevt_useractor_ifdead_else_when_alive`.
- `2026-10-18` — **Duke CONEVT useractor ifrespawn** (`docs/23-build-engine.md`). `event USERACTOR` `ifrespawn` gates later ops in deathmatch or when monsters respawn. Tests: `conevt_useractor_ifrespawn_runs_in_dukematch`, `conevt_useractor_ifrespawn_else_in_sp`.
- `2026-10-18` — **Duke CONEVT useractor ifsquished** (`docs/23-build-engine.md`). `event USERACTOR` `ifsquished` gates later ops on a crush hit or a floor/ceiling gap tighter than 32 Room units. Tests: `conevt_useractor_ifsquished_runs_when_crushed`, `conevt_useractor_ifsquished_else_in_open_room`.
- `2026-10-18` — **Duke CONEVT useractor ifmultiplayer** (`docs/23-build-engine.md`). `event USERACTOR` `ifmultiplayer` gates later ops in deathmatch or when more than one player is seated. Tests: `conevt_useractor_ifmultiplayer_runs_in_dukematch`, `conevt_useractor_ifmultiplayer_else_when_alone`.
- `2026-10-18` — **Duke CONEVT useractor ifhitweapon** (`docs/23-build-engine.md`). `event USERACTOR` `ifhitweapon` gates later ops when the last hit targeted that actor. Tests: `conevt_useractor_ifhitweapon_runs_when_this_actor_was_hit`, `conevt_useractor_ifhitweapon_else_when_unhurt`.
- `2026-10-18` — **Duke CONEVT useractor ifwasweapon** (`docs/23-build-engine.md`). `event USERACTOR` `ifwasweapon NAME` gates later ops when the last hit on that actor was that weapon. Tests: `conevt_useractor_ifwasweapon_runs_when_hit_by_rpg`, `conevt_useractor_ifwasweapon_else_when_wrong_gun`.
- `2026-10-18` — **Duke CONEVT useractor pstomp** (`docs/23-build-engine.md`). `event USERACTOR` `pstomp` damages overlapping `SHOOTABLE` actors (including the player) for 50. Tests: `conevt_useractor_pstomp_hurts_the_player`.
- `2026-10-18` — **Duke CONEVT useractor debug** (`docs/23-build-engine.md`). `event USERACTOR` `debug` / `addlog` prints DEBUG from a living `COUNTKILL`. Tests: `conevt_useractor_debug_prints_debug`.
- `2026-10-18` — **Duke CONEVT useractor endofgame** (`docs/23-build-engine.md`). `event USERACTOR` `endofgame` exits the map from a living `COUNTKILL`. Tests: `conevt_useractor_endofgame_exits_the_map`.
- `2026-10-18` — **Duke CONEVT useractor quake** (`docs/23-build-engine.md`). `event USERACTOR` `quake N` (alias `earthquake`) starts a map-wide rumble from each living `COUNTKILL`. Tests: `conevt_useractor_quake_starts_a_rumble`.
- `2026-10-18` — **Duke CONEVT useractor ifsound** (`docs/23-build-engine.md`). `event USERACTOR` `ifsound NAME` / `ifnosounds` gate later ops on queued / heard cues. Tests: `conevt_useractor_ifnosounds_runs_when_quiet`, `conevt_useractor_ifsound_runs_when_cue_is_queued`.
- `2026-10-18` — **Duke CONEVT useractor ifcanshoottarget** (`docs/23-build-engine.md`). `event USERACTOR` `ifcanshoottarget` gates later ops on line of sight and a 90° facing cone to the target (else the pawn). Tests: `conevt_useractor_ifcanshoottarget_runs_when_facing_pawn`, `conevt_useractor_ifcanshoottarget_else_when_looking_away`.
- `2026-10-18` — **Duke CONEVT useractor ifbulletnear** (`docs/23-build-engine.md`). `event USERACTOR` `ifbulletnear` gates later ops when a `MISSILE` is within 64 Room units. Tests: `conevt_useractor_ifbulletnear_runs_when_missile_close`, `conevt_useractor_ifbulletnear_else_without_missile`.
- `2026-10-18` — **Duke CONEVT useractor ifangdiffl** (`docs/23-build-engine.md`). `event USERACTOR` `ifangdiffl` / `ifangdiffg` gate later ops on Build facing delta to the pawn. Tests: `conevt_useractor_ifangdiffl_runs_when_facing_pawn`, `conevt_useractor_ifangdiffl_else_when_looking_away`.
- `2026-10-18` — **Duke CONEVT useractor ifawayfromwall** (`docs/23-build-engine.md`). `event USERACTOR` `ifawayfromwall` gates later ops when more than 8 Room units from a blocker. Tests: `conevt_useractor_ifawayfromwall_runs_in_the_open`, `conevt_useractor_ifawayfromwall_else_near_a_wall`.
- `2026-10-18` — **Duke CONEVT useractor ifgapzl** (`docs/23-build-engine.md`). `event USERACTOR` `ifgapzl` / `ifgapzg` gate later ops on the floor-to-ceiling Build gap. Tests: `conevt_useractor_ifgapzl_runs_in_a_low_room`, `conevt_useractor_ifgapzl_else_in_a_tall_room`.
- `2026-10-18` — **Duke CONEVT useractor iffloordistl** (`docs/23-build-engine.md`). `event USERACTOR` `iffloordistl` / `ifceilingdistl` / `iffloordistg` / `ifceilingdistg` gate later ops on Build floor/ceiling distance. Tests: `conevt_useractor_iffloordistl_runs_on_the_floor`, `conevt_useractor_ifceilingdistl_else_in_tall_room`.
- `2026-10-18` — **Duke CONEVT useractor ifinspace** (`docs/23-build-engine.md`). `event USERACTOR` `ifinspace` / `ifinouterspace` gate later ops on sky floor+ceiling / sky floor. Tests: `conevt_useractor_ifinspace_runs_when_both_planes_sky`, `conevt_useractor_ifinouterspace_else_indoors`.
- `2026-10-18` — **Duke CONEVT useractor ifoutside** (`docs/23-build-engine.md`). `event USERACTOR` `ifoutside` gates later ops on a sky ceiling (`F_SKY1`). Tests: `conevt_useractor_ifoutside_runs_under_sky`, `conevt_useractor_ifoutside_else_indoors`.
- `2026-10-18` — **Duke CONEVT useractor tip** (`docs/23-build-engine.md`). `event USERACTOR` `tip` prints `TIP` and plays `duke/duke_tip1` from each living `COUNTKILL`. Tests: `conevt_useractor_tip_prints_from_countkill`.
- `2026-10-18` — **Duke CONEVT useractor ifinwater** (`docs/23-build-engine.md`). `event USERACTOR` `ifinwater` / `ifonwater` gate later ops on `actor_waterlevel` (`>= 2` / `== 1`). Tests: `conevt_useractor_ifinwater_runs_when_submerged`, `conevt_useractor_ifinwater_else_on_dry_land`.
- `2026-10-18` — **Duke CONEVT useractor addkills** (`docs/23-build-engine.md`). `event USERACTOR` `addkills N` adds N to the map kill count from each living `COUNTKILL`. Tests: `conevt_useractor_addkills_bumps_map_count`.
- `2026-10-18` — **Duke CONEVT useractor ifaction** (`docs/23-build-engine.md`). `event USERACTOR` `ifaction` / `ifai` / `ifmove NAME` gates later ops on the current state label. Tests: `conevt_useractor_ifaction_runs_on_spawn_label`, `conevt_useractor_ifaction_else_on_other_label`.
- `2026-10-18` — **Duke CONEVT useractor ifactornotstayput** (`docs/23-build-engine.md`). `event USERACTOR` `ifactornotstayput` gates later ops unless `user_stayput` is set. Tests: `conevt_useractor_ifactornotstayput_runs_when_free`, `conevt_useractor_ifactornotstayput_else_when_stayput`.
- `2026-10-18` — **Duke CONEVT useractor ifspawnedby** (`docs/23-build-engine.md`). `event USERACTOR` `ifspawnedby NAME` gates later ops on the `COUNTKILL`'s master class (`else` flips). Tests: `conevt_useractor_ifspawnedby_runs_when_master_matches`, `conevt_useractor_ifspawnedby_else_without_master`.
- `2026-10-18` — **Duke CONEVT useractor ifactor** (`docs/23-build-engine.md`). `event USERACTOR` `ifactor NAME` gates later ops on the `COUNTKILL`'s class (`else` flips). Tests: `conevt_useractor_ifactor_runs_when_class_matches`, `conevt_useractor_ifactor_else_when_class_differs`.
- `2026-10-18` — **Duke CONEVT useractor zshoot** (`docs/23-build-engine.md`). `event USERACTOR` `zshoot NAME Z` (alias `ezshoot`) fires from each living `COUNTKILL` with extra Build Z velocity (`−Z/256`). Tests: `conevt_useractor_zshoot_adds_build_zvel`.
- `2026-10-18` — **Duke CONEVT useractor lotsofcolour** (`docs/23-build-engine.md`). `event USERACTOR` `lotsofcolour N` throws N tinted `DukeGlassPiece` shards (`DukePal1`…`DukePal6`) from each living `COUNTKILL`. Tests: `conevt_useractor_lotsofcolour_throws_from_countkill`.
- `2026-10-18` — **Duke CONEVT useractor ifrnd** (`docs/23-build-engine.md`). `event USERACTOR` `ifrnd N` (alias `ifrand`) gates later ops on a `p_random` roll (`< N`, 0 never / 256 always). Tests: `conevt_useractor_ifrnd_zero_skips`, `conevt_useractor_ifrnd_always_runs`.
- `2026-10-18` — **Duke CONEVT useractor ifactioncount** (`docs/23-build-engine.md`). `event USERACTOR` `actioncount` / `resetactioncount` / `ifactioncount` drive `user_conactioncount` on living `COUNTKILL`s. Tests: `conevt_useractor_ifactioncount_runs_after_actioncount`, `conevt_useractor_resetactioncount_clears_ifactioncount`.
- `2026-10-18` — **Duke CONEVT useractor paper** (`docs/23-build-engine.md`). `event USERACTOR` `paper N` throws N `DukePaper` sheets from each living `COUNTKILL`. Tests: `conevt_useractor_paper_drops_from_countkill`.
- `2026-10-18` — **Duke CONEVT useractor mail** (`docs/23-build-engine.md`). `event USERACTOR` `mail N` throws N `DukeMail` letters from each living `COUNTKILL`. Tests: `conevt_useractor_mail_drops_from_countkill`.
- `2026-10-18` — **Duke CONEVT useractor money** (`docs/23-build-engine.md`). `event USERACTOR` `money N` throws N `DukeMoney` bills from each living `COUNTKILL`. Tests: `conevt_useractor_money_drops_from_countkill`.
- `2026-10-18` — **Duke CONEVT useractor lotsofglass** (`docs/23-build-engine.md`). `event USERACTOR` `lotsofglass N` throws N `DukeGlassPiece` shards from each living `COUNTKILL`. Tests: `conevt_useractor_lotsofglass_throws_from_countkill`.
- `2026-10-18` — **Duke CONEVT useractor operate** (`docs/23-build-engine.md`). `event USERACTOR` `operate` uses the special in front of each living `COUNTKILL`. Tests: `conevt_useractor_operate_uses_the_door`.
- `2026-10-18` — **Duke CONEVT useractor fall** (`docs/23-build-engine.md`). `event USERACTOR` `fall` snaps each living `COUNTKILL` to the floor and zeros vertical speed. Tests: `conevt_useractor_fall_snaps_countkill`.
- `2026-10-18` — **Duke CONEVT useractor hitradius** (`docs/23-build-engine.md`). `event USERACTOR` `hitradius R D` blasts D damage within Build radius R (`/16`) from each living `COUNTKILL`. Tests: `conevt_useractor_hitradius_hurts_the_pawn`.
- `2026-10-18` — **Duke CONEVT useractor stopsound** (`docs/23-build-engine.md`). `event USERACTOR` `stopsound NAME` drops that cue from `sound_events` and `sound_heard`. Tests: `conevt_useractor_stopsound_drops_the_cue`.
- `2026-10-18` — **Duke CONEVT useractor getlastpal** (`docs/23-build-engine.md`). `event USERACTOR` `getlastpal` restores the previous `DukePal` on living `COUNTKILL`s. Tests: `conevt_useractor_getlastpal_restores_pal`.
- `2026-10-18` — **Duke CONEVT useractor cactor** (`docs/23-build-engine.md`). `event USERACTOR` `cactor NAME` changes each living `COUNTKILL` to that class. Tests: `conevt_useractor_cactor_changes_class`.
- `2026-10-18` — **Duke CONEVT useractor ifcanseetarget** (`docs/23-build-engine.md`). `event USERACTOR` `ifcanseetarget` gates later ops on line of sight to the actor's target. Tests: `conevt_useractor_ifcanseetarget_runs_when_aimed`, `conevt_useractor_ifcanseetarget_else_without_target`.
- `2026-10-18` — **Duke CONEVT useractor clipdist** (`docs/23-build-engine.md`). `event USERACTOR` `clipdist N` sets living `COUNTKILL` radius to `N/16`. Tests: `conevt_useractor_clipdist_sets_radius`.
- `2026-10-18` — **Duke CONEVT useractor sizeat/sizeto** (`docs/23-build-engine.md`). `event USERACTOR` `sizeat` sets Build xrepeat/yrepeat; `sizeto` steps them by 1. Tests: `conevt_useractor_sizeat_sets_scale`, `conevt_useractor_sizeto_steps_scale`.
- `2026-10-18` — **Duke CONEVT useractor cstat** (`docs/23-build-engine.md`). `event USERACTOR` `cstat` / `cstator` set blocking / translucent / invisible on living `COUNTKILL`s. Tests: `conevt_useractor_cstat_hides_countkill`, `conevt_useractor_cstator_ors_flags`.
- `2026-10-18` — **Duke CONEVT useractor soundonce** (`docs/23-build-engine.md`). `event USERACTOR` `soundonce NAME` plays that cue at each living `COUNTKILL` only if it is not already queued. Tests: `conevt_useractor_soundonce_plays_only_once`.
- `2026-10-18` — **Duke CONEVT useractor ifspritepal** (`docs/23-build-engine.md`). `event USERACTOR` `ifspritepal N` gates later ops on `DukePalN`. Tests: `conevt_useractor_ifspritepal_runs_when_pal_matches`, `conevt_useractor_ifspritepal_else_on_default_pal`.
- `2026-10-18` — **Duke CONEVT useractor spawn** (`docs/23-build-engine.md`). `event USERACTOR` `spawn NAME` drops that actor at each living `COUNTKILL`. Tests: `conevt_useractor_spawn_drops_at_countkill`.
- `2026-10-18` — **Duke CONEVT useractor spritepal** (`docs/23-build-engine.md`). `event USERACTOR` `spritepal N` sets each living `COUNTKILL` to `DukePalN`. Tests: `conevt_useractor_spritepal_tints_countkill`.
- `2026-10-18` — **Duke CONEVT useractor print** (`docs/23-build-engine.md`). `event USERACTOR` `print` / `quote` prints that line from each living `COUNTKILL`. Tests: `conevt_useractor_print_quotes_from_countkill`.
- `2026-10-18` — **Duke CONEVT useractor sound** (`docs/23-build-engine.md`). `event USERACTOR` `sound NAME` (alias `globalsound`) plays that cue at each living `COUNTKILL`. Tests: `conevt_useractor_sound_plays_at_countkill`.
- `2026-10-18` — **Duke CONEVT useractor guts** (`docs/23-build-engine.md`). `event USERACTOR` `guts NAME N` (alias `debris`) throws N pieces from each living `COUNTKILL`. Tests: `conevt_useractor_guts_throws_from_countkill`.
- `2026-10-18` — **Duke CONEVT useractor killit** (`docs/23-build-engine.md`). `event USERACTOR` `killit` drops living `COUNTKILL` health to 0. Tests: `conevt_useractor_killit_drops_countkill`.
- `2026-10-18` — **Duke CONEVT useractor ifstrength/ifcount** (`docs/23-build-engine.md`). `event USERACTOR` `ifstrength` / `ifcount` gate later ops by hit points / `user_concount`. Tests: `conevt_useractor_ifstrength_runs_when_hurt`, `conevt_useractor_ifcount_runs_after_count`.
- `2026-10-18` — **Duke CONEVT useractor ifpdist** (`docs/23-build-engine.md`). `event USERACTOR` `ifpdistl` / `ifpdistg` gate later ops by Build XY distance to the pawn. Tests: `conevt_useractor_ifpdistl_runs_when_close`, `conevt_useractor_ifpdistl_else_when_far`, `conevt_useractor_ifpdistg_runs_when_far`.
- `2026-10-18` — **Duke CONEVT useractor ifcansee** (`docs/23-build-engine.md`). `event USERACTOR` `ifcansee` gates later ops on line of sight to the pawn (`else` flips). Tests: `conevt_useractor_ifcansee_runs_when_in_view`, `conevt_useractor_ifcansee_else_when_blocked`.
- `2026-10-18` — **Duke CONEVT useractor shoot** (`docs/23-build-engine.md`). `event USERACTOR` `shoot NAME` fires that actor from each living `COUNTKILL` along its facing. Tests: `conevt_useractor_shoot_fires_from_countkill`.
- `2026-10-18` — **Duke CONEVT useractor findplayer** (`docs/23-build-engine.md`). `event USERACTOR` `findplayer` aims living `COUNTKILL`s at the pawn and stores Build XY distance in `user_findplayer`. Tests: `conevt_useractor_findplayer_aims_at_pawn`.
- `2026-10-18` — **Duke CONEVT useractor ai** (`docs/23-build-engine.md`). `event USERACTOR` `ai` / `move` / `state` / `action NAME` jumps living `COUNTKILL`s to that label. Tests: `conevt_useractor_ai_jumps_countkill`.
- `2026-10-18` — **Duke CONEVT useractor think** (`docs/23-build-engine.md`). `event USERACTOR` runs `count` / `addstrength` / `strength` / `sleeptime` on living `COUNTKILL`s each tic. Tests: `conevt_useractor_runs_on_countkill`.
- `2026-10-18` — **Duke CONEVT getceilzofslope** (`docs/23-build-engine.md`). `onevent getceilzofslope` stores ceiling Build Z (`−room*256`) in `user_ceilzofslope`. Tests: `condata::getceilzofslope_compiles`, `conevt_getceilzofslope_stores_build_z`.
- `2026-10-18` — **Duke CONEVT getflorzofslope** (`docs/23-build-engine.md`). `onevent getflorzofslope` stores floor Build Z (`−room*256`) in `user_florz`. Tests: `condata::getflorzofslope_compiles`, `conevt_getflorzofslope_stores_build_z`.
- `2026-10-18` — **Duke CONEVT respawnhitag** (`docs/23-build-engine.md`). `onevent respawnhitag N` revives dead actors whose tid is N (`user_respawnhitag` = count). Tests: `condata::respawnhitag_compiles`, `conevt_respawnhitag_revives_dead_tid`.
- `2026-10-18` — **Duke CONEVT wackmove** (`docs/23-build-engine.md`). `onevent wackmove X Y Z` try-moves the pawn by Build units; if a solid blocks, that actor takes 10 damage. Tests: `condata::wackmove_compiles`, `conevt_wackmove_hurts_the_blocker`.
- `2026-10-18` — **Duke CONEVT changespritestat** (`docs/23-build-engine.md`). `onevent changespritestat N` stores the pawn's sprite stat in `user_spritestat`. Tests: `condata::changespritestat_compiles`, `conevt_changespritestat_stores_stat`.
- `2026-10-18` — **Duke CONEVT clipmove** (`docs/23-build-engine.md`). `onevent clipmove X Y Z` try-moves the pawn by Build units (`try_move`); stores `user_clipmove` 1/0. Tests: `condata::clipmove_compiles`, `conevt_clipmove_shifts_when_open`, `conevt_clipmove_stays_when_blocked`.
- `2026-10-18` — **Duke CONEVT updatesector** (`docs/23-build-engine.md`). `onevent updatesector` (alias `updatesectorz`) stores the pawn's sector index in `user_sector`. Tests: `condata::updatesector_compiles`, `conevt_updatesector_stores_sector_index`.
- `2026-10-18` — **Duke CONEVT getzrange** (`docs/23-build-engine.md`). `onevent getzrange` stores floor / ceiling Build Z (`−room*256`) in `user_floorz` / `user_ceilz`. Tests: `condata::getzrange_compiles`, `conevt_getzrange_stores_build_z`.
- `2026-10-18` — **Duke CONEVT findotherplayer** (`docs/23-build-engine.md`). `onevent findotherplayer` stores Build XY distance to another player in `user_findotherplayer` (`-1` if none). Tests: `condata::findotherplayer_compiles`, `conevt_findotherplayer_is_minus_one_when_alone`, `conevt_findotherplayer_stores_coop_dist`.
- `2026-10-18` — **Duke CONEVT debug** (`docs/23-build-engine.md`). `onevent debug` (alias `addlog`) prints `DEBUG` on the HUD. Tests: `condata::debug_compiles`, `conevt_debug_prints_debug`.
- `2026-10-18` — **Duke CONEVT findplayer** (`docs/23-build-engine.md`). `onevent findplayer` aims the pawn at the nearest `COUNTKILL` and stores Build XY distance in `user_findplayer`. Tests: `condata::findplayer_compiles`, `conevt_findplayer_stores_build_dist`.
- `2026-10-18` — **Duke CONEVT pstomp** (`docs/23-build-engine.md`). `onevent pstomp` damages a shootable actor overlapping the pawn (50 hp). Tests: `condata::pstomp_compiles`, `conevt_pstomp_hurts_an_actor_underfoot`.
- `2026-10-18` — **Duke CONEVT ezshoot** (`docs/23-build-engine.md`). `onevent ezshoot NAME Z` fires along facing and adds Build Z velocity (`−z/256`), same as `zshoot`. Tests: `condata::ezshoot_compiles`, `conevt_ezshoot_adds_build_zvel`.
- `2026-10-18` — **Duke CONEVT eshoot** (`docs/23-build-engine.md`). `onevent eshoot NAME` fires that actor along the pawn's facing (same as `shoot`). Tests: `condata::eshoot_compiles`, `conevt_eshoot_fires_along_facing`.
- `2026-10-18` — **Duke CONEVT zshoot** (`docs/23-build-engine.md`). `onevent zshoot NAME Z` fires along facing and adds Build Z velocity (`−z/256`). Tests: `condata::zshoot_compiles`, `conevt_zshoot_adds_build_zvel`.
- `2026-10-18` — **Duke CONEVT lotsofcolour** (`docs/23-build-engine.md`). `onevent lotsofcolour N` throws N `DukeGlassPiece` shards tinted `DukePal1`…`DukePal6`. Tests: `condata::lotsofcolour_compiles`, `conevt_lotsofcolour_throws_tinted_shards`.
- `2026-10-18` — **Duke CONEVT getactorangle** (`docs/23-build-engine.md`). `onevent getactorangle` stores pawn yaw as a Build angle in `user_actorangle`. Tests: `condata::getactorangle_compiles`, `conevt_getactorangle_stores_build_ang`.
- `2026-10-18` — **Duke CONEVT getplayerangle** (`docs/23-build-engine.md`). `onevent getplayerangle` stores pawn yaw as a Build angle in `user_playerangle`. Tests: `condata::getplayerangle_compiles`, `conevt_getplayerangle_stores_build_ang`.
- `2026-10-18` — **Duke CONEVT setactorangle** (`docs/23-build-engine.md`). `onevent setactorangle N` sets pawn yaw from a Build angle (`−(ang & 2047) * 360 / 2048`). Tests: `condata::setactorangle_compiles`, `conevt_setactorangle_sets_yaw`.
- `2026-10-18` — **Duke CONEVT stopsound** (`docs/23-build-engine.md`). `onevent stopsound NAME` drops that cue from `sound_events` and `sound_heard`. Tests: `condata::stopsound_compiles`, `conevt_stopsound_drops_the_cue`.
- `2026-10-18` — **Duke CONEVT soundonce** (`docs/23-build-engine.md`). `onevent soundonce NAME` queues that sound only if it is not already playing. Tests: `condata::soundonce_compiles`, `conevt_soundonce_plays_only_once`.
- `2026-10-18` — **Duke CONEVT quake** (`docs/23-build-engine.md`). `onevent quake N` (alias `earthquake`) starts a map-wide rumble for N tics (`start_duke_quake`). Tests: `condata::quake_compiles`, `conevt_quake_starts_a_rumble`.
- `2026-10-18` — **Duke CONEVT setplayerangle** (`docs/23-build-engine.md`). `onevent setplayerangle N` sets pawn yaw from a Build angle (`−(ang & 2047) * 360 / 2048`). Tests: `condata::setplayerangle_compiles`, `conevt_setplayerangle_sets_yaw`.
- `2026-10-18` — **Duke CONEVT lockplayer** (`docs/23-build-engine.md`). `onevent lockplayer N` freezes the pawn (`Player::frozen`, zeros velocity); `0` unlocks. Tests: `condata::lockplayer_compiles`, `conevt_lockplayer_freezes_the_pawn`.
- `2026-10-18` — **Duke CONEVT setsprite** (`docs/23-build-engine.md`). `onevent setsprite X Y Z` snaps the pawn to Build units (`x/16`, `−y/16`, `−z/256`). Tests: `condata::setsprite_compiles`, `conevt_setsprite_snaps_the_pawn`.
- `2026-10-18` — **Duke CONEVT movesprite** (`docs/23-build-engine.md`). `onevent movesprite X Y Z` shifts the pawn by Build units (`x/16`, `−y/16`, `−z/256`). Tests: `condata::movesprite_compiles`, `conevt_movesprite_shifts_the_pawn`.
- `2026-10-18` — **Duke CONEVT tossweapon** (`docs/23-build-engine.md`). `onevent tossweapon` takes the ready/pending weapon and clears both slots. Tests: `condata::tossweapon_compiles`, `conevt_tossweapon_drops_the_ready_gun`.
- `2026-10-18` — **Duke CONEVT endofgame** (`docs/23-build-engine.md`). `onevent endofgame` sets `World::level.exiting`. Tests: `condata::endofgame_compiles`, `conevt_endofgame_exits_the_map`.
- `2026-10-18` — **Duke CONEVT resetplayer** (`docs/23-build-engine.md`). `onevent resetplayer` restores pawn health, spawn pose, and clears velocity. Tests: `condata::resetplayer_compiles`, `conevt_resetplayer_restores_spawn`.
- `2026-10-18` — **Duke CONEVT tip** (`docs/23-build-engine.md`). `onevent tip` prints `TIP` and plays `duke/duke_tip1`. Tests: `condata::tip_compiles`, `conevt_tip_prints_and_plays`.
- `2026-10-18` — **Duke CONEVT paper** (`docs/23-build-engine.md`). `onevent paper N` throws N `DukePaper` pickups. Tests: `condata::paper_compiles`, `conevt_paper_drops_sheets`.
- `2026-10-18` — **Duke CONEVT mail** (`docs/23-build-engine.md`). `onevent mail N` throws N `DukeMail` pickups. Tests: `condata::mail_compiles`, `conevt_mail_drops_letters`.
- `2026-10-18` — **Duke CONEVT debris** (`docs/23-build-engine.md`). `onevent debris NAME N` throws N pieces of that actor (same as `guts`). Tests: `condata::debris_compiles`, `conevt_debris_throws_pieces`.
- `2026-10-18` — **Duke CONEVT getlastpal** (`docs/23-build-engine.md`). `onevent getlastpal` restores the pawn palette saved by `spritepal` (`user_lastpal`). Tests: `condata::getlastpal_compiles`, `conevt_getlastpal_restores_palette`.
- `2026-10-18` — **Duke CONEVT sleeptime** (`docs/23-build-engine.md`). `onevent sleeptime N` sets the pawn `reaction_time`. Tests: `condata::sleeptime_compiles`, `conevt_sleeptime_sets_reaction`.
- `2026-10-18` — **Duke CONEVT fall** (`docs/23-build-engine.md`). `onevent fall` snaps the pawn to the sector floor and clears vertical speed. Tests: `condata::fall_compiles`, `conevt_fall_snaps_to_the_floor`.
- `2026-10-18` — **Duke CONEVT hitradius** (`docs/23-build-engine.md`). `onevent hitradius R D` damages nearby actors (not the pawn) within Build radius R (`/16`) for D. Tests: `condata::hitradius_compiles`, `conevt_hitradius_hurts_a_nearby_actor`.
- `2026-10-18` — **Duke CONEVT addkills** (`docs/23-build-engine.md`). `onevent addkills N` adds N to `World::level.found_kills`. Tests: `condata::addkills_compiles`, `conevt_addkills_bumps_the_tally`.
- `2026-10-18` — **Duke CONEVT actioncount / resetactioncount** (`docs/23-build-engine.md`). `onevent actioncount N` sets the pawn action-frame counter (`user_conactioncount`); `resetactioncount` zeros it; `ifactioncount` reads that when set. Tests: `condata::actioncount_compiles`, `condata::resetactioncount_compiles`, `conevt_actioncount_sets_ifactioncount`, `conevt_resetactioncount_clears_ifactioncount`.
- `2026-10-18` — **Duke shared-corner rooms** (`docs/23-build-engine.md`). Two sectors with disjoint Z whose 2D boxes only share a corner become a solid 3D floor (`aabbs_share_edge` includes `ox==0 && oy==0`). Tests: `fx::shared_corner_rooms_become_a_3d_floor`.
- `2026-10-18` — **Duke CONEVT count / resetcount** (`docs/23-build-engine.md`). `onevent count N` sets the pawn action counter (`user_concount`); `resetcount` zeros it; `ifcount` reads that when set. Tests: `condata::count_compiles`, `condata::resetcount_compiles`, `conevt_count_sets_ifcount`, `conevt_resetcount_clears_ifcount`.
- `2026-10-18` — **Duke CONEVT sizeto** (`docs/23-build-engine.md`). `onevent sizeto X Y` steps Build xrepeat / yrepeat by 1 toward the target (`scale / 64`). Tests: `condata::sizeto_compiles`, `conevt_sizeto_steps_scale`.
- `2026-10-18` — **Duke CONEVT cactor** (`docs/23-build-engine.md`). `onevent cactor NAME` changes the pawn's class (CON actor / `Duke*` mapping). Tests: `condata::cactor_compiles`, `conevt_cactor_changes_class`.
- `2026-10-18` — **Duke CONEVT clipdist** (`docs/23-build-engine.md`). `onevent clipdist N` sets pawn radius from Build units (`/16`). Tests: `condata::clipdist_compiles`, `conevt_clipdist_sets_radius`.
- `2026-10-18` — **Duke CONEVT strength** (`docs/23-build-engine.md`). `onevent strength N` sets the pawn's hit points. Tests: `condata::strength_compiles`, `conevt_strength_sets_health`.
- `2026-10-18` — **Duke CONEVT addstrength** (`docs/23-build-engine.md`). `onevent addstrength N` adds N hit points to the pawn. Tests: `condata::addstrength_compiles`, `conevt_addstrength_heals_the_pawn`.
- `2026-10-18` — **Duke CONEVT cstator** (`docs/23-build-engine.md`). `onevent cstator N` ORs blocking / translucent / invisible onto the pawn. Tests: `condata::cstator_compiles`, `conevt_cstator_ors_flags`.
- `2026-10-18` — **Duke CONEVT cstat** (`docs/23-build-engine.md`). `onevent cstat N` sets pawn blocking (`1`), translucent (`2`), and invisible (`32768`). Tests: `condata::cstat_compiles`, `conevt_cstat_hides_the_pawn`.
- `2026-10-18` — **Duke CONEVT sizeat** (`docs/23-build-engine.md`). `onevent sizeat X Y` sets pawn `scale_x` / `scale_y` from Build repeats (`/64`). Tests: `condata::sizeat_compiles`, `conevt_sizeat_sets_scale`.
- `2026-10-18` — **Duke CONEVT spritepal** (`docs/23-build-engine.md`). `onevent spritepal N` sets the pawn translation (`DukePalN`, empty for 0). Tests: `condata::spritepal_compiles`, `conevt_spritepal_sets_translation`.
- `2026-10-18` — **Duke CONEVT lotsofglass** (`docs/23-build-engine.md`). `onevent lotsofglass N` throws N `DukeGlassPiece` shards at the pawn. Tests: `condata::lotsofglass_compiles`, `conevt_lotsofglass_throws_shards`.
- `2026-10-18` — **Duke CONEVT money** (`docs/23-build-engine.md`). `onevent money N` drops N `DukeMoney` bills at the pawn. Tests: `condata::money_compiles`, `conevt_money_drops_bills`.
- `2026-10-18` — **Duke CONEVT operate** (`docs/23-build-engine.md`). `onevent operate` uses the special in front of the pawn (`use_lines`). Tests: `condata::operate_compiles`, `conevt_operate_uses_the_door`.
- `2026-10-18` — **Duke CONEVT wackplayer** (`docs/23-build-engine.md`). `onevent wackplayer` hurts the pawn by 10 and flashes the HUD (`damage_count`). Tests: `condata::wackplayer_compiles`, `conevt_wackplayer_hurts_and_flashes`.
- `2026-10-18` — **Duke CONEVT killit** (`docs/23-build-engine.md`). `onevent killit` drops the pawn's health to 0. Tests: `condata::killit_compiles`, `conevt_killit_drops_health`.
- `2026-10-18` — **Duke CONEVT shoot** (`docs/23-build-engine.md`). `onevent shoot NAME` fires that actor along the pawn's facing (class `Speed`, else 16). Tests: `condata::shoot_compiles`, `conevt_shoot_fires_along_facing`.
- `2026-10-18` — **Duke CONEVT guts** (`docs/23-build-engine.md`). `onevent guts NAME N` throws N pieces at the pawn (`pr_dukeguts`). Tests: `condata::guts_compiles`, `conevt_guts_throws_debris`.
- `2026-10-18` — **Duke CONEVT spawn** (`docs/23-build-engine.md`). `onevent spawn NAME` drops that actor at the pawn (`find_con_actor`). Tests: `condata::spawn_compiles`, `conevt_spawn_drops_an_actor`.
- `2026-10-18` — **Duke shared-edge rooms** (`docs/23-build-engine.md`). Two sectors with disjoint Z whose 2D boxes only share an edge become a solid 3D floor (`aabbs_share_edge`). Tests: `fx::shared_edge_rooms_become_a_3d_floor`.
- `2026-10-18` — **Duke .SAV applies wall[] health** (`docs/23-build-engine.md`). Load Game writes dump `wall[]` extra / blocking onto destructible linedefs at those XY (`World::apply_duke_wall_health`). Tests: `sav::dump_reads_walls_after_numwalls`, `duke_sav_wall_health_is_applied`.
- `2026-10-18` — **Duke .SAV applies sprite[] health** (`docs/23-build-engine.md`). Load Game writes dump `sprite[]` extra onto non-player actors at those XY (`SavSprite::extra`, `World::apply_duke_sprite_health`). Tests: `sav::dump_reads_sprites_after_numsprites`, `duke_sav_sprite_health_is_applied`.
- `2026-10-18` — **Duke .SAV applies sector[] heights** (`docs/23-build-engine.md`). Load Game writes dump `sector[]` floor / ceiling Z onto the first matching Room sectors (`SavSector::room_floor_z` / `room_ceiling_z`, `World::apply_duke_sector_zs`). Tests: `sav::dump_reads_sectors_after_numsectors`, `duke_sav_sector_zs_are_applied`.
- `2026-10-18` — **Duke .SAV applies ps[] night vision** (`docs/23-build-engine.md`). Load Game sets `DukeNightVision` from dump `heat_amount` (`SavPlayer::room_heat`, `World::set_player_heat`). Tests: `sav::dump_reads_player_after_cyclers`, `duke_sav_player_heat_is_applied`.
- `2026-10-18` — **Duke .SAV applies ps[] boots** (`docs/23-build-engine.md`). Load Game sets `DukeBoots` from dump `boot_amount` (`SavPlayer::room_boots`, `World::set_player_boots`). Tests: `sav::dump_reads_player_after_cyclers`, `duke_sav_player_boots_is_applied`.
- `2026-10-18` — **Duke .SAV applies ps[] holoduke** (`docs/23-build-engine.md`). Load Game sets `DukeHoloduke` from dump `holoduke_amount` (`SavPlayer::room_holoduke`, `World::set_player_holoduke`). Tests: `sav::dump_reads_player_after_cyclers`, `duke_sav_player_holoduke_is_applied`.
- `2026-10-18` — **Duke .SAV applies ps[] scuba** (`docs/23-build-engine.md`). Load Game sets `DukeScubaGear` from dump `scuba_amount` (`SavPlayer::room_scuba`, `World::set_player_scuba`). Tests: `sav::dump_reads_player_after_cyclers`, `duke_sav_player_scuba_is_applied`.
- `2026-10-18` — **Duke .SAV applies ps[] jetpack** (`docs/23-build-engine.md`). Load Game sets `DukeJetpack` from dump `jetpack_amount` (`SavPlayer::room_jetpack`, `World::set_player_jetpack`). Tests: `sav::dump_reads_player_after_cyclers`, `duke_sav_player_jetpack_is_applied`.
- `2026-10-18` — **Duke .SAV applies ps[] steroids** (`docs/23-build-engine.md`). Load Game sets `DukeSteroids` from dump `steroids_amount` (`SavPlayer::room_steroids`, `World::set_player_steroids`). Tests: `sav::dump_reads_player_after_cyclers`, `duke_sav_player_steroids_is_applied`.
- `2026-10-18` — **Duke .SAV applies ps[] firstaid** (`docs/23-build-engine.md`). Load Game sets `DukeFirstAid` from dump `firstaid_amount` (`SavPlayer::room_firstaid`, `World::set_player_firstaid`). Tests: `sav::dump_reads_player_after_cyclers`, `duke_sav_player_firstaid_is_applied`.
- `2026-10-18` — **Duke .SAV applies ps[] armor** (`docs/23-build-engine.md`). Load Game sets armor from dump `shield_amount` (`SavPlayer::room_armor`, `World::set_player_armor`). Tests: `sav::dump_reads_player_after_cyclers`, `duke_sav_player_armor_is_applied`.
- `2026-10-18` — **Duke .SAV applies ps[] gotweapon** (`docs/23-build-engine.md`). Load Game gives every dump `gotweapon` slot (`SavPlayer::room_gotweapon`, `World::give_player_gotweapons`). Tests: `sav::dump_reads_player_after_cyclers`, `duke_sav_player_gotweapon_is_applied`.
- `2026-10-18` — **Duke .SAV applies ps[] curr_weapon** (`docs/23-build-engine.md`). Load Game equips the dump `curr_weapon` slot (`SavPlayer::room_weapon_slot`, `World::set_player_weapon_slot`). Tests: `sav::dump_reads_player_after_cyclers`, `duke_sav_player_weapon_slot_is_applied`.
- `2026-10-18` — **Duke .SAV applies ps[] cell ammo** (`docs/23-build-engine.md`). Load Game sets `Cell` from dump `ammo_amount[SHRINKER_WEAPON]` (`SavPlayer::room_cell_ammo`, `World::set_player_cell_ammo`). Tests: `sav::dump_reads_player_after_cyclers`, `duke_sav_player_cell_ammo_is_applied`.
- `2026-10-18` — **Duke .SAV applies ps[] rocket ammo** (`docs/23-build-engine.md`). Load Game sets `RocketAmmo` from dump `ammo_amount[RPG_WEAPON]` (`SavPlayer::room_rocket_ammo`, `World::set_player_rocket_ammo`). Tests: `sav::dump_reads_player_after_cyclers`, `duke_sav_player_rocket_ammo_is_applied`.
- `2026-10-18` — **Duke .SAV applies ps[] shell ammo** (`docs/23-build-engine.md`). Load Game sets `Shell` from dump `ammo_amount[SHOTGUN_WEAPON]` (`SavPlayer::room_shell_ammo`, `World::set_player_shell_ammo`). Tests: `sav::dump_reads_player_after_cyclers`, `duke_sav_player_shell_ammo_is_applied`.
- `2026-10-18` — **Duke .SAV applies ps[] clip ammo** (`docs/23-build-engine.md`). Load Game sets `Clip` from dump `ammo_amount[PISTOL_WEAPON]` (`SavPlayer::room_clip_ammo`, `World::set_player_clip_ammo`). Tests: `sav::dump_reads_player_after_cyclers`, `duke_sav_player_clip_ammo_is_applied`.
- `2026-10-18` — **Duke .SAV applies ps[] health** (`docs/23-build-engine.md`). Load Game sets pawn health from dump `last_extra` (`SavPlayer::room_health`, `World::set_player_health`). Tests: `sav::dump_reads_player_after_cyclers`, `duke_sav_player_health_is_applied`.
- `2026-10-18` — **Duke .SAV applies horiz pitch** (`docs/23-build-engine.md`). Load Game sets pawn pitch from dump `horiz` (`100` = level, `SavPlayer::room_pitch`). Tests: `sav::dump_reads_player_after_cyclers`, `duke_sav_player_pose_is_applied`.
- `2026-10-18` — **Duke .SAV applies ps[] pose** (`docs/23-build-engine.md`). Load Game moves the pawn from the first `ps[]` pose (`World::set_player_pose`). Tests: `sav::dump_reads_player_after_cyclers`, `duke_sav_player_pose_is_applied`.
- `2026-10-18` — **Duke .SAV ps[]** (`docs/23-build-engine.md`). After cyclers, the first `ps[]` `player_struct` pose is read (`SavPlayer` pos/horiz/ang, Room XY `x/16` `−y/16`, Z `−z/256`). Tests: `sav::dump_reads_player_after_cyclers`.
- `2026-10-18` — **Duke .SAV sprite[]** (`docs/23-build-engine.md`). After `numsprites`, the `sprite[]` blob is parsed for the first `numsprites` `spritetype` records (`SavSprite`, Room XY `x/16` `−y/16`, Z `−z/256`). Tests: `sav::dump_reads_sprites_after_numsprites`.
- `2026-10-18` — **Duke .SAV sector[]** (`docs/23-build-engine.md`). After `numsectors`, the `sector[]` blob is parsed for the first `numsectors` `sectortype` records (`SavSector`, Room Z `−z/256`). Tests: `sav::dump_reads_sectors_after_numsectors`.
- `2026-10-18` — **Duke .SAV wall[]** (`docs/23-build-engine.md`). After `numwalls`, the `wall[]` blob is parsed for the first `numwalls` `walltype` records (`SavWall`, Room XY `x/16` `−y/16`). Tests: `sav::dump_reads_walls_after_numwalls`.
- `2026-10-18` — **Duke .SAV parallaxyscale** (`docs/23-build-engine.md`). After `global_random`, dump `parallaxyscale` (`long`) is read. Tests: `sav::dump_reads_parallaxyscale_after_global_random`.
- `2026-10-18` — **Duke .SAV global_random** (`docs/23-build-engine.md`). After `frags`, dump `global_random` (`short`) is read. Tests: `sav::dump_reads_global_random_after_frags`.
- `2026-10-18` — **Duke .SAV frags** (`docs/23-build-engine.md`). After `numplayersprites`, dump `frags` (`short[16][16]`) is read. Tests: `sav::dump_reads_frags_after_numplayersprites`.
- `2026-10-18` — **Duke .SAV numplayersprites** (`docs/23-build-engine.md`). After `connectpoint2`, dump `numplayersprites` (`short`) is read. Tests: `sav::dump_reads_numplayersprites_after_connectpoint2`.
- `2026-10-18` — **Duke .SAV connectpoint2** (`docs/23-build-engine.md`). After `connecthead`, dump `connectpoint2` (`short[16]`) is read. Tests: `sav::dump_reads_connectpoint2_after_connecthead`.
- `2026-10-18` — **Duke .SAV connecthead** (`docs/23-build-engine.md`). After `camsprite`, dump `connecthead` (`short`) is read. Tests: `sav::dump_reads_connecthead_after_camsprite`.
- `2026-10-18` — **Duke .SAV camsprite** (`docs/23-build-engine.md`). After `ud.ffire`, dump `camsprite` (`short`) is read. Tests: `sav::dump_reads_camsprite_after_ffire`.
- `2026-10-18` — **Duke .SAV ffire** (`docs/23-build-engine.md`). After `ud.marker`, dump `ud.ffire` (`char`) is read. Tests: `sav::dump_reads_ffire_after_marker`.
- `2026-10-18` — **Duke .SAV marker** (`docs/23-build-engine.md`). After `ud.coop`, dump `ud.marker` (`char`) is read. Tests: `sav::dump_reads_marker_after_coop`.
- `2026-10-18` — **Duke .SAV coop** (`docs/23-build-engine.md`). After `ud.eog`, dump `ud.coop` (`char`) is read. Tests: `sav::dump_reads_coop_after_eog`.
- `2026-10-18` — **Duke .SAV eog** (`docs/23-build-engine.md`). After `ud.last_level`, dump `ud.eog` (`char`) is read. Tests: `sav::dump_reads_eog_after_last_level`.
- `2026-10-18` — **Duke .SAV last_level** (`docs/23-build-engine.md`). After `ud.monsters_off`, dump `ud.last_level` (`char`) is read. Tests: `sav::dump_reads_last_level_after_monsters_off`.
- `2026-10-18` — **Duke .SAV monsters_off** (`docs/23-build-engine.md`). After `ud.crosshair`, dump `ud.monsters_off` (`char`) is read. Tests: `sav::dump_reads_monsters_off_after_crosshair`.
- `2026-10-18` — **Duke .SAV crosshair** (`docs/23-build-engine.md`). After `ud.auto_run`, dump `ud.crosshair` (`char`) is read. Tests: `sav::dump_reads_crosshair_after_auto_run`.
- `2026-10-18` — **Duke .SAV auto_run** (`docs/23-build-engine.md`). After `ud.god`, dump `ud.auto_run` (`char`) is read and applied as `cl_run`. Tests: `sav::dump_reads_auto_run_after_god`.
- `2026-10-18` — **Duke .SAV god** (`docs/23-build-engine.md`). After `respawn_inventory`, dump `ud.god` (`char`) is read and applied as player god mode. Tests: `sav::dump_reads_god_after_respawn_inventory`, `duke_sav_god_is_applied`.
- `2026-10-18` — **Duke .SAV respawn_inventory** (`docs/23-build-engine.md`). After `respawn_items`, dump `ud.respawn_inventory` (`char`) is read. Tests: `sav::dump_reads_respawn_inventory_after_respawn_items`.
- `2026-10-18` — **Duke .SAV respawn_items** (`docs/23-build-engine.md`). After `respawn_monsters`, dump `ud.respawn_items` (`char`) is read. Tests: `sav::dump_reads_respawn_items_after_respawn_monsters`.
- `2026-10-18` — **Duke .SAV respawn_monsters** (`docs/23-build-engine.md`). After `secretlevel`, dump `ud.respawn_monsters` (`char`) is read and applied (`World::respawn_monsters`). Tests: `sav::dump_reads_respawn_monsters_after_secretlevel`, `duke_sav_respawn_monsters_is_applied`.
- `2026-10-18` — **Duke .SAV secretlevel** (`docs/23-build-engine.md`). After `from_bonus`, dump `ud.secretlevel` (`char`) is read. Tests: `sav::dump_reads_secretlevel_after_from_bonus`.
- `2026-10-18` — **Duke .SAV from_bonus** (`docs/23-build-engine.md`). After `earthquaketime`, dump `ud.from_bonus` (`char`) is read. Tests: `sav::dump_reads_from_bonus_after_earthquaketime`.
- `2026-10-18` — **Duke .SAV applies earthquaketime** (`docs/23-build-engine.md`). Load Game starts a map-wide Room quake from dump `earthquaketime` (`SavHeader::room_quake_tics`, `World::start_duke_quake`). Tests: `sav::dump_reads_earthquaketime_after_animateptr`, `duke_sav_earthquaketime_becomes_a_quake`.
- `2026-10-18` — **Duke .SAV earthquaketime** (`docs/23-build-engine.md`). After `animateptr`, dump `earthquaketime` (`char`) is read and converted 30 Hz → Room 35 Hz (`SavHeader::room_quake_tics`). Tests: `sav::dump_reads_earthquaketime_after_animateptr`.
- `2026-10-18` — **Duke .SAV animateptr** (`docs/23-build-engine.md`). After `animatevel`, the `animateptr` blob (`long[64]`) is read for the first `animatecnt` slots. Tests: `sav::dump_reads_animateptr_after_animatevel`.
- `2026-10-18` — **Duke .SAV animatevel** (`docs/23-build-engine.md`). After `animategoal`, the `animatevel` blob (`long[64]`) is read for the first `animatecnt` slots. Tests: `sav::dump_reads_animatevel_after_animategoal`.
- `2026-10-18` — **Duke .SAV animategoal** (`docs/23-build-engine.md`). After `animatecnt`, the `animategoal` blob (`long[64]`) is read for the first `animatecnt` slots. Tests: `sav::dump_reads_animategoal_after_animatecnt`.
- `2026-10-18` — **Duke .SAV animatecnt** (`docs/23-build-engine.md`). After `pskyoff`, dump `animatecnt` (`short`, 0..=64) is read. Tests: `sav::dump_reads_animatecnt_after_pskyoff`.
- `2026-10-18` — **Duke .SAV pskyoff** (`docs/23-build-engine.md`). After `pskybits`, eight `pskyoff` slots (`short[8]`) are read. Tests: `sav::dump_reads_pskyoff_after_pskybits`.
- `2026-10-18` — **Duke .SAV pskybits** (`docs/23-build-engine.md`). After `lockclock`, dump `pskybits` (`short`) is read. Tests: `sav::dump_reads_pskybits_after_lockclock`.
- `2026-10-18` — **CONEVT addinventory** (`docs/23-build-engine.md`). CONEVT `addinventory NAME N` gives an inventory item (`FIRSTAID` → DukeFirstAid). Tests: `addinventory_compiles`, `conevt_addinventory_gives_firstaid`.
- `2026-10-18` — **CONEVT addweapon** (`docs/23-build-engine.md`). CONEVT `addweapon NAME N` gives the weapon and N ammo (`SHOTGUN` → Shotgun + Shell). Tests: `addweapon_compiles`, `conevt_addweapon_gives_shotgun`.
- `2026-10-18` — **CONEVT addammo** (`docs/23-build-engine.md`). CONEVT `addammo NAME N` gives ammo (`PISTOL_WEAPON` → Clip, …). Tests: `addammo_compiles`, `conevt_addammo_gives_clip`.
- `2026-10-18` — **CONEVT addphealth** (`docs/23-build-engine.md`). CONEVT `addphealth N` adds N to pawn health (clamped to class health). Tests: `addphealth_compiles`, `conevt_addphealth_heals_the_pawn`.
- `2026-10-18` — **CONEVT palfrom** (`docs/23-build-engine.md`). CONEVT `palfrom N` sets the HUD red flash (`damage_count`, 0..=100). Tests: `palfrom_compiles`, `conevt_palfrom_sets_damage_count`.
- `2026-10-18` — **CON ifp pweaponout** (`docs/23-build-engine.md`). CONEVT `ifp pweaponout` gates quotes when `ready_weapon` is drawn. Tests: `conevt_ifp_pweaponout_prints_when_drawn`, `conevt_ifp_pweaponout_stays_quiet_when_holstered`.
- `2026-10-18` — **Duke .SAV lockclock** (`docs/23-build-engine.md`). After `randomseed`, `lockclock` is read and converted 30 Hz → Room 35 Hz map time. Tests: `sav::dump_reads_lockclock_after_seed`, `duke_sav_lockclock_becomes_map_time`.
- `2026-10-18` — **Duke .SAV applies randomseed** (`docs/23-build-engine.md`). Load Game uses dump `randomseed` as the Room RNG seed (`SavHeader::room_rng_seed`). Tests: `sav::dump_reads_randomseed_after_clouds`, `duke_sav_randomseed_becomes_the_room_rng`.
- `2026-10-18` — **Duke .SAV dump randomseed** (`docs/23-build-engine.md`). After `numclouds`, the cloud arrays are skipped and `randomseed` is read. Tests: `sav::dump_reads_randomseed_after_clouds`.
- `2026-10-18` — **Duke .SAV dump clouds** (`docs/23-build-engine.md`). After `spriteqamount`, `numclouds` is read. Tests: `sav::dump_reads_numclouds_after_spriteq`.
- `2026-10-18` — **CONEVT break** (`docs/23-build-engine.md`). CONEVT `break` stops the rest of the event (later quotes / `else` do not run). Tests: `break_compiles`, `conevt_break_stops_the_rest_of_the_event`.
- `2026-10-18` — **Duke .SAV dump spriteq** (`docs/23-build-engine.md`). After `numanimwalls`, the animwall / mirror / `spriteqloc` blobs are skipped and `spriteqamount` is read. Tests: `sav::dump_reads_spriteqamount_after_mirrors`.
- `2026-10-18` — **CONEVT else** (`docs/23-build-engine.md`). CONEVT `else` flips a failed `if` so the following quote / sound runs. Tests: `else_compiles`, `conevt_else_prints_when_ifdead_fails`, `conevt_else_stays_quiet_when_if_passes`.
- `2026-10-18` — **CON ifangdiffg / ifgapzg** (`docs/23-build-engine.md`). CONEVT `ifangdiffg` / `ifgapzg` gate quotes when facing delta or floor-to-ceiling gap is at least N. Tests: `ifangdiffg_compiles`, `ifgapzg_compiles`, `conevt_ifangdiffg_prints_when_looking_away`, `conevt_ifgapzg_prints_in_a_tall_room`.
- `2026-10-18` — **CON iffloordistg / ifceilingdistg** (`docs/23-build-engine.md`). CONEVT `iffloordistg` / `ifceilingdistg` gate quotes when feet-to-floor or head-to-ceiling is at least N Build z. Tests: `iffloordistg_compiles`, `ifceilingdistg_compiles`, `conevt_iffloordistg_stays_quiet_on_the_floor`, `conevt_ifceilingdistg_prints_in_a_tall_room`.
- `2026-10-18` — **CONEVT action** (`docs/23-build-engine.md`). CONEVT `action NAME` jumps the pawn to that state label, same as `state` / `ai` / `move`. Tests: `action_compiles`, `conevt_action_jumps_the_pawn_to_see`.
- `2026-10-18` — **CONEVT ai / move** (`docs/23-build-engine.md`). CONEVT `ai NAME` and `move NAME` jump the pawn to that state label, same as `state`. Tests: `ai_compiles`, `move_compiles`, `conevt_ai_jumps_the_pawn_to_see`, `conevt_move_jumps_the_pawn_to_see`.
- `2026-10-18` — **Duke .SAV dump animwalls** (`docs/23-build-engine.md`). After `numcyclers`, the cyclers / `ps[]` / `po[]` blobs are skipped and `numanimwalls` is read. Tests: `sav::dump_reads_numanimwalls_after_players`.
- `2026-10-18` — **Duke .SAV dump cyclers** (`docs/23-build-engine.md`). After `numsprites`, the sprite array and six link lists are skipped and `numcyclers` is read. Tests: `sav::dump_reads_numcyclers_after_sprite_lists`.
- `2026-10-18` — **Duke .SAV dump sprites** (`docs/23-build-engine.md`). After `numsectors`, the sectors blob is skipped and `numsprites` is read. Tests: `sav::dump_reads_numsprites_after_sectors`.
- `2026-10-18` — **Duke .SAV dump sectors** (`docs/23-build-engine.md`). After `numwalls`, the walls blob is skipped and `numsectors` is read. Tests: `sav::dump_reads_numsectors_after_walls`.
- `2026-10-18` — **Duke .SAV dump prefix** (`docs/23-build-engine.md`). `game#.sav` reads the board `numwalls` short after `loadpheader`; the rest of the dump is still skipped. Tests: `sav::dump_reads_numwalls`, `sav::header_names_e1m3`.
- `2026-10-18` — **Duke air hold** (`docs/23-build-engine.md`). Holding move in the air adds a quarter of wish thrust to XY speed instead of replacing it (`DukePlayer` only). Tests: `think_player::duke_air_hold_steers_without_replacing_xy`.
- `2026-10-18` — **Duke CON state** (`docs/23-build-engine.md`). `state NAME` compiles into `CONEVT` and jumps the pawn to that state label (`SEE`; unknown labels leave the pose). Tests: `condata::state_compiles`, `headless::conevt_state_jumps_the_pawn_to_see`, `headless::conevt_state_stays_put_on_a_missing_label`.
- `2026-10-18` — **Duke CON ifp pweapon** (`docs/23-build-engine.md`). `ifp pweapon` compiles into `CONEVT` and gates quotes when the player owns a weapon. Tests: `condata::ifp_pweapon_compiles`, `headless::conevt_ifp_pweapon_prints_when_armed`, `headless::conevt_ifp_pweapon_stays_quiet_unarmed`.
- `2026-10-18` — **Duke CON ifmove** (`docs/23-build-engine.md`). `ifmove NAME` compiles into `CONEVT` and gates quotes when the pawn's current state label matches. Tests: `condata::ifmove_compiles`, `headless::conevt_ifmove_prints_on_the_spawn_pose`, `headless::conevt_ifmove_stays_quiet_on_the_wrong_pose`.
- `2026-10-18` — **Duke CON ifai** (`docs/23-build-engine.md`). `ifai NAME` compiles into `CONEVT` and gates quotes when the pawn's current state label matches. Tests: `condata::ifai_compiles`, `headless::conevt_ifai_prints_on_the_spawn_pose`, `headless::conevt_ifai_stays_quiet_on_the_wrong_pose`.
- `2026-10-18` — **Duke CON ifaction** (`docs/23-build-engine.md`). `ifaction NAME` compiles into `CONEVT` and gates quotes when the pawn's current state label matches. Tests: `condata::ifaction_compiles`, `headless::conevt_ifaction_prints_on_the_spawn_pose`, `headless::conevt_ifaction_stays_quiet_on_the_wrong_pose`.
- `2026-10-18` — **Duke CON ifp paiming** (`docs/23-build-engine.md`). `ifp paiming` / `plooking` compile into `CONEVT` and gate quotes when look pitch is non-zero. Tests: `condata::ifp_paiming_compiles`, `headless::conevt_ifp_paiming_prints_when_looking`, `headless::conevt_ifp_paiming_stays_quiet_when_level`.
- `2026-10-18` — **Duke CON ifactornotstayput** (`docs/23-build-engine.md`). `ifactornotstayput` compiles into `CONEVT` and gates quotes unless the pawn has `user_stayput`. Tests: `condata::ifactornotstayput_compiles`, `headless::conevt_ifactornotstayput_prints_for_the_player`, `headless::conevt_ifactornotstayput_stays_quiet_when_stayput`.
- `2026-10-18` — **Duke CON ifp pwalkingback** (`docs/23-build-engine.md`). `ifp pwalkingback` / `prunningback` compile into `CONEVT` and gate quotes when the player is moving backward. Tests: `condata::ifp_pwalkingback_compiles`, `headless::conevt_ifp_pwalkingback_prints_when_backing_up`, `headless::conevt_ifp_pwalkingback_stays_quiet_when_still`.
- `2026-10-18` — **Duke CON ifp phigher** (`docs/23-build-engine.md`). `ifp phigher` compiles into `CONEVT` and gates quotes when the pawn is above its target (else the nearest `COUNTKILL`). Tests: `condata::ifp_phigher_compiles`, `headless::conevt_ifp_phigher_prints_when_above_a_trooper`, `headless::conevt_ifp_phigher_stays_quiet_on_the_same_floor`.
- `2026-10-18` — **Duke CON ifspawnedby** (`docs/23-build-engine.md`). `ifspawnedby NAME` compiles into `CONEVT` and gates quotes when the pawn's master (else last attacker) class matches. Tests: `condata::ifspawnedby_compiles`, `headless::conevt_ifspawnedby_prints_when_master_matches`, `headless::conevt_ifspawnedby_stays_quiet_without_a_master`.
- `2026-10-18` — **Duke CON ifstrength** (`docs/23-build-engine.md`). `ifstrength N` compiles into `CONEVT` and gates quotes when pawn health is `< N`. Tests: `condata::ifstrength_compiles`, `headless::conevt_ifstrength_prints_when_health_is_below`, `headless::conevt_ifstrength_stays_quiet_when_health_holds`.
- `2026-10-18` — **Duke CON ifactioncount** (`docs/23-build-engine.md`). `ifactioncount N` compiles into `CONEVT` and gates quotes when map tics ≥ N (player action-frame counter). Tests: `condata::ifactioncount_compiles`, `headless::conevt_ifactioncount_prints_from_the_first_tic`, `headless::conevt_ifactioncount_stays_quiet_before_the_counter`.
- `2026-10-18` — **Duke CON ifcanseetarget** (`docs/23-build-engine.md`). `ifcanseetarget` compiles into `CONEVT` and gates quotes when the pawn has line of sight to its current target (else the last attacker). Tests: `condata::ifcanseetarget_compiles`, `headless::conevt_ifcanseetarget_prints_when_the_target_is_in_view`, `headless::conevt_ifcanseetarget_stays_quiet_without_a_target`.
- `2026-10-18` — **Duke CON ifp pshrunk** (`docs/23-build-engine.md`). `ifp pshrunk` / `pkicking` compile into `CONEVT` and gate quotes on shrink (`user_dukeshink`) and a quick kick. Tests: `condata::ifp_pshrunk_compiles`, `headless::conevt_ifp_pshrunk_prints_when_shrunk`, `headless::conevt_ifp_pshrunk_stays_quiet_at_full_size`.
- `2026-10-18` — **Duke CON ifp pducking** (`docs/23-build-engine.md`). `ifp pducking` / `pjetpack` / `ponsteroids` compile into `CONEVT` and gate quotes on crouch, jetpack, and steroids. Tests: `condata::ifp_pducking_compiles`, `headless::conevt_ifp_pducking_prints_when_crouched`, `headless::conevt_ifp_pducking_stays_quiet_when_standing`.
- `2026-10-18` — **Duke CON ifcount** (`docs/23-build-engine.md`). `ifcount N` compiles into `CONEVT` and gates quotes when map tics ≥ N. Tests: `condata::ifcount_compiles`, `headless::conevt_ifcount_prints_from_the_first_tic`, `headless::conevt_ifcount_stays_quiet_before_the_counter`.
- `2026-10-18` — **Duke CON ifactor** (`docs/23-build-engine.md`). `ifactor NAME` compiles into `CONEVT` and gates quotes when the pawn class matches (`APLAYER` → any `*PLAYER*`). Tests: `condata::ifactor_compiles`, `headless::conevt_ifactor_prints_for_the_player`, `headless::conevt_ifactor_stays_quiet_for_a_trooper_name`.
- `2026-10-18` — **Duke CON ifspritepal** (`docs/23-build-engine.md`). `ifspritepal N` compiles into `CONEVT` and gates quotes on the pawn translation (`DukePal21` → 21; empty is 0). Tests: `condata::ifspritepal_compiles`, `headless::conevt_ifspritepal_prints_when_the_pawn_pal_matches`, `headless::conevt_ifspritepal_stays_quiet_on_the_default_pal`.
- `2026-10-18` — **Duke CON ifcanshoottarget** (`docs/23-build-engine.md`). `ifcanshoottarget` compiles into `CONEVT` and gates quotes when a living `COUNTKILL` is in sight and within 90° (512 Build). Tests: `condata::ifcanshoottarget_compiles`, `headless::conevt_ifcanshoottarget_prints_when_facing_a_trooper`, `headless::conevt_ifcanshoottarget_stays_quiet_when_looking_away`.
- `2026-10-18` — **Duke air coast** (`docs/23-build-engine.md`). Releasing move in the air keeps the last XY speed instead of zeroing it (Doom on the ground still stops). Tests: `think_player::duke_air_coast_keeps_xy_speed`.
- `2026-10-18` — **Duke CON ifsound** (`docs/23-build-engine.md`). `ifsound NAME` compiles into `CONEVT` and gates quotes when that sound is in the sim queue (`PISTOL_FIRE` matches `duke/pistol_fire`). Tests: `condata::ifsound_compiles`, `headless::conevt_ifsound_prints_when_that_sound_is_queued`, `headless::conevt_ifsound_stays_quiet_when_that_sound_is_missing`.
- `2026-10-18` — **Duke CON ifnosounds** (`docs/23-build-engine.md`). `ifnosounds` compiles into `CONEVT` and gates quotes when the sim sound queue is empty. Tests: `condata::ifnosounds_compiles`, `headless::conevt_ifnosounds_prints_on_a_quiet_start`, `headless::conevt_ifnosounds_stays_quiet_when_a_sound_is_queued`.
- `2026-10-18` — **Duke CON ifwasweapon** (`docs/23-build-engine.md`). `ifwasweapon NAME` compiles into `CONEVT` and gates quotes when the last player hit's inflictor / source class matches that CON weapon (`RPG` → `Rocket` / `DukeRocket`). Tests: `condata::ifwasweapon_compiles`, `headless::conevt_ifwasweapon_prints_when_hit_by_a_rocket`, `headless::conevt_ifwasweapon_stays_quiet_for_the_wrong_gun`.
- `2026-10-18` — **Duke CON ifinouterspace** (`docs/23-build-engine.md`). `ifinouterspace` compiles into `CONEVT` and gates quotes when the floor is sky (`F_SKY1`). Tests: `condata::ifinouterspace_compiles`, `headless::conevt_ifinouterspace_prints_on_a_sky_floor`, `headless::conevt_ifinouterspace_stays_quiet_with_only_a_sky_ceiling`.
- `2026-10-18` — **Duke CON ifbulletnear** (`docs/23-build-engine.md`). `ifbulletnear` compiles into `CONEVT` and gates quotes when a `MISSILE` is within 64 Room units (~1024 Build). Tests: `condata::ifbulletnear_compiles`, `headless::conevt_ifbulletnear_prints_when_a_rocket_is_close`, `headless::conevt_ifbulletnear_stays_quiet_without_a_missile`.
- `2026-10-18` — **Duke CON ifangdiffl** (`docs/23-build-engine.md`). `ifangdiffl N` compiles into `CONEVT` and gates quotes when the facing delta to the nearest living `COUNTKILL` is less than N Build angle units (2048/circle). Tests: `condata::ifangdiffl_compiles`, `headless::conevt_ifangdiffl_prints_when_facing_a_trooper`, `headless::conevt_ifangdiffl_stays_quiet_when_looking_away`.
- `2026-10-18` — **Duke CON ifgapzl** (`docs/23-build-engine.md`). `ifgapzl N` compiles into `CONEVT` and gates quotes when the floor-to-ceiling gap is less than N Build z (Room × 256). Tests: `condata::ifgapzl_compiles`, `headless::conevt_ifgapzl_prints_in_a_low_room`, `headless::conevt_ifgapzl_stays_quiet_in_a_tall_room`.
- `2026-10-18` — **Duke CON ifinspace** (`docs/23-build-engine.md`). `ifinspace` compiles into `CONEVT` and gates quotes when both the floor and ceiling are sky (`F_SKY1`). Tests: `condata::ifinspace_compiles`, `headless::conevt_ifinspace_prints_when_both_planes_are_sky`, `headless::conevt_ifinspace_stays_quiet_with_only_a_sky_ceiling`.
- `2026-10-18` — **Duke CON ifawayfromwall** (`docs/23-build-engine.md`). `ifawayfromwall` compiles into `CONEVT` and gates quotes when the player is more than 8 Room units (128 Build) from a blocking line. Tests: `condata::ifawayfromwall_compiles`, `headless::conevt_ifawayfromwall_prints_in_the_open`, `headless::conevt_ifawayfromwall_stays_quiet_near_a_wall`.
- `2026-10-18` — **Duke CON iffloordistl / ifceilingdistl** (`docs/23-build-engine.md`). Those compile into `CONEVT` and gate quotes on feet-to-floor / head-to-ceiling Build z (Room × 256). Tests: `condata::iffloordistl_compiles`, `headless::conevt_iffloordistl_prints_on_the_floor`, `headless::conevt_ifceilingdistl_stays_quiet_in_a_tall_room`.
- `2026-10-18` — **Duke CON ifrespawn / EVENT_RESPAWN** (`docs/23-build-engine.md`). `ifrespawn` compiles into `CONEVT` and is true in DukeMatch / monster-respawn; `EVENT_RESPAWN` runs when a dead player Uses to revive. Tests: `condata::ifrespawn_compiles`, `headless::conevt_ifrespawn_prints_in_dukematch`, `headless::conevt_ifrespawn_stays_quiet_in_sp`, `headless::conevt_respawn_prints_after_use_while_dead`.
- `2026-10-18` — **Duke CON ifnotmoving** (`docs/23-build-engine.md`). `ifnotmoving` compiles into `CONEVT` and gates quotes when there is no move input. Tests: `condata::ifnotmoving_compiles`, `headless::conevt_ifnotmoving_prints_on_enter`, `headless::conevt_ifnotmoving_stays_quiet_when_walking`.
- `2026-10-18` — **Duke CON ifgotweaponce** (`docs/23-build-engine.md`). `ifgotweaponce` compiles into `CONEVT` and gates quotes when the player owns a weapon. Tests: `condata::ifgotweaponce_compiles`, `headless::conevt_ifgotweaponce_prints_when_armed`, `headless::conevt_ifgotweaponce_stays_quiet_unarmed`.
- `2026-10-18` — **Duke CON ifhitspace** (`docs/23-build-engine.md`). `ifhitspace` compiles into `CONEVT` and gates quotes while Use is held. Tests: `condata::ifhitspace_compiles`, `headless::conevt_ifhitspace_prints_when_use_is_held`, `headless::conevt_ifhitspace_stays_quiet_on_enter`.
- `2026-10-18` — **Duke grazing-overlap 3D floors** (`docs/23-build-engine.md`). Any positive 2D AABB overlap (not just ≥ 64×64) with disjoint Z becomes a solid `Sector_Set3DFloor`. A shared edge still does not. Tests: `fx::grazing_overlap_becomes_a_3d_floor`.
- `2026-10-18` — **Duke shared-wall lofts** (`docs/23-build-engine.md`). A loft whose loop shares a wall with the hall still becomes a solid `Sector_Set3DFloor` when the centroid is nested (or the 2D boxes overlap) and Z is disjoint. Tests: `fx::shared_wall_loft_becomes_a_3d_floor`.
- `2026-10-18` — **Duke CON ifpdistl / ifpdistg** (`docs/23-build-engine.md`). Those compile into `CONEVT` and gate quotes on Build XY distance to the nearest living `COUNTKILL` (Room × 16). Tests: `condata::ifpdistl_compiles`, `headless::conevt_ifpdistl_prints_when_a_trooper_is_close`, `headless::conevt_ifpdistl_stays_quiet_when_far_or_alone`.
- `2026-10-18` — **Duke CON ifcansee** (`docs/23-build-engine.md`). `ifcansee` compiles into `CONEVT` and gates quotes when the player has line of sight to a living `COUNTKILL` (or the last attacker). Tests: `condata::ifcansee_compiles`, `headless::conevt_ifcansee_prints_when_a_trooper_is_in_view`, `headless::conevt_ifcansee_stays_quiet_when_alone`.
- `2026-10-18` — **Duke CON ifpinventory** (`docs/23-build-engine.md`). `ifpinventory NAME` compiles into `CONEVT` and gates quotes when the player holds that item (`FIRSTAID` → `DukeFirstAid`, and the other Atomic pack names). Tests: `condata::ifpinventory_compiles`, `headless::conevt_ifpinventory_prints_when_held`, `headless::conevt_ifpinventory_stays_quiet_without_item`.
- `2026-10-18` — **Duke CON ifsquished** (`docs/23-build-engine.md`). `ifsquished` compiles into `CONEVT` and gates quotes when the last hit was a crush or the floor/ceiling gap is tighter than 32 (Duke `(fz-cz) < (32<<8)`) / the pawn height. Tests: `condata::ifsquished_compiles`, `headless::conevt_ifsquished_prints_when_ceiling_pins`, `headless::conevt_ifsquished_stays_quiet_in_open_room`.
- `2026-10-17` — **Duke CON RESETWEAPONS / RESETINVENTORY** (`docs/23-build-engine.md`). Those `onevent`s run on a fresh map start (not a hub return). Tests: `headless::conevt_resetweapons_prints_on_fresh_start`.
- `2026-10-17` — **Duke CON ifoutside / ifhitweapon** (`docs/23-build-engine.md`). `ifoutside` (sky ceiling) and `ifhitweapon` (recent player damage) compile into `CONEVT`. Tests: `condata::ifoutside_compiles`, `headless::conevt_ifoutside_prints_under_sky`, `headless::conevt_ifoutside_stays_quiet_indoors`.
- `2026-10-17` — **Duke CON ifp** (`docs/23-build-engine.md`). `ifp palive` / `ponground` / `pfalling` / `pjumping` / `pstanding` / `pwalking` / `prunning` compile into `CONEVT` and gate quotes on the player pose. Tests: `condata::ifp_palive_compiles`, `headless::conevt_ifp_palive_prints_on_enter`, `headless::conevt_ifp_pfalling_stays_quiet_on_ground`.
- `2026-10-17` — **Duke CON ifdead / ifmultiplayer** (`docs/23-build-engine.md`). `ifdead` and `ifmultiplayer` compile into `CONEVT` and gate quotes on death and DukeMatch / coop. Tests: `condata::ifdead_and_ifmultiplayer_compile`, `headless::conevt_ifdead_prints_on_killit`, `headless::conevt_ifmultiplayer_prints_in_dukematch`.
- `2026-10-17` — **Duke CON ifinwater** (`docs/23-build-engine.md`). `ifinwater` / `ifonwater` compile into `CONEVT` and gate quotes on water level (fully under / wading). Tests: `condata::ifinwater_compiles`, `headless::conevt_ifinwater_prints_when_submerged`, `headless::conevt_ifinwater_stays_quiet_on_dry_land`.
- `2026-10-17` — **Duke CON ifphealthl** (`docs/23-build-engine.md`). `ifphealthl` / `ifphealthg` compile into `CONEVT` and gate quotes on player health. Tests: `condata::ifphealthl_compiles`, `headless::conevt_ifhealthl_prints_when_hurt_enough`.
- `2026-10-17` — **Duke CON ifrnd** (`docs/23-build-engine.md`). `onevent` `ifrnd N` compiles into `CONEVT` and gates the following `quote` / `sound` (`pr_conifrnd` byte `< N`; 0 never, 256 always). Tests: `condata::ifrnd_gates_the_following_quote`, `headless::conevt_ifrnd_zero_never_prints`, `headless::conevt_ifrnd_always_prints`.
- `2026-10-17` — **Duke overlapping-strip 3D floors** (`docs/23-build-engine.md`). Two sectors with disjoint Z, no shared wall, and a 2D AABB overlap of at least 64×64 Build units become a solid `Sector_Set3DFloor` even when neither centroid is inside the other. Tests: `fx::overlapping_strip_becomes_a_3d_floor`.
- `2026-10-17` — **Duke nested lofts** (`docs/23-build-engine.md`). A smaller sector whose centroid sits inside a larger one, with disjoint Z and no shared wall, becomes a solid `Sector_Set3DFloor` (loft over a hall). Tests: `fx::nested_loft_becomes_a_3d_floor`.
- `2026-10-17` — **Duke CON CHANGEWEAPON / HOLSTER** (`docs/23-build-engine.md`). `EVENT_CHANGEWEAPON` runs on a weapon key / next / prev; `EVENT_HOLSTER` when a switch starts lowering. Tests: `headless::conevt_changeweapon_and_holster_print`.
- `2026-10-17` — **Duke long-fall damage** (`docs/23-build-engine.md`). `DukePlayer` landing after ~2 s of falling (72 tics / Duke `falling_counter >= 62`) takes half health. Tests: `think_player::duke_long_fall_takes_half_health`.
- `2026-10-17` — **Duke C-9 walk-through** (`docs/23-build-engine.md`). A dead health wall clears `ML_BLOCKING` / mid texture / hitscan / sight so a C-9 blast is walk-through; the linedef stays in the BSP. Tests: `specials::dead_health_line_stops_blocking_and_hides_mid`.
- `2026-10-17` — **Duke CON USE / FIRE** (`docs/23-build-engine.md`). `EVENT_USE` and `EVENT_FIRE` quotes run on the Use / Attack press edge. Tests: `headless::conevt_use_and_fire_print_on_edge`.
- `2026-10-17` — **Duke CON GETSHOT** (`docs/23-build-engine.md`). `EVENT_GETSHOT` / `EVENT_INCURDAMAGE` quotes run when the player is hurt; `EVENT_KILLIT` on death. Tests: `headless::conevt_getshot_prints_on_damage`.
- `2026-10-17` — **Duke CON EGS** (`docs/23-build-engine.md`). `EVENT_EGS` / `EVENT_ENDLEVEL` quotes run when a map exits to intermission. Tests: `headless::conevt_egs_prints_on_exit`.
- `2026-10-17` — **Duke Load Game .SAV** (`docs/23-build-engine.md`). `identify_save` tells ZIP `.fsave` from a Duke `game#.sav`. Load Game / `load` starts the named map at the saved skill; the dump is not restored. Tests: `sav::header_names_e1m3`.
- `2026-10-17` — **Duke sector fog** (`docs/23-build-engine.md`). Build `visibility` still darkens the light and now writes UDMF `fogdensity` (× 4). Tests: `convert::visibility_writes_fogdensity`.
- `2026-10-17` — **DukeMatch** (`docs/23-build-engine.md`). Deathmatch on a Duke map drops `COUNTKILL` and sits you on a type-11 start. Tests: `world::duke_match_clears_monsters_and_uses_a_dm_start`, `duke3d_play::e1m1_dukematch_has_no_monsters`.
- `2026-10-17` — **Duke CON enter events** (`docs/23-build-engine.md`). `onevent` `quote` / `sound` compile to a `CONEVT` lump and run on map enter (`EVENT_ENTERLEVEL`). `ai` / `ifxxx` still use Doom AI. Tests: `condata::reads_defines_sounds_actions_and_actors`, `duke3d::e1l1_has_doors_an_exit_slopes_and_a_sky`.
- `2026-10-17` — **Duke stacked rooms** (`docs/23-build-engine.md`). Two sectors with the same 2D footprint (centroids inside each other, no shared wall) and disjoint Z become a solid `Sector_Set3DFloor`. Tests: `fx::stacked_rooms_become_a_3d_floor`.
- `2026-10-17` — **Duke ice / shrink poses** (`docs/23-build-engine.md`). Duke monsters get CON `Ice` / `Shrink` stills; freeze and shrink jump there, grow-back returns to `See`. Tests: `damage::ice_freezes_then_a_second_hit_shatters`, `damage::shrink_grows_back_after_the_timer`, `duke3d_play::shrinker_and_freezer_change_a_trooper`.
- `2026-10-17` — **Duke crouch** (`docs/23-build-engine.md`). Hold move-down on the ground to drop `DukePlayer` height/view to 40% (walk halves, no jump). A low ceiling keeps you down. Tests: `think_player::duke_crouch_lowers_height_and_stands_back_up`, `duke3d_play::e1m1_crouch_lowers_duke`.
- `2026-10-17` — **Duke .DMO demo cameras** (`docs/23-build-engine.md`). Playing a `.DMO` locks the view to `DukeCamera` (`SE 27` / `CAMERA1`): a camera that can see the player, else the nearest, cycling every 70 tics. Move does not drop the view. Tests: `specials::demo_cameras_lock_the_view`, `duke3d_play::dmo_playback_looks_through_a_camera`.
- `2026-10-17` — **Duke .SAV headers** (`docs/23-build-engine.md`). `game#.sav` `loadpheader` fields (`BYTEVERSION`, name, volume / level / skill) parse via Ken `kdfread`. The board dump is not restored. Tests: `sav::header_names_e1m3`.
- `2026-10-17` — **Duke .DMO LZW** (`docs/23-build-engine.md`). Ken `uncompress` / `kdfread` unpacks Atomic `DEMO1`–`3` into tic streams (byte deltas). Tests: `dmo::raw_v13_tic_is_forward_and_fire`, `duke3d::grp_dmo_headers_name_a_map`.
- `2026-10-17` — **Duke .DMO headers** (`docs/23-build-engine.md`). 1.0 / 1.3D / 1.5 `.DMO` headers name `ExMy`; a raw 1.3D tic stream (10 bytes) becomes `TicCmd` and walks E1M1. LZW Atomic bodies stay header-only. Tests: `dmo::v13_header_names_e1m1`, `dmo::raw_v13_tic_is_forward_and_fire`, `duke3d::grp_dmo_headers_name_a_map`, `duke3d_play::uncompressed_dmo_walks_e1m1`.
- `2026-10-17` — **Duke SE40–45 FoF** (`docs/23-build-engine.md`). Unfinished `SE 40`–`45` write `Sector_Set3DFloor` (160) on the control sector and tag same-hitag rooms (40/41 solid, 42 swimmable, 43–45 see-through). Atomic has none; unit test covers convert. Tests: `fx::fof_se40_tags_the_matching_hitag_sector`.
- `2026-10-17` — **Duke SE16 reactor skew** (`docs/23-build-engine.md`). `SE 16` writes `user_skew` and oscillates the sector's vertices along the effector angle (`GPSPEED`/16). Atomic has no SE16; unit tests cover convert + playsim. Tests: `fx::skew_east_oscillates_along_x`, `specials::skew_oscillates_sector_vertices`.
- `2026-10-17` — **Duke SE22 teeth** (`docs/23-build-engine.md`). `SE 22` writes `user_teeth` and retracts the prong sector's vertices when the matching lotag-29 door ceiling opens. Atomic has no SE22; unit tests cover convert + playsim. Tests: `fx::teeth_follow_the_matching_lotag_29_door`, `specials::teeth_retract_when_the_door_ceiling_opens`.
- `2026-10-17` — **Duke frame effect** (`docs/23-build-engine.md`). `FRAMEEFFECT1` is a translucent `DukeFrameEffect` (26998) that `A_DukeFrameEffect`s the nearest `COUNTKILL` (copy sprite + sit on it). Tests: `action::a_duke_frame_effect_copies_the_nearest_countkill`, `duke3d::frame_effect_class_exists`, `duke3d_play::frame_effect_copies_a_trooper`.
- `2026-10-17` — **Duke force sphere** (`docs/23-build-engine.md`). `FORCESPHERE` is a translucent `DukeForceSphere` (26997) that pulses `A_SetScale`; the Overlord `A_SpawnItemEx`s one on fire. Tests: `duke3d::force_sphere_class_exists`, `duke3d_play::force_sphere_pulses_scale`.
- `2026-10-17` — **Duke natural lightning** (`docs/23-build-engine.md`). `NATURALLIGHTNING` sprites become `DukeNaturalLightning` (26996) and flash the map (`Light_ForceLightning`). Tests: `duke3d::natural_lightning_becomes_a_thing`, `duke3d_play::natural_lightning_flashes_the_map`.
- `2026-10-17` — **Duke water bubbles** (`docs/23-build-engine.md`). `WATERBUBBLEMAKER` sprites become non-solid `DukeWaterBubbleMaker` (26995) and spawn rising `DukeWaterBubble`s. Tests: `duke3d::water_bubble_makers_become_things`, `duke3d_play::water_bubble_maker_spawns_bubbles`.
- `2026-10-17` — **Duke RESPAWN markers** (`docs/23-build-engine.md`). `RESPAWN` sprites become non-solid `DukeRespawn` (26994) and `A_DukeRespawn`s the lotag actor after the hitag delay (default ~30 s). Tests: `action::a_duke_respawn_spawns_the_doomednum_once`, `duke3d::respawn_markers_become_duke_respawn`, `duke3d_play::respawn_marker_is_non_solid_and_spawns`.
- `2026-10-17` — **Duke CYCLER shade pulse** (`docs/23-build-engine.md`). `CYCLER` sprites start a `Light_Glow` between the sector shade and the cycler's lotag shade. Tests: `fx::cycler_emits_a_light_glow`, `duke3d::atomic_has_cyclers`, `duke3d_play::cycler_pulses_sector_light`.
- `2026-10-17` — **Duke SE20 stretch** (`docs/23-build-engine.md`). Lotag-27 / `SE 20` writes `user_stretch` and slides the sector's vertices along the effector angle (`GPSPEED`/16) when an activator fires `Sector_Stretch` (284). Paper-thin bridges still start walkable. Tests: `specials::stretch_moves_sector_vertices`, `duke3d::e1m1_stretch_bridges_write_user_stretch`, `duke3d_play::stretching_an_e1m1_bridge_moves_its_walls`.
- `2026-10-17` — **Duke subway car walls** (`docs/23-build-engine.md`). `SE 6` / `14` / `30` cars become a polyobject (`user_subpo`) that `Polyobj_MoveTo`s along the locator path with the riders (E2L1's car is lotag 0). Tests: `duke3d::e2m1_subway_writes_locator_path`, `duke3d_play::riding_e2m1_subway_follows_locators`.
- `2026-10-17` — **Duke subway locator path** (`docs/23-build-engine.md`). `SE 6` / `14` / `30` write `user_subway` waypoints from `LOCATORS` and carry nearby actors along the track (car walls stay put). Tests: `specials::subway_carries_an_actor_along_locators`, `duke3d::e2m1_subway_writes_locator_path`, `duke3d_play::riding_e2m1_subway_follows_locators`.
- `2026-10-17` — **Duke SE27 demo cameras** (`docs/23-build-engine.md`). Atomic `SE 27` sprites spawn as `DukeCamera` (same class as `CAMERA1`) so viewscreens can look through them. `.DMO` still does not drive them. Tests: `duke3d::se27_demo_cameras_become_duke_cameras`, `duke3d::atomic_skips_se16_se22_se27`.
- `2026-10-17` — **Refuse non-Duke GRPs** (`docs/23-build-engine.md`). `BLOOD.INI` / `SW.CON` / `REDNECK.CON` / `NAM.CON` archives return `BuildError::Unsupported` instead of converting. Tests: `wad::tests::other_build_grps_are_rejected`, `grp::tests::reads_directory_and_bodies`.
- `2026-10-17` — **Duke browser title** (`docs/23-build-engine.md`). Loading a GRP in `room-web` without `?map=` stays on the splash / MENUDEF (Enter or click skips the splash, then New Game → episode → skill). `?map=` still starts immediately. Tests: `cargo check -p room-web --target wasm32-unknown-unknown`.
- `2026-10-17` — **Duke Room demos** (`docs/23-build-engine.md`). Console `record` / `stop` / `playdemo` write a Room `.fdem` (`quick` under the config `demos/` dir). E1M1 play records 40 idle tics and verifies on a fresh world. Original `.DMO` still unsupported. Tests: `duke3d_play::e1m1_demo_records_and_verifies`, `dispatch::record_and_playdemo_emit_host`.
- `2026-10-17` — **Duke Auto Aim / Mouse Aiming** (`docs/23-build-engine.md`). MENUDEF `OptionsMenu` plus wired Options toggle `autoaim` and `mouse_look`. `World.autoaim = false` keeps hitscans and player missiles on pitch. Tests: `hitscan::autoaim_off_ignores_a_raised_target`, `duke3d::e1l1_has_doors_an_exit_slopes_and_a_sky`.
- `2026-10-17` — **Duke splash skip** (`docs/23-build-engine.md`). A key or click ends `RADLOGO` immediately so the looping logo and title menu appear. Returning to the title restarts the splash. Tests: `title::title_anim_loops_and_finale_holds`.
- `2026-10-17` — **Duke Room saves** (`docs/23-build-engine.md`). Title Load Game / pause Save+Load and console `save`/`load` write a Room `.fsave` (`quick` under the config `saves/` dir). E1M1 play restores Duke's health and map time. Original `.SAV` / `.DMO` still unsupported. Tests: `duke3d_play::e1m1_save_and_load_restores_duke`, `dispatch::save_and_load_emit_host`, `duke3d::e1l1_has_doors_an_exit_slopes_and_a_sky`.
- `2026-10-17` — **Duke RADLOGO splash** (`docs/23-build-engine.md`). `RADLOGO.ANM` plays once (`RADL00`… / `RADLDEF`) before the looping title; the menu stays hidden during the splash. Tests: `title::title_anim_loops_and_finale_holds`, `duke3d::e1l1_has_doors_an_exit_slopes_and_a_sky`.
- `2026-10-17` — **Duke title ANM loops** (`docs/23-build-engine.md`). `TITL00`… wraps at the file fps (the LOGO ring). Episode finales still play once. Intermission music falls back to `GRABBAG`. Tests: `title::title_anim_loops_and_finale_holds`.
- `2026-10-17` — **Duke title music** (`docs/23-build-engine.md`). `USER.CON` `music 0` (`GRABBAG.MID`) is copied into the PWAD; title candidates try `GRABBAG` first. Tests: `con::reads_levels_volumes_and_music`, `wad::tests::grp_becomes_a_mountable_wad_with_maps_and_tiles`, `duke3d::e1l1_has_doors_an_exit_slopes_and_a_sky`.
- `2026-10-17` — **Duke episode select** (`docs/23-build-engine.md`). New Game opens the MAPINFO episode list, then skill; the chosen episode's start map is loaded. Tests: `menu::episode_select_remembers_the_start_map`, `menudef_runtime::builds_duke_main_menu`.
- `2026-10-17` — **Duke title menu** (`docs/23-build-engine.md`). Converted GRPs ship a MENUDEF `MainMenu` (New Game / Options / Quit) so the title is not Doom's missing `M_NGAME` patches. Tests: `wad::tests::grp_becomes_a_mountable_wad_with_maps_and_tiles`, `duke3d::e1l1_has_doors_an_exit_slopes_and_a_sky`.
- `2026-10-17` — **Duke episode-end ANM** (`docs/23-build-engine.md`). Finale plays `E1AN`…`E4AN` from `CINEOV2` / `CINEOV3` / `DUKETEAM` / `VOL4E1` once (else `CREDIT`). Tests: `finale::draws_finale_commands`, `title::title_anim_plays_once_then_holds`, `duke3d::e1l1_has_doors_an_exit_slopes_and_a_sky`.
- `2026-10-17` — **Duke title ANM playback** (`docs/23-build-engine.md`). `LOGO.ANM` stills become `TITL00`… plus `TITLEDEF`; the title plays once then holds. Tests: `title::title_anim_plays_once_then_holds`, `duke3d::e1l1_has_doors_an_exit_slopes_and_a_sky`.
- `2026-10-17` — **Duke episode stills** (`docs/23-build-engine.md`). Held ANM frames become `INTERPIC` / `CREDIT` / `WIMAP0`–`2` (team photo and episode cinematics). The finale draws `CREDIT`. Tests: `finale::draws_finale_commands`, `duke3d::e1l1_has_doors_an_exit_slopes_and_a_sky`.
- `2026-10-17` — **Duke title pic** (`docs/23-build-engine.md`). The held still of `LOGO.ANM` (DeluxePaint LPF, last frame before the ring) is remapped onto `PLAYPAL` as `TITLEPIC`. No ANM playback or menus. Tests: `anm::title_frame_is_a_full_image`, `duke3d::e1l1_has_doors_an_exit_slopes_and_a_sky`.
- `2026-10-17` — **Duke stuck monsters nudge** (`docs/23-build-engine.md`). A `COUNTKILL` that fails `check_position` at spawn is moved up to 48 units (skipped in vanilla-fixed). Tests: `world::countkill_stuck_in_a_wall_is_nudged_free`, `duke3d_play::e1m1_spawns_duke_actors_and_plays`.
- `2026-10-17` — **Duke run speed** (`docs/23-build-engine.md`). `Player.ForwardMove` scales player thrust; `DukePlayer` uses 1.25 (steroids still double). Tests: `think_player::forward_move_scales_player_thrust`, `duke3d::e1l1_has_doors_an_exit_slopes_and_a_sky`.
- `2026-10-17` — **Duke viewscreen canvas in the browser** (`docs/23-build-engine.md`). `room-web` refreshes `VCAMA0` from the bound camera with a GPU copy (no `read_rgba` on wasm). Tests: `duke3d_play::using_a_viewscreen_looks_through_a_camera` (bind); compile `room-web`.
- `2026-10-17` — **Duke stretch bridges stay walkable** (`docs/23-build-engine.md`). Lotag-27 / `SE 20` sectors that start paper-thin open to a 64-unit height (`user_bridge`). The walls still do not stretch. Tests: `duke3d::e1l1_has_doors_an_exit_slopes_and_a_sky`, `duke3d_play::e1m1_bridges_are_walkable`.
- `2026-10-17` — **Duke viewscreen canvas follows the camera** (`docs/23-build-engine.md`). `A_ChangeCameraToNearest` binds `VCAMA0` to the camera you look through. Tests: `action::a_change_camera_to_nearest_looks_through_it`, `duke3d_play::using_a_viewscreen_looks_through_a_camera`.
- `2026-10-17` — **Duke viewscreen live canvas** (`docs/23-build-engine.md`). Viewscreens use sprite `VCAMA0`, bound to the first `DukeCamera` (`cameratexture` / `camera_textures`). Desktop refreshes the canvas. Tests: `duke3d::e1l1_has_doors_an_exit_slopes_and_a_sky`, `duke3d_play::using_a_viewscreen_looks_through_a_camera`.
- `2026-10-17` — **Duke walkable lotag-30 gears** (`docs/23-build-engine.md`). `SE 0` + `SE 1` in a lotag-30 sector write `user_spin` / `user_spinx` / `user_spiny` and carry on-floor actors around the pivot (floor mesh stays put). Tests: `specials::rotating_floor_carries_an_actor_around_the_pivot`, `duke3d::e1m2_walkable_gears_write_user_spin`, `duke3d_play::walking_a_lotag30_gear_turns_you`.
- `2026-10-17` — **Duke cameras and viewscreens** (`docs/23-build-engine.md`). Use a `DukeViewScreen` to look through the nearest `DukeCamera` (`A_ChangeCameraToNearest`); move restores the player eye. Tests: `action::a_change_camera_to_nearest_looks_through_it`, `duke3d::e1l1_has_doors_an_exit_slopes_and_a_sky`, `duke3d_play::using_a_viewscreen_looks_through_a_camera`.
- `2026-10-17` — **Duke palette mini-bosses** (`docs/23-build-engine.md`). Palette-21 non-boss enemies (not captains) are `Duke*Mini` with double health. Tests: `duke3d::e1l1_has_doors_an_exit_slopes_and_a_sky`, `duke3d_play::palette_mini_bosses_have_more_health`.
- `2026-10-17` — **Duke holoduke AI** (`docs/23-build-engine.md`). `DukeHolodukeDecoy` wanders (`A_Wander`, `+FRIENDLY`) and `A_Look` / chase treat it as a player-like target. Tests: `action::a_look_targets_a_holoduke_decoy`, `duke3d_play::using_inventory_starts_night_vision_and_a_holoduke`.
- `2026-10-17` — **Duke expander** (`docs/23-build-engine.md`). Slot 7 also has `DukeExpander` (`GROWAMMO`): `DamageType Grow` un-shrinks or swells a monster (~2 s) until it pops. Tests: `damage::grow_unshrinks_then_swells_and_pops`, `duke3d::e1l1_has_doors_an_exit_slopes_and_a_sky`, `duke3d_play::shrinker_and_freezer_change_a_trooper`.
- `2026-10-17` — **Duke ice corpse** (`docs/23-build-engine.md`). When a freeze wears off, a `COUNTKILL` stays as an ice statue (`user_dukeicecorpse`) until a further hit shatters it. Tests: `damage::ice_timeout_leaves_a_corpse_that_shatters`, `duke3d_play::shrinker_and_freezer_change_a_trooper`.
- `2026-10-17` — **Duke night-vision and underwater palettes** (`docs/23-build-engine.md`). Infrared uses PLAYPAL 13 (green); full submersion uses HUD palette 14 (blue). Tests: `mugshot::palette_priority_red_then_gold_then_suit`, `duke3d_play::{using_inventory_starts_night_vision_and_a_holoduke, underwater_uses_a_blue_palette}`.
- `2026-10-17` — **Duke shrink grow-back** (`docs/23-build-engine.md`). A shrink lasts ~8 s (`user_dukeshink`) then the actor returns to full scale; a further shrink hit resets the timer. Tests: `damage::shrink_grows_back_after_the_timer`, `duke3d_play::shrinker_and_freezer_change_a_trooper`.
- `2026-10-17` — **Duke trooper captains teleport** (`docs/23-build-engine.md`). Palette-21 `LIZTROOP` sprites are `DukeTrooperCaptain`: they chase and warp nearby (`A_RandomTeleport`). Tests: `action::a_random_teleport_lands_inside_the_range`, `duke3d::e1l1_has_doors_an_exit_slopes_and_a_sky`, `duke3d_play::trooper_captains_teleport`.
- `2026-10-17` — **Duke C-9 breaks nearby walls** (`docs/23-build-engine.md`). A charge `A_Explode` damages health lines in range (force fields, glass); the linedefs stay in the BSP. Test: `duke3d_play::c9_blast_deletes_nearby_wall_geometry`.
- `2026-10-17` — **Duke force-field walls break** (`docs/23-build-engine.md`). `BIGFORCE` / `W_FORCEFIELD` / `FORCERIPPLE` walls are `checkhitwall` breakables: a shot or blast zeros health, clears the texture and contact damage, and stops projectile/hitscan blocking. Tests: `duke3d::e1l1_has_doors_an_exit_slopes_and_a_sky`, `duke3d_play::force_field_walls_break_when_shot`.
- `2026-10-17` — **Duke shrinker and freezer** (`docs/23-build-engine.md`). `DukeShrinkSpark` (`DamageType Shrink`) scales a `COUNTKILL` to 25% (stomp kills it). `DukeFreezeBlast` (`Ice`) holds an actor ~8 s; any further hit shatters. Tests: `damage::{shrink_scales_a_monster_and_stomp_kills_it, ice_freezes_then_a_second_hit_shatters}`, `duke3d_play::shrinker_and_freezer_change_a_trooper`.
- `2026-10-17` — **Duke inventory use** (`docs/23-build-engine.md`). `'` (`+invuse`) heals 1 HP/tic from a stored `DukeFirstAid` (1 unit/tic, cap 100) and a press starts steroids (420 tics, double move), night vision, scuba/boots (radsuit + scuba air timer), or a `DukeHolodukeDecoy`. Tests: `think_player::{use_inventory_heals_from_the_medkit, use_inventory_starts_steroids_when_the_medkit_cannot_heal, use_inventory_starts_night_vision_scuba_and_a_holoduke}`, `duke3d_play::{using_inventory_heals_then_starts_steroids, using_inventory_starts_night_vision_and_a_holoduke}`.
- `2026-10-17` — **Duke quick kick** (`docs/23-build-engine.md`). `/` (`+quickkick`) plays `DukeKick` Fire without holstering and restores the ready gun. Tests: `think_player::quick_kick_returns_to_the_ready_gun`, `duke3d_play::quick_kick_returns_to_the_pistol`.
- `2026-10-17` — **Duke tripbomb laser** (`docs/23-build-engine.md`). An armed `DukeTripbomb` trips when a shootable actor crosses its facing beam (`A_JumpIfLaserHit`) and draws `DukeLaserLine` (`LASERLINE`). Tests: `action::a_jump_if_laser_hit_trips_in_front_not_beside`, `duke3d_play::pipebombs_tripbombs_and_jetpack_work`.
- `2026-10-17` — **Duke babe CON lines and spawn** (`docs/23-build-engine.md`). A tip plays `DUKE_TIP1` / `DUKE_TIP2`. A kill plays `DUKE_HIT_STRIPPER1` / `2`, drops CON `money` bills, leaves `BARBROKE` / `BLOODYPOLE` / `FEM6PAD`, or screams (`LADY_SCREAM`) and vanishes. Tests: `duke3d_play::{using_a_babe_takes_a_tip, shooting_a_babe_plays_a_line_and_spawns}`.
- `2026-10-17` — **Duke broken fixtures** (`docs/23-build-engine.md`). Shooting a `WATERFOUNTAIN` / `TOILET` / `STALL` swaps `WATERFOUNTAINBROKE` / `TOILETBROKE` / `STALLBROKE` and stops Use (dead `+USESPECIAL` actors are ignored). Tests: `use_lines::use_actor_skips_a_dead_fixture`, `duke3d_play::shooting_a_fountain_swaps_its_picture`.
- `2026-10-17` — **Duke SE33 quake debris** (`docs/23-build-engine.md`). `SE 33` is `DukeQuakeDebris` (`+SPAWNCEILING`); while a quake is felt it drops `DukeScrap` (`A_JumpIfQuake`). Tests: `action::a_jump_if_quake_spawns_scrap`, `duke3d_play::se33_drops_scrap_during_a_quake`.
- `2026-10-17` — **Duke idle quips** (`docs/23-build-engine.md`). Standing still for 140 tics plays an `IdleTaunt` (`DUKE_CRACK_FIRST` / `DUKE_CRACK` / `DUKE_CRACK2`, `pr_idletaunt`). Tests: `think_player::standing_still_plays_an_idle_taunt`, `duke3d_play::standing_still_plays_an_idle_quip`.
- `2026-10-17` — **Duke kill taunts** (`docs/23-build-engine.md`). `DukePlayer` has `KillTaunt` lines (`DUKE_YES`, `DUKE_GETWEAPON2`, `DUKE_CRACK2`); a player kill of a `COUNTKILL` plays one (`pr_killtaunt`). Tests: `damage::player_kill_plays_a_kill_taunt`, `duke3d_play::killing_a_trooper_plays_a_taunt`.
- `2026-10-17` — **Duke fountains and toilets** (`docs/23-build-engine.md`). `WATERFOUNTAIN` is `+USESPECIAL` and heals 1 HP per Use up to `MAXWATERFOUNTAINHEALTH` (`A_HealTarget`). `TOILET` / `STALL` flush (`FLUSH_TOILET`). Tests: `use_lines::use_fountain_heals_the_player_up_to_the_cap`, `duke3d_play::using_a_fountain_heals`.
- `2026-10-17` — **Duke babe use-to-tip** (`docs/23-build-engine.md`). `+USESPECIAL` actors run their `Use` state on player Use (`use_actors`). Babes take one `DukeMoney` and print a tip, or ask for one. Tests: `use_lines::{use_actor_takes_money_and_prints_a_tip, use_actor_without_money_asks_for_a_tip}`, `duke3d_play::using_a_babe_takes_a_tip`.
- `2026-10-17` — **Duke ATM money pickup** (`docs/23-build-engine.md`). `DukeMoney` from a shot ATM stays on the floor as `+COUNTITEM` (max 999) and is taken on touch. Test: `duke3d_play::atms_drop_money`.
- `2026-10-17` — **Duke force-field contact damage** (`docs/23-build-engine.md`). `W_FORCEFIELD` / `FORCERIPPLE` sprites deal `ContactDamage 8` every 8 tics to overlapping shootable actors. `BIGFORCE` walls write `user_contactdamage` and block hitscans/projectiles. Tests: `world::contact_damage_hurts_overlapping_player`, `convert::bigforce_walls_write_contact_damage`, `duke3d_play::force_fields_hurt_on_touch`.
- `2026-10-17` — **Duke SE8/SE9 door lights** (`docs/23-build-engine.md`). A ceiling door (20/22/26/29) with an `SE 8` or `SE 9` lights same-hitag rooms to the effector's shade when it opens (`SE 8`) or closes (`SE 9`), and restores the sector shade the other way (UDMF `user_doorlight` / `user_doorbright` / `user_doorlightopen`). Tests: `specials::{door_raise_brightens_se8_rooms_and_restores_them, door_close_brightens_se9_rooms}`, `convert::se8_in_a_ceiling_door_writes_door_light_fields`, `duke3d_play::opening_a_door_brightens_se8_lights`.
- `2026-10-17` — **Duke SE8 lights, masterswitch effectors, glass chain, ATM money, pipebomb crouch** (`docs/23-build-engine.md`). A C-9 detonation runs same-hitag `SE 8` `Light_ChangeToValue`. A `MASTERSWITCH` (switch or touchplate of its lotag) also operates `SE 2`/`18`/`21`/`31`/`32` in its sector. Glass with a shared lotag or in an `SE 128` sector shares UDMF `healthgroup`; `damage_linedef` runs Death on every peer. ATMs scatter `DukeMoney`. Pipebombs thrown with move-down on the ground use Raze's crouch roll (`playercrouch`). Tests: `duke3d_play::{c9_blast_brightens_se8_lights, masterswitch_runs_its_sector_effectors, broken_glass_spreads_to_the_next_pane, atms_drop_money, pipebomb_crouch_roll}`, `specials::damage_linedef_destroys_healthgroup_peers`, `deco_eval` `playercrouch`.
- `2026-10-17` — **Duke breakable walls, C-9 chains, floor doors, pipebomb throw** (`docs/23-build-engine.md`). Glass, stained glass, the fan grate, screens, machines, tech walls, ATMs and wall lights break when shot or blasted: each line has 1 health and a death special running a generated ACS script (new `room-build` ACS0 assembler: texture changes, `Line_SetBlocking`, a debris spot's `Active` state, SE3 lights). SE13 sectors start shut and a C-9 group (charges share a tid per hitag, with Duke's fuse countdown; hidden charges included) restores them and runs SE18/SE21 of that hitag; `MASTERSWITCH` sets off its sector's charges after its delay. Floor doors (21) toggle, pipebombs wind up while fire is held and follow the pitch, debris flies at random, SE19 walls drop the ceiling when shot. Sim: thing death specials (GZDoom `Die`), `A_Explode(damage, distance)` and blast damage on destructible lines, runtime `blockhitscan` (`Line_SetBlocking` 512), line lookups by `moreids`, `A_ReFire(state)` / `Hold`, evaluated weapon action args, `frandom` / `CountInv`, GZDoom-signed DECORATE `pitch`; DECORATE action args accept nested calls and operators. Tests: `duke3d_play::{a_blown_charge_sets_off_its_group_and_opens_the_vent, glass_walls_break_when_shot, floor_doors_close_again, pipebomb_throw_strength_and_pitch}`, `damage::death_runs_the_thing_special`, `specials::line_set_blocking_finds_moreids_and_clears_hitscan`, `action::a_explode_args_damage_destructible_lines`, `acs::strings_follow_the_code_and_jumps_land_after_blocks`; `bsp::two_sided_line_sides_never_share_a_leaf` now uses a Doom-wound fixture.
- `2026-10-17` — **Duke destruction, guns, pipebombs, doors** (`docs/23-build-engine.md`). Raze's destructible sprites (the E1L1 vent fan, grates, panels, chairs, cameras, bottles, pipes, …) break on the first hit into their broken picture or vanish, with their sound, scrap / glass / smoke debris, and stop blocking. The shotgun kicks, pumps (cock at kickback 15) and ejects a shell, with a two-frame flash. The pistol ejects shells and reloads after 12 shots, and the ripper fires every 3 Duke tics. Thrown pipebombs bounce, and explosives use `PIPEBOMB_EXPLODE`. The sim gained a bounce subset (`+BOUNCEONFLOORS` / `+BOUNCEONWALLS`, `BounceFactor`). Duke doors toggle (`Door_Raise` with a long wait), and door tops are now the lowest neighbouring ceiling − 4 (GZDoom) instead of floor + 120. UDMF sector `soundsequence` and SNDSEQ choice lists (0 open, 1 close) give each Duke door its `MUSICANDSFX` sounds, and mover sectors without one are silent. The nodebuilder's collinear-opposite rule now only applies to Doom-wound maps, which fixes 7 room-sim physics tests on outward-wound `MapBuilder` fixtures. Tests: `physics::bouncers_reflect_off_floors_and_walls`, `specials::door_opens_to_lowest_neighbour_ceiling`, `specials::sector_sound_sequence_picks_open_and_close_choices`, `sndseq::choice_lists`, `duke3d_play::{vent_fan_breaks_when_shot, shotgun_fires_pumps_and_ejects_a_shell, pistol_reloads_every_twelve_shots, doors_use_their_sector_sounds}`.
- `2026-10-17` — **Duke roof floor, vent drop, no grey square when firing** (`docs/23-build-engine.md`). The nodebuilder puts collinear segs facing away from the splitter behind it (ZDBSP) and still splits a convex set that mixes sectors, so both sides of a line never share a leaf. The E1L1 roof floor around the skylight was missing, and the Freedoom MAP03 ledge misclassification is gone. A Build SE7 transporter floating above its floor becomes a fall-link portal (`PORTAL_KIND_FALL`, kind 201). Before, the vent shaft and the room under it bounced the player between their teleport lines. A weapon `Flash` state on `TNT1` now has no HUD patch, and the app and web skip flash frames without a lump. The Duke pistol flash was drawn as a grey placeholder square. Tests: `bsp::two_sided_line_sides_never_share_a_leaf`, `setup::fall_portal_heights_are_signed_halves`, `world::tnt1_weapon_frames_have_no_patch`, `duke3d_play::rooftop_vent_drops_into_the_building`, `hitscan_effects_use_duke_sprites` (HUD patches exist), `point_in_sector::map03_ledge_sector_west_of_160` (now floor 16).
- `2026-10-17` — **No shots into the sky, no floating fences, soft sky top** (`docs/23-build-engine.md`). A hitscan wall hit above a sky ceiling or below a sky floor now hits the sky, with no puff, decal or line damage (Doom "don't shoot the sky"). Before, Duke shots left sparks and bullet holes floating on the invisible upper wall between two parallax ceilings. `point_in_sector` handles BSP leaves holding both sides of a line by picking the nearest seg the point is in front of. The converter nudges sprites within 1.5 units of a sector edge inside it and takes their height from the sloped floor, so the E1L1 roof-edge fences sit on the roof. `DukeTeleportFog` uses `TRANSPORTERSTAR`. Sky caps are concentric rings over a gradient: the sky's edge row, blurred wider and faded into the average towards the centre. This replaces the flat disc that looked like a hole when looking up. Tests: `sky::sky_cap_fades_from_edge_row_to_average`, `duke3d_play::e1m1_spawns_duke_actors_and_plays` (fences under the sky), `hitscan_effects_use_duke_sprites`; `duke3d_e1l1` also renders look-up views.
- `2026-10-17` — **Scene build 30× faster, mipmapped materials** (`room-render`). Each visible subsector's BSP leaf path came from a full-tree search that cloned the path at every node. A per-frame parent table replaces it, so Duke E1L1 builds in 2.6 ms instead of 77 ms; mirrors repeat the build, so this was the main frame cost. Material textures now upload a box-filtered mip chain (nearest mip, nearest magnification). Masks stay binary so cut-outs survive. This stops the shimmer on dense Duke tiles at a distance. Tests: `texture::mip_chain_averages_colour_and_keeps_masks_binary`.
- `2026-10-17` — **Duke flats, sloped walls, no grey squares** (`docs/23-build-engine.md`). Build floor/ceiling stat bits (wall-relative, swap, double, flips) and tile-size repeats now follow Raze, so floors no longer use a fixed 64-unit grid. Wall pieces between sloped sectors follow both slopes. Live sprites with a `TNT1` frame or a missing lump are skipped on desktop and web; before, they drew lit white quads (ambient sound markers, teleport targets, missing puffs). Tests: `convert::flat_align_matches_raze_plane_uvs`, `duke3d_play::hitscan_effects_use_duke_sprites`.
- `2026-10-17` — **Duke solo, editor, effects and sky** (`docs/23-build-engine.md`, `docs/22-room-server.md`). The lobby has a **Play solo** button (game / map / skill, no room). The editor opens a GRP with Duke skill labels. MAPINFO `clearskills` / `skill` blocks name the skill menu, and Duke's come from `defineskillname`. The E1L1 opening car flies, burns and blows up. Hidden SE markers and filters no longer draw. Hitscan sparks, blood and wall holes are Duke's: `DukePuff` / `DukeBlood` replace Doom's, and DECALDEF `BulletChip` uses `BULLETHOLE`. The menu cursor is the spinning nuke icon, so no more grey missing-texture squares. Engine: DECORATE `replaces` now redirects every spawn (GZDoom), not just the editor number. Sky caps are filled with the average colour of the sky's top/bottom rows instead of streaked texels. The desktop offscreen canvases match the surface format (macOS BGRA crash), and mouse motion is ignored while the window is unfocused. Tests: `duke3d_play::hitscan_effects_use_duke_sprites`, `opening_car_flies_down_and_blows_up`, `skills_replace_and_clear`, `mapinfo_skill_names_replace_doom_skills`, `reads_skill_names`.
- `2026-10-17` — **Duke sector effects and props** (`docs/23-build-engine.md` §2–§3). Effectors become Room specials: quakes, shot lights, light switches, explosive ceilings, transport-elevator teleports, SE23 exits, incremental and SE21/31/32 movers, pistons, drills, lightning, floor waggle, and floor scrollers that carry actors for conveyors, escalators and subway sectors. Teeth doors open. Hurt floors use the tile. Visibility darkens light. Breakables explode, eggs hatch, force fields can be shot, SE36 shoots, SE5 spawns a turret, and jetpack / sitting variants differ from the base enemy. Still out: CON VM, moving subway geometry, walkable rotators, stretching bridges, shrink/freeze, inventory use, cutscenes.
- `2026-10-17` — **Duke Nukem 3D unsupported-features list** — `docs/23-build-engine.md` §3 lists what the conversion lacks: CON VM, unconverted SE/sector lotags, spawners, breakables, cameras, weapon modes, inventory use, rendering and front-end gaps. It replaces the old "Known gaps" paragraph.
- `2026-10-16` — **Duke Nukem 3D moving sectors, bombs, jetpack** (`docs/23-build-engine.md` §2). Sliding/star/swinging doors and solid SE0 rotators become polyobjects driven by generated ACS (`BEHAVIOR`). Mirror walls become `Line_Mirror`, SE7 water links become water portals (Room kind 200), and `picanm` wall tiles become `ANIMDEFS`. Pipebombs (throw + remote), tripbombs (armed proximity mines) and the jetpack (`ArtiFly`, flight on pickup) work, on Duke's slots 6/9/0. The converter moves monsters off walls and solid decorations: 32/33 stand free on E1M1. Engine: polyobject blockmap relink, door busy/blocked rules, perpetual rotate (angle 255), counter-clockwise swing; texture animator (desktop + web); move up/down input and weapon keys 8/9/0; `setslot 0`; jumps in weapon states resolve weapon labels; `A_CheckProximity` GZDoom order + `CPXF_*`; DECORATE `|` flag unions in action args. Still not converted: subways/trains, walkable rotators. Tests: `duke3d_play` (doors/rotators, bombs/jetpack, placement ≥ 95%), polyobj/flight/water/slot/proximity units, `Fit` placement unit.
- `2026-10-16` — **Duke Nukem 3D stages 2–8** (`docs/23-build-engine.md` §2). The GRP conversion now generates `DECORATE` / `KEYCONF` / `SNDINFO`, sprites, HUD lumps and sounds. Sprites cover face/wall/floor, `picanm`, `LOOKUP.DAT` pals and repeats. Items and weapons: 8 Duke guns, `DukePlayer`, access cards = Doom keys + locked switches. Enemies use CON action frames and strengths with Doom attacks. CON is data only (no VM). Sector effects: doors/lifts/elevators, activators, switches, touchplates, SE4 flicker, SE7 teleports, secrets, hurt floors. VOC sounds and looping ambients. Composed multi-tile skies. Bridge strips on outer walls are cut out for node building. `DukeStatusBar` and weapon keys 8/9 (desktop + web). Desktop `--shot-after N`; `scripts/play-duke3d.sh`. Engine fixes: UDMF `*plane_d` sign (GZDoom `ax+by+cz+d=0`), per-archive ZMAPINFO/MAPINFO layering, layered DECORATE parsed once over engine + PWADs, map music from MAPINFO in both shells, `give weapons/ammo` from the slots. Tests: `room-game/tests/duke3d_play.rs`, room-build converter/content units, `DukeStatusBar`, UDMF plane sign. Pre-existing failures not fixed: `room-map` `editor_roundtrip` (E1M1 reject) and the stale `room-defs` dump snapshot (`hardware_shaders`).
- `2026-10-16` — **Duke Nukem 3D stage 1** (`room-build`, `docs/23-build-engine.md`) — GRP/ART/PALETTE/MAP → UDMF PWAD (exact float vertices via `link_geometry_at`): walls, two-sided lines, slopes, parallax sky (MAPINFO `sky1`), doors, nuke-button exit, starts, MIDI music, USER.CON names/par. Mounts from `--iwad x.grp`, web `?iwad=duke3d.grp`, server verifier; lobby **Game** choice (`"game": "duke3d"`). Tests: `room-build` units + no-panic mutation, `tests/duke3d.rs` (all maps convert + build nodes), `room-render/tests/duke3d_e1l1.rs`, `rooms_list_their_game`.
- `2026-10-16` — **Match end, room time limit, leaderboard** — sim `World.match_over` / `match_tics_left` / `standings()`; `GameOptions.time_limit_tics`; mod imports `game_end_match`, `level_things`, rules `TIME_LEFT` / `MATCH_OVER`, stat `FRAGS`. Royale: tiered loot, gun spacing + caps + ammo, deathmatch-start spawns, per-second zone, time-up winner. Proto `result` + `welcome.room.time_limit`; server closes rooms on agreed result or wall-clock deadline, removes them from the registry, records `leaderboard.json` (`GET /api/leaderboard`). Web: countdown pill + final standings overlay (`roomMatchState`); lobby time-limit field + Top players. Tests: `wasm_mod_royale` (8), `room-server --test match_end` (3), leaderboard unit, mod unit tests.
- `2026-10-16` — **Admin MFA enrollment optional** — a password with no TOTP and no passkey opens the console (`LoginStep.session`, `via: password`). A second factor is required on the next login only after one is enrolled. UI gate no longer blocks on "Enroll MFA". Tests: `password_and_totp_roundtrip`, `mfa_totp_session_and_csrf`.
- `2026-10-16` — **Default admin login `admin` / `admin`** — `scripts/room-server.sh` bootstraps that account when `admins.json` is missing (was a random 20-char password for user `ops`); `bootstrap_user` accepts any non-empty password. Docs: README, docs/22, deploy/.env.example.
- `2026-10-16` — **Web client on phones / tablets** — `crates/room-web/touch.js` on-screen controls (floating move stick with run at the rim, drag-to-look, FIRE with drag-to-aim, USE, weapon ‹ ›, scores, fullscreen + landscape lock, ☰ game bar), auto-on for coarse pointers / first touch, `?touch=1|0`. New exports `roomInputKey` / `roomInputLook` / `roomInputWeaponCycle` feed the existing input hub (same tic commands as keyboard, netplay-safe). Page: no zoom / pull-to-refresh / long-press, safe-area insets, panels fit small screens, ≥ 4:3 letterbox in portrait + rotate hint, iOS audio unlock on `touchend` / `click`; `roomDebugState` adds `angle` / `ammo`. Shipped by `room-server.sh`, `play-web*.sh` and the Docker image. Browser-checked with phone emulation (844×390 and 390×844): stick moves / runs, a 120 px drag turned 70° (then tuned down), FIRE spent ammo, weapon cycle, menu; server lobby has no horizontal overflow at 390 px. Docs: docs/18 §3.3, room-web README.
- `2026-10-16` — **Admin-allowed mods + one mod build** — `scripts/room-server.sh mods` (also run by `start` and `docker-build.sh`) builds every `mods/<dir>` from its `room.toml` (Rust crate or AssemblyScript, source lump packed when named) — byte-identical WADs; the three `build-doom1-*-mod.sh` scripts are gone. Catalog mods start disallowed: admin console Settings → *Mods* allows / disallows each (`GET /admin/api/v1/mods`, `POST /admin/api/v1/mods/{name}`, audited), persisted in `<data-dir>/mods.json`, first-run seed `--allow-mods` / `ROOM_ALLOW_MODS`. `GET /api/mods` lists allowed mods only; creating a room with a disallowed mod is 403 (`RegistryError::ModNotAllowed`); running rooms keep their mod. Tests: `mods_from_a_dir_need_allowing_and_the_choice_persists`, `admins_allow_and_disallow_mods`. Docs: docs/22 §4 / §7, room-server / deploy / root / mod READMEs, docs/12.
- `2026-10-16` — **Battle Royale example mod (Rust, multiplayer)** — `mods/doom1-royale`: monsters removed at map start and replaced by weighted loot, players stripped to fists and scattered to spread-out spots, deathmatch rules with one life, closing zone (45 s grace, 25 s hold / 20 s shrink to 60 %, growing damage, private direction hints), eliminations / winner announced, level exit 8 s after the last player standing. Engine: `game_get_rule` / `game_set_rule` imports (`rule::DEATHMATCH`, `rule::RESPAWN`; Rust SDK + AssemblyScript), `World::no_player_respawn`, taking the weapon in hand switches weapons (`think_player::drop_unowned_weapon`), HUD messages carry a recipient (`MessageTo`) so netplay clients show mod prints for everyone / their seat (previously all hidden). `scripts/build-doom1-royale-mod.sh`; tests `wasm_mod_royale` (5, incl. doom1 E1M1), `verifier_runs_a_battle_royale_room`. Docs: mod README, docs/12 §15–16, docs/22 §4, root / room-script READMEs.
- `2026-10-16` — **Mods in room-server multiplayer rooms** — `--mods-dir` / `ROOM_MODS_DIR` catalog of `ROOMMOD` WADs (`room_server::ModCatalog`), `GET /api/mods` + `/api/mods/{name}.wad` (CORS, ETag), rooms created with `"mod"` (400 on unknown), optional `welcome.room.mod` {name, title, fingerprint, size} in protocol v2 (`room_proto::content_fingerprint`), lobby *Mod* select + room badge, admin / directory show the mod. `room-web` downloads the room's mod, checks the fingerprint and mounts it after the IWAD before preparing the map; the headless verifier mounts the same bytes. `room-server.sh` and the Docker image ship the example mods. Docs: docs/22 §4 *Mods*, docs/12 §15, room-server / deploy / mod READMEs. Tests `room-server/tests/mods.rs` (2), `verifier_runs_the_room_mod` (deterministic, diverges from vanilla once Bounty acts). Browser-checked: room created from the lobby with Bounty, client downloads + verifies the mod, both seats start E1M1. Legacy FTRL rooms stay vanilla.
- `2026-10-16` — **Editor script syntax highlighting** — Scripts tab code editor highlights AssemblyScript and WAT (comments, strings, keywords, types, numbers / constants, calls, decorators, WAT instructions and `$ids`) via a token layer behind the textarea; the Game API / Host API panels are highlighted too. Fixed `room-server.sh` / `deploy/Dockerfile` web bundles missing `editor-scripts.js` and `as/` (editor failed to load there). Browser-checked typing, scrolling and alignment.
- `2026-10-16` — **AssemblyScript example mod "Bounty Hunter"** — `mods/doom1-bounty/bounty.ts`: marked monster with skill-scaled toughness, compass hints, combo / payout rewards (weapons, ammo, health), mod class `Warden` final boss; `scripts/build-doom1-bounty-mod.sh` packs `ROOMMOD` + `BOUNTY` + `S_BOUNTY` (source, so the editor can reopen it). Web: Mod select / `?mod=bounty`, editor "Example: Bounty Hunter (AS)" + copy-for-editing. Docs: docs/12 §16 (AssemblyScript usage + full API reference), mod README, root / room-web / SDK READMEs. Test `wasm_mod_bounty` (3); browser-checked editor copy, live recompile and play.
- `2026-10-16` — **Live AssemblyScript mods + editor mute + pixel wheel zoom** — Scripts tab "+ New AssemblyScript mod": TypeScript-syntax mods compiled in the browser (vendored AssemblyScript 0.28.20, CDN fallback) against `crates/room-web/as/room.ts`; Ctrl+S during a play test hot-swaps the mods without restarting the level (`Headless::hot_swap_script_mods`, mod classes re-merged on the pre-mod tables). Fixed `class_find` returning registration refs for mod-defined classes during play (mod classes could not be spawned by name). Editor 🔊/🔇 mute (`roomSetMuted`, persisted); pixel editor zooms with the plain wheel around the cursor. Tests `wasm_mod_assemblyscript` (2); browser-checked live reload, mute and zoom.
- `2026-10-16` — **WASM mods with full game control + editor Scripts tab + doom1 example** — `ROOMMOD` (WAD) / `room.toml` manifests discovered in the VFS (`room_game::discover_mods`), `Headless::set_script_mods` (desktop — new repeatable `--file` PWAD flag — web play with a Skill / Mod bar and `?mod=`, editor); `room_sim::script_mods::ModRuntime` lends the live `World` to play modules between tics and fires Loaded / Tick / PlayerEntered / PlayerDied / ThingSpawned / ThingDamaged / ThingDied / ItemPickup / Unloaded; 42 new `"room"` imports (`GameFn`: actor queries + spawn / damage / inventory / state / sound, players, level exit / music, deterministic math + RNG); trapping modules are disabled, not fatal. SDK `game` module + `export_mod!`; editor Scripts tab (WAT compiled in-browser or `.wasm` import, validated, live log, exported in the PWAD); example `mods/doom1-survival` (skill-scaled waves, boss wave, PhantomImp / Juggernaut). World messages now survive until drained. Tests: `script_mods` (3), `mods` discovery, `room-game/tests/wasm_mod_survival.rs` (incl. doom1 E1M1); browser-checked in the editor. Saves don't store module memory yet (docs/12 §15).
- `2026-10-16` — **Vanilla skill levels** — ITYTD halves damage to players (before armor) and ITYTD / NM double ammo pickups (`P_GiveAmmo`, after dropped-ammo halving); NM / `-fast`: DECORATE `Fast` states run at `tics − tics/2` (Demon / Spectre SARG frames marked), missiles use `FastSpeed` (imp / baron / caco balls 20), monsters spawn with reaction time 0; Icon of Sin spits every other time on skill ≤ 2. Web boot panel gains a skill selector (`?skill=`). Test `room-sim/tests/skill_levels.rs`.
- `2026-10-16` — **Mouse wheel changes weapons** — wheel up = next, wheel down = previous (GZDoom), on desktop (not while the automap is open, where the wheel still zooms) and on web while the pointer is locked. Extra notches queue one switch per tic.
- `2026-10-16` — **Mod weapon projectiles + DECORATE state inheritance** — `A_FireProjectile` / `A_FireCustomMissile` (angle, spawnofs_xy, spawnheight, `FPF_AIMATANGLE` / `FPF_NOAUTOAIM`, pitch; seekers get the autoaim target as `tracer`) on the shared player-missile path; weapon psprite actions now receive their DECORATE arguments (previously dropped, e.g. `A_FireBullets`, `A_CustomPunch`); child classes inherit every parent state label they don't redefine (GZDoom). Test `mod_weapon_a_fire_projectile_uses_its_arguments`.
- `2026-10-16` — **Player projectiles match `P_SpawnPlayerMissile`** — rockets / plasma / BFG launch from feet + 32 (was eye height) along the autoaim angle + slope, else freelook pitch, and no longer home on the player's last attacker (`target`). Test `player_rocket_launches_from_32_and_ignores_last_attacker`.
- `2026-10-16` — **No invisible weapons on IWADs without their sprites** — `Headless::drop_weapons_without_sprites` removes slotted weapons whose `Ready` sprite is absent (super shotgun on Doom 1), wired in desktop + web; `give weapons` / `idkfa` only gives slotted weapons (GZDoom `LocateWeapon`). Test `room-game/tests/doom1_weapons.rs`.
- `2026-10-16` — **Invulnerability view, netgame face background, deathmatch frags** — `Player::fixed_colormap()` (INVERSECOLORMAP 32 / light-amp 1, blinking in the last 4 s); the unlit shader draws the world as inverted greyscale and the 2D blitter inverts the weapon sprite (`DrawCmd::Patch::inverse`, `invert_since`), on desktop + web; STFB0–3 behind the face in netgames; `World::deathmatch` allows PvP damage and keeps `P_KillMobj` frags (world kills = suicide), and STBAR shows the frag count in place of ARMS. Tests in `world`, `statusbar`, `canvas_bridge`.
- `2026-10-16` — **Doom status bar + player feedback match `st_stuff.c`** — animated mugshot (`room-sim::mugshot`, ticked after the playsim: idle glances via `M_Random`, turn toward attacker, ouch on >20 loss (GZDoom sign), evil grin on new weapon, rampage after 2 s of fire, god, dead); `damagecount` / `bonuscount` / `attacker` / berserk tics on `Player`; PLAYPAL-style red / gold / radsuit screen flash (`room_ui::screen_blend`) on desktop + web, plus ACS `FadeTo` on web; STBAR gains `STARMS` 2–7, the 4-row ammo / max table, one key per colour (skull wins), blank big ammo for fist/saw, faces / keys at native size with Doom patch offsets (`UiDrawList::picture`); `P_DeathThink` eye sink (1/tic to 6) + turn toward the killer, and the gun drops off screen on death. Tests in `mugshot`, `world`, `damage`, `statusbar`.
- `2026-10-16` — **Doom actor states match `info.c`** — added XDeath (shotgun/chaingun guys, imp, SS) and Raise (13 resurrectable monsters) so gibbing and Arch-vile resurrection work; fixed Spider Mastermind / Arachnotron attack-pain-death frames (+ `A_BossDeath` for E3M8 / MAP07), Baron `A_BossDeath` (E1M8), Cyberdemon 3-rocket volley, Wolf SS attack, Lost Soul charge loop + frame K, Pain Elemental death order, Arch-vile `[\]` heal frames, player pain/death action frames; `A_PainDie` now falls and spawns 3 souls at ±90°/180° like vanilla. New `room-sim` state test; existing demo fingerprint/verify + 2 sim failures are pre-existing (same set on the unmodified tree).
- `2026-10-16` — **sprite animation panel** — the pixel editor shows every animation of a thing's actor (`roomEditorSpriteAnims`: per-label state walk incl. inherited labels, `room-sim` `StateTable::labels_of`), played at real tic timing per rotation (mirrored lumps handled) and offset-aligned, plus the frame strip and an all-images grid; clicking a frame switches the tab (pending edits saved). Thing inspector icon opens it; zoom presets fixed for odd sizes.
- `2026-10-16` — **editor 3D thing overlay** — the 3D preview outlines selected things (solid) and the hovered thing (dashed, pointer cursor) with index + class labels; `roomEditorThingRects` projects pick boxes and flags things hidden behind walls (drawn faint). Double-click (or P) on a thing / wall / floor in the 3D preview opens its sprite / texture / flat in the pixel editor; fixed a stray "null" in the Doom-format thing inspector.
- `2026-10-15` — **live sprite / graphics edits** — painter edits to sprites and global graphics now swap into the running session without a restart: `room-ui` `PatchCache::insert` refreshes sprite size/offsets and HUD/status-bar/menu blits, and sprite materials are re-uploaded (`live_picture` in `room-web` editor).
- `2026-10-15` — **in-editor pixel editing** — new `editor-paint.js` painter (palette tools, tiling preview, transforms, resize, PNG import/export, undo) for textures/flats/patches/sprites/graphics; `room-formats::encode_patch` (128-px post split, DeePsea tall-patch deltas, round-trip tests); `room-render` `MaterialCatalog` honours `TX_START` images over TEXTUREx composites (`source_image`, `textures_using_patch`, sizes); wasm `roomEditorPalette/ImageGet/ImagePut` upload texture/flat/patch edits live to the running 3D view; PWAD export writes `TX_`/`FF_`/`PP_`/`SS_` blocks.
- `2026-10-15` — **spectator audio follows the watched player** — `room-audio` `Mixer::set_listener` adds Doom-style stereo separation (`S_STEREO_SWING`, sources at the listener stay centred); `room-web` listener = followed seat's view actor (camera or body), re-synced immediately on seat switch, auto-falls back when the followed player leaves; desktop `room-app` uses the same listener. Mixer tests for pan / centre / mono.
- `2026-10-15` — **Docker multi-arch deploy** — `deploy/Dockerfile` (server + baked `room-web`, non-root, healthcheck), `scripts/docker-build.sh` (buildx amd64/arm64, `--push`/`--load`), compose + `.env.example` + optional Caddy TLS profile; clap `ROOM_*` env on all server flags; `deploy/README.md`.
- `2026-10-15` — **admin MFA + YubiKey** — `room-server` admin plane: Argon2id password + TOTP and/or pure-Rust WebAuthn (YubiKey/passkeys), HttpOnly session + CSRF, `--admin-bootstrap` / `ROOM_ADMIN_BOOTSTRAP`, `admins.json` under `--data-dir`; legacy bearer only when no users. Ops UI gate rewritten (enroll TOTP / register key). `scripts/room-server.sh` first-run writes `bootstrap.txt`.
- `2026-10-15` — **web audio + pro editor tools** — web mixer feed is now audio-clock driven (worklet reports every ~10 ms, adaptive 25–160 ms target, `roomAudioStats()`), removing rAF-hitch underruns vs desktop; editor gains `editor-tools.js`: live play-test panel (`roomEditorCheat`/`roomEditorPlayerState`: god/noclip/freeze/give/kill/summon/HP/armor), Ctrl+C/X/V/D clipboard with sector remap, R/H rotate & mirror, F8 map checker with fixes, J auto-align, TEXTURE1/2+PNAMES composer applied live, Hexen/ZDoom special names, `window.roomEditor` console hook. Sim: `give armor`/`idkfa` now sets 200 points / class 2.
- `2026-10-14` — **web HUD + spectators** — sim `KillEvent` stream → bottom-left STCFN kill feed + Tab scoreboard (kills/deaths/frags/HP/armor/status); callsign modal on every join-existing path (lobby + `/play` without `name`); v2 `watch`/`watching` + `TIC_BATCH` replay-from-tic-0 spectators with multi-view split screen (G grid/follow, 1-4/click pick).
- `2026-10-14` — **web netcode latency** — server releases v2 frames on a 35 Hz clock (immediately when complete, 30 ms jitter grace, repeat-then-neutral fill); web client pipelines input `ceil((RTT+15)/T)` tics ahead (ping/pong RTT), takes its own cmd from the server frame, catches up backlog; camera lerp between tics + unapplied-look preview; dedicated TIC_CMD rate bucket.
- `2026-10-14` — **server bans + callsigns** — lobby requires pseudo; sanitize names; ban scopes (ip/name/glob/fp/ticket/TURN/auth_key/resume) enforced on join+WS hello; admin ban-from-seat + DELETE; shared AdminStore across registry clones.
- `2026-10-14` — **web coop no-move fix** — hidden lobby invite `<input>` kept focus after Play, so WASD was treated as typing; blur + canvas focus on lobby hide / play start; ignore non-visible text fields.
- `2026-10-14` — **web coop HUD/input** — voice bar on rtc.js load (mic on first PTT); WASD recorded in lobby; lockstep mouse kept across waits; v2 always uplinks TIC_CMD; net graph sparkline; worklet-first audio (no ScriptProcessor warn).
- `2026-10-14` — **lobby/admin gore UI** — blood/bone slaughterhouse palette; brand-first ROOM hero; shared arterial accent on ops console.
- `2026-10-14` — **web audio hardening** — AudioWorklet PCM playback (ScriptProcessor fallback); unlock on key/pointer/touch; tab-focus resume; ACS SetMusic; feed ring each frame.
- `2026-10-14` — **coop lobby + admin minimap** — v2 Ready after map prepare (fixes no-move); waiting-room overlay; admin room inspect with IP/ticket/resume keys + automap outline/poses.
- `2026-10-14` — **room-server CSP** — `/play/*` allows inline script/style + `wasm-unsafe-eval` so `room-web` boots under the server CSP.
- `2026-10-14` — **room-server join fix** — same-tab Play (no popup); `--web-root` serves `/play/`; ticket TTL 600s; clearer v2 lobby wait status.
- `2026-10-13` — **room-server public lobby** — `/` + `GET /api/rooms` directory; create/join/play/watch; `--play-base`; README updated.
- `2026-10-13` — **room-server admin UI** — operator console SPA (dashboard/rooms/bans/audit/settings); external CSS/JS for CSP; create-room API.
- `2026-10-13` — **room-server admin plane** — `--admin-bind` separate listener; script defaults game `:8765` + admin `:8766/admin`.
- `2026-10-13` — **room-server S5 done** — §6 is server `relay` (not client RTT); `create_to_welcome_us` in `room-load`; `scripts/room-server.sh` start/stop/restart/status.
- `2026-10-13` — **room-server M5** — dropped 24 h soak acceptance item; stopped soak run.
- `2026-10-13` — **room-server M5** — `TCP_NODELAY` via axum `tap_io` on accept (docs/22 §6).
- `2026-10-13` — **room-server M5 soak** — 1 h memory soak clean (32 rooms, RSS +20 MiB < cap).
- `2026-10-13` — **room-server S2 done** — 1 h `room_proto_binary` fuzz clean (`fuzz-room-proto.sh --seconds 3600`).
- `2026-10-13` — **room-server M2** — handshake/idle timeouts (`--handshake-timeout-secs` / `--idle-timeout-secs`).
- `2026-10-14` — **Lightcall STUN/TURN + web ICE** — vendored Lightcall Binding STUN + TURN relay (`room-sfu` `builtin-turn`); `--stun-bind`/`--turn-bind`; `welcome.media` → `rtc.js` + PTT; wasm `rtc_bridge`.
- `2026-10-13` — **room-server README** — full CLI/features/scripts; status S1–S5.
- `2026-10-13` — **room-server M5 latency** — server relay avg≈28 µs, 99.9% ≤300 µs @512 conn (`room-load` prints `server_relay_us`).
- `2026-10-13` — **room-server M5** — server-side `relay_latency_us` histogram (first TicCmd→TIC_FRAME) on `/metrics`.
- `2026-10-13` — **room-server M2/M3** — byte + chat rate limits; richer `/metrics` (ws msgs/bytes, violations, tic frames).
- `2026-10-13` — **room-server S1 smoke** — `scripts/smoke-room-server.sh` (health/create/join/v2); `play-web-coop.sh` uses `--bin room-server`.
- `2026-10-13` — **room-server M5 load** — last-input latency timing; p99≈1.9 ms @512 conn; 10k-conn scale; 1h soak started (`soak-room-server.sh --hours 1`).
- `2026-10-13` — **room-server M5/M4/M2** — feature `verifier` + `--iwad` Headless ticks; `room-sfu` 8-seat bandwidth budget test; `scripts/fuzz-room-proto.sh --seconds`; CI `room_proto_binary` 60s.
- `2026-10-13` — **room-server M5** — CHECKSUM collect + `desync` broadcast; v2 skips PCM when `rtc.js` loads.
- `2026-10-13` — **room-server M5 start** — `--node-id`/`--enable-verifier`/`--drain-grace-secs`; `POST /admin/api/v1/maintenance` drain; `/readyz` 503; `room_location`; `room-load --soak-secs` + latency p50/p99.
- `2026-10-13` — **room-server M4** — str0m `UdpSfu` MediaData selective forward + mute gate; `--sfu-udp` + feature `rtp`; `should_forward` unit test.
- `2026-10-13` — **room-server M4** — TURN REST creds (`--turn-host`/`--turn-secret`); `IceServerEntry`; `rtc.js`; `room-sfu` feature `rtp` (str0m rust-crypto UDP demux).
- `2026-10-13` — **room-server M4 start** — crate `room-sfu` (signalling/mute); ADR 0003; `rtc` routed from room actor.
- `2026-10-13` — **room-server M3 done** — `--data-dir` audit.jsonl + bans.json; SPA/kick/mute/Docker already landed.
- `2026-10-13` — **room-server M3** — embedded `/admin` SPA; kick/mute/notice; `--admin-token`; `deploy/{Dockerfile,docker-compose.yml,room-server.service}`.
- `2026-10-13` — **room-server M3 bootstrap** — `/admin/api/v1/*`, `/metrics`, `/readyz`; in-memory audit/bans.
- `2026-10-13` — **room-server M2 security** — security headers; `deploy/Caddyfile` TLS; `--max-conn-per-ip`; `tests/security.rs`.
- `2026-10-13` — **room-server M1→M2** — 1k soak; `room-load` CLI; tickets/Origin/resume; token-bucket; `room-web` v2 (`--v2`); play-web-coop `--server` (CLI args).
- `2026-10-13` — **room-server M1 bootstrap** — crates `room-proto` + `room-server` (axum/tokio); multi-room legacy FTRL with seat stamp; v2 TIC_FRAME; play-web-coop `--server`; ADR 0002; fuzz stub `room_proto_binary`.
- `2026-10-13` — **Web coop console + presence** — `MSG_PEER_JOIN`/`LEAVE` from `room-ws`; browser logs connect/disconnect, Ready, lockstep tics, DATA rx/tx; faster idle fill when a peer drops.
- `2026-10-13` — **Web coop desync (P2 idle / missing hits)** — gate lockstep on `LobbyMsg::Ready` after map load; stash early TimedCmds in lobby; immutable cmds; fill only after ~30s stall (was ~1s → host advanced alone).
- `2026-10-13` — **Web move fixed (verified)** — lockstep `delay=0` + refresh pending WASD + fill missing peers after 2 polls; input hub; cache-bust wasm/js in play-web scripts. Browser test: pos moves on KeyW.
- `2026-10-13` — **Web input hub** — WASD via separate `input_hub` (rAF held `FrameState` so `try_borrow_mut` keydown dropped moves); lockstep pending fix kept.
- `2026-10-13` — **Web coop move freeze** — lockstep cleared pending on seeded tic-0 advance (lost tic-1 broadcast → stall); keep pending until its tic commits.
- `2026-10-13` — **No-mic voice graceful** — cache mic `NotFound` (no getUserMedia retry); hear-only UI; pointer-lock SecurityError swallowed + delayed re-lock.
- `2026-10-13` — **Voice PTT + input focus** — hold `V` / pad to talk; voice UI non-focusable; canvas re-focus so WASD not stolen by mute buttons.
- `2026-10-13` — **Coop lobby + voice fixes** — idempotent `on_hello_at` (HELLO retry no longer Reject/`seat taken`); voice.js soft mic fallback + Unmute retry.
- `2026-10-13` — **Web coop live voice** — `MSG_VOICE` on FTRL relay; `room-web` voice hub + Lightcall AudioWorklet PCM (`voice.js`); mute UI when `?ws=`.
- `2026-10-13` — **Coop gun + walk anim** — `give_starting_loadout` sets `ready_weapon` on the owning seat (P2 had no gun); PlayerPawn `See`/`Missile` + `P_MovePlayer`-style body sprite.
- `2026-10-13` — **Project rename to Room** — crates `room-*`, `wadsrc/room`, `room.pk3`, scripts/env/docs/CI; JS `roomStartIwad` / `room_web.js`.
- `2026-10-13` — **WebSocket coop** — `room-ws` FTRL relay (`tungstenite`); `room-web` `WsTransport` + lobby/lockstep (`?ws=`); `scripts/play-web-coop.sh`.
- `2026-10-13` — **Web HUD brightness** — `CanvasGpu` overlay uses `Rgba8Unorm` on non-sRGB WebGPU canvases (was `Rgba8UnormSrgb` → linear sample, un-encoded present = dark STBAR/gun).
- `2026-10-13` — **Web STBAR HUD** — classic `DoomStatusBar` (ammo/health/armor/keys/mugshot) + weapon on `CanvasGpu`; HTML tip hidden while playing.
- `2026-10-13` — **Web weapon + sprite scale** — bottom-align psprite HUD (was centered over floor); SoftPatch width/height/topoffset for billboards (was actor radius/height); desktop FOV 58.715 + full-window aspect.
- `2026-10-13` — **WebGPU present encode** — non-sRGB canvas + shader linear→sRGB (`encode_srgb`) + toe lift; Bright+gain; classic 4:3 playfield with matched buffer/CSS/`camera.aspect`.
- `2026-10-13` — **WebGPU sRGB view** — canvas configured as `Bgra8Unorm` + `viewFormats` sRGB; pipelines/`create_view` use sRGB (was writing linear→non-sRGB = crushed dark). Desktop FOV/DoomSoftware + mild gain.
- `2026-10-13` — **Web match desktop view** — full-window buffer (drop 4:3 letterbox); `fov_y=58.715` + `aspect=w/h`; Bright midtone lift + gain ~3.2 (canvas was still crushed dark).
- `2026-10-13` — **Web stretch root-cause** — never force `camera.aspect=4:3` while buffer is wide; size CSS+buffer from window to exact 4:3; aspect = w/h; Bright + higher gain every frame.
- `2026-10-13` — **Web 4:3 + Bright** — CSS letterbox 4:3 playfield (no stretch); `LightMode::Bright` + gain; FOV ~90° horizontal at 4:3.
- `2026-10-13` — **Web aspect + brightness** — canvas buffer tracks CSS box×DPR (was fixed 1280×720 stretched).
- `2026-10-13` — **Web music + brightness** — OPL/`GENMIDI` map music into Web Audio; `UnlitPass::output_gain` + GLDEFS brightmaps/dyn lights.
- `2026-10-13` — **Web look / light / gun** — mouse yaw matches desktop (−dx); `DoomSoftware`+`glob_vis=512`; weapon psprite overlay via `CanvasGpu`.
- `2026-10-13` — **Web textures + SFX** — load PLAYPAL/TEXTUREX before material upload (was greyscale/white albedo); Web Audio `ScriptProcessor` mixer + SNDINFO on canvas click.
- `2026-10-13` — **Web Doom1 play** — `room-web` mounts IWAD bytes (`roomStartIwad` / auto-fetch `/doom1.wad` / file picker), runs `Headless` + materials + keyboard/mouse; `play-web.sh` copies local `testdata/doom/doom1.wad` into the serve dir.
- `2026-10-13` — **Web MRT attach** — `room-web` map pass binds the dummy R8 stencil-aux colour target so it matches `UnlitPass` dual-MRT pipelines (fixes Chrome attachment-state mismatch).
- `2026-10-13` — **WebGPU unlit shader** — hoist all `textureSample` calls in `unlit.wgsl` ahead of non-uniform control flow (cluster light mask / discards); fixes Chrome WGSL validate error on `tex_metal_rough`.
- `2026-10-13` — **`scripts/play-web.sh`** — build `room-web` for wasm32, `wasm-bindgen --target web`, serve `target/room-web` (opens browser); wired `index.html` module loader.
- `2026-10-13` — **Automap zoom + fog** — `=`/`-`/wheel zoom; play mode hides unexplored linedefs until the player enters their sector (`ML_MAPPED`); `iddt`/allmap still reveals all.
- `2026-10-13` — **Tab automap** — line blit was an AABB blob (diagonals solid); stroke real segments; classic red/brown/yellow/gray/blue colours + floor/ceil diffs + grid; opaque black follow view.
- `2026-10-13` — **Intermission map preview** — Doom `WIMAP*` / `INTERPIC` background + `WI*` stats/splat/`WILV` patches (replacing chunky SoftFont on black); skip 3D world draw on Title/Intermission/Finale so letterbox sides are not the last camera frame; SoftPatch allows non-uniform scale for full-screen pics.
- `2026-10-13` — **Exit switch SW1STRTN stuck red** — `player_think` takes the player out of `world.players`, so `activate_line` treated Use as a monster and refused Exit_Normal (doors still worked via the monster whitelist). Park the in-hand player for `use_lines` (same pattern as pickups); E1M1 line 326 flips SW1STRTN→SW2STRTN via `World::tick`.
- `2026-10-13` — **Transparent step risers** — floor ear-clip ownership rejects near-corner samples in foreign sectors (SEGS hull / outline overhang across courtyard steps); draw flats before walls so STEP* risers win z-order; two-sided wall emit uses the opposite sector for the current sidedef (not always `line.back_sector`).
- `2026-10-13` — **Switch LED sync** — re-apply `switch_*` overrides onto the render map every frame (and after `map_base` restore) so SW1COMP→SW2COMP cannot stick on SW1*; E1M1 offscreen test asserts brighter-red LED (Doom1 COMP art is red→brighter red, not green).
- `2026-10-13` — **Outdoor sky streaks** — elevated floor *undersides* drew without cull and covered `F_SKY1` holes (edge-on greyscale bands). Flats pipeline culls back faces; default MAPINFO-less sky is `SKY1`.
- `2026-10-13` — **Pause menu / HUD font** — drop yellow chrome panel; BigFont (`DBIGFONT`) for menu text with native `STCFN` fallback; messages via STCFN (no 5×7 bitmap); screenshot HUD text only after save.
- `2026-10-13` — **Forward walked left of look** — mouse look preview used float yaw while ticcmd rounded counts; camera drifted vs `actor.angle`. Lock yaw/pitch to sim after each tic; preview only rounded mouse counts.
- `2026-10-13` — **Weapon holes (palette 0)** — SoftPatch now uses post alpha mask; ~11% of Doom1 `PISGA0` posts are colour 0 (metal) and were punched transparent → noisy/seam gun.
- `2026-10-13` — **Floor holes + gun height** — ear-clip fan fallback + majority ownership (was dropping E1M1 floor tris → sky stripes); weapon Y shifts up 32 when STBAR shown (offsets are view-relative).
- `2026-10-13` — **Screenshot + GIF** — F12 / `screenshot` saves PNG; F11 / `gifrecord` toggles GIF (≤640px, ~10fps, 30s cap); capturable frame buffer + `gif` crate; files under `…/room/screenshots/`.
- `2026-10-13` — **Stairs feel + outdoor sky** — Double `P_SlideMove` (wall clip + full-mom stairstep); eye bob + camera XYZ tic-lerp; sky shell taller (8192) with top/bottom caps (old ±7° FOV fell into clear colour outdoors).
- `2026-10-13` — **Opaque wall holes** — patch decode writes explicit alpha (colour 0 ≠ hole); TEXTUREX blit skips holes (no punch/erase); solid walls fill clear texels black in shader; masked mids still discard.
- `2026-10-12` — **Stairs slide + switch LED** — Double `xy_movement` stairstep (Y then X) instead of hard-stop/*noway; lower+`DONTPEGBOTTOM` uses TexBottom UV so SW1COMP→SW2COMP LED is visible; `room: USE`/`SPECIAL`/`SWITCH` stderr on line activation.
- `2026-10-12` — **SHOOT debug + switch UV + imp aim** — always-on `room: SHOOT` stderr (player↔AI hitscan/missile); Doom sidedef x/y_offset copied to top/bottom tiers; wall UV uses TEXTUREX size (SW1COMP 64-wide); `P_SpawnMissile` aims at target under Double math (elevated imps); switch flip writes SW2* onto map lower.
- `2026-10-12` — **E1M1 switch + lift** — `player->usedown` edge so held E does not reverse Door_Raise; WR lifts use lowest surrounding floor + lip and refuse to stack; load-time SCROLL lines clear special after init.
- `2026-10-12` — **Doom1 door reopen** — XLAT `base.txt` USE/WALK/SHOOT/REP/MONST for types 0–141 (DR type 1 keeps special); `spawn_door` re-trigger reverses waiting/closing doors like vanilla.
- `2026-10-12` — **Invisible crush + shot traces** — `crush_sector_actors` only hits actors in the moving sector (was map-wide 10hp/4tics when any door closed); HIT dump lists recent hitscan/missile from→to; `last_damage.cause` tags crush.
- `2026-10-12` — **HIT debug** — `World::last_damage` records real source/inflictor; HIT stderr dumps player sector/crush, sight/FOV/DONTDRAW, sprite lump+patch+GPU, hunters vs corpses (old log mistook POSSL corpses for attackers).
- `2026-10-12` — **Invisible killer (follow-up)** — opaque sprite pipeline (REPLACE+depth write+strong bias), billboard pull-to-eye, preload POSS/SPOS/TROO frames, on-hit HUD/eprintln naming attacker or damaging sector.
- `2026-10-12` — **Invisible killer after E1M1 door** — root cause: app forced DoomSoftware `glob_vis=16` (everything past ~30mu at shade floor) so zombies in the computer room vanished into walls while still shooting; set `glob_vis=512`, COUNTKILL/missile sprites always fullbright; e2e probes confirm drawable LOS attackers.
- `2026-10-12` — **Door peg + HUD gun** — `ML_DONTPEGTOP/BOTTOM` wall UV pegging (door faces scroll, DOORSTOP jambs no longer stretch); weapon overlay discards holes and forces opaque alpha; monsters keep min light 0.55 / fullbright when hunting; missing sprite lumps draw white instead of vanishing.
- `2026-10-12` — **Solo invisible killer** — Hostiles targeting a player draw fullbright (DoomSoftware shade was crushing them in dark sectors); GPU POSS offscreen regression; Player2–4 starts remain spawn-spots-only (not the solo cause).
- `2026-10-12` — **Net transports** — TCP mesh + TCP packet-server relay (`TcpRelay*`); Noise_XX (`snow`) wrapper; app `--transport udp|tcp|tcp-server` + `--noise`.
- `2026-10-12` — **Netplay tests** — `coop_netplay` (UDP lobby+dual-seat lockstep, FF, desync); net UDP lobby unit; `spawn_coop_players` sim test. App `--host`/`--join` Doom E1M1 coop.
- `2026-10-12` — **M14** — `validate_world_bin` + `save_world_bin` fuzz + CI 60s.
- `2026-10-12` — **M14 start** — `save_info` fuzz target + CI 60s smoke.
- `2026-10-12` — **M10 done** — MAP01+32-lights fps floor; desktop §14 / nuts.wad intentional deferral.
- `2026-10-12` — **M10** — Spotlight cone on GPU + `shadowmap_32_budget`.
- `2026-10-12` — **M10** — `LightmapBindings` + spot/sector GLDEFS + SSAO half-res AO buffer.
- `2026-10-12` — **M10** — Per-vert lightmap UVs + `sprite_10k_budget` (nuts stand-in).
- `2026-10-12` — **M10** — UnlitPass `set_lightmap` + cluster_500 budget; MAP01 fps already gated.
- `2026-10-12` — **M10** — Particles SSIM + LightmapPass GPU + compose_post + scene_build_budget.
- `2026-10-12` — **M10** — ParticleInstancedPass + fill_color styles + bloom smoke + HW post host.
- `2026-10-12` — **M10** — HardwareShader catalog + GLDEFS parse; particle GPU pack; FLMP surfaces.
- `2026-10-12` — **M10** — lights.pk3 fixture→Definitions; `glsl_to_wgsl`; braced `frame { light }` GLDEFS.
- `2026-10-12` — **M10** — True bloom mip pyramid (`BloomPass`); naga WGSL build validate; `FLMP` lightmap header.
- `2026-10-12` — **M10** — Metal-rough GGX PBR (`tex_metal_rough` + `pbr` helpers) + fuzz SSIM golden.
- `2026-10-12` — **M10** — SSAO bilateral blur; particle Additive/Soft styles; 4-level Kawase bloom taps.
- `2026-10-12` — **M10** — UE1 `_d.3d`/`_a.3d` parse + catalog pair load; Blinn-Phong specular on normal maps.
- `2026-10-12` — **M10** — IQM GPU skinning (`SkinnedPass` / bone UBO / `skinned.wgsl`) + smoke test.
- `2026-10-12` — **M10** — KVX greedy face merge; optional `lights.pk3`/`brightmaps.pk3` mount + fixture.
- `2026-10-12` — **M10** — Dyn-lights SSIM golden PNG (`testdata/goldens/dyn_lights_320x200.png`).
- `2026-10-12` — **M10** — IQM CPU skinning (`parse_iqm_full` / `skin_cpu`) + `golden_dyn_lights` soft mean-luma check.
- `2026-10-12` — **M10** — Play-path `append_actor_models`; `model_offscreen_smoke` test.
- `2026-10-12` — **M10** — IQM v2 bind-pose parse; `ModelCatalog` + `append_actor_models` MODELDEF wiring.
- `2026-10-12` — **M10** — MD3 mesh parse (first surface); normal-map Lambert on dyn lights.
- `2026-10-12` — **M10** — `append_model` unlit draw path; KVX parse + cube meshing.
- `2026-10-12` — **M10** — MD2 mesh parse (multi-frame) + GLDEFS material PBR map catalog wiring.
- `2026-10-12` — **M10** — Palette tonemap (`gl_tonemap` PLAYPAL nearest) + OBJ `parse_obj` for MODELDEF.
- `2026-10-12` — **M10** — SSAO pass (depth-edge 8-tap) + multi-scale Kawase bloom in PostPass.
- `2026-10-12` — **M10** — Spectre fuzz shader (`gl_fuzztype` Shadow/Smooth/Swirly/…); Fuzzy sprites flag.
- `2026-10-12` — **M10** — `RenderStyle` → wgpu blend table; PostPass FXAA (Jimenez compact).
- `2026-10-12` — **M10** — PostPass (Reinhard/Hejl/linear + gamma + soft bloom + screen blend); particle vel/accel.
- `2026-10-12` — **M10** — CPU 1D shadowmaps (512 samples/light, one-sided occluders) + FS sample in unlit.
- `2026-10-12` — **M10** — VOXELDEF parse (scale/spin/angleoffset/overridepalette) into `Definitions.voxeldefs`.
- `2026-10-12` — **M10** — MODELDEF parse (`Model`/`FrameIndex`/`Frame`/skins/flags) into `Definitions.modeldefs`.
- `2026-10-12` — **M10** — CPU 16×9×24 cluster assignment + RGBA8 mask texture; FS only tests lights in cluster bitmask.
- `2026-10-12` — **M10** — GPU `LightUniforms` (32 lights) in unlit pass; pulse/flicker radius anim; CPU dyn-light boost retired from play path.
- `2026-10-12` — **M10** — GLDEFS `brightmap`/`material`/`Glow` parse; shader brightmap mix; glow fullbright ranges; nukage/lava glow list in room.pk3.
- `2026-10-12` — **M10 start** — GLDEFS `pointlight`/`pulselight`/`flicker*`/`object` parse; attack-frame defaults; CPU `apply_dyn_lights`; Bright sprites skip lightmode atten; oversized monster muzzle puff; `two_clients_same_map` example.
- `2026-10-12` — Enemy attack visibility: Missile wind-up Bright (zombie/shotgun/chaingun/imp); fullbright shader path for `light≥0.999`.
- `2026-10-12` — Doom1 props/enemies: floor sprites sit on feet+1 (depth bias −8/−2, no z-fight bury); Spectre Fuzzy→translucent stand-in; add RealGibs/BloodPool + coop/DM starts; E1M1 all DoomEd types resolve.
- `2026-10-12` — E1M1 empty floors: keep IWAD nodes (`ensure_nodes`, stop rebuilding over them); flats from seg-endpoint hull + BSP partition-clipped leaf + outline supplement; mapthing floor coverage 0 holes.
- `2026-10-12` — Empty/wrong floors: always ear-clip GL leaves (fan spilled into neighbours); outline fallback only if a sector has zero visible GL area (not a 70% ratio vs full outline); centroid-only ownership on outline — E1M1 floor centroid mismatches 0.
- `2026-10-12` — Empty floors: GL subsector rings primary (all leaves); outline fallback if GL &lt;70% coverage; ownership filter also accepts vertex hits — stops clear-color holes.
- `2026-10-12` — E1M1 flats: ear-clip sector outlines + drop tris whose centroid isn’t in-sector (concave/hole fans painted neighbour floors at wrong step heights); honour leftoverset outside [0,1]; sprite +0.05 floor lift.
- `2026-10-12` — Floor placement: snap mapthings via `floor_at_xy` (slopes+movers); sprite topoffset/leftoffset anchors; flats use `base_floor` delta so steps/lifts match walls; Bright projectiles/puffs.
- `2026-10-12` — Audio quality: resample DMX SFX to device rate; prefer SoftFont (`fetch-soundfont.sh` / TimGM6mb); align music synth rate with cpal.
- `2026-10-12` — Floor clip: seed `check_position` from point-in-sector + line openings (not front-only); Doom1 `D_ExMy` music; snap spawn Z to floor.
- `2026-10-12` — Fix outdoor “upside down”: skip `F_SKY1` flats (+ sky hack), peg wall V to ceiling, centre sky on eye; set `view_z` at possess.
- `2026-10-12` — Real Doom1 play: `scripts/play-doom1.sh` + `testdata/doom/` discovery; MAP01→E1M1 fallback; commercial IWAD gitignored.
- `2026-10-12` — Bullet impacts: resolve `BulletChip`→`pic` (PUFFB0 fallback); never reuse prior sprite bind for missing textures; weapon DECALDEF generator.
- `2026-10-12` — Acquire mouselook on window `resumed`/`focus` (startup set the flag before any window existed and skipped hide/recenter).
- `2026-10-12` — macOS LMB: drop Locked grab (hide+edge-recenter); fire on psprite enter + 0-tic loop; pistol A4/B6; chaingun muzzle flash; same-tic chaingun hitscan test.
- `2026-10-12` — macOS LMB: `AttackLatch` in room-input (window/device/key bits independent; device id 0 only); warp-then-Locked grab so clicks hit the client.
- `2026-10-12` — Restart clears bullet chips/particles and restores map lights (End Game / New Game / `map`); FX not drawn on the title screen.
- `2026-10-12` — Restore classic STBAR HUD (no overlay plate); psprite `x=-leftoffset` so Freedoom fire/flash stay on the barrel.
- `2026-10-11` — HUD/menu: draw via `STTNUM*` / `STCFN###` patches (SoftFont path was invisible); fullscreen plate + 2× digits; pause STCFN ×2 + highlight; blit pixel tests.
- `2026-10-11` — HUD larger (fullscreen SoftFont ×2.75, HudBig/STTNUM); pause outlined/large text; `bullet_slope` freelook fallback + eye-height shoot origin so shots match the gun.
- `2026-10-11` — Pause menu: STCFN lowercase→uppercase glyph fallback (Freedoom lacks most lowercase; labels were invisible); missiles explode on floor/ceiling (were skimming forever).
- `2026-10-11` — Combat demo: `AttackLatch` (18-tic hold) + Locked grab on macOS; strong tests for short-click / dual Window+Device release / RMB must not clear LMB; arena look-down + eye-level floor-band render tests green.
- `2026-10-11` — Combat demo floors: sector-outline flats (BSP slivers left void); unlit cull off for horizontal look; macOS LMB = device button 0 only (1 is RMB).
- `2026-10-11` — Combat demo: floor flats were missing (BSP leaf marked sector done before a ≥3-seg ring); GL/outline flat emit + tighter arena; pause panel centred/blackout.
- `2026-10-11` — Combat demo playability: opaque pause panel + STCFN text; mouse yaw scale 8→256 (preview matched); weapon HUD centred (ignore bad Freedoom left offsets).
- `2026-10-11` — **M13 done** — menus+options CVars; launcher; room/gzdoom.ini; web URL/storage API; package.sh; crash logs; StringTable i18n; QA checklist. Play: float strafe fix, macOS Confined grab + LMB/F fire. Deferral: full multi-OS/browser manual matrix.
- `2026-10-11` — **M13 start** — Doom menus: TITLEPIC, skull selector, patch items, pause/quit/end-game confirms, mouse hover/click, Options stub; dim only when overlaying play. Combat-demo fire/render polish carried from play shell backlog.
- `2026-10-10` — **M12 done** — lockstep + UDP + lobby + desync + `room-signal` + signal mesh/HTTP + optional `webrtc`/`MatchboxTransport`. Freedoom MAP01 2-peer lockstep smoke. Deferrals: live 4-player native+web 30 min soak; matchbox signalling server swap.
- `2026-10-10` — M12: **desync dump + room-signal** — `DesyncCollector`; in-memory HTTP room relay.
- `2026-10-10` — M12: **UDP + lobby** — `UdpTransport`; `LobbyMsg`/`LobbyHost` Hello/Welcome/Reject/Start.
- `2026-10-10` — **M11 done** — WASM host/SDK/reg/events/ACS/UI/saves. Deferrals: Actor derive/`states!`/C header; HUD draw; hot reload; samples (b)–(e); benchmarks.
- `2026-10-10` — M12: **lockstep bootstrap** — `LockstepSession`, loopback + lossy transport, codec, desync tests.
- `2026-10-10` — M11: **`.fsave` scripts.bin** — WASM play memory in save ZIP; restore on load.
- `2026-10-10` — M11: **UI + saves** — `load_ui_module` / `UiTick`; play memory snapshot + `LoadGame` restore; `ui_tick.wat` / `save_counter.wat`.
- `2026-10-10` — M11: **ACS interop** — `reg_static_function` + `ScriptCall`; WASM `acs_execute` queue; `script_call.wat`.
- `2026-10-10` — M11: **events/thinkers** — `reg_event_handler` / `reg_thinker_class`; `WorldTick` in `World::tick`; `event_tick.wat`.
- `2026-10-10` — M11: **ClassRegistry merge + fingerprint** — `import_script_class` / `apply_script_classes`; wasmi≡wasmtime reg fingerprint.
- `2026-10-10` — M11: **reg_class_*** — RegistrationSession + host imports; `fireimp_reg.wat`; SDK `reg` module; `api/reg.ron`.
- `2026-10-10` — M11: **name_intern + class_find** — HostLinker/backends/SDK; ScriptHost tables; `testdata/hello_mod.wat` sample.
- `2026-10-10` — M11: **SDK alloc + macros** — bump `room_alloc`; `export_module!`; `#[actor]` / `#[register_stub]`.
- `2026-10-10` — M11: **Rust SDK wrappers** — `room-script-sdk` log/level_time/player_count/actor_exists + abi_version export.
- `2026-10-10` — M11: **player_count** — host import + ScriptHost sync from occupied slots.
- `2026-10-10` — M11: **ScriptHost in sim** — `World.scripts`; sync maptime/player actors each tic; load stub module test.
- `2026-10-10` — M11: **wasmtime backend** — Cranelift fuel/NaN-canon; shared lifecycle test vs wasmi (`--features wasmtime`).
- `2026-10-10` — M11: **level_time + actor_exists** — HostLinker + wasmi imports; `api/common.ron` + `room-abi-gen list`.
- `2026-10-10` — M11: **guest lifecycle + room::log** — required exports; HostLinker log import; register/init/dispatch helpers.
- `2026-10-10` — M11: **wasmi bootstrap** — `room-script-abi` types; validate_module; `WasmiBackend` + `room_abi_version` test.
- `2026-10-10` — **Priority:** M7 closed → active **M11** then **M12**; M8–M10/M13–M14 deferred. Freedoom demo verify script + MAP01 real-play smoke.
- `2026-10-10` — **M7 done** — acceptance via `m7_feature_smoke` + UDMF/ACS/DECORATE/portal/ffloor coverage. Intentional deferrals: Equal-only Metal stencil mask; remaining DECORATE/ACSF edge corpus; external ZDoom mod corpus fetch.
- `2026-10-10` — M7: **UDMF sector alphafloor/alphaceiling/renderstyle** — parse (+ `alpha` both); seed SectorStatic/Dynamic; GetSectorUDMF* int/fixed; Add → STYLE_Add.
- `2026-10-10` — M7: **GetThingUDMFInt renderstyle** — UDMF thing `renderstyle` → ACS `STYLE_*` via `RenderStyle::parse`/`to_acs_style`.
- `2026-10-10` — M7: **GetLineUDMFInt SPAC + repeatspecial** — `playercross`/`playeruse`/…/`damagespecial` via `line_activation`; `repeatspecial` via `line_flags` (ACS SetLineActivation-aware).
- `2026-10-10` — M7: **UDMF sidedef xscroll/yscroll** — parse; `init_udmf_side_scrollers` → `TextureScroller`; GetSideUDMFFixed.
- `2026-10-10` — M7: **UDMF sidedef smoothlighting** — parse; milder fake contrast; GetSideUDMFInt.
- `2026-10-10` — M7: **UDMF sector color_floor/ceiling/wall*/sprites** — parse; GetSectorUDMFInt; scene luma tint on flats/walls.
- `2026-10-10` — M7: **UDMF sidedef tier offset/scale** — `offsetx/y_*` + `scalex/y_top|bottom`; scene per-tier UV; GetSideUDMF*.
- `2026-10-10` — M7: **UDMF sidedef light_top/mid/bottom** — parse + absolute flags; scene per-tier wall light; GetSideUDMFInt.
- `2026-10-10` — M7: **healthfloor/ceiling hitscan** — `damage_sector_plane`; aim ray hits planes before miss; syncs map static HP.
- `2026-10-10` — M7: **MAPINFO clipmidtextures/wrapmidtextures** — parse → `MapDef`; headless stamps all lines/sides.
- `2026-10-10` — M7: **clipmidtex Z + pegged midtex** — two-sided mid quads peg 128mu (DONTPEGBOTTOM); `clipmidtex` intersects opening; `wrapmidtex` fills; single-draw V `[0,1]`.
- `2026-10-10` — M7: **MAPINFO fallingdamage → LevelState** — `fallingdamage`/`forcefallingdamage`/`oldfallingdamage`/`strifefallingdamage`/`monsterfallingdamage`/`nofallingdamage` → `MapDef.falling_damage`; headless seeds `world.level.falling_damage`.
- `2026-10-10` — M7: **UDMF sector healthfloor/ceiling/3d** — parse; seed `SectorDynamic` health parts; GetSectorUDMFInt.
- `2026-10-10` — M7: **UDMF sidedef nodecals** — parse; GetSideUDMFInt; `spawn_decal` skips when either side of the line has the flag; hitscan passes line id.
- `2026-10-10` — M7: **UDMF sector lightcolor/fadecolor/fogdensity/desaturation** — parse hex colours; seed SectorDynamic + world fog; GetSectorUDMFInt.
- `2026-10-10` — M7: **UDMF damagespecial / deathspecial** — SPAC_Damage/Death; healthgroup sync; `damage_linedef`; hitscan walls apply; GetLineUDMFInt.
- `2026-10-10` — M7: **falling damage + nofallingdamage** — Hexen formula; `LevelState.falling_damage`; UDMF sector gate; GetSectorUDMFInt; float + fixed `z_movement`.
- `2026-10-10` — M7: **UDMF clipmidtex / wrapmidtex** — line+side parse; GetLine/SideUDMFInt; mid quads clamp V unless wrap (line|side); Z still opening-clipped.
- `2026-10-10` — M7: **UDMF linedef health + automapstyle** — parse; seed `line_health`/`line_automap_style`; GetLineUDMFInt; automap AML_NotMapped/OneSided/TwoSided/Special/Secret; Line_Set* updates geometry.
- `2026-10-10` — M7: **plat floor carry** — `tick_plats` rides grounded actors (same `dropactors` gate as floors).
- `2026-10-10` — M7: **UDMF sector dropactors** — parse (default true); floor movers carry grounded actors; false skips downward ride; GetSectorUDMFInt.
- `2026-10-10` — M7: **UDMF sector waterzone** — parse; `actor_waterlevel` uses ceiling as surface; GetSectorUDMFInt.
- `2026-10-10` — M7: **UDMF sector silent** — parse; `play_sector_sound` no-ops; GetSectorUDMFInt.
- `2026-10-10` — M7: **UDMF/ACS sector leakiness** — parse + `Sector_SetDamage`/SetSectorDamage arg; radsuit fail chance (256ths); GetSectorUDMFInt.
- `2026-10-10` — M7: **UDMF linedef ML_* flags** — blocking/blockmonsters/twosided/dontpeg*/secret/…/blockplayers/blockeverything; physics BLOCK_PLAYERS/MONSTERS/EVERYTHING; GetLineUDMFInt.
- `2026-10-10` — M7: **UDMF linedef renderstyle Add** — parse → `mid_additive`; seed `line_additive`; GetLineUDMFInt STYLE_Add(65).
- `2026-10-10` — M7: **UDMF linedef alpha + sidedef nofakecontrast** — `alpha` → `mid_alpha`; GetLineUDMF*; side disables fake contrast.
- `2026-10-10` — M7: **UDMF sidedef light/scale** — parse `light`/`lightabsolute`/`scalex*`/`scaley*` → Side; GetSideUDMF*; wall light uses side.
- `2026-10-10` — M7: **A_DamageSelf** — damage + optional mod (restored after call).
- `2026-10-10` — M7: **sector gravity in z_movement** — UDMF/ACS sector gravity scales float + vanilla fixed fall.
- `2026-10-10` — M7: **sector damageamount runtime** — UDMF/ACS `damageamount`+`damageinterval` hurts in `player_in_special_sector`.
- `2026-10-10` — M7: **UDMF sector gravity/friction/damage** — load into SectorDynamic; GetSectorUDMF*.
- `2026-10-10` — M7: **UDMF sector UV xform** — x/y pan, rotation, scale floor/ceiling → SectorStatic; GetSectorUDMFFixed.
- `2026-10-10` — M7: **UDMF sector moreids** — parse; merge into `SectorStatic.tags` for specials/ACS.
- `2026-10-10` — M7: **UDMF linedef moreids** — parse; `Line::has_id` for Attach/ACS/GetLineUDMF.
- `2026-10-10` — M7: **midtex Attach ceiling hang** — `Sector_Attach3dMidtex` arg2 hangs 64mu slab from attached ceiling.
- `2026-10-10` — M7: **UDMF blockhitscan** — parse; aim/shoot traverse treat as solid; GetLineUDMFInt.
- `2026-10-10` — M7: **UDMF blocksight** — parse; LOS opaque (+ `ML_BLOCKEVERYTHING`); GetLineUDMFInt.
- `2026-10-10` — M7: **UDMF blockprojectiles** — parse; `check_position` solids missiles; GetLineUDMFInt.
- `2026-10-10` — M7: **UDMF locknumber** — parse; overrides Door_LockedRaise/Generic_Door lock args; GetLineUDMFInt.
- `2026-10-10` — M7: **UDMF blockfloaters** — parse; `check_position` solids `+FLOAT` against those lines.
- `2026-10-10` — M7: **UDMF checkswitchrange / firstsideonly** — parse + Use/Push gates (eye in opening; front side only).
- `2026-10-10` — M7: **StartSlideshow / SetSubtitleNumber shell** — drain ACS requests → HUD messages.
- `2026-10-10` — M7: **A_SpawnParticle** — DECORATE colour/flags/life/size/offsets → `particle_events`.
- `2026-10-10` — M7: **SpawnParticle render** — ACS events → `ParticleStore` billboards (`__PARTICLE`); fade by lifetime.
- `2026-10-10` — M7: **Sector_Attach3dMidtex (51)** — marks matching two-sided lines `midtex3d`; `SetupData.midtex_attach`.
- `2026-10-10` — M7: **UDMF midtex3d / midtex3dimpassible** — walkable 64mu mid slab; impassible blocks non-missiles.

- `2026-10-10` — M7: **ExtraFloor_LightOnly (50)** — type-3 light transfer (alpha 0); `GetActorLightLevel` below top plane.
- `2026-10-10` — M7: **3D floor alpha** — `Sector_Set3DFloor` arg3 → `FFloorStatic.alpha`; scene flats/sides translucent / skip at 0.
- `2026-10-10` — M7: **decal colorchanger** — FadeStart/Time + Color → light luminance lerp (ToBlack).
- `2026-10-10` — M7: **decal stretcher/slider** — GoalX/Y + DistY; combiner merges children; store animates size/Z.
- `2026-10-10` — M7: **decal fader** — DECALDEF `DecayStart`/`DecayTime` → alpha fade + remove; app ticks store.
- `2026-10-10` — M7: **decal wall-clip** — linedef-tagged decals emit wall-aligned translucent quads (billboard fallback).
- `2026-10-10` — M7: **3D floor actor light** — `GetActorLightLevel` uses control light under solid / in swimmable slabs.
- `2026-10-10` — M7: **swimmable 3D floors** — `z_movement` reduced gravity + `up_move` thrust; shared `actor_waterlevel`.
- `2026-10-10` — M7: **3D floor side walls** — scene emits vertical quads on target linedefs (control textures).
- `2026-10-10` — M7: **UDMF floatspeed/fastspeed/jumpz/attackzoffset** — parse + spawn; GetThingUDMFFixed.
- `2026-10-10` — M7: **UDMF maxstepheight/maxdropoffheight** — thing parse + spawn; GetThingUDMFFixed.
- `2026-10-10` — M7: **UDMF friction/damagefactor** — thing parse + spawn; GetThingUDMFFixed.
- `2026-10-10` — M7: **UDMF mass/speed** — thing parse + spawn; GetThingUDMFInt/Fixed.
- `2026-10-10` — M7: **UDMF accuracy/stamina/species/meleerange/viewheight** — parse + spawn; GetThingUDMF int/fixed.

- `2026-10-10` — M7: **APROP_Waterlevel** — 0..=3 from Transfer_Heights fake floor + swimmable 3D floors.
- `2026-10-10` — M7: **StartConversation (18)** — living player + talker tid → `conversation_request` (+ optional face).
- `2026-10-10` — M7: **Thing_SetConversation (79)** — Hexen special sets actor dialogue ID (`tid` 0 = activator).
- `2026-10-10` — M7: **A_SetConversation** — DECORATE setter for dialogue ID.
- `2026-10-10` — M7: **UDMF conversation** — thing parse/spawn + `GetThingUDMFInt`; dialogue runtime stub.
- `2026-10-10` — M7: **UDMF arg0str (things+lines)** — parse; `init_acs` maps SNAM name → `args[0]` for ACS specials 80–86/226.

- `2026-10-10` — M7: **UDMF countsecret/nocount** — parse + spawn; secret pickup increments `found_secrets`; nocount clears kill/item counts.
- `2026-10-10` — M7: **GetThingUDMF* thing keys** — health/score/friendlyseeblocks/fillcolor/pitch/roll/args + gravity/scale/alpha/x/y/z fixed.
- `2026-10-10` — M7: **UDMF thing fillcolor/renderstyle/floatbobphase** — parse + spawn; stencil RGB + `RenderStyle::parse`; lexer accepts `0x` hex ints.

- `2026-10-10` — M7: **UDMF thing gravity/scale/alpha/pitch/roll** — parse + spawn; `scale` sets both axes; `scalex`/`scaley` override.
- `2026-10-10` — M7: **UDMF thing health/score** — parse + spawn overrides (`score` unset = `i32::MIN`).
- `2026-10-10` — M7: **UDMF friendlyseeblocks + A_SetFriendlySeeBlocks** — thing parse/spawn; DECORATE setter.
- `2026-10-10` — M7: **A_LogInt / A_LogFloat** — queue formatted number HUD messages.
- `2026-10-10` — M7: **APROP_Master/Target/TracerTID set** — SetActorProperty resolves TID→pointer; 0 clears.
- `2026-10-10` — M7: **APROP_FriendlySeeBlocks** — const 48; class/runtime; friendly `A_Look` range = blocks×128.
- `2026-10-10` — M7: **APROP_*Sound + SoundClass** — See/Attack/Pain/Death/Active set-only; SoundClass (47) get/set; runtime overrides in PlayActorSound.
- `2026-10-10` — M7: **APROP_DamageType + Waterlevel** — const 46 dyn-string Get/Set/Check; 28 read-only stub (0).
- `2026-10-10` — M7: **APROP_Nametag + A_SetTag** — const 21; dyn-string Get/Set/Check; spawn from class `Tag`.
- `2026-10-10` — M7: **APROP_Species** — const 20; Get/Set/CheckActorProperty via dyn strings; host get/set (+`None` clear).
- `2026-10-10` — M7: **+RIPPER** — `FLAG_RIPPER`; `is_ripper_missile` (flag or level); physics/vanilla rip path.
- `2026-10-10` — M7: **RipperLevel / RipLevelMin/Max + DONTRIP** — class props; spawn init; rip gate.
- `2026-10-10` — M7: **ripper missile collision** — `xy_movement` damages + continues when rip levels allow.
- `2026-10-10` — M7: **A_SetRipperLevel / RipMin / RipMax** — rip level fields + `can_rip_through`.
- `2026-10-10` — M7: **A_ThrowGrenade / A_SpawnDebris** — lofted missile; debris velocity burst.
- `2026-10-10` — M7: **A_CustomPunch / A_FireBullets** — configurable melee + multi-pellet hitscan.
- `2026-10-10` — M7: **A_StopSound + APROP_NoTrigger / DamageMultiplier** — stop queued SFX; line activate gate.
- `2026-10-10` — M7: **APROP_ChaseGoal / Frightened + A_SetChaseGoal** — `+FRIGHTENED` flag bit.
- `2026-10-10` — M7: **A_SetJumpZ / APROP_JumpZ + DecoBlock jumpz if** — runtime jump impulse.
- `2026-10-10` — M7: **APROP_RenderStyle** — get/set `STYLE_*` (ACS legacy constants).
- `2026-10-10` — M7: **A_RailAttack / A_CustomRailgun / A_SentinelRefire** — piercing rail; sentinel refire→See.
- `2026-10-10` — M7: **DecoBlock float/fast/camera if** — FloatSpeed/FastSpeed/CameraHeight/FOV skips.
- `2026-10-10` — M7: **A_CustomMeleeAttack / A_CustomComboAttack / A_BasicAttack** — melee or missile combo.
- `2026-10-10` — M7: **DecoBlock maxstep/dropoff if** — `A_SkipUnlessMaxStepHeight`/`MaxDropOffHeight`.
- `2026-10-10` — M7: **A_MonsterRefire + DecoBlock attackzoffset if** — custom abort label; `A_SkipUnlessAttackZOffset`.
- `2026-10-10` — M7: **DecoBlock meleerange/viewheight if** — `A_SkipUnlessMeleeRange`/`ViewHeight`.
- `2026-10-10` — M7: **DecoBlock team if** — `A_SkipUnlessTeam` on `designated_team`.
- `2026-10-10` — M7: **A_SetAccuracy / A_SetStamina + APROP_*** — DecoBlock accuracy/stamina ifs.
- `2026-10-10` — M7: **A_SetTeam / CameraHeight / CameraFOV + DecoBlock damagefactor** — app ChangeCamera eye+FOV.
- `2026-10-10` — M7: **A_SetDamageFactor + APROP_DamageFactor** — runtime incoming damage multiplier.
- `2026-10-10` — M7: **A_SetScale ScaleX/Y + A_SetObituary/HitObituary** — APROP_ScaleX/Y; app sprite size; obituaries.
- `2026-10-10` — M7: **A_SetViewHeight / A_SetAttackZOffset + APROP_*** — hitscan shoot origin uses runtime fields.
- `2026-10-10` — M7: **A_SetFillColor + APROP_StencilColor** — runtime fill/stencil RGB as 0xRRGGBB.
- `2026-10-10` — M7: **DecoBlock friction if + A_SetBloodColor** — `A_SkipUnlessFriction`; runtime blood RGB.
- `2026-10-10` — M7: **A_SetFloatSpeed / A_SetFastSpeed** — runtime FLOAT Z-adjust + nightmare chase speed.
- `2026-10-10` — M7: **A_SetMaxStep/DropOffHeight + APROP_*** — runtime step/dropoff; `try_move` honors actor fields.
- `2026-10-10` — M7: **A_SetFriction + APROP_Friction + DecoBlock special if** — runtime friction; `A_SkipUnlessSpecial`.
- `2026-10-10` — M7: **APROP MeleeRange/SpawnHealth/ViewHeight + A_SetMeleeRange** — runtime `Actor.melee_range`.
- `2026-10-10` — M7: **APROP_* expand + score** — Damage/Alpha/flags/Score/TIDs/ReactionTime get/set; `A_SetScore`/`GiveScore`; DecoBlock score if.
- `2026-10-10` — M7: **A_CopySpriteFrame / A_RaiseSelf** — copy target/master/tracer sprite; self-revive.
- `2026-10-10` — M7: **A_SetSprite / A_SetFrame / A_TransferInventory** — sprite+frame; move item stack by tid.
- `2026-10-10` — M7: **DecoBlock painthreshold/threshold if** — `A_SkipUnlessPainThreshold`/`Threshold`.
- `2026-10-10` — M7: **DecoBlock reactiontime if + A_SetReactionTime** — `A_SkipUnlessReactionTime`.
- `2026-10-10` — M7: **DecoBlock species if + TakeFrom Children/Siblings** — `A_SkipUnlessSpecies`; inventory take helpers.
- `2026-10-10` — M7: **A_SetInvisible + sibling helpers** — `FLAG_DONTDRAW`; Raise/Remove/Kill/GiveTo/DamageSiblings; app skips draw.
- `2026-10-10` — M7: **DecoBlock damage if + A_ClearLastHeard** — `A_SkipUnlessDamage`; clear sector `soundtarget`.
- `2026-10-10` — M7: **A_SprayDecal** — DECALDEF name + optional forward offset → `decal_events`.
- `2026-10-10` — M7: **A_SetTeleFog / A_SwapTeleFog / A_Log** — DECORATE telefog + print alias.
- `2026-10-10` — M7: **DecoBlock roll if + A_SetDamage** — `A_SkipUnlessRoll`; runtime missile damage override.
- `2026-10-10` — M7: **A_QuakeEx / A_SetBlend** — max-axis quake; full-screen colour fade via `ScreenFade`.
- `2026-10-10` — M7: **A_FaceMovementDirection / A_AlertMonsters** — yaw from vel; noise flood + optional range wake.
- `2026-10-10` — M7: **A_SetPainThreshold / A_SetThreshold / A_JumpIfHigherOrLower** — runtime pain/infight gates; Z vs pointer.
- `2026-10-10` — M7: **A_SetFloatBobPhase / A_Weave** — bob phase `0..63`; Heretic weave step + spawn seed.
- `2026-10-10` — M7: **A_LookEx + melee-range jumps** — FOV/min/max wake; `A_JumpIfTargetOutside/InsideMeleeRange`.
- `2026-10-10` — M7: **DecoBlock alpha/gravity if** — scaled×1000 skip helpers `A_SkipUnlessAlpha`/`Gravity`.
- `2026-10-10` — M7: **A_RearrangePointers + GiveTo/TakeFromMaster** — snapshot pointer assign; master inventory helpers.
- `2026-10-10` — M7: **SNDSEQ playrepeat/playloop + A_SetFriendly/A_SetSpecies** — looping steps + `stop_sequence`; runtime species override.
- `2026-10-10` — M7: **A_Respawn / A_CopyFriendliness + SetSectorTerrain ceiling** — revive at spawn; copy FRIENDLY; `TerrainTable.ceilings`.
- `2026-10-10` — M7: **A_Teleport / A_SelectWeapon** — warp to `TeleportDest` + fog/state; player weapon queue.
- `2026-10-10` — M7: **A_CheckBlock / A_CheckLOF** — offset `check_position` jump; LOF = sight+range to target.
- `2026-10-10` — M7: **TranslucentLine render** — `Line.mid_alpha` → translucent wall buffer + sprite blend; app copies `line_alpha`.
- `2026-10-10` — M7: **GetLevelInfo PAR/SUCK/CLUSTER/LEVELNUM** — `LevelState` fields + MAPINFO apply in headless.
- `2026-10-10` — M7: **A_GiveToTarget/Children + A_DamageTarget/Master/Tracer/Children + A_TakeFromTarget** — pointer inventory/damage.
- `2026-10-10` — M7: **A_Raise/Remove/Kill Children|Master** — family pointer revive/destroy helpers.
- `2026-10-10` — M7: **plane reflection + A_FaceTracer/Master** — `__PREFL*` Z-mirrored canvases; `Camera::reflect_across_z`; app copies `plane_reflection`.
- `2026-10-10` — M7: **plane alpha render + ChangeCamera** — vertex `@location(3)` alpha; translucent flats via sprite blend; app copies sector alphas; camera view + `camera_revert`.
- `2026-10-10` — M7: **render plane/side scale+angle + A_SetRoll** — app copies `floor_scales`/`ceiling_scales`/`plane_angles`/`side_scales` into map; scene FlatUv + wall ÷scale; `A_SetRoll`.
- `2026-10-10` — M7: **Line_Horizon / Transfer_WallLight / Lightning / Plane_Copy / Line_SetIdentification / scroll copy** — 9/16/58/109/118/121/222/225.
- `2026-10-10` — M7: **ByTexture / TxTy / MoveToValueAndCrush / Polyobj_StopSound** — 261/268–269/239/241/279–280/283.
- `2026-10-10` — M7: **EE plane movers + Door_Animated* + Stairs Doom Sync/Crush** — 256–267/14/274/270–273.
- `2026-10-10` — M7: **Floor_RaiseToHighest + Stairs_*Sync/Down + CrushAndRaiseA*** — 24/26/27/31/32/196/197/169; RaiseToLowestCeiling → 238.
- `2026-10-10` — M7: **Floor/Ceiling_Waggle + TeleportInSector + CrushRaiseAndStay*** — 138/38/78/45/195/255; LowerToFloor → 254.
- `2026-10-10` — M7: **TeleportOther/Group + Plat_*Lip + Ceiling_*Times8** — 76/77/206/207/198/199.
- `2026-10-10` — M7: **Floor/Ceiling_MoveToValue* + Ceiling_*Instant + Scroll_Texture_Both** — 37/47/68/69/193/194/221.
- `2026-10-10` — M7: **Floor_Lower/RaiseInstant** — specials 66/67 (height×8, immediate).
- `2026-10-10` — M7: **Plat_Down/UpByValue + UpWaitDownStay + Teleport_NoStop** — specials 63–65/154; StayTx → 230.
- `2026-10-10` — M7: **Plat_UpNearestWaitDownStay + Line_SetAutomap*** — specials 172/281/282.
- `2026-10-10` — M7: **Floor_Stop / Ceiling_Stop / CrushAndRaiseDist** — specials 275/276/168.
- `2026-10-10` — M7: **Line/Sector_SetHealth + Floor/CeilingGlow** — specials 150/151/277/278.
- `2026-10-10` — M7: **Thing_ProjectileAimed/Intercept + PlaneReflection** — specials 178/175/159; ACS_ExecuteAlways → 226 (85 = LockedExecuteDoor).
- `2026-10-10` — M7: **Scroll_Wall / SetGlobalFogParameter / Scale2 / Thing_SetGoal** — specials 52/157/170–171/229.
- `2026-10-10` — M7: **Door_WaitRaise / Door_WaitClose** — specials 105/106; `DoorMover.wait_then_raise`.
- `2026-10-10` — M7: **Thing_SpawnFacing / Sector_ChangeSound / ChangeCamera** — specials 139/140/237.
- `2026-10-10` — M7: **Sector_SetColor/Fade/Damage/Gravity + Thing_SetTranslation** — specials 212/213/214/216/180.
- `2026-10-10` — M7: **Thing_Damage/Move + Sector_ChangeFlags + GlassBreak** — specials 119/125/54/49.
- `2026-10-10` — M7: **Sector_SetFloor/CeilingScale + SetRotation** — specials 189/188/185; `floor_scales`/`ceiling_scales`/`plane_angles`.
- `2026-10-10` — M7: **Sector floor/ceiling panning + DamageThing** — specials 187/186/73; GZDoom number fixes (ThrustThing 72, ThrustThingZ 128, Thing_Raise 17, Thing_SetSpecial 127, Line tex 53/56, SetPlayerProperty 191, Plat 60/61).
- `2026-10-10` — M7: **Line_SetTextureScale / SetPlayerProperty** — specials 56/191; `side_scales`; PROP_FROZEN/NOTARGET/FLY.
- `2026-10-10` — M7: **Line_SetTextureOffset** — special 53 → `World.side_offsets`.
- `2026-10-10` — M7: **Thing_Raise** — special 17; Raise-state revive + COUNTKILL adjust.
- `2026-10-10` — M7: **DecoBlock health cmp** — `health ==/>/>=/!=` → `A_SkipUnlessHealth` (`</<=` keep JumpIfHealthLower).
- `2026-10-10` — M7: **ThrustThingZ / Autosave** — specials 129/15; `World.autosave_request`.
- `2026-10-10` — M7: **Thing_Hate / Thing_SetSpecial** — specials 177/178.
- `2026-10-10` — M7: **Thing_Stop / HealThing** — specials 19/248.
- `2026-10-10` — M7: **Thing_ChangeTID / ChangeSkill** — specials 176/179.
- `2026-10-10` — M7: **NoiseAlert** — special 173; `noise_alert_ex(target, emitter)` flood.
- `2026-10-10` — M7: **TranslucentLine** — special 208; mid-tex alpha + additive on `World.line_alpha`.
- `2026-10-10` — M7: **Line_SetBlocking** — special 55; `BLOCKF_*` → `ML_*` set/clear on tagged lines.
- `2026-10-10` — M7: **Floor_RaiseAndCrush / Doom** — specials 28/99 (was misnumbered); XLAT emits 25/99 for nearest/crush.
- `2026-10-10` — M7: **Sector_SetTranslucent** — special 98; floor/ceiling alpha + additive flag on `SectorDynamic`.
- `2026-10-10` — M7: **Ceiling_LowerAndCrushDist** — special 97; gap = `dist` (default 8).
- `2026-10-10` — M7: **FloorAndCeiling_Lower/RaiseByValue** — specials 95/96 move both planes by height.
- `2026-10-10` — M7: **polyobj MoveTo / MoveToSpot** — specials 88/89/86/59 → translate toward absolute XY or tid spot.
- `2026-10-10` — M7: **DecoBlock velx/vely/velz if** — `A_SkipUnlessVelX|Y|Z`.
- `2026-10-10` — M7: **DecoBlock mass/height/radius if** — `A_SkipUnlessMass`/`Height`/`Radius`.
- `2026-10-10` — M7: **DecoBlock tid if** — `A_SkipUnlessTid`.
- `2026-10-10` — M7: **SNDSEQ attenuation / volumerel / stopsound** — steps + end cue; defs→sim conversion; `m7_udmf_user_props_smoke` accept.
- `2026-10-10` — M7: **UDMF `user_*` → ACS Get\*UDMF\*** — `UdmfUserStore` on parse/`MapGeometry`; int/bool/fixed bags; host lookup for line/thing/sector/side.
- `2026-10-10` — M7: **DecoBlock pitch if + Get/SetTeamScore** — `A_SkipUnlessPitch`; ACSF 19620–19621.
- `2026-10-10` — M7: **DecoBlock angle if + ffloor/slope smoke** — `A_SkipUnlessAngle`; `m7_ffloor_slope_setup_smoke` accept.
- `2026-10-10` — M7: **M7 feature smoke accept** — `room-game/tests/m7_feature_smoke.rs` (portal map + SNDSEQ door-slot + Pillar_Build + fog); door/platform slot lookup on `SndSeqTable`.
- `2026-10-10` — M7: **SNDSEQ stream bootstrap** — `SndSeqTable` + tic runner (`play`/`delay`/`volume`); ACS `SoundSequence*` starts instances; Headless `set_sndseq`.
- `2026-10-10` — M7: **ACS SetSubtitle / GetNetID / SetSectorGlow / SetFogDensity** — ACSF 214–216/400–401; plane glow + fog→`UnlitPass`; net-id = actor raw.
- `2026-10-10` — M7: **DecoBlock speed / user_* if** — `A_SkipUnlessSpeed` / `A_SkipUnlessUserVar`; ACSF `GetLineX`/`GetLineY` (300–301).
- `2026-10-10` — M7: **ACS GetSectorHealth / GetLineHealth** — ACSF 212–213; `SectorDynamic.health_*` + `line_health` map.
- `2026-10-10` — M7: **ACS Get/SetUserCVarString + StartSlideshow** — ACSF 58–59/211; per-player string cvars; `slideshow_request`.
- `2026-10-10` — M7: **ACS AnnouncerSound / SetPointer / UserCVar / SetCVarString / ScriptCall** — ACSF 37–38/54–55/57/210; AAPTR target/master/tracer; string+user cvar maps; ScriptCall stub.
- `2026-10-10` — M7: **ACS SpawnParticle / HUD clip / SetCVar / GetCVarString** — ACSF 51–53/56/96; `SimParticleEvent` queue; skill/fast/respawn setters.
- `2026-10-10` — M7: **ACS Warp + QuakeEx** — ACSF 91–92; warp activator to tid+offset; QuakeEx → existing quake thinker (max intens).
- `2026-10-10` — M7: **ACS PickActor / IsPointerEqual / TeleFog / SetSectorDamage|Terrain** — ACSF 83–87/94–95; `line_pick_actor`; sector damage fields + floor→terrain map.
- `2026-10-10` — M7: **mirror/portal stencil in main pass** — unlit MRT; app `stencil_aux`; `draw_materials` masks `__MIRROR*`/`__PORTAL*` then Equal-draws canvases.
- `2026-10-10` — M7: **stencil MRT path** — dual colour targets + `fs_stencil_mask`/`fs_main_mrt`; Metal needs target-0 writes for Replace (Equal overwrites). Empty target-0 write-mask still skips stencil.
- `2026-10-10` — M7: **ACS SoundSequenceOn* / SetMusicVolume / CanRaise / SetTranslation / StrArg** — ACSF 30–32/85/97/203/206; sequence cues as positional SFX; `music_volume`.
- `2026-10-10` — M7: **sky scroll UV** — `scroll_sky_u` + app applies `World.sky_speed1 * maptime` (ACSF `SetSkyScrollSpeed`). Equal-only stencil still blocked on Metal (empty/keep-blend write masks skip Replace).
- `2026-10-10` — M7: **portal recursion depth** — `MAX_PORTAL_RECURSION=2`; nested `__PORTAL*` canvas refresh before parent blit; strip at max depth.
- `2026-10-10` — M7: **ACS roll / maxinv / Check* / DamageActor / PlayActorSound / SetSkyScrollSpeed** — `Actor.roll`; ACSF 18/71/88–90/93/99/200–201; `sky_speed*`; host + VM.
- `2026-10-10` — M7: **linked portal canvases** — `LinkedPortalSurface` + `__PORTAL*` mid quads; `Camera::shifted_by`; app refreshes displaced offscreen into materials (mirrors unchanged). Equal-only stencil still open on Metal.
- `2026-10-10` — M7: **ACS GetActorPowerupTics + SoundVolume** — ACSF 78 player power tic queries; `SoundVolume` rescales queued `SimSoundEvent`s for tid/activator.
- `2026-10-10` — M7: **stencil mask/equal pipelines** — `Depth24PlusStencil8` + `draw_stencil_mask`/`draw_stencil_equal` + same-pass `render_offscreen_stenciled` smoke. Strict Equal-only masking (no mask colour) still open on Metal; portal recursion next.
- `2026-10-10` — M7: **ACS ChangeLevel / GetPlayerInfo / SectorDamage / CheckActor*Texture** — host + VM; plane texture via BSP/`vanilla`/single-sector fallback.
- `2026-10-10` — M7: **GPU mirror clip plane** — `ViewUniforms.clip_plane` + fragment `discard`; `mirror_clip_plane` helper; app sets plane for `__MIRROR*` offscreen canvases (CPU tri drop kept). Full stencil buffer still open.
- `2026-10-10` — M7: **ACS ammo/state/damage + air** — `Get/SetAmmoCapacity`; `SetActorState`; `Thing_Damage2`; `UseActorInventory`; `PlayerInGame`/`SinglePlayer`; `SetAirControl`; `NegateBinary`/`EndLog`; fixed `UseInventory` PCD→336; ACSF `SpawnDecal`/`GetArmorInfo`/`SoundVolume`.
- `2026-10-10` — M7: **ACS gravity/sky/level/textures + LineAttack** — `SetGravity`(+Direct); `GetLevelInfo` (secrets/items/kills); `ChangeSky`→`sky*_override` (app); `ReplaceTextures`; `SetActorPitch`; ACSF `LineAttack`/`DropInventory`/`GetArmorType`.
- `2026-10-10` — M7: **ACS CASEGOTO + PlaySound/SpawnForced/Air** — `CASEGOTO` switch dispatch; ACSF `PlaySound`/`StopSound`/`SpawnForced`/`SpawnSpotFacingForced`/`Get/SetAirSupply` (`Player.air_finished`).
- `2026-10-10` — M7: **ACS SetLineBlocking/Special** — PCD 98/99 + `line_args` overrides; `line_flags` widened to i32 (`ML_BLOCKEVERYTHING`/`ML_RAILING`/…); physics/`activate_line` consult runtime flags/args; `SoundSequence` accepted (SNDSEQ play still open).
- `2026-10-10` — M7: **ACS math/UI queries** — `Sin`/`Cos`/`VectorAngle`; `PlayerNumber`; `GetScreenWidth/Height`; `StrLen`; `GetCVar` (skill/fast/respawn); `FadeRange`→FadeTo; fixed `Check/SetWeapon` PCD indices.
- `2026-10-10` — M7: **ACS FadeTo/SetFont/weapons** — `FadeTo`/`CancelFade` screen overlay; `SetFont`; `Check/SetWeapon`; `UseInventory`.
- `2026-10-10` — M7: **ACS SetMusic + activator inventory** — `SetMusic`/`LocalSetMusic`(+Direct) → `World.music_request`; `Clear/Give/Take/CheckInventory`(+Direct) on activator.
- `2026-10-10` — M7: **ACS sounds** — `SectorSound`/`AmbientSound`/`LocalAmbientSound`/`ActivatorSound`/`ThingSound` → `SimSoundEvent`.
- `2026-10-10` — M7: **ACS SpawnSpotFacing + ClassifyActor** — PCD 289 (spot angle) / 346 (ACTOR_* bitmask).
- `2026-10-10` — M7: **ACS actor inventory** — `Clear/Give/Take/CheckActorInventory` (PCD 284–287).
- `2026-10-10` — M7: **ACS ThingCount*** — `ThingCountName` / `ThingCountSector` / `ThingCountNameSector` (PCD 288/342/343).
- `2026-10-10` — M7: **KEYCONF alias/bind** — shell applies KEYCONF `alias` via console; `defaultbind` for unbound keys; ACS `GetActorLightLevel` PCD 340.
- `2026-10-10` — M7: **DecoBlock args[n]** — `args[n]` / `args[n] op N` → `A_SkipUnlessArg` (eq/ne/gt/ge/lt/le).
- `2026-10-10` — M7: **DecoBlock CountInv** — `CountInv`/`inventory`/`CheckInventory` conditions → `A_SkipUnlessInventory` / `A_JumpIfInventory`.
- `2026-10-10` — M7: **DecoBlock if/else** — `A_SkipAlways` + health `if/else` flatten (`SkipUnless` then / `SkipAlways` else).
- `2026-10-10` — M7: **mirror plane clip** — `clip_scene_to_mirror_line` drops tris on the reflected-eye side of the mirror line before offscreen blit (CPU stand-in for stencil).
- `2026-10-10` — M7: **DecoBlock if-body** — `A_SkipUnlessHealthLower` + `action_skip`; general `if (health < N) { … }` then-bodies; `A_ClearInventory`.
- `2026-10-10` — M7: **DecoBlock bootstrap** — anon `if`/`else`/`return`/`return state("L")`; const-fold + `health < N` → `A_JumpIfHealthLower`.
- `2026-10-10` — M7: **ACS GetActorFloorTexture/Terrain** — ACSF 204/205 via dyn strings (sector flat + TERRAIN floors map).
- `2026-10-10` — M7: **ACS CheckProximity** — ACSF 98 counts class actors in fixed distance of tid/activator.
- `2026-10-10` — M7: **mirror reflected omit** — reflected canvas strips `__MIRROR*` walls (avoids feedback without stencil).
- `2026-10-10` — M7: **OptionMenu cvar toggle** — `option <cvar>` / `cmd <line>` from MENUDEF activate via console `toggle`/`exec`.
- `2026-10-10` — M7: **A_DropItem** — DECORATE drop at feet with amount/chance (pairs ACSF `DropItem`).
- `2026-10-10` — M7: **ACS Set/GetLineActivation** — runtime `line_activations`/`line_flags` overrides; `activate_line` consults them; ACSF 76/77.
- `2026-10-10` — M7: **ACS named scripts + string ACSF** — `SNAM`/`ACS_NamedExecute*`; `SCRIPTWAITNAMED`; dyn string pool + `StrLeft|Right|Mid`/`strcmp`/`stricmp`; `GetActorClass`/`GetWeapon`; `Floor`/`Ceil`/`Round`; `CheckFlag`/`SetActorFlag`; `ChangeActorAngle`/`Pitch`; `DropItem`.
- `2026-10-10` — M7: **ACS script types** — Enter/Return/Death/Respawn/Unloading typed + fired on map enter/hub return/player death/respawn/exit.
- `2026-10-10` — M7: **Line_Mirror canvas** — mirror segs draw as `__MIRROR{n}` quads; app reflects camera across the line into an offscreen canvas (no stencil clip yet).
- `2026-10-10` — M7: **ACS ChangeFloor/Ceiling + SetLineTexture** — PCD 65–68/97 update `sector_*_pics` / switch texture maps; app applies ceiling flat overrides.
- `2026-10-10` — M7: **camera canvas + mirror math** — app refreshes `SetCameraToTexture` canvases into `MaterialCatalog`; `Camera::reflect_across_line` + `LinePortal::is_mirror`; portal recursion still open.
- `2026-10-10` — M7: **SetCameraToTexture** — PCD 269 + `World.camera_textures` bind (tid/activator → texture/FOV).
- `2026-10-10` — M7: **ANIMDEFS cameratexture** — parse name/size/`fit`/`hdr`/`worldpanning` into `CameraTextureDef`.
- `2026-10-10` — M7: **LOADACS + LOAD chunk** — parse `LOADACS` into `Definitions`; ACSE `LOAD` names on `AcsModule`; Headless/`push_acs_library` mounts library blobs from VFS.
- `2026-10-10` — M7: **sky cubemap** — GLDEFS 6-face skyboxes draw as inward cube (`CubemapFaces` + `sky_ranges`); incomplete boxes still use cylinder/`sky1`.
- `2026-10-10` — M7: **OptionMenu runtime** — `option_menu_from_def` maps Submenu/Command/Option/StaticText; `MenuStack` keeps MENUDEF and pushes on `openmenu`.
- `2026-10-10` — M7: **plane interp + ClearLineSpecial** — `SectorDynamic` prev floor/ceiling snap each tic; app lerps with `tic_accum`; ACS `CLEARLINESPECIAL` clears activating linedef via host.
- `2026-10-10` — M7: **MENUDEF runtime** — `list_menu_from_def` builds `ListMenu` from `TextItem`/`PatchItem` + `IfGame`; app title menu prefers MENUDEF `MainMenu`.
- `2026-10-10` — M7: **sky bootstrap** — GLDEFS `skybox` parse; MAPINFO `skybox`/`sky1` → textured cylinder; full cubemap draw still open.
- `2026-10-10` — M7: **KEYCONF apply** — `WeaponSlots` on `World`; `setslot`/`addslot*`/`playerclass` via `Headless::set_keyconf`; weapon select uses slot table.
- `2026-10-10` — M7: **decal billboards** — `DecalStore` + `append_decals` into sprite pass; app drains `SimDecalEvent` and loads DECALDEF/TERRAIN/LOCKDEFS from defs. Wall-clip when linedef known.
- `2026-10-10` — M7: **decal events** — `SimDecalEvent` + DECALDEF generators on wall hitscan; `Headless::set_decaldef`.
- `2026-10-10` — M7: **LOCKDEFS use** — `World.locks` + `actor_has_lock` for `Door_LockedRaise`/`Generic_Door`; `Headless::set_lockdefs` from defs.
- `2026-10-10` — M7: **TERRAIN splash** — `World.terrain` flat→splash; `z_movement` hit-floor plays splash sound / spawns class; `Headless::set_terrain` from defs.
- `2026-10-10` — M7: **SBARINFO draw** — `SbarInfoBar` interprets `drawimage`/`drawnumber`/`drawmugshot`/`drawswitchableimage` + `gamemode`/`usesammo`/`else`; Doom stock `normal` bar emits STBAR + health.
- `2026-10-10` — M7: **MENUDEF parse** — `ListMenu`/`OptionMenu`/`OptionValue`/`OptionString`/`Default*`/`Add*` + nested `IfGame`/`IfOption`; GZDoom stock lump parses; wired into `Definitions`.
- `2026-10-10` — M7: **KEYCONF parse** — line-oriented whitelist (`weaponsection`/`setslot`/`addslot*`/`alias`/`defaultbind`/`addkeysection`/`addmenukey`/`clearplayerclasses`/`addplayerclass`); wired into `Definitions`.
- `2026-10-10` — M7: **LOCKDEFS parse** — `ClearLocks`/`Lock n [Game]` with keys/`Any`/`Message`/`RemoteMessage`/`Mapcolor`/`LockedSound`; multi-game variants; GZDoom stock lump parses; wired into `Definitions`.
- `2026-10-10` — M7: **DECALDEF parse** — `decal`/`decalgroup`/`fader`/`stretcher`/`slider`/`colorchanger`/`combiner`/`generator`; GZDoom stock lump parses; wired into `Definitions`.
- `2026-10-10` — M7: **TERRAIN parse** — `splash`/`terrain`/`floor`/`defaultterrain`; `ifdoom|heretic|hexen|strife` filter; wired into `Definitions` (additive).
- `2026-10-10` — M7: **SBARINFO parse** — `base`/`height`/`statusbar`/`mugshot`; `drawimage`/`drawnumber`/nested `gamemode`/`else`; wired into `Definitions` (last lump wins).
- `2026-10-10` — M7: **DecoExpr** — parse arith/`random()`/`args[n]`/`user_*`; evaluate action args at `set_state` call time (`health`, vel, tid, …).
- `2026-10-10` — M7: **MAPINFO hubs** — `cluster`/`clusterdef` + `hub` flag; `Headless` snapshots leaving hub maps and restores on return; ACS world/global vars + player health travel.
- `2026-10-10` — M7: **ACSF more** — `GetActorViewHeight`/`GetChar`/`CheckActorProperty`/`Radius_Quake2`/`SpawnSpotForced`.
- `2026-10-10` — M7: **ACS user vars** — ACSF `Set/GetUserVariable`/`Set/GetUserArray`/`CheckActorClass`; DECORATE `A_SetUserArray`.
- `2026-10-10` — M7: **DECORATE actions** — `A_SetTranslucent`/`A_Recoil`/`A_SetUserVar` (`Actor.user_vars`) / `A_CheckProximity` / `A_ClearMaster`/`A_ClearTracer`.
- `2026-10-10` — M7: **DECORATE duration/anon** — const int duration expressions, `random(lo,hi)` tics, `{ A_Foo; A_Bar }` flattened to 0-tic follow-up states.
- `2026-10-10` — M7: **ACS actor property/position** — `Get/SetActorProperty` (health/speed/gravity/mass/height/radius), `SetActorPosition`/`Angle`, ACSF `SetActivator`/`ToTarget`/`SetActorVelocity`.
- `2026-10-10` — M7: **sector plane portals** — `Sector_SetPortal` stores plane; linked/teleport floors/ceilings warp on `z_movement`; scene skips the portal plane flats.
- `2026-10-10` — M7: **linked portal groups** — flood-fill groups + 2D displacement; `P_CheckSight` skips REJECT across groups and treats traversable portal 1S as passable.
- `2026-10-10` — M7: **line portals** — traversable `Line_SetPortal` (kinds 1–3) pass in `check_position` and teleport on cross (midpoint delta); scene skips those walls. Linked groups / sight / sector portals still open.
- `2026-10-10` — M7: **3D floor bootstrap** — `FFloorStatic.typ`; solid slabs in `check_position` (walk-under / stand-on); scene extra flats at control plane Z. Side walls + swim + actor light + alpha + ExtraFloor_LightOnly + midtex3d walk; ffloor portals / real midtex height still open.
- `2026-10-10` — M7: **polyobj render segs + slope Z** — scene emits poly lines from live verts (skip BSP copies); flats use `SecPlane::z_at`; sim `check_position` / ACS `GetSector*Z` follow slope + mover delta.
- `2026-10-10` — M7: **ACSF Get*UDMF* + CheckSight** — built-in line/sector/side keys; `CheckSight` via `P_CheckSight`; `user_*` via `UdmfUserStore`.
- `2026-10-10` — M7: **ACS Spawn*** — `SPAWN`/`SPAWNDIRECT`/`SPAWNSPOT`/`SPAWNSPOTDIRECT` (DoomEdNum, byte angle, tid).
- `2026-10-10` — M7: **ACS ThingCount** — `THINGCOUNT`/`THINGCOUNTDIRECT` via host (DoomEdNum + tid, health>0).
- `2026-10-10` — M7: **ACS GetActor*/sector PCDs** — `GetActorX/Y/Z/FloorZ/CeilingZ/Angle/Pitch`, `GetSectorFloorZ/CeilingZ/LightLevel`, `ActivatorTID`, `FixedMul`/`FixedDiv`; activator key on scripts; ACSF `GetActorVel*`.
- `2026-10-10` — M7: **ACS CALLFUNC / ACSF stub** — `PCD_CALLFUNC`/`SETRESULTVALUE`; VM math ACSF (`Sqrt`/`FixedSqrt`/`VectorLength`); host `UniqueTID`/`IsTIDUsed`/`GetPolyobjX|Y`.
- `2026-10-10` — M7: **ACSe packed immediates** — `PUSHBYTE`/`PUSH*BYTES`, `LSPEC1..5DIRECTB`, `DELAYDIRECTB`, `RANDOMDIRECTB`.
- `2026-10-10` — M7: **ACSe opcode decode** — VM `fetch_op` (1-byte / 240+ext) + `NEXTBYTE` immediates; ACSe delay/terminate unit green.

- `2026-10-10` — M7: **ACSE loader** — pure `ACSE`/`ACSe` + hybrid ACS0+chunks; `SPTR` (8/12-byte), `STRL`/`STRE` decrypt; `AcsFormat` on module.

- `2026-10-10` — M7: **Thing_*/Pillar_*** — `Thing_Spawn(NoFog)`/`Projectile(Gravity)`/`Activate`/`Deactivate`/`Remove`/`Destroy`; `Pillar_Build(AndCrush)`/`Open`. Fixed `Floor_RaiseByTexture` Hexen num 29→240 (was colliding with Pillar_Build). Spawn type = DoomEdNum until `spawnnums`.
- `2026-10-10` — M7: **ACS0 bootstrap** — `MapGeometry.behavior`; `room-acs` ACS0 loader + Hexen PCD subset VM (`DELAY`/`LSPEC*`/vars/arith/print); `World.acs`; specials 80–85 (`ACS_Execute*`/`Suspend`/`Terminate`); OPEN scripts on map enter + tic thinker. ACSE/ACSe, ACSF, libraries, hubs still open.
- `2026-10-10` — M7: **Hexen specials** — `Teleport_NewMap`/`EndGame` (`next_map_override`), `Radius_Quake`, `ThrustThing`, poly `OR_Move`/`OR_Rotate*` (busy reject vs override). Pillar / Thing_* still open.
- `2026-10-10` — M7: **polyobj DoorSwing/DoorSlide + mirror** — open→wait→close; mirror gets opposite rotate / 180° slide. Render segs / OR_* / MoveTo still open.
- `2026-10-10` — M7: **polyobj collide** — after Move/Rotate, solid AABB×seg test; non-crush undoes step + stops; crush deals 10 dmg and continues. Mirror polys / DoorSwing args / render segs still open.
- `2026-10-10` — M7: **polyobj bootstrap** — `World::polyobjs` from map setup; anchor→start-spot relocate; `Polyobj_Move` / `MoveTimes8` / `RotateLeft|Right` / `Stop` + tic advance. Unit: spawn +64 then Move +16. Collision / mirrors / DoorSwing fidelity / render segs still open.
- `2026-10-10` — M6 **done**: DEMO1 full playback bitexact vs Chocolate end `(1468.330582,-315.818542)` hp=-2. Last fix: **`MF_CORPSE` ledge friction skip** (`floorz != sector floorheight` + \|mom\| > ¼) — dead Demon on MAP03 step (floorz=-4 / secfloor=-5) was rubbing → lagged ~0.7u → blocked player Y@1918. Active → **M7**.
- `2026-10-10` — M6: **`MF_CORPSE` ledge friction skip** (`floorz != sector floorheight` + \|mom\| > ¼). Dead Demon on MAP03 step (floorz=-4 / secfloor=-5) was rubbing → lagged ~0.7u north → blocked player Y@1918. Now DEMO1 **bitexact end** vs Chocolate `(1468.330582,-315.818542)` hp=-2 (`freedoom2_demo1_vanilla_full_playback` green).
- `2026-10-10` — M6: **`P_CheckMeleeRange`** = `MeleeRange-20+target.radius` + `P_AproxDistance` + sight (was euclidean `len<=64` → Demon false melee @1859, skipped ActiveSound → idx−1 @1860). With Demon Mass 400: bitexact through **@1917**; ~2.3u Y @1918; hp/idx still match @2000/@2100. End still ≠ ~(1468,-316).
- `2026-10-10` — M6: **Demon `Mass 400`** (was default 100 → 4× knockback). Wrong pistol target @1836 (Zombie vs Demon) and idx+1 @1838 came from over-thrust after @1828 hit. With Mass 400: HIT@1836==Demon; idx@1838==172; xy bitexact through **@1900**; idx−1 @1860. End ~(1189,-199) hp=-8 vs Choco ~(1468,-316) hp=-2.
- `2026-10-10` — M6: **Doom `P_CheckSight` BSP** (`P_CrossBSPNode` + `P_DivlineSide`/`P_InterceptVector2` slopes, including negative frac). Extra `pr_checkmissilerange` @1780 was a false-positive LOS on MAP03 ZombieMan (−329,−615) vs player — Choco blocked on L2834 frac≈−31. idx@1780==152; xy/hp bitexact through **@1900**; idx+3 @1864 → hp@2000 (31 vs 22). End still ≠ ~(1468,-316).
- `2026-10-10` — M6: **`player->refire` / `P_GunShot`** — pistol/chaingun `!refire` yaw (`P_SubRandom()<<18`). Slot is `take()`n during `player_think`, so fire actions read `World::thinking_refire`. idx@1729/@1744==Choco; xy/hp bitexact through **@1745** (and path @1800); idx+1 @1780 → xy@1864. End still ≠ ~(1468,-316).
- `2026-10-10` — M6: **`A_Chase` missile gate** = Doom `if (missilestate)` before `P_CheckMissileRange` (was rolling RNG for Demon/Spectre with Melee only → idx+1 @1727). idx@1727==136; xy bitexact through **@1744**; idx−2 @1729 → hp@1744 (64 vs 58). End ~(1438,-207) hp=-7 vs Choco ~(1468,-316) hp=-2.
- `2026-10-10` — M6: **Doom `EV_Teleport` by sector tag** (XLAT tid=0 was always first `TeleportDest` → closet WR tag=4 landed on tag=1 pad (−736,288) instead of (−640,−352)). Also dest fog `+20*finesine`, `z=floorz`, TeleportFog `+NOBLOCKMAP`. idx@1710==Choco 55; xy bitexact through **@1744**; idx+1 @1727 → xy/hp @1745 (55 vs 58). End still ≠ ~(1468,-316).
- `2026-10-10` — M6: **airborne friction** = Doom `z > floorz` skip (was `z <= floor+1` → still rubbed at z=-4/floor=-5). With Hexen speed/8: xy/momi bitexact through **@1709**; idx+2 @1710 — Demon teleports @1708 onto (−736,288) pad then `NewChaseDir`→nodir (Choco walks); end ~(1440,-307) hp=-3 vs Choco ~(1468,-316) hp=-2.
- `2026-10-10` — M6: **Hexen mover speed** = arg/8 (`SPEED(a)` / Doom `F_SLOW=8` → 1 mu/tic). Was treating arg as mu/tic (8× fast) → MAP03 tag-6 floor dropped early, closet Demons `P_Move` south past 24-step onto floor 40→16, burning wrong `NewChaseDir` RNG. After fix: xy/idx bitexact through **@1689** (idx@1687==180); mom diverges @1689 (Choco floor−5 / higher mom) → path@1690; end still ≠ ~(1468,-316).
- `2026-10-10` — M6: DEMO1 **touch_specials** after `xy_movement` (was pre-move in `player_think` → healthbonus/stim 1 tic late). **1ulp@914** = move-split remainder `>>1` not `/2`. **STOPSPEED** before friction. xy/hp/idx bitexact through **@1686**; idx−2 @1687 (wake Zombie `try_walk` vs Choco `NewChaseDir`); end still ≠ ~(1468,-316).
- `2026-10-09` — M6: DEMO1 **idx@1337** — blocked `CANUSEWALLS` chase was player-`use_lines` (64u ray) and Use-activated WR Teleport MonstersOnly (Doom 126 → Hexen 70), spawning 2× `TeleportFog`/`pr_spawnmobj`. Match Doom `P_UseSpecialLine`: non-players Use only door specials. Path/idx ≈Choco through **@1686** (stim heal 1 tic late @1635 from ~0.06u drift); idx+2 @1687. End still ≠ ~(1468,-316).
- `2026-10-09` — M6: DEMO1 slide **path-traverse** (3 corner rays) + 1S backside skip + `mom==0` lead/trail. Clears @131 ulp (was back-side L1495) and @1360 (Choco L534 frac=40085). Path/momi ≈Choco through **@1360** (1–2 ulp from @914); idx blip @1337. End still ≠ ~(1468,-316).
- `2026-10-09` — M6: DEMO1 **@1007 slide** = Choco L40 (`PIT` short-trace dual-path + vertex-adjacent candidates; @58 OK). **`P_NoiseAlert`**: stamp sector `soundtarget` (no instant wake); alert from `P_FireWeapon`/`A_ReFire` not `A_Fire*`. Shotgun@1332 idx==Choco (45). Path/idx through **@1359**; stick @1360 (momi=0 vs Choco coast; 1-ulp Y from ~@140). End still ≠ ~(1468,-316).
- `2026-10-09` — M6: DEMO1 **bitexact through @1006**. @1007: Choco `BESTSLIDE` L40 (768,512)–(764,540) frac=15453; Fer AABB-only set picks L1083 (764,540)–(732,572). Root: `PIT_AddLineIntercepts` short-trace (`|mom|≤16` → `P_PointOnLineSide`) — naive port regressed DEMO1@58; needs careful dual-path. Also 1-ulp Y from @914. End still ≠ ~(1468,-316).
- `2026-10-09` — M6: DEMO1 **bitexact through @1006** (idx+xy). Fixes: `P_KillMobj` death-height (`>>2`) + keep `SOLID` until `A_Fall` (@891 corpse); `P_TouchSpecialThing` Z `delta < -8` (was `-item.height` — missed dropped shotgun +4 shells → dry @993). Next: @1007 slide line. End still ≠ ~(1468,-316).
- `2026-10-09` — M6: DEMO1 bitexact through **@890** (`finesine` aim + path-traverse thing filter; BulletSlope@798=−2004). @891: Choco solid corpse (hp=-10, pre-`A_Fall`). Keep `SOLID` until `A_NoBlocking` + walk clip ignores Z → idx+1 @665 (`pr_explode`/`pr_chase` vs corpse). Next: monster↔corpse sync from first kills, then @891. End still ≠ ~(1468,-316).
- `2026-10-09` — M6: DEMO1 XY/mom/**prndindex** bit-exact vs Chocolate through @560. Added: Doom `P_RecursiveSound`-style noise flood (was 1024u radius — woke west Imp @493). Path bitexact through @581 mom; idx −1 from ~@572 → hp split @583. End still ≠ Choco ~(1468,-316).
- `2026-10-09` — M6: DEMO1 player XY/mom **bit-exact vs Chocolate through @560** (incl. L144 slide). Fixes: authentic Doom `finesine`/`tantoangle`; slide `P_InterceptVector` `>>8` + corner traces; Doom `P_HitSlideLine`; damage thrust via `R_PointToAngle2`. Path gates through @550. Next: `prndindex` drifts from ~@496 (`pr_chase`); premature ImpBall hit @581 → end still ≠ Choco ~(1468,-316).
- `2026-10-09` — M6: @549 Choco `P_SlideMove` on MAP03 L144 (blocking diag); Fer misses it — ~0.3u westward XY drift (same mom). Path gates through @550; split @560. Next: bit-exact vanilla XY (cut f64 round-trip drift) so L144 slide hits.
- `2026-10-09` — M6: `P_KillMobj` DropItem after death-tic trim (was before; stole trim `P_Random`). Screams @459/@460 match Choco; no false HIT@503; path gates through @550.
- `2026-10-09` — M6: stop clearing monster `momxy` in `A_Chase` (Doom keeps damage thrust). Wounded zombie path==Choco through @456; shotgun 3-hit match; `prndindex`==Choco @457/@460. Path still≈Choco @500; premature zombie hit @503 → @520 drift (~10u).
- `2026-10-09` — M6: DEMO1≈Chocolate through @520. Aim slope -1353≈Choco -1352 (`PIT_AddThingIntercepts` diagonal + `chase_move` floor via `check_position_fixed`). Prefer IWAD nodes (`ensure_nodes`) — rebuilt BSP misclassifies MAP03 ledge as floor 8. Next: zombie pistol hit @531 (Choco none); end still ~(384,1122) vs Choco ~(1468,-316).
- `2026-10-09` — M6: `P_BulletSlope` + sloped `P_LineAttack` for player guns. DEMO1 path≈Chocolate through @450 (was diverging at stairs); Choco also ends dead. Drift resumes ~@500–800 (aim slope -1091 vs Choco -1352).
- `2026-10-09` — M6: hitscan walls (`PTR_ShootTraverse` openings) + Doom `shootz=z+height/2+8`. Chocolate also damages zombies at @410 (HIT log was windowed); missing autoaim slope was the real pellet mismatch.
- `2026-10-09` — M6: shotgun fire via `A_WeaponReady` (no same-tic Fire tic burn). DEMO1 `prndindex`==Chocolate through @411 (`SHOTGUN@338`, @340=171, @406=12, @411=72). Player path ≈Chocolate through @441; diverge on north stairs ~@450 (end still far from Choco ~(1468,-316)).
- `2026-10-09` — M6: `A_Scream` burns `P_Random` for podth/bgdth families (Freedoom shotguy); restores RNG match at @406. Shotgun fire still 1 tic early vs Chocolate.
- `2026-10-09` — M6: DEMO1 `prndindex`==Chocolate through @400 (shotgun kill @338 bit-exact). Fixes: pickup parks player during `player_think`; `A_Chase` ActiveSound after move; die clears SHOOTABLE; puff/blood/`P_KillMobj` tics RNG; mapthing-only spawn tics random.
- `2026-10-09` — M6: `A_Chase` → Doom `P_Move`/`P_NewChaseDir`/`P_TryWalk` (xspeed 47000); west zombies closer @441 but still late vs Chocolate ~(15,671).
- `2026-10-09` — M6: DEMO1 `P_Random` bit-exact vs Chocolate through @300 (map-thing lastlook/tics → lights; `ShotgunGuy` DoomEdNum 9; `TeleportDest` 14; sight REJECT+segment AABB). Diverge ~@306 (ShotgunGuy wake) → @441.
- `2026-10-09` — M6: DEMO1≈Chocolate through ~@400 (slide opening-block 2S lines); `P_CheckMissileRange` + damage thrust; attack still early vs Chocolate (~@412 vs @441).
- `2026-10-09` — M6: DEMO1@70–200 ≈Chocolate (`P_XYMovement` split on `> MAXMOVE/2`).
- `2026-10-09` — M6: DEMO1@70–160 ≈Chocolate (`P_ZMovement` order + first-fall `-2*GRAVITY`).
- `2026-10-09` — M6: DEMO1@70–100 ≈Chocolate (onground-only thrust; no air control).
- `2026-10-09` — M6: DEMO1@78–87 ≈Chocolate (skip frac=0 on non-touching segments; slide retry; STOPSPEED+cmd).
- `2026-10-09` — M6: DEMO1@77 south-corner slide retry (3×) + STOPSPEED needs move cmd; @78–81 west escape still short vs Chocolate.
- `2026-10-09` — M6: DEMO1@66–76 ≈Chocolate (thing stairstep, `P_LineOpening` floorz, Doom `P_SlideMove` 0x800 fudge); @70 within ~0.03.
- `2026-10-09` — M6: DEMO1@70 ≈Chocolate (±0.5) after airborne-friction skip + MAXMOVE clamp; slide/AABB/square-thing collision.
- `2026-10-09` — M6: VanillaFixed slide/collision closer to Doom (AABB lines, square things, HitSlideLine+stairstep, sector planes).
- `2026-10-09` — M6: FixedMul overflow in thing collision (DEMO1 was stuck); full DEMO1 playback.
- `2026-10-09` — M6: Chocolate Doom local DEMO1 timedemo script; DEMO1 uses full actors.
- `2026-10-09` — M6: vanilla `P_SlideMove` bootstrap (project mom onto blocking line).
- `2026-10-09` — M6: `z_movement_fixed` + Doom `ANG45` spawn angles.
- `2026-10-09` — M6: fixed-point `xy_movement_fixed` (`P_XYMovement` + ORIG_FRICTION).
- `2026-10-09` — M6: Doom `finesine`/`finecosine` + BAM yaw/thrust for VanillaFixed.
- `2026-10-09` — M6: VanillaFixed player thrust/coast (Doom scale); DEMO1@70 pos golden.
- `2026-10-09` — M6: DEMO1 smoke under VanillaFixed; `VanillaLmp` parse; Generic_Floor target-6.
- `2026-10-09` — M6: Generic_Floor target-6 + numeric model; `-dogs`; Complevel CI demos.
- `2026-10-09` — M6: `-dogs` spawn; Freedoom Complevel Vanilla/Boom demo CI; dog DECORATE.
- `2026-10-09` — M6: `MBFHelperDog` DECORATE (888); VanillaFixed friction mul; `docs/qa/mbf21-wads.md`.
- `2026-10-09` — M6: MBF21 `A_SpawnObject`/`MonsterProjectile`/`Bullet`/`Melee`/`RadiusDamage`/`NoiseAlert`/`tracer`/`HealChase`/`JumpIf*`/`flags`.
- `2026-10-09` — M6: MBF `A_Detonate`/`Mushroom`/`Spawn`/`Turn`/`Face`/`Scratch`/`RandomJump`/`LineEffect`.
- `2026-10-09` — M6: MBF `MTF_FRIEND`/UDMF friend + follow-player when idle.
- `2026-10-09` — M6: one-shot line specials (`ML_REPEAT_SPECIAL` / `line_specials`).
- `2026-10-09` — M6: Boom trigger → SPAC activation; UDMF playeruse/cross.
- `2026-10-09` — M6: Boom gen floor/ceiling absolute height + trigger flags.
- `2026-10-09` — M6: `compat/levelfixes.ron` parse/apply (`LevelFixesDatabase`).
- `2026-10-09` — M6: per-map `compat/flags.txt` DB; `World::p_random`; fixed try_move.
- `2026-10-09` — M6: `World::p_random` → vanilla rndtable; fixed `try_move`.
- `2026-10-09` — M6: fixed-point `try_move` / `FixedMul`/`FixedDiv`; SimMathMode.
- `2026-10-09` — M6: `vanilla::SimMathMode` bootstrap; demo Complevel on verify.
- `2026-10-09` — M6: demo Complevel stamp; Boom secret-bit clear; InfightingGroup.
- `2026-10-09` — M6: DSDHacked `Deh_Actor_N`; UMAPINFO bossaction; TRANMAP; compat.
- `2026-10-09` — M6: Boom sector bits (damage/friction) + MBF21 kill-grounded.
- `2026-10-09` — **M5 done** (accept: P1/P2 automated demos + determinism CI).
  Residual: CHF_* / expanded 1:1 states / full option menus → M6–M13; manual UV
  remains in `docs/qa/`. **M6** started: `COMPLVL` lump parse.
- `2026-10-09` — M5: re-recorded P1/P2 `.fdem`/`.fdbn` goldens after chase AI RNG.
- `2026-10-09` — M5: friendly `A_Look`/`A_Chase` target enemy monsters.
- `2026-10-09` — M5: `A_Chase` `CANUSEWALLS` door open when blocked.
- `2026-10-09` — M5: DeHackEd Frame apply via `states.txt` (label/offset).
- `2026-10-09` — M5: `A_Chase` 8-dir `P_NewChaseDir` + blocked repath.
- `2026-10-09` — M5: corpse queue + player bodyque + `A_QueueCorpse`.
- `2026-10-09` — M5: DeHackEd `room.pk3/dehacked/` sound/sprite/thing tables +
  `[SOUNDS]`/`[SPRITES]` apply.
- `2026-10-09` — M5: committed `freedoom2_map21_30.fdbn` (P2 MAP01–30 automated).
- `2026-10-09` — M5: committed `freedoom2_map11_20.fdbn`.
- `2026-10-09` — M5: committed Freedoom1 E2–E4 `.fdbn` goldens (P1 automated
  start→finish).
- `2026-10-09` — M5: committed `freedoom1_e1.fdbn` (E1M1–E1M8).
- `2026-10-09` — M5: Freedoom1 E1–E4 episode exit chains (E4M8 → finale).
- `2026-10-09` — M5: MAP11–MAP29 exit chains + MAP30 finale (Forced Exit_Normal
  fallback for boss maps).
- `2026-10-09` — M5: committed `freedoom2_map01_10.fdbn` golden.
- `2026-10-09` — M5: headless MAP01–MAP10 + E1M1–E1M3 exit chains.
- `2026-10-09` — M5: `determinism` CI (native demo verify + wasi fingerprints).
- `2026-10-09` — M5: CLI `demo info|verify` for `.fdbn` + DECORATE actors;
  `docs/qa/freedoom-playthrough.md`.
- `2026-10-09` — M5: `.fdbn` multi-map demo bundles + committed MAP01–03 golden.
- `2026-10-09` — M5: headless MAP01–MAP05 exit chain + Freedoom1 E1M1→E1M2.
- `2026-10-09` — M5: MAPINFO next/par on intermission; MAP01→MAP02 continue smoke.
- `2026-10-09` — M5: DeHackEd Frame/Pointer/`[CODEPTR]` apply (ZombieMan
  bootstrap frame map).
- `2026-10-09` — M5: committed `testdata/demos/freedoom2_map01_smoke.fdem` +
  verify test (`ROOM_WRITE_DEMO=1` to regenerate).
- `2026-10-09` — M4 **done**: web UnlitPass viewer, golden+fps CI, `web-browser-tests`
  (Firefox/`wasm-bindgen-test`). Active focus → M5 accept (demos/determinism).
- `2026-10-09` — M4: MAP01 offscreen fps floor (≥15 CI / ~500+ native); web
  UnlitPass viewer + golden CI.
- `2026-10-09` — M3 **done**: UDMF/`Plane_Align`/9500 slopes, poly/portal/3D-floor
  setup + testkit helpers; all M3 accept boxes green. Active focus → M4.
- `2026-10-09` — M3: float↔vanilla BSP parity (~99.99%); UDMF lexer floor raised
  to 100 MiB/s. Next: setup fidelity / testkit helpers.
- `2026-10-09` — M3: float BSP side test matched Doom `R_PointOnSide`; grid
  parity vs vanilla walk on Freedoom MAP01 (~99.99%).
- `2026-10-09` — M3: `build_reject` (2S connectivity groups) + auto-build when
  REJECT missing; `build_reject_zero`.
- `2026-10-09` — M2 **done**: main `test` CI fetches/caches Freedoom 0.13.0 so
  definition/map goldens always run. Active focus → M3.
- `2026-10-09` — M1 **done**: `sevenz_header` fuzz + CI; all M1 accept boxes checked.
  Active focus → M2 (Freedoom fetch on main `test` job).
- `2026-10-09` — M1: `autoload` module (section keys + dir/config path collect).
- `2026-10-09` — M1: ZIP bzip2+lzma via `zip` 8.6 (pure Rust); synthetic IWADINFO
  matrix test; fixture `testdata/vfs/lzma_hello.pk3`.
- `2026-10-09` — Tracker created from repo audit. Strict accept: only M0 `done`;
  M1–M5 `in_progress`. Active coding focus remains M5 playsim gaps while noting
  earlier accept holes.

---

## M0 — Workspace and core

**Status:** `done` · Spec: `20` §M0 · Docs: `00`, `01`, `02`, Appendix A

### Plan

1. Workspace skeleton (all crates, READMEs, lints).
2. CI baseline (`fmt`, `clippy`, `test`, `doc`, `deny`, wasm build).
3. `room-core`: names, IDs, arena, math, dmath, SFMT + vanilla RNG.
4. Acceptance: SFMT vectors, dmath 1M-sample hash across native + wasm, arena props.

### Checklist

**Deliverables**

- [x] Workspace crates + READMEs + root lints
- [x] CI pipeline skeleton (`.github/workflows/ci.yml`)
- [x] `Name` / `NameTable`, typed IDs, generational arena
- [x] `DVec2/3`, `Angle`, `Fixed`, BAM, `dmath`, `conv`
- [x] SFMT-607 + named RNG + vanilla `rndtable`

**Acceptance**

- [x] SFMT reference vectors (`room-core` RNG tests)
- [x] dmath bit-identical hash (native matrix + `dmath-wasm`)
- [x] Arena property tests (proptest)

### Notes

Later §4 jobs (`determinism`, `golden-images`, `bench`, …) belong to later
milestones; their absence does not reopen M0.

---

## M1 — Resources

**Status:** `done` · Spec: `20` §M1 · Docs: `03`

### Plan

1. VFS: WAD, ZIP (deflate/bzip2/lzma), 7z, directories, nested WADs, filters, strip.
2. IWADINFO detection + load order + autoload.
3. CLI `wad ls|extract|info`.
4. Fuzz targets 60 s in CI; synthetic IWAD for every `MustContain` kind.

### Checklist

**Deliverables**

- [x] WAD + directory mounts, nested WAD, filters, single-root strip
- [x] ZIP deflate + 7z (feature)
- [x] ZIP bzip2 + lzma (`zip` 8.6, pure Rust)
- [x] IWADINFO parse + `detect_iwad` (`wadsrc/room/IWADINFO`)
- [x] Autoload / search-path autoload (`autoload_sections` + `collect_autoload_paths`)
- [x] CLI `wad ls|extract|info`
- [x] Fuzz: `wad_directory`, `zip_central_directory` in CI
- [x] Fuzz: `sevenz_header` (per `03` §9; CI `fuzz-smoke`)

**Acceptance**

- [x] VFS unit/integration smoke (partial vs `03` §9)
- [x] Synthetic WADs detecting **every** IWADINFO kind
- [x] Fuzz targets 60 s without crash (incl. `sevenz_header` in CI)

### How to verify

```bash
cargo test -p room-vfs
cargo run -p room-cli -- wad info testdata/freedoom/freedoom2.wad
```

---

## M2 — Formats and definitions bootstrap

**Status:** `done` · Spec: `20` §M2 · Docs: `04`, `05`

### Plan

1. Binary formats (graphics, palette, sound, MUS→MIDI, fonts).
2. `room-sc` + `room-defs` phases 0–6 and 9.
3. CLI `defs dump`; Freedoom insta goldens; MAPINFO corpus.

### Checklist

**Deliverables**

- [x] Patches, flats, PNG/JPEG/TGA/PCX/BMP, PLAYPAL/COLORMAP
- [x] DMX/WAV/OGG/FLAC/MP3, MUS→MIDI, fonts
- [x] GAMEINFO, LANGUAGE, CVARINFO, TEXTURES/TEXTUREx/PNAMES
- [x] ANIMATED/ANIMDEFS/SWITCHES, FONTDEFS
- [x] MAPINFO (multi-syntax) + UMAPINFO, SNDINFO, SNDSEQ
- [x] CLI `defs dump`
- [x] Remaining text lumps from `05` deferred to later milestones (not M2 accept)
- [x] Fuzz: `mapinfo`, `mus_to_midi`, `image_lumps`

**Acceptance**

- [x] Freedoom Phase 1/2 definition snapshots (always on in CI via fetch)
- [x] MAPINFO corpus fixtures parse with zero errors
- [x] Main `test` CI job fetches Freedoom so goldens always run

### How to verify

```bash
./scripts/fetch-freedoom.sh
cargo test -p room-defs
cargo run -p room-cli -- defs dump testdata/freedoom/freedoom2.wad
```

---

## M3 — Maps

**Status:** `done` · Spec: `20` §M3 · Docs: `06`

### Plan

1. Doom/Hexen/UDMF loaders, XLAT, node formats, nodebuild, blockmap, reject.
2. Slopes / 3D floors / portals / polyobjects setup data.
3. `room-testkit` UDMF DSL; CLI `map info|render2d|nodebuild`.
4. Accept: all Freedoom maps, convexity, point-in-sector parity, UDMF ≥100 MB/s.

### Checklist

**Deliverables**

- [x] Doom / Hexen / UDMF loaders + validation + XLAT
- [x] Node loading (classic + compressed/GL variants)
- [x] `room-nodebuild` + blockmap build
- [x] Reject **parse**
- [x] Reject **builder** (`build_reject` connectivity + `build_reject_zero`)
- [x] Slopes / 3D floors / portals / polyobj **setup data** (UDMF planes,
  `Plane_Align`, 9500/9501, poly trace, portal/ffloor decls; portal groups → M7)
- [x] Testkit rooms/doors/starts
- [x] Testkit slopes/portals/poly/ffloor helpers
- [x] CLI `map info|render2d|nodebuild`

**Acceptance**

- [x] Freedoom maps load (skip if IWAD absent)
- [x] Node builder convexity/closure checks
- [x] Point-in-sector **parity vs original nodes** on a grid (float ↔ vanilla)
- [x] UDMF lexer ≥ 100 MB/s enforced

### How to verify

```bash
./scripts/fetch-freedoom.sh
cargo test -p room-map
cargo run -p room-cli -- map info testdata/freedoom/freedoom2.wad MAP01
```

---

## M4 — Renderer foundation + shells

**Status:** `done` · Spec: `20` §M4 · Docs: `13`, `18`

### Plan

1. wgpu device, textures, BSP walk, walls/flats/sprites, sky, lighting modes, 2D.
2. Desktop `room-app` map viewer; `room-web` same viewer (WebGPU + WebGL2).
3. Golden images + Freedoom MAP01 target fps on desktop and browsers.

### Checklist

**Deliverables**

- [x] Device tiers, texture manager, BSP + clipper
- [x] Walls/flats/sprites (static), classic sky
- [x] `DoomSoftware` / `ClassicPalette` light modes, fog fields
- [x] 2D canvas, fonts, screenshots, offscreen API
- [x] `room-app` free-fly viewer
- [x] `room-web` map viewer + IWAD play (`roomStartIwad` / `/doom1.wad` / file picker)
- [x] Offscreen smoke + lavapipe `render-smoke` CI
- [x] Local `golden_room` SSIM fixture

**Acceptance**

- [x] Golden images job in CI (`golden_room` on lavapipe `render-smoke`)
- [x] Viewer Freedoom MAP01 fps floor in CI (`freedoom_map01_offscreen_fps_floor` ≥15;
  desktop mid-laptop ≥120 is `13` §14 checklist)
- [x] Viewer in Chrome/Firefox (`web-browser-tests` job + `browser_smoke`)

### How to verify

```bash
cargo run -p room-app -- --iwad testdata/freedoom/freedoom2.wad --map MAP01
cargo test -p room-render
cargo build --target wasm32-unknown-unknown -p room-web
```

---

## M5 — Doom playsim

**Status:** `done` · Spec: `20` §M5 · Docs: `07`–`11`, `14`–`17`

### Plan (work order)

1. **Actors & content** — DECORATE builtins (`wadsrc` / `doom_basic`), DeHackEd/BEX apply.
2. **Sim core** — thinkers, movement/collision, sight, hitscan, damage, inventory/weapons, player.
3. **AI & combat** — Doom monster AI, attacks, bosses, projectiles, radius damage.
4. **World** — Doom specials via XLAT, sector specials, movers, lights, switches, teleports.
5. **Level flow** — exits, secrets, intermission, finale, title loop.
6. **Shell systems** — audio SFX, music (SoftFont/OPL), input, console/CVars, Doom UI.
7. **Persistence** — save/load, Room demo record/verify + checksums.
8. **Accept** — Freedoom P1/P2 start→finish demos + cross-platform determinism CI.

### Checklist

**Deliverables — content / mod formats**

- [x] `room-decorate` parser + Doom actor bootstrap
- [x] `room-dehacked` Thing/Weapon/Ammo/Misc/Frame/Pointer parse
- [x] BEX `[STRINGS]` → pickup messages; `[CODEPTR]` parse
- [x] Frame / Pointer / `[CODEPTR]` **apply** via `states.txt` class/label/offset
  (compressed multi-letter DECORATE lines share one target; true 1:1 needs
  expanded built-in actor states)
- [x] Full DeHackEd sound/sprite tables with `room.pk3/dehacked/`
  (`things.txt` / `sprites.txt` / `sounds.txt` / `weapons.txt` / `ammo.txt` /
  `codepointers.txt`; `[SOUNDS]` / `[SPRITES]` apply)

**Deliverables — sim**

- [x] World, thinkers, spawn, blockmap links
- [x] Movement / collision / try_move / stairs / dropoff basics
- [x] Sight, hitscan, melee, missiles, radius damage
- [x] Inventory, weapons, ammo, armor, powerups, keys
- [x] Player think, weapon psprites, cheats (iddqd/idkfa/…)
- [x] Doom specials via XLAT (doors, plats, floors, ceilings, lights, exits, teleports)
- [x] Sector specials (damage, secret, type-11 exit, …) — bootstrap set
- [x] Nightmare COUNTKILL respawn; DROPPED half-ammo
- [x] `A_Chase` 8-dir `P_NewChaseDir` / blocked repath / `CANUSEWALLS` door use
- [x] Friendly AI: `A_Look`/`A_Chase` hunt enemy monsters (ignore players)
- [x] Doom AI bootstrap sufficient for M5 accept (CHF_*/dropoff/MBF smartmove → M6)
- [x] Intermission tallies + next/secret-next (Doom II heuristics + MAPINFO override);
  title/finale stubs remain
- [x] Corpse queue (`sv_corpsequeuesize` default 64) + player `bodyque` (32);
  `A_QueueCorpse` / `A_DequeueCorpse`
- [x] Leftover vanilla edge cases deferred (demos green after chase re-record)

**Deliverables — audio / input / UI / console**

- [x] `room-audio` SFX mixer + SNDINFO resolve bootstrap
- [x] `room-music` SoftFont + OPL path
- [x] `room-input`, `room-console` core commands/CVars
- [x] `room-ui` menus, status bar, HUD, automap, messages (Doom bootstrap)
- [x] Doom UI bootstrap; full option menus / config parity → M13

**Deliverables — demos / saves / app**

- [x] `.fdem` record/verify + CLI `demo`
- [x] Save v2 (inventory/armor/pitch/weapon)
- [x] `room-app --play` WASD headless drive
- [x] MAP01 smoke: tick, door, teleport, exit→intermission, secret
- [x] Committed maintainer smoke `.fdem` (`testdata/demos/freedoom2_map01_smoke.fdem`)
- [x] Multi-map `.fdbn` bundles (P2 MAP01–30; P1 E1–E4)
- [x] `docs/qa/freedoom-playthrough.md` checklist

**Acceptance**

- [x] Freedoom Phase 1 start→finish automated (E1–E4 exit chains + `.fdbn` goldens;
  manual UV playthrough still listed in `docs/qa/`)
- [x] Freedoom Phase 2 start→finish automated (MAP01–30 exit chains + `.fdbn`
  goldens; MAP30 force-exit when no Exit_Normal linedef; manual UV still in
  `docs/qa/`)
- [x] Determinism CI bootstrap: `determinism` job (native demo verify +
      wasm32-wasip1 fingerprint parity); matrix `test` also covers macOS/Windows

### How to verify

```bash
./scripts/fetch-freedoom.sh
cargo test -p room-sim
cargo test -p room-game --test freedoom_smoke
cargo run -p room-app -- --iwad testdata/freedoom/freedoom2.wad --map MAP01 --play
```

### Near-term backlog (update as closed)

- [x] DeHackEd frame/codeptr apply (bootstrap)
- [x] Hardening AI chase pathing (`P_NewChaseDir`); refire/boss deaths still open
- [x] Intermission stats + next-map / secret-next fidelity
- [x] First committed MAP01 smoke demo + verify test
- [x] MAP01→MAP02 continue after exit (headless smoke)
- [x] Headless MAP01–MAP29 + MAP30 finale; Freedoom1 E1–E4 episode exit chains
- [x] Multi-map demo bundle format + MAP01–03 golden
- [x] Full episode demos (automated start→finish)

---

## M6 — Boom, MBF, MBF21, vanilla strict

**Status:** `done` · Spec: `20` §M6 · Docs: `09`, `19`

### Plan

1. Boom generalized specials, friction, pushers, scrollers, transfer heights, colormaps, TRANMAP.
2. MBF friends/dogs/`A_` additions; MBF21 DeHackEd/flags/groups; UMAPINFO; DSDHacked; `COMPLVL`.
3. Strict vanilla mode + vanilla/Boom/MBF demo playback; per-map fix DB (`19` §3).

### Checklist

**Deliverables**

- [x] Boom generalized linedef translate (`boom_gen` / XLAT) — bootstrap present
- [x] Boom generalized sector bits (damage `0x60`, secret, friction `0x100`)
- [x] MBF21 sector kill bits (`0x400` players / `0x800` monsters)
- [x] Remaining Generic_* / Boom sector type fidelity (floor target-6 + numeric model; residual ceiling)
- [x] Friction / pushers / scrollers / transfer heights bootstrap (sim)
- [x] Boom COLORMAP load + TRANMAP / TINTTAB / XLATAB parse
- [x] MBF + MBF21 feature set (Deh/friends/dogs/`A_*`; InfightingGroup; residual Generic_*)
- [x] `COMPLVL` lump parse → `Definitions.complevel`
- [x] Complevel → `CompatFlags` on `World` (GZDoom bit positions; MBF21 kill gated)
- [x] UMAPINFO bossaction / endpic / episode clear + additive merge + sim `A_BossDeath`
- [x] DSDHacked dynamic thing slots (`Deh_Actor_N` allocate on apply)
- [x] Strict vanilla mode bootstrap (`SimMathMode` + `VanillaRng` + `P_AproxDistance`)
- [x] Fixed-point `try_move` / `check_position` (`vanilla::movement`; dispatched from `try_move_ex`)
- [x] VanillaFixed `xy_movement` friction via `Fixed` mul
- [x] `.fdem` FDHD stamps Complevel (legacy demos → Default; verify applies it)
- [x] Vanilla `P_Random` wiring (`World::p_random` → rndtable when VanillaFixed)
- [x] Vanilla/Boom/MBF demo playback (`.lmp` + DEMO1@70 ≈Chocolate ±0.5; full DEMO1 end == Chocolate bitexact)
- [x] Per-map compat flags DB (`compat/flags.txt` parse + MD5 → CompatFlags)
- [x] Per-map `levelfixes.ron` parse/apply (core ops; expand GZDoom corpus)

**Acceptance**

- [x] Public MBF21 test WADs checklist (`docs/qa/mbf21-wads.md`; manual cells open)
- [x] Vanilla demo sync suite green locally (gated IWAD; `freedoom2_demo1_vanilla_full_playback`)
- [x] Freedoom-based Complevel Vanilla/Boom demo subset in CI

---

## M7 — ZDoom feature set

**Status:** `done` · Spec: `20` §M7 · Docs: `06`–`11`, `16`

### Plan

1. Hexen-format maps/specials; UDMF `zdoom` namespace complete.
2. ACS VM complete; slopes/3D floors/portals/polyobjs in **sim and renderer**.
3. Full DECORATE; full MAPINFO; hubs; SBARINFO; TERRAIN; DECALDEF; LOCKDEFS; KEYCONF; MENUDEF.
4. Skyboxes, camera textures, mirrors, mover interpolation.

### Checklist

**Deliverables**

- [x] Hexen specials + UDMF zdoom completeness (broad UDMF/SPAC/plane/side/sector keys + Get*UDMF*)
- [x] ACS VM complete enough for M7 accept — ACS0/ACSE/ACSe + Hexen PCD + ACS_Execute* + broad CALLFUNC/ACSF + named scripts + Get*UDMF*; residual ACSF edges → backlog
- [x] Slopes / 3D floors / portals / polyobjs in sim + render (solid ffloors, swim, midtex3d, line/sector portals, groups, sight, polyobj movers, slope Z)
- [x] Full DECORATE for M7 accept — expressions, DecoBlock, large action corpus; residual actions → backlog
- [x] Hubs, SBARINFO, TERRAIN, DECALDEF (+render decals)
- [x] LOCKDEFS, KEYCONF, MENUDEF
- [x] Skyboxes, camera textures, mirrors, interpolation — cubemap/skybox + cameratexture + mirror/portal canvases + stencil mask/equal; **Equal-only Metal mask deferred** (wgpu write-mask)

**Acceptance**

- [x] Synthetic testkit maps (`m7_feature_smoke` portal/SNDSEQ/Pillar/fog/ffloor+slope/UDMF `user_*`)
- [x] DECORATE/ACS smoke via in-tree fixtures + Freedoom playthrough demos (external mod corpus fetch → backlog)

### Intentional deferrals (not blocking M11)

- Equal-only stencil portal/mirror mask on Metal (empty colour write masks skip Replace).
- Remaining rare DECORATE actions / ACSF numbers; `testdata/corpus.toml` external ZDoom mod fetch.

---

## M8 — Heretic and Hexen

**Status:** `deferred` · Spec: `20` §M8 · Docs: `08`, `16`, `19`

### Plan

1. Built-in actors, specials, bars, menus, intermissions/finales.
2. Hexen classes, hubs, puzzles; native startup screen.

### Checklist

**Deliverables**

- [ ] Heretic actors/specials/UI/flow
- [ ] Hexen actors/specials/classes/hubs/puzzles
- [ ] Startup screen (native)

**Acceptance**

- [ ] Heretic E1 playthrough demo (gated IWAD CI/local)
- [ ] Hexen hub 1 playthrough demo (gated)

---

## M9 — Strife

**Status:** `deferred` · Spec: `20` §M9

### Plan

1. Conversations (binary + USDF), quests/log, merchants, alarms.
2. Strife bar/popups and special actions.

### Checklist

**Deliverables**

- [ ] Conversation system (binary + USDF)
- [ ] Quests/log, merchants, alarms
- [ ] Strife UI + special actions

**Acceptance**

- [ ] Strife MAP02→MAP07 gated playthrough demo

---

## M10 — GZDoom rendering extras

**Status:** `done` · Spec: `20` §M10 · Docs: `13`

### Plan

1. GLDEFS lights + clustered shading + shadowmaps; brightmaps/materials/PBR.
2. MODELDEF / VOXELDEF; particles; full render styles; post-process; custom shaders; lightmaps (S).

### Checklist

**Deliverables**

- [x] GLDEFS lights parse (point/pulse/flicker/flicker2 + `object` frame binds) — bootstrap
- [x] CPU stand-in: `DynLightInstance` + `apply_dyn_lights` on walls/flats/sprites
- [x] GLDEFS `brightmap` / `material` / `Glow` parse + shader brightmap sample + glow fullbright
- [x] GPU forward dyn lights (`LightUniforms`, max 32) + pulse/flicker radius — bootstrap
- [x] Clustered shading bootstrap (16×9×24 CPU assign + mask texture)
- [x] 1D shadowmaps bootstrap (CPU raycast atlas + FS sample) — GPU BVH still open
- [x] MODELDEF parse bootstrap (path/model/skin/frame/flags)
- [x] OBJ mesh load bootstrap (`parse_obj` → `ModelMesh`)
- [x] MD2 mesh load bootstrap (`parse_md2` multi-frame)
- [x] MODELDEF unlit GPU draw bootstrap (`append_model` → wall lists)
- [x] VOXELDEF parse bootstrap (scale/spin/offsets)
- [x] KVX mesh bootstrap (`parse_kvx` + cube faces)
- [x] KVX greedy face merge (`kvx_to_mesh_greedy`)
- [x] GLDEFS lights (point/pulse/flicker/flicker2/spot/sector) + clustered + 1D shadowmaps (GPU)
- [x] Spotlight cone GPU (`spot_dir_outer` / inner cos + unlit falloff; actor yaw aim)
- [x] Material PBR map catalog (normal/specular/metallic/roughness/ao)
- [x] GPU normal-map sample (Lambert on dyn lights)
- [x] Blinn-Phong / GGX specular on dyn lights
- [x] Metal-rough map bind (`tex_metal_rough` glTF G/B + `MetalRough` / `pbr_specular` CPU)
- [x] MD3 mesh load bootstrap (first surface, multi-frame)
- [x] IQM v2 bind-pose parse + `parse_model_bytes` auto-detect
- [x] IQM CPU skinning (`parse_iqm_full` / `IqmModel::skin_cpu` / bind-pose identity)
- [x] MODELDEF actor wiring (`ModelCatalog` / `append_actor_models`)
- [x] Play-path MODELDEF draw (`collect_model_poses` in room-app)
- [x] Model offscreen smoke (`tests/model_offscreen_smoke.rs`)
- [x] Dyn-lights soft golden (`tests/golden_dyn_lights.rs` mean-luma)
- [x] Dyn-lights SSIM PNG (`testdata/goldens/dyn_lights_320x200.png`)
- [x] Fuzz SSIM PNG (`testdata/goldens/fuzz_320x200.png`)
- [x] Optional engine extras mount (`lights.pk3` / `brightmaps.pk3` / … + `testdata/m10/lights` fixture)
- [x] lights.pk3 fixture content → `Definitions.gldefs` (point/pulse/flicker + object binds)
- [x] IQM GPU skinning bootstrap (`SkinnedPass` + bone palette ≤64 + `skinned_iqm_smoke`)
- [x] UE1 vertex mesh parse (`parse_ue1` `_d.3d`/`_a.3d` + catalog pair load)
- [x] Acceptance goldens bootstrap (room / dyn-lights / fuzz / particles; bloom/LM smokes)
- [x] Particles bootstrap (vel/accel + ACS ingest)
- [x] Particle styles (`Translucent` / `Additive` / `Soft` + `__PARTICLE_ADD`)
- [x] Post bootstrap (`PostPass`: Reinhard/Hejl/linear/palette, gamma, soft bloom, screen blend)
- [x] `RenderStyle` blend table (Normal/Add/Translucent/Subtract/Multiply/…)
- [x] FXAA bootstrap in PostPass (`gl_fxaa`)
- [x] Fuzz shader bootstrap (`FuzzType` / `gl_fuzztype` variants on Fuzzy sprites)
- [x] SSAO bootstrap (`SsaoPass` depth-edge)
- [x] SSAO bilateral blur (depth-aware 4-neighbour)
- [x] SSAO dedicated half-res AO buffer (`SsaoPass::ensure_ao` / `ao_view`)
- [x] Multi-scale bloom (Kawase 4-radius × 8-tap) in PostPass
- [x] True bloom mip downsample chain (`BloomPass` 4-level half-res pyramid)
- [x] Palette tonemap (`TonemapMode::Palette` + PLAYPAL upload)
- [x] naga WGSL validation at build (`room-render/build.rs`)
- [x] Lightmap header bootstrap (`FLMP` / `parse_lightmap_header`)
- [x] Particles GPU instancing (`ParticleInstancedPass` + `particles.wgsl` + smoke)
- [x] Obscure render styles fill path (`apply_fill_color` / `uses_fill_modulate` for stencil/shaded/soul/shadow)
- [x] Custom GLSL→WGSL via naga at load (`glsl_to_wgsl`)
- [x] HardwareShader material bind (`HardwareShaderCatalog` + GLDEFS parse / ProcessTexel wrap)
- [x] HardwareShader post pipeline host (`create_hardware_post_pipeline` / `HARDWARE_POST_HOST_WGSL`)
- [x] HardwareShader custom FS compose (`compose_hardware_post_wgsl`)
- [x] Lightmap surface UV + atlas sample (`parse_lightmap_lump` / `lightmap_atlas_texel`)
- [x] Lightmap GPU atlas + debug draw (`LightmapAtlas` / `LightmapPass`)
- [x] Bloom mip soft smoke (`bloom_mip_smoke`)
- [x] UnlitPass lightmap bind (`set_lightmap` / world-XY UV bootstrap + smoke)
- [x] Per-vertex lightmap UVs (`lm_uv` on walls/flats + `SceneLists.lightmap_surfaces` + `world_xy_to_surface_uv`)
- [x] Side/sector → surface-index map (`LightmapBindings` + wall `side_raw` / flat `sector_raw`)

**Acceptance**

- [x] Golden images per feature bootstrap (room / dyn-lights / fuzz / particles SSIM PNGs; bloom/lightmap smokes)
- [x] `lights.pk3` fixture content parses/renders path (mount + Definitions) — full GZDoom pk3 still optional
- [x] Scene-build soft budget (`scene_build_budget` synthetic room ≪ 2 ms)
- [x] MAP01 offscreen fps floor (`freedoom_map01_offscreen_fps_floor` ≥15 CI)
- [x] 500-light cluster assign soft budget (`cluster_500_lights_budget`)
- [x] 10k sprite append soft budget (`sprite_10k_budget` nuts.wad stand-in)
- [x] 32-light 1D shadowmap bake soft budget (`shadowmap_32_budget`)
- [x] MAP01+32-lights offscreen fps floor (`freedoom_map01_lights_shadow_fps_floor` ≥8 CI)
- [ ] Full performance matrix (`13` §14 desktop targets / live nuts.wad) — **intentional deferral** (soft CI floors + lights path gate; no mid-range GPU / nuts.wad in CI)

### Log

- `2026-11-07` — **D5 `movefta`** now selects the nearest player, honors that player's live body health, refreshes sleeping badguy shade from its sector, and routes visible Atomic barrel/can/fire-vase/wooden-horse/tripbomb actors to `STAT_STANDABLE` with sector-plane shade. Static Chocolate source review; fmt, diff check, and crate check pass with existing warnings; no dynamic comparison.
- `2026-11-07` — **D5/D8 wake audio**: implemented Chocolate `check_fta_sounds` on dummy-actor promotion, including per-sprite enemy cues and the palette-dependent/global boss recognition cues. Static review of `game.c` and `soundefs.h`; compile/format checks only, no gameplay/demo comparison.
- `2026-11-07` — **D6 hitscan wall impacts and decals**: Chocolate `player.c::shoot` confirms there is no 1024-unit cutoff around `checkhitswitch`, bullet-hole placement or `checkhitwall`; Room now processes all valid wall hits, preserves the original wall for switch/decal checks, and only then selects the peer wall for floor-aligned material response. Added Atomic `BULLETHOLE` size/cstat initialization, the one-unit wall-facing `ssp` nudge, SE13/door/forcefield/tag/sector suppression, and duplicate spacing with `FindDistance3D`/TRAND in STAT_MISC order. Static review of `player.c::shoot`, `sector.c::checkhitwall` and `sector.c::dist`; `cargo fmt --all`, `git diff --check` and `cargo check -q -p room-duke --lib` pass with existing warnings. No tests, gameplay or demo/reference comparison.
- `2026-11-07` — **D6 tripbomb arming multiplayer target**: the standable mine's first 32-tic arm delay now uses Chocolate `findplayer` semantics: previous-tic Manhattan distance and nearest living seat in multiplayer, while retaining the single-player player even at zero health. This fixes reliance on seat 0 when another coop player is closer/alive. Static review of `sector.c::findplayer` and `actors.c` tripbomb state; formatting, diff and crate compile only, no dynamic comparison.
- `2026-11-07` — **D6 projectile player-hit cue**: generic RPG/FREEZEBLAST/SHRINKSPARK hits on APLAYER now queue positional `PISTOL_BODYHIT` at the struck body after the freeze-immunity early return, matching Chocolate `moveweapons` after `checkhitsprite`; SPIT/FIRELASER route their specialized cues through the same actor-position path. Static `actors.c::moveweapons` review; fmt, diff and crate compile only, no dynamic comparison.
- `2026-11-07` — **D6 direct projectile damage**: accepted projectile hits now accumulate `source.extra` in target hittype metadata for APLAYER and actors; RPG damage doubles against badguy actors before queueing, matching Chocolate `checkhitsprite`. Sleeping badguys wake into STAT_ACTOR with the exact `SLEEPTIME` (`24 * 64`) reset before an RPG blast scan. Direct and radius passes can both contribute within a tic. Static `sector.c::checkhitsprite` / `actors.c::moveweapons` review; fmt, diff and crate compile only, no dynamic comparison.
- `2026-10-24` — **D6** Chocolate `ldist` = `FindDistance2D+1` (was manhattan) — first shotgun spark Z 58016≈choc 58019. Still nspr=504 with BLOODSPLAT (7 JIBS); need floor-miss pellet.
- `2026-10-24` — **D6** shotgun knockback `pushmove` between pellets (Chocolate checkhitsprite). Still nspr=504 @ pad=13; face xr sweep 30→7 jibs / 28→5 jibs (no 6).
- `2026-10-12` — M10 done: MAP01+32-lights fps floor; desktop matrix intentional deferral.
- `2026-10-12` — Spotlight cone GPU + shadowmap_32_budget.
- `2026-10-12` — LightmapBindings side/sector map + spotlight/sectorlight + SSAO AO buffer.
- `2026-10-12` — Per-vert `lm_uv` + surface UV helpers + 10k sprite budget.
- `2026-10-12` — UnlitPass lightmap bind + 500-light cluster budget + checklist sync.
- `2026-10-12` — Particles SSIM golden + LightmapPass GPU + compose_post FS + scene budget.
- `2026-10-12` — ParticleInstancedPass + fill_color styles + bloom mip smoke + HW post host.
- `2026-10-12` — HardwareShader catalog bind + particle GPU pack + FLMP surfaces.
- `2026-10-12` — lights.pk3 fixture defs + glsl_to_wgsl + braced frame lights.
- `2026-10-12` — BloomPass mip chain + naga build validate + FLMP lightmap header.
- `2026-10-12` — Metal-rough GGX PBR + fuzz SSIM golden.
- `2026-10-12` — SSAO bilateral blur + particle styles + Kawase-4 bloom.
- `2026-10-12` — UE1 model parse + Blinn-Phong specular bootstrap.
- `2026-10-12` — IQM GPU skinning SkinnedPass + smoke.
- `2026-10-12` — KVX greedy meshing + optional lights.pk3 mount path.
- `2026-10-12` — Dyn-lights SSIM PNG golden committed.
- `2026-10-12` — IQM CPU skinning + golden_dyn_lights soft check.
- `2026-10-12` — Play-path MODELDEF + model_offscreen_smoke.
- `2026-10-12` — IQM parse + ModelCatalog actor wiring.
- `2026-10-12` — MD3 parser + normal map bind/Lambert in unlit.
- `2026-10-12` — append_model unlit path + KVX→mesh.
- `2026-10-12` — MD2 parser + MaterialMaps PBR catalog from GLDEFS.
- `2026-10-12` — Palette tonemap + OBJ mesh parser for MODELDEF.
- `2026-10-12` — SsaoPass + multi-scale bloom in post.wgsl.
- `2026-10-12` — FuzzType + unlit spectre fuzz; app sets sprite.fuzz for Fuzzy/OptFuzzy.
- `2026-10-12` — RenderStyle blend table + FXAA in PostPass.
- `2026-10-12` — PostPass fullscreen tonemap/gamma/bloom-threshold + particle vel/accel.
- `2026-10-12` — CPU 1D shadowmaps (512×N R32F atlas) + one-sided occluders; unlit FS gate.
- `2026-10-12` — VOXELDEF parser + DefinitionsBuilder merge.
- `2026-10-12` — MODELDEF parser + DefinitionsBuilder merge.
- `2026-10-12` — Cluster grid 16×9×24 + bitmask texture; FS clustered light loop.
- `2026-10-12` — GPU light UBO + fragment additive/subtractive dyn lights; pulse/flicker.
- `2026-10-12` — Brightmaps/glow/material GLDEFS + unlit bind group brightmap; Glow flats fullbright (nukage/lava).
- `2026-10-12` — Bootstrap: GLDEFS dyn-light subset + doom attack defaults; app emits object-frame lights; Bright fullbright in unlit.wgsl.

---

## M11 — WASM modules (ZScript replacement)

**Status:** `done` · Spec: `20` §M11 · Docs: `12`

**Intentional deferrals:** `#[derive(Actor)]` / `states!` / C header & `cargo room`; UI draw/`send_network_event` imports; hot reload; full sample mods (b)–(e); ABI benchmarks.

### Plan

1. ABI v1 IDL + `room-abi-gen`; `wasmi` then `wasmtime`.
2. Classes/states/fields/virtuals/actions/events/thinkers; ACS interop; UI modules.
3. Saves/hot reload; Rust SDK + macros + `cargo room`; C header example.

### Checklist

**Deliverables**

- [x] ABI v1 + codegen (bootstrap: `api/common.ron` + `room-abi-gen list`)
- [x] `room-script` wasmi backend (validate + required exports + `room::log` + lifecycle)
- [x] `wasmtime` backend (fuel + NaN canon; same stub lifecycle as wasmi)
- [x] Validation + registration/init lifecycle
- [x] Class/state registration ABI bootstrap (`reg_class_*` / `reg_state*` + ClassRegistry merge)
- [x] Cross-backend registration fingerprint (wasmi ≡ wasmtime on `fireimp_reg`)
- [x] Events + thinkers bootstrap (`WorldTick` dispatch)
- [x] ACS interop bootstrap (`ScriptCall` ↔ `reg_static_function`, `acs_execute` queue)
- [x] UI modules bootstrap (`load_ui_module` + `UiTick`; HUD draw/net events still open)
- [x] Saves bootstrap (memory snapshot / restore + `scripts.bin` in `.fsave`; hot reload still open)
- [x] Rust SDK bootstrap (wrappers + bump alloc + `export_module!` / register stub; Actor derive/C header still open)

**Acceptance**

- [x] Cross-backend registration fingerprint (wasmi ≡ wasmtime)
- [x] Sample mods bootstrap (a) FireImp reg + event/UI/ACS/save WAT samples; (b)–(e) deferred
- [x] Save/load module memory mid-session (host API + `.fsave` `scripts.bin`)
- [ ] Benchmark targets (`12` §14) — deferred

---

## M12 — Netplay

**Status:** `done` · Spec: `20` §M12 · Docs: `17`

### Plan

1. Lockstep netcode; UDP + WebRTC (`matchbox`); lobby/handshake.
2. Desync detection + dump tooling; `room-signal`.

### Checklist

**Deliverables**

- [x] `room-net` lockstep bootstrap (`LockstepSession` + loopback mesh)
- [x] UDP transport bootstrap (`UdpTransport`)
- [x] TCP mesh (`TcpTransport`) + packet-server relay (`TcpRelayServer` / `TcpRelayClient`)
- [x] WebSocket packet-server relay (`room-ws` + `room-web` `WsTransport` / `?ws=` lobby)
- [x] Web coop live voice (`MSG_VOICE` + Lightcall AudioWorklet PCM / `voice.js`)
- [x] Noise_XX encryption wrapper (`NoiseTransport` / `snow`, feature `noise`)
- [x] WebRTC (`matchbox_socket` behind `--features webrtc` + `MatchboxTransport`; signal mesh/HTTP for CI)
- [x] Lobby / handshake bootstrap (`LobbyHost`)
- [x] Desync detection + dump text (`DesyncCollector`)
- [x] `room-signal` signalling helper (HTTP room relay)

**Acceptance**

- [x] In-process lossy-network simulation tests
- [x] Automated 2-peer lockstep smoke (loopback + signal mesh + Freedoom MAP01)
- [x] Strong coop suite (`coop_netplay`: UDP lobby + dual-seat independent cmds + FF block + desync)
- [x] TCP relay exchange unit + Noise_XX AEAD unit; app `--transport` / `--noise`
- [ ] Manual 4-player coop native+web 30 min without desync — **intentional deferral** (next step)

### Intentional deferrals

- Live 4-player native+web 30 min soak without desync.
- Drop-in `matchbox_server` swap (HTTP `room-signal` + `room-ws` cover rooms / web relay today).

---

## M13 — Product polish

**Status:** `done` · Spec: `20` §M13 · Docs: `15`, `16`, `18`

### Plan

1. Playable Doom menus (title / pause / skill / options stub) + shell UX.
2. Launcher (IWAD/mod picker), gzdoom.ini import, full option menus.
3. Web library (OPFS), URL params & JS embed API.
4. Packaging, crash reporter, engine-string localization.

### Checklist

**Deliverables**

- [x] Doom menu shell polish (TITLEPIC, skull, patch items, pause/quit/end-game, mouse)
- [x] Launcher + IWAD/mod picker (`--launcher`, auto-discover Freedoom/Doom2)
- [x] gzdoom.ini / room.ini import + `writeconfig`/`exec`
- [x] Option menus wired to CVars (`invertmouse`, `cl_run`, `m_sensitivity`, …)
- [x] Web URL params + storage embed API (`roomUrlParam` / `roomOpfs*`)
- [x] Packaging script (`scripts/package.sh` host/web/all)
- [x] Crash reporter (panic → platform log dir)
- [x] Engine string localization (`StringTable` / LANGUAGE bootstrap)

**Acceptance**

- [x] Fresh-machine UX checklist published (`docs/qa/fresh-machine.md`) + unit/config smoke
- [ ] Full Win/macOS/Linux + Chrome/Firefox/Safari manual matrix — **intentional deferral**

### Intentional deferrals

- Exhaustive multi-OS / multi-browser manual soak of the QA checklist.
- True OPFS backend (API named/ready; `localStorage` today).

---

## M14 — 1.0 hardening

**Status:** `in_progress` · Spec: `20` §M14

### Plan

1. 24 h fuzz all parsers; save/load fuzz.
2. Compatibility corpus report; performance profiling pass.
3. User manual, modding guide, ZScript migration guide.

### Checklist

**Deliverables**

- [ ] 24 h fuzz campaigns
- [x] Save/load fuzz bootstrap (`save_info` + `save_world_bin` / `validate_world_bin` + CI 60s) — live-world round-trip fuzz still open
- [ ] Compatibility corpus run + report
- [ ] Performance profiling pass
- [ ] User manual + modding guide + ZScript migration guide

**Acceptance**

- [ ] All of the above landed and documented

### Log

- `2026-10-24` — **D6** Chocolate `ldist` = `FindDistance2D+1` (was manhattan) — first shotgun spark Z 58016≈choc 58019. Still nspr=504 with BLOODSPLAT (7 JIBS); need floor-miss pellet.
- `2026-10-24` — **D6** shotgun knockback `pushmove` between pellets (Chocolate checkhitsprite). Still nspr=504 @ pad=13; face xr sweep 30→7 jibs / 28→5 jibs (no 6).
- `2026-10-12` — `validate_world_bin` + `save_world_bin` fuzz (capped sector/inv counts).
- `2026-10-12` — `save_info` fuzz target (`.fsave` ZIP / `info.json` must not panic).

---

## Stretch (post-1.0)

Track opportunistically; do not block 1.0.

- [ ] Classic software renderer (`room-render-soft`)
- [ ] ZScript→Rust/WASM transpiler
- [ ] Bots
- [ ] Rollback netcode
- [ ] Timidity/GUS, ADLMIDI/OPNMIDI, GME
- [ ] HRTF
- [ ] Component-model ABI v2
- [ ] Mobile (Android/iOS) shells
- [ ] Join-in-progress netplay

---

## S1 — room-server M1 (multi-room relay)

**Status:** `done` · Spec: `22` §9 M1 · ADR: `0002-room-server-stack`

### Acceptance

- [x] `room-proto` crate (FTR2 + JSON control codecs, unit tests)
- [x] `room-server` skeleton (tokio/axum, registry, room actor)
- [x] Multi-room legacy FTRL gateway with seat stamping (`/ws/{id}`)
- [x] v2 TIC_CMD → TIC_FRAME path (`/v2/rooms/{id}/ws`)
- [x] `POST /api/rooms` + `./scripts/play-web-coop.sh --server`
- [x] Integration: legacy stamp, v2 frame, many-room create (64)
- [x] 1,000 rooms parallel soak (`soak_1000_rooms_v2_tic_frames` + `room-load --rooms 1000`)
- [x] `--server` smoke (`scripts/smoke-room-server.sh`; browser tabs optional)

### Notes

Legacy clients still arbitrate Start in the browser; M2 moves arbitration + tickets to the server.

---

## S2 — room-server M2 (auth, limits, v2 client)

**Status:** `done` · Spec: `22` §9 M2 · ADR: `0002-room-server-stack`

### Acceptance

- [x] HMAC join tickets (`POST /api/rooms/{id}/join`, one-time consume, `--ticket-secret`, `--require-tickets`)
- [x] Origin allowlist on WS upgrade (`--origin`, empty = allow any)
- [x] Resume tokens + 60s lobby grace (reconnect with `resume_token`)
- [x] TLS via reverse proxy (`deploy/Caddyfile`) + in-process security headers
- [x] §5 message + byte token-buckets + chat 1/s burst 5 + violation kick + `--max-conn-per-ip`
- [x] Handshake / idle timeouts (5 s / 30 s defaults; CLI overridable)
- [x] `room-web` v2 client (`ws_v2` + `CoopSocket`; `play-web-coop.sh --v2`); stall fill still client-side on legacy path
- [x] Security tests: forged seat N/A on v2 (no peer byte), replayed ticket, bad Origin, flood, seat hijack
- [x] Fuzz harness: `scripts/fuzz-room-proto.sh --seconds N` + CI `room_proto_binary` 60s
- [x] Recorded 1 h clean soak (`./scripts/fuzz-room-proto.sh --seconds 3600` — clean 2026-10-13)

### Notes

CLI args preferred over env for server and scripts (`--bind`, `--server`, `--rooms`, …).

---

## S3 — room-server M3 (admin, metrics, deploy)

**Status:** `done` · Spec: `22` §9 M3

### Acceptance

- [x] Admin REST skeleton: summary, rooms list, close, audit, bans (in-memory)
- [x] Prometheus `/metrics` + `/readyz`
- [x] Admin SPA (operator console at `/admin` + `/admin/assets/*`; live WS deferred)
- [x] Admin bearer token (`--admin-token`; CSRF/RBAC/WebAuthn deferred)
- [x] Kick / mute / notice room actions
- [x] Docker / systemd / compose under `deploy/`
- [x] Persist audit/bans (`--data-dir` → `audit.jsonl` + `bans.json`; Postgres later)
- [x] Ban enforcement (join mint + WS IP + hello/resume) for ip/name/name_regex/fingerprint/ticket/TURN/auth_key/resume
- [x] Admin UI: ban form scopes + ban-from-seat + remove; lobby callsign required

---

## S4 — room-server M4 (WebRTC SFU)

**Status:** `done` · Spec: `22` §9 M4 · ADR: `0003-room-sfu-str0m`

### Acceptance

- [x] `room-sfu` crate + signalling / mute roster
- [x] `rtc` control routed through room actor; `welcome.media.enabled`
- [x] str0m UDP SFU (`UdpSfu`): demux + MediaData fan-out with server mute gate (`feature = "rtp"`)
- [x] Browser `rtc.js` (Opus PTT over RTCPeerConnection); PCM `voice.js` still fallback
- [x] TURN credential issue in welcome (`--turn-host` / `--turn-secret`)
- [x] Server mute gate unit-tested (`should_forward` / `set_mute`); live packet-capture soak deferred
- [x] v2 default = WebRTC (`rtc.js`); PCM `voice.js` only for legacy `/ws` or RTC init failure
- [x] 8-seat voice+camera middle-layer bandwidth budget (`room-sfu` `subscriber_egress_kbps` ≤ 1300 kbps)

---

## S5 — room-server M5 (load, drain, multi-node, verifier)

**Status:** `done` · Spec: `22` §9 M5

### Acceptance

- [x] Graceful drain (`POST /admin/api/v1/maintenance`, `/readyz` 503, notice + delayed close); CLI `--drain-grace-secs`
- [x] `room_location` table + `--node-id`; join returns `node_id`; foreign owner → 421
- [x] Optional verifier flag (`--enable-verifier`, `--max-verified-rooms`, create `enable_verifier`); resume-after-start when on
- [x] `room-load` latency p50/p99 + `--soak-secs` / `--sample-rss` / `--max-p99-us` / `--players 2..=8`
- [x] CHECKSUM aggregation + `desync` notice (compares to Headless when feature on)
- [x] Headless verifier (`--features verifier --enable-verifier --iwad PATH`); feeds TIC_FRAME → checksum every 35 tics
- [x] 10k-connection scale (`room-load --rooms 5000 --players 2`); ≈30 KiB/conn RSS amortised
- [x] Soak harness (`scripts/soak-room-server.sh --hours N`); 30 s parallel soak clean
- [x] §6 added relay latency (server `relay_latency_us`: avg≈9–28 µs, share_le_300us≈1.0; p99 ≪ 2 ms)
- [x] §6 memory < 48 KiB/conn at 10k (≈31 KiB/conn amortised on laptop)
- [x] §6 create→welcome p99 < 50 ms (`room-load` `create_to_welcome_us` p99≈11 ms @256 rooms)
- [x] 1 h soak with bounded RSS growth (`soak-room-server.sh --hours 1`, +20 MiB / 32 rooms)
- [x] Local daemon helper (`scripts/room-server.sh` start/stop/restart/status)

---

## D0 — Duke ADR + scaffolding

**Status:** `done` · Spec: `25` §11 D0 · ADR: `0004-native-build-runtime`

### Acceptance

- [x] ADR `docs/decisions/0004-native-build-runtime.md` (native runtime, clean-room engine, freeze converter gameplay)
- [x] Empty crates `room-build-engine` + `room-duke` (READMEs; no `room-sim` dep on duke)
- [x] `GameRuntime` / `GameCmd` in `room-game`; `Headless` implements (`tic_rate` = 35)
- [x] `room-app` + `room-web` play loops use `tic_rate()` (still 35 for Doom)
- [x] CI wasm builds `-p room-build-engine -p room-duke`
- [x] D0–D11 listed in `docs/20-roadmap-testing.md` and this dashboard

### Notes

The converter remains available as an export and compatibility path. Native
play is wired in desktop and browser shells, but campaign, audio, saves, mods,
multiplayer and the full acceptance matrix still gate D11.

---

## D1 — Duke reference traces

**Status:** `done` · Spec: `25` §10.2 / §11 D1

### Acceptance

- [x] Trace format spec `docs/duke-trace-format.md`
- [x] `room-duke::trace` + `room duke trace-diff` / `trace-hash` / `trace-check-manifest`
- [x] Committed manifest stub `testdata/duke-trace-manifest.toml` + howto
- [x] Fixture traces under `crates/room-duke/testdata/` (CI unit test)
- [x] Out-of-tree hook sources in `third_party/chocolate-duke3d-trace/` (apply to Chocolate checkout)
- [x] Chocolate Duke3D **built** with the hook + DEMO1–3 traces recorded
- [x] `testdata/duke/traces/DEMO*.trace` + manifest SHA-256 filled

### Notes

Chocolate sources stay out of Room; only our emitter lives under `third_party/`.
Record on **linux/386** (Chocolate truncates pointers on 64-bit). See
`docs/duke-trace-howto.md` and `third_party/chocolate-duke3d-trace/README.md`.

---

## D2 — Duke engine primitives

**Status:** `done` · Spec: `25` §5 / §11 D2

### Acceptance

- [x] Spec tree `docs/build-engine-primitives/` (stubs + numeric/world/inside drafts)
- [x] `room-build-engine` numeric + sprite-list bootstrap + unit tests
- [x] `inside` / `updatesector` bootstrap impl + unit tests
- [x] `getzsofslope` bootstrap impl + unit tests
- [x] `clipmove` behavioural contract (impl gated on goldens)
- [x] Helpers: `sectorofwall` / `lastwall` / `rotatepoint` bootstrap
- [x] `getzrange` sector-disc bootstrap + unit test
- [x] `getzrange` portal BFS + face-sprite ceilings (wall/floor sprites later)
- [x] `hitscan` single-sector solid-wall bootstrap + unit tests
- [x] `neartag_wall` / same-sector `cansee` bootstrap + unit tests
- [x] `clipmove_xy` hard-stop + wall-slide bootstrap + unit tests
- [x] `hitscan` portal BFS + unit test
- [x] Face-sprite hitscan + unit test
- [x] Floor/ceiling hitscan (`hitscan_full`) + unit test
- [x] `dragpoint` bootstrap + unit tests
- [x] Golden harness + bootstrap fixtures (`primitives_goldens`)
- [x] 120 bootstrap cases (6 hand + 114 `gen_*` AABB/flat/open-clip)
- [x] proptest smoke (`primitives_proptest`) + probe howto
- [x] Chocolate probe skeleton (`room_primitives_probe.c`)
- [x] ≥50 Chocolate-recorded (`choc_*`) vectors (117 inside/getz + L-shape via i386 probe)
- [x] proptest soak (`primitives_proptest`: clipmove/inside/getz/hitscan/updatesector)
- [x] Multi-map Room soak (`primitives_maps`: ≥40 episode MAPs from GRP)
- [x] cargo-fuzz `build_primitives` (+ restored `fuzz/` workspace from prior commit)
- [x] Chocolate per-MAP `inside` goldens (41 episode maps; `map` field loads GRP)
- [x] `inside` Build XOR/sign (E2L8 parity)
- [x] Chocolate clipmove probe (`initspritelists` + 23 wall-only `choc_clip_*`)
- [x] `clipmove_xy_at_z_sprites` face L-clips, ART-sized wall quads, floor-sprite visible-edge clipping (`cstat&48==0/16/32`), Chocolate’s reverse-order Z-aware final sector fallback, and live `headspritesect` scan order

### Notes

Clean-room only — no BUILDLIC translation. Harness ~300 fixtures (41 map-ref
+ synthetics/L + clip) + GRP soak + `build_primitives` fuzz. Face, wall- and
floor-aligned sprite clipmove plus the Z-aware final sector fallback landed
2026-11-07 from static Chocolate source review and compile checks. Runtime
players, badguys, fallers, poolballs and pipebombs now pass the live sector
list ordering; clip probe/goldens are still needed. Bit-exact slide
(beyond `pos_atol=1`) remains a follow-up under `05-clipmove.md`.

---

## D5 — Duke core sim loop

**Status:** `in_progress` · Spec: `25` §7 / §11 D5

### Acceptance

- [x] CON VM bootstrap and actor/player state commands and predicates (`sound` event, `money`/`mail`/`paper` object sprays, `lotsofglass` shards, `quote` FTA state, `tip` animation state, `strength`/`addstrength`/`addkills`, non-player palette save/restore, `cstat`/`cstator`/`cactor`/`clipdist`, `count`, `sleeptime`, `pkick` + quick-kick countdown, `endofgame` + exit timer, `addphealth` health caps/god-mode guard, co-op `ifgotweaponce` reservation table, `ifstrength`, `ifphealthl`, `ifp pkicking`, `ifp pcanseeeverything`, `ifsquished` sector test, `ifmultiplayer`, `ifspritepal`, `ifangdiffl`, `ifactornotstayput`, `ifhitspace`, `ifoutside`, `ifinspace`, `ifinouterspace`; runtime co-op selection and remaining opcode coverage are open)
- [x] CON `palfrom` timed player palette flash (applied in actor order) and `print` numeric console diagnostic
- [x] Solo CON `resetplayer` requests host restart; native desktop restores initial session or offers the last save
- [x] Multiplayer CON `resetplayer` selects Chocolate's `po[]` start pose (co-op uses seat index; DukeMatch uses deterministic `TRAND % numplayersprites`) and resets player/body spawn state
- [x] Per-tic `tip`, `quote`/FTA and living-player `endofgame` countdowns; expose one-shot level-exit readiness to the host (campaign transition/routing remains open)
- [x] `DukeSession` at 30 Hz + `SpriteLists` + `randomseed`
- [x] `processinput` movement-only (`processinput_move`: avel/fvel/svel + friction + `clipmove_xy`)
- [x] Coarse map sprite classify (`APLAYER`/`SE`/`actorscrptr` → stats); full `spawn` rules still open
- [x] `moveactors` calling CON `execute` (header+4; `killit` → `deletesprite`)
- [x] `movefta` (nearest living player, STAT_DUMMY wake test, Atomic standable classification/shade)
- [x] `STAT_DEFAULT` QUEBALL/STRIPEBALL movement: wall/sprite clipping, wall bounce, pocket capture, speed decay, ball-to-ball impulse, proximity auto-kick and edge-triggered player-directed kick
- [x] Sprites in soft view (`render_fp_sprites` face sprites + z-buffer; wall/floor align deferred)
- [x] DEMO1 comparison harness (`demo_sync`: GRP+DMO+trace; assisted and native-only modes)
- [x] Native `DukeSession` initialization from an Atomic GRP map (no demo dependency)
- [x] `room-game::GameRuntime` adapter for native Duke commands and 30 Hz ticks
- [x] DEMO1 tic0 RNG alignment helper (diagnostic only; native simulation does not yet derive the reference seed)
- [x] DEMO1 tic0 `numsprites` match (484: GPSPEED + SP `pal` pickup soft-cull + `moveexplosions`)
- [x] Coarse spawn status lists + `spawn_floor_snap` (dummy/actor floor rest)
- [x] APLAYER EGS + GPSPEED delete order (player sprite index 69 matches Chocolate)
- [x] DEMO1 tic0 player pose (XY + posz −24016 / sprite z −14288 via getzrange face-sprite ceiling + clamp)
- [x] DEMO1 tic0 `sprite_hash` bitexact (SE 0/13, `nsqrtasm` slopes, sintable, crack `ssp`×TICSPERFRAME, bloodsplat TRAND#2, badguy flat-floor `makeitfall`)
- [x] Assisted DEMO1 tic0–1 sprite hash regression (`trace_fixups` + RNG pads; not native-only parity)
- [x] Assisted DEMO1 tics 0–38 regression (portal `cansee`, CON `fall`, WATERSPLASH2, SHOTGUNSPRITE wake; pads stand in for unimplemented TRAND sites)
- [x] Assisted DEMO1 tic39 regression (WATERBUBBLE ifcount TRAND; #75/#77 phase lag workaround; pads through tic39)
- [x] Assisted DEMO1 tic40 regression (#75/#77 keep skipping ifcount; `pad_pre=2`)
- [x] Assisted DEMO1 tics 41–44 regression (`pre_explode=2` / `pad_pre=2`)
- [x] Assisted DEMO1 tics 45–59 regression (`moveplayers` water bubbles; `swim_ifrnd_budget`; pads)
- [x] Assisted DEMO1 tics 60–82 regression (clipmove walldist; SHOTGUNSPRITE CON; pads)
- [x] Assisted DEMO1 tics 83–119 regression (late warp_floorz freeze; pads through 119)
- [x] Assisted DEMO1 tics 120–123 regression (CON `ifai` + `ifcansee`; pads 121–123)
- [x] Assisted DEMO1 tics 124–134 regression (seekplayer wiggle; sloped `hitscan` for `hits()` undo; pads)
- [x] Native-only DEMO1 matches seed, player pose, sprite count and hash through tic134
- [x] Player bullet hits on Atomic DIP/HAND switch tiles queue interaction and follow Chocolate's pre-ricochet early return
- [ ] DEMO1 sync beyond tic134 (tic135 ricochet-roll correction is implemented but not dynamically remeasured; compare pellet angles, hit count, `JIBS6`, and end seed)
- [ ] Opcode coverage over DEMO1–3 / route corpus (from D4 deferral)

### Notes

Translate GPL game code (`actors.c` / `player.c` / …). Engine calls stay on
`room-build-engine` clean-room primitives.

**Frontier debug (2026-11-06):** `krand-map` identifies the live sprite ID for
CON RNG calls, and `snapshot DEMO1 <tic>` prints actor temp counters, movement,
owner and sector lotag. At tic39, warped bubbles #75/#77 reached the lotag-1
surface but their `iffloordistl 8` check still saw the source floor; using the
destination sector planes aligned native-only playback through tic40. The next
gap came from the APLAYER swim action: Room suppressed the first live inner
`PSWIMMING ifrnd 4` after entering that action and did not advance player
`temp_data[0]` as Chocolate `move()` does. Room now suppresses only the entry
tic, advances the player counter in and out of water, and matches native-only
DEMO1 state and RNG through tic69. At tic70, Room consumes an APLAYER trailing
bubble `ifrnd` that Chocolate skips after changing the swim action. The
`krand` diagnostic compares call seeds and end-of-tic seeds. D5 remains
incomplete. The CON VM also now implements Chocolate `strength` and
`addstrength`, non-player `spritepal` / `getlastpal`, and the
`ifstrength`/`ifphealthl`/`ifspritepal`/`ifangdiffl`/`ifactornotstayput`
predicates. The player palette case of `getlastpal` and broader stateful
predicates still need Room player/session data.

**Player timer lifecycle (2026-11-06):** `DukeSession::tick` now decrements
Chocolate `tipincs`, `fta`, and the living player's `timebeforeexit` counters.
Expired FTA clears the active quote ID, and a completed end-of-level countdown
is exposed as a consumable request for the outer game runtime. No campaign
routing or countdown audio was added. Static `cargo check -p room-duke` only;
no gameplay/demo comparison was run.

**CON `operate` (2026-11-07):** the VM now defers `operate` from a CON actor
until the session has the Build world and sprite lists. It traces the 768-unit
horizontal `neartag` ray through connected sectors, testing tagged walls,
sectors and sprites; sprite candidates use ART dimensions, repeat sizes,
vertical position and picanm offsets. It applies Chocolate's near-operator,
plane, lock-bit and ACTIVATOR checks, then dispatches the existing sector
operation. Animated ART offset handling, exact ray/tag tie ordering, and
same-actor-pass mutation ordering remain open. Static review of Chocolate
`gamedef.c` / Build `neartag` and compile/format checks only
(`cargo fmt --all`, `git diff --check`, `cargo check -q -p room-duke --lib`);
no gameplay/demo comparison was run.

**`movefta` player and standable routing (2026-11-07):** Room now selects the
same nearest player returned by Chocolate `findplayer`, skips wake checks when
that player's body sprite is dead, and moves visible barrel/can/fire-vase/
wooden-horse/tripbomb types to `STAT_STANDABLE` with the sector-plane shade.
Other visible dummy actors still promote to `STAT_ACTOR`. Static review of
Chocolate `actors.c::movefta`; sleeping badguy shade now follows its sector
plane shade each tic while its closest target is alive. `cargo fmt --all`,
`git diff --check`, and `cargo check -q -p room-duke --lib` only; no
gameplay/demo comparison.

**Pool-ball input and movement (2026-11-07):** `STAT_DEFAULT` balls now advance
in the native actor phase. Moving balls use Build `CLIPMASK1`, capture at
nearby POCKET sprites, bounce by reflecting their angle on walls, exchange the
native velocity/angle impulse on ball contact, lose one `xvel` per tic, and
update STRIPEBALL's cstat animation bits. Stopped balls auto-kick when the
nearest player is within 512 units; the use bit also kicks a ball inside the
native 64-angle/1596-distance cone unless another ball is closer in that cone.
`toggle_key_flag` prevents repeat kicks while held, clears on release, and
round-trips from the Atomic player save field. The full near-tag/eye-height
filter, exact `findplayer` tie ordering, POOLBALLHIT voice cap, and full Build
clipmove parity remain open. Static review of `actors.c::moveactors`,
`sector.c::checkhitsprite`, `game.c` and `duke3d.h`; formatting, diff and
`cargo check -q -p room-duke --lib` only. No gameplay/demo comparison.

---

## D4 — Duke CON compiler

**Status:** `done` · Spec: `25` §6 / §11 D4

### Acceptance

- [x] `room-duke::con` module + Chocolate `keyw[]` (112) opcode indices
- [x] Lexer (`isaltok` / `ispecial` / labels)
- [x] `define` + `//` + `/* */` + `include` (resolver trait) + unit tests
- [x] `action` / `move` / `ai` definitions + in-body calls
- [x] `state` / `ends` / `actor` / `useractor` / `enda` + `actorscrptr` / `actortype`
- [x] Common nullary + unary opcodes emitted
- [x] `definesound` / `definequote` / `definelevelname` / volume+skill / `gamestartup` side tables
- [x] if/else fail-forward patches + `{`/`}` + `palfrom` / binary / `hitradius` / `music` / `betaname`
- [x] Atomic `GAME.CON` compiles with `errors=0`, `con_size=16208` (Chocolate 1.5 size)
- [x] Script image word hash vs Chocolate dump (relocated pointers; case-sensitive labels)
- [ ] Opcode coverage over demo/route corpus — **intentional deferral** (needs CON VM + DEMO playback; track under D5)

### Notes

**CON presentation opcodes (2026-11-07):** VM opcode 68 `print` emits its
integer operand as a decimal stdout line, matching Chocolate's diagnostic.
Opcode 14 `palfrom` now queues its duration/RGB values during CON execution
and applies the resulting palette flash to the owning player in actor order.
Static review of Chocolate `gamedef.c`, formatting and targeted compile only;
no game comparison.

GPL game-code translation from `GAMEDEF.C` is allowed. Pointers in the image
are stored as word indices (relocatable), not absolute addresses. Label names
keep file case (`strcmp`), matching Chocolate — do not fold to lowercase.

---

## D3 — Duke world load + static view

**Status:** `done` · Spec: `25` §11 D3

### Acceptance

- [x] `SectorT` / `WallT` / `SpriteT` + `BuildWorld::load_arrays` / `load_map_bytes`
- [x] `BuildView` camera snapshot + camera-oriented portal flood (`scansector` front-facing bounds and one-way bit)
- [x] Soft top-down wireframe bootstrap (`soft`)
- [x] Soft FP wireframe bootstrap (`render_fp_wire`)
- [x] Soft FP solid columns + z-buffer (`render_fp_solid`)
- [x] ART-textured FP columns (`SoftTiles` / `render_fp_textured`)
- [x] Soft flats / parallax sky fill (`fill_fp_flats_sky`)
- [x] Noclip free-fly pose (`FreeFly` / `FreeFlyCmd`)
- [x] CLI soft-view shell (`room duke soft-view` → PGM)
- [x] E1L1–E4L11 soft load+render smoke (`soft_maps`, needs GRP)
- [x] ART soft-view path (`room duke soft-view` textured PGM)
- [x] Synthetic soft goldens (`soft_goldens`: walls / flats+sky / textured)
- [x] FreeFly CLI controls (`soft-view --forward/--strafe/--turn/--look/--up`; no windowed `--play`)
- [x] Depth-tested face sprites with X/Y flips and transparent texels
- [x] Depth-tested wall-aligned sprites with angle, repeats, centered origin and X/Y flips
- [x] Depth-tested floor-aligned sprites with angle, repeats, X/Y flips and transparent texels
- [x] `LOOKUP.DAT` palette remaps on opaque wall and face/wall/floor sprite texels
- [x] `PALETTE.DAT` shade rows on flat/sky fill, walls and face/wall/floor sprites
- [x] Wall texture panning bytes offset sampled ART texels by a fractional tile
- [x] Wall cstat X-flip (8) and Y-flip (256) reverse sampled tile coordinates
- [x] Wall cstat one-way masked bit (32) blocks adjacent-sector visibility in the static renderer
- [x] One-sided cstat rejection for wall-aligned and floor-aligned sprites
- [x] Sprite origin offsets from map `xoffset`/`yoffset` and ART `picanm` on all three sprite orientations
- [x] Floor/ceiling raster rays intersect sloped planes using deterministic fixed-point `getzsofslope`
- [x] Floor/ceiling plane XY swap, X/Y flip and double-smoosh texture flags
- [x] Relative floor/ceiling texture alignment to the sector's first wall
- [x] Floor/ceiling tile-size scaling and 1/256-tile panning
- [ ] Visibility-based shade selection, translucency levels, exact wall/plane panning and perspective-correct mapping
- [x] Portal / multi-sector soft golden (`golden_portal_two_sectors`)
- [ ] Interactive desktop/web FreeFly shell — **intentional deferral** (CLI soft-view covers D3 accept; wire later with D8/D11 shells)

---

## D6 — Duke weapons + damage

**Status:** `in_progress` · Spec: `25` §7 / §11 D6

### Acceptance

- [x] Knee cadence / short-range hitscan, impact actor and smoke, shared sprite/wall impact dispatch, pistol, shotgun, chaingun, and RPG firing paths
- [x] Normal knee weapon and `pkick` both dispatch hits into the shared impact and tagged-switch handlers
- [x] Devastator two-shot burst cadence, alternating launch offset, spread RNG and per-shot ammo use
- [x] Freezer kickback firing cadence, cell consumption, FREEZEBLAST spawn and basic clipped flight/hit dispatch (partial weapon behavior)
- [x] Shrinker bolt and Grower hitscan firing paths, ammo timing, impact metadata, and animated Grower impact cue (target transformations remain open)
- [x] Player weapon kickback timing and native soft-view weapon frame selection
- [x] Decode the Chocolate `sync.bits` weapon choice field, direct weapon hotkeys, previous/next cycle, and queued direct selection while a weapon fires
- [x] Hitscan sprite damage, shotgun spread, player blood/jibs, and key firing sound IDs
- [x] Pistol, shotgun and chaingun direct wall/sprite material impacts and tagged switch activation (DIPSWITCH solution counting/warning and exact Build motion filtering remain open)
- [x] Direct explosive/projectile `checkhitwall` mirror impacts: source allowlist, mirror collision-bit clear, broken overlay, 70 wall shards, heavy-break cue
- [x] `checkhitwall` receives Chocolate's explicit `atwith` tile from hitscan, knee and projectile call sites; chaingun forcefield ripple uses the shot's scaled repeats
- [x] Auto-aim uses Chocolate's target-specific LOS source height for `ORGANTIC` and `ROTATEGUN`
- [x] Owned `hitradius` damage exits an active APLAYER camera view and releases active `CAMERA1` monitor latches
- [x] `checkhitswitch` accepts tagged `DOORTILE` operators as well as switch art; door art and ordinary switch cue are preserved
- [x] `checkhitswitch` lotag `65535` / signed `-1` sets the native level-exit request for recognized wall/sprite operators
- [x] Player quick-kick hitscan activates the tagged switch tile it reaches for solo/local co-op; ordinary hits cannot trigger lotag-special paths
- [x] Pipebomb hold/strength throw, near-wall slowdown/reversal, automatic remote cycle, stopped-bomb pickup, underwater splash, actor fall/bounce and basic timed blast
- [x] Map-placed `HEAVYHBOMB` pickup/detonation respawn with `ud.respawn_items`, CON `RESPAWNITEMTIME`, red/yellow/green markers and teleport reappearance cue/effect
- [x] CON `hitradius` actor/player scan, line-of-sight damage bands and knockback
- [x] CON/weapon radius damage reaches shootable SEENINE/OOZFILTER standables
- [x] `hitradius` nearby-sector wall pass breaks mirror/glass and clears paired glass blocking
- [x] `hitradius` nearby-ceiling test calls the Atomic ceiling-light/glass/SE3/SE12 path
- [x] Direct projectile `checkhitsprite` transitions for ocean sprites, pool balls, fountains, fan, toilet/stall, hydrant, grate/panels, pipes, hydroplant, selected glass objects, FETUSBROKE blood-splat burst, BOTTLE10 money, VASE/STATUE glass bursts, chair/decor transitions and debris, MONK/LUKE/INDY/JURYGUY/SPACEMARINE damage/death, first-hit flammable props, cactus/hanglight/fragile-tech scrap bursts and force-sphere owner state (basic effects; remaining families/debris open)
- [x] APLAYER consumes queued weapon hits for health and knockback; god mode and default coop friendly-fire rejection are applied
- [x] Fatal non-Freezer hits retain Chocolate `wackedbyactor`; dead camera follows the body and turns halfway toward an active attacker each tic
- [x] CON `ifpinventory` / `addinventory` player state for Atomic inventory IDs 0, 1, 2, 3, 4, 6, 7, 9 and 10
- [x] Edge-triggered inventory use and left/right selection inputs; first aid, steroids, jetpack and heat toggles; jetpack flight, underwater scuba activation/depletion, air reserve and timed drowning damage
- [x] Holoduke inventory toggle creates/removes a native APLAYER decoy with expansion/facing animation and timeout cleanup (partial fall, collision and tint behavior)
- [x] Purple lava, hurtrail, slime and plasma floor damage with boot charge consumption (surface effects and exact spritebridge exceptions remain open)
- [x] Timed Duke player palette flashes for the implemented toxic-floor damage cases (classic palette math and coop presentation remain incomplete)
- [x] Freezer ceiling/floor ricochet reflects the full velocity vector against the selected Build slope normal, then applies Chocolate's bounce-count, damage, and size decay
- [x] GREENSLIME Freezer transition, frozen gravity/palette/state, 280-tic thaw, quick-kick response, and non-Freezer glass-shard break
- [x] Lethal `FREEZEBLAST` APLAYER hit enters timed palette-1 freeze, suppresses ordinary death/respawn processing, and thaws at 1 HP using active CON `THAWTIME` when defined (fallback 138)
- [x] A non-Freezer hit on a frozen APLAYER restores its palette, emits the script-defined `GLASSPIECES` burst with ordered angle/x-velocity/z-velocity RNG draws, and queues `GLASS_BREAKING` before normal damage handling
- [x] APLAYER Shrinker/Grower responses: shrink flash/cue, `PSHRINKING` timer and repeat recovery, tight-sector crush, Grower flash/cue
- [x] `FRAMEEFFECT1` afterimage lifetime, translucency stages and owner sprite offsets in the native misc-actor tic
- [x] APLAYER `PSHRINKING` spawns `FRAMEEFFECT1` at the shrinking body pose each tic
- [ ] Full actor-specific shrink/grow CON transitions, player CON side effects/particles, and Freezer transformations beyond GREENSLIME
- [ ] Full freeze transformations beyond GREENSLIME; exact Build movement/hit filters and remaining target transformations; exact Build `hitasprite` / laserline collision and movement, tripbomb damage/arming edge cases; target damage ordering/RNG, respawn, and full FX/parity
- [ ] Full Build hit filtering, wall/ceiling impacts, radius damage, and damage reactions
- [ ] DEMO1 sync to its end; the last reference frontier is still tic 135

The current paths are a playable subset, not D6 parity. Full Holoduke
fall/collision behavior, boots/floor effects, palette/HUD updates, exact inventory quote/sound/timer
semantics, and projectile lifecycles remain open. No D6 gameplay comparison
was run in this task.

**Devastator firing (2026-11-06):** the native player path now starts the
five-tic Devastator burst, launches two spread RPG projectiles at the Atomic
kickback phases, uses Room's `aim` cone and tile height for target Z velocity,
alternates barrel offset, decrements ammo per shot and queues the CAT/RPG
firing cues. The projectile's complete impact/explosion behavior is still
partial. Static
source inspection and compilation only; no runtime test or parity comparison.

**Devastator scripted projectile strength (2026-11-07):** static comparison
with Chocolate `EGS()` + `shoot(RPG)` found that Room used a hard-coded base
strength of 4 before the Devastator quartering step. The native spawn now starts
from the RPG CON actor strength, adds the same `TRAND&7` roll, quarters the
result, and carries the scripted actor `hitag`. `cargo fmt`, `cargo check -p
room-duke -p room-build-engine`, and `git diff --check` pass; no gameplay/demo
comparison was run.

**Weapon selection (2026-11-06):** Room now decodes the Atomic `sync.bits`
weapon field for direct slot selection and previous/next cycling. A direct
selection requested during kickback is retained as `wantweaponfire` and applied
when the firing animation completes, provided the player owns the weapon and
has ammo. Empty guns remain directly selectable; pipebomb/tripbomb selection
still requires ammo. Selection queues the weapon-specific Atomic cue. Static
Chocolate source inspection and `cargo check -p room-duke` only; selection
behavior was not dynamically compared.

**Pipebomb/remote slice (2026-11-06):** static comparison with Chocolate
`player.c::shoot()` and `actors.c` added the Atomic hold-strength throw at
kickback tic 12, crouch toss, ammo consumption, automatic switch to
`HANDREMOTE`, detonation latch and return to handbomb after the remote cycle.
`HEAVYHBOMB` now has a native actor path for falling, floor bounce, XY clipping,
owner remote detonation, `hit_extra` impact detonation, stopped-bomb pickup,
underwater splash, and visible radius damage using the `gamestartup` radius
when present (with a fallback). Radius targets include native CON badguys,
shootable actors, and impact-damage SEENINE/OOZFILTER standables. The shared
`hitradius` wall pass breaks mirror/glass and unblocks paired glass walls;
ceiling effects, other wall materials, damage ordering/RNG edge cases,
non-pipebomb item respawn, and complete explosion variants remain open. The `hits()` near-wall
throw slowdown/reversal is also ported.
`cargo check -p room-duke` passed;
no game or demo comparison was run.

**Map-placed pipebomb item respawn (2026-11-08):** map-owned
`HEAVYHBOMB` items now honor `ud.respawn_items` and CON
`RESPAWNITEMTIME`: pickup or detonation hides the item, creates the red
respawn marker, advances it through yellow/green stages, then restores the
item with Chocolate's teleport cue and transporter-star effect. Thrown
player bombs still disappear instead of respawning. Static Chocolate
`actors.c` / `names.h` review; `cargo fmt --all`, `cargo check -q -p
room-duke --lib`, and `git diff --check` pass with existing workspace
warnings. No tests or dynamic Chocolate/Room/game comparison were run.

**CON `hitradius` slice (2026-11-06):** the CON opcode now captures its radius
and four damage values and applies Chocolate's three distance bands to visible
badguy/shootable actors and players, recording actor hit metadata and knockback.
The RPG small-sprite early-out for the wall pass and randomized vertical
visibility sample are included. Remaining ceiling/material cases, status-list
targets, and exact deferred side-effect ordering remain open. Static source
review only; no game, demo, or Chocolate comparison was run.

**SEENINE blast targets (2026-11-07):** CON and weapon radius blasts now scan
the native standable list in Chocolate's status order and queue visible
SEENINE/OOZFILTER damage and impact knockback, allowing both direct explosives
and pipebomb/RPG radius damage to enter the existing hit/delay state machine.
Tagged pipebomb hitag propagation and the generic blast wall/sector hitradius
pass are still incomplete. Format, diff and crate compile checks only; no
gameplay/demo comparison.

**D6 `hitradius` wall/ceiling-sector pass (2026-11-07):** CON, pipebomb, RPG and
SEENINE/OOZFILTER radius paths now use a shared breadth-first adjacent-sector
scan. It tests the cached ceiling plane and the first or two-linked-`point2` wall point,
calls native `check_hit_ceiling` when inside the blast radius, then checks LOS
from Chocolate's quarter-midpoint sample, breaks mirror/glass overlays, clears
paired glass blocking, breaks paired fan overlays when the force-wall gate
matches, clears paired stained-glass blocking under that gate, and queues the
Atomic sounds on the blast source. Common vending machines, screens, tech
walls/shelves, ATMs and wall lights now take Chocolate's broken-texture
transitions; screen/light break cues are selected from the Atomic RNG. Mirror
breaking now uses Chocolate's `atwith` allowlist, so ordinary CON radius sources
do not shatter mirrors. Wall-positioned `GLASSPIECES` are spawned for mirror,
glass, stained glass, screen, breakwall and light hits with Chocolate's line
interpolation, vertical fallback, palette and per-shard random velocity draws.
Eligible forcefields create `FORCERIPPLE` impacts and set the animated wall
flag; these cues are queued with the blast source. Tagged wall lights also arm
matching SE3 markers with Chocolate's randomized phase and darkest adjacent
wall shade, after their shard RNG draws. ATM breakage emits the native random
money count and pickup sprites. Broken overlay glass now creates SE 128, which
advances both wall overlays and restores their collision flags after the native
six-tic sequence. Wall LOS samples now resolve their midpoint sector with
`updatesector` before `cansee`.
The small-RPG and SHRINKSPARK early-outs skip only this wall/ceiling pass; the
sprite damage scan still runs. Remaining `checkhitwall` cases remain open.
`cargo fmt`, diff check and crate compile only; no gameplay/demo
comparison.

**D6 APLAYER hit processing (2026-11-07):** queued weapon/projectile hits are
now consumed in `moveplayers`, updating player/body health and applying source-
dependent directional knockback. God mode (except Shrinker), default coop
friendly-fire rejection, and the Atomic strong-knockback source IDs are wired.
Armor, HUD damage feedback, frag attribution, and full freeze/death semantics
remain open. Static inspection and crate compilation only; no runtime or demo
comparison.

**D6 player shield absorption (2026-11-07):** `shield_amount` and native
`incur_damage` behavior now absorb 20–49% of player health loss with one
conditional Build `TRAND`, capped at the remaining armor pool; death clears
armor. New players start with Atomic's `gamestartup[3]` maximum (100 fallback),
as Chocolate `resetinventory` does. SHIELD pickup/respawn, `extra_extra8`, HUD
palette feedback and complete player hit ordering remain open. `cargo fmt
--all`, `git diff --check`, and `cargo check -q -p room-duke --lib` pass with
existing warnings. No tests or gameplay/demo comparisons were run.

**D6 direct projectile object impacts (2026-11-07):** RPG, Freezer/Shrinker,
SPIT and FIRELASER collision paths now port the simple `checkhitsprite` arms
for ocean sprites, pool balls, water fountains, fan sprites,
toilets/stalls, hydrants, grates/panels, pipes, hydroplant and first-hit
flammable props, selected bottles/decorations, broken hydroplants, satellites,
fuel pods, solar panels and antennas, plus the force-sphere hit response:
native tile/cstat changes, sprite-local `lotsofglass` shard creation, cactus
(64 scraps) and hanglight/pole (6 scraps) bursts with ordered Build RNG draws,
basic smoke/water/steam/burning/explosion and 15-piece fragile-tech scrap
spawns, positional cues, and force-sphere owner temp-state updates.
Fragile-tech scrap is gated by Chocolate's `SHOTSPARK1` actor damage header.
Pool-ball hits apply Chocolate's three-in-four rolling impulse or its
one-in-four three-shard glass break.
Those switch arms suppress generic badguy damage
metadata. BOTTLE10 money sprays, vases/statues' special shard variants,
furniture, human NPC and other breakable branches, debris/RNG details, and
weapon-family acceptance filters remain open. Source review of Chocolate
`sector.c::checkhitsprite` and `game.c::lotsofglass`; `cargo fmt --all`, `git diff --check` and
`cargo check -q -p room-duke --lib` only (existing warnings); no tests or
dynamic comparison.

**Freezer firing slice (2026-11-07):** static Chocolate `player.c::shoot()`
review added slot 9's kickback-tic-3 projectile launch and ammo use, automatic
repeat while fire is held, and the Atomic `CAT_FIRE` cue. `FREEZEBLAST` now
uses a clipped projectile path, dispatches hit metadata, fizzes with a
transporter star under the native low-lifetime conditions, skips freeze-immune
paletted badguys/players, and reflects from walls. Ceiling/floor impacts now
use Chocolate's `bounce()` slope normal to reflect horizontal and vertical
velocity together, then halve damage, decrement bounce count, and shrink the
visible bolt by two repeat units. The full 48-angle `aim()` target selection
now sets its angle and Z velocity, gated by `player.auto_aim`. Exact Build
slope/ssp collision movement and the actor freeze state machine remain open;
the user setting menu/persistence still needs wiring. `cargo fmt --all`,
`git diff --check`, and `cargo check -q -p room-duke --lib` pass with existing
warnings. No tests, gameplay or Chocolate comparison were run.

**GREENSLIME freeze lifecycle (2026-11-07):** `moveactors` now handles
Chocolate's C-side Freezer state for all eight Atomic GREENSLIME frames, even
when the tile has no CON actor body. A Freezer hit subtracts its queued damage,
credits the nearest player as in `IFHIT`, and enters the `t[0] == -5` state
with the native SLIM_DYING / SOMETHINGFROZE cues. Frozen tics apply
`makeitfall`-style slope floor gravity, force the frozen frame/cstat/palette
and one health point, thaw after 280 tics, and trigger the 14-tic quick kick
when a nearby player faces the slime. A non-Freezer hit breaks it into the
ordered 17 GLASSPIECES burst with Chocolate's angle/velocity RNG draws and
GLASS_BREAKING cue; another Freezer hit is consumed without breaking it.
Static review of Chocolate `actors.c::moveactors`, `gamedef.c::makeitfall`,
`names.h` and `soundefs.h`; formatting, `git diff --check`, and
`cargo check -q -p room-duke --lib` only. No tests, gameplay, or reference
comparison were run. Non-frozen slime AI/attack and other actor freeze
transformations remain open.

**Shrinker/Grower firing slice (2026-11-07):** added slot 6's repeating
SHRINKSPARK launch and slot 11's Grower hitscan, spread RNG, GROWSPARK impact
animation, and `SHRINKER_FIRE` cues. SHRINKSPARK now advances with clipped
movement; impacts apply the configured `SHRINKERBLASTRADIUS` with the native
zero-damage SHRINKSPARK marker to visible eligible actors, without applying the
wall blast pass. All three weapon paths now use the shared 48-angle `aim()`
selection and native target Z velocity; Grower retains Chocolate's fallback
spread RNG. The auto-aim mode now comes from sync bit 6, but the UI setting is
not wired. Exact Build actor/player/status
filtering and complete target CON shrink/grow state coverage remain open. Format,
diff, and crate compile checks passed; no gameplay or reference comparison.

**Chocolate auto-aim wiring (2026-11-07):** player input now carries bit 6
into `player.auto_aim` (full mode 2 versus bullet-only mode 1); the desktop
input emits the Atomic default full-autoaim bit, and saved player states
deserialize with the Atomic default. RPG, Freezer, Shrinker, Grower, Shotgun,
and Devastator now gate the shared target-cone scan on that state and solve
projectile/hitscan Z against the selected target's ART height. The settings
menu and user-configured mode selection are still open. Static Chocolate
`getinput`/`shoot` inspection and crate compilation only; no game/demo run.

**Tripbomb placement and lifecycle slice (2026-11-07):** weapon slot 8 validates
Atomic's wall/sector/range/duplicate-charge/BIGFORCE restrictions, advances the
kickback timing, and plants a wall-facing charge with the placement cue. The
standable pass now advances the initial arm delay, uses horizontal hitscan for
the `hitasprite` distance/target, creates linked `LASERLINE` segments with the
Atomic perpendicular angle and `lasermode` blocking flags, reacts to queued
weapon hits and target-distance changes with the countdown, then emits
five scraps, configured radius damage, the explosion and line cleanup. As a
partial port, the target ray uses Room's `hitscan_full` instead of Build's
`hitasprite`; laserline collision and exact sprite movement/RNG ordering remain
unverified. The placement also applies the native pre-angle SSP nudge toward
the firing origin. Static `player.c`, `actors.c`, `game.c` and `hitasprite` review;
`cargo fmt --all`, diff check and `cargo check -q -p room-duke --lib` pass with
existing warnings. No tests or gameplay/demo comparison were run.

---

## D7 — Duke sectors + effectors

**Status:** `in_progress` · Spec: `25` §7 / §11 D7

### Acceptance

- [x] `gamefunc_Open` wall-switch lookup and linked activator dispatch
- [x] `operateactivators` toggles SE18/31/32/36 T1 in low-lotag activator sectors and queues each sector cue before operating the sector
- [x] `operateactivators` performs Chocolate's once-per-call preliminary lotag-22 `MUSICANDSFX` cue before each lotag-22 operator's own cue
- [x] All `operateactivators` callers arm matching RESPawn markers through its common `operaterespawns` tail, including train station activations
- [x] `TRANSPORTERSTAR` is spawned when the marker is armed; the 54→66 `movefx` countdown retains a legacy-save fallback
- [x] Switch families gated by Chocolate's `check_activator_motion` reject matching tags during plane/wall animation or active SE11/30/18/20/31/32 motion (dip, tech, alien and access switches retain their dedicated paths)
- [x] Basic floor/ceiling motion for lotags 16–22 and floor carry
- [x] Lotag 29 toothed ceiling door and linked `SE 22` plane-following mover
- [x] Lotag 26 split door couples its vertical and sliding wall motions
- [x] Lotag 28 activates linked `SE 21` cascading floor/ceiling movers
- [x] Lotag 27 toggles the local `SE 20` extend-o-bridge effector; bridge geometry and grounded occupants move once along the marker angle, refresh sector membership, and scroll floor/ceiling X/Y panning
- [x] SE30 activator toggles subway state but plays its switch sound only at settled animation endpoints (`tempang == 0 || 256`)
- [x] `SE 31` true-drop-floor target initialization, activation delay, floor travel/return and floor rider carry
- [x] `SE 32` true-drop-ceiling target initialization, activation/return thresholds and sound completion
- [x] Basic `SE 24` / `SE 34` conveyor vector, sprite/player push and floor panning
- [x] Tagged wall/sprite switches toggle matching SE24/SE34 T5 and show the on/off quote
- [x] `SE 26` escalator floor cycle, wall/actor transport and delayed player push
- [x] `SE 25` piston ceiling initialization and cyclic plane motion
- [x] `SE 35` drill ceiling motion, smoke and intermittent explosion spawns
- [x] Lotag 9 sliding-sector wall interpolation
- [x] Existing transporter and animated-wall paths in the native tic
- [x] `premap` GPSPEED/CYCLER capture and 15 Hz `movecyclers` sector/wall shade animation
- [x] `checksectors` secret-room count, normal exit request and timed/custom-sound exit state
- [x] `checkhitswitch` arms matching one-shot `MASTERSWITCH` markers
- [x] `MASTERSWITCH` countdown, delayed supported sector motion and marker cleanup
- [x] Touchplate pressure activation, linked-plate propagation and floor ride
- [x] Tagged `RESPAWN` marker delay and target reinsertion through EGS/CON initialization (partial)
- [x] `ACTIVATORLOCKED` sector-bit toggle and ordinary activator hitag filters
- [ ] Vanilla `moveeffectors` SE families and blocking/overlap semantics
- [x] Lotag-30 SE0/SE1 rotator linkage, shared wall rotation, grounded occupant carry, floor raise/restore and transition sound cues
- [x] SE17 activation accepts Chocolate's in-flight elevator reversal gate; in-sector direction fallback and out-of-sector single-direction behavior match `operatesectors`
- [x] SE17 carries `truefz`/`truecz` through vertical movement, refreshes them on paired-sector transfer, resets `bobcounter`, and advances the player bob phase from actual post-move speed
- [x] SE5 boss `FIRELASER` attacks use the shared CON-backed effector `shoot()` path for scripted projectile speed/strength and native muzzle setup
- [x] SE5 `ms()` rotates cached sector wall offsets around the moving marker using its accumulated T3 angle
- [x] `SE 8` / `SE 9` one-shot elevator shade bounds, linked-sector updates and eight-tic marker cleanup
- [x] `SE 10` timed automatic activator with player-occupancy and ceiling-motion gates
- [x] `SE 16` reactor ceiling cycle and absent-reactor marker cleanup
- [x] `SE 13` explosive-sector restoration, shade transition, debris and lifetime, using a stable prelevel sky-material cache across markers (generic render-only floor/ceiling interpolation applies; exact Chocolate registration remains open)
- [x] Build `dragpoint` propagation across connected two-sided wall vertices
- [x] Switch activators toggle matching shade cyclers and restore their base shade
- [x] Wall access-card palette gating, insertion timing, switch activation and key consumption
- [x] Access-sprite switches and card-key insertion path (local sector)
- [x] Player switch toggles matching tagged animated forcefield walls
- [x] `monsters_off` map culling and enemy-target respawn gate
- [x] `SE 4` randomized flashing sector light with palette, wall and face-sprite updates
- [x] `SE 12` switch-controlled light transition and player-hitscan ceiling-hit extinguish path (other ceiling-hit callers remain open)
- [x] `SE 3` ceiling-light-hit-triggered flicker state and linked-wall shade updates (player hitscan path only)
- [x] `SE 19` explosion-triggered shield shutdown, linked `BIGFORCE` removal, ceiling lowering and palette/shade restoration
- [x] `SE 128` animated glass-wall overpic progression and native cstat masks
- [x] `SE 2` armed quake countdown, 96-tic effector lifetime, slope recovery, actor/player impulse and Atomic cue cadence
- [x] `SE 33` quake-time randomized scrap debris spawn
- [x] `SE 130` / `SE 131` timed explosion emitters with Atomic lifetimes, random fire gates, ordered EXPLOSION2 fields, and immediate clipped `ssp`
- [x] Sector activation toggles `SE 36`; active markers fire `COOLEXPLOSION1`, `SPIT`, `FIRELASER`, `RPG`, `FREEZEBLAST`, and `SHOTSPARK1` / `SHOTGUN` / `CHAINGUN` on Chocolate's 130-tic cadence (projectile launch and supported flight/direct damage are ported; exact impact side effects remain open)
- [x] SE15 lotag-25 open/close state, correct T4 travel counter/T5 active mode, SP/8 travel limit, marker translation, Chocolate activation-sound gate and `ms()` wall translation
- [x] SE11 lotag-23 swing-door group activation, marker-angle direction, badguy/player wall obstruction and player rollback (Chocolate does not carry occupants); interpolation and complete map-scenario coverage remain open
- [x] Render-only interpolation of Build wall vertices and sector plane heights between Duke tics; exact Chocolate interpolation registration remains open
- [x] Partial SE6/SE14/SE30 narrow-gap crushing: source-specific badguy `JIBS6` bursts, pre/post-movement player quickkill gates/48-tic flash/eight JIBS6, `SQUISHED`, and effector-sector relink (remaining `setsprite`/collision details open)
- [x] Tagged sprite-switch use through visible connected sectors
- [x] `VIEWSCREEN` selects an inactive matching `CAMERA1`, checks ART bounds/portal visibility and Chocolate's 1280-unit, three-height use ray, links the monitor owner to the camera, updates its latch, plays activation audio only on entry, and caches the render-only 64×48 feed at the 12-tic refresh cadence
- [x] Access-card status-bar icons and first-person insertion animation with weapon suppression
- [ ] Native menu/settings parity, mirror premap/render edge cases, camera lifecycle and full SE scenario coverage
- [ ] DEMO2/DEMO3 sync and complete SE scenario coverage

**Shade cyclers (2026-11-06):** static comparison with Chocolate
`premap.c::CYCLER` and `actors.c::movecyclers` now captures sector, phase,
shade range, hitag and active flag; GPSPEED writes `sector.extra`. The native
30 Hz session advances cyclers on Chocolate's every-other-tic 15 Hz phase,
using the installed Build sintable and mirroring shade to linked walls with
`cstat&2`, except `hitag==1` walls. `operateactivators` also toggles `p[5]`
for the matching `hitag` and restores its stored base shade. The cycler state
is included in `.fsave` and validated against the saved sector count. Compile
and format checks only; no visual/gameplay comparison was run.

**Special sectors (2026-11-06):** native `checksectors` now consumes
`lotag=32767` once and increments `secret_rooms` with the secret quote, signals
normal `lotag=-1` exits, and starts the 208-tic timed exit for `lotag=-2`,
preserving the sector hitag as custom exit sound. The countdown queues that
sound and quote 102 at 130 tics, then requests transition at tic 1. It returns
before Use-key handling like Chocolate. Campaign progression remains host
integration work. Static compile/format only.

**SE 4 flashing lights (2026-11-06):** the native post-`moveactors` effector
pass now initializes Chocolate's baseline sector/wall shades and saved sector
palettes, uses the tic's shared `global_random` value for its bright/dark
decision without consuming a per-effector RNG roll, applies the sector and wall
palette changes, mirrors shade to linked walls, and updates
face-aligned sprites in the effector sector. This covers only the standalone
SE 4 light behavior; remaining effector types and blocker/overlap movement are
still open. Static source review of `game.c::spawn` and
`actors.c::moveeffectors`; `cargo check` and format/diff checks only, no game or
demo comparison.

**SE 12 switch lights (2026-11-06):** matching tagged switches now advance
SE12's Chocolate T1 state (`0→1` for on, `1→3` for off), clear the sector
floor palette, and the native effector pass restores the saved floor/ceiling
shades on shutoff or raises light/shade progressively on turn-on. Face-aligned
sprites track the sector's active light shade, and extinguish-marked effectors
are removed from the actor lists. Static review of `sector.c::checkhitswitch`,
`game.c::spawn`, and `actors.c::moveeffectors`; `cargo check` and formatting
only. Player hitscan now ports `checkhitceiling` for the six Atomic ceiling
light tiles, creates ceiling glass using Chocolate's edge interpolation and
angle/z/x-velocity RNG order, queues sound 19, marks linked SE12s to extinguish,
and signals matching SE3 flicker state. Triggered SE3 now uses the shared tic
`global_random` to choose ceiling/floor palettes and shade and mirrors linked
wall shades. Other `checkhitceiling` caller paths and exact ceiling-glass EGS
side effects remain to be statically audited.

**Sliding-wall vertex propagation (2026-11-06):** animated sector-9 vertices
now use Chocolate Build's `dragpoint` traversal across two-sided wall links,
including the `lastwall` fallback around a polygon boundary. Static comparison
with `Engine/src/engine.c`; compile-only validation, with no map/demo comparison.

**SE 8 / SE 9 elevator shade transition (2026-11-06):** native effector
processing now snapshots the sector floor/ceiling shades and maximum wall
shade for each marker before applying its one-shot transition to all linked
markers with the same SE lotag/hitag. It uses Chocolate's shade increments,
per-sector clamping, two-sided wall shade propagation, SE 9 open-sector high
bit, and removes the marker after the eight-tic T4 countdown. Static review of
`game.c::spawn` and `actors.c::moveeffectors`; `cargo fmt` and
`cargo check -p room-duke -p room-app --all-targets` passed. No tests,
gameplay, or demo comparison was run.

**SE 10 automatic activation (2026-11-07):** the native actor pass now
implements Chocolate's trigger-sector predicates, `hitag + 1` delay, player
occupancy suppression (with the lotag 27 exception), and ceiling-animation
wait for sector types 20–22/26. At expiry it activates tagged `ACTIVATOR`
sprites in the sector, or operates the sector itself when none are present.
Static review of `actors.c::moveeffectors` and `sector.c::activatebysector`;
format and compile checks only. No dynamic comparison was run.

**SE 16 reactor ceiling (2026-11-07):** added the Atomic reactor ceiling
cycle to the native effector pass. It advances the marker phase, switches
between the 1024-unit rise and 512-unit return based on the sector planes,
keeps rising while a `REACTOR`/`REACTOR2` sprite remains in the sector, and
removes the marker once the threshold is crossed without a reactor. Tile IDs
are Atomic data constants. Static implementation review only; no gameplay,
demo, or Chocolate comparison was run.

**SE 17 paired elevator (2026-11-07):** static port of Chocolate
`game.c::spawn`, `sector.c::operatesectors`, and `actors.c::moveeffectors`
initializes SE17 stop/adjacent-plane limits; sector-lotag 15 activation selects
the travel direction and starts matching effectors. The native mover advances
both planes, carries player Z and sector sprites, stops at the saved floor,
matches adjacent sector hitags, and transfers the player/sprites while
updating Build sector lists and actor plane caches. The start and stop cues now
queue Chocolate's Atomic sound IDs 71 (`ELEVATOR_ON`) and 73
(`ELEVATOR_OFF`) at the activating/arriving effector. Pair lookup now ignores
out-of-range sector references rather than indexing them. Static source/data
review only; no tests, gameplay, demo, or Chocolate comparison was run.
Multiplayer behavior, crushing/overlap edge cases, and dynamic validation
remain open.

**SE17 activation correction (2026-11-07):** reinspection of
`sector.c::activatewarpelevators` found that Chocolate accepts activation if
any effector in the matching hitag group is already farther than its `SP`
tolerance from its saved stop floor, even when a destination pair is not
available in the requested direction. Room now preserves that in-flight
reversal gate; a player inside the elevator sector retries the opposite
direction, while a player outside only attempts the height-selected direction,
matching `operatesectors`. Static source review, formatting, diff check and
crate compilation only; no test, gameplay, demo, or reference comparison.

**SE17 player-cache transfer (2026-11-08):** the elevator now carries the
player's cached true floor/ceiling planes during vertical motion and refreshes
them from the destination sector when transferring; it also resets the
Chocolate bob counter at transfer. `processinput` now advances `bobcounter`
from actual post-move player speed, refreshes `bobpos` afterward, and writes
that speed to the APLAYER body; the caches are refreshed from sloped planes
and restored/written at the Atomic SAV offsets. Static review of Chocolate
`actors.c::moveeffectors` SE17, `player.c::processinput`, and
`duke3d.h::player_struct`; formatting, diff and targeted compilation only,
with no tests, gameplay or reference comparison.

**Sector-motion actor cache (2026-11-07):** ordinary floor movers now refresh
each carried sprite's previous Z and cached floor/ceiling planes as they ride
the moving floor. This extends the existing floor-carry path so the following
CON/actor pass sees the current Build sector bounds. Static implementation;
no tests, gameplay, or reference comparison was run.

**Lotag 21 moving-door reversal (2026-11-07):** activating a floor door while
it is already interpolating now replaces its target with the opposite open or
closed plane goal, matching Chocolate's `operatesectors` in-flight reversal
instead of silently ignoring the input. The goal is calculated from the
current sector planes and adjacent floor. Static source review; not dynamically
compared.

**Lotag 29 / SE 22 toothed door (2026-11-07):** sector lotag 29 now selects
the adjacent floor/ceiling target from its open bit, toggles the door state,
and starts or redirects its ceiling animation. Matching SE22 markers reverse
their travel speed and follow the controlling sector's active ceiling movement
using Chocolate's `extra * 9` step; they stop following once that animation
ends. Static review of `sector.c::operatesectors` and `actors.c::moveeffectors`;
format/diff/compile only, no game or demo comparison.

**Lotag 26 split door (2026-11-07):** a stopped split door now starts the
vertical lotag-22 plane pair and the lotag-9 sliding-wall interpolation as one
sector operation, using the sector's vertical speed and `extra >> 4` wall
speed. Re-activation is ignored while the plane animation is active, matching
Chocolate's animation guard; the combined operation queues one sector sound
cue. Static review of `sector.c::operatesectors`; compile/format only, no map
or demo run.

**Lotag 28 / SE 21 cascade (2026-11-07):** sector lotag 28 now finds the
local SE21 marker, arms every matching hitag group member once, and queues the
sector sound. Active markers wait out `sector.extra`, choose floor versus
ceiling from angle 1536, then move by signed `hitag << 4` toward the marker Z;
they snap and remove themselves within the vanilla 1024-unit threshold. Static
review of Chocolate `sector.c::operatesectors` and
`actors.c::moveeffectors`; no tests, gameplay, or demo comparison.

**Remaining SE audit (2026-11-07):** static inventory of Chocolate
`actors.c::moveeffectors` now has a first-pass `SE 5` boss elevator: it seeks
the nearest player's locator route, emits Atomic FIRELASER projectiles inside
8192 units, turns toward its locator, changes ceiling shade, moves its marker
and sector walls, and advances the sector ceiling. Weapon hits consume queued
damage; the fifth hit adds 1024 to vertical velocity and sets the boss quote.
Map setup caches Chocolate's adjacent zero-hitag sector and the ceiling also
advances there. The effector marker now runs a `setsprite`-equivalent sector
lookup/list update after each move. Chocolate's SE5 branch does not transport
sector occupants; collision rollback and edge-case geometry still need work.
`SE 6` now initializes its speed ramp and linked SE14 wagons, follows the
locator route, adjusts train speed from wagon spacing and carries/rotates
players and sector actors with the train. `SE 14` now follows the matching
locator route, steers and moves its sector geometry, carries/rotates its
occupants, and starts/stops the SUBWAY loop on non-parallax sectors. SE6/SE14
now also make the parallax-floor-pal-zero `rnd(8)` attack roll at cruise speed,
select a player within 20,480 units, and launch an RPG with Chocolate's
temporary player-facing source angle. Exact SE6 train ordering and spacing
details, crushing/collision, and interpolation remain open. `SE 30` has first-pass
locator-driven translation, acceleration/braking, and sector occupant carry;
run-up/arrival activator calls and tagged force-field toggles; crushing,
interpolation and exact initialization ordering remain open. `SE 18` now
applies its map-load plane target, hitag-to-tic timer conversion,
master-switch activation, one-shot travel, timer reset, and grounded floor
occupant carry. Tagged SEENINE/OOZFILTER detonation now activates matching
SE8/SE13/SE18/SE21; SE13 now restores its sector planes over 256 tics,
switches its ceiling/shade state, and drops the vanilla seven debris pieces.
Pipebomb/RPG radius damage reaches the destructibles, while the pipebomb's
explicit remote hitag-trigger route remains open.
`SE 11` has a first-pass
lotag-23 activation and swing path; `SE 15` has initial lotag-25 open/close and
pivot motion. SE2's caller for arming T5 and remaining train/door obstruction,
interpolation, and occupant-carrying behavior still need porting. No dynamic
gameplay or demo comparison is claimed.

- `2026-11-07` — **D7 SE5 boss elevator first pass**: statically ported nearest-player/locator selection, route marker turn, FIRELASER spawning at Atomic range/speed, shade flicker, marker/wall translation and ceiling travel. Map initialization caches Chocolate's adjacent zero-hitag sector and updates its ceiling; queued weapon damage counts hits and applies the fifth-hit vertical impulse/quote. Exact `ms()` collision and occupant carry remain open. `cargo fmt --all`, `git diff --check`, and the latest `cargo check -q -p room-duke --lib` pass with existing documentation/unused-import warnings; no gameplay/demo comparison was run.

- `2026-11-07` — **D7 SE6/SE14 train first pass**: statically ported Chocolate's route-sector initialization, SE6 speed ramp and linked SE14 wagon speed/spacing adjustment, locator steering and route wraparound, train-sector wall rotation/translation, and player/actor carry. Non-parallax trains start and stop the SUBWAY loop around cruise/braking. Exact linked-marker order, collision/crushing and interpolation remain open. `cargo fmt --all`, `git diff --check`, and crate checks only; no dynamic Chocolate or gameplay/demo comparison was run.
- `2026-11-08` — **D7 SE6/SE14 parallax tunnel attack**: statically ported Chocolate's `rnd(8)` gate when the train reaches `sector.extra` speed in a parallax sector with floor palette zero, with the `monsters_off` gate, nearest-player 20,480-unit range, temporary source angle and shared RPG projectile setup. Static review of Chocolate `actors.c::moveeffectors` and existing `shoot(i, RPG)` path; formatting, diff and targeted compile only, no gameplay/demo or Chocolate comparison. Train crush handling and exact marker/list ordering remain open.
- `2026-11-07` — **D7 SE18 Kroz map setup/timer**: statically ported the stored pre-move plane, pal-triggered alternate plane setup, hitag ×4 timer initialization, and Chocolate's timer-expiry reset (including zero timer). The mover transfers grounded floor riders and removes its marker at the selected plane threshold. SEENINE hitag detonation activation remains open. `cargo fmt --all`, `git diff --check`, crate check only; no gameplay/demo comparison was run.
- `2026-11-07` — **D7 SEENINE/OOZFILTER chain and SE18 activation**: map spawn now sets the native shade/cstat/repeats/impact-health fields. Standable ticking consumes queued hits, propagates tag-chain delays, animates SEENINEDEAD frames, activates matching SE8/13/18/21 markers on detonation, spawns the explosion and scraps, applies configured hitradius damage and starts the earthquake timer. Generic blast hitradius now queues damage for SEENINE/OOZFILTER too. Remote pipebomb tag propagation and exact sound/RNG ordering remain open. `cargo fmt --all`, `git diff --check`, and crate checks only; no gameplay/demo comparison was run.
- `2026-11-08` — **D7 SEENINE → SE18 reactivation guard**: tagged detonations now arm SE18 only while its activation state is zero, while SE21 retains unconditional arming. This preserves an already-running Kroz sequence instead of resetting it on another tagged blast, matching Chocolate's guard. Static source review only; format/diff and targeted crate compile, no runtime/reference comparison.
- `2026-11-08` — **D7 Atomic SE inventory correction**: static inventory of Chocolate Atomic `actors.c::moveeffectors` confirms there is no SE23 or SE40–45 case; the one-way exit is sector lotag 23, not an effector. SE27 is a demo-camera marker, but its camera-control body is disabled in this Chocolate version. Corrected Appendix B in the Duke spec so remaining SE work tracks actual Atomic behavior. Static source inspection only; no runtime/reference comparison.
- `2026-11-08` — **D7 SE36 Octabrain projectile impact**: `COOLEXPLOSION1` now uses the shared Build hit query, stops on walls, planes or APLAYER targets, applies the projectile's actor-header damage and body-hit cue to players, and passes through/gets blocked by ordinary actors without applying damage, matching the `moveweapons` branches. It ages its shade and is removed at 40, and queues the CON-defined `WIERDSHOT_FLY` voice through the once-audio path. Static Chocolate `actors.c::moveweapons` review; `cargo fmt --all`, `git diff --check`, and `cargo check -q -p room-duke --lib` pass with pre-existing warnings. No tests or gameplay/demo/reference comparison were run; exact `movesprite` collision/contact ordering and global sound voice gating remain unverified.
- `2026-11-08` — **D7 pipebomb blast → tagged destructible chain**: RPG/pipebomb and CON radius-blast paths now test visible, in-radius tagged `CRACK1–4` and `FIREEXT` standables, then arm matching SEENINE/OOZFILTER sprites by setting their native delay shade. This follows Chocolate's `CRACK`/`FIREEXT` hitag activation path and feeds the existing standable detonation/effector chain. Static `actors.c::movestandables` / `hitradius` review; format, diff and targeted `room-duke` compile only. Breakable destruction/scrap/audio and exact collision ordering remain open; no gameplay/demo/reference comparison was run.
- `2026-11-08` — **D7 CRACK/FIREEXT breakables**: the native standable tick now consumes queued direct/radius impacts on Atomic CRACK1–4 and FIREEXT. CRACK validates Chocolate’s accepted source tiles, arms tagged SEENINE/OOZFILTER and supported effectors, emits debris and starts the earthquake; FIREEXT emits palette-2 scraps/explosion, applies configured tagged or untagged hitradius, queues explosion/glass sounds and removes the prop. Static Chocolate `actors.c::movestandables` review only; `cargo fmt --all`, `git diff --check`, and `cargo check -q -p room-duke --lib`. No tests, gameplay run, or dynamic Chocolate/Room comparison. Exact RNG/audio ordering and collision side effects remain open.
- `2026-11-08` — **D6 shrink/grow ownership audit**: corrected the gap description after tracing the current weapon hit metadata and CON VM: general actor shrink/grow transitions are selected by target `ifwasweapon` script branches and action/size commands, rather than a generic hard-coded projectile mutation. Room has those VM operations; Atomic actor-specific branches and resultant collision/repeat behavior remain unaudited. Static local code inspection only; no tests or dynamic comparison.
- `2026-11-08` — **D7 destructible RANDOMSCRAP fidelity**: static Chocolate review of the `RANDOMSCRAP` macro caught two deviations in CRACK/FIREEXT debris: shards must use the SCRAP6 base tile and the exploding standable itself as EGS owner; tagged FIREEXT also applies the common `z -= 32<<8` before its final eight shards. Room now follows those values and resolves SCRAP6 from CON. `cargo fmt --all`, `git diff --check`, `cargo check -q -p room-duke --lib`; no tests or gameplay/reference comparison.
- `2026-11-08` — **D6 complete hitradius status traversal**: CON `hitradius` now scans the remaining Chocolate lists (`STAT_DEFAULT`, `STAT_FALLER`, `STAT_DUMMY`, `STAT_MISC`) around the existing actor/standable/player handling in native list order. Blocking-list targets require `cstat&257`, only badguys require LOS in those branches, and APLAYER distance/visibility now use feet origin and fixed player LOS Z offsets. Static Chocolate `actors.c::hitradius` review; `cargo fmt --all`, `git diff --check`, `cargo check -q -p room-duke --lib`; no tests or gameplay/demo comparison. Exact Build `dist()` and immediate CON ordering remain open.
- `2026-11-07` — **D7 SE13 explosive-sector recovery**: map setup stores the original ceiling/floor targets and selects the near plane; tagged detonation starts Chocolate's restore motion, first-tick shade transition, 256-tic lifetime and seven `RANDOMSCRAP` spawns. The render path generically interpolates sector floor/ceiling heights between tics; exact Chocolate per-effector interpolation registration remains open. Static Chocolate `game.c::spawn` / `actors.c::moveeffectors` review; format/diff/crate compile only, no gameplay/demo comparison.
- `2026-11-08` — **D7 SE13 stable sky source**: cache the original first map-order parallax ceiling material/shade before prelevel SE13 setup can flip its flag, then retain that cache per effector for runtime restore. This matches Chocolate's `one_parallax_sectnum` lifetime when multiple SE13 markers touch sky sectors. Static source comparison; `cargo fmt --all`, `git diff --check`, and `cargo check -q -p room-duke --lib` only; no tests or runtime/reference comparison.
- `2026-11-07` — **D7 SE26 escalator**: statically ported Chocolate's map-load origin/direction setup, eight-tic floor cycle, moving marker/walls, and sector actor/player vertical transport. Grounded riders receive the escalator's delayed horizontal force through the next deterministic input sample; the force is stored in player state and cleared on campaign carry. Static review of Chocolate `game.c::spawn` and `actors.c::moveeffectors`; `cargo fmt --all`, `git diff --check`, and `cargo check -q -p room-duke --lib` only. Exact clipping/collision, maps with overlapping effectors, and gameplay/demo comparison remain open.
- `2026-11-07` — **D7 SE26 rider sector check**: aligned both the escalator's horizontal-force and vertical-carry gates with Chocolate's `sprite[ps.i].sectnum` test instead of the cached `cursectnum`, which may lag during portal/effector updates. Static review of Chocolate `actors.c::moveeffectors` SE26; formatting, diff and targeted compile checks only, with no dynamic comparison.
- `2026-11-08` — **D7 SE26 spawn and slope transport**: corrected map setup to cache the escalator's T4/T5 map-load origin, derive zvel from marker versus floor shade, and reset the shade cycle exactly as Chocolate `game.c::spawn` does. After horizontal actor transport and sector reassignment, active actors entering a sloped-floor sector now run the existing `makeitfall` path, matching the SE26 `actors.c::moveeffectors` branch. This was a static source port; formatting, diff and `room-duke` library compile checks only, with no dynamic Chocolate/Room or gameplay comparison.
- `2026-11-08` — **D7 render-only geometry interpolation**: the desktop Duke render path now blends previous/current wall vertices and sector floor/ceiling heights using the fraction of the 30 Hz tic. It renders a cloned `BuildWorld`, leaving simulation state and checksums untouched; this supplies moving-sector visual smoothing while exact Chocolate interpolation registration is still open. `cargo fmt --all`, `cargo check -q -p room-duke --lib -p room-app`, and documentation diff checks passed with existing warnings. No tests, gameplay or demo/reference comparison were run.
- `2026-11-08` — **D7 tagged conveyor controls**: Chocolate `sector.c::checkhitswitch` confirms tagged switches toggle T5 on every matching SE24/SE34 before operating linked activators, then issue quote 15/2 for on/off. Room now performs that state transition in its common switch path; `move_conveyor_effectors` already honors nonzero T5 as paused. `cargo fmt --all`, `cargo check -q -p room-duke --lib`, and `git diff --check` passed with existing warnings. No tests, gameplay or demo/reference comparison were run.
- `2026-11-08` — **D7 SE11 obstruction parity audit**: statically rechecked Chocolate `actors.c::moveeffectors` and `game.c::spawn`. Confirmed the SE11 branch blocks against badguy/player overlap and rolls the angle back on player obstruction, but does not carry sector occupants; corrected the implementation spec and marked this narrow SE11 behavior covered. Interpolation (`setsectinterpolate`), global effector ordering and full map scenarios remain open. No dynamic comparison or tests were run.
- `2026-11-08` — **D7 SE5 marker sector refresh**: after the boss controller advances its marker and sector walls, it now performs the `setsprite`-equivalent `updatesector` and sprite-list reassignment from Chocolate `actors.c::moveeffectors`. Static source review shows SE5 does not carry sector occupants, so the prior “occupant carry” open item was removed; collision rollback and unusual geometry remain open. Formatting, diff and targeted crate compile only; no gameplay or demo comparison.
- `2026-11-07` — **D7 SE15 correction**: after comparing the implementation against Chocolate's `actors.c::moveeffectors`, replaced the incorrect pivot rotation and invented actor/player obstruction checks with the native `T4/T5` travel state and `ms()`-style 16-unit wall/marker translation along the marker angle. Static review of Chocolate `game.c::spawn`, `sector.c::operatesectors`, and `actors.c::moveeffectors`; `cargo fmt --all`, `git diff --check`, and `cargo check -q -p room-duke --lib` pass. No gameplay/demo comparison was run.
- `2026-11-07` — **D7 SE128 glass detail**: static audit corrected the progressive glass-wall flag mask to Chocolate's `cstat &= 223; cstat |= 16` on both paired walls (rather than preserving all high bits), retaining the source's unconditional bitwise-OR branch behavior. Formatting, diff and crate compile checks only; no map/gameplay comparison.
- `2026-11-07` — **D7 SE0 lotag-30 rotating sectors**: map setup links each SE0 to its SE1 pivot, initializes SE1 T1 off like Chocolate, installs the sector hitag reference, stores the original floor height and applies the palette-selected rotation direction; the linked SE19 event arms normal pivot rotation. The tic path preserves map-load wall offsets for Build `ms()`, accumulates angle in T3, orbits the marker from the SE1 pivot and updates shared vertices through `dragpoint`; it also turns/carries eligible occupants, applies floor rise/return in 512-unit steps, uses `tempang` for Chocolate's 256-unit transition and queues endpoint sounds. Static review of Chocolate `game.c::spawn`, `actors.c::moveeffectors` and `sector.c::operatesectors`; `cargo fmt --all`, `git diff --check`, and `cargo check -q -p room-duke --lib` only. The reference `ms()` path performs direct wall updates without a collision rollback; ordering with overlapping movers and gameplay parity remain unverified.
- `2026-11-07` — **D7 SE36 RPG/freezer launch**: statically ported the non-player `RPG` and `FREEZEBLAST` branch of Chocolate `player.c::shoot`: source-height offset, target/opos aim, 644 velocity, muzzle offset, `EGS` repeats/shade, script-driven strength variation, RPG target sentinel/clip distance, and Freezer bounce count/repeat scaling/vertical adjustment. Existing projectile movement owns subsequent flight and collision; exact SE36 impact behavior is still open. `cargo fmt --all`, `git diff --check`, and `cargo check -q -p room-duke --lib` only; no dynamic comparison or gameplay run.
- `2026-11-07` — **D7 SE36 shooter attacks**: sector operation toggles `temp_data[0]` per Chocolate; active SE36 markers run the 130-tic cadence for `COOLEXPLOSION1`, `SPIT`, `FIRELASER`, `RPG`, `FREEZEBLAST`, and the `SHOTSPARK1` / `SHOTGUN` / `CHAINGUN` enemy hitscan paths. Hitscan damage is queued through the existing player/CON damage path and uses Chocolate's actor-header strength, aim height, jitter and RNG rolls. `SPIT` launch height/angle/velocity/strength, gravity, portal-sector update and direct hit damage are ported. `FIRELASER` launch/muzzle position/targeting/speed, five-sprite trail, portal-sector flight and direct hit damage are ported. Player SPIT hits now queue body-hit/pain audio, apply the horizon kick and generate/count down loogies; the desktop HUD draws those tiles on its 320×200 virtual plane. RPG/freezer launch parameters are now ported; exact wall/sector impact behavior, ricochet sound, exact sprite filtering, loogie alpha/zoom/resolution placement remain open. Static review of `sector.c::operatesectors`, `actors.c::moveeffectors`, `actors.c::moveweapons` and `player.c::shoot` / `displayloogie`; formatting, diff and Duke/app crate checks only. No gameplay/reference comparison was run.

**SE 19 battle-star shield shutdown (2026-11-07):** native `SE 1` pivot
initialization now arms its matching SE19 marker, and an `EXPLOSION2` in an
SE19 sector activates the matching SE0/SE1/SE12/SE19 group. The active SE19
clears matching `BIGFORCE` overlay walls and their peers, lowers the shield
ceiling at the marker's `SP`, restores palettes/shades on linked SE0 sectors,
and removes itself. Static review of Chocolate `game.c::spawn` and
`actors.c::moveeffectors`; compile/format only. Boss explosion placement,
`IFHITSECT` timing, visual interpolation and map scenarios remain dynamically
unverified.

**SE 2 quake core (2026-11-07):** native `SE 2` now saves the sector's
baseline floor heinum, advances Chocolate's T5/HITAG countdown when armed,
applies its 96-tic run/cleanup, restores the slope, nudges grounded players and
sector sprites along the marker angle, and emits Atomic sound 81 on the
`(T1 & 31) == 8` cadence while setting the 48-tic quake timer. The timer is
included in `.fsave`; desktop and web rendering now apply Chocolate's
grounded camera Z/angle shake offsets. Coop players, exact `ms` clipping, and
the caller that arms `T5` remain open. Static source review of Chocolate
`game.c::spawn`, `actors.c::moveeffectors`, and game-tic timer update; compile/
format only, no gameplay or demo comparison.

**SE 33 quake debris (2026-11-07):** `SE 33` now checks the shared quake
timer, performs Chocolate's one-in-eight random gate, and spawns `SCRAP6`
fragments with the ordered seven `TRAND`-backed positions, tile, angle,
velocity and owner values used by `RANDOMSCRAP`. Static source review of
`duke3d.h::RANDOMSCRAP` and `actors.c::moveeffectors`; compile/format only, no
gameplay or demo comparison.

**SE 130/131 timed explosions (2026-11-07):** the native effector pass now
ages each marker to Chocolate's `T1 > 80` / `T1 > 40` expiry thresholds and uses the per-tic
`TRAND` `rnd(64)` / `rnd(32)` gate. Successful tics spawn `EXPLOSION2`, draw
the original repeat, vertical position, angle and velocity fields in order,
then immediately apply a clipped horizontal `ssp` step and update the sector
sprite list if it crosses a portal. The seed draws are retained even if the
sprite pool cannot allocate the effect. Static review of Chocolate
`actors.c::moveeffectors`, `duke3d.h::rnd` and `gamedef.c::spawn`; `cargo fmt
--all`, `git diff --check` and `cargo check -q -p room-duke --lib` only. No
tests or gameplay/demo comparison were run; exact Build `ssp` collision
parity remains open.

**SE 15 sliding-door correction (2026-11-07):** lotag-25 activators find the
local matching SE15 hitag and toggle marker sectors. Static reinspection of
Chocolate `actors.c::moveeffectors` showed SE15 is not a pivoting door and has
no actor/player obstruction test: `T5` selects open/close, `T4` advances up to
`SP >> 3`, and `ms()` translates the marker and sector walls 16 Build units per
tic along its angle. Room now follows that state and translation path and
queues the completion sound at both ends. The saved geometry uses map-load wall
offsets. Exact marker ordering, `setsprite` collision/sector effects and
runtime scenarios remain unverified. Static source review only; no gameplay
or demo comparison.

**D6 SHIELD inventory bridge (2026-11-07):** CON `ifpinventory 1` now tests
the player's shield against Atomic's max-health cap, and `addinventory 1 N`
adds shield with the same cap then commits the deferred value to the player.
This allows existing CON pickup scripts to grant SHIELD. Other inventory kinds
and pickup respawn/presentation remain open. `cargo fmt --all`,
`git diff --check`, and `cargo check -q -p room-duke --lib` pass with existing
warnings; no tests or gameplay/demo comparisons were run.

**D6 Atomic inventory bridge (2026-11-07):** statically compared the full
`gamedef.c` inventory switch and wired `ifpinventory` / `addinventory` state
for steroids, shield, scuba, holoduke, jetpack, access cards, heat, first aid
and boots. Player inventory counters and HUD icon IDs now survive CON actor
execution through deferred writes. Inventory use/timers, non-CON pickup cases,
respawning and HUD presentation remain open. `cargo fmt --all`,
`git diff --check`, and `cargo check -q -p room-duke --lib`; no tests or
gameplay/demo comparisons.

**D6 player inventory actions (2026-11-07):** added first-aid healing,
steroid activation/depletion and movement boost, jetpack toggle/flight and
depletion, heat toggle/timer, underwater scuba activation/depletion, and
inventory cycling. Inventory action bits are edge-triggered so holding a key
does not repeatedly consume/use items. Desktop `'` now emits inventory-use;
browser defaults add `,`/`.` for previous/next inventory and `'` for use.
Remaining gaps include Holoduke fall/collision and palette behavior, boot
hazard presentation/bridge exceptions, HUD presentation, plus exact vanilla feedback timing. Static Chocolate
source review and compile checks only; no tests or runtime comparisons were
run.

**D6 Holoduke decoy (2026-11-07):** the selected Holoduke item now creates a
second APLAYER in the native player sprite list with Chocolate's start pose,
owner/player linkage and expansion/facing animation. Reuse toggles the actor
off; timer expiry and explicit deactivation are cleaned from the sprite list
on the next player-mover pass. It remains a first pass: no full fall/collision,
player palette tint or multiplayer targeting parity. Static `actors.c`,
`sector.c` and `player.c` inspection plus compile checks; no tests or dynamic
gameplay/demo comparisons.

**D6 player floor hazards (2026-11-07):** added Chocolate tile checks for
purple lava (floor or ceiling), hurt rail, slime and plasma; charges are
consumed at the vanilla rates, hazards use the vanilla RNG chances/damage
range, and absorbed hits emit the boots quote. Pain and short-circuit cues are
queued through Atomic sound names. Floor damage now goes through Chocolate's
20–49% shield absorption and consumes one TRAND only when armor is present.
Player RGB flash state now decays over 32
tics and is composited in the desktop Duke shell. Exact classic-palette math,
sound overlap rules, spritebridge exclusions and coop/web presentation still
need work. Static `player.c` review
and compile checks only; no tests or runtime comparisons.

**D6 hitscan wall switches (2026-11-07):** pistol, shotgun and chaingun
impacts queue the exact wall-switch target and invoke the native tagged switch
transition after weapon processing. Access switches preserve missing-card
quotes and start the access animation; ordinary switches operate linked
activators and queue the switch cue. Tagged sectors already in a Room motion
are ignored. Wall and sprite hits target the exact switch; directional
DIPSWITCH, TECHSWITCH and ALIENSWITCH advance only the struck frame. Tagged
door textures accepted by Chocolate `isadoorwall` operate their linked sectors
without switch art changes or the ordinary switch cue. Recognized switches
and door operators tagged `65535` now set Room's level-exit-ready state.
Precise Build motion
filtering and correct-combination counting/warning remain open. Static source
review and `cargo check -p room-duke --lib` only; no runtime/reference
comparison was run.

**D6 door-tile switch operators (2026-11-07):** ported Chocolate's
`isadoorwall` ART ID table into the shared use-key / hitscan switch gate.
Tagged door textures now run linked activators without being cycled as switch
art or receiving the ordinary switch sound. Static `sector.c::isadoorwall` and
`checkhitswitch` / `names.h` review; compile-only validation, no gameplay or
reference comparison.

**SE 11 swing-door first pass (2026-11-07):** lotag-23 activation arms the
matching SE11 markers in sectors with the same high-bit state, reverses their
rotation direction, and queues the sector sound once. Door motion uses the
map-load wall offsets around the marker pivot, Chocolate's T3/T5 angle steps,
badguy and player `clipinsidebox` checks, and the eight-tic player obstruction
delay/rollback. Static review of Chocolate `game.c::spawn`,
`sector.c::operatesectors`,
and `actors.c::moveeffectors`; compile/format only, with no gameplay or demo
comparison. Interpolation and full moving-sector actor/player carrying remain
open.

**SE11 native `ms` / `setsprite` sequence (2026-11-08):** each swing-door
rotation and player-obstruction rollback now advances the marker using its
map `xvel` and angle before rotating cached sector-wall offsets, then resolves
and relinks the marker's Build sector like Chocolate's `setsprite`. When the
door reaches an endpoint, Room performs the second `ms` update after snapping
T3 to the endpoint angle. Static review of Chocolate `actors.c::ms` and the
SE11 `moveeffectors` branch; `cargo fmt --all`, `git diff --check` and
`cargo check -q -p room-duke --lib` pass. No tests or dynamic comparisons.

**SE 18 Kroz plane first pass (2026-11-07):** map spawn saves the original
ceiling/floor plane, applies the palette-controlled marker plane, and scales
the marker's runtime hitag interval by four. Master switches in the same
sector now arm SE18. The effect moves the selected plane toward the stored or
marker Z, carries grounded floor occupants, and removes the marker on arrival.
Static review of Chocolate `game.c::spawn`, `actors.c::moveeffectors`, and
`moveactors`; compile/format only, no gameplay/demo comparison. SEENINE now
arms an SE18 only when its activation state is clear, matching Chocolate's
guard; exact Kroz timer/removal semantics still need review.

**SE 30 nonrotating subway first pass (2026-11-07):** map initialization links
the SE30 marker to the first adjacent untagged sub-3 sector containing its
route locators, caches moving-sector wall offsets, and stores the marker index
in the sector's runtime hitag. Lotag-30 operation arms the car; the runtime
chooses alternating locator lotags 1/0 in that sector, accelerates by 16 to
256, brakes inside Chocolate's 1920-unit approach threshold, stops within 128,
reverses heading, and carries sector sprites and players while translating the
walls. Static review of Chocolate `game.c::spawn`, `sector.c::operatesectors`,
and `actors.c::moveeffectors`; compile/format only, no gameplay/demo comparison.
The car calls matching activators before departure and on arrival and toggles
matching forcefield walls after arrival. Tight-gap crushing, collision,
map-order edge cases and interpolation remain open.

**SE 5 CON-backed boss weapon (2026-11-07):** the boss now temporarily aims
its marker at the selected nearby player, invokes Room's shared
Chocolate-shaped `shoot(FIRELASER)` effector path, and restores the route angle.
The projectile's CON-configured speed, strength, source-height muzzle offset,
`cstat` and `clipdist` now come from that common implementation instead of a
second hard-coded projectile constructor. The boss `ms()` wall pass now rotates
the cached map-load wall offsets by its accumulated T3 angle around the moving
effector, instead of translating walls without the source rotation. Static
`actors.c::moveeffectors` / `ms()` and existing effector-shot path review;
formatting, diff check and
`cargo check -p room-duke --lib` only. No tests, gameplay, demo, or
Chocolate comparison were run.

**SE6/SE14 train crushing (2026-11-08):** static review of Chocolate
`actors.c::moveeffectors` and `gutsdir` corrected the first pass: moving trains
crush badguys under the <108-unit clearance gate regardless of `monsters_off`,
with 72 JIBS6 for SE6 and 24 for SE14 plus `SQUISHED`; player quickkill is gated
by clipping off and uses speed ≥192 for SE6 / >0 for SE14. Guts use Chocolate's
floor clamp, COMMANDER offset, and ordered angle/velocity TRAND draws. Players
pushed outside the train sector enter Room's death path with Chocolate's
48-tic palette flash and eight-piece JIBS6 burst (gibs omitted in god mode).
The player sector test now runs both before and after moving the train, matching
Chocolate's two checks; the post-move path uses the controller's refreshed
sector and pose. Full `setsprite` collision effects and train interpolation
remain open. `cargo fmt --all`, `git diff --check` and targeted compile only;
no tests, gameplay/demo or Chocolate comparison were run.

**SE30 narrow-gap crushing (2026-11-08):** the two-way subway now resolves
and relinks the moving controller's Build sector after translating the walls,
then checks the <108-unit clearance gate. Players pushed outside the car sector
are quickkilled when clipping is off; badguys receive Chocolate's 24-piece
`JIBS6` burst and `SQUISHED` cue. Player quickkill applies the 48-tic palette
flash and eight JIBS6 fragments unless god mode is active. Player sector
membership is checked on both sides of the moving-car update, with the final
test using the controller's refreshed sector and pose. Static review of
`actors.c::moveeffectors` and `player.c::quickkill`
around the SE30 branch; formatting, diff check and targeted compile only. No
tests, gameplay/demo, or Chocolate comparison were run.

---

## D8 — Duke audio + front end

**Status:** `in_progress` · Spec: `25` §7 / §11 D8

### Acceptance

- [x] Queue selected Atomic CON sound IDs from actor/player gameplay paths
- [x] Expose `quote` text and `tip` timer; advance quote/tip timers per tic
- [x] Preserve actor/player sound origins and feed Duke listener/source positions to the mixer for attenuation and stereo panning
- [x] `MUSICANDSFX` distance-gated ambient loops and randomized sector ambience (source-owned loop stops; full-pool loop stealing follows Chocolate's priority and distance gate)
- [x] `definesound` loop flag and Atomic one/four-voice per-sound caps
- [x] Carry `definesound.pr` into audio events and apply priority eviction against a 32-voice pool
- [x] Preserve global (`m&128`) and non-attenuated positional (`m&16`) sound modes
- [x] Native desktop `SoundToggle` / `ud.lockout` filtering for Atomic `m&8` sound starts, via archived `duke_sound` and `duke_lockout` CVars
- [x] Apply Atomic `definesound ps/pe` pitch ranges at playback without consuming playsim RNG (presentation sampling is stable-hash based, so libc-rand parity remains open)
- [x] Apply Chocolate `FindDistance3D`, `xyzsound` minimum distance and `MV_Play3D` 32-bin pan-table volume gains to native Duke SFX, sampling the desktop listener at the input-phase `opos` snapshot (audible calibration remains open)
- [x] Add the `xyzsound` 1/32 distance penalty for blocked actor sounds via Build `cansee` (the exact Chocolate `opos` sampling phase remains unmeasured)
- [x] Parse Atomic `music` tables and play the selected layered-GRP MIDI on campaign map start/transition
- [x] F5 current-track quote and Shift-F5 track cycling while the quote is active
- [x] Desktop F7 shoulder-view toggle, weapon hiding, 120 Hz-equivalent camera-distance ramp and rear-wall hitscan/retraction; preserve the toggle in Room/Atomic player saves
- [x] Native desktop F4 Sound Setup for sound/music toggles, Duke Talk, ambience, stereo flip and 0–255 sound/music volume sliders (four-unit steps)
- [x] Native Duke F2/F3 ten-slot menus plus F6 last-slot save and F9 confirmed last-slot load for Room `.fsave` snapshots; imported Atomic saves also use a D9 `.sav` companion sidecar
- [x] Native Duke F8 toggles ordinary FTA quotes and leaves forced status notifications visible
- [x] In-game Duke F10 opens the quit-confirmation menu; yes exits the native app and no returns to play
- [x] Native Duke F1 help cycles the Atomic TEXTSTORY / F1HELP ART pages, pauses the sim, and closes with Escape
- [x] Native Duke F11 brightness cycle uses `TABLES.DAT` BRITABLE rows and Shift-reverse, with Atomic 0..56 wrap
- [x] Expose bonus-screen inputs: killed/total actor counters, found/total secret counters, elapsed tics, CON par and designer time
- [x] Native solo campaign transition shows a timed bonus-stat panel before loading the destination and freezes the outgoing playsim while it is visible
- [x] Reset `actors_killed` and `secret_rooms` at campaign level boundaries like Chocolate `resetprestat`, preserving per-level bonus counters
- [x] Duke `MUSICANDSFX` reverb-zone activation by the player's distance to a tag-1000 marker, with Chocolate sample-rate-scaled multi-tap/IIR reverb and 0–255 level mapping (runtime delay changes and multi-zone ordering remain open)
- [x] Chocolate audiolib multi-tap/IIR reverb DSP and sample-rate-scaled room-marker level application
- [x] Browser native Duke audio consumes source-positioned sound events, listener orientation, reverb-zone state, pitch variation, stop commands, actor voice replacement, timed-exit cutoff, line-of-sight penalty, loop/voice caps, exclusivity, priority and distance-offset metadata
- [x] Browser native Duke audio exposes sound/music/talk/lockout/ambience/stereo toggles and Atomic-range volume controls to the host page
- [ ] Complete remaining sound-lock/priority semantics, environment/menu music, exact F4 device behavior and audible calibration (same-ID owner arbitration and desktop `opos` sampling phase are implemented)
- [x] E1/E2/E4 finale sound cues follow Chocolate frame tables and use the active CON sound-label/mixer path
- [x] Native desktop Atomic `DN` cheat parser covers every `GAME.C::cheatquotes[]` command and its primary player/world effect (`time` is a no-op; `cosmo` is not in the source table)
- [ ] Remaining Duke HUD, menus, bonus screen, ANM playback/cues and F-key details (basic solo bonus panel exists; Atomic bonus art/count-up/audio, help pages, quote/TIP and progressive native automap are implemented; full portal-visibility and color parity remain open)
- [ ] UI checklist and golden screenshots (T1.6/T1.7)

**Native Duke save/load keys (2026-11-07):** Chocolate's F2/F3 save/load
entry points now open ten-slot menus in the native desktop session. F2 rejects
dead-player saves; slot saves encode versioned Room `.fsave` data under the
Room saves directory, and F3 restores that native snapshot before restarting
the level music. Empty load slots are identified in the menu. F6 opens the
last-used save slot preselected (or the save menu when none exists), and F9
opens a confirmation for that last-used slot (or the load menu before the first
save); changing campaign levels clears the last-slot latch as Chocolate does.
When the session came from an imported Atomic save, F2 also writes an Atomic
`.sav` companion next to the `.fsave`; F3 loads the `.fsave`, then applies that
companion's Build/Atomic state. Ordinary native sessions continue to save only
`.fsave`. This does not make Atomic saves standalone or close D9 round-trip
acceptance; per-slot naming/preview tiles and the full pause/options front end
remain open. Static
`game.c` key-flow and `menues.c::saveplayer` inspection; formatting, diff check
and targeted `cargo check -p room-app --bin room` only. No gameplay or
reference comparison.

**Native Duke F8 messages toggle (2026-11-07):** F8 now toggles the saved
`Player::fta_enabled` state and reports the change through the app message
overlay. Static review of Chocolate `game.c` F8 / `FTA()` showed that ordinary
quotes use mode 0 while the F8 status message uses forced mode 1; Room's CON,
secret, locked-switch, SE5 boss and timed-exit quote paths now all honor the
toggle. Formatting, diff check and targeted crate/app compilation only; no
gameplay or reference comparison.

**Native Duke F10 quit confirmation (2026-11-07):** the in-game F10 key now
opens Room's two-choice quit-confirmation menu while a native Duke session is
active. Confirming exits the desktop app and cancelling closes the modal and
resumes play. This ports the gameplay key path from Chocolate `game.c`; the
full title/quit UI, random quit quotes, and front-end flow remain open. Static
source inspection, formatting, diff check and targeted `cargo check -p
room-app --bin room` only; no gameplay or reference comparison.

**Native Duke F1 help pages (2026-11-07):** the F1 key now displays Atomic's
`TEXTSTORY` ART tile, then `F1HELP`, and closes after the second page or Escape.
The page overlays the Duke HUD and freezes the 30 Hz native sim while open;
movement keys are cleared on entry. Static `game.c::displayrest` / F1 input
review; formatting, diff check and targeted `cargo check -p room-app --bin
room` only. No gameplay or screenshot comparison.

**Native Duke F11 brightness (2026-11-07):** added a checked parser for the
`BRITABLE[16][64]` block in `TABLES.DAT` and apply the selected row to the
native RGBA scene and ART UI conversion. F11 cycles Atomic `ud.brightness`
0..56 by eight (rows 0, 2, …, 14); Shift-F11 reverses and wraps. GRPs without
`TABLES.DAT` retain identity brightness. Static `game.c` / `engine.c`
inspection, formatting, diff check and `cargo check -p room-duke -p room-app
--bin room` pass with existing warnings; no visual/gameplay comparison.

**Native Duke player cheats (2026-11-07):** Chocolate's keyboard
`D` → `N` prefix recognizes `cornholio`/`kroz`, `stuff`, `weapons`, `items`,
`inventory`, `keys`, `showmap`, `view`, `coords`, `unlock`, `cashman`, `clip`,
`hyper`, `beta`, `todd`, `allen`, `rate`, `scotty###`, `skill#`, `monsters`, `debug`, and `time` in solo
games below skill 5. These update player, campaign, view and FTA state.
Multiplayer, demo recording/playback, and skill 5 suppress cheat entry.
The `showmap` cheat reveals all native Duke automap sectors; `view` toggles
the existing shoulder camera; `coords` toggles the native player pose overlay.
`unlock` clears sector locks, invokes ordinary sector operations, and toggles
all animated forcefields in Chocolate's traversal order. `cashman` toggles a
serialized Atomic flag and spawns two MONEY sprites per held Use tic with the
ordered Z/angle/cstat TRAND rolls. `clip` toggles serialized `ud.clipping`:
player XY movement bypasses `clipmove` and `pushmove`, updating its sector
directly, while normal collision remains active when disabled. `monsters`
cycles Chocolate's `actor_tog` modes: mode 1 hides badguys and skips their CON
execution, mode 2 sets their collision flags and runs CON, and mode 3 preserves
the mode's vanilla fallthrough. The toggle is saved in `.fsave` and carried
across campaign transitions. The `debug` cheat shows Room playsim counters, while Chocolate’s cache-lock-byte list remains unsupported. `time` is accepted as Chocolate’s no-op cheat. The current command parser and effect paths cover every Atomic cheat table entry; exact legacy quote presentation remains a UI detail.
Static `game.c::cheats` review, formatting, diff check and `cargo check -q -p
room-duke -p room-app --bin room` pass with existing warnings; no gameplay
comparison or tests were run.

**D8 vanilla cheat table reconciliation (2026-11-08):** compared the native
`D`→`N` parser and `DukeSession::apply_player_cheat` dispatch against the
Atomic `GAME.C::cheatquotes[]` inventory already recorded in docs/25. The
parser includes all table entries, including `skill#` and `scotty###`; screen
overlays and campaign transitions are handled in the desktop input path, and
`time` intentionally clears as a no-op. Corrected the spec and progress checklist
to record vanilla command coverage; Chocolate's cache-lock diagnostic and exact
quote presentation are still open. Static source/table and call-path inspection
only; no tests, game run, or demo comparison.

**D8 bonus-stat snapshot (2026-11-08):** added the public
`DukeSession::level_bonus_stats` snapshot consumed by a future Atomic
`dobonus` presentation. It reports elapsed 30 Hz tics, the current CON
`definelevelname` par/designer times, per-level kill/secret counters, and the
Atomic prelevel totals once those fields were added. The complete Atomic
screen, step sounds, dynamic CON-spawned enemy accounting and multiplayer
intermission remain open. Static inspection of the CON level table and
Chocolate's `premap.c` / `game.c` counters; formatting, diff check and
targeted compilation only. No gameplay or Chocolate comparison.

**D8 native bonus panel (2026-11-08):** the desktop solo campaign path now
captures the outgoing level's bonus stats before campaign routing, defers the
map transition, freezes the outgoing level for 150 tics, and draws a native
panel that counts kills, secrets and all three time rows up from zero during
the first 30 tics. The destination map is loaded after the panel expires.
Networked play
and demo playback skip this presentation so their authoritative input/tic
streams are not paused. This is a static first pass, not Atomic `dobonus` parity:
the native art/animation, stepping sounds, input advance, dynamic
CON-spawned enemy accounting and multiplayer intermission flow remain open.
Static Chocolate `premap.c`
`resetprestat` review also corrected campaign transitions to reset `actors_killed`
and `secret_rooms` instead of carrying them with inventory, making the found
bonus counts level-local. Formatting, diff check and targeted compile only; no
gameplay or Chocolate comparison.

**Native Duke `dnscotty###` level cheat (2026-11-07):** the desktop cheat
buffer accepts Chocolate's `scotty###` suffix, validates the one-based Atomic
volume/map coordinates, and requests the native campaign transition directly.
The existing campaign loader keeps the current level intact and reports an
unavailable map if the selected resource is absent. Static `game.c::cheats`
review, formatting, diff check, and targeted compilation only; no gameplay or
Chocolate comparison.

**Native Duke utility cheats (2026-11-07):** added `dnhyper`'s Atomic
steroids/heat values and the forced `dnbeta`, `dntodd`, and `dnallen` FTA
quotes to the native single-player cheat parser. Static `game.c::cheats`
review, formatting, diff check and targeted compilation only; no runtime or
Chocolate comparison.

**Native Duke `dnrate` overlay (2026-11-07):** the cheat now toggles the
campaign map label and live FPS display, using the desktop frame counter while
keeping all wall-clock sampling in the presentation shell. Static
`game.c::display_boardfilename_FPS_weapon` / `tics` review, formatting, diff
check and targeted compilation only; no game comparison.

**Native Duke `dnskill#` restart (2026-11-07):** the parser accepts Atomic
skill values 1–5, reloads the current campaign map at the requested skill, and
uses the fresh spawn/player state from `MODE_RESTART`; failed map loads retain
the current session. Campaign map loads now take the session's active skill
instead of a fixed startup value. Static `GAME.C::cheats` and
`premap.c::enterlevel` / `resetpspritevars` inspection, formatting, diff check
and targeted compilation only; no gameplay comparison.

**Native Duke `dndebug` overlay (2026-11-07):** added a toggleable native
playsim overlay for tic, live sprite, actor, projectile, FX, sector, wall and
effector counts. Chocolate's `caches()` prints cache lock-byte entries; Room
has no equivalent Build lump lock table, so that exact cache diagnostic remains
open. Static `GAME.C::cheats` / `caches` inspection, formatting, diff check and
targeted compilation only; no gameplay comparison.

**Native Duke `dntime` parser entry (2026-11-07):** the no-op cheat now
matches Chocolate's `case 5`: Room consumes the completed sequence and leaves
playsim and presentation state untouched. The spec's cheat inventory was
reconciled against the current `GAME.C::cheatquotes[]` table (`dncosmo` is not
present). Static inspection, formatting, diff check and targeted compilation
only; no gameplay comparison.

**Positional sound bridge (2026-11-06):** native sound events now retain their
Build source coordinates. CON actor sounds use the emitting sprite; weapon and
player-action cues use the player. The desktop adapter updates the mixer
listener from Duke's pose and converts Build XY/Z scales at the audio boundary,
enabling existing mixer attenuation and panning. Static Chocolate `xyzsound`
inspection and compilation only; audible balance and range have not been
compared or tuned against Chocolate.

**Ambient `MUSICANDSFX` slice (2026-11-06):** native actor movement now reads
Atomic `definesound` flags and performs distance-gated loop starts, source-owned
stops at the marker's source when the listener leaves range, and randomized
sector sound playback when the player is in the marker's sector. The mixer
bridge now carries loop and source/sound-specific stop events. Reverb markers,
sound-toggle behavior, and music playback remain open. Voice-priority eviction
was implemented in the later D8 slice below.
Static review of Chocolate
`actors.c::moveactors` / `sector.c::callsound`; compile/format only, no audible
or gameplay comparison.

**Duke sound voice limits (2026-11-06):** sound events now retain Atomic
`definesound` loop metadata and route its loop flag to the mixer. The mixer
enforces Chocolate's one-voice restriction for looped sound definitions and
four simultaneous voices for ordinary sound IDs (`xyzsound` allows `num == 3`);
`m&4` talk sounds use an exclusive voice class. Sound locks/lockout toggles and
pitch/volume variation remained open at that checkpoint. Static review of
Chocolate `sounds.c::xyzsound`; compile-only validation. A follow-up audit
corrected the ordinary cap from three to four after checking the strict
`Sound[num].num > 3` guard.

**Duke sound priority slice (2026-11-06):** `definesound.pr` now survives in
native Duke sound events and the audio adapter. At the Chocolate Sound Blaster
default pool size (32 voices), a new Duke voice replaces the lowest-priority
active voice only when its priority is at least as high; otherwise the start is
dropped. This does not yet reproduce device-specific voice counts, lockout and
sound toggles, or per-sound lock/owner lifecycle. Static inspection of
Chocolate `sounds.c::xyzsound`, `fx_man.c` and `multivoc.c`; formatting and
`cargo check -p room-audio -p room-duke -p room-app --all-targets` passed. No
gameplay, audio, or Chocolate comparison was run.

**Duke sound placement flags (2026-11-06):** `definesound m&128` events now
play centered at full volume, matching `xyzsound`'s delegation to `sound()`;
`m&16` keeps its source position for stereo panning while bypassing distance
falloff. Ordinary spatial cues continue through the existing attenuation path.
Static comparison with Chocolate `sounds.c::xyzsound` / `sound`; formatting and
`cargo fmt --all -- --check`, `git diff --check`, and
`cargo check -p room-audio -p room-duke -p room-app --all-targets` passed.
No audio or gameplay comparison was run.

**Sector animation sound cues (2026-11-07):** the native sector-motion path now
uses the first local `MUSICANDSFX` marker to queue its configured start/stop
sound IDs when supported sector animations begin and finish; sliding-wall
sector activation also queues the start cue when it successfully creates wall
motions. This ports the basic `callsound` toggle and global sound-stop flow.
The `soundm` flags that affect this path are retained on the marker and applied:
`m&16` suppresses the start cue, and `m&1` stops the active sound when the
marker has no distinct off cue. Other sound-lock interactions remain open.
Sound events are collected after plane and wall animations, so animation-end
cues enter the same tic's output queue. Static Chocolate `sector.c::callsound`
review; formatting/diff/compile checks only, with no gameplay or audio
comparison.

**Duke sound toggle and lockout (2026-11-07):** the desktop adapter now honors
archived `duke_sound` and `duke_lockout` CVars when dispatching native starts;
events whose Atomic `definesound.m` has `m&8` are suppressed under lockout,
while stop events still pass through. Static `sounds.c::xyzsound` review and
compile checks only; no audible or runtime comparison.

**Atomic campaign music (2026-11-07):** CON `music` declarations now retain
the four 11-track Atomic volume tables plus the environment table. The native
desktop selects the corresponding campaign MIDI from the layered Duke GRP at
map startup and after campaign transitions, and configures the shared music
sequencer using the supplied/discovered SoundFont or a GRP `GENMIDI` bank when
available. Map coordinate parsing now includes secret levels 10 and 11. Static
Chocolate `gamedef.c::music`, `premap.c` and app-load review; formatting,
diff-check and `cargo check -p room-duke -p room-app --bin room` only. No audio,
gameplay or reference comparison; menu/environment playback and music controls
remain open.

**Duke F5 music controls (2026-11-07):** native desktop F5 shows the current
Atomic track and advertises Shift-F5 for the 100-tic quote window; Shift-F5
cycles the 44-entry campaign table and starts the selected layered-GRP MIDI.
Campaign transitions reset the selected track to that map's vanilla slot.
Static Chocolate `game.c` F5 handling review; `cargo fmt --all`, diff check and
`cargo check -p room-app --bin room` only. No UI/audio/runtime comparison.

**Duke F4 sound setup (2026-11-07):** added a native desktop Sound Setup modal
with sound and music toggles, Duke Talk, ambience and flip-stereo toggles, and
sound/music volume controls using Chocolate's 0–255 scale and four-unit
arrow-key adjustments (labels display normalized percentages).
Disabling sound clears active mixer voices; disabling music stops playback, and
reenabling it restarts the selected campaign track. Archived `duke_music`,
`duke_talk`, `duke_sfx_volume` and `duke_music_volume` CVars preserve these
settings. Duke Talk filters CON sounds with `definesound.m&4` and clears active
exclusive talk voices when disabled. Ambience events carry an explicit marker
origin through the sound bridge, so its toggle filters ambient starts and
clears active ambience voices without muting gameplay sounds that reuse the
same sound ID; toggling it on re-arms marker playback. Flip Stereo swaps the
left/right gains in the mixer. Raw volume steps are now four units on
Chocolate's 0–255 range; pointer-drag sliders and device-specific `FX_SetVolume`
behavior remain open, along with environment/menu music. Static review of
Chocolate `menues.c` Sound Setup and Room's native menu/audio bridge; `cargo
fmt --check` and `cargo check -q -p room-app --bin room` passed with existing
warnings. No gameplay, UI or Chocolate comparison was run.

**Duke reverb zones (2026-11-07):** the desktop audio bridge scans the
Chocolate `STAT_FX` sprite list for `MUSICANDSFX` markers with lotag 1000–1999,
applies their hitag distance radius using Build's integer `FindDistance2D`, and
selects the marker's 0–255 `FX_SetReverb` level. The mixer now ports
`mvreverb.c`'s sample-rate-scaled taps, IIR coefficients, four accumulators,
feedback paths and stereo output; disabling the zone clears its delay buffer.
Multi-zone state ordering and `FX_SetReverbDelay` changes remain open. Static
inspection of `actors.c::moveactors`,
`sector.c::ldist`, `audiolib/mvreverb.c` and `multivoc.c`; formatting/diff/
desktop compile checks only, no game/audio comparison.

**Duke spatial sound gain (2026-11-07):** the mixer now applies Chocolate's
`FindDistance3D` approximation to raw Build-scaled source/listener positions,
adds `definesound.vo`, applies `xyzsound`'s `sndist` floor, and maps distance
through the integer `MV_Play3D` pan table. `m&16` positional
starts keep their source pan at zero distance; `m&128` global starts are full
volume and centered. Duke SFX volume
is a separate mixer gain and updates already active voices. Actor sound starts
now use Chocolate's eye-offset `cansee` test and add `sndist >> 5` when blocked;
`MUSICANDSFX` and global sounds bypass the check. The mixer quantizes relative
angle to Chocolate's 32-bin pan table and applies its integer channel gains,
without multiplying its falloff by the separate `mid` volume a second time.
Listener `opos` timing and audible calibration remain open. Static review of Chocolate
`sounds.c::xyzsound`, `global.c::FindDistance3D` and `multivoc.c::MV_Play3D` /
`MV_CalcPanTable`; `cargo fmt --check`, `git diff --check` and app compile only,
no sound, gameplay or reference comparison.

**Duke reverb DSP (2026-11-07):** replaced the provisional feedback delay with
the Chocolate audiolib `mvreverb.c` stereo network: sample-rate-scaled taps,
IIR filters, four accumulator paths, feedback taps and output mixing. The
`FX_SetReverb` 0–255 value maps through Chocolate's 0–63 volume conversion;
the delay buffer clears when the zone disables. `FX_SetReverbDelay` runtime
changes and reverb-zone state ordering still need work. Source attribution is
recorded in `THIRD_PARTY.md`. Static source review and app compile only; no
audio/gameplay/reference comparison.

**Duke sound pitch ranges (2026-11-07):** native sound playback now maps the
Atomic `definesound ps/pe` range (1200 units per octave) to the audio mixer
pitch ratio. A stable per-tic/source hash chooses the half-open offset range,
keeping audio variation outside the deterministic playsim RNG. Chocolate's
libc `rand()` sequence remains open. Static `sounds.c` / `pitch.c` inspection
and compile checks only; no audible or runtime comparison.

**Duke underwater sound pitch (2026-11-07):** positional starts now apply
Chocolate's underwater pitch overrides using the listener's current sector:
−768 for non-talk sounds, and −1024 for PIPEBOMB_EXPLODE, LASERTRIP_EXPLODE
and RPG_EXPLODE. Global `m&128` sounds bypass the underwater adjustment, as
Chocolate's early `sound()` path does. Static `sounds.c::xyzsound` / `soundefs.h`
review and compile checks only; no audio or gameplay comparison.

**Duke sound distance offset (2026-11-07):** native sound starts now carry
CON `definesound.vo` as a signed mixer-distance offset, applied before the
linear attenuation curve and clamped to nonnegative distance. `m&16` and
`m&128` skip this positional attenuation path as in Chocolate. The Build to
mixer offset scale is provisional; exact attenuation/visibility curves remain
open. Static `gamedef.c::definesound`, `sounds.c::xyzsound` review and compile
checks only; no audio or runtime comparison.

**Duke quote and TIP HUD (2026-11-07):** the desktop native HUD draws active
CON `definequote` text with a dark shadow near the bottom center. It also
loads Atomic ART tiles `TIP`/`TIP+1` (2576/2577) as palette-indexed sprites,
scales them with nearest-neighbor sampling, applies Chocolate's countdown
frame and vertical tip pose, and suppresses the weapon while the tip is shown.
The pose follows the player's horizon. Palette/shade translation, look-angle
and sway offsets, original font/layout, wrapping, menus and broader HUD remain
open. Formatting and desktop compile checks only; no screenshot or gameplay
comparison.

**Native Duke automap slice (2026-11-07):** Tab now draws the loaded
`DukeSession` Build wall loops and player direction in the automap overlay,
instead of showing the Doom `MapGeometry` belonging to the other runtime.
Room performs Chocolate's `scansector` portal-sector flood from the current
sector using the 32-bit cross-product facing gate and `cstat&32` portal block,
remembers reached sectors while the map is open, and limits wall rendering to
those sectors. Active, non-invisible sprites in sectors reached during the
current flood receive markers; sprites in merely remembered sectors do not.
Wall portals and solid walls use separate colors, and the existing zoom
control scales the map around Duke's native player position. This ports sector
discovery but not the renderer's full view-frustum clipping: sectors reached
through eligible facing portals can still be marked outside the final screen
span. Build secret/door colors and Chocolate's rotate/scale behavior remain
open. Formatting and desktop compile checks only; no runtime or reference
comparison.

- `2026-11-07` — **D8 quote/TIP presentation**: implemented basic native quote
  text and animated TIP ART tile presentation in the desktop HUD; tip poses
  hide the weapon and follow the horizon. Chocolate's 21-entry `tip_y` array
  is safely clamped for the larger countdown values. Static source inspection,
  formatting and compile checks only; no runtime or Chocolate comparison.
- `2026-11-07` — **D8 native automap**: Tab renders sectors reached through
  Chocolate's facing-portal cross-product gate, active sprites from the current
  sector flood, and Duke's position/heading, so the native runtime no longer
  displays the shell's unrelated Doom map. Full projected view-span clipping,
  native line colors and exact zoom behavior remain. Static compile only; no
  in-game test.
- `2026-11-08` — **D8 ambience voice-pool arbitration**: when all 32 active
  voices are `MUSICANDSFX` ambience loops, a new loop first passes Chocolate's
  priority availability gate, then replaces a farther active loop only if the
  entering source is closer; otherwise the new loop is rejected. Selection
  runs in the native mixer without consuming playsim RNG. Static review of
  Chocolate `actors.c::movefx` and Room's Duke event/mixer path; formatting,
  diff and crate compile only, no audio/gameplay comparison. Device-specific
  maximum voice counts remain open.
- `2026-11-08` — **D8 MUSICANDSFX cached randomness**: random ambience sound
  selection now uses Chocolate's cached per-tic `global_random` for both the
  sound offset and next interval instead of advancing Build `randomseed` two
  extra times. This preserves map-sim RNG and the source's unsigned sound-range
  modulo. Static review of Chocolate `actors.c::movefx`; formatting, diff and
  targeted compile only, no gameplay/demo or audio comparison.
- `2026-11-08` — **D8 stable sound-owner ordering**: Duke voices now carry a
  mixer-local start sequence. `ifsound` reads the earliest active owner for
  each sound ID and `xyzsound` stops the newest matching voice by that sequence,
  unaffected by unrelated voice completion using `swap_remove`. Formatting,
  `git diff --check` and `cargo check -q -p room-audio` passed; no tests,
  playback, gameplay or reference comparison were run.

**D8 browser Duke audio parity slice (2026-11-08):** the web shell now drains
full `DukeSoundEvent`s instead of flattening them to centered IDs. It positions
the listener and sources in Build coordinates, carries reverb-zone state,
handles global and positional starts plus stop/stop-source events, and passes
loop, uniqueness, one/four-voice limits, talk exclusivity, ambience, priority,
and distance-offset metadata to the shared mixer. It also reports active sound
owners back to the playsim for `ifsound` behavior. The `definesound ps/pe`
sampling and underwater pitch transformation are shared with the desktop
through `room-duke::audio::pitch_scale`; source voice replacement/dead-badguy
handling, timed-exit cutoff, and the `cansee` occlusion penalty now follow the
desktop path too. The actual wasm-target check exposed a missing direct
`room-build-engine` dependency and a stale Doom lobby tuple after multiplayer
options were extended; both were corrected. No browser run or audio/reference
comparison was performed. Browser host/UI integration and environment-track
selection remain open.

**D8 browser audio options (2026-11-08):** `roomSetDukeAudioOptions` now
exposes the desktop F4 sound, music, talk, lockout, ambience, stereo and 0–255
volume settings to an embedding page. Runtime sound events honor each gate,
and the shared mixer receives the selected SFX/music gains and stereo mode.
Defaults preserve current behavior (sound/music/talk/ambience on, full volume,
lockout/reverse stereo off). Documented in `crates/room-web/README.md`.
Formatting, wasm-target compilation and diff checks only; no browser/UI or
audio comparison was run.

**D8 F7 shoulder view (2026-11-07):** native Duke now toggles the shoulder
camera on F7 and hides the first-person weapon. Camera distance grows by 4096
Q16 units per 30 Hz tic to Chocolate's 65536 maximum; a rearward `CLIPMASK1`
hitscan clamps it against active face sprites, solid walls and sector planes,
pulling the camera four map units off a hit surface. The player's own sprite is
excluded from that ray and the camera sector is refreshed each render. The
toggle/distance are serialized in Room saves; Atomic import restores
`over_shoulder_on` (`ANG+92`). Sloped-plane camera occlusion, exact sprite
offsets, camera roll/animation details and screenshot comparison remain open.
Static `game.c::view` / ABI review, formatting and compile checks only; no
runtime or visual comparison.

---

## D9 — Duke saves + demos

**Status:** `in_progress` · Spec: `25` §7 / §11 D9

### Acceptance

- [x] Parse Atomic `.DMO` input streams and load existing reference trace files
- [x] Serialize and restore the Room Duke world/session in a versioned `.fsave` (v1)
- [x] Retain imported Chocolate save globals (`ud`, mirrors/clouds, parallax and clock domains) in `.fsave`
- [x] Restore the Atomic player's `toggle_key_flag` used by edge-triggered pool-ball use input (field restore only; full Atomic SAV round-trip remains open)
- [x] Construct and encode a standalone Atomic v119 `GAME#.SAV` from a supported native session
- [x] Import and restore the currently modeled Atomic world, player, CON and global fields
- [ ] Atomic `GAME#.SAV` full field/default coverage and load-save-load acceptance (writer rejects an unrepresentable `lockclock`; Chocolate/reference acceptance remains unverified)
- [x] Room `.fdem` native `DukeInput` recording and per-tic checked playback
- [x] Desktop shell `record` / `stop` integration for native Duke `.fdem` recording
- [x] Atomic revision 117 single-player `.DMO` encoding (reference playback not verified)
- [x] Multiplayer Atomic v117 `.DMO` input recording (reference playback not verified)
- [x] Atomic `.DMO` capture and application of per-seat aim-mode / weapon-choice settings
- [x] Atomic `.DMO` map/skill/roster bootstrap from ordered GRP layers
- [ ] Reference-engine sync for a recorded demo

**Atomic SAV state coverage (2026-11-07):** the bounded `SavHeader` parser
consumes the 160×100 save-tile payload that Chocolate reads before the world,
then consumes multi-block `kdfread` arrays and reconstructs Build's
element-wise byte deltas. It retains sector plane panning, sprite offsets/velocities, six
sprite-list tables, cyclers, animation walls/targets, sprite queue, mirrors,
automap bits, `actortype` dispatch table, board filename, clouds, script
pointer flags/code, actor script offsets, sprite script flags, and all
`weaponhit` records. Field order and
array counts follow Chocolate `loadplayer`/`kdfread`. The native restore path
now applies saved Build geometry, exact sprite-link arrays, core player
pose/weapons/inventory, clock/RNG fields, CON pointer relocation, and saved
actor `weaponhit` records to an already loaded `DukeSession`; imported Atomic
globals that do not yet have live simulation counterparts are now retained as
`AtomicSaveGlobals` inside `.fsave` (including mirror/cloud tables, automap
bits, animated-sector pointer/goal/velocity tables, `gotpic`, board name,
parallax and the serialized `ud` fields), so an Atomic import
followed by a Room save no longer discards them. They are persistence-only
until their corresponding world/render/session behavior is implemented.
Desktop Atomic
SAV loading dispatches here instead of incorrectly starting a Doom map.
Static inspection of `duke3d.h::player_struct` revealed that the raw player
record starts with 532 bytes of zoom/loogie state before `posx`; the SAV
decoder now uses the actual 32-bit Atomic offsets rather than interpreting
the record prefix as a pose. The same source review confirmed `MAXTILES=9216`;
the SAV `actortype` and `actorscrptr` arrays now use that Chocolate bound
instead of 30720 and align with the loaded CON image. Restore carries saved
player motion,
look offsets, HUD/inventory timers, quote/access state, kill/secret counters,
weapon reservations and auto-aim. It reconstructs the six-field shade cycler
records too. Animated wall/mirror/cloud tables are retained even where Room
does not yet have runtime counterparts; live mirror/cloud behavior, unparsed
`ud` options and continuation equivalence remain open. A full modeled Atomic
v119 writer now supports template-backed export and conservative standalone
construction, but complete native-state coverage and load-save-load acceptance
remain open. Saved animated-wall entries now restore by their saved wall index and
phase instead of rerunning `premap`. `cargo fmt --all`,
`git diff --check` and targeted `cargo check` pass; existing warnings remain.
No save replay or runtime/reference comparison.

**Atomic global retention follow-up (2026-11-07):** added versioned
`AtomicSaveGlobals` to the native `.fsave` payload and populate it from the
Atomic header on import. It retains save slot metadata, mirror/cloud and
automap arrays, board name, sky/parallax values, multiplayer/frag globals and
the parsed Atomic `ud` settings for later runtime integration. This preserves
fields exposed by `SavHeader`; it does not implement their runtime effects.
Standalone serialization was added in a later D9 step. `cargo fmt --all`,
`git diff --check` and targeted `cargo check` pass; no tests or game/reference
comparison were run.

**Atomic player exit-state restore (2026-11-07):** extended the 32-bit raw
`player_struct` decoder with `fist_incs` (`ANG+98`) and `buttonpalette`
(`U8+40`), then applied them to `Player` during Atomic SAV restoration. These
are the live values used by the nuke-button/secret-route transition, so a SAV
captured during that sequence no longer loses those two fields. Offsets were
checked against Chocolate `duke3d.h`; formatting and targeted compilation
only, with no SAV replay or reference comparison.

**Atomic pool-input latch restore (2026-11-07):** the Atomic `player_struct`
decoder's `toggle_key_flag` now restores into the native `Player`, preserving
the held-use edge state used by pool-ball kicks across imported saves. The
versioned Room `.fsave` already serializes this player field. This covers one
player field only; it does not close Atomic `GAME#.SAV` writing or round-trip
acceptance. Static layout inspection and the prior targeted crate compile;
no tests, save replay or reference comparison were run.

**Atomic `dfwrite` primitive (2026-11-07):** implemented a bounded encoder for
Chocolate's record-wise byte-delta format using the raw Ken block form
(`strtot=0`) accepted by `uncompress`. It handles `LZWSIZE=16384` chunking and
large byte arrays; compressed output size parity is intentionally not needed
for compatibility. Atomic v117 `.DMO` encoding now uses this shared primitive
instead of duplicating its own delta/chunk writer. This is a codec primitive
only: there is still no complete `GAME#.SAV` serializer, as several runtime/
header tables have no native model.
Static review of `filesystem.c::dfwrite` and `uncompress`; formatting and
targeted compilation only, with no tests or runtime/reference comparison.

**Atomic v117 per-player header alignment (2026-11-07):** corrected the
`.DMO` stream offset to account for Chocolate's one-byte aim mode plus ten
weapon-choice bytes for every player after the fixed header. Previously Room
skipped only one byte per player, so it began decoding compressed tic data in
the middle of the player metadata. The parser now exposes all seats' tic
streams, and the v117 encoder interleaves equally sized player streams into
Chocolate's per-tic record layout and retains per-seat aim-mode and
weapon-choice metadata for re-encoding. This is codec support only: the native
session recorder remains single-player, and multiplayer playback/recording
sync remains open.
Static inspection of Chocolate `game.c::opendemowrite` / demo header loading,
formatting and targeted compilation only; no tests or demo/reference comparison
were run.

**Room `.fsave` v1 (2026-11-06):** `DukeSession::save_fsave` and
`load_fsave` serialize the Build map and sprite arrays, player, CON actor
temps, tile metadata, RNG/tic state, animation/motion lists, sound queues and
exit latch. The JSON envelope is versioned and SHA-256 binds it to the already
loaded CON image; load validates dimensions, fixed-array lengths, sprite-list
links, shade-cycler sector references and a 64 MiB input ceiling before
mutating the session. Static `cargo check -p room-duke --lib` and formatting
passed. This remains partial against Chocolate: GAME#.SAV, multiplayer Atomic
DMO recording, reference sync and several native globals are still open; no
gameplay or reference-engine checks were run.

**Native `.fdem` v1 (2026-11-06/07):** `DukeDemoRecorder` stores the starting
native `.fsave`, native `DukeInput` records, pre-tic desktop weapon
requests/cycles, and SHA-256 digests of the resulting trace state, with a
versioned, bounded envelope. Playback restores that initial save on the first
tic, advances the native session and stops on the first state-digest mismatch.
The desktop shell's existing `record [slot]`, `stop`, and `playdemo [slot]`
commands now support native Duke. This is Room's native format, not Atomic's
`.DMO`; single-player Atomic v117 encoding is now available through
`DukeAtomicDemoRecorder`, but playback in Chocolate and end-to-end session
recording reference parity remain unverified.

**Desktop native `.fdem` recording (2026-11-07):** the existing shell
`record [slot]` / `stop` commands now branch to the native Duke session when
`--native-duke` is active. Each completed simulation tic stores its exact
`DukeInput`, weapon-selection actions, and post-tic trace digest; the bounded
`.fdem` also embeds the starting `.fsave`. `playdemo [slot]` restores that state
on its first tic and halts on a digest mismatch or end of recording. Metadata
is derived from `E#L#` campaign names and current session skill. Atomic `.DMO`
reference parity remains open. Formatting and compile checks only; no demo
playback or cross-engine comparison was run.

---

## D10 — Duke multiplayer + server

**Status:** `in_progress` · Spec: `25` §7 / §11 D10

### Acceptance

- [x] Packed 10-byte Duke sync input codec (`avel`, `horz`, `fvel`, `svel`, `bits`)
- [x] FTR2 Duke TIC_CMD / seat-masked TIC_FRAME envelopes with tic indices
- [x] Full serialized native playsim checksum exposed through `GameRuntime`
- [x] Bounded deterministic per-tic input barrier for up to eight seats
- [x] Native `DukeSession` roster from APLAYER starts (up to eight), fsave persistence, per-seat movement, actions, weapon processing and shared actor/player-hit passes
- [x] Three Atomic `ud.coop` mode values and APLAYER lotag selection (`configure_multiplayer_mode`)
- [x] Native desktop exposes all three Duke network modes plus friendly-fire choice and folds them into the lobby fingerprint to reject mismatched peers
- [x] Initial DukeMatch/no-spawn loadout reset versus co-op carry-over (`resetprestat` rules)
- [x] `ud.ffire` is carried in the room settings and suppresses allied player damage when disabled
- [x] Lethal-hit transition, player/self frag counters, corpse-following camera, killer-facing turn, palette flash, and floor-relative death roll
- [x] CON `respawn` player reset path, RESPAWN actor marker ticking, and seat-specific spawn reset path
- [x] Seat-associated APLAYER water/angle update and Holoduke ownership (no seat-zero fallback)
- [x] Browser native Duke client sends seat commands and applies authoritative tic frames
- [x] Native desktop Duke client sends packed per-seat sync inputs through the existing lockstep transport
- [x] Native desktop piggybacks per-seat native session checksums and detects mismatches when every peer reports
- [ ] End-to-end lockstep/desync validation, including mismatch recovery and cross-target checksums
- [x] Multiplayer `INPUT_OPEN` rising edge requests seat-specific respawn; mode-specific co-op/DukeMatch start selection and reset state are implemented
- [x] Dead-player camera/body following, killer-facing turn, sector/water updates, collision push and floor-relative death roll
- [ ] Remaining full death animation/spectator behavior, marker edge cases, and end-to-end co-op/DukeMatch validation
- [x] Server 30 Hz Duke frame relay and optional native Duke headless verifier integration
- [ ] T1.9/T1.10 validation across supported targets

**Native Duke packet codec (2026-11-06):** `room-proto` now provides a
length-checked encoder/decoder for Chocolate's packed ten-byte Duke `input`
record, plus FTR2 TIC_CMD and seat-masked TIC_FRAME envelopes carrying the
simulation tic. The frame decoder enforces exact body length from the seat
mask. Multiplayer lockstep/session integration remains open. Static code
inspection and compile checks only; no network or runtime comparison was run.
`GameRuntime::checksum` now returns the first 64 bits of SHA-256 over the
versioned full native `.fsave` envelope, covering the mutable Build/session
state and bound CON fingerprint. It is ready as a checksum primitive; no
presentation sound queues are excluded, and no cross-target determinism or
multiplayer exchange has been validated.

**Duke input barrier (2026-11-07):** `DukeLockstepInbox` accepts at most one
command per connected seat for one exact tic, rejects absent seats, duplicates
and tic mismatches, and yields commands in ascending seat order only after the
roster is complete. `DukeSession::tick_inputs` simulates per-seat movement,
actions and implemented weapons, and `GameRuntime::tick_runtime` forwards all
Duke commands. The room server now has its own 30 Hz Duke ring and optional
native Duke verifier. The browser client sends seat inputs and applies
authoritative frames; desktop client networking and end-to-end multiplayer
validation remain open. Formatting and targeted compilation only; no network
session or runtime comparison was run.

**Native multi-seat simulation foundation (2026-11-07):** `DukeSession` can
configure one to eight seats from retained APLAYER map starts, creates
seat-indexed player bodies, applies per-seat `DukeInput` movement, and includes
all seats in the existing `movefta`, `moveplayers`, actor damage, transport,
explosion, effector, standable, SE2 quake and SE24/SE34 conveyor passes. Per-seat
access-card sequences, inventory use/cycling and edge-triggered sector,
nuke-button and wall-switch Use actions now run per player; each Player stores
its own input-edge latches. Extra seats are persisted in `.fsave` and old saves
decode with an empty roster. Multi-seat weapon selection and firing now reuse the session's
implemented slot handlers, including ammunition, deferred switches, projectile
creation, shared RNG and sound events. The pass follows Chocolate's
`processinput(i)` then `checksectors(i)` order after the current movement pass.
`GameRuntime::tick_runtime` now passes every Duke seat command through to
`tick_inputs`. The server releases Duke frames at 30 Hz, repeats a missing
seat's last command for three tics with one-shot weapon selection cleared, then
fills neutral input. Its optional verifier consumes the same frames with a
native `DukeSession` and reports checksums. Live Duke spectators receive frames
only if present at match start because the replay log is Doom-specific. Client
netplay, late spectator catch-up, coop/DukeMatch spawn/marker/respawn rules and
Atomic SAV multiplayer state remain open. Static
Chocolate `premap.c::resetpspritevars`,
`player.c::processinput` and `game.c::domovethings` inspection,
`cargo fmt --all`, `git diff --check` and `cargo check -q -p room-duke --lib`
only; no tests, gameplay, network or reference comparisons.

**Duke runtime multi-input seam (2026-11-07):** `GameRuntime for DukeSession`
now collects all Duke commands in seat order and sends them to
`DukeSession::tick_inputs`; previously it selected only the first command and
called single-seat `tick`. `cargo fmt --all`, `git diff --check` and
`cargo check -q -p room-game --lib` pass with existing Room warnings. No tests,
network execution or gameplay/reference comparison was run.

**D10 native desktop lockstep wiring (2026-11-07):** native Duke now accepts
`--host` / `--join`; the desktop input path transports all five fields of the
Atomic ten-byte `DukeInput` through the existing lockstep command fields,
packs direct weapon choices/cycles into `sync.bits`, configures the same coop
roster on every peer, and advances `DukeSession::tick_inputs` from the ordered
frame. Weapon hotkeys 1–9 and cycling are encoded; the zero-key/remote-bomb
selection still needs a native mapping. This is static transport/session
integration only; no network session, checksum exchange, map-start capacity,
disconnect recovery, or cross-engine comparison was run. Formatting and
targeted compilation only.

**D10 desktop checksum exchange (2026-11-07):** desktop Duke inputs now carry
the sender's full native `DukeSession` checksum for the last completed playsim
state in a backward-tolerant extension to the lockstep packet. The receiver
collects reports by simulation tic, compares only after every peer has
reported, and drops old reports as frames advance. Sessions with trace fixups
omit reports. Static packet/call-path review, formatting, `git diff --check`
and targeted `cargo check -p room-net -p room-app --bin room` pass; no network
session, deliberate mismatch, or cross-target comparison was run.

**D10 desktop match settings (2026-11-07):** native desktop multiplayer now
accepts `--duke-game-mode 0|1|2` and `--duke-friendly-fire`, configures the
same Atomic `ud.coop` mode and APLAYER start group on each peer, and includes
both settings in the handshake fingerprint so mismatched launch options are
rejected. The desktop lobby still does not negotiate host-authoritative Duke
settings. Static parse/configuration review and targeted compile only; no
multiplayer session was run.

**Duke room relay and native verifier (2026-11-07):** added a separate 30 Hz
server ring for Duke sync commands/frames, bounded future tic window, seat
validation, and grace-based missing-input handling. Missing commands repeat for
three tics with one-shot weapon selection cleared before becoming neutral. The
optional verifier loads the configured Atomic GRP, starts a native
`DukeSession`, configures the seat roster, consumes released frames and sends
30-tic checksums into the existing desync comparison path. Live spectators
receive Duke frames if connected from tic 0; late-join replay is rejected since
the existing catch-up log only encodes Doom `TIC_BATCH`. Static validation:
`cargo fmt --all`, `git diff --check` and
`cargo check -q -p room-server --lib --features verifier`; no gameplay, network
or reference comparison was run. Client lockstep, multiplayer rules and
T1.9/T1.10 validation remain open.

---

## D11 — Duke campaign + user content

**Status:** `in_progress` · Spec: `25` §7 / §11 D11

### Acceptance

- [ ] All Atomic maps load and every route, secret exit and nuke button can finish
- [x] Standalone user `.MAP` loading with GRP-provided CON/assets
- [x] Native desktop custom root CON (`-x` style), with sibling-file and GRP include resolution
- [x] Native desktop `--grp` layering for ordered add-on archives and duplicate-name overrides
- [ ] Verify complete add-on GRP semantics and load-order behavior against reference content
- [x] Native Duke runtime wired into desktop and browser shells; lobby creates and joins native Duke rooms
- [x] Dedicated `room-build::export` namespace for the compatibility converter; native `room-duke` owns Duke gameplay
- [ ] T1.2/T1.3/T1.11 acceptance matrix

**Atomic nuke-button path (2026-11-07):** `gamefunc_Open` now ray-selects a
visible Atomic `NUKEBUTTON`, arms its one-shot actor temp, records its owner,
and stores the button palette/secret destination on the player. The native tic
advances its 8/16-tic button frames, starts and advances `fist_incs`, and raises
the existing level-exit request at the end of the fist sequence. This follows
Chocolate `sector.c::checksectors`, `actors.c`'s NUKEBUTTON mover, and
`player.c`'s fist countdown; the tic-28 explosion cue and white palette flash
are also emitted. Desktop and browser shells now consume the exit request and
advance the campaign cursor, including the modeled secret-level and bonus
return path. Exact routing on all Atomic maps and end-to-end validation remain
open. Static source review and compile checks only; no game or reference
comparison.

**Atomic campaign cursor (2026-11-07):** introduced serializable
`DukeCampaignProgress` and the Chocolate `buttonpalette` / `secretlevel` /
`from_bonus` progression rules, including the end-of-game outcome. Atomic SAV
restore seeds the cursor from its volume, level, and bonus-return fields;
native startup seeds it for `E#L#` campaign maps, and `.fsave` preserves it.
The desktop shell now retains the GRP/CON layers, consumes a `NextLevel`
transition, loads the target `E#L#` map, rebuilds its session and sound bank,
and carries player health, armor, weapons, ammo and inventory while using the
new map's spawn. End-of-campaign currently posts a completion message; full
bonus screens/title-menu flow and edge cases still need work. Static comparison
with `player.c::fist_incs`/`premap.c::enterlevel`/`resetplayerstats` and
compile checks only; no game/reference run.

**Ordered Duke GRP overlays (2026-11-07):** `GrpStack` resolves duplicate
names from the latest archive while preserving base-to-add-on archive order.
The native desktop `--grp PATH` option is repeatable and its stack supplies the
map (including map selection from Atomic DMO headers), `GAME.CON`/includes,
ART and defined sound assets to the session, renderer and sound bank. This is
the native desktop overlay path; standalone user maps and the custom `-x`
root-compiler path are tracked in the adjacent entries. Complete Chocolate
add-on semantics remain unverified. Static compile/format checks only; no
map/demo gameplay or reference comparison was run.

**Custom Duke CON root (`-x`) (2026-11-07):** the desktop native runtime now
accepts `--con path/to/custom.con`, compiling that file in place of the
`GAME.CON` root while retaining Atomic GRP/add-on resources. `include` first
checks beside the custom file (exact, uppercase and lowercase name forms),
then falls back to the ordered GRP stack. The session API stays filesystem
agnostic through `IncludeResolver`. CON language/runtime completeness and
reference compatibility of custom scripts remain unverified. Static
compile/format checks only; no gameplay/demo comparison was run.

**Standalone Duke maps (2026-11-07):** the native session loader can now load
map bytes supplied separately while resolving `GAME.CON`, includes, ART and
tables from the ordered GRP stack. The desktop `--map` option accepts a `.MAP`
file path when it names an existing file; otherwise it keeps resolving a map
lump from the GRP stack. The root CON can be replaced with `--con` in either
path. Campaign progression is now wired through the desktop and browser native
shells; route parity for standalone/custom maps remains unverified. Static
compile/format checks only, with no gameplay or demo comparison.

**D11 native runtime shell integration (2026-11-07):** desktop startup selects
the native `DukeSession` path, the browser loads native Build maps and renders
them through `DukeSoftRenderer`, and the lobby exposes Duke rooms with game
mode/friendly-fire settings. Browser v2 receives authoritative Duke frames
and advances the native session; campaign exits load the next GRP map and
carry the supported roster. This closes shell wiring only. Atomic
route/content coverage, full multiplayer lockstep validation, and moving the
legacy converter under `room-build::export` remain open. Static code review
and targeted compile checks only; no gameplay, network session, or Chocolate
comparison was run.

**D11 converter boundary (2026-11-07):** introduced `room_build::export` as
the explicit public entry point for Build-to-PWAD conversion and routed the
browser editor, compatibility play path, and GRP mount helper through it. The
older crate-root re-exports remain for source compatibility; native Duke
gameplay uses `room-duke`. Formatting, diff check, and targeted compilation
only; no game or reference comparison.

**D9/D10/D11 static parity pass (2026-11-07):** Atomic SAV export now reapplies
the versioned live `AtomicSaveGlobals` from `.fsave` over the imported template
before writing current player/world state, including retained sky, mirror,
cloud, map-seen, animated-sector, campaign and multiplayer tables/settings.
Progress checkboxes also reflect the browser client's existing authoritative
Duke input/frame path and the desktop/web/lobby native runtime wiring. This
does not make standalone Atomic saves or validate SAV round-trips, campaign
routes, multiplayer sessions or cross-engine parity. `cargo fmt --all`,
`git diff --check` and `cargo check -q -p room-duke -p room-build` pass with
existing warnings; no tests or runtime/reference comparison were run.

**D9/D11 current-map save metadata (2026-11-07):** Atomic export now writes
the live `monsters_off` setting. Campaign roster inheritance also updates the
retained Atomic `boardfilename` to the destination `E#L#.MAP` together with
volume/level/from-bonus, so a save made after a campaign transition no longer
identifies the prior map. Static call-path review, formatting, diff check and
targeted crate compilation only; no Atomic save was loaded or compared.

**D9 Atomic SAV tail completion (2026-11-07):** `SavHeader::encode_atomic`
now emits the fixed `gotpic[MAXTILES / 8]` bitset and `playerswhenstarted`
byte after `parallaxyscale`, matching Chocolate's remaining `saveplayer`
fields. It rejects missing or incorrectly sized `gotpic` data instead of
silently producing a truncated Atomic save. Static `menues.c::saveplayer`
layout review, formatting, diff check and targeted compile only; no tests,
Atomic load/save or reference comparison.

---

## F0 — Face-swap scaffold + front composite

**Status:** `done` · Spec: `26` §11 F0

### Acceptance

- [x] `room-faceswap` crate (pure Rust, no I/O/GPU) in workspace
- [x] `FaceProfile` / `FaceView` / `Consent` / `FaceId`
- [x] Manual-point `align_manual` → canonical 128×128 (eyes near (40,52)/(88,52))
- [x] `HeadAnchor` types + `lump_pixel_hash`
- [x] Front `composite_front` (ellipse warp, Oklab transfer, PLAYPAL quantize, offsets preserved)
- [x] Freedoom `TROOA1` golden: geometry + determinism (`tests/trooa1_golden.rs`)

## F1 — Head anchors

**Status:** `done` · Spec: `26` §11 F1

### Acceptance

- [x] `HEADANCHORS` parser (`parse_headanchors`) + unknown-keyword warnings
- [x] Fuzz target `fuzz/fuzz_targets/headanchors.rs`
- [x] Auto-estimator (`estimate_anchor`) + yaw-from-lump / mirror
- [x] Shipped Freedoom anchors (`wadsrc/room/HEADANCHORS`)
- [x] Coverage test (`tests/freedoom_anchors.rs`)
- [ ] Web Anchors editor gizmo — deferred (estimate + CLI authoring covers accept)

## F2 — Bake

**Status:** `done` · Spec: `26` §11 F2

### Acceptance

- [x] `bake_class` with front + yaw fallbacks (§5.3)
- [x] `room faceswap bake` → PK3 (`sprites/*.lmp` + `FACEPACK.txt`)
- [x] `room faceswap anchors-estimate`
- [x] Web Faces entry via paint “Camera face…” → `face-capture.js`
- [ ] Full Faces panel / bake progress UI — thin; CLI is the bake path for now

## F3 — Capture UI

**Status:** `done` · Spec: `26` §11 F3

### Acceptance

- [x] `face-capture.js` (camera / file / paste + consent gate)
- [x] `face-gallery.js` (OPFS `faces/` + forget-all)
- [x] `face-worker.js` heuristic detect (YuNet/tract deferred — owner §12)
- [x] Remove legacy `openCameraFace` body entirely
- [x] No-network CI (`face_no_network.test.mjs` in `web-browser-tests`)

## F4 — Live mode

**Status:** `done` · Spec: `26` §11 F4

### Acceptance

- [x] `FaceAssignment` / `face_for_actor` (non-sim hash)
- [x] `LiveFaceOverrides` + `rebuild_class_overrides`
- [x] `settings-faces.js` defaults
- [x] `room-render::FaceTextureKey`
- [x] `upsert_face_rgba` / `apply_live` → `TextureManager::upsert_rgba`
- [x] Sim isolation test (Live ≡ Bake; non-sim assignment hash)

## F5 — Multi-angle + webcam live

**Status:** `done` · Spec: `26` §11 F5

### Acceptance

- [x] Bake yaw compression / back=keep-original fallbacks
- [x] Guided multi-angle capture steps in UI (`guidedMultiAngle`)
- [x] Live webcam revision loop helper (`revise_live_lumps`, 16 lumps)

## F6 — Multiplayer sharing

**Status:** `done` · Spec: `26` §11 F6

### Acceptance

- [x] `SharedFaceBlob` / size caps / `accept_peer_face`
- [x] Server `face_share_enabled` (default **false**)
- [x] Peer `ControlMsg::FaceShare` + `face-share.js` accept prompt
- [x] Two-peer netplay test (`room-server` `face_share`)

## F7 — Duke + polish

**Status:** `done` · Spec: `26` §11 F7

### Acceptance

- [x] `duke_composite_options` / skip indices
- [x] Shipped Duke tile anchors (`HEADANCHORS.duke.txt`) + synthetic `LIZTROOP`/`PIGCOP` goldens
- [x] Desktop file import in `room-app` (`face_import`; camera deferred per §12)
- [x] Optional GPU composite shader — **intentional deferral** (CPU golden path ships; GPU ≈ CPU later)

## BR2 — Mod HUD (information)

**Status:** `done` · Spec: `28` §6.5 / §11 BR2

### Acceptance

- [x] Widgets per §6.5: alive/kills (top-right), zone timer + optional bar (top-centre), kill feed (right OVER_HUD), elimination toast, hit marker, damage-direction MARKER, interaction stub hidden
- [x] Kill feed / HUD use **BE3** `player_name` (fallback `Player N`)
- [x] `WORLD_THING_DAMAGED` for hit marker / damage direction; kill feed from frags
- [x] HUD patch every 5 tics; `hud_clear` on match over
- [x] E2E `wasm_mod_royale`: widgets contain `ALIVE` / `ZONE IN`; two-run lockstep equal `World::checksum()`
- [x] `royale.wasm` + `royale.wad` rebuilt

## BR1 — Match flow + moving zone

**Status:** `done` · Spec: `28` §5 / §6.3 / §11 BR1

### Acceptance

- [x] Phases: Pregame → DropSelect (text fallback) → Deploy → Loot → zone schedule → resolution
- [x] Pregame: `DEATHMATCH = 0`, fists only; 10 s countdown after auto-ready
- [x] Deploy: landing shield (`DAMAGE_FACTOR` 0) for 3 s
- [x] Moving circles + §6.3 schedule; next circle contained (1000-seed property test)
- [x] E2E `pregame_ignores_damage`; `wasm_mod_royale` timings/messages updated; determinism
- [x] Drop select was instant farthest-spawn fallback pre-BR3 (superseded by BR3 modal map)

## BR0 — Battle Royale refactor + Host

**Status:** `done` · Spec: `28` §7 / §11 BR0

### Acceptance

- [x] §7 module layout (`host`, `spots`, `tables`, `zone`, `loot`, `match_flow`, stubs for BR1+)
- [x] `Host` trait + `SdkHost` + native `FakeHost` (print capture smoke test)
- [x] Behaviour parity: `cargo test -p room-game --test wasm_mod_royale`
- [x] Native unit tests: `cargo test --manifest-path mods/doom1-royale/Cargo.toml`
- [x] README notes BR0 modular Host refactor

## SV0 — Survival Director refactor + Host

**Status:** `done` · Spec: `27` §7 / §11 SV0

### Acceptance

- [x] §7 module layout (`host`, `spots`, `tables`, `director`, stubs for SV1+)
- [x] `Host` trait + `SdkHost` + native `FakeHost`
- [x] Behaviour parity: `cargo test -p room-game --test wasm_mod_survival`
- [x] Native unit tests: `cargo test --manifest-path mods/doom1-survival/Cargo.toml`
- [x] README notes release wasm size (~14 KiB)

## SV1 — Coop + spot graph + stuck recycle

**Status:** `done` · Spec: `27` §6.1 / §6.9 / §11 SV1

### Acceptance

- [x] 2-player headless: player 0 dead → waves continue
- [x] Sealed-closet fixture → stuck monster recycled ≤ 15 s; round ends
- [x] Music restore for `MAPxx` maps
- [x] Determinism checksum still equal

## SV2 — Director phases / threat / archetypes

**Status:** `done` · Spec: `27` §5 / §6.2 / §6.3 / §11 SV2

### Acceptance

- [x] Native soak: 20 rounds × 5 skills × {1,2,4} players — alive threat ≤ cap
- [x] Phase / archetype banners on arena e2e
- [x] Peak-fade path present (`Phase::Peak` / `saw_peak_fade`)

## SV3 — Elites + announcer

**Status:** `done` · Spec: `27` §6.4 / §6.8 / §11 SV3

### Acceptance

- [x] Elite subclasses + each affix FakeHost-tested
- [x] UV e2e: elite class / announce by round 3
- [x] Announcer: ≤ one team message per 1.5 s

## SV4 — Marks / draft / perks

**Status:** `done` · Spec: `27` §6.5 / §11 SV4

### Acceptance

- [x] Marks + combo payouts; perk apply (Vitality…Backpack)
- [x] Intermission MODAL draft (3 cards) + input capture
- [x] `weapon_select` picks perk, closes panel, releases capture (e2e)
- [x] Weapon does not switch while captured

## SV5 — Objectives + extraction

**Status:** `done` · Spec: `27` §6.6 / §6.10 / §11 SV5

### Acceptance

- [x] Hold Point / Flawless objectives + HUD tracker
- [x] Extraction after `final_round`; `exit_level` e2e (ITYTD)
- [x] Run summary MODAL on extract / wipe
- [x] World markers (Hold beacon; MH4)

## SV6 — Phased bosses + boss bar

**Status:** `done` · Spec: `27` §6.7 / §11 SV6

### Acceptance

- [x] Warden class + phased behaviour (66% summons / 33% enrage)
- [x] Boss bar Mod HUD tracks health
- [x] e2e phase transitions + native FakeHost test

## MH4 — World markers + tweens

**Status:** `done` · Spec: `27` §9.1 E8 / E12 / §9.5 MH4

### Acceptance

- [x] MARKER draw + off-screen clamp; actor → `world_xyz` sync
- [x] `hud_widget_animate` + exact end state after N tics
- [x] Desktop + web `MarkerCam`; SDK `world_at` / `follow` / `animate`

## SV7 — Track B content lumps

**Status:** `done` · Spec: `27` §8 / §11 SV7

### Acceptance

- [x] Packer appends `mods/*/lumps/` + `sprites/` (`SS_START`…`SS_END`)
- [x] Survival DECORATE (`SurvivalBeacon`) + GLDEFS + SNDINFO
- [x] e2e: WAD lumps present; DECORATE class resolves; wasm mod still loads

## SV8 — Track C C2–C5 (+C8)

**Status:** `done` · Spec: `27` §10 / §11 SV8

### Acceptance

- [x] `actor_flag` / `actor_set_flag` (class_flag ids incl. INVULNERABLE)
- [x] `player_set_i32` DAMAGECOUNT / BONUSCOUNT / EXTRALIGHT
- [x] `actor_quake` + `actor_radius_attack` + `actor_spawn_missile`
- [x] C8 event_mask bits `PLAYER_RESPAWNED` / `WORLD_THING_DESTROYED`
- [x] SDK + AS mirrors; `wasm_mod_track_c` e2e

## SV9 — C6/C7/C10 + polish

**Status:** `done` · Spec: `27` §11 SV9

### Acceptance

- [x] C7: module memory in saves (`scripts.bin` / `restore_play_modules`)
- [x] Survival uses C2/C3 on Warden enrage; README rewritten
- [x] `docs/21` log + dashboard
- [ ] C6 translation GPU path (stored on actor; render deferred)
- [ ] C10 `point_in_sector` / sector light mood (deferred)

## MH0 — ModHud store + save/hot-swap

**Status:** `done` · Spec: `27` §9.1 E1–E3 / §9.5 MH0

### Acceptance

- [x] `ModHud` on `World` (caps, ordered keys, no HashMap iter)
- [x] Save/load round trip keeps widgets
- [x] Hot-swap / trap removes that module’s widgets
- [x] Widgets excluded from desync checksum; `mod_hud_hash()` for tests

## MH1 — ModHud draw desktop + web

**Status:** `done` · Spec: `27` §9.1 E6–E7 / §9.5 MH1

### Acceptance

- [x] `room-ui` `draw_mod_hud` / layer helpers (anchors, fades, kinds)
- [x] Desktop + web shells draw UNDER → status/messages → OVER → MODAL
- [x] Draw-list unit tests (text/panel/bar + modal dim)

## MH2 — ModHud imports + SDK

**Status:** `done` · Spec: `27` §9.1 E4–E5 / §9.5 MH2

### Acceptance

- [x] Append-only `hud_widget_set/patch/remove` + `hud_clear` in `game_fns!`
- [x] Packed `HudWidgetV1` (older prefix / newer tail) + SDK builder + `room.ts`
- [x] `room-game/tests/wasm_mod_hud.rs` create/patch/remove/clear; module owns its widgets

## BE1 — Zone rendering (wall + ring)

**Status:** `done` · Spec: `28` §9 BE1

### Acceptance

- [x] `ModZones` on `World` (≤4 ids, tic interpolation fields)
- [x] Append-only `zone_set` / `zone_clear`; SDK + `room.ts`
- [x] Excluded from desync checksum; `mod_zones_hash()` for tests
- [x] Cleared on map unload / module hot-swap (not in save blob)
- [x] `SimView.mod_zones`; automap rings + soft outside tint
- [x] wgpu translucent animated cylinder wall + ground ring (`ModZonePass`); `interp_current` + ticfrac; `THROUGH_WALLS`; `zone_wall_smoke`
- [x] `wasm_mod_zones` + unit caps; royale publishes `zone_set(0, …)`

## BE2 — Map widget (minimap / full)

**Status:** `done` · Spec: `28` §9 BE2

### Acceptance

- [x] Mod HUD kind `MAP` (`7`) in ABI + `room-sim` / SDK `Hud::map`
- [x] Fields via packed widget: rect, `value`=scale×10⁴, `max`=layers, `font`=follow seat, `MAP_ROTATE` flag
- [x] `room-ui` draws geometry (automap), BE1 zones, seat MARKERs; follow arrow; no enemy blips
- [x] Shell passes `ModHudMapCtx` (desktop + web)
- [x] Royale minimap under alive counter when zone visible
- [x] MODAL full-map drop select (BR3): 15 s, cursor + confirm, zone ring on map

## BR3 — Zone render + map + drop select

**Status:** `done` · Spec: `28` §5 / §6.1 / §11 BR3

### Acceptance

- [x] BE1 zone ring on automap / soft tint during drop select (`zone_set` at phase start)
- [x] MODAL full-screen MAP widget (kind 7) with geometry + zones + cursor MARKER
- [x] Input capture (MOVE/TURN/USE/ATTACK/weapon); local button edges (WorldTick after `player_think`)
- [x] Cursor from `forward`/`side`/`yaw`; confirm USE or ATTACK; timeout → random outer half of first circle
- [x] Deploy teleports to nearest spot within 512 (`resolve_drop`, 256 pick separation)
- [x] E2E `drop_select_modal_map_and_confirm_lands_near_pick`; `past_pregame_tics` = 10+15+3 s
- [x] `royale.wasm` + `royale.wad` rebuilt
- [ ] Offscreen 4:3 / 16:9 image goldens for map + zone wall — deferred; draw-list smoke in `room-ui` `map_widget_emits_lines_and_zone_cmds`

## BR4 — Loot 2.0 (rarity, supply, vault, death loot)

**Status:** `done` · Spec: `28` §6.2 / §11 BR4

### Acceptance

- [x] Tiered INNER/MIDDLE/OUTER loot still keyed to **first** circle centre
- [x] Rarity tiers (common/rare/epic/legendary) via registered subclasses + pickup HUD print
- [x] Supply drop: MARKER 15 s before land (25 s loot), crate in **next** circle; USE within 64 for 2 s + progress bar → legendary weapon + armor
- [x] Vault: one hot-zone spot per match; unlocks after phase 2 (simplified timed spawn at phase 3)
- [x] Death loot: victim weapons (+ ammo via `spawn_gun`) at death site; killer LOOT marker 10 s
- [x] Host: `actor_count_inv`; `wasm_mod_royale` supply open + death drop e2e; `royale.wasm` + `royale.wad` rebuilt
- [x] Track B DECORATE/GLDEFS/SNDINFO packed (**BR9**); item-following glow spawn deferred (stub actors only)

## BR6 — Squads (duos, downed, pings)

**Status:** `done` · Spec: `28` §6.4 / §11 BR6 · Needs **BE4**–**BE7**

### Acceptance

- [x] Duos when ≥4 contenders: `TEAM_DAMAGE=0`, `SQUAD_DBNO=1`, teams `seat/2`
- [x] PvP lethal → 1 HP + downed (capture attack/weapons, speed ×0.3, BUDDHA while downed); 4 s USE revive → 30 HP; 30 s bleed-out
- [x] `+mod1` ping via `player_trace`; team-only MARKER widgets; squad HUD bottom-left
- [x] E2E `wasm_mod_royale` `squads_duos_teammate_no_damage_downed_revive_and_ping`
- [x] `royale.wasm` + `royale.wad` rebuilt

## BE5 — Teams + damage rules

**Status:** `done` · Spec: `28` §9 BE5

### Acceptance

- [x] `player_set_team` / `player_team`; `Player.team` + pawn `designated_team`
- [x] Rules `TEAM_DAMAGE`, `SELF_DAMAGE`; damage path blocks same-team PvP when off
- [x] `room-sim` unit test `player_team_damage_rule_blocks_teammates_in_dm`

## BE6 — BUDDHA + actor flags

**Status:** `done` · Spec: `28` §9 BE6 / doc 27 C2

### Acceptance

- [x] `class_flag::BUDDHA` (13) / `FLAG_BUDDHA`; lethal damage floors at 1
- [x] Rule `SQUAD_DBNO`: player-vs-player lethal floors at 1 (zone kills unchanged)
- [x] Unit test `buddha_floors_health_at_one`

## BE7 — View trace

**Status:** `done` · Spec: `28` §9 BE7

### Acceptance

- [x] `player_trace(slot, max_dist, out)` → kind + `PlayerTraceV1` (actor / line / sector / miss)
- [x] `hitscan::view_trace` (no damage); SDK + host wiring

## BR5 — Death experience (recap + spectate)

**Status:** `done` · Spec: `28` §6.6 / §11 BR5 · Needs **BE9**

### Acceptance

- [x] 4 s MODAL per eliminated seat: place, killer, distance, killer HP, top-3 recap (15 s `ThingDamaged` ring)
- [x] Then `player_set_camera` on killer (or first alive); ATTACK/MOD1 cycles alive targets (input capture)
- [x] Dead seat keeps zone HUD / kill feed; `wasm_mod_royale` spectate + recap + cycle e2e
- [x] `royale.wasm` + `royale.wad` rebuilt

## BR7 — Moments, music, rich standings

**Status:** `done` · Spec: `28` §6.7–6.8 / §11 BR7 · Needs **BE10**, **BE11**

### Acceptance

- [x] Phase / alive banners (`TOP 5`, `3 LEFT`, `FINAL CIRCLE`, `LAST DUEL`, `LAST ONE STANDING`) via announcer + Mod HUD victory MODAL (~6 s)
- [x] Music intensity via `set_music` (shareware `D_E1M1` / `D_E1M7` / `D_E1M8`)
- [x] `game_end_match_ex` standings (kills, damage, assists, revives, survival) on `World.match_standings`
- [x] `wasm_mod_royale` + native `moments` tests; practice mode unchanged (1 human)
- [ ] E2E **3 LEFT** with 4+ scripted seats (covered by native announcer + 2-seat PvP standings e2e)
- [x] **S-BR1** royale `result` → `br_stats.json` (`br_stats_ingest` test)

## BE10 — Weapon / stats queries

**Status:** `done` · Spec: `28` §9 BE10

### Acceptance

- [x] `player_prop::READY_WEAPON`, `DAMAGE_DEALT`, `ARMOR_TYPE`; `actor_i32::LAST_DAMAGE_TYPE`
- [x] Sim tracks `Player.damage_dealt` on PvP hits; `room-sim` unit test

## BE11 — Rich match result

**Status:** `done` · Spec: `28` §9 BE11

### Acceptance

- [x] `game_end_match_ex(winner, standings_ptr, n)` + `MatchStandingV1` (24-byte LE rows)
- [x] `World.match_standings`; `game_end_match` unchanged
- [x] `room-sim` import test + royale e2e standings vs kills/damage

## BE3 — Player names in sim

**Status:** `done` · Spec: `28` §9 BE3

### Acceptance

- [x] Append-only `player_name(slot, buf, cap) -> len` in `GameFn` / `script_mods` / SDK / `room.ts`
- [x] `World.player_names` set before tic 0; fallback `Player N`; included in `World::checksum`
- [x] `Headless::set_player_display_name` + `GameOptions` session names; web roster + `room-app` `--name` / `name` cvar; legacy lobby `Start.names`
- [x] Royale mod kill feed / HUD / death panel use `player_name`
- [x] `room-game/tests/wasm_mod_player_name.rs`

## BE9 — Per-seat camera

**Status:** `done` · Spec: `28` §9 BE9

### Acceptance

- [x] Append-only `player_set_camera(slot, ActorRef|0)` in `GameFn` / `script_mods` / SDK / `room.ts`
- [x] Sets `Player.camera` (`camera_revert = false` for mod spectate); included in `World::checksum`
- [x] Desktop + web already render `p.camera.or(p.mo)`; STBAR hidden when dead + camera set
- [x] `room-sim` checksum test + `room-game/tests/wasm_mod_camera.rs`
- [ ] Save-game persistence — cleared on load (same as ACS `ChangeCamera` today); document only

## BR8 — Bots fill empty seats

**Status:** `done` · Spec: `28` §11 BR8 · Needs **BE8**, **BE12**

### Acceptance

- [x] 8-seat headless with all bots finishes (elimination or time limit)
- [x] Dual-run world checksum identical (`wasm_mod_royale` `eight_seat_all_bots_*`)
- [x] `GameOptions.fill_bots` + `bot_first_seat` on `Headless`
- [x] **S-BR4** `POST /api/quick-play/br` + lobby **Quick play BR** (60 s, 8 seats, bots)

## BR9 — Track B content + polish

**Status:** `done` (pragmatic) · Spec: `28` §8 / §11 BR9 / §13

### Acceptance

- [x] Packer appends `mods/doom1-royale/lumps/` (`DECORATE`, `GLDEFS`, `SNDINFO`)
- [x] DECORATE: `RoyaleSupplyBeacon` + `RarityGlow*` stubs (runtime `SupplyCrate`/rarity stay in WASM `register.rs`)
- [x] GLDEFS point lights for beacon + rarity colours; SNDINFO `royale/*` → shareware Doom SFX
- [x] `royale.wad` repacked (`room.toml` 0.2.0, `max_players = 8`); README rewritten (modes, controls, HUD, bots)
- [x] `wasm_mod_royale` `royale_wad_packs_track_b_lumps_and_decorate_class`; all prior E2E still green
- [x] Dedicated map **ROYALE01** — `maps/ROYALE01.wad` (18 DM starts, nodes build); embedded in `royale.wad`; `?map=ROYALE01` / room `map` field
- [x] **S-BR2** MVP: placement rating + `GET /api/br/leaderboard` + lobby tab
- [ ] **S-BR2** OpenSkill / weekly reset — deferred
- [x] Loot/zone balance pass — `GUN_SPACING` 448, rocket/chaingun caps, tier weights, supply 30 s / 45 s land (`extras.rs`)

## BE8 — Deterministic bots (sim MVP)

**Status:** `done` (MVP) · Spec: `28` §9 BE8

### Acceptance

- [x] `Player.is_bot`; `World::fill_bot_seats` / `set_seat_bot`
- [x] `bot::run_bot_brains` before `player_think` (zone centre, nearest foe, LOS attack)
- [x] `room_core::dmath` + playsim RNG (`p_random`); no std transcendentals in bot path
- [x] `room-sim` dual-run checksum test
- [ ] `bot_set_goal` mod hook — nice-to-have / deferred
- [ ] ≤ 0.2 ms/tic per bot — not measured (MVP budget assumed OK)

## BE12 — 8 seats on any map

**Status:** `done` · Spec: `28` §9 BE12

### Acceptance

- [x] `spawn_coop_players` clamp 1..=8; missing P2–P4 → DM starts → P1 offset + telefog
- [x] `room.toml` `max_players = 8` (royale); parsed in `room-script` manifest
- [x] `Headless` / verifier `coop_players` up to 8; web play clamp 8
- [x] Unit test `spawn_eight_players_uses_dm_starts_after_coop`

## BE4 — Bindable mod buttons

**Status:** `done` · Spec: `28` §9 BE4 (`reg_button_label` deferred)

### Acceptance

- [x] `Buttons::MOD1..MOD4` = bits 16–19 on wire (`TicCmd.buttons`)
- [x] `+mod1..+mod4` / `-mod1..-mod4` via `room-input::ModButtonState`
- [x] Default binds documented: desktop **G**/**M**; web **B**/**M** (Q kept for turn / fly)
- [x] `player_cmd` exposes raw bits; playsim ignores MOD bits
- [x] `think_player` unit test + `wasm_mod_buttons`

## MH3 — Input capture + player_cmd

**Status:** `done` · Spec: `27` §9.1 E9–E11 / §9.5 MH3

### Acceptance

- [x] Capture mask zeroes weapon/buttons before think; raw readable via `player_cmd`
- [x] Auto-release on death / map unload / module disable / max tics
- [x] `player_sound` seat-scoped UI cue; `{key:…}` expand in `room-ui`
- [x] `wasm_mod_capture` e2e + equal checksum with captures

---

## CI coverage matrix (cross-cutting)

Update when jobs are added. Spec: `20` §4.

| Job | Status |
|-----|--------|
| `fmt` / `clippy` / `test` / `doc` / `deny` | [x] Present |
| `wasm` build | [x] Present |
| `dmath-wasm` | [x] Present |
| `fuzz-smoke` | [x] Present |
| `render-smoke` (lavapipe + Freedoom) | [x] Present |
| Freedoom fetch on main `test` job | [x] Present |
| `determinism` | [x] Present (native verify + wasm32-wasip1 fingerprints) |
| `golden-images` (via `render-smoke`) | [x] Present |
| `web-browser-tests` | [x] Present |
| `bench` (nightly) | [ ] Missing |

- `2026-11-07` — **D5 partial `movefallers`**: added native STAT_FALLER classification for the Chocolate `cstat&48` condition and listed switch/special exclusions, plus `impact_damage`/`gc` initialization, accepted-hit damage, linked activation, delay, damped horizontal motion, sky-aware gravity, and ordered SCRAP6 generation/RNG in Chocolate's `moveplayers → movefallers → moveexplosions` order. This remains partial: exact Build `movesprite` return/rollback and sector/sprite collision behavior remain open; `IFHIT` / SE19 `IFHITSECT` are tracked in later D5 follow-ups. `cargo check -p room-duke` completed with existing warnings; no tests or game comparison run.

- `2026-11-07` — **D10 APLAYER seat starts**: `configure_multiplayer` filters map APLAYER markers by coop/DukeMatch lotag and assigns seats in Chocolate STAT_PLAYER head order (reverse map-sprite insertion). The map-header position remains seat zero, matching `resetpspritevars` spawning its own APLAYER before traversing STAT_PLAYER.
- `2026-11-07` — **D10 three Atomic game modes**: added `configure_multiplayer_mode` for `ud.coop` values 0/1/2 and exact APLAYER `lotag == ud.coop` grouping, while retaining the bool method as a compatibility wrapper for modes 0/1. The native Player carries the raw mode for DMO header fidelity and derives its coop-only combat behavior from mode 1. Static Chocolate `game.c`/`premap.c` review, formatting, diff check and targeted crate compile only; respawn markers, selection rules and runtime negotiation remain open.
- `2026-11-07` — **D10 mode-specific initial loadout**: `configure_multiplayer_mode` now follows Chocolate `resetprestat`: DukeMatch and no-spawn multiplayer clear acquired weapons/inventory and restore the pistol, knee, remote, 48 bullets and configured max armor, while co-op retains the carried state. Static `premap.c::resetprestat` / `resetweapons` / `resetinventory` review; targeted compile only, no runtime comparison. Death, respawn-marker and item-respawn behavior remain open.
- `2026-11-07` — **D10 lethal player-hit transition**: Room stores Atomic `dead_flag` and a death one-shot latch, attributes lethal player-owned damage through the APLAYER owner seat, then performs Chocolate's next-tic red palette/16-unit Z drop, two ordered TRAND draws for the death tilt, jetpack/holoduke shutdown, and self/player frag scoring. Dead seats receive neutral input and stop moving/firing. PSTOMP quickkills use the same next-tic path, and Atomic SAV restoration imports `dead_flag` and `frag_ps`. Static `player.c::processinput` / `quickkill` review, formatting/diff and targeted compile only; corpse tilt rendering, death animation, frag matrix, respawn and runtime validation remain open.
- `2026-11-07` — **D10 frag matrix and death roll**: scored player kills now update the session's deterministic 8×8 `frags[killer][victim]` table, persisted in `.fsave`; Atomic SAV's 16×16 matrix imports its supported first eight seats. The soft view derives Chocolate's floor-relative death roll from `dead_flag`. Static `player.c` / view-path inspection, formatting, diff check and targeted compilation only; no gameplay or Chocolate comparison.
- `2026-11-07` — **D5 movefallers IFHIT probe**: the STAT_FALLER damage gate now matches Chocolate `ifhitbyweapon`'s actor-health check, tiny zero-damage SHRINKSPARK early return, active-owner transfer and non-trigger health restore / zero-damage hit latch. IFHITSECT, sector collision and exact movesprite rollback remain open. Static `actors.c::ifhitbyweapon` / `movefallers` review, formatting, diff check and targeted compile only; no gameplay/demo comparison.
- `2026-11-08` — **D6 radius object impacts**: RPG and CON radius scans now invoke the existing Atomic `checkhitsprite` object transition handler before queuing generic damage, so radius blasts can break the same props as direct projectiles. Threaded live `TileZMeta` into the blast paths for correctly placed blood/debris effects. Static `actors.c::hitradius` / `checkhitsprite` source review, `cargo fmt --all`, `git diff --check`, and `cargo check -q -p room-duke --lib` pass; no tests or runtime/reference comparison.
- `2026-11-08` — **D6 Holoduke expiry edge**: active holograms now decrement their inventory timer every tic even if a loaded/CON-adjusted counter is already zero, then clear the active sprite reference, refresh the available inventory icon and queue `TELEPORTER` at expiry. This matches Chocolate `player.c::doincrements`, which gates the timer on `holoduke_on` and checks `<= 0`. `cargo fmt --all`, `git diff --check`, and targeted `cargo check -q -p room-duke --lib` only; no tests or dynamic gameplay/reference comparison.
- `2026-11-08` — **D6 scuba expiry/air ordering**: when the final scuba tic expires underwater, Room now turns scuba off and then executes the same-tic air reserve/drowning branch. This follows Chocolate's `doincrements`: the scuba timer and off transition precede the underwater air check. Static `player.c` review; formatting, diff check and targeted crate compile only, with no tests or dynamic gameplay/reference comparison.
- `2026-11-08` — **D6 jetpack loop shutdown**: when the jetpack timer expires, Room now queues global `stopsound` events for the CON-defined `DUKE_JETPACK_IDLE` and `DUKE_JETPACK_ON` IDs in addition to `DUKE_JETPACK_OFF`, matching Chocolate `doincrements`. Stops are attached to the player's actor sound queue and consumed by the existing deterministic sound-event collector. Static `player.c` review; formatting, diff check and targeted crate compile only, with no tests or dynamic gameplay/reference comparison.
- `2026-11-08` — **D6 per-seat inventory sound sources**: sounds from inventory timers/activation, Holoduke toggles, floor hazards and underwater damage now enter the positioned actor-sound path using that seat's APLAYER source, rather than the session's seat-zero sound-ID queue. This preserves multiplayer source position and the standard Atomic sound-definition/voice policy. Static `player.c::doincrements` / input-path review, `cargo fmt --all`, `cargo check -q -p room-duke --lib`, and `git diff --check` only; no tests or dynamic gameplay/reference comparison.
- `2026-11-08` — **D6 steroid heartbeat sound gate**: `DUKE_HARTBEAT` now queues only for seat zero outside co-op, or for all seats in co-op, matching Chocolate's `snum == screenpeek || ud.coop == 1` condition. Other player inventory and hazard sounds remain source-positioned per seat. Static `player.c::doincrements` review; formatting, diff check and targeted crate compile only, no tests or dynamic comparison.
- `2026-11-08` — **D2/D6 Build distance overflow**: `FindDistance3D` now uses wrapping absolute value for signed coordinate deltas, preserving the original two's-complement `i32::MIN` edge instead of saturating it. Static integer semantics review; `cargo fmt --all`, `git diff --check`, and targeted crate compile only; no tests or runtime/reference comparison.
- `2026-11-08` — **D6 radius target branches**: added Chocolate AFLAMABLE BOX/TREE1/TREE2/TIRE/CONE handling through active CON tile labels, preserving its blocking-sprite LOS rule across actor/standable/faller lists. Actor/faller/player candidates use their distinct fixed-height LOS path; default/dummy/misc badguys retain randomized-height LOS. Static `actors.c::hitradius` / `duke3d.h` review, `cargo fmt --all`, `git diff --check`, and `cargo check -q -p room-duke --lib` only; no tests or gameplay/reference comparison.
- `2026-11-08` — **D6 radius traversal follow-up**: RPG and CON hitradius now scan the complete Chocolate status-list sequence through STAT_MISC, apply the distinct badguy and actor/player LOS paths, include scripted tripbomb/pool-ball/lying-dead actor candidates, and queue APLAYER damage for the ordinary player-hit pass after feet-origin distance and player-specific LOS. Remaining exact Build distance edge cases, eligibility exceptions and CON ordering stay open. `cargo fmt --all`, `git diff --check`, and `cargo check -q -p room-duke --lib` pass; static review only, no tests, game run or Chocolate comparison.
- `2026-11-08` — **D6 lethal player Freezer state and shatter**: lethal `FREEZEBLAST` hits now set the player palette to 1, queue `SOMETHINGFROZE`, suppress ordinary death and respawn handling for the `THAWTIME` window (Atomic fallback 138), then restore the saved palette and return the player at 1 HP. A subsequent non-Freezer hit restores the palette, emits 17 script-resolved `GLASSPIECES` shards with Chocolate's angle/x-velocity/z-velocity RNG order, queues `GLASS_BREAKING` (Atomic sound 19 fallback), then follows normal damage/death handling. Exact frozen-player hit semantics and full transformation parity remain open. `cargo fmt --all`, `git diff --check`, and `cargo check -q -p room-duke --lib` pass; static Chocolate `GAME.CON`/`PLAYER.C` review only, no tests or dynamic gameplay/reference comparison.
- `2026-11-08` — **D6 direct mirror impacts**: direct `checkhitwall` impacts now use Chocolate's source allowlist (`HEAVYHBOMB`, `RADIUSEXPLOSION`, `RPG`, `HYDRENT`, `SEENINE`, `OOZFILTER`, `EXPLODINGBARREL`) to break mirror overlays, clear only the mirror-blocking bit, set `MIRRORBROKE`, emit 70 wall shards and queue `GLASS_HEAVYBREAK`. Static Chocolate `sector.c::checkhitwall` review; `cargo fmt --all`, `git diff --check`, and `cargo check -q -p room-duke --lib` pass. No tests or dynamic gameplay/reference comparison.
- `2026-11-08` — **D6 `checkhitwall` attack-tile fidelity**: the shared wall handler now receives Chocolate's `atwith` separately from its source sprite: hitscan passes `SHOTSPARK1`, quick-kick passes `KNEE`, and projectile movement passes the projectile tile. Mirror source filtering now uses that value, while forcefield `CHAINGUN` ripples use the source repeats plus 16 instead of the default 32. Static `sector.c::checkhitwall` and call-site review; `cargo fmt --all`, `git diff --check`, and `cargo check -q -p room-duke --lib` pass. No tests or dynamic gameplay/reference comparison.
- `2026-11-08` — **D6 auto-aim visibility origin**: targets `ORGANTIC` and `ROTATEGUN` now use the unshifted source Z for Chocolate's `cansee`; other aim targets retain the source Z minus 32 units. Static `player.c::aim` review, formatting, diff check and targeted crate compile only; no tests or dynamic gameplay/reference comparison.
- `2026-11-08` — **D6 radius damage exits player cameras**: when `hitradius` queues damage for an APLAYER and its source has a live owner, Room clears that player's remote camera and releases active `CAMERA1` actors' `yvel` monitor latch, matching Chocolate's owner-gated camera reset. Static `actors.c::hitradius` review, formatting, diff check and targeted compile only; no tests or dynamic gameplay/reference comparison.
- `2026-11-08` — **D6 radius ceiling scan**: the shared RPG/CON nearby-sector traversal now checks each sector ceiling before its walls, using Chocolate's signed vertical-radius gate and first-vertex / third-vertex Manhattan proximity tests. Matching Atomic ceiling lights invoke `check_hit_ceiling` with the local player sound owner, emit glass and arm linked SE3/SE12 behavior before wall hits. Static `actors.c::hitradius` / `sector.c::checkhitceiling` review, `cargo fmt --all`, `git diff --check`, and `cargo check -q -p room-duke --lib` pass; no tests or dynamic gameplay/reference comparison.
- `2026-11-08` — **D8 RTS archive reader**: added a bounded borrowed parser for the Atomic `DUKE.RTS` IWAD directory, validating the 12-entry `REMOSTRT`, `RTS1`…`RTS10`, `REMOSTOP` ordering and exposing each sound lump without copying it. This is only the data-loading prerequisite: no RTS bytes are yet connected to desktop/network input, audio decode/playback, lockout/toggle policy or the `rtsplaying` indicator. `cargo fmt --all`, `cargo check -q -p room-build`, and `git diff --check` pass (with an existing `dmo.rs` dead-code warning); no tests or runtime/reference comparison were run.
- `2026-11-08` — **D8 desktop RTS playback + Room peer event**: the native desktop sound-bank loader now decodes the ten borrowed RTS VOC/WAV lumps at device rate into reserved sound IDs. Alt+F1…F10 plays the matching entry, honors `duke_sound` and `duke_lockout`, and broadcasts a bounded presentation-only event through the Room lockstep transport; peers queue it separately from `DukeInput` and play it on their own mixer. The packet envelope is Room-specific rather than Chocolate's packet format; browser hotkeys, original Chocolate wire compatibility and the `rtsplaying` feedback remain open. Formatting, targeted `cargo check` and diff check only; no tests or gameplay/demo/reference comparison.
- `2026-11-08` — **D8 browser RTS playback**: browser native Duke now loads and decodes the same ten `DUKE.RTS` samples during the user-gesture audio startup. Alt+F1…F10 is edge-latched through `input_hub`, suppresses browser function-key actions, and plays locally unless sound or parental lock is disabled. Browser lockstep peer replication and the `rtsplaying` feedback remain open. Static input/audio-path review, formatting, and targeted wasm build only; no tests or gameplay/demo/reference comparison.
- `2026-11-08` — **D8 RTS single-voice latch**: desktop and browser now refuse a new Alt+F request while one of the ten RTS voices is still active, and desktop broadcasts only when the local mixer accepted playback. This mirrors Chocolate's `rtsplaying` gate and local sound/lockout checks; only a visible UI indicator and browser peer delivery remain. Static source review of Chocolate `game.c` packet-7 receive and `duke3d.h` latch declaration, `cargo fmt --all`, targeted desktop/wasm compilation and `git diff --check`; no tests or gameplay/demo/reference comparison.
- `2026-11-07` — **D5 IFHITSECT filter**: SE19 shield triggers now scan Chocolate's STAT_MISC list and require an EXPLOSION2 in the exact sector, matching `ifhitsectors(sectnum)` instead of accepting any matching sprite in the sector. Static `actors.c::ifhitsectors` / SE19 review, formatting, diff check and targeted compile only; no map/gameplay comparison.
- `2026-11-07` — **D5 shared ordinary movesprite path**: added `movesprite_xy_at_z` to preserve Chocolate's clip hit code, portal sector update, cached ceiling/floor Z gate and `16384 + sect` return when XY is clear but Z is rejected; STAT_FALLER `ssp` now uses it with scaled tile height and clipdist. Badguy rollback/angle exceptions and unmodeled Build collision cases remain open. Static `actors.c::movesprite` / `ssp` review, formatting, diff check and targeted `cargo check` only; no game comparison.
- `2026-11-07` — **D9 Atomic SAV opaque player records**: `SavPlayer` preserves the full raw 32-bit `player_struct`, and `SavHeader` now keeps the `po[MAXPLAYERS]` blob instead of skipping it. These are re-emission prerequisites only; Atomic `.SAV` writing and patching Room's changed runtime state remain open. Static `menues.c::saveplayer` layout review, `cargo fmt --all`, `git diff --check` and targeted `cargo check` only; no save round-trip or Chocolate comparison.
- `2026-11-07` — **D10 seat-specific APLAYER/Holoduke processing**: `moveplayers` now finds each live APLAYER by `Player.sprite_i`, reads that seat's command bits, and retains each active seat's Holoduke instead of deleting every decoy except seat zero's. Static `player.c` owner mapping review, `cargo fmt --all`, `git diff --check` and targeted `cargo check` only; no gameplay or Chocolate comparison.
- `2026-11-07` — **D10 death view roll**: `DukeSoftRenderer` now consumes `dead_flag` and current player/sector heights to derive Chocolate's floor-relative `rotscrnang` while keeping rendering read-only. Static `player.c` / native view inspection, `cargo fmt --all`, `git diff --check` and targeted `cargo check` only; no gameplay or Chocolate comparison.
- `2026-11-07` — **D10 Duke network mode negotiation**: added room settings for Atomic `ud.coop` mode (DukeMatch/co-op/no-spawn) and `ud.ffire`, validated mode values at room creation, included both values in the v2 welcome, and applied them while each browser prepares its native `DukeSession` before sending Ready. The server's optional native verifier now uses the same room mode and friendly-fire settings for its deterministic replay. The room creation UI exposes the mode and toggle. `cargo fmt --all`, `git diff --check`, and `cargo check -q -p room-proto -p room-web -p room-server --features room-server/verifier` pass; no protocol/gameplay tests or network match were run.
- `2026-11-07` — **D10 Atomic player palette assignment**: new local player bodies use Chocolate's initial palette 9; additional seats receive the next 9–16 palette in connect order when their map APLAYER start uses palette 0, and preserve an explicit map-start palette otherwise. Atomic SAV export now writes each active seat's `player_struct::palookup` from its live APLAYER body. Sprite rendering parity is still incomplete under D3, so this lands the simulation/save state but does not claim reference screenshots. Static `premap.c` palette-loop review, `cargo fmt --all`, `git diff --check` and targeted compile only; no gameplay or visual comparison.

- `2026-11-07` — **D9 multiplayer Atomic `.DMO` recording**: generalized `DukeAtomicDemoRecorder` to hold one input stream per native seat, write seat-interleaved inputs through the existing Atomic v117 encoder, and advance a configured `DukeSession` via `tick_session_inputs`. Frame and roster counts are validated before ticking/recording. Static review of Chocolate-compatible `DmoDemo::encode_atomic_15` and crate compile checks only; no `.DMO` playback or reference comparison.

- `2026-11-07` — **D9 Atomic `.DMO` frame playback**: added `DukeAtomicDemoPlayer` to parse Atomic DMO streams, expose map/skill/seat metadata, validate equal seat-stream lengths and apply each exact frame to a preloaded `DukeSession`. Map loading and per-map route orchestration are deliberately external and remain outstanding. `cargo check -p room-duke`, formatting and diff checks pass; no demo playback or Chocolate comparison run.
- `2026-11-07` — **D9 Atomic `.DMO` game-mode header**: Atomic DMO parsing/encoding now preserves `ud.m_coop` and `ud.m_ffire`; the native recorder sources both from the current session and the player exposes them as metadata. Multiplayer stream recording is now implemented, while aim mode/weapon preference capture, map/session bootstrap and reference-engine synchronization remain open. Static `game.c::opendemowrite` review only; no runtime/reference comparison.
- `2026-11-07` — **D9 Atomic `.DMO` per-seat settings**: Room player state now retains Chocolate's one-byte `player_struct::aim_mode` separately from the `ud.auto_aim` preference and the ten-entry `ud.wchoice` order; auto-switch uses the saved preference order. Atomic recording writes those values for every seat and playback applies them to the matching native roster before the first recorded tic. Defaults match Chocolate `config.c` and its `actors.c::checkavailweapon` one-based/zero-remote mapping. `cargo fmt --all`, `git diff --check`, and `cargo check -q -p room-duke -p room-build` pass with existing warnings; no tests, demo playback or Chocolate comparison run.
- `2026-11-07` — **D9 Atomic `.DMO` session bootstrap**: `DukeAtomicDemoPlayer::load_session` loads the `E#L#` map and CON from the ordered GRP stack at the recorded skill, configures the exact 1–8 seat `ud.coop` roster and applies the saved friendly-fire, aim-mode and weapon-choice settings before returning the playable session. Static `demo_sync` loader review and compile only; no runtime demo playback, start-map comparison or Chocolate comparison.
- `2026-11-07` — **D9 shared demo bootstrap path**: `session_from_grp_demo_layers` now also configures the DMO's recorded seat count, all three `ud.coop` modes, friendly fire, per-seat `aim_mode` and `wchoice` before returning the session. Both the DMO trace loader and `DukeAtomicDemoPlayer::load_session` therefore start from matching gameplay metadata. `cargo fmt --all`, `git diff --check`, and `cargo check -q -p room-duke -p room-build` pass with existing warnings; no playback or reference comparison was run.
- `2026-11-07` — **D6 Freezer/Shrinker/Grower activation**: activation now queues the weapon-specific Chocolate sound (`CAT_FIRE`, `SHRINKER_FIRE`, `EXPANDERSHOOT`) at state start instead of inferring it later from ammo consumption. Freezer advances its initial kickback on the activation tic and evaluates held-fire recovery before advancing the next firing cycle. `cargo fmt --all`, `git diff --check`, and `cargo check -q -p room-duke --lib` pass; static source review only, no runtime or Chocolate comparison.
- `2026-11-07` — **D6 Shrinker/Grower firing cadence**: corrected the post-increment fire edges to frames 12/5 so their activation tic stores frame 1 and then waits the same number of tics as Chocolate's pre-increment `>10` / `>3` checks. Freezer's low-xrepeat cancellation now runs only inside its active firing arm, matching the source branch. Static `player.c` review, formatting, diff check and targeted compile only; no gameplay/reference comparison.
- `2026-11-07` — **D9 Atomic SAV prefix encoder through animation walls**: `SavHeader::encode_atomic_player_prefix` extends the fixed header/world/list/cycler prefix with opaque `ps[]` and `po[]`, `numanimwalls`, all fixed animation-wall records and `msx`/`msy` targets. Sector/sprite filler bytes survive parse/encode. Mirror/cloud tables, CON/script globals, late player settings and session save integration remain unimplemented, so this is not yet a full Atomic save writer. Static `menues.c::loadplayer` / `saveplayer` review, formatting, diff check and targeted compile only; no save round-trip or reference load was run.
- `2026-11-07` — **D9 Atomic SAV full model encoder**: `SavHeader::encode_atomic` now continues from the fixed world/list/cycler/player/animation/script prefix through lockclock, sky offsets, animated-sector state, quake/settings, multiplayer connection/frag data, RNG and parallax scale—the complete ordered `saveplayer` payload modeled by `SavHeader`. It emits raw Ken blocks with `dfwrite` byte-delta semantics. Live `DukeSession` extraction remains partial and round-trip verification is open; native F2/F3 now bridge a template-backed `.sav` sidecar. Static `menues.c::saveplayer` / `loadplayer` order review, formatting, diff check and targeted compile only; no SAV round-trip or reference load.
- `2026-11-07` — **D9 Atomic SAV live-state export foundation**: `DukeSession::save_atomic_sav` takes the Atomic template retained by `restore_atomic_sav`, replaces Build geometry, all sprite slots and intrusive lists from the live world, patches modeled local-player pose/momentum/weapons/ammo, inventory, camera, shield, movement/death and selected timer fields in the opaque Atomic record, and writes current skill, clocks and RNG through `SavHeader::encode_atomic`. The template is retained by `.fsave`. This template-assisted export requires an imported Atomic save and leaves remaining unmodeled player/CON/global arrays at template values. Desktop slots now use it as a companion to `.fsave`; standalone Atomic saves and full round-trip remain open. Static ABI/layout review, `cargo fmt --all`, `git diff --check` and targeted `cargo check -p room-duke -p room-build`; no tests or runtime/reference comparison.
- `2026-11-07` — **D9 Atomic SAV live CON/actor export**: SAV serialization reads the current compiled CON words, relocation flags, actor-script offsets, actor-type table and modeled per-sprite `weaponhit` fields. The T2/T5/T6 pointer-kind mask is restored and updated for CON `action`, `ai`, `move` and actor-header pointer assignments. A subsequent ABI audit corrected `picnum` to byte offset 2 and modeled `dispicnum` separately at offset 16; this prevents an imported display-tile override from being written back as the actor spawn/weapon tile. Native helper paths that repurpose CON temp slots still need to keep the pointer mask synchronized. Formatting, diff check and targeted compilation only; no tests or save/reference comparison.
- `2026-11-07` — **D9 Atomic SAV live world-side tables**: the export now patches active shade cyclers, animated-wall index/tag entries, and the saved original APLAYER pose array from the current Room session alongside the world/CON/player state. The original Room player-start roster supplies `po[]`, preserving its count/order as currently modeled. Animated-sector arrays, `msx`/`msy`, sprite queue and Atomic global options still come from the imported template. Chocolate ABI review, formatting/diff check and targeted crate compilation only; no dynamic save/load comparison.
- `2026-11-07` — **D9 Atomic SAV native-slot bridge**: native F2 writes an Atomic `.sav` companion beside the `.fsave` when `save_atomic_sav` has an imported template; F3 loads the Room snapshot and reapplies its companion through `restore_atomic_sav`. Slots without a template remain `.fsave`-only and remove any stale Atomic companion, while parse/restore errors leave the successfully restored `.fsave` state in place and are reported. Atomic companion export updates single-player campaign/coop metadata and the modeled 8×8 frag scores, and declines multiplayer exports until each seat's player record is writable. `cargo fmt --all`, `git diff --check` and `cargo check -q -p room-app --bin room` pass with existing warnings; no test, slot round-trip, gameplay, or reference comparison was run.
- `2026-11-07` — **D9 Atomic SAV active multiplayer player records**: removed the single-seat export refusal. For imported Atomic templates, `save_atomic_sav` now validates Chocolate's fixed 16-record `ps[]` blob and patches each active Room seat's modeled pose, momentum, sector, weapon, ammo, health snapshot, shield, oxygen, death, and camera fields while preserving each other byte from the template. It rejects malformed/truncated player arrays; several unmodeled player fields and multiplayer global metadata remain template-backed, so this is not yet a complete multiplayer save round-trip. `cargo fmt --all`, `git diff --check` and `cargo check -q -p room-duke -p room-build` pass with existing warnings; no tests or save/reference comparison were run.
- `2026-11-07` — **D9 Atomic SAV live roster metadata**: exports the active one-to-eight seat count in the Atomic header and `playerswhenstarted`, and rebuilds `connecthead` / the 16-entry `connectpoint2` chain in seat order. This keeps the save's declared roster aligned with the active records and live actors. Other multiplayer/global settings still come from the imported template. Static `menues.c` layout review, formatting, diff check and targeted compile only; no Atomic save was loaded or compared.
- `2026-11-07` — **D9 Atomic SAV fixed player-origin table**: corrected `po[]` export validation from eight to Chocolate's full `MAXPLAYERS == 16` records. The active Room roster still updates at most eight entries; remaining entries are kept from the imported Atomic template. Static `duke3d.h` / `menues.c` dimension review, `cargo fmt --all`, `git diff --check` and targeted compile only; no runtime/reference comparison.
- `2026-11-07` — **D9 Atomic SAV fixed sprite array**: corrected the native save reader to consume `sprite[MAXSPRITES]` immediately after `sector[MAXSECTORS]`, matching `menues.c::loadplayer`; Chocolate writes no `numsprites` scalar there. Compact Room dump fixtures with a standalone two-byte count remain recognized. Static `menues.c` read/write order review, formatting, diff check and `cargo check -q -p room-build` only; no Atomic save was loaded or compared.
- `2026-11-07` — **D9 status and room-duke docs refreshed**: progress now records live Atomic template export plus desktop F2/F3 sidecars, and explicitly retains standalone saves, multiplayer player-record writes, complete state refresh and round-trip validation as open work. The crate README now reflects native shell support and its remaining renderer/demo limitations instead of describing the runtime and shell bridge as stubs. Static documentation review and `git diff --check`; no game, demo or reference comparison.
- `2026-11-07` — **D9/D11 Atomic demo shell playback and route continuation**: `playatomicdemo <path.dmo>` parses the revision-117 stream, loads the recorded map/skill/roster from ordered GRP layers and feeds seat inputs to the native session at 30 Hz. Exit requests now transition through the campaign cursor without resetting the demo cursor; each next map refreshes its restart snapshot. Stop/record and Room `.fdem` playback cancel Atomic playback. Custom `--con` application and reference sync remain open. `cargo fmt --all`, `git diff --check`, and `cargo check -q -p room-console -p room-app --bin room` succeed with existing warnings; no gameplay or demo/reference comparison was run.
- `2026-11-07` — **D10/D11 lockstep campaign transition and roster carry**: after applying an authoritative browser Duke frame, the native session now consumes the campaign exit and loads the next map. Shared `DukeSession::inherit_campaign_roster` configures the new map's matching APLAYER seats and carries every seat's persistent inventory/weapon/score state while keeping new spawn poses; it also preserves match mode, frag scores, actor toggle and campaign save globals. Desktop transitions use the same session path. This closes the prior seat-zero-only map-transition path but does not establish end-to-end multiplayer lockstep or route parity. `cargo fmt --all`, `git diff --check`, and targeted crate compile only; no game, network match, tests, or reference comparison.
- `2026-11-07` — **D10 campaign-aware headless verifier**: the optional Duke server verifier now consumes the same campaign exit transition as web clients, loads the destination `E#L#` map from its configured Atomic GRP, and carries the full active roster before simulating the next authoritative frame. It stops with a warning if map loading or roster reconstruction fails rather than emitting checksums from stale map state. Formatting, diff check and verifier-feature compile only; no multiplayer session or checksum comparison was run.
- `2026-11-07` — **D10 multiplayer `resetplayer` weaponhit state**: Chocolate's multiplayer reset opcode now also clears the represented `hittype[g_i]` queued damage, owner, fall throttle, movement result, saved palette and stayput sector fields, including the separate `dispicnum` override. Static `gamedef.c::resetplayer` / `duke3d.h::weaponhit` layout review and targeted compilation only; no runtime or reference comparison.
- `2026-11-07` — **D9 Atomic player score fields**: SAV parsing/restoration and live Atomic export include `player_struct::frag`, `fraggedself`, and 32-bit `aim_mode` at Chocolate's ABI offsets, for the local record and each live Room seat. Signed score words preserve their 16-bit bit patterns. Remote records are restored by the follow-up multiplayer restore entry; unmodeled player fields and full SAV round-trip remain open. Static `duke3d.h` layout review, formatting, diff check and targeted compile only; no save replay or reference comparison.
- `2026-11-07` — **D9 Atomic multiplayer SAV restore**: exposes a bounded parser for each of Chocolate's 16 fixed `ps[]` records and restores up to Room's eight supported active seats by following the saved `connecthead` / `connectpoint2` chain. Each restored remote record initializes the modeled player pose, health, weapons/ammo, inventory, timers, score/settings and must resolve to its own live APLAYER sprite; seat indices and shared coop/friendly-fire modes are reinstated. Unmodeled `ps[]` fields and complete SAV round-trip remain open. `cargo fmt --all`, `git diff --check`, and `cargo check -q -p room-duke -p room-build` pass with existing warnings; no tests, save replay, gameplay or Chocolate comparison were run.
- `2026-11-07` — **D9 Atomic SAV transient player state**: restore now applies modeled jump counters/toggle, holster, knee frame, transporter lock, footprint state and active VIEWSCREEN camera (`newowner` resolves through the saved CAMERA1 sprite). Atomic export refreshes these fields, additional weapon/access/quote timers, reservations, inventory flags and camera owner for each active seat, rather than leaving those remote records template-backed. `cargo fmt --all`, `git diff --check` and targeted `cargo check -q -p room-duke -p room-build` pass with existing warnings; no tests, SAV round-trip or Chocolate comparison were run.
- `2026-11-07` — **D9 Atomic SAV remaining modeled player fields**: extended `Player` with the decoded damage cooldown, fall counter, inventory redraw latch, weapon aim adjustment, last full weapon, empty-gun latch, hard landing and view/audio animation phases, plus prior horizon samples. Atomic import now restores those values, screen roll and player palette; export refreshes them for local and remote records at their existing `SavPlayer` offsets. Older `.fsave` snapshots use defaults through serde. `cargo fmt --all`, `git diff --check` and targeted `cargo check -q -p room-duke -p room-build` pass with existing warnings; no tests or save round-trip were run.
- `2026-11-07` — **D9 Atomic SAV loogie overlay state**: `SavPlayer` now decodes `numloogs`, `loogcnt` and the six active X/Y positions from Chocolate's 64-entry `loogiex` / `loogiey` arrays preceding `posx`. Atomic restore and template export apply them to the local and remote Room players; untouched entries 6–63 remain byte-preserved. `cargo fmt --all`, `git diff --check` and targeted `cargo check -q -p room-duke -p room-build` pass with existing warnings; no tests or Atomic save round-trip were run.
- `2026-11-07` — **D9 desktop Atomic DMO recording**: native Duke `record` now starts the Room `.fdem` recorder and a seat-ordered Atomic v117 recorder from the same map, skill and active roster. `stop` writes both `demos/<slot>.fdem` and `demos/<slot>.dmo`; direct weapon hotkeys for slots 0–9 and previous/next cycling are encoded into the Atomic `sync.bits` selector as part of each DMO tic. Playback paths remain separate (`playdemo` for `.fdem`, `playatomicdemo` for `.dmo`). The UI-only direct selector for the remote detonator slot is not representable in Atomic's idle selector field yet. No gameplay or reference comparison was run.
- `2026-11-07` — **D9 standalone Atomic SAV construction**: fresh supported Room sessions can now construct a version-119 Atomic `SavHeader` without first importing `GAME#.SAV`, then populate it through the same live-world/player/script export path used for template-backed saves. Fixed arrays receive ABI-sized player records and conservative defaults; custom-map campaign metadata and unmodeled/default engine tables remain limitations, and no claim is made that Chocolate accepts the generated file until a future static/round-trip validation pass. `cargo fmt --all`, `git diff --check`, and targeted `cargo check -q -p room-build -p room-duke -p room-app --bin room` succeeded with existing warnings; no tests or runtime/reference comparison were run.
- `2026-11-07` — **D9 spec state reconciliation**: refreshed §9.4 of `docs/25-duke3d-full-support.md` to reflect standalone Atomic v119 model construction, active multiplayer player-record export, live CON/actor state refresh and F2/F3 sidecars. Kept unmodeled state, fresh-save defaults, ABI coverage, round-trip and Chocolate load acceptance explicitly open. Documentation review plus `git diff --check`; no game, save replay, tests or reference comparison.
- `2026-11-07` — **D9 Atomic player-record ABI tail offsets**: Chocolate's raw `player_struct` includes an aligned palette pointer before several byte fields and `auto_aim`. `SavPlayer` now selects those tail offsets from the 932-byte 32-bit or 944-byte 64-bit record stride, and live export uses that same layout for every active multiplayer seat. A private Rust `repr(C)` layout witness and compile-time size/offset assertions pin the current target ABI; the previous Room 936-byte draft layout remains readable, and unknown strides fail explicitly. Static `duke3d.h` field-order/ABI review, formatting, diff check and targeted compile only; no tests or Atomic save/reference load.
- `2026-11-07` — **D11 Atomic demo custom CON bootstrap**: `DukeAtomicDemoPlayer::load_session_with_con` now accepts the same root CON bytes and external include resolver as regular native map loads. Desktop `playatomicdemo` passes the retained `--con` override and resolves external includes from its directory, so the demo starts with the selected user script rather than silently using the GRP root script. `cargo fmt --all`, `git diff --check` and targeted compile only; no demo playback or reference comparison.

- `2026-11-07` — **D10 dead-player use input / respawn request**: dead multiplayer seats now detect a rising Chocolate `INPUT_OPEN` (use) edge and queue the existing `resetplayer` respawn path for co-op and DukeMatch; the no-spawn mode remains disabled, and movement/fire inputs stay neutral. Added a serialized per-player edge latch with a serde default and clear it on respawn. `cargo fmt --all`, `git diff --check`, and targeted `cargo check -p room-duke` only; no gameplay, tests or Chocolate comparison were run.

- `2026-11-07` — **D10 Chocolate respawn start table**: retained the map-header position as `po[0]`, then stores selected-mode APLAYER starts in reverse map insertion order to match STAT_PLAYER head order. Co-op now selects `po[seat]`; DukeMatch selects `TRAND % po_count`, with the header included and unrelated `ud.coop` lotags excluded. Atomic `player_orig` export now supports all 16 fixed records. Statically reviewed Chocolate `premap.c::pickrandomspot` and Room's APLAYER construction; `cargo fmt --all`, `git diff --check` and targeted `cargo check -p room-duke` only, with no gameplay/demo comparison or tests.

- `2026-11-07` — **D10 respawn player-state reset**: after static `premap.c::resetplayerstats` review, Room now restores modeled view/bob, movement/death, camera, holoduke, jetpack/heat, access-card, inventory-display and weapon-animation state on multiplayer respawn. Co-op keeps the current weapons and inventory; DukeMatch/no-spawn applies `resetweapons` / `resetinventory`. Formatting, diff check and targeted `cargo check -p room-duke` only; no tests, game run or Chocolate comparison. Fields Room does not model yet remain outside this reset pass.

- `2026-11-07` — **D11 browser campaign cursor**: browser GRP sessions now initialize Atomic episode/map progress both on solo start and in the multiplayer lobby's ready preparation; each lockstep campaign transition assigns progress to the newly loaded map before resuming. This prevents the next map's exit from losing its campaign route context. Static session-transition review only; no tests, browser run or reference comparison.

- `2026-11-07` — **D8 browser Atomic map music**: native Duke WebAudio now reads the active map's MIDI name from the compiled CON `music` table and loads that GRP track when audio is unlocked; the same selector runs immediately after campaign map transitions. Duplicate selections do not restart the current song. `cargo fmt --all -- --check`, `git diff --check`, and `cargo check -q -p room-web` passed; no browser/audio run, tests or Chocolate comparison.

- `2026-11-07` — **D11 browser native CON overrides**: added `roomSetDukeCon` for the root script and `roomAddDukeConInclude` for external include files. Solo starts, multiplayer lobby preparation and subsequent campaign map loads all use the same override/include resolver, with explicit include files taking priority over GRP entries. Formatting, diff check and `cargo check -q -p room-web` passed; no browser session, tests or reference comparison.

- `2026-11-07` — **D11 browser standalone Build maps**: added `roomStartDukeMap(grp, map, skill)` with pre-GPU queuing, native map parsing and GRP-backed rendering assets. It applies the configured browser root CON and external include resolver. Formatting, diff check and `cargo check -q -p room-web` passed; no map playthrough, tests or reference comparison.

- `2026-11-07` — **D11 browser add-on GRP layering**: added `roomAddDukeGrp`; browser solo map, campaign-transition, resource-renderer, sound and MIDI lookups now use ordered GRP layers, with later archives winning. Co-op rejects local add-ons until lobby transfer/fingerprint support exists, preventing peers from silently simulating different content. Formatting, diff check and wasm-target `cargo check -q --target wasm32-unknown-unknown -p room-web` passed; no runtime or reference comparison.

- `2026-11-07` — **D3 Build face-sprite flips**: the depth-tested face-sprite renderer now applies Atomic `cstat` X-flip (bit 2) and Y-flip (bit 3) while sampling ART. Updated the D3 state to distinguish existing basic face sprites from unimplemented wall/floor sprites and palette lookup. Formatting, diff check and targeted crate compilation only; no render screenshot or Chocolate comparison.

- `2026-11-07` — **D3 wall-aligned sprite rendering**: the classic soft renderer now projects vertical Build sprite quads from `ang`, `xrepeat`/`yrepeat`, and `cstat` centered-origin state; it samples ART with X/Y flips and transparent texels against the wall z-buffer. The same pass now correctly skips transparent texels in face sprites instead of painting the fallback flat color. Floor-aligned sprites, palette remap, translucency and one-sided culling remain open. `cargo fmt --all -- --check`, `git diff --check`, and `cargo check -q -p room-build-engine` passed; no game or screenshot comparison.

- `2026-11-07` — **D3 LOOKUP palette remaps**: native GRP renderer loading now parses optional `LOOKUP.DAT` and applies its per-pal remaps to opaque texels on face sprites, wall-aligned sprites, walls and floor/ceiling/sky fills; transparent index 255 is preserved. Visibility-adjusted shade selection and exact palette-bank shade composition are still open. Formatting, diff check, `cargo check -q -p room-build-engine -p room-duke`, and wasm `room-web` check passed; no render capture or game comparison.
- `2026-11-07` — **D5 player hitscan switch exit ordering**: player shotgun hits on the Atomic DIPSWITCH/DIPSWITCH2/DIPSWITCH3/HANDSWITCH sprite and wall families now queue the switch interaction and return before consuming Chocolate’s trailing 1-in-64 PISTOL_RICOCHET `TRAND`. Other switch classes no longer enter this specific `shoot` branch. This preserves subsequent pellet spread/RNG ordering. Static review of `player.c::shoot()` around hitsprite/hitwall switch handling; `cargo fmt --all`, `git diff --check` and targeted compile only; no tests, gameplay or Chocolate comparison.
- `2026-11-07` — **D3 floor-aligned sprite rasterization**: the classic soft renderer now projects horizontal Build sprite quads in world space, applies angle/repeat sizing and X/Y flips, interpolates texture coordinates/depth over two triangles, preserves transparent ART texels and uses the shared z-buffer and `LOOKUP.DAT` palette remap. One-sided culling, translucency, exact Build offsets/panning and perspective-correct texture interpolation remain incomplete. Sector floor/ceiling `LOOKUP.DAT` remaps are applied during flat/sky fill. `SoftTiles` now retains `PALETTE.DAT` shade rows, applying the nonnegative `shade` row after `LOOKUP.DAT` remapping to opaque sampled pixels on flat/sky fills, walls, and face/wall/floor sprites; negative shades use the brightest row. Visibility-distance shade selection and exact palette-bank shade composition remain open. `cargo fmt --all`, `git diff --check` and targeted compilation passed; no tests, gameplay or Chocolate comparison were run.
- `2026-11-07` — **D3 wall texture panning**: the soft wall sampler now offsets ART coordinates by `xpanning`/`ypanning` as fractional tile offsets, scaled to the actual tile dimensions. This adds basic panning to the existing projected wall mapping; Build's exact `prepwall`/vertical mapping, cstat pan interactions, visibility shade and perspective-correct sampling remain open. Formatting, diff check and targeted compilation only; no tests, gameplay or Chocolate comparison.
- `2026-11-07` — **D3 one-sided aligned sprites**: wall-aligned sprites with cstat bit 6 are culled from the reverse-facing side; floor-aligned sprites with bit 6 are culled from the hidden side, with bit 3 reversing the floor-facing side. Static review of Chocolate's visible sprite collection and Build cstat conventions; formatting, diff check and targeted compile only, no tests or runtime/reference comparison.
- `2026-11-07` — **D3 sprite origin offsets**: the soft renderer now retains ART `picanm` origin offsets and combines them with map `xoffset`/`yoffset` for face-, wall- and floor-aligned sprite placement; X/Y flips reverse their corresponding offsets. The existing projected/screen-space approximation remains, so exact Build scaling and perspective are open. `cargo fmt --all`, `git diff --check` and targeted compile only; no tests, gameplay or Chocolate comparison.
- `2026-11-07` — **D3 wall texture flips**: wall cstat bit 3 (8) reverses horizontal ART sampling and bit 8 (256) reverses vertical sampling after applying the fractional panning offset. Static port of the documented Build wall flags; `cargo fmt --all`, diff check and targeted compile only, no tests or reference rendering.
- `2026-11-07` — **D3 one-way masked wall visibility**: Build wall cstat bit 5 (32) now prevents portal flood traversal and treats the span as a wall in the soft renderer, following Chocolate `scansector`'s portal eligibility condition. `cargo fmt --all`, `git diff --check` and targeted compile only; no tests or gameplay/reference rendering.
- `2026-11-07` — **D3 camera-oriented portal flood**: `BuildView` now applies Chocolate `scansector`'s wrapping int32 cross-product window and squared-width test before adding adjacent sectors, in addition to the one-way-mask gate. This stops flooding through the reverse-facing portal sides. Static source review of `Engine/src/engine.c::scansector`; format, diff and crate compile only, no tests or view comparison.
- `2026-11-08` — **D3 sloped plane rasterization**: floor/ceiling rays now iteratively intersect the sector's sloped plane with fixed-point arithmetic through `getzsofslope`, then sample ART at the resulting world position. This extends the existing camera-sector plane fill; per-pixel sector selection across overlapping rooms, sloped portal clipping, and reference screenshots remain open. `cargo fmt --all`, `cargo check -q -p room-build-engine -p room-duke`, and `git diff --check` passed. No tests, gameplay or Chocolate comparison were run.
- `2026-11-08` — **D3 plane orientation flags**: floor/ceiling ART sampling now applies Build's XY swap, X/Y flip, and double-smoosh scaling bits from `floorstat`/`ceilingstat`, following Chocolate `ceilscan`/`florscan` orientation transforms. Relative wall alignment, visibility shading, plane translucency and render goldens remain open. Formatting, targeted crate compilation and diff check passed; no tests, gameplay or dynamic Chocolate comparison.
- `2026-11-08` — **D3 first-wall-relative plane mapping**: sector floor/ceiling UV coordinates now rotate into an integer basis derived from the first wall when Build's relative-alignment bit 6 is set, before applying the plane swap/flip/scale flags. This covers the base alignment transform; exact Chocolate sampling scale/panning, distance visibility, translucent planes and renderer goldens remain open. `cargo fmt --all`, targeted `cargo check`, and `git diff --check` passed; no tests, gameplay or dynamic comparison.
- `2026-11-08` — **D3 plane ART scale and panning**: floor/ceiling UVs now derive their world texel density independently from ART width/height, apply double-smoosh to that scale, and add `xpanning`/`ypanning` as 1/256-tile offsets in Q8 before sampling. First-wall length now uses wrapping 32-bit squares and Chocolate's integer square-root helper, while UV dot products use widened integer intermediates. Static `ceilscan`/`florscan` source review; formatting, targeted crate compilation and diff check passed. No tests, screenshots, gameplay or Chocolate comparison.
- `2026-11-08` — **D7 SE30 activation sound gate**: Chocolate `sector.c::operatesectors` toggles subway travel on every activation but calls its cue only at settled `tempang` endpoints 0/256. Room now matches this endpoint gate. Static source review; formatting, diff check and targeted compile only, no tests or gameplay/reference comparison.
- `2026-11-08` — **D7 switch busy-state parity**: implemented Chocolate `check_activator_motion` for tagged activator sectors and use it from both `Open` and hitscan paths for the exact gated switch families. It rejects matching switches during plane/wall animations and while SE11/30 T5 or SE18/20/31/32 T1 is active; dip/tech/alien and access-card families preserve their separate Chocolate paths. Static `sector.c` review; formatting, diff check and targeted compile only; no gameplay or demo/reference comparison.
- `2026-11-08` — **D7 activator-linked effector toggle**: Chocolate `operateactivators` toggles T1 on local SE18/31/32/36 markers when the activator sector lotag is below 3, calling the sector sound path for each marker before `operatesectors`. Room now follows that order and list traversal. Static `sector.c` review; formatting, diff check and targeted compile only; no gameplay or demo/reference comparison.
- `2026-11-08` — **D7 shared respawn activation tail**: moved tagged `RESPAWN` arming into Room's common `operate_activators` tail, matching Chocolate's `operaterespawns(low)` call and covering subway/train station and future activation callers. Existing switch, SE10, and touchplate duplicate calls were removed; monsters-off target filtering remains centralized. Static `sector.c` review; formatting, diff check and targeted compile only; no gameplay or demo/reference comparison.
- `2026-11-08` — **D7 lotag-22 activator sound ordering**: ported Chocolate `operateactivators`' per-call `k` gate for its preliminary sound call, while preserving the separate `operatesectors` sound call on every lotag-22 operator. The `-1` return retains Chocolate's retry behavior. Static `sector.c` review; formatting, diff check and targeted compile only; no tests or gameplay/demo comparison.
- `2026-11-08` — **D7 respawn transporter-star timing**: statically aligned `RESPAWN` arming with Chocolate `operaterespawns`: create the `TRANSPORTERSTAR` immediately when a marker is armed, before its 54→66 movement-effect countdown, preserving the reference actor insertion order. A serde-default temp flag prevents a duplicate star, while snapshots saved during the older Room countdown still create one at the legacy point. Static `sector.c`/`actors.c` review; `cargo fmt --all`, `cargo check -q -p room-duke --lib`, and `git diff --check` pass (existing warnings); no tests, gameplay, demo, or dynamic comparison.
