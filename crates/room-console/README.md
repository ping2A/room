# room-console

Console, commands, CVars, and config persistence.

## Public API (M5 bootstrap)

- `Console::with_defaults` — engine CVars + `exec` (`echo`/`set`/`get`/`toggle`/`cvarlist`/`cmdlist`)
- Host cheats: `god`/`iddqd`, `noclip`, `give`, `map`/`changemap`, `kill` → `HostCommand`s in `ExecResult`
- `CvarStore` — typed bool/int/float/string values with `ARCHIVE` / `SERVERINFO` flags
- Config (M13): `write_ini` / `apply_ini` / `load_ini_file` / `save_ini_file`; console
  `writeconfig` / `exec` (gzdoom.ini-compatible `name value` + `bind`)
