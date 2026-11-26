//! iOS entry point: `room-ios`'s `main` (or an Xcode project's `main.m`)
//! calls [`room_ios_main`] on the main thread. winit then runs
//! `UIApplicationMain`, which never returns.

use crate::platform::{MobilePlatform, start_config};
use room_app::EventLoop;
use std::path::PathBuf;
use std::sync::Arc;

/// Start the app. Call once, from the main thread, from `main()`.
#[allow(unsafe_code)] // `no_mangle` exports the symbol an Xcode `main.m` calls.
#[unsafe(no_mangle)]
pub extern "C" fn room_ios_main() {
    // The app sandbox: `$HOME/Documents` is private and backed up; the
    // executable sits at the top of the read-only `.app` bundle.
    let home = std::env::var_os("HOME").map_or_else(|| PathBuf::from("."), PathBuf::from);
    let bundle = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from("."));
    // Documents is shown in the Files app (UIFileSharingEnabled), so games
    // copied there appear in the launcher.
    let documents = home.join("Documents");
    let platform = MobilePlatform::new("ios", documents.clone(), bundle).with_import_dir(documents);
    let config = start_config();
    room_app::platform::install(Arc::new(platform));
    room_app::install_crash_handler();
    let event_loop = match EventLoop::new() {
        Ok(event_loop) => event_loop,
        Err(e) => {
            eprintln!("room: event loop: {e}");
            return;
        }
    };
    if let Err(e) = room_app::run_with_event_loop(config, event_loop) {
        // The reason is already on stderr (and on the launcher screen when a
        // game picked there fails to start).
        eprintln!(
            "room: stopped after a start-up error (code {})",
            e.exit_code
        );
    }
}
