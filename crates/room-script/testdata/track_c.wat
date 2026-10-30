;; Track C C2–C5 smoke: set INVULNERABLE, bonus flash, quake, radius attack.
(module
  (import "room" "reg_event_handler" (func $reg_event_handler (param i32 i32 i32 i32 i32 i32 i32) (result i32)))
  (import "room" "player_actor" (func $player_actor (param i32) (result i64)))
  (import "room" "actor_set_flag" (func $actor_set_flag (param i64 i32 i32)))
  (import "room" "actor_flag" (func $actor_flag (param i64 i32) (result i32)))
  (import "room" "player_set_i32" (func $player_set_i32 (param i32 i32 i32)))
  (import "room" "actor_quake" (func $actor_quake (param i64 f64 i32 f64 f64)))
  (import "room" "actor_radius_attack" (func $actor_radius_attack (param i64 i64 i32 f64)))
  (memory (export "memory") 1)
  (data (i32.const 0) "TrackC")
  (global $ok (mut i32) (i32.const 0))
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
  (func (export "room_dispatch") (param $id i32) (param $ptr i32) (param $len i32) (result i64)
    (local $mo i64)
    (if (i32.eq (local.get $id) (i32.const 103)) ;; WORLD_LOADED
      (then
        (local.set $mo (call $player_actor (i32.const 1)))
        ;; FlagId INVULNERABLE = 10
        (call $actor_set_flag (local.get $mo) (i32.const 10) (i32.const 1))
        (if (i32.eqz (call $actor_flag (local.get $mo) (i32.const 10)))
          (then (return (i64.const 0))))
        ;; BONUSCOUNT = 8
        (call $player_set_i32 (i32.const 1) (i32.const 8) (i32.const 12))
        (call $actor_quake (local.get $mo) (f64.const 4.0) (i32.const 10) (f64.const 0.0) (f64.const 512.0))
        (call $actor_radius_attack (local.get $mo) (local.get $mo) (i32.const 0) (f64.const 64.0))
        (global.set $ok (i32.const 1))
        (return (i64.const 1))))
    (i64.const 0))
  (func (export "ok") (result i32) (global.get $ok)))
