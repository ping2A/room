(module
  (import "room" "reg_event_handler" (func $reg_event_handler (param i32 i32 i32 i32 i32 i32 i32) (result i32)))
  (import "room" "player_name" (func $player_name (param i32 i32 i32) (result i32)))

  (memory (export "memory") 1)
  (data (i32.const 0) "NameMod")

  (func (export "room_abi_version") (result i32) (i32.const 1))
  (func (export "room_alloc") (param i32 i32) (result i32) (i32.const 64))
  (func (export "room_free") (param i32 i32 i32))
  (func (export "room_register") (result i32)
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

  (func (export "room_dispatch") (param i32 i32 i32) (result i64)
    (if (i32.eq (local.get 0) (i32.const 100))
      (then
        (drop (call $player_name (i32.const 1) (i32.const 64) (i32.const 32)))))
    (i64.const 0))
)
