#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = ferrite_formats::decode_image(data);
    let _ = ferrite_formats::parse_fon1(data);
    let _ = ferrite_formats::parse_fon2(data);
    let _ = ferrite_formats::parse_texturex(data);
    let _ = ferrite_formats::parse_animated(data);
    let _ = ferrite_formats::parse_switches(data);
    let _ = ferrite_formats::Tranmap::parse(data);
    let _ = ferrite_formats::Colormap::parse(data);
});
