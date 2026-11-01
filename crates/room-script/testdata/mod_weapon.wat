;; MA2: weapon + ammo props + WEAPON.NOALERT (`docs/31`).
(module
  (import "room" "class_find" (func $class_find (param i32 i32) (result i32)))
  (import "room" "reg_class_begin" (func $begin (param i32 i32 i32 i32 i32 i32) (result i32)))
  (import "room" "reg_class_property_i32" (func $prop (param i32 i32)))
  (import "room" "reg_class_property_str" (func $prop_str (param i32 i32 i32)))
  (import "room" "reg_class_flag" (func $flag (param i32 i32)))
  (import "room" "reg_class_end" (func $end))
  (import "room" "reg_state_label" (func $label (param i32 i32)))
  (import "room" "reg_state" (func $state (param i32 i32 i32 i32 i32) (result i32)))
  (import "room" "reg_state_flow" (func $flow (param i32 i32 i32 i32) (result i32)))
  (memory (export "memory") 1)
  (data (i32.const 0) "Clip")
  (data (i32.const 16) "QuietClip")
  (data (i32.const 32) "Pistol")
  (data (i32.const 48) "QuietGun")
  (data (i32.const 64) "Spawn")
  (data (i32.const 80) "CLIP")
  (data (i32.const 96) "QuietClip")
  (data (i32.const 112) "shuriken/up")
  (func (export "room_abi_version") (result i32) (i32.const 1))
  (func (export "room_alloc") (param i32 i32) (result i32) (i32.const 1024))
  (func (export "room_free") (param i32 i32 i32))
  (func (export "room_register") (result i32)
    (local $clip i32)
    (local $pistol i32)
    (local.set $clip (call $class_find (i32.const 0) (i32.const 4)))
    (local.set $pistol (call $class_find (i32.const 32) (i32.const 6)))
    (if (i32.eqz (local.get $clip)) (then (return (i32.const 1))))
    (if (i32.eqz (local.get $pistol)) (then (return (i32.const 1))))
    ;; QuietClip : Clip
    (drop (call $begin (i32.const 16) (i32.const 9) (local.get $clip) (i32.const 0) (i32.const -1) (i32.const 0)))
    (call $prop (i32.const 30) (i32.const 5))       ;; InventoryAmount
    (call $prop (i32.const 31) (i32.const 40))      ;; InventoryMaxAmount
    (call $prop (i32.const 32) (i32.const 10))      ;; AmmoBackpackAmount
    (call $prop (i32.const 33) (i32.const 80))      ;; AmmoBackpackMaxAmount
    (call $prop_str (i32.const 55) (i32.const 80) (i32.const 4)) ;; InventoryIcon CLIP
    (call $label (i32.const 64) (i32.const 5))
    (drop (call $state (i32.const 80) (i32.const 65) (i32.const -1) (i32.const 0) (i32.const -1)))
    (drop (call $flow (i32.const 1) (i32.const 0) (i32.const 0) (i32.const 0))) ;; Stop
    (call $end)
    ;; QuietGun : Pistol
    (drop (call $begin (i32.const 48) (i32.const 8) (local.get $pistol) (i32.const 0) (i32.const -1) (i32.const 0)))
    (call $prop_str (i32.const 50) (i32.const 96) (i32.const 9)) ;; WeaponAmmoType QuietClip
    (call $prop (i32.const 34) (i32.const 1))       ;; WeaponAmmoUse
    (call $prop (i32.const 35) (i32.const 10))      ;; WeaponAmmoGive
    (call $prop (i32.const 36) (i32.const 1600))    ;; WeaponSelectionOrder
    (call $prop (i32.const 37) (i32.const 50))      ;; WeaponKickback
    (call $prop (i32.const 38) (i32.const 8))       ;; WeaponSlotNumber
    (call $prop_str (i32.const 52) (i32.const 112) (i32.const 11)) ;; WeaponUpSound
    (call $flag (i32.const 25) (i32.const 1))       ;; WEAPON_NOALERT
    (call $end)
    (i32.const 0))
  (func (export "room_init") (param i32) (result i32) (i32.const 0))
  (func (export "room_dispatch") (param i32 i32 i32) (result i64) (i64.const 0))
)
