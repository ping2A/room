//! Print which lump a sprite name resolves to and its decoded pixels summary.

// Dev tool: failing loudly on bad input is the right behaviour.
#![allow(clippy::unwrap_used, clippy::expect_used)]
use room_vfs::{Namespace, VfsBuilder};
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let mut b = room_game::mount_embedded_engine(VfsBuilder::new()).unwrap();
    for p in &a[2..] {
        b = b.mount_path(p).unwrap();
    }
    let vfs = b.build().unwrap();
    let name = &a[1];
    let id = vfs
        .find(name, Namespace::Sprites)
        .or_else(|| vfs.find_any_ns(name))
        .unwrap();
    let bytes = vfs.read(id).unwrap();
    println!("lump {id:?} len {}", bytes.len());
    let img = room_formats::decode_image(&bytes).unwrap();
    match &img.pixels {
        room_formats::ImagePixels::Rgba8(r) => {
            let n = r.pixels.len() / 4;
            let opaque = r.pixels.chunks(4).filter(|p| p[3] > 20).count();
            let maxp = r
                .pixels
                .chunks(4)
                .max_by_key(|p| p[3] as u32 * 1000 + p[0] as u32)
                .unwrap();
            println!(
                "rgba {}x{} px={n} alpha>20: {opaque} max={maxp:?} offs=({},{}) transl={}",
                r.width, r.height, img.left_offset, img.top_offset, img.has_translucency
            );
        }
        room_formats::ImagePixels::Paletted(p) => println!("paletted {}x{}", p.width, p.height),
    }
}
