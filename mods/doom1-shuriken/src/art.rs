//! Generate every shuriken sprite patch (`docs/32` §3.3 / SK0).

#![deny(clippy::float_arithmetic)]

use crate::palette::Palette;
use crate::raster::{self, Canvas};
use crate::tables::{SPIN_ANGLES, SPRITE_LUMPS};
use alloc::string::String;
use alloc::vec::Vec;

/// One generated sprite.
pub struct SpriteLump {
    /// Doom short name.
    pub name: String,
    /// Encoded patch bytes.
    pub data: Vec<u8>,
}

/// Generate all SK0 sprites for `pal`.
///
/// # Errors
///
/// Patch encoder rejection (should not happen for these sizes).
pub fn generate_all(pal: &Palette) -> Result<Vec<SpriteLump>, &'static str> {
    let mut out = Vec::with_capacity(SPRITE_LUMPS.len());
    for (i, &name) in SPRITE_LUMPS.iter().enumerate() {
        let (canvas, left, top) = match name {
            "SHKMA0" | "SHKMB0" | "SHKMC0" | "SHKMD0" => {
                let ang = SPIN_ANGLES[i];
                (raster::paint_star(pal, 16, ang, 4), 8, 8)
            }
            "SHKSA0" => (stuck_star(pal), 8, 10),
            "SHKPA0" => (pickup_star(pal), 10, 12),
            "SHKBA0" => (ammo_box(pal), 12, 16),
            "SHKWA0" => (weapon_pickup(pal), 14, 16),
            "SHKGA0" => (psprite_hand(pal, 0), -180, -130),
            "SHKGB0" => (psprite_hand(pal, 1), -180, -130),
            "SHKGC0" => (psprite_hand(pal, 2), -180, -130),
            "SHKGD0" => (psprite_hand(pal, 3), -180, -130),
            "SHKGE0" => (psprite_hand(pal, 4), -180, -130),
            "SHKIA0" => (raster::paint_star(pal, 16, 0, 5), 0, 0),
            _ => return Err("unknown sprite"),
        };
        let data = raster::to_patch(&canvas, left, top)?;
        out.push(SpriteLump {
            name: String::from(name),
            data,
        });
    }
    Ok(out)
}

fn stuck_star(pal: &Palette) -> Canvas {
    let mut c = Canvas::new(16, 10);
    let star = raster::paint_star(pal, 16, 32, 3);
    // Copy bottom half of the star into the short canvas.
    for y in 0..10u32 {
        for x in 0..16u32 {
            let sy = y + 6;
            let si = (sy * 16 + x) as usize;
            let di = (y * 16 + x) as usize;
            if star.mask[si] != 0 {
                c.indices[di] = star.indices[si];
                c.mask[di] = 255;
            }
        }
    }
    c
}

fn pickup_star(pal: &Palette) -> Canvas {
    let mut c = Canvas::new(20, 12);
    let star = raster::paint_star(pal, 16, 8, 4);
    for y in 0..12u32 {
        for x in 0..16u32 {
            let si = (y.min(15) * 16 + x) as usize;
            let di = (y * 20 + x + 2) as usize;
            if y < 16 && star.mask.get(si).copied().unwrap_or(0) != 0 {
                c.indices[di] = star.indices[si];
                c.mask[di] = 255;
            }
        }
    }
    c
}

fn ammo_box(pal: &Palette) -> Canvas {
    let mut c = Canvas::new(24, 16);
    let wood = pal.index(110, 72, 40);
    let dark = pal.index(40, 24, 12);
    for y in 2..14 {
        for x in 2..22 {
            c.indices[(y * 24 + x) as usize] = wood;
            c.mask[(y * 24 + x) as usize] = 255;
        }
    }
    // Top rim.
    for x in 2..22 {
        c.indices[(2 * 24 + x) as usize] = dark;
    }
    let star = raster::paint_star(pal, 8, 0, 5);
    for (ox, oy) in [(4i32, 4), (10, 3), (15, 5)] {
        blit(&mut c, &star, ox, oy);
    }
    c
}

fn weapon_pickup(pal: &Palette) -> Canvas {
    let mut c = Canvas::new(28, 16);
    let cloth = pal.index(50, 70, 40);
    for y in 8..15 {
        for x in 2..26 {
            c.indices[(y * 28 + x) as usize] = cloth;
            c.mask[(y * 28 + x) as usize] = 255;
        }
    }
    let star = raster::paint_star(pal, 8, 16, 5);
    for (i, ox) in [3i32, 8, 13, 18, 22].iter().enumerate() {
        let ang_star = raster::paint_star(pal, 8, (i as u8).wrapping_mul(12), 5);
        blit(&mut c, if i % 2 == 0 { &star } else { &ang_star }, *ox, 2);
    }
    c
}

fn psprite_hand(pal: &Palette, pose: u8) -> Canvas {
    let (w, h, hand_y) = match pose {
        0 => (120u32, 80u32, 40i32),
        1 => (130, 90, 30),
        2 => (110, 70, 35),
        3 => (110, 70, 45),
        _ => (130, 90, 28),
    };
    let mut c = Canvas::new(w, h);
    let glove = pal.index(48, 90, 48);
    let dark = pal.index(20, 40, 20);
    // Simple glove block bottom-right.
    let gx = w as i32 - 50;
    for y in hand_y..(h as i32 - 4) {
        for x in gx..(w as i32 - 8) {
            c.indices[(y * w as i32 + x) as usize] = glove;
            c.mask[(y * w as i32 + x) as usize] = 255;
        }
    }
    // Fingers.
    for f in 0..4 {
        let fx = gx + 8 + f * 8;
        for y in (hand_y - 12)..hand_y {
            for x in fx..(fx + 5) {
                if x >= 0 && y >= 0 && (x as u32) < w && (y as u32) < h {
                    c.indices[(y * w as i32 + x) as usize] = glove;
                    c.mask[(y * w as i32 + x) as usize] = 255;
                }
            }
        }
    }
    c.outline(dark);
    let n_stars = if pose == 4 { 5 } else if pose == 2 { 0 } else { 3 };
    let star = raster::paint_star(pal, 12, pose.wrapping_mul(20), 6);
    for i in 0..n_stars {
        let ox = gx - 10 + i * 10;
        let oy = hand_y - 18 - (i % 2) * 4;
        blit(&mut c, &star, ox, oy);
    }
    c
}

fn blit(dst: &mut Canvas, src: &Canvas, ox: i32, oy: i32) {
    for y in 0..src.h as i32 {
        for x in 0..src.w as i32 {
            let si = (y * src.w as i32 + x) as usize;
            if src.mask[si] == 0 {
                continue;
            }
            let dx = ox + x;
            let dy = oy + y;
            if dx < 0 || dy < 0 || dx as u32 >= dst.w || dy as u32 >= dst.h {
                continue;
            }
            let di = (dy * dst.w as i32 + dx) as usize;
            dst.indices[di] = src.indices[si];
            dst.mask[di] = 255;
        }
    }
}
