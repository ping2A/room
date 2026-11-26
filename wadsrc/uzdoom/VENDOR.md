# Vendored UZDoom sources

Pinned for `docs/36` UZ0. Room compiles this tree in later milestones (UZ3);
it is not linked into the playsim yet.

| | |
|---|---|
| Upstream | <https://github.com/UZDoom/UZDoom> |
| Commit | `67bdb4b7dcc2ebf0021ee061ad1a024865d93b44` |
| Date | 2026-10-28 |
| Licence | GPL-3.0-or-later (same as Room). Code written before 2026 is also under `LicenseRef-ZDoom-Conditional` as stated in the upstream headers. |
| Paths | `zscript/` ← `wadsrc/static/zscript`, `zscript.txt` ← `wadsrc/static/zscript.txt`, `mapinfo/` ← `wadsrc/static/mapinfo` |

Room-only patches belong in `wadsrc/room/zscript/overrides/`, not in this tree.
Refresh with a sparse checkout of the same paths and replace these directories
in one change, then re-run `room parity --uzdoom <checkout>`.
