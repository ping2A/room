#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = ferrite_formats::mus_to_midi(data);
    let _ = ferrite_formats::parse_dmx(data);
    let _ = ferrite_formats::parse_wav(data);
});
