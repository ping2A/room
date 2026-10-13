//! Soft bitmap fonts (`FON1` / `FON2` → draw-list text).

use crate::draw::{DrawCmd, Rgba, UiDrawList, Vec2};
use ferrite_formats::{Fon1, Fon2, FontGlyph, FormatError, parse_fon1, parse_fon2, parse_patch};

/// Logical font handle used by menus / HUD (resolved by name in the shell).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct FontId(pub String);

impl FontId {
    /// Built-in Doom small font.
    pub const SMALL: &'static str = "SmallFont";
    /// Built-in Doom big font (`DBIGFONT`).
    pub const BIG: &'static str = "BigFont";
    /// Status-bar big digits (`STTNUM*`).
    pub const HUD_BIG: &'static str = "HudBig";
    /// Console font (`FON1`).
    pub const CONSOLE: &'static str = "ConFont";
}

/// Soft (CPU) font: metrics + glyph lookup for UI layout.
#[derive(Clone, Debug)]
pub enum SoftFont {
    /// `FON1` (fixed cell).
    Fon1(Fon1),
    /// `FON2` (variable width + palette).
    Fon2(Fon2),
    /// Doom `STCFNxxx` / sprite sheet (256 slots; missing = `None`).
    Patches {
        /// Shared line height (max glyph height).
        height: u16,
        /// Glyphs indexed by character code.
        glyphs: Box<[Option<FontGlyph>; 256]>,
    },
}

impl SoftFont {
    /// Parse a font lump (auto-detect `FON1` / `FON2`).
    pub fn from_lump(data: &[u8]) -> Result<Self, FormatError> {
        if data.len() >= 4 && &data[0..4] == b"FON2" {
            return Ok(Self::Fon2(parse_fon2(data)?));
        }
        if data.len() >= 4 && &data[0..4] == b"FON1" {
            return Ok(Self::Fon1(parse_fon1(data)?));
        }
        Err(FormatError::Invalid("not FON1/FON2"))
    }

    /// Build Doom `SmallFont` from `STCFN###` patch lumps.
    ///
    /// `lumps` yields `(name, bytes)` pairs; names like `STCFN065` map to ASCII 65.
    pub fn from_stcfn_lumps<'a, I>(lumps: I) -> Result<Self, FormatError>
    where
        I: IntoIterator<Item = (&'a str, &'a [u8])>,
    {
        let mut glyphs: Box<[Option<FontGlyph>; 256]> = Box::new(std::array::from_fn(|_| None));
        let mut height = 0u16;
        let mut count = 0usize;
        for (name, data) in lumps {
            let Some(code) = stcfn_code(name) else {
                continue;
            };
            let patch = parse_patch(data)?;
            height = height.max(patch.height);
            glyphs[usize::from(code)] = Some(FontGlyph {
                width: patch.width,
                height: patch.height,
                indices: patch.to_indices(),
            });
            count += 1;
        }
        if count == 0 {
            return Err(FormatError::Invalid("no STCFN glyphs"));
        }
        if height == 0 {
            height = 8;
        }
        Ok(Self::Patches { height, glyphs })
    }

    /// Build status-bar digit font from `STTNUM0`–`STTNUM9` (+ optional `%` / `-`).
    pub fn from_sttnum_lumps<'a, I>(lumps: I) -> Result<Self, FormatError>
    where
        I: IntoIterator<Item = (&'a str, &'a [u8])>,
    {
        let mut glyphs: Box<[Option<FontGlyph>; 256]> = Box::new(std::array::from_fn(|_| None));
        let mut height = 0u16;
        let mut count = 0usize;
        for (name, data) in lumps {
            let Some(code) = sttnum_code(name) else {
                continue;
            };
            let patch = parse_patch(data)?;
            height = height.max(patch.height);
            glyphs[usize::from(code)] = Some(FontGlyph {
                width: patch.width,
                height: patch.height,
                indices: patch.to_indices(),
            });
            count += 1;
        }
        if count == 0 {
            return Err(FormatError::Invalid("no STTNUM glyphs"));
        }
        if height == 0 {
            height = 16;
        }
        Ok(Self::Patches { height, glyphs })
    }

    /// Line height in pixels.
    #[must_use]
    pub fn height(&self) -> u16 {
        match self {
            Self::Fon1(f) => f.char_height,
            Self::Fon2(f) => f.font_height,
            Self::Patches { height, .. } => *height,
        }
    }

    /// Advance width for one byte (missing glyph → space width or 0).
    #[must_use]
    pub fn char_width(&self, ch: u8) -> u16 {
        match self {
            Self::Fon1(f) => f.char_width,
            Self::Fon2(f) => {
                if ch < f.first_char || ch > f.last_char {
                    return space_width_fon2(f);
                }
                let idx = (ch - f.first_char) as usize;
                f.glyphs
                    .get(idx)
                    .and_then(|g| g.as_ref())
                    .map(|g| g.width)
                    .unwrap_or_else(|| space_width_fon2(f))
            }
            Self::Patches { height, glyphs } => {
                if let Some(g) = patch_glyph(glyphs, ch) {
                    g.width
                } else if ch == b' ' {
                    (height / 3).max(4)
                } else {
                    0
                }
            }
        }
    }

    /// Pixel width of a string (ASCII; UTF-8 bytes above 127 use glyph if present).
    #[must_use]
    pub fn measure(&self, text: &str) -> f32 {
        let kern = match self {
            Self::Fon1(_) | Self::Patches { .. } => 0i16,
            Self::Fon2(f) => f.kerning,
        };
        let mut w = 0i32;
        let mut first = true;
        for b in text.bytes() {
            if b == b'\n' {
                continue;
            }
            if !first {
                w += i32::from(kern);
            }
            first = false;
            w += i32::from(self.char_width(b));
        }
        w.max(0) as f32
    }

    /// Emit a [`DrawCmd::Text`] using `logical_name` for renderer glyph lookup.
    pub fn draw(
        &self,
        list: &mut UiDrawList,
        logical_name: &str,
        pos: Vec2,
        text: impl Into<String>,
        color: Rgba,
        scale: f32,
    ) {
        let _ = self; // metrics available to callers via `measure` / `height`
        list.cmds.push(DrawCmd::Text {
            font: logical_name.to_owned(),
            pos,
            text: text.into(),
            color,
            scale,
        });
    }

    /// Glyph for character code, if defined.
    #[must_use]
    pub fn glyph(&self, ch: u8) -> Option<&FontGlyph> {
        match self {
            Self::Fon1(f) => f.glyphs.get(usize::from(ch)),
            Self::Fon2(f) => {
                if ch < f.first_char || ch > f.last_char {
                    return None;
                }
                f.glyphs
                    .get((ch - f.first_char) as usize)
                    .and_then(|g| g.as_ref())
            }
            Self::Patches { glyphs, .. } => patch_glyph(glyphs, ch),
        }
    }

    /// Paint opaque glyph pixels as axis-aligned rects into `emit(x, y, w, h, rgba)`.
    ///
    /// Coordinates are in font pixels (caller scales). `color` tints FON1/STCFN;
    /// FON2 uses its embedded palette modulated by `color`.
    pub fn paint_text(
        &self,
        text: &str,
        origin_x: f32,
        origin_y: f32,
        scale: f32,
        color: Rgba,
        mut emit: impl FnMut(f32, f32, f32, f32, [f32; 4]),
    ) {
        let kern = match self {
            Self::Fon1(_) | Self::Patches { .. } => 0i16,
            Self::Fon2(f) => f.kerning,
        };
        let mut x = origin_x;
        let mut first = true;
        for b in text.bytes() {
            if b == b'\n' {
                continue;
            }
            if !first {
                x += f32::from(kern) * scale;
            }
            first = false;
            let advance = f32::from(self.char_width(b)) * scale;
            if let Some(g) = self.glyph(b) {
                self.paint_glyph(g, x, origin_y, scale, color, &mut emit);
            }
            x += advance;
        }
    }

    fn paint_glyph(
        &self,
        g: &FontGlyph,
        x0: f32,
        y0: f32,
        scale: f32,
        color: Rgba,
        emit: &mut impl FnMut(f32, f32, f32, f32, [f32; 4]),
    ) {
        let w = usize::from(g.width);
        let h = usize::from(g.height);
        if w == 0 || h == 0 || g.indices.len() < w * h {
            return;
        }
        for row in 0..h {
            let mut col = 0usize;
            while col < w {
                let idx = g.indices[row * w + col];
                if idx == 0 {
                    col += 1;
                    continue;
                }
                let start = col;
                col += 1;
                while col < w && g.indices[row * w + col] == idx {
                    col += 1;
                }
                let rgba = self.index_color(idx, color);
                if rgba[3] <= 0.0 {
                    continue;
                }
                let px = x0 + start as f32 * scale;
                let py = y0 + row as f32 * scale;
                let pw = (col - start) as f32 * scale;
                emit(px, py, pw, scale.max(1.0), rgba);
            }
        }
    }

    fn index_color(&self, index: u8, tint: Rgba) -> [f32; 4] {
        match self {
            Self::Fon2(f) => {
                let i = usize::from(index);
                if i == 0 || i >= f.palette.len() {
                    return [0.0, 0.0, 0.0, 0.0];
                }
                let [r, g, b] = f.palette[i];
                [
                    (f32::from(r) / 255.0) * tint.r,
                    (f32::from(g) / 255.0) * tint.g,
                    (f32::from(b) / 255.0) * tint.b,
                    tint.a,
                ]
            }
            Self::Fon1(_) | Self::Patches { .. } => [tint.r, tint.g, tint.b, tint.a],
        }
    }
}

fn space_width_fon2(f: &Fon2) -> u16 {
    if f.constant_width {
        return f
            .glyphs
            .iter()
            .find_map(|g| g.as_ref().map(|g| g.width))
            .unwrap_or(4);
    }
    // Prefer the space glyph when present.
    if b' ' >= f.first_char && b' ' <= f.last_char {
        let idx = (b' ' - f.first_char) as usize;
        if let Some(Some(g)) = f.glyphs.get(idx) {
            return g.width.max(1);
        }
    }
    (f.font_height / 3).max(4)
}

/// STCFN / patch fonts: Doom ships uppercase (+ sparse lowercase). Map `a`→`A`
/// when the exact code is missing so menu labels like "Resume" still paint.
fn patch_glyph(glyphs: &[Option<FontGlyph>; 256], ch: u8) -> Option<&FontGlyph> {
    if let Some(g) = glyphs[usize::from(ch)].as_ref() {
        return Some(g);
    }
    if ch.is_ascii_lowercase() {
        return glyphs[usize::from(ch.to_ascii_uppercase())].as_ref();
    }
    None
}

/// Parse `STTNUM0` → `'0'`, `STTPRCNT` → `'%'`, `STTMINUS` → `'-'`.
fn sttnum_code(name: &str) -> Option<u8> {
    let upper = name.to_ascii_uppercase();
    if upper == "STTPRCNT" {
        return Some(b'%');
    }
    if upper == "STTMINUS" {
        return Some(b'-');
    }
    let rest = upper.strip_prefix("STTNUM")?;
    let d = rest.parse::<u8>().ok()?;
    if d <= 9 { Some(b'0' + d) } else { None }
}

/// Parse `STCFN065` / `stcfn065` → character code.
fn stcfn_code(name: &str) -> Option<u8> {
    let upper = name.to_ascii_uppercase();
    let rest = upper.strip_prefix("STCFN")?;
    let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
    if digits.is_empty() {
        return None;
    }
    let n: u16 = digits.parse().ok()?;
    u8::try_from(n).ok()
}

/// Registry of named soft fonts (SmallFont / BigFont / …).
#[derive(Clone, Debug, Default)]
pub struct FontRegistry {
    fonts: Vec<(String, SoftFont)>,
}

impl FontRegistry {
    /// Empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Load Doom `SmallFont` (`STCFN*`), `BigFont` (`DBIGFONT`), and `HudBig`
    /// (`STTNUM*` status-bar digits) from a VFS.
    pub fn load_doom_fonts(vfs: &ferrite_vfs::Vfs) -> Self {
        let mut reg = Self::new();
        let mut stcfn: Vec<(String, Vec<u8>)> = Vec::new();
        let mut sttnum: Vec<(String, Vec<u8>)> = Vec::new();
        for (_id, info) in vfs.iter() {
            let name = info.short_name.as_str();
            if name.len() >= 5 && name.as_bytes()[..5].eq_ignore_ascii_case(b"STCFN") {
                if let Ok(bytes) = vfs.read(_id) {
                    stcfn.push((name.to_owned(), bytes.to_vec()));
                }
            }
            let u = name.to_ascii_uppercase();
            if u.starts_with("STTNUM") || u == "STTPRCNT" || u == "STTMINUS" {
                if let Ok(bytes) = vfs.read(_id) {
                    sttnum.push((name.to_owned(), bytes.to_vec()));
                }
            }
        }
        if let Ok(font) =
            SoftFont::from_stcfn_lumps(stcfn.iter().map(|(n, b)| (n.as_str(), b.as_slice())))
        {
            reg.insert(FontId::SMALL, font);
        }
        if let Ok(font) =
            SoftFont::from_sttnum_lumps(sttnum.iter().map(|(n, b)| (n.as_str(), b.as_slice())))
        {
            reg.insert(FontId::HUD_BIG, font);
        }
        if let Some(id) = vfs.find_any_ns("DBIGFONT") {
            if let Ok(bytes) = vfs.read(id) {
                if let Ok(font) = SoftFont::from_lump(&bytes) {
                    reg.insert(FontId::BIG, font);
                }
            }
        }
        // Prefer BigFont for menus when present; keep SmallFont as fallback.
        reg
    }

    /// Insert or replace a font by logical name.
    pub fn insert(&mut self, name: impl Into<String>, font: SoftFont) {
        let name = name.into();
        if let Some((_, slot)) = self.fonts.iter_mut().find(|(n, _)| n == &name) {
            *slot = font;
        } else {
            self.fonts.push((name, font));
        }
    }

    /// Lookup by logical name.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&SoftFont> {
        self.fonts
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, f)| f)
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use std::path::PathBuf;

    fn freedoom2() -> Option<PathBuf> {
        let p =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/freedoom/freedoom2.wad");
        p.is_file().then_some(p)
    }

    fn lump_from_wad(path: &std::path::Path, name: &str) -> Option<Vec<u8>> {
        let data = std::fs::read(path).ok()?;
        if data.len() < 12 || &data[0..4] != b"IWAD" && &data[0..4] != b"PWAD" {
            return None;
        }
        let n = u32::from_le_bytes(data[4..8].try_into().ok()?) as usize;
        let off = u32::from_le_bytes(data[8..12].try_into().ok()?) as usize;
        for i in 0..n {
            let base = off + i * 16;
            if base + 16 > data.len() {
                break;
            }
            let lo = u32::from_le_bytes(data[base..base + 4].try_into().ok()?) as usize;
            let ls = u32::from_le_bytes(data[base + 4..base + 8].try_into().ok()?) as usize;
            let raw = &data[base + 8..base + 16];
            let end = raw.iter().position(|&b| b == 0).unwrap_or(8);
            let nm = std::str::from_utf8(&raw[..end]).ok()?;
            if nm.eq_ignore_ascii_case(name) {
                return Some(data.get(lo..lo + ls)?.to_vec());
            }
        }
        None
    }

    #[test]
    fn softfont_loads_freedoom_dbigfont() {
        let Some(wad) = freedoom2() else {
            return;
        };
        let lump = lump_from_wad(&wad, "DBIGFONT").unwrap();
        let font = SoftFont::from_lump(&lump).unwrap();
        assert!(font.height() > 0);
        let w = font.measure("MAP01");
        assert!(w > 10.0, "expected non-trivial width, got {w}");
        let mut reg = FontRegistry::new();
        reg.insert(FontId::BIG, font);
        assert!(reg.get("BigFont").is_some());
        let mut list = UiDrawList::new();
        reg.get("BigFont").unwrap().draw(
            &mut list,
            FontId::BIG,
            Vec2 { x: 0.0, y: 0.0 },
            "HI",
            Rgba {
                r: 1.0,
                g: 1.0,
                b: 1.0,
                a: 1.0,
            },
            1.0,
        );
        assert_eq!(list.cmds.len(), 1);
    }

    #[test]
    fn softfont_from_freedoom_stcfn_smallfont() {
        let Some(wad) = freedoom2() else {
            return;
        };
        let data = std::fs::read(&wad).unwrap();
        assert_eq!(&data[0..4], b"IWAD");
        let n = u32::from_le_bytes(data[4..8].try_into().unwrap()) as usize;
        let off = u32::from_le_bytes(data[8..12].try_into().unwrap()) as usize;
        let mut owned: Vec<(String, Vec<u8>)> = Vec::new();
        for i in 0..n {
            let base = off + i * 16;
            let lo = u32::from_le_bytes(data[base..base + 4].try_into().unwrap()) as usize;
            let ls = u32::from_le_bytes(data[base + 4..base + 8].try_into().unwrap()) as usize;
            let raw = &data[base + 8..base + 16];
            let end = raw.iter().position(|&b| b == 0).unwrap_or(8);
            let nm = std::str::from_utf8(&raw[..end]).unwrap_or("");
            if nm.to_ascii_uppercase().starts_with("STCFN") {
                owned.push((nm.to_owned(), data[lo..lo + ls].to_vec()));
            }
        }
        assert!(!owned.is_empty(), "expected STCFN lumps");
        let font =
            SoftFont::from_stcfn_lumps(owned.iter().map(|(n, b)| (n.as_str(), b.as_slice())))
                .unwrap();
        assert!(font.height() > 0);
        assert!(font.char_width(b'A') > 0);
        let w = font.measure("MAP01");
        assert!(w > 10.0, "SmallFont width {w}");
        let mut reg = FontRegistry::new();
        reg.insert(FontId::SMALL, font);
        assert!(reg.get("SmallFont").is_some());
    }

    #[test]
    fn stcfn_lowercase_falls_back_to_uppercase() {
        let Some(wad) = freedoom2() else {
            return;
        };
        let vfs = ferrite_vfs::VfsBuilder::new()
            .mount_path(&wad)
            .unwrap()
            .build()
            .unwrap();
        let reg = FontRegistry::load_doom_fonts(&vfs);
        let font = reg.get(FontId::SMALL).unwrap();
        // Freedoom STCFN lacks most lowercase; menus use "Resume" etc.
        assert!(
            font.char_width(b'e') > 0,
            "lowercase must map to STCFN uppercase"
        );
        assert!(
            font.measure("Resume") > 10.0,
            "pause labels must be measurable"
        );
        let mut fills = 0u32;
        font.paint_text(
            "Resume",
            0.0,
            0.0,
            1.0,
            Rgba {
                r: 1.0,
                g: 1.0,
                b: 1.0,
                a: 1.0,
            },
            |_, _, _, _, _| fills += 1,
        );
        assert!(fills > 0, "Resume must emit glyph pixels");
    }

    #[test]
    fn font_registry_load_doom_fonts_from_freedoom() {
        let Some(wad) = freedoom2() else {
            return;
        };
        let vfs = ferrite_vfs::VfsBuilder::new()
            .mount_path(&wad)
            .unwrap()
            .build()
            .unwrap();
        let reg = FontRegistry::load_doom_fonts(&vfs);
        assert!(reg.get("SmallFont").is_some());
        assert!(reg.get("BigFont").is_some());
        assert!(reg.get("SmallFont").unwrap().measure("HI") > 0.0);
        assert!(reg.get("BigFont").unwrap().measure("HI") > 0.0);
        let hud = reg.get(FontId::HUD_BIG).unwrap();
        assert!(hud.char_width(b'0') > 0);
        assert!(hud.measure("100%") > 10.0);
    }
}
