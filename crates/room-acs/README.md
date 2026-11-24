# room-acs

ACS bytecode loader and virtual machine (`BEHAVIOR` lumps).

## Public API (M7 bootstrap)

- [`load_behavior`] — parse ACS0 / ACSE / ACSe (directory) / hybrid; compressed ACSe opcodes still expand at run time later
- [`AcsState`] — per-level script runtime (map/world/global vars, running scripts)
- [`AcsHost`] — host callbacks for line specials / RNG while interpreting
- [`tick_scripts`] — advance delayed / running scripts one tic
- [`start_script`] / [`suspend_script`] / [`terminate_script`] — `ACS_Execute*` family
- [`compile_acs`] / [`compile_acs_standalone`] — ACS source → `ACSE` `BEHAVIOR`
  (ACC syntax: scripts, functions, map / world / global variables and arrays,
  `#include` / `#define`, print items, `HudMessage`, `CreateTranslation`, the
  internal p-code functions, line specials and `CallFunc` functions; the
  standard `zcommon.acs` headers are built in, libraries / `#import` are not)

`cargo run -p room-acs --example acsc -- SCRIPTS.acs -I include-dir -o BEHAVIOR.o`
compiles from the command line. `tools/gen_acs_defs.py` regenerates the
built-in specials, functions and constants (`src/compile/defs.rs`).

Opcode coverage is a Hexen-era subset (`PUSHNUMBER`, arith, vars, `GOTO`/`IFGOTO`,
`DELAY*`, `LSPEC*`, `TERMINATE`/`SUSPEND`) plus `CALLFUNC` / `SETRESULTVALUE`,
`FixedMul`/`FixedDiv`, `ThingCount`, `Spawn`/`SpawnSpot`, actor/sector queries
(`GetActor*` / `GetSector*` / `ActivatorTID`), and a first ACSF stub (`Sqrt` /
`FixedSqrt` / `VectorLength` / `Get*UDMF*` key resolve in-VM; `UniqueTID` /
`IsTIDUsed` / `GetPolyobjX|Y` / `GetActorVel*` / `CheckSight` via host). Full PCD /
ACSF tables land as M7 progresses — see `docs/10-acs.md`.
