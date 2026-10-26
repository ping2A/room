#![no_main]
//! Fuzz 7z/PK7 header + directory parsing (docs/03-vfs.md §9).
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    const MAGIC: &[u8; 6] = b"7z\xBC\xAF'\x1C";
    let mut bytes = Vec::with_capacity(data.len() + MAGIC.len());
    if data.len() < MAGIC.len() || data.get(..MAGIC.len()) != Some(MAGIC.as_slice()) {
        bytes.extend_from_slice(MAGIC);
        bytes.extend_from_slice(data);
    } else {
        bytes.extend_from_slice(data);
    }
    let _ = room_vfs::VfsBuilder::new().mount_memory("fuzz.pk7", bytes);
});
