//! Decode every PNG in a zip-like dir tree list (paths on argv) and report failures.

#![allow(clippy::unwrap_used)] // A dev tool: an unreadable path should stop it.
fn main() {
    let mut fail = std::collections::BTreeMap::<String, u32>::new();
    let mut n = 0;
    let mut bad = 0;
    let mut ex = Vec::new();
    for p in std::env::args().skip(1) {
        let b = std::fs::read(&p).unwrap();
        if !room_formats::is_png(&b) {
            continue;
        }
        n += 1;
        let r = room_formats::decode_image(&b);
        let ok = match &r {
            Ok(img) => match &img.pixels {
                room_formats::ImagePixels::Rgba8(r) => {
                    r.pixels.len() == (r.width * r.height * 4) as usize
                }
                _ => true,
            },
            Err(_) => false,
        };
        if !ok {
            bad += 1;
            let k = match &r {
                Err(e) => format!("err {e}"),
                Ok(_) => "wrong pixel count".into(),
            };
            *fail.entry(k).or_default() += 1;
            if r.is_err() {
                eprintln!("ERRFILE {p}");
            }
            if ex.len() < 6 {
                ex.push(p.clone());
            }
        }
    }
    println!("png {n} bad {bad} {fail:?} {ex:?}");
}
