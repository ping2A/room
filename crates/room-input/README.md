# room-input

Device abstraction, bindings, and ticcmd building.

## Public API (M5 bootstrap)

- `InputSample` / `Key` — per-tic held keys + mouse deltas
- `build_ticcmd` / `build_ticcmd_with_mods` → `TicCmdBits` — maps into sim `TicCmd` in the game shell
- `ModButtonState` — `+mod1..+mod4` / `-mod1..-mod4` (bits 16–19, `docs/28` BE4); suggested defaults `bind g +mod1`, `bind m +mod2` (desktop keeps Q as turn)
- `AttackLatch` — window / device / key attack sources (macOS Locked cursor: never
  let RMB or a WindowEvent release cancel DeviceEvent LMB)
- `device_button_is_primary` — winit `DeviceEvent::Button` id for left (macOS/Win: 0)
