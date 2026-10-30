(module
  (import "room" "reg_event_handler" (func $reg_event_handler (param i32 i32 i32 i32 i32 i32 i32) (result i32)))
  (import "room" "zone_set" (func $zone_set (param i32 i32 i32 i32 i32 i32 i32 i32 i32) (result i32)))
  (import "room" "zone_clear" (func $zone_clear (param i32) (result i32)))

  (memory (export "memory") 1)

  (data (i32.const 0) "ZoneMod")

  (func (export "room_abi_version") (result i32) (i32.const 1))
  (func (export "room_alloc") (param i32 i32) (result i32) (i32.const 64))
  (func (export "room_free") (param i32 i32 i32))
  (func (export "room_register") (result i32)
    (if (i32.ne
          (call $reg_event_handler
            (i32.const 0) (i32.const 7)
            (i32.const 0) (i32.const 0)
            (i32.const 8) (i32.const 0)
            (i32.const 100))
          (i32.const 0))
      (then (return (i32.const 1))))
    (i32.const 0))
  (func (export "room_init") (param i32) (result i32) (i32.const 0))

  (func (export "room_dispatch") (param i32 i32 i32) (result i64)
    (if (i32.eq (local.get 0) (i32.const 103))
      (then
        (drop (call $zone_set
          (i32.const 0)
          (i32.const 256) (i32.const 256) (i32.const 512)
          (i32.const 300) (i32.const 300) (i32.const 256)
          (i32.const -870544214) (i32.const 0)))
        (drop (call $zone_set
          (i32.const 1)
          (i32.const 100) (i32.const 100) (i32.const 64)
          (i32.const 0) (i32.const 0) (i32.const 32)
          (i32.const -16776961) (i32.const 0)))
        (drop (call $zone_set
          (i32.const 2)
          (i32.const 0) (i32.const 0) (i32.const 32)
          (i32.const 0) (i32.const 0) (i32.const 16)
          (i32.const -65281) (i32.const 0)))
        (drop (call $zone_set
          (i32.const 3)
          (i32.const 0) (i32.const 0) (i32.const 16)
          (i32.const 0) (i32.const 0) (i32.const 8)
          (i32.const -65536) (i32.const 0)))
        (drop (call $zone_set
          (i32.const 4)
          (i32.const 0) (i32.const 0) (i32.const 8)
          (i32.const 0) (i32.const 0) (i32.const 4)
          (i32.const -1) (i32.const 0)))
        (drop (call $zone_clear (i32.const 1)))
        (return (i64.const 1))))
    (i64.const 0)))