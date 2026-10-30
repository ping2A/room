;; BE2: MAP widget on WorldLoaded.
(module
  (import "room" "reg_event_handler" (func $reg_event_handler (param i32 i32 i32 i32 i32 i32 i32) (result i32)))
  (import "room" "hud_widget_set" (func $hud_widget_set (param i32 i32 i32 i32) (result i32)))
  (memory (export "memory") 1)
  (data (i32.const 0) "MapHud")
  (data (i32.const 64)
    "\4c\00" ;; size = 76
    "\07"    ;; MAP
    "\01"    ;; OVER_HUD
    "\02"    ;; TOP_RIGHT
    "\00"    ;; align
    "\00"    ;; font
    "\00"    ;; flags
    "\b8\ff" ;; x = -72
    "\08\00" ;; y = 8
    "\40\00" ;; w = 64
    "\30\00" ;; h = 48
    "\00\00" ;; z
    "\ff\ff\ff\ff" ;; color
    "\18\10\10\ff" ;; color2
    "\ff"    ;; text_color
    "\00\00\00" ;; pad
    "\b0\04\00\00" ;; value = 1200
    "\03\00\00\00" ;; max layers
    "\00\00" ;; segments
    "\e8\03" ;; alpha
    "\00\00" ;; fade_in
    "\00\00" ;; hold
    "\00\00" ;; fade_out
    "\00\00\00\00\00\00\00\00" ;; actor
    "\00\00\00\00\00\00\00\00\00\00\00\00" ;; world_xyz
    "\00\00" ;; name_off
    "\00\00" ;; name_len
    "\00\00" ;; text_off
    "\00\00" ;; text_len
  )
  (func (export "room_abi_version") (result i32) (i32.const 1))
  (func (export "room_alloc") (param i32 i32) (result i32) (i32.const 256))
  (func (export "room_free") (param i32 i32 i32))
  (func (export "room_register") (result i32)
    (if (i32.ne
          (call $reg_event_handler
            (i32.const 0) (i32.const 6)
            (i32.const 0) (i32.const 0)
            (i32.const 8) (i32.const 0)
            (i32.const 100))
          (i32.const 0))
      (then (return (i32.const 1))))
    (i32.const 0))
  (func (export "room_init") (param i32) (result i32) (i32.const 0))
  (func (export "room_dispatch") (param $id i32) (param $ptr i32) (param $len i32) (result i64)
    (if (i32.eq (local.get $id) (i32.const 103))
      (then
        (drop (call $hud_widget_set (i32.const 1) (i32.const 50) (i32.const 64) (i32.const 76)))
        (return (i64.const 1))))
    (i64.const 0))
)
