//! Fuzz `room_proto::parse_binary` / `parse_control_bytes`.
#![no_main]

use libfuzzer_sys::fuzz_target;
use room_proto::{parse_binary, parse_control_bytes};

fuzz_target!(|data: &[u8]| {
    let _ = parse_binary(data);
    let _ = parse_control_bytes(data);
});
