# ferrite-defs

Compiles text and binary definition lumps into immutable `Definitions`.

## Status

- `GAMEINFO` / MAPINFO / UMAPINFO / SNDINFO / … via `ferrite-sc`
- `COMPLVL` → [`Complevel`](crate::Complevel) on [`Definitions`](crate::Definitions)
- Depends on `ferrite-formats` for PLAYPAL / COLORMAP / TRANMAP

Full phase pipeline: `docs/05-text-lumps.md §2`.
