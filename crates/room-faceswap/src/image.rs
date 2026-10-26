//! Lightweight image buffers used inside the crate (not the Doom lump IR).

/// Row-major RGBA8 image.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RgbaImage {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// `width * height * 4` bytes (R,G,B,A).
    pub pixels: Vec<u8>,
}

impl RgbaImage {
    /// Empty opaque black image.
    #[must_use]
    pub fn new(width: u32, height: u32) -> Self {
        let n = (width as usize)
            .saturating_mul(height as usize)
            .saturating_mul(4);
        Self {
            width,
            height,
            pixels: vec![0; n],
        }
    }

    /// Sample with bilinear filtering (premultiplied alpha). Out of bounds → transparent.
    #[must_use]
    pub fn sample_bilinear(&self, x: f32, y: f32) -> [u8; 4] {
        if self.width == 0 || self.height == 0 {
            return [0, 0, 0, 0];
        }
        let w = self.width as f32;
        let h = self.height as f32;
        if !(0.0..w).contains(&x) || !(0.0..h).contains(&y) {
            return [0, 0, 0, 0];
        }
        let x0 = libm::floorf(x) as i32;
        let y0 = libm::floorf(y) as i32;
        let x1 = x0 + 1;
        let y1 = y0 + 1;
        let fx = x - x0 as f32;
        let fy = y - y0 as f32;
        let c00 = self.pixel_clamped(x0, y0);
        let c10 = self.pixel_clamped(x1, y0);
        let c01 = self.pixel_clamped(x0, y1);
        let c11 = self.pixel_clamped(x1, y1);
        let mut out = [0u8; 4];
        for i in 0..4 {
            let v = (1.0 - fx) * (1.0 - fy) * f32::from(c00[i])
                + fx * (1.0 - fy) * f32::from(c10[i])
                + (1.0 - fx) * fy * f32::from(c01[i])
                + fx * fy * f32::from(c11[i]);
            out[i] = (v + 0.5) as u8;
        }
        out
    }

    fn pixel_clamped(&self, x: i32, y: i32) -> [u8; 4] {
        let w = self.width as i32;
        let h = self.height as i32;
        if x < 0 || y < 0 || x >= w || y >= h {
            return [0, 0, 0, 0];
        }
        let i = ((y as u32 * self.width + x as u32) * 4) as usize;
        [
            self.pixels[i],
            self.pixels[i + 1],
            self.pixels[i + 2],
            self.pixels[i + 3],
        ]
    }

    /// Set one pixel (no bounds check beyond clamp).
    pub fn put(&mut self, x: u32, y: u32, rgba: [u8; 4]) {
        if x >= self.width || y >= self.height {
            return;
        }
        let i = ((y * self.width + x) * 4) as usize;
        self.pixels[i..i + 4].copy_from_slice(&rgba);
    }
}

/// Single-channel alpha mask (`0`..=`255`), same dimensions as a view.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AlphaMask {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// Row-major alpha.
    pub data: Vec<u8>,
}

impl AlphaMask {
    /// Fully transparent mask.
    #[must_use]
    pub fn zeros(width: u32, height: u32) -> Self {
        let n = (width as usize).saturating_mul(height as usize);
        Self {
            width,
            height,
            data: vec![0; n],
        }
    }

    /// Soft elliptical mask centred in the image (F0 default face matte).
    #[must_use]
    pub fn feathered_ellipse(width: u32, height: u32, soft: f32) -> Self {
        let mut m = Self::zeros(width, height);
        if width == 0 || height == 0 {
            return m;
        }
        let cx = (width as f32 - 1.0) * 0.5;
        let cy = (height as f32 - 1.0) * 0.52;
        let rx = width as f32 * 0.42;
        let ry = height as f32 * 0.48;
        let soft = soft.clamp(0.05, 0.5);
        for y in 0..height {
            for x in 0..width {
                let nx = (x as f32 - cx) / rx;
                let ny = (y as f32 - cy) / ry;
                let d = libm::sqrtf(nx * nx + ny * ny);
                let a = if d <= 1.0 - soft {
                    255u8
                } else if d >= 1.0 {
                    0u8
                } else {
                    let t = (1.0 - d) / soft;
                    ((t * 255.0) + 0.5) as u8
                };
                m.data[(y * width + x) as usize] = a;
            }
        }
        m
    }

    /// Sample nearest.
    #[must_use]
    pub fn get(&self, x: u32, y: u32) -> u8 {
        if x >= self.width || y >= self.height {
            return 0;
        }
        self.data[(y * self.width + x) as usize]
    }
}
