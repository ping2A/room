//! Raw `"room"` imports (wasm32) / host stubs (native tests).

#[cfg(target_arch = "wasm32")]
mod imp {
    #[link(wasm_import_module = "room")]
    unsafe extern "C" {
        pub fn log(level: i32, msg_ptr: i32, msg_len: i32);
        pub fn level_time() -> i32;
        pub fn player_count() -> i32;
        pub fn actor_exists(actor: i64) -> i32;
        pub fn name_intern(ptr: i32, len: i32) -> i32;
        pub fn class_find(ptr: i32, len: i32) -> i32;
        pub fn reg_class_begin(
            name_ptr: i32,
            name_len: i32,
            parent: i32,
            replaces: i32,
            doomednum: i32,
            flags: i32,
        ) -> i32;
        pub fn reg_class_property_i32(prop: i32, value: i32);
        pub fn reg_class_flag(flag: i32, on: i32);
        pub fn reg_class_end();
        pub fn reg_state_label(label_ptr: i32, label_len: i32);
        pub fn reg_state(
            sprite_ptr: i32,
            frame: i32,
            tics: i32,
            flags: i32,
            callback_id: i32,
        ) -> i32;
        pub fn reg_state_flow(
            kind: i32,
            label_ptr: i32,
            label_len: i32,
            offset: i32,
        ) -> i32;
        pub fn reg_state_action(
            name_ptr: i32,
            name_len: i32,
            args_ptr: i32,
            args_len: i32,
        ) -> i32;
        pub fn reg_state_offset(x: i32, y: i32) -> i32;
        pub fn reg_class_virtual(slot: i32, callback_id: i32);
        pub fn reg_event_handler(
            name_ptr: i32,
            name_len: i32,
            order: i32,
            flags: i32,
            mask_lo: i32,
            mask_hi: i32,
            callback_base: i32,
        ) -> i32;
        pub fn reg_thinker_class(
            name_ptr: i32,
            name_len: i32,
            statnum: i32,
            callback_id: i32,
        ) -> i32;
        pub fn reg_static_function(
            class_ptr: i32,
            class_len: i32,
            func_ptr: i32,
            func_len: i32,
            callback_id: i32,
        ) -> i32;
        pub fn reg_lump(
            name_ptr: i32,
            name_len: i32,
            namespace: i32,
            data_ptr: i32,
            data_len: i32,
        ) -> i32;
        pub fn reg_sound_alias(
            logical_ptr: i32,
            logical_len: i32,
            lump_ptr: i32,
            lump_len: i32,
            limit: i32,
        ) -> i32;
        pub fn reg_weapon_slot(
            slot: i32,
            class_ptr: i32,
            class_len: i32,
            priority: i32,
        ) -> i32;
        pub fn manifest_option_i32(key_ptr: i32, key_len: i32, default: i32) -> i32;
        pub fn manifest_option_str(key_ptr: i32, key_len: i32, buf_ptr: i32, cap: i32) -> i32;
        pub fn lump_find(name_ptr: i32, name_len: i32, namespace: i32) -> i32;
        pub fn lump_read(index: i32, buf_ptr: i32, cap: i32) -> i32;
        pub fn acs_execute(script: i32, map: i32, args_ptr: i32, n: i32) -> i32;
    }
}

#[cfg(not(target_arch = "wasm32"))]
mod imp {
    pub unsafe fn log(_level: i32, _msg_ptr: i32, _msg_len: i32) {}
    pub unsafe fn level_time() -> i32 {
        0
    }
    pub unsafe fn player_count() -> i32 {
        0
    }
    pub unsafe fn actor_exists(_actor: i64) -> i32 {
        0
    }
    pub unsafe fn name_intern(ptr: i32, len: i32) -> i32 {
        if len <= 0 {
            return 0;
        }
        // Mirror host default FNV-1a for native unit tests without a linker.
        let _ = ptr;
        1
    }
    pub unsafe fn class_find(_ptr: i32, _len: i32) -> i32 {
        0
    }
    pub unsafe fn reg_class_begin(
        _name_ptr: i32,
        _name_len: i32,
        _parent: i32,
        _replaces: i32,
        _doomednum: i32,
        _flags: i32,
    ) -> i32 {
        0
    }
    pub unsafe fn reg_class_property_i32(_prop: i32, _value: i32) {}
    pub unsafe fn reg_class_flag(_flag: i32, _on: i32) {}
    pub unsafe fn reg_class_end() {}
    pub unsafe fn reg_state_label(_label_ptr: i32, _label_len: i32) {}
    pub unsafe fn reg_state(
        _sprite_ptr: i32,
        _frame: i32,
        _tics: i32,
        _flags: i32,
        _callback_id: i32,
    ) -> i32 {
        0
    }
    pub unsafe fn reg_state_flow(
        _kind: i32,
        _label_ptr: i32,
        _label_len: i32,
        _offset: i32,
    ) -> i32 {
        0
    }
    pub unsafe fn reg_state_action(
        _name_ptr: i32,
        _name_len: i32,
        _args_ptr: i32,
        _args_len: i32,
    ) -> i32 {
        0
    }
    pub unsafe fn reg_state_offset(_x: i32, _y: i32) -> i32 {
        0
    }
    pub unsafe fn reg_class_virtual(_slot: i32, _callback_id: i32) {}
    pub unsafe fn reg_event_handler(
        _name_ptr: i32,
        _name_len: i32,
        _order: i32,
        _flags: i32,
        _mask_lo: i32,
        _mask_hi: i32,
        _callback_base: i32,
    ) -> i32 {
        0
    }
    pub unsafe fn reg_thinker_class(
        _name_ptr: i32,
        _name_len: i32,
        _statnum: i32,
        _callback_id: i32,
    ) -> i32 {
        0
    }
    pub unsafe fn reg_static_function(
        _class_ptr: i32,
        _class_len: i32,
        _func_ptr: i32,
        _func_len: i32,
        _callback_id: i32,
    ) -> i32 {
        0
    }
    pub unsafe fn reg_lump(
        _name_ptr: i32,
        _name_len: i32,
        _namespace: i32,
        _data_ptr: i32,
        _data_len: i32,
    ) -> i32 {
        0
    }
    pub unsafe fn reg_sound_alias(
        _logical_ptr: i32,
        _logical_len: i32,
        _lump_ptr: i32,
        _lump_len: i32,
        _limit: i32,
    ) -> i32 {
        0
    }
    pub unsafe fn reg_weapon_slot(
        _slot: i32,
        _class_ptr: i32,
        _class_len: i32,
        _priority: i32,
    ) -> i32 {
        0
    }
    pub unsafe fn manifest_option_i32(_key_ptr: i32, _key_len: i32, default: i32) -> i32 {
        default
    }
    pub unsafe fn manifest_option_str(
        _key_ptr: i32,
        _key_len: i32,
        _buf_ptr: i32,
        _cap: i32,
    ) -> i32 {
        -1
    }
    pub unsafe fn lump_find(_name_ptr: i32, _name_len: i32, _namespace: i32) -> i32 {
        -1
    }
    pub unsafe fn lump_read(_index: i32, _buf_ptr: i32, _cap: i32) -> i32 {
        -1
    }
    pub unsafe fn acs_execute(_script: i32, _map: i32, _args_ptr: i32, _n: i32) -> i32 {
        0
    }
}

pub use imp::*;
