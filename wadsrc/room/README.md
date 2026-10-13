# room.pk3 source tree

Engine data packed into `room.pk3` (embedded at build time; also loadable from disk).

Layout mirrors GZDoom `wadsrc/static/` where practical:

- `actors/` — built-in DECORATE (Doom set first; Heretic/Hexen/Strife later)
- `mapinfo/` — game MAPINFO fragments
- `xlat/` — Doom/Heretic/Strife line-type translators (GPL data from GZDoom)
- `dehacked/` — vanilla DeHackEd index tables (things/sprites/sounds/weapons/ammo/codepointers)

Pack with `scripts/pack-room-pk3.sh` (produces `target/room.pk3`).
