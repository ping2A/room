//! Android entry point: the NativeActivity glue calls `android_main` on its
//! own thread once `libroom_mobile.so` is loaded.

use crate::bundle::{BundleSource, sync_bundle};
use crate::platform::{MobilePlatform, start_config};
use crate::{crash_signal, logcat};
use room_app::EventLoop;
use std::ffi::CString;
use std::io::{self, Read};
use std::path::PathBuf;
use std::sync::Arc;
use winit::platform::android::EventLoopBuilderExtAndroid;
use winit::platform::android::activity::AndroidApp;

/// APK `assets/` through the Android asset manager.
struct ApkAssets(AndroidApp);

impl BundleSource for ApkAssets {
    fn len(&self, name: &str) -> Option<u64> {
        let name = CString::new(name).ok()?;
        let asset = self.0.asset_manager().open(&name)?;
        u64::try_from(asset.length()).ok()
    }

    fn open(&self, name: &str) -> io::Result<Box<dyn Read + '_>> {
        let c_name = CString::new(name).map_err(|_| io::ErrorKind::InvalidInput)?;
        let asset = self
            .0
            .asset_manager()
            .open(&c_name)
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, name.to_owned()))?;
        Ok(Box::new(asset))
    }
}

/// Runs when Android loads `libroom_mobile.so`, before `android_main`: the
/// first line in logcat shows the library loaded at all.
extern "C" fn library_loaded() {
    logcat::log_line("room: libroom_mobile.so loaded");
}

#[allow(unsafe_code)] // A static constructor in `.init_array`.
#[used]
#[unsafe(link_section = ".init_array")]
static LIBRARY_LOADED: extern "C" fn() = library_loaded;

/// Called by `android-activity`; the symbol name is fixed by that crate.
#[allow(unsafe_code)] // `no_mangle` exports the symbol the glue looks up.
#[unsafe(no_mangle)]
fn android_main(app: AndroidApp) {
    crash_signal::install();
    logcat::install_panic_logger();
    let data_dir = app
        .internal_data_path()
        .unwrap_or_else(|| PathBuf::from("/data/local/tmp/room"));
    // `/sdcard/Android/data/<package>/files`: reachable over USB and adb.
    let external = app.external_data_path();
    if let Some(dir) = &external {
        let _ = std::fs::create_dir_all(dir);
    }
    // The log file goes where `adb pull` and a computer can read it.
    let (log_path, previous) = logcat::open_log_file(external.as_deref().unwrap_or(&data_dir), 25);
    // A copy of the log and the screenshots in `Documents/Room`, which the
    // Files app shows (`Android/data` is hidden from it since Android 11).
    let shared = shared_dir();
    let shared_log = shared.as_deref().map(|dir| (dir, logcat::open_shared_log(dir)));
    // Each start-up step is logged straight to logcat (`adb logcat -s room`)
    // and the file, so the last line shows where a start that dies stopped.
    logcat::log_line(&format!(
        "room: android_main: Room {} on {} {} (Android {}, API {}, {} {})",
        env!("CARGO_PKG_VERSION"),
        logcat::system_property("ro.product.manufacturer"),
        logcat::system_property("ro.product.model"),
        logcat::system_property("ro.build.version.release"),
        logcat::system_property("ro.build.version.sdk"),
        std::env::consts::ARCH,
        if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        },
    ));
    logcat::log_line(&format!("room: start: log file {}", log_path.display()));
    match shared_log {
        Some((_, Some(path))) => {
            logcat::log_line(&format!("room: start: log copy {}", path.display()));
        }
        Some((dir, None)) => logcat::log_line(&format!(
            "room: start: cannot write in {} (Android 10 or older?): log and screenshots stay in the app's folder",
            dir.display()
        )),
        None => {}
    }
    if !previous.is_empty() {
        logcat::log_line("room: start: the previous run ended with:");
        for line in &previous {
            logcat::log_line(&format!("room: previous| {line}"));
        }
    }
    report_old_crashes(&data_dir.join("logs"));
    room_app::startlog::set_sink(logcat::log_step);
    if let Err(e) = logcat::redirect_stdio() {
        logcat::log_error(&format!("room: logcat redirect: {e}"));
    }
    logcat::log_line(&format!("room: start: app storage {}", data_dir.display()));
    let mut platform = MobilePlatform::new("android", data_dir.clone(), data_dir.join("bundle"));
    if let Some(dir) = external {
        platform = platform.with_import_dir(dir);
    }
    if let Some(dir) = shared.filter(|d| writable(d)) {
        platform = platform.with_shared_dir(dir);
    }
    logcat::log_line("room: start: unpacking app files");
    match sync_bundle(&ApkAssets(app.clone()), platform.bundle_dir()) {
        Ok(report) => {
            for name in &report.copied {
                eprintln!("room: unpacked {name}");
            }
        }
        Err(e) => logcat::log_error(&format!("room: unpacking app files: {e}")),
    }
    logcat::log_line(&format!(
        "room: start: app files in {}",
        platform.bundle_dir().display()
    ));
    let config = start_config();
    room_app::platform::install(Arc::new(platform));
    logcat::log_line("room: start: platform installed");
    // Writes a crash report file, then calls the logcat panic logger
    // installed above.
    room_app::install_crash_handler();
    logcat::log_line("room: start: building the event loop");
    let event_loop = match EventLoop::builder().with_android_app(app).build() {
        Ok(event_loop) => event_loop,
        Err(e) => {
            logcat::log_error(&format!("room: event loop: {e}"));
            return;
        }
    };
    logcat::log_line("room: start: event loop ready, opening the window");
    match room_app::run_with_event_loop(config, event_loop) {
        Ok(()) => logcat::log_line("room: stopped (window closed)"),
        Err(e) => logcat::log_error(&format!(
            "room: stopped after a start-up error (code {}); the reason is in the lines above",
            e.exit_code
        )),
    }
    // Let the stderr pipe thread pass on the last engine lines before the
    // activity is torn down.
    std::thread::sleep(std::time::Duration::from_millis(300));
}

/// Log crash reports a panic wrote in an earlier run (`logs/crash-*.log`),
/// once each: they are renamed to `.seen` afterwards.
fn report_old_crashes(dir: &std::path::Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let is_report = path
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.starts_with("crash-") && n.ends_with(".log"));
        if !is_report {
            continue;
        }
        logcat::log_error(&format!(
            "room: crash report from an earlier run: {}",
            path.display()
        ));
        if let Ok(text) = std::fs::read_to_string(&path) {
            for line in text.lines().take(40) {
                logcat::log_error(&format!("room: crash| {line}"));
            }
        }
        let _ = std::fs::rename(&path, path.with_extension("seen"));
    }
}

/// `Documents/Room` on the phone's shared storage. Since Android 11 an app
/// may create files there without any permission; the Files app shows them
/// (under Documents), and a computer sees them over USB.
fn shared_dir() -> Option<PathBuf> {
    let root = std::env::var_os("EXTERNAL_STORAGE")
        .map(PathBuf::from)
        .filter(|p| p.is_dir())
        .or_else(|| {
            let p = PathBuf::from("/storage/emulated/0");
            p.is_dir().then_some(p)
        })?;
    Some(root.join("Documents").join("Room"))
}

/// A file can be created in `dir` (made if missing).
fn writable(dir: &std::path::Path) -> bool {
    if std::fs::create_dir_all(dir.join("Screenshots")).is_err() {
        return false;
    }
    let probe = dir.join(format!(".room-probe-{}", std::process::id()));
    let ok = std::fs::write(&probe, b"ok").is_ok();
    let _ = std::fs::remove_file(&probe);
    ok
}
