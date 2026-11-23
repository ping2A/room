//! The vanilla renderer: `r_main.c`, `r_bsp.c`, `r_segs.c`, `r_plane.c` and
//! `r_things.c` and the column / span drawers of `r_draw.c` (Chocolate Doom, GPL-2.0-or-later),
//! ported to one state struct. Fixed-point arithmetic and table lookups are
//! kept as in the C code so frames can match Chocolate Doom pixel for pixel.
//!
//! Differences on purpose: the visplane, drawseg and opening limits grow
//! instead of stopping or crashing unless [`SoftRenderer::vanilla_limits`]
//! is set, and the view can be any size.

mod ffloor;
mod portal;
mod quads;
mod slices;
mod voxels;

pub use voxels::{SoftLight, SoftParticle, SoftVoxelPose};

pub use portal::MAX_PORTAL_DEPTH;
use portal::PL_PORTAL;

pub use quads::{SoftDecal, SoftSpriteKind};

use crate::data::{NUMCOLORMAPS, SoftData};
use crate::fixed::{
    ANG45, ANG90, ANG180, ANGLETOFINESHIFT, FINEANGLES, FRACBITS, FRACUNIT, finecosine, finesine,
    finetangent, fixed_div, fixed_mul, tantoangle,
};
use crate::level::{
    FlatXform, ML_DONTPEGBOTTOM, ML_DONTPEGTOP, PART_BOTTOM, PART_MID, PART_TOP, SKY_FLAT,
    SoftLevel, SoftSector, SoftSeg, SoftSlope, SoftVertex,
};

/// `LIGHTLEVELS`.
const LIGHTLEVELS: usize = 16;
/// `LIGHTSEGSHIFT`.
const LIGHTSEGSHIFT: i32 = 4;
/// `MAXLIGHTSCALE`.
const MAXLIGHTSCALE: usize = 48;
/// `LIGHTSCALESHIFT`.
const LIGHTSCALESHIFT: i32 = 12;
/// `MAXLIGHTZ`.
const MAXLIGHTZ: usize = 128;
/// `LIGHTZSHIFT`.
const LIGHTZSHIFT: i32 = 20;
/// `DISTMAP`.
const DISTMAP: i32 = 2;
/// `FIELDOFVIEW`.
const FIELDOFVIEW: usize = 2048;
/// `HEIGHTBITS` / `HEIGHTUNIT`.
const HEIGHTBITS: i32 = 12;
const HEIGHTUNIT: i32 = 1 << HEIGHTBITS;
/// `ANGLETOSKYSHIFT`.
const ANGLETOSKYSHIFT: u32 = 22;
/// The width Doom's light tables are written for (`SCREENWIDTH`).
const ORIGWIDTH: i32 = 320;
/// Vanilla `MAXDRAWSEGS` / `MAXVISPLANES` (with [`SoftRenderer::vanilla_limits`]).
const MAXDRAWSEGS: usize = 256;
const MAXVISPLANES: usize = 128;
/// "Unset" in a visplane's top row (`0xff` in vanilla's bytes).
const PLANE_UNSET: u16 = u16::MAX;
/// `SIL_*`.
const SIL_BOTTOM: u8 = 1;
const SIL_TOP: u8 = 2;

/// `MAXVISSPRITES`.
const MAXVISSPRITES: usize = 128;
/// `MINZ`.
const MINZ: i32 = 4 * FRACUNIT;
/// `BASEYCENTER`.
const BASEYCENTER: i32 = 100;
/// `FF_FULLBRIGHT`.
pub const FF_FULLBRIGHT: u32 = 0x8000;
/// `FF_FRAMEMASK`.
pub const FF_FRAMEMASK: u32 = 0x7fff;
/// `MF_SHADOW` (spectres, invisibility): drawn with the fuzz effect.
pub const MF_SHADOW: u32 = 0x40000;
/// `MF_TRANSLATION`: player colour (`>> MF_TRANSSHIFT`, 0–3).
pub const MF_TRANSLATION: u32 = 0x0c00_0000;
/// `MF_TRANSSHIFT`.
pub const MF_TRANSSHIFT: u32 = 26;
/// Boom / MBF `MF_TRANSLUCENT`: drawn through the translucency table.
pub const MF_TRANSLUCENT: u32 = 0x8000_0000;
/// Visplane picture of a Boom sky transfer: this bit plus the side index.
const PL_SKYFLAT: u32 = 0x8000_0000;
/// Masked column already drawn (`MAXSHORT`).
const MASKED_DONE: i16 = i16::MAX;
/// `FUZZTABLE` with `FUZZOFF` as ±1 row.
const FUZZOFFSET: [i8; 50] = [
    1, -1, 1, -1, 1, 1, -1, 1, 1, -1, 1, 1, 1, -1, 1, 1, 1, -1, -1, -1, -1, 1, -1, -1, 1, 1, 1, 1,
    -1, 1, -1, 1, 1, -1, -1, 1, 1, -1, -1, -1, -1, 1, 1, 1, 1, -1, 1, 1, -1, 1,
];

/// ZDoom render styles beyond vanilla's (SR6). `Normal` draws as Doom
/// does (with [`MF_SHADOW`] / [`MF_TRANSLUCENT`] still honoured).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SoftStyle {
    /// Opaque (or the mobj flags' fuzz / Boom translucency).
    #[default]
    Normal,
    /// `alpha` blend over the background.
    Translucent,
    /// Additive.
    Add,
    /// Background minus the sprite.
    Subtract,
    /// The sprite's shape in the fill colour, blended by `alpha`.
    Stencil,
    /// Additive stencil.
    AddStencil,
    /// The fill colour with each texel's palette index as its opacity.
    Shaded,
    /// Additive shaded.
    AddShaded,
}

/// A map thing to draw (one `mobj_t` as `R_ProjectSprite` reads it).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SoftThing {
    /// Position (16.16).
    pub x: i32,
    /// Position (16.16).
    pub y: i32,
    /// Bottom (16.16).
    pub z: i32,
    /// Facing (BAM).
    pub angle: u32,
    /// Sprite name (`TROO`), see [`sprite_name`].
    pub sprite: [u8; 4],
    /// Frame (`A` = 0) with [`FF_FULLBRIGHT`].
    pub frame: u32,
    /// `mobj_t::flags` ([`MF_SHADOW`], [`MF_TRANSLATION`] are read).
    pub flags: u32,
    /// Sector index the thing is in (`subsector->sector`).
    pub sector: u32,
    /// Render style (SR6).
    pub style: SoftStyle,
    /// Opacity 0–255 for the blending styles.
    pub alpha: u8,
    /// Fill colour 0xRRGGBB for stencil and shaded styles.
    pub fill: u32,
    /// Horizontal scale (16.16; `0` = 1.0).
    pub scale_x: i32,
    /// Vertical scale (16.16; `0` = 1.0).
    pub scale_y: i32,
    /// Mirror the sprite (`+XFLIP`).
    pub xflip: bool,
    /// Billboard, wall or flat sprite (SR6).
    pub kind: SoftSpriteKind,
    /// Spin in the picture's plane (BAM, `+ROLLSPRITE`; 0 = none).
    pub roll: u32,
    /// Spin about the picture's centre instead of its origin
    /// (`+ROLLCENTER`).
    pub roll_center: bool,
    /// Upside down (`+YFLIP`).
    pub yflip: bool,
    /// Drawn as a voxel model instead of the sprite (VOXELDEF, SR8).
    pub voxel: Option<SoftVoxelPose>,
}

/// One weapon layer (`pspdef_t`): the weapon or its flash.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SoftPSprite {
    /// Sprite name, see [`sprite_name`].
    pub sprite: [u8; 4],
    /// Frame with [`FF_FULLBRIGHT`].
    pub frame: u32,
    /// `sx` (16.16, 0 = left edge of the 320 screen; vanilla rests at 1.0).
    pub sx: i32,
    /// `sy` (16.16, `WEAPONTOP` = 32.0).
    pub sy: i32,
}

/// A sprite name as the 4 bytes [`SoftThing::sprite`] holds.
#[must_use]
pub fn sprite_name(name: &str) -> [u8; 4] {
    let mut out = [b' '; 4];
    for (o, b) in out.iter_mut().zip(name.bytes()) {
        *o = b.to_ascii_uppercase();
    }
    out
}

fn sprite_str(name: &[u8; 4]) -> &str {
    std::str::from_utf8(name).unwrap_or("")
}

/// What to draw this frame.
#[derive(Clone, Debug, Default)]
pub struct SoftFrame {
    /// Eye X (16.16).
    pub x: i32,
    /// Eye Y (16.16).
    pub y: i32,
    /// Eye Z (`viewz`, 16.16).
    pub z: i32,
    /// View angle (BAM).
    pub angle: u32,
    /// `extralight` (gun flashes, 0–2 in vanilla).
    pub extralight: i32,
    /// `fixedcolormap` row (invulnerability 32, light amp 1); `None` = lit.
    pub fixed_colormap: Option<usize>,
    /// Sector state this frame (`None` = as loaded).
    pub sectors: Option<Vec<SoftSector>>,
    /// Sky texture id (`SKY1`… resolved through [`SoftData::texture`]).
    pub sky: u16,
    /// Things, each sector's in its thing-list order (newest first in
    /// vanilla). Their sprites must be loaded ([`SoftData::sprite`]).
    pub things: Vec<SoftThing>,
    /// Weapon layers, drawn in order (weapon, then flash).
    pub psprites: Vec<SoftPSprite>,
    /// The weapon is drawn with the fuzz effect (invisibility running).
    pub psprite_fuzz: bool,
    /// `leveltime` (tics), for animated textures and flats.
    pub leveltime: u32,
    /// Free look (SR5): rows the view is sheared down (positive = looking
    /// up), as ZDoom's software renderer; clamped to the view height.
    pub yshear: i32,
    /// Wall and floor decals (SR6); their pictures must be loaded
    /// ([`SoftData::picture`]).
    pub decals: Vec<SoftDecal>,
    /// Hexen double sky (SR7): the back sky, drawn behind [`Self::sky`]
    /// whose colour 0 is clear.
    pub sky2: Option<u16>,
    /// Sky scroll (BAM added to the view angle) of sky1 and sky2.
    pub sky_offsets: [u32; 2],
    /// Particles (SR8).
    pub particles: Vec<SoftParticle>,
    /// Dynamic lights (SR8; true colour only).
    pub lights: Vec<SoftLight>,
}

impl SoftFrame {
    /// Every sprite name the frame uses (things and weapon).
    pub fn sprite_names(&self) -> impl Iterator<Item = &str> {
        self.things
            .iter()
            .map(|t| &t.sprite)
            .chain(self.psprites.iter().map(|p| &p.sprite))
            .map(sprite_str)
    }
}

/// The 8-bit frame: palette indices, row-major.
#[derive(Clone, Debug)]
pub struct SoftTarget {
    /// Width.
    pub width: u32,
    /// Height.
    pub height: u32,
    /// `width * height` palette indices.
    pub pixels: Vec<u8>,
    /// True-colour frame (0x00RRGGBB per pixel) when the renderer draws in
    /// true colour; empty otherwise.
    pub rgb: Vec<u32>,
}

/// How a pixel is written: through a `COLORMAP` row, or shaded in true
/// colour.
#[derive(Clone, Copy)]
enum Px<'a> {
    Index(&'a [u8]),
    True(TrueShade<'a>),
}

/// True-colour shading of palette texels: `light` 0..=256 scales the
/// texel tinted by the sector's light colour towards its fade colour; a
/// Boom colormap set or a special row (invulnerability) goes through the
/// row and the palette instead.
#[derive(Clone, Copy)]
struct TrueShade<'a> {
    pal: &'a [[u8; 3]; 256],
    cmap: Option<&'a [u8]>,
    /// Per channel: `tint × light / 255` in 8.16 (so a channel is
    /// `c × mul >> 16`) and the fade's share `fade × (256 − light) >> 8`
    /// (SR9: no division per pixel).
    mul: [u32; 3],
    add: [u32; 3],
    /// Sector desaturation 0..=255 (UDMF `desaturation`).
    desat: u32,
    /// The palette already shaded for this light (white light, no fade,
    /// no desaturation; SR9): palette texels are a lookup.
    lut: Option<&'a [u32; 256]>,
}

impl<'a> TrueShade<'a> {
    fn new(
        pal: &'a [[u8; 3]; 256],
        cmap: Option<&'a [u8]>,
        light: u32,
        tint: [u32; 3],
        fade: [u32; 3],
        desat: u32,
    ) -> Self {
        let l = light.min(256);
        Self {
            pal,
            cmap,
            mul: tint.map(|t| t * l * 256 / 255),
            add: fade.map(|f| (f * (256 - l)) >> 8),
            desat,
            lut: None,
        }
    }
}

/// The palette shaded by plain light at every level 0..=256 (SR9).
pub(crate) fn shade_lut(pal: &[[u8; 3]; 256]) -> Vec<[u32; 256]> {
    (0..=256u32)
        .map(|l| {
            let sh = TrueShade::new(pal, None, l, [255; 3], [0; 3], 0);
            let mut row = [0u32; 256];
            for (c, out) in row.iter_mut().enumerate() {
                let [r, g, b] = pal[c];
                *out = sh.shade([u32::from(r), u32::from(g), u32::from(b)]);
            }
            row
        })
        .collect()
}

#[inline]
fn pack(r: u32, g: u32, b: u32) -> u32 {
    (r.min(255) << 16) | (g.min(255) << 8) | b.min(255)
}

#[inline]
fn unpack(c: u32) -> [u32; 3] {
    [(c >> 16) & 0xff, (c >> 8) & 0xff, c & 0xff]
}

impl TrueShade<'_> {
    #[inline(always)]
    fn rgb(&self, texel: u8) -> u32 {
        if let Some(l) = self.lut {
            return l[usize::from(texel)];
        }
        if let Some(c) = self.cmap {
            let [r, g, b] = self.pal[usize::from(c[usize::from(texel)])];
            return pack(u32::from(r), u32::from(g), u32::from(b));
        }
        let [r, g, b] = self.pal[usize::from(texel)];
        self.shade([u32::from(r), u32::from(g), u32::from(b)])
    }

    /// Light, tint, desaturate and fade one RGB colour.
    #[inline(always)]
    fn shade(&self, [mut r, mut g, mut b]: [u32; 3]) -> u32 {
        if self.desat != 0 {
            let grey = (r * 77 + g * 151 + b * 28) >> 8;
            let d = self.desat;
            r = (r * (255 - d) + grey * d) / 255;
            g = (g * (255 - d) + grey * d) / 255;
            b = (b * (255 - d) + grey * d) / 255;
        }
        let ch = |c: u32, k: usize| ((c * self.mul[k]) >> 16) + self.add[k];
        pack(ch(r, 0), ch(g, 1), ch(b, 2))
    }
}

impl Px<'_> {
    #[inline(always)]
    fn put(self, target: &mut SoftTarget, i: usize, texel: u8) {
        match self {
            Px::Index(c) => target.pixels[i] = c[usize::from(texel)],
            Px::True(t) => target.rgb[i] = t.rgb(texel),
        }
    }

    /// Like [`Self::put`], with the picture's own true colour when it has
    /// one (special colormap rows still go through the palette index).
    #[inline(always)]
    fn put_rgb(self, target: &mut SoftTarget, i: usize, texel: u8, rgb: Option<u32>) {
        match (self, rgb) {
            (Px::True(t), Some(c)) if t.cmap.is_none() => target.rgb[i] = t.shade(unpack(c)),
            _ => self.put(target, i, texel),
        }
    }
}

impl SoftTarget {
    /// A cleared frame.
    #[must_use]
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            pixels: vec![0; (width * height) as usize],
            rgb: Vec::new(),
        }
    }

    /// RGBA8 through `palette`.
    #[must_use]
    pub fn to_rgba(&self, palette: &[[u8; 3]; 256]) -> Vec<u8> {
        if self.rgb.len() == self.pixels.len() && !self.rgb.is_empty() {
            return self
                .rgb
                .iter()
                .flat_map(|&c| {
                    let [r, g, b] = unpack(c);
                    [r as u8, g as u8, b as u8, 255]
                })
                .collect();
        }
        self.pixels
            .iter()
            .flat_map(|&i| {
                let [r, g, b] = palette[usize::from(i)];
                [r, g, b, 255]
            })
            .collect()
    }

    /// RGBA8 for an `out_w` × `out_h` screen: the frame scaled (nearest
    /// pixel) into the largest centred box with vanilla's shape — pixels
    /// 1.2 times taller than wide, so 320×200 fills a 4:3 box — and black
    /// bars around it.
    #[must_use]
    pub fn to_rgba_fit(&self, palette: &[[u8; 3]; 256], out_w: u32, out_h: u32) -> Vec<u8> {
        let mut out = Vec::new();
        self.to_rgba_fit_into(palette, out_w, out_h, &mut out);
        out
    }

    /// [`Self::to_rgba_fit`] into `out` (resized; no other allocation).
    pub fn to_rgba_fit_into(
        &self,
        palette: &[[u8; 3]; 256],
        out_w: u32,
        out_h: u32,
        out: &mut Vec<u8>,
    ) {
        let (ow, oh) = (out_w as usize, out_h as usize);
        out.clear();
        out.resize(ow * oh * 4, 0);
        for px in out.chunks_exact_mut(4) {
            px[3] = 255;
        }
        if ow == 0 || oh == 0 || self.width == 0 || self.height == 0 {
            return;
        }
        let aspect = f64::from(self.width) / (f64::from(self.height) * 1.2);
        let (bw, bh) = if ow as f64 / oh as f64 > aspect {
            (((oh as f64) * aspect).round() as usize, oh)
        } else {
            (ow, ((ow as f64) / aspect).round() as usize)
        };
        let (bw, bh) = (bw.clamp(1, ow), bh.clamp(1, oh));
        let (x0, y0) = ((ow - bw) / 2, (oh - bh) / 2);
        let (sw, sh) = (self.width as usize, self.height as usize);
        let truecolor = self.rgb.len() == self.pixels.len() && !self.rgb.is_empty();
        let mut last_src = usize::MAX;
        for y in 0..bh {
            let sy = y * sh / bh;
            let at = ((y0 + y) * ow + x0) * 4;
            if sy == last_src {
                // Same source row as the row above: copy it.
                let prev = at - ow * 4;
                out.copy_within(prev..prev + bw * 4, at);
                continue;
            }
            last_src = sy;
            let row = &mut out[at..at + bw * 4];
            if truecolor {
                let src = &self.rgb[sy * sw..][..sw];
                for (x, px) in row.chunks_exact_mut(4).enumerate() {
                    let c = src[x * sw / bw];
                    px[0] = (c >> 16) as u8;
                    px[1] = (c >> 8) as u8;
                    px[2] = c as u8;
                }
            } else {
                let src = &self.pixels[sy * sw..][..sw];
                for (x, px) in row.chunks_exact_mut(4).enumerate() {
                    let [r, g, b] = palette[usize::from(src[x * sw / bw])];
                    px[0] = r;
                    px[1] = g;
                    px[2] = b;
                }
            }
        }
    }
}

#[derive(Clone, Debug)]
struct Visplane {
    height: i32,
    /// Flat id, [`SKY_FLAT`] as `u32`, or [`PL_SKYFLAT`] | side.
    picnum: u32,
    light: i32,
    xoffs: i32,
    yoffs: i32,
    /// Light / fade colours (true colour; part of the plane's key).
    tint: (u32, u32),
    /// Sloped plane (SR6) and UDMF flat placement (part of the key).
    slope: Option<SoftSlope>,
    xform: FlatXform,
    minx: i32,
    maxx: i32,
    /// `top[x + 1]` (one pad column each side, as vanilla's `pad1` / `pad4`).
    top: Vec<u16>,
    bottom: Vec<u16>,
}

/// Where a drawseg's sprite clip rows live: in [`SoftRenderer`]'s openings
/// (index = base + x), or the shared full-height / none arrays.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Clip {
    None,
    ScreenHeight,
    NegOne,
    Openings(i32),
}

#[derive(Clone, Copy, Debug)]
struct DrawSeg {
    seg: u32,
    x1: i32,
    x2: i32,
    scale1: i32,
    scale2: i32,
    scalestep: i32,
    silhouette: u8,
    bsilheight: i32,
    tsilheight: i32,
    sprtopclip: Clip,
    sprbottomclip: Clip,
    maskedtexturecol: Clip,
    /// 3D floor sides on this seg (SR6): texture columns, and the range in
    /// the frame's slab side list.
    slabcol: Clip,
    slab_first: u32,
    slab_count: u32,
}

impl DrawSeg {
    /// Has masked columns to draw (middle texture or slab sides).
    fn masked(&self) -> bool {
        self.maskedtexturecol != Clip::None || self.slabcol != Clip::None
    }
}

/// `vissprite_t`.
#[derive(Clone, Copy, Debug)]
struct VisSprite {
    x1: i32,
    x2: i32,
    gx: i32,
    gy: i32,
    gz: i32,
    gzt: i32,
    startfrac: i32,
    scale: i32,
    xiscale: i32,
    texturemid: i32,
    patch: u32,
    /// `None` = fuzz.
    colormap: Option<usize>,
    /// 0 = none, 1–3 = player colours.
    translation: u8,
    /// Translucency table (Boom `MF_TRANSLUCENT`).
    tranmap: Option<u16>,
    /// The thing's sector's `heightsec` (Boom deep-water clipping).
    heightsec: Option<u32>,
    /// Light / fade colours of the thing's sector (true colour).
    tint: (u32, u32),
    /// Vertical screen scale and its inverse (sprite `scale_y`).
    yscale: i32,
    yiscale: i32,
    /// ZDoom style, opacity (0..=256) and fill colour.
    style: SoftStyle,
    alpha: u32,
    fill: u32,
    /// A quad (wall / flat / rolled sprite, decal) in the frame's quads.
    special: Option<u32>,
    /// A voxel model / particle of the frame (SR8).
    voxel: Option<u32>,
    particle: Option<u32>,
}

/// How a masked column is coloured (`colfunc`).
#[derive(Clone, Copy, Debug)]
enum ColFunc {
    Normal(usize),
    Translated(usize, u8),
    /// Boom `R_DrawTLColumn`: colormap, translucency table.
    Translucent(usize, u16),
    /// ZDoom styles: colormap, style, opacity 0..=256, fill colour.
    Blend(usize, SoftStyle, u32, u32),
    Fuzz,
}

/// Per-wall state (`rw_*` and friends in `r_segs.c`).
#[derive(Clone, Copy, Debug, Default)]
struct Wall {
    segtextured: bool,
    markfloor: bool,
    markceiling: bool,
    maskedtexture: bool,
    toptexture: u16,
    bottomtexture: u16,
    midtexture: u16,
    normalangle: u32,
    angle1: u32,
    x: i32,
    stopx: i32,
    centerangle: u32,
    offset: i32,
    distance: i32,
    scale: i32,
    scalestep: i32,
    midtexturemid: i32,
    toptexturemid: i32,
    bottomtexturemid: i32,
    pixhigh: i32,
    pixlow: i32,
    pixhighstep: i32,
    pixlowstep: i32,
    topfrac: i32,
    topstep: i32,
    bottomfrac: i32,
    bottomstep: i32,
    walllights: Option<usize>,
    maskedcol_base: i32,
    /// Slab side texture columns (openings base; 3D floors).
    slab_base: Option<i32>,
    /// A portal wall: its region (SR7).
    portal: Option<usize>,
    /// UDMF per-part placement (top, bottom, mid).
    parts: [crate::level::SidePart; 3],
}

/// The renderer: view tables for one size, and the per-frame working sets.
#[derive(Clone, Debug)]
pub struct SoftRenderer {
    /// Keep vanilla's limits: at most 256 drawsegs (later walls are not
    /// drawn) and 128 visplanes (further planes are dropped instead of
    /// `I_Error`). Off by default.
    pub vanilla_limits: bool,
    /// Boom's drawers (on by default): wall columns wrap at the texture's
    /// height instead of `& 127` (no "tutti-frutti"), sprite columns are not
    /// masked, sprite and weapon light average the floor and ceiling light,
    /// the sky follows fixed colormaps. Off for vanilla parity.
    pub boom: bool,
    /// True-colour drawers (`vid_rendermode 1`, SR5): pixels go to
    /// [`SoftTarget::rgb`] lit by smooth shading, sector light colours and
    /// fades instead of `COLORMAP` rows. Off by default.
    pub truecolor: bool,
    /// `r_visibility` relative to Doom's (1.0): higher lights distant
    /// walls, things and planes more.
    pub visibility: f64,
    width: i32,
    height: i32,
    /// The 4:3 width the projection is built for (`width` in vanilla sizes;
    /// widescreen views are wider and see more to the sides).
    basew: i32,
    /// Free-look shear of this frame (rows).
    yshear: i32,
    /// `visibility` as 16.16, applied to light indices.
    visfixed: i32,
    /// Light / fade colours for the surface being drawn (true colour).
    tint: (u32, u32),
    #[allow(dead_code)] // vanilla's `centerx`; the code reads `centerxfrac`
    centerx: i32,
    centery: i32,
    centerxfrac: i32,
    centeryfrac: i32,
    projection: i32,
    viewangletox: Vec<i32>,
    xtoviewangle: Vec<u32>,
    clipangle: u32,
    yslope: Vec<i32>,
    distscale: Vec<i32>,
    /// `scalelight[light][scale]` → colormap row.
    scalelight: Vec<[usize; MAXLIGHTSCALE]>,
    /// `zlight[light][dist]` → colormap row.
    zlight: Vec<[usize; MAXLIGHTZ]>,
    pspriteiscale: i32,
    pspritescale: i32,
    /// `translationtables`: green → gray, brown, red.
    translations: [[u8; 256]; 3],
    /// `fuzzpos`, kept across frames like vanilla's.
    fuzzpos: usize,
    // Per frame.
    viewsin: i32,
    viewcos: i32,
    things: Vec<SoftThing>,
    /// Thing indices per sector, and whether the sector was added.
    sector_things: Vec<Vec<u32>>,
    sector_added: Vec<bool>,
    vissprites: Vec<VisSprite>,
    clipbot: Vec<i16>,
    cliptop: Vec<i16>,
    viewx: i32,
    viewy: i32,
    viewz: i32,
    viewangle: u32,
    extralight: i32,
    fixed: Option<usize>,
    solidsegs: Vec<(i32, i32)>,
    drawsegs: Vec<DrawSeg>,
    openings: Vec<i16>,
    visplanes: Vec<Visplane>,
    floorplane: Option<usize>,
    ceilingplane: Option<usize>,
    floorclip: Vec<i16>,
    ceilingclip: Vec<i16>,
    spanstart: Vec<i32>,
    cachedheight: Vec<i32>,
    cacheddistance: Vec<i32>,
    cachedxstep: Vec<i32>,
    cachedystep: Vec<i32>,
    basexscale: i32,
    baseyscale: i32,
    planeheight: i32,
    planezlight: usize,
    wall: Wall,
    curseg: u32,
    frontsector: SoftSector,
    backsector: Option<SoftSector>,
    front_index: usize,
    back_index: Option<usize>,
    leveltime: u32,
    /// Colormap set of this frame (Boom 242 colormaps of the viewer's sector).
    cmap_set: u16,
    /// `heightsec` of the viewer's sector.
    view_heightsec: Option<u32>,
    plane_xoffs: i32,
    plane_yoffs: i32,
    /// Flat id of the plane being drawn (its true-colour pixels).
    plane_flat: u16,
    /// Slope / placement of the plane being found or drawn.
    plane_slope: Option<SoftSlope>,
    plane_xform: FlatXform,
    /// Height of the plane being drawn (16.16, not relative to the eye).
    plane_height_raw: i32,
    /// Per-column view ray (x, y) with unit forward depth, this frame.
    col_dirs: Vec<(f64, f64)>,
    /// 3D floors and the depth buffer (SR6).
    x: ffloor::Extra,
    /// Visplanes of earlier frames, reused (SR9).
    plane_pool: Vec<Visplane>,
    /// Sprite draw order (scale, index), reused (SR9).
    sprite_order: Vec<(i32, u32)>,
    /// Draw the frame in this many vertical slices on as many threads
    /// (SR9, UZDoom `r_scene_multithreaded`; 1 = one thread, the default;
    /// wasm always draws on one).
    pub threads: usize,
    /// The slices' renderers and frames, kept between frames.
    slices: slices::Slices,
}

impl SoftRenderer {
    /// Tables for a `width` × `height` view (`R_ExecuteSetViewSize` with the
    /// view filling the frame; 320×200 or 320×168 are vanilla's sizes).
    #[must_use]
    pub fn new(width: u32, height: u32) -> Self {
        Self::with_view(width, height, width, 90.0)
    }

    /// A modern view (SR5): `height` rows of vanilla-shaped pixels (1.2 times
    /// taller than wide), `width` as wide as the screen needs — wider than
    /// 1.6 × `height` is widescreen and sees more to the sides — and a
    /// horizontal field of view `fov` degrees across the 4:3 part (90 =
    /// Doom's).
    #[must_use]
    pub fn new_view(width: u32, height: u32, fov: f64) -> Self {
        let basew = ((f64::from(height.clamp(2, 8192)) * 1.6).round() as u32).max(2);
        Self::with_view(width, height, basew, fov)
    }

    fn with_view(width: u32, height: u32, basew: u32, fov: f64) -> Self {
        let width = width.clamp(2, 8192) as i32;
        let height = height.clamp(2, 8192) as i32;
        let basew = basew.clamp(2, 8192) as i32;
        let centery = height / 2;
        let centerx = width / 2;
        let centerxfrac = centerx << FRACBITS;
        let centeryfrac = centery << FRACBITS;
        let fov = fov.clamp(30.0, 120.0);
        let projection = if (fov - 90.0).abs() < 1e-9 {
            (basew / 2) << FRACBITS
        } else {
            (f64::from(basew / 2) * 65536.0 / (fov.to_radians() / 2.0).tan()).round() as i32
        };
        let mut r = Self {
            vanilla_limits: false,
            boom: true,
            truecolor: false,
            visibility: 1.0,
            width,
            height,
            basew,
            yshear: 0,
            visfixed: FRACUNIT,
            tint: (0, 0),
            centerx,
            centery,
            centerxfrac,
            centeryfrac,
            projection,
            viewangletox: vec![0; FINEANGLES / 2],
            xtoviewangle: vec![0; width as usize + 1],
            clipangle: 0,
            yslope: vec![0; height as usize],
            distscale: vec![0; width as usize],
            scalelight: vec![[0; MAXLIGHTSCALE]; LIGHTLEVELS],
            zlight: vec![[0; MAXLIGHTZ]; LIGHTLEVELS],
            pspriteiscale: ((i64::from(FRACUNIT) * i64::from(ORIGWIDTH)) / i64::from(basew)) as i32,
            pspritescale: ((i64::from(FRACUNIT) * i64::from(basew)) / i64::from(ORIGWIDTH)) as i32,
            translations: translation_tables(),
            fuzzpos: 0,
            viewsin: 0,
            viewcos: 0,
            things: Vec::new(),
            sector_things: Vec::new(),
            sector_added: Vec::new(),
            vissprites: Vec::with_capacity(MAXVISSPRITES),
            clipbot: vec![0; width as usize],
            cliptop: vec![0; width as usize],
            viewx: 0,
            viewy: 0,
            viewz: 0,
            viewangle: 0,
            extralight: 0,
            fixed: None,
            solidsegs: Vec::with_capacity(64),
            drawsegs: Vec::with_capacity(512),
            openings: Vec::new(),
            visplanes: Vec::with_capacity(256),
            floorplane: None,
            ceilingplane: None,
            floorclip: vec![0; width as usize],
            ceilingclip: vec![0; width as usize],
            spanstart: vec![0; height as usize],
            cachedheight: vec![0; height as usize],
            cacheddistance: vec![0; height as usize],
            cachedxstep: vec![0; height as usize],
            cachedystep: vec![0; height as usize],
            basexscale: 0,
            baseyscale: 0,
            planeheight: 0,
            planezlight: 0,
            wall: Wall::default(),
            curseg: 0,
            frontsector: SoftSector::default(),
            backsector: None,
            front_index: 0,
            back_index: None,
            leveltime: 0,
            cmap_set: 0,
            view_heightsec: None,
            plane_xoffs: 0,
            plane_yoffs: 0,
            plane_flat: 0,
            plane_slope: None,
            plane_xform: FlatXform::default(),
            plane_height_raw: 0,
            col_dirs: Vec::new(),
            x: ffloor::Extra::default(),
            plane_pool: Vec::new(),
            sprite_order: Vec::new(),
            threads: 1,
            slices: slices::Slices::default(),
        };
        r.init_texture_mapping();
        r.init_tables();
        r
    }

    /// View width.
    #[must_use]
    pub fn width(&self) -> u32 {
        self.width as u32
    }

    /// `fuzzpos`: where the fuzz table continues (it runs on across frames).
    #[must_use]
    pub fn fuzzpos(&self) -> usize {
        self.fuzzpos
    }

    /// Set `fuzzpos` (to match a capture taken mid-game).
    pub fn set_fuzzpos(&mut self, pos: usize) {
        self.fuzzpos = pos % FUZZOFFSET.len();
    }

    /// Vertical projection in rows per unit at distance 1 (free-look shear
    /// for a pitch is `tan(pitch) × this`).
    #[must_use]
    pub fn projection_rows(&self) -> f64 {
        f64::from(self.projection) / 65536.0
    }

    /// View height.
    #[must_use]
    pub fn height(&self) -> u32 {
        self.height as u32
    }

    /// `R_InitTextureMapping`.
    fn init_texture_mapping(&mut self) {
        // Vanilla: `centerxfrac / finetangent[FINEANGLES/4 + FIELDOFVIEW/2]`
        // (`centerxfrac` is its projection); every view uses the same rule.
        let focallength = fixed_div(
            self.projection,
            finetangent(FINEANGLES / 4 + FIELDOFVIEW / 2),
        );
        let w = self.width;
        for i in 0..FINEANGLES / 2 {
            let ft = finetangent(i);
            let t = if ft > FRACUNIT * 2 {
                -1
            } else if ft < -FRACUNIT * 2 {
                w + 1
            } else {
                let t = fixed_mul(ft, focallength);
                ((self.centerxfrac - t + FRACUNIT - 1) >> FRACBITS).clamp(-1, w + 1)
            };
            self.viewangletox[i] = t;
        }
        for x in 0..=w {
            let mut i = 0usize;
            while self.viewangletox[i] > x {
                i += 1;
            }
            self.xtoviewangle[x as usize] = ((i as u32) << ANGLETOFINESHIFT).wrapping_sub(ANG90);
        }
        for v in &mut self.viewangletox {
            if *v == -1 {
                *v = 0;
            } else if *v == w + 1 {
                *v = w;
            }
        }
        self.clipangle = self.xtoviewangle[0];
    }

    /// `yslope` for the current centre row (free look shears it).
    fn init_yslope(&mut self) {
        for i in 0..self.height {
            let dy = (((i - self.centery) << FRACBITS) + FRACUNIT / 2).abs();
            self.yslope[i as usize] = fixed_div(self.projection, dy);
        }
    }

    /// Light-table column for a wall / sprite `scale`, with `r_visibility`.
    #[inline]
    fn scale_light_index(&self, scale: i32) -> usize {
        let s = if self.visfixed == FRACUNIT {
            scale
        } else {
            fixed_mul(scale, self.visfixed)
        };
        ((s >> LIGHTSCALESHIFT).max(0) as usize).min(MAXLIGHTSCALE - 1)
    }

    /// Light-table column for a plane `distance`, with `r_visibility`.
    #[inline]
    fn z_light_index(&self, distance: i32) -> usize {
        let d = if self.visfixed == FRACUNIT {
            distance
        } else {
            fixed_div(distance, self.visfixed)
        };
        ((d >> LIGHTZSHIFT).max(0) as usize).min(MAXLIGHTZ - 1)
    }

    /// `R_ExecuteSetViewSize` tables and `R_InitLightTables`.
    fn init_tables(&mut self) {
        let w = self.width;
        self.init_yslope();
        for i in 0..w as usize {
            let cosadj = finecosine((self.xtoviewangle[i] >> ANGLETOFINESHIFT) as usize).abs();
            self.distscale[i] = fixed_div(FRACUNIT, cosadj);
        }
        let ncm = NUMCOLORMAPS as i32;
        for i in 0..LIGHTLEVELS {
            let startmap = ((LIGHTLEVELS as i32 - 1 - i as i32) * 2) * ncm / LIGHTLEVELS as i32;
            for j in 0..MAXLIGHTSCALE {
                let level = startmap - (j as i32) * ORIGWIDTH / self.basew / DISTMAP;
                self.scalelight[i][j] = level.clamp(0, ncm - 1) as usize;
            }
            for j in 0..MAXLIGHTZ {
                let scale = fixed_div((ORIGWIDTH / 2) * FRACUNIT, ((j as i32) + 1) << LIGHTZSHIFT)
                    >> LIGHTSCALESHIFT;
                let level = startmap - scale / DISTMAP;
                self.zlight[i][j] = level.clamp(0, ncm - 1) as usize;
            }
        }
    }

    /// `R_RenderPlayerView`: walls, floors, ceilings, sky, things, masked
    /// middle textures and the weapon into `target` (which must be this
    /// renderer's size).
    pub fn render(
        &mut self,
        level: &SoftLevel,
        data: &SoftData,
        frame: &SoftFrame,
        target: &mut SoftTarget,
    ) {
        // Slices pay off from about two 640×400 frames' worth of pixels.
        // wasm32 has no threads (and the GPU material catalog in `SoftData`
        // is not `Send` there), so the browser always draws one slice.
        #[cfg(not(target_arch = "wasm32"))]
        {
            let worth = ((self.width * self.height) as usize / (640 * 400)).max(1);
            if self.threads.min(worth) > 1 {
                self.render_sliced(level, data, frame, target);
                return;
            }
        }
        self.render_view(level, data, frame, None, target);
    }

    /// [`Self::render`], or a portal's view inside `sub`'s region (SR7).
    fn render_view(
        &mut self,
        level: &SoftLevel,
        data: &SoftData,
        frame: &SoftFrame,
        sub: Option<&portal::SubView>,
        target: &mut SoftTarget,
    ) {
        self.view_bsp(level, data, frame, sub, None, target);
        self.view_finish(level, data, frame, true, target);
    }

    /// The first half of a frame: set-up and the BSP walk (walls, plane
    /// spans noted, sprites projected), in columns `slice` only when given
    /// (SR9 threaded slices).
    fn view_bsp(
        &mut self,
        level: &SoftLevel,
        data: &SoftData,
        frame: &SoftFrame,
        sub: Option<&portal::SubView>,
        slice: Option<(i32, i32)>,
        target: &mut SoftTarget,
    ) {
        assert_eq!(
            (target.width, target.height),
            (self.width as u32, self.height as u32),
            "target size must match the renderer"
        );
        let sectors = frame.sectors.as_deref().unwrap_or(&level.sectors);
        self.viewx = frame.x;
        self.viewy = frame.y;
        self.viewz = frame.z;
        self.viewangle = frame.angle;
        self.extralight = frame.extralight;
        self.fixed = frame.fixed_colormap;
        self.leveltime = frame.leveltime;
        // View rays per column (unit forward depth) for modern planes.
        self.col_dirs.clear();
        let va = f64::from(self.viewangle) / 4_294_967_296.0 * std::f64::consts::TAU;
        for x in 0..self.width as usize {
            let off = f64::from(self.xtoviewangle[x]) / 4_294_967_296.0 * std::f64::consts::TAU;
            let c = off.cos().max(1e-6);
            self.col_dirs
                .push(((va + off).cos() / c, (va + off).sin() / c));
        }
        // Free look: move the centre row, and the plane slopes with it.
        let shear = frame.yshear.clamp(-self.height, self.height);
        if shear != self.yshear {
            self.yshear = shear;
            self.centery = self.height / 2 + shear;
            self.centeryfrac = self.centery << FRACBITS;
            self.init_yslope();
        }
        self.visfixed = (self.visibility.clamp(0.0625, 16.0) * 65536.0).round() as i32;
        if self.truecolor && target.rgb.len() != target.pixels.len() {
            target.rgb = vec![0; target.pixels.len()];
        }
        // Boom: the viewer's sector's `heightsec` picks the colormap set
        // (below / between / above its planes) and the fake-plane views.
        self.view_heightsec = level
            .point_in_sector(self.viewx, self.viewy)
            .and_then(|s| level.heightsec.get(s).copied().flatten());
        self.cmap_set = match self.view_heightsec {
            Some(h) => match (sectors.get(h as usize), level.colormaps.get(h as usize)) {
                (Some(s), Some(m)) => {
                    if self.viewz < s.floor {
                        m[0]
                    } else if self.viewz > s.ceiling {
                        m[2]
                    } else {
                        m[1]
                    }
                }
                _ => 0,
            },
            None => 0,
        };
        let fine = (self.viewangle >> ANGLETOFINESHIFT) as usize;
        self.viewsin = finesine(fine);
        self.viewcos = finecosine(fine);
        // R_ClearSprites, and the sector thing lists.
        self.vissprites.clear();
        self.things.clear();
        self.things.extend_from_slice(&frame.things);
        self.sector_things.iter_mut().for_each(Vec::clear);
        self.sector_things.resize_with(sectors.len(), Vec::new);
        self.sector_added.clear();
        self.sector_added.resize(sectors.len(), false);
        for (i, t) in self.things.iter().enumerate() {
            if let Some(list) = self.sector_things.get_mut(t.sector as usize) {
                list.push(i as u32);
            }
        }
        // R_ClearClipSegs.
        self.solidsegs.clear();
        self.solidsegs.push((-0x7fff_ffff, -1));
        self.solidsegs.push((self.width, 0x7fff_ffff));
        // R_ClearDrawSegs / R_ClearPlanes.
        self.drawsegs.clear();
        self.openings.clear();
        self.plane_pool.append(&mut self.visplanes);
        for x in 0..self.width as usize {
            self.floorclip[x] = self.height as i16;
            self.ceilingclip[x] = -1;
        }
        self.cachedheight.fill(0);
        self.begin_extras(level, sectors);
        self.begin_portals(frame, sub);
        self.begin_dyn(frame);
        if let Some((x0, x1)) = slice {
            // Other slices' columns are solid.
            self.solidsegs.clear();
            self.solidsegs.push((-0x7fff_ffff, x0 - 1));
            self.solidsegs.push((x1 + 1, 0x7fff_ffff));
        }
        let angle = (self.viewangle.wrapping_sub(ANG90) >> ANGLETOFINESHIFT) as usize;
        // Vanilla divides by `centerxfrac`, which is its projection.
        self.basexscale = fixed_div(finecosine(angle), self.projection);
        self.baseyscale = -fixed_div(finesine(angle), self.projection);
        if level.nodes.is_empty() {
            self.subsector(level, data, sectors, 0, target);
        } else {
            self.render_bsp_node(level, data, sectors, (level.nodes.len() - 1) as u32, target);
        }
    }

    /// The second half of a frame: decals, particles, planes, 3D floors,
    /// portals, sprites and masked walls, then the weapon when `weapon`.
    fn view_finish(
        &mut self,
        level: &SoftLevel,
        data: &SoftData,
        frame: &SoftFrame,
        weapon: bool,
        target: &mut SoftTarget,
    ) {
        let sectors = frame.sectors.as_deref().unwrap_or(&level.sectors);
        self.add_decals(level, data, sectors, &frame.decals);
        self.add_particles(data, &frame.particles);
        self.draw_planes(level, data, frame.sky, target);
        if !self.x.seg_slabs.is_empty() {
            self.zbuf_on();
        }
        self.draw_slab_planes(level, data, true, target);
        self.draw_portals(level, data, frame, sectors, target);
        self.draw_masked(level, data, sectors, frame, weapon, target);
    }

    fn point_to_angle(&self, x: i32, y: i32) -> u32 {
        room_core::math::point_to_angle(x.wrapping_sub(self.viewx), y.wrapping_sub(self.viewy))
    }

    /// `R_PointToDist`.
    fn point_to_dist(&self, x: i32, y: i32) -> i32 {
        let mut dx = x.wrapping_sub(self.viewx).wrapping_abs();
        let mut dy = y.wrapping_sub(self.viewy).wrapping_abs();
        if dy > dx {
            std::mem::swap(&mut dx, &mut dy);
        }
        let frac = if dx != 0 { fixed_div(dy, dx) } else { 0 };
        // DBITS = FRACBITS - SLOPEBITS = 5.
        let angle =
            (tantoangle((frac >> 5) as usize).wrapping_add(ANG90) >> ANGLETOFINESHIFT) as usize;
        fixed_div(dx, finesine(angle))
    }

    /// `R_ScaleFromGlobalAngle`.
    fn scale_from_global_angle(&self, visangle: u32) -> i32 {
        let anglea = ANG90.wrapping_add(visangle.wrapping_sub(self.viewangle));
        let angleb = ANG90.wrapping_add(visangle.wrapping_sub(self.wall.normalangle));
        let sinea = finesine((anglea >> ANGLETOFINESHIFT) as usize);
        let sineb = finesine((angleb >> ANGLETOFINESHIFT) as usize);
        let num = fixed_mul(self.projection, sineb);
        let den = fixed_mul(self.wall.distance, sinea);
        if den > num >> 16 {
            fixed_div(num, den).clamp(256, 64 * FRACUNIT)
        } else {
            64 * FRACUNIT
        }
    }

    /// `R_PointOnSide`.
    fn point_on_side(x: i32, y: i32, n: &crate::level::SoftNode) -> usize {
        point_on_line(x, y, n.x, n.y, n.dx, n.dy)
    }

    fn render_bsp_node(
        &mut self,
        level: &SoftLevel,
        data: &SoftData,
        sectors: &[SoftSector],
        bspnum: u32,
        target: &mut SoftTarget,
    ) {
        if let Some(leaf) = SoftLevel::leaf(bspnum) {
            let leaf = if bspnum == u32::MAX { 0 } else { leaf };
            self.subsector(level, data, sectors, leaf, target);
            return;
        }
        let Some(node) = level.nodes.get(bspnum as usize).copied() else {
            return;
        };
        let side = Self::point_on_side(self.viewx, self.viewy, &node);
        self.render_bsp_node(level, data, sectors, node.children[side], target);
        if self.check_bbox(&node.bbox[side ^ 1]) {
            self.render_bsp_node(level, data, sectors, node.children[side ^ 1], target);
        }
    }

    /// `R_CheckBBox`.
    fn check_bbox(&self, bspcoord: &[i32; 4]) -> bool {
        const BOXTOP: usize = 0;
        const BOXBOTTOM: usize = 1;
        const BOXLEFT: usize = 2;
        const BOXRIGHT: usize = 3;
        const CHECKCOORD: [[usize; 4]; 11] = [
            [3, 0, 2, 1],
            [3, 0, 2, 0],
            [3, 1, 2, 0],
            [0, 0, 0, 0],
            [2, 0, 2, 1],
            [0, 0, 0, 0],
            [3, 1, 3, 0],
            [0, 0, 0, 0],
            [2, 0, 3, 1],
            [2, 1, 3, 1],
            [2, 1, 3, 0],
        ];
        let boxx = if self.viewx <= bspcoord[BOXLEFT] {
            0
        } else if self.viewx < bspcoord[BOXRIGHT] {
            1
        } else {
            2
        };
        let boxy = if self.viewy >= bspcoord[BOXTOP] {
            0
        } else if self.viewy > bspcoord[BOXBOTTOM] {
            1
        } else {
            2
        };
        let boxpos = (boxy << 2) + boxx;
        if boxpos == 5 {
            return true;
        }
        let c = CHECKCOORD[boxpos];
        let (x1, y1, x2, y2) = (
            bspcoord[c[0]],
            bspcoord[c[1]],
            bspcoord[c[2]],
            bspcoord[c[3]],
        );
        let mut angle1 = self.point_to_angle(x1, y1).wrapping_sub(self.viewangle);
        let mut angle2 = self.point_to_angle(x2, y2).wrapping_sub(self.viewangle);
        let span = angle1.wrapping_sub(angle2);
        if span >= ANG180 {
            return true;
        }
        let clip = self.clipangle;
        let mut tspan = angle1.wrapping_add(clip);
        if tspan > clip.wrapping_mul(2) {
            tspan = tspan.wrapping_sub(clip.wrapping_mul(2));
            if tspan >= span {
                return false;
            }
            angle1 = clip;
        }
        tspan = clip.wrapping_sub(angle2);
        if tspan > clip.wrapping_mul(2) {
            tspan = tspan.wrapping_sub(clip.wrapping_mul(2));
            if tspan >= span {
                return false;
            }
            angle2 = clip.wrapping_neg();
        }
        let a1 = (angle1.wrapping_add(ANG90) >> ANGLETOFINESHIFT) as usize;
        let a2 = (angle2.wrapping_add(ANG90) >> ANGLETOFINESHIFT) as usize;
        let sx1 = self.viewangletox[a1];
        let mut sx2 = self.viewangletox[a2];
        if sx1 == sx2 {
            return false;
        }
        sx2 -= 1;
        let mut start = 0;
        while self.solidsegs[start].1 < sx2 {
            start += 1;
        }
        !(sx1 >= self.solidsegs[start].0 && sx2 <= self.solidsegs[start].1)
    }

    /// `R_FindPlane`.
    fn find_plane(
        &mut self,
        mut height: i32,
        picnum: u32,
        mut light: i32,
        xoffs: i32,
        yoffs: i32,
    ) -> Option<usize> {
        let mut tint = self.tint;
        let (mut slope, mut xform) = (self.plane_slope, self.plane_xform);
        if picnum == u32::from(SKY_FLAT) || picnum & (PL_SKYFLAT | PL_PORTAL) != 0 {
            height = 0;
            light = 0;
            tint = (0, 0);
            slope = None;
            xform = FlatXform::default();
        }
        if let Some(i) = self.visplanes.iter().position(|p| {
            p.height == height
                && p.picnum == picnum
                && p.light == light
                && p.xoffs == xoffs
                && p.yoffs == yoffs
                && p.tint == tint
                && p.slope == slope
                && p.xform == xform
        }) {
            return Some(i);
        }
        if self.vanilla_limits && self.visplanes.len() >= MAXVISPLANES {
            return None;
        }
        let w = self.width as usize;
        let mut pl = self.new_plane(w);
        pl.height = height;
        pl.picnum = picnum;
        pl.light = light;
        pl.xoffs = xoffs;
        pl.yoffs = yoffs;
        pl.tint = tint;
        pl.slope = slope;
        pl.xform = xform;
        pl.minx = self.width;
        pl.maxx = -1;
        self.visplanes.push(pl);
        Some(self.visplanes.len() - 1)
    }

    /// A cleared visplane, reused from earlier frames when one is free (SR9:
    /// no allocation per frame).
    fn new_plane(&mut self, w: usize) -> Visplane {
        match self.plane_pool.pop() {
            Some(mut pl) => {
                pl.top.clear();
                pl.top.resize(w + 2, PLANE_UNSET);
                pl.bottom.resize(w + 2, 0);
                pl
            }
            None => Visplane {
                height: 0,
                picnum: 0,
                light: 0,
                xoffs: 0,
                yoffs: 0,
                tint: (0, 0),
                slope: None,
                xform: FlatXform::default(),
                minx: self.width,
                maxx: -1,
                top: vec![PLANE_UNSET; w + 2],
                bottom: vec![0; w + 2],
            },
        }
    }

    /// `R_CheckPlane`.
    fn check_plane(&mut self, pl: usize, start: i32, stop: i32) -> Option<usize> {
        let (intrl, unionl) = if start < self.visplanes[pl].minx {
            (self.visplanes[pl].minx, start)
        } else {
            (start, self.visplanes[pl].minx)
        };
        let (intrh, unionh) = if stop > self.visplanes[pl].maxx {
            (self.visplanes[pl].maxx, stop)
        } else {
            (stop, self.visplanes[pl].maxx)
        };
        let top = &self.visplanes[pl].top;
        let free = (intrl..=intrh).all(|x| top[(x + 1) as usize] == PLANE_UNSET);
        if free {
            self.visplanes[pl].minx = unionl;
            self.visplanes[pl].maxx = unionh;
            return Some(pl);
        }
        if self.vanilla_limits && self.visplanes.len() >= MAXVISPLANES {
            return None;
        }
        let (height, picnum, light, xoffs, yoffs, tint, slope, xform) = {
            let p = &self.visplanes[pl];
            (
                p.height, p.picnum, p.light, p.xoffs, p.yoffs, p.tint, p.slope, p.xform,
            )
        };
        let w = self.width as usize;
        let mut pl = self.new_plane(w);
        pl.height = height;
        pl.picnum = picnum;
        pl.light = light;
        pl.xoffs = xoffs;
        pl.yoffs = yoffs;
        pl.tint = tint;
        pl.slope = slope;
        pl.xform = xform;
        pl.minx = start;
        pl.maxx = stop;
        self.visplanes.push(pl);
        Some(self.visplanes.len() - 1)
    }

    /// `R_Subsector` (no sprites yet).
    fn subsector(
        &mut self,
        level: &SoftLevel,
        data: &SoftData,
        sectors: &[SoftSector],
        num: usize,
        target: &mut SoftTarget,
    ) {
        let Some(&(sector, first, count)) = level.subsectors.get(num) else {
            return;
        };
        if self.x.portal.depth > 0
            && count > 0
            && (first..first + count).all(|i| self.portal_culls_seg(level, &level.segs[i as usize]))
        {
            return;
        }
        let si = sector as usize;
        let (front, mut floorlight, mut ceilinglight) = self.fake_flat(level, sectors, si, false);
        self.frontsector = front;
        self.front_index = si;
        self.tint = tint_of(&front);
        let heightsec = level.heightsec.get(si).copied().flatten();
        self.add_sprites(data, si, (floorlight + ceilinglight) / 2, heightsec);
        if self.has_slabs(si) {
            // 3D floors light what is below them; note the slab planes.
            floorlight = self.light_at(si, front.floor, floorlight);
            ceilinglight = self.light_at(si, front.ceiling.saturating_sub(1), ceilinglight);
            self.note_slab_subsector(level, num, si);
        }
        let ctrl = level
            .heightsec
            .get(si)
            .copied()
            .flatten()
            .and_then(|h| sectors.get(h as usize));
        let floor_portal = self.plane_portal(level, si, 0, front.floor_pic);
        let ceiling_portal = self.plane_portal(level, si, 1, front.ceiling_pic);
        let pic = |p: u16| -> u32 {
            match level.sky.get(si).copied().flatten() {
                Some(t) if p == SKY_FLAT => PL_SKYFLAT | t.side,
                _ => u32::from(p),
            }
        };
        self.plane_slope = front.floor_slope;
        self.plane_xform = front.floor_xform;
        self.floorplane = if front.floor_at(self.viewx, self.viewy) < self.viewz
            || front.floor_slope.is_some()
            || ctrl.is_some_and(|c| c.ceiling_pic == SKY_FLAT)
        {
            self.find_plane(
                front.floor,
                floor_portal.unwrap_or_else(|| pic(front.floor_pic)),
                floorlight,
                front.floor_xoffs,
                front.floor_yoffs,
            )
        } else {
            None
        };
        self.plane_slope = front.ceiling_slope;
        self.plane_xform = front.ceiling_xform;
        self.ceilingplane = if front.ceiling_at(self.viewx, self.viewy) > self.viewz
            || front.ceiling_slope.is_some()
            || front.ceiling_pic == SKY_FLAT
            || ctrl.is_some_and(|c| c.floor_pic == SKY_FLAT)
        {
            self.find_plane(
                front.ceiling,
                ceiling_portal.unwrap_or_else(|| pic(front.ceiling_pic)),
                ceilinglight,
                front.ceiling_xoffs,
                front.ceiling_yoffs,
            )
        } else {
            None
        };
        self.plane_slope = None;
        self.plane_xform = FlatXform::default();
        // Hexen: the subsector's polyobject segs first.
        if let Some(polys) = level.subsector_polys.get(num) {
            for &p in polys {
                self.add_line(level, data, sectors, POLY_SEG | p, target);
            }
        }
        for i in first..first + count {
            let line = level.segs[i as usize].line as usize;
            if level.line_is_poly.get(line).copied().unwrap_or(false) {
                continue;
            }
            self.add_line(level, data, sectors, i, target);
        }
    }

    /// `R_AddLine`.
    fn add_line(
        &mut self,
        level: &SoftLevel,
        data: &SoftData,
        sectors: &[SoftSector],
        segnum: u32,
        target: &mut SoftTarget,
    ) {
        let seg = seg_at(level, segnum);
        if self.portal_culls_seg(level, &seg) {
            return;
        }
        self.curseg = segnum;
        let v1 = level.vertices[seg.v1 as usize];
        let v2 = level.vertices[seg.v2 as usize];
        let mut angle1 = self.point_to_angle(v1.x, v1.y);
        let mut angle2 = self.point_to_angle(v2.x, v2.y);
        let span = angle1.wrapping_sub(angle2);
        if span >= ANG180 {
            return;
        }
        self.wall.angle1 = angle1;
        angle1 = angle1.wrapping_sub(self.viewangle);
        angle2 = angle2.wrapping_sub(self.viewangle);
        let clip = self.clipangle;
        let mut tspan = angle1.wrapping_add(clip);
        if tspan > clip.wrapping_mul(2) {
            tspan = tspan.wrapping_sub(clip.wrapping_mul(2));
            if tspan >= span {
                return;
            }
            angle1 = clip;
        }
        tspan = clip.wrapping_sub(angle2);
        if tspan > clip.wrapping_mul(2) {
            tspan = tspan.wrapping_sub(clip.wrapping_mul(2));
            if tspan >= span {
                return;
            }
            angle2 = clip.wrapping_neg();
        }
        let a1 = (angle1.wrapping_add(ANG90) >> ANGLETOFINESHIFT) as usize;
        let a2 = (angle2.wrapping_add(ANG90) >> ANGLETOFINESHIFT) as usize;
        let x1 = self.viewangletox[a1];
        let x2 = self.viewangletox[a2];
        if x1 == x2 {
            return;
        }
        let front = self.frontsector;
        if segnum & POLY_SEG == 0 && self.seg_portal(level, &seg) {
            // A portal: a solid wall whose middle is the portal's region.
            self.back_index = None;
            self.backsector = None;
            let key = portal::RegionKey::Line(seg.line, self.front_index as u32);
            self.x.portal.cur = Some(self.portal_region(key));
            self.clip_solid_wall_segment(level, data, x1, x2 - 1, target);
            self.x.portal.cur = None;
            return;
        }
        self.back_index = seg.back.map(|b| b as usize);
        self.backsector = self
            .back_index
            .filter(|&b| b < sectors.len())
            .map(|b| self.fake_flat(level, sectors, b, true).0);
        let sloped = |s: &SoftSector| s.floor_slope.is_some() || s.ceiling_slope.is_some();
        let any_slope = sloped(&front) || self.backsector.as_ref().is_some_and(sloped);
        let solid = match self.backsector {
            None => true,
            // Slopes: closed only when closed at both ends of the seg.
            Some(back) if any_slope => [v1, v2].iter().all(|v| {
                back.ceiling_at(v.x, v.y) <= front.floor_at(v.x, v.y)
                    || back.floor_at(v.x, v.y) >= front.ceiling_at(v.x, v.y)
            }),
            Some(back) => back.ceiling <= front.floor || back.floor >= front.ceiling,
        };
        if solid {
            self.clip_solid_wall_segment(level, data, x1, x2 - 1, target);
            return;
        }
        let back = self.backsector.unwrap_or_default();
        let window = back.ceiling != front.ceiling
            || back.floor != front.floor
            || (any_slope
                && (back.floor_slope != front.floor_slope
                    || back.ceiling_slope != front.ceiling_slope));
        let bi = self.back_index.unwrap_or(0);
        let fi = self.front_index;
        let lightsec = |v: &Vec<Option<u32>>, i: usize| v.get(i).copied().flatten();
        if !window
            && back.ceiling_pic == front.ceiling_pic
            && back.floor_pic == front.floor_pic
            && back.light == front.light
            && level.sides[seg.side as usize].mid == 0
            && back.floor_xoffs == front.floor_xoffs
            && back.floor_yoffs == front.floor_yoffs
            && back.ceiling_xoffs == front.ceiling_xoffs
            && back.ceiling_yoffs == front.ceiling_yoffs
            && lightsec(&level.floorlightsec, bi) == lightsec(&level.floorlightsec, fi)
            && lightsec(&level.ceilinglightsec, bi) == lightsec(&level.ceilinglightsec, fi)
        {
            return;
        }
        self.clip_pass_wall_segment(level, data, x1, x2 - 1, target);
    }

    /// `R_ClipSolidWallSegment`.
    fn clip_solid_wall_segment(
        &mut self,
        level: &SoftLevel,
        data: &SoftData,
        first: i32,
        last: i32,
        target: &mut SoftTarget,
    ) {
        let mut start = 0;
        while self.solidsegs[start].1 < first - 1 {
            start += 1;
        }
        if first < self.solidsegs[start].0 {
            if last < self.solidsegs[start].0 - 1 {
                self.store_wall_range(level, data, first, last, target);
                self.solidsegs.insert(start, (first, last));
                return;
            }
            let s0 = self.solidsegs[start].0;
            self.store_wall_range(level, data, first, s0 - 1, target);
            self.solidsegs[start].0 = first;
        }
        if last <= self.solidsegs[start].1 {
            return;
        }
        let mut next = start;
        while last >= self.solidsegs[next + 1].0 - 1 {
            let (a, b) = (self.solidsegs[next].1 + 1, self.solidsegs[next + 1].0 - 1);
            self.store_wall_range(level, data, a, b, target);
            next += 1;
            if last <= self.solidsegs[next].1 {
                self.solidsegs[start].1 = self.solidsegs[next].1;
                self.solidsegs.drain(start + 1..=next);
                return;
            }
        }
        let a = self.solidsegs[next].1 + 1;
        self.store_wall_range(level, data, a, last, target);
        self.solidsegs[start].1 = last;
        if next != start {
            self.solidsegs.drain(start + 1..=next);
        }
    }

    /// `R_ClipPassWallSegment`.
    fn clip_pass_wall_segment(
        &mut self,
        level: &SoftLevel,
        data: &SoftData,
        first: i32,
        last: i32,
        target: &mut SoftTarget,
    ) {
        let mut start = 0;
        while self.solidsegs[start].1 < first - 1 {
            start += 1;
        }
        if first < self.solidsegs[start].0 {
            if last < self.solidsegs[start].0 - 1 {
                self.store_wall_range(level, data, first, last, target);
                return;
            }
            let s0 = self.solidsegs[start].0;
            self.store_wall_range(level, data, first, s0 - 1, target);
        }
        if last <= self.solidsegs[start].1 {
            return;
        }
        while last >= self.solidsegs[start + 1].0 - 1 {
            let (a, b) = (self.solidsegs[start].1 + 1, self.solidsegs[start + 1].0 - 1);
            self.store_wall_range(level, data, a, b, target);
            start += 1;
            if last <= self.solidsegs[start].1 {
                return;
            }
        }
        let a = self.solidsegs[start].1 + 1;
        self.store_wall_range(level, data, a, last, target);
    }

    fn alloc_openings(&mut self, n: i32) -> i32 {
        let base = self.openings.len() as i32;
        self.openings.resize((base + n.max(0)) as usize, 0);
        base
    }

    fn light_row(
        &self,
        lightlevel: i32,
        v1: crate::level::SoftVertex,
        v2: crate::level::SoftVertex,
    ) -> usize {
        let mut lightnum = (lightlevel >> LIGHTSEGSHIFT) + self.extralight;
        if v1.y == v2.y {
            lightnum -= 1;
        } else if v1.x == v2.x {
            lightnum += 1;
        }
        lightnum.clamp(0, LIGHTLEVELS as i32 - 1) as usize
    }

    /// `R_StoreWallRange`.
    #[allow(clippy::too_many_lines)]
    fn store_wall_range(
        &mut self,
        level: &SoftLevel,
        data: &SoftData,
        start: i32,
        stop: i32,
        target: &mut SoftTarget,
    ) {
        if self.vanilla_limits && self.drawsegs.len() >= MAXDRAWSEGS {
            return;
        }
        if start >= self.width || start > stop {
            return;
        }
        let seg = seg_at(level, self.curseg);
        let sidedef = level.sides[seg.side as usize];
        let line_flags = level.line_flags[seg.line as usize];
        let front = self.frontsector;
        let back = self.backsector;
        let v1 = level.vertices[seg.v1 as usize];
        let v2 = level.vertices[seg.v2 as usize];
        let texh = |id: u16| (data.textures[id as usize].height as i32) << FRACBITS;
        let lt = self.leveltime;
        let anim = |id: u16| data.anim_texture(id, lt);

        let mut w = Wall {
            angle1: self.wall.angle1,
            parts: sidedef.parts,
            ..Wall::default()
        };
        w.normalangle = seg.angle.wrapping_add(ANG90);
        let mut offsetangle = (w.normalangle.wrapping_sub(w.angle1) as i32).unsigned_abs();
        if offsetangle > ANG90 {
            offsetangle = ANG90;
        }
        let distangle = ANG90 - offsetangle;
        let hyp = self.point_to_dist(v1.x, v1.y);
        let sineval = finesine((distangle >> ANGLETOFINESHIFT) as usize);
        w.distance = fixed_mul(hyp, sineval);
        self.wall.normalangle = w.normalangle;
        self.wall.distance = w.distance;

        w.x = start;
        w.stopx = stop + 1;
        let scale1 = self.scale_from_global_angle(
            self.viewangle
                .wrapping_add(self.xtoviewangle[start as usize]),
        );
        w.scale = scale1;
        let (scale2, scalestep) = if stop > start {
            let s2 = self.scale_from_global_angle(
                self.viewangle
                    .wrapping_add(self.xtoviewangle[stop as usize]),
            );
            (s2, (s2 - scale1) / (stop - start))
        } else {
            (scale1, 0)
        };
        w.scalestep = scalestep;
        let mut ds = DrawSeg {
            seg: self.curseg,
            x1: start,
            x2: stop,
            scale1,
            scale2,
            scalestep,
            silhouette: 0,
            bsilheight: 0,
            tsilheight: 0,
            sprtopclip: Clip::None,
            sprbottomclip: Clip::None,
            maskedtexturecol: Clip::None,
            slabcol: Clip::None,
            slab_first: 0,
            slab_count: 0,
        };

        let mut worldtop = front.ceiling - self.viewz;
        let mut worldbottom = front.floor - self.viewz;
        let mut worldhigh = 0;
        let mut worldlow = 0;
        match back {
            None => {
                w.portal = self.x.portal.cur;
                w.midtexture = if w.portal.is_some() {
                    0
                } else {
                    anim(sidedef.mid)
                };
                w.markfloor = true;
                w.markceiling = true;
                w.midtexturemid = if line_flags & ML_DONTPEGBOTTOM != 0 {
                    front.floor + texh(sidedef.mid) - self.viewz
                } else {
                    worldtop
                };
                w.midtexturemid += sidedef.row_offset;
                ds.silhouette = SIL_BOTTOM | SIL_TOP;
                ds.sprtopclip = Clip::ScreenHeight;
                ds.sprbottomclip = Clip::NegOne;
                ds.bsilheight = i32::MAX;
                ds.tsilheight = i32::MIN;
            }
            Some(back) => {
                if front.floor > back.floor {
                    ds.silhouette = SIL_BOTTOM;
                    ds.bsilheight = front.floor;
                } else if back.floor > self.viewz {
                    ds.silhouette = SIL_BOTTOM;
                    ds.bsilheight = i32::MAX;
                }
                if front.ceiling < back.ceiling {
                    ds.silhouette |= SIL_TOP;
                    ds.tsilheight = front.ceiling;
                } else if back.ceiling < self.viewz {
                    ds.silhouette |= SIL_TOP;
                    ds.tsilheight = i32::MIN;
                }
                if back.ceiling <= front.floor {
                    ds.sprbottomclip = Clip::NegOne;
                    ds.bsilheight = i32::MAX;
                    ds.silhouette |= SIL_BOTTOM;
                }
                if back.floor >= front.ceiling {
                    ds.sprtopclip = Clip::ScreenHeight;
                    ds.tsilheight = i32::MIN;
                    ds.silhouette |= SIL_TOP;
                }
                worldhigh = back.ceiling - self.viewz;
                worldlow = back.floor - self.viewz;
                if front.ceiling_pic == SKY_FLAT && back.ceiling_pic == SKY_FLAT {
                    worldtop = worldhigh;
                }
                // Boom: panning, fake planes and light transfers also
                // start a new plane.
                let (fi, bi) = (self.front_index, self.back_index.unwrap_or(0));
                let get = |v: &Vec<Option<u32>>, i: usize| v.get(i).copied().flatten();
                let front_heightsec = get(&level.heightsec, fi).is_some();
                w.markfloor = worldlow != worldbottom
                    || back.floor_pic != front.floor_pic
                    || back.light != front.light
                    || back.floor_xoffs != front.floor_xoffs
                    || back.floor_yoffs != front.floor_yoffs
                    || front_heightsec
                    || get(&level.floorlightsec, bi) != get(&level.floorlightsec, fi);
                w.markceiling = worldhigh != worldtop
                    || back.ceiling_pic != front.ceiling_pic
                    || back.light != front.light
                    || back.ceiling_xoffs != front.ceiling_xoffs
                    || back.ceiling_yoffs != front.ceiling_yoffs
                    || (front_heightsec && front.ceiling_pic != SKY_FLAT)
                    || get(&level.ceilinglightsec, bi) != get(&level.ceilinglightsec, fi);
                if back.ceiling <= front.floor || back.floor >= front.ceiling {
                    w.markceiling = true;
                    w.markfloor = true;
                }
                // The texture heights are set either way (a sloped wall may
                // need them where the flat heights say there is no step).
                w.toptexturemid = if line_flags & ML_DONTPEGTOP != 0 {
                    worldtop
                } else {
                    back.ceiling + texh(sidedef.top) - self.viewz
                };
                w.bottomtexturemid = if line_flags & ML_DONTPEGBOTTOM != 0 {
                    worldtop
                } else {
                    worldlow
                };
                if worldhigh < worldtop {
                    w.toptexture = anim(sidedef.top);
                }
                if worldlow > worldbottom {
                    w.bottomtexture = anim(sidedef.bottom);
                }
                w.toptexturemid += sidedef.row_offset;
                w.bottomtexturemid += sidedef.row_offset;
                if sidedef.mid != 0 {
                    w.maskedtexture = true;
                    let base = self.alloc_openings(w.stopx - w.x) - w.x;
                    w.maskedcol_base = base;
                    ds.maskedtexturecol = Clip::Openings(base);
                }
                // 3D floor sides seen across this line.
                let (first, count) = self.seg_slab_sides(self.front_index, self.back_index);
                if count > 0 {
                    let base = self.alloc_openings(w.stopx - w.x) - w.x;
                    w.slab_base = Some(base);
                    ds.slabcol = Clip::Openings(base);
                    ds.slab_first = first;
                    ds.slab_count = count;
                }
            }
        }
        let any_slope = |s: &SoftSector| s.floor_slope.is_some() || s.ceiling_slope.is_some();
        let sloped = any_slope(&front) || back.as_ref().is_some_and(any_slope);
        if sloped {
            self.slope_wall(
                &mut w,
                &front,
                back.as_ref(),
                v1,
                v2,
                start,
                stop,
                scale2,
                sidedef,
                &anim,
                false,
            );
        }
        w.segtextured = w.midtexture != 0
            || w.toptexture != 0
            || w.bottomtexture != 0
            || w.maskedtexture
            || w.slab_base.is_some();
        if w.segtextured {
            let mut offsetangle = w.normalangle.wrapping_sub(w.angle1);
            if offsetangle > ANG180 {
                offsetangle = offsetangle.wrapping_neg();
            }
            if offsetangle > ANG90 {
                offsetangle = ANG90;
            }
            let sineval = finesine((offsetangle >> ANGLETOFINESHIFT) as usize);
            w.offset = fixed_mul(hyp, sineval);
            if w.normalangle.wrapping_sub(w.angle1) < ANG180 {
                w.offset = -w.offset;
            }
            w.offset = w
                .offset
                .wrapping_add(sidedef.texture_offset)
                .wrapping_add(seg.offset);
            w.centerangle = ANG90
                .wrapping_add(self.viewangle)
                .wrapping_sub(w.normalangle);
            if self.fixed.is_none() {
                w.walllights = Some(self.light_row(front.light, v1, v2));
            }
        }
        self.wall_light_splits(self.front_index, front.light, v1, v2);
        self.dyn_wall(v1, v2);
        if front.floor >= self.viewz {
            w.markfloor = false;
        }
        if front.ceiling <= self.viewz && front.ceiling_pic != SKY_FLAT {
            w.markceiling = false;
        }
        worldtop >>= 4;
        worldbottom >>= 4;
        w.topstep = -fixed_mul(w.scalestep, worldtop);
        w.topfrac = (self.centeryfrac >> 4) - fixed_mul(worldtop, w.scale);
        w.bottomstep = -fixed_mul(w.scalestep, worldbottom);
        w.bottomfrac = (self.centeryfrac >> 4) - fixed_mul(worldbottom, w.scale);
        if back.is_some() {
            worldhigh >>= 4;
            worldlow >>= 4;
            if worldhigh < worldtop {
                w.pixhigh = (self.centeryfrac >> 4) - fixed_mul(worldhigh, w.scale);
                w.pixhighstep = -fixed_mul(w.scalestep, worldhigh);
            }
            if worldlow > worldbottom {
                w.pixlow = (self.centeryfrac >> 4) - fixed_mul(worldlow, w.scale);
                w.pixlowstep = -fixed_mul(w.scalestep, worldlow);
            }
        }
        if sloped {
            self.slope_wall(
                &mut w,
                &front,
                back.as_ref(),
                v1,
                v2,
                start,
                stop,
                scale2,
                sidedef,
                &anim,
                true,
            );
        }
        if w.markceiling {
            self.ceilingplane = self
                .ceilingplane
                .and_then(|p| self.check_plane(p, w.x, w.stopx - 1));
        }
        if w.markfloor {
            self.floorplane = self
                .floorplane
                .and_then(|p| self.check_plane(p, w.x, w.stopx - 1));
        }
        self.wall = w;
        self.tint = tint_of(&front);
        self.render_seg_loop(data, target);

        let n = w.stopx - start;
        let masked = w.maskedtexture || w.slab_base.is_some();
        if (ds.silhouette & SIL_TOP != 0 || masked) && ds.sprtopclip == Clip::None {
            let base = self.alloc_openings(n);
            for i in 0..n {
                self.openings[(base + i) as usize] = self.ceilingclip[(start + i) as usize];
            }
            ds.sprtopclip = Clip::Openings(base - start);
        }
        if (ds.silhouette & SIL_BOTTOM != 0 || masked) && ds.sprbottomclip == Clip::None {
            let base = self.alloc_openings(n);
            for i in 0..n {
                self.openings[(base + i) as usize] = self.floorclip[(start + i) as usize];
            }
            ds.sprbottomclip = Clip::Openings(base - start);
        }
        if w.maskedtexture && ds.silhouette & SIL_TOP == 0 {
            ds.silhouette |= SIL_TOP;
            ds.tsilheight = i32::MIN;
        }
        if w.maskedtexture && ds.silhouette & SIL_BOTTOM == 0 {
            ds.silhouette |= SIL_BOTTOM;
            ds.bsilheight = i32::MAX;
        }
        self.drawsegs.push(ds);
    }

    /// `R_RenderSegLoop`.
    fn render_seg_loop(&mut self, data: &SoftData, target: &mut SoftTarget) {
        let mut w = self.wall;
        while w.x < w.stopx {
            let x = w.x as usize;
            if self.x.dl.active {
                self.dyn_column(w.x, w.scale);
            }
            let mut yl = (w.topfrac + HEIGHTUNIT - 1) >> HEIGHTBITS;
            if yl < i32::from(self.ceilingclip[x]) + 1 {
                yl = i32::from(self.ceilingclip[x]) + 1;
            }
            if w.markceiling {
                let top = i32::from(self.ceilingclip[x]) + 1;
                let mut bottom = yl - 1;
                if bottom >= i32::from(self.floorclip[x]) {
                    bottom = i32::from(self.floorclip[x]) - 1;
                }
                if top <= bottom {
                    if let Some(p) = self.ceilingplane {
                        self.visplanes[p].top[x + 1] = top as u16;
                        self.visplanes[p].bottom[x + 1] = bottom as u16;
                    }
                }
            }
            let mut yh = w.bottomfrac >> HEIGHTBITS;
            if yh >= i32::from(self.floorclip[x]) {
                yh = i32::from(self.floorclip[x]) - 1;
            }
            if w.markfloor {
                let mut top = yh + 1;
                let bottom = i32::from(self.floorclip[x]) - 1;
                if top <= i32::from(self.ceilingclip[x]) {
                    top = i32::from(self.ceilingclip[x]) + 1;
                }
                if top <= bottom {
                    if let Some(p) = self.floorplane {
                        self.visplanes[p].top[x + 1] = top as u16;
                        self.visplanes[p].bottom[x + 1] = bottom as u16;
                    }
                }
            }
            let mut texturecolumn = 0;
            let mut texcol_fixed = 0;
            let mut colormap = 0usize;
            let mut iscale = 0i32;
            if w.segtextured {
                let angle =
                    (w.centerangle.wrapping_add(self.xtoviewangle[x]) >> ANGLETOFINESHIFT) as usize;
                // Vanilla's `finetangent` is followed by `finesine` in
                // memory; an angle past it reads there.
                let tan = if angle < FINEANGLES / 2 {
                    finetangent(angle)
                } else {
                    finesine(angle - FINEANGLES / 2)
                };
                texcol_fixed = w.offset.wrapping_sub(fixed_mul(tan, w.distance));
                texturecolumn = texcol_fixed >> FRACBITS;
                colormap = match (self.fixed, w.walllights) {
                    (Some(f), _) => f,
                    (None, Some(l)) => self.scale_light(l, w.scale),
                    (None, None) => 0,
                };
                iscale = (0xffff_ffffu32 / (w.scale as u32)) as i32;
            }
            // UDMF scaling / panning per part (identity = vanilla).
            let part = |p: usize, mid: i32| -> (i32, i32, i32) {
                let sp = w.parts[p];
                if sp == crate::level::SidePart::default() {
                    (texturecolumn, mid, iscale)
                } else {
                    (
                        fixed_mul(texcol_fixed.wrapping_add(sp.xoff), sp.xscale) >> FRACBITS,
                        fixed_mul(mid.wrapping_add(sp.yoff), sp.yscale),
                        fixed_mul(iscale, sp.yscale),
                    )
                }
            };
            if let Some(region) = w.portal {
                // The portal's region instead of the wall.
                self.portal_region_set(region, x as i32, yl, yh);
                self.ceilingclip[x] = self.height as i16;
                self.floorclip[x] = -1;
            } else if w.midtexture != 0 {
                let (col, mid, isc) = part(PART_MID, w.midtexturemid);
                self.wall_column(
                    data,
                    w.midtexture,
                    col,
                    x as i32,
                    yl,
                    yh,
                    mid,
                    isc,
                    colormap,
                    w.scale,
                    target,
                );
                self.ceilingclip[x] = self.height as i16;
                self.floorclip[x] = -1;
            } else {
                if w.toptexture != 0 {
                    let mut mid = w.pixhigh >> HEIGHTBITS;
                    w.pixhigh += w.pixhighstep;
                    if mid >= i32::from(self.floorclip[x]) {
                        mid = i32::from(self.floorclip[x]) - 1;
                    }
                    if mid >= yl {
                        let (col, tmid, isc) = part(PART_TOP, w.toptexturemid);
                        self.wall_column(
                            data,
                            w.toptexture,
                            col,
                            x as i32,
                            yl,
                            mid,
                            tmid,
                            isc,
                            colormap,
                            w.scale,
                            target,
                        );
                        self.ceilingclip[x] = mid as i16;
                    } else {
                        self.ceilingclip[x] = (yl - 1) as i16;
                    }
                } else if w.markceiling {
                    self.ceilingclip[x] = (yl - 1) as i16;
                }
                if w.bottomtexture != 0 {
                    let mut mid = (w.pixlow + HEIGHTUNIT - 1) >> HEIGHTBITS;
                    w.pixlow += w.pixlowstep;
                    if mid <= i32::from(self.ceilingclip[x]) {
                        mid = i32::from(self.ceilingclip[x]) + 1;
                    }
                    if mid <= yh {
                        let (col, bmid, isc) = part(PART_BOTTOM, w.bottomtexturemid);
                        self.wall_column(
                            data,
                            w.bottomtexture,
                            col,
                            x as i32,
                            mid,
                            yh,
                            bmid,
                            isc,
                            colormap,
                            w.scale,
                            target,
                        );
                        self.floorclip[x] = mid as i16;
                    } else {
                        self.floorclip[x] = (yh + 1) as i16;
                    }
                } else if w.markfloor {
                    self.floorclip[x] = (yh + 1) as i16;
                }
                if let Some(base) = w.slab_base {
                    if let Some(slot) = self.openings.get_mut((base + w.x) as usize) {
                        *slot = texturecolumn as i16;
                    }
                }
                if w.maskedtexture {
                    let i = (w.maskedcol_base + w.x) as usize;
                    let col = part(PART_MID, 0).0;
                    if let Some(slot) = self.openings.get_mut(i) {
                        *slot = col as i16;
                    }
                }
            }
            w.scale += w.scalestep;
            w.topfrac += w.topstep;
            w.bottomfrac += w.bottomstep;
            w.x += 1;
        }
        self.dyn_column_end();
        self.wall = w;
    }

    /// `R_DrawColumn` (the C version: `(frac >> FRACBITS) & 127`).
    #[allow(clippy::too_many_arguments)]
    fn draw_column(
        &self,
        data: &SoftData,
        tex: u16,
        texturecolumn: i32,
        x: i32,
        yl: i32,
        yh: i32,
        texturemid: i32,
        iscale: i32,
        colormap: usize,
        target: &mut SoftTarget,
    ) {
        if yh < yl || x < 0 || x >= self.width {
            return;
        }
        let yl = yl.max(0);
        let yh = yh.min(self.height - 1);
        let Some(texture) = data.textures.get(tex as usize) else {
            return;
        };
        let source = texture.column(texturecolumn);
        let rgb = if self.truecolor {
            texture.rgb_column(texturecolumn)
        } else {
            None
        };
        let px = self.px(data, colormap);
        let lit = self.truecolor && self.x.dl.col.is_some();
        let mut frac = texturemid.wrapping_add((yl - self.centery).wrapping_mul(iscale));
        let w = self.width as usize;
        let h = texture.height as i32;
        if let (Px::Index(cmap), false) = (px, lit) {
            // 8-bit: the colormap row straight into the frame (SR9: no
            // per-pixel dispatch).
            let pixels = &mut target.pixels;
            let mut i = yl as usize * w + x as usize;
            if self.boom && h > 0 && h & (h - 1) != 0 {
                let hm = h << FRACBITS;
                frac = frac.rem_euclid(hm);
                for _ in yl..=yh {
                    let t = (frac >> FRACBITS) as usize;
                    pixels[i] = cmap[usize::from(source.get(t).copied().unwrap_or(0))];
                    frac = (frac + iscale).rem_euclid(hm);
                    i += w;
                }
            } else {
                let mask = if self.boom && h > 0 { h - 1 } else { 127 };
                for _ in yl..=yh {
                    let t = ((frac >> FRACBITS) & mask) as usize;
                    pixels[i] = cmap[usize::from(source.get(t).copied().unwrap_or(0))];
                    frac = frac.wrapping_add(iscale);
                    i += w;
                }
            }
            return;
        }
        if let (Px::True(TrueShade { lut: Some(lut), .. }), None, false) = (px, rgb, lit) {
            // True colour, plain light: the shaded palette (SR9).
            let out = &mut target.rgb;
            let mut i = yl as usize * w + x as usize;
            if self.boom && h > 0 && h & (h - 1) != 0 {
                let hm = h << FRACBITS;
                frac = frac.rem_euclid(hm);
                for _ in yl..=yh {
                    let t = (frac >> FRACBITS) as usize;
                    out[i] = lut[usize::from(source.get(t).copied().unwrap_or(0))];
                    frac = (frac + iscale).rem_euclid(hm);
                    i += w;
                }
            } else {
                let mask = if self.boom && h > 0 { h - 1 } else { 127 };
                for _ in yl..=yh {
                    let t = ((frac >> FRACBITS) & mask) as usize;
                    out[i] = lut[usize::from(source.get(t).copied().unwrap_or(0))];
                    frac = frac.wrapping_add(iscale);
                    i += w;
                }
            }
            return;
        }
        if self.boom && h > 0 && h & (h - 1) != 0 {
            // Boom: a height that is no power of two wraps at the height.
            let hm = h << FRACBITS;
            frac = frac.rem_euclid(hm);
            for y in yl..=yh {
                let t = (frac >> FRACBITS) as usize;
                let texel = source.get(t).copied().unwrap_or(0);
                let c = rgb.and_then(|r| r.get(t).copied());
                px.put_rgb(target, y as usize * w + x as usize, texel, c);
                if lit {
                    let base = Self::texel_rgb(data, texel, c);
                    self.dyn_wall_pixel(target, y as usize * w + x as usize, y, base);
                }
                frac = (frac + iscale).rem_euclid(hm);
            }
            return;
        }
        let mask = if self.boom && h > 0 { h - 1 } else { 127 };
        for y in yl..=yh {
            let t = ((frac >> FRACBITS) & mask) as usize;
            let texel = source.get(t).copied().unwrap_or(0);
            let c = rgb.and_then(|r| r.get(t).copied());
            px.put_rgb(target, y as usize * w + x as usize, texel, c);
            if lit {
                let base = Self::texel_rgb(data, texel, c);
                self.dyn_wall_pixel(target, y as usize * w + x as usize, y, base);
            }
            frac = frac.wrapping_add(iscale);
        }
    }

    /// The map point (units) of the seg `v1`–`v2` seen through column `x`.
    fn seg_point_at_column(&self, v1: SoftVertex, v2: SoftVertex, x: i32) -> (f64, f64) {
        let a = f64::from(
            self.viewangle
                .wrapping_add(self.xtoviewangle[x.clamp(0, self.width - 1) as usize]),
        ) / 4_294_967_296.0
            * std::f64::consts::TAU;
        let (dx, dy) = (a.cos(), a.sin());
        let (vx, vy) = (
            f64::from(self.viewx) / 65536.0,
            f64::from(self.viewy) / 65536.0,
        );
        let (ax, ay) = (f64::from(v1.x) / 65536.0, f64::from(v1.y) / 65536.0);
        let (ex, ey) = (
            f64::from(v2.x) / 65536.0 - ax,
            f64::from(v2.y) / 65536.0 - ay,
        );
        // view + t·d = a + u·e
        let den = dx * ey - dy * ex;
        if den.abs() < 1e-12 {
            return (ax, ay);
        }
        let u = (((ax - vx) * dy - (ay - vy) * dx) / den).clamp(0.0, 1.0);
        (ax + u * ex, ay + u * ey)
    }

    /// ZDoom sloped walls: plane heights at the two drawn ends of the wall
    /// give its screen edges (a planar wall's edge projects to a straight
    /// line), which replace the flat `topfrac` / `bottomfrac` / `pixhigh` /
    /// `pixlow` and decide the upper and lower textures.
    #[allow(clippy::too_many_arguments)]
    fn slope_wall(
        &self,
        w: &mut Wall,
        front: &SoftSector,
        back: Option<&SoftSector>,
        v1: SoftVertex,
        v2: SoftVertex,
        start: i32,
        stop: i32,
        scale2: i32,
        sidedef: crate::level::SoftSide,
        anim: &impl Fn(u16) -> u16,
        edges: bool,
    ) {
        let p0 = self.seg_point_at_column(v1, v2, start);
        let p1 = self.seg_point_at_column(v1, v2, stop);
        let vz = f64::from(self.viewz) / 65536.0;
        let cy = f64::from(self.centeryfrac) / 65536.0;
        let (s0, s1) = (f64::from(w.scale) / 65536.0, f64::from(scale2) / 65536.0);
        let n = f64::from((stop - start).max(1));
        let h = |s: &SoftSector, ceiling: bool, p: (f64, f64)| -> f64 {
            let slope = if ceiling {
                s.ceiling_slope
            } else {
                s.floor_slope
            };
            match slope {
                Some(sl) => sl.z_at(p.0, p.1),
                None => f64::from(if ceiling { s.ceiling } else { s.floor }) / 65536.0,
            }
        };
        // Edge as HEIGHTBITS fixed rows: (frac at start, step per column).
        let edge = |s: &SoftSector, ceiling: bool| -> (i32, i32) {
            let y0 = (cy - (h(s, ceiling, p0) - vz) * s0) * 4096.0;
            let y1 = (cy - (h(s, ceiling, p1) - vz) * s1) * 4096.0;
            (y0.round() as i32, ((y1 - y0) / n).round() as i32)
        };
        let top_sector = match back {
            Some(b) if front.ceiling_pic == SKY_FLAT && b.ceiling_pic == SKY_FLAT => b,
            _ => front,
        };
        if !edges {
            // Which textures the wall has.
            if let Some(b) = back {
                let higher = |p| h(b, true, p) < h(top_sector, true, p);
                w.toptexture = if higher(p0) || higher(p1) {
                    anim(sidedef.top)
                } else {
                    0
                };
                let lower = |p| h(b, false, p) > h(front, false, p);
                w.bottomtexture = if lower(p0) || lower(p1) {
                    anim(sidedef.bottom)
                } else {
                    0
                };
            }
            return;
        }
        (w.topfrac, w.topstep) = edge(top_sector, true);
        (w.bottomfrac, w.bottomstep) = edge(front, false);
        if let Some(b) = back {
            (w.pixhigh, w.pixhighstep) = edge(b, true);
            (w.pixlow, w.pixlowstep) = edge(b, false);
            if b.floor_slope != front.floor_slope || b.floor != front.floor {
                w.markfloor = true;
            }
            if b.ceiling_slope != front.ceiling_slope || b.ceiling != front.ceiling {
                w.markceiling = true;
            }
        }
        // A sloped plane is seen when the eye is on its upper (floor) or
        // lower (ceiling) side there (vanilla tests the flat height).
        let eye = (
            f64::from(self.viewx) / 65536.0,
            f64::from(self.viewy) / 65536.0,
        );
        if front.floor_slope.is_some() {
            if h(front, false, eye) >= vz {
                w.markfloor = false;
            } else if back.is_none() {
                w.markfloor = true;
            }
        }
        if front.ceiling_slope.is_some() && front.ceiling_pic != SKY_FLAT {
            if h(front, true, eye) <= vz {
                w.markceiling = false;
            } else if back.is_none() {
                w.markceiling = true;
            }
        }
    }

    /// Colormap row `row` of this frame's set (Boom 242 colormaps).
    #[inline]
    fn cmap<'a>(&self, data: &'a SoftData, row: usize) -> &'a [u8] {
        data.colormap_in(self.cmap_set, row)
    }

    /// The pixel writer for colormap row `row` and the current tint.
    #[inline]
    fn px<'a>(&self, data: &'a SoftData, row: usize) -> Px<'a> {
        if !self.truecolor {
            return Px::Index(self.cmap(data, row & !FINE_LIGHT));
        }
        // Smooth light: a fine level (1/256 rows) from the true-colour
        // light functions.
        let (row, fine) = if row & FINE_LIGHT != 0 {
            let f = (row & 0xffff).min(NUMCOLORMAPS * 256 - 1);
            (f >> 8, f)
        } else {
            (row, row.min(NUMCOLORMAPS) * 256)
        };
        let special = self.cmap_set != 0 || row >= NUMCOLORMAPS;
        let tint = if self.tint.0 == 0 {
            [255; 3]
        } else {
            unpack(self.tint.0)
        };
        let light = ((NUMCOLORMAPS * 256 - fine.min(NUMCOLORMAPS * 256)) / NUMCOLORMAPS) as u32;
        let mut sh = TrueShade::new(
            &data.palette,
            special.then(|| self.cmap(data, row)),
            light,
            tint,
            unpack(self.tint.1 & 0xff_ffff),
            self.tint.1 >> 24,
        );
        if !special && self.tint.0 == 0 && self.tint.1 == 0 {
            sh.lut = data.shade_lut().get(light.min(256) as usize);
        }
        Px::True(sh)
    }

    /// `scalelight[l][scale]` (walls, sprites); in true colour a fine,
    /// continuous level (no 32-row steps, no 48 scale steps).
    fn scale_light(&self, l: usize, scale: i32) -> usize {
        if !self.truecolor {
            return self.scalelight[l][self.scale_light_index(scale)];
        }
        let s = if self.visfixed == FRACUNIT {
            f64::from(scale)
        } else {
            f64::from(scale) * f64::from(self.visfixed) / 65536.0
        };
        let j = (s / f64::from(1 << LIGHTSCALESHIFT)).clamp(0.0, (MAXLIGHTSCALE - 1) as f64);
        let level =
            startmap(l) - j * f64::from(ORIGWIDTH) / f64::from(self.basew) / f64::from(DISTMAP);
        fine_level(level)
    }

    /// `zlight[l][distance]` (planes); fine and continuous in true colour.
    fn z_light(&self, l: usize, distance: i32) -> usize {
        if !self.truecolor {
            return self.zlight[l][self.z_light_index(distance)];
        }
        let d = f64::from(distance) * 65536.0 / f64::from(self.visfixed);
        let j = (d / f64::from(1 << LIGHTZSHIFT)).clamp(0.0, (MAXLIGHTZ - 1) as f64);
        // Vanilla: `scale = (160 << 16) / ((j + 1) << 20) >> 12 = 160 / (j + 1)`.
        let level = startmap(l) - f64::from(ORIGWIDTH / 2) / (j + 1.0) / f64::from(DISTMAP);
        fine_level(level)
    }

    /// Boom `R_FakeFlat`: sector `si` as drawn, with a 242 control sector's
    /// planes (normal, under the fake floor or over the fake ceiling) and
    /// the 213 / 261 light transfers. Returns the sector and the floor and
    /// ceiling light levels.
    fn fake_flat(
        &self,
        level: &SoftLevel,
        sectors: &[SoftSector],
        si: usize,
        back: bool,
    ) -> (SoftSector, i32, i32) {
        let sec = sectors.get(si).copied().unwrap_or_default();
        let light_of = |list: &Vec<Option<u32>>, i: usize, own: i32| {
            list.get(i)
                .copied()
                .flatten()
                .and_then(|l| sectors.get(l as usize))
                .map_or(own, |s| s.light)
        };
        let mut floorlight = light_of(&level.floorlightsec, si, sec.light);
        let mut ceilinglight = light_of(&level.ceilinglightsec, si, sec.light);
        let Some(hs) = level.heightsec.get(si).copied().flatten() else {
            return (sec, floorlight, ceilinglight);
        };
        let Some(&s) = sectors.get(hs as usize) else {
            return (sec, floorlight, ceilinglight);
        };
        let view_hs = self.view_heightsec.and_then(|v| sectors.get(v as usize));
        let underwater = view_hs.is_some_and(|v| self.viewz <= v.floor);
        let mut t = sec;
        t.floor = s.floor;
        t.ceiling = s.ceiling;
        let control_lights = |fl: &mut i32, cl: &mut i32| {
            *fl = light_of(&level.floorlightsec, hs as usize, s.light);
            *cl = light_of(&level.ceilinglightsec, hs as usize, s.light);
        };
        if underwater {
            t.floor = sec.floor;
            t.ceiling = s.floor - 1;
        }
        if underwater && !back {
            // Head below the fake floor.
            t.floor_pic = s.floor_pic;
            t.floor_xoffs = s.floor_xoffs;
            t.floor_yoffs = s.floor_yoffs;
            if s.ceiling_pic == SKY_FLAT {
                t.floor = t.ceiling + 1;
                t.ceiling_pic = t.floor_pic;
                t.ceiling_xoffs = t.floor_xoffs;
                t.ceiling_yoffs = t.floor_yoffs;
            } else {
                t.ceiling_pic = s.ceiling_pic;
                t.ceiling_xoffs = s.ceiling_xoffs;
                t.ceiling_yoffs = s.ceiling_yoffs;
            }
            t.light = s.light;
            control_lights(&mut floorlight, &mut ceilinglight);
        } else if view_hs.is_some_and(|v| self.viewz >= v.ceiling) && sec.ceiling > s.ceiling {
            // Above the fake ceiling.
            t.ceiling = s.ceiling;
            t.floor = s.ceiling + 1;
            t.floor_pic = s.ceiling_pic;
            t.ceiling_pic = s.ceiling_pic;
            t.floor_xoffs = s.ceiling_xoffs;
            t.ceiling_xoffs = s.ceiling_xoffs;
            t.floor_yoffs = s.ceiling_yoffs;
            t.ceiling_yoffs = s.ceiling_yoffs;
            if s.floor_pic != SKY_FLAT {
                t.ceiling = sec.ceiling;
                t.floor_pic = s.floor_pic;
                t.floor_xoffs = s.floor_xoffs;
                t.floor_yoffs = s.floor_yoffs;
            }
            t.light = s.light;
            control_lights(&mut floorlight, &mut ceilinglight);
        }
        (t, floorlight, ceilinglight)
    }

    /// `R_MapPlane` + `R_DrawSpan`.
    fn map_plane(
        &mut self,
        data: &SoftData,
        flat: &[u8],
        y: i32,
        x1: i32,
        x2: i32,
        target: &mut SoftTarget,
    ) {
        if x2 < x1 || x1 < 0 || x2 >= self.width || y < 0 || y >= self.height {
            return;
        }
        if self.plane_slope.is_some()
            || !self.plane_xform.is_identity()
            || self.x.slab_plane.is_some()
            || self.x.dl.active
        {
            self.map_plane_modern(data, flat, y, x1, x2, target);
            return;
        }
        let yu = y as usize;
        let (distance, xstep, ystep) = if self.planeheight != self.cachedheight[yu] {
            self.cachedheight[yu] = self.planeheight;
            let d = fixed_mul(self.planeheight, self.yslope[yu]);
            self.cacheddistance[yu] = d;
            self.cachedxstep[yu] = fixed_mul(d, self.basexscale);
            self.cachedystep[yu] = fixed_mul(d, self.baseyscale);
            (d, self.cachedxstep[yu], self.cachedystep[yu])
        } else {
            (
                self.cacheddistance[yu],
                self.cachedxstep[yu],
                self.cachedystep[yu],
            )
        };
        let length = fixed_mul(distance, self.distscale[x1 as usize]);
        let angle = (self.viewangle.wrapping_add(self.xtoviewangle[x1 as usize])
            >> ANGLETOFINESHIFT) as usize;
        let xfrac = self
            .viewx
            .wrapping_add(fixed_mul(finecosine(angle), length))
            .wrapping_add(self.plane_xoffs);
        let yfrac = self
            .viewy
            .wrapping_neg()
            .wrapping_sub(fixed_mul(finesine(angle), length))
            .wrapping_add(self.plane_yoffs);
        let colormap = self
            .fixed
            .unwrap_or_else(|| self.z_light(self.planezlight, distance));
        let px = self.px(data, colormap);
        let rgb = if self.truecolor {
            data.flat_rgb
                .get(usize::from(self.plane_flat))
                .and_then(Option::as_deref)
                .filter(|r| r.len() >= 4096)
        } else {
            None
        };
        let mut position =
            (((xfrac as u32) << 10) & 0xffff_0000) | (((yfrac >> 6) as u32) & 0x0000_ffff);
        let step = (((xstep as u32) << 10) & 0xffff_0000) | (((ystep >> 6) as u32) & 0x0000_ffff);
        let row = yu * self.width as usize;
        if let Px::Index(cmap) = px {
            // 8-bit: no per-pixel dispatch (SR9).
            for p in &mut target.pixels[row + x1 as usize..=row + x2 as usize] {
                let ytemp = (position >> 4) & 0x0fc0;
                let xtemp = position >> 26;
                *p = cmap[usize::from(flat[(xtemp | ytemp) as usize])];
                position = position.wrapping_add(step);
            }
            return;
        }
        if let (Px::True(TrueShade { lut: Some(lut), .. }), None) = (px, rgb) {
            for p in &mut target.rgb[row + x1 as usize..=row + x2 as usize] {
                let ytemp = (position >> 4) & 0x0fc0;
                let xtemp = position >> 26;
                *p = lut[usize::from(flat[(xtemp | ytemp) as usize])];
                position = position.wrapping_add(step);
            }
            return;
        }
        for x in x1..=x2 {
            let ytemp = (position >> 4) & 0x0fc0;
            let xtemp = position >> 26;
            let spot = (xtemp | ytemp) as usize;
            px.put_rgb(target, row + x as usize, flat[spot], rgb.map(|r| r[spot]));
            position = position.wrapping_add(step);
        }
    }

    /// ZDoom's tilted / transformed spans: each pixel's view ray meets the
    /// plane (sloped, or flat with UDMF scale / rotation); texture and light
    /// follow the hit point and its depth.
    fn map_plane_modern(
        &mut self,
        data: &SoftData,
        flat: &[u8],
        y: i32,
        x1: i32,
        x2: i32,
        target: &mut SoftTarget,
    ) {
        let proj = f64::from(self.projection) / 65536.0;
        let dz = (f64::from(self.centery) - f64::from(y) - 0.5) / proj;
        let (vx, vy, vz) = (
            f64::from(self.viewx) / 65536.0,
            f64::from(self.viewy) / 65536.0,
            f64::from(self.viewz) / 65536.0,
        );
        let (xo, yo) = (
            f64::from(self.plane_xoffs) / 65536.0,
            f64::from(self.plane_yoffs) / 65536.0,
        );
        let xf = self.plane_xform;
        let (sa, ca) = (f64::from(xf.angle) / 4_294_967_296.0 * std::f64::consts::TAU).sin_cos();
        let (xs, ys) = (
            f64::from(xf.xscale) / 65536.0,
            f64::from(xf.yscale) / 65536.0,
        );
        let rgb = if self.truecolor {
            data.flat_rgb
                .get(usize::from(self.plane_flat))
                .and_then(Option::as_deref)
                .filter(|r| r.len() >= 4096)
        } else {
            None
        };
        let row = y as usize * self.width as usize;
        let h = f64::from(self.plane_height_raw) / 65536.0;
        for x in x1..=x2 {
            let (rx, ry) = self.col_dirs[x as usize];
            let t = match self.plane_slope {
                Some(p) => {
                    let den = dz - p.dzdx * rx - p.dzdy * ry;
                    if den.abs() < 1e-9 {
                        continue;
                    }
                    (p.z_at(vx, vy) - vz) / den
                }
                None => {
                    if dz.abs() < 1e-9 {
                        continue;
                    }
                    (h - vz) / dz
                }
            };
            if t.is_nan() || t <= 0.0 {
                continue;
            }
            let (wx, wy) = (vx + t * rx, vy + t * ry);
            // Vanilla texture space: (x + xoffs, -y + yoffs), then UDMF
            // rotation and scale.
            let (u, v) = (wx + xo, -wy + yo);
            let (u, v) = ((u * ca - v * sa) * xs, (u * sa + v * ca) * ys);
            let spot = ((v.floor() as i64 & 63) * 64 + (u.floor() as i64 & 63)) as usize;
            let colormap = self.fixed.unwrap_or_else(|| {
                self.z_light(
                    self.planezlight,
                    (t * 65536.0).min(f64::from(i32::MAX)) as i32,
                )
            });
            let px = self.px(data, colormap);
            self.plane_pixel(
                px,
                data,
                target,
                row + x as usize,
                flat[spot],
                rgb.map(|r| r[spot]),
                t,
            );
            if self.x.dl.active && self.truecolor {
                let hz = vz + t * dz;
                let n = match self.plane_slope {
                    Some(p) => {
                        let l = (p.dzdx * p.dzdx + p.dzdy * p.dzdy + 1.0).sqrt();
                        [-p.dzdx / l, -p.dzdy / l, 1.0 / l]
                    }
                    None => [0.0, 0.0, 1.0],
                };
                // Towards the viewer.
                let n = if vz >= hz { n } else { n.map(|v| -v) };
                let base = Self::texel_rgb(data, flat[spot], rgb.map(|r| r[spot]));
                self.dyn_plane_pixel(target, row + x as usize, [wx, wy, hz], n, base);
            }
        }
    }

    /// `R_DrawPlanes`.
    fn draw_planes(
        &mut self,
        level: &SoftLevel,
        data: &SoftData,
        sky: u16,
        target: &mut SoftTarget,
    ) {
        let planes = std::mem::take(&mut self.visplanes);
        for pl in &planes {
            if pl.minx > pl.maxx {
                continue;
            }
            if pl.picnum & PL_SKYFLAT == 0 && pl.picnum & PL_PORTAL != 0 {
                // Drawn later as a portal.
                self.portal_plane(pl);
                continue;
            }
            if pl.picnum == u32::from(SKY_FLAT) || pl.picnum & PL_SKYFLAT != 0 {
                // Default sky, or a Boom transfer: the line's upper texture,
                // panned by its side, mirrored for 271.
                let (texture, texturemid, an, flip) = if pl.picnum & PL_SKYFLAT != 0 {
                    let side_index = (pl.picnum & !PL_SKYFLAT) as usize;
                    let side = level.sides.get(side_index).copied().unwrap_or_default();
                    let mirrored = level
                        .sky
                        .iter()
                        .flatten()
                        .find(|t| t.side as usize == side_index)
                        .is_some_and(|t| t.mirrored);
                    (
                        data.anim_texture(side.top, self.leveltime),
                        side.row_offset - 28 * FRACUNIT,
                        self.viewangle.wrapping_add(side.texture_offset as u32),
                        if mirrored { !0u32 } else { 0 },
                    )
                } else {
                    (
                        sky,
                        100 * FRACUNIT,
                        self.viewangle.wrapping_add(self.x.portal.sky_offsets[0]),
                        0,
                    )
                };
                let double = pl.picnum & PL_SKYFLAT == 0;
                let sky2 = self.x.portal.sky2.filter(|_| double);
                // Vanilla draws the sky with colormap 0 whatever the fixed
                // colormap; MBF follows it.
                let colormap = if self.boom {
                    self.fixed.unwrap_or(0)
                } else {
                    0
                };
                self.tint = (0, 0);
                let iscale = self.pspriteiscale;
                for x in pl.minx..=pl.maxx {
                    let yl = i32::from(pl.top[(x + 1) as usize]);
                    let yh = i32::from(pl.bottom[(x + 1) as usize]);
                    if pl.top[(x + 1) as usize] != PLANE_UNSET && yl <= yh {
                        let angle = (an.wrapping_add(self.xtoviewangle[x as usize]) ^ flip)
                            >> ANGLETOSKYSHIFT;
                        if let Some(back) = sky2 {
                            // Hexen: the back sky, then the front with
                            // colour 0 clear.
                            let a2 = self.sky_angle(x, 1);
                            self.draw_column(
                                data, back, a2, x, yl, yh, texturemid, iscale, colormap, target,
                            );
                            self.sky_front_column(
                                data,
                                texture,
                                angle as i32,
                                x,
                                yl,
                                yh,
                                texturemid,
                                iscale,
                                colormap,
                                target,
                            );
                            continue;
                        }
                        self.draw_column(
                            data,
                            texture,
                            angle as i32,
                            x,
                            yl,
                            yh,
                            texturemid,
                            iscale,
                            colormap,
                            target,
                        );
                    }
                }
                continue;
            }
            self.draw_flat_plane(data, pl, target);
        }
        self.visplanes = planes;
    }

    /// One flat (non-sky) visplane: `R_DrawPlanes`' span loop.
    fn draw_flat_plane(&mut self, data: &SoftData, pl: &Visplane, target: &mut SoftTarget) {
        {
            let picnum = data.anim_flat(pl.picnum as u16, self.leveltime);
            self.tint = pl.tint;
            self.plane_xoffs = pl.xoffs;
            self.plane_yoffs = pl.yoffs;
            self.plane_flat = picnum;
            self.plane_slope = pl.slope;
            self.plane_xform = pl.xform;
            self.plane_height_raw = pl.height;
            let flat = data
                .flats
                .get(picnum as usize)
                .map_or(&[0u8; 4096][..], Vec::as_slice);
            self.planeheight = pl.height.wrapping_sub(self.viewz).wrapping_abs();
            self.planezlight = self.plane_light_index(pl.light);
            // Vanilla's pad columns: unset by construction (no clone).
            let top = &pl.top;
            let bottom = &pl.bottom;
            let stop = pl.maxx + 1;
            for x in pl.minx..=stop {
                let (i0, i1) = (x as usize, (x + 1) as usize);
                self.make_spans(data, flat, x, top, bottom, i0, i1, target);
            }
        }
    }

    /// `R_MakeSpans`.
    #[allow(clippy::too_many_arguments)]
    fn make_spans(
        &mut self,
        data: &SoftData,
        flat: &[u8],
        x: i32,
        top: &[u16],
        bottom: &[u16],
        prev: usize,
        cur: usize,
        target: &mut SoftTarget,
    ) {
        let val = |v: u16| {
            if v == PLANE_UNSET {
                i32::from(u16::MAX)
            } else {
                i32::from(v)
            }
        };
        let (mut t1, mut b1) = (val(top[prev]), i32::from(bottom[prev]));
        let (mut t2, mut b2) = (val(top[cur]), i32::from(bottom[cur]));
        while t1 < t2 && t1 <= b1 {
            let s = self.spanstart[t1 as usize];
            self.map_plane(data, flat, t1, s, x - 1, target);
            t1 += 1;
        }
        while b1 > b2 && b1 >= t1 {
            let s = self.spanstart[b1 as usize];
            self.map_plane(data, flat, b1, s, x - 1, target);
            b1 -= 1;
        }
        while t2 < t1 && t2 <= b2 {
            self.spanstart[t2 as usize] = x;
            t2 += 1;
        }
        while b2 > b1 && b2 >= t2 {
            self.spanstart[b2 as usize] = x;
            b2 -= 1;
        }
    }
}

/// A sector's light colour and fade for the true-colour drawers, with its
/// desaturation in the fade's top byte.
fn tint_of(s: &SoftSector) -> (u32, u32) {
    (
        s.color,
        (s.fade & 0xff_ffff) | (u32::from(s.desaturation) << 24),
    )
}

/// One pixel of a ZDoom render style: `src` over `dst` at opacity `a`
/// (0..=256).
fn blend(style: SoftStyle, src: [u32; 3], dst: [u32; 3], a: u32) -> [u32; 3] {
    let mut out = [0u32; 3];
    for k in 0..3 {
        let (s, d) = (src[k], dst[k]);
        out[k] = match style {
            SoftStyle::Add | SoftStyle::AddStencil | SoftStyle::AddShaded => {
                (d + ((s * a) >> 8)).min(255)
            }
            SoftStyle::Subtract => d.saturating_sub((s * a) >> 8),
            _ => (s * a + d * (256 - a)) >> 8,
        };
    }
    out
}

/// Seg numbers with this bit are [`SoftLevel::poly_segs`].
const POLY_SEG: u32 = 1 << 31;

/// A BSP seg or (with [`POLY_SEG`]) a polyobject seg.
#[inline]
fn seg_at(level: &SoftLevel, i: u32) -> SoftSeg {
    if i & POLY_SEG != 0 {
        level.poly_segs[(i & !POLY_SEG) as usize]
    } else {
        level.segs[i as usize]
    }
}

/// Marks a colormap value as a fine level (row × 256) for the true-colour
/// drawers.
const FINE_LIGHT: usize = 1 << 20;

/// `startmap` of light level `l` (0..16) as a colormap row.
fn startmap(l: usize) -> f64 {
    ((LIGHTLEVELS - 1 - l.min(LIGHTLEVELS - 1)) * 2 * NUMCOLORMAPS) as f64 / LIGHTLEVELS as f64
}

fn fine_level(level: f64) -> usize {
    FINE_LIGHT | (level.clamp(0.0, (NUMCOLORMAPS - 1) as f64) * 256.0).round() as usize
}

/// `R_PointOnSide` / `R_PointOnSegSide` for a line through `(lx, ly)` along
/// `(ldx, ldy)`: 0 = front, 1 = back.
pub(crate) fn point_on_line(x: i32, y: i32, lx: i32, ly: i32, ldx: i32, ldy: i32) -> usize {
    if ldx == 0 {
        return usize::from(if x <= lx { ldy > 0 } else { ldy < 0 });
    }
    if ldy == 0 {
        return usize::from(if y <= ly { ldx < 0 } else { ldx > 0 });
    }
    let dx = x.wrapping_sub(lx);
    let dy = y.wrapping_sub(ly);
    if (ldy ^ ldx ^ dx ^ dy) & i32::MIN != 0 {
        return usize::from((ldy ^ dx) & i32::MIN != 0);
    }
    let left = fixed_mul(ldy >> FRACBITS, dx);
    let right = fixed_mul(dy, ldx >> FRACBITS);
    usize::from(right >= left)
}

/// `R_InitTranslationTables`: the green ramp 0x70–0x7f to gray, brown, red.
fn translation_tables() -> [[u8; 256]; 3] {
    let mut t = [[0u8; 256]; 3];
    for i in 0..256usize {
        let v = i as u8;
        if (0x70..=0x7f).contains(&v) {
            t[0][i] = 0x60 + (v & 0xf);
            t[1][i] = 0x40 + (v & 0xf);
            t[2][i] = 0x20 + (v & 0xf);
        } else {
            t[0][i] = v;
            t[1][i] = v;
            t[2][i] = v;
        }
    }
    t
}

impl SoftRenderer {
    /// `R_AddSprites`: every thing of `sector`, once per frame.
    fn add_sprites(&mut self, data: &SoftData, sector: usize, light: i32, heightsec: Option<u32>) {
        if self.sector_added.get(sector).copied().unwrap_or(true) {
            return;
        }
        self.sector_added[sector] = true;
        let lightnum =
            ((light >> LIGHTSEGSHIFT) + self.extralight).clamp(0, LIGHTLEVELS as i32 - 1);
        let list = std::mem::take(&mut self.sector_things[sector]);
        let slabs = self.has_slabs(sector);
        for &i in &list {
            let thing = self.things[i as usize];
            if self.portal_culled(thing.x, thing.y) {
                continue;
            }
            let lightnum = if slabs {
                let l = self.light_at(sector, thing.z, light);
                ((l >> LIGHTSEGSHIFT) + self.extralight).clamp(0, LIGHTLEVELS as i32 - 1)
            } else {
                lightnum
            };
            self.project_sprite(data, &thing, lightnum as usize, heightsec);
        }
        self.sector_things[sector] = list;
    }

    /// `R_ProjectSprite`.
    fn project_sprite(
        &mut self,
        data: &SoftData,
        thing: &SoftThing,
        lightnum: usize,
        heightsec: Option<u32>,
    ) {
        if let Some(pose) = thing.voxel {
            self.project_voxel(data, thing, pose, lightnum);
            return;
        }
        if thing.kind != SoftSpriteKind::Billboard || thing.roll != 0 || thing.yflip {
            self.project_quad_thing(data, thing, lightnum);
            return;
        }
        let tr_x = thing.x.wrapping_sub(self.viewx);
        let tr_y = thing.y.wrapping_sub(self.viewy);
        let gxt = fixed_mul(tr_x, self.viewcos);
        let gyt = -fixed_mul(tr_y, self.viewsin);
        let tz = gxt.wrapping_sub(gyt);
        if tz < MINZ {
            return;
        }
        let xscale = fixed_div(self.projection, tz);
        let gxt = -fixed_mul(tr_x, self.viewsin);
        let gyt = fixed_mul(tr_y, self.viewcos);
        let mut tx = -(gyt.wrapping_add(gxt));
        if tx.wrapping_abs() > tz << 2 {
            return;
        }
        let Some(frames) = data.sprite_frames(sprite_str(&thing.sprite)) else {
            return;
        };
        let Some(sprframe) = frames.get((thing.frame & FF_FRAMEMASK) as usize) else {
            return;
        };
        let rot = if sprframe.rotate {
            let ang = self.point_to_angle(thing.x, thing.y);
            (ang.wrapping_sub(thing.angle)
                .wrapping_add((ANG45 / 2).wrapping_mul(9))
                >> 29) as usize
        } else {
            0
        };
        let (Some(lump), flip) = (sprframe.lump[rot], sprframe.flip[rot]) else {
            return;
        };
        let flip = flip ^ thing.xflip;
        let patch = &data.patches[lump as usize];
        // ZDoom sprite scale (1.0 = vanilla).
        let sx = if thing.scale_x == 0 {
            FRACUNIT
        } else {
            thing.scale_x
        };
        let sy = if thing.scale_y == 0 {
            FRACUNIT
        } else {
            thing.scale_y
        };
        let scaled = sx != FRACUNIT || sy != FRACUNIT;
        let offset = if scaled {
            fixed_mul(patch.left << FRACBITS, sx)
        } else {
            patch.left << FRACBITS
        };
        let width = patch.width << FRACBITS;
        let swidth = if scaled { fixed_mul(width, sx) } else { width };
        tx = tx.wrapping_sub(offset);
        let x1 = (self.centerxfrac.wrapping_add(fixed_mul(tx, xscale))) >> FRACBITS;
        if x1 > self.width {
            return;
        }
        tx = tx.wrapping_add(swidth);
        let x2 = ((self.centerxfrac.wrapping_add(fixed_mul(tx, xscale))) >> FRACBITS) - 1;
        if x2 < 0 {
            return;
        }
        if self.vanilla_limits && self.vissprites.len() >= MAXVISSPRITES {
            return;
        }
        let top = if scaled {
            fixed_mul(patch.top << FRACBITS, sy)
        } else {
            patch.top << FRACBITS
        };
        let gzt = thing.z.wrapping_add(top);
        let mut iscale = fixed_div(FRACUNIT, xscale);
        let (yscale, yiscale) = if scaled {
            iscale = fixed_div(iscale, sx);
            let ys = fixed_mul(xscale, sy);
            (ys, fixed_div(FRACUNIT, ys))
        } else {
            (xscale, iscale)
        };
        let mut vis = VisSprite {
            x1: x1.max(0),
            x2: x2.min(self.width - 1),
            gx: thing.x,
            gy: thing.y,
            gz: thing.z,
            gzt,
            startfrac: 0,
            scale: xscale,
            xiscale: iscale,
            texturemid: gzt.wrapping_sub(self.viewz),
            patch: lump,
            colormap: None,
            translation: ((thing.flags & MF_TRANSLATION) >> MF_TRANSSHIFT) as u8,
            tranmap: (thing.flags & MF_TRANSLUCENT != 0 && !data.tranmaps.is_empty()).then_some(0),
            heightsec: None,
            tint: self.tint,
            yscale,
            yiscale,
            style: thing.style,
            alpha: u32::from(thing.alpha) * 256 / 255,
            fill: thing.fill,
            special: None,
            voxel: None,
            particle: None,
        };
        if flip {
            vis.startfrac = width - 1;
            vis.xiscale = -iscale;
        }
        if vis.x1 > x1 {
            vis.startfrac = vis
                .startfrac
                .wrapping_add(vis.xiscale.wrapping_mul(vis.x1 - x1));
        }
        vis.colormap = if thing.flags & MF_SHADOW != 0 {
            None
        } else if let Some(f) = self.fixed {
            Some(f)
        } else if thing.frame & FF_FULLBRIGHT != 0 {
            // Fullbright ignores the sector's colour and fog (UZDoom's
            // `r_fullbrightignoresectorcolor`, on by default).
            vis.tint = (0, 0);
            Some(0)
        } else {
            Some(self.scale_light(lightnum, xscale))
        };
        vis.heightsec = heightsec;
        self.vissprites.push(vis);
    }

    /// `R_DrawMasked`: sprites back to front, the masked walls left over,
    /// then the weapon.
    fn draw_masked(
        &mut self,
        level: &SoftLevel,
        data: &SoftData,
        sectors: &[SoftSector],
        frame: &SoftFrame,
        weapon: bool,
        target: &mut SoftTarget,
    ) {
        // R_SortVisSprites: ascending scale, ties in list order.
        let sprites = std::mem::take(&mut self.vissprites);
        let mut order = std::mem::take(&mut self.sprite_order);
        order.clear();
        order.extend(sprites.iter().enumerate().map(|(i, v)| (v.scale, i as u32)));
        order.sort_unstable();
        for &(_, i) in &order {
            self.draw_sprite(level, data, sectors, &sprites[i as usize], target);
        }
        self.sprite_order = order;
        self.vissprites = sprites;
        for i in (0..self.drawsegs.len()).rev() {
            let ds = self.drawsegs[i];
            if ds.masked() {
                self.render_masked_seg_range(level, data, sectors, &ds, ds.x1, ds.x2, target);
            }
        }
        self.draw_slab_planes(level, data, false, target);
        self.x.ztest = false;
        self.x.zwrite = false;
        if weapon {
            self.draw_player_sprites(level, data, sectors, frame, target);
        }
    }

    fn clip_at(&self, clip: Clip, x: i32) -> Option<i16> {
        match clip {
            Clip::None => None,
            Clip::ScreenHeight => Some(self.height as i16),
            Clip::NegOne => Some(-1),
            Clip::Openings(base) => self.openings.get((base + x) as usize).copied(),
        }
    }

    /// `R_DrawSprite`: clip against the walls in front, then draw.
    fn draw_sprite(
        &mut self,
        level: &SoftLevel,
        data: &SoftData,
        sectors: &[SoftSector],
        spr: &VisSprite,
        target: &mut SoftTarget,
    ) {
        for x in spr.x1..=spr.x2 {
            self.clipbot[x as usize] = -2;
            self.cliptop[x as usize] = -2;
        }
        for i in (0..self.drawsegs.len()).rev() {
            let ds = self.drawsegs[i];
            if ds.x1 > spr.x2 || ds.x2 < spr.x1 || (ds.silhouette == 0 && !ds.masked()) {
                continue;
            }
            let r1 = ds.x1.max(spr.x1);
            let r2 = ds.x2.min(spr.x2);
            let (lowscale, scale) = if ds.scale1 > ds.scale2 {
                (ds.scale2, ds.scale1)
            } else {
                (ds.scale1, ds.scale2)
            };
            let seg = seg_at(level, ds.seg);
            let v1 = level.vertices[seg.v1 as usize];
            let v2 = level.vertices[seg.v2 as usize];
            let in_front = || {
                point_on_line(
                    spr.gx,
                    spr.gy,
                    v1.x,
                    v1.y,
                    v2.x.wrapping_sub(v1.x),
                    v2.y.wrapping_sub(v1.y),
                ) == 0
            };
            if scale < spr.scale || (lowscale < spr.scale && in_front()) {
                if ds.masked() {
                    self.render_masked_seg_range(level, data, sectors, &ds, r1, r2, target);
                }
                continue;
            }
            let mut silhouette = ds.silhouette;
            if spr.gz >= ds.bsilheight {
                silhouette &= !SIL_BOTTOM;
            }
            if spr.gzt <= ds.tsilheight {
                silhouette &= !SIL_TOP;
            }
            for x in r1..=r2 {
                let xu = x as usize;
                if silhouette & SIL_BOTTOM != 0 && self.clipbot[xu] == -2 {
                    if let Some(v) = self.clip_at(ds.sprbottomclip, x) {
                        self.clipbot[xu] = v;
                    }
                }
                if silhouette & SIL_TOP != 0 && self.cliptop[xu] == -2 {
                    if let Some(v) = self.clip_at(ds.sprtopclip, x) {
                        self.cliptop[xu] = v;
                    }
                }
            }
        }
        if let Some(hs) = spr.heightsec.and_then(|h| sectors.get(h as usize)) {
            self.clip_deep_water(spr, hs, sectors);
        }
        for x in spr.x1..=spr.x2 {
            let xu = x as usize;
            if self.clipbot[xu] == -2 {
                self.clipbot[xu] = self.height as i16;
            }
            if self.cliptop[xu] == -2 {
                self.cliptop[xu] = -1;
            }
        }
        self.draw_vis_sprite(data, spr, true, target);
    }

    /// Boom: clip a sprite of a 242 sector at the control sector's planes,
    /// from whichever side the viewer is on.
    fn clip_deep_water(&mut self, spr: &VisSprite, hs: &SoftSector, sectors: &[SoftSector]) {
        let phs = self.view_heightsec.and_then(|p| sectors.get(p as usize));
        let (centeryfrac, viewz, height) = (self.centeryfrac, self.viewz, self.height);
        let row = |mh: i32| -> Option<i16> {
            let h = centeryfrac.wrapping_sub(fixed_mul(mh.wrapping_sub(viewz), spr.scale));
            (h >= 0 && (h >> FRACBITS) < height).then_some((h >> FRACBITS) as i16)
        };
        let clip_bottom = |r: &mut Self, h: i16| {
            for x in spr.x1..=spr.x2 {
                let c = &mut r.clipbot[x as usize];
                if *c == -2 || h < *c {
                    *c = h;
                }
            }
        };
        let clip_top = |r: &mut Self, h: i16| {
            for x in spr.x1..=spr.x2 {
                let c = &mut r.cliptop[x as usize];
                if *c == -2 || h > *c {
                    *c = h;
                }
            }
        };
        let mh = hs.floor;
        if mh > spr.gz {
            if let Some(h) = row(mh) {
                if mh <= self.viewz || phs.is_some_and(|p| self.viewz > p.floor) {
                    clip_bottom(self, h);
                } else if phs.is_some_and(|p| self.viewz <= p.floor) {
                    clip_top(self, h);
                }
            }
        }
        let mh = hs.ceiling;
        if mh < spr.gzt {
            if let Some(h) = row(mh) {
                if phs.is_some_and(|p| self.viewz >= p.ceiling) {
                    clip_bottom(self, h);
                } else {
                    clip_top(self, h);
                }
            }
        }
    }

    /// `R_DrawVisSprite` with `mfloorclip` / `mceilingclip` = `clipbot` /
    /// `cliptop` (`clipped`) or the full view (the weapon).
    fn draw_vis_sprite(
        &mut self,
        data: &SoftData,
        vis: &VisSprite,
        clipped: bool,
        target: &mut SoftTarget,
    ) {
        if let Some(q) = vis.special {
            if let Some(&quad) = self.x.quads.get(q as usize) {
                self.draw_quad(data, vis, quad, clipped, target);
            }
            return;
        }
        if let Some(v) = vis.voxel {
            if let Some(&job) = self.x.voxjobs.get(v as usize) {
                self.draw_voxel(data, vis, job, clipped, target);
            }
            return;
        }
        if let Some(p) = vis.particle {
            if let Some(&job) = self.x.partjobs.get(p as usize) {
                self.draw_particle(data, vis, job, clipped, target);
            }
            return;
        }
        // Dynamic light on the sprite (not on fullbright frames).
        self.x.dl.sprite = if clipped && !(vis.tint == (0, 0) && vis.colormap == Some(0)) {
            self.dyn_thing([
                f64::from(vis.gx) / 65536.0,
                f64::from(vis.gy) / 65536.0,
                (f64::from(vis.gz) + f64::from(vis.gzt)) / 131_072.0,
            ])
        } else {
            None
        };
        self.tint = vis.tint;
        let Some(patch) = data.patches.get(vis.patch as usize) else {
            return;
        };
        self.x.zscale = vis.scale;
        self.x.ztest = clipped;
        self.x.zwrite = clipped
            && vis.colormap.is_some()
            && vis.tranmap.is_none()
            && vis.style == SoftStyle::Normal;
        let colfunc = match (vis.colormap, vis.translation, vis.tranmap) {
            (None, _, _) => ColFunc::Fuzz,
            (Some(c), _, _) if vis.style != SoftStyle::Normal => {
                ColFunc::Blend(c, vis.style, vis.alpha, vis.fill)
            }
            (Some(c), 0, Some(t)) => ColFunc::Translucent(c, t),
            (Some(c), 0, None) => ColFunc::Normal(c),
            (Some(c), t, _) => ColFunc::Translated(c, t),
        };
        // Vertical: the sprite's own scale (equals the horizontal one for
        // unscaled sprites, as vanilla).
        let iscale = vis.yiscale;
        let spryscale = vis.yscale;
        let sprtopscreen = self
            .centeryfrac
            .wrapping_sub(fixed_mul(vis.texturemid, spryscale));
        let mut frac = vis.startfrac;
        for x in vis.x1..=vis.x2 {
            let texturecolumn = frac >> FRACBITS;
            frac = frac.wrapping_add(vis.xiscale);
            let Some(posts) = usize::try_from(texturecolumn)
                .ok()
                .and_then(|c| patch.columns.get(c))
            else {
                continue;
            };
            let (fclip, cclip) = if clipped {
                (
                    i32::from(self.clipbot[x as usize]),
                    i32::from(self.cliptop[x as usize]),
                )
            } else {
                (self.height, -1)
            };
            for post in posts {
                self.masked_post(
                    &post.pixels,
                    if self.truecolor {
                        post.rgb.as_deref()
                    } else {
                        None
                    },
                    post.top,
                    x,
                    sprtopscreen,
                    spryscale,
                    vis.texturemid,
                    iscale,
                    fclip,
                    cclip,
                    colfunc,
                    data,
                    target,
                );
            }
        }
    }

    /// One post of `R_DrawMaskedColumn`.
    #[allow(clippy::too_many_arguments)]
    fn masked_post(
        &mut self,
        pixels: &[u8],
        rgb: Option<&[u32]>,
        topdelta: i32,
        x: i32,
        sprtopscreen: i32,
        spryscale: i32,
        basetexturemid: i32,
        iscale: i32,
        fclip: i32,
        cclip: i32,
        colfunc: ColFunc,
        data: &SoftData,
        target: &mut SoftTarget,
    ) {
        let topscreen = sprtopscreen.wrapping_add(spryscale.wrapping_mul(topdelta));
        let bottomscreen = topscreen.wrapping_add(spryscale.wrapping_mul(pixels.len() as i32));
        let mut yl = (topscreen.wrapping_add(FRACUNIT - 1)) >> FRACBITS;
        let mut yh = (bottomscreen.wrapping_sub(1)) >> FRACBITS;
        if yh >= fclip {
            yh = fclip - 1;
        }
        if yl <= cclip {
            yl = cclip + 1;
        }
        if yl > yh || pixels.is_empty() {
            return;
        }
        let texturemid = basetexturemid.wrapping_sub(topdelta << FRACBITS);
        let w = self.width as usize;
        // Off-screen rows (vanilla trusts its clip arrays; ours may not).
        let yl = yl.max(0);
        let yh = yh.min(self.height - 1);
        if yl > yh || x < 0 || x >= self.width {
            return;
        }
        if self.x.zon && self.x.ztest {
            // Depth buffer (3D floors, slopes): draw the visible runs.
            let scale = self.x.zscale;
            let mut y = yl;
            while y <= yh {
                while y <= yh && self.z_hidden(y as usize * w + x as usize, scale) {
                    y += 1;
                }
                let start = y;
                while y <= yh && !self.z_hidden(y as usize * w + x as usize, scale) {
                    y += 1;
                }
                if start < y {
                    self.masked_run(
                        pixels,
                        rgb,
                        x,
                        start,
                        y - 1,
                        texturemid,
                        iscale,
                        colfunc,
                        data,
                        target,
                    );
                    if self.x.zwrite {
                        for r in start..y {
                            let i = r as usize * w + x as usize;
                            self.x.zbuf[i] = self.x.zbuf[i].max(scale);
                        }
                    }
                }
            }
            return;
        }
        self.masked_run(
            pixels, rgb, x, yl, yh, texturemid, iscale, colfunc, data, target,
        );
    }

    /// Rows `yl..=yh` of a masked post (`R_DrawColumn` and friends).
    #[allow(clippy::too_many_arguments)]
    fn masked_run(
        &mut self,
        pixels: &[u8],
        rgb: Option<&[u32]>,
        x: i32,
        yl: i32,
        yh: i32,
        texturemid: i32,
        iscale: i32,
        colfunc: ColFunc,
        data: &SoftData,
        target: &mut SoftTarget,
    ) {
        let w = self.width as usize;
        // Vanilla reads the post's trailing byte (a copy of the last pixel)
        // or beyond; repeat the last pixel instead.
        let texel = |t: usize| pixels.get(t).copied().unwrap_or(pixels[pixels.len() - 1]);
        let color = |t: usize| {
            rgb.filter(|r| !r.is_empty())
                .map(|r| r.get(t).copied().unwrap_or(r[r.len() - 1]))
        };
        let mut frac = texturemid.wrapping_add((yl - self.centery).wrapping_mul(iscale));
        // Boom draws masked columns unmasked (`dc_texheight` 0 / the post).
        let mask = if self.boom { -1 } else { 127 };
        match colfunc {
            ColFunc::Normal(c) => {
                let px = self.px(data, c);
                let dl = self.x.dl.sprite.filter(|_| self.truecolor);
                for y in yl..=yh {
                    let t = ((frac >> FRACBITS) & mask).max(0) as usize;
                    let i = y as usize * w + x as usize;
                    px.put_rgb(target, i, texel(t), color(t));
                    if let Some(l) = dl {
                        Self::dyn_add(target, i, Self::texel_rgb(data, texel(t), color(t)), l);
                    }
                    frac = frac.wrapping_add(iscale);
                }
            }
            ColFunc::Translucent(c, table) => match self.px(data, c) {
                Px::Index(cmap) => {
                    let tran = &data.tranmaps[usize::from(table)];
                    for y in yl..=yh {
                        let t = ((frac >> FRACBITS) & mask).max(0) as usize;
                        let i = y as usize * w + x as usize;
                        let src = cmap[usize::from(texel(t))];
                        target.pixels[i] =
                            tran[(usize::from(target.pixels[i]) << 8) + usize::from(src)];
                        frac = frac.wrapping_add(iscale);
                    }
                }
                Px::True(shade) => {
                    // Boom's table is 66 % source over 34 % background.
                    for y in yl..=yh {
                        let t = ((frac >> FRACBITS) & mask).max(0) as usize;
                        let i = y as usize * w + x as usize;
                        let s = unpack(match color(t) {
                            Some(c) if shade.cmap.is_none() => shade.shade(unpack(c)),
                            _ => shade.rgb(texel(t)),
                        });
                        let d = unpack(target.rgb[i]);
                        let mix = |a: u32, b: u32| (a * 169 + b * 87) >> 8;
                        target.rgb[i] = pack(mix(s[0], d[0]), mix(s[1], d[1]), mix(s[2], d[2]));
                        frac = frac.wrapping_add(iscale);
                    }
                }
            },
            ColFunc::Translated(c, t) => {
                let px = self.px(data, c);
                let tr = &self.translations[usize::from(t - 1).min(2)];
                for y in yl..=yh {
                    // R_DrawTranslatedColumn has no `& 127`.
                    let i = (frac >> FRACBITS).max(0) as usize;
                    px.put(
                        target,
                        y as usize * w + x as usize,
                        tr[usize::from(texel(i))],
                    );
                    frac = frac.wrapping_add(iscale);
                }
            }
            ColFunc::Blend(c, style, alpha, fill) => {
                let px = self.px(data, c);
                let pal_rgb = |i: u8| {
                    let [r, g, b] = data.palette[usize::from(i)];
                    [u32::from(r), u32::from(g), u32::from(b)]
                };
                // The fill colour as this light shows it.
                let lit_fill = match px {
                    Px::True(sh) => unpack(sh.shade(unpack(fill))),
                    Px::Index(_) => {
                        let f = if c >= NUMCOLORMAPS {
                            256
                        } else {
                            ((NUMCOLORMAPS - c) * 256 / NUMCOLORMAPS) as u32
                        };
                        unpack(fill).map(|v| (v * f) >> 8)
                    }
                };
                for y in yl..=yh {
                    let t = ((frac >> FRACBITS) & mask).max(0) as usize;
                    let i = y as usize * w + x as usize;
                    let tx = texel(t);
                    let src = match style {
                        SoftStyle::Stencil
                        | SoftStyle::AddStencil
                        | SoftStyle::Shaded
                        | SoftStyle::AddShaded => lit_fill,
                        _ => match px {
                            Px::True(sh) => unpack(match color(t) {
                                Some(cc) if sh.cmap.is_none() => sh.shade(unpack(cc)),
                                _ => sh.rgb(tx),
                            }),
                            Px::Index(cm) => pal_rgb(cm[usize::from(tx)]),
                        },
                    };
                    let a = match style {
                        SoftStyle::Shaded | SoftStyle::AddShaded => alpha * u32::from(tx) / 255,
                        _ => alpha,
                    };
                    let dst = if self.truecolor {
                        unpack(target.rgb[i])
                    } else {
                        pal_rgb(target.pixels[i])
                    };
                    let out = blend(style, src, dst, a.min(256));
                    if self.truecolor {
                        target.rgb[i] = pack(out[0], out[1], out[2]);
                    } else {
                        target.pixels[i] = data.nearest_rgb(out);
                    }
                    frac = frac.wrapping_add(iscale);
                }
            }
            ColFunc::Fuzz => self.fuzz_column(data, x, yl, yh, target),
        }
    }

    /// `R_DrawFuzzColumn`: darken with the pixel above or below.
    fn fuzz_column(&mut self, data: &SoftData, x: i32, yl: i32, yh: i32, target: &mut SoftTarget) {
        let yl = if yl == 0 { 1 } else { yl };
        let yh = if yh == self.height - 1 {
            self.height - 2
        } else {
            yh
        };
        if yh < yl {
            return;
        }
        let w = self.width as usize;
        let cmap = self.cmap(data, 6);
        for y in yl..=yh {
            let src = (y + i32::from(FUZZOFFSET[self.fuzzpos])).clamp(0, self.height - 1);
            let (from, to) = (src as usize * w + x as usize, y as usize * w + x as usize);
            if self.truecolor {
                // Colormap 6 darkens to 26/32.
                let [r, g, b] = unpack(target.rgb[from]);
                target.rgb[to] = pack(r * 26 / 32, g * 26 / 32, b * 26 / 32);
            } else {
                let under = target.pixels[from];
                target.pixels[to] = cmap[usize::from(under)];
            }
            self.fuzzpos = (self.fuzzpos + 1) % FUZZOFFSET.len();
        }
    }

    /// `R_RenderMaskedSegRange`: a two-sided line's middle texture.
    #[allow(clippy::too_many_arguments)]
    fn render_masked_seg_range(
        &mut self,
        level: &SoftLevel,
        data: &SoftData,
        sectors: &[SoftSector],
        ds: &DrawSeg,
        x1: i32,
        x2: i32,
        target: &mut SoftTarget,
    ) {
        if ds.slabcol != Clip::None {
            self.x.ztest = true;
            self.draw_slab_sides(level, data, ds, x1, x2, target);
        }
        let Clip::Openings(colbase) = ds.maskedtexturecol else {
            return;
        };
        let seg = seg_at(level, ds.seg);
        let side = level.sides[seg.side as usize];
        if side.mid == 0 {
            return;
        }
        let texnum = data.anim_texture(side.mid, self.leveltime);
        let Some(tex) = data.textures.get(texnum as usize) else {
            return;
        };
        let tranmap = level
            .line_tranmap
            .get(seg.line as usize)
            .copied()
            .flatten()
            .filter(|t| usize::from(*t) < data.tranmaps.len());
        let front = sectors.get(seg.front as usize).copied().unwrap_or_default();
        let back = seg
            .back
            .and_then(|b| sectors.get(b as usize).copied())
            .unwrap_or(front);
        let v1 = level.vertices[seg.v1 as usize];
        let v2 = level.vertices[seg.v2 as usize];
        // Boom lights the texture as its front sector is drawn (`R_FakeFlat`).
        let light = self
            .fake_flat(level, sectors, seg.front as usize, false)
            .0
            .light;
        let walllights = self.light_row(light, v1, v2);
        self.tint = tint_of(&front);
        let mut spryscale = ds
            .scale1
            .wrapping_add((x1 - ds.x1).wrapping_mul(ds.scalestep));
        let flags = level.line_flags[seg.line as usize];
        let mut texturemid = if flags & ML_DONTPEGBOTTOM != 0 {
            front
                .floor
                .max(back.floor)
                .wrapping_add((tex.height as i32) << FRACBITS)
                .wrapping_sub(self.viewz)
        } else {
            front.ceiling.min(back.ceiling).wrapping_sub(self.viewz)
        };
        texturemid = texturemid.wrapping_add(side.row_offset);
        let h = tex.height as usize;
        for x in x1..=x2 {
            let slot = (colbase + x) as usize;
            let col = self.openings.get(slot).copied().unwrap_or(MASKED_DONE);
            if col != MASKED_DONE {
                self.x.zscale = spryscale;
                self.x.ztest = true;
                self.x.zwrite = tranmap.is_none();
                let colormap = self
                    .fixed
                    .unwrap_or_else(|| self.scale_light(walllights, spryscale));
                let sprtopscreen = self
                    .centeryfrac
                    .wrapping_sub(fixed_mul(texturemid, spryscale));
                let iscale = (0xffff_ffffu32 / (spryscale as u32).max(1)) as i32;
                let fclip = self
                    .clip_at(ds.sprbottomclip, x)
                    .map_or(self.height, i32::from);
                let cclip = self.clip_at(ds.sprtopclip, x).map_or(-1, i32::from);
                let pixels = tex.column(i32::from(col));
                let rgb_col = if self.truecolor {
                    tex.rgb_column(i32::from(col))
                } else {
                    None
                };
                let alpha = tex.column_alpha(i32::from(col));
                let n = h.min(pixels.len()).min(alpha.len());
                // The column's opaque runs are its posts.
                let mut y = 0;
                while y < n {
                    if alpha[y] == 0 {
                        y += 1;
                        continue;
                    }
                    let start = y;
                    while y < n && alpha[y] != 0 {
                        y += 1;
                    }
                    self.masked_post(
                        &pixels[start..y],
                        rgb_col.map(|r| &r[start.min(r.len())..y.min(r.len())]),
                        start as i32,
                        x,
                        sprtopscreen,
                        spryscale,
                        texturemid,
                        iscale,
                        fclip,
                        cclip,
                        match tranmap {
                            Some(t) => ColFunc::Translucent(colormap, t),
                            None => ColFunc::Normal(colormap),
                        },
                        data,
                        target,
                    );
                }
                self.openings[slot] = MASKED_DONE;
            }
            spryscale = spryscale.wrapping_add(ds.scalestep);
        }
    }

    /// `R_DrawPlayerSprites`.
    fn draw_player_sprites(
        &mut self,
        level: &SoftLevel,
        data: &SoftData,
        sectors: &[SoftSector],
        frame: &SoftFrame,
        target: &mut SoftTarget,
    ) {
        if frame.psprites.is_empty() {
            return;
        }
        let view_sector = level
            .point_in_sector(self.viewx, self.viewy)
            .filter(|&s| s < sectors.len());
        let lightnum = match view_sector {
            // MBF: the mean of the floor and ceiling light as drawn.
            Some(s) if self.boom => {
                let (_, fl, cl) = self.fake_flat(level, sectors, s, false);
                ((fl + cl) >> (LIGHTSEGSHIFT + 1)) + self.extralight
            }
            Some(s) => (sectors[s].light >> LIGHTSEGSHIFT) + self.extralight,
            None => (255 >> LIGHTSEGSHIFT) + self.extralight,
        }
        .clamp(0, LIGHTLEVELS as i32 - 1) as usize;
        // The weapon ignores free look (ZDoom); light from the view sector.
        let (cy, cyf) = (self.centery, self.centeryfrac);
        self.centery = self.height / 2;
        self.centeryfrac = self.centery << FRACBITS;
        self.tint = view_sector
            .and_then(|s| sectors.get(s))
            .map_or((0, 0), tint_of);
        for psp in &frame.psprites {
            self.draw_psprite(data, psp, lightnum, frame.psprite_fuzz, target);
        }
        self.centery = cy;
        self.centeryfrac = cyf;
    }

    /// `R_DrawPSprite`.
    fn draw_psprite(
        &mut self,
        data: &SoftData,
        psp: &SoftPSprite,
        lightnum: usize,
        fuzz: bool,
        target: &mut SoftTarget,
    ) {
        let Some(sprframe) = data
            .sprite_frames(sprite_str(&psp.sprite))
            .and_then(|f| f.get((psp.frame & FF_FRAMEMASK) as usize))
        else {
            return;
        };
        let (Some(lump), flip) = (sprframe.lump[0], sprframe.flip[0]) else {
            return;
        };
        let patch = &data.patches[lump as usize];
        let width = patch.width << FRACBITS;
        let mut tx = psp
            .sx
            .wrapping_sub((ORIGWIDTH / 2) * FRACUNIT)
            .wrapping_sub(patch.left << FRACBITS);
        let x1 = (self
            .centerxfrac
            .wrapping_add(fixed_mul(tx, self.pspritescale)))
            >> FRACBITS;
        if x1 > self.width {
            return;
        }
        tx = tx.wrapping_add(width);
        let x2 = ((self
            .centerxfrac
            .wrapping_add(fixed_mul(tx, self.pspritescale)))
            >> FRACBITS)
            - 1;
        if x2 < 0 {
            return;
        }
        let mut vis = VisSprite {
            x1: x1.max(0),
            x2: x2.min(self.width - 1),
            gx: 0,
            gy: 0,
            gz: 0,
            gzt: 0,
            startfrac: 0,
            scale: self.pspritescale,
            xiscale: self.pspriteiscale,
            texturemid: (BASEYCENTER << FRACBITS) + FRACUNIT / 2
                - psp.sy.wrapping_sub(patch.top << FRACBITS),
            patch: lump,
            colormap: None,
            translation: 0,
            tranmap: None,
            heightsec: None,
            tint: self.tint,
            yscale: self.pspritescale,
            yiscale: self.pspriteiscale,
            style: SoftStyle::Normal,
            alpha: 256,
            fill: 0,
            special: None,
            voxel: None,
            particle: None,
        };
        if flip {
            vis.xiscale = -self.pspriteiscale;
            vis.startfrac = width - 1;
        }
        if vis.x1 > x1 {
            vis.startfrac = vis
                .startfrac
                .wrapping_add(vis.xiscale.wrapping_mul(vis.x1 - x1));
        }
        vis.colormap = if fuzz {
            None
        } else if let Some(f) = self.fixed {
            Some(f)
        } else if psp.frame & FF_FULLBRIGHT != 0 {
            // Fullbright ignores the sector's colour and fog (UZDoom's
            // `r_fullbrightignoresectorcolor`, on by default).
            vis.tint = (0, 0);
            Some(0)
        } else {
            Some(self.scale_light(lightnum, i32::MAX))
        };
        self.draw_vis_sprite(data, &vis, false, target);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fit_keeps_vanilla_shape_with_bars() {
        let mut t = SoftTarget::new(320, 200);
        t.pixels.fill(1);
        let mut pal = [[0u8; 3]; 256];
        pal[1] = [255, 0, 0];
        // 1920×1080: a 1440×1080 (4:3) box, 240-pixel bars each side.
        let out = t.to_rgba_fit(&pal, 1920, 1080);
        let px = |x: usize, y: usize| &out[(y * 1920 + x) * 4..][..4];
        assert_eq!(px(239, 540), [0, 0, 0, 255]);
        assert_eq!(px(240, 540), [255, 0, 0, 255]);
        assert_eq!(px(1679, 0), [255, 0, 0, 255]);
        assert_eq!(px(1680, 1079), [0, 0, 0, 255]);
    }

    #[test]
    fn view_tables_match_vanilla_320x200() {
        let r = SoftRenderer::new(320, 200);
        // Vanilla's clipangle (`xtoviewangle[0]`) is 537395200, just over 45°.
        assert_eq!(r.clipangle, 0x2008_0000);
        assert_eq!(r.xtoviewangle[160], 0, "centre column looks straight ahead");
        assert_eq!(r.viewangletox[FINEANGLES / 4], 160);
        // Brightest light, nearest: colormap 0; darkest, far: 31.
        assert_eq!(r.scalelight[15][MAXLIGHTSCALE - 1], 0);
        assert_eq!(r.zlight[0][MAXLIGHTZ - 1], 31);
    }
}
