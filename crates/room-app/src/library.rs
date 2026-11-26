//! The game library (docs/41 M4): every game and mod file the app can see,
//! identified by its contents rather than its name, with a reason for each
//! file it cannot use.

use std::io::Read;
use std::path::{Path, PathBuf};

/// What a file is, from its first bytes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum FileKind {
    /// Doom-engine game data (`IWAD`).
    Iwad,
    /// Doom-engine add-on (`PWAD`).
    Pwad,
    /// Zip archive (`.pk3` / `.zip` / `.ipk3`).
    Pk3,
    /// Build-engine group file (`KenSilverman`).
    Grp,
    /// Not usable; the reason is shown in the launcher.
    Unusable(String),
}

/// Which engine a game runs on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Engine {
    /// Doom, Heretic, Hexen, Strife, Freedoom, … (IWAD + PWAD / PK3).
    Doom,
    /// Duke Nukem 3D (GRP + add-on GRPs).
    Duke,
}

/// One usable file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Entry {
    /// Where it is.
    pub path: PathBuf,
    /// File name.
    pub file: String,
    /// Name shown in the launcher.
    pub title: String,
    /// Size in bytes.
    pub size: u64,
    /// Shipped with the app (cannot be removed).
    pub bundled: bool,
    /// Engine it belongs to.
    pub engine: Engine,
    /// Files loaded right after this one whenever it is (see [`COMPANIONS`]);
    /// they are not listed on their own.
    pub companions: Vec<PathBuf>,
}

/// Everything found by [`scan`].
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Library {
    /// Playable base games (IWADs, Duke GRPs), in folder order.
    pub games: Vec<Entry>,
    /// Add-ons (PWADs, PK3s, add-on GRPs).
    pub mods: Vec<Entry>,
    /// `(file name, reason)` for files that were skipped.
    pub problems: Vec<(String, String)>,
}

impl Library {
    /// Mods that load on top of `game`.
    pub(crate) fn mods_for(&self, game: &Entry) -> Vec<&Entry> {
        self.mods
            .iter()
            .filter(|m| m.engine == game.engine)
            .collect()
    }

    /// Mods ticked when `game` is picked: the ones shipped with the app
    /// (phones ship Brutal Doom, so Freedoom starts with it).
    pub(crate) fn default_mods(&self, game: &Entry) -> Vec<PathBuf> {
        self.mods_for(game)
            .into_iter()
            .filter(|m| m.bundled)
            .map(|m| m.path.clone())
            .collect()
    }
}

/// `(companion file, prefix of the mod it goes with)`: Room's Rust port of
/// Brutal Doom's ZScript (`mods/brutal-zscript-rs`) loads on top of any
/// `brutal*.pk3`, the way the web build loads it.
const COMPANIONS: [(&str, &str); 1] = [("brutal-zscript.wad", "brutal")];

/// Attach companion files to the mods they go with and take them off the
/// mod list. A companion with no matching mod stays an ordinary mod.
fn attach_companions(lib: &mut Library) {
    for (companion, prefix) in COMPANIONS {
        let Some(at) = lib
            .mods
            .iter()
            .position(|m| m.file.eq_ignore_ascii_case(companion))
        else {
            continue;
        };
        let owner = |m: &Entry| {
            let lower = m.file.to_ascii_lowercase();
            lower.starts_with(prefix) && lower.ends_with(".pk3")
        };
        if !lib.mods.iter().any(owner) {
            continue;
        }
        let entry = lib.mods.remove(at);
        for m in lib.mods.iter_mut().filter(|m| owner(m)) {
            m.companions.push(entry.path.clone());
        }
    }
}

/// Names people know their IWAD by.
const KNOWN: [(&str, &str); 17] = [
    ("freedoom1.wad", "Freedoom: Phase 1"),
    ("freedoom2.wad", "Freedoom: Phase 2"),
    ("freedm.wad", "FreeDM"),
    ("doom.wad", "The Ultimate Doom"),
    ("doom1.wad", "Doom (shareware)"),
    ("doom2.wad", "Doom II"),
    ("doom2f.wad", "Doom II (French)"),
    ("plutonia.wad", "Final Doom: Plutonia"),
    ("tnt.wad", "Final Doom: TNT"),
    ("heretic.wad", "Heretic"),
    ("heretic1.wad", "Heretic (shareware)"),
    ("hexen.wad", "Hexen"),
    ("strife1.wad", "Strife"),
    ("chex.wad", "Chex Quest"),
    ("duke3d.grp", "Duke Nukem 3D"),
    ("nam.grp", "NAM"),
    ("brutal.pk3", "Brutal Doom"),
];

/// Title for a game file.
fn title_for(file: &str) -> String {
    let lower = file.to_ascii_lowercase();
    KNOWN
        .iter()
        .find(|(name, _)| *name == lower)
        .map_or_else(|| file.to_owned(), |(_, title)| (*title).to_owned())
}

/// Identify `path` from its header and, for WAD and GRP files, check the
/// directory fits in the file (a truncated copy is caught here, not when
/// the game starts).
pub(crate) fn identify(path: &Path) -> std::io::Result<FileKind> {
    let len = std::fs::metadata(path)?.len();
    let mut head = [0u8; 16];
    let mut file = std::fs::File::open(path)?;
    let mut got = 0;
    while got < head.len() {
        let n = file.read(&mut head[got..])?;
        if n == 0 {
            break;
        }
        got += n;
    }
    let head = &head[..got];
    let u32_at = |at: usize| -> u64 {
        head.get(at..at + 4).map_or(0, |b| {
            u64::from(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        })
    };
    if head.starts_with(b"IWAD") || head.starts_with(b"PWAD") {
        if head.len() < 12 {
            return Ok(FileKind::Unusable("truncated WAD header".into()));
        }
        let lumps = u32_at(4);
        let dir = u32_at(8);
        if dir + lumps * 16 > len {
            return Ok(FileKind::Unusable(format!(
                "truncated: its directory ends at byte {}, the file has {len}",
                dir + lumps * 16
            )));
        }
        return Ok(if head.starts_with(b"IWAD") {
            FileKind::Iwad
        } else {
            FileKind::Pwad
        });
    }
    if head.starts_with(b"PK\x03\x04") || head.starts_with(b"PK\x05\x06") {
        return Ok(FileKind::Pk3);
    }
    if head.starts_with(b"KenSilverman") {
        let files = u32_at(12);
        if 16 + files * 16 > len {
            return Ok(FileKind::Unusable("truncated GRP directory".into()));
        }
        return Ok(FileKind::Grp);
    }
    Ok(FileKind::Unusable("not a WAD, PK3 or GRP file".into()))
}

/// Extensions worth looking at (others are ignored without a note).
fn candidate(file: &str) -> bool {
    let lower = file.to_ascii_lowercase();
    [".wad", ".iwad", ".pk3", ".ipk3", ".zip", ".pk7", ".grp"]
        .iter()
        .any(|ext| lower.ends_with(ext))
}

/// Scan `dirs` (`(folder, bundled)`) in order. A file name seen in an
/// earlier folder hides the same name later, so an imported copy replaces a
/// bundled one.
pub(crate) fn scan(dirs: &[(PathBuf, bool)]) -> Library {
    let mut lib = Library::default();
    let mut seen = std::collections::BTreeSet::new();
    for (dir, bundled) in dirs {
        let Ok(read) = std::fs::read_dir(dir) else {
            continue;
        };
        let mut paths: Vec<PathBuf> = read.flatten().map(|e| e.path()).collect();
        paths.sort();
        for path in paths {
            let Some(file) = path.file_name().and_then(|n| n.to_str()).map(str::to_owned) else {
                continue;
            };
            if !path.is_file() || !candidate(&file) || !seen.insert(file.to_ascii_lowercase()) {
                continue;
            }
            let kind = match identify(&path) {
                Ok(kind) => kind,
                Err(e) => FileKind::Unusable(e.to_string()),
            };
            let size = std::fs::metadata(&path).map_or(0, |m| m.len());
            let lower = file.to_ascii_lowercase();
            let entry = |engine| Entry {
                path: path.clone(),
                file: file.clone(),
                title: title_for(&file),
                size,
                bundled: *bundled,
                engine,
                companions: Vec::new(),
            };
            match kind {
                FileKind::Iwad => lib.games.push(entry(Engine::Doom)),
                FileKind::Pwad | FileKind::Pk3 => lib.mods.push(entry(Engine::Doom)),
                // Base Duke GRPs are games; other GRPs are add-ons.
                FileKind::Grp if lower.starts_with("duke3d") || lower == "nam.grp" => {
                    lib.games.push(entry(Engine::Duke));
                }
                FileKind::Grp => lib.mods.push(entry(Engine::Duke)),
                FileKind::Unusable(why) => lib.problems.push((file, why)),
            }
        }
    }
    attach_companions(&mut lib);
    lib
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    fn wad(magic: &[u8; 4], lumps: u32, dir: u32, len: usize) -> Vec<u8> {
        let mut b = Vec::new();
        b.extend_from_slice(magic);
        b.extend_from_slice(&lumps.to_le_bytes());
        b.extend_from_slice(&dir.to_le_bytes());
        b.resize(len.max(12), 0);
        b
    }

    fn grp(files: u32, len: usize) -> Vec<u8> {
        let mut b = b"KenSilverman".to_vec();
        b.extend_from_slice(&files.to_le_bytes());
        b.resize(len.max(16), 0);
        b
    }

    fn dir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("room-lib-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn files_are_identified_by_content_and_checked_for_truncation() {
        let d = dir("ident");
        let cases: Vec<(&str, Vec<u8>, FileKind)> = vec![
            ("a.wad", wad(b"IWAD", 2, 12, 12 + 32), FileKind::Iwad),
            ("b.wad", wad(b"PWAD", 1, 12, 12 + 16), FileKind::Pwad),
            ("c.pk3", b"PK\x03\x04rest".to_vec(), FileKind::Pk3),
            ("d.grp", grp(2, 16 + 32), FileKind::Grp),
        ];
        for (name, bytes, want) in cases {
            std::fs::write(d.join(name), bytes).unwrap();
            assert_eq!(identify(&d.join(name)).unwrap(), want, "{name}");
        }
        // Cut short: the directory runs past the end.
        std::fs::write(d.join("short.wad"), wad(b"IWAD", 100, 12, 40)).unwrap();
        assert!(matches!(
            identify(&d.join("short.wad")).unwrap(),
            FileKind::Unusable(why) if why.starts_with("truncated")
        ));
        std::fs::write(d.join("short.grp"), grp(100, 40)).unwrap();
        assert!(matches!(
            identify(&d.join("short.grp")).unwrap(),
            FileKind::Unusable(_)
        ));
        // A text file renamed to .wad.
        std::fs::write(d.join("notes.wad"), b"hello").unwrap();
        assert!(matches!(
            identify(&d.join("notes.wad")).unwrap(),
            FileKind::Unusable(_)
        ));
        std::fs::write(d.join("empty.wad"), b"").unwrap();
        assert!(matches!(
            identify(&d.join("empty.wad")).unwrap(),
            FileKind::Unusable(_)
        ));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn scan_sorts_games_from_mods_and_reports_problems() {
        let imported = dir("imported");
        let bundle = dir("bundle");
        std::fs::write(bundle.join("freedoom2.wad"), wad(b"IWAD", 0, 12, 12)).unwrap();
        std::fs::write(bundle.join("freedoom1.wad"), wad(b"IWAD", 0, 12, 12)).unwrap();
        // The imported Freedoom 2 hides the bundled one.
        std::fs::write(imported.join("FREEDOOM2.WAD"), wad(b"IWAD", 0, 12, 14)).unwrap();
        std::fs::write(imported.join("brutal.pk3"), b"PK\x03\x04").unwrap();
        std::fs::write(imported.join("map.wad"), wad(b"PWAD", 0, 12, 12)).unwrap();
        std::fs::write(imported.join("DUKE3D.GRP"), grp(0, 16)).unwrap();
        std::fs::write(imported.join("dukedc.grp"), grp(0, 16)).unwrap();
        std::fs::write(imported.join("broken.wad"), b"oops").unwrap();
        std::fs::write(imported.join("readme.txt"), b"ignored").unwrap();
        let lib = scan(&[(imported.clone(), false), (bundle.clone(), true)]);
        let games: Vec<(&str, &str, bool)> = lib
            .games
            .iter()
            .map(|g| (g.file.as_str(), g.title.as_str(), g.bundled))
            .collect();
        assert_eq!(
            games,
            [
                ("DUKE3D.GRP", "Duke Nukem 3D", false),
                ("FREEDOOM2.WAD", "Freedoom: Phase 2", false),
                ("freedoom1.wad", "Freedoom: Phase 1", true),
            ]
        );
        let doom = lib.games.iter().find(|g| g.engine == Engine::Doom).unwrap();
        let doom_mods: Vec<&str> = lib.mods_for(doom).iter().map(|m| m.file.as_str()).collect();
        assert_eq!(doom_mods, ["brutal.pk3", "map.wad"]);
        let duke = &lib.games[0];
        let duke_mods: Vec<&str> = lib.mods_for(duke).iter().map(|m| m.file.as_str()).collect();
        assert_eq!(duke_mods, ["dukedc.grp"]);
        assert_eq!(lib.problems.len(), 1);
        assert_eq!(lib.problems[0].0, "broken.wad");
        // Nothing is bundled here, so nothing is ticked by default.
        assert!(lib.default_mods(doom).is_empty());
        // Missing folders are fine.
        assert_eq!(scan(&[(imported.join("nope"), false)]), Library::default());
        let _ = std::fs::remove_dir_all(&imported);
        let _ = std::fs::remove_dir_all(&bundle);
    }

    #[test]
    fn shipped_brutal_doom_is_ticked_and_brings_its_script_port() {
        let bundle = dir("bundle-brutal");
        std::fs::write(bundle.join("freedoom2.wad"), wad(b"IWAD", 0, 12, 12)).unwrap();
        std::fs::write(bundle.join("brutal.pk3"), b"PK\x03\x04").unwrap();
        std::fs::write(bundle.join("brutal-zscript.wad"), wad(b"PWAD", 0, 12, 12)).unwrap();
        let imported = dir("imported-brutal");
        std::fs::write(imported.join("map.wad"), wad(b"PWAD", 0, 12, 12)).unwrap();
        let lib = scan(&[(imported.clone(), false), (bundle.clone(), true)]);
        let game = &lib.games[0];
        let mods: Vec<(&str, &str)> = lib
            .mods_for(game)
            .iter()
            .map(|m| (m.file.as_str(), m.title.as_str()))
            .collect();
        // The port is not listed on its own; it rides along with Brutal Doom.
        assert_eq!(
            mods,
            [("map.wad", "map.wad"), ("brutal.pk3", "Brutal Doom")]
        );
        let brutal = lib.mods.iter().find(|m| m.file == "brutal.pk3").unwrap();
        assert_eq!(brutal.companions, [bundle.join("brutal-zscript.wad")]);
        // Shipped mods are ticked by default; the player's own are not.
        assert_eq!(lib.default_mods(game), [bundle.join("brutal.pk3")]);
        let _ = std::fs::remove_dir_all(&imported);
        let _ = std::fs::remove_dir_all(&bundle);
    }

    #[test]
    fn a_companion_without_its_mod_stays_a_mod() {
        let d = dir("lonely-port");
        std::fs::write(d.join("doom2.wad"), wad(b"IWAD", 0, 12, 12)).unwrap();
        std::fs::write(d.join("brutal-zscript.wad"), wad(b"PWAD", 0, 12, 12)).unwrap();
        let lib = scan(&[(d.clone(), false)]);
        assert_eq!(lib.mods.len(), 1);
        assert!(lib.mods[0].companions.is_empty());
        let _ = std::fs::remove_dir_all(&d);
    }
}
