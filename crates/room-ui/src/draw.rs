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
        /// Doom `V_DrawPatch`: shift `pos` by the lump's left/top offsets.
        apply_offsets: bool,
        /// Draw through Doom's `INVERSECOLORMAP` (invulnerability psprites).
        inverse: bool,
        /// RGB scale 0–1 (sector light / extralight for psprites; docs/30 W6).
        light: f32,
        /// Spectre-style fuzz (partial invisibility psprite).
        fuzz: bool,
        /// `PSPF_FLIP`: mirror the patch left to right.
        flip: bool,
        /// `PSPF_MIRROR`: mirror the patch top to bottom.
        mirror: bool,
        /// Overlay rotation in degrees. 0 leaves the patch axis-aligned.
        angle: f32,
        /// Rotation pivot as a fraction of the patch. 0.5 is the center.
        pivot_x: f32,
        /// Rotation pivot Y.
        pivot_y: f32,
        /// RGB tint 0–1. White is untinted.
        tint: [f32; 3],
        /// Opacity 0–1 (ACS `HudMessage` fades / `HUDMSG_ALPHA`).
        alpha: f32,
        /// Font colour translation: glyph luminance mapped onto a dark → bright
        /// ramp (GZDoom `EColorRange`); `None` keeps the patch colours.
        recolor: Option<[[f32; 3]; 2]>,
        /// Corner offsets in virtual pixels (UZDoom `A_OverlayVertexOffset`
        /// order: top-left, bottom-left, top-right, bottom-right). All zero
        /// draws the plain rectangle.
        corners: [Vec2; 4],
    },
    /// Restrict the patches that follow to `rect` (virtual pixels); `None`
    /// lifts it (SBARINFO `DrawBar`).
    Clip {
        /// Visible area.
        rect: Option<Rect>,
    },
    /// Area filled with a repeating flat (textured automap, `am_textured`):
    /// the even-odd interior of `edges`, each pixel sampling the flat at
    /// `tex` × its virtual position (`u = t0·x + t1·y + t2`,
    /// `v = t3·x + t4·y + t5`, in texels).
    FlatPoly {
        /// Outline segments, virtual pixels (any order; holes allowed).
        edges: Vec<[Vec2; 2]>,
        /// Flat name ([`crate::PatchCache::ensure_flat`]).
        flat: String,
        /// Virtual position → texel affine map.
        tex: [f32; 6],
        /// Brightness 0–1.
        light: f32,
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
        self.patch_lit(name, pos, size, 1.0, false);
    }

    /// Push patch with psprite lighting / fuzz (docs/30 W6).
    pub fn patch_lit(&mut self, name: &str, pos: Vec2, size: Vec2, light: f32, fuzz: bool) {
        self.cmds.push(DrawCmd::Patch {
            name: name.to_owned(),
            pos,
            size,
            apply_offsets: false,
            inverse: false,
            light: light.clamp(0.0, 1.0),
            fuzz,
            flip: false,
            mirror: false,
            angle: 0.0,
            pivot_x: 0.5,
            pivot_y: 0.5,
            tint: [1.0, 1.0, 1.0],
            alpha: 1.0,
            recolor: None,
            corners: [Vec2::new(0.0, 0.0); 4],
        });
    }

    /// Same as [`Self::patch_lit`] with `PSPF_FLIP`.
    pub fn patch_lit_flip(
        &mut self,
        name: &str,
        pos: Vec2,
        size: Vec2,
        light: f32,
        fuzz: bool,
        flip: bool,
        mirror: bool,
        angle: f32,
        pivot_x: f32,
        pivot_y: f32,
        tint: [f32; 3],
    ) {
        self.cmds.push(DrawCmd::Patch {
            name: name.to_owned(),
            pos,
            size,
            apply_offsets: false,
            inverse: false,
            light: light.clamp(0.0, 1.0),
            fuzz,
            flip,
            mirror,
            angle,
            pivot_x,
            pivot_y,
            tint,
            alpha: 1.0,
            recolor: None,
            corners: [Vec2::new(0.0, 0.0); 4],
        });
    }

    /// Route every patch pushed since `start` through the inverse colormap
    /// (call with the command count taken before drawing the weapon).
    pub fn invert_since(&mut self, start: usize) {
        for cmd in self.cmds.iter_mut().skip(start) {
            if let DrawCmd::Patch { inverse, .. } = cmd {
                *inverse = true;
            }
        }
    }

    /// Set opacity and colour translation on the patches pushed since `start`.
    pub fn style_since(&mut self, start: usize, alpha: f32, recolor: Option<[[f32; 3]; 2]>) {
        for cmd in self.cmds.iter_mut().skip(start) {
            if let DrawCmd::Patch {
                alpha: a,
                recolor: r,
                ..
            } = cmd
            {
                *a = alpha.clamp(0.0, 1.0);
                *r = recolor;
            }
        }
    }

    /// Push a native-size picture placed by its lump offsets (`V_DrawPatch`).
    pub fn picture(&mut self, name: &str, pos: Vec2) {
        self.cmds.push(DrawCmd::Patch {
            name: name.to_owned(),
            pos,
            size: Vec2::new(0.0, 0.0),
            apply_offsets: true,
            inverse: false,
            light: 1.0,
            fuzz: false,
            flip: false,
            mirror: false,
            angle: 0.0,
            pivot_x: 0.5,
            pivot_y: 0.5,
            tint: [1.0, 1.0, 1.0],
            alpha: 1.0,
            recolor: None,
            corners: [Vec2::new(0.0, 0.0); 4],
        });
    }
}
