//! Binary lump parsers with no engine knowledge.

#![cfg_attr(not(test), deny(clippy::unwrap_used))]

pub mod animated;
pub mod bmp;
pub mod colormap;
pub mod error;
pub mod flac;
pub mod flat;
pub mod font;
pub mod image;
pub mod jpeg;
pub mod mp3;
pub mod mus;
pub mod ogg;
pub mod patch;
pub mod pcx;
pub mod playpal;
pub mod pnames;
pub mod png_img;
pub mod sound;
pub mod switches;
pub mod texturex;
pub mod tga;
pub mod tranmap;
pub mod wav;

pub use animated::{AnimatedRecord, parse_animated};
pub use bmp::{is_bmp, parse_bmp};
pub use colormap::Colormap;
pub use error::FormatError;
pub use flac::{is_flac, parse_flac};
pub use flat::{FlatSize, flat_dimensions};
pub use font::{Fon1, Fon2, FontGlyph, parse_fon1, parse_fon2};
pub use image::{Image, ImagePixels, PalettedImage, RgbaImage};
pub use jpeg::{is_jpeg, parse_jpeg};
pub use mp3::{is_mp3, parse_mp3};
pub use mus::{is_mus, mus_to_midi};
pub use ogg::{is_ogg, parse_ogg};
pub use patch::{Patch, PatchColumn, encode_patch, parse_patch};
pub use pcx::{is_pcx, parse_pcx};
pub use playpal::{Palette, Playpal, parse_playpal};
pub use pnames::{Pnames, parse_pnames};
pub use png_img::{PngZdoomMeta, is_png, parse_png, scan_zdoom_png_chunks};
pub use sound::{DecodedSound, decode_sound, is_dmx, parse_dmx};
pub use switches::{SwitchRecord, parse_switches};
pub use texturex::{TexturexDef, TexturexPatch, parse_texturex};
pub use tga::{is_tga, parse_tga};
pub use tranmap::{TRANMAP_SIZE, Tranmap};
pub use wav::{is_wav, parse_wav};

/// Detect and decode a common image lump (PNG / JPEG / BMP / TGA / Doom patch).
pub fn decode_image(data: &[u8]) -> Result<Image, FormatError> {
    if is_png(data) {
        return parse_png(data);
    }
    if is_jpeg(data) {
        return parse_jpeg(data);
    }
    if is_bmp(data) {
        return parse_bmp(data);
    }
    if is_pcx(data) {
        return parse_pcx(data);
    }
    if is_tga(data) {
        return parse_tga(data);
    }
    // Doom patch → paletted Image with an explicit alpha mask.
    // Unwritten column gaps stay alpha=0; written posts (including palette index 0)
    // get alpha=255 so TEXTUREX / sprites do not treat real colour 0 as a hole.
    let patch = parse_patch(data)?;
    let w = u32::from(patch.width);
    let h = u32::from(patch.height);
    let n = (w * h) as usize;
    let mut indices = vec![0u8; n];
    let mut alpha = vec![0u8; n];
    for (x, col) in patch.columns.iter().enumerate() {
        for post in &col.posts {
            let top = post.top as u32;
            for (i, &pix) in post.pixels.iter().enumerate() {
                let y = top + i as u32;
                if y < h {
                    let di = (y * w + x as u32) as usize;
                    indices[di] = pix;
                    alpha[di] = 255;
                }
            }
        }
    }
    let has_translucency = alpha.iter().any(|&a| a < 255);
    Ok(Image {
        left_offset: i32::from(patch.left_offset),
        top_offset: i32::from(patch.top_offset),
        has_translucency,
        pixels: ImagePixels::Paletted(PalettedImage {
            width: w,
            height: h,
            indices,
            alpha,
        }),
    })
}
