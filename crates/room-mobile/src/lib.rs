//! Android and iOS entry points for the Room app (docs/41).
//!
//! Both shells do the same three things: install a [`MobilePlatform`] that
//! keeps every file inside the app's own storage, build the start-up
//! [`AppConfig`] from what is installed, and hand control to
//! [`room_app::run_with_event_loop`].
//!
//! - Android: the OS loads `libroom_mobile.so` and calls `android_main`.
//! - iOS: the `room-ios` executable (inside `Room.app`, built by
//!   `scripts/build-ios.sh`) calls `room_ios_main()`.

pub mod bundle;
mod os;
mod platform;

#[cfg(target_os = "android")]
mod android;
#[cfg(target_os = "android")]
mod crash_signal;
#[cfg(target_os = "ios")]
mod ios;
#[cfg(target_os = "android")]
mod logcat;

#[cfg(target_os = "ios")]
pub use ios::room_ios_main;
pub use platform::{MobilePlatform, start_config};
pub use room_app::AppConfig;
