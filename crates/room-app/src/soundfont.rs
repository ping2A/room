//! Locate and load a GM SoundFont for map music.

use room_music::SoftFont;
use std::path::{Path, PathBuf};

/// Candidate paths for `snd_soundfont` / shipped GM banks.
pub fn soundfont_search_paths(explicit: Option<&Path>) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = explicit.map(Path::to_path_buf).into_iter().collect();
    out.extend(crate::platform::current().soundfont_candidates());
    out
}

/// Load the first readable SF2/SF3 from the search list.
pub fn load_first_soundfont(explicit: Option<&Path>) -> Option<(PathBuf, SoftFont)> {
    for path in soundfont_search_paths(explicit) {
        if !path.is_file() {
            continue;
        }
        match std::fs::read(&path) {
            Ok(bytes) => match SoftFont::load(&bytes) {
                Ok(font) => return Some((path, font)),
                Err(e) => eprintln!("room: soundfont {}: {e}", path.display()),
            },
            Err(e) => eprintln!("room: soundfont {}: {e}", path.display()),
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_paths_include_env_and_defaults() {
        let paths = soundfont_search_paths(Some(Path::new("/tmp/custom.sf2")));
        assert!(paths.iter().any(|p| p.ends_with("custom.sf2")));
        assert!(paths.iter().any(|p| p.ends_with("room.sf2")));
    }
}
