;; ACS ScriptCall target + acs_execute queue sample (M11).
(module
  (import "ferrite" "reg_static_function" (func $reg_static_function (param i32 i32 i32 i32 i32) (result i32)))
  (import "ferrite" "acs_execute" (func $acs_execute (param i32 i32 i32 i32) (result i32)))
  (memory (export "memory") 1)
  (data (i32.const 0) "Util")
  (data (i32.const 8) "Add")
  ;; three i32 args for acs_execute: 7, 8, 9
  (data (i32.const 32) "\07\00\00\00\08\00\00\00\09\00\00\00")
  (func (export "ferrite_abi_version") (result i32) (i32.const 1))
  (func (export "ferrite_alloc") (param i32 i32) (result i32) (i32.const 64))
  (func (export "ferrite_free") (param i32 i32 i32))
  (func (export "ferrite_register") (result i32)
    (call $reg_static_function
      (i32.const 0) (i32.const 4)
      (i32.const 8) (i32.const 3)
      (i32.const 200)))
  (func (export "ferrite_init") (param i32) (result i32) (i32.const 0))
  ;; 200 = Add(a,b) via payload_ptr/len; 201 = queue ACS script 1
  (func (export "ferrite_dispatch") (param $id i32) (param $ptr i32) (param $len i32) (result i64)
    (if (result i64) (i32.eq (local.get $id) (i32.const 200))
      (then (i64.extend_i32_s (i32.add (local.get $ptr) (local.get $len))))
      (else
        (if (result i64) (i32.eq (local.get $id) (i32.const 201))
          (then (i64.extend_i32_s
            (call $acs_execute (i32.const 1) (i32.const 0) (i32.const 32) (i32.const 3))))
          (else (i64.const 0))))))
)
