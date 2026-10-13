#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = ferrite_vfs::VfsBuilder::new().mount_memory("fuzz.wad", data.to_vec());
});
