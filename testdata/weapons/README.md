# Weapon timeline reference tables (docs/30 WF0 / §1)

Each `*.txt` is a vanilla Doom fire/flash transcript used by
`room-sim` timeline tests. Times are in tics (35 Hz).

- **held** — tics between shots while fire is held (`A_ReFire` frame tics
  are *not* spent; the action jumps back to Fire when its frame starts).
- **full** — single tap from entering `Fire` through the `A_ReFire` frame
  back to `Ready` (includes the ReFire frame’s tics).

Frame lines: `SPRITE FRAME TICS [ACTION]` (frame letter `A`=0 …).
