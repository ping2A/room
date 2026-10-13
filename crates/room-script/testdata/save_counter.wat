;; Play module with mutable global + optional on_save/on_load (M11 §10).
(module
  (memory (export "memory") 1)
  (global $counter (mut i32) (i32.const 0))
  ;; store counter mirror at mem[0..4] so memory snapshot restores it
  (func $sync (param $v i32)
    (i32.store (i32.const 0) (local.get $v)))
  (func (export "room_abi_version") (result i32) (i32.const 1))
  (func (export "room_alloc") (param i32 i32) (result i32) (i32.const 64))
  (func (export "room_free") (param i32 i32 i32))
  (func (export "room_register") (result i32) (i32.const 0))
  (func (export "room_init") (param $kind i32) (result i32)
    ;; NewGame clears; LoadGame keeps memory (host overwrites after)
    (if (i32.eqz (local.get $kind))
      (then
        (global.set $counter (i32.const 0))
        (call $sync (i32.const 0))))
    (i32.const 0))
  (func (export "room_on_load")
    (global.set $counter (i32.load (i32.const 0))))
  (func (export "room_on_save") (param $buf i32) (result i32)
    (i32.store (local.get $buf) (global.get $counter))
    (i32.const 4))
  (func (export "room_dispatch") (param $id i32) (param $ptr i32) (param $len i32) (result i64)
    (if (result i64) (i32.eq (local.get $id) (i32.const 1))
      (then
        (global.set $counter (i32.add (global.get $counter) (i32.const 1)))
        (call $sync (global.get $counter))
        (i64.extend_i32_s (global.get $counter)))
      (else
        (if (result i64) (i32.eq (local.get $id) (i32.const 2))
          (then (i64.extend_i32_s (global.get $counter)))
          (else (i64.const 0))))))
)
