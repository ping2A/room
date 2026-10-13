# ferrite-testkit

UDMF map builder DSL for scenario tests.

```rust
use ferrite_testkit::{DoorSpec, MapBuilder, rect};

let map = MapBuilder::new()
    .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
    .room("b", rect(256.0, 0.0, 512.0, 256.0), 0.0, 128.0)
    .door_between("a", "b", DoorSpec::default())
    .player_start(64.0, 128.0, 0)
    .build_udmf("MAP01")?;
```

Adjacent rooms share a two-sided linedef; `door_between` sets Hexen special 12
(`Door_Raise`) with `playeruse` / `repeatspecial`.
