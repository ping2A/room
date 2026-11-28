//! ZScript status bars (`BaseStatusBar` subclasses, UZ4).
//!
//! A compiled program holding the UZDoom stdlib (and any mod sources) runs a
//! status bar class — `DoomStatusBar` or a mod's `GameInfo` `StatusBarClass` —
//! in its own VM. `Init` runs once, `Draw(state, TicFrac)` every frame. The
//! `StatusBarCore` drawing natives turn into [`DrawCmd`]s in the 320×200 HUD
//! space Room's other bars use.
//!
//! Player data comes in as an [`SbarSnapshot`]. Before each frame it is
//! mirrored into a `PlayerInfo` object (`CPlayer`), its pawn (`CPlayer.mo`,
//! with the inventory the `FindInventory` / `CountInv` natives read) and the
//! ready weapon with its `Ammo1` / `Ammo2`.

use std::cell::RefCell;

use room_zvm::{
    ActorSlots, CompiledProgram, NativeRegs, ObjHandle, ObjKind, Trap, Value, Zvm, ZvmCtx,
};

use crate::draw::{DrawCmd, Rgba, UiDrawList, Vec2};

/// What the bar shows for the local player.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SbarSnapshot {
    /// Health.
    pub health: i32,
    /// Doom key bits: 1 blue card, 2 yellow card, 4 red card, 8 blue skull,
    /// 16 yellow skull, 32 red skull.
    pub keys: u8,
    /// Weapon slots 0–9 that hold an owned weapon.
    pub slots: [bool; 10],
    /// Mugshot lump (`STFST01`, …).
    pub face: String,
    /// Inventory `(class, amount, max amount)`; armor as `BasicArmor`.
    pub inventory: Vec<(String, i32, i32)>,
    /// Icon lump per class name (ammo, armor, items).
    pub icons: Vec<(String, String)>,
    /// Ready weapon class.
    pub ready_weapon: Option<String>,
    /// Its primary ammo class.
    pub ammo1: Option<String>,
    /// Its secondary ammo class.
    pub ammo2: Option<String>,
    /// Frags.
    pub frags: i32,
    /// Selected inventory-bar item (`InvSel`).
    pub inv_sel: Option<String>,
    /// Inventory-bar items in bar order (`+INVENTORY.INVBAR`).
    pub inv_bar: Vec<String>,
    /// Tics the inventory bar stays open (`inventorytics`).
    pub inventory_tics: i32,
    /// Weapons in the player's weapon slots (owned or not), for the alt
    /// HUD's ammo and weapon lists.
    pub weapons: Vec<SbarWeapon>,
    /// Spawn-state sprite per class (`BKEYA0`): the alt HUD draws keys with
    /// it rather than the small status bar icon.
    pub spawn_sprites: Vec<(String, String)>,
    /// `MaxAmount` defaults of classes the player does not carry.
    pub max_amounts: Vec<(String, i32)>,
    /// Running powers as `(PowerupGiver class, tics left, full tics)`
    /// (SBARINFO `PowerupTime`).
    pub power_tics: Vec<(String, i32, i32)>,
    /// Parent chain (nearest first) per class, so a DECORATE class the
    /// script program lacks is typed as its nearest script ancestor.
    pub ancestors: Vec<(String, Vec<String>)>,
    /// The pawn's class (typed with its script ancestors for `CPlayer.mo`).
    pub player_class: Option<String>,
    /// The pawn's angle, pitch, roll (degrees), velocity x / y / z, height
    /// and floor height (`StatusBarItems::pose`).
    pub pose: [f64; 8],
}

/// One weapon-slot entry (`WeaponSlots.GetWeapon(slot, index)`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SbarWeapon {
    /// Weapon class.
    pub class: String,
    /// Slot 0–9.
    pub slot: u8,
    /// `AmmoType1`.
    pub ammo1: Option<String>,
    /// `AmmoType2`.
    pub ammo2: Option<String>,
}

/// Doom's key classes in definition order (`Key.GetKeyType`).
const DOOM_KEYS: [&str; 6] = [
    "BlueCard",
    "YellowCard",
    "RedCard",
    "BlueSkull",
    "YellowSkull",
    "RedSkull",
];

/// Patch size and offsets, from the renderer's patch cache.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PatchDims {
    /// Width.
    pub w: f32,
    /// Height.
    pub h: f32,
    /// Left offset.
    pub left: f32,
    /// Top offset.
    pub top: f32,
}

/// `HUD_StatusBar` / `HUD_Fullscreen` (`BaseStatusBar.Draw` state).
pub const HUD_STATUSBAR: i64 = 0;
/// See [`HUD_STATUSBAR`].
pub const HUD_FULLSCREEN: i64 = 1;

// `StatusBarCore` DI_* flags.
const DI_SCREEN_MANUAL_ALIGN: i64 = 0x4000;
const DI_SCREEN_VMASK: i64 = 0x18000 | DI_SCREEN_MANUAL_ALIGN;
const DI_SCREEN_HMASK: i64 = 0x60000 | DI_SCREEN_MANUAL_ALIGN;
const DI_SCREEN_BOTTOM: i64 = 0x10000 | DI_SCREEN_MANUAL_ALIGN;
const DI_SCREEN_VCENTER: i64 = 0x8000 | DI_SCREEN_MANUAL_ALIGN;
const DI_SCREEN_RIGHT: i64 = 0x40000 | DI_SCREEN_MANUAL_ALIGN;
const DI_SCREEN_HCENTER: i64 = 0x20000 | DI_SCREEN_MANUAL_ALIGN;
const DI_ITEM_VMASK: i64 = 0x18_0000;
const DI_ITEM_TOP: i64 = 0x8_0000;
const DI_ITEM_VCENTER: i64 = 0x10_0000;
const DI_ITEM_VOFFSET: i64 = 0x18_0000;
const DI_ITEM_HMASK: i64 = 0x60_0000;
const DI_ITEM_LEFT: i64 = 0x20_0000;
const DI_ITEM_RIGHT: i64 = 0x40_0000;
const DI_ITEM_HOFFSET: i64 = 0x60_0000;
const DI_TEXT_ALIGN_RIGHT: i64 = 0x80_0000;
const DI_TEXT_ALIGN_CENTER: i64 = 0x100_0000;
const DI_MIRROR: i64 = 0x1000;
const DI_MIRRORY: i64 = 0x4000_0000;
const DI_DIM: i64 = 0x80;

/// Per-frame state the drawing natives read and write.
#[derive(Default)]
struct Frame {
    cmds: Vec<DrawCmd>,
    fullscreen: bool,
    snap: SbarSnapshot,
    dims: Vec<(String, PatchDims)>,
    /// `BeginHUD` / `BeginStatusBar` alpha.
    alpha: f32,
    /// `TextureID` values: index + 1 into this list (0 is no texture).
    textures: Vec<String>,
    /// The running status bar (`StatusBar` global).
    bar: ObjHandle,
    /// `gameinfo` global (`GameInfoStruct`).
    gameinfo: ObjHandle,
    /// HUD CVars the scripts read, over [`HUD_CVARS`] defaults.
    cvars: Vec<(String, i64)>,
    /// The pawn's items with their class and ancestors (lower case), for
    /// `FindInventory`.
    items: Vec<(ObjHandle, Vec<String>)>,
    /// Class defaults (`GetDefaultByType`, `.default`), lower-case class.
    defaults: Vec<(String, ObjHandle)>,
    /// Window size in pixels ([`ScriptStatusBar::set_screen_size`]); zero
    /// keeps the full-screen HUD in the 320×200 box.
    screen: (f32, f32),
    /// First command drawn since `BeginHUD`, still in HUD space.
    hud_start: Option<usize>,
    /// `BeginStatusBar` was called this frame.
    used_statusbar: bool,
}

/// UZDoom's full-screen HUD space for a `w × h` pixel window: the window
/// divided by the automatic UI scale (`GetUIScale(0)`: the whole number of
/// times 640×400 fits, at least 1), and how it maps onto the 320×200 box the
/// shell draws the HUD list in (that box is scaled to fit the window and
/// centred, so a wide window shows box coordinates below 0 and above 320).
#[derive(Clone, Copy, Debug, PartialEq)]
struct HudSpace {
    /// Virtual width / height.
    w: f32,
    h: f32,
    /// HUD unit → box unit.
    k: f32,
    /// Box coordinate of HUD (0, 0).
    x0: f32,
    y0: f32,
}

impl HudSpace {
    const BOX: Self = Self {
        w: 320.0,
        h: 200.0,
        k: 1.0,
        x0: 0.0,
        y0: 0.0,
    };

    fn for_screen((sw, sh): (f32, f32)) -> Self {
        if sw < 1.0 || sh < 1.0 {
            return Self::BOX;
        }
        let ui = (sw / 640.0).min(sh / 400.0).floor().max(1.0);
        let fit = (sw / 320.0).min(sh / 200.0);
        let (bx, by) = ((sw - 320.0 * fit) * 0.5, (sh - 200.0 * fit) * 0.5);
        Self {
            w: sw / ui,
            h: sh / ui,
            k: ui / fit,
            x0: -bx / fit,
            y0: -by / fit,
        }
    }

    fn pt(self, v: Vec2) -> Vec2 {
        Vec2::new(self.x0 + v.x * self.k, self.y0 + v.y * self.k)
    }

    fn rect(self, r: crate::draw::Rect) -> crate::draw::Rect {
        crate::draw::Rect {
            x: self.x0 + r.x * self.k,
            y: self.y0 + r.y * self.k,
            w: r.w * self.k,
            h: r.h * self.k,
        }
    }
}

fn hud_space() -> HudSpace {
    FRAME.with(|fr| HudSpace::for_screen(fr.borrow().screen))
}

/// Move the commands drawn since `BeginHUD` from HUD space into the box.
/// A picture at its own size gets that size spelled out (scaled); one whose
/// size is not known yet is left out this frame instead of drawn at the
/// wrong size.
fn finish_hud_commands(fr: &mut Frame) {
    let Some(start) = fr.hud_start.take() else {
        return;
    };
    let sp = HudSpace::for_screen(fr.screen);
    if sp == HudSpace::BOX || start >= fr.cmds.len() {
        return;
    }
    let tail: Vec<DrawCmd> = fr.cmds.drain(start..).collect();
    for mut cmd in tail {
        match &mut cmd {
            DrawCmd::Fill { rect, .. } => *rect = sp.rect(*rect),
            DrawCmd::Text { pos, scale, .. } => {
                *pos = sp.pt(*pos);
                *scale *= sp.k;
            }
            DrawCmd::Patch {
                name,
                pos,
                size,
                apply_offsets,
                corners,
                ..
            } => {
                let d = fr
                    .dims
                    .iter()
                    .find(|(n, _)| n.eq_ignore_ascii_case(name))
                    .map(|(_, d)| *d);
                let natural = size.x <= 0.0 || size.y <= 0.0;
                let Some(d) = d.or(if natural {
                    None
                } else {
                    Some(PatchDims::default())
                }) else {
                    continue;
                };
                let (dw, dh) = if natural {
                    (d.w, d.h)
                } else {
                    (size.x, size.y)
                };
                let mut p = *pos;
                if *apply_offsets && d.w > 0.0 && d.h > 0.0 {
                    p.x -= d.left * dw / d.w;
                    p.y -= d.top * dh / d.h;
                    *apply_offsets = false;
                }
                *pos = sp.pt(p);
                *size = Vec2::new(dw * sp.k, dh * sp.k);
                for c in corners.iter_mut() {
                    *c = Vec2::new(c.x * sp.k, c.y * sp.k);
                }
            }
            DrawCmd::Clip { rect } => *rect = rect.map(|r| sp.rect(r)),
            DrawCmd::Line {
                a, b, thickness, ..
            } => {
                *a = sp.pt(*a);
                *b = sp.pt(*b);
                *thickness *= sp.k;
            }
            DrawCmd::FlatPoly { edges, tex, .. } => {
                for e in edges.iter_mut() {
                    *e = [sp.pt(e[0]), sp.pt(e[1])];
                }
                // texel = t·hud = t·(box − x0)/k
                let inv = 1.0 / sp.k;
                let (t0, t1, t3, t4) = (tex[0] * inv, tex[1] * inv, tex[3] * inv, tex[4] * inv);
                tex[2] -= t0 * sp.x0 + t1 * sp.y0;
                tex[5] -= t3 * sp.x0 + t4 * sp.y0;
                tex[0] = t0;
                tex[1] = t1;
                tex[3] = t3;
                tex[4] = t4;
            }
        }
        fr.cmds.push(cmd);
    }
}

/// UZDoom's defaults for the HUD CVars `AltHud` reads (`shared_hud.cpp`);
/// the shell passes its values in with [`ScriptStatusBar::set_cvar`].
pub const HUD_CVARS: &[(&str, i64)] = &[
    ("hud_health_red", 25),
    ("hud_health_yellow", 50),
    ("hud_health_green", 100),
    ("hud_armor_red", 25),
    ("hud_armor_yellow", 50),
    ("hud_armor_green", 100),
    ("hud_ammo_red", 25),
    ("hud_ammo_yellow", 50),
    ("hud_ammo_order", 0),
    ("hud_berserk_health", 1),
    ("hud_showweapons", 1),
    ("hud_showammo", 2),
    ("hud_showtime", 0),
    ("hud_showtimestat", 0),
    ("hud_timecolor", 5),
    ("hud_showlag", 0),
    ("hud_swaphealtharmor", 0),
    ("hud_showsecrets", 1),
    ("hud_showmonsters", 1),
    ("hud_showitems", 0),
    ("hud_showstats", 0),
    ("hud_showscore", 0),
    ("hud_showangles", 0),
    ("hudcolor_titl", 10),
    ("hudcolor_time", 6),
    ("hudcolor_ltim", 8),
    ("hudcolor_ttim", 5),
    ("hudcolor_xyco", 3),
    ("hudcolor_statnames", 6),
    ("hudcolor_stats", 3),
    ("map_point_coordinates", 1),
    ("am_showtime", 1),
    ("am_showtotaltime", 0),
    ("am_showepisode", 0),
    ("am_showcluster", 0),
    ("idmypos", 0),
    ("deathmatch", 0),
];

/// A HUD CVar read as a bare name (`hud_health_red`).
fn hud_cvar(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
    let name = ctx.play.native_name.to_ascii_lowercase();
    let v = FRAME
        .with(|fr| {
            fr.borrow()
                .cvars
                .iter()
                .find(|(k, _)| *k == name)
                .map(|(_, v)| *v)
        })
        .or_else(|| HUD_CVARS.iter().find(|(k, _)| *k == name).map(|(_, v)| *v))
        .unwrap_or(0);
    ret_i(regs, v);
    Ok(())
}

fn global_statusbar(_: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
    let h = FRAME.with(|fr| fr.borrow().bar);
    ret_a(regs, h);
    Ok(())
}

fn global_gameinfo(_: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
    let h = FRAME.with(|fr| fr.borrow().gameinfo);
    ret_a(regs, h);
    Ok(())
}

// `DTA_*` tags the alt HUD passes to `Screen.DrawText` / `DrawTexture`.
const DTA_ALPHA: i64 = 1_073_746_827;
const DTA_DEST_WIDTH: i64 = 1_073_746_825;
const DTA_DEST_HEIGHT: i64 = 1_073_746_826;
const DTA_DEST_WIDTH_F: i64 = 1_073_746_861;
const DTA_DEST_HEIGHT_F: i64 = 1_073_746_862;
const DTA_CENTER_OFFSET: i64 = 1_073_746_844;
const DTA_CENTER_BOTTOM_OFFSET: i64 = 1_073_746_845;
const DTA_VIRTUAL_WIDTH: i64 = 1_073_746_840;
const DTA_VIRTUAL_HEIGHT: i64 = 1_073_746_841;
const DTA_VIRTUAL_WIDTH_F: i64 = 1_073_746_865;
const DTA_VIRTUAL_HEIGHT_F: i64 = 1_073_746_866;
const DTA_SPACING: i64 = 1_073_746_880;
const DTA_MONOSPACE: i64 = 1_073_746_881;
const DTA_SCALE_X: i64 = 1_073_746_884;
const DTA_SCALE_Y: i64 = 1_073_746_885;

/// One `vararg` argument as the compiler passed it.
#[derive(Clone, Copy, Debug, PartialEq)]
enum VarArg {
    I(i64),
    F(f64),
    S(usize),
}

impl VarArg {
    fn as_f(self) -> f64 {
        match self {
            Self::I(v) => v as f64,
            Self::F(v) => v,
            Self::S(_) => 0.0,
        }
    }
    #[allow(clippy::cast_possible_truncation)]
    fn as_i(self) -> i64 {
        match self {
            Self::I(v) => v,
            Self::F(v) => v as i64,
            Self::S(_) => 0,
        }
    }
}

/// The extra arguments of a `vararg` native in call order
/// ([`room_zvm::vararg_values`]).
fn varargs(
    ctx: &ZvmCtx<'_>,
    regs: &NativeRegs<'_>,
    ni: usize,
    nf: usize,
    ns: usize,
) -> Vec<VarArg> {
    room_zvm::vararg_values(ctx, regs, ni, nf, ns)
        .into_iter()
        .enumerate()
        .map(|(k, v)| match v {
            Value::F(x) => VarArg::F(x),
            Value::S(_) => VarArg::S(k),
            Value::I(x) => VarArg::I(x),
            Value::A(_) => VarArg::I(0),
        })
        .collect()
}

/// Draw options from `DTA_*` tag / value pairs.
#[derive(Clone, Copy, Debug)]
struct DrawTags {
    virt: (f64, f64),
    alpha: f64,
    dest: (Option<f64>, Option<f64>),
    scale: (f64, f64),
    center: bool,
    center_bottom: bool,
    spacing: f64,
    mono: bool,
}

fn draw_tags(args: &[VarArg]) -> DrawTags {
    let mut t = DrawTags {
        virt: (320.0, 200.0),
        alpha: 1.0,
        dest: (None, None),
        scale: (1.0, 1.0),
        center: false,
        center_bottom: false,
        spacing: 0.0,
        mono: false,
    };
    let mut it = args.iter().copied();
    while let Some(tag) = it.next() {
        let tag = tag.as_i();
        if tag == 0 {
            break;
        }
        let Some(v) = it.next() else {
            break;
        };
        match tag {
            DTA_VIRTUAL_WIDTH | DTA_VIRTUAL_WIDTH_F => t.virt.0 = v.as_f().max(1.0),
            DTA_VIRTUAL_HEIGHT | DTA_VIRTUAL_HEIGHT_F => t.virt.1 = v.as_f().max(1.0),
            DTA_ALPHA => t.alpha = v.as_f(),
            DTA_DEST_WIDTH | DTA_DEST_WIDTH_F => t.dest.0 = Some(v.as_f()),
            DTA_DEST_HEIGHT | DTA_DEST_HEIGHT_F => t.dest.1 = Some(v.as_f()),
            DTA_SCALE_X => t.scale.0 = v.as_f(),
            DTA_SCALE_Y => t.scale.1 = v.as_f(),
            DTA_CENTER_OFFSET => t.center = v.as_i() != 0,
            DTA_CENTER_BOTTOM_OFFSET => t.center_bottom = v.as_i() != 0,
            DTA_SPACING => t.spacing = v.as_f(),
            DTA_MONOSPACE => t.mono = v.as_i() != 0,
            _ => {}
        }
    }
    t
}

/// Virtual coordinates → Room's 320×200 HUD space.
#[allow(clippy::cast_possible_truncation)]
fn virt_to_hud(t: &DrawTags, x: f64, y: f64) -> (f32, f32) {
    ((x * 320.0 / t.virt.0) as f32, (y * 200.0 / t.virt.1) as f32)
}

fn patch_cmd(
    name: String,
    pos: Vec2,
    size: Vec2,
    alpha: f64,
    recolor: Option<[[f32; 3]; 2]>,
) -> DrawCmd {
    #[allow(clippy::cast_possible_truncation)]
    DrawCmd::Patch {
        name,
        pos,
        size,
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
        alpha: (alpha as f32).clamp(0.0, 1.0),
        recolor,
        corners: [Vec2::new(0.0, 0.0); 4],
    }
}

/// `Screen.DrawText(font, color, x, y, text, tags…)`.
fn screen_draw_text(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
    let font = object_name(ctx, arg_a(regs, 0));
    let color = arg_i(regs, 0, 11);
    let (x, y) = (arg_f(regs, 0, 0.0), arg_f(regs, 1, 0.0));
    let text = arg_s(ctx, regs, 0);
    let t = draw_tags(&varargs(ctx, regs, 1, 2, 1));
    let (sx, sy) = (320.0 / t.virt.0, 200.0 / t.virt.1);
    let recolor = crate::acs_hud::ramp(i32::try_from(color).unwrap_or(11));
    let mut pen = x;
    for ch in strip_color_escapes(&text).chars() {
        let natural = f64::from(advance(&font, ch, 0));
        let cell = if t.mono {
            t.spacing.max(natural)
        } else {
            natural + t.spacing
        };
        if let Some(g) = glyph(&font, ch) {
            if let Some(d) = dims_of(&g) {
                // `MONO_CellCenter`: each glyph centred in its cell.
                let gx = if t.mono {
                    pen + (cell - f64::from(d.w)) / 2.0
                } else {
                    pen
                };
                let (hx, hy) = virt_to_hud(&t, gx, y);
                #[allow(clippy::cast_possible_truncation)]
                let size = Vec2::new(d.w * sx as f32, d.h * sy as f32);
                FRAME.with(|fr| {
                    fr.borrow_mut().cmds.push(patch_cmd(
                        g,
                        Vec2::new(hx, hy),
                        size,
                        t.alpha,
                        recolor,
                    ));
                });
            }
        }
        pen += cell;
    }
    Ok(())
}

/// `\c` colour escapes (`\cX`, `\c[Name]`) are not drawn.
fn strip_color_escapes(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut it = text.chars().peekable();
    while let Some(c) = it.next() {
        if c == '\x1c' {
            if it.peek() == Some(&'[') {
                for d in it.by_ref() {
                    if d == ']' {
                        break;
                    }
                }
            } else {
                it.next();
            }
            continue;
        }
        out.push(c);
    }
    out
}

/// `Screen.DrawTexture(tex, animate, x, y, tags…)`.
fn screen_draw_texture(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
    let name = texture_name(arg_i(regs, 0, 0));
    if name.is_empty() || name.eq_ignore_ascii_case("TNT1A0") {
        return Ok(());
    }
    let (x, y) = (arg_f(regs, 0, 0.0), arg_f(regs, 1, 0.0));
    let t = draw_tags(&varargs(ctx, regs, 2, 2, 0));
    let d = dims_of(&name).unwrap_or_default();
    let w = t.dest.0.unwrap_or(f64::from(d.w) * t.scale.0);
    let h = t.dest.1.unwrap_or(f64::from(d.h) * t.scale.1);
    // Placement: centre-bottom, centre, else the patch offsets (scaled).
    let (left, top) = if t.center_bottom {
        (w / 2.0, h)
    } else if t.center {
        (w / 2.0, h / 2.0)
    } else {
        let kx = if d.w > 0.0 { w / f64::from(d.w) } else { 1.0 };
        let ky = if d.h > 0.0 { h / f64::from(d.h) } else { 1.0 };
        (f64::from(d.left) * kx, f64::from(d.top) * ky)
    };
    let (hx, hy) = virt_to_hud(&t, x - left, y - top);
    let (hw, hh) = virt_to_hud(&t, w, h);
    FRAME.with(|fr| {
        fr.borrow_mut().cmds.push(patch_cmd(
            name,
            Vec2::new(hx, hy),
            Vec2::new(hw, hh),
            t.alpha,
            None,
        ));
    });
    Ok(())
}

/// A `TextureID` (an int in this VM) for `name`.
fn texture_id(name: &str) -> i64 {
    if name.is_empty() {
        return 0;
    }
    FRAME.with(|fr| {
        let mut fr = fr.borrow_mut();
        if let Some(k) = fr
            .textures
            .iter()
            .position(|t| t.eq_ignore_ascii_case(name))
        {
            return k as i64 + 1;
        }
        fr.textures.push(name.to_owned());
        fr.textures.len() as i64
    })
}

fn texture_name(id: i64) -> String {
    FRAME.with(|fr| {
        usize::try_from(id - 1)
            .ok()
            .and_then(|k| fr.borrow().textures.get(k).cloned())
            .unwrap_or_default()
    })
}

thread_local! {
    static FRAME: RefCell<Frame> = RefCell::new(Frame::default());
}

fn dims_of(name: &str) -> Option<PatchDims> {
    FRAME.with(|fr| {
        fr.borrow()
            .dims
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, d)| *d)
    })
}

/// `(bank, slot)` of `field` on `class` or an ancestor.
fn field_slot(program: &CompiledProgram, class: &str, field: &str) -> Option<(u8, usize)> {
    let key = field.to_ascii_lowercase();
    let mut cur = program
        .classes
        .iter()
        .find(|c| c.name.eq_ignore_ascii_case(class))?;
    for _ in 0..64 {
        if let Some(&(bank, idx)) = cur.fields.get(&key) {
            return Some((bank, usize::from(idx)));
        }
        if cur.parent.is_empty() {
            return None;
        }
        let parent = cur.parent.clone();
        cur = program
            .classes
            .iter()
            .find(|c| c.name.eq_ignore_ascii_case(&parent))?;
    }
    None
}

/// The status bar's mirrored player objects ([`ScriptStatusBar`]): one per
/// key (`"mo"`, `"item:clip"`, …), reused while its key is.
#[derive(Default)]
struct Mirror {
    cache: Vec<(String, ObjHandle)>,
    used: Vec<String>,
}

impl Mirror {
    /// The object for `key`, typed as the first class of `chain` the
    /// program knows; made on first use.
    fn obtain(&mut self, vm: &mut Zvm, key: &str, chain: &[&str]) -> ObjHandle {
        if !self.used.iter().any(|k| k == key) {
            self.used.push(key.to_owned());
        }
        if let Some((_, h)) = self.cache.iter().find(|(k, _)| k == key)
            && vm.heap.get(*h).is_some()
        {
            return *h;
        }
        self.cache.retain(|(k, _)| k != key);
        let h = new_typed(vm, chain);
        self.cache.push((key.to_owned(), h));
        h
    }

    /// Start a frame: nothing used yet.
    fn begin(&mut self) {
        self.used.clear();
    }

    /// End a frame: free the objects it did not use.
    fn end(&mut self, vm: &mut Zvm) {
        let used = std::mem::take(&mut self.used);
        self.cache.retain(|(k, h)| {
            let keep = used.iter().any(|u| u == k);
            if !keep {
                vm.heap.destroy(*h);
            }
            keep
        });
        self.used = used;
    }
}

/// A typed object of `class` (or the first existing fallback).
fn new_typed(vm: &mut Zvm, classes: &[&str]) -> ObjHandle {
    for class in classes {
        if let Some((id, layout)) = vm
            .program
            .classes
            .iter()
            .enumerate()
            .find(|(_, c)| c.name.eq_ignore_ascii_case(class))
        {
            let (n_i, n_a, n_s, n_f) = (layout.n_i, layout.n_a, layout.n_s, layout.n_f);
            let (arr_i, arr_s) = (layout.array_i.clone(), layout.array_s.clone());
            #[allow(clippy::cast_possible_truncation)]
            let h = vm.heap.new_object_f(id as u32, n_i, n_a, n_s, n_f);
            // `Array<…>` members, as `new` (`Op::NewObj`) makes them.
            let mut made = Vec::new();
            for &k in &arr_i {
                made.push((k, vm.heap.new_array_i()));
            }
            for &k in &arr_s {
                made.push((k, vm.heap.new_array_s()));
            }
            if let Some(ObjKind::Object { fields_a, .. }) = vm.heap.get_mut(h) {
                for (k, arr) in made {
                    if let Some(slot) = fields_a.get_mut(usize::from(k)) {
                        *slot = arr;
                    }
                }
            }
            return h;
        }
    }
    vm.heap.new_object(0, 8, 8, 4)
}

fn class_of(vm: &Zvm, h: ObjHandle) -> Option<String> {
    match vm.heap.get(h)? {
        ObjKind::Object { class_id, .. } => vm
            .program
            .classes
            .get(*class_id as usize)
            .map(|c| c.name.clone()),
        _ => None,
    }
}

fn set_int(vm: &mut Zvm, h: ObjHandle, field: &str, v: i64) {
    let Some(class) = class_of(vm, h) else {
        return;
    };
    if let Some((0, idx)) = field_slot(&vm.program, &class, field) {
        if let Some(ObjKind::Object { fields_i, .. }) = vm.heap.get_mut(h) {
            if fields_i.len() <= idx {
                fields_i.resize(idx + 1, 0);
            }
            fields_i[idx] = v;
        }
    }
}

/// A float or vector field: `v` fills its consecutive slots.
fn set_floats(vm: &mut Zvm, h: ObjHandle, field: &str, v: &[f64]) {
    let Some(class) = class_of(vm, h) else {
        return;
    };
    if let Some((3, idx)) = field_slot(&vm.program, &class, field) {
        if let Some(ObjKind::Object { fields_f, .. }) = vm.heap.get_mut(h) {
            if fields_f.len() < idx + v.len() {
                fields_f.resize(idx + v.len(), 0.0);
            }
            fields_f[idx..idx + v.len()].copy_from_slice(v);
        }
    }
}

fn set_str(vm: &mut Zvm, h: ObjHandle, field: &str, v: &str) {
    let Some(class) = class_of(vm, h) else {
        return;
    };
    if let Some((2, idx)) = field_slot(&vm.program, &class, field) {
        let sh = vm.heap.intern_string(v);
        if let Some(ObjKind::Object { fields_s, .. }) = vm.heap.get_mut(h) {
            if fields_s.len() <= idx {
                fields_s.resize(idx + 1, sh);
            }
            fields_s[idx] = sh;
        }
    }
}

/// `class` and its ancestors, lower case (just `class` when unknown).
fn lineage(program: &CompiledProgram, class: &str) -> Vec<String> {
    let mut out = vec![class.to_ascii_lowercase()];
    let mut cur = class.to_owned();
    for _ in 0..64 {
        let Some(c) = program
            .classes
            .iter()
            .find(|c| c.name.eq_ignore_ascii_case(&cur))
        else {
            break;
        };
        if c.parent.is_empty() {
            break;
        }
        cur = c.parent.clone();
        out.push(cur.to_ascii_lowercase());
    }
    out
}

fn get_obj(vm: &Zvm, h: ObjHandle, field: &str) -> Option<ObjHandle> {
    let class = class_of(vm, h)?;
    let (1, idx) = field_slot(&vm.program, &class, field)? else {
        return None;
    };
    match vm.heap.get(h)? {
        ObjKind::Object { fields_a, .. } => fields_a.get(idx).copied().filter(|o| !o.is_null()),
        _ => None,
    }
}

fn set_obj(vm: &mut Zvm, h: ObjHandle, field: &str, v: ObjHandle) {
    let Some(class) = class_of(vm, h) else {
        return;
    };
    if let Some((1, idx)) = field_slot(&vm.program, &class, field) {
        if let Some(ObjKind::Object { fields_a, .. }) = vm.heap.get_mut(h) {
            if fields_a.len() <= idx {
                fields_a.resize(idx + 1, ObjHandle::NULL);
            }
            fields_a[idx] = v;
        }
    }
}

/// Most-derived definition of `method` on `class`'s chain.
fn find_method(program: &CompiledProgram, class: &str, method: &str) -> Option<String> {
    let mut cur = class.to_owned();
    for _ in 0..64 {
        let name = format!("{cur}.{method}");
        if program
            .functions
            .iter()
            .any(|f| f.name.eq_ignore_ascii_case(&name))
        {
            return program
                .functions
                .iter()
                .find(|f| f.name.eq_ignore_ascii_case(&name))
                .map(|f| f.name.clone());
        }
        let parent = program
            .classes
            .iter()
            .find(|c| c.name.eq_ignore_ascii_case(&cur))?
            .parent
            .clone();
        if parent.is_empty() {
            return None;
        }
        cur = parent;
    }
    None
}

/// Patches the alt HUD may draw beyond the status bar's set.
const ALTHUD_PATCHES: &[&str] = &[
    "MEDIA0", "PSTRA0", "ARM1A0", "ARM2A0", "HU_FRAGS", "INVGEML1", "INVGEMR1", "CLIPA0", "SHELA0",
    "ROCKA0", "CELLA0", "PISTA0", "SHOTA0", "SGN2A0", "MGUNA0", "LAUNA0", "PLASA0", "BFUGA0",
    "CSAWA0", "BKEYA0", "YKEYA0", "RKEYA0", "BSKUA0", "YSKUA0", "RSKUA0",
];

/// A running ZScript status bar.
pub struct ScriptStatusBar {
    vm: Zvm,
    class: String,
    bar: ObjHandle,
    init_done: bool,
    /// Objects mirrored for the player, kept from frame to frame (by role
    /// and class) so the handles a script keeps in its own fields between
    /// `Tick`s stay valid; ones a frame no longer needs are freed.
    mirror: Mirror,
    /// The last [`Self::draw`] drew a status bar (`BeginStatusBar`), not
    /// only a full-screen HUD.
    drew_status_bar: bool,
    /// The pawn mirrored for the current frame.
    mo: ObjHandle,
    /// `InvFirst` the script left on the pawn, carried to the next frame.
    inv_first: Option<String>,
    /// `CPlayer` mirrored for the current frame.
    player: ObjHandle,
    /// The alternative HUD object (`AltHud` or `GameInfo.AltHudClass`),
    /// made on first use.
    althud: ObjHandle,
    /// Class [`Self::draw_althud`] runs.
    althud_class: String,
    /// Last trap from `Init` / `Draw`, if any.
    pub last_error: Option<String>,
    /// Pictures the bar drew that are not stock HUD patches (`BARBACK2`):
    /// their sizes are asked for every frame so `DI_ITEM_*` alignment works.
    seen_patches: Vec<String>,
    /// Window size in pixels for the full-screen HUD ([`Self::set_screen_size`]).
    screen: (f32, f32),
    /// Pictures drawn whose size was unknown, for the host to load.
    unknown_patches: Vec<String>,
    /// Game tic of the last [`Self::set_game_tic`].
    game_tic: Option<u32>,
    /// `Tick` calls owed to the bar (UZDoom ticks the status bar every tic).
    pending_tics: u32,
}

impl ScriptStatusBar {
    /// Load `class` from `program`. Fails when the class is missing or does
    /// not have a `Draw`.
    pub fn new(mut program: CompiledProgram, class: &str) -> Result<Self, String> {
        // Library functions whose engine parts this VM lacks run as host
        // natives: `DrawBar` (clip rectangles, `TexMan.GetScaledSize`) and
        // `GetAirTime` (`Level.airsupply`).
        program.route_to_host_actions(&["BaseStatusBar"], &["DrawBar", "GetAirTime"]);
        if !program
            .classes
            .iter()
            .any(|c| c.name.eq_ignore_ascii_case(class))
        {
            return Err(format!("status bar class {class} not found"));
        }
        if find_method(&program, class, "Draw").is_none() {
            return Err(format!("status bar class {class} has no Draw"));
        }
        let slots = actor_slots(&program);
        let mut vm = Zvm::new_with_stubs(program);
        // Imports are already linked by id; replacing keeps the ids.
        register(&mut vm);
        vm.play.actor_slots = slots;
        let bar = new_typed(&mut vm, &[class]);
        let class = class_of(&vm, bar).unwrap_or_else(|| class.to_owned());
        Ok(Self {
            vm,
            class,
            bar,
            init_done: false,
            mirror: Mirror::default(),
            drew_status_bar: true,
            mo: ObjHandle::NULL,
            inv_first: None,
            player: ObjHandle::NULL,
            althud: ObjHandle::NULL,
            althud_class: "AltHud".to_owned(),
            last_error: None,
            seen_patches: Vec::new(),
            screen: (0.0, 0.0),
            unknown_patches: Vec::new(),
            game_tic: None,
            pending_tics: 0,
        })
    }

    /// A mod CVar's value (CVARINFO; `CVar.GetCVar` in the bar's scripts):
    /// numbers as numbers, `true` / `false` as 1 / 0, anything else as text.
    pub fn set_mod_cvar(&mut self, name: &str, value: &str) {
        let v = value.trim();
        let play = &mut self.vm.play;
        if let Ok(i) = v.parse::<i32>() {
            play.set_cvar_int(name, i);
        } else if let Ok(f) = v.parse::<f64>() {
            play.set_cvar_float(name, f);
        } else if v.eq_ignore_ascii_case("true") {
            play.set_cvar_int(name, 1);
        } else if v.eq_ignore_ascii_case("false") {
            play.set_cvar_int(name, 0);
        } else {
            play.set_cvar_string(name, v);
        }
    }

    /// Whether the last frame drew a status bar across the bottom
    /// (`BeginStatusBar`). A bar that only ever draws a full-screen HUD
    /// (Project Brutality's) leaves the weapon at the bottom of the screen,
    /// as UZDoom's full view does. `true` before the first frame.
    #[must_use]
    pub fn drew_status_bar(&self) -> bool {
        self.drew_status_bar
    }

    /// The window's size in pixels. The full-screen HUD (`BeginHUD`) is then
    /// laid out as in UZDoom: in the window divided by the automatic UI
    /// scale, with `DI_SCREEN_RIGHT` / `BOTTOM` at the window's edges, and
    /// its commands are mapped into the 320×200 box the shell draws (outside
    /// it on a wide window). Not set: the HUD stays in the 320×200 box.
    pub fn set_screen_size(&mut self, w: f32, h: f32) {
        self.screen = (w, h);
    }

    /// The game tic now: the bar's `Tick` runs once per tic passed since the
    /// last call (at most 4 at a time), before the next [`Self::draw`]. PB's
    /// HUD copies health and armor into its own fields there.
    pub fn set_game_tic(&mut self, tic: u32) {
        match self.game_tic {
            Some(prev) if tic > prev => {
                self.pending_tics = (self.pending_tics + (tic - prev)).min(4);
            }
            None => self.pending_tics = 1,
            _ => {}
        }
        self.game_tic = Some(tic);
    }

    /// Pictures [`Self::draw`] asked for whose size the `dims` callback did
    /// not know, each reported once: load them before the next draw. The
    /// full-screen HUD (`BeginHUD` with a window size) leaves them out
    /// until then, so a shell that never loads them shows none of them.
    pub fn take_unknown_patches(&mut self) -> Vec<String> {
        std::mem::take(&mut self.unknown_patches)
    }

    /// `GameInfo` `AltHudClass` (must derive from `AltHud`; else the stock
    /// one runs, as `CreateAltHUD`).
    pub fn set_althud_class(&mut self, class: &str) {
        let derives = |vm: &Zvm, mut cur: String| {
            for _ in 0..64 {
                if cur.eq_ignore_ascii_case("AltHud") {
                    return true;
                }
                let Some(c) = vm
                    .program
                    .classes
                    .iter()
                    .find(|c| c.name.eq_ignore_ascii_case(&cur))
                else {
                    return false;
                };
                if c.parent.is_empty() {
                    return false;
                }
                cur = c.parent.clone();
            }
            false
        };
        let class = if derives(&self.vm, class.to_owned()) {
            class
        } else {
            "AltHud"
        };
        if !class.eq_ignore_ascii_case(&self.althud_class) {
            self.althud_class = class.to_owned();
            if !self.althud.is_null() {
                self.vm.heap.destroy(self.althud);
            }
            self.althud = ObjHandle::NULL;
        }
    }

    /// Patches [`Self::draw_althud`] may ask about for `snap`.
    #[must_use]
    pub fn althud_patches(snap: &SbarSnapshot) -> Vec<String> {
        let mut names = Self::wanted_patches(snap);
        names.extend(ALTHUD_PATCHES.iter().map(|n| (*n).to_owned()));
        names
    }

    /// Run one frame of the alternative HUD (`hud_althud`,
    /// `DBaseStatusBar::DrawAltHUD`): `AltHud.Draw(CPlayer, w, h)` in a
    /// `w × h` virtual screen (UZDoom: `640 / hud_scalefactor` wide, height by
    /// the screen's aspect ratio, ×5/6 with `hud_aspectscale`). The commands
    /// land in the usual 320×200 HUD space.
    pub fn draw_althud(
        &mut self,
        snap: &SbarSnapshot,
        size: (i32, i32),
        dims: &dyn Fn(&str) -> Option<PatchDims>,
        list: &mut UiDrawList,
    ) {
        if find_method(&self.vm.program, &self.althud_class, "Draw").is_none() {
            self.last_error = Some(format!("{} has no Draw", self.althud_class));
            return;
        }
        FRAME.with(|fr| {
            let mut fr = fr.borrow_mut();
            fr.cmds.clear();
            fr.fullscreen = true;
            fr.snap = snap.clone();
            fr.alpha = 1.0;
            fr.dims = Self::althud_patches(snap)
                .into_iter()
                .filter_map(|n| dims(&n).map(|d| (n, d)))
                .collect();
        });
        self.mirror_player(snap);
        if self.althud.is_null() {
            let class = self.althud_class.clone();
            self.althud = new_typed(&mut self.vm, &[&class, "AltHud"]);
            if let Some(init) = find_method(&self.vm.program, &class, "Init") {
                if let Err(e) = self
                    .vm
                    .call_method_banks(&init, self.althud, &[], &[], &[], &[])
                {
                    self.last_error = Some(format!("{init}: {e:?}"));
                }
            }
        }
        if let Some(draw) = find_method(&self.vm.program, &self.althud_class, "Draw") {
            let (w, h) = (i64::from(size.0.max(1)), i64::from(size.1.max(1)));
            if let Err(e) =
                self.vm
                    .call_method_banks(&draw, self.althud, &[w, h], &[], &[], &[self.player])
            {
                self.last_error = Some(format!("{draw}: {e:?}"));
            }
        }
        FRAME.with(|fr| list.cmds.append(&mut fr.borrow_mut().cmds));
    }

    /// Patch names [`Self::draw`] asks `dims` about for `snap`: load these
    /// into the patch cache first.
    #[must_use]
    pub fn wanted_patches(snap: &SbarSnapshot) -> Vec<String> {
        let mut names: Vec<String> = [
            "STBAR", "STTPRCNT", "STTMINUS", "STARMS", "STFBANY", "MEDIA0", "PSTRA0",
        ]
        .iter()
        .map(|k| (*k).to_owned())
        .collect();
        for d in 0..10 {
            names.push(format!("STTNUM{d}"));
            names.push(format!("STYSNUM{d}"));
            names.push(format!("STGNUM{d}"));
        }
        // Console font (STCFN033–STCFN095) for `SmallFont` strings.
        for c in 33..=95 {
            names.push(format!("STCFN{c:03}"));
        }
        for k in 0..9 {
            names.push(format!("STKEYS{k}"));
        }
        // Inventory bar box, selector and scroll arrows.
        for k in [
            "ARTIBOX", "SELECTBO", "INVGEML1", "INVGEML2", "INVGEMR1", "INVGEMR2",
        ] {
            names.push(k.to_owned());
        }
        names.push(snap.face.clone());
        for (_, icon) in snap.icons.iter().chain(snap.spawn_sprites.iter()) {
            names.push(icon.clone());
        }
        names
    }

    /// Set a HUD CVar the scripts read (`hud_showweapons`, …); others keep
    /// UZDoom's defaults.
    pub fn set_cvar(&mut self, name: &str, value: i64) {
        let name = name.to_ascii_lowercase();
        FRAME.with(|fr| {
            let mut fr = fr.borrow_mut();
            fr.cvars.retain(|(k, _)| *k != name);
            fr.cvars.push((name, value));
        });
    }

    /// The running class.
    #[must_use]
    pub fn class(&self) -> &str {
        &self.class
    }

    /// Run one frame of `Draw` and append its commands to `list`.
    pub fn draw(
        &mut self,
        snap: &SbarSnapshot,
        fullscreen: bool,
        tic_frac: f64,
        dims: &dyn Fn(&str) -> Option<PatchDims>,
        list: &mut UiDrawList,
    ) {
        FRAME.with(|fr| {
            let mut fr = fr.borrow_mut();
            fr.cmds.clear();
            fr.fullscreen = fullscreen;
            fr.snap = snap.clone();
            fr.alpha = 1.0;
            fr.screen = self.screen;
            fr.hud_start = None;
            fr.used_statusbar = false;
            // Patches the natives will ask about, resolved up front.
            let names = Self::wanted_patches(snap);
            fr.dims = names
                .into_iter()
                .chain(self.seen_patches.iter().cloned())
                .filter_map(|n| dims(&n).map(|d| (n, d)))
                .collect();
        });
        self.mirror_player(snap);
        if !self.init_done {
            self.init_done = true;
            if let Some(init) = find_method(&self.vm.program, &self.class, "Init") {
                if let Err(e) = self
                    .vm
                    .call_method_banks(&init, self.bar, &[], &[], &[], &[])
                {
                    self.last_error = Some(format!("{init}: {e:?}"));
                }
            }
            // UZDoom calls `NewGame` when play starts (PB starts its visor
            // animation there).
            if let Some(new_game) = find_method(&self.vm.program, &self.class, "NewGame") {
                if let Err(e) = self
                    .vm
                    .call_method_banks(&new_game, self.bar, &[], &[], &[], &[])
                {
                    self.last_error = Some(format!("{new_game}: {e:?}"));
                }
            }
        }
        if self.pending_tics > 0 {
            if let Some(tick) = find_method(&self.vm.program, &self.class, "Tick") {
                for _ in 0..std::mem::take(&mut self.pending_tics) {
                    if let Err(e) = self
                        .vm
                        .call_method_banks(&tick, self.bar, &[], &[], &[], &[])
                    {
                        self.last_error = Some(format!("{tick}: {e:?}"));
                        break;
                    }
                }
            }
            self.pending_tics = 0;
        }
        if let Some(draw) = find_method(&self.vm.program, &self.class, "Draw") {
            let state = if fullscreen {
                HUD_FULLSCREEN
            } else {
                HUD_STATUSBAR
            };
            if let Err(e) =
                self.vm
                    .call_method_banks(&draw, self.bar, &[state], &[tic_frac], &[], &[])
            {
                self.last_error = Some(format!("{draw}: {e:?}"));
            }
        }
        // Keep the bar's scroll position (`InvFirst`) for the next frame.
        self.inv_first = get_obj(&self.vm, self.mo, "invfirst").and_then(|h| class_of(&self.vm, h));
        FRAME.with(|fr| {
            let mut fr = fr.borrow_mut();
            // Pictures `finish_hud_commands` will leave out (size unknown).
            let pending: Vec<String> = fr
                .cmds
                .iter()
                .filter_map(|c| match c {
                    DrawCmd::Patch { name, .. } => Some(name.clone()),
                    _ => None,
                })
                .collect();
            finish_hud_commands(&mut fr);
            // Reported once each (`take_unknown_patches`): the shell loads
            // them, and a name that does not load is not asked about again.
            let drawn = fr.cmds.iter().filter_map(|c| match c {
                DrawCmd::Patch { name, .. } => Some(name.clone()),
                _ => None,
            });
            let names: Vec<String> = pending.into_iter().chain(drawn).collect();
            for name in names {
                let known = fr.dims.iter().any(|(n, _)| n.eq_ignore_ascii_case(&name));
                if !known
                    && !self
                        .seen_patches
                        .iter()
                        .any(|n| n.eq_ignore_ascii_case(&name))
                {
                    self.seen_patches.push(name.clone());
                    self.unknown_patches.push(name);
                }
            }
            self.drew_status_bar = fr.used_statusbar;
            list.cmds.append(&mut fr.cmds);
        });
    }

    /// `CPlayer`, its pawn and ready weapon for this frame.
    fn mirror_player(&mut self, snap: &SbarSnapshot) {
        let vm = &mut self.vm;
        let made = &mut self.mirror;
        made.begin();
        let player = made.obtain(vm, "player", &["PlayerInfo"]);
        // The pawn typed as its class or nearest script ancestor, so a mod's
        // `PlayerPawnBase(plr)` cast holds.
        let mut pawn_chain: Vec<&str> = Vec::new();
        let mut pawn_key = String::from("mo");
        if let Some(pc) = snap.player_class.as_deref() {
            pawn_chain.push(pc);
            if let Some((_, a)) = snap
                .ancestors
                .iter()
                .find(|(c, _)| c.eq_ignore_ascii_case(pc))
            {
                pawn_chain.extend(a.iter().map(String::as_str));
            }
            pawn_key = format!("mo:{}", pc.to_ascii_lowercase());
        }
        pawn_chain.extend(["DoomPlayer", "PlayerPawn", "Actor"]);
        let mo = made.obtain(vm, &pawn_key, &pawn_chain);
        let [angle, pitch, roll, vx, vy, vz, z, floor] = snap.pose;
        set_floats(vm, mo, "angle", &[angle]);
        set_floats(vm, mo, "pitch", &[pitch]);
        set_floats(vm, mo, "roll", &[roll]);
        set_floats(vm, mo, "vel", &[vx, vy, vz]);
        set_floats(vm, mo, "floorz", &[floor]);
        // `pos` keeps x / y at 0: the HUD reads its height only.
        set_floats(vm, mo, "pos", &[0.0, 0.0, z]);
        set_obj(vm, mo, "player", player);
        // Field of view: UZDoom's default 90 (PB's zoom offset reads
        // `fov - DesiredFov`).
        set_floats(vm, player, "fov", &[90.0]);
        set_floats(vm, player, "desiredfov", &[90.0]);
        // Links rebuilt below; clear what the last frame left.
        for f in ["inv", "invsel", "invfirst"] {
            set_obj(vm, mo, f, ObjHandle::NULL);
        }
        set_obj(vm, player, "readyweapon", ObjHandle::NULL);
        set_int(vm, player, "health", i64::from(snap.health));
        set_int(vm, player, "fragcount", i64::from(snap.frags));
        set_int(vm, mo, "health", i64::from(snap.health));
        // `GetMaxHealth()` reads `MaxHealth` (`Player.MaxHealth`, 100 for
        // Doom; 0 would fall to the DEHACKED global, which is not mirrored).
        // PB scales its health bar by it.
        set_int(vm, mo, "maxhealth", 100);
        set_obj(vm, player, "mo", mo);
        // `mo.cursector` with a light level and colormap (PB tints its visor
        // by them); the snapshot carries no sector, so a mid light.
        let sector = made.obtain(vm, "sector", &["Sector"]);
        set_int(vm, sector, "lightlevel", 192);
        let colormap = made.obtain(vm, "colormap", &["FColormap"]);
        set_int(vm, colormap, "lightcolor", 0x00ff_ffff);
        set_obj(vm, sector, "colormap", colormap);
        set_obj(vm, mo, "cursector", sector);
        // `CPlayer.weapons` (a non-null receiver for the slot natives).
        let slots = made.obtain(vm, "slots", &["WeaponSlots"]);
        set_obj(vm, player, "weapons", slots);
        let icon_of = |class: &str| {
            snap.icons
                .iter()
                .find(|(c, _)| c.eq_ignore_ascii_case(class))
                .map(|(_, i)| i.as_str())
        };
        let ancestors_of = |class: &str| -> &[String] {
            snap.ancestors
                .iter()
                .find(|(c, _)| c.eq_ignore_ascii_case(class))
                .map_or(&[], |(_, a)| a.as_slice())
        };
        let weapon_of = |class: &str| {
            snap.weapons
                .iter()
                .find(|w| w.class.eq_ignore_ascii_case(class))
        };
        let ammo_max = |class: &str| {
            snap.inventory
                .iter()
                .find(|(c, _, _)| c.eq_ignore_ascii_case(class))
                .map(|(_, _, m)| *m)
                .or_else(|| {
                    snap.max_amounts
                        .iter()
                        .find(|(c, _)| c.eq_ignore_ascii_case(class))
                        .map(|(_, m)| *m)
                })
        };
        // One object per class: typed, with its icon, ammo types and amounts.
        let item = |vm: &mut Zvm, made: &mut Mirror, class: &str, owned: bool| -> ObjHandle {
            // A class the script program lacks (a DECORATE weapon such as
            // PB's `PB_DMR`) still gets the right base, so `ReadyWeapon.Ammo1`
            // reads a weapon's field.
            let is_weapon = weapon_of(class).is_some()
                || snap
                    .ready_weapon
                    .as_deref()
                    .is_some_and(|w| w.eq_ignore_ascii_case(class));
            let base = if is_weapon { "Weapon" } else { "Ammo" };
            let mut chain: Vec<&str> = vec![class];
            chain.extend(ancestors_of(class).iter().map(String::as_str));
            chain.extend([base, "Inventory"]);
            let key = format!(
                "{}:{}",
                if owned { "item" } else { "default" },
                class.to_ascii_lowercase()
            );
            let h = made.obtain(vm, &key, &chain);
            if owned {
                set_obj(vm, h, "inv", ObjHandle::NULL);
                set_int(vm, h, "binvbar", 0);
            }
            if let Some(icon) = icon_of(class) {
                set_int(vm, h, "icon", texture_id(icon));
            }
            if DOOM_KEYS.iter().any(|k| k.eq_ignore_ascii_case(class)) {
                if let Some((_, spr)) = snap
                    .spawn_sprites
                    .iter()
                    .find(|(c, _)| c.eq_ignore_ascii_case(class))
                {
                    // Drawn like UZDoom's alt HUD: the spawn sprite, not the icon.
                    set_int(vm, h, "althudicon", texture_id(spr));
                }
            }
            if let Some(w) = weapon_of(class) {
                if let Some(a) = &w.ammo1 {
                    set_str(vm, h, "ammotype1", a);
                }
                if let Some(a) = &w.ammo2 {
                    set_str(vm, h, "ammotype2", a);
                }
            }
            if let Some((_, amount, max)) = snap
                .inventory
                .iter()
                .find(|(c, _, _)| c.eq_ignore_ascii_case(class))
            {
                if owned {
                    set_int(vm, h, "amount", i64::from(*amount));
                }
                set_int(vm, h, "maxamount", i64::from(*max));
            } else if let Some(max) = ammo_max(class) {
                set_int(vm, h, "maxamount", i64::from(max));
            }
            h
        };
        // The pawn's inventory as the `Inv` chain the stdlib walks, each
        // item owned by the pawn; `FindInventory` searches the same items.
        let mut items = Vec::new();
        let mut prev = mo;
        for (class, _, _) in &snap.inventory {
            let h = item(vm, made, class, true);
            set_obj(vm, h, "owner", mo);
            set_obj(vm, prev, "inv", h);
            prev = h;
            let mut l = lineage(&vm.program, class);
            if l.len() == 1 {
                l.extend(ancestors_of(class).iter().map(|a| a.to_ascii_lowercase()));
            }
            items.push((h, l));
            if snap.inv_bar.iter().any(|c| c.eq_ignore_ascii_case(class)) {
                set_int(vm, h, "binvbar", 1);
            }
            if snap
                .inv_sel
                .as_deref()
                .is_some_and(|c| c.eq_ignore_ascii_case(class))
            {
                set_obj(vm, mo, "invsel", h);
            }
            if self
                .inv_first
                .as_deref()
                .is_some_and(|c| c.eq_ignore_ascii_case(class))
            {
                set_obj(vm, mo, "invfirst", h);
            }
        }
        if let Some(weapon) = &snap.ready_weapon {
            // The owned weapon object itself, so `weapon == ReadyWeapon` holds.
            let w = items
                .iter()
                .find(|(_, l)| l.first().is_some_and(|c| c.eq_ignore_ascii_case(weapon)))
                .map(|(h, _)| *h)
                .unwrap_or_else(|| {
                    let mut chain: Vec<&str> = vec![weapon];
                    chain.extend(ancestors_of(weapon).iter().map(String::as_str));
                    chain.push("Weapon");
                    made.obtain(
                        vm,
                        &format!("ready:{}", weapon.to_ascii_lowercase()),
                        &chain,
                    )
                });
            set_obj(vm, w, "ammo1", ObjHandle::NULL);
            set_obj(vm, w, "ammo2", ObjHandle::NULL);
            if let Some(a1) = &snap.ammo1 {
                set_str(vm, w, "ammotype1", a1);
                let h = items
                    .iter()
                    .find(|(_, l)| l.first().is_some_and(|c| c.eq_ignore_ascii_case(a1)))
                    .map(|(h, _)| *h)
                    .unwrap_or_else(|| item(vm, made, a1, true));
                set_obj(vm, w, "ammo1", h);
            }
            if let Some(a2) = &snap.ammo2 {
                set_str(vm, w, "ammotype2", a2);
                let h = items
                    .iter()
                    .find(|(_, l)| l.first().is_some_and(|c| c.eq_ignore_ascii_case(a2)))
                    .map(|(h, _)| *h)
                    .unwrap_or_else(|| item(vm, made, a2, true));
                set_obj(vm, w, "ammo2", h);
            }
            set_obj(vm, player, "readyweapon", w);
        }
        // Class defaults: every slot weapon, its ammo, every item and key.
        let mut classes: Vec<String> = snap.inventory.iter().map(|(c, _, _)| c.clone()).collect();
        for w in &snap.weapons {
            classes.push(w.class.clone());
            classes.extend(w.ammo1.iter().cloned());
            classes.extend(w.ammo2.iter().cloned());
        }
        classes.extend(DOOM_KEYS.iter().map(|k| (*k).to_owned()));
        let mut defaults: Vec<(String, ObjHandle)> = Vec::new();
        for class in classes {
            let key = class.to_ascii_lowercase();
            if defaults.iter().any(|(k, _)| *k == key) {
                continue;
            }
            let h = item(vm, made, &class, false);
            defaults.push((key, h));
        }
        FRAME.with(|fr| {
            let mut fr = fr.borrow_mut();
            fr.items = items;
            fr.defaults = defaults;
        });
        set_int(vm, player, "inventorytics", i64::from(snap.inventory_tics));
        self.mo = mo;
        self.player = player;
        // `gameinfo`: Doom's health / berserk pictures (`GameInfoStruct`).
        let gi = made.obtain(vm, "gameinfo", &["GameInfoStruct"]);
        made.end(vm);
        set_int(vm, gi, "gametype", 1);
        set_int(vm, gi, "healthpic", texture_id("MEDIA0"));
        set_int(vm, gi, "berserkpic", texture_id("PSTRA0"));
        let bar = self.bar;
        FRAME.with(|fr| {
            let mut fr = fr.borrow_mut();
            fr.bar = bar;
            fr.gameinfo = gi;
        });
        set_obj(vm, self.bar, "cplayer", player);
        vm.play.inventory = vec![(
            mo,
            snap.inventory
                .iter()
                .map(|(c, a, m)| (c.to_ascii_lowercase(), *a, *m))
                .collect(),
        )];
        vm.play.sight.clear();
    }
}

fn actor_slots(program: &CompiledProgram) -> ActorSlots {
    let slot = |class: &str, name: &str, bank: u8| {
        field_slot(program, class, name)
            .filter(|(b, _)| *b == bank)
            .map(|(_, i)| i)
    };
    ActorSlots {
        pos: slot("Actor", "pos", 3),
        vel: slot("Actor", "vel", 3),
        angle: slot("Actor", "angle", 3),
        pitch: slot("Actor", "pitch", 3),
        radius: slot("Actor", "radius", 3),
        height: slot("Actor", "height", 3),
        amount: slot("Inventory", "amount", 0),
        max_amount: slot("Inventory", "maxamount", 0),
        target: slot("Actor", "target", 1),
        master: slot("Actor", "master", 1),
        tracer: slot("Actor", "tracer", 1),
    }
}

// --- natives -----------------------------------------------------------------

fn arg_i(regs: &NativeRegs<'_>, k: usize, d: i64) -> i64 {
    regs.i.get(k).copied().unwrap_or(d)
}

fn arg_f(regs: &NativeRegs<'_>, k: usize, d: f64) -> f64 {
    regs.f.get(k).copied().unwrap_or(d)
}

fn arg_s(ctx: &ZvmCtx<'_>, regs: &NativeRegs<'_>, k: usize) -> String {
    regs.s
        .get(k)
        .map(|h| ctx.heap.str(*h).to_owned())
        .unwrap_or_default()
}

fn arg_a(regs: &NativeRegs<'_>, k: usize) -> ObjHandle {
    regs.a.get(k).copied().unwrap_or(ObjHandle::NULL)
}

fn ret_i(regs: &mut NativeRegs<'_>, v: i64) {
    if let Some(slot) = regs.i.first_mut() {
        *slot = v;
    }
    if let Some(slot) = regs.f.first_mut() {
        *slot = v as f64;
    }
}

fn ret_a(regs: &mut NativeRegs<'_>, h: ObjHandle) {
    if let Some(slot) = regs.a.first_mut() {
        *slot = h;
    }
}

fn ret_s(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>, text: &str) {
    let h = ctx.heap.intern_string(text);
    if let Some(slot) = regs.s.first_mut() {
        *slot = h;
    }
    if regs.s.len() > 1 {
        regs.s[1] = h;
    }
}

/// Named objects (fonts, textures): the name lives in `fields_s[0]`.
fn named_object(ctx: &mut ZvmCtx<'_>, name: &str) -> ObjHandle {
    let h = ctx.heap.new_object(0, 4, 2, 2);
    let s = ctx.heap.intern_string(name);
    if let Some(ObjKind::Object { fields_s, .. }) = ctx.heap.get_mut(h) {
        fields_s[0] = s;
    }
    h
}

fn object_name(ctx: &ZvmCtx<'_>, h: ObjHandle) -> String {
    match ctx.heap.get(h) {
        Some(ObjKind::Object { fields_s, .. }) => fields_s
            .first()
            .map(|s| ctx.heap.str(*s).to_owned())
            .unwrap_or_default(),
        _ => String::new(),
    }
}

/// Glyph lump for `ch` in `font` (Doom's status bar fonts are patch sets).
fn glyph(font: &str, ch: char) -> Option<String> {
    let f = font.to_ascii_lowercase();
    let digit = ch.to_digit(10);
    match f.as_str() {
        "hudfont_doom" => match (ch, digit) {
            (_, Some(d)) => Some(format!("STTNUM{d}")),
            ('%', _) => Some("STTPRCNT".into()),
            ('-', _) => Some("STTMINUS".into()),
            _ => None,
        },
        "indexfont_doom" => digit.map(|d| format!("STYSNUM{d}")),
        _ => {
            let code = ch.to_ascii_uppercase() as u32;
            (33..=95).contains(&code).then(|| format!("STCFN{code:03}"))
        }
    }
}

/// Advance of `ch` in `font` with HUDFont `spacing` / monospacing.
fn advance(font: &str, ch: char, mono: i64) -> f32 {
    if mono > 0 {
        #[allow(clippy::cast_precision_loss)]
        return mono as f32;
    }
    if ch == ' ' {
        return 4.0;
    }
    glyph(font, ch)
        .and_then(|g| dims_of(&g))
        .map_or(8.0, |d| d.w)
}

/// Screen position for HUD coordinates. In the full-screen HUD negative
/// coordinates count from the right / bottom (`DI_SCREEN_AUTO`).
fn screen_xy(x: f64, y: f64, flags: i64) -> (f32, f32) {
    #[allow(clippy::cast_possible_truncation)]
    let (mut px, mut py) = (x as f32, y as f32);
    let fullscreen = FRAME.with(|fr| fr.borrow().fullscreen);
    if !fullscreen {
        return (px, py);
    }
    let sp = hud_space();
    if flags & DI_SCREEN_MANUAL_ALIGN != 0 {
        match flags & DI_SCREEN_HMASK {
            DI_SCREEN_RIGHT => px += sp.w,
            DI_SCREEN_HCENTER => px += sp.w / 2.0,
            _ => {}
        }
        match flags & DI_SCREEN_VMASK {
            DI_SCREEN_BOTTOM => py += sp.h,
            DI_SCREEN_VCENTER => py += sp.h / 2.0,
            _ => {}
        }
    } else {
        if x < 0.0 {
            px += sp.w;
        }
        if y < 0.0 {
            py += sp.h;
        }
    }
    (px, py)
}

/// `GetAirTime()`: `Level.airsupply` (20 s) while above water. The snapshot
/// has no water level, so PB's oxygen readout stays hidden.
fn sbar_host_air_time(_: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
    if let Some(slot) = regs.i.first_mut() {
        *slot = 20 * 35;
    }
    Ok(())
}

/// `DrawBar(ongfx, offgfx, curval, maxval, pos, border, vertical, flags,
/// alpha)`: the background picture, then the foreground clipped to the
/// filled part (`SHADER_VERT` = 2 fills bottom-up, `SHADER_REVERSE` = 1
/// from the other end), as UZDoom's script does with clip rectangles.
fn sbar_host_draw_bar(ctx: &mut ZvmCtx<'_>, _regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
    use room_zvm::Value;
    let args = &ctx.play.native_args;
    let s = |k: usize| match args.get(k) {
        Some(Value::S(h)) => ctx.heap.str(*h).to_owned(),
        _ => String::new(),
    };
    let f = |k: usize| match args.get(k) {
        Some(Value::F(v)) => *v,
        Some(Value::I(v)) => *v as f64,
        _ => 0.0,
    };
    let i = |k: usize| match args.get(k) {
        Some(Value::I(v)) => *v,
        _ => 0,
    };
    // `self`, on, off, cur, max, pos.x, pos.y, border, vertical, flags, alpha.
    let (on, off) = (s(1), s(2));
    let (cur, max) = (f(3), f(4));
    let (x, y) = (f(5), f(6));
    let vertical = i(8);
    let flags = i(9);
    let alpha = if args.len() > 10 { f(10) } else { 1.0 };
    let value = if max != 0.0 {
        (cur / max).clamp(0.0, 1.0)
    } else {
        0.0
    };
    if !off.is_empty() {
        push_image(&off, x, y, flags, alpha, (1.0, 1.0));
    }
    let Some(d) = dims_of(&on) else {
        // Size unknown (first frame): no clip is possible, so the full image
        // is not shown; drawn invisible so the shell loads it.
        push_image(&on, x, y, flags, 0.0, (1.0, 1.0));
        return Ok(());
    };
    let (sx, sy) = screen_xy(x, y, flags);
    let px = match flags & DI_ITEM_HMASK {
        DI_ITEM_LEFT | DI_ITEM_HOFFSET => sx,
        DI_ITEM_RIGHT => sx - d.w,
        _ => sx - d.w / 2.0,
    };
    let py = match flags & DI_ITEM_VMASK {
        DI_ITEM_TOP | DI_ITEM_VOFFSET => sy,
        DI_ITEM_VCENTER => sy - d.h / 2.0,
        _ => sy - d.h,
    };
    #[allow(clippy::cast_possible_truncation)]
    let v = value as f32;
    let horizontal = vertical & 2 == 0;
    let reverse = vertical & 1 != 0;
    let rect = match (horizontal, reverse) {
        (true, false) => crate::draw::Rect {
            x: px,
            y: py,
            w: d.w * v,
            h: d.h,
        },
        (true, true) => crate::draw::Rect {
            x: px + d.w * (1.0 - v),
            y: py,
            w: d.w * v,
            h: d.h,
        },
        (false, false) => crate::draw::Rect {
            x: px,
            y: py + d.h * (1.0 - v),
            w: d.w,
            h: d.h * v,
        },
        (false, true) => crate::draw::Rect {
            x: px,
            y: py,
            w: d.w,
            h: d.h * v,
        },
    };
    FRAME.with(|fr| {
        fr.borrow_mut()
            .cmds
            .push(DrawCmd::Clip { rect: Some(rect) })
    });
    push_image(&on, x, y, flags, alpha, (1.0, 1.0));
    FRAME.with(|fr| fr.borrow_mut().cmds.push(DrawCmd::Clip { rect: None }));
    Ok(())
}

/// Draw a patch with `DI_ITEM_*` alignment.
fn push_image(name: &str, x: f64, y: f64, flags: i64, alpha: f64, scale: (f64, f64)) {
    if name.is_empty() || name.eq_ignore_ascii_case("TNT1A0") {
        return;
    }
    let (sx, sy) = screen_xy(x, y, flags);
    let d = dims_of(name);
    #[allow(clippy::cast_possible_truncation)]
    let (scale_x, scale_y) = (scale.0 as f32, scale.1 as f32);
    let (w, h) = d.map_or((0.0, 0.0), |d| (d.w * scale_x, d.h * scale_y));
    let mut apply_offsets = false;
    let px = match flags & DI_ITEM_HMASK {
        DI_ITEM_HOFFSET => {
            apply_offsets = true;
            sx
        }
        DI_ITEM_LEFT => sx,
        DI_ITEM_RIGHT => sx - w,
        _ => sx - w / 2.0,
    };
    let py = match flags & DI_ITEM_VMASK {
        DI_ITEM_VOFFSET => {
            apply_offsets = true;
            sy
        }
        DI_ITEM_TOP => sy,
        DI_ITEM_VCENTER => sy - h / 2.0,
        _ => sy - h,
    };
    // Offsets apply per axis in UZDoom; with only one axis on, fold the other in.
    let (px, py) = if apply_offsets {
        let d = d.unwrap_or_default();
        let ox = if flags & DI_ITEM_HMASK == DI_ITEM_HOFFSET {
            0.0
        } else {
            -d.left * scale_x
        };
        let oy = if flags & DI_ITEM_VMASK == DI_ITEM_VOFFSET {
            0.0
        } else {
            -d.top * scale_y
        };
        (px + ox, py + oy)
    } else {
        (px, py)
    };
    let frame_alpha = FRAME.with(|fr| fr.borrow().alpha);
    let dim = if flags & DI_DIM != 0 { 0.5 } else { 1.0 };
    #[allow(clippy::cast_possible_truncation)]
    let cmd = DrawCmd::Patch {
        name: name.to_owned(),
        pos: Vec2::new(px, py),
        size: if (scale_x - 1.0).abs() > f32::EPSILON || (scale_y - 1.0).abs() > f32::EPSILON {
            Vec2::new(w, h)
        } else {
            Vec2::new(0.0, 0.0)
        },
        apply_offsets,
        inverse: false,
        light: dim,
        fuzz: false,
        flip: flags & DI_MIRROR != 0,
        mirror: flags & DI_MIRRORY != 0,
        angle: 0.0,
        pivot_x: 0.5,
        pivot_y: 0.5,
        tint: [1.0, 1.0, 1.0],
        alpha: (alpha as f32 * frame_alpha).clamp(0.0, 1.0),
        recolor: None,
        corners: [Vec2::new(0.0, 0.0); 4],
    };
    FRAME.with(|fr| fr.borrow_mut().cmds.push(cmd));
}

fn sbar_begin_statusbar(_: &mut ZvmCtx<'_>, _: &mut NativeRegs<'_>) -> Result<(), Trap> {
    FRAME.with(|fr| {
        let mut fr = fr.borrow_mut();
        finish_hud_commands(&mut fr);
        fr.used_statusbar = true;
        fr.fullscreen = false;
        fr.alpha = 1.0;
    });
    Ok(())
}

fn sbar_begin_hud(_: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
    #[allow(clippy::cast_possible_truncation)]
    let alpha = arg_f(regs, 0, 1.0) as f32;
    FRAME.with(|fr| {
        let mut fr = fr.borrow_mut();
        fr.fullscreen = true;
        fr.alpha = if alpha > 0.0 { alpha } else { 1.0 };
        if fr.hud_start.is_none() {
            fr.hud_start = Some(fr.cmds.len());
        }
    });
    Ok(())
}

fn sbar_noop(_: &mut ZvmCtx<'_>, _: &mut NativeRegs<'_>) -> Result<(), Trap> {
    Ok(())
}

/// `DrawImage(texture, pos, flags, alpha, box, scale, …)`.
fn sbar_draw_image(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
    let name = arg_s(ctx, regs, 0);
    draw_image_args(&name, regs, 0);
    Ok(())
}

/// `DrawTexture(TextureID, pos, flags, alpha, box, scale, …)`; the id is `i[0]`.
fn sbar_draw_texture(_: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
    let name = texture_name(arg_i(regs, 0, 0));
    draw_image_args(&name, regs, 1);
    Ok(())
}

/// `pos` is `f[0..2]`, flags `i[flags_at]`, alpha `f[2]`, box `f[3..5]`, scale `f[5..7]`.
fn draw_image_args(name: &str, regs: &NativeRegs<'_>, flags_at: usize) {
    let (x, y) = (arg_f(regs, 0, 0.0), arg_f(regs, 1, 0.0));
    let flags = arg_i(regs, flags_at, 0);
    let alpha = arg_f(regs, 2, 1.0);
    let scale = (arg_f(regs, 5, 1.0), arg_f(regs, 6, 1.0));
    let scale = (
        if scale.0 == 0.0 { 1.0 } else { scale.0 },
        if scale.1 == 0.0 { 1.0 } else { scale.1 },
    );
    push_image(
        name,
        x,
        y,
        flags,
        if alpha <= 0.0 { 1.0 } else { alpha },
        scale,
    );
}

/// `DrawString(HUDFont, text, pos, flags, translation, alpha, …)`.
fn sbar_draw_string(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
    let hud_font = arg_a(regs, 1);
    let (font, spacing, mono) = match ctx.heap.get(hud_font) {
        Some(ObjKind::Object {
            fields_s, fields_i, ..
        }) => (
            fields_s
                .first()
                .map(|s| ctx.heap.str(*s).to_owned())
                .unwrap_or_default(),
            fields_i.first().copied().unwrap_or(0),
            fields_i.get(1).copied().unwrap_or(0),
        ),
        _ => (String::from("SmallFont"), 0, 0),
    };
    let text = arg_s(ctx, regs, 0);
    let (x, y) = (arg_f(regs, 0, 0.0), arg_f(regs, 1, 0.0));
    let flags = arg_i(regs, 0, 0);
    let alpha = arg_f(regs, 2, 1.0);
    // Monospaced fonts advance by `spacing`; others add it to each glyph.
    let mono_w = if mono != 0 { spacing } else { 0 };
    let extra = if mono == 0 { spacing } else { 0 };
    let chars: Vec<char> = text.chars().collect();
    #[allow(clippy::cast_precision_loss)]
    let width: f32 = chars
        .iter()
        .map(|&c| advance(&font, c, mono_w) + extra as f32)
        .sum();
    let start = if flags & DI_TEXT_ALIGN_RIGHT != 0 {
        x - f64::from(width)
    } else if flags & DI_TEXT_ALIGN_CENTER != 0 {
        x - f64::from(width) / 2.0
    } else {
        x
    };
    let mut cx = start;
    // A right-aligned string whose x is negative still counts from the right edge.
    let screen_flags = if x < 0.0 && start >= 0.0 {
        flags | DI_SCREEN_MANUAL_ALIGN | DI_SCREEN_RIGHT
    } else {
        flags
    };
    for &c in &chars {
        let adv = f64::from(advance(&font, c, mono_w)) + extra as f64;
        if let Some(g) = glyph(&font, c) {
            if dims_of(&g).is_some() {
                // Glyphs sit at the pen position, top-left (`DTA` text drawing).
                let (sx, sy) = screen_xy(cx, y, screen_flags);
                let frame_alpha = FRAME.with(|fr| fr.borrow().alpha);
                #[allow(clippy::cast_possible_truncation)]
                let cmd = DrawCmd::Patch {
                    name: g,
                    pos: Vec2::new(sx, sy),
                    size: Vec2::new(0.0, 0.0),
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
                    alpha: (alpha as f32 * frame_alpha).clamp(0.0, 1.0),
                    recolor: None,
                    corners: [Vec2::new(0.0, 0.0); 4],
                };
                FRAME.with(|fr| fr.borrow_mut().cmds.push(cmd));
            } else {
                let (sx, sy) = screen_xy(cx, y, screen_flags);
                FRAME.with(|fr| {
                    fr.borrow_mut().cmds.push(DrawCmd::Text {
                        font: font.clone(),
                        pos: Vec2::new(sx, sy),
                        text: c.to_string(),
                        color: Rgba::WHITE,
                        scale: 1.0,
                    });
                });
            }
        }
        cx += adv;
    }
    Ok(())
}

/// `Fill(color, x, y, w, h, flags)`.
fn sbar_fill(_: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
    let argb = arg_i(regs, 0, 0) as u32;
    let flags = arg_i(regs, 1, 0);
    let (x, y, w, h) = (
        arg_f(regs, 0, 0.0),
        arg_f(regs, 1, 0.0),
        arg_f(regs, 2, 0.0),
        arg_f(regs, 3, 0.0),
    );
    let (sx, sy) = screen_xy(x, y, flags);
    let c = |shift: u32| f32::from(((argb >> shift) & 0xff) as u8) / 255.0;
    let a = if argb >> 24 == 0 { 1.0 } else { c(24) };
    #[allow(clippy::cast_possible_truncation)]
    FRAME.with(|fr| {
        fr.borrow_mut().cmds.push(DrawCmd::Fill {
            rect: crate::draw::Rect {
                x: sx,
                y: sy,
                w: w as f32,
                h: h as f32,
            },
            color: Rgba {
                r: c(16),
                g: c(8),
                b: c(0),
                a,
            },
        });
    });
    Ok(())
}

/// `FormatNumber(number, minsize, maxsize, format, prefix)`.
fn sbar_format_number(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
    let n = arg_i(regs, 0, 0);
    let minsize = arg_i(regs, 1, 0).max(0) as usize;
    let maxsize = arg_i(regs, 2, 0);
    let prefix = arg_s(ctx, regs, 0);
    let mut v = n;
    if maxsize > 0 {
        // Clamp to the digits that fit (`10^maxsize - 1`).
        let cap = 10i64.saturating_pow(u32::try_from(maxsize).unwrap_or(9)) - 1;
        v = v.min(cap);
    }
    let text = format!("{prefix}{v:>minsize$}");
    ret_s(ctx, regs, &text);
    Ok(())
}

/// `GetMugShot(accuracy, stateflags, default_face)` → the face lump.
fn sbar_get_mugshot(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
    let face = FRAME.with(|fr| fr.borrow().snap.face.clone());
    let _ = ctx;
    ret_i(regs, texture_id(&face));
    Ok(())
}

/// `GetInventoryIcon(item, flags)` → `TextureID, bool`.
fn sbar_get_inventory_icon(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
    let item = arg_a(regs, 0);
    let class = match ctx.heap.get(item) {
        Some(ObjKind::Object { .. }) => {
            // Class name comes from the object layout via `object_layouts`.
            let id = match ctx.heap.get(item) {
                Some(ObjKind::Object { class_id, .. }) => *class_id,
                _ => 0,
            };
            ctx.play
                .object_layouts
                .iter()
                .find(|(_, v)| v.0 == id)
                .map(|(k, _)| k.clone())
                .unwrap_or_default()
        }
        _ => String::new(),
    };
    let icon = FRAME.with(|fr| {
        fr.borrow()
            .snap
            .icons
            .iter()
            .find(|(c, _)| c.eq_ignore_ascii_case(&class))
            .map(|(_, i)| i.clone())
            .unwrap_or_default()
    });
    let id = texture_id(&icon);
    ret_i(regs, id);
    ctx.play.multi_ret = vec![Value::I(id), Value::I(0)];
    Ok(())
}

/// `TextureID.IsValid()` / `Exists()`: the id is the receiver's int.
fn sbar_tex_valid(_: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
    let id = arg_i(regs, 0, 0);
    ret_i(regs, i64::from(id > 0 && !texture_name(id).is_empty()));
    Ok(())
}

/// `TexMan.CheckForTexture(name, usetype, flags)`.
/// `TexMan.GetScaledSize(tex)`: the patch size, as a vector.
fn texman_scaled_size(_ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
    let name = texture_name(arg_i(regs, 0, 0));
    let d = dims_of(&name).unwrap_or_default();
    if let Some(slot) = regs.f.get_mut(0) {
        *slot = f64::from(d.w);
    }
    if let Some(slot) = regs.f.get_mut(1) {
        *slot = f64::from(d.h);
    }
    Ok(())
}

fn texman_check(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
    let name = arg_s(ctx, regs, 0);
    let id = if dims_of(&name).is_some() {
        texture_id(&name)
    } else {
        0
    };
    ret_i(regs, id);
    Ok(())
}

/// `Font.GetFont(name)` and friends: a named font object.
fn font_get(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
    let name = arg_s(ctx, regs, 0);
    let h = named_object(ctx, if name.is_empty() { "SmallFont" } else { &name });
    ret_a(regs, h);
    Ok(())
}

fn font_small(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
    let alt = arg_a(regs, 0);
    if !object_name(ctx, alt).is_empty() {
        ret_a(regs, alt);
        return Ok(());
    }
    let h = named_object(ctx, "SmallFont");
    ret_a(regs, h);
    Ok(())
}

fn font_big(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
    // `GetBigTextFont(altfont)`: the alternative font when given.
    let alt = arg_a(regs, 0);
    if !object_name(ctx, alt).is_empty() {
        ret_a(regs, alt);
        return Ok(());
    }
    let h = named_object(ctx, "BigFont");
    ret_a(regs, h);
    Ok(())
}

/// `Font.GetCharWidth(code)`; a string argument (`"0"`) arrives as a truth value.
fn font_char_width(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
    let font = object_name(ctx, arg_a(regs, 0));
    let code = arg_i(regs, 0, 48);
    let ch = char::from_u32(u32::try_from(code).unwrap_or(48))
        .filter(|c| !c.is_control())
        .unwrap_or('0');
    #[allow(clippy::cast_possible_truncation)]
    ret_i(regs, advance(&font, ch, 0) as i64);
    Ok(())
}

fn font_height(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
    let font = object_name(ctx, arg_a(regs, 0));
    let h = glyph(&font, '0')
        .and_then(|g| dims_of(&g))
        .map_or(8.0, |d| d.h);
    #[allow(clippy::cast_possible_truncation)]
    ret_i(regs, h as i64);
    Ok(())
}

fn font_string_width(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
    let font = object_name(ctx, arg_a(regs, 0));
    let text = arg_s(ctx, regs, 0);
    let w: f32 = text.chars().map(|c| advance(&font, c, 0)).sum();
    #[allow(clippy::cast_possible_truncation)]
    ret_i(regs, w as i64);
    Ok(())
}

/// `HUDFont.Create(font, spacing, monospacing, shadowx, shadowy)`.
fn hudfont_create(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
    let font_obj = arg_a(regs, 0);
    let mut name = object_name(ctx, font_obj);
    if name.is_empty() {
        // `HUDFont.Create("INDEXFONT")`: the name came as a string.
        name = arg_s(ctx, regs, 0);
    }
    let spacing = arg_i(regs, 0, 0);
    let mono = arg_i(regs, 1, 0);
    let h = named_object(ctx, &name);
    if let Some(ObjKind::Object {
        fields_i, fields_a, ..
    }) = ctx.heap.get_mut(h)
    {
        fields_i[0] = spacing;
        fields_i[1] = mono;
        fields_a[0] = font_obj;
    }
    ret_a(regs, h);
    Ok(())
}

/// `PlayerInfo.HasWeaponsInSlot(slot)`.
fn player_has_slot(_: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
    let slot = usize::try_from(arg_i(regs, 0, 0)).unwrap_or(0);
    let has = FRAME.with(|fr| fr.borrow().snap.slots.get(slot).copied().unwrap_or(false));
    ret_i(regs, i64::from(has));
    Ok(())
}

/// `CheckKeys(lock, remote, quiet)` for Doom locks 1–6.
fn actor_check_keys(_: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
    let keys = FRAME.with(|fr| fr.borrow().snap.keys);
    let bit = match arg_i(regs, 0, 0) {
        1 => 4,  // red card
        2 => 1,  // blue card
        3 => 2,  // yellow card
        4 => 32, // red skull
        5 => 8,  // blue skull
        6 => 16, // yellow skull
        _ => 0,
    };
    ret_i(regs, i64::from(keys & bit != 0));
    Ok(())
}

/// A mirrored class default, when the frame made one.
fn frame_default(class: &str) -> Option<ObjHandle> {
    let class = class.to_ascii_lowercase();
    FRAME.with(|fr| {
        fr.borrow()
            .defaults
            .iter()
            .find(|(k, _)| *k == class)
            .map(|(_, h)| *h)
    })
}

/// Class name of a heap object (via `object_layouts`).
fn obj_class(ctx: &ZvmCtx<'_>, h: ObjHandle) -> String {
    let Some(ObjKind::Object { class_id, .. }) = ctx.heap.get(h) else {
        return String::new();
    };
    let id = *class_id;
    ctx.play
        .object_layouts
        .iter()
        .find(|(_, v)| v.0 == id)
        .map(|(k, _)| k.clone())
        .unwrap_or_default()
}

/// `FindInventory(class, subclass)` over the mirrored pawn's items.
fn sbar_find_inventory(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
    let class = arg_s(ctx, regs, 0).to_ascii_lowercase();
    let subclass = arg_i(regs, 0, 0) != 0;
    let hit = FRAME.with(|fr| {
        fr.borrow()
            .items
            .iter()
            .find(|(_, l)| {
                l.first().is_some_and(|c| *c == class) || (subclass && l.contains(&class))
            })
            .map(|(h, _)| *h)
    });
    ret_a(regs, hit.unwrap_or(ObjHandle::NULL));
    Ok(())
}

/// `obj.default`: the class default when mirrored, else the object.
fn sbar_get_default(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
    let obj = arg_a(regs, 0);
    let class = obj_class(ctx, obj);
    ret_a(regs, frame_default(&class).unwrap_or(obj));
    Ok(())
}

/// `WeaponSlots.SlotSize(slot)`.
fn weaponslots_size(_: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
    let slot = arg_i(regs, 0, 0);
    let n = FRAME.with(|fr| {
        fr.borrow()
            .snap
            .weapons
            .iter()
            .filter(|w| i64::from(w.slot) == slot)
            .count()
    });
    ret_i(regs, n as i64);
    Ok(())
}

/// `WeaponSlots.GetWeapon(slot, index)` → the class (by name).
fn weaponslots_get(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
    let (slot, index) = (arg_i(regs, 0, 0), arg_i(regs, 1, 0));
    let class = FRAME.with(|fr| {
        fr.borrow()
            .snap
            .weapons
            .iter()
            .filter(|w| i64::from(w.slot) == slot)
            .nth(usize::try_from(index).unwrap_or(usize::MAX))
            .map(|w| w.class.clone())
            .unwrap_or_default()
    });
    ret_s(ctx, regs, &class);
    Ok(())
}

/// `WeaponSlots.LocateWeapon(class)` → `bool, int slot, int index`.
fn weaponslots_locate(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
    let class = arg_s(ctx, regs, 0);
    let hit = FRAME.with(|fr| {
        let fr = fr.borrow();
        let w = fr
            .snap
            .weapons
            .iter()
            .find(|w| w.class.eq_ignore_ascii_case(&class))?;
        let index = fr
            .snap
            .weapons
            .iter()
            .filter(|o| o.slot == w.slot)
            .position(|o| o.class.eq_ignore_ascii_case(&class))?;
        Some((i64::from(w.slot), index as i64))
    });
    let (found, slot, index) = hit.map_or((0, -1, -1), |(s, i)| (1, s, i));
    ret_i(regs, found);
    ctx.play.multi_ret = vec![Value::I(found), Value::I(slot), Value::I(index)];
    Ok(())
}

/// `obj.GetClass()`: the class (by name).
fn sbar_get_class(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
    let class = obj_class(ctx, arg_a(regs, 0));
    ret_s(ctx, regs, &class);
    Ok(())
}

/// `TextureID.IsNull()`: the null texture (id 0).
fn sbar_tex_null(_: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
    ret_i(regs, i64::from(arg_i(regs, 0, 0) == 0));
    Ok(())
}

/// `Key.GetKeyTypeCount()`: Doom's six keys.
fn key_type_count(_: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
    ret_i(regs, DOOM_KEYS.len() as i64);
    Ok(())
}

/// `Key.GetKeyType(index)`.
fn key_type(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
    let k = usize::try_from(arg_i(regs, 0, 0)).unwrap_or(usize::MAX);
    ret_s(ctx, regs, DOOM_KEYS.get(k).copied().unwrap_or(""));
    Ok(())
}

/// `TexMan.GetSize(tex)` → `int, int`.
fn texman_size(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
    let d = dims_of(&texture_name(arg_i(regs, 0, 0))).unwrap_or_default();
    #[allow(clippy::cast_possible_truncation)]
    let (w, h) = (d.w as i64, d.h as i64);
    ret_i(regs, w);
    ctx.play.multi_ret = vec![Value::I(w), Value::I(h)];
    Ok(())
}

/// `TextureID.Exists()`: any id but the invalid one (`SetInvalid`, -1).
fn sbar_tex_exists(_: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
    ret_i(regs, i64::from(arg_i(regs, 0, 0) >= 0));
    Ok(())
}

/// `GetDefaultByType(class)`: the mirrored default, else an empty item.
fn get_default_by_type(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
    let class = arg_s(ctx, regs, 0).to_ascii_lowercase();
    if let Some(h) = frame_default(&class) {
        ret_a(regs, h);
        return Ok(());
    }
    let (id, n_i, n_a, n_s) = ctx
        .play
        .object_layouts
        .get(&class)
        .copied()
        .unwrap_or((0, 8, 4, 4));
    let h = ctx.heap.new_object(id, n_i, n_a, n_s);
    let max = FRAME.with(|fr| {
        fr.borrow()
            .snap
            .inventory
            .iter()
            .find(|(c, _, _)| c.eq_ignore_ascii_case(&class))
            .map_or(0, |(_, _, m)| *m)
    });
    if let (Some(slot), Some(ObjKind::Object { fields_i, .. })) =
        (ctx.play.actor_slots.max_amount, ctx.heap.get_mut(h))
    {
        if let Some(cell) = fields_i.get_mut(slot) {
            *cell = i64::from(max);
        }
    }
    ret_a(regs, h);
    Ok(())
}

fn register(vm: &mut Zvm) {
    let t = &mut vm.natives;
    t.replace("statusbarcore.beginstatusbar", sbar_begin_statusbar);
    t.replace("Host.DrawBar", sbar_host_draw_bar);
    t.replace("Host.GetAirTime", sbar_host_air_time);
    t.replace("statusbarcore.beginhud", sbar_begin_hud);
    t.replace("statusbarcore.setsize", sbar_noop);
    t.replace("statusbarcore.setcliprect", sbar_noop);
    t.replace("statusbarcore.drawimage", sbar_draw_image);
    t.replace("statusbarcore.drawtexture", sbar_draw_texture);
    t.replace("statusbarcore.drawimagerotated", sbar_draw_image);
    t.replace("statusbarcore.drawtexturerotated", sbar_draw_texture);
    t.replace("statusbarcore.drawstring", sbar_draw_string);
    t.replace("statusbarcore.fill", sbar_fill);
    t.replace("statusbarcore.formatnumber", sbar_format_number);
    t.replace("basestatusbar.getmugshot", sbar_get_mugshot);
    t.replace("basestatusbar.getinventoryicon", sbar_get_inventory_icon);
    t.replace("basestatusbar.refreshbackground", sbar_noop);
    // The native part of `BaseStatusBar.Tick` (message and mugshot timers)
    // has nothing to do here; the script `Tick` around it runs.
    t.replace("basestatusbar.tick", sbar_noop);
    t.replace("basestatusbar.drawcrosshair", sbar_noop);
    t.replace("basestatusbar.updatescreengeometry", sbar_noop);
    t.replace("textureid.isvalid", sbar_tex_valid);
    t.replace("texman.checkfortexture", texman_check);
    t.replace("texman.getscaledsize", texman_scaled_size);
    t.replace("textureid.exists", sbar_tex_exists);
    t.replace("icon.isvalid", sbar_tex_valid);
    t.replace("font.getfont", font_get);
    t.replace("font.findfont", font_get);
    t.replace("font.getsmalltextfont", font_small);
    t.replace("font.getbigtextfont", font_big);
    t.replace("font.getcharwidth", font_char_width);
    t.replace("font.getheight", font_height);
    t.replace("font.stringwidth", font_string_width);
    t.replace("hudfont.create", hudfont_create);
    t.replace("playerinfo.hasweaponsinslot", player_has_slot);
    t.replace("actor.checkkeys", actor_check_keys);
    t.replace("playerpawn.checkkeys", actor_check_keys);
    t.replace("global.getdefaultbytype", get_default_by_type);
    t.replace("screen.drawtext", screen_draw_text);
    for name in [
        "actor.findinventory",
        "playerpawn.findinventory",
        "global.findinventory",
    ] {
        t.replace(name, sbar_find_inventory);
    }
    t.replace("actor.getdefault", sbar_get_default);
    t.replace("weaponslots.slotsize", weaponslots_size);
    t.replace("weaponslots.getweapon", weaponslots_get);
    t.replace("weaponslots.locateweapon", weaponslots_locate);
    t.replace("textureid.isnull", sbar_tex_null);
    for name in [
        "actor",
        "inventory",
        "weapon",
        "ammo",
        "playerpawn",
        "object",
    ] {
        t.replace(&format!("{name}.getclass"), sbar_get_class);
    }
    t.replace("key.getkeytypecount", key_type_count);
    t.replace("key.getkeytype", key_type);
    t.replace("texman.getsize", texman_size);
    t.replace("_.statusbar", global_statusbar);
    t.replace("_.gameinfo", global_gameinfo);
    for (name, _) in HUD_CVARS {
        t.replace(name, hud_cvar);
    }
    t.replace("screen.drawtexture", screen_draw_texture);
    t.replace("actor.getdefaultbytype", get_default_by_type);
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    fn stdlib_program() -> CompiledProgram {
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../wadsrc/uzdoom");
        let entry = std::fs::read_to_string(root.join("zscript.txt")).unwrap();
        let mut sources = vec![("zscript.txt".to_owned(), entry.clone())];
        for line in entry.lines() {
            let Some(rest) = line.trim().strip_prefix("#include") else {
                continue;
            };
            let path = rest.trim().trim_matches('"');
            let text = std::fs::read_to_string(root.join(path)).unwrap();
            sources.push((path.replace('\\', "/"), text));
        }
        room_zscript::check(&sources).program.unwrap()
    }

    fn dims(name: &str) -> Option<PatchDims> {
        let up = name.to_ascii_uppercase();
        let (w, h) = if up == "STBAR" {
            (320.0, 32.0)
        } else if up.starts_with("STTNUM") || up == "STTPRCNT" || up == "STTMINUS" {
            (14.0, 16.0)
        } else if up.starts_with("STYSNUM") || up.starts_with("STGNUM") {
            (4.0, 6.0)
        } else if up.starts_with("STKEYS") {
            (7.0, 5.0)
        } else if up == "STARMS" {
            (40.0, 32.0)
        } else if up.starts_with("STF") {
            (24.0, 29.0)
        } else if up == "MEDIA0" || up == "CLIPA0" || up == "ARM1A0" || up == "SHELA0" {
            (14.0, 12.0)
        } else if up == "BKEYA0" {
            (8.0, 10.0)
        } else if up == "PISTA0" || up == "SHOTA0" {
            (24.0, 12.0)
        } else {
            return None;
        };
        Some(PatchDims {
            w,
            h,
            left: 0.0,
            top: 0.0,
        })
    }

    fn patches(list: &UiDrawList) -> Vec<(String, f32, f32)> {
        list.cmds
            .iter()
            .filter_map(|c| match c {
                DrawCmd::Patch { name, pos, .. } => Some((name.clone(), pos.x, pos.y)),
                _ => None,
            })
            .collect()
    }

    fn althud_dims(name: &str) -> Option<PatchDims> {
        let up = name.to_ascii_uppercase();
        if up.starts_with("STCFN") {
            return Some(PatchDims {
                w: 7.0,
                h: 7.0,
                left: 0.0,
                top: 0.0,
            });
        }
        dims(name)
    }

    fn althud_snapshot() -> SbarSnapshot {
        SbarSnapshot {
            health: 75,
            keys: 1,
            face: "STFST01".into(),
            inventory: vec![
                ("Clip".into(), 50, 200),
                ("Shell".into(), 0, 50),
                ("BasicArmor".into(), 25, 0),
                ("BlueCard".into(), 1, 1),
                ("Fist".into(), 1, 1),
                ("Pistol".into(), 1, 1),
            ],
            icons: vec![
                ("Clip".into(), "CLIPA0".into()),
                ("Shell".into(), "SHELA0".into()),
                ("BasicArmor".into(), "ARM1A0".into()),
                ("BlueCard".into(), "STKEYS0".into()),
                ("Pistol".into(), "PISTA0".into()),
                ("Shotgun".into(), "SHOTA0".into()),
            ],
            weapons: vec![
                SbarWeapon {
                    class: "Fist".into(),
                    slot: 1,
                    ammo1: None,
                    ammo2: None,
                },
                SbarWeapon {
                    class: "Pistol".into(),
                    slot: 2,
                    ammo1: Some("Clip".into()),
                    ammo2: None,
                },
                SbarWeapon {
                    class: "Shotgun".into(),
                    slot: 3,
                    ammo1: Some("Shell".into()),
                    ammo2: None,
                },
            ],
            spawn_sprites: vec![("BlueCard".into(), "BKEYA0".into())],
            ready_weapon: Some("Pistol".into()),
            ammo1: Some("Clip".into()),
            ..SbarSnapshot::default()
        }
    }

    fn cmd_lines(list: &UiDrawList) -> String {
        let mut out = String::new();
        for c in &list.cmds {
            if let DrawCmd::Patch {
                name,
                pos,
                size,
                alpha,
                recolor,
                ..
            } = c
            {
                let cr = recolor.map_or(String::new(), |r| {
                    format!(
                        " cr{:02x}{:02x}{:02x}",
                        (r[1][0] * 255.0) as u8,
                        (r[1][1] * 255.0) as u8,
                        (r[1][2] * 255.0) as u8
                    )
                });
                out.push_str(&format!(
                    "{name} {:.1},{:.1} {:.1}x{:.1} a{alpha:.2}{cr}\n",
                    pos.x, pos.y, size.x, size.y
                ));
            }
        }
        out
    }

    /// UZ4 acceptance: the stock UZDoom `AltHud` (vendored ZScript) draws
    /// stats, health, armor, keys, ammo and weapons; the command list is
    /// checked against `testdata/goldens/uz4_althud.txt`
    /// (`ROOM_UPDATE_GOLDEN=1` rewrites it).
    #[test]
    fn althud_matches_its_golden() {
        let mut bar = ScriptStatusBar::new(stdlib_program(), "DoomStatusBar").unwrap();
        let mut list = UiDrawList::new();
        bar.draw_althud(&althud_snapshot(), (640, 400), &althud_dims, &mut list);
        assert!(bar.last_error.is_none(), "{:?}", bar.last_error);
        let got = cmd_lines(&list);
        // Health 75 (green), armor 25 (gold, under `hud_armor_yellow`), the blue
        // key's spawn sprite, clip and shell rows, the pistol.
        for want in [
            "MEDIA0 ", "ARM1A0 ", "BKEYA0 ", "CLIPA0 ", "SHELA0 ", "PISTA0 ",
        ] {
            assert!(got.contains(want), "{want} missing:\n{got}");
        }
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../testdata/goldens/uz4_althud.txt");
        if std::env::var_os("ROOM_UPDATE_GOLDEN").is_some() {
            std::fs::write(&path, &got).unwrap();
        }
        let want = std::fs::read_to_string(&path).unwrap();
        assert_eq!(got, want, "alt HUD differs from {}", path.display());
        // `hud_swaphealtharmor` swaps the two rows.
        bar.set_cvar("hud_swaphealtharmor", 1);
        let mut swapped = UiDrawList::new();
        bar.draw_althud(&althud_snapshot(), (640, 400), &althud_dims, &mut swapped);
        let y_of = |list: &UiDrawList, n: &str| {
            list.cmds.iter().find_map(|c| match c {
                DrawCmd::Patch { name, pos, .. } if name == n => Some(pos.y),
                _ => None,
            })
        };
        assert!(y_of(&list, "MEDIA0") < y_of(&list, "ARM1A0"));
        assert!(y_of(&swapped, "MEDIA0") > y_of(&swapped, "ARM1A0"));
    }

    #[test]
    fn hud_space_follows_uzdoom_ui_scale() {
        // No window size: the 320×200 box itself.
        assert_eq!(HudSpace::for_screen((0.0, 0.0)), HudSpace::BOX);
        // 1280×720: UI scale 1 (640×400 fits once), the box is 3.6× and
        // centred, 17.8 box units in from each side.
        let sp = HudSpace::for_screen((1280.0, 720.0));
        assert_eq!((sp.w, sp.h), (1280.0, 720.0));
        assert!((sp.k - 1.0 / 3.6).abs() < 1e-5);
        assert!((sp.x0 + 64.0 / 3.6).abs() < 1e-3);
        assert!(sp.y0.abs() < 1e-5);
        // 1920×1080: UI scale 2, a 960×540 HUD.
        let sp = HudSpace::for_screen((1920.0, 1080.0));
        assert_eq!((sp.w, sp.h), (960.0, 540.0));
        // Every HUD pixel lands on a whole number of window pixels.
        assert!((sp.k * 5.4 - 2.0).abs() < 1e-5);
    }

    /// `BeginHUD` on a wide window: `DI_SCREEN_RIGHT_BOTTOM` is the window's
    /// corner, not the 320×200 box's (PB's two clusters overlapped in the
    /// middle of the screen), and pictures are scaled by the UI scale.
    #[test]
    fn fullscreen_hud_anchors_to_the_window_edges() {
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../wadsrc/uzdoom");
        let entry = std::fs::read_to_string(root.join("zscript.txt")).unwrap();
        let mut sources = vec![("zscript.txt".to_owned(), entry.clone())];
        for line in entry.lines() {
            if let Some(rest) = line.trim().strip_prefix("#include") {
                let path = rest.trim().trim_matches('"');
                let text = std::fs::read_to_string(root.join(path)).unwrap();
                sources.push((path.replace('\\', "/"), text));
            }
        }
        sources.push((
            "bar.zs".into(),
            r#"
class CornerBar : BaseStatusBar
{
    override void Draw(int state, double TicFrac)
    {
        BeginHUD();
        DrawImage("PANELR", (-10, -10), DI_SCREEN_RIGHT_BOTTOM | DI_ITEM_RIGHT_BOTTOM);
        DrawImage("PANELL", (10, 10), DI_SCREEN_LEFT_TOP | DI_ITEM_LEFT_TOP);
        // PB scales its health bar by this; 0 left the bar empty.
        if (CPlayer.mo.GetMaxHealth() == 100) DrawImage("PANELM", (0, 0), DI_SCREEN_CENTER);
    }
}
"#
            .into(),
        ));
        let program = room_zscript::check(&sources).program.unwrap();
        let mut bar = ScriptStatusBar::new(program, "CornerBar").unwrap();
        bar.set_screen_size(1280.0, 720.0);
        let dims = |name: &str| {
            name.starts_with("PANEL").then_some(PatchDims {
                w: 100.0,
                h: 40.0,
                left: 0.0,
                top: 0.0,
            })
        };
        let snap = SbarSnapshot::default();
        let mut list = UiDrawList::new();
        bar.draw(&snap, true, 0.0, &dims, &mut list);
        let mut list = UiDrawList::new();
        bar.draw(&snap, true, 0.0, &dims, &mut list);
        assert!(bar.last_error.is_none(), "{:?}", bar.last_error);
        let sp = HudSpace::for_screen((1280.0, 720.0));
        let at = |want: &str| {
            list.cmds
                .iter()
                .find_map(|c| match c {
                    DrawCmd::Patch {
                        name, pos, size, ..
                    } if name == want => Some((*pos, *size)),
                    _ => None,
                })
                .unwrap_or_else(|| panic!("{want} not drawn: {:?}", list.cmds))
        };
        let (pos, size) = at("PANELR");
        // Window pixels: right edge at 1270, bottom at 710, 100×40.
        let px = |v: f32| (v - sp.x0) / sp.k;
        let py = |v: f32| (v - sp.y0) / sp.k;
        assert!(
            (px(pos.x + size.x) - 1270.0).abs() < 0.01,
            "{pos:?} {size:?}"
        );
        assert!(
            (py(pos.y + size.y) - 710.0).abs() < 0.01,
            "{pos:?} {size:?}"
        );
        assert!((size.x / sp.k - 100.0).abs() < 0.01);
        let (pos, _) = at("PANELL");
        assert!((px(pos.x) - 10.0).abs() < 0.01, "{pos:?}");
        assert!((py(pos.y) - 10.0).abs() < 0.01, "{pos:?}");
        // Left of the 320×200 box on a 16:9 window.
        assert!(pos.x < 0.0);
        let _ = at("PANELM");
    }

    /// The shell's loop: pictures start unloaded, the bar reports them, the
    /// shell loads them, and the next frame draws them (the app used to drop
    /// the report, and PB's HUD never appeared).
    #[test]
    fn unknown_pictures_are_reported_once_and_drawn_once_loaded() {
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../wadsrc/uzdoom");
        let entry = std::fs::read_to_string(root.join("zscript.txt")).unwrap();
        let mut sources = vec![("zscript.txt".to_owned(), entry.clone())];
        for line in entry.lines() {
            if let Some(rest) = line.trim().strip_prefix("#include") {
                let path = rest.trim().trim_matches('"');
                let text = std::fs::read_to_string(root.join(path)).unwrap();
                sources.push((path.replace('\\', "/"), text));
            }
        }
        sources.push((
            "bar.zs".into(),
            r#"
class LoadBar : BaseStatusBar
{
    override void Draw(int state, double TicFrac)
    {
        BeginHUD();
        DrawImage("PANELR", (-10, -10), DI_SCREEN_RIGHT_BOTTOM | DI_ITEM_RIGHT_BOTTOM);
        DrawImage("NOSUCHPIC", (10, 10), DI_SCREEN_LEFT_TOP | DI_ITEM_LEFT_TOP);
    }
}
"#
            .into(),
        ));
        let program = room_zscript::check(&sources).program.unwrap();
        let mut bar = ScriptStatusBar::new(program, "LoadBar").unwrap();
        bar.set_screen_size(1280.0, 720.0);
        let loaded = std::cell::RefCell::new(Vec::<String>::new());
        let dims = |name: &str| {
            loaded
                .borrow()
                .iter()
                .any(|n| n == name)
                .then_some(PatchDims {
                    w: 100.0,
                    h: 40.0,
                    left: 0.0,
                    top: 0.0,
                })
        };
        let snap = SbarSnapshot::default();
        let drawn = |list: &UiDrawList| {
            patches(list)
                .into_iter()
                .map(|p| p.0)
                .collect::<Vec<String>>()
        };
        let mut list = UiDrawList::new();
        bar.draw(&snap, true, 0.0, &dims, &mut list);
        assert!(drawn(&list).is_empty(), "unsized pictures wait");
        let mut asked = bar.take_unknown_patches();
        asked.sort();
        assert_eq!(asked, ["NOSUCHPIC", "PANELR"]);
        // The shell loads what exists; NOSUCHPIC does not.
        loaded.borrow_mut().push("PANELR".into());
        let mut list = UiDrawList::new();
        bar.draw(&snap, true, 0.0, &dims, &mut list);
        assert_eq!(drawn(&list), ["PANELR"]);
        assert!(bar.take_unknown_patches().is_empty(), "asked once");
    }

    /// PB keeps `weap = CPlayer.ReadyWeapon` from its `Tick` and draws the
    /// ammo panel from it; frames between two tics (the screen draws faster
    /// than 35 Hz) must see the same, live objects (they were remade every
    /// frame, so the panel flickered).
    #[test]
    fn objects_kept_from_tick_stay_valid_between_tics() {
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../wadsrc/uzdoom");
        let entry = std::fs::read_to_string(root.join("zscript.txt")).unwrap();
        let mut sources = vec![("zscript.txt".to_owned(), entry.clone())];
        for line in entry.lines() {
            if let Some(rest) = line.trim().strip_prefix("#include") {
                let path = rest.trim().trim_matches('"');
                let text = std::fs::read_to_string(root.join(path)).unwrap();
                sources.push((path.replace('\\', "/"), text));
            }
        }
        sources.push((
            "bar.zs".into(),
            r#"
class KeepBar : BaseStatusBar
{
    Weapon weap;
    Weapon oldWeap;
    int switches;
    override void Tick()
    {
        Super.Tick();
        weap = CPlayer.ReadyWeapon;
        if (oldWeap && oldWeap != weap) switches++;
        oldWeap = weap;
    }
    override void Draw(int state, double TicFrac)
    {
        BeginHUD();
        if (weap && weap == CPlayer.ReadyWeapon && weap.Ammo1 && weap.Ammo1.Amount == 7)
            DrawImage("AMMOOK", (-10, -10), DI_SCREEN_RIGHT_BOTTOM | DI_ITEM_RIGHT_BOTTOM);
        if (switches > 0) DrawImage("SWITCHED", (0, 0), DI_SCREEN_CENTER);
    }
}
"#
            .into(),
        ));
        let program = room_zscript::check(&sources).program.unwrap();
        let mut bar = ScriptStatusBar::new(program, "KeepBar").unwrap();
        let snap = SbarSnapshot {
            health: 100,
            inventory: vec![("Pistol".into(), 1, 1), ("Clip".into(), 7, 200)],
            ready_weapon: Some("Pistol".into()),
            ammo1: Some("Clip".into()),
            weapons: vec![SbarWeapon {
                class: "Pistol".into(),
                slot: 2,
                ammo1: Some("Clip".into()),
                ammo2: None,
            }],
            ..SbarSnapshot::default()
        };
        let dims = |_: &str| {
            Some(PatchDims {
                w: 10.0,
                h: 10.0,
                left: 0.0,
                top: 0.0,
            })
        };
        let mut shown = Vec::new();
        for frame in 0..8u32 {
            // Two screen frames per tic.
            bar.set_game_tic(frame / 2);
            let mut list = UiDrawList::new();
            bar.draw(&snap, true, 0.0, &dims, &mut list);
            assert!(bar.last_error.is_none(), "{:?}", bar.last_error);
            let names: Vec<String> = patches(&list).into_iter().map(|p| p.0).collect();
            shown.push((
                names.iter().any(|n| n == "AMMOOK"),
                names.iter().any(|n| n == "SWITCHED"),
            ));
        }
        assert!(shown.iter().all(|(ok, _)| *ok), "{shown:?}");
        assert!(
            shown.iter().all(|(_, sw)| !*sw),
            "same weapon every tic: {shown:?}"
        );
    }

    /// Project Brutality shape: the ready weapon is a DECORATE class the
    /// script program lacks, so it is typed as its nearest script ancestor
    /// (`PB_WeaponBase(CPlayer.ReadyWeapon)` holds), and `DrawBar` draws the
    /// empty image, then the full one clipped to the filled part.
    #[test]
    fn decorate_weapon_casts_to_its_script_ancestor_and_bars_draw() {
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../wadsrc/uzdoom");
        let entry = std::fs::read_to_string(root.join("zscript.txt")).unwrap();
        let mut sources = vec![("zscript.txt".to_owned(), entry.clone())];
        for line in entry.lines() {
            if let Some(rest) = line.trim().strip_prefix("#include") {
                let path = rest.trim().trim_matches('"');
                let text = std::fs::read_to_string(root.join(path)).unwrap();
                sources.push((path.replace('\\', "/"), text));
            }
        }
        sources.push((
            "bar.zs".into(),
            r#"
class TestModWeapon : Weapon { int magSize; }
class TestBar : BaseStatusBar
{
    override void Draw(int state, double TicFrac)
    {
        Super.Draw(state, TicFrac);
        BeginHUD();
        let w = TestModWeapon(CPlayer.ReadyWeapon);
        if (w) DrawImage("WEAPOK", (10, 10), DI_ITEM_OFFSETS);
        else DrawImage("WEAPNO", (10, 10), DI_ITEM_OFFSETS);
        DrawBar("BARON", "BAROFF", 30, 100, (20, 50), 0, SHADER_HORZ, DI_ITEM_OFFSETS);
    }
}
"#
            .into(),
        ));
        let result = room_zscript::check(&sources);
        let program = result.program.unwrap();
        let mut bar = ScriptStatusBar::new(program, "TestBar").unwrap();
        let snap = SbarSnapshot {
            health: 100,
            inventory: vec![("DecoRifle".into(), 1, 1)],
            ready_weapon: Some("DecoRifle".into()),
            ancestors: vec![(
                "DecoRifle".into(),
                vec![
                    "TestModWeapon".into(),
                    "Weapon".into(),
                    "StateProvider".into(),
                    "Inventory".into(),
                    "Actor".into(),
                ],
            )],
            ..SbarSnapshot::default()
        };
        let bar_dims = |name: &str| {
            let up = name.to_ascii_uppercase();
            (up.starts_with("BAR") || up.starts_with("WEAP")).then_some(PatchDims {
                w: 100.0,
                h: 10.0,
                left: 0.0,
                top: 0.0,
            })
        };
        // The first frame learns which patches the script draws; their sizes
        // are known from the next one on (as in the app).
        let mut list = UiDrawList::new();
        bar.draw(&snap, true, 0.0, &bar_dims, &mut list);
        let mut list = UiDrawList::new();
        bar.draw(&snap, true, 0.0, &bar_dims, &mut list);
        assert!(bar.last_error.is_none(), "{:?}", bar.last_error);
        let names: Vec<String> = patches(&list).into_iter().map(|p| p.0).collect();
        assert!(names.iter().any(|n| n == "WEAPOK"), "{names:?}");
        let off = names.iter().position(|n| n == "BAROFF").expect("empty bar");
        let on = names.iter().position(|n| n == "BARON").expect("full bar");
        assert!(off < on, "{names:?}");
        // The full image is clipped to 30 % of the bar's width.
        let clip = list.cmds.iter().find_map(|c| match c {
            DrawCmd::Clip { rect: Some(r) } => Some(*r),
            _ => None,
        });
        let r = clip.expect("clip around the filled part");
        assert!((r.w - 30.0).abs() < 0.6, "{r:?}");
        assert!((r.h - 10.0).abs() < 0.6, "{r:?}");
    }

    #[test]
    fn doom_status_bar_draws_the_stock_bar_from_zscript() {
        let mut bar = ScriptStatusBar::new(stdlib_program(), "DoomStatusBar").unwrap();
        let mut slots = [false; 10];
        slots[2] = true;
        let snap = SbarSnapshot {
            health: 100,
            keys: 1,
            slots,
            face: "STFST01".into(),
            inventory: vec![
                ("Clip".into(), 50, 200),
                ("Shell".into(), 0, 50),
                ("BasicArmor".into(), 25, 0),
            ],
            ready_weapon: Some("Pistol".into()),
            ammo1: Some("Clip".into()),
            ..SbarSnapshot::default()
        };
        let mut list = UiDrawList::new();
        bar.draw(&snap, false, 0.0, &dims, &mut list);
        assert!(bar.last_error.is_none(), "{:?}", bar.last_error);
        let p = patches(&list);
        let has = |name: &str, x: f32, y: f32| {
            p.iter()
                .any(|(n, px, py)| n == name && (px - x).abs() < 0.5 && (py - y).abs() < 0.5)
        };
        assert!(has("STBAR", 0.0, 168.0), "{p:?}");
        assert!(has("STTPRCNT", 90.0, 171.0), "{p:?}");
        // Health 100, right-aligned at 90 in 14-wide cells.
        assert!(has("STTNUM1", 48.0, 171.0), "{p:?}");
        assert!(has("STTNUM0", 62.0, 171.0), "{p:?}");
        assert!(has("STTNUM0", 76.0, 171.0), "{p:?}");
        // Ammo 50 right-aligned at 44, armor 25 at 221.
        assert!(has("STTNUM5", 16.0, 171.0), "{p:?}");
        assert!(has("STTNUM2", 193.0, 171.0), "{p:?}");
        // Arms: slot 2 owned, slot 3 not.
        assert!(has("STARMS", 104.0, 168.0), "{p:?}");
        assert!(has("STYSNUM2", 111.0, 172.0), "{p:?}");
        assert!(has("STGNUM3", 123.0, 172.0), "{p:?}");
        // Blue card in the first key box.
        assert!(has("STKEYS0", 239.0, 171.0), "{p:?}");
        // Ammo table: 50 of 200 clips.
        assert!(has("STYSNUM5", 280.0, 173.0), "{p:?}");
        assert!(has("STYSNUM2", 302.0, 173.0), "{p:?}");
        // The mugshot.
        assert!(has("STFST01", 143.0, 168.0), "{p:?}");
    }

    #[test]
    fn doom_status_bar_fullscreen_counts_from_the_screen_edges() {
        let mut bar = ScriptStatusBar::new(stdlib_program(), "DoomStatusBar").unwrap();
        let snap = SbarSnapshot {
            health: 75,
            face: "STFST01".into(),
            inventory: vec![("Clip".into(), 50, 200), ("BasicArmor".into(), 25, 0)],
            icons: vec![
                ("Clip".into(), "CLIPA0".into()),
                ("BasicArmor".into(), "ARM1A0".into()),
            ],
            ready_weapon: Some("Pistol".into()),
            ammo1: Some("Clip".into()),
            ..SbarSnapshot::default()
        };
        let mut list = UiDrawList::new();
        bar.draw(&snap, true, 0.0, &dims, &mut list);
        assert!(bar.last_error.is_none(), "{:?}", bar.last_error);
        let p = patches(&list);
        let has = |name: &str, x: f32, y: f32| {
            p.iter()
                .any(|(n, px, py)| n == name && (px - x).abs() < 0.5 && (py - y).abs() < 0.5)
        };
        // Medkit bottom-centered on (20, 198); health " 75" from x 44 at y 180.
        assert!(has("MEDIA0", 13.0, 186.0), "{p:?}");
        assert!(has("STTNUM7", 58.0, 180.0), "{p:?}");
        assert!(has("STTNUM5", 72.0, 180.0), "{p:?}");
        // Armor 25 at y 160 with its icon.
        assert!(has("STTNUM2", 58.0, 160.0), "{p:?}");
        assert!(p.iter().any(|(n, _, _)| n == "ARM1A0"), "{p:?}");
        // Ammo " 50" right-aligned at 320 - 30.
        assert!(has("STTNUM5", 262.0, 180.0), "{p:?}");
        assert!(has("STTNUM0", 276.0, 180.0), "{p:?}");
        assert!(p.iter().any(|(n, _, _)| n == "CLIPA0"), "{p:?}");
    }

    #[test]
    fn doom_status_bar_draws_the_inventory_bar_while_it_is_open() {
        let mut bar = ScriptStatusBar::new(stdlib_program(), "DoomStatusBar").unwrap();
        let mut snap = SbarSnapshot {
            health: 100,
            face: "STFST01".into(),
            inventory: vec![
                ("Clip".into(), 50, 200),
                ("ArtiHealth".into(), 3, 16),
                ("ArtiTorch".into(), 1, 16),
            ],
            icons: vec![
                ("ArtiHealth".into(), "MEDIA0".into()),
                ("ArtiTorch".into(), "CLIPA0".into()),
            ],
            ready_weapon: Some("Pistol".into()),
            ammo1: Some("Clip".into()),
            inv_sel: Some("ArtiTorch".into()),
            inv_bar: vec!["ArtiHealth".into(), "ArtiTorch".into()],
            inventory_tics: 100,
            ..SbarSnapshot::default()
        };
        let mut list = UiDrawList::new();
        bar.draw(&snap, false, 0.0, &dims, &mut list);
        assert!(bar.last_error.is_none(), "{:?}", bar.last_error);
        let p = patches(&list);
        let center = |name: &str| {
            let d = dims(name).unwrap();
            p.iter()
                .rev()
                .find(|(n, _, _)| n == name)
                .map(|(_, x, y)| (x + d.w / 2.0, y + d.h / 2.0))
        };
        // Seven 32-wide boxes from (48, 169): items centred in the first two.
        let (hx, hy) = center("MEDIA0").unwrap_or_else(|| panic!("first bar item drawn: {p:?}"));
        let (tx, _) = center("CLIPA0").expect("second bar item drawn");
        assert!(
            (hx - 64.0).abs() < 1.0 && (hy - 185.0).abs() < 1.0,
            "{hx},{hy} {p:?}"
        );
        assert!((tx - 96.0).abs() < 1.0, "{tx} {p:?}");
        // The selected item takes the face's place, bottom-centred on (160, 198).
        let sel = dims("CLIPA0").unwrap();
        assert!(
            p.iter().any(|(n, x, y)| n == "CLIPA0"
                && (x + sel.w / 2.0 - 160.0).abs() < 1.0
                && (y + sel.h - 198.0).abs() < 1.0),
            "{p:?}"
        );
        // Closed again: only the selected item's icon is left.
        snap.inventory_tics = 0;
        let mut list = UiDrawList::new();
        bar.draw(&snap, false, 0.0, &dims, &mut list);
        let p = patches(&list);
        assert!(!p.iter().any(|(n, _, _)| n == "MEDIA0"), "{p:?}");
        assert_eq!(
            p.iter().filter(|(n, _, _)| n == "CLIPA0").count(),
            1,
            "{p:?}"
        );
    }
}
