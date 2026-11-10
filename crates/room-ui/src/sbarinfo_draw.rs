//! SBARINFO status bars (UZDoom `DSBarInfo`, `g_statusbar/sbarinfo*.cpp`).
//!
//! A mod's SBARINFO replaces the stock status bar. [`SbarInfoBar::draw`]
//! runs the `normal` or `fullscreen` bar against the same [`SbarSnapshot`]
//! the ZScript bars use and returns its draw commands together with where
//! they go on the screen: SBARINFO places a bar differently from Room's
//! 320×200 menu space (a `forcescaled` normal bar keeps 4:3 proportions, so
//! on a widescreen display its negative coordinates land in the side
//! margins; `fullscreenoffsets` coordinates count from the screen edges).

use std::collections::BTreeMap;

use room_defs::{FontDef, SbarCmd, SbarInfo};

use crate::draw::{DrawCmd, Rect, UiDrawList, Vec2};
use crate::script_sbar::{PatchDims, SbarSnapshot};

/// A compiled SBARINFO lump with the FONTDEFS fonts it draws with.
#[derive(Clone, Debug)]
pub struct SbarInfoBar {
    /// Parsed SBARINFO.
    pub info: SbarInfo,
    /// FONTDEFS fonts by lowercase name.
    fonts: BTreeMap<String, FontDef>,
}

/// Per-frame inputs.
pub struct SbarInfoFrame<'a> {
    /// Player state.
    pub snap: &'a SbarSnapshot,
    /// Full-screen HUD (`screenblocks` 11) instead of the status bar.
    pub fullscreen: bool,
    /// Framebuffer size in pixels.
    pub screen: (f32, f32),
    /// Deathmatch game.
    pub deathmatch: bool,
    /// Team game.
    pub teamplay: bool,
    /// Multiplayer (cooperative when not deathmatch).
    pub multiplayer: bool,
    /// Player class display name (`PlayerClass` blocks).
    pub player_class: &'a str,
    /// `hud_aspectscale`.
    pub hud_aspectscale: bool,
    /// Patch sizes and offsets.
    pub dims: &'a dyn Fn(&str) -> Option<PatchDims>,
}

/// Draw commands for one bar and where they go.
#[derive(Clone, Debug)]
pub struct SbarInfoOutput {
    /// Commands in the bar's virtual space (`list.virtual_w/h`).
    pub list: UiDrawList,
    /// Screen pixel of the virtual space's top-left corner.
    pub origin: (f32, f32),
    /// Screen pixel size of the whole virtual space.
    pub size: (f32, f32),
}

/// Running state of one draw.
struct Ctx<'a, 'f> {
    f: &'f SbarInfoFrame<'a>,
    bar: &'f SbarInfoBar,
    out: Vec<DrawCmd>,
    /// Coordinates count from the screen edges.
    fullscreen_offsets: bool,
    /// Virtual canvas size.
    vw: f32,
    vh: f32,
}

impl SbarInfoBar {
    /// Wrap a parsed lump and the mod's FONTDEFS.
    #[must_use]
    pub fn new(info: SbarInfo, fonts: &BTreeMap<String, FontDef>) -> Self {
        Self {
            info,
            fonts: fonts
                .iter()
                .map(|(k, v)| (k.to_ascii_lowercase(), v.clone()))
                .collect(),
        }
    }

    /// Lumps the bar may draw, for preloading the patch cache.
    #[must_use]
    pub fn wanted_patches(&self, snap: &SbarSnapshot) -> Vec<String> {
        let mut out = Vec::new();
        for bar in self.info.statusbars.values() {
            collect_patches(&bar.commands, &mut out);
        }
        for d in 0..10 {
            out.push(format!("STTNUM{d}"));
            out.push(format!("STYSNUM{d}"));
        }
        out.extend(["STTMINUS".into(), "STTPRCNT".into()]);
        for code in 33..=95 {
            out.push(format!("STCFN{code:03}"));
        }
        for font in self.fonts.values() {
            out.extend(font.chars.values().cloned());
        }
        out.extend(snap.icons.iter().map(|(_, icon)| icon.clone()));
        // Mugshot prefixes: the current face under each (`STM` + `ST01`).
        let prefixes: Vec<String> = out
            .iter()
            .filter_map(|n| n.strip_prefix("@face:").map(str::to_owned))
            .collect();
        out.retain(|n| !n.starts_with("@face:"));
        if snap.face.len() > 3 {
            for p in prefixes {
                out.push(format!("{p}{}", &snap.face[3..]));
            }
        }
        out.push(snap.face.clone());
        out.retain(|n| !n.is_empty());
        out.sort();
        out.dedup();
        out
    }

    /// Draw the bar for this frame. `None` when the lump has no bar for the
    /// mode (the caller keeps its stock bar).
    #[must_use]
    pub fn draw(&self, f: &SbarInfoFrame<'_>) -> Option<SbarInfoOutput> {
        let kind = if f.fullscreen { "fullscreen" } else { "normal" };
        let bar = self.info.statusbars.get(kind)?;
        let has = |flag: &str| bar.flags.iter().any(|x| x.eq_ignore_ascii_case(flag));
        let fullscreen_offsets = has("fullscreenoffsets");
        let force_scaled = has("forcescaled");
        let (w, h) = (f.screen.0.max(1.0), f.screen.1.max(1.0));
        // `DStatusBarCore::SetScale` (the HUD scale): a 4:3 reference box,
        // 1/1.2 taller pixels unless `hud_aspectscale`.
        let (refw, mut refh) = if w / h < 4.0 / 3.0 {
            (w, w * 3.0 / 4.0)
        } else {
            (h * 4.0 / 3.0, h)
        };
        if !f.hud_aspectscale {
            refh /= 1.2;
        }
        let (sbx, sby) = (refw / 320.0, refh / 200.0);
        let (vw, vh, origin, size) = if fullscreen_offsets {
            // Fullscreen offsets: HUD scale from the screen edges.
            let (vw, vh) = (w / sbx, h / sby);
            (vw, vh, (0.0, 0.0), (w, h))
        } else if force_scaled {
            // `VirtualToRealCoords(320, 200, keepaspect)`: the full height,
            // 4:3 wide (16:9 at most), centred.
            let ratio = (w / h).min(16.0 / 9.0);
            let sx = if ratio > 1.334 { w / (320.0 * ratio * 0.75) } else { w / 320.0 };
            let sy = h / 200.0;
            (320.0, 200.0, (((w - 320.0 * sx) * 0.5).round(), 0.0), (320.0 * sx, 200.0 * sy))
        } else {
            // `ST_X` / `ST_Y`: the bar's box at the bottom, centred.
            (
                320.0,
                200.0,
                (((w - refw) * 0.5).round(), (h - refh).round()),
                (refw, refh),
            )
        };
        let mut cx = Ctx {
            f,
            bar: self,
            out: Vec::new(),
            fullscreen_offsets,
            vw,
            vh,
        };
        cx.run(&bar.commands, 1.0);
        Some(SbarInfoOutput {
            list: UiDrawList {
                cmds: cx.out,
                virtual_w: vw,
                virtual_h: vh,
            },
            origin,
            size,
        })
    }
}

fn collect_patches(cmds: &[SbarCmd], out: &mut Vec<String>) {
    for c in cmds {
        match c {
            SbarCmd::DrawMugShot { args } => {
                if let Some(p) = args.first().filter(|a| coord(a).is_none()) {
                    out.push(format!("@face:{}", p.trim_matches('"').to_ascii_uppercase()));
                }
            }
            SbarCmd::DrawImage { image, .. } => out.push(image.clone()),
            SbarCmd::DrawBar { args } => out.extend(args.iter().take(2).cloned()),
            SbarCmd::Other { name, args } if name.eq_ignore_ascii_case("drawswitchableimage") => {
                out.extend(args.iter().skip(1).filter(|a| coord(a).is_none()).cloned());
            }
            SbarCmd::Block { body, .. } => collect_patches(body, out),
            _ => {}
        }
    }
}

/// `N`, `-N`, `N + center`: a coordinate and whether it is relative to the
/// screen centre (`SBarInfoCoordinate::RelCenter`).
fn coord(tok: &str) -> Option<(i32, bool)> {
    let t: String = tok.chars().filter(|c| !c.is_whitespace()).collect();
    let lower = t.to_ascii_lowercase();
    if let Some(n) = lower.strip_suffix("+center") {
        return n.parse().ok().map(|n| (n, true));
    }
    if let Some(n) = lower.strip_prefix("center+") {
        return n.parse().ok().map(|n| (n, true));
    }
    lower.parse().ok().map(|n| (n, false))
}

/// Split flag tokens (`drawshadow(1,1) | whennotzero`) into lowercase words.
fn flag_words(tokens: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    for t in tokens {
        for w in t.split(['|', ' ']) {
            let w = w.trim();
            if !w.is_empty() {
                out.push(w.to_ascii_lowercase());
            }
        }
    }
    out
}

/// `(x, y)` plus what follows them: the first two coordinates in `tokens`.
fn coords(tokens: &[String]) -> Option<((i32, bool), (i32, bool), usize)> {
    let mut found = Vec::new();
    for (i, t) in tokens.iter().enumerate() {
        if let Some(c) = coord(t) {
            found.push((i, c));
            if found.len() == 2 {
                return Some((found[0].1, found[1].1, i + 1));
            }
        }
    }
    None
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Align {
    Left,
    Center,
    Right,
}

/// Image placement relative to its coordinate.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Anchor {
    /// Top-left minus the patch offsets (`DrawTexture` default).
    Offsets,
    /// `center`.
    Center,
    /// `centerbottom` (`DTA_CenterBottomOffset`).
    CenterBottom,
}

impl Ctx<'_, '_> {
    fn snap(&self) -> &SbarSnapshot {
        self.f.snap
    }

    fn dims(&self, lump: &str) -> Option<PatchDims> {
        (self.f.dims)(lump)
    }

    /// `FindInventory(class)`: `(amount, max)`.
    fn item(&self, class: &str) -> Option<(i32, i32)> {
        let class = class.trim_matches('"');
        self.snap()
            .inventory
            .iter()
            .find(|(n, _, _)| n.eq_ignore_ascii_case(class))
            .map(|(_, a, m)| (*a, *m))
    }

    fn amount(&self, class: &str) -> i32 {
        self.item(class).map_or(0, |(a, _)| a)
    }

    fn max_amount(&self, class: &str) -> i32 {
        let class = class.trim_matches('"');
        self.item(class).map(|(_, m)| m).or_else(|| {
            self.snap()
                .max_amounts
                .iter()
                .find(|(n, _)| n.eq_ignore_ascii_case(class))
                .map(|(_, m)| *m)
        })
        .unwrap_or(0)
    }

    fn icon(&self, class: &str) -> Option<String> {
        self.snap()
            .icons
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(class.trim_matches('"')))
            .map(|(_, i)| i.clone())
            .filter(|i| !i.is_empty())
    }

    fn armor(&self) -> i32 {
        self.amount("BasicArmor")
    }

    /// `GetEffectTicsForItem(giver)` as `(tics, full)`.
    fn power(&self, giver: &str) -> Option<(i32, i32)> {
        self.snap()
            .power_tics
            .iter()
            .find(|(n, _, _)| n.eq_ignore_ascii_case(giver.trim_matches('"')))
            .map(|(_, t, m)| (*t, *m))
    }

    fn ready_is(&self, class: &str) -> bool {
        self.snap()
            .ready_weapon
            .as_deref()
            .is_some_and(|w| w.eq_ignore_ascii_case(class.trim_matches('"')))
    }

    /// Screen position of a bar coordinate in the virtual canvas.
    fn place(&self, (x, xc): (i32, bool), (y, yc): (i32, bool)) -> (f32, f32) {
        #[allow(clippy::cast_precision_loss)]
        let (mut px, mut py) = (x as f32, y as f32);
        if self.fullscreen_offsets {
            if xc {
                px += self.vw / 2.0;
            } else if x < 0 {
                px += self.vw;
            }
            if yc {
                py += self.vh / 2.0;
            } else if y < 0 {
                py += self.vh;
            }
        }
        (px, py)
    }

    fn run(&mut self, cmds: &[SbarCmd], alpha: f32) {
        let mut i = 0;
        while i < cmds.len() {
            match &cmds[i] {
                SbarCmd::Block { kind, args, body } => {
                    let kind = kind.to_ascii_lowercase();
                    let else_body = match cmds.get(i + 1) {
                        Some(SbarCmd::Block {
                            kind: k, body, ..
                        }) if k.eq_ignore_ascii_case("else") => Some(body),
                        _ => None,
                    };
                    match kind.as_str() {
                        "alpha" => {
                            let a = args.first().and_then(|a| a.parse::<f32>().ok()).unwrap_or(1.0);
                            self.run(body, alpha * a);
                        }
                        "drawselectedinventory" => self.selected_inventory(args, Some(body), alpha),
                        // An `else` with no condition before it (already
                        // handled when its condition ran).
                        "else" => {}
                        _ => {
                            if self.condition(&kind, args) {
                                self.run(body, alpha);
                            } else if let Some(e) = else_body {
                                self.run(e, alpha);
                            }
                        }
                    }
                    if else_body.is_some() {
                        i += 1;
                    }
                }
                SbarCmd::DrawImage { image, x, y, flags } => {
                    self.draw_image_cmd(image, *x, *y, flags, alpha);
                }
                SbarCmd::DrawNumber {
                    digits,
                    font,
                    color,
                    value,
                    rest,
                } => self.draw_number(digits, font, color, value, rest, alpha),
                SbarCmd::DrawString { args } => self.draw_string_cmd(args, alpha),
                SbarCmd::DrawBar { args } => self.draw_bar(args, alpha),
                SbarCmd::DrawMugShot { args } => self.mugshot(args, alpha),
                SbarCmd::Other { name, args } => match name.to_ascii_lowercase().as_str() {
                    "drawswitchableimage" => self.switchable_image(args, alpha),
                    "drawselectedinventory" => self.selected_inventory(args, None, alpha),
                    "drawkeybar" => self.key_bar(args, alpha),
                    "drawimage" => {
                        if let Some(img) = args.first() {
                            let x = args.get(1).cloned().unwrap_or_default();
                            let y = args.get(2).cloned().unwrap_or_default();
                            let mut toks = vec![x, y];
                            toks.extend(args.iter().skip(3).cloned());
                            self.draw_image(img, &toks, alpha);
                        }
                    }
                    _ => {}
                },
            }
            i += 1;
        }
    }

    /// Flow-control truth (`SBarInfoNegatableFlowControl`).
    fn condition(&self, kind: &str, args: &[String]) -> bool {
        let (negate, args) = match args.first() {
            Some(a) if a.eq_ignore_ascii_case("not") => (true, &args[1..]),
            _ => (false, args),
        };
        let truth = match kind {
            "gamemode" => args.iter().any(|a| match a.to_ascii_lowercase().as_str() {
                "singleplayer" => !self.f.multiplayer,
                "cooperative" => self.f.multiplayer && !self.f.deathmatch,
                "deathmatch" => self.f.deathmatch,
                "teamgame" => self.f.teamplay,
                _ => false,
            }),
            // `CommandUsesAmmo`: the ready weapon has an ammo type.
            "usesammo" => self.snap().ammo1.is_some() || self.snap().ammo2.is_some(),
            "usessecondaryammo" => {
                let s = self.snap();
                s.ammo2.is_some() && s.ammo2 != s.ammo1
            }
            "inventorybarnotvisible" => self.snap().inventory_tics <= 0,
            "isselected" => args.iter().take(2).any(|w| self.ready_is(w)),
            "ininventory" => self.in_inventory(args),
            "weaponammo" => self.weapon_ammo(args),
            "aspectratio" => args.first().is_some_and(|r| self.aspect_is(r)),
            "playerclass" => args
                .iter()
                .any(|a| a.trim_matches('"').eq_ignore_ascii_case(self.f.player_class)),
            "armortype" => {
                // `armortype name, amount`: not tracked by name; any armor.
                let min = args.get(1).and_then(|a| a.parse().ok()).unwrap_or(1);
                self.armor() >= min
            }
            "health" | "ifhealth" => {
                let pct = args.iter().any(|a| a == "%");
                let n: i32 = args.iter().find_map(|a| a.parse().ok()).unwrap_or(0);
                let hp = self.snap().health;
                if pct { hp * 100 / 100 >= n } else { hp >= n }
            }
            "armor" | "ifarmor" => {
                let n: i32 = args.iter().find_map(|a| a.parse().ok()).unwrap_or(0);
                self.armor() >= n
            }
            "hasweaponpiece" | "ifinvulnerable" => false,
            _ => true,
        };
        truth != negate
    }

    /// `InInventory item [, amount] [||/&& item [, amount]]`.
    fn in_inventory(&self, args: &[String]) -> bool {
        let mut items: Vec<(String, i32)> = Vec::new();
        let mut and = false;
        for a in args {
            if a == "||" {
                continue;
            }
            if a == "&&" {
                and = true;
                continue;
            }
            if let (Ok(n), Some(last)) = (a.parse::<i32>(), items.last_mut()) {
                last.1 = n;
                continue;
            }
            items.push((a.clone(), 0));
        }
        let has = |(class, amount): &(String, i32)| {
            self.item(class)
                .is_some_and(|(a, _)| *amount <= 0 || a >= *amount)
        };
        match items.len() {
            0 => false,
            1 => has(&items[0]),
            _ if and => has(&items[0]) && has(&items[1]),
            _ => has(&items[0]) || has(&items[1]),
        }
    }

    /// `WeaponAmmo ammo [||/&& ammo]`: the ready weapon uses that ammo.
    fn weapon_ammo(&self, args: &[String]) -> bool {
        let s = self.snap();
        let uses = |a: &str| {
            s.ammo1.as_deref().is_some_and(|x| x.eq_ignore_ascii_case(a))
                || s.ammo2.as_deref().is_some_and(|x| x.eq_ignore_ascii_case(a))
        };
        let names: Vec<&String> = args.iter().filter(|a| *a != "||" && *a != "&&").collect();
        if args.iter().any(|a| a == "&&") {
            names.iter().all(|a| uses(a))
        } else {
            names.iter().any(|a| uses(a))
        }
    }

    /// `AspectRatio "16:9"` (`CommandAspectRatio`, `CheckRatio`).
    fn aspect_is(&self, ratio: &str) -> bool {
        let (w, h) = self.f.screen;
        let r = w / h.max(1.0);
        let screen = if r >= 1.74 {
            "16:9"
        } else if r >= 1.58 {
            "16:10"
        } else if r >= 1.3 {
            "4:3"
        } else {
            "5:4"
        };
        let want = ratio.trim_matches('"');
        want == screen || (want == "17:10" && (1.65..1.74).contains(&r))
    }

    // --- images -------------------------------------------------------

    fn draw_image_cmd(&mut self, image: &str, x: i32, y: i32, flags: &[String], alpha: f32) {
        // The parser keeps plain integer coordinates; a `N + center`
        // coordinate shows up as 0 with the raw text in the flags.
        let mut toks = vec![x.to_string(), y.to_string()];
        toks.extend(flags.iter().cloned());
        self.draw_image(image, &toks, alpha);
    }

    /// `drawimage image, x, y [, alignment [, maxw, maxh] [, flags]]`.
    fn draw_image(&mut self, image: &str, toks: &[String], alpha: f32) {
        let Some((x, y, next)) = coords(toks) else {
            return;
        };
        let words = flag_words(&toks[next..]);
        let anchor = anchor_of(&words);
        let nums: Vec<i32> = toks[next..].iter().filter_map(|t| t.trim().parse().ok()).collect();
        let boxsize = (nums.len() >= 2).then(|| (nums[0], nums[1]));
        let Some(lump) = self.resolve_image(image) else {
            return;
        };
        self.image(&lump, x, y, anchor, boxsize, alpha, None);
    }

    /// Image keyword / class / lump → lump (`CommandDrawImage::Tick`).
    fn resolve_image(&self, image: &str) -> Option<String> {
        let raw = image.trim_matches('"');
        let word = raw
            .strip_prefix("translatable ")
            .unwrap_or(raw)
            .trim()
            .trim_matches('"');
        let s = self.snap();
        let lump = match word.to_ascii_lowercase().as_str() {
            "playericon" => Some(s.face.clone()),
            "ammoicon1" => s.ammo1.as_deref().and_then(|a| self.icon(a)),
            "ammoicon2" => s.ammo2.as_deref().and_then(|a| self.icon(a)),
            "armoricon" => (self.armor() > 0).then(|| self.icon("BasicArmor")).flatten(),
            "weaponicon" => s.ready_weapon.as_deref().and_then(|w| self.icon(w)),
            "nullimage" | "" => None,
            _ => {
                // An inventory class draws its icon; anything else is a lump.
                if self.dims(word).is_none() {
                    self.icon(word).or_else(|| Some(word.to_owned()))
                } else {
                    Some(word.to_owned())
                }
            }
        };
        lump.filter(|l| !l.is_empty() && !l.eq_ignore_ascii_case("TNT1A0"))
    }

    /// `DSBarInfo::DrawGraphic`.
    #[allow(clippy::too_many_arguments)]
    fn image(
        &mut self,
        lump: &str,
        x: (i32, bool),
        y: (i32, bool),
        anchor: Anchor,
        boxsize: Option<(i32, i32)>,
        alpha: f32,
        clip: Option<Rect>,
    ) {
        let d = self.dims(lump).unwrap_or(PatchDims {
            w: 16.0,
            h: 16.0,
            left: 0.0,
            top: 0.0,
        });
        // `DI_DRAWINBOX`: shrink to fit the box, keeping the aspect.
        let (w, h) = match boxsize {
            #[allow(clippy::cast_precision_loss)]
            Some((bw, bh)) if bw > 0 || bh > 0 => {
                let s1 = if bw > 0 && (bw as f32) < d.w { bw as f32 / d.w } else { 1.0 };
                let s2 = if bh > 0 && (bh as f32) < d.h { bh as f32 / d.h } else { 1.0 };
                let s = s1.min(s2);
                (d.w * s, d.h * s)
            }
            _ => (d.w, d.h),
        };
        let (sx, sy) = (w / d.w.max(1.0), h / d.h.max(1.0));
        let (px, py) = self.place(x, y);
        let (px, py) = match anchor {
            Anchor::Offsets => (px - d.left * sx, py - d.top * sy),
            Anchor::Center => (px - w / 2.0, py - h / 2.0),
            Anchor::CenterBottom => (px - w / 2.0, py - h),
        };
        if let Some(r) = clip {
            self.out.push(DrawCmd::Clip { rect: Some(r) });
        }
        self.out.push(patch(lump, Vec2::new(px, py), (w, h), (d.w, d.h), alpha, None));
        if clip.is_some() {
            self.out.push(DrawCmd::Clip { rect: None });
        }
    }

    /// `drawswitchableimage cond, off… , on, x, y [, alignment …]`.
    fn switchable_image(&mut self, args: &[String], alpha: f32) {
        let Some(cond) = args.first() else {
            return;
        };
        let lower = cond.to_ascii_lowercase();
        let and = lower.contains("&&");
        let images_n = if and { 3 } else { 1 };
        if args.len() < 1 + images_n + 3 {
            return;
        }
        let alt_images: Vec<&String> = args[1..=images_n].iter().collect();
        let main_image = &args[1 + images_n];
        let rest = &args[2 + images_n..];
        let words: Vec<&str> = lower.split(|c: char| c.is_whitespace() || c == '&').filter(|w| !w.is_empty()).collect();
        let s = self.snap();
        // drawAlt: 0 main, 1 first ("off"), 2 second, 3 third.
        let draw_alt = match words.first().copied() {
            Some("weaponslot") => {
                let slot: usize = words.get(1).and_then(|n| n.parse().ok()).unwrap_or(0);
                usize::from(!s.slots.get(slot).copied().unwrap_or(false))
            }
            Some("invulnerable") => 0,
            Some("keyslot") => {
                let has = |n: Option<&&str>| {
                    n.and_then(|n| n.parse::<u8>().ok())
                        .is_some_and(|slot| s.keys & key_slot_bit(slot) != 0)
                };
                let (k1, k2) = (has(words.get(1)), has(words.get(2)));
                if and {
                    match (k1, k2) {
                        (true, true) => 0,
                        (true, false) => 2,
                        (false, true) => 3,
                        _ => 1,
                    }
                } else {
                    usize::from(!k1)
                }
            }
            Some("armortype") => usize::from(self.armor() <= 0),
            _ => {
                // Inventory items with an optional `> n` comparison.
                let parts: Vec<&str> = cond.split("&&").collect();
                let ok = |p: &str| {
                    let toks: Vec<&str> = p.split_whitespace().collect();
                    let Some(name) = toks.first() else { return false };
                    let Some((amount, _)) = self.item(name) else { return false };
                    match (toks.get(1), toks.get(2).and_then(|n| n.parse::<i32>().ok())) {
                        (Some(&"=="), Some(v)) => amount == v,
                        (Some(&"<"), Some(v)) => amount < v,
                        (Some(&"<="), Some(v)) => amount <= v,
                        (Some(&">="), Some(v)) => amount >= v,
                        (Some(&"!="), Some(v)) => amount != v,
                        (Some(&">"), Some(v)) => amount > v,
                        _ => amount > 0,
                    }
                };
                let first = ok(parts[0]);
                if and {
                    let second = parts.get(1).is_some_and(|p| ok(p));
                    match (first, second) {
                        (true, true) => 0,
                        (true, false) => 2,
                        (false, true) => 3,
                        _ => 1,
                    }
                } else {
                    usize::from(!first)
                }
            }
        };
        let image = if draw_alt == 0 {
            main_image
        } else {
            alt_images[draw_alt - 1]
        };
        self.draw_image(image, rest, alpha);
    }

    // --- numbers and strings --------------------------------------------

    /// Glyph lump for `ch` in `font`.
    fn glyph(&self, font: &str, ch: char) -> Option<String> {
        let f = font.trim_matches('"').to_ascii_lowercase();
        if let Some(def) = self.bar.fonts.get(&f) {
            let key = ch.to_string();
            if let Some(l) = def.chars.get(&key).or_else(|| def.chars.get(&key.to_ascii_uppercase())) {
                return Some(l.clone());
            }
            if let Some(t) = &def.template {
                let code = i64::from(u32::from(ch.to_ascii_uppercase()));
                if code >= def.first.unwrap_or(33) {
                    return Some(format_template(t, code));
                }
            }
            return None;
        }
        let digit = ch.to_digit(10);
        match f.as_str() {
            "hudfont_doom" => match (ch, digit) {
                (_, Some(d)) => Some(format!("STTNUM{d}")),
                ('%', _) => Some("STTPRCNT".into()),
                ('-', _) => Some("STTMINUS".into()),
                _ => None,
            },
            "indexfont_doom" | "indexfont" => digit.map(|d| format!("STYSNUM{d}")),
            _ => {
                let code = ch.to_ascii_uppercase() as u32;
                (33..=95).contains(&code).then(|| format!("STCFN{code:03}"))
            }
        }
    }

    fn space_width(&self, font: &str) -> f32 {
        let f = font.trim_matches('"').to_ascii_lowercase();
        #[allow(clippy::cast_precision_loss)]
        self.bar
            .fonts
            .get(&f)
            .and_then(|d| d.space_width)
            .map_or(4.0, |w| w as f32)
    }

    fn char_width(&self, font: &str, ch: char) -> f32 {
        if ch == ' ' {
            return self.space_width(font);
        }
        self.glyph(font, ch)
            .and_then(|g| self.dims(&g))
            .map_or(0.0, |d| d.w)
    }

    /// Monospace cell width (`monospacefonts true, "0"`).
    fn mono_cell(&self, font: &str) -> Option<f32> {
        let ch = self.bar.info.monospace_char.filter(|_| self.bar.info.monospace_fonts)?;
        let w = self.char_width(font, ch);
        (w > 0.0).then_some(w)
    }

    /// `RealignString` + `DSBarInfo::DrawString`.
    #[allow(clippy::too_many_arguments)]
    fn text(
        &mut self,
        font: &str,
        color: &str,
        text: &str,
        x: (i32, bool),
        y: (i32, bool),
        spacing: i32,
        align: Align,
        shadow: Option<(i32, i32)>,
        alpha: f32,
    ) {
        if text.is_empty() {
            return;
        }
        #[allow(clippy::cast_precision_loss)]
        let spacing_f = spacing as f32;
        let mono = self.mono_cell(font);
        #[allow(clippy::cast_precision_loss)]
        let len = text.chars().count() as f32;
        let width = match mono {
            Some(cell) => (cell + spacing_f) * len,
            None => text.chars().map(|c| self.char_width(font, c)).sum::<f32>() + spacing_f * len,
        };
        let shift = match align {
            Align::Left => 0.0,
            Align::Right => width.trunc(),
            Align::Center => (width / 2.0).trunc(),
        };
        let recolor = if color.eq_ignore_ascii_case("untranslated") {
            None
        } else {
            crate::acs_hud::ramp(crate::acs_hud::color_by_name(color))
        };
        let mono_align = match self.bar.info.monospace_align.as_deref() {
            Some("left") => Align::Left,
            Some("right") => Align::Right,
            _ => Align::Center,
        };
        #[allow(clippy::cast_precision_loss)]
        let mut ax = x.0 as f32 - shift + spacing_f;
        #[allow(clippy::cast_precision_loss)]
        let ay = y.0 as f32;
        for ch in text.chars() {
            if ch == ' ' {
                ax += mono.unwrap_or_else(|| self.space_width(font));
                continue;
            }
            let Some(lump) = self.glyph(font, ch) else {
                continue;
            };
            let d = self.dims(&lump).unwrap_or_default();
            // Glyph left edge: monospaced glyphs sit in their cell; others
            // are drawn one pixel in (`ax += left + 1`, then the offset).
            let gx = match mono {
                Some(cell) => {
                    let inner = match mono_align {
                        Align::Left => 0.0,
                        Align::Center => cell / 2.0 - d.w / 2.0,
                        Align::Right => cell - d.w,
                    };
                    ax + inner - d.left
                }
                None => ax + 1.0,
            };
            let gy = ay - d.top;
            // Each glyph is anchored on its own (`rx < 0` per character).
            #[allow(clippy::cast_possible_truncation)]
            let (px, py) = self.place((gx.floor() as i32, x.1), (gy.floor() as i32, y.1));
            let frac = (gx - gx.floor(), gy - gy.floor());
            if let Some((dx, dy)) = shadow {
                #[allow(clippy::cast_precision_loss)]
                let sp = Vec2::new(px + frac.0 + dx as f32, py + frac.1 + dy as f32);
                let mut c = patch(&lump, sp, (d.w, d.h), (d.w, d.h), alpha * 0.5, None);
                if let DrawCmd::Patch { tint, .. } = &mut c {
                    *tint = [0.0, 0.0, 0.0];
                }
                self.out.push(c);
            }
            self.out.push(patch(
                &lump,
                Vec2::new(px + frac.0, py + frac.1),
                (d.w, d.h),
                (d.w, d.h),
                alpha,
                recolor,
            ));
            ax += match mono {
                Some(cell) => cell + spacing_f,
                None => d.w + spacing_f,
            };
        }
    }

    /// `CommandDrawNumber`.
    fn draw_number(&mut self, digits: &str, font: &str, color: &str, value: &str, rest: &[String], alpha: f32) {
        let Some((x, y, next)) = coords(rest) else {
            return;
        };
        let spacing = rest.get(next).and_then(|t| t.trim().parse::<i32>().ok()).unwrap_or(0);
        let words = flag_words(&rest[..next.saturating_sub(2)]);
        let has = |w: &str| words.iter().any(|x| x == w || x.starts_with(&format!("{w}(")));
        let length: i32 = digits.trim().parse().unwrap_or(3);
        let Some(mut num) = self.number_value(value) else {
            return;
        };
        if value.eq_ignore_ascii_case("health") && self.bar.info.lower_health_cap && num < 0 {
            num = 0;
        }
        if has("whennotzero") && num == 0 {
            return;
        }
        let fill = has("fillzeros");
        if !has("dontcap") {
            let maxval = if length <= 9 { 10i64.pow(length.max(1) as u32) - 1 } else { i64::from(i32::MAX) };
            let minval = if fill && length > 1 && length <= 9 { -(10i64.pow((length - 1) as u32) - 1) } else { -maxval };
            #[allow(clippy::cast_possible_truncation)]
            {
                num = i64::from(num).clamp(minval, maxval) as i32;
            }
        }
        let text = if fill {
            let width = if num < 0 { length - 1 } else { length };
            format!("{num:0width$}", width = width.max(0) as usize)
        } else {
            num.to_string()
        };
        let align = alignment_of(&words).unwrap_or(Align::Right);
        let shadow = shadow_of(&words);
        self.text(font, color, &text, x, y, spacing, align, shadow, alpha);
    }

    /// A number's value (`CommandDrawNumber::Tick`); `None` hides it.
    fn number_value(&self, value: &str) -> Option<i32> {
        let v = value.trim();
        let lower = v.to_ascii_lowercase();
        let (head, arg) = match lower.split_once(|c: char| c == ' ' || c == '(') {
            Some((h, _)) => (h.to_owned(), Some(v[h.len()..].trim().trim_matches(['(', ')', '"', ' ']).to_owned())),
            None => (lower.clone(), None),
        };
        let s = self.snap();
        Some(match (head.as_str(), arg) {
            ("health", None) => s.health,
            ("armor", None) => self.armor(),
            ("ammo1", None) => self.amount(s.ammo1.as_deref()?),
            ("ammo2", None) => self.amount(s.ammo2.as_deref()?),
            ("ammo1capacity", None) => self.max_amount(s.ammo1.as_deref()?),
            ("ammo2capacity", None) => self.max_amount(s.ammo2.as_deref()?),
            ("ammo", Some(a)) => self.amount(&a),
            ("ammocapacity", Some(a)) => self.max_amount(&a),
            ("poweruptime", Some(g)) => self.power(&g).map_or(0, |(t, _)| if t < 0 { 0 } else { t / 35 + 1 }),
            ("frags", None) => s.frags,
            ("keys", None) => s.keys.count_ones() as i32,
            ("selectedinventory", None) => s.inv_sel.as_deref().map_or(0, |c| self.amount(c)),
            ("kills" | "monsters" | "items" | "totalitems" | "secrets" | "totalsecrets" | "score"
                | "armorclass" | "savepercent" | "airtime" | "accuracy" | "stamina", None) => 0,
            (_, None) => v.parse().unwrap_or_else(|_| self.amount(v)),
            (_, Some(_)) => self.amount(v),
        })
    }

    /// `drawstring font, translation, string, x, y [, spacing [, flags]]`.
    fn draw_string_cmd(&mut self, args: &[String], alpha: f32) {
        if args.len() < 5 {
            return;
        }
        let font = &args[0];
        let color = &args[1];
        let raw = args[2].trim_matches('"');
        let s = self.snap();
        let text = match raw.to_ascii_lowercase().as_str() {
            "weapontag" => s.ready_weapon.clone().unwrap_or_default(),
            "ammo1tag" => s.ammo1.clone().unwrap_or_default(),
            "ammo2tag" => s.ammo2.clone().unwrap_or_default(),
            "inventorytag" => s.inv_sel.clone().unwrap_or_default(),
            "playerclass" => self.f.player_class.to_owned(),
            "levelname" | "levellump" | "skillname" | "playername" | "time" | "logtext" => String::new(),
            _ => raw.to_owned(),
        };
        let Some((x, y, next)) = coords(&args[3..]) else {
            return;
        };
        let next = next + 3;
        let spacing = args.get(next).and_then(|t| t.trim().parse().ok()).unwrap_or(0);
        let words = flag_words(&args[(next + 1).min(args.len())..]);
        let align = alignment_of(&words).unwrap_or(Align::Right);
        self.text(font, color, &text, x, y, spacing, align, shadow_of(&words), alpha);
    }

    // --- bars, mugshot, inventory, keys ---------------------------------

    /// `CommandDrawBar`.
    fn draw_bar(&mut self, args: &[String], alpha: f32) {
        if args.len() < 5 {
            return;
        }
        let fg = args[0].trim_matches('"').to_owned();
        let bg = args[1].trim_matches('"').to_owned();
        let spec = args[2].trim().to_owned();
        let Some((x, y, next)) = coords(&args[3..]) else {
            return;
        };
        let next = next + 3;
        let words = flag_words(&args[3..next - 2]);
        let horizontal = words.iter().any(|w| w == "horizontal");
        let mut reverse = words.iter().any(|w| w == "reverse");
        let border: f32 = args.get(next).and_then(|t| t.trim().parse().ok()).unwrap_or(0.0);
        if border != 0.0 {
            reverse = !reverse;
        }
        let (value, max) = self.bar_value(&spec);
        #[allow(clippy::cast_precision_loss)]
        let mut frac = if max > 0 { (value as f32 / max as f32).clamp(0.0, 1.0) } else { 0.0 };
        let Some(fd) = self.dims(&fg) else {
            return;
        };
        let bgd = self.dims(&bg);
        let same = bgd.is_some_and(|b| (b.w - fd.w).abs() < 0.5 && (b.h - fd.h).abs() < 0.5);
        if border != 0.0 {
            frac = 1.0 - frac;
            self.image(&fg, x, y, Anchor::Offsets, None, alpha, None);
        } else if same {
            self.image(&bg, x, y, Anchor::Offsets, None, alpha, None);
        }
        // Clip `[left, top, right, bottom]` off the image.
        let size = if horizontal { fd.w } else { fd.h } - border * 2.0;
        let cut = size - size * frac;
        let idx = usize::from(!horizontal) | (usize::from(if horizontal { !reverse } else { reverse }) << 1);
        let mut clip = [0.0f32; 4];
        clip[idx] = cut;
        if border != 0.0 {
            for c in &mut clip {
                *c += border;
            }
        }
        let (px, py) = self.place(x, y);
        let (left, top) = (px - fd.left, py - fd.top);
        let rect = Rect {
            x: left + clip[0],
            y: top + clip[1],
            w: (fd.w - clip[0] - clip[2]).max(0.0),
            h: (fd.h - clip[1] - clip[3]).max(0.0),
        };
        let img = if border != 0.0 && same { bg } else { fg };
        if rect.w > 0.0 && rect.h > 0.0 {
            self.image(&img, x, y, Anchor::Offsets, None, alpha, Some(rect));
        }
    }

    /// Bar `(value, max)` (`CommandDrawBar::Tick`).
    fn bar_value(&self, spec: &str) -> (i32, i32) {
        let lower = spec.to_ascii_lowercase();
        let (head, arg) = match lower.split_once(|c: char| c == ' ' || c == '(') {
            Some((h, _)) => (h.to_owned(), Some(spec[h.len()..].trim().trim_matches(['(', ')', '"', ' ']).to_owned())),
            None => (lower.clone(), None),
        };
        let s = self.snap();
        match (head.as_str(), arg) {
            ("health", a) => {
                let max = a.filter(|a| !a.is_empty()).map_or(100, |a| a.parse().unwrap_or_else(|_| self.amount(&a)));
                (s.health.max(0), max)
            }
            ("armor", a) => {
                let max = a.filter(|a| !a.is_empty()).map_or(100, |a| a.parse().unwrap_or_else(|_| self.amount(&a)));
                (self.armor(), max)
            }
            ("ammo1", _) => s.ammo1.as_deref().map_or((0, 1), |a| (self.amount(a), self.max_amount(a))),
            ("ammo2", _) => s.ammo2.as_deref().map_or((0, 1), |a| (self.amount(a), self.max_amount(a))),
            ("ammo", Some(a)) => (self.amount(&a), self.max_amount(&a)),
            ("poweruptime", Some(g)) => self.power(&g).map_or((0, 1), |(t, m)| (t + 1, m + 1)),
            ("frags", _) => (s.frags, 0),
            (_, _) => (self.amount(spec), self.max_amount(spec)),
        }
    }

    /// `drawmugshot ["prefix",] accuracy [, flags], x, y`.
    fn mugshot(&mut self, args: &[String], alpha: f32) {
        // The accuracy is a number too: the coordinates are the last two.
        let nums: Vec<(i32, bool)> = args.iter().filter_map(|a| coord(a)).collect();
        let [.., x, y] = nums[..] else {
            return;
        };
        let face = &self.snap().face;
        if face.is_empty() {
            return;
        }
        let prefix = args
            .first()
            .filter(|a| coord(a).is_none() && !a.trim().is_empty())
            .map(|a| a.trim_matches('"').to_ascii_uppercase());
        // `STFST01` with the bar's face prefix (`STM` → `STMST01`).
        let lump = match prefix {
            Some(p) if face.len() > 3 && !p.eq_ignore_ascii_case("STF") => {
                let custom = format!("{p}{}", &face[3..]);
                if self.dims(&custom).is_some() { custom } else { face.clone() }
            }
            _ => face.clone(),
        };
        self.image(&lump, x, y, Anchor::Offsets, None, alpha, None);
    }

    /// `drawselectedinventory [flags,] font, x, y [, cx, cy [, color]] [{ alternate }]`.
    fn selected_inventory(&mut self, args: &[String], alternate: Option<&Vec<SbarCmd>>, alpha: f32) {
        let words = flag_words(args);
        let sel = self.snap().inv_sel.clone();
        let Some(sel) = sel else {
            if words.iter().any(|w| w == "alternateonempty") {
                if let Some(body) = alternate {
                    self.run(body, alpha);
                }
            }
            return;
        };
        let nums: Vec<(i32, bool)> = args.iter().filter_map(|a| coord(a)).collect();
        if nums.len() < 2 {
            return;
        }
        let anchor = anchor_of(&words);
        if let Some(icon) = self.icon(&sel) {
            self.image(&icon, nums[0], nums[1], anchor, None, alpha, None);
        }
        let amount = self.amount(&sel);
        if nums.len() >= 4 && (amount > 1 || words.iter().any(|w| w == "alwaysshowcounter")) {
            let font = args
                .iter()
                .find(|a| coord(a).is_none() && !is_selinv_flag(a))
                .cloned()
                .unwrap_or_else(|| "INDEXFONT".into());
            let color = args
                .iter()
                .rev()
                .find(|a| coord(a).is_none() && !is_selinv_flag(a) && **a != font)
                .cloned()
                .unwrap_or_else(|| "untranslated".into());
            let shadow = shadow_of(&words);
            self.text(&font, &color, &amount.to_string(), nums[2], nums[3], 0, Align::Right, shadow, alpha);
        }
    }

    /// `drawkeybar number, direction [, flags], spacing|auto, x, y [, offset [, rowsize [, rowspacing]]]`.
    fn key_bar(&mut self, args: &[String], alpha: f32) {
        let words = flag_words(args);
        let vertical = words.iter().any(|w| w == "vertical");
        let reverse = words.iter().any(|w| w == "reverse");
        let size: usize = args.first().and_then(|a| a.trim().parse().ok()).unwrap_or(1);
        // Tokens after the direction / flags: spacing, x, y, offset, ….
        let tail: Vec<&String> = args
            .iter()
            .skip(2)
            .filter(|a| {
                let l = a.to_ascii_lowercase();
                l != "reverse" && l != "reverserows"
            })
            .collect();
        let spacing = tail.first().and_then(|a| a.trim().parse::<f32>().ok());
        let (Some(x), Some(y)) = (tail.get(1).and_then(|a| coord(a)), tail.get(2).and_then(|a| coord(a))) else {
            return;
        };
        let offset: usize = tail.get(3).and_then(|a| a.trim().parse().ok()).unwrap_or(0);
        let keys: Vec<String> = self
            .snap()
            .spawn_sprites
            .iter()
            .filter(|(k, _)| self.amount(k) > 0)
            .filter_map(|(k, _)| self.icon(k))
            .skip(offset)
            .take(size)
            .collect();
        let mut pos = 0.0f32;
        for icon in keys {
            let d = self.dims(&icon).unwrap_or_default();
            let step = spacing.unwrap_or(if vertical { d.h } else { d.w });
            let delta = if reverse { -pos } else { pos };
            #[allow(clippy::cast_possible_truncation)]
            let (kx, ky) = if vertical {
                (x, (y.0 + delta as i32, y.1))
            } else {
                ((x.0 + delta as i32, x.1), y)
            };
            self.image(&icon, kx, ky, Anchor::Offsets, None, alpha, None);
            pos += step;
        }
    }
}

fn is_selinv_flag(a: &str) -> bool {
    matches!(
        a.trim().to_ascii_lowercase().as_str(),
        "alternateonempty" | "artiflash" | "alwaysshowcounter" | "center" | "centerbottom" | "drawshadow"
    ) || a.to_ascii_lowercase().starts_with("drawshadow(")
}

fn anchor_of(words: &[String]) -> Anchor {
    if words.iter().any(|w| w == "centerbottom") {
        Anchor::CenterBottom
    } else if words.iter().any(|w| w == "center") {
        Anchor::Center
    } else {
        Anchor::Offsets
    }
}

fn alignment_of(words: &[String]) -> Option<Align> {
    words.iter().find_map(|w| match w.as_str() {
        "alignment(left)" => Some(Align::Left),
        "alignment(center)" => Some(Align::Center),
        "alignment(right)" => Some(Align::Right),
        _ => None,
    })
}

fn shadow_of(words: &[String]) -> Option<(i32, i32)> {
    words.iter().find_map(|w| {
        if w == "drawshadow" {
            Some((2, 2))
        } else {
            let inner = w.strip_prefix("drawshadow(")?.trim_end_matches(')');
            let (a, b) = inner.split_once(',')?;
            Some((a.trim().parse().ok()?, b.trim().parse().ok()?))
        }
    })
}

/// SBARINFO key slot → `SbarSnapshot::keys` bit (Doom: 1 red, 2 blue,
/// 3 yellow cards; 4–6 the skulls).
fn key_slot_bit(slot: u8) -> u8 {
    match slot {
        1 => 4,
        2 => 1,
        3 => 2,
        4 => 32,
        5 => 8,
        6 => 16,
        _ => 0,
    }
}

/// `STCFN%03d`-style FONTDEFS template.
fn format_template(t: &str, code: i64) -> String {
    if let Some(i) = t.find('%') {
        let rest = &t[i + 1..];
        let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
        let width = digits.trim_start_matches('0').parse::<usize>().unwrap_or(0);
        let after = &rest[digits.len()..];
        let after = after.strip_prefix('d').unwrap_or(after);
        format!("{}{code:0width$}{after}", &t[..i])
    } else {
        t.to_owned()
    }
}

fn patch(name: &str, pos: Vec2, size: (f32, f32), native: (f32, f32), alpha: f32, recolor: Option<[[f32; 3]; 2]>) -> DrawCmd {
    let scaled = (size.0 - native.0).abs() > 0.01 || (size.1 - native.1).abs() > 0.01;
    DrawCmd::Patch {
        name: name.to_owned(),
        pos,
        size: if scaled { Vec2::new(size.0, size.1) } else { Vec2::new(0.0, 0.0) },
        apply_offsets: false,
        inverse: false,
        light: 1.0,
        fuzz: false,
        flip: false,
        mirror: false,
        angle: 0.0,
        pivot_x: 0.5,
        pivot_y: 0.5,
        tint: [1.0, 1.0, 1.0],
        alpha: alpha.clamp(0.0, 1.0),
        recolor,
        corners: [Vec2::new(0.0, 0.0); 4],
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use room_defs::parse_sbarinfo;

    fn dims(name: &str) -> Option<PatchDims> {
        let d = |w: f32, h: f32| Some(PatchDims { w, h, left: 0.0, top: 0.0 });
        match name {
            n if n.starts_with("STTNUM") || n == "STTMINUS" || n == "STTPRCNT" => d(14.0, 16.0),
            n if n.starts_with("STYSNUM") => d(4.0, 6.0),
            "STBAR" | "STBARBD" => d(320.0, 32.0),
            "STMAG" => d(48.0, 32.0),
            "BAR" | "BARBG" => d(40.0, 8.0),
            n if n.starts_with("STF") => d(24.0, 29.0),
            _ => None,
        }
    }

    fn snap() -> SbarSnapshot {
        SbarSnapshot {
            health: 87,
            face: "STFST01".into(),
            inventory: vec![
                ("Clip2".into(), 120, 300),
                ("RifleAmmo".into(), 23, 31),
                ("BasicArmor".into(), 45, 200),
            ],
            ready_weapon: Some("Rifle".into()),
            ammo1: Some("Clip2".into()),
            ammo2: Some("RifleAmmo".into()),
            ..SbarSnapshot::default()
        }
    }

    /// Digits drawn left to right as `(digit, x, y)`.
    fn digits(out: &SbarInfoOutput, prefix: &str) -> Vec<(char, i32, i32)> {
        out.list
            .cmds
            .iter()
            .filter_map(|c| match c {
                DrawCmd::Patch { name, pos, .. } => name
                    .strip_prefix(prefix)
                    .and_then(|d| d.chars().next())
                    .map(|d| (d, pos.x.round() as i32, pos.y.round() as i32)),
                _ => None,
            })
            .collect()
    }

    fn frame<'a>(s: &'a SbarSnapshot, d: &'a dyn Fn(&str) -> Option<PatchDims>, fullscreen: bool) -> SbarInfoFrame<'a> {
        SbarInfoFrame {
            snap: s,
            fullscreen,
            screen: (1280.0, 720.0),
            deathmatch: false,
            teamplay: false,
            multiplayer: false,
            player_class: "",
            hud_aspectscale: false,
            dims: d,
        }
    }

    const BD_LIKE: &str = r#"
base Doom;
height 32;
monospacefonts true, "0";
statusbar normal, forcescaled
{
    drawimage "STBARBD", 0, 168;
    InInventory not NoReloading, 1 || IsPlayingAsPurist, 1
    {
        UsesSecondaryAmmo
        {
            DrawImage "STMAG", -49, 168;
            drawnumber 3, HUDFONT_DOOM, untranslated, ammo2, alignment(center), -25, 171;
            drawnumber 3, HUDFONT_DOOM, untranslated, ammo1, 44, 171;
        }
    }
    UsesSecondaryAmmo not
    {
        drawnumber 3, HUDFONT_DOOM, untranslated, ammo1, 44, 171;
    }
    drawnumber 3, HUDFONT_DOOM, untranslated, health, 90, 171;
    drawnumber 3, INDEXFONT_DOOM, untranslated, ammo Clip2, 284, 176;
}
statusbar fullscreen, fullscreenoffsets, forcescaled
{
    drawnumber 2147483647, HUDFONT_DOOM, untranslated, ammo2, drawshadow, -55, -20;
    IsSelected Pistol
    {
        drawnumber 3, HUDFONT_DOOM, untranslated, health, 10, 10;
    }
    else InInventory Clip2, 100
    {
        drawbar "BAR", "BARBG", ammo RifleAmmo, horizontal, -140 + center, -41;
    }
}
"#;

    #[test]
    fn brutal_doom_bar_shows_the_magazine_left_of_the_bar() {
        let info = parse_sbarinfo(BD_LIKE).unwrap();
        assert_eq!(info.monospace_char, Some('0'));
        let bar = SbarInfoBar::new(info, &BTreeMap::new());
        let s = snap();
        let out = bar.draw(&frame(&s, &dims, false)).unwrap();
        // `forcescaled` at 1280×720: 3.0 wide, 3.6 tall, 160 px margins.
        assert_eq!(out.origin, (160.0, 0.0));
        assert!((out.size.0 - 960.0).abs() < 0.01 && out.size.1 == 720.0, "{:?}", out.size);
        assert!(out.list.cmds.iter().any(|c| matches!(c, DrawCmd::Patch { name, pos, .. } if name == "STMAG" && pos.x == -49.0)));
        let big = digits(&out, "STTNUM");
        // Magazine 23 centred on x=-25: two 14-wide cells from -39.
        assert_eq!(&big[..2], &[('2', -39, 171), ('3', -25, 171)]);
        // Reserve 120 right-aligned on x=44 (three cells from 2).
        assert_eq!(&big[2..5], &[('1', 2, 171), ('2', 16, 171), ('0', 30, 171)]);
        // Health 87 right-aligned on 90.
        assert_eq!(&big[5..], &[('8', 62, 171), ('7', 76, 171)]);
        // `ammo Clip2` in the small font.
        let small = digits(&out, "STYSNUM");
        assert_eq!(small.iter().map(|d| d.0).collect::<String>(), "120");

        // A gun without a magazine: only the reserve.
        let mut s2 = snap();
        s2.ammo2 = None;
        let out = bar.draw(&frame(&s2, &dims, false)).unwrap();
        assert!(!out.list.cmds.iter().any(|c| matches!(c, DrawCmd::Patch { name, .. } if name == "STMAG")));
        // `NoReloading` hides the magazine panel too.
        let mut s3 = snap();
        s3.inventory.push(("NoReloading".into(), 1, 1));
        let out = bar.draw(&frame(&s3, &dims, false)).unwrap();
        assert!(!out.list.cmds.iter().any(|c| matches!(c, DrawCmd::Patch { name, .. } if name == "STMAG")));
    }

    #[test]
    fn fullscreen_offsets_count_from_the_screen_edges() {
        let bar = SbarInfoBar::new(parse_sbarinfo(BD_LIKE).unwrap(), &BTreeMap::new());
        let s = snap();
        let out = bar.draw(&frame(&s, &dims, true)).unwrap();
        // HUD scale 3×3 at 720p: a 426.67×240 canvas over the whole screen.
        assert_eq!(out.size, (1280.0, 720.0));
        assert!((out.list.virtual_w - 426.667).abs() < 0.01);
        assert_eq!(out.list.virtual_h, 240.0);
        let big = digits(&out, "STTNUM");
        // "23" right-aligned on -55, anchored to the right edge; the
        // shadow copies come first.
        let main: Vec<_> = big.iter().skip(1).step_by(2).collect();
        assert_eq!(main.len(), 2);
        assert_eq!(main[0].1, (426.667f32 - 55.0 - 28.0).round() as i32);
        assert_eq!(main[0].2, 240 - 20);
        // `else InInventory`: not a Pistol, and 120 ≥ 100 Clip2 → the bar,
        // clipped to 23/31 of its width.
        let clip = out.list.cmds.iter().find_map(|c| match c {
            DrawCmd::Clip { rect: Some(r) } => Some(*r),
            _ => None,
        });
        let r = clip.expect("drawbar clips its foreground");
        assert!((r.w - 40.0 * 23.0 / 31.0).abs() < 0.01, "{r:?}");
        assert!((r.x - (426.667 / 2.0 - 140.0)).abs() < 0.01);
    }

    #[test]
    fn brace_less_blocks_take_one_command() {
        let text = r#"
statusbar normal
{
    gamemode deathmatch, teamgame
        drawnumber 2, HUDFONT_DOOM, untranslated, frags, 138, 171;
    else InInventory Clip2, 1
    {
        drawnumber 3, HUDFONT_DOOM, untranslated, health, 90, 171;
    }
    Alpha 0.5
    InInventory RifleAmmo
    {
        drawimage "STBAR", 0, 168;
    }
}
"#;
        let bar = SbarInfoBar::new(parse_sbarinfo(text).unwrap(), &BTreeMap::new());
        let s = snap();
        let out = bar.draw(&frame(&s, &dims, false)).unwrap();
        let big = digits(&out, "STTNUM");
        assert_eq!(big.iter().map(|d| d.0).collect::<String>(), "87", "else branch draws health, not frags");
        let alpha = out.list.cmds.iter().find_map(|c| match c {
            DrawCmd::Patch { name, alpha, .. } if name == "STBAR" => Some(*alpha),
            _ => None,
        });
        assert_eq!(alpha, Some(0.5));
    }
}
