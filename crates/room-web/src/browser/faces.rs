//! wasm-bindgen surface for `room-faceswap` detect (docs/26 F3).

use room_faceswap::{RgbaImage, detect};
use wasm_bindgen::prelude::*;

/// Run heuristic face detection on RGBA8 pixels.
///
/// Returns a JSON array of `{bbox:[x,y,w,h], landmarks:[{x,y}×5], score}`.
#[wasm_bindgen(js_name = detectFacesRgba)]
pub fn detect_faces_rgba(width: u32, height: u32, pixels: &[u8]) -> String {
    let expected = (width as usize)
        .saturating_mul(height as usize)
        .saturating_mul(4);
    if width == 0 || height == 0 || pixels.len() < expected {
        return "[]".into();
    }
    let img = RgbaImage {
        width,
        height,
        pixels: pixels[..expected].to_vec(),
    };
    let faces = detect(&img);
    let mut out = String::from("[");
    for (i, f) in faces.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str("{\"bbox\":[");
        out.push_str(&format!(
            "{},{},{},{}",
            f.bbox.0, f.bbox.1, f.bbox.2, f.bbox.3
        ));
        out.push_str("],\"landmarks\":[");
        for (j, p) in f.landmarks.iter().enumerate() {
            if j > 0 {
                out.push(',');
            }
            out.push_str(&format!("{{\"x\":{},\"y\":{}}}", p.x, p.y));
        }
        out.push_str(&format!("],\"score\":{}}}", f.score));
    }
    out.push(']');
    out
}
