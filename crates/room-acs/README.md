# room-acs

ACS bytecode loader and virtual machine (`BEHAVIOR` lumps).

## Public API (M7 bootstrap)

- [`load_behavior`] — parse ACS0 / ACSE / ACSe (directory) / hybrid; compressed ACSe opcodes still expand at run time later
- [`AcsState`] — per-level script runtime (map/world/global vars, running scripts)
- [`AcsHost`] — host callbacks for line specials / RNG while interpreting
- [`tick_scripts`] — advance delayed / running scripts one tic
- [`start_script`] / [`suspend_script`] / [`terminate_script`] — `ACS_Execute*` family

Opcode coverage is a Hexen-era subset (`PUSHNUMBER`, arith, vars, `GOTO`/`IFGOTO`,
`DELAY*`, `LSPEC*`, `TERMINATE`/`SUSPEND`) plus `CALLFUNC` / `SETRESULTVALUE`,
`FixedMul`/`FixedDiv`, `ThingCount`, `Spawn`/`SpawnSpot`, actor/sector queries
(`GetActor*` / `GetSector*` / `ActivatorTID`), and a first ACSF stub (`Sqrt` /
`FixedSqrt` / `VectorLength` / `Get*UDMF*` key resolve in-VM; `UniqueTID` /
`IsTIDUsed` / `GetPolyobjX|Y` / `GetActorVel*` / `CheckSight` via host). Full PCD /
ACSF tables land as M7 progresses — see `docs/10-acs.md`.
