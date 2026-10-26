#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(text) = std::str::from_utf8(data) {
        let _ = room_defs::parse_mapinfo(text);
        let _ = room_defs::parse_umapinfo(text);
        let _ = room_defs::parse_sndseq(text);
        let _ = room_defs::parse_sndinfo(text);
    }
});
