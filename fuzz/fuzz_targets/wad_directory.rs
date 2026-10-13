#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = room_vfs::VfsBuilder::new().mount_memory("fuzz.wad", data.to_vec());
});
