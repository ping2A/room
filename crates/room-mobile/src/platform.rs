//! [`MobilePlatform`]: every path inside the app's private storage.

use room_app::AppConfig;
use room_app::platform::{Platform, SOUNDFONT_BANKS, Thermal};
use std::path::{Path, PathBuf};

/// Paths for a phone app.
///
/// - `data_dir`: private, writable, backed up by the OS (settings, saves,
///   screenshots, imported games in `library/`).
/// - `bundle_dir`: read-only files shipped inside the app (Freedoom, SoundFont).
/// - `import_dir`: a folder the player can reach from a computer or the
///   phone's Files app (Android: the app's external files folder; iOS: the
///   app's Documents, shown in Files). Game files put there appear in the
///   launcher.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MobilePlatform {
    name: &'static str,
    data_dir: PathBuf,
    bundle_dir: PathBuf,
    import_dir: Option<PathBuf>,
    /// A folder the phone's Files app shows (Android: `Documents/Room`):
    /// screenshots go there.
    shared_dir: Option<PathBuf>,
}

impl MobilePlatform {
    /// A platform rooted at `data_dir` and `bundle_dir`.
    #[must_use]
    pub fn new(name: &'static str, data_dir: PathBuf, bundle_dir: PathBuf) -> Self {
        Self {
            name,
            data_dir,
            bundle_dir,
            import_dir: None,
            shared_dir: None,
        }
    }

    /// Also list games the player copies into `dir`.
    #[must_use]
    pub fn with_import_dir(mut self, dir: PathBuf) -> Self {
        self.import_dir = Some(dir);
        self
    }

    /// Save screenshots in `dir` (a folder the player can open), in
    /// `dir/Screenshots`.
    #[must_use]
    pub fn with_shared_dir(mut self, dir: PathBuf) -> Self {
        self.shared_dir = Some(dir);
        self
    }

    /// Writable app storage.
    #[must_use]
    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    /// Read-only shipped files.
    #[must_use]
    pub fn bundle_dir(&self) -> &Path {
        &self.bundle_dir
    }

    /// Where imported IWADs, PWADs and GRPs are copied (docs/41 M4).
    #[must_use]
    pub fn library_dir(&self) -> PathBuf {
        self.data_dir.join("library")
    }
}

impl Platform for MobilePlatform {
    fn name(&self) -> &'static str {
        self.name
    }

    fn config_file(&self) -> PathBuf {
        self.data_dir.join("room.ini")
    }

    fn screenshots_dir(&self) -> PathBuf {
        match &self.shared_dir {
            Some(dir) => dir.join("Screenshots"),
            None => self.data_dir.join("screenshots"),
        }
    }

    fn crash_dir(&self) -> PathBuf {
        self.data_dir.join("logs")
    }

    fn dev_root(&self) -> Option<PathBuf> {
        None
    }

    fn asset_roots(&self) -> Vec<PathBuf> {
        vec![self.bundle_dir.clone()]
    }

    fn iwad_search_dirs(&self) -> Vec<PathBuf> {
        let mut dirs: Vec<PathBuf> = self.import_dir.iter().cloned().collect();
        dirs.push(self.library_dir());
        dirs.push(self.bundle_dir.clone());
        dirs
    }

    fn library_dirs(&self) -> Vec<(PathBuf, bool)> {
        let mut dirs: Vec<(PathBuf, bool)> =
            self.import_dir.iter().map(|d| (d.clone(), false)).collect();
        dirs.push((self.library_dir(), false));
        dirs.push((self.bundle_dir.clone(), true));
        dirs
    }

    fn import_hint(&self) -> Option<String> {
        self.import_dir
            .as_ref()
            .map(|d| format!("copy WAD, PK3 or GRP files to {}", d.display()))
    }

    fn gzdoom_ini_candidates(&self) -> Vec<PathBuf> {
        Vec::new()
    }

    fn soundfont_candidates(&self) -> Vec<PathBuf> {
        [
            self.data_dir.join("soundfonts"),
            self.bundle_dir.join("soundfonts"),
        ]
        .iter()
        .flat_map(|root| SOUNDFONT_BANKS.iter().map(move |bank| root.join(bank)))
        .collect()
    }

    fn engine_extra_roots(&self) -> Vec<PathBuf> {
        vec![self.bundle_dir.clone()]
    }

    fn touch_screen(&self) -> bool {
        true
    }

    fn map_large_files(&self) -> bool {
        true
    }

    fn default_max_fps(&self) -> u32 {
        60
    }

    fn default_cvars(&self) -> Vec<(&'static str, &'static str)> {
        // Phones vary too much for one 3D resolution: start at full and let
        // it drop when frames run late (docs/41 M6). Settings can turn it off.
        // The frame-rate graph stays off (the CVar default); the pause menu
        // and the launcher's settings turn it on.
        vec![("r_dynres", "true")]
    }

    fn thermal(&self) -> Thermal {
        crate::os::thermal()
    }

    fn set_frame_rate(&self, window: &room_app::Window, fps: u32) {
        crate::os::set_frame_rate(window, fps);
    }
}

/// What a phone app starts with: the launcher screen (docs/41 M4).
#[must_use]
pub fn start_config() -> AppConfig {
    AppConfig {
        launcher_screen: true,
        ..AppConfig::default()
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    fn phone(root: &Path) -> MobilePlatform {
        MobilePlatform::new("test", root.join("data"), root.join("bundle"))
    }

    #[test]
    fn every_path_stays_inside_app_storage() {
        let root = Path::new("/app");
        let p = phone(root);
        let writable = [
            p.config_file(),
            p.saves_dir().expect("saves"),
            p.screenshots_dir(),
            p.crash_dir(),
            p.library_dir(),
        ];
        for path in writable {
            assert!(path.starts_with("/app/data"), "{}", path.display());
        }
        let readable = p
            .iwad_search_dirs()
            .into_iter()
            .chain(p.asset_roots())
            .chain(p.soundfont_candidates())
            .chain(p.engine_extra_roots());
        for path in readable {
            assert!(path.starts_with("/app"), "{}", path.display());
        }
        assert_eq!(p.dev_root(), None);
        assert!(p.gzdoom_ini_candidates().is_empty());
        assert!(
            p.touch_screen(),
            "phones show the touch controls from the start"
        );
        assert_eq!(
            p.default_max_fps(),
            60,
            "phones cap the frame rate for battery"
        );
        assert_eq!(
            p.default_cvars(),
            [("r_dynres", "true")],
            "phones lower the 3D resolution to keep speed; the frame-rate graph starts off"
        );
    }

    #[test]
    fn imported_games_are_searched_before_bundled_ones() {
        let p = phone(Path::new("/app"));
        assert_eq!(
            p.iwad_search_dirs(),
            vec![
                PathBuf::from("/app/data/library"),
                PathBuf::from("/app/bundle")
            ]
        );
        // A user SoundFont wins over the shipped one.
        assert!(p.soundfont_candidates()[0].starts_with("/app/data/soundfonts"));
    }

    #[test]
    fn the_import_folder_is_listed_first_and_named_in_the_launcher() {
        let p = phone(Path::new("/app")).with_import_dir("/sdcard/room".into());
        assert_eq!(
            p.library_dirs(),
            vec![
                (PathBuf::from("/sdcard/room"), false),
                (PathBuf::from("/app/data/library"), false),
                (PathBuf::from("/app/bundle"), true),
            ]
        );
        assert_eq!(p.iwad_search_dirs()[0], PathBuf::from("/sdcard/room"));
        assert!(p.import_hint().is_some_and(|h| h.contains("/sdcard/room")));
        assert_eq!(phone(Path::new("/app")).import_hint(), None);
    }

    #[test]
    fn screenshots_go_to_the_shared_folder_when_there_is_one() {
        let p = phone(Path::new("/app"));
        assert_eq!(p.screenshots_dir(), PathBuf::from("/app/data/screenshots"));
        let p = p.with_shared_dir("/storage/emulated/0/Documents/Room".into());
        assert_eq!(
            p.screenshots_dir(),
            PathBuf::from("/storage/emulated/0/Documents/Room/Screenshots")
        );
    }

    #[test]
    fn phones_start_in_the_launcher() {
        let c = start_config();
        assert!(c.launcher_screen && !c.play && c.iwad.is_none());
    }
}
