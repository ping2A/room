#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = room_formats::mus_to_midi(data);
    let _ = room_formats::parse_dmx(data);
    let _ = room_formats::parse_wav(data);
});
