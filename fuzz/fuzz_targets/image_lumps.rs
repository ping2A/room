#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = room_formats::decode_image(data);
    let _ = room_formats::parse_fon1(data);
    let _ = room_formats::parse_fon2(data);
    let _ = room_formats::parse_texturex(data);
    let _ = room_formats::parse_animated(data);
    let _ = room_formats::parse_switches(data);
    let _ = room_formats::Tranmap::parse(data);
    let _ = room_formats::Colormap::parse(data);
});
