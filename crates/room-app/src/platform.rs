//! Where the app keeps its files, and the few OS services that differ between
//! desktop and phones (docs/41 §3.1).
//!
//! Everything in `room-app` that needs a path on disk asks the current
//! [`Platform`] instead of reading `$HOME` / `%APPDATA%` itself. The desktop
//! implementation reproduces the paths the binary has always used; Android and
//! iOS supply their own in `room-mobile`.

pub use crate::frame_pacer::Thermal;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

/// Screen edges the UI must keep clear (notch, rounded corners, gesture bar),
/// in physical pixels.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Insets {
    /// Top edge.
    pub top: u32,
    /// Bottom edge.
    pub bottom: u32,
    /// Left edge.
    pub left: u32,
    /// Right edge.
    pub right: u32,
}

/// A short vibration on devices that have one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Haptic {
    /// Light tap (button press).
    Light,
    /// Stronger pulse (taking damage, firing a heavy weapon).
    Heavy,
}

/// Paths and OS services for one platform.
pub trait Platform: Send + Sync + 'static {
    /// Short name for logs (`desktop`, `android`, `ios`).
    fn name(&self) -> &'static str;

    /// The user's `room.ini`.
    fn config_file(&self) -> PathBuf;

    /// Folder for `.fsave` slots. Defaults to `saves/` next to [`Self::config_file`];
    /// `None` means the current directory.
    fn saves_dir(&self) -> Option<PathBuf> {
        self.config_file().parent().map(|dir| dir.join("saves"))
    }

    /// Folder for F12 screenshots and F11 GIFs.
    fn screenshots_dir(&self) -> PathBuf;

    /// Folder for crash reports.
    fn crash_dir(&self) -> PathBuf;

    /// Source checkout root in development builds (`testdata/`, `wadsrc/`,
    /// `soundfonts/` live under it). `None` in shipped apps.
    fn dev_root(&self) -> Option<PathBuf>;

    /// Roots searched for shipped game data, in order. The desktop searches
    /// the current directory, then the checkout. Phones return the folder the
    /// app bundle unpacks to.
    fn asset_roots(&self) -> Vec<PathBuf> {
        let mut roots = vec![PathBuf::new()];
        roots.extend(self.dev_root());
        roots
    }

    /// Folders scanned for IWADs (launcher, `--combat-demo` auto pick).
    fn iwad_search_dirs(&self) -> Vec<PathBuf>;

    /// Possible `gzdoom.ini` / `zdoom.ini` files to import settings from
    /// (existence is checked by the caller).
    fn gzdoom_ini_candidates(&self) -> Vec<PathBuf>;

    /// SoundFont files to try, most specific first, after any explicit one.
    fn soundfont_candidates(&self) -> Vec<PathBuf>;

    /// Roots searched for optional `lights.pk3` / `brightmaps.pk3` / ….
    fn engine_extra_roots(&self) -> Vec<PathBuf>;

    /// Screen edges to keep controls away from.
    fn safe_area(&self) -> Insets {
        Insets::default()
    }

    /// Vibrate, where supported.
    fn haptic(&self, _kind: Haptic) {}

    /// Stop the screen from dimming while a level is running.
    fn keep_screen_on(&self, _on: bool) {}

    /// Folders the launcher lists games and mods from, `(folder, shipped
    /// with the app)`, first match of a file name wins.
    fn library_dirs(&self) -> Vec<(PathBuf, bool)> {
        self.iwad_search_dirs()
            .into_iter()
            .map(|dir| (dir, false))
            .collect()
    }

    /// Where the player can put game files, shown in the launcher.
    fn import_hint(&self) -> Option<String> {
        None
    }

    /// Memory-map large game files instead of reading them (see
    /// `room_vfs::mapped`): less memory, but a file changed while the game
    /// runs can crash it. Phones say yes; desktops keep reading.
    fn map_large_files(&self) -> bool {
        false
    }

    /// A touch screen is the main input: show the on-screen controls from
    /// the start instead of after the first touch.
    fn touch_screen(&self) -> bool {
        false
    }

    /// `vid_maxfps` before `room.ini` is read: 0 (no cap) on desktops; phones
    /// cap at 60 for battery and heat (120 Hz screens can choose 120).
    fn default_max_fps(&self) -> u32 {
        0
    }

    /// More CVar defaults for this platform, applied before `room.ini` (which
    /// still wins): `(name, value)` in console syntax.
    fn default_cvars(&self) -> Vec<(&'static str, &'static str)> {
        Vec::new()
    }

    /// The OS's thermal state, polled about once a second.
    fn thermal(&self) -> Thermal {
        Thermal::Nominal
    }

    /// Tell the OS the frame rate the game wants (`fps` 0: no preference),
    /// so a 120 Hz screen can run at 120 or drop to 60. Called when the cap
    /// changes and after a new surface.
    fn set_frame_rate(&self, _window: &winit::window::Window, _fps: u32) {}
}

static CURRENT: OnceLock<Arc<dyn Platform>> = OnceLock::new();

/// Make `platform` the one the app uses. The first call wins; later calls
/// return `false` and change nothing.
pub fn install(platform: Arc<dyn Platform>) -> bool {
    CURRENT.set(platform).is_ok()
}

/// The installed platform, or the desktop one built from the process
/// environment when nothing was installed (tests, tools).
#[must_use]
pub fn current() -> &'static dyn Platform {
    CURRENT
        .get_or_init(|| Arc::new(DesktopPlatform::from_process()))
        .as_ref()
}

/// Which desktop OS layout to follow for per-user folders.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DesktopOs {
    /// `~/Library/...`
    MacOs,
    /// `%APPDATA%` / `%LOCALAPPDATA%`
    Windows,
    /// XDG (`~/.config`, `~/.local/share`).
    Unix,
}

impl DesktopOs {
    /// The OS this binary was built for.
    #[must_use]
    pub const fn host() -> Self {
        if cfg!(target_os = "macos") {
            Self::MacOs
        } else if cfg!(target_os = "windows") {
            Self::Windows
        } else {
            Self::Unix
        }
    }
}

/// The environment values desktop paths are built from, read once at start.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DesktopEnv {
    /// `$HOME`.
    pub home: Option<PathBuf>,
    /// `$XDG_CONFIG_HOME`.
    pub xdg_config_home: Option<PathBuf>,
    /// `$XDG_DATA_HOME`.
    pub xdg_data_home: Option<PathBuf>,
    /// `%APPDATA%`.
    pub appdata: Option<PathBuf>,
    /// `%LOCALAPPDATA%`.
    pub localappdata: Option<PathBuf>,
    /// `$DOOMWADDIR`.
    pub doomwaddir: Option<PathBuf>,
    /// `$ROOM_IWAD_DIR`.
    pub room_iwad_dir: Option<PathBuf>,
    /// `$ROOM_SOUNDFONT`.
    pub room_soundfont: Option<PathBuf>,
    /// `$SND_SOUNDFONT`.
    pub snd_soundfont: Option<PathBuf>,
    /// Folder holding the running executable.
    pub exe_dir: Option<PathBuf>,
}

impl DesktopEnv {
    /// Read the real process environment.
    #[must_use]
    pub fn from_process() -> Self {
        let os = |k: &str| std::env::var_os(k).map(PathBuf::from);
        // `std::env::var` (not `var_os`) for the values the old code read that
        // way: a non-UTF-8 value was ignored, and still is.
        let utf8 = |k: &str| std::env::var(k).ok().map(PathBuf::from);
        Self {
            home: os("HOME"),
            xdg_config_home: os("XDG_CONFIG_HOME"),
            xdg_data_home: os("XDG_DATA_HOME"),
            appdata: os("APPDATA"),
            localappdata: os("LOCALAPPDATA"),
            doomwaddir: utf8("DOOMWADDIR"),
            room_iwad_dir: utf8("ROOM_IWAD_DIR"),
            room_soundfont: utf8("ROOM_SOUNDFONT"),
            snd_soundfont: utf8("SND_SOUNDFONT"),
            exe_dir: std::env::current_exe()
                .ok()
                .and_then(|exe| exe.parent().map(Path::to_path_buf)),
        }
    }
}

/// Desktop paths: the same ones the `room` binary used before docs/41 M0.
#[derive(Clone, Debug)]
pub struct DesktopPlatform {
    env: DesktopEnv,
    os: DesktopOs,
    dev_root: Option<PathBuf>,
}

impl DesktopPlatform {
    /// From the process environment, for the host OS, with the source
    /// checkout this crate was built from as the dev root.
    #[must_use]
    pub fn from_process() -> Self {
        Self::new(
            DesktopEnv::from_process(),
            DesktopOs::host(),
            Some(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")),
        )
    }

    /// From explicit values (tests).
    #[must_use]
    pub fn new(env: DesktopEnv, os: DesktopOs, dev_root: Option<PathBuf>) -> Self {
        Self { env, os, dev_root }
    }

    /// `<per-user dir>/room/<leaf>`, following the old lookup order:
    /// the OS-specific folder first, then XDG, then `~/<unix_home>`, then `fallback`.
    fn user_path(&self, kind: UserDir, leaf: &str, fallback: &str) -> PathBuf {
        let env = &self.env;
        match (self.os, kind) {
            (DesktopOs::MacOs, UserDir::Config | UserDir::Data) => {
                if let Some(home) = &env.home {
                    return home
                        .join("Library")
                        .join("Application Support")
                        .join("room")
                        .join(leaf);
                }
            }
            (DesktopOs::MacOs, UserDir::Logs) => {
                if let Some(home) = &env.home {
                    return home.join("Library").join("Logs").join("room");
                }
            }
            (DesktopOs::Windows, UserDir::Config | UserDir::Data) => {
                if let Some(app) = &env.appdata {
                    return app.join("room").join(leaf);
                }
            }
            (DesktopOs::Windows, UserDir::Logs) => {
                if let Some(local) = &env.localappdata {
                    return local.join("room").join(leaf);
                }
            }
            (DesktopOs::Unix, _) => {}
        }
        let xdg = match kind {
            UserDir::Config => env.xdg_config_home.as_ref(),
            UserDir::Data => env.xdg_data_home.as_ref(),
            UserDir::Logs => None,
        };
        if let Some(xdg) = xdg {
            return xdg.join("room").join(leaf);
        }
        if let Some(home) = &env.home {
            let base = match kind {
                UserDir::Config => home.join(".config"),
                UserDir::Data | UserDir::Logs => home.join(".local").join("share"),
            };
            return base.join("room").join(leaf);
        }
        PathBuf::from(fallback)
    }
}

#[derive(Clone, Copy)]
enum UserDir {
    Config,
    Data,
    Logs,
}

impl Platform for DesktopPlatform {
    fn name(&self) -> &'static str {
        "desktop"
    }

    fn config_file(&self) -> PathBuf {
        self.user_path(UserDir::Config, "room.ini", "room.ini")
    }

    fn screenshots_dir(&self) -> PathBuf {
        self.user_path(UserDir::Data, "screenshots", "screenshots")
    }

    fn crash_dir(&self) -> PathBuf {
        self.user_path(UserDir::Logs, "logs", "room-logs")
    }

    fn dev_root(&self) -> Option<PathBuf> {
        self.dev_root.clone()
    }

    fn iwad_search_dirs(&self) -> Vec<PathBuf> {
        let mut dirs = vec![
            PathBuf::from("testdata/doom"),
            PathBuf::from("testdata/freedoom"),
            PathBuf::from("wad"),
            PathBuf::from("wads"),
            PathBuf::from("."),
        ];
        if let Some(home) = &self.env.home {
            dirs.push(home.join("Games").join("Doom"));
            dirs.push(home.join("games").join("doom"));
            dirs.push(home.join(".local").join("share").join("games").join("doom"));
            dirs.push(
                home.join("Library")
                    .join("Application Support")
                    .join("gzdoom"),
            );
        }
        if let Some(xdg) = &self.env.xdg_data_home {
            dirs.push(xdg.join("games").join("doom"));
        }
        dirs.extend(self.env.doomwaddir.clone());
        dirs.extend(self.env.room_iwad_dir.clone());
        dirs
    }

    fn gzdoom_ini_candidates(&self) -> Vec<PathBuf> {
        let mut out = Vec::new();
        if let Some(home) = &self.env.home {
            out.push(
                home.join("Library")
                    .join("Application Support")
                    .join("gzdoom")
                    .join("gzdoom.ini"),
            );
            out.push(home.join(".config").join("gzdoom").join("gzdoom.ini"));
            out.push(home.join(".config").join("zdoom").join("zdoom.ini"));
        }
        if let Some(appdata) = &self.env.appdata {
            out.push(appdata.join("gzdoom").join("gzdoom.ini"));
        }
        out
    }

    fn soundfont_candidates(&self) -> Vec<PathBuf> {
        let mut out = Vec::new();
        out.extend(self.env.room_soundfont.clone());
        out.extend(self.env.snd_soundfont.clone());
        let mut roots: Vec<PathBuf> = self
            .asset_roots()
            .into_iter()
            .map(|root| root.join("soundfonts"))
            .collect();
        roots.push(PathBuf::from("."));
        for root in roots {
            for bank in SOUNDFONT_BANKS {
                out.push(root.join(bank));
            }
        }
        out
    }

    fn engine_extra_roots(&self) -> Vec<PathBuf> {
        let mut roots = vec![PathBuf::from(".")];
        roots.extend(
            self.asset_roots()
                .into_iter()
                .map(|root| root.join("testdata/m10")),
        );
        roots.extend(self.env.exe_dir.clone());
        roots
    }
}

/// GM bank file names tried in each SoundFont root.
pub const SOUNDFONT_BANKS: [&str; 5] = [
    "room.sf2",
    "TimGM6mb.sf2",
    "default.sf2",
    "gzdoom.sf2",
    "FluidR3_GM.sf2",
];

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    fn env_home(home: &str) -> DesktopEnv {
        DesktopEnv {
            home: Some(PathBuf::from(home)),
            ..DesktopEnv::default()
        }
    }

    fn unix(env: DesktopEnv) -> DesktopPlatform {
        DesktopPlatform::new(env, DesktopOs::Unix, Some(PathBuf::from("/src/room")))
    }

    #[test]
    fn unix_paths_follow_xdg_then_home_then_cwd() {
        let p = unix(env_home("/home/a"));
        assert_eq!(
            p.config_file(),
            PathBuf::from("/home/a/.config/room/room.ini")
        );
        assert_eq!(
            p.saves_dir(),
            Some(PathBuf::from("/home/a/.config/room/saves"))
        );
        assert_eq!(
            p.screenshots_dir(),
            PathBuf::from("/home/a/.local/share/room/screenshots")
        );
        assert_eq!(
            p.crash_dir(),
            PathBuf::from("/home/a/.local/share/room/logs")
        );

        let p = unix(DesktopEnv {
            xdg_config_home: Some("/x/cfg".into()),
            xdg_data_home: Some("/x/data".into()),
            ..env_home("/home/a")
        });
        assert_eq!(p.config_file(), PathBuf::from("/x/cfg/room/room.ini"));
        assert_eq!(
            p.screenshots_dir(),
            PathBuf::from("/x/data/room/screenshots")
        );
        // Crash logs never used XDG.
        assert_eq!(
            p.crash_dir(),
            PathBuf::from("/home/a/.local/share/room/logs")
        );

        let p = unix(DesktopEnv::default());
        assert_eq!(p.config_file(), PathBuf::from("room.ini"));
        assert_eq!(p.saves_dir(), Some(PathBuf::new().join("saves")));
        assert_eq!(p.screenshots_dir(), PathBuf::from("screenshots"));
        assert_eq!(p.crash_dir(), PathBuf::from("room-logs"));
    }

    #[test]
    fn macos_paths_use_library() {
        let p = DesktopPlatform::new(env_home("/Users/a"), DesktopOs::MacOs, None);
        assert_eq!(
            p.config_file(),
            PathBuf::from("/Users/a/Library/Application Support/room/room.ini")
        );
        assert_eq!(
            p.screenshots_dir(),
            PathBuf::from("/Users/a/Library/Application Support/room/screenshots")
        );
        assert_eq!(p.crash_dir(), PathBuf::from("/Users/a/Library/Logs/room"));
    }

    #[test]
    fn windows_paths_use_appdata() {
        let p = DesktopPlatform::new(
            DesktopEnv {
                appdata: Some("C:/R".into()),
                localappdata: Some("C:/L".into()),
                ..DesktopEnv::default()
            },
            DesktopOs::Windows,
            None,
        );
        assert_eq!(p.config_file(), PathBuf::from("C:/R/room/room.ini"));
        assert_eq!(p.screenshots_dir(), PathBuf::from("C:/R/room/screenshots"));
        assert_eq!(p.crash_dir(), PathBuf::from("C:/L/room/logs"));
        // No APPDATA: falls through to the XDG / home rules.
        let p = DesktopPlatform::new(env_home("/h"), DesktopOs::Windows, None);
        assert_eq!(p.config_file(), PathBuf::from("/h/.config/room/room.ini"));
    }

    #[test]
    fn asset_roots_are_cwd_then_checkout() {
        let p = unix(DesktopEnv::default());
        assert_eq!(
            p.asset_roots(),
            vec![PathBuf::new(), PathBuf::from("/src/room")]
        );
        // Joining keeps the old relative spelling (`testdata/...`, no `./`).
        assert_eq!(
            p.asset_roots()[0].join("testdata/freedoom/freedoom2.wad"),
            PathBuf::from("testdata/freedoom/freedoom2.wad")
        );
        let shipped = DesktopPlatform::new(DesktopEnv::default(), DesktopOs::Unix, None);
        assert_eq!(shipped.asset_roots(), vec![PathBuf::new()]);
    }

    #[test]
    fn iwad_dirs_keep_order_and_env_overrides_last() {
        let p = unix(DesktopEnv {
            xdg_data_home: Some("/x".into()),
            doomwaddir: Some("/w".into()),
            room_iwad_dir: Some("/r".into()),
            ..env_home("/h")
        });
        let dirs = p.iwad_search_dirs();
        assert_eq!(dirs[0], PathBuf::from("testdata/doom"));
        assert_eq!(dirs[4], PathBuf::from("."));
        assert_eq!(dirs[5], PathBuf::from("/h/Games/Doom"));
        assert_eq!(
            dirs[8],
            PathBuf::from("/h/Library/Application Support/gzdoom")
        );
        assert_eq!(dirs[9], PathBuf::from("/x/games/doom"));
        assert_eq!(&dirs[10..], &[PathBuf::from("/w"), PathBuf::from("/r")]);
    }

    #[test]
    fn soundfonts_env_first_then_banks_per_root() {
        let p = unix(DesktopEnv {
            room_soundfont: Some("/a.sf2".into()),
            snd_soundfont: Some("/b.sf2".into()),
            ..DesktopEnv::default()
        });
        let c = p.soundfont_candidates();
        assert_eq!(c[0], PathBuf::from("/a.sf2"));
        assert_eq!(c[1], PathBuf::from("/b.sf2"));
        assert_eq!(c[2], PathBuf::from("soundfonts/room.sf2"));
        assert_eq!(c[7], PathBuf::from("/src/room/soundfonts/room.sf2"));
        assert_eq!(c[12], PathBuf::from("./room.sf2"));
        assert_eq!(c.len(), 2 + 3 * SOUNDFONT_BANKS.len());
    }

    #[test]
    fn engine_extra_roots_cwd_testdata_then_exe() {
        let p = unix(DesktopEnv {
            exe_dir: Some("/bin/room".into()),
            ..DesktopEnv::default()
        });
        assert_eq!(
            p.engine_extra_roots(),
            vec![
                PathBuf::from("."),
                PathBuf::from("testdata/m10"),
                PathBuf::from("/src/room/testdata/m10"),
                PathBuf::from("/bin/room"),
            ]
        );
    }

    #[test]
    fn gzdoom_ini_candidates_listed_before_existence_check() {
        let p = unix(DesktopEnv {
            appdata: Some("/ad".into()),
            ..env_home("/h")
        });
        assert_eq!(
            p.gzdoom_ini_candidates(),
            vec![
                PathBuf::from("/h/Library/Application Support/gzdoom/gzdoom.ini"),
                PathBuf::from("/h/.config/gzdoom/gzdoom.ini"),
                PathBuf::from("/h/.config/zdoom/zdoom.ini"),
                PathBuf::from("/ad/gzdoom/gzdoom.ini"),
            ]
        );
    }

    #[test]
    fn mobile_style_platform_overrides_defaults() {
        struct Phone;
        impl Platform for Phone {
            fn name(&self) -> &'static str {
                "phone"
            }
            fn config_file(&self) -> PathBuf {
                "/data/app/room.ini".into()
            }
            fn screenshots_dir(&self) -> PathBuf {
                "/data/app/shots".into()
            }
            fn crash_dir(&self) -> PathBuf {
                "/data/app/logs".into()
            }
            fn dev_root(&self) -> Option<PathBuf> {
                None
            }
            fn asset_roots(&self) -> Vec<PathBuf> {
                vec!["/bundle".into()]
            }
            fn iwad_search_dirs(&self) -> Vec<PathBuf> {
                vec!["/data/app/library".into()]
            }
            fn gzdoom_ini_candidates(&self) -> Vec<PathBuf> {
                Vec::new()
            }
            fn soundfont_candidates(&self) -> Vec<PathBuf> {
                Vec::new()
            }
            fn engine_extra_roots(&self) -> Vec<PathBuf> {
                Vec::new()
            }
        }
        let p = Phone;
        assert_eq!(p.saves_dir(), Some(PathBuf::from("/data/app/saves")));
        assert_eq!(p.safe_area(), Insets::default());
        p.haptic(Haptic::Light);
        p.keep_screen_on(true);
    }
}
