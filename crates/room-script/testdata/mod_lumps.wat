;; MA3: reg_lump + reg_sound_alias (`docs/31`).
(module
  (import "room" "reg_lump" (func $reg_lump (param i32 i32 i32 i32 i32) (result i32)))
  (import "room" "reg_sound_alias" (func $reg_alias (param i32 i32 i32 i32 i32) (result i32)))
  (memory (export "memory") 1)
  ;; name "TSTLUMP0" @0, logical "test/beep" @16, lump "DSTSTBEP" @32
  ;; 1×1 Doom patch @48: w=1 h=1 left=0 top=0 col@8 post(0,1,0,7,0) FF
  (data (i32.const 0) "TSTLUMP0")
  (data (i32.const 16) "test/beep")
  (data (i32.const 32) "DSTSTBEP")
  (data (i32.const 48) "\01\00\01\00\00\00\00\00\08\00\00\00\00\01\00\07\00\ff")
  ;; tiny DMX @80: type3, 11025 Hz, 36 bytes (16 pad + 4 pcm + 16 pad)
  (data (i32.const 80) "\03\00\11\2b\24\00\00\00")
  (func (export "room_abi_version") (result i32) (i32.const 1))
  (func (export "room_alloc") (param i32 i32) (result i32) (i32.const 256))
  (func (export "room_free") (param i32 i32 i32))
  (func (export "room_register") (result i32)
    ;; sprite namespace = 1
    (if (i32.ne (call $reg_lump (i32.const 0) (i32.const 8) (i32.const 1) (i32.const 48) (i32.const 18))
                (i32.const 0))
      (then (return (i32.const 1))))
    ;; sound namespace = 3; 8 header + 36 samples = 44
    (if (i32.ne (call $reg_lump (i32.const 32) (i32.const 8) (i32.const 3) (i32.const 80) (i32.const 44))
                (i32.const 0))
      (then (return (i32.const 1))))
    (if (i32.ne (call $reg_alias (i32.const 16) (i32.const 9) (i32.const 32) (i32.const 8) (i32.const -1))
                (i32.const 0))
      (then (return (i32.const 1))))
    (i32.const 0))
  (func (export "room_init") (param i32) (result i32) (i32.const 0))
  (func (export "room_dispatch") (param i32 i32 i32) (result i64) (i64.const 0))
)
