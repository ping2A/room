//! Actor natives that read live actor fields and world data.
//!
//! The playsim mirrors each calling actor's native fields (and those of the
//! actors its pointers name) into their script objects before a call, and
//! fills the per-call tables in [`ZvmPlayHost`] (sight, melee range,
//! inventory). These natives read those, so they stay deterministic and
//! never borrow the world.
//!
//! Calling convention (typed ABI, see `room-zscript` `typed_native`): the
//! receiver is `a[0]`, object parameters follow in `a[1..]`, `double` and
//! vector components go to `f`, ints and bools to `i`, class and name
//! parameters to `s`. Vector results come back in `f[0..n]` (`CallNat` b = 4).

use crate::heap::{ObjHandle, ObjKind, ZvmHeap};
use crate::natives::{NativeRegs, NativeTable, ZvmCtx};
use crate::trap::Trap;

/// Slots of UZDoom `Actor` / `Inventory` native fields in script objects.
///
/// Subclass fields come after inherited ones, so these are the same for
/// every actor class.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ActorSlots {
    /// `pos` (3 floats).
    pub pos: Option<usize>,
    /// `vel` (3 floats).
    pub vel: Option<usize>,
    /// `angle`.
    pub angle: Option<usize>,
    /// `pitch`.
    pub pitch: Option<usize>,
    /// `radius`.
    pub radius: Option<usize>,
    /// `height`.
    pub height: Option<usize>,
    /// `Inventory.Amount` (int bank).
    pub amount: Option<usize>,
    /// `Inventory.MaxAmount` (int bank).
    pub max_amount: Option<usize>,
    /// `target` (object bank).
    pub target: Option<usize>,
    /// `master` (object bank).
    pub master: Option<usize>,
    /// `tracer` (object bank).
    pub tracer: Option<usize>,
}

pub(crate) fn float_at(heap: &ZvmHeap, h: ObjHandle, slot: Option<usize>, k: usize) -> Option<f64> {
    let slot = slot? + k;
    match heap.get(h)? {
        ObjKind::Object { fields_f, .. } => Some(fields_f.get(slot).copied().unwrap_or(0.0)),
        _ => None,
    }
}

pub(crate) fn set_float_at(
    heap: &mut ZvmHeap,
    h: ObjHandle,
    slot: Option<usize>,
    k: usize,
    v: f64,
) {
    let Some(slot) = slot.map(|s| s + k) else {
        return;
    };
    if let Some(ObjKind::Object { fields_f, .. }) = heap.get_mut(h) {
        if fields_f.len() <= slot {
            fields_f.resize(slot + 1, 0.0);
        }
        fields_f[slot] = v;
    }
}

pub(crate) fn pos_of(ctx: &ZvmCtx<'_>, h: ObjHandle) -> Option<[f64; 3]> {
    let slot = ctx.play.actor_slots.pos;
    Some([
        float_at(ctx.heap, h, slot, 0)?,
        float_at(ctx.heap, h, slot, 1)?,
        float_at(ctx.heap, h, slot, 2)?,
    ])
}

pub(crate) fn ret_f(regs: &mut NativeRegs<'_>, v: f64) {
    if let Some(slot) = regs.f.first_mut() {
        *slot = v;
    }
    if let Some(slot) = regs.i.first_mut() {
        *slot = v as i64;
    }
}

pub(crate) fn ret_bool(regs: &mut NativeRegs<'_>, v: bool) {
    if let Some(slot) = regs.i.first_mut() {
        *slot = i64::from(v);
    }
    if let Some(slot) = regs.f.first_mut() {
        *slot = f64::from(u8::from(v));
    }
}

pub(crate) fn ret_vec(regs: &mut NativeRegs<'_>, v: &[f64]) {
    for (k, x) in v.iter().enumerate() {
        if let Some(slot) = regs.f.get_mut(k) {
            *slot = *x;
        }
    }
}

pub(crate) fn arg_a(regs: &NativeRegs<'_>, k: usize) -> ObjHandle {
    regs.a.get(k).copied().unwrap_or(ObjHandle::NULL)
}

pub(crate) fn arg_f(regs: &NativeRegs<'_>, k: usize, default: f64) -> f64 {
    regs.f.get(k).copied().unwrap_or(default)
}

pub(crate) fn arg_i(regs: &NativeRegs<'_>, k: usize, default: i64) -> i64 {
    regs.i.get(k).copied().unwrap_or(default)
}

/// Self and other positions, or `None` when either is not an actor object.
fn pair(ctx: &ZvmCtx<'_>, regs: &NativeRegs<'_>) -> Option<([f64; 3], [f64; 3])> {
    let me = pos_of(ctx, arg_a(regs, 0))?;
    let other = arg_a(regs, 1);
    if other.is_null() {
        return None;
    }
    Some((me, pos_of(ctx, other)?))
}

fn distance(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>, three_d: bool, squared: bool) {
    let Some((a, b)) = pair(ctx, regs) else {
        ret_f(regs, 0.0);
        return;
    };
    let (dx, dy, dz) = (
        b[0] - a[0],
        b[1] - a[1],
        if three_d { b[2] - a[2] } else { 0.0 },
    );
    let sq = dx * dx + dy * dy + dz * dz;
    ret_f(
        regs,
        if squared {
            sq
        } else {
            room_core::dmath::sqrt(sq)
        },
    );
}

fn distance_2d(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
    distance(ctx, regs, false, false);
    Ok(())
}

fn distance_3d(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
    distance(ctx, regs, true, false);
    Ok(())
}

fn distance_2d_sq(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
    distance(ctx, regs, false, true);
    Ok(())
}

fn distance_3d_sq(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
    distance(ctx, regs, true, true);
    Ok(())
}

pub(crate) fn rad_to_deg(r: f64) -> f64 {
    r * 180.0 / std::f64::consts::PI
}

fn deg_to_rad(d: f64) -> f64 {
    d * std::f64::consts::PI / 180.0
}

/// `AngleTo(target, absolute)`: degrees from self to target.
fn angle_to(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
    let Some((a, b)) = pair(ctx, regs) else {
        ret_f(regs, 0.0);
        return Ok(());
    };
    ret_f(
        regs,
        rad_to_deg(room_core::dmath::atan2(b[1] - a[1], b[0] - a[0])),
    );
    Ok(())
}

fn vec_to(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
    let Some((a, b)) = pair(ctx, regs) else {
        ret_vec(regs, &[0.0, 0.0, 0.0]);
        return Ok(());
    };
    ret_vec(regs, &[b[0] - a[0], b[1] - a[1], b[2] - a[2]]);
    Ok(())
}

/// `Vec3Offset(x, y, z, absolute)` / `Vec2OffsetZ(x, y, z)` / `Vec2Offset(x, y)`.
fn vec3_offset(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
    let p = pos_of(ctx, arg_a(regs, 0)).unwrap_or([0.0; 3]);
    let out = [
        p[0] + arg_f(regs, 0, 0.0),
        p[1] + arg_f(regs, 1, 0.0),
        p[2] + arg_f(regs, 2, 0.0),
    ];
    ret_vec(regs, &out);
    Ok(())
}

fn vec2_offset_z(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
    let p = pos_of(ctx, arg_a(regs, 0)).unwrap_or([0.0; 3]);
    let out = [
        p[0] + arg_f(regs, 0, 0.0),
        p[1] + arg_f(regs, 1, 0.0),
        arg_f(regs, 2, 0.0),
    ];
    ret_vec(regs, &out);
    Ok(())
}

/// `Vec3Angle(length, angle, z, absolute)` / `Vec2Angle(length, angle, absolute)`.
fn vec3_angle(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
    let p = pos_of(ctx, arg_a(regs, 0)).unwrap_or([0.0; 3]);
    let (len, ang, z) = (
        arg_f(regs, 0, 0.0),
        arg_f(regs, 1, 0.0),
        arg_f(regs, 2, 0.0),
    );
    let r = deg_to_rad(ang);
    ret_vec(
        regs,
        &[
            p[0] + room_core::dmath::cos(r) * len,
            p[1] + room_core::dmath::sin(r) * len,
            p[2] + z,
        ],
    );
    Ok(())
}

fn vec2_angle(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
    let p = pos_of(ctx, arg_a(regs, 0)).unwrap_or([0.0; 3]);
    let (len, ang) = (arg_f(regs, 0, 0.0), arg_f(regs, 1, 0.0));
    let r = deg_to_rad(ang);
    ret_vec(
        regs,
        &[
            p[0] + room_core::dmath::cos(r) * len,
            p[1] + room_core::dmath::sin(r) * len,
        ],
    );
    Ok(())
}

/// `SetOrigin(pos, moving)`: writes the mirrored `pos`; the playsim applies it.
fn set_origin(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
    let h = arg_a(regs, 0);
    let slot = ctx.play.actor_slots.pos;
    for k in 0..3 {
        set_float_at(ctx.heap, h, slot, k, arg_f(regs, k, 0.0));
    }
    Ok(())
}

fn set_z(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
    let h = arg_a(regs, 0);
    let slot = ctx.play.actor_slots.pos;
    set_float_at(ctx.heap, h, slot, 2, arg_f(regs, 0, 0.0));
    Ok(())
}

fn add_z(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
    let h = arg_a(regs, 0);
    let slot = ctx.play.actor_slots.pos;
    let z = float_at(ctx.heap, h, slot, 2).unwrap_or(0.0);
    set_float_at(ctx.heap, h, slot, 2, z + arg_f(regs, 0, 0.0));
    Ok(())
}

/// `pos.z` of the receiver (`GetZ`).
fn get_z(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
    let h = arg_a(regs, 0);
    let z = float_at(ctx.heap, h, ctx.play.actor_slots.pos, 2).unwrap_or(0.0);
    ret_f(regs, z);
    Ok(())
}

/// `CheckSight(other, flags)`: the playsim's line-of-sight from the caller.
fn check_sight(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
    let other = arg_a(regs, 1);
    let seen = ctx
        .play
        .sight
        .iter()
        .find(|(h, _)| *h == other)
        .is_some_and(|(_, v)| *v);
    ret_bool(regs, seen);
    Ok(())
}

/// `CheckMeleeRange(range = -1)`: the caller against its target.
fn check_melee_range(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
    let range = arg_f(regs, 0, -1.0);
    if range < 0.0 {
        let ok = ctx.play.melee_ok;
        ret_bool(regs, ok);
        return Ok(());
    }
    // An explicit range: 2D distance against range + the target's radius.
    let target = ctx.play.self_target;
    let ok = match (pos_of(ctx, arg_a(regs, 0)), pos_of(ctx, target)) {
        (Some(a), Some(b)) => {
            let r = float_at(ctx.heap, target, ctx.play.actor_slots.radius, 0).unwrap_or(0.0);
            let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
            let seen = ctx
                .play
                .sight
                .iter()
                .find(|(h, _)| *h == target)
                .is_some_and(|(_, v)| *v);
            room_core::dmath::sqrt(dx * dx + dy * dy) < range + r && seen
        }
        _ => false,
    };
    ret_bool(regs, ok);
    Ok(())
}

fn class_arg(ctx: &ZvmCtx<'_>, regs: &NativeRegs<'_>) -> String {
    let s = regs.s.first().copied().unwrap_or_default();
    let name = ctx.heap.str(s);
    if !name.is_empty() {
        return name.to_ascii_lowercase();
    }
    let id = arg_i(regs, 0, -1);
    usize::try_from(id)
        .ok()
        .and_then(|i| ctx.play.class_names.get(i))
        .map(|n| n.to_ascii_lowercase())
        .unwrap_or_default()
}

pub(crate) fn inventory_entry(
    ctx: &ZvmCtx<'_>,
    owner: ObjHandle,
    class: &str,
) -> Option<(i32, i32)> {
    ctx.play
        .inventory
        .iter()
        .find(|(h, _)| *h == owner)?
        .1
        .iter()
        .find(|(c, _, _)| c == class)
        .map(|(_, amount, max)| (*amount, *max))
}

/// `CountInv(type, ptr)`: the caller's stack of `type`.
fn count_inv(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
    let class = class_arg(ctx, regs);
    let owner = arg_a(regs, 0);
    let n = inventory_entry(ctx, owner, &class).map_or(0, |(amount, _)| amount);
    ret_f(regs, f64::from(n));
    Ok(())
}

/// `FindInventory(type)`: a detached item object with `Amount` / `MaxAmount`.
fn find_inventory(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
    let class = class_arg(ctx, regs);
    let owner = arg_a(regs, 0);
    let found = inventory_entry(ctx, owner, &class);
    let h = match found {
        Some((amount, max)) => {
            // A DECORATE item has no script layout: use `Inventory`'s, so
            // `Amount` / `MaxAmount` have their slots.
            let (class_id, n_i, n_a, n_s) = ctx
                .play
                .object_layouts
                .get(&class)
                .or_else(|| ctx.play.object_layouts.get("inventory"))
                .copied()
                .unwrap_or((0, 8, 4, 4));
            let h = ctx.heap.new_object(class_id, n_i, n_a, n_s);
            let slots = ctx.play.actor_slots;
            if let Some(ObjKind::Object { fields_i, .. }) = ctx.heap.get_mut(h) {
                for (slot, v) in [(slots.amount, amount), (slots.max_amount, max)] {
                    if let Some(cell) = slot.and_then(|s| fields_i.get_mut(s)) {
                        *cell = i64::from(v);
                    }
                }
            }
            h
        }
        None => ObjHandle::NULL,
    };
    if let Some(slot) = regs.a.first_mut() {
        *slot = h;
    }
    Ok(())
}

pub(crate) fn object_at(heap: &ZvmHeap, h: ObjHandle, slot: Option<usize>) -> ObjHandle {
    let Some(slot) = slot else {
        return ObjHandle::NULL;
    };
    match heap.get(h) {
        Some(ObjKind::Object { fields_a, .. }) => fields_a.get(slot).copied().unwrap_or_default(),
        _ => ObjHandle::NULL,
    }
}

/// `GetPointer(aaptr)`: self, `target`, `master`, `tracer` (`AAPTR_*`).
fn get_pointer(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
    let me = arg_a(regs, 0);
    let slots = ctx.play.actor_slots;
    let h = match arg_i(regs, 0, 0) {
        0 => me,
        2 => object_at(ctx.heap, me, slots.target),
        4 => object_at(ctx.heap, me, slots.master),
        8 => object_at(ctx.heap, me, slots.tracer),
        _ => ObjHandle::NULL,
    };
    if let Some(slot) = regs.a.first_mut() {
        *slot = h;
    }
    Ok(())
}

/// Natives that use the typed ABI. `room-zscript` lowers calls to these by
/// their declared parameter types.
pub const TYPED_NATIVES: &[&str] = &[
    "actor.distance2d",
    "actor.distance3d",
    "actor.distance2dsquared",
    "actor.distance3dsquared",
    "actor.angleto",
    "actor.vec2to",
    "actor.vec3to",
    "actor.vec3offset",
    "actor.vec2offset",
    "actor.vec2offsetz",
    "actor.vec3angle",
    "actor.vec2angle",
    "actor.setorigin",
    "actor.setz",
    "actor.addz",
    "actor.checksight",
    "actor.checkmeleerange",
    "actor.countinv",
    "actor.findinventory",
    "actor.getpointer",
];

/// Vector size of a typed native's result.
#[must_use]
pub fn typed_native_vector_result(name: &str) -> Option<u8> {
    match name.to_ascii_lowercase().as_str() {
        "actor.vec2to"
        | "actor.vec2offset"
        | "actor.vec2angle"
        | "actor.rotatevector"
        | "texman.getscaledsize"
        | "texman.getscaledoffset" => Some(2),
        "levellocals.vec2diff"
        | "levellocals.vec2offset"
        | "levellocals.getdisplacement"
        | "sector.getportaldisplacement" => Some(2),
        "actor.vec3to" | "actor.vec3offset" | "actor.vec2offsetz" | "actor.vec3angle" => Some(3),
        "levellocals.vec3diff"
        | "levellocals.vec3offset"
        | "levellocals.vec2offsetz"
        | "actor.posrelative" => Some(3),
        "quat.fromangles" | "quat.axisangle" | "quat.slerp" | "quat.nlerp" => Some(4),
        _ => None,
    }
}

/// Install (replacing approximations of the same names).
pub(crate) fn register(t: &mut NativeTable) {
    t.replace("actor.distance2d", distance_2d);
    t.replace("actor.distance3d", distance_3d);
    t.replace("actor.distance2dsquared", distance_2d_sq);
    t.replace("actor.distance3dsquared", distance_3d_sq);
    t.replace("actor.angleto", angle_to);
    t.replace("actor.vec2to", vec_to);
    t.replace("actor.vec3to", vec_to);
    t.replace("actor.vec3offset", vec3_offset);
    t.replace("actor.vec2offset", vec3_offset);
    t.replace("actor.vec2offsetz", vec2_offset_z);
    t.replace("actor.vec3angle", vec3_angle);
    t.replace("actor.vec2angle", vec2_angle);
    t.replace("actor.setorigin", set_origin);
    t.replace("actor.setz", set_z);
    t.replace("actor.addz", add_z);
    t.replace("object.addz", add_z);
    t.replace("object.setz", set_z);
    t.replace("object.getz", get_z);
    t.replace("actor.getz", get_z);
    t.replace("actor.checksight", check_sight);
    t.replace("actor.checkmeleerange", check_melee_range);
    t.replace("actor.countinv", count_inv);
    t.replace("actor.findinventory", find_inventory);
    t.replace("actor.getpointer", get_pointer);
}
