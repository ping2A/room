//! Optional memory mapping of large game files (docs/41 M6).
//!
//! Off by default. Phones turn it on (`set_map_large_files`): a mapped
//! 160 MB PK3 is read from flash as needed and its pages are clean, so the OS
//! can drop them under memory pressure instead of killing the app, and iOS
//! does not count them against the app's memory limit. The cost: if the file
//! is changed or truncated while the game runs, reading it can crash the
//! process, which is why desktops keep reading files into memory.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

/// Files at least this big are mapped when mapping is on.
pub const MAP_THRESHOLD: u64 = 16 * 1024 * 1024;

static MAP_LARGE_FILES: AtomicBool = AtomicBool::new(false);

/// Map files of [`MAP_THRESHOLD`] bytes or more instead of reading them.
pub fn set_map_large_files(on: bool) {
    MAP_LARGE_FILES.store(on, Ordering::Relaxed);
}

/// Whether large files are mapped.
#[must_use]
pub fn map_large_files() -> bool {
    MAP_LARGE_FILES.load(Ordering::Relaxed)
}

/// Read `path` whole, mapping it instead when mapping is on and it is large.
pub(crate) fn load(path: &Path) -> std::io::Result<bytes::Bytes> {
    #[cfg(not(target_arch = "wasm32"))]
    if map_large_files() {
        let file = std::fs::File::open(path)?;
        if file.metadata()?.len() >= MAP_THRESHOLD {
            // SAFETY: the mapping stays valid while the returned `Bytes` (its
            // owner) lives. It is read-only; the one hazard is another process
            // truncating the file meanwhile, the documented trade-off above.
            #[allow(unsafe_code)]
            let map = unsafe { memmap2::Mmap::map(&file)? };
            return Ok(bytes::Bytes::from_owner(map));
        }
    }
    Ok(bytes::Bytes::from(std::fs::read(path)?))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    #[test]
    fn large_files_map_small_files_read_same_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let small = dir.path().join("small.wad");
        let large = dir.path().join("large.pk3");
        std::fs::write(&small, b"PWAD tiny").unwrap();
        let big: Vec<u8> = (0..MAP_THRESHOLD + 7).map(|i| (i % 253) as u8).collect();
        std::fs::write(&large, &big).unwrap();
        for on in [false, true] {
            set_map_large_files(on);
            assert_eq!(load(&small).unwrap().as_ref(), b"PWAD tiny");
            let b = load(&large).unwrap();
            assert_eq!(b.len(), big.len());
            assert_eq!(&b[..1000], &big[..1000]);
            assert_eq!(b.slice(b.len() - 7..).as_ref(), &big[big.len() - 7..]);
        }
        set_map_large_files(false);
        assert!(!map_large_files());
    }
}
