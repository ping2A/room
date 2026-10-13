//! Immediate-mode UI draw list.

/// 2D point in virtual HUD coordinates (320×200 base).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Vec2 {
    /// X.
    pub x: f32,
    /// Y.
    pub y: f32,
}

impl Vec2 {
    /// Construct.
    #[must_use]
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

/// Axis-aligned rect.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    /// Left.
    pub x: f32,
    /// Top.
    pub y: f32,
    /// Width.
    pub w: f32,
    /// Height.
    pub h: f32,
}

impl Rect {
    /// Construct.
    #[must_use]
    pub const fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h }
    }
}

/// Premultiplied-friendly RGBA 0..=1.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rgba {
    /// Red.
    pub r: f32,
    /// Green.
    pub g: f32,
    /// Blue.
    pub b: f32,
    /// Alpha.
    pub a: f32,
}

impl Rgba {
    /// Opaque white.
    pub const WHITE: Self = Self {
        r: 1.0,
        g: 1.0,
        b: 1.0,
        a: 1.0,
    };
    /// Doom status-bar grey.
    pub const STBAR_BG: Self = Self {
        r: 0.2,
        g: 0.2,
        b: 0.2,
        a: 1.0,
    };
    /// Health red.
    pub const HEALTH: Self = Self {
        r: 0.85,
        g: 0.15,
        b: 0.1,
        a: 1.0,
    };
    /// Armor green.
    pub const ARMOR: Self = Self {
        r: 0.2,
        g: 0.75,
        b: 0.25,
        a: 1.0,
    };
    /// Ammo yellow.
    pub const AMMO: Self = Self {
        r: 0.9,
        g: 0.85,
        b: 0.2,
        a: 1.0,
    };

    /// Construct.
    #[must_use]
    pub const fn new(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self { r, g, b, a }
    }
}

/// One draw command.
#[derive(Clone, Debug, PartialEq)]
pub enum DrawCmd {
    /// Solid fill.
    Fill {
        /// Destination.
        rect: Rect,
        /// Colour.
        color: Rgba,
    },
    /// Bitmap text (logical font name; renderer resolves glyphs).
    Text {
        /// Font key (`SmallFont`, `HudBig`, …).
        font: String,
        /// Baseline position.
        pos: Vec2,
        /// UTF-8 text.
        text: String,
        /// Colour.
        color: Rgba,
        /// Uniform scale.
        scale: f32,
    },
    /// Named patch / texture (status bar faces, STBAR, …).
    Patch {
        /// Lump / texture name.
        name: String,
        /// Destination top-left.
        pos: Vec2,
        /// Optional size override (0 = native).
        size: Vec2,
    },
    /// Coloured line segment (automap).
    Line {
        /// Start.
        a: Vec2,
        /// End.
        b: Vec2,
        /// Colour.
        color: Rgba,
        /// Thickness in virtual pixels.
        thickness: f32,
    },
}

/// Commands for one frame.
#[derive(Clone, Debug, Default)]
pub struct UiDrawList {
    /// Ordered draw cmds.
    pub cmds: Vec<DrawCmd>,
    /// Virtual width (default 320).
    pub virtual_w: f32,
    /// Virtual height (default 200).
    pub virtual_h: f32,
}

impl UiDrawList {
    /// Empty 320×200 list.
    #[must_use]
    pub fn new() -> Self {
        Self {
            cmds: Vec::new(),
            virtual_w: 320.0,
            virtual_h: 200.0,
        }
    }

    /// Push fill.
    pub fn fill(&mut self, rect: Rect, color: Rgba) {
        self.cmds.push(DrawCmd::Fill { rect, color });
    }

    /// Push text.
    pub fn text(
        &mut self,
        font: &str,
        pos: Vec2,
        text: impl Into<String>,
        color: Rgba,
        scale: f32,
    ) {
        self.cmds.push(DrawCmd::Text {
            font: font.to_owned(),
            pos,
            text: text.into(),
            color,
            scale,
        });
    }

    /// Push patch.
    pub fn patch(&mut self, name: &str, pos: Vec2, size: Vec2) {
        self.cmds.push(DrawCmd::Patch {
            name: name.to_owned(),
            pos,
            size,
        });
    }
}
