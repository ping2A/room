# room-input

Device abstraction, bindings, and ticcmd building.

## Public API (M5 bootstrap)

- `InputSample` / `Key` — per-tic held keys + mouse deltas
- `build_ticcmd` → `TicCmdBits` — maps into sim `TicCmd` in the game shell
- `AttackLatch` — window / device / key attack sources (macOS Locked cursor: never
  let RMB or a WindowEvent release cancel DeviceEvent LMB)
- `device_button_is_primary` — winit `DeviceEvent::Button` id for left (macOS/Win: 0)
