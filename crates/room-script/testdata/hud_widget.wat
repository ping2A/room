;; MH2: create / patch / remove / clear Mod HUD widgets.
(module
  (import "room" "reg_event_handler" (func $reg_event_handler (param i32 i32 i32 i32 i32 i32 i32) (result i32)))
  (import "room" "hud_widget_set" (func $hud_widget_set (param i32 i32 i32 i32) (result i32)))
  (import "room" "hud_widget_patch" (func $hud_widget_patch (param i32 i32 i32 i32) (result i32)))
  (import "room" "hud_widget_remove" (func $hud_widget_remove (param i32 i32) (result i32)))
  (import "room" "hud_clear" (func $hud_clear (param i32) (result i32)))
  (memory (export "memory") 1)
  (global $step (mut i32) (i32.const 0))
  ;; name at 0: "HudMod"
  (data (i32.const 0) "HudMod")
  ;; Full header (76) + 4 newer bytes + "HELLO" at text_off 80. params_len = 85.
  ;; size = 80 so the host ignores the unknown tail and still reads text_off.
  (data (i32.const 64)
    "\50\00" ;; size = 80
    "\00"    ;; TEXT
    "\01"    ;; OVER_HUD
    "\01"    ;; anchor top-center
    "\00"    ;; align
    "\01"    ;; font BIG
    "\00"    ;; flags
    "\00\00" ;; x
    "\18\00" ;; y = 24
    "\00\00" ;; w
    "\00\00" ;; h
    "\00\00" ;; z
    "\ff\ff\ff\ff" ;; color
    "\00\00\00\00" ;; color2
    "\05"    ;; text_color Gold
    "\00\00\00" ;; pad
    "\01\00\00\00" ;; value = 1
    "\00\00\00\00" ;; max
    "\00\00" ;; segments
    "\e8\03" ;; alpha = 1000
    "\00\00" ;; fade_in
    "\00\00" ;; hold
    "\00\00" ;; fade_out
    "\00\00\00\00\00\00\00\00" ;; actor
    "\00\00\00\00\00\00\00\00\00\00\00\00" ;; world_xyz
    "\00\00" ;; name_off
    "\00\00" ;; name_len
    "\50\00" ;; text_off = 80
    "\05\00" ;; text_len = 5
    "\00\00\00\00" ;; newer fields (ignored)
    "HELLO")
  ;; Older 8-byte prefix: size=8, kind=FILL. Tail fields stay at host defaults.
  (data (i32.const 160) "\08\00\01\00\00\00\00\00")
  (func (export "room_abi_version") (result i32) (i32.const 1))
  (func (export "room_alloc") (param i32 i32) (result i32) (i32.const 256))
  (func (export "room_free") (param i32 i32 i32))
  (func (export "room_register") (result i32)
    ;; WORLD_LOADED (1<<3) | WORLD_TICK (1<<0) = 9
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
    (if (i32.eq (local.get $id) (i32.const 103)) ;; 100 + WORLD_LOADED bit index
      (then
        (drop (call $hud_widget_set (i32.const 1) (i32.const 7) (i32.const 64) (i32.const 85)))
        (drop (call $hud_widget_set (i32.const 1) (i32.const 8) (i32.const 160) (i32.const 8)))
        (return (i64.const 1))))
    (if (i32.eq (local.get $id) (i32.const 100)) ;; WORLD_TICK
      (then
        (global.set $step (i32.add (global.get $step) (i32.const 1)))
        (if (i32.eq (global.get $step) (i32.const 1))
          (then
            (drop (call $hud_widget_patch (i32.const 1) (i32.const 7) (i32.const 0) (i32.const 99)))))
        (if (i32.eq (global.get $step) (i32.const 2))
          (then
            (drop (call $hud_widget_remove (i32.const 1) (i32.const 7)))))
        (if (i32.eq (global.get $step) (i32.const 3))
          (then
            (drop (call $hud_clear (i32.const 1)))))
        (return (i64.const 2))))
    (i64.const 0))
)
