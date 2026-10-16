//! Declarative guest exports (`docs/12` §11).

/// Emit the lifecycle exports for a [`crate::game::RoomMod`] type: registers
/// its event handler (callbacks from [`crate::game::EVENT_CALLBACK_BASE`]),
/// keeps one instance, and decodes every dispatch into an
/// [`crate::game::Event`].
///
/// ```ignore
/// #[derive(Default)]
/// struct Director { kills: i32 }
/// impl room_script_sdk::game::RoomMod for Director {
///     const NAME: &'static str = "Director";
///     const EVENTS: u64 = room_script_sdk::game::event_mask::WORLD_THING_DIED;
///     fn on_event(&mut self, ev: room_script_sdk::game::Event) { self.kills += 1; }
/// }
/// room_script_sdk::export_mod!(Director);
/// ```
#[macro_export]
macro_rules! export_mod {
    ($ty:ty) => {
        static __ROOM_MOD: $crate::game::ModCell<$ty> = $crate::game::ModCell::new();

        #[unsafe(no_mangle)]
        pub extern "C" fn room_register() -> i32 {
            use $crate::game::RoomMod as _;
            let code = $crate::reg::event_handler(
                <$ty>::NAME,
                <$ty>::ORDER,
                0,
                <$ty>::EVENTS,
                $crate::game::EVENT_CALLBACK_BASE,
            );
            if code != 0 {
                return code;
            }
            __ROOM_MOD.with(|m| m.register()).map_or(1, |()| 0)
        }

        #[unsafe(no_mangle)]
        pub extern "C" fn room_init(session_kind: i32) -> i32 {
            use $crate::game::RoomMod as _;
            __ROOM_MOD.with(|m| m.init(session_kind)).map_or(1, |()| 0)
        }

        #[unsafe(no_mangle)]
        pub extern "C" fn room_dispatch(callback_id: i32, ptr: i32, len: i32) -> i64 {
            use $crate::game::RoomMod as _;
            let index = callback_id - $crate::game::EVENT_CALLBACK_BASE;
            if !(0..64).contains(&index) {
                return 0;
            }
            // SAFETY: `ptr`/`len` are exactly what the host passed in.
            let bytes = unsafe { $crate::game::payload(ptr, len) };
            let event = $crate::game::Event::decode(index, bytes);
            __ROOM_MOD.with(|m| m.on_event(event));
            0
        }
    };
}

/// Emit required lifecycle exports with no-op register/init/dispatch.
///
/// ```ignore
/// room_script_sdk::export_module!();
/// ```
///
/// With class names (each must provide `__room_register() -> i32`):
///
/// ```ignore
/// room_script_sdk::export_module!(classes = [FireImp]);
/// ```
#[macro_export]
macro_rules! export_module {
    () => {
        #[unsafe(no_mangle)]
        pub extern "C" fn room_register() -> i32 {
            0
        }

        #[unsafe(no_mangle)]
        pub extern "C" fn room_init(_session_kind: i32) -> i32 {
            0
        }

        #[unsafe(no_mangle)]
        pub extern "C" fn room_dispatch(
            _callback_id: i32,
            _payload_ptr: i32,
            _payload_len: i32,
        ) -> i64 {
            0
        }
    };
    (classes = [$($class:ident),* $(,)?] $(,)?) => {
        #[unsafe(no_mangle)]
        pub extern "C" fn room_register() -> i32 {
            $(
                let code = $class::__room_register();
                if code != 0 {
                    return code;
                }
            )*
            0
        }

        #[unsafe(no_mangle)]
        pub extern "C" fn room_init(_session_kind: i32) -> i32 {
            0
        }

        #[unsafe(no_mangle)]
        pub extern "C" fn room_dispatch(
            _callback_id: i32,
            _payload_ptr: i32,
            _payload_len: i32,
        ) -> i64 {
            0
        }
    };
}
