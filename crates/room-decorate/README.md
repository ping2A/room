# room-decorate

DECORATE parser → `ClassDefIr` (M5 bootstrap).

```rust
use room_decorate::parse_decorate;
let ir = parse_decorate(include_str!("zombieman.txt"))?;
```

Currently: `actor` headers, flags, simple properties, `states` labels/lines/flow,
constant duration expressions, `random(a,b)` tics, and `{ A_Foo; A_Bar }` anon
state functions (flattened to 0-tic follow-up states), plus DecoBlock bootstrap
(`if`/`else`/`return state("Label")`, const-fold and `health < N` jumps). Full
`DecoExpr` trees and
property tables still open.
