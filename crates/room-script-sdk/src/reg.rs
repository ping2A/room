//! Registration helpers (`docs/12` §6).

use crate::sys;
use room_script_abi::ClassRef;

/// Begin a class (`reg_class_begin`).
#[must_use]
pub fn class_begin(
    name: &str,
    parent: ClassRef,
    replaces: ClassRef,
    doomednum: i32,
    flags: u32,
) -> ClassRef {
    // SAFETY: name slice lives for the import call.
    unsafe {
        ClassRef(sys::reg_class_begin(
            name.as_ptr() as i32,
            name.len() as i32,
            parent.0 as i32,
            replaces.0 as i32,
            doomednum,
            flags as i32,
        ) as u32)
    }
}

/// `reg_class_property_i32`.
pub fn class_property_i32(prop: u32, value: i32) {
    // SAFETY: scalar import.
    unsafe { sys::reg_class_property_i32(prop as i32, value) }
}

/// `reg_class_flag`.
pub fn class_flag(flag: u32, on: bool) {
    // SAFETY: scalar import.
    unsafe { sys::reg_class_flag(flag as i32, i32::from(on)) }
}

/// `reg_class_end`.
pub fn class_end() {
    // SAFETY: scalar import.
    unsafe { sys::reg_class_end() }
}

/// `reg_state_label`.
pub fn state_label(label: &str) {
    // SAFETY: label slice lives for the import call.
    unsafe { sys::reg_state_label(label.as_ptr() as i32, label.len() as i32) }
}

/// `reg_state` — `sprite` must be 4 bytes.
#[must_use]
pub fn state(sprite: &[u8; 4], frame: u8, tics: i32, flags: u32, callback_id: i32) -> u32 {
    // SAFETY: sprite pointer valid for call.
    unsafe {
        sys::reg_state(
            sprite.as_ptr() as i32,
            i32::from(frame),
            tics,
            flags as i32,
            callback_id,
        ) as u32
    }
}

/// `reg_state_flow` — [`room_script_abi::state_flow_kind`]; returns 0 on success.
pub fn state_flow(kind: i32, label: &str, offset: i32) -> i32 {
    // SAFETY: label slice lives for the import call.
    unsafe { sys::reg_state_flow(kind, label.as_ptr() as i32, label.len() as i32, offset) }
}

/// `reg_state_action` — native action on the last state; returns 0 on success.
pub fn state_action(name: &str, args: &str) -> i32 {
    // SAFETY: name/args slices live for the import call.
    unsafe {
        sys::reg_state_action(
            name.as_ptr() as i32,
            name.len() as i32,
            args.as_ptr() as i32,
            args.len() as i32,
        )
    }
}

/// `reg_state_offset` — weapon OFFSET(x,y) on the last state; returns 0 on success.
pub fn state_offset(x: i32, y: i32) -> i32 {
    // SAFETY: scalar import.
    unsafe { sys::reg_state_offset(x, y) }
}

/// `reg_class_virtual`.
pub fn class_virtual(slot: u32, callback_id: i32) {
    // SAFETY: scalar import.
    unsafe { sys::reg_class_virtual(slot as i32, callback_id) }
}

/// `reg_event_handler` — returns 0 on success.
pub fn event_handler(
    name: &str,
    order: i32,
    flags: u32,
    event_mask: u64,
    callback_base: i32,
) -> i32 {
    let lo = event_mask as u32 as i32;
    let hi = (event_mask >> 32) as u32 as i32;
    // SAFETY: name slice lives for the import call.
    unsafe {
        sys::reg_event_handler(
            name.as_ptr() as i32,
            name.len() as i32,
            order,
            flags as i32,
            lo,
            hi,
            callback_base,
        )
    }
}

/// `reg_thinker_class` — returns 0 on success.
pub fn thinker_class(name: &str, statnum: i32, callback_id: i32) -> i32 {
    // SAFETY: name slice lives for the import call.
    unsafe {
        sys::reg_thinker_class(
            name.as_ptr() as i32,
            name.len() as i32,
            statnum,
            callback_id,
        )
    }
}

/// `reg_static_function` — ACS `ScriptCall` target; returns 0 on success.
pub fn static_function(class_name: &str, func_name: &str, callback_id: i32) -> i32 {
    // SAFETY: name slices live for the import call.
    unsafe {
        sys::reg_static_function(
            class_name.as_ptr() as i32,
            class_name.len() as i32,
            func_name.as_ptr() as i32,
            func_name.len() as i32,
            callback_id,
        )
    }
}

/// `reg_lump` — register generated Doom patch / DMX / … bytes; 0 = ok.
pub fn lump(name: &str, namespace: i32, data: &[u8]) -> i32 {
    // SAFETY: name/data slices live for the import call.
    unsafe {
        sys::reg_lump(
            name.as_ptr() as i32,
            name.len() as i32,
            namespace,
            data.as_ptr() as i32,
            data.len() as i32,
        )
    }
}

/// `reg_sound_alias` — SNDINFO logical → lump (`limit` −1 = default); 0 = ok.
pub fn sound_alias(logical: &str, lump_name: &str, limit: i32) -> i32 {
    // SAFETY: string slices live for the import call.
    unsafe {
        sys::reg_sound_alias(
            logical.as_ptr() as i32,
            logical.len() as i32,
            lump_name.as_ptr() as i32,
            lump_name.len() as i32,
            limit,
        )
    }
}

/// `reg_weapon_slot` — KEYCONF `addslotdefault` (`docs/31` MA4); 0 = ok.
pub fn weapon_slot(slot: i32, class_name: &str, priority: i32) -> i32 {
    // SAFETY: class name slice lives for the import call.
    unsafe {
        sys::reg_weapon_slot(
            slot,
            class_name.as_ptr() as i32,
            class_name.len() as i32,
            priority,
        )
    }
}

/// `manifest_option_i32` — `[options]` integer (`docs/31` MA4b).
#[must_use]
pub fn option_i32(key: &str, default: i32) -> i32 {
    // SAFETY: key slice lives for the import call.
    unsafe { sys::manifest_option_i32(key.as_ptr() as i32, key.len() as i32, default) }
}

/// `manifest_option_str` — copy `[options]` string into `buf`; length or −1.
pub fn option_str(key: &str, buf: &mut [u8]) -> i32 {
    // SAFETY: key/buf live for the import call.
    unsafe {
        sys::manifest_option_str(
            key.as_ptr() as i32,
            key.len() as i32,
            buf.as_mut_ptr() as i32,
            buf.len() as i32,
        )
    }
}

/// `lump_find` → opaque index (≥ 0) or −1.
#[must_use]
pub fn lump_find(name: &str, namespace: i32) -> i32 {
    // SAFETY: name slice lives for the import call.
    unsafe { sys::lump_find(name.as_ptr() as i32, name.len() as i32, namespace) }
}

/// `lump_read` into `buf`; returns bytes written or −1.
pub fn lump_read(index: i32, buf: &mut [u8]) -> i32 {
    // SAFETY: buf pointer/len valid for the call.
    unsafe { sys::lump_read(index, buf.as_mut_ptr() as i32, buf.len() as i32) }
}

pub use crate::game::class_property_str;
