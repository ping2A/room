//! UZ3 (docs/36): the rest of the vendored UZDoom stdlib's native imports.
//!
//! Three kinds:
//!
//! - **Computed** natives read the mirrored actor fields and the per-call /
//!   per-level tables in [`ZvmPlayHost`](crate::ZvmPlayHost) (sight,
//!   inventory, census, class tables) and answer directly.
//! - **Queued** natives record a [`ZsNativeOp`](crate::ZsNativeOp) with their
//!   receiver and raw arguments; the playsim applies it after the call
//!   (`room-sim` `apply_zscript_native_ops`). Models / bones (UZ7) and
//!   post-process shaders go this way too.
//! - **Engine-managed** natives (`BeginPlay`, `Tick`, floor clipping,
//!   interpolation resets, sound precaching, …) are work Room's playsim does
//!   natively around every script call, so the script-side call has nothing
//!   left to do.
//!
//! Natives that only make sense inside a host Room does not run in play
//! scope (menus, save games, intermission, joystick and reverb editors, the
//! map-load `LevelPostProcessor`, the 2D `Screen` / status bar drawers when no
//! status bar VM is attached) are [`HOST_NATIVES`]: they trap with a message
//! naming the host. The status bar VM (`room-ui`) replaces the drawing ones
//! with real implementations.

use crate::heap::{ObjHandle, ObjKind};
use crate::interp::Value;
use crate::natives::{NativeRegs, NativeTable, ZsNativeOp, ZvmCtx};
use crate::trap::Trap;
use crate::world_natives::{
    arg_a, arg_f, arg_i, float_at, inventory_entry, pos_of, rad_to_deg, ret_bool, ret_f, ret_vec,
};

fn s_arg(ctx: &ZvmCtx<'_>, regs: &NativeRegs<'_>, k: usize) -> String {
    ctx.heap.str(regs.s.get(k).copied().unwrap_or_default()).to_owned()
}

fn ret_s(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>, text: &str) {
    let h = ctx.heap.intern_string(text);
    if regs.s.len() > 1 {
        regs.s[1] = h;
    } else if let Some(slot) = regs.s.first_mut() {
        *slot = h;
    }
}

fn ret_i(regs: &mut NativeRegs<'_>, v: i64) {
    if let Some(slot) = regs.i.first_mut() {
        *slot = v;
    }
    if let Some(slot) = regs.f.first_mut() {
        *slot = v as f64;
    }
}

/// Class name of a script object (lowercase layout key).
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

/// Record the running native with its receiver (`a[0]`) and raw arguments.
fn queue(ctx: &mut ZvmCtx<'_>, regs: &NativeRegs<'_>) {
    let [ni, nf, ns, _] = ctx.play.native_arg_counts;
    let name = ctx.play.native_name.to_ascii_lowercase();
    let s = regs
        .s
        .iter()
        .take(ns)
        .map(|h| ctx.heap.str(*h).to_owned())
        .collect();
    ctx.play.native_ops.push(ZsNativeOp {
        name,
        recv: arg_a(regs, 0),
        objs: regs.a.iter().skip(1).copied().collect(),
        i: regs.i.iter().take(ni).copied().collect(),
        f: regs.f.iter().take(nf).copied().collect(),
        s,
    });
}

fn queued(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
    queue(ctx, regs);
    ret_i(regs, 0);
    Ok(())
}

/// Queued, returns `true` (`UsePuzzleItem`, `Slam`).
fn queued_true(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
    queue(ctx, regs);
    ret_bool(regs, true);
    Ok(())
}

/// Work the playsim already does natively around script calls.
fn engine_managed(_ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
    ret_i(regs, 0);
    Ok(())
}

fn ret_true(_ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
    ret_bool(regs, true);
    Ok(())
}

/// A native provided by a host other than play scope (see [`HOST_NATIVES`]).
fn host_trap(ctx: &mut ZvmCtx<'_>, _regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
    let name = ctx.play.native_name.clone();
    let lower = name.to_ascii_lowercase();
    let host = HOST_NATIVES
        .iter()
        .find(|(prefix, _)| lower.starts_with(prefix))
        .map_or("UI", |(_, host)| host);
    Err(Trap::Native(format!(
        "{name}: needs the {host} host, which Room does not run in this VM"
    )))
}

/// Native-name prefixes served by a host other than play scope, and that
/// host. Calls trap unless the host VM replaces them.
pub const HOST_NATIVES: &[(&str, &str)] = &[
    ("screen.", "2D drawing (status bar / menu)"),
    ("statusbarcore.", "status bar"),
    ("basestatusbar.", "status bar"),
    ("sbarinfo.", "SBARINFO status bar"),
    ("interbackground.", "intermission"),
    ("intermissioncontroller.", "intermission"),
    ("movieplayer.", "cutscene player"),
    ("savegamemanager.", "save menu"),
    ("joystickconfig.", "joystick menu"),
    ("keybindings.", "key binding menu"),
    ("reverbedit.", "reverb editor menu"),
    ("newplayermenu.", "player setup menu"),
    ("advancedfontmenu", "font options menu"),
    ("font.getglyphheight", "UI font"),
    ("levelpostprocessor.", "map loader (LevelPostProcessor)"),
    ("vertex.index", "map loader (LevelPostProcessor)"),
    ("object.setplanelight", "map loader (LevelPostProcessor)"),
    ("object.settexture", "map loader (LevelPostProcessor)"),
    ("object.draw", "menu"),
    ("object.onmenucreated", "menu"),
    ("object.ontick", "menu"),
    ("object.resize", "menu"),
    ("object.ticker", "menu"),
    ("super.init", "menu"),
    ("super.ondestroy", "status bar"),
];

/// Every host native the vendored stdlib imports (registered as traps).
const HOST_NATIVE_NAMES: &[&str] = &[
    "screen.clear",
    "screen.clearcliprect",
    "screen.dim",
    "screen.drawchar",
    "screen.drawframe",
    "screen.drawlineframe",
    "screen.drawtext",
    "screen.drawtexture",
    "screen.drawthickline",
    "screen.setcliprect",
    "screen.setoffset",
    "screen.setscreenfade",
    "statusbarcore.beginhud",
    "statusbarcore.beginstatusbar",
    "statusbarcore.drawimage",
    "statusbarcore.drawstring",
    "statusbarcore.drawtexture",
    "statusbarcore.fill",
    "statusbarcore.setcliprect",
    "statusbarcore.setsize",
    "statusbarcore.statusbartorealcoords",
    "basestatusbar.dodrawautomaphud",
    "basestatusbar.drawcrosshair",
    "basestatusbar.receivedweapon",
    "basestatusbar.refreshbackground",
    "basestatusbar.tick",
    "basestatusbar.updatescreengeometry",
    "sbarinfo.attachtoplayer",
    "sbarinfo.destroy",
    "sbarinfo.draw",
    "sbarinfo.getprotrusion",
    "sbarinfo.mustdrawlog",
    "sbarinfo.newgame",
    "sbarinfo.showpop",
    "sbarinfo.tick",
    "interbackground.drawbackground",
    "interbackground.isusingmusic",
    "interbackground.loadbackground",
    "interbackground.updateanimatedback",
    "intermissioncontroller.drawer",
    "intermissioncontroller.responder",
    "intermissioncontroller.start",
    "intermissioncontroller.ticker",
    "movieplayer.destroy",
    "movieplayer.frame",
    "movieplayer.start",
    "savegamemanager.dosave",
    "savegamemanager.drawsavepic",
    "savegamemanager.extractsavedata",
    "savegamemanager.insertnewsavenode",
    "savegamemanager.loadsavegame",
    "savegamemanager.readsavestrings",
    "savegamemanager.removenewsavenode",
    "savegamemanager.removesaveslot",
    "savegamemanager.savegamecount",
    "savegamemanager.setfileinfo",
    "savegamemanager.unloadsavedata",
    "joystickconfig.getenabled",
    "joystickconfig.getenabledinbackground",
    "joystickconfig.reset",
    "joystickconfig.setaxisdeadzone",
    "joystickconfig.setaxisdigitalthreshold",
    "joystickconfig.setaxisresponsecurve",
    "joystickconfig.setaxisresponsecurvepoint",
    "joystickconfig.setaxisscale",
    "joystickconfig.setenabled",
    "joystickconfig.setenabledinbackground",
    "joystickconfig.sethapticsstrength",
    "joystickconfig.setsensitivity",
    "keybindings.getallkeysforcommand",
    "keybindings.setbind",
    "keybindings.unbindacommand",
    "reverbedit.fillsavemenu",
    "reverbedit.fillselectmenu",
    "reverbedit.getsaveselection",
    "reverbedit.getselectedenvironment",
    "reverbedit.getvalue",
    "reverbedit.graycheck",
    "reverbedit.setvalue",
    "reverbedit.togglesaveselection",
    "newplayermenu.updatecolorsets",
    "newplayermenu.updateskinoptions",
    "advancedfontmenu.filladvancedfontmenu",
    "advancedfontmenuremapchoicemenu.filladvancedfontmenuremapchoices",
    "font.getglyphheight",
    "levelpostprocessor.addsectortag",
    "levelpostprocessor.clearsectortags",
    "levelpostprocessor.fliplinesiderefs",
    "levelpostprocessor.offsetsectorplane",
    "levelpostprocessor.setlinesectorref",
    "levelpostprocessor.setlinevertexes",
    "levelpostprocessor.setthingargument",
    "levelpostprocessor.setthingednum",
    "levelpostprocessor.setthingflags",
    "levelpostprocessor.setthingid",
    "levelpostprocessor.setthingskills",
    "levelpostprocessor.setthingspecial",
    "levelpostprocessor.setthingxy",
    "levelpostprocessor.setthingz",
    "levelpostprocessor.setvertex",
    "vertex.index",
    "object.setplanelight",
    "object.settexture",
    "object.settexturexoffset",
    "object.settextureyoffset",
    "object.settextureyscale",
    "object.draw",
    "object.drawframe",
    "object.drawselector",
    "object.onmenucreated",
    "object.ontick",
    "object.resize",
    "object.ticker",
    "super.init",
    "super.ondestroy",
];

/// Natives queued for the playsim ([`ZsNativeOp`]).
pub const QUEUED_NATIVES: &[&str] = &[
    "actor.activate",
    "actor.deactivate",
    "actor.destroyallinventory",
    "actor.startsoundsequence",
    "actor.displaynametag",
    "actor.setanimation",
    "actor.setanimationframerate",
    "actor.setmodelflag",
    "actor.clearmodelflag",
    "actor.resetmodelflags",
    "actor.setbonerotation",
    "actor.setnamedbonerotation",
    "actor.setbonetranslation",
    "actor.setnamedbonetranslation",
    "actor.setbonescaling",
    "actor.setnamedbonescaling",
    "actor.clearboneoffsets",
    "playerinfo.poisonplayer",
    "playerinfo.setfullbrightmode",
    "playerinfo.setsafeflash",
    "playerinfo.setlognumber",
    "playerinfo.setsubtitlenumber",
    "dynamiclight.activatelight",
    "dynamiclight.deactivatelight",
    "dynamiclight.attachlight",
    "sector.setcolor",
    "sector.setfade",
    "sector.removeforcefield",
    "levellocals.replacetextures",
    "spotstate.addspot",
    "spotstate.removespot",
    "ppshader.setenabled",
    "ppshader.setuniform1f",
    "ppshader.setuniform1i",
    "ppshader.setuniform2f",
    "ppshader.setuniform3f",
];

/// Engine-managed natives (see the module docs).
const ENGINE_MANAGED: &[&str] = &[
    "actor.beginplay",
    "thinker.tick",
    "thinker.postbeginplay",
    "actor.adjustfloorclip",
    "actor.checkfakefloortriggers",
    "actor.updatewaterlevel",
    "actor.checkportaltransition",
    "actor.handlespawnflags",
    "actor.clearinterpolation",
    "actor.clearfovinterpolation",
    "object.marksound",
    "ambientsound.markambientsounds",
    "playerpawn.markplayersounds",
    "seqnode.markprecachesounds",
    "seqnode.addchoice",
    "fcheckposition.clearlastripped",
    "playerinfo.sendpitchlimits",
    "console.hideconsole",
    "thinkeriterator.reinit",
    "visualthinker.setrenderstyle",
    "visualthinker.updatesector",
    "visualthinker.updatespriteinfo",
];

/// `GetClass()`: the receiver's class.
fn get_class(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
    let class = obj_class(ctx, arg_a(regs, 0));
    ret_s(ctx, regs, &class);
    Ok(())
}

/// `class.GetClassName()`: class values are names already.
fn class_name(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
    let name = s_arg(ctx, regs, 0);
    ret_s(ctx, regs, &name);
    Ok(())
}

/// `class.IsAbstract()`.
fn class_is_abstract(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
    let name = s_arg(ctx, regs, 0).to_ascii_lowercase();
    let abs = ctx.play.abstract_classes.contains(&name);
    ret_bool(regs, abs);
    Ok(())
}

/// `class.GetParentClass()`.
fn class_parent(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
    let name = s_arg(ctx, regs, 0).to_ascii_lowercase();
    let parent = ctx.play.class_parents.get(&name).cloned().unwrap_or_default();
    ret_s(ctx, regs, &parent);
    Ok(())
}

/// `GetReplacement(class)`: the class a DECORATE / ZScript `replaces` swaps in.
fn get_replacement(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
    let name = s_arg(ctx, regs, 0);
    let mut cur = name.to_ascii_lowercase();
    for _ in 0..16 {
        match ctx.play.replacements.get(&cur) {
            Some(next) if !next.eq_ignore_ascii_case(&cur) => cur = next.to_ascii_lowercase(),
            _ => break,
        }
    }
    let out = if cur == name.to_ascii_lowercase() { name } else { cur };
    ret_s(ctx, regs, &out);
    Ok(())
}

/// `GetReplacee(class)`: the class this one replaces.
fn get_replacee(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
    let name = s_arg(ctx, regs, 0);
    let lower = name.to_ascii_lowercase();
    let out = ctx
        .play
        .replacements
        .iter()
        .find(|(_, to)| to.eq_ignore_ascii_case(&lower))
        .map_or(name, |(from, _)| from.clone());
    ret_s(ctx, regs, &out);
    Ok(())
}

fn basic_armor_class(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
    ret_s(ctx, regs, "BasicArmor");
    Ok(())
}

fn hexen_armor_class(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
    ret_s(ctx, regs, "HexenArmor");
    Ok(())
}

/// `GetFlechetteType(other)`: the poison bag a Hexen player class throws.
fn flechette_type(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
    let class = obj_class(ctx, arg_a(regs, 0));
    let bag = if class.contains("fighter") {
        "ArtiPoisonBag3"
    } else if class.contains("cleric") {
        "ArtiPoisonBag1"
    } else if class.contains("mage") {
        "ArtiPoisonBag2"
    } else {
        "ArtiPoisonBag"
    };
    ret_s(ctx, regs, bag);
    Ok(())
}

/// `Color(r, g, b)` / `Color(a, r, g, b)`.
fn color_ctor(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
    let n = ctx.play.native_arg_counts[0];
    let c = |k: usize| (regs.i.get(k).copied().unwrap_or(0) & 0xff) as u32;
    let (a, r, g, b) = if n >= 4 {
        (c(0), c(1), c(2), c(3))
    } else {
        (0, c(0), c(1), c(2))
    };
    let v = (a << 24) | (r << 16) | (g << 8) | b;
    ret_i(regs, i64::from(v as i32));
    Ok(())
}

/// `Quat.FromAngles(yaw, pitch, roll)` (degrees): UZDoom's `TQuaternion::FromAngles`.
fn quat_from_angles(_ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
    let q = quat_from_ypr(arg_f(regs, 0, 0.0), arg_f(regs, 1, 0.0), arg_f(regs, 2, 0.0));
    ret_vec(regs, &q);
    Ok(())
}

/// Quaternion (x, y, z, w) for yaw / pitch / roll in degrees, composed
/// yaw (Z) · pitch (Y) · roll (X) like UZDoom's `FromAngles`.
#[must_use]
pub fn quat_from_ypr(yaw: f64, pitch: f64, roll: f64) -> [f64; 4] {
    let (sy, cy) = (yaw.to_radians() * 0.5).sin_cos();
    let (sp, cp) = (pitch.to_radians() * 0.5).sin_cos();
    let (sr, cr) = (roll.to_radians() * 0.5).sin_cos();
    [
        cy * cp * sr - sy * sp * cr,
        cy * sp * cr + sy * cp * sr,
        sy * cp * cr - cy * sp * sr,
        cy * cp * cr + sy * sp * sr,
    ]
}

fn angle_of(ctx: &ZvmCtx<'_>, h: ObjHandle) -> f64 {
    float_at(ctx.heap, h, ctx.play.actor_slots.angle, 0).unwrap_or(0.0)
}

fn sees(ctx: &ZvmCtx<'_>, h: ObjHandle) -> bool {
    ctx.play.sight.iter().any(|(o, v)| *o == h && *v)
}

fn wrap180(mut d: f64) -> f64 {
    while d > 180.0 {
        d -= 360.0;
    }
    while d < -180.0 {
        d += 360.0;
    }
    d
}

/// Is `other` within `fov` degrees (total) of `viewer`'s facing?
fn in_fov(ctx: &ZvmCtx<'_>, viewer: ObjHandle, other: ObjHandle, fov: f64) -> bool {
    if fov <= 0.0 || fov >= 360.0 {
        return true;
    }
    let (Some(a), Some(b)) = (pos_of(ctx, viewer), pos_of(ctx, other)) else {
        return false;
    };
    let to = rad_to_deg(room_core::dmath::atan2(b[1] - a[1], b[0] - a[0]));
    wrap180(to - angle_of(ctx, viewer)).abs() <= fov * 0.5
}

/// `CheckFOV(target, fov)`.
fn check_fov(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
    let ok = in_fov(ctx, arg_a(regs, 0), arg_a(regs, 1), arg_f(regs, 0, 0.0));
    ret_bool(regs, ok);
    Ok(())
}

fn dist2d(a: [f64; 3], b: [f64; 3]) -> f64 {
    room_core::dmath::sqrt((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2))
}

/// `CheckIfTargetInLOS(fov, flags, dist_max, dist_close)`: the caller sees
/// its target inside `fov` and range (`dist_max` 0 = any).
fn target_in_los(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
    let me = arg_a(regs, 0);
    let target = ctx.play.self_target;
    let fov = arg_f(regs, 0, 0.0);
    let dist_max = arg_f(regs, 1, 0.0);
    let ok = sees(ctx, target)
        && in_fov(ctx, me, target, fov)
        && match (pos_of(ctx, me), pos_of(ctx, target)) {
            (Some(a), Some(b)) => dist_max <= 0.0 || dist2d(a, b) <= dist_max,
            _ => false,
        };
    ret_bool(regs, ok);
    Ok(())
}

/// `CheckIfInTargetLOS(fov, dist_max, …)`: the target faces the caller.
fn in_target_los(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
    let me = arg_a(regs, 0);
    let target = ctx.play.self_target;
    let fov = arg_f(regs, 0, 0.0);
    let dist_max = arg_f(regs, 1, 0.0);
    let ok = sees(ctx, target)
        && in_fov(ctx, target, me, fov)
        && match (pos_of(ctx, me), pos_of(ctx, target)) {
            (Some(a), Some(b)) => dist_max <= 0.0 || dist2d(a, b) <= dist_max,
            _ => false,
        };
    ret_bool(regs, ok);
    Ok(())
}

/// `CheckLOF(…)`: a clear line of fire to the target (sight).
fn check_lof(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
    let target = ctx.play.self_target;
    let ok = sees(ctx, target);
    ret_bool(regs, ok);
    Ok(())
}

/// `CheckMissileRange()`: Doom's `P_CheckMissileRange` without the random
/// roll (the VM has no access to the playsim RNG): in sight and within the
/// distance where the vanilla chance passes half the time.
fn check_missile_range(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
    let me = arg_a(regs, 0);
    let target = ctx.play.self_target;
    let ok = sees(ctx, target)
        && match (pos_of(ctx, me), pos_of(ctx, target)) {
            (Some(a), Some(b)) => {
                let mut d = dist2d(a, b) - 64.0;
                if !ctx.play.melee_ok {
                    d -= 128.0;
                }
                d.min(200.0) < 128.0
            }
            _ => false,
        };
    ret_bool(regs, ok);
    Ok(())
}

/// Doom key classes per lock: `(lock, [any of these])`.
const DOOM_LOCKS: &[(i64, &[&str])] = &[
    (1, &["redcard"]),
    (2, &["bluecard"]),
    (3, &["yellowcard"]),
    (4, &["redskull"]),
    (5, &["blueskull"]),
    (6, &["yellowskull"]),
    (129, &["redcard", "redskull"]),
    (130, &["bluecard", "blueskull"]),
    (131, &["yellowcard", "yellowskull"]),
    (132, &["redcard", "redskull"]),
    (133, &["bluecard", "blueskull"]),
    (134, &["yellowcard", "yellowskull"]),
];

/// `CheckKeys(locknum, remote, quiet)`: Doom's standard locks against the
/// caller's mirrored inventory (0 always opens; 100 needs any key; 229 all
/// six).
fn check_keys(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
    let me = arg_a(regs, 0);
    let lock = arg_i(regs, 0, 0);
    let has = |ctx: &ZvmCtx<'_>, k: &str| inventory_entry(ctx, me, k).is_some_and(|(n, _)| n > 0);
    let all = ["redcard", "bluecard", "yellowcard", "redskull", "blueskull", "yellowskull"];
    let ok = match lock {
        0 => true,
        100 => all.iter().any(|k| has(ctx, k)),
        229 => all.iter().all(|k| has(ctx, k)),
        n => DOOM_LOCKS
            .iter()
            .find(|(l, _)| *l == n)
            .is_none_or(|(_, keys)| keys.iter().any(|k| has(ctx, k))),
    };
    ret_bool(regs, ok);
    Ok(())
}

/// `CheckProximity(class, distance, count = 1, flags = 0, ptr = AAPTR_DEFAULT)`
/// over the level census: at least `count` live actors of the class
/// (subclasses too unless `CPXF_EXACT`, 1 << 1… UZDoom `CPXF_NOZ` = 1 << 6)
/// within `distance`. `CPXF_LESSOREQUAL` (1) inverts to "at most count".
fn check_proximity(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
    let me = arg_a(regs, 0);
    let class = s_arg(ctx, regs, 0).to_ascii_lowercase();
    let distance = arg_f(regs, 0, 0.0);
    let count = arg_i(regs, 0, 1).max(1);
    let flags = arg_i(regs, 1, 0);
    let Some(origin) = pos_of(ctx, me) else {
        ret_bool(regs, false);
        return Ok(());
    };
    let exact = flags & (1 << 5) != 0;
    let no_z = flags & (1 << 6) != 0;
    let mut found = 0i64;
    for (cls, p) in &ctx.play.census {
        let matches = cls == &class
            || (!exact && {
                let mut cur = cls.clone();
                let mut hit = false;
                for _ in 0..32 {
                    match ctx.play.class_parents.get(&cur) {
                        Some(par) if !par.is_empty() => {
                            if *par == class {
                                hit = true;
                                break;
                            }
                            cur = par.clone();
                        }
                        _ => break,
                    }
                }
                hit
            });
        if !matches {
            continue;
        }
        let d = if no_z {
            dist2d(origin, *p)
        } else {
            room_core::dmath::sqrt(
                (p[0] - origin[0]).powi(2) + (p[1] - origin[1]).powi(2) + (p[2] - origin[2]).powi(2),
            )
        };
        if d <= distance {
            found += 1;
        }
    }
    let ok = if flags & 1 != 0 { found <= count } else { found >= count };
    ret_bool(regs, ok);
    Ok(())
}

/// `double, double GetFriction()`: Doom's default friction and move factor
/// (the census carries no sector friction).
fn get_friction(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
    ret_f(regs, 0.906_25);
    ctx.play.multi_ret = vec![Value::F(0.906_25), Value::F(0.031_25)];
    Ok(())
}

/// `PitchFromVel()`: the pitch of the caller's velocity (UZDoom: positive down).
fn pitch_from_vel(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
    let me = arg_a(regs, 0);
    let slot = ctx.play.actor_slots.vel;
    let v = [
        float_at(ctx.heap, me, slot, 0).unwrap_or(0.0),
        float_at(ctx.heap, me, slot, 1).unwrap_or(0.0),
        float_at(ctx.heap, me, slot, 2).unwrap_or(0.0),
    ];
    let len = room_core::dmath::sqrt(v[0] * v[0] + v[1] * v[1]);
    ret_f(regs, -rad_to_deg(room_core::dmath::atan2(v[2], len)));
    Ok(())
}

/// `DoSpecialDamage(target, damage, mod)`: the default passes the damage on.
fn do_special_damage(_ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
    let d = arg_i(regs, 0, 0);
    ret_i(regs, d);
    Ok(())
}

/// `Grind(items)`: the playsim crushes corpses itself; the default "not
/// handled" leaves that to it.
fn grind(_ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
    ret_bool(regs, false);
    Ok(())
}

/// `PlayerInfo.HasWeaponsInSlot(slot)`: a weapon of that slot is owned by a
/// mirrored player pawn.
fn has_weapons_in_slot(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
    let slot = usize::try_from(arg_i(regs, 0, -1)).unwrap_or(usize::MAX);
    let ok = ctx.play.weapon_slots.get(slot).is_some_and(|weapons| {
        weapons.iter().any(|w| {
            let w = w.to_ascii_lowercase();
            ctx.play
                .inventory
                .iter()
                .any(|(_, items)| items.iter().any(|(c, n, _)| *c == w && *n > 0))
        })
    });
    ret_bool(regs, ok);
    Ok(())
}

/// `WeaponSlots.SlotSize(slot)`.
fn slot_size(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
    let slot = usize::try_from(arg_i(regs, 0, -1)).unwrap_or(usize::MAX);
    let n = ctx.play.weapon_slots.get(slot).map_or(0, Vec::len);
    ret_i(regs, n as i64);
    Ok(())
}

/// `PlayerClass.CheckSkin(skin)`: Room has the base skin (0) only.
fn check_skin(_ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
    let skin = arg_i(regs, 0, 0);
    ret_bool(regs, skin == 0);
    Ok(())
}

/// `PlayerClass.EnumColorSets(out Array<int>)`: no color sets.
fn enum_color_sets(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
    let arr = arg_a(regs, 1);
    if let Some(ObjKind::ArrayI(v)) = ctx.heap.get_mut(arr) {
        v.clear();
    }
    Ok(())
}

/// `Sector.GetPortalType(plane)`: Room's sector data reaches scripts
/// through the playsim only; no portal on the plane as far as scripts see.
fn no_portal(_ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
    ret_i(regs, 0);
    Ok(())
}

/// `State.InStateSequence(base)`: same state or one after it in a sequence
/// (state ids are table indexes; Room's `StateTable` keeps sequences contiguous).
fn in_state_sequence(_ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
    let me = arg_i(regs, 0, -1);
    let base = arg_i(regs, 1, -1);
    ret_bool(regs, me >= 0 && base >= 0 && me >= base && me - base < 64);
    Ok(())
}

/// `SeqNode.GetSequenceName()`.
fn seq_name(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
    ret_s(ctx, regs, "");
    Ok(())
}

/// `int, int String.GetNextCodePoint(pos)`: UTF-8 code point at a byte offset.
fn next_code_point(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
    let text = s_arg(ctx, regs, 0);
    let pos = usize::try_from(arg_i(regs, 0, 0)).unwrap_or(usize::MAX);
    let (cp, next) = match text.get(pos..).and_then(|rest| rest.chars().next()) {
        Some(c) => (i64::from(u32::from(c)), (pos + c.len_utf8()) as i64),
        None => (0, pos.min(text.len()) as i64),
    };
    ret_i(regs, cp);
    ctx.play.multi_ret = vec![Value::I(cp), Value::I(next)];
    Ok(())
}

/// `String.AppendFormat(fmt, ...)`: the receiver with the formatted text
/// added (returned; like Room's other string methods it does not write back).
fn append_format(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
    let base = s_arg(ctx, regs, 0);
    let fmt = s_arg(ctx, regs, 1);
    // Called through an untyped receiver, so the extras carry no vararg
    // layout: the extra string arguments fill `%s` in order.
    let vals: Vec<Value> = regs
        .s
        .iter()
        .skip(2)
        .take(ctx.play.native_arg_counts[2].saturating_sub(2))
        .map(|h| Value::S(*h))
        .collect();
    let tail = crate::format_printf(ctx.heap, &fmt, &vals);
    ret_s(ctx, regs, &format!("{base}{tail}"));
    Ok(())
}

/// `TransformByBone(bone, pos, forward, up, offsets)`: the playsim has no
/// model skeleton, so the position is taken in the actor's frame (yaw only).
fn transform_by_bone(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
    let me = arg_a(regs, 0);
    let p = [arg_f(regs, 0, 0.0), arg_f(regs, 1, 0.0), arg_f(regs, 2, 0.0)];
    let origin = pos_of(ctx, me).unwrap_or([0.0; 3]);
    let (s, c) = angle_of(ctx, me).to_radians().sin_cos();
    let world = [
        origin[0] + p[0] * c - p[1] * s,
        origin[1] + p[0] * s + p[1] * c,
        origin[2] + p[2],
    ];
    ret_vec(regs, &world);
    Ok(())
}

/// Install every UZ3 native.
pub(crate) fn register(t: &mut NativeTable) {
    for name in HOST_NATIVE_NAMES {
        t.replace(name, host_trap);
    }
    for name in QUEUED_NATIVES {
        t.replace(name, queued);
    }
    for name in ENGINE_MANAGED {
        t.replace(name, engine_managed);
    }
    t.replace("actor.usepuzzleitem", queued_true);
    t.replace("actor.slam", queued_true);
    t.replace("actor.getclass", get_class);
    t.replace("class.getclassname", class_name);
    t.replace("class.isabstract", class_is_abstract);
    t.replace("class.getparentclass", class_parent);
    t.replace("global.getreplacement", get_replacement);
    t.replace("global.getreplacee", get_replacee);
    t.replace("global.getbasicarmorclass", basic_armor_class);
    t.replace("global.gethexenarmorclass", hexen_armor_class);
    t.replace("global.getflechettetype", flechette_type);
    t.replace("global.color", color_ctor);
    t.replace("quat.fromangles", quat_from_angles);
    t.replace("actor.checkfov", check_fov);
    t.replace("actor.checkiftargetinlos", target_in_los);
    t.replace("actor.checkifintargetlos", in_target_los);
    t.replace("actor.checklof", check_lof);
    t.replace("actor.checkmissilerange", check_missile_range);
    t.replace("actor.checkkeys", check_keys);
    t.replace("actor.checkproximity", check_proximity);
    t.replace("actor.getfriction", get_friction);
    t.replace("actor.pitchfromvel", pitch_from_vel);
    t.replace("actor.dospecialdamage", do_special_damage);
    t.replace("actor.grind", grind);
    t.replace("actor.transformbybone", transform_by_bone);
    t.replace("playerinfo.hasweaponsinslot", has_weapons_in_slot);
    t.replace("playerinfo.getclassicflight", engine_managed);
    t.replace("playerinfo.getnoautostartmap", engine_managed);
    t.replace("playerpawn.checkuse", ret_true);
    t.replace("playerclass.checkskin", check_skin);
    t.replace("playerclass.enumcolorsets", enum_color_sets);
    t.replace("weaponslots.slotsize", slot_size);
    t.replace("weaponslots.setupweaponslots", engine_managed);
    t.replace("sector.getportaltype", no_portal);
    t.replace("sector.gettexture", no_portal);
    t.replace("state.instatesequence", in_state_sequence);
    t.replace("seqnode.getsequencename", seq_name);
    t.replace("string.getnextcodepoint", next_code_point);
    t.replace("string.appendformat", append_format);
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn quat_from_angles_matches_axis_rotations() {
        let q = quat_from_ypr(90.0, 0.0, 0.0);
        let h = std::f64::consts::FRAC_1_SQRT_2;
        assert!((q[2] - h).abs() < 1e-9 && (q[3] - h).abs() < 1e-9, "{q:?}");
        let q = quat_from_ypr(0.0, 0.0, 90.0);
        assert!((q[0] - h).abs() < 1e-9 && (q[3] - h).abs() < 1e-9, "{q:?}");
        let q = quat_from_ypr(0.0, 90.0, 0.0);
        assert!((q[1] - h).abs() < 1e-9 && (q[3] - h).abs() < 1e-9, "{q:?}");
    }

    #[test]
    fn queued_bone_setter_records_its_arguments() {
        use crate::{NativeRegs, NativeTable, StrHandle, ZvmCtx, ZvmHeap, ZvmPlayHost};
        let table = NativeTable::with_builtins();
        let f = table
            .get(table.id_ci("Actor.SetNamedBoneRotation").expect("bound"))
            .expect("fn");
        let mut heap = ZvmHeap::new();
        let mut play = ZvmPlayHost::default();
        play.native_name = "Actor.SetNamedBoneRotation".into();
        play.native_arg_counts = [1, 5, 1, 1];
        let mut ctx = ZvmCtx {
            heap: &mut heap,
            host: 0,
            play: &mut play,
        };
        let mut i = [2i64];
        let mut fl = [0.0, 0.0, 0.0, 1.0, 5.0];
        let mut st = [StrHandle::EMPTY; 2];
        st[0] = ctx.heap.intern_string("arm");
        let mut a = [crate::ObjHandle::NULL];
        let mut regs = NativeRegs {
            i: &mut i,
            f: &mut fl,
            s: &mut st,
            a: &mut a,
        };
        f(&mut ctx, &mut regs).expect("queued natives never trap");
        let op = ctx.play.native_ops.pop().expect("op queued");
        assert_eq!(op.name, "actor.setnamedbonerotation");
        assert_eq!(op.s, vec!["arm".to_string()]);
        assert_eq!(op.i, vec![2]);
        assert_eq!(op.f, vec![0.0, 0.0, 0.0, 1.0, 5.0]);
    }

    #[test]
    fn host_prefixes_cover_every_host_name() {
        for name in HOST_NATIVE_NAMES {
            assert!(
                HOST_NATIVES.iter().any(|(p, _)| name.starts_with(p)),
                "{name} has no host"
            );
        }
    }
}
