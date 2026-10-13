//! Declarative guest exports (`docs/12` §11).

/// Emit required lifecycle exports with no-op register/init/dispatch.
///
/// ```ignore
/// ferrite_script_sdk::export_module!();
/// ```
///
/// With class names (each must provide `__ferrite_register() -> i32`):
///
/// ```ignore
/// ferrite_script_sdk::export_module!(classes = [FireImp]);
/// ```
#[macro_export]
macro_rules! export_module {
    () => {
        #[unsafe(no_mangle)]
        pub extern "C" fn ferrite_register() -> i32 {
            0
        }

        #[unsafe(no_mangle)]
        pub extern "C" fn ferrite_init(_session_kind: i32) -> i32 {
            0
        }

        #[unsafe(no_mangle)]
        pub extern "C" fn ferrite_dispatch(
            _callback_id: i32,
            _payload_ptr: i32,
            _payload_len: i32,
        ) -> i64 {
            0
        }
    };
    (classes = [$($class:ident),* $(,)?] $(,)?) => {
        #[unsafe(no_mangle)]
        pub extern "C" fn ferrite_register() -> i32 {
            $(
                let code = $class::__ferrite_register();
                if code != 0 {
                    return code;
                }
            )*
            0
        }

        #[unsafe(no_mangle)]
        pub extern "C" fn ferrite_init(_session_kind: i32) -> i32 {
            0
        }

        #[unsafe(no_mangle)]
        pub extern "C" fn ferrite_dispatch(
            _callback_id: i32,
            _payload_ptr: i32,
            _payload_len: i32,
        ) -> i64 {
            0
        }
    };
}
