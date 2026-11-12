# ADR 0007: Atomic RPG blast damage bands

## Context

The native RPG path needed a damage rule for impact. Atomic's `hitradius` in
3D Realms' GPL Duke game code uses four damage inputs and assigns one of three
randomized ranges based on distance: inner third `[hp3, hp4)`, middle third
`[hp2, hp3)`, and outer third `[hp1, hp2)`. The RPG caller supplies
`hp1 = strength >> 2`, `hp2 = strength >> 1`, `hp3 = strength - (strength >> 2)`,
and `hp4 = strength`. It filters by visibility and applies actor/player damage
and knockback. Chocolate Duke's compatibility header defines `TRAND` as
`krand()`, sharing Build's gameplay random stream.

References: 3D Realms' [GPL source release, `ACTORS.C` hitradius](https://github.com/commercial-game-sources/duke_nukem_3d/blob/master/source/ACTORS.C#L2892-L3190); Chocolate Duke's [`duke3d.h` `TRAND` definition](https://github.com/fabiensanglard/chocolate_duke3D/blob/master/Game/src/duke3d.h#L1983).

## Decision

Use the parsed RPG actor `extra` as blast strength, Atomic's `RPGBLASTRADIUS`
as the range, and the three integer damage bands above. Apply the blast to
visible badguys and players on impact. Advance the shared Build `krand` stream
for projectile strength, `hitradius`'s pre-scan offset and damage rolls, as in
Chocolate's `TRAND` macro. A proposed second C `rand()` stream caused the
DEMO1 trace to diverge at tic 0 and was removed. The current pass covers
STAT_ACTOR victims and player health; other status lists, all sprite damage
paths, and complete source-specific knockback remain follow-up work.

## Consequences

RPG impacts now damage targets using Atomic's distance bands instead of a
linear falloff, and the player can take self-damage. These hits are not yet
demo-synchronized, and the full `hitradius` scan remains incomplete. The
implementation and source provenance are recorded in `THIRD_PARTY.md`.
