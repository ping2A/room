//! Thermal state and frame-rate hints from the phone OS (docs/41 M6).
//!
//! Both are looked up at run time, so the app still starts on OS versions
//! that lack them (Android before 11, where they read as "nominal" / do
//! nothing).

use room_app::Window;
use room_app::platform::Thermal;

/// Android `AThermalStatus` → [`Thermal`]. `LIGHT` is warm; `MODERATE`
/// already throttles; `SEVERE` and above hurt the game a lot. Errors (-1)
/// and unknown values read as nominal.
#[must_use]
#[cfg_attr(not(target_os = "android"), allow(dead_code))]
pub fn from_android_status(status: i32) -> Thermal {
    match status {
        1 => Thermal::Fair,
        2 => Thermal::Serious,
        3.. => Thermal::Critical,
        _ => Thermal::Nominal,
    }
}

/// iOS `NSProcessInfoThermalState` (0 nominal … 3 critical) → [`Thermal`].
#[must_use]
#[cfg_attr(not(target_os = "ios"), allow(dead_code))]
pub fn from_ios_state(state: isize) -> Thermal {
    match state {
        1 => Thermal::Fair,
        2 => Thermal::Serious,
        3.. => Thermal::Critical,
        _ => Thermal::Nominal,
    }
}

/// The device's thermal state now.
#[must_use]
pub fn thermal() -> Thermal {
    imp::thermal()
}

/// Ask the OS to run the screen at `fps` for this window (0: no preference).
pub fn set_frame_rate(window: &Window, fps: u32) {
    imp::set_frame_rate(window, fps);
}

#[cfg(target_os = "android")]
#[allow(unsafe_code)] // dlopen/dlsym and calls into libandroid.
mod imp {
    use super::{Thermal, Window, from_android_status};
    use std::ffi::{CStr, c_void};
    use std::sync::OnceLock;
    use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};

    type AcquireManager = unsafe extern "C" fn() -> *mut c_void;
    type CurrentStatus = unsafe extern "C" fn(*mut c_void) -> i32;
    type SetFrameRate = unsafe extern "C" fn(*mut c_void, f32, i8) -> i32;

    /// Functions from `libandroid.so` that only exist from API 30 on.
    struct Api {
        /// `AThermalManager*`, kept for the life of the process.
        manager: usize,
        status: Option<CurrentStatus>,
        set_frame_rate: Option<SetFrameRate>,
    }

    fn sym(lib: *mut c_void, name: &CStr) -> *mut c_void {
        // SAFETY: `lib` is a handle from dlopen (or null, which dlsym accepts
        // as "not found" on bionic); `name` is NUL-terminated.
        unsafe { libc::dlsym(lib, name.as_ptr()) }
    }

    fn api() -> &'static Api {
        static API: OnceLock<Api> = OnceLock::new();
        API.get_or_init(|| {
            // SAFETY: plain dlopen of a system library already loaded by the
            // NativeActivity; the handle is never closed.
            let lib = unsafe { libc::dlopen(c"libandroid.so".as_ptr(), libc::RTLD_NOW) };
            if lib.is_null() {
                return Api {
                    manager: 0,
                    status: None,
                    set_frame_rate: None,
                };
            }
            let acquire = sym(lib, c"AThermal_acquireManager");
            let status = sym(lib, c"AThermal_getCurrentStatus");
            let set_frame_rate = sym(lib, c"ANativeWindow_setFrameRate");
            let manager = if acquire.is_null() || status.is_null() {
                0
            } else {
                // SAFETY: the symbol has this signature in the NDK headers.
                let acquire: AcquireManager = unsafe { std::mem::transmute(acquire) };
                // SAFETY: no arguments; returns a manager or null.
                unsafe { acquire() as usize }
            };
            Api {
                manager,
                status: (manager != 0).then(|| {
                    // SAFETY: signature from the NDK headers.
                    unsafe { std::mem::transmute::<*mut c_void, CurrentStatus>(status) }
                }),
                set_frame_rate: (!set_frame_rate.is_null()).then(|| {
                    // SAFETY: signature from the NDK headers.
                    unsafe { std::mem::transmute::<*mut c_void, SetFrameRate>(set_frame_rate) }
                }),
            }
        })
    }

    pub(super) fn thermal() -> Thermal {
        let api = api();
        match api.status {
            // SAFETY: `manager` came from AThermal_acquireManager.
            Some(status) => from_android_status(unsafe { status(api.manager as *mut c_void) }),
            None => Thermal::Nominal,
        }
    }

    pub(super) fn set_frame_rate(window: &Window, fps: u32) {
        let Some(set) = api().set_frame_rate else {
            return;
        };
        let Ok(handle) = window.window_handle() else {
            return;
        };
        if let RawWindowHandle::AndroidNdk(h) = handle.as_raw() {
            // ANATIVEWINDOW_FRAME_RATE_COMPATIBILITY_DEFAULT (0): the game
            // prefers this rate but the screen may pick a multiple.
            // SAFETY: the window pointer is valid while `window` has a surface.
            let r = unsafe { set(h.a_native_window.as_ptr(), fps as f32, 0) };
            if r != 0 {
                eprintln!("room: ANativeWindow_setFrameRate({fps}) failed ({r})");
            }
        }
    }
}

#[cfg(target_os = "ios")]
#[allow(unsafe_code)] // Objective-C runtime calls.
mod imp {
    use super::{Thermal, Window, from_ios_state};
    use std::ffi::{c_char, c_void};

    #[link(name = "objc")]
    unsafe extern "C" {
        fn objc_getClass(name: *const c_char) -> *mut c_void;
        fn sel_registerName(name: *const c_char) -> *mut c_void;
        fn objc_msgSend();
    }
    #[link(name = "Foundation", kind = "framework")]
    unsafe extern "C" {}

    pub(super) fn thermal() -> Thermal {
        // [[NSProcessInfo processInfo] thermalState]
        type SendId = unsafe extern "C" fn(*mut c_void, *mut c_void) -> *mut c_void;
        type SendInt = unsafe extern "C" fn(*mut c_void, *mut c_void) -> isize;
        // SAFETY: objc_msgSend is called through the exact signature of each
        // message (no arguments; returns an object / an NSInteger), the
        // documented way to call it from C.
        unsafe {
            let class = objc_getClass(c"NSProcessInfo".as_ptr());
            if class.is_null() {
                return Thermal::Nominal;
            }
            let send_id: SendId = std::mem::transmute(objc_msgSend as unsafe extern "C" fn());
            let send_int: SendInt = std::mem::transmute(objc_msgSend as unsafe extern "C" fn());
            let info = send_id(class, sel_registerName(c"processInfo".as_ptr()));
            if info.is_null() {
                return Thermal::Nominal;
            }
            from_ios_state(send_int(info, sel_registerName(c"thermalState".as_ptr())))
        }
    }

    /// ProMotion screens follow the presented frames once Info.plist has
    /// `CADisableMinimumFrameDurationOnPhone`; the cap does the rest.
    pub(super) fn set_frame_rate(_window: &Window, _fps: u32) {}
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
mod imp {
    use super::{Thermal, Window};

    pub(super) fn thermal() -> Thermal {
        Thermal::Nominal
    }

    pub(super) fn set_frame_rate(_window: &Window, _fps: u32) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn android_statuses_map_to_levels() {
        assert_eq!(from_android_status(-1), Thermal::Nominal, "error");
        assert_eq!(from_android_status(0), Thermal::Nominal);
        assert_eq!(from_android_status(1), Thermal::Fair);
        assert_eq!(from_android_status(2), Thermal::Serious);
        for status in 3..=6 {
            assert_eq!(from_android_status(status), Thermal::Critical);
        }
    }

    #[test]
    fn ios_states_map_to_levels() {
        assert_eq!(from_ios_state(0), Thermal::Nominal);
        assert_eq!(from_ios_state(1), Thermal::Fair);
        assert_eq!(from_ios_state(2), Thermal::Serious);
        assert_eq!(from_ios_state(3), Thermal::Critical);
        assert_eq!(from_ios_state(-4), Thermal::Nominal);
    }

    #[test]
    fn the_desktop_build_reads_nominal() {
        assert_eq!(thermal(), Thermal::Nominal);
    }
}
