//! 2D canvas for HUD / menus (CPU command list; GPU later).

/// One 2D draw command.
#[derive(Clone, Debug, PartialEq)]
pub enum CanvasCmd {
    /// Clear the 2D layer.
    Clear {
        /// RGBA 0–1.
        color: [f32; 4],
    },
    /// Axis-aligned filled rect in screen pixels.
    FillRect {
        /// Left.
        x: f32,
        /// Top.
        y: f32,
        /// Width.
        w: f32,
        /// Height.
        h: f32,
        /// RGBA.
        color: [f32; 4],
    },
    /// Pre-rasterized RGBA8 image (`w*h*4` bytes, straight alpha).
    ///
    /// Used for Doom weapon / HUD patches so the CPU path is one blit instead of
    /// thousands of per-span [`Self::FillRect`] commands.
    BlitRgba {
        /// Left.
        x: f32,
        /// Top.
        y: f32,
        /// Width in pixels.
        w: u32,
        /// Height in pixels.
        h: u32,
        /// Row-major RGBA8.
        pixels: Vec<u8>,
    },
    /// Text run (font atlas later).
    Text {
        /// Left.
        x: f32,
        /// Baseline Y.
        y: f32,
        /// UTF-8 string.
        text: String,
        /// RGBA.
        color: [f32; 4],
    },
}

/// Accumulates 2D commands for one frame.
#[derive(Clone, Debug, Default)]
pub struct Canvas {
    /// Ordered commands.
    pub cmds: Vec<CanvasCmd>,
}

impl Canvas {
    /// Empty canvas.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Clear.
    pub fn clear(&mut self, color: [f32; 4]) {
        self.cmds.push(CanvasCmd::Clear { color });
    }

    /// Filled rectangle.
    pub fn fill_rect(&mut self, x: f32, y: f32, w: f32, h: f32, color: [f32; 4]) {
        self.cmds.push(CanvasCmd::FillRect { x, y, w, h, color });
    }

    /// Blit a pre-rasterized RGBA8 image.
    pub fn blit_rgba(&mut self, x: f32, y: f32, w: u32, h: u32, pixels: Vec<u8>) {
        self.cmds.push(CanvasCmd::BlitRgba { x, y, w, h, pixels });
    }

    /// Text.
    pub fn text(&mut self, x: f32, y: f32, text: impl Into<String>, color: [f32; 4]) {
        self.cmds.push(CanvasCmd::Text {
            x,
            y,
            text: text.into(),
            color,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_cmds() {
        let mut c = Canvas::new();
        c.clear([0.0, 0.0, 0.0, 0.0]);
        c.fill_rect(0.0, 0.0, 10.0, 10.0, [1.0, 0.0, 0.0, 1.0]);
        c.text(8.0, 16.0, "hi", [1.0; 4]);
        assert_eq!(c.cmds.len(), 3);
    }
}
