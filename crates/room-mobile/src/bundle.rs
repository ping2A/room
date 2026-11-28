//! Copy the files shipped inside the app package (APK `assets/`) into
//! [`MobilePlatform::bundle_dir`](crate::MobilePlatform::bundle_dir), where
//! the engine reads them as ordinary files.
//!
//! Android keeps APK assets compressed inside the package, behind its own
//! asset API; the engine's archive code wants seekable files. Copying once on
//! first run (and again when an app update ships a different file) costs a
//! few seconds and ~60 MB of storage for Freedoom 1 + 2.

use std::io::{self, Read};
use std::path::Path;

/// Files the app package may ship, relative to its asset root. Missing ones
/// are skipped (a build may leave the SoundFont or Brutal Doom out).
/// Brutal Doom and Room's Rust port of its ZScript ship like the web build's
/// "Play solo" bundle; the launcher ticks shipped mods by default.
pub const BUNDLED_FILES: [&str; 6] = [
    "freedoom2.wad",
    "freedoom1.wad",
    "soundfonts/TimGM6mb.sf2",
    "brutal.pk3",
    "brutal-zscript.wad",
    "BRUTALDOOM-LICENSE.txt",
];

/// Optional list of extra shipped files (Room's own mods), one file name per
/// line, written by `scripts/stage-mods.sh`. A copy is kept in the
/// destination so the next sync can drop files a newer package no longer
/// lists.
pub const MODS_LIST: &str = "bundled-mods.txt";

/// Names from a [`MODS_LIST`]: plain file names only (no folders, nothing
/// hidden), each once.
fn listed_mods(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for line in text.lines() {
        let name = line.trim();
        let plain = !name.is_empty()
            && !name.starts_with('.')
            && !name.contains(['/', '\\'])
            && name != MODS_LIST;
        if plain && !out.iter().any(|n| n.eq_ignore_ascii_case(name)) {
            out.push(name.to_owned());
        }
    }
    out
}

fn read_list(source: &dyn BundleSource) -> io::Result<Option<String>> {
    if source.len(MODS_LIST).is_none() {
        return Ok(None);
    }
    let mut text = String::new();
    source.open(MODS_LIST)?.read_to_string(&mut text)?;
    Ok(Some(text))
}

/// Where shipped files come from (APK assets on Android, a folder in tests).
pub trait BundleSource {
    /// Size in bytes, or `None` when the package does not contain `name`.
    fn len(&self, name: &str) -> Option<u64>;
    /// Open `name` for reading.
    ///
    /// # Errors
    /// The file exists but cannot be read.
    fn open(&self, name: &str) -> io::Result<Box<dyn Read + '_>>;
}

/// What [`sync_bundle`] did.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SyncReport {
    /// Copied now (new, or a different size from the copy on disk).
    pub copied: Vec<String>,
    /// Already on disk with the right size.
    pub kept: Vec<String>,
    /// Not in this package.
    pub missing: Vec<String>,
}

/// Make `dest` hold every [`BUNDLED_FILES`] entry `source` has.
///
/// A file is written to `<name>.part` and renamed into place, so an app
/// killed mid-copy never leaves a truncated WAD that looks complete.
///
/// # Errors
/// Creating a folder, reading an asset or writing a file failed.
pub fn sync_bundle(source: &dyn BundleSource, dest: &Path) -> io::Result<SyncReport> {
    let mut report = SyncReport::default();
    let list = read_list(source)?;
    let mods = list.as_deref().map(listed_mods).unwrap_or_default();
    // Files the previous package listed and this one does not.
    if let Ok(old) = std::fs::read_to_string(dest.join(MODS_LIST)) {
        for name in listed_mods(&old) {
            if !mods.iter().any(|m| m.eq_ignore_ascii_case(&name)) {
                let _ = std::fs::remove_file(dest.join(&name));
            }
        }
    }
    let names = BUNDLED_FILES
        .iter()
        .map(|n| (*n).to_owned())
        .chain(mods.iter().cloned());
    for name in names {
        let name = name.as_str();
        let Some(len) = source.len(name) else {
            report.missing.push(name.to_owned());
            continue;
        };
        let target = dest.join(name);
        if std::fs::metadata(&target).is_ok_and(|m| m.is_file() && m.len() == len) {
            report.kept.push(name.to_owned());
            continue;
        }
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let part = target.with_extension("part");
        {
            let mut reader = source.open(name)?;
            let mut file = std::fs::File::create(&part)?;
            let copied = io::copy(&mut reader, &mut file)?;
            if copied != len {
                drop(file);
                let _ = std::fs::remove_file(&part);
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    format!("{name}: copied {copied} of {len} bytes"),
                ));
            }
            file.sync_all()?;
        }
        std::fs::rename(&part, &target)?;
        report.copied.push(name.to_owned());
    }
    match list {
        Some(text) => {
            std::fs::create_dir_all(dest)?;
            std::fs::write(dest.join(MODS_LIST), text)?;
        }
        None => {
            let _ = std::fs::remove_file(dest.join(MODS_LIST));
        }
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use std::collections::HashMap;
    use std::path::PathBuf;

    struct Memory(HashMap<&'static str, Vec<u8>>);

    impl BundleSource for Memory {
        fn len(&self, name: &str) -> Option<u64> {
            self.0.get(name).map(|b| b.len() as u64)
        }
        fn open(&self, name: &str) -> io::Result<Box<dyn Read + '_>> {
            let bytes = self.0.get(name).ok_or(io::ErrorKind::NotFound)?;
            Ok(Box::new(bytes.as_slice()))
        }
    }

    /// Claims more bytes than it delivers, like an asset cut short.
    struct Short;

    impl BundleSource for Short {
        fn len(&self, name: &str) -> Option<u64> {
            (name == "freedoom2.wad").then_some(10)
        }
        fn open(&self, _name: &str) -> io::Result<Box<dyn Read + '_>> {
            Ok(Box::new(&b"IWAD"[..]))
        }
    }

    fn temp(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("room-bundle-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn listed_mods_are_copied_and_dropped_when_unlisted() {
        let dir = temp("mods");
        let src = Memory(HashMap::from([
            ("freedoom2.wad", b"IWAD-two".to_vec()),
            (
                MODS_LIST,
                b"colorworld.wad\nsurvival.wad\n../evil.wad\n.hidden\nsurvival.wad\n\n".to_vec(),
            ),
            ("colorworld.wad", b"PWAD-color".to_vec()),
            ("survival.wad", b"PWAD-survival".to_vec()),
        ]));
        let r = sync_bundle(&src, &dir).expect("sync");
        assert_eq!(
            r.copied,
            ["freedoom2.wad", "colorworld.wad", "survival.wad"]
        );
        assert_eq!(
            std::fs::read(dir.join("survival.wad")).unwrap(),
            b"PWAD-survival"
        );
        assert!(!dir.join("../evil.wad").exists());
        // The next package drops Survival: its copy goes, Color World stays.
        let src = Memory(HashMap::from([
            ("freedoom2.wad", b"IWAD-two".to_vec()),
            (MODS_LIST, b"colorworld.wad\n".to_vec()),
            ("colorworld.wad", b"PWAD-color".to_vec()),
        ]));
        let r = sync_bundle(&src, &dir).expect("sync again");
        assert!(r.copied.is_empty());
        assert!(!dir.join("survival.wad").exists());
        assert!(dir.join("colorworld.wad").exists());
        // A package with no list removes every listed mod.
        let src = Memory(HashMap::from([("freedoom2.wad", b"IWAD-two".to_vec())]));
        sync_bundle(&src, &dir).expect("sync without mods");
        assert!(!dir.join("colorworld.wad").exists());
        assert!(!dir.join(MODS_LIST).exists());
        assert!(dir.join("freedoom2.wad").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn first_run_copies_then_keeps() {
        let dir = temp("first");
        let src = Memory(HashMap::from([
            ("freedoom2.wad", b"IWAD-two".to_vec()),
            ("soundfonts/TimGM6mb.sf2", b"RIFF".to_vec()),
        ]));
        let r = sync_bundle(&src, &dir).expect("sync");
        assert_eq!(r.copied, ["freedoom2.wad", "soundfonts/TimGM6mb.sf2"]);
        assert_eq!(
            r.missing,
            [
                "freedoom1.wad",
                "brutal.pk3",
                "brutal-zscript.wad",
                "BRUTALDOOM-LICENSE.txt"
            ]
        );
        assert_eq!(
            std::fs::read(dir.join("freedoom2.wad")).expect("read"),
            b"IWAD-two"
        );
        assert_eq!(
            std::fs::read(dir.join("soundfonts/TimGM6mb.sf2")).expect("read"),
            b"RIFF"
        );
        let r = sync_bundle(&src, &dir).expect("sync again");
        assert!(r.copied.is_empty());
        assert_eq!(r.kept, ["freedoom2.wad", "soundfonts/TimGM6mb.sf2"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_update_with_a_different_file_replaces_it() {
        let dir = temp("update");
        std::fs::create_dir_all(&dir).expect("mkdir");
        std::fs::write(dir.join("freedoom2.wad"), b"old").expect("write");
        let src = Memory(HashMap::from([("freedoom2.wad", b"IWAD-newer".to_vec())]));
        let r = sync_bundle(&src, &dir).expect("sync");
        assert_eq!(r.copied, ["freedoom2.wad"]);
        assert_eq!(
            std::fs::read(dir.join("freedoom2.wad")).expect("read"),
            b"IWAD-newer"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_short_copy_fails_and_leaves_nothing_behind() {
        let dir = temp("short");
        let err = sync_bundle(&Short, &dir).expect_err("short");
        assert_eq!(err.kind(), io::ErrorKind::UnexpectedEof);
        assert!(!dir.join("freedoom2.wad").exists());
        assert!(!dir.join("freedoom2.part").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
