;; MH3: capture weapon_select on WorldLoaded; read raw cmd each WorldTick.
(module
  (import "room" "reg_event_handler" (func $reg_event_handler (param i32 i32 i32 i32 i32 i32 i32) (result i32)))
  (import "room" "player_capture_input" (func $capture (param i32 i32)))
  (import "room" "player_cmd" (func $cmd (param i32 i32) (result i32)))
  (import "room" "player_sound" (func $sound (param i32 i32 i32 f64)))
  (memory (export "memory") 1)
  (data (i32.const 0) "CapMod")
  (data (i32.const 64) "menu/cursor")
  (func (export "room_abi_version") (result i32) (i32.const 1))
  (func (export "room_alloc") (param i32 i32) (result i32) (i32.const 256))
  (func (export "room_free") (param i32 i32 i32))
  (func (export "room_register") (result i32)
    ;; WORLD_LOADED|WORLD_TICK = 9, callback_base 100
    (if (i32.ne
          (call $reg_event_handler
            (i32.const 0) (i32.const 6)
            (i32.const 0) (i32.const 0)
            (i32.const 9) (i32.const 0)
            (i32.const 100))
          (i32.const 0))
      (then (return (i32.const 1))))
    (i32.const 0))
  (func (export "room_init") (param i32) (result i32) (i32.const 0))
  (func (export "room_dispatch") (param $id i32) (param $ptr i32) (param $len i32) (result i64)
    (if (result i64) (i32.eq (local.get $id) (i32.const 103)) ;; WORLD_LOADED
      (then
        ;; PlayerRef 1, mask WEAPON_SELECT|WEAPON_CYCLE|USE|ATTACK = 15
        (call $capture (i32.const 1) (i32.const 15))
        (call $sound (i32.const 1) (i32.const 64) (i32.const 11) (f64.const 1))
        (i64.const 1))
      (else
        (if (result i64) (i32.eq (local.get $id) (i32.const 100)) ;; WORLD_TICK
          (then
            (drop (call $cmd (i32.const 1) (i32.const 0x100)))
            (i32.store8 (i32.const 0x200) (i32.load8_u (i32.const 0x102)))
            (i64.const 2))
          (else (i64.const 0))))))
)
