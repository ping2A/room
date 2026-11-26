//! Room's native app: window, game loop, renderer, audio and menus.
//!
//! The desktop `room` binary (`src/main.rs`) turns its command line into an
//! [`AppConfig`] and calls [`run`]. The Android and iOS shells (docs/41) build
//! the config from their own launcher, install their [`Platform`], and call
//! [`run_with_event_loop`] with the event loop their OS hands them.

#![cfg_attr(not(test), deny(clippy::unwrap_used))]

mod app;
mod audio_out;
mod bench;
pub mod config;
mod crash;
mod face_import;
mod frame_pacer;
mod gamepad;
mod lan;
mod launcher;
mod launcher_art;
mod launcher_screen;
mod library;
mod lifecycle;
mod midi_device;
mod netplay;
mod perf;
pub mod platform;
mod radar;
mod render_scale;
mod screen_capture;
mod shader_cvars;
mod soundfont;
pub mod startlog;
mod touch_overlay;
mod ui_font;
mod ui_text;

pub use config::{AppConfig, StartError};
pub use midi_device::MidiBackend;
pub use netplay::NetTransportKind;
pub use platform::{DesktopPlatform, Platform};
/// Re-exported so shells build the event loop with the same winit version.
pub use winit::event_loop::EventLoop;
/// The window type [`Platform::set_frame_rate`] receives.
pub use winit::window::Window;

/// Write a crash report under [`Platform::crash_dir`] when the app panics.
/// Install the platform first.
pub fn install_crash_handler() {
    crash::install();
}

/// Run until the window closes, building the event loop here (desktop).
///
/// # Errors
/// Start-up failed (missing or unreadable game files, bad netplay address,
/// …). The message was already printed to stderr; the desktop binary exits
/// with [`StartError::exit_code`].
pub fn run(config: AppConfig) -> Result<(), StartError> {
    app::run_with(config, None)
}

/// Run until the window closes on an event loop the caller built (phones
/// build it from the OS activity).
///
/// # Errors
/// As [`run`].
pub fn run_with_event_loop(config: AppConfig, event_loop: EventLoop<()>) -> Result<(), StartError> {
    app::run_with(config, Some(event_loop))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    // Start-up failures come back as errors instead of ending the process
    // (a phone app must stay alive to show them). Both cases fail before any
    // window or event loop exists.

    #[test]
    fn conflicting_settings_return_exit_code_2() {
        let err = run(AppConfig {
            host: Some("5029".into()),
            join: Some("127.0.0.1:5029".into()),
            ..AppConfig::default()
        })
        .expect_err("conflict");
        assert_eq!(err.exit_code, 2);
        assert_eq!(err.message, "use only one of --host / --join");
    }

    #[test]
    fn missing_game_file_returns_exit_code_1() {
        let err = run(AppConfig {
            iwad: Some("/definitely/not/here/doom2.wad".into()),
            play: true,
            ..AppConfig::default()
        })
        .expect_err("missing file");
        assert_eq!(err.exit_code, 1);
        assert!(err.message.contains("I/O error"), "{}", err.message);
    }
}
