#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Must not panic on arbitrary `world.bin` layout (capped counts inside).
    let _ = ferrite_game::validate_world_bin(data);
});
