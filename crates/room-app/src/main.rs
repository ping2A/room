//! The `room` desktop binary: command line → [`AppConfig`] → [`room_app::run`].

use clap::Parser;
use room_app::{AppConfig, MidiBackend, NetTransportKind};
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(name = "room", about = "Room map viewer / play shell (M4–M5)")]
struct Args {
    /// Run the native Build-based Duke runtime for a Duke 3D GRP.
    #[arg(long, default_value_t = false)]
    native_duke: bool,
    /// IWAD / PWAD / directory to mount (optional; uses built-in test room).
    #[arg(long)]
    iwad: Option<PathBuf>,
    /// Add-on Duke GRP mounted after the base GRP (repeatable; native Duke only).
    #[arg(long, requires = "native_duke")]
    grp: Vec<PathBuf>,
    /// Replace the native Duke root CON script (`-x` style; includes resolve from GRPs).
    #[arg(long, requires = "native_duke")]
    con: Option<PathBuf>,
    /// PWAD / PK3 / directory mounted after `--iwad` (repeatable; e.g. WASM mods).
    #[arg(long)]
    file: Vec<PathBuf>,
    /// Map lump name (Duke: E1L1) or a standalone `.MAP` path.
    #[arg(long, default_value = "MAP01")]
    map: String,
    /// Run Headless playsim and drive the player with WASD (M5 bootstrap).
    #[arg(long, default_value_t = false)]
    play: bool,
    /// Keep the title / main menu instead of entering `--map` immediately (with `--play`).
    #[arg(long, default_value_t = false)]
    title: bool,
    /// Built-in combat arena (monsters + weapons/ammo). Implies `--play`, skips title.
    /// Uses `--iwad` for sprites/textures (defaults to `testdata/freedoom/freedoom2.wad`).
    #[arg(long, default_value_t = false)]
    combat_demo: bool,
    /// Doom skill 1..=5 (Hurt Me Plenty = 3).
    #[arg(long, default_value_t = 3)]
    skill: u8,
    /// Start with all weapons, ammo, keys, and armor (`give all`).
    #[arg(long, default_value_t = false)]
    give_all: bool,
    /// Start with god mode on (`iddqd` / console `god`). Handy with `--combat-demo`.
    #[arg(long, default_value_t = false)]
    god: bool,
    /// Save a screenshot after this many frames, then quit (smoke checks).
    #[arg(long)]
    shot_after: Option<u64>,
    /// Optional DECORATE file (defaults to testdata/actors/doom_basic.txt when --play).
    #[arg(long)]
    decorate: Option<PathBuf>,
    /// GM SoundFont (`.sf2`/`.sf3`) for MIDI/MUS music. Also searches `soundfonts/`,
    /// `$ROOM_SOUNDFONT`, and common bank names.
    #[arg(long)]
    soundfont: Option<PathBuf>,
    /// MIDI backend: `auto` (SF2 then OPL), `sf2`, or `opl` (IWAD `GENMIDI`).
    #[arg(long, value_enum, default_value_t = MidiBackend::Auto)]
    mididevice: MidiBackend,
    /// List discovered IWADs and exit (launcher bootstrap).
    #[arg(long, default_value_t = false)]
    launcher: bool,
    /// Import a gzdoom.ini / room.ini before starting.
    #[arg(long)]
    import_ini: Option<PathBuf>,
    /// Host coop (`--host 5029`). Implies `--play`. Waits for `--players` joiners.
    #[arg(long)]
    host: Option<String>,
    /// Join coop host (`--join 127.0.0.1:5029`). Implies `--play`.
    #[arg(long)]
    join: Option<String>,
    /// Coop player count when hosting (2..=4 UDP/TCP; 2..=8 with `--transport tcp-server`).
    #[arg(long, default_value_t = 2)]
    players: u8,
    /// Deathmatch instead of co-op: alone against `--bots`, or when hosting
    /// (`--host`; joiners take the host's rules).
    #[arg(long, default_value_t = false)]
    deathmatch: bool,
    /// Bot seats after the players (0..=7): fill a deathmatch or co-op game.
    #[arg(long, default_value_t = 0, value_parser = clap::value_parser!(u8).range(0..=7))]
    bots: u8,
    /// Native Duke network mode: 0 = DukeMatch, 1 = co-op, 2 = no-spawn.
    #[arg(
        long,
        requires = "native_duke",
        default_value_t = 1,
        value_parser = clap::value_parser!(u8).range(0..=2)
    )]
    duke_game_mode: u8,
    /// Enable damage between allied Duke players (native Duke network only).
    #[arg(long, requires = "native_duke", default_value_t = false)]
    duke_friendly_fire: bool,
    /// Net transport: `udp` (default), `tcp` (P2P), or `tcp-server` (host packet-server relay).
    #[arg(long, value_enum, default_value_t = NetTransportKind::Udp)]
    transport: NetTransportKind,
    /// Encrypt lockstep payloads with Noise_XX (after lobby; UDP/TCP/tcp-server).
    #[arg(long, default_value_t = false)]
    noise: bool,
    /// Local display name for netplay / session (also `name` cvar when `--import-ini`).
    #[arg(long, default_value = "")]
    name: String,
    /// Set a CVar at startup (`--set gl_bloom=1`, repeatable). Applied after
    /// `room.ini` / `--import-ini` so launcher menus can override graphics.
    #[arg(long = "set", value_name = "NAME=VALUE")]
    set_cvars: Vec<String>,
    /// Open the launcher screen (games found in the IWAD folders, mods,
    /// touch settings) instead of starting a game.
    #[arg(long, default_value_t = false)]
    launcher_screen: bool,
    /// Measure this many frames after a 10-frame warm-up, print frame times
    /// and peak memory, then quit (performance checks).
    #[arg(long, value_name = "N")]
    bench_frames: Option<u64>,
}

impl From<Args> for AppConfig {
    fn from(a: Args) -> Self {
        Self {
            native_duke: a.native_duke,
            iwad: a.iwad,
            grp: a.grp,
            con: a.con,
            file: a.file,
            map: a.map,
            play: a.play,
            title: a.title,
            combat_demo: a.combat_demo,
            skill: a.skill,
            give_all: a.give_all,
            god: a.god,
            shot_after: a.shot_after,
            decorate: a.decorate,
            soundfont: a.soundfont,
            mididevice: a.mididevice,
            launcher: a.launcher,
            import_ini: a.import_ini,
            host: a.host,
            join: a.join,
            players: a.players,
            deathmatch: a.deathmatch,
            bots: a.bots,
            duke_game_mode: a.duke_game_mode,
            duke_friendly_fire: a.duke_friendly_fire,
            transport: a.transport,
            noise: a.noise,
            name: a.name,
            set_cvars: a.set_cvars,
            launcher_screen: a.launcher_screen,
            bench_frames: a.bench_frames,
            from_launcher: false,
        }
    }
}

fn main() {
    room_app::platform::install(std::sync::Arc::new(
        room_app::DesktopPlatform::from_process(),
    ));
    room_app::install_crash_handler();
    if let Err(e) = room_app::run(Args::parse().into()) {
        // `run` already printed the reason.
        std::process::exit(e.exit_code);
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use clap::CommandFactory;

    fn parse(line: &[&str]) -> AppConfig {
        let mut argv = vec!["room"];
        argv.extend_from_slice(line);
        Args::try_parse_from(argv).expect("parses").into()
    }

    #[test]
    fn clap_definition_is_valid() {
        Args::command().debug_assert();
    }

    #[test]
    fn no_flags_is_the_default_config() {
        assert_eq!(parse(&[]), AppConfig::default());
    }

    #[test]
    fn every_flag_reaches_the_config() {
        let c = parse(&[
            "--native-duke",
            "--iwad",
            "/g/duke3d.grp",
            "--grp",
            "/g/a.grp",
            "--grp",
            "/g/b.grp",
            "--con",
            "/m/GAME.CON",
            "--file",
            "/m/x.pk3",
            "--map",
            "E2L1",
            "--play",
            "--title",
            "--skill",
            "4",
            "--give-all",
            "--god",
            "--shot-after",
            "30",
            "--decorate",
            "/d.txt",
            "--soundfont",
            "/s.sf2",
            "--mididevice",
            "sf2",
            "--launcher",
            "--import-ini",
            "/i.ini",
            "--host",
            "5029",
            "--players",
            "3",
            "--deathmatch",
            "--bots",
            "2",
            "--duke-game-mode",
            "0",
            "--duke-friendly-fire",
            "--transport",
            "tcp-server",
            "--noise",
            "--name",
            "anthony",
            "--set",
            "gl_bloom=1",
            "--set",
            "r_fov=100",
            "--launcher-screen",
            "--bench-frames",
            "600",
        ]);
        let expected = AppConfig {
            native_duke: true,
            iwad: Some("/g/duke3d.grp".into()),
            grp: vec!["/g/a.grp".into(), "/g/b.grp".into()],
            con: Some("/m/GAME.CON".into()),
            file: vec!["/m/x.pk3".into()],
            map: "E2L1".into(),
            play: true,
            title: true,
            combat_demo: false,
            skill: 4,
            give_all: true,
            god: true,
            shot_after: Some(30),
            decorate: Some("/d.txt".into()),
            soundfont: Some("/s.sf2".into()),
            mididevice: MidiBackend::SoftFont,
            launcher: true,
            import_ini: Some("/i.ini".into()),
            host: Some("5029".into()),
            join: None,
            players: 3,
            deathmatch: true,
            bots: 2,
            duke_game_mode: 0,
            duke_friendly_fire: true,
            transport: NetTransportKind::TcpServer,
            noise: true,
            name: "anthony".into(),
            set_cvars: vec!["gl_bloom=1".into(), "r_fov=100".into()],
            launcher_screen: true,
            bench_frames: Some(600),
            from_launcher: false,
        };
        assert_eq!(c, expected);
        let c = parse(&[
            "--join",
            "1.2.3.4:5029",
            "--combat-demo",
            "--transport",
            "tcp",
            "--mididevice",
            "opl",
        ]);
        assert_eq!(c.join.as_deref(), Some("1.2.3.4:5029"));
        assert!(c.combat_demo);
        assert_eq!(c.transport, NetTransportKind::Tcp);
        assert_eq!(c.mididevice, MidiBackend::Opl);
    }

    #[test]
    fn range_and_requires_rules_still_apply() {
        assert!(Args::try_parse_from(["room", "--bots", "8"]).is_err());
        assert!(Args::try_parse_from(["room", "--duke-game-mode", "3", "--native-duke"]).is_err());
        assert!(Args::try_parse_from(["room", "--grp", "x.grp"]).is_err());
        assert!(Args::try_parse_from(["room", "--con", "x.con"]).is_err());
        assert!(Args::try_parse_from(["room", "--duke-friendly-fire"]).is_err());
    }
}
