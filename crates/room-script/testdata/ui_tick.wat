;; UI-scope UiTick handler sample (M11).
(module
  (import "room" "reg_event_handler" (func $reg_event_handler (param i32 i32 i32 i32 i32 i32 i32) (result i32)))
  (memory (export "memory") 1)
  (global $frames (mut i32) (i32.const 0))
  (data (i32.const 0) "HudOverlay")
  (func (export "room_abi_version") (result i32) (i32.const 1))
  (func (export "room_alloc") (param i32 i32) (result i32) (i32.const 64))
  (func (export "room_free") (param i32 i32 i32))
  (func (export "room_register") (result i32)
    ;; UI_TICK = 1<<16 → mask_lo=65536 mask_hi=0; callback 50
    (call $reg_event_handler
      (i32.const 0) (i32.const 10)
      (i32.const 0) (i32.const 0)
      (i32.const 65536) (i32.const 0)
      (i32.const 50)))
  (func (export "room_init") (param i32) (result i32) (i32.const 0))
  (func (export "room_dispatch") (param $id i32) (param $ptr i32) (param $len i32) (result i64)
    (if (result i64) (i32.eq (local.get $id) (i32.const 50))
      (then
        (global.set $frames (i32.add (global.get $frames) (i32.const 1)))
        (i64.extend_i32_s (global.get $frames)))
      (else (i64.const 0))))
)
