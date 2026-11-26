//! What to start: the settings the desktop command line used to hold
//! (docs/41 §3, M0). The `room` binary fills this from `clap`; a phone app
//! fills it from its launcher screen.

use crate::midi_device::MidiBackend;
use crate::netplay::NetTransportKind;
use std::path::{Path, PathBuf};

/// Start-up settings for one run of the app.
///
/// `Default` matches the `room` command line with no flags.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AppConfig {
    /// Run the native Build-based Duke runtime for a Duke 3D GRP.
    pub native_duke: bool,
    /// IWAD / PWAD / directory to mount (`None`: built-in test room).
    pub iwad: Option<PathBuf>,
    /// Add-on Duke GRPs mounted after the base GRP (native Duke only).
    pub grp: Vec<PathBuf>,
    /// Replacement native Duke root CON script.
    pub con: Option<PathBuf>,
    /// PWAD / PK3 / directories mounted after `iwad`.
    pub file: Vec<PathBuf>,
    /// Map lump name (Duke: `E1L1`) or a standalone `.MAP` path.
    pub map: String,
    /// Run the playsim (otherwise the free-fly map viewer).
    pub play: bool,
    /// Keep the title / main menu instead of entering `map` at once.
    pub title: bool,
    /// Built-in combat arena.
    pub combat_demo: bool,
    /// Doom skill 1..=5.
    pub skill: u8,
    /// Start with everything (`give all`).
    pub give_all: bool,
    /// Start with god mode on.
    pub god: bool,
    /// Save a screenshot after this many frames, then quit.
    pub shot_after: Option<u64>,
    /// Optional DECORATE file.
    pub decorate: Option<PathBuf>,
    /// GM SoundFont for MIDI/MUS music.
    pub soundfont: Option<PathBuf>,
    /// MIDI backend.
    pub mididevice: MidiBackend,
    /// List discovered IWADs and exit.
    pub launcher: bool,
    /// gzdoom.ini / room.ini to import before starting.
    pub import_ini: Option<PathBuf>,
    /// Host co-op on this bind address.
    pub host: Option<String>,
    /// Join a co-op host at this address.
    pub join: Option<String>,
    /// Player count when hosting.
    pub players: u8,
    /// Deathmatch instead of co-op.
    pub deathmatch: bool,
    /// Bot seats after the players (0..=7).
    pub bots: u8,
    /// Native Duke network mode: 0 = DukeMatch, 1 = co-op, 2 = no-spawn.
    pub duke_game_mode: u8,
    /// Damage between allied Duke players.
    pub duke_friendly_fire: bool,
    /// Net transport.
    pub transport: NetTransportKind,
    /// Encrypt lockstep payloads with Noise_XX.
    pub noise: bool,
    /// Local display name for netplay.
    pub name: String,
    /// `NAME=VALUE` CVars applied at start-up, after `room.ini`.
    pub set_cvars: Vec<String>,
    /// Open the launcher screen (game library) instead of a game.
    pub launcher_screen: bool,
    /// Measure this many frames (after a short warm-up), print frame times
    /// and peak memory, then quit.
    pub bench_frames: Option<u64>,
    /// Started from the launcher screen: quitting (and the end of a speed
    /// test) goes back to it instead of closing the app.
    pub from_launcher: bool,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            native_duke: false,
            iwad: None,
            grp: Vec::new(),
            con: None,
            file: Vec::new(),
            map: "MAP01".into(),
            play: false,
            title: false,
            combat_demo: false,
            skill: 3,
            give_all: false,
            god: false,
            shot_after: None,
            decorate: None,
            soundfont: None,
            mididevice: MidiBackend::Auto,
            launcher: false,
            import_ini: None,
            host: None,
            join: None,
            players: 2,
            deathmatch: false,
            bots: 0,
            duke_game_mode: 1,
            duke_friendly_fire: false,
            transport: NetTransportKind::Udp,
            noise: false,
            name: String::new(),
            set_cvars: Vec::new(),
            launcher_screen: false,
            bench_frames: None,
            from_launcher: false,
        }
    }
}

/// A combination of settings that cannot start; `exit_code` is what the
/// desktop binary exits with.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StartError {
    /// Message shown after `room: `.
    pub message: String,
    /// Process exit code.
    pub exit_code: i32,
}

impl std::fmt::Display for StartError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for StartError {}

impl AppConfig {
    /// Apply what one setting implies for others (native Duke plays its
    /// first level, joining a game skips the title, …).
    ///
    /// `dev_root` is the source checkout (default test files live there);
    /// `is_file` checks whether a default file exists. The `--launcher`
    /// listing and the combat demo's IWAD search are done by the caller,
    /// because they scan the disk.
    pub fn apply_implications(
        &mut self,
        dev_root: Option<&Path>,
        is_file: impl Fn(&Path) -> bool,
    ) -> Result<(), StartError> {
        if self.native_duke {
            if self.combat_demo {
                return Err(StartError {
                    message: "native Duke cannot be combined with --combat-demo".into(),
                    exit_code: 2,
                });
            }
            self.play = true;
            self.title = false;
            if self.iwad.is_none() {
                if let Some(root) = dev_root {
                    self.iwad = Some(root.join("testdata/duke/duke3d.grp"));
                }
            }
            if self.map.eq_ignore_ascii_case("MAP01") {
                self.map = "E1L1".into();
            }
        }
        if self.host.is_some() && self.join.is_some() {
            return Err(StartError {
                message: "use only one of --host / --join".into(),
                exit_code: 2,
            });
        }
        if self.host.is_some() || self.join.is_some() {
            self.play = true;
            self.title = false;
            if self.iwad.is_none() {
                if let Some(root) = dev_root {
                    let doom1 = root.join("testdata/doom/doom1.wad");
                    if is_file(&doom1) {
                        self.iwad = Some(doom1);
                        if self.map == "MAP01" {
                            self.map = "E1M1".into();
                        }
                    }
                }
            }
        }
        if self.combat_demo {
            self.play = true;
            self.title = false;
            // Instant arsenal so the fight is the focus; ground pickups still exist.
            self.give_all = true;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    fn apply(mut c: AppConfig, files: &[&str]) -> Result<AppConfig, StartError> {
        let root = PathBuf::from("/src/room");
        let files: Vec<PathBuf> = files.iter().map(PathBuf::from).collect();
        c.apply_implications(Some(&root), |p| files.iter().any(|f| f == p))?;
        Ok(c)
    }

    #[test]
    fn plain_config_is_unchanged() {
        let c = apply(AppConfig::default(), &[]).expect("ok");
        assert_eq!(c, AppConfig::default());
    }

    #[test]
    fn native_duke_plays_e1l1_from_the_checkout_grp() {
        let c = apply(
            AppConfig {
                native_duke: true,
                title: true,
                ..AppConfig::default()
            },
            &[],
        )
        .expect("ok");
        assert!(c.play && !c.title);
        assert_eq!(c.map, "E1L1");
        assert_eq!(
            c.iwad,
            Some(PathBuf::from("/src/room/testdata/duke/duke3d.grp"))
        );
        // Any spelling of MAP01 is replaced; other maps are kept.
        let c = apply(
            AppConfig {
                native_duke: true,
                map: "map01".into(),
                iwad: Some("/g/duke3d.grp".into()),
                ..AppConfig::default()
            },
            &[],
        )
        .expect("ok");
        assert_eq!(c.map, "E1L1");
        assert_eq!(c.iwad, Some(PathBuf::from("/g/duke3d.grp")));
        let c = apply(
            AppConfig {
                native_duke: true,
                map: "E2L3".into(),
                ..AppConfig::default()
            },
            &[],
        )
        .expect("ok");
        assert_eq!(c.map, "E2L3");
    }

    #[test]
    fn native_duke_without_checkout_keeps_iwad_unset() {
        let mut c = AppConfig {
            native_duke: true,
            ..AppConfig::default()
        };
        c.apply_implications(None, |_| true).expect("ok");
        assert_eq!(c.iwad, None);
    }

    #[test]
    fn conflicting_modes_fail_with_exit_code_2() {
        let e = apply(
            AppConfig {
                native_duke: true,
                combat_demo: true,
                ..AppConfig::default()
            },
            &[],
        )
        .expect_err("conflict");
        assert_eq!(e.exit_code, 2);
        assert_eq!(
            e.to_string(),
            "native Duke cannot be combined with --combat-demo"
        );
        let e = apply(
            AppConfig {
                host: Some("5029".into()),
                join: Some("1.2.3.4:5029".into()),
                ..AppConfig::default()
            },
            &[],
        )
        .expect_err("conflict");
        assert_eq!(e.exit_code, 2);
        assert_eq!(e.to_string(), "use only one of --host / --join");
    }

    #[test]
    fn netplay_skips_title_and_prefers_doom1_when_present() {
        let doom1 = "/src/room/testdata/doom/doom1.wad";
        let c = apply(
            AppConfig {
                join: Some("h:1".into()),
                title: true,
                ..AppConfig::default()
            },
            &[doom1],
        )
        .expect("ok");
        assert!(c.play && !c.title);
        assert_eq!(c.iwad, Some(PathBuf::from(doom1)));
        assert_eq!(c.map, "E1M1");
        // No doom1.wad: IWAD stays unset and the map is kept.
        let c = apply(
            AppConfig {
                host: Some("5029".into()),
                ..AppConfig::default()
            },
            &[],
        )
        .expect("ok");
        assert_eq!(c.iwad, None);
        assert_eq!(c.map, "MAP01");
        // An explicit map is never replaced.
        let c = apply(
            AppConfig {
                host: Some("5029".into()),
                map: "MAP07".into(),
                ..AppConfig::default()
            },
            &[doom1],
        )
        .expect("ok");
        assert_eq!(c.map, "MAP07");
    }

    #[test]
    fn combat_demo_plays_with_everything() {
        let c = apply(
            AppConfig {
                combat_demo: true,
                title: true,
                ..AppConfig::default()
            },
            &[],
        )
        .expect("ok");
        assert!(c.play && !c.title && c.give_all);
        assert_eq!(c.iwad, None, "IWAD search is the caller's job");
    }
}
