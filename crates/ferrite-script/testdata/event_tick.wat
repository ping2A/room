;; EventHandler WorldTick sample (M11).
(module
  (import "ferrite" "reg_event_handler" (func $reg_event_handler (param i32 i32 i32 i32 i32 i32 i32) (result i32)))
  (import "ferrite" "reg_thinker_class" (func $reg_thinker_class (param i32 i32 i32 i32) (result i32)))
  (import "ferrite" "level_time" (func $level_time (result i32)))
  (memory (export "memory") 1)
  (global $ticks (mut i32) (i32.const 0))
  (data (i32.const 0) "TickCounter")
  (data (i32.const 16) "PulseThinker")
  (func (export "ferrite_abi_version") (result i32) (i32.const 1))
  (func (export "ferrite_alloc") (param i32 i32) (result i32) (i32.const 64))
  (func (export "ferrite_free") (param i32 i32 i32))
  (func (export "ferrite_register") (result i32)
    ;; event_mask WORLD_TICK = 1 → mask_lo=1 mask_hi=0; callback_base=100
    (if (i32.ne
          (call $reg_event_handler
            (i32.const 0) (i32.const 11)
            (i32.const 0) (i32.const 0)
            (i32.const 1) (i32.const 0)
            (i32.const 100))
          (i32.const 0))
      (then (return (i32.const 1))))
    (if (i32.ne
          (call $reg_thinker_class (i32.const 16) (i32.const 12) (i32.const 64) (i32.const 101))
          (i32.const 0))
      (then (return (i32.const 2))))
    (i32.const 0))
  (func (export "ferrite_init") (param i32) (result i32) (i32.const 0))
  (func (export "ferrite_dispatch") (param $id i32) (param $ptr i32) (param $len i32) (result i64)
    (if (result i64) (i32.eq (local.get $id) (i32.const 100))
      (then
        (global.set $ticks (i32.add (global.get $ticks) (i32.const 1)))
        (i64.extend_i32_s (global.get $ticks)))
      (else
        (if (result i64) (i32.eq (local.get $id) (i32.const 101))
          (then (i64.extend_i32_s (call $level_time)))
          (else (i64.const 0))))))
)
