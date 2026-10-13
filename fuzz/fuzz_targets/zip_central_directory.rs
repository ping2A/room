#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let mut bytes = Vec::with_capacity(data.len() + 4);
    if data.len() < 4 || &data[0..2] != b"PK" {
        bytes.extend_from_slice(b"PK\x03\x04");
        bytes.extend_from_slice(data);
    } else {
        bytes.extend_from_slice(data);
    }
    let _ = ferrite_vfs::VfsBuilder::new().mount_memory("fuzz.pk3", bytes);
});
