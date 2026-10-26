#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Must not panic on arbitrary `.fsave` ZIP / info.json bytes.
    let _ = room_game::read_save_info(data);
});
