//! Native function table.

use crate::Value;
use crate::heap::{ObjHandle, StrHandle, ZvmHeap};
use crate::trap::Trap;
use std::collections::BTreeMap;

/// Register view passed to natives (typed banks).
#[derive(Debug)]
pub struct NativeRegs<'a> {
    /// Integer registers.
    pub i: &'a mut [i64],
    /// Float registers.
    pub f: &'a mut [f64],
    /// String registers.
    pub s: &'a mut [StrHandle],
    /// Object registers.
    pub a: &'a mut [ObjHandle],
}

/// Context for a native call.
pub struct ZvmCtx<'a> {
    /// Object / string heap.
    pub heap: &'a mut ZvmHeap,
    /// Host userdata cookie (engine binds this).
    pub host: usize,
    /// Play-scope values for this VM.
    pub play: &'a mut play_natives::ZvmPlayHost,
}

/// Native function signature.
pub type NativeFn = fn(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap>;

/// A play-scope native recorded for the playsim (UZ3 / UZ7): the native's
/// lowercase name, its receiver, and the raw arguments per bank.
#[derive(Clone, Debug, PartialEq)]
pub struct ZsNativeOp {
    /// Lowercase registered name (`actor.setbonerotation`).
    pub name: String,
    /// Receiver (`a[0]`; null for statics).
    pub recv: ObjHandle,
    /// Further object arguments.
    pub objs: Vec<ObjHandle>,
    /// Int arguments.
    pub i: Vec<i64>,
    /// Float arguments (vectors as components).
    pub f: Vec<f64>,
    /// String / name / class arguments.
    pub s: Vec<String>,
}

/// A `LevelLocals` sky / fog native recorded for the playsim (UZDoom 5.0).
#[derive(Clone, Debug, PartialEq)]
pub enum ZsLevelOp {
    /// `Level.SetSkyFog(int)`.
    SkyFog(i32),
    /// `Level.SetThickFog(distance, multiplier)`.
    ThickFog(f64, f64),
    /// `Level.ChangeSkyMist(TextureID, usemist, yscale)`; `None` = null texture.
    SkyMist {
        /// Mist texture name.
        texture: Option<String>,
        /// `usemist` (`LEVEL3_SKYMIST`).
        use_mist: bool,
        /// Mist y scale.
        yscale: f64,
    },
}

/// A ZScript actor native recorded for the playsim to apply after the call.
///
/// The VM cannot borrow the world while a script is running, so `A_Look`,
/// `LineAttack`, and the other fight natives append one of these and return.
#[derive(Clone, Debug)]
pub enum ZsActorOp {
    /// `A_Look`.
    Look,
    /// `A_Chase` with optional state labels and UZDoom chase flags.
    Chase {
        /// Melee state label (empty means the actor's default `Melee` state).
        melee: String,
        /// Missile state label (empty means the actor's default `Missile` state).
        missile: String,
        /// `CHF_*` bit field.
        flags: i32,
    },
    /// `A_RadiusDamageSelf`: damage the actor's target based on distance.
    RadiusDamageSelf {
        /// Maximum damage.
        damage: i32,
        /// Full-radius distance.
        distance: f64,
        /// `RDSF_*` flags.
        flags: i32,
        /// Optional flash actor class name.
        flash_class: Option<String>,
    },
    /// `A_Burst` with a resolved chunk actor class name.
    Burst {
        /// Name of the chunk actor class.
        chunk_class: String,
    },
    /// Restore the actor's prepared resurrection candidate (`A_CheckForResurrection`).
    ResurrectCandidate {
        /// Generational arena index of the corpse selected before this VM call.
        actor_index: u32,
        /// Generation of the selected corpse handle.
        generation: u32,
    },
    /// `A_Face` / `A_FaceTarget`.
    Face,
    /// `A_StartSound`.
    Sound(String),
    /// Full ZScript `Actor.A_StartSound` call.
    SoundStart {
        /// Sound name or `*player-sound` token.
        name: String,
        /// Channel slot.
        channel: i32,
        /// `CHANF_*` flags.
        flags: i32,
        /// Volume.
        volume: f64,
        /// Attenuation.
        attenuation: f64,
        /// Playback pitch (zero selects default pitch).
        pitch: f64,
        /// Start position in seconds.
        start_time: f64,
    },
    /// `A_StartSoundIfNotSame` with UZDoom's sound comparison and channel parameters.
    SoundIfNotSame {
        /// Sound to start when the current voice differs.
        name: String,
        /// Sound that suppresses the start when it is already playing.
        check_name: String,
        /// Actor sound channel.
        channel: i32,
        /// `CHANF_*` flags.
        flags: i32,
        /// Linear volume.
        volume: f64,
        /// Distance attenuation.
        attenuation: f64,
        /// Playback pitch.
        pitch: f64,
        /// Start position in seconds.
        start_time: f64,
    },
    /// `LineAttack` hitscan. `range` is map units; `damage` is hit points.
    LineAttack {
        /// Trace length.
        range: f64,
        /// Damage applied on a hit.
        damage: i32,
    },
    /// `Actor.Thrust`: add this much map-unit velocity.
    Thrust {
        /// Added X velocity. Angle 0 points along +X.
        vx: f64,
        /// Added Y velocity.
        vy: f64,
    },
    /// `Actor.SetIdle`: return to the Spawn state.
    Idle {
        /// Run the Spawn state's action. False when `nofunction` is set.
        call_action: bool,
    },
    /// `PlayActiveSound`: the class `ActiveSound`, resolved in the playsim.
    ActiveSound,
    /// `PlaySpawnSound`: the class `SeeSound`, resolved in the playsim.
    SpawnSound,
    /// `Howl`: the class `HowlSound`, or `PainSound` when that is empty.
    Howl,
    /// `ClearBounce`: drop floor and wall bounce flags.
    ClearBounce,
    /// `LinkToWorld` / `UnlinkFromWorld`. `linked` clears `NOBLOCKMAP`.
    WorldLink {
        /// Put the actor back in the blockmap.
        linked: bool,
    },
    /// `FindFloorCeiling`: keep the actor between the floor and the ceiling.
    FitSector,
    /// `Sector.SetEnvironmentID(id)` / `Sector.SetEnvironment(name)` on the
    /// caller's `CurSector` (Room's ZScript has no free sector objects; the
    /// call is applied to the sector the calling actor stands in).
    SetEnvironment {
        /// Environment ID (`id1 << 8 | id2`), when set by number.
        id: Option<u16>,
        /// REVERBS name, when set by name.
        name: Option<String>,
    },
    /// `RestoreDamage`: put the class's damage value back on the actor.
    RestoreDamage,
    /// `RemoveFromHash`: the actor is no longer found by its tid.
    ClearTid,
    /// `ClearCounters`: this actor no longer counts as a kill or an item.
    ClearCounters,
    /// `TraceBleed`: spawn blood for this much damage.
    Bleed {
        /// Hit points that choose the blood frame.
        damage: i32,
    },
    /// `DrawSplash`: the floor-splash path, using the actor's vertical speed.
    Splash,
    /// `DisableLocalRendering`: hide or show the actor for the local player.
    LocalHide {
        /// `DONTDRAW` is set when this is true.
        hide: bool,
    },
    /// `SetShade`: stencil color, or none when the packed color is 0.
    Shade {
        /// Red, green, and blue, or none.
        rgb: Option<[u8; 3]>,
    },
    /// `SetRenderStyle`: GZDoom `ERenderStyle` value, 0 through 11.
    RenderMode {
        /// Style index. 0 is invisible, 1 is opaque, 6 is translucent.
        mode: i32,
    },
    /// `SetViewPos`: extra eye height in map units.
    ViewZ {
        /// Added to the player's eye height.
        z: f64,
    },
    /// `CopyFriendliness`: copy the master or target friendly flag.
    CopyFriend {
        /// Aim at the actor the flag was copied from.
        change_target: bool,
        /// Restore spawn health when aiming at that actor.
        reset_health: bool,
    },
    /// `A_Pain`.
    Pain,
    /// `A_NoBlocking` / `A_Fall`.
    NoBlocking,
    /// A playsim action already implemented for DECORATE. `args` are the
    /// integer registers the call passed, trailing zeros omitted.
    Action {
        /// Action name, such as `A_SetHealth`.
        name: String,
        /// Up to four integer arguments.
        args: [i64; 4],
    },
    /// `Actor.A_SpawnParticleEx` motion and fade request.
    ParticleEx {
        /// Packed RGB color.
        color: u32,
        /// Texture name from the `TextureID` argument (`None` = untextured).
        texture: Option<String>,
        /// UZDoom legacy render style (`STYLE_*`).
        render_style: i32,
        /// Particle flags (`SPF_*`).
        flags: u32,
        /// Lifetime in tics.
        lifetime: i32,
        /// Initial size.
        size: f64,
        /// Direction angle in degrees.
        angle: f64,
        /// Spawn offset.
        offset: [f64; 3],
        /// Initial velocity.
        velocity: [f64; 3],
        /// Acceleration.
        acceleration: [f64; 3],
        /// Starting opacity.
        alpha: f64,
        /// Opacity delta per tic.
        fade_step: f64,
        /// Size delta per tic.
        size_step: f64,
        /// Initial roll in degrees.
        roll: f64,
        /// Roll velocity in degrees per tic.
        roll_velocity: f64,
        /// Roll acceleration in degrees per tic squared.
        roll_acceleration: f64,
        /// Opacity step used after fade-in when a fade mode is active.
        fade_out_step: f64,
    },
}

/// Named native table (stable BTreeMap iteration).
#[derive(Default)]
pub struct NativeTable {
    by_name: BTreeMap<String, (u32, NativeFn)>,
    by_id: Vec<(String, NativeFn)>,
}

impl NativeTable {
    /// Empty table.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a native; returns its id.
    pub fn register(&mut self, name: impl Into<String>, f: NativeFn) -> u32 {
        let name = name.into();
        if let Some(&(id, _)) = self.by_name.get(&name) {
            return id;
        }
        let id = self.by_id.len() as u32;
        self.by_id.push((name.clone(), f));
        self.by_name.insert(name, (id, f));
        id
    }

    /// Bind `name` (any case) to `f`, replacing earlier bindings of it.
    pub fn replace(&mut self, name: &str, f: NativeFn) {
        let mut hit = false;
        for (n, (id, g)) in &mut self.by_name {
            if n.eq_ignore_ascii_case(name) {
                *g = f;
                if let Some(slot) = self.by_id.get_mut(*id as usize) {
                    slot.1 = f;
                }
                hit = true;
            }
        }
        if !hit {
            self.register(name, f);
        }
    }

    /// Resolve by id.
    #[must_use]
    pub fn get(&self, id: u32) -> Option<NativeFn> {
        self.by_id.get(id as usize).map(|(_, f)| *f)
    }

    /// Registered spelling for `id` (`Actor.A_SetSize`).
    #[must_use]
    pub fn name(&self, id: u32) -> Option<&str> {
        self.by_id.get(id as usize).map(|(n, _)| n.as_str())
    }

    /// Resolve by name.
    #[must_use]
    pub fn get_named(&self, name: &str) -> Option<(u32, NativeFn)> {
        self.by_name.get(name).copied()
    }

    /// Resolve by name, ASCII case-insensitive (GZDoom native lookup).
    #[must_use]
    pub fn id_ci(&self, name: &str) -> Option<u32> {
        self.by_name
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, (id, _))| *id)
    }

    /// Register a no-op stub for every import that is not already in the table.
    ///
    /// Existing natives are left in place. The returned count is how many stubs
    /// were added. A stub returns 0 so the caller keeps running.
    pub fn bind_imports<I, S>(&mut self, names: I) -> usize
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut known: std::collections::BTreeSet<String> = self
            .by_name
            .keys()
            .map(|n| n.to_ascii_lowercase())
            .collect();
        let mut added = 0;
        for name in names {
            let name = name.as_ref();
            if name.is_empty() || !known.insert(name.to_ascii_lowercase()) {
                continue;
            }
            self.register(name.to_owned(), native_unbound);
            added += 1;
        }
        added
    }

    /// Built-in natives used by conformance fixtures + BD priority set.
    #[must_use]
    pub fn with_builtins() -> Self {
        let mut t = Self::new();
        t.register("Object.Destroy", native_destroy);
        t.register("Console.Printf", native_printf);
        t.register("Array.PushI", native_array_push_i);
        let _ = play_natives::register_bd_natives(&mut t);
        crate::world_natives::register(&mut t);
        crate::uz3_natives::register(&mut t);
        t
    }
}

fn native_destroy(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
    let h = regs.a.first().copied().unwrap_or(ObjHandle::NULL);
    ctx.heap.destroy(h);
    Ok(())
}

fn native_printf(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
    // Format string in s[0]; optional int in i[0]. Stores result in s[1] for tests.
    // Prefer host `console_printf` when installed (ZS4 / room-sim capture).
    let arg = regs.i.first().copied().unwrap_or(0);
    let out = {
        let fmt = ctx
            .heap
            .str(regs.s.first().copied().unwrap_or_default())
            .to_owned();
        if fmt.contains("%d") {
            fmt.replacen("%d", &arg.to_string(), 1)
        } else {
            fmt
        }
    };
    ctx.play.printf.push(out.clone());
    if regs.s.len() > 1 {
        regs.s[1] = ctx.heap.intern_string(&out);
    }
    Ok(())
}

/// The extra arguments of a `vararg` native in call order, after `ni` / `nf`
/// / `ns` declared ints, floats and strings. `room-zscript` passes the
/// extras in the bank of their type, then each extra's bank (0 int, 1 float,
/// 2 string) and their count at the end of the int bank.
#[must_use]
pub fn vararg_values(ctx: &ZvmCtx<'_>, regs: &NativeRegs<'_>, ni: usize, nf: usize, ns: usize) -> Vec<Value> {
    let len = ctx.play.native_arg_counts[0].min(regs.i.len());
    let Some(&n) = len.checked_sub(1).and_then(|k| regs.i.get(k)) else {
        return Vec::new();
    };
    let Ok(n) = usize::try_from(n) else {
        return Vec::new();
    };
    if n + 1 > len.saturating_sub(ni) {
        return Vec::new();
    }
    let codes = &regs.i[len - 1 - n..len - 1];
    let (mut ci, mut cf, mut cs) = (ni, nf, ns);
    let mut out = Vec::with_capacity(n);
    for code in codes {
        match code {
            1 => {
                out.push(Value::F(regs.f.get(cf).copied().unwrap_or(0.0)));
                cf += 1;
            }
            2 => {
                out.push(Value::S(regs.s.get(cs).copied().unwrap_or_default()));
                cs += 1;
            }
            _ => {
                out.push(Value::I(regs.i.get(ci).copied().unwrap_or(0)));
                ci += 1;
            }
        }
    }
    out
}

/// `String.Format` / `Printf`: C `printf` conversions (`%d %i %u %x %X %o
/// %c %s %f %e %g %%`, flags `-+ 0#`, width, `.precision`, `*`), each
/// taking the next argument, coerced as C's varargs would.
#[must_use]
pub fn format_printf(heap: &crate::heap::ZvmHeap, fmt: &str, args: &[Value]) -> String {
    let mut out = String::with_capacity(fmt.len() + 16);
    let mut args = args.iter().copied();
    let as_i = |v: Option<Value>| match v {
        Some(Value::I(x)) => x,
        #[allow(clippy::cast_possible_truncation)]
        Some(Value::F(x)) => x as i64,
        _ => 0,
    };
    let chars: Vec<char> = fmt.chars().collect();
    let mut k = 0;
    while k < chars.len() {
        let c = chars[k];
        k += 1;
        if c != '%' {
            out.push(c);
            continue;
        }
        if chars.get(k) == Some(&'%') {
            out.push('%');
            k += 1;
            continue;
        }
        let (mut left, mut plus, mut space, mut zero, mut alt) = (false, false, false, false, false);
        while let Some(&f) = chars.get(k) {
            match f {
                '-' => left = true,
                '+' => plus = true,
                ' ' => space = true,
                '0' => zero = true,
                '#' => alt = true,
                _ => break,
            }
            k += 1;
        }
        let mut width = 0usize;
        if chars.get(k) == Some(&'*') {
            width = usize::try_from(as_i(args.next())).unwrap_or(0);
            k += 1;
        }
        while let Some(d) = chars.get(k).and_then(|c| c.to_digit(10)) {
            width = width * 10 + d as usize;
            k += 1;
        }
        let mut prec: Option<usize> = None;
        if chars.get(k) == Some(&'.') {
            k += 1;
            let mut p = 0usize;
            if chars.get(k) == Some(&'*') {
                p = usize::try_from(as_i(args.next())).unwrap_or(0);
                k += 1;
            }
            while let Some(d) = chars.get(k).and_then(|c| c.to_digit(10)) {
                p = p * 10 + d as usize;
                k += 1;
            }
            prec = Some(p);
        }
        // Length modifiers are accepted and ignored.
        while matches!(chars.get(k), Some('l' | 'h' | 'z' | 'j' | 't' | 'L' | 'q')) {
            k += 1;
        }
        let Some(&conv) = chars.get(k) else {
            break;
        };
        k += 1;
        let sign = |neg: bool| {
            if neg {
                "-"
            } else if plus {
                "+"
            } else if space {
                " "
            } else {
                ""
            }
        };
        let (body, prefix, numeric): (String, String, bool) = match conv {
            'd' | 'i' => {
                let v = as_i(args.next());
                let mut digits = v.unsigned_abs().to_string();
                if let Some(p) = prec {
                    while digits.len() < p {
                        digits.insert(0, '0');
                    }
                }
                (digits, sign(v < 0).to_owned(), true)
            }
            'u' => (u64::from(as_i(args.next()) as u32).to_string(), String::new(), true),
            'x' | 'X' | 'o' => {
                #[allow(clippy::cast_sign_loss)]
                let v = as_i(args.next()) as u64 & 0xFFFF_FFFF;
                let digits = match conv {
                    'x' => format!("{v:x}"),
                    'X' => format!("{v:X}"),
                    _ => format!("{v:o}"),
                };
                let pre = if alt && v != 0 {
                    match conv {
                        'x' => "0x",
                        'X' => "0X",
                        _ => "0",
                    }
                } else {
                    ""
                };
                (digits, pre.to_owned(), true)
            }
            'c' => {
                let v = as_i(args.next());
                (
                    char::from_u32(u32::try_from(v).unwrap_or(0)).map(String::from).unwrap_or_default(),
                    String::new(),
                    false,
                )
            }
            's' => {
                let text = match args.next() {
                    Some(Value::S(h)) => heap.str(h).to_owned(),
                    Some(Value::I(x)) => x.to_string(),
                    Some(Value::F(x)) => x.to_string(),
                    _ => String::new(),
                };
                let text = match prec {
                    Some(p) => text.chars().take(p).collect(),
                    None => text,
                };
                (text, String::new(), false)
            }
            'f' | 'F' | 'e' | 'E' | 'g' | 'G' => {
                let v = match args.next() {
                    Some(Value::F(x)) => x,
                    Some(Value::I(x)) => x as f64,
                    _ => 0.0,
                };
                let p = prec.unwrap_or(6);
                let body = match conv {
                    'f' | 'F' => format!("{:.*}", p, v.abs()),
                    'e' | 'E' => {
                        let s = format!("{:.*e}", p, v.abs());
                        // C writes a signed two-digit exponent (`1.5e+00`).
                        let (m, e) = s.split_once('e').unwrap_or((&s, "0"));
                        let e: i32 = e.parse().unwrap_or(0);
                        let s = format!("{m}e{}{:02}", if e < 0 { '-' } else { '+' }, e.abs());
                        if conv == 'E' { s.to_uppercase() } else { s }
                    }
                    _ => {
                        // %g: shortest of %e / %f with `p` significant digits.
                        let p = p.max(1);
                        let a = v.abs();
                        let exp = if a == 0.0 { 0 } else { a.log10().floor() as i32 };
                        let s = if exp < -4 || exp >= p as i32 {
                            let s = format!("{:.*e}", p - 1, a);
                            let (m, e) = s.split_once('e').unwrap_or((&s, "0"));
                            let m = if alt { m.to_owned() } else { trim_zeros(m) };
                            let e: i32 = e.parse().unwrap_or(0);
                            format!("{m}e{}{:02}", if e < 0 { '-' } else { '+' }, e.abs())
                        } else {
                            let decimals = usize::try_from(p as i32 - 1 - exp).unwrap_or(0);
                            let s = format!("{a:.decimals$}");
                            if alt { s } else { trim_zeros(&s) }
                        };
                        if conv == 'G' { s.to_uppercase() } else { s }
                    }
                };
                (body, sign(v.is_sign_negative() && v != 0.0).to_owned(), true)
            }
            other => {
                // Unknown conversion: print it as written.
                out.push('%');
                out.push(other);
                continue;
            }
        };
        let len = prefix.chars().count() + body.chars().count();
        let pad = width.saturating_sub(len);
        if left {
            out.push_str(&prefix);
            out.push_str(&body);
            out.extend(std::iter::repeat_n(' ', pad));
        } else if zero && numeric && (prec.is_none() || matches!(conv, 'f' | 'F' | 'e' | 'E' | 'g' | 'G')) {
            out.push_str(&prefix);
            out.extend(std::iter::repeat_n('0', pad));
            out.push_str(&body);
        } else {
            out.extend(std::iter::repeat_n(' ', pad));
            out.push_str(&prefix);
            out.push_str(&body);
        }
    }
    out
}

fn trim_zeros(s: &str) -> String {
    if s.contains('.') {
        s.trim_end_matches('0').trim_end_matches('.').to_owned()
    } else {
        s.to_owned()
    }
}

fn native_unbound(_ctx: &mut ZvmCtx<'_>, _regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
    // A default of 0. Trapping here aborts every stdlib function at its first
    // engine global (`_.menuactive`, …) before a real native such as `LineAttack`.
    Ok(())
}

fn native_array_push_i(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
    let h = regs.a.first().copied().unwrap_or(ObjHandle::NULL);
    let v = regs.i.first().copied().unwrap_or(0);
    match ctx.heap.get_mut(h) {
        Some(crate::heap::ObjKind::ArrayI(items)) => {
            items.push(v);
            Ok(())
        }
        _ => Err(Trap::Native("Array.PushI: not an Array<int>".into())),
    }
}

mod play_natives {
    //! Brutal Doom–priority natives (`docs/34` §6 / `docs/33` §3.10).

    use super::{NativeFn, NativeRegs, NativeTable, ZvmCtx};
    use crate::heap::{ObjHandle, ObjKind};
    use crate::trap::Trap;
    use std::collections::BTreeMap;

    /// Play-scope data natives read during a call (`docs/34` §14.3.3).
    ///
    /// Owned by the [`crate::Zvm`] that is running, so two worlds do not share skill,
    /// lumps, or inventory grants.
    #[derive(Clone, Debug)]
    pub struct ZvmPlayHost {
        /// `skill` global. Default 3 (Hurt Me Plenty).
        pub skill: i32,
        /// `consoleplayer`.
        pub consoleplayer: i32,
        /// Lowercase hexadecimal MD5 for the active map's gameplay lumps.
        pub level_checksum: String,
        /// `gamestate` (`0` = `GS_LEVEL`).
        pub gamestate: i32,
        /// In-memory lumps for `Wads.*`, keyed by name. Index is `BTreeMap` order.
        pub wads: BTreeMap<String, String>,
        /// `GiveInventory` / `TakeInventory` (negative amount) recorded for the world to apply.
        pub grants: Vec<(i64, String, i32)>,
        /// Integer CVars (`CVar.GetCVar`).
        pub cvars: BTreeMap<String, i32>,
        /// Floating-point CVars. Keys are normalized to lowercase on writes.
        pub cvar_floats: BTreeMap<String, f64>,
        /// String CVars. Keys are normalized to lowercase on writes.
        pub cvar_strings: BTreeMap<String, String>,
        /// Floating-point defaults for CVars whose declared type is float.
        pub cvar_float_defaults: BTreeMap<String, f64>,
        /// String defaults for CVars whose declared type is string.
        pub cvar_string_defaults: BTreeMap<String, String>,
        /// Menu labels from `SetCVarDescription`, keyed by CVar name.
        pub cvar_desc: BTreeMap<String, String>,
        /// `Console.Printf` lines.
        pub printf: Vec<String>,
        /// `EventHandler.Find` instances.
        pub handlers: BTreeMap<String, ObjHandle>,
        /// `Actor.ChangeTID` log `(cookie, tid)`.
        pub tid_changes: Vec<(i64, i32)>,
        /// Explicit `PlayerInGame` answers. Missing keys: player 0 is in game.
        pub players_in_game: BTreeMap<i32, bool>,
        /// `GetPlayerTID` / `SetPlayerTID`.
        pub player_tids: BTreeMap<i32, i32>,
        /// Player-mo cookie → player number.
        pub mo_player: BTreeMap<i64, i32>,
        /// `SoundClass` written for a player number.
        pub sound_class: BTreeMap<i32, String>,
        /// `gametic`. The host sets this before a script runs.
        pub gametic: i32,
        /// Stable `WP_NOCHANGE` weapon sentinel. Null until the first read.
        pub wp_nochange: ObjHandle,
        /// Map name from `Level.SetInterMusic`. Empty until the stats screen sets it.
        pub inter_music: String,
        /// Name from `PlayerMenu.PlayerNameChanged`. Empty until the menu sets it.
        pub player_name: String,
        /// Degrees from `PlayerMenu.AutoaimChanged`. 35 until the menu sets it.
        pub autoaim: f64,
        /// RGB from `PlayerMenu.ColorChanged`. Absent until the menu sets it.
        pub color_rgb: Option<(u8, u8, u8)>,
        /// Actor natives queued for the playsim (`A_Look`, `LineAttack`, …).
        pub actor_ops: Vec<super::ZsActorOp>,
        /// `LevelLocals` sky / fog natives queued for the playsim.
        pub level_ops: Vec<super::ZsLevelOp>,
        /// Other play-scope natives queued for the playsim (UZ3 / UZ7).
        pub native_ops: Vec<super::ZsNativeOp>,
        /// DECORATE / ZScript `replaces`: lowercase class → replacement.
        pub replacements: BTreeMap<String, String>,
        /// Lowercase class → lowercase parent (empty for roots).
        pub class_parents: BTreeMap<String, String>,
        /// Lowercase abstract class names.
        pub abstract_classes: std::collections::BTreeSet<String>,
        /// Weapon slots 0–9: weapon class names in slot order.
        pub weapon_slots: Vec<Vec<String>>,
        /// Live actors this tic `(lowercase class, position)`, for
        /// `CheckProximity` (filled by the playsim when a program needs it).
        pub census: Vec<(String, [f64; 3])>,
        /// Physical corpse candidate for the current actor VM call.
        pub resurrection_candidate: Option<(u32, u32)>,
        /// Registered name of the native currently running (`Actor.A_SetSize`).
        pub native_name: String,
        /// Arguments pushed to the running native per bank (int, float,
        /// string, object): the register slices are padded past these.
        pub native_arg_counts: [usize; 4],
        /// ZScript class-id → class name table for `class<T>` values passed to natives.
        pub class_names: Vec<String>,
        /// `TextureID` table: id `n > 0` names `texture_names[n - 1]`
        /// (`TexMan.CheckForTexture` interns; 0 = null, −1 = invalid).
        pub texture_names: Vec<String>,
        /// Script class layouts used by `Object.New`, keyed by lowercase class name.
        pub object_layouts: BTreeMap<String, (u32, u16, u16, u16)>,
        /// Per-player `(2d distance², vertical-adjusted distance², visible)` for the actor
        /// whose ZScript method is currently running.
        pub ai_observers: Vec<(f64, f64, bool)>,
        /// Slots of `Actor` native fields in script objects (set at load).
        pub actor_slots: crate::world_natives::ActorSlots,
        /// Per call: line of sight from the calling actor to each mirrored object.
        pub sight: Vec<(ObjHandle, bool)>,
        /// Per call: `CheckMeleeRange()` of the calling actor against its target.
        pub melee_ok: bool,
        /// Per call: the calling actor's target object.
        pub self_target: ObjHandle,
        /// Per call: mirrored actors' inventory `(class lowercase, amount, max)`.
        pub inventory: Vec<(ObjHandle, Vec<(String, i32, i32)>)>,
        /// Results of a multi-return native (`int, int F()`), set by the native.
        pub multi_ret: Vec<crate::interp::Value>,
    }

    impl Default for ZvmPlayHost {
        fn default() -> Self {
            Self {
                skill: 3,
                consoleplayer: 0,
                level_checksum: String::new(),
                gamestate: 0,
                wads: BTreeMap::new(),
                grants: Vec::new(),
                cvars: BTreeMap::new(),
                cvar_floats: BTreeMap::new(),
                cvar_strings: BTreeMap::new(),
                cvar_float_defaults: BTreeMap::new(),
                cvar_string_defaults: BTreeMap::new(),
                cvar_desc: BTreeMap::new(),
                printf: Vec::new(),
                handlers: BTreeMap::new(),
                tid_changes: Vec::new(),
                players_in_game: BTreeMap::new(),
                player_tids: BTreeMap::new(),
                mo_player: BTreeMap::new(),
                sound_class: BTreeMap::new(),
                actor_ops: Vec::new(),
                level_ops: Vec::new(),
                native_ops: Vec::new(),
                replacements: BTreeMap::new(),
                class_parents: BTreeMap::new(),
                abstract_classes: std::collections::BTreeSet::new(),
                weapon_slots: Vec::new(),
                census: Vec::new(),
                resurrection_candidate: None,
                native_name: String::new(),
                native_arg_counts: [0; 4],
                class_names: Vec::new(),
                texture_names: Vec::new(),
                object_layouts: BTreeMap::new(),
                ai_observers: Vec::new(),
                actor_slots: crate::world_natives::ActorSlots::default(),
                sight: Vec::new(),
                melee_ok: false,
                self_target: ObjHandle::NULL,
                inventory: Vec::new(),
                multi_ret: Vec::new(),
                gametic: 0,
                wp_nochange: ObjHandle::NULL,
                inter_music: String::new(),
                player_name: String::new(),
                autoaim: 35.0,
                color_rgb: None,
            }
        }
    }

    impl ZvmPlayHost {
        /// Intern a texture name as a `TextureID`: `""` is invalid (−1),
        /// `"-"` the null texture (0), as `FTextureManager::CheckForTexture`.
        pub fn intern_texture(&mut self, name: &str) -> i64 {
            if name.is_empty() {
                return -1;
            }
            if name == "-" {
                return 0;
            }
            if let Some(i) = self
                .texture_names
                .iter()
                .position(|n| n.eq_ignore_ascii_case(name))
            {
                return i as i64 + 1;
            }
            self.texture_names.push(name.to_owned());
            self.texture_names.len() as i64
        }

        /// Name of `TextureID` `id` (`None` for null / invalid / unknown).
        #[must_use]
        pub fn texture_name(&self, id: i64) -> Option<&str> {
            usize::try_from(id - 1)
                .ok()
                .and_then(|i| self.texture_names.get(i))
                .map(String::as_str)
        }

        /// `Wads.CheckNumForName` / `CheckNumForFullName` over the in-memory map.
        #[must_use]
        pub fn wad_check(&self, name: &str) -> i32 {
            self.wads
                .keys()
                .position(|k| k.eq_ignore_ascii_case(name))
                .map_or(-1, |i| i as i32)
        }

        /// `Wads.FindLump`: find the next matching entry at or after `start`.
        /// The current injected lump view has no archive namespace metadata, so
        /// the namespace filter is intentionally applied by its caller only
        /// when that metadata becomes available.
        #[must_use]
        pub fn wad_find(&self, name: &str, start: i32) -> i32 {
            let start = usize::try_from(start.max(0)).unwrap_or(usize::MAX);
            self.wads
                .keys()
                .enumerate()
                .skip(start)
                .find(|(_, key)| key.eq_ignore_ascii_case(name))
                .map_or(-1, |(index, _)| i32::try_from(index).unwrap_or(-1))
        }

        /// `Wads.ReadLump`.
        #[must_use]
        pub fn wad_read(&self, lump: i32) -> Option<String> {
            if lump < 0 {
                return None;
            }
            self.wads.values().nth(lump as usize).cloned()
        }

        /// Built-in value used when a CVar has not been set.
        #[must_use]
        pub fn cvar_default(&self, name: &str) -> i32 {
            let key = name.to_ascii_lowercase();
            if let Some(value) = self.cvar_float_defaults.get(&key) {
                return *value as i32;
            }
            if let Some(value) = self.cvar_string_defaults.get(&key) {
                return value.trim().parse::<i32>().unwrap_or(0);
            }
            match name.to_ascii_lowercase().as_str() {
                "screenblocks" => 10,
                "skill" => self.skill,
                "freelook" | "cl_run" | "alwaysrun" | "sv_ammofactor" => 1,
                _ => 0,
            }
        }

        /// Integer CVar, ASCII case-insensitive. Missing names use [`Self::cvar_default`].
        #[must_use]
        pub fn cvar(&self, name: &str) -> i32 {
            let key = name.to_ascii_lowercase();
            if let Some(value) = self.cvar_strings.get(&key) {
                return value.trim().parse::<i32>().unwrap_or(0);
            }
            if let Some(value) = self.cvar_floats.get(&key) {
                return *value as i32;
            }
            self.cvars
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case(name))
                .map_or_else(|| self.cvar_default(name), |(_, v)| *v)
        }

        /// Floating CVar value, falling back to the integer/default value.
        #[must_use]
        pub fn cvar_float(&self, name: &str) -> f64 {
            let key = name.to_ascii_lowercase();
            if let Some(value) = self.cvar_floats.get(&key) {
                return *value;
            }
            if let Some(value) = self.cvar_strings.get(&key) {
                return value.trim().parse::<f64>().unwrap_or(0.0);
            }
            if let Some(value) = self.cvars.get(&key) {
                return f64::from(*value);
            }
            self.cvar_default_float(name)
        }

        /// Floating default value with integer fallback.
        #[must_use]
        pub fn cvar_default_float(&self, name: &str) -> f64 {
            self.cvar_float_defaults
                .get(&name.to_ascii_lowercase())
                .copied()
                .unwrap_or_else(|| f64::from(self.cvar_default(name)))
        }

        /// String default value with typed numeric conversion fallback.
        #[must_use]
        pub fn cvar_default_string(&self, name: &str) -> String {
            let key = name.to_ascii_lowercase();
            if let Some(value) = self.cvar_string_defaults.get(&key) {
                value.clone()
            } else if let Some(value) = self.cvar_float_defaults.get(&key) {
                value.to_string()
            } else {
                self.cvar_default(name).to_string()
            }
        }

        /// CVar text value using UZDoom-style type conversion.
        #[must_use]
        pub fn cvar_string(&self, name: &str) -> String {
            let key = name.to_ascii_lowercase();
            if let Some(value) = self.cvar_strings.get(&key) {
                return value.clone();
            }
            match key.as_str() {
                "name" | "playername" => return "Player".into(),
                "language" => return "en".into(),
                "playerclass" => return "DoomPlayer".into(),
                "gender" => return "male".into(),
                _ => {}
            }
            if let Some(value) = self.cvar_floats.get(&key) {
                return value.to_string();
            }
            if let Some(value) = self.cvar_string_defaults.get(&key) {
                return value.clone();
            }
            if let Some(value) = self.cvar_float_defaults.get(&key) {
                return value.to_string();
            }
            self.cvar(name).to_string()
        }

        /// CVar runtime type (`CVAR_Int` = 1, `CVAR_Float` = 2, `CVAR_String` = 3).
        #[must_use]
        pub fn cvar_type(&self, name: &str) -> i32 {
            let key = name.to_ascii_lowercase();
            if self.cvar_strings.contains_key(&key) || self.cvar_string_defaults.contains_key(&key)
            {
                3
            } else if self.cvar_floats.contains_key(&key)
                || self.cvar_float_defaults.contains_key(&key)
            {
                2
            } else {
                1
            }
        }

        /// Store an integer CVar and clear any previous typed value.
        pub fn set_cvar_int(&mut self, name: &str, value: i32) {
            let key = name.to_ascii_lowercase();
            self.cvar_floats.remove(&key);
            self.cvar_strings.remove(&key);
            self.cvars.insert(key, value);
        }

        /// Store a floating-point CVar and its integer conversion.
        pub fn set_cvar_float(&mut self, name: &str, value: f64) {
            let key = name.to_ascii_lowercase();
            self.cvar_strings.remove(&key);
            self.cvars.insert(key.clone(), value as i32);
            self.cvar_floats.insert(key, value);
        }

        /// Store a string CVar and its numeric conversion when available.
        pub fn set_cvar_string(&mut self, name: &str, value: &str) {
            let key = name.to_ascii_lowercase();
            self.cvar_floats.remove(&key);
            self.cvars
                .insert(key.clone(), value.trim().parse::<i32>().unwrap_or(0));
            self.cvar_strings.insert(key, value.to_owned());
        }

        /// Install a floating-point CVar default.
        pub fn set_cvar_float_default(&mut self, name: &str, value: f64) {
            self.cvar_float_defaults
                .insert(name.to_ascii_lowercase(), value);
        }

        /// Install a string CVar default.
        pub fn set_cvar_string_default(&mut self, name: &str, value: &str) {
            self.cvar_string_defaults
                .insert(name.to_ascii_lowercase(), value.to_owned());
        }

        /// `EventHandler.Find`.
        #[must_use]
        pub fn find_handler(&self, name: &str) -> ObjHandle {
            self.handlers
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case(name))
                .map_or(ObjHandle::NULL, |(_, &v)| v)
        }

        /// `PlayerInGame`.
        #[must_use]
        pub fn player_in_game(&self, pn: i32) -> i32 {
            i32::from(self.players_in_game.get(&pn).copied().unwrap_or(pn == 0))
        }
    }

    /// Register BD-priority natives on a table. Returns name→id map.
    pub fn register_bd_natives(table: &mut NativeTable) -> BTreeMap<String, u32> {
        let mut ids = BTreeMap::new();
        let mut add = |name: &str, f: NativeFn| {
            let id = table.register(name, f);
            ids.insert(name.to_owned(), id);
        };
        add("Wads.CheckNumForFullName", wad_check_full);
        add("Wads.CheckNumForName", wad_check_name);
        add("Wads.ReadLump", wad_read_lump);
        add("Console.Printf", console_printf);
        add("consoleplayer", consoleplayer_get);
        add("skill", skill_get);
        add("gamestate", gamestate_get);
        add("Object.Destroy", object_destroy);
        add("Array.Size", array_size);
        add("Array.Push", array_push_i);
        add("Array.Clear", array_clear);
        add("Array.Delete", array_delete);
        add("Array.Insert", array_insert);
        add("Array.InsertS", array_insert_s);
        add("Array.NewI", array_new_i);
        add("Array.NewS", array_new_s);
        add("Array.PushS", array_push_s);
        add("Array.FindS", array_find_s);
        add("String.CharCodeAt", string_char_code_at);
        add("String.ByteAt", string_byte_at);
        add("String.Length", string_length);
        add("String.Mid", string_mid);
        add("String.Format", string_format);
        add("String.ToInt", string_to_int);
        add("String.ToDouble", string_to_double);
        add("String.Eq", string_eq);
        add("String.Left", string_left);
        add("String.Right", string_right);
        add("String.MakeLower", string_make_lower);
        add("String.MakeUpper", string_make_upper);
        add("String.Replace", string_replace);
        add("String.CharLower", string_char_lower);
        add("String.Filter", string_filter);
        add("String.IndexOf", string_index_of);
        add("String.LastIndexOf", string_right_index_of);
        add("String.RightIndexOf", string_right_index_of);
        add("String.IsEmpty", string_is_empty);
        add("String.IsNotEmpty", string_is_not_empty);
        add("String.IsInt", string_is_int);
        add("String.IsDouble", string_is_double);
        add("String.Compare", string_compare);
        add("String.CompareNoCase", string_compare_nocase);
        add("String.Split", string_split);
        add("StringTable.Localize", string_identity);
        add("StringTable.GetUnlocalizedString", string_identity);
        add("font.StringWidth", font_string_width);
        add("font.GetCharWidth", font_char_width);
        add("font.GetHeight", font_height);
        add("font.GetMaxAscender", font_ascender);
        add("font.CanPrint", font_can_print);
        add("font.IsValidDynamicFont", font_not_dynamic);
        add("font.BreakLines", font_new);
        add("ThinkerIterator.Create", font_new);
        add("BlockThingsIterator.Create", font_new);
        add("ThinkerIterator.Next", object_next_null);
        add("BlockThingsIterator.Next", object_next_null);
        add("Menu.GetCurrentMenu", object_next_null);
        add("MenuDescriptor.GetDescriptor", object_next_null);
        add("SavegameManager.GetManager", object_next_null);
        add("AltIcon.isNull", object_is_null);
        add("AltIcon.Exists", actor_false);
        add("AltIcon.isValid", object_is_valid);
        add("Icon.isValid", object_is_valid);
        add("Sucks.IsValid", object_is_valid);
        for name in [
            "HUDFont.Create",
            "InterBackground.Create",
            "InventoryBarState.Create",
            "InventoryBarState.CreateNoBox",
            "Menu.OptionFont",
        ] {
            add(name, font_new);
        }
        add("MoviePlayer.Create", object_next_null);
        add("MoviePlayerJob.CreateWithSoundInfo", object_next_null);
        add("TextEnterMenu.OpenTextEnter", object_next_null);
        add("console.midprint", console_midprint);
        add("DynamicValueInterpolator.Create", font_new);
        add("InputGridChars.ByteAt", actor_false);
        add("OptionValues.GetValue", actor_false);
        add("actor.DropInventory", actor_false);
        for name in [
            "CreateActorIterator",
            "CreateLineIdIterator",
            "CreateSectorTagIterator",
            "PointInSector",
            "pickplayerstart",
            "pickdeathmatchstart",
        ] {
            add(&format!("levellocals.{name}"), font_new);
        }
        for name in [
            "FindHealthGroup",
            "GetSpotState",
            "getskyboxportal",
            "SpawnVisualThinker",
        ] {
            add(&format!("levellocals.{name}"), object_next_null);
        }
        for name in ["getclustername", "getepisodename", "getudmfstring"] {
            add(&format!("levellocals.{name}"), actor_tag);
        }
        add("levellocals.getudmfint", actor_false);
        add("levellocals.getudmffloat", actor_flat_aim);
        add("levellocals.vec3diff", actor_zero_vec);
        add("levellocals.givesecret", actor_false);
        add("levellocals.setintermusic", level_set_inter_music);
        // `Level.X(...)` lowers to the `level` global's name; bind both spellings.
        for recv in ["levellocals", "level"] {
            add(&format!("{recv}.setskyfog"), level_set_sky_fog);
            add(&format!("{recv}.setthickfog"), level_set_thick_fog);
            add(&format!("{recv}.changeskymist"), level_change_sky_mist);
        }
        add("levellocals.exitlevel", actor_false);
        add("levellocals.executespecial", actor_false);
        add("playerclass.GetColorSetName", player_color);
        add("playerpawn.Distance2D", global_distance2d);
        add("playerpawn.GetClass", font_new);
        for name in ["CheckKeys", "GiveInventoryType", "IsTeammate"] {
            add(&format!("playerpawn.{name}"), actor_false);
        }
        for name in ["FindInventory", "FirstInv", "PickNewWeapon"] {
            add(&format!("playerpawn.{name}"), object_next_null);
        }
        add("playerinfo.getteam", player_team);
        add("playerinfo.getcolorset", player_color_set);
        for name in [
            "GetDeltaViewHeight",
            "GetMoveBob",
            "getstillbob",
            "getwbobfire",
            "getaveragelatency",
            "poisondamage",
            "setpsprite",
        ] {
            add(&format!("playerinfo.{name}"), actor_false);
        }
        add("playerinfo.FindPSprite", object_next_null);
        add("playerinfo.GetPSprite", object_next_null);
        for name in [
            "PlayerNumber",
            "isFrozen",
            "cheatsuicide",
            "checkenvironment",
            "checkmusicchange",
            "checkweaponbuttons",
            "getpainflashfortype",
            "SetInventory",
        ] {
            add(&format!("playerpawn.{name}"), actor_false);
        }
        for name in [
            "GetTextureHeight",
            "GetTextureLeftOffset",
            "GetTextureTopOffset",
            "GetTextureWidth",
            "palettecolor",
        ] {
            add(&format!("screen.{name}"), actor_false);
        }
        add("texman.checkrealheight", actor_false);
        add("texman.okforlocalization", actor_false);
        add("texman.getsize", actor_zero_vec);
        add("texman.getscaledsize", actor_zero_vec);
        add("texman.getscaledoffset", actor_zero_vec);
        add("texture.IsValid", object_is_valid);
        add("translation.getid", actor_missing_index);
        add("translation.makeid", actor_missing_index);
        add("translation.setplayertranslation", actor_false);
        add("soundinfo", font_new);
        add("sounds", font_new);
        add("soundsequenceslot.StartSoundSequence", actor_start_sound);
        add("actoriterator.Next", object_next_null);
        add("array.Find", array_find);
        add("array.Get", array_at);
        add("array.CheckKey", array_has);
        add("array.CountUsed", array_size);
        add("array.Pushv", object_push);
        add("althud.getlatency", actor_false);
        add("basestatusbar.isscoreboardopen", actor_false);
        add("alwaysapplydmflags", actor_false);
        add("basestatusbar.ValidateInvFirst", object_next_null);
        add("basestatusbar.getinventoryicon", object_next_null);
        add("basestatusbar.getmugshot", object_next_null);
        add("ammo.GetClass", font_new);
        add("chaintex.IsValid", object_is_valid);
        add("cl_run", cvar_cl_run);
        add("cleanwidth", screen_width);
        add("cleanwidth_1", screen_width);
        add("cleanheight", screen_height);
        add("ReplyText.AppendFormat", reply_append);
        for name in [
            "am_showcluster",
            "am_showepisode",
            "am_showtime",
            "am_showtotaltime",
        ] {
            add(name, actor_false);
        }
        add("Dictionary.Create", dict_new);
        add("dictionary.at", dict_at);
        add("dictionary.insert", dict_insert);
        add("Dictionary.Remove", dict_remove);
        add("Dictionary.ToString", dict_to_string);
        add("Dictionary.FromString", dict_from_string);
        add("DictionaryIterator.Create", dict_iter_new);
        add("DictionaryIterator.Next", dict_iter_next);
        add("DictionaryIterator.Key", dict_iter_key);
        add("DictionaryIterator.Value", dict_iter_value);
        add("actor.instatesequence", actor_false);
        add("cursorTex.IsValid", object_is_valid);
        add("custommessageboxmenubase.optioncount", actor_false);
        add("custommessageboxmenubase.optionxoffset", actor_false);
        add(
            "custommessageboxmenubase.optionforshortcut",
            actor_missing_index,
        );
        add("custommessageboxmenubase.optionname", actor_tag);
        add("damagetypedefinition.ignorearmor", actor_false);
        for name in [
            "deathmatch",
            "dmflags",
            "dmflags2",
            "dmflags3",
            "demoplayback",
            "gameaction",
        ] {
            add(name, actor_false);
        }
        add("diff.Length", vec_length);
        add("distvec.length", vec_length);
        add("dlg_vgafont", font_new);
        add("flamethrower.FindState", object_next_null);
        add("gametic", host_gametic);
        add("gameticrate", host_ticrate);
        add("gl_texture_hqresizemode", actor_false);
        add("gl_texture_hqresizemult", level_yes);
        add("decal.spawndecal", object_next_null);
        add("conversationmenu.sendconversationreply", actor_false);
        add("dehackedpickup.determinetype", object_next_null);
        for name in ["cleanxfac", "cleanxfac_1", "cleanyfac", "cleanyfac_1"] {
            add(name, level_yes);
        }
        for name in ["hud_health_red", "hud_armor_red", "hud_ammo_red"] {
            add(name, hud_25);
        }
        for name in ["hud_health_yellow", "hud_armor_yellow", "hud_ammo_yellow"] {
            add(name, hud_50);
        }
        for name in ["hud_health_green", "hud_armor_green", "hud_berserk_health"] {
            add(name, hud_100);
        }
        for name in [
            "compatflags",
            "compatflags2",
            "hud_ammo_order",
            "hud_showammo",
            "hud_showangles",
            "hud_showitems",
            "hud_showlag",
            "hud_showmonsters",
            "hud_showscore",
            "hud_showsecrets",
            "hud_showstats",
            "hud_showtime",
            "hud_showtimestat",
            "hud_showweapons",
            "hud_swaphealtharmor",
            "hud_timecolor",
            "hudcolor_ltim",
            "hudcolor_statnames",
            "hudcolor_stats",
            "hudcolor_time",
            "hudcolor_titl",
            "hudcolor_ttim",
            "hudcolor_xyco",
            "idmypos",
        ] {
            add(name, actor_false);
        }
        add("lines", empty_int_array);
        add("log_vgafont", font_new);
        add("nullptr", object_next_null);
        for name in [
            "sb_cooperative_enable",
            "snd_footstepvolume",
            "snd_menuvolume",
            "sv_ammofactor",
        ] {
            add(name, level_yes);
        }
        for name in [
            "netgame",
            "paletteflash",
            "sb_cooperative_headingcolor",
            "sb_cooperative_otherplayercolor",
            "sb_cooperative_yourplayercolor",
            "sb_deathmatch_enable",
            "sb_deathmatch_headingcolor",
            "sb_deathmatch_otherplayercolor",
            "sb_deathmatch_yourplayercolor",
            "sb_teamdeathmatch_enable",
            "sb_teamdeathmatch_headingcolor",
            "sv_barrelrespawn",
            "sv_coophalveammo",
            "sv_cooploseammo",
            "sv_cooplosearmor",
            "sv_cooploseinventory",
            "sv_cooplosekeys",
            "sv_cooplosepowerups",
            "sv_cooploseweapons",
            "sv_coopsharekeys",
            "sv_degeneration",
            "sv_disableautohealth",
            "sv_dropstyle",
            "sv_forcerespawn",
            "sv_infiniteammo",
            "sv_infiniteinventory",
            "sv_itemrespawn",
            "sv_killbossmonst",
            "sv_localitems",
            "sv_noarmor",
        ] {
            add(name, actor_false);
        }
        add("ticrate", host_ticrate);
        add("maxplayers", host_max_players);
        for name in [
            "m_cleanscale",
            "m_use_mouse",
            "m_tooltip_alpha",
            "vid_gamma",
            "vid_i_whitepoint",
            "wi_percents",
        ] {
            add(name, level_yes);
        }
        for name in [
            "m_show_backbutton",
            "m_showinputgrid",
            "map_point_coordinates",
            "multiplayer",
            "net_cutscenereadytype",
            "sv_nobfgaim",
            "sv_noexit",
            "sv_noextraammo",
            "sv_nohealth",
            "sv_nolocaldrops",
            "sv_nomonsters",
            "sv_norespawn",
            "sv_respawnprotect",
            "sv_respawnsuper",
            "sv_singleplayerrespawn",
            "sv_unlimited_pickup",
            "sv_weapondrop",
            "sv_weaponstay",
            "teamplay",
            "timelimit",
            "vid_i_blackpoint",
            "wi_autoadvance",
            "wi_showtotaltime",
            "wipetype",
        ] {
            add(name, actor_false);
        }
        add("global.BulletSlope", actor_flat_aim);
        add("global.FindInventory", object_next_null);
        add("global.FindState", object_next_null);
        add("colorpickermenu.GetColor", player_color);
        for name in [
            "ACS_Execute",
            "ACS_ExecuteAlways",
            "ACS_ExecuteWithResult",
            "ACS_LockedExecute",
            "ACS_LockedExecuteDoor",
            "ACS_Suspend",
            "ACS_Terminate",
            "AddAmmo",
            "Door_Close",
            "Door_Open",
            "Floor_LowerToHighestEE",
            "Floor_LowerToLowest",
        ] {
            add(&format!("global.{name}"), actor_false);
        }
        add("compareFunc.Call", actor_false);
        add("global.GetClass", font_new);
        add("global.GetDefaultByType", font_new);
        add("global.GetCurrentMenu", object_next_null);
        add("global.GetPointer", object_next_null);
        add("global.GetAltAtkState", object_next_null);
        add("global.GetUpState", object_next_null);
        add("global.PickNewWeapon", object_next_null);
        add("global.GetSliderValue", actor_false);
        add("global.GiveInventoryType", actor_false);
        add("global.Thing_Destroy", actor_false);
        add("global.StartSoundSequenceID", actor_false);
        add("global.call", actor_false);
        add("global.TweakSpeeds", level_yes);
        add("global.MSTimeF", host_ms_time);
        add("global.StatusbarToRealCoords", actor_vec2_offset);
        add("global.double", global_double);
        add("global.name", reply_append);
        for name in [
            "Spawn",
            "SpawnBlood",
            "SpawnMissile",
            "SpawnMissileAngle",
            "SpawnMissileAngleZ",
            "SpawnMissileXYZ",
            "SpawnMissileZ",
            "SpawnMissileZAimed",
            "SpawnPlayerMissile",
            "SpawnPuff",
            "SpawnSubMissile",
        ] {
            add(&format!("global.{name}"), object_next_null);
        }
        add("global.sound", object_next_null);
        add("sector.GetHeightSec", object_next_null);
        add("sector.SetEnvironmentID", sector_set_environment_id);
        add("sector.SetEnvironment", sector_set_environment);
        add("sector.LowestFloorAt", actor_flat_aim);
        add("sector.nextlowestfloorat", actor_flat_aim);
        add("sector.NextHighestCeilingAt", sector_ceiling);
        for name in ["line.index", "sector.index", "side.index"] {
            add(name, actor_missing_index);
        }
        add("goodbyestr.Left", string_left);
        add("goodbyestr.Length", string_length);
        add("hotkey.GetNextCodePoint", actor_missing_index);
        add("icon.SetInvalid", object_next_null);
        add("icon.SetNull", object_next_null);
        add("haptics.rumble", actor_false);
        add("custominventory.callstatechain", actor_false);
        add("destructible.projectilehitplane", actor_false);
        add("ambientsound.activate", actor_start_sound);
        add("icon.isNull", object_is_null);
        add("it.Next", object_next_null);
        add("it.GetKey", actor_tag);
        add("inventory.GetClass", font_new);
        add("inventory.DisplayNameTag", actor_tag);
        add("inventory.printpickupmessage", console_midprint);
        add("inventory.Destroy", object_next_null);
        for name in ["NextInv", "PrevInv", "CreateTossable"] {
            add(&format!("inventory.{name}"), object_next_null);
        }
        add("joystickconfig.GetSensitivity", level_yes);
        add("joystickconfig.GetAxisScale", level_yes);
        add("joystickconfig.GetAxisName", actor_tag);
        add("keybindings.getbinding", actor_tag);
        add("keybindings.nameallkeys", key_name_all);
        add("key.getkeytypecount", key_type_count);
        add("key.getkeytype", key_type);
        add("interpolationpoint.ScanForLoop", actor_false);
        for name in [
            "joystickconfig.getnumaxes",
            "joystickconfig.hashaptics",
            "joystickconfig.GetAxisDeadZone",
            "joystickconfig.GetHapticsStrength",
        ] {
            add(name, actor_false);
        }
        add("mCurrentTooltip.IsEmpty", string_is_empty);
        add("mEnterString.AppendCharacter", string_append_char);
        add("mEnterString.DeleteLastCharacter", string_delete_last);
        add("mTexture.Exists", object_is_valid);
        add("m_tooltip_lines", tooltip_lines);
        add("m_tooltip_speed", level_yes);
        add("menu.setmenu", object_next_null);
        for name in [
            "m_tooltip_delay",
            "m_tooltip_small",
            "m_tooltip_capratio",
            "menu.activatemenu",
            "menu.close",
            "menu.menutime",
        ] {
            add(name, actor_false);
        }
        add("label.GetNextCodePoint", actor_missing_index);
        add("left.IsValid", object_is_valid);
        add("mid.IsValid", object_is_valid);
        add("levellocals.getautomapposition", actor_zero_vec);
        add("menu.startmessage", console_midprint);
        add("menudelegatebase.PickFont", font_new);
        add("movieplayer.GetTexture", object_next_null);
        add("negviewpos.Angle", vec_angle);
        add("object.AddSpot", object_next_null);
        for name in [
            "createceiling",
            "createfloor",
            "setfrozen",
            "startintermission",
            "startslideshow",
        ] {
            add(&format!("levellocals.{name}"), actor_false);
        }
        add("menu.setmousecapture", actor_false);
        add("messageboxmenu.callhandler", actor_false);
        for name in [
            "Activate",
            "ActivateMenu",
            "AttachToOwner",
            "CallTryPickup",
            "CheckCoordinate",
            "CheckHotkey",
            "Close",
            "DepleteBy",
            "DepleteOrDestroy",
        ] {
            add(&format!("object.{name}"), actor_false);
        }
        add("object.SetInvalid", object_next_null);
        add("object.GetAction", actor_tag);
        add("object.GetTooltip", actor_tag);
        add("object.GetIndent", actor_false);
        add("object.ReleaseFocus", object_release_focus);
        add("object.RemoveSpot", object_next_null);
        for name in [
            "SetState",
            "SetStateLabel",
            "Enable",
            "Start",
            "Init",
            "InitNamed",
        ] {
            add(&format!("object.{name}"), actor_false);
        }
        for name in ["MenuEvent", "MouseEvent", "OnEvent", "ProcessInput"] {
            add(&format!("object.{name}"), actor_false);
        }
        add("object.Use", actor_false);
        add("object.mstimef", host_ms_time);
        add("object.g_skillpropertyint", actor_false);
        add("object.isclass", actor_false);
        add("object.builtinnametoclass", object_next_null);
        for name in [
            "s_pausesound",
            "s_resumesound",
            "setmusicvolume",
            "throwabortexception",
        ] {
            add(&format!("object.{name}"), actor_false);
        }
        add("weapon.GetClass", font_new);
        add("weapon.PickupMessage", object_pickup_message);
        add("weapon.PlayPickupSound", actor_start_sound);
        add("weapon.isAbstract", actor_false);
        for name in [
            "CreateTossable",
            "Destroy",
            "FindState",
            "GetAltAtkState",
            "GetAtkState",
            "GetDownState",
            "GetDropItems",
            "GetReadyState",
            "GetUpState",
        ] {
            add(&format!("weapon.{name}"), object_next_null);
        }
        for name in [
            "offtex.IsValid",
            "ontex.IsValid",
            "prevpic.IsValid",
            "right.IsValid",
            "picnum.isValid",
            "picnum.Exists",
            "slot.IsValid",
        ] {
            add(name, object_is_valid);
        }
        add("optionmenu.GetItem", object_next_null);
        add("optionvalues.getcount", actor_false);
        for name in ["gettext", "gettextvalue", "gettooltip"] {
            add(&format!("optionvalues.{name}"), actor_tag);
        }
        add("levelpostprocessor.getdefaultactor", object_next_null);
        add("levelpostprocessor.getthingflags", actor_false);
        add("savegamemanager.GetSavegame", object_next_null);
        add("screenjobrunner.getreadytimer", actor_false);
        add("screenjobrunner.isplayerready", actor_false);
        add("seqnode.getsequenceslot", object_next_null);
        add("spotstate.GetNextInList", object_next_null);
        add("state.GetRandomSpot", object_next_null);
        add("state.GetSpotWithMinMaxDistance", object_next_null);
        add("state.GetSpriteTexture", object_next_null);
        add("state.distanceto", actor_flat_aim);
        add("statusbarcore.gethudscale", level_yes);
        add("joystickconfig.GetAxisMap", actor_missing_index);
        add("joystickconfig.allowsenabledinbackground", actor_false);
        add("joystickconfig.GetAxisDigitalThreshold", actor_flat_aim);
        add("joystickconfig.GetAxisResponseCurve", actor_false);
        add("joystickconfig.GetAxisResponseCurvePoint", actor_flat_aim);
        add("intermissioncontroller.Destroy", object_next_null);
        add("state.validatespriteframe", level_yes);
        add("super.GetIndent", actor_false);
        add("team.isvalid", actor_false);
        add("tex.IsValid", object_is_valid);
        add("tex.isNull", object_is_null);
        add("texid.IsValid", object_is_valid);
        add("text.IsEmpty", string_is_empty);
        add("type.GetParentClass", object_next_null);
        add("weaponslots.GetWeapon", object_next_null);
        add("pos.PlusZ", object_plus_z);
        add("os_Query.fromString", font_new);
        add("secplane.ZatPoint", actor_flat_aim);
        add("statusbarcore.formatnumber", statusbar_format_number);
        add("psprite.setstate", actor_false);
        add("strifedialoguereply.shouldskipreply", actor_false);
        add("super.DoSpecialDamage", actor_damage_factor);
        add("super.Die", actor_die);
        for name in [
            "AddInventory",
            "HandlePickup",
            "DoPickupSpecial",
            "Activate",
            "Deactivate",
            "AttachToOwner",
            "AttachToPlayer",
            "Grind",
        ] {
            add(&format!("super.{name}"), actor_false);
        }
        add("screenjobrunner.readyplayer", actor_false);
        add("screenjobrunner.resetreadytimer", actor_false);
        add("screenjobrunner.settransition", actor_false);
        add("optionmenuitemcommand.docommand", actor_false);
        add("playerpawn.ClearInventory", actor_false);
        add("brokenlines.Destroy", object_next_null);
        for name in ["MenuEvent", "MouseEvent", "OnInputEvent", "OnUIEvent"] {
            add(&format!("super.{name}"), actor_false);
        }
        add("super.PickupMessage", object_pickup_message);
        add("super.PlayPickupSound", actor_start_sound);
        add("super.RemoveInventory", actor_false);
        add("super.ShouldRespawn", actor_false);
        add("super.Slam", actor_false);
        add("super.TryDeleteMessage", actor_false);
        add("super.TryPickup", actor_false);
        add("super.UndoMonsterMorph", object_next_null);
        add("super.Use", actor_false);
        add("super.UseInventory", actor_false);
        add("system.stopallsounds", act_stop_sounds);
        add("system.stopmusic", actor_false);
        add("systemtime.format", actor_tag);
        add("systemtime.now", host_ms_time);
        add("text.ByteAt", string_byte_at);
        add("text.ToDouble", string_to_double);
        add("textentermenu.ActivateMenu", actor_false);
        add("thinker.changestatnum", actor_false);
        add("thrust.Unit", vec_unit);
        add("vect.Length", vec_length);
        add("weaponslots.LocateWeapon", actor_false);
        add("actor.thrust", actor_thrust);
        add("actor.setdamage", actor_set_damage);
        add("actor.setidle", actor_set_idle);
        add("actor.playactivesound", actor_active_sound);
        add("actor.playspawnsound", actor_spawn_sound);
        add("actor.howl", actor_howl);
        add("actor.clearbounce", actor_clear_bounce);
        add("actor.linktoworld", actor_link_world);
        add("actor.unlinkfromworld", actor_unlink_world);
        add("actor.findfloorceiling", actor_fit_sector);
        add("actor.restoredamage", actor_restore_damage);
        add("actor.removefromhash", actor_clear_tid);
        add("actor.clearcounters", actor_clear_counters);
        add("inventory.ClearCounters", actor_clear_counters);
        add("object.ClearCounters", actor_clear_counters);
        add("actor.damagemobj", actor_damage_mobj);
        add("playerpawn.DamageMobj", actor_damage_mobj);
        add("playerpawn.SetupCrouchSprite", player_setup_crouch_sprite);
        add("actor.tracebleed", actor_trace_bleed);
        add("actor.tracebleedangle", actor_trace_bleed);
        add("ftranslatedlinetarget.tracebleed", actor_trace_bleed);
        add("actor.drawsplash", actor_draw_splash);
        add("actor.disablelocalrendering", actor_local_hide);
        add("actor.setshade", actor_set_shade);
        add("object.SetRenderStyle", object_set_render_style);
        add("actor.setviewpos", actor_set_view_pos);
        add("actor.copyfriendliness", actor_copy_friendliness);
        add("actor.setfriendplayer", actor_set_friend_player);
        add("custommessageboxmenubase.handleresult", actor_false);
        for name in [
            "FindFont",
            "GetFont",
            "GetBigTextFont",
            "GetConsoleFont",
            "GetDescriptionFont",
            "GetSmallTextFont",
            "GetTitleFont",
        ] {
            add(&format!("Font.{name}"), font_new);
        }
        add("Screen.GetAspectRatio", screen_aspect);
        add("screen.GetWidth", screen_width);
        add("screen.GetHeight", screen_height);
        add("screen.GetClipRect", screen_clip);
        add("screen.GetFullscreenRect", screen_full);
        for name in [
            "gameticrate",
            "cleanwidth",
            "cleanwidth_1",
            "cleanheight",
            "cleanheight_1",
            "cleanxfac",
            "cleanxfac_1",
            "cleanyfac",
            "cleanyfac_1",
            "consoleplayer",
            "gamestate",
            "skill",
            "bigfont",
            "bigupper",
            "confont",
            "newconsolefont",
            "newsmallfont",
            "intermissionfont",
            "alternativebigfont",
            "alternativesmallfont",
            "bindings",
            "doublebindings",
            "automapbindings",
            "deh",
            "gameinfo",
            "level",
            "AllClasses",
            "AllActorClasses",
            "AllEpisodes",
            "AllSkills",
            "PlayerClasses",
            "PlayerSkins",
            "Teams",
            "Terrains",
            "allservices",
            "smallfont",
            "smallfont2",
            "originalbigfont",
            "originalsmallfont",
            "symbolsfont",
            "menudelegate",
            "musplaying",
            "optionmenusettings",
            "statusbar",
            "players",
            "notifyfontscale",
            "viewactive",
            "playeringame",
            "paused",
            "menuactive",
            "multiplayer",
            "netgame",
            "automapactive",
            "globalfreeze",
            "gameaction",
            "consolestate",
            "generic_ui",
            "backbuttonalpha",
            "backbuttontime",
            "localviewpitch",
            "demoplayback",
            "gametic",
            "net_arbitrator",
            "validcount",
            "skyflatnum",
            "wp_nochange",
        ] {
            add(&format!("_.{name}"), global_slot);
        }
        add("PlayerInfo.GetNextPlayerNumber", next_player_number);
        add("PlayerInfo.GetNextPlayer", next_player_obj);
        add("playerpawn.GetPrintableDisplayName", player_display_name);
        add("CVar.FindCVar", cvar_get_cvar);
        add("cvar.GetDefaultInt", cvar_get_default_int);
        add("cvar.GetFloat", cvar_get_float);
        add("cvar.GetDefaultFloat", cvar_get_default_float);
        add("cvar.GetString", cvar_get_string);
        add("cvar.GetDefaultString", cvar_get_default_string);
        add("cvar.SetInt", cvar_set_int);
        add("cvar.SetFloat", cvar_set_float);
        add("cvar.SetString", cvar_set_string);
        add("cvar.GetRealType", cvar_real_type);
        add("cvar.ResetToDefault", cvar_reset_to_default);
        add("font.FindFontColor", font_find_color);
        add("font.GetCursor", font_cursor);
        add("font.GetDisplacement", font_displacement);
        add("brokenlines.count", broken_count);
        add("brokenlines.stringat", broken_string_at);
        add("brokenlines.stringwidth", broken_count);
        add("PlayerInfo.GetAutoaim", player_autoaim);
        add("PlayerInfo.GetNeverSwitch", player_never_switch);
        add("PlayerInfo.GetPlayerClassNum", player_class_num);
        add("PlayerInfo.GetSkin", player_skin);
        add("PlayerInfo.GetWBobSpeed", level_yes);
        add("PlayerInfo.GetColor", player_info_color);
        add("PlayerInfo.GetDisplayColor", player_color);
        add("basestatusbar.GetTopOfStatusbar", status_top);
        add("levellocals.FindUniqueTid", level_yes);
        add("levellocals.FormatMapName", level_map_name);
        for name in [
            "IsCrouchingAllowed",
            "IsFreelookAllowed",
            "IsJumpingAllowed",
            "IsPointInLevel",
        ] {
            add(&format!("levellocals.{name}"), level_yes);
        }
        add("Actor.GetClassName", actor_get_class_name);
        add("players.at", players_at);
        // BD / ZS8 — Find + New wired; remaining stubs documented.
        add("EventHandler.Find", event_handler_find);
        add("Object.New", object_new);
        add("Actor.GiveInventory", actor_give_inventory);
        add("Actor.TakeInventory", actor_take_inventory);
        add("Actor.SetSoundClass", actor_set_sound_class);
        add("Actor.ChangeTID", actor_change_tid);
        add("PlayerInfo.GetGender", player_get_gender);
        add("PlayerInfo.GetUserName", player_user_name);
        add("playermenu.playernamechanged", player_name_changed);
        add("playermenu.genderchanged", player_gender_changed);
        add("playermenu.alwaysrunchanged", player_always_run_changed);
        add(
            "playermenu.switchonpickupchanged",
            player_switch_on_pickup_changed,
        );
        add("playermenu.autoaimchanged", player_autoaim_changed);
        add("playermenu.colorchanged", player_color_changed);
        add("playermenu.classchanged", player_class_changed);
        add("playermenu.teamchanged", player_team_changed);
        add("playermenu.colorsetchanged", player_color_set_changed);
        add("playermenu.skinchanged", player_skin_changed);
        add("CVar.GetCVar", cvar_get_cvar);
        add("CVar.GetInt", cvar_get_int);
        add(
            "optionmenuitemoption.setcvardescription",
            cvar_set_description,
        );
        add("Wads.FindLump", wad_find_lump);
        add("Wads.FindLumpFullName", wad_find_lump);
        add("LevelLocals.GetChecksum", level_checksum);
        // Room is not Zandronum — BD takes GZDoom branches (docs/33 §7).
        add("ZandroCompat.IsZandronum", stub_zero);
        add("System.IsZandronum", stub_zero);
        add("ZandroCompat.IsClient", stub_zero);
        add("ZandroCompat.PlayerInGame", player_in_game);
        add("ZandroCompat.GetPlayerTID", get_player_tid);
        add("ZandroCompat.SetPlayerTID", set_player_tid);
        add("math.random", math_random);
        add("math.random2", math_random2);
        for name in [
            "abs",
            "min",
            "max",
            "clamp",
            "sin",
            "cos",
            "tan",
            "atan2",
            "pow",
            "round",
            "ceil",
            "int",
            "float",
            "double",
            "frandom",
            "vectorangle",
            "sqrt",
            "floor",
            "exp",
            "log",
            "fmin",
            "fmax",
            "deltaangle",
            "absangle",
        ] {
            add(&format!("math.{name}"), math_op);
        }
        add("global.Ceil", math_op);
        add("global.Clamp", math_op);
        add("global.Floor", math_op);
        add("global.Abs", math_op);
        add("global.Min", math_op);
        add("global.Max", math_op);
        add("global.Round", math_op);
        add("global.Lerp", global_lerp);
        add("global.Splerp", global_splerp);
        add("global.Distance2D", global_distance2d);
        add("global.DistanceBySpeed", global_distance_by_speed);
        add("global.AngleTo", global_angle_to);
        add("global.VectorAngle", math_op);
        add("global.sin", math_op);
        add("global.cos", math_op);
        add("global.tan", math_op);
        add("global.int", math_op);
        add("global.deltaangle", global_delta_angle);
        add("global.absangle", global_abs_angle);
        add("global.GetFriction", global_friction);
        add("global.AccuracyFactor", global_one);
        add("global.G_SkillPropertyFloat", global_skill_float);
        add("global.GetCVar", cvar_get_cvar);
        add("global.S_StartSound", actor_start_sound);
        add("global.GetClassName", actor_get_class_name);
        add("object.IsNull", object_is_null);
        add("object.isValid", object_is_valid);
        add("object.CompareNoCase", object_compare_nocase);
        add("object.IsNotEmpty", object_not_empty);
        add("object.Length", string_length);
        add("object.Size", array_size);
        add("object.Count", array_size);
        add("object.AngleTo", global_angle_to);
        add("object.BreakLines", font_new);
        add("object.GetBool", cvar_get_int);
        add("object.GetClass", font_new);
        add("object.GetCursor", font_cursor);
        add("object.GetDisplacement", font_displacement);
        add("object.GetHeight", font_height);
        add("object.GetMaxAscender", font_ascender);
        add("object.GetString", cvar_get_string);
        add("object.GetWidth", font_char_width);
        add("object.IsValidDynamicFont", font_not_dynamic);
        add("object.StringWidth", font_string_width);
        add("object.g_skillpropertyfloat", global_skill_float);
        add("object.s_startsound", actor_start_sound);
        add("object.Visible", level_yes);
        add("object.Selectable", level_yes);
        add("object.VelFromAngle", object_vel_from_angle);
        add("global.VelFromAngle", object_vel_from_angle);
        add("object.Vel3DFromAngle", object_vel3d_from_angle);
        add("global.Vel3DFromAngle", object_vel3d_from_angle);
        add("object.PitchFromVel", object_pitch_from_vel);
        add("global.PitchFromVel", object_pitch_from_vel);
        add("object.bam", object_bam);
        add("object.GetPlayerColor", player_color);
        add("object.GetTextColor", object_text_color);
        add("object.GetLightLevel", object_light_get);
        add("object.SetLightLevel", object_light_set);
        add("object.Vec3Offset", object_vec3_offset);
        add("object.PlusZ", object_plus_z);
        add("object.GetX", object_get_x);
        add("object.SetX", object_set_x);
        add("object.GetY", object_get_y);
        add("object.SetY", object_set_y);
        add("object.GetValue", object_get_value);
        add("object.SetValue", object_set_value);
        add("object.StringAt", object_string_at);
        add("object.getdeltatime", object_delta_time);
        add("object.CursorSpace", object_cursor_space);
        add("object.PlayPickupSound", actor_start_sound);
        add("object.SetString", object_set_string);
        add("object.PickupMessage", object_pickup_message);
        add("object.Next", object_next_null);
        add("object.Push", object_push);
        add("object.Pop", object_pop);
        add("object.Clear", object_clear);
        add("object.SetZ", object_set_z);
        add("object.GetZ", object_get_z);
        add("object.SetOrigin", object_set_origin);
        add("object.CheckMissileSpawn", level_yes);
        add("object.CheckStopped", level_yes);
        add("object.CheckFocus", object_check_focus);
        add("object.SetFocus", object_set_focus);
        add("object.FindInventory", object_next_null);
        add("object.GetParentAmmo", object_next_null);
        add("object.GetLogo", font_new);
        add("object.Delete", object_delete);
        add("actor.CheckMissileSpawn", level_yes);
        add("actor.checklocalview", level_yes);
        add("actor.absangle", global_abs_angle);
        add("actor.angleto", global_angle_to);
        add("actor.angletovector", actor_angle_to_vector);
        add("actor.addz", actor_add_z);
        add("Actor.ChangeFlag", actor_change_flag);
        add("Actor.CheckFlag", actor_check_flag);
        add("actor.ApplyDamageFactor", actor_damage_factor);
        add("Actor.GetDefault", font_new);
        add("actor.deltaangle", global_delta_angle);
        add("actor.distance2d", global_distance2d);
        add("actor.distancebyspeed", global_distance_by_speed);
        add("actor.findinventory", object_next_null);
        add("actor.findstate", object_next_null);
        add("actor.countsaskill", level_yes);
        add("actor.checkposition", level_yes);
        add("actor.getdefaultspeed", actor_default_speed);
        add("actor.getspawnhealth", actor_spawn_health);
        add("actor.getmissiledamage", actor_missile_damage);
        add("actor.getmodifieddamage", actor_modified_damage);
        add("actor.getfloorterrain", actor_floor_terrain);
        for name in [
            "getpointer",
            "getreplacement",
            "getreplacee",
            "getdropitems",
            "getspawnabletype",
        ] {
            add(&format!("actor.{name}"), object_next_null);
        }
        add("actor.isvisible", level_yes);
        add("actor.hitfloor", level_yes);
        add("actor.getspecies", actor_species);
        add("actor.checkifseen", actor_check_if_seen);
        add("actor.checksightorrange", actor_check_sight_or_range);
        add("actor.checkrange", actor_check_range);
        add("actor.lookforplayers", actor_look);
        add("actor.die", actor_die);
        add("actor.explodemissile", actor_explode);
        add("actor.monstermove", actor_chase);
        add("actor.newchasedir", actor_chase);
        add("actor.railattack", actor_rail);
        add("actor.radiusattack", actor_radius);
        add("actor.normalize180", actor_normalize180);
        for name in [
            "isfrozen",
            "isfriend",
            "isteammate",
            "iszerodamage",
            "hitfriend",
            "hitwater",
        ] {
            add(&format!("actor.{name}"), actor_false);
        }
        add("actor.shouldpassthroughplayer", actor_false);
        add("actor.spawnhealth", actor_spawn_health);
        add("actor.setorigin", object_set_origin);
        add("actor.setz", object_set_z);
        add("actor.setxyz", object_set_origin);
        add("actor.soundalert", actor_look);
        for name in [
            "spawn",
            "spawnmissile",
            "spawnmissileanglezspeed",
            "spawnmissilexyz",
            "spawnmissilez",
            "spawnmissilezaimed",
            "spawnplayermissile",
            "spawnsubmissile",
            "spawnteleportfog",
            "resolvestate",
        ] {
            add(&format!("actor.{name}"), object_next_null);
        }
        add("actor.velfromangle", object_vel_from_angle);
        add("actor.vec2angle", object_vel_from_angle);
        add("actor.vel3dfromangle", object_vel3d_from_angle);
        add("actor.vec3angle", object_vel3d_from_angle);
        add("actor.vec3offset", object_vec3_offset);
        add("actor.vec2offsetz", object_vec3_offset);
        add("actor.vec3to", object_vec3_offset);
        add("actor.vec2offset", actor_vec2_offset);
        add("actor.testmobjlocation", level_yes);
        add("actor.stopsoundsequence", act_stop_sounds);
        for name in ["trymove", "teleport", "teleportmove", "setstate"] {
            add(&format!("actor.{name}"), actor_false);
        }
        for name in ["canseek", "checkblock", "checkbossdeath", "checkifseen"] {
            add(&format!("actor.{name}"), actor_false);
        }
        add("actor.bulletslope", actor_flat_aim);
        add("actor.applydamagefactors", actor_damage_factor);
        add("actor.Destroy", actor_die);
        for name in [
            "GetBasicArmorClass",
            "RoughMonsterSearch",
            "SpawnMissileAngle",
        ] {
            add(&format!("actor.{name}"), object_next_null);
        }
        add("actor.getboboffset", actor_flat_aim);
        add("actor.getboneindex", actor_missing_index);
        add("actor.gettag", actor_tag);
        add("actor.playerskincheck", level_yes);
        add("actor.rotatevector", actor_vec2_offset);
        add("actor.getbonebaseposition", actor_zero_vec);
        add("actor.posrelative", actor_zero_vec);
        for name in [
            "playernumber",
            "checknodelay",
            "givebody",
            "giveinventorytype",
            "givesecret",
        ] {
            add(&format!("actor.{name}"), actor_false);
        }
        for name in ["oldspawnmissile", "morphinto"] {
            add(&format!("actor.{name}"), object_next_null);
        }
        add("Actor.A_Look", actor_look);
        add("Actor.A_Chase", actor_chase);
        add("Actor.A_Face", actor_face);
        add("Actor.A_StartSound", actor_start_sound_full);
        add("global.A_StartSound", actor_start_sound_full);
        add("Actor.A_CheckForResurrection", act_check_for_resurrection);
        // MBF21 weapon psprite actions used by ID24 and DeHackEd-derived ZScript.
        // They use the same playsim action dispatcher as DECORATE actions.
        for name in [
            "A_CheckAmmo",
            "A_GunFlashTo",
            "A_RefireTo",
            "A_BulletAttack",
            "A_ChangeCountFlags",
            "A_ChangeLinkFlags",
            "A_KillTarget",
            "A_KillTracer",
            "A_SetChaseThreshold",
            "A_ActiveAndUnblock",
            "A_ActiveSound",
            "A_ClearOverlays",
            "A_ClearSoundTarget",
            "A_Overlay",
            "A_OverlayAlpha",
            "A_OverlayFlags",
            "A_OverlayOffset",
            "A_OverlayPivot",
            "A_OverlayPivotAlign",
            "A_OverlayRenderStyle",
            "A_OverlayRotate",
            "A_OverlayScale",
            "A_OverlayTranslation",
            "A_SpriteOffset",
            "A_WeaponOffset",
            "A_StopSounds",
            "A_SetViewAngle",
            "A_SetViewPitch",
            "A_SetViewPos",
            "A_SetViewRoll",
            "A_HideThing",
            "A_LowGravity",
            "A_ResetHealth",
            "A_RemoveTarget",
            "A_RemoveTracer",
            "A_ScreamAndUnblock",
            "A_SetFloat",
            "A_UnsetFloat",
            "A_SetReflective",
            "A_UnSetReflective",
            "A_SetReflectiveInvulnerable",
            "A_UnSetReflectiveInvulnerable",
            "A_SetShootable",
            "A_UnSetShootable",
            "A_SetSolid",
            "A_UnsetSolid",
            "A_UnHideThing",
            "A_WeaponAlert",
        ] {
            add(&format!("Actor.{name}"), act_by_name);
            add(&format!("global.{name}"), act_by_name);
        }
        add("Actor.AimLineAttack", actor_aim_line);
        add("Actor.LineAttack", actor_line_attack);
        add("Actor.A_Pain", actor_pain);
        add("Actor.A_NoBlocking", actor_no_blocking);
        // Statement calls whose declared return is not an int lower as `global.*`.
        add("global.AimLineAttack", actor_aim_line);
        add("global.LineAttack", actor_line_attack);
        add("global.A_Pain", actor_pain);
        add("global.A_NoBlocking", actor_no_blocking);
        // Playsim actions the stdlib calls as natives. Both prefixes: a
        // resolved method is `Actor.*`, an unresolved statement is `global.*`.
        add("Actor.A_SetHealth", act_set_health);
        add("global.A_SetHealth", act_set_health);
        add("Actor.A_SetSpeed", act_set_speed);
        add("global.A_SetSpeed", act_set_speed);
        add("Actor.A_SetTics", act_set_tics);
        add("global.A_SetTics", act_set_tics);
        add("Actor.A_SetAngle", act_set_angle);
        add("global.A_SetAngle", act_set_angle);
        add("Actor.A_SetPitch", act_set_pitch);
        add("global.A_SetPitch", act_set_pitch);
        add("Actor.A_SetRoll", act_set_roll);
        add("global.A_SetRoll", act_set_roll);
        add("Actor.A_Weave", act_weave);
        add("global.A_Weave", act_weave);
        add("Actor.A_SetTranslucent", act_set_translucent);
        add("global.A_SetTranslucent", act_set_translucent);
        add("Actor.A_Recoil", act_recoil);
        add("global.A_Recoil", act_recoil);
        add("Actor.A_FastChase", act_fast_chase);
        add("global.A_FastChase", act_fast_chase);
        add("Actor.A_FadeOut", act_fade_out);
        add("global.A_FadeOut", act_fade_out);
        add("Actor.A_FadeIn", act_fade_in);
        add("global.A_FadeIn", act_fade_in);
        add("Actor.A_FadeTo", act_fade_to);
        add("global.A_FadeTo", act_fade_to);
        add("Actor.A_StopSound", act_stop_sound);
        add("global.A_StopSound", act_stop_sound);
        add("Actor.A_DamageSelf", act_damage_self);
        add("global.A_DamageSelf", act_damage_self);
        add("Actor.A_DamageTarget", act_damage_target);
        add("global.A_DamageTarget", act_damage_target);
        add("Actor.A_RadiusDamageSelf", act_radius_damage_self);
        add("global.A_RadiusDamageSelf", act_radius_damage_self);
        add("Actor.A_Burst", actor_burst);
        add("global.A_Burst", actor_burst);
        add("Actor.A_Respawn", act_respawn);
        add("global.A_Respawn", act_respawn);
        add("Actor.A_VileChase", act_vile_chase);
        add("global.A_VileChase", act_vile_chase);
        add("Actor.A_BossDeath", act_boss_death);
        add("global.A_BossDeath", act_boss_death);
        add("Actor.A_Wander", act_wander);
        add("global.A_Wander", act_wander);
        add("Actor.A_PlayerScream", act_player_scream);
        add("global.A_PlayerScream", act_player_scream);
        add("Actor.A_QueueCorpse", act_queue_corpse);
        add("global.A_QueueCorpse", act_queue_corpse);
        add("Actor.A_ClearLastHeard", act_clear_last_heard);
        add("global.A_ClearLastHeard", act_clear_last_heard);
        // Remaining stdlib `A_*` imports that already have a playsim arm.
        // `act_by_name` reads the registered spelling from `native_name`.
        for name in [
            "A_SetSize",
            "A_SetRenderStyle",
            "A_Print",
            "A_Log",
            "A_SetTranslation",
            "A_ExtChase",
            "A_DropItem",
            "A_SpawnProjectile",
            "A_FireProjectile",
            "A_SeekerMissile",
        ] {
            add(&format!("Actor.{name}"), act_by_name);
            add(&format!("global.{name}"), act_by_name);
        }
        add("object.A_StartSound", actor_start_sound_full);
        add("Actor.A_StartSoundIfNotSame", actor_start_sound_if_not_same);
        add(
            "global.A_StartSoundIfNotSame",
            actor_start_sound_if_not_same,
        );
        add("Actor.A_StopSounds", act_stop_sounds);
        add("global.A_StopSounds", act_stop_sounds);
        add("Actor.A_MonsterRail", act_monster_rail);
        add("global.A_MonsterRail", act_monster_rail);
        add("Actor.A_SpawnParticleEx", actor_spawn_particle_ex);
        add("TexMan.CheckForTexture", texman_check_for_texture);
        add("TexMan.GetName", texman_get_name);
        add("TextureID.IsValid", texture_id_is_valid);
        add("TextureID.Exists", texture_id_is_valid);
        add("TextureID.IsNull", texture_id_is_null);
        add("TextureID.SetInvalid", texture_id_set_invalid);
        add("TextureID.SetNull", texture_id_set_null);
        ids
    }

    fn act_stop_sounds(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        queue_action(ctx, regs, "A_StopSound");
        Ok(())
    }

    fn act_monster_rail(ctx: &mut ZvmCtx<'_>, _regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        queue_action(ctx, _regs, "A_MonsterRail");
        Ok(())
    }

    /// `TexMan.CheckForTexture(name, usetype, flags)`: intern `name` as a
    /// `TextureID` (`"-"` = null, `""` = invalid). Lump existence is
    /// checked by the renderer when the texture is drawn.
    fn texman_check_for_texture(
        ctx: &mut ZvmCtx<'_>,
        regs: &mut NativeRegs<'_>,
    ) -> Result<(), Trap> {
        let name = ctx
            .heap
            .str(regs.s.first().copied().unwrap_or_default())
            .trim()
            .to_owned();
        let id = ctx.play.intern_texture(&name);
        if let Some(slot) = regs.i.first_mut() {
            *slot = id;
        }
        Ok(())
    }

    /// `TexMan.GetName(tex)`.
    fn texman_get_name(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let id = regs.i.first().copied().unwrap_or(0);
        let name = ctx.play.texture_name(id).unwrap_or("").to_owned();
        store_string(ctx, regs, &name);
        Ok(())
    }

    fn texture_id_is_valid(_ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let id = regs.i.first().copied().unwrap_or(0);
        if let Some(slot) = regs.i.first_mut() {
            *slot = i64::from(id > 0);
        }
        Ok(())
    }

    fn texture_id_is_null(_ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let id = regs.i.first().copied().unwrap_or(0);
        if let Some(slot) = regs.i.first_mut() {
            *slot = i64::from(id == 0);
        }
        Ok(())
    }

    fn texture_id_set_invalid(_ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        if let Some(slot) = regs.i.first_mut() {
            *slot = -1;
        }
        Ok(())
    }

    fn texture_id_set_null(_ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        if let Some(slot) = regs.i.first_mut() {
            *slot = 0;
        }
        Ok(())
    }

    fn actor_spawn_particle_ex(
        ctx: &mut ZvmCtx<'_>,
        regs: &mut NativeRegs<'_>,
    ) -> Result<(), Trap> {
        let integer = |index: usize| regs.i.get(index).copied().unwrap_or(0);
        let float = |index: usize, default: f64| regs.f.get(index).copied().unwrap_or(default);
        let flags = integer(3) as u32;
        let texture = ctx.play.texture_name(integer(1)).map(str::to_owned);
        ctx.play.actor_ops.push(super::ZsActorOp::ParticleEx {
            color: integer(0) as u32,
            texture,
            render_style: integer(2) as i32,
            flags,
            lifetime: integer(4) as i32,
            size: float(0, 1.0),
            angle: float(1, 0.0),
            offset: [float(2, 0.0), float(3, 0.0), float(4, 0.0)],
            velocity: [float(5, 0.0), float(6, 0.0), float(7, 0.0)],
            acceleration: [float(8, 0.0), float(9, 0.0), float(10, 0.0)],
            alpha: float(11, 1.0),
            fade_step: float(12, -1.0),
            size_step: float(13, 0.0),
            roll: if flags & (1 << 6) != 0 {
                float(14, 0.0)
            } else {
                0.0
            },
            roll_velocity: if flags & (1 << 6) != 0 {
                float(15, 0.0)
            } else {
                0.0
            },
            roll_acceleration: if flags & (1 << 6) != 0 {
                float(16, 0.0)
            } else {
                0.0
            },
            fade_out_step: float(17, 0.0),
        });
        Ok(())
    }

    fn actor_burst(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let string_class = regs
            .s
            .first()
            .map(|class| ctx.heap.str(*class).to_owned())
            .filter(|class| !class.is_empty());
        let class_name = string_class.unwrap_or_else(|| {
            usize::try_from(regs.i.first().copied().unwrap_or(-1))
                .ok()
                .and_then(|id| ctx.play.class_names.get(id))
                .cloned()
                .unwrap_or_default()
        });
        ctx.play.actor_ops.push(super::ZsActorOp::Burst {
            chunk_class: class_name,
        });
        Ok(())
    }

    fn queue_action(ctx: &mut ZvmCtx<'_>, regs: &NativeRegs<'_>, name: impl Into<String>) {
        let mut args = [0i64; 4];
        for (i, slot) in args.iter_mut().enumerate() {
            let iv = regs.i.get(i).copied().unwrap_or(0);
            let fv = regs.f.get(i).copied().unwrap_or(0.0);
            *slot = if iv != 0 { iv } else { fv as i64 };
        }
        ctx.play.actor_ops.push(super::ZsActorOp::Action {
            name: name.into(),
            args,
        });
    }

    /// Dispatch from the registered name (`Actor.A_SetSize` → `A_SetSize`).
    fn act_by_name(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let full = ctx.play.native_name.clone();
        let name = full.rsplit('.').next().unwrap_or(&full).to_owned();
        if !name.is_empty() {
            queue_action(ctx, regs, name);
        }
        Ok(())
    }

    fn act_set_health(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        queue_action(ctx, regs, "A_SetHealth");
        Ok(())
    }
    fn act_set_speed(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        queue_action(ctx, regs, "A_SetSpeed");
        Ok(())
    }
    fn act_set_tics(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        queue_action(ctx, regs, "A_SetTics");
        Ok(())
    }
    fn act_set_angle(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        queue_action(ctx, regs, "A_SetAngle");
        Ok(())
    }
    fn act_set_pitch(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        queue_action(ctx, regs, "A_SetPitch");
        Ok(())
    }
    fn act_set_roll(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        queue_action(ctx, regs, "A_SetRoll");
        Ok(())
    }
    fn act_weave(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        queue_action(ctx, regs, "A_Weave");
        Ok(())
    }
    fn act_set_translucent(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        queue_action(ctx, regs, "A_SetTranslucent");
        Ok(())
    }
    fn act_recoil(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        queue_action(ctx, regs, "A_Recoil");
        Ok(())
    }
    fn act_fast_chase(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        queue_action(ctx, regs, "A_FastChase");
        Ok(())
    }
    fn act_fade_out(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        queue_action(ctx, regs, "A_FadeOut");
        Ok(())
    }
    fn act_fade_in(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        queue_action(ctx, regs, "A_FadeIn");
        Ok(())
    }
    fn act_fade_to(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        queue_action(ctx, regs, "A_FadeTo");
        Ok(())
    }
    fn act_stop_sound(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        queue_action(ctx, regs, "A_StopSound");
        Ok(())
    }
    fn act_damage_self(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        queue_action(ctx, regs, "A_DamageSelf");
        Ok(())
    }
    fn act_damage_target(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        queue_action(ctx, regs, "A_DamageTarget");
        Ok(())
    }
    fn act_radius_damage_self(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        ctx.play.actor_ops.push(super::ZsActorOp::RadiusDamageSelf {
            damage: regs.i.first().copied().unwrap_or(128) as i32,
            distance: regs.f.first().copied().unwrap_or(128.0),
            flags: regs.i.get(1).copied().unwrap_or(0) as i32,
            flash_class: regs
                .s
                .first()
                .map(|handle| ctx.heap.str(*handle).to_owned())
                .filter(|name| !name.is_empty()),
        });
        Ok(())
    }
    fn act_respawn(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        queue_action(ctx, regs, "A_Respawn");
        Ok(())
    }
    fn act_vile_chase(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        queue_action(ctx, regs, "A_VileChase");
        Ok(())
    }
    fn act_check_for_resurrection(
        ctx: &mut ZvmCtx<'_>,
        regs: &mut NativeRegs<'_>,
    ) -> Result<(), Trap> {
        // Actor natives receive the VM receiver in A[0]; a non-null state
        // argument, when present, follows it. Bare/global calls have no
        // receiver and therefore use A[0] for their first argument.
        let receiver_present = regs.a.first().is_some_and(|value| !value.is_null());
        let custom_state = regs
            .a
            .get(usize::from(receiver_present))
            .is_some_and(|value| !value.is_null());
        let custom_sound = regs.i.first().copied().unwrap_or(0) != 0;
        if custom_state || custom_sound {
            ctx.play.resurrection_candidate = None;
            if let Some(result) = regs.i.first_mut() {
                *result = 0;
            }
            return Ok(());
        }
        let Some((actor_index, generation)) = ctx.play.resurrection_candidate.take() else {
            if let Some(result) = regs.i.first_mut() {
                *result = 0;
            }
            return Ok(());
        };
        ctx.play
            .actor_ops
            .push(super::ZsActorOp::ResurrectCandidate {
                actor_index,
                generation,
            });
        if let Some(result) = regs.i.first_mut() {
            *result = 1;
        }
        Ok(())
    }
    fn act_boss_death(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        queue_action(ctx, regs, "A_BossDeath");
        Ok(())
    }
    fn act_wander(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        queue_action(ctx, regs, "A_Wander");
        Ok(())
    }
    fn act_player_scream(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        queue_action(ctx, regs, "A_PlayerScream");
        Ok(())
    }
    fn act_queue_corpse(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        queue_action(ctx, regs, "A_QueueCorpse");
        Ok(())
    }
    fn act_clear_last_heard(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        queue_action(ctx, regs, "A_ClearLastHeard");
        Ok(())
    }

    fn math_random(_ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        // Deterministic stand-in: the upper bound, so `random(1, 3)` is 3.
        let hi = regs.f.get(1).copied().filter(|v| *v != 0.0).unwrap_or(1.0);
        if let Some(slot) = regs.f.first_mut() {
            *slot = hi;
        }
        Ok(())
    }

    fn math_random2(_ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        if let Some(slot) = regs.f.first_mut() {
            *slot = 0.0;
        }
        Ok(())
    }

    fn wrap_180(d: f64) -> f64 {
        let r = d - 360.0 * room_core::dmath::floor((d + 180.0) / 360.0);
        if r >= 180.0 { r - 360.0 } else { r }
    }

    fn math_op(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let name = ctx
            .play
            .native_name
            .rsplit('.')
            .next()
            .unwrap_or("")
            .to_ascii_lowercase();
        let a = regs.f.first().copied().unwrap_or(0.0);
        let b = regs.f.get(1).copied().unwrap_or(0.0);
        let c = regs.f.get(2).copied().unwrap_or(0.0);
        let v = match name.as_str() {
            "abs" => abs_f(a),
            "min" => a.min(b),
            "max" => a.max(b),
            "clamp" => {
                let (lo, hi) = if b <= c { (b, c) } else { (c, b) };
                a.max(lo).min(hi)
            }
            "sin" => room_core::dmath::sin(deg_to_rad(a)),
            "cos" => room_core::dmath::cos(deg_to_rad(a)),
            "tan" => room_core::dmath::tan(deg_to_rad(a)),
            "atan2" => rad_to_deg(room_core::dmath::atan2(a, b)),
            "vectorangle" => rad_to_deg(room_core::dmath::atan2(b, a)),
            "pow" => room_core::dmath::pow(a, b),
            "sqrt" => room_core::dmath::sqrt(a),
            "exp" => room_core::dmath::exp(a),
            "log" => room_core::dmath::log(a),
            "fmin" => a.min(b),
            "fmax" => a.max(b),
            // UZDoom `deltaangle(a1, a2)`: a2 - a1 wrapped to [-180, 180).
            "deltaangle" => wrap_180(b - a),
            "absangle" => abs_f(wrap_180(b - a)),
            "floor" => room_core::dmath::floor(a),
            "round" => round_half_away(a),
            "ceil" => ceil_f(a),
            "int" => trunc_f(a),
            "frandom" => {
                if b != 0.0 {
                    b
                } else {
                    a
                }
            }
            _ => a,
        };
        if let Some(slot) = regs.f.first_mut() {
            *slot = v;
        }
        Ok(())
    }

    fn object_is_null(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let h = regs.a.first().copied().unwrap_or(ObjHandle::NULL);
        let missing = h.is_null() || ctx.heap.get(h).is_none();
        write_num(regs, if missing { 1.0 } else { 0.0 });
        Ok(())
    }

    fn object_is_valid(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let h = regs.a.first().copied().unwrap_or(ObjHandle::NULL);
        let live = !h.is_null() && ctx.heap.get(h).is_some();
        write_num(regs, if live { 1.0 } else { 0.0 });
        Ok(())
    }

    fn object_not_empty(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let text = ctx.heap.str(regs.s.first().copied().unwrap_or_default());
        write_num(regs, if text.is_empty() { 0.0 } else { 1.0 });
        Ok(())
    }

    fn object_compare_nocase(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let left = ctx
            .heap
            .str(regs.s.first().copied().unwrap_or_default())
            .to_lowercase();
        let right = ctx
            .heap
            .str(regs.s.get(1).copied().unwrap_or_default())
            .to_lowercase();
        let v = match left.cmp(&right) {
            std::cmp::Ordering::Less => -1,
            std::cmp::Ordering::Equal => 0,
            std::cmp::Ordering::Greater => 1,
        };
        write_num(regs, v as f64);
        Ok(())
    }

    fn finite_speed(v: f64, fallback: f64) -> f64 {
        if v.abs() > 1.0e30 { fallback } else { v }
    }

    fn actor_active_sound(ctx: &mut ZvmCtx<'_>, _regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        ctx.play.actor_ops.push(super::ZsActorOp::ActiveSound);
        Ok(())
    }

    fn actor_set_friend_player(
        ctx: &mut ZvmCtx<'_>,
        regs: &mut NativeRegs<'_>,
    ) -> Result<(), Trap> {
        let player = regs.a.get(1).copied().unwrap_or(ObjHandle::NULL);
        if player.is_null() {
            return Ok(());
        }
        ctx.play.actor_ops.push(super::ZsActorOp::Action {
            name: "A_SetFriendly".into(),
            args: [1, 0, 0, 0],
        });
        Ok(())
    }

    fn actor_link_world(ctx: &mut ZvmCtx<'_>, _regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        ctx.play
            .actor_ops
            .push(super::ZsActorOp::WorldLink { linked: true });
        Ok(())
    }

    fn actor_copy_friendliness(
        ctx: &mut ZvmCtx<'_>,
        regs: &mut NativeRegs<'_>,
    ) -> Result<(), Trap> {
        let other = regs.a.get(1).copied().unwrap_or(ObjHandle::NULL);
        if other.is_null() {
            return Ok(());
        }
        ctx.play.actor_ops.push(super::ZsActorOp::CopyFriend {
            change_target: regs.i.first().copied().unwrap_or(0) != 0,
            reset_health: regs.i.get(1).copied().unwrap_or(0) != 0,
        });
        Ok(())
    }

    fn actor_set_view_pos(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let z = regs.f.get(2).copied().unwrap_or(0.0);
        ctx.play.actor_ops.push(super::ZsActorOp::ViewZ { z });
        Ok(())
    }

    fn object_set_render_style(
        ctx: &mut ZvmCtx<'_>,
        regs: &mut NativeRegs<'_>,
    ) -> Result<(), Trap> {
        let mode = regs.i.first().copied().unwrap_or(0);
        if (0..=11).contains(&mode) {
            ctx.play
                .actor_ops
                .push(super::ZsActorOp::RenderMode { mode: mode as i32 });
        }
        Ok(())
    }

    fn actor_set_shade(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let col = regs.i.first().copied().unwrap_or(0);
        let rgb = if col == 0 {
            None
        } else {
            let col = col as u32;
            Some([
                ((col >> 16) & 0xff) as u8,
                ((col >> 8) & 0xff) as u8,
                (col & 0xff) as u8,
            ])
        };
        ctx.play.actor_ops.push(super::ZsActorOp::Shade { rgb });
        Ok(())
    }

    fn actor_local_hide(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let hide = regs.i.get(1).copied().unwrap_or(0) != 0;
        ctx.play
            .actor_ops
            .push(super::ZsActorOp::LocalHide { hide });
        Ok(())
    }

    fn actor_draw_splash(ctx: &mut ZvmCtx<'_>, _regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        ctx.play.actor_ops.push(super::ZsActorOp::Splash);
        Ok(())
    }

    fn actor_trace_bleed(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let dmg = regs.i.first().copied().unwrap_or(0);
        if dmg <= 0 {
            return Ok(());
        }
        ctx.play
            .actor_ops
            .push(super::ZsActorOp::Bleed { damage: dmg as i32 });
        Ok(())
    }

    fn actor_damage_mobj(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let dmg = regs.i.first().copied().unwrap_or(0);
        write_num(regs, dmg.max(0) as f64);
        if dmg > 0 {
            ctx.play.actor_ops.push(super::ZsActorOp::Action {
                name: "A_DamageSelf".into(),
                args: [dmg, 0, 0, 0],
            });
        }
        Ok(())
    }

    fn actor_clear_counters(ctx: &mut ZvmCtx<'_>, _regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        ctx.play.actor_ops.push(super::ZsActorOp::ClearCounters);
        Ok(())
    }

    fn actor_clear_tid(ctx: &mut ZvmCtx<'_>, _regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        ctx.play.actor_ops.push(super::ZsActorOp::ClearTid);
        Ok(())
    }

    fn actor_restore_damage(ctx: &mut ZvmCtx<'_>, _regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        ctx.play.actor_ops.push(super::ZsActorOp::RestoreDamage);
        Ok(())
    }

    fn actor_fit_sector(ctx: &mut ZvmCtx<'_>, _regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        ctx.play.actor_ops.push(super::ZsActorOp::FitSector);
        Ok(())
    }

    fn actor_unlink_world(ctx: &mut ZvmCtx<'_>, _regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        ctx.play
            .actor_ops
            .push(super::ZsActorOp::WorldLink { linked: false });
        Ok(())
    }

    fn actor_clear_bounce(ctx: &mut ZvmCtx<'_>, _regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        ctx.play.actor_ops.push(super::ZsActorOp::ClearBounce);
        Ok(())
    }

    fn actor_howl(ctx: &mut ZvmCtx<'_>, _regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        ctx.play.actor_ops.push(super::ZsActorOp::Howl);
        Ok(())
    }

    fn actor_spawn_sound(ctx: &mut ZvmCtx<'_>, _regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        ctx.play.actor_ops.push(super::ZsActorOp::SpawnSound);
        Ok(())
    }

    fn actor_set_idle(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let nofunction = regs.i.first().copied().unwrap_or(0) != 0;
        ctx.play.actor_ops.push(super::ZsActorOp::Idle {
            call_action: !nofunction,
        });
        Ok(())
    }

    fn actor_set_damage(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        queue_action(ctx, regs, "A_SetDamage");
        Ok(())
    }

    fn actor_thrust(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let speed = regs.f.first().copied().unwrap_or(0.0);
        let angle = regs.f.get(1).copied().unwrap_or(0.0);
        if speed.abs() > 1.0e30 || angle.abs() > 1.0e30 {
            return Ok(());
        }
        let rad = deg_to_rad(angle);
        ctx.play.actor_ops.push(super::ZsActorOp::Thrust {
            vx: speed * room_core::dmath::cos(rad),
            vy: speed * room_core::dmath::sin(rad),
        });
        Ok(())
    }

    fn object_vel_from_angle(_ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let speed = finite_speed(regs.f.first().copied().unwrap_or(1.0), 1.0);
        let angle = finite_speed(regs.f.get(1).copied().unwrap_or(0.0), 0.0);
        let rad = deg_to_rad(angle);
        if let Some(slot) = regs.f.first_mut() {
            *slot = speed * room_core::dmath::cos(rad);
        }
        if let Some(slot) = regs.f.get_mut(1) {
            *slot = speed * room_core::dmath::sin(rad);
        }
        Ok(())
    }

    fn object_vel3d_from_angle(
        _ctx: &mut ZvmCtx<'_>,
        regs: &mut NativeRegs<'_>,
    ) -> Result<(), Trap> {
        let speed = finite_speed(regs.f.first().copied().unwrap_or(1.0), 1.0);
        let angle = finite_speed(regs.f.get(1).copied().unwrap_or(0.0), 0.0);
        let pitch = finite_speed(regs.f.get(2).copied().unwrap_or(0.0), 0.0);
        let horiz = speed * room_core::dmath::cos(deg_to_rad(pitch));
        let rad = deg_to_rad(angle);
        if let Some(slot) = regs.f.first_mut() {
            *slot = horiz * room_core::dmath::cos(rad);
        }
        if let Some(slot) = regs.f.get_mut(1) {
            *slot = horiz * room_core::dmath::sin(rad);
        }
        if let Some(slot) = regs.f.get_mut(2) {
            *slot = speed * room_core::dmath::sin(deg_to_rad(pitch));
        }
        Ok(())
    }

    fn object_pitch_from_vel(_ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let vx = regs.f.first().copied().unwrap_or(0.0);
        let vy = regs.f.get(1).copied().unwrap_or(0.0);
        let vz = regs.f.get(2).copied().unwrap_or(0.0);
        let horiz = room_core::dmath::sqrt(vx * vx + vy * vy);
        if let Some(slot) = regs.f.first_mut() {
            *slot = rad_to_deg(room_core::dmath::atan2(vz, horiz));
        }
        Ok(())
    }

    fn object_text_color(_ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        write_num(regs, 11.0);
        Ok(())
    }

    fn object_light_get(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let h = regs.a.first().copied().unwrap_or(ObjHandle::NULL);
        let v = match ctx.heap.get(h) {
            Some(ObjKind::Object { fields_i, .. }) if fields_i.get(1).copied() == Some(1) => {
                fields_i.first().copied().unwrap_or(255)
            }
            _ => 255,
        };
        write_num(regs, v as f64);
        Ok(())
    }

    fn object_light_set(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let h = regs.a.first().copied().unwrap_or(ObjHandle::NULL);
        let v = regs.i.first().copied().unwrap_or(0);
        if let Some(ObjKind::Object { fields_i, .. }) = ctx.heap.get_mut(h) {
            if let Some(slot) = fields_i.first_mut() {
                *slot = v;
            }
            if let Some(flag) = fields_i.get_mut(1) {
                *flag = 1;
            }
        }
        Ok(())
    }

    fn object_vec3_offset(_ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let x = regs.f.first().copied().unwrap_or(0.0);
        let y = regs.f.get(1).copied().unwrap_or(0.0);
        let z = regs.f.get(2).copied().unwrap_or(0.0);
        if let Some(slot) = regs.f.first_mut() {
            *slot = x;
        }
        if let Some(slot) = regs.f.get_mut(1) {
            *slot = y;
        }
        if let Some(slot) = regs.f.get_mut(2) {
            *slot = z;
        }
        Ok(())
    }

    fn object_set_flagged(
        ctx: &mut ZvmCtx<'_>,
        h: ObjHandle,
        value_slot: usize,
        flag_slot: usize,
        value: i64,
    ) {
        if let Some(ObjKind::Object { fields_i, .. }) = ctx.heap.get_mut(h) {
            if let Some(slot) = fields_i.get_mut(value_slot) {
                *slot = value;
            }
            if let Some(slot) = fields_i.get_mut(flag_slot) {
                *slot = 1;
            }
        }
    }

    fn object_get_x(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let h = regs.a.first().copied().unwrap_or(ObjHandle::NULL);
        let v = match ctx.heap.get(h) {
            Some(ObjKind::Object { fields_i, .. }) if fields_i.get(1).copied() == Some(1) => {
                f64::from_bits(fields_i.first().copied().unwrap_or(0) as u64)
            }
            _ => 0.0,
        };
        if let Some(slot) = regs.f.first_mut() {
            *slot = v;
        }
        Ok(())
    }

    fn object_set_x(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let h = regs.a.first().copied().unwrap_or(ObjHandle::NULL);
        let bits = regs.f.first().copied().unwrap_or(0.0).to_bits() as i64;
        object_set_flagged(ctx, h, 0, 1, bits);
        Ok(())
    }

    fn object_get_y(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let h = regs.a.first().copied().unwrap_or(ObjHandle::NULL);
        let v = match ctx.heap.get(h) {
            Some(ObjKind::Object { fields_i, .. }) if fields_i.get(3).copied() == Some(1) => {
                f64::from_bits(fields_i.get(2).copied().unwrap_or(0) as u64)
            }
            _ => 0.0,
        };
        if let Some(slot) = regs.f.first_mut() {
            *slot = v;
        }
        Ok(())
    }

    fn object_set_y(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let h = regs.a.first().copied().unwrap_or(ObjHandle::NULL);
        let bits = regs.f.first().copied().unwrap_or(0.0).to_bits() as i64;
        object_set_flagged(ctx, h, 2, 3, bits);
        Ok(())
    }

    fn object_get_value(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let h = regs.a.first().copied().unwrap_or(ObjHandle::NULL);
        // Flag 2 marks an integer menu value. Flag 1 is a stored X coordinate.
        let v = match ctx.heap.get(h) {
            Some(ObjKind::Object { fields_i, .. }) if fields_i.get(1).copied() == Some(2) => {
                fields_i.first().copied().unwrap_or(0)
            }
            _ => 0,
        };
        write_num(regs, v as f64);
        Ok(())
    }

    fn object_set_value(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let h = regs.a.first().copied().unwrap_or(ObjHandle::NULL);
        let v = regs.i.first().copied().unwrap_or(0);
        if let Some(ObjKind::Object { fields_i, .. }) = ctx.heap.get_mut(h) {
            if let Some(slot) = fields_i.first_mut() {
                *slot = v;
            }
            if let Some(slot) = fields_i.get_mut(1) {
                *slot = 2;
            }
        }
        Ok(())
    }

    fn object_string_at(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let text = ctx
            .heap
            .str(regs.s.first().copied().unwrap_or_default())
            .to_owned();
        let idx = regs.i.first().copied().unwrap_or(0).max(0) as usize;
        let ch = text
            .chars()
            .nth(idx)
            .map(|c| c.to_string())
            .unwrap_or_default();
        store_string(ctx, regs, &ch);
        Ok(())
    }

    fn object_delta_time(_ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        if let Some(slot) = regs.f.first_mut() {
            *slot = 1.0 / 35.0;
        }
        Ok(())
    }

    fn object_cursor_space(_ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        write_num(regs, 8.0);
        Ok(())
    }

    fn object_set_string(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let h = regs.a.first().copied().unwrap_or(ObjHandle::NULL);
        let text = regs.s.first().copied().unwrap_or_default();
        if let Some(ObjKind::Object {
            fields_i, fields_s, ..
        }) = ctx.heap.get_mut(h)
        {
            if let Some(slot) = fields_s.first_mut() {
                *slot = text;
            }
            if let Some(slot) = fields_i.get_mut(2) {
                *slot = 1;
            }
        }
        Ok(())
    }

    fn object_pickup_message(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let h = regs.a.first().copied().unwrap_or(ObjHandle::NULL);
        let text = object_user_string(ctx, h).unwrap_or_default();
        store_string(ctx, regs, &text);
        Ok(())
    }

    fn object_next_null(_ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        if let Some(slot) = regs.a.first_mut() {
            *slot = ObjHandle::NULL;
        }
        Ok(())
    }

    fn object_push(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let h = regs.a.first().copied().unwrap_or(ObjHandle::NULL);
        let v = regs.i.first().copied().unwrap_or(0);
        if let Some(ObjKind::ArrayI(items)) = ctx.heap.get_mut(h) {
            items.push(v);
        }
        Ok(())
    }

    fn object_pop(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let h = regs.a.first().copied().unwrap_or(ObjHandle::NULL);
        let v = match ctx.heap.get_mut(h) {
            Some(ObjKind::ArrayI(items)) => items.pop().unwrap_or(0),
            _ => 0,
        };
        write_num(regs, v as f64);
        Ok(())
    }

    fn object_clear(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let h = regs.a.first().copied().unwrap_or(ObjHandle::NULL);
        match ctx.heap.get_mut(h) {
            Some(ObjKind::ArrayI(items)) => items.clear(),
            Some(ObjKind::ArrayS(items)) => items.clear(),
            _ => {}
        }
        Ok(())
    }

    fn object_get_z(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let h = regs.a.first().copied().unwrap_or(ObjHandle::NULL);
        let v = match ctx.heap.get(h) {
            Some(ObjKind::Object { fields_i, .. }) if fields_i.get(5).copied() == Some(1) => {
                f64::from_bits(fields_i.get(4).copied().unwrap_or(0) as u64)
            }
            _ => 0.0,
        };
        if let Some(slot) = regs.f.first_mut() {
            *slot = v;
        }
        Ok(())
    }

    fn object_set_z(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let h = regs.a.first().copied().unwrap_or(ObjHandle::NULL);
        let bits = regs.f.first().copied().unwrap_or(0.0).to_bits() as i64;
        object_set_flagged(ctx, h, 4, 5, bits);
        Ok(())
    }

    fn object_check_focus(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let h = regs.a.first().copied().unwrap_or(ObjHandle::NULL);
        let on = match ctx.heap.get(h) {
            Some(ObjKind::Object { fields_i, .. }) => fields_i.get(3).copied() == Some(4),
            _ => false,
        };
        write_num(regs, if on { 1.0 } else { 0.0 });
        Ok(())
    }

    fn object_release_focus(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let h = regs.a.first().copied().unwrap_or(ObjHandle::NULL);
        if let Some(ObjKind::Object { fields_i, .. }) = ctx.heap.get_mut(h) {
            if fields_i.get(3).copied() == Some(4) {
                if let Some(slot) = fields_i.get_mut(3) {
                    *slot = 0;
                }
            }
        }
        Ok(())
    }

    fn object_set_focus(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let h = regs.a.first().copied().unwrap_or(ObjHandle::NULL);
        if let Some(ObjKind::Object { fields_i, .. }) = ctx.heap.get_mut(h) {
            if let Some(slot) = fields_i.get_mut(3) {
                *slot = 4;
            }
        }
        Ok(())
    }

    fn actor_angle_to_vector(_ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let angle = finite_speed(regs.f.first().copied().unwrap_or(0.0), 0.0);
        let length = finite_speed(regs.f.get(1).copied().unwrap_or(1.0), 1.0);
        let rad = deg_to_rad(angle);
        if let Some(slot) = regs.f.first_mut() {
            *slot = length * room_core::dmath::cos(rad);
        }
        if let Some(slot) = regs.f.get_mut(1) {
            *slot = length * room_core::dmath::sin(rad);
        }
        Ok(())
    }

    fn actor_add_z(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let h = regs.a.first().copied().unwrap_or(ObjHandle::NULL);
        let dz = regs.f.first().copied().unwrap_or(0.0);
        let cur = match ctx.heap.get(h) {
            Some(ObjKind::Object { fields_i, .. }) if fields_i.get(5).copied() == Some(1) => {
                f64::from_bits(fields_i.get(4).copied().unwrap_or(0) as u64)
            }
            _ => 0.0,
        };
        object_set_flagged(ctx, h, 4, 5, (cur + dz).to_bits() as i64);
        Ok(())
    }

    fn actor_change_flag(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let h = regs.a.first().copied().unwrap_or(ObjHandle::NULL);
        let name = regs.s.first().copied().unwrap_or_default();
        let value = regs.i.first().copied().unwrap_or(0);
        if let Some(ObjKind::Object {
            fields_i, fields_s, ..
        }) = ctx.heap.get_mut(h)
        {
            if let Some(slot) = fields_s.first_mut() {
                *slot = name;
            }
            if let Some(slot) = fields_i.first_mut() {
                *slot = value;
            }
            if let Some(slot) = fields_i.get_mut(1) {
                *slot = 5;
            }
        }
        Ok(())
    }

    fn actor_check_flag(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let h = regs.a.first().copied().unwrap_or(ObjHandle::NULL);
        let want = ctx
            .heap
            .str(regs.s.first().copied().unwrap_or_default())
            .to_owned();
        let (stored, value) = match ctx.heap.get(h) {
            Some(ObjKind::Object {
                fields_i, fields_s, ..
            }) if fields_i.get(1).copied() == Some(5) => {
                let name = fields_s.first().copied().unwrap_or_default();
                let value = fields_i.first().copied().unwrap_or(0);
                (ctx.heap.str(name).to_owned(), value)
            }
            _ => (String::new(), 0),
        };
        let on = !stored.is_empty() && stored.eq_ignore_ascii_case(&want);
        write_num(regs, if on { value as f64 } else { 0.0 });
        Ok(())
    }

    fn actor_default_speed(_ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        write_num(regs, 8.0);
        Ok(())
    }

    fn actor_spawn_health(_ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        write_num(regs, 100.0);
        Ok(())
    }

    fn actor_missile_damage(_ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let add = regs.i.get(1).copied().unwrap_or(0);
        write_num(regs, add as f64);
        Ok(())
    }

    fn actor_modified_damage(_ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let damage = regs.i.first().copied().unwrap_or(0);
        write_num(regs, damage as f64);
        Ok(())
    }

    fn actor_floor_terrain(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        store_string(ctx, regs, "Rock");
        Ok(())
    }

    fn actor_species(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        store_string(ctx, regs, "None");
        Ok(())
    }

    fn actor_die(ctx: &mut ZvmCtx<'_>, _regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        ctx.play.actor_ops.push(super::ZsActorOp::Action {
            name: "A_Die".to_owned(),
            args: [0; 4],
        });
        Ok(())
    }

    fn actor_vec2_offset(_ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let x = regs.f.first().copied().unwrap_or(0.0);
        let y = regs.f.get(1).copied().unwrap_or(0.0);
        if let Some(slot) = regs.f.first_mut() {
            *slot = x;
        }
        if let Some(slot) = regs.f.get_mut(1) {
            *slot = y;
        }
        Ok(())
    }

    fn actor_missing_index(_ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        write_num(regs, -1.0);
        Ok(())
    }

    fn actor_tag(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        store_string(ctx, regs, "");
        Ok(())
    }

    fn actor_zero_vec(_ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        for slot in regs.f.iter_mut().take(3) {
            *slot = 0.0;
        }
        Ok(())
    }

    fn actor_flat_aim(_ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        write_num(regs, 0.0);
        Ok(())
    }

    fn actor_false(_ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        write_num(regs, 0.0);
        Ok(())
    }

    // GZDoom's Check* methods return true when the actor is outside the
    // relevant range/sight of every in-game player (the state wrappers jump
    // when these return false). Distances and LOS are captured by room-sim
    // before entering the VM, keeping the native deterministic and world-free.
    fn actor_check_if_seen(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        write_num(regs, f64::from(!ctx.play.ai_observers.iter().any(|o| o.2)));
        Ok(())
    }

    fn actor_check_sight_or_range(
        ctx: &mut ZvmCtx<'_>,
        regs: &mut NativeRegs<'_>,
    ) -> Result<(), Trap> {
        let range = regs.f.first().copied().unwrap_or(0.0).max(0.0);
        let two_d = regs.i.first().copied().unwrap_or(0) != 0;
        let range2 = range * range;
        let in_interest = ctx
            .play
            .ai_observers
            .iter()
            .any(|o| (if two_d { o.0 } else { o.1 }) <= range2 || o.2);
        write_num(regs, f64::from(!in_interest));
        Ok(())
    }

    fn actor_check_range(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let range = regs.f.first().copied().unwrap_or(0.0).max(0.0);
        let two_d = regs.i.first().copied().unwrap_or(0) != 0;
        let range2 = range * range;
        let in_range = ctx
            .play
            .ai_observers
            .iter()
            .any(|o| (if two_d { o.0 } else { o.1 }) <= range2);
        write_num(regs, f64::from(!in_range));
        Ok(())
    }

    /// BeginPlay asks this before keeping `crouchsprite`. No crouch frames are built.
    fn player_setup_crouch_sprite(
        _ctx: &mut ZvmCtx<'_>,
        regs: &mut NativeRegs<'_>,
    ) -> Result<(), Trap> {
        write_num(regs, 0.0);
        Ok(())
    }

    fn key_name_all(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let h = regs.a.first().copied().unwrap_or(ObjHandle::NULL);
        let codes: Vec<i64> = match ctx.heap.get(h) {
            Some(ObjKind::ArrayI(items)) => items.clone(),
            _ => Vec::new(),
        };
        let mut out = String::new();
        for (n, code) in codes.iter().copied().enumerate() {
            if n > 0 {
                out.push_str(" or ");
            }
            out.push_str(&doom_key_name(code));
        }
        store_string(ctx, regs, &out);
        Ok(())
    }

    /// DirectInput scan codes from `InputEvent.EDoomInputKeys`.
    fn doom_key_name(code: i64) -> String {
        let named = match code {
            0x01 => "Escape",
            0x0e => "Backspace",
            0x0f => "Tab",
            0x1c => "Enter",
            0x39 => "Space",
            0xc7 => "Home",
            0xc8 => "Up",
            0xc9 => "PgUp",
            0xcb => "Left",
            0xcd => "Right",
            0xcf => "End",
            0xd0 => "Down",
            0xd1 => "PgDn",
            0xd2 => "Ins",
            0xd3 => "Del",
            0x3b => "F1",
            0x3c => "F2",
            0x3d => "F3",
            0x3e => "F4",
            0x3f => "F5",
            0x40 => "F6",
            0x41 => "F7",
            0x42 => "F8",
            0x43 => "F9",
            0x44 => "F10",
            0x57 => "F11",
            0x58 => "F12",
            0x100 => "Mouse1",
            0x101 => "Mouse2",
            0x102 => "Mouse3",
            _ => "",
        };
        if !named.is_empty() {
            return named.to_owned();
        }
        if (32..127).contains(&code) {
            return char::from_u32(code as u32)
                .unwrap_or('?')
                .to_ascii_uppercase()
                .to_string();
        }
        code.to_string()
    }

    /// Doom's six key types, in `doomkeys.zs` order.
    const DOOM_KEYS: [&str; 6] = [
        "BlueCard",
        "YellowCard",
        "RedCard",
        "BlueSkull",
        "YellowSkull",
        "RedSkull",
    ];

    fn key_type_count(_ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        write_num(regs, DOOM_KEYS.len() as f64);
        Ok(())
    }

    fn key_type(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let idx = regs.i.first().copied().unwrap_or(-1);
        let name = if idx >= 0 {
            DOOM_KEYS.get(idx as usize).copied()
        } else {
            None
        };
        let Some(name) = name else {
            write_num(regs, -1.0);
            store_string(ctx, regs, "");
            if let Some(slot) = regs.a.first_mut() {
                *slot = ObjHandle::NULL;
            }
            return Ok(());
        };
        let name_h = ctx.heap.intern_string(name);
        let obj = ctx.heap.new_object(0, 0, 0, 1);
        if let Some(ObjKind::Object { fields_s, .. }) = ctx.heap.get_mut(obj) {
            if let Some(slot) = fields_s.first_mut() {
                *slot = name_h;
            }
        }
        write_num(regs, idx as f64);
        store_string(ctx, regs, name);
        if let Some(slot) = regs.a.first_mut() {
            *slot = obj;
        }
        Ok(())
    }

    fn actor_normalize180(_ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let angle = regs.f.first().copied().unwrap_or(0.0);
        if let Some(slot) = regs.f.first_mut() {
            *slot = wrap_angle(angle);
        }
        Ok(())
    }

    fn actor_rail(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        queue_action(ctx, regs, "A_RailAttack");
        Ok(())
    }

    fn actor_radius(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        queue_action(ctx, regs, "A_Explode");
        Ok(())
    }

    fn actor_explode(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let damage = regs.i.first().copied().unwrap_or(0);
        ctx.play.actor_ops.push(super::ZsActorOp::Action {
            name: "A_Explode".to_owned(),
            args: [damage, 0, 0, 0],
        });
        Ok(())
    }

    fn actor_damage_factor(_ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let damage = regs.i.first().copied().unwrap_or(0);
        write_num(regs, damage as f64);
        Ok(())
    }

    fn statusbar_format_number(
        ctx: &mut ZvmCtx<'_>,
        regs: &mut NativeRegs<'_>,
    ) -> Result<(), Trap> {
        let n = regs.i.first().copied().unwrap_or(0);
        store_string(ctx, regs, &n.to_string());
        Ok(())
    }

    fn object_delete(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let h = regs.a.first().copied().unwrap_or(ObjHandle::NULL);
        let idx = regs.i.first().copied().unwrap_or(0).max(0) as usize;
        match ctx.heap.get_mut(h) {
            Some(ObjKind::ArrayI(items)) if idx < items.len() => {
                items.remove(idx);
            }
            Some(ObjKind::ArrayS(items)) if idx < items.len() => {
                items.remove(idx);
            }
            _ => {}
        }
        Ok(())
    }

    fn object_set_origin(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let h = regs.a.first().copied().unwrap_or(ObjHandle::NULL);
        let x = regs.f.first().copied().unwrap_or(0.0).to_bits() as i64;
        let y = regs.f.get(1).copied().unwrap_or(0.0).to_bits() as i64;
        let z = regs.f.get(2).copied().unwrap_or(0.0).to_bits() as i64;
        if let Some(ObjKind::Object { fields_i, .. }) = ctx.heap.get_mut(h) {
            if let Some(slot) = fields_i.first_mut() {
                *slot = x;
            }
            if let Some(slot) = fields_i.get_mut(1) {
                *slot = 1;
            }
            if let Some(slot) = fields_i.get_mut(2) {
                *slot = y;
            }
            if let Some(slot) = fields_i.get_mut(3) {
                *slot = 1;
            }
            if let Some(slot) = fields_i.get_mut(4) {
                *slot = z;
            }
            if let Some(slot) = fields_i.get_mut(5) {
                *slot = 1;
            }
        }
        Ok(())
    }

    fn object_plus_z(_ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let dz = regs.f.first().copied().unwrap_or(0.0);
        if let Some(slot) = regs.f.first_mut() {
            *slot = 0.0;
        }
        if let Some(slot) = regs.f.get_mut(1) {
            *slot = 0.0;
        }
        if let Some(slot) = regs.f.get_mut(2) {
            *slot = dz;
        }
        Ok(())
    }

    fn object_bam(_ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let angle = regs.f.first().copied().unwrap_or(0.0);
        write_num(regs, angle * (4294967296.0 / 360.0));
        Ok(())
    }

    fn global_one(_ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        if let Some(slot) = regs.f.first_mut() {
            *slot = 1.0;
        }
        Ok(())
    }

    fn global_lerp(_ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let a = regs.f.first().copied().unwrap_or(0.0);
        let b = regs.f.get(1).copied().unwrap_or(0.0);
        let t = regs.f.get(2).copied().unwrap_or(0.0);
        if let Some(slot) = regs.f.first_mut() {
            *slot = a + (b - a) * t;
        }
        Ok(())
    }

    fn global_deltas(regs: &NativeRegs<'_>) -> (f64, f64) {
        let x0 = regs.f.first().copied().unwrap_or(0.0);
        let y0 = regs.f.get(1).copied().unwrap_or(0.0);
        let x1 = regs.f.get(2).copied().unwrap_or(0.0);
        let y1 = regs.f.get(3).copied().unwrap_or(0.0);
        if x1 != 0.0 || y1 != 0.0 {
            (x1 - x0, y1 - y0)
        } else {
            (x0, y0)
        }
    }

    fn global_distance2d(_ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let (dx, dy) = global_deltas(regs);
        if let Some(slot) = regs.f.first_mut() {
            *slot = room_core::dmath::sqrt(dx * dx + dy * dy);
        }
        Ok(())
    }

    fn global_angle_to(_ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let (dx, dy) = global_deltas(regs);
        if let Some(slot) = regs.f.first_mut() {
            *slot = rad_to_deg(room_core::dmath::atan2(dy, dx));
        }
        Ok(())
    }

    fn global_splerp(_ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let a = regs.f.first().copied().unwrap_or(0.0);
        let b = regs.f.get(1).copied().unwrap_or(0.0);
        let t = regs.f.get(2).copied().unwrap_or(0.0).clamp(0.0, 1.0);
        let s = t * t * (3.0 - 2.0 * t);
        if let Some(slot) = regs.f.first_mut() {
            *slot = a + (b - a) * s;
        }
        Ok(())
    }

    fn global_distance_by_speed(
        _ctx: &mut ZvmCtx<'_>,
        regs: &mut NativeRegs<'_>,
    ) -> Result<(), Trap> {
        let dist = regs.f.first().copied().unwrap_or(0.0);
        let speed = regs.f.get(1).copied().unwrap_or(0.0);
        if let Some(slot) = regs.f.first_mut() {
            *slot = if speed == 0.0 { 0.0 } else { dist / speed };
        }
        Ok(())
    }

    fn wrap_angle(delta: f64) -> f64 {
        let mut d = delta % 360.0;
        if d > 180.0 {
            d -= 360.0;
        } else if d < -180.0 {
            d += 360.0;
        }
        d
    }

    fn global_delta_angle(_ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let from = regs.f.first().copied().unwrap_or(0.0);
        let to = regs.f.get(1).copied().unwrap_or(0.0);
        if let Some(slot) = regs.f.first_mut() {
            *slot = wrap_angle(to - from);
        }
        Ok(())
    }

    fn global_abs_angle(_ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let from = regs.f.first().copied().unwrap_or(0.0);
        let to = regs.f.get(1).copied().unwrap_or(0.0);
        let d = wrap_angle(to - from);
        if let Some(slot) = regs.f.first_mut() {
            *slot = if d < 0.0 { -d } else { d };
        }
        Ok(())
    }

    fn global_friction(_ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        if let Some(slot) = regs.f.first_mut() {
            *slot = 0.90625;
        }
        Ok(())
    }

    fn global_skill_float(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let prop = regs.i.first().copied().unwrap_or(0);
        let skill = ctx.play.skill;
        let v = match prop {
            0 => match skill {
                0 => 2.0,
                1 => 1.5,
                _ => 1.0,
            },
            _ => 1.0,
        };
        if let Some(slot) = regs.f.first_mut() {
            *slot = v;
        }
        Ok(())
    }

    fn abs_f(v: f64) -> f64 {
        if v < 0.0 { -v } else { v }
    }

    fn deg_to_rad(deg: f64) -> f64 {
        deg * (core::f64::consts::PI / 180.0)
    }

    fn rad_to_deg(rad: f64) -> f64 {
        rad * (180.0 / core::f64::consts::PI)
    }

    fn trunc_f(v: f64) -> f64 {
        if v >= 0.0 {
            room_core::dmath::floor(v)
        } else {
            -room_core::dmath::floor(-v)
        }
    }

    fn ceil_f(v: f64) -> f64 {
        let down = room_core::dmath::floor(v);
        if down == v { down } else { down + 1.0 }
    }

    fn round_half_away(v: f64) -> f64 {
        if v >= 0.0 {
            room_core::dmath::floor(v + 0.5)
        } else {
            -room_core::dmath::floor(-v + 0.5)
        }
    }

    fn actor_look(ctx: &mut ZvmCtx<'_>, _regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        ctx.play.actor_ops.push(super::ZsActorOp::Look);
        Ok(())
    }

    fn actor_pain(ctx: &mut ZvmCtx<'_>, _regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        ctx.play.actor_ops.push(super::ZsActorOp::Pain);
        Ok(())
    }

    fn actor_no_blocking(ctx: &mut ZvmCtx<'_>, _regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        ctx.play.actor_ops.push(super::ZsActorOp::NoBlocking);
        Ok(())
    }

    fn actor_chase(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let melee = regs
            .s
            .first()
            .copied()
            .map(|handle| {
                let label = ctx.heap.str(handle);
                if label == "__room_null_state" {
                    "null".to_owned()
                } else {
                    label.to_owned()
                }
            })
            .unwrap_or_default();
        let missile = regs
            .s
            .get(1)
            .copied()
            .map(|handle| {
                let label = ctx.heap.str(handle);
                if label == "__room_null_state" {
                    "null".to_owned()
                } else {
                    label.to_owned()
                }
            })
            .unwrap_or_default();
        let flags = regs.i.first().copied().unwrap_or(0) as i32;
        ctx.play.actor_ops.push(super::ZsActorOp::Chase {
            melee,
            missile,
            flags,
        });
        Ok(())
    }

    fn actor_face(ctx: &mut ZvmCtx<'_>, _regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        ctx.play.actor_ops.push(super::ZsActorOp::Face);
        Ok(())
    }

    fn actor_start_sound(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let name = ctx
            .heap
            .str(regs.s.first().copied().unwrap_or_default())
            .to_owned();
        ctx.play.actor_ops.push(super::ZsActorOp::Sound(name));
        Ok(())
    }

    fn actor_start_sound_full(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let name = ctx
            .heap
            .str(regs.s.first().copied().unwrap_or_default())
            .to_owned();
        let number = |index: usize, fallback: f64| {
            regs.f
                .get(index)
                .copied()
                .filter(|value| value.is_finite())
                .unwrap_or(fallback)
        };
        ctx.play.actor_ops.push(super::ZsActorOp::SoundStart {
            name,
            channel: i32::try_from(regs.i.first().copied().unwrap_or(4)).unwrap_or(4),
            flags: i32::try_from(regs.i.get(1).copied().unwrap_or(0)).unwrap_or(0),
            volume: number(0, 1.0),
            attenuation: number(1, 1.0),
            pitch: number(2, 0.0),
            start_time: number(3, 0.0),
        });
        Ok(())
    }

    fn actor_start_sound_if_not_same(
        ctx: &mut ZvmCtx<'_>,
        regs: &mut NativeRegs<'_>,
    ) -> Result<(), Trap> {
        let name = ctx
            .heap
            .str(regs.s.first().copied().unwrap_or_default())
            .to_owned();
        let check_name = ctx
            .heap
            .str(regs.s.get(1).copied().unwrap_or_default())
            .to_owned();
        let number = |index: usize, fallback: f64| {
            regs.f
                .get(index)
                .copied()
                .filter(|value| value.is_finite())
                .unwrap_or(fallback)
        };
        ctx.play.actor_ops.push(super::ZsActorOp::SoundIfNotSame {
            name,
            check_name,
            channel: i32::try_from(regs.i.first().copied().unwrap_or(4)).unwrap_or(4),
            flags: i32::try_from(regs.i.get(1).copied().unwrap_or(0)).unwrap_or(0),
            volume: number(0, 1.0),
            attenuation: number(1, 1.0),
            pitch: number(2, 0.0),
            start_time: number(3, 0.0),
        });
        Ok(())
    }

    fn actor_aim_line(_ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        if let Some(slot) = regs.f.first_mut() {
            *slot = 0.0;
        }
        Ok(())
    }

    fn actor_line_attack(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        // Signature path: float angle, range, pitch, then int damage.
        // Statement path: every argument was lowered as an int
        // (angle, range, pitch, damage, …).
        let range_f = regs.f.get(1).copied().unwrap_or(0.0);
        let (range, damage) = if range_f > 0.0 {
            let damage = i32::try_from(regs.i.first().copied().unwrap_or(0)).unwrap_or(0);
            (range_f, damage)
        } else {
            let range_i = regs.i.get(1).copied().unwrap_or(0);
            let range = if range_i > 0 { range_i as f64 } else { 2048.0 };
            let damage = i32::try_from(regs.i.get(3).copied().unwrap_or(0)).unwrap_or(0);
            (range, damage)
        };
        ctx.play
            .actor_ops
            .push(super::ZsActorOp::LineAttack { range, damage });
        Ok(())
    }

    fn wad_check_full(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let name = ctx
            .heap
            .str(regs.s.first().copied().unwrap_or_default())
            .to_owned();
        let v = ctx.play.wad_check(&name);
        if let Some(slot) = regs.i.first_mut() {
            *slot = i64::from(v);
        }
        Ok(())
    }

    fn wad_check_name(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let name = ctx
            .heap
            .str(regs.s.first().copied().unwrap_or_default())
            .to_owned();
        let v = ctx.play.wad_check(&name);
        if let Some(slot) = regs.i.first_mut() {
            *slot = i64::from(v);
        }
        Ok(())
    }

    fn wad_read_lump(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let lump = regs.i.first().copied().unwrap_or(-1) as i32;
        let text = ctx.play.wad_read(lump).unwrap_or_default();
        let h = ctx.heap.intern_string(&text);
        if let Some(slot) = regs.s.first_mut() {
            *slot = h;
        }
        if regs.s.len() > 1 {
            regs.s[1] = h;
        }
        Ok(())
    }

    fn wad_find_lump(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let name = ctx
            .heap
            .str(regs.s.first().copied().unwrap_or_default())
            .to_owned();
        let start = i32::try_from(regs.i.first().copied().unwrap_or(0)).unwrap_or(i32::MAX);
        let found = ctx.play.wad_find(&name, start);
        if let Some(slot) = regs.i.first_mut() {
            *slot = i64::from(found);
        }
        Ok(())
    }

    fn console_midprint(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let first = ctx
            .heap
            .str(regs.s.first().copied().unwrap_or_default())
            .to_owned();
        let msg = if first.is_empty() {
            ctx.heap
                .str(regs.s.get(1).copied().unwrap_or_default())
                .to_owned()
        } else {
            first
        };
        ctx.play.printf.push(msg);
        Ok(())
    }

    fn console_printf(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let msg = ctx
            .heap
            .str(regs.s.first().copied().unwrap_or_default())
            .to_owned();
        ctx.play.printf.push(msg);
        Ok(())
    }

    fn consoleplayer_get(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let v = ctx.play.consoleplayer;
        if let Some(slot) = regs.i.first_mut() {
            *slot = i64::from(v);
        }
        Ok(())
    }

    fn skill_get(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let v = ctx.play.skill;
        if let Some(slot) = regs.i.first_mut() {
            *slot = i64::from(v);
        }
        Ok(())
    }

    fn gamestate_get(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let v = ctx.play.gamestate;
        if let Some(slot) = regs.i.first_mut() {
            *slot = i64::from(v);
        }
        Ok(())
    }

    fn object_destroy(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let h = regs.a.first().copied().unwrap_or(ObjHandle::NULL);
        ctx.heap.destroy(h);
        Ok(())
    }

    fn hud_25(_ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        write_num(regs, 25.0);
        Ok(())
    }

    fn hud_50(_ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        write_num(regs, 50.0);
        Ok(())
    }

    fn hud_100(_ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        write_num(regs, 100.0);
        Ok(())
    }

    fn empty_int_array(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let h = ctx.heap.new_array_i();
        if let Some(slot) = regs.a.first_mut() {
            *slot = h;
        }
        Ok(())
    }

    fn host_gametic(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        write_num(regs, f64::from(ctx.play.gametic));
        Ok(())
    }

    fn host_ms_time(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        write_num(regs, f64::from(ctx.play.gametic) * 1000.0 / 35.0);
        Ok(())
    }

    fn sector_set_environment_id(
        ctx: &mut ZvmCtx<'_>,
        regs: &mut NativeRegs<'_>,
    ) -> Result<(), Trap> {
        let id = regs.i.first().copied().unwrap_or(0);
        ctx.play.actor_ops.push(super::ZsActorOp::SetEnvironment {
            id: Some(u16::try_from(id & 0xFFFF).unwrap_or(0)),
            name: None,
        });
        Ok(())
    }

    fn sector_set_environment(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let name = ctx.heap.str(regs.s.first().copied().unwrap_or_default()).to_owned();
        ctx.play
            .actor_ops
            .push(super::ZsActorOp::SetEnvironment { id: None, name: Some(name) });
        Ok(())
    }

    fn sector_ceiling(_ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let bottom = regs.f.get(2).copied().unwrap_or(0.0);
        if let Some(slot) = regs.f.first_mut() {
            *slot = bottom + 128.0;
        }
        Ok(())
    }

    fn global_double(_ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let value = regs.f.first().copied().unwrap_or(0.0);
        write_num(regs, value);
        Ok(())
    }

    fn host_max_players(_ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        write_num(regs, 1.0);
        Ok(())
    }

    fn host_ticrate(_ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        write_num(regs, 35.0);
        Ok(())
    }

    fn vec_angle(_ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let x = regs.f.first().copied().unwrap_or(0.0);
        let y = regs.f.get(1).copied().unwrap_or(0.0);
        if let Some(slot) = regs.f.first_mut() {
            *slot = rad_to_deg(room_core::dmath::atan2(y, x));
        }
        Ok(())
    }

    fn vec_length(_ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let x = regs.f.first().copied().unwrap_or(0.0);
        let y = regs.f.get(1).copied().unwrap_or(0.0);
        let z = regs.f.get(2).copied().unwrap_or(0.0);
        let len = room_core::dmath::sqrt(x * x + y * y + z * z);
        if let Some(slot) = regs.f.first_mut() {
            *slot = len;
        }
        Ok(())
    }

    fn vec_unit(_ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let x = regs.f.first().copied().unwrap_or(0.0);
        let y = regs.f.get(1).copied().unwrap_or(0.0);
        let z = regs.f.get(2).copied().unwrap_or(0.0);
        let len = room_core::dmath::sqrt(x * x + y * y + z * z);
        if len != 0.0 {
            if let Some(slot) = regs.f.first_mut() {
                *slot = x / len;
            }
            if let Some(slot) = regs.f.get_mut(1) {
                *slot = y / len;
            }
            if let Some(slot) = regs.f.get_mut(2) {
                *slot = z / len;
            }
        }
        Ok(())
    }

    fn string_byte_at(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let s = ctx.heap.str(regs.s.first().copied().unwrap_or_default());
        let pos = regs.i.first().copied().unwrap_or(0).max(0) as usize;
        let b = s.as_bytes().get(pos).copied().unwrap_or(0) as i64;
        write_num(regs, b as f64);
        Ok(())
    }

    fn string_to_double(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let s = ctx.heap.str(regs.s.first().copied().unwrap_or_default());
        let v = s.parse::<f64>().unwrap_or(0.0);
        write_num(regs, v);
        Ok(())
    }

    fn dict_insert(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let h = regs.a.first().copied().unwrap_or(ObjHandle::NULL);
        let key = regs.s.first().copied().unwrap_or_default();
        let val = regs.s.get(1).copied().unwrap_or_default();
        if let Some(ObjKind::Object { fields_s, .. }) = ctx.heap.get_mut(h) {
            let mut i = 0;
            while i + 1 < fields_s.len() {
                if fields_s[i] == key {
                    fields_s[i + 1] = val;
                    return Ok(());
                }
                i += 2;
            }
            fields_s.push(key);
            fields_s.push(val);
        }
        Ok(())
    }

    /// A `Dictionary` keeps `key, value` string pairs in `fields_s`.
    fn dict_new(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let h = ctx.heap.new_object(0, 0, 0, 0);
        if let Some(slot) = regs.a.first_mut() {
            *slot = h;
        }
        Ok(())
    }

    fn dict_pairs(ctx: &ZvmCtx<'_>, h: ObjHandle) -> Vec<(String, String)> {
        match ctx.heap.get(h) {
            Some(ObjKind::Object { fields_s, .. }) => fields_s
                .chunks_exact(2)
                .map(|kv| (ctx.heap.str(kv[0]).to_owned(), ctx.heap.str(kv[1]).to_owned()))
                .collect(),
            _ => Vec::new(),
        }
    }

    fn dict_remove(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let h = regs.a.first().copied().unwrap_or(ObjHandle::NULL);
        let key = regs.s.first().copied().unwrap_or_default();
        if let Some(ObjKind::Object { fields_s, .. }) = ctx.heap.get_mut(h) {
            let mut i = 0;
            while i + 1 < fields_s.len() {
                if fields_s[i] == key {
                    fields_s.drain(i..i + 2);
                    break;
                }
                i += 2;
            }
        }
        Ok(())
    }

    /// UZDoom `DictionaryToString`: a JSON object of string values.
    pub(crate) fn dict_json(pairs: &[(String, String)]) -> String {
        fn quote(out: &mut String, s: &str) {
            out.push('"');
            for ch in s.chars() {
                match ch {
                    '"' => out.push_str("\\\""),
                    '\\' => out.push_str("\\\\"),
                    '\n' => out.push_str("\\n"),
                    '\r' => out.push_str("\\r"),
                    '\t' => out.push_str("\\t"),
                    '\u{8}' => out.push_str("\\b"),
                    '\u{c}' => out.push_str("\\f"),
                    c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04X}", c as u32)),
                    c => out.push(c),
                }
            }
            out.push('"');
        }
        let mut out = String::from("{");
        for (n, (k, v)) in pairs.iter().enumerate() {
            if n > 0 {
                out.push(',');
            }
            quote(&mut out, k);
            out.push(':');
            quote(&mut out, v);
        }
        out.push('}');
        out
    }

    /// Parse `{"k":"v",…}`; `None` when it is not an object of strings.
    pub(crate) fn dict_parse_json(text: &str) -> Option<Vec<(String, String)>> {
        let mut it = text.chars().peekable();
        fn ws(it: &mut std::iter::Peekable<std::str::Chars<'_>>) {
            while it.peek().is_some_and(|c| c.is_whitespace()) {
                it.next();
            }
        }
        fn string(it: &mut std::iter::Peekable<std::str::Chars<'_>>) -> Option<String> {
            if it.next()? != '"' {
                return None;
            }
            let mut out = String::new();
            loop {
                match it.next()? {
                    '"' => return Some(out),
                    '\\' => match it.next()? {
                        '"' => out.push('"'),
                        '\\' => out.push('\\'),
                        '/' => out.push('/'),
                        'n' => out.push('\n'),
                        'r' => out.push('\r'),
                        't' => out.push('\t'),
                        'b' => out.push('\u{8}'),
                        'f' => out.push('\u{c}'),
                        'u' => {
                            let hex: String = (0..4).filter_map(|_| it.next()).collect();
                            let code = u32::from_str_radix(&hex, 16).ok()?;
                            out.push(char::from_u32(code).unwrap_or('\u{fffd}'));
                        }
                        _ => return None,
                    },
                    c => out.push(c),
                }
            }
        }
        let mut pairs = Vec::new();
        ws(&mut it);
        if it.next()? != '{' {
            return None;
        }
        ws(&mut it);
        if it.peek() == Some(&'}') {
            it.next();
        } else {
            loop {
                ws(&mut it);
                let k = string(&mut it)?;
                ws(&mut it);
                if it.next()? != ':' {
                    return None;
                }
                ws(&mut it);
                let v = string(&mut it)?;
                if let Some(slot) = pairs.iter_mut().find(|(pk, _): &&mut (String, String)| *pk == k) {
                    slot.1 = v;
                } else {
                    pairs.push((k, v));
                }
                ws(&mut it);
                match it.next()? {
                    ',' => continue,
                    '}' => break,
                    _ => return None,
                }
            }
        }
        ws(&mut it);
        it.next().is_none().then_some(pairs)
    }

    fn store_string_ret(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>, text: &str) {
        let h = ctx.heap.intern_string(text);
        if let Some(slot) = regs.s.first_mut() {
            *slot = h;
        }
        if regs.s.len() > 1 {
            regs.s[1] = h;
        }
    }

    fn dict_to_string(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let h = regs.a.first().copied().unwrap_or(ObjHandle::NULL);
        let text = dict_json(&dict_pairs(ctx, h));
        store_string_ret(ctx, regs, &text);
        Ok(())
    }

    fn dict_from_string(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let text = ctx.heap.str(regs.s.first().copied().unwrap_or_default()).to_owned();
        if text == "null" {
            if let Some(slot) = regs.a.first_mut() {
                *slot = ObjHandle::NULL;
            }
            return Ok(());
        }
        let pairs = if text.is_empty() {
            Vec::new()
        } else {
            dict_parse_json(&text).ok_or_else(|| {
                Trap::Native("Dictionary is expected to be a JSON object of strings.".into())
            })?
        };
        let h = ctx.heap.new_object(0, 0, 0, 0);
        let handles: Vec<_> = pairs
            .iter()
            .flat_map(|(k, v)| [k.as_str(), v.as_str()])
            .map(|t| ctx.heap.intern_string(t))
            .collect();
        if let Some(ObjKind::Object { fields_s, .. }) = ctx.heap.get_mut(h) {
            *fields_s = handles;
        }
        if let Some(slot) = regs.a.first_mut() {
            *slot = h;
        }
        Ok(())
    }

    /// `DictionaryIterator`: `fields_a[0]` = dictionary, `fields_i[0]` = pair index.
    fn dict_iter_new(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let dict = regs.a.first().copied().unwrap_or(ObjHandle::NULL);
        let h = ctx.heap.new_object(0, 1, 1, 0);
        if let Some(ObjKind::Object {
            fields_i, fields_a, ..
        }) = ctx.heap.get_mut(h)
        {
            fields_i[0] = -1;
            fields_a[0] = dict;
        }
        if let Some(slot) = regs.a.first_mut() {
            *slot = h;
        }
        Ok(())
    }

    fn dict_iter_state(ctx: &ZvmCtx<'_>, h: ObjHandle) -> Option<(ObjHandle, i64)> {
        match ctx.heap.get(h) {
            Some(ObjKind::Object {
                fields_i, fields_a, ..
            }) => Some((*fields_a.first()?, *fields_i.first()?)),
            _ => None,
        }
    }

    fn dict_iter_next(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let h = regs.a.first().copied().unwrap_or(ObjHandle::NULL);
        let mut more = false;
        if let Some((dict, idx)) = dict_iter_state(ctx, h) {
            let len = dict_pairs(ctx, dict).len() as i64;
            let next = (idx + 1).min(len);
            more = next < len;
            if let Some(ObjKind::Object { fields_i, .. }) = ctx.heap.get_mut(h) {
                fields_i[0] = next;
            }
        }
        if let Some(slot) = regs.i.first_mut() {
            *slot = i64::from(more);
        }
        Ok(())
    }

    fn dict_iter_entry(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>, value: bool) {
        let h = regs.a.first().copied().unwrap_or(ObjHandle::NULL);
        let text = dict_iter_state(ctx, h)
            .and_then(|(dict, idx)| {
                let pairs = dict_pairs(ctx, dict);
                let (k, v) = pairs.get(usize::try_from(idx).ok()?)?.clone();
                Some(if value { v } else { k })
            })
            .unwrap_or_default();
        store_string_ret(ctx, regs, &text);
    }

    fn dict_iter_key(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        dict_iter_entry(ctx, regs, false);
        Ok(())
    }

    fn dict_iter_value(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        dict_iter_entry(ctx, regs, true);
        Ok(())
    }

    fn dict_at(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let h = regs.a.first().copied().unwrap_or(ObjHandle::NULL);
        let key = regs.s.first().copied().unwrap_or_default();
        let found = match ctx.heap.get(h) {
            Some(ObjKind::Object { fields_s, .. }) => {
                let mut out = None;
                let mut i = 0;
                while i + 1 < fields_s.len() {
                    if fields_s[i] == key {
                        out = Some(fields_s[i + 1]);
                        break;
                    }
                    i += 2;
                }
                out
            }
            _ => None,
        };
        let text = match found {
            Some(handle) => ctx.heap.str(handle).to_owned(),
            None => String::new(),
        };
        store_string(ctx, regs, &text);
        Ok(())
    }

    fn array_find(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let h = regs.a.first().copied().unwrap_or(ObjHandle::NULL);
        let needle = regs.i.first().copied().unwrap_or(0);
        // UZDoom `Find`: the first index, else `Size()` (scripts test
        // `Find(x) == Size()`).
        let found = match ctx.heap.get(h) {
            Some(ObjKind::ArrayI(items)) => items
                .iter()
                .position(|item| *item == needle)
                .map_or(items.len() as i64, |idx| idx as i64),
            Some(ObjKind::ArrayS(items)) => items.len() as i64,
            _ => 0,
        };
        write_num(regs, found as f64);
        Ok(())
    }

    fn array_at(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let h = regs.a.first().copied().unwrap_or(ObjHandle::NULL);
        let idx = regs.i.first().copied().unwrap_or(0);
        let value = match ctx.heap.get(h) {
            Some(ObjKind::ArrayI(items)) if idx >= 0 => {
                items.get(idx as usize).copied().unwrap_or(0)
            }
            _ => 0,
        };
        write_num(regs, value as f64);
        Ok(())
    }

    fn array_has(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let h = regs.a.first().copied().unwrap_or(ObjHandle::NULL);
        let idx = regs.i.first().copied().unwrap_or(0);
        let ok = match ctx.heap.get(h) {
            Some(ObjKind::ArrayI(items)) => idx >= 0 && (idx as usize) < items.len(),
            Some(ObjKind::ArrayS(items)) => idx >= 0 && (idx as usize) < items.len(),
            _ => false,
        };
        write_num(regs, if ok { 1.0 } else { 0.0 });
        Ok(())
    }

    fn reply_append(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let first = ctx
            .heap
            .str(regs.s.first().copied().unwrap_or_default())
            .to_owned();
        let text = if first.is_empty() {
            ctx.heap
                .str(regs.s.get(1).copied().unwrap_or_default())
                .to_owned()
        } else {
            first
        };
        store_string(ctx, regs, &text);
        Ok(())
    }

    fn array_size(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let h = regs.a.first().copied().unwrap_or(ObjHandle::NULL);
        let n = match ctx.heap.get(h) {
            Some(ObjKind::ArrayI(v)) => v.len() as i64,
            Some(ObjKind::ArrayS(v)) => v.len() as i64,
            _ => 0,
        };
        if let Some(slot) = regs.i.first_mut() {
            *slot = n;
        }
        Ok(())
    }

    fn array_push_i(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let h = regs.a.first().copied().unwrap_or(ObjHandle::NULL);
        let v = regs
            .i
            .get(1)
            .copied()
            .or_else(|| regs.i.first().copied())
            .unwrap_or(0);
        match ctx.heap.get_mut(h) {
            Some(ObjKind::ArrayI(items)) => {
                items.push(v);
                Ok(())
            }
            _ => Err(Trap::Native("Array.Push: not Array<int>".into())),
        }
    }

    fn array_clear(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let h = regs.a.first().copied().unwrap_or(ObjHandle::NULL);
        match ctx.heap.get_mut(h) {
            Some(ObjKind::ArrayI(items)) => {
                items.clear();
                Ok(())
            }
            Some(ObjKind::ArrayS(items)) => {
                items.clear();
                Ok(())
            }
            _ => Err(Trap::Native("Array.Clear: not an Array".into())),
        }
    }

    fn array_delete(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let h = regs.a.first().copied().unwrap_or(ObjHandle::NULL);
        let idx = regs.i.first().copied().unwrap_or(0).max(0) as usize;
        match ctx.heap.get_mut(h) {
            Some(ObjKind::ArrayI(items)) => {
                if idx < items.len() {
                    items.remove(idx);
                }
                Ok(())
            }
            Some(ObjKind::ArrayS(items)) => {
                if idx < items.len() {
                    items.remove(idx);
                }
                Ok(())
            }
            _ => Err(Trap::Native("Array.Delete: not an Array".into())),
        }
    }

    fn array_new_i(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let h = ctx.heap.new_array_i();
        if let Some(slot) = regs.a.first_mut() {
            *slot = h;
        }
        Ok(())
    }

    fn array_new_s(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let h = ctx.heap.new_array_s();
        if let Some(slot) = regs.a.first_mut() {
            *slot = h;
        }
        Ok(())
    }

    /// `Array<String>.Find(v)` (names and class values too, case-insensitive
    /// like names).
    fn array_find_s(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let h = regs.a.first().copied().unwrap_or(ObjHandle::NULL);
        let v = ctx.heap.str(regs.s.first().copied().unwrap_or_default()).to_owned();
        let k = match ctx.heap.get(h) {
            Some(ObjKind::ArrayS(items)) => items
                .iter()
                .position(|x| ctx.heap.str(*x).eq_ignore_ascii_case(&v))
                .unwrap_or(items.len()),
            Some(ObjKind::ArrayI(items)) => items.len(),
            _ => 0,
        };
        if let Some(slot) = regs.i.first_mut() {
            *slot = k as i64;
        }
        Ok(())
    }

    fn array_push_s(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let h = regs.a.first().copied().unwrap_or(ObjHandle::NULL);
        let v = regs.s.first().copied().unwrap_or_default();
        match ctx.heap.get_mut(h) {
            Some(ObjKind::ArrayS(items)) => {
                items.push(v);
                Ok(())
            }
            _ => Err(Trap::Native("Array.PushS: not Array<string>".into())),
        }
    }

    fn array_insert_s(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let h = regs.a.first().copied().unwrap_or(ObjHandle::NULL);
        let idx = regs.i.first().copied().unwrap_or(0).max(0) as usize;
        let v = regs.s.first().copied().unwrap_or_default();
        match ctx.heap.get_mut(h) {
            Some(ObjKind::ArrayS(items)) => {
                let at = idx.min(items.len());
                items.insert(at, v);
                Ok(())
            }
            _ => Err(Trap::Native("Array.InsertS: not Array<string>".into())),
        }
    }

    fn string_char_code_at(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let s = ctx.heap.str(regs.s.first().copied().unwrap_or_default());
        let idx = regs.i.first().copied().unwrap_or(0);
        let v = if idx < 0 {
            0
        } else {
            s.chars().nth(idx as usize).map(|c| c as i64).unwrap_or(0)
        };
        if let Some(slot) = regs.i.first_mut() {
            *slot = v;
        }
        Ok(())
    }

    fn array_insert(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let h = regs.a.first().copied().unwrap_or(ObjHandle::NULL);
        let idx = regs.i.first().copied().unwrap_or(0).max(0) as usize;
        let v = regs.i.get(1).copied().unwrap_or(0);
        match ctx.heap.get_mut(h) {
            Some(ObjKind::ArrayI(items)) => {
                let at = idx.min(items.len());
                items.insert(at, v);
                Ok(())
            }
            _ => Err(Trap::Native("Array.Insert: not Array<int>".into())),
        }
    }

    fn string_is_empty(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let text = ctx.heap.str(regs.s.first().copied().unwrap_or_default());
        write_num(regs, if text.is_empty() { 1.0 } else { 0.0 });
        Ok(())
    }

    fn string_append_char(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let mut text = ctx
            .heap
            .str(regs.s.first().copied().unwrap_or_default())
            .to_owned();
        let code = regs.i.first().copied().unwrap_or(0);
        if let Some(ch) = char::from_u32(u32::try_from(code).unwrap_or(0)) {
            text.push(ch);
        }
        store_string(ctx, regs, &text);
        Ok(())
    }

    fn string_delete_last(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let text = ctx.heap.str(regs.s.first().copied().unwrap_or_default());
        let mut chars: Vec<char> = text.chars().collect();
        chars.pop();
        let out: String = chars.into_iter().collect();
        store_string(ctx, regs, &out);
        Ok(())
    }

    fn tooltip_lines(_ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        write_num(regs, 4.0);
        Ok(())
    }

    fn string_length(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let s = ctx.heap.str(regs.s.first().copied().unwrap_or_default());
        if let Some(slot) = regs.i.first_mut() {
            *slot = s.chars().count() as i64;
        }
        Ok(())
    }

    fn store_string(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>, text: &str) {
        let h = ctx.heap.intern_string(text);
        if regs.s.len() > 1 {
            regs.s[1] = h;
        } else if let Some(slot) = regs.s.first_mut() {
            *slot = h;
        }
    }

    fn string_left(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let s = ctx.heap.str(regs.s.first().copied().unwrap_or_default());
        let len = regs.i.first().copied().unwrap_or(0).max(0) as usize;
        let out: String = s.chars().take(len).collect();
        store_string(ctx, regs, &out);
        Ok(())
    }

    fn string_right(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let s = ctx.heap.str(regs.s.first().copied().unwrap_or_default());
        let len = usize::try_from(regs.i.first().copied().unwrap_or(0).max(0)).unwrap_or(0);
        let chars: Vec<char> = s.chars().collect();
        let start = chars.len().saturating_sub(len);
        let out: String = chars.into_iter().skip(start).collect();
        store_string(ctx, regs, &out);
        Ok(())
    }

    fn string_make_lower(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let s = ctx
            .heap
            .str(regs.s.first().copied().unwrap_or_default())
            .to_lowercase();
        store_string(ctx, regs, &s);
        Ok(())
    }

    fn string_make_upper(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let s = ctx
            .heap
            .str(regs.s.first().copied().unwrap_or_default())
            .to_uppercase();
        store_string(ctx, regs, &s);
        Ok(())
    }

    fn string_is_not_empty(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let text = ctx.heap.str(regs.s.first().copied().unwrap_or_default());
        write_num(regs, if text.is_empty() { 0.0 } else { 1.0 });
        Ok(())
    }

    fn string_is_int(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let text = ctx.heap.str(regs.s.first().copied().unwrap_or_default());
        let valid = text.trim().parse::<i64>().is_ok();
        write_num(regs, if valid { 1.0 } else { 0.0 });
        Ok(())
    }

    fn string_is_double(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let text = ctx.heap.str(regs.s.first().copied().unwrap_or_default());
        let valid = text.trim().parse::<f64>().is_ok_and(f64::is_finite);
        write_num(regs, if valid { 1.0 } else { 0.0 });
        Ok(())
    }

    fn string_compare(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let left = ctx.heap.str(regs.s.first().copied().unwrap_or_default());
        let right = ctx.heap.str(regs.s.get(1).copied().unwrap_or_default());
        let ordering = left.cmp(right);
        if let Some(slot) = regs.i.first_mut() {
            *slot = match ordering {
                std::cmp::Ordering::Less => -1,
                std::cmp::Ordering::Equal => 0,
                std::cmp::Ordering::Greater => 1,
            };
        }
        Ok(())
    }

    fn string_compare_nocase(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let left = ctx
            .heap
            .str(regs.s.first().copied().unwrap_or_default())
            .to_ascii_lowercase();
        let right = ctx
            .heap
            .str(regs.s.get(1).copied().unwrap_or_default())
            .to_ascii_lowercase();
        let ordering = left.cmp(&right);
        if let Some(slot) = regs.i.first_mut() {
            *slot = match ordering {
                std::cmp::Ordering::Less => -1,
                std::cmp::Ordering::Equal => 0,
                std::cmp::Ordering::Greater => 1,
            };
        }
        Ok(())
    }

    fn string_split(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let text = ctx
            .heap
            .str(regs.s.first().copied().unwrap_or_default())
            .to_owned();
        let delim = ctx
            .heap
            .str(regs.s.get(1).copied().unwrap_or_default())
            .to_owned();
        // `TOK_SKIPEMPTY` is 0. An omitted flag keeps empty tokens.
        let keep_empty = regs.i.first().copied().unwrap_or(1) != 0;
        let parts = split_tokens(&text, &delim, keep_empty);
        let handles: Vec<_> = parts
            .iter()
            .map(|part| ctx.heap.intern_string(part))
            .collect();
        let h = regs.a.first().copied().unwrap_or(ObjHandle::NULL);
        if let Some(ObjKind::ArrayS(items)) = ctx.heap.get_mut(h) {
            items.clear();
            items.extend(handles);
        }
        Ok(())
    }

    /// `String.Split`. An empty delimiter yields the whole string as one token.
    fn split_tokens(text: &str, delim: &str, keep_empty: bool) -> Vec<String> {
        if delim.is_empty() {
            return if text.is_empty() {
                if keep_empty {
                    vec![String::new()]
                } else {
                    Vec::new()
                }
            } else {
                vec![text.to_owned()]
            };
        }
        let mut out = Vec::new();
        let mut last = 0;
        while let Some(rel) = text[last..].find(delim) {
            let start = last + rel;
            if start > last || keep_empty {
                out.push(text[last..start].to_owned());
            }
            last = start + delim.len();
        }
        if last < text.len() || keep_empty {
            out.push(text.get(last..).unwrap_or("").to_owned());
        }
        out
    }

    fn string_replace(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let src = ctx
            .heap
            .str(regs.s.first().copied().unwrap_or_default())
            .to_owned();
        let pat = ctx
            .heap
            .str(regs.s.get(1).copied().unwrap_or_default())
            .to_owned();
        let rep = ctx
            .heap
            .str(regs.s.get(2).copied().unwrap_or_default())
            .to_owned();
        let out = if pat.is_empty() {
            src
        } else {
            src.replace(&pat, &rep)
        };
        store_string(ctx, regs, &out);
        Ok(())
    }

    fn string_char_lower(_ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let code = regs.i.first().copied().unwrap_or(0);
        let lower = u32::try_from(code)
            .ok()
            .and_then(char::from_u32)
            .and_then(|ch| ch.to_lowercase().next())
            .map(|ch| i64::from(ch as u32))
            .unwrap_or(code);
        if let Some(slot) = regs.i.first_mut() {
            *slot = lower;
        }
        Ok(())
    }

    fn string_index_of(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let hay = ctx
            .heap
            .str(regs.s.first().copied().unwrap_or_default())
            .to_owned();
        let needle = ctx
            .heap
            .str(regs.s.get(1).copied().unwrap_or_default())
            .to_owned();
        let start = usize::try_from(regs.i.first().copied().unwrap_or(0).max(0)).unwrap_or(0);
        let found = if needle.is_empty() {
            Some(start as i64)
        } else {
            let rest: String = hay.chars().skip(start).collect();
            rest.find(&needle).map(|byte| {
                let chars = rest[..byte].chars().count();
                (start + chars) as i64
            })
        };
        if let Some(slot) = regs.i.first_mut() {
            *slot = found.unwrap_or(-1);
        }
        Ok(())
    }

    fn string_right_index_of(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let hay = ctx
            .heap
            .str(regs.s.first().copied().unwrap_or_default())
            .to_owned();
        let needle = ctx
            .heap
            .str(regs.s.get(1).copied().unwrap_or_default())
            .to_owned();
        let hay: Vec<char> = hay.chars().collect();
        let needle: Vec<char> = needle.chars().collect();
        let end = usize::try_from(regs.i.first().copied().unwrap_or(i64::MAX).max(0))
            .unwrap_or(usize::MAX)
            .min(hay.len());
        let found = if needle.is_empty() {
            Some(end)
        } else if needle.len() > hay.len() {
            None
        } else {
            let last_start = hay.len().saturating_sub(needle.len()).min(end);
            (0..=last_start)
                .rev()
                .find(|&start| hay[start..start + needle.len()] == needle)
        };
        if let Some(slot) = regs.i.first_mut() {
            *slot = found.map_or(-1, |index| i64::try_from(index).unwrap_or(i64::MAX));
        }
        Ok(())
    }

    fn string_identity(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let s = ctx
            .heap
            .str(regs.s.first().copied().unwrap_or_default())
            .to_owned();
        store_string(ctx, regs, &s);
        Ok(())
    }

    fn font_string_width(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let s = ctx.heap.str(regs.s.first().copied().unwrap_or_default());
        if let Some(slot) = regs.i.first_mut() {
            *slot = s.chars().count() as i64 * 8;
        }
        Ok(())
    }

    fn font_char_width(_ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        if let Some(slot) = regs.i.first_mut() {
            *slot = 8;
        }
        Ok(())
    }

    fn font_height(_ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        if let Some(slot) = regs.i.first_mut() {
            *slot = 16;
        }
        Ok(())
    }

    fn font_ascender(_ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        if let Some(slot) = regs.i.first_mut() {
            *slot = 12;
        }
        Ok(())
    }

    fn font_not_dynamic(_ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        if let Some(slot) = regs.i.first_mut() {
            *slot = 0;
        }
        Ok(())
    }

    fn write_num(regs: &mut NativeRegs<'_>, v: f64) {
        if let Some(slot) = regs.f.first_mut() {
            *slot = v;
        }
        if let Some(slot) = regs.i.first_mut() {
            *slot = v as i64;
        }
    }

    fn screen_aspect(_ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        write_num(regs, 4.0 / 3.0);
        Ok(())
    }

    fn screen_width(_ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        write_num(regs, 320.0);
        Ok(())
    }

    fn screen_height(_ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        write_num(regs, 200.0);
        Ok(())
    }

    fn screen_clip(_ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        for (slot, v) in regs.i.iter_mut().zip([0, 0, 320, 200]) {
            *slot = v;
        }
        Ok(())
    }

    fn screen_full(_ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        for (slot, v) in regs.f.iter_mut().zip([0.0, 0.0, 320.0, 200.0]) {
            *slot = v;
        }
        Ok(())
    }

    fn global_slot(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let name = ctx.play.native_name.to_ascii_lowercase();
        let key = name.rsplit('.').next().unwrap_or(&name);
        match key {
            "gameticrate" => write_num(regs, 35.0),
            "cleanwidth" | "cleanwidth_1" => write_num(regs, 320.0),
            "cleanheight" | "cleanheight_1" => write_num(regs, 200.0),
            "cleanxfac" | "cleanxfac_1" | "cleanyfac" | "cleanyfac_1" => write_num(regs, 1.0),
            "consoleplayer" => write_num(regs, f64::from(ctx.play.consoleplayer)),
            "gamestate" => write_num(regs, f64::from(ctx.play.gamestate)),
            "skill" => write_num(regs, f64::from(ctx.play.skill)),
            "notifyfontscale" | "viewactive" => write_num(regs, 1.0),
            "paused" | "menuactive" | "multiplayer" | "netgame" | "automapactive"
            | "globalfreeze" | "gameaction" | "consolestate" | "generic_ui" | "backbuttonalpha"
            | "backbuttontime" | "localviewpitch" | "demoplayback" => {
                write_num(regs, 0.0);
            }
            "gametic" => write_num(regs, f64::from(ctx.play.gametic)),
            "net_arbitrator" => write_num(regs, f64::from(ctx.play.consoleplayer)),
            "validcount" => write_num(regs, 1.0),
            "skyflatnum" => write_num(regs, -1.0),
            "wp_nochange" => {
                if ctx.play.wp_nochange.is_null() {
                    ctx.play.wp_nochange = ctx.heap.new_object(0, 1, 0, 0);
                }
                if let Some(slot) = regs.a.first_mut() {
                    *slot = ctx.play.wp_nochange;
                }
            }
            "playeringame" => {
                let idx = regs.i.first().copied().unwrap_or(0);
                write_num(regs, if idx == 0 { 1.0 } else { 0.0 });
            }
            "allclasses" | "allactorclasses" | "allepisodes" | "allskills" | "playerclasses"
            | "playerskins" | "teams" | "terrains" | "allservices" => {
                let h = ctx.heap.new_array_i();
                if let Some(slot) = regs.a.first_mut() {
                    *slot = h;
                }
            }
            _ => {
                let h = ctx.heap.new_object(0, 4, 2, 2);
                if let Some(slot) = regs.a.first_mut() {
                    *slot = h;
                }
            }
        }
        Ok(())
    }

    fn next_player_number(_ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let cur = regs.i.first().copied().unwrap_or(-1);
        write_num(regs, if cur < 0 { 0.0 } else { -1.0 });
        Ok(())
    }

    fn next_player_obj(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        if let Some(slot) = regs.a.first_mut() {
            *slot = if slot.is_null() {
                ctx.heap.new_object(0, 2, 1, 1)
            } else {
                ObjHandle::NULL
            };
        }
        Ok(())
    }

    fn player_display_name(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        store_string(ctx, regs, "Player");
        Ok(())
    }

    fn player_name_changed(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        ctx.play.player_name = ctx
            .heap
            .str(regs.s.first().copied().unwrap_or_default())
            .to_owned();
        Ok(())
    }

    fn player_gender_changed(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let v = regs.i.first().copied().unwrap_or(0);
        ctx.play.cvars.insert("gender".to_owned(), v as i32);
        Ok(())
    }

    fn player_always_run_changed(
        ctx: &mut ZvmCtx<'_>,
        regs: &mut NativeRegs<'_>,
    ) -> Result<(), Trap> {
        let v = regs.i.first().copied().unwrap_or(0) as i32;
        ctx.play.cvars.insert("cl_run".to_owned(), v);
        ctx.play.cvars.insert("alwaysrun".to_owned(), v);
        Ok(())
    }

    fn cvar_cl_run(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        write_num(regs, f64::from(ctx.play.cvar("cl_run")));
        Ok(())
    }

    fn player_switch_on_pickup_changed(
        ctx: &mut ZvmCtx<'_>,
        regs: &mut NativeRegs<'_>,
    ) -> Result<(), Trap> {
        // The Switch menu stores `GetNeverSwitch` itself.
        let v = regs.i.first().copied().unwrap_or(0) as i32;
        ctx.play.cvars.insert("neverswitch".to_owned(), v);
        Ok(())
    }

    fn player_never_switch(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        write_num(regs, f64::from(ctx.play.cvar("neverswitch")));
        Ok(())
    }

    fn player_user_name(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let name = if ctx.play.player_name.is_empty() {
            ctx.play.cvar_string("name")
        } else {
            ctx.play.player_name.clone()
        };
        let limit = regs.i.first().copied().unwrap_or(0).max(0) as usize;
        let value = if limit > 0 {
            let mut chars = name.chars();
            let prefix = chars.by_ref().take(limit).collect::<String>();
            if chars.next().is_some() {
                format!("{prefix}...")
            } else {
                name
            }
        } else {
            name
        };
        store_string(ctx, regs, &value);
        Ok(())
    }

    fn broken_count(_ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        write_num(regs, 0.0);
        Ok(())
    }

    fn broken_string_at(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        store_string(ctx, regs, "");
        Ok(())
    }

    fn player_autoaim(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        write_num(regs, ctx.play.autoaim);
        Ok(())
    }

    fn player_autoaim_changed(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        ctx.play.autoaim = regs.f.first().copied().unwrap_or(ctx.play.autoaim);
        Ok(())
    }

    fn player_color_changed(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let chan = |slot: usize| regs.i.get(slot).copied().unwrap_or(0).clamp(0, 255) as u8;
        ctx.play.color_rgb = Some((chan(0), chan(1), chan(2)));
        Ok(())
    }

    fn player_info_color(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let text = match ctx.play.color_rgb {
            Some((r, g, b)) => format!("{r} {g} {b}"),
            None => "Green".to_owned(),
        };
        store_string(ctx, regs, &text);
        Ok(())
    }

    fn player_class_changed(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let sel = regs.i.first().copied().unwrap_or(0) as i32;
        ctx.play.cvars.insert("playerclass".to_owned(), sel);
        Ok(())
    }

    fn player_class_num(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        write_num(regs, f64::from(ctx.play.cvar("playerclass")));
        Ok(())
    }

    fn player_team_changed(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let sel = regs.i.first().copied().unwrap_or(0);
        let team = if sel == 0 { -1 } else { sel - 1 };
        ctx.play.cvars.insert("team".to_owned(), team as i32);
        Ok(())
    }

    fn player_team(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        write_num(regs, f64::from(ctx.play.cvar("team")));
        Ok(())
    }

    fn player_color_set_changed(
        ctx: &mut ZvmCtx<'_>,
        regs: &mut NativeRegs<'_>,
    ) -> Result<(), Trap> {
        let set = regs.i.first().copied().unwrap_or(0) as i32;
        ctx.play.cvars.insert("colorset".to_owned(), set);
        Ok(())
    }

    fn player_color_set(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        write_num(regs, f64::from(ctx.play.cvar("colorset")));
        Ok(())
    }

    fn player_skin_changed(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let skin = regs.i.first().copied().unwrap_or(0) as i32;
        ctx.play.cvars.insert("skin".to_owned(), skin);
        Ok(())
    }

    fn player_skin(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let skin = ctx.play.cvar("skin");
        let text = if skin == 0 {
            "base".to_owned()
        } else {
            skin.to_string()
        };
        store_string(ctx, regs, &text);
        Ok(())
    }

    fn level_yes(_ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        write_num(regs, 1.0);
        Ok(())
    }

    fn level_checksum(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let checksum = ctx.play.level_checksum.clone();
        store_string(ctx, regs, &checksum);
        Ok(())
    }

    fn level_set_inter_music(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        ctx.play.inter_music = ctx
            .heap
            .str(regs.s.first().copied().unwrap_or_default())
            .to_owned();
        Ok(())
    }

    fn level_set_sky_fog(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let v = regs.i.first().copied().unwrap_or(0);
        ctx.play
            .level_ops
            .push(super::ZsLevelOp::SkyFog(i32::try_from(v).unwrap_or(0)));
        Ok(())
    }

    fn level_set_thick_fog(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let d = regs.f.first().copied().unwrap_or(0.0);
        let m = regs.f.get(1).copied().unwrap_or(0.0);
        ctx.play.level_ops.push(super::ZsLevelOp::ThickFog(d, m));
        Ok(())
    }

    fn level_change_sky_mist(
        ctx: &mut ZvmCtx<'_>,
        regs: &mut NativeRegs<'_>,
    ) -> Result<(), Trap> {
        // Args: TextureID (int bank), bool usemist = true (int bank), float yscale = 1.
        let id = regs.i.first().copied().unwrap_or(0);
        let texture = ctx.play.texture_name(id).map(str::to_owned);
        let use_mist = if ctx.play.native_arg_counts[0] >= 2 {
            regs.i.get(1).copied().unwrap_or(1) != 0
        } else {
            true
        };
        let yscale = if ctx.play.native_arg_counts[1] >= 1 {
            regs.f.first().copied().unwrap_or(1.0)
        } else {
            1.0
        };
        ctx.play.level_ops.push(super::ZsLevelOp::SkyMist {
            texture,
            use_mist,
            yscale,
        });
        Ok(())
    }

    fn status_top(_ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        write_num(regs, 168.0);
        Ok(())
    }

    fn player_color(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        store_string(ctx, regs, "Green");
        Ok(())
    }

    fn level_map_name(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let name = ctx
            .heap
            .str(regs.s.first().copied().unwrap_or_default())
            .to_owned();
        let out = if name.is_empty() {
            "MAP01"
        } else {
            name.as_str()
        };
        store_string(ctx, regs, out);
        Ok(())
    }

    fn font_new(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let h = ctx.heap.new_object(0, 4, 2, 2);
        if let Some(slot) = regs.a.first_mut() {
            *slot = h;
        }
        Ok(())
    }

    fn font_can_print(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let s = ctx.heap.str(regs.s.first().copied().unwrap_or_default());
        let ok = s.chars().all(|ch| !ch.is_control());
        if let Some(slot) = regs.i.first_mut() {
            *slot = i64::from(ok);
        }
        Ok(())
    }

    fn string_filter(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let s = ctx.heap.str(regs.s.first().copied().unwrap_or_default());
        let mut out = String::new();
        let mut chars = s.chars();
        while let Some(ch) = chars.next() {
            // Doom text-color escape is `0x1c` plus one code.
            if ch == '\u{1c}' {
                let _ = chars.next();
                continue;
            }
            out.push(ch);
        }
        store_string(ctx, regs, &out);
        Ok(())
    }

    fn string_mid(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let s = ctx.heap.str(regs.s.first().copied().unwrap_or_default());
        let start = regs.i.first().copied().unwrap_or(0).max(0) as usize;
        let len = regs.i.get(1).copied().unwrap_or(0).max(0) as usize;
        let out: String = s.chars().skip(start).take(len).collect();
        let h = ctx.heap.intern_string(&out);
        if regs.s.len() > 1 {
            regs.s[1] = h;
        } else if let Some(slot) = regs.s.first_mut() {
            *slot = h;
        }
        Ok(())
    }

    fn string_format(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let fmt = ctx.heap.str(regs.s.first().copied().unwrap_or_default()).to_owned();
        let args = super::vararg_values(ctx, regs, 0, 0, 1);
        let out = super::format_printf(ctx.heap, &fmt, &args);
        let h = ctx.heap.intern_string(&out);
        if regs.s.len() > 1 {
            regs.s[1] = h;
        } else if let Some(slot) = regs.s.first_mut() {
            *slot = h;
        }
        Ok(())
    }

    fn players_at(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let pn = regs.i.first().copied().unwrap_or(0) as i32;
        // Actor ints are health, bISMONSTER, TID — `players.at` copies the host tid into TID.
        let mo = ctx.heap.new_object(0, 4, 1, 1);
        let cookie = (i64::from(mo.generation) << 32) | i64::from(mo.index);
        ctx.play.mo_player.insert(cookie, pn);
        let tid = ctx.play.player_tids.get(&pn).copied().unwrap_or(0);
        if let Some(ObjKind::Object { fields_i, .. }) = ctx.heap.get_mut(mo) {
            if fields_i.len() > 2 {
                fields_i[2] = i64::from(tid);
            }
        }
        let info = ctx.heap.new_object(0, 1, 1, 0);
        if let Some(ObjKind::Object { fields_a, .. }) = ctx.heap.get_mut(info) {
            if !fields_a.is_empty() {
                fields_a[0] = mo;
            }
        }
        if let Some(slot) = regs.a.first_mut() {
            *slot = info;
        }
        Ok(())
    }

    fn actor_set_sound_class(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let recv = regs.a.first().copied().unwrap_or(ObjHandle::NULL);
        let class = ctx
            .heap
            .str(regs.s.first().copied().unwrap_or_default())
            .to_owned();
        let cookie = (i64::from(recv.generation) << 32) | i64::from(recv.index);
        if let Some(pn) = ctx.play.mo_player.get(&cookie).copied() {
            ctx.play.sound_class.insert(pn, class);
        }
        Ok(())
    }

    fn actor_get_class_name(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let h = regs.a.first().copied().unwrap_or(ObjHandle::NULL);
        let name = match ctx.heap.get(h) {
            Some(ObjKind::Object { fields_s, .. }) => ctx
                .heap
                .str(fields_s.first().copied().unwrap_or_default())
                .to_owned(),
            _ => String::new(),
        };
        let out = ctx.heap.intern_string(&name);
        if let Some(slot) = regs.s.first_mut() {
            *slot = out;
        }
        if regs.s.len() > 1 {
            regs.s[1] = out;
        }
        Ok(())
    }

    fn string_eq(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let left = ctx.heap.str(regs.s.first().copied().unwrap_or_default());
        let right = ctx.heap.str(regs.s.get(1).copied().unwrap_or_default());
        let eq = left.eq_ignore_ascii_case(right);
        if let Some(slot) = regs.i.first_mut() {
            *slot = i64::from(eq);
        }
        Ok(())
    }

    fn string_to_int(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let s = ctx.heap.str(regs.s.first().copied().unwrap_or_default());
        let v = s.trim().parse::<i64>().unwrap_or(0);
        if let Some(slot) = regs.i.first_mut() {
            *slot = v;
        }
        Ok(())
    }

    fn actor_give_inventory(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        // ABI: a[0] = receiver handle (cookie in generation/index or host maps handle→id),
        // s[0] = item class name, i[0] = amount. Returns success in i[0].
        let recv = regs.a.first().copied().unwrap_or(ObjHandle::NULL);
        let item = ctx
            .heap
            .str(regs.s.first().copied().unwrap_or_default())
            .to_owned();
        let amount = regs.i.first().copied().unwrap_or(1) as i32;
        // Encode handle as cookie: (generation << 32) | index; host may ignore.
        let cookie = (i64::from(recv.generation) << 32) | i64::from(recv.index);
        ctx.play.grants.push((cookie, item, amount));
        let ok = 1;
        if let Some(slot) = regs.i.first_mut() {
            *slot = i64::from(ok);
        }
        Ok(())
    }

    fn actor_take_inventory(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        // The playsim applies negative inventory requests as TakeInventory.
        let recv = regs.a.first().copied().unwrap_or(ObjHandle::NULL);
        let item = ctx
            .heap
            .str(regs.s.first().copied().unwrap_or_default())
            .to_owned();
        let amount = regs.i.first().copied().unwrap_or(0) as i32;
        let cookie = (i64::from(recv.generation) << 32) | i64::from(recv.index);
        ctx.play
            .grants
            .push((cookie, item, amount.saturating_abs().saturating_neg()));
        if let Some(slot) = regs.i.first_mut() {
            *slot = 1;
        }
        Ok(())
    }

    /// `CVar.GetCVar` / `FindCVar` → object whose `GetInt` returns the value (ZS4).
    fn cvar_get_cvar(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let name_h = regs.s.first().copied().unwrap_or_default();
        let name = ctx.heap.str(name_h).to_owned();
        if name.is_empty() {
            if let Some(slot) = regs.a.first_mut() {
                *slot = ObjHandle::NULL;
            }
            return Ok(());
        }
        let v = ctx.play.cvar(&name);
        let def = ctx.play.cvar_default(&name);
        let float_value = ctx.play.cvar_float(&name);
        let float_default = ctx.play.cvar_default_float(&name);
        let cvar_type = ctx.play.cvar_type(&name);
        let current_string = ctx.play.cvar_string(&name);
        let default_string = ctx.play.cvar_default_string(&name);
        let current_string_h = ctx.heap.intern_string(&current_string);
        let default_string_h = ctx.heap.intern_string(&default_string);
        let h = ctx.heap.new_object(0, 6, 0, 3);
        if let Some(ObjKind::Object {
            fields_i, fields_s, ..
        }) = ctx.heap.get_mut(h)
        {
            if let Some(slot) = fields_i.first_mut() {
                *slot = i64::from(v);
            }
            if let Some(slot) = fields_i.get_mut(1) {
                *slot = i64::from(def);
            }
            if let Some(slot) = fields_i.get_mut(3) {
                *slot = i64::from(cvar_type);
            }
            if let Some(slot) = fields_i.get_mut(4) {
                *slot = float_value.to_bits() as i64;
            }
            if let Some(slot) = fields_i.get_mut(5) {
                *slot = float_default.to_bits() as i64;
            }
            if let Some(slot) = fields_s.first_mut() {
                *slot = name_h;
            }
            if let Some(slot) = fields_s.get_mut(1) {
                *slot = current_string_h;
            }
            if let Some(slot) = fields_s.get_mut(2) {
                *slot = default_string_h;
            }
        }
        if let Some(slot) = regs.a.first_mut() {
            *slot = h;
        }
        Ok(())
    }

    fn cvar_int_field(ctx: &ZvmCtx<'_>, h: ObjHandle, idx: usize) -> i64 {
        match ctx.heap.get(h) {
            Some(ObjKind::Object { fields_i, .. }) => fields_i.get(idx).copied().unwrap_or(0),
            _ => 0,
        }
    }

    fn cvar_stored_name(ctx: &ZvmCtx<'_>, h: ObjHandle) -> String {
        let sh = match ctx.heap.get(h) {
            Some(ObjKind::Object { fields_s, .. }) => fields_s.first().copied().unwrap_or_default(),
            _ => Default::default(),
        };
        ctx.heap.str(sh).to_owned()
    }

    /// `cvar.GetInt()` — reads the integer stored by [`cvar_get_cvar`] on `self` (a[0]).
    fn cvar_get_int(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let h = regs.a.first().copied().unwrap_or(ObjHandle::NULL);
        let v = cvar_int_field(ctx, h, 0);
        if let Some(slot) = regs.i.first_mut() {
            *slot = v;
        }
        Ok(())
    }

    fn cvar_set_description(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let h = regs.a.first().copied().unwrap_or(ObjHandle::NULL);
        let label = ctx
            .heap
            .str(regs.s.first().copied().unwrap_or_default())
            .to_owned();
        let name = match ctx.heap.get(h) {
            Some(ObjKind::Object { fields_s, .. }) => ctx
                .heap
                .str(fields_s.first().copied().unwrap_or_default())
                .to_owned(),
            _ => String::new(),
        };
        if !name.is_empty() {
            ctx.play.cvar_desc.insert(name, label);
        }
        Ok(())
    }

    fn cvar_get_default_int(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let h = regs.a.first().copied().unwrap_or(ObjHandle::NULL);
        write_num(regs, cvar_int_field(ctx, h, 1) as f64);
        Ok(())
    }

    fn cvar_get_float(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let h = regs.a.first().copied().unwrap_or(ObjHandle::NULL);
        let ty = cvar_int_field(ctx, h, 3);
        let bits = cvar_int_field(ctx, h, 4) as u64;
        let value = if ty == 2 {
            f64::from_bits(bits)
        } else {
            cvar_int_field(ctx, h, 0) as f64
        };
        write_num(regs, value);
        Ok(())
    }

    fn cvar_get_default_float(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let h = regs.a.first().copied().unwrap_or(ObjHandle::NULL);
        let ty = cvar_int_field(ctx, h, 3);
        let bits = cvar_int_field(ctx, h, 5) as u64;
        let value = if ty == 2 {
            f64::from_bits(bits)
        } else {
            cvar_int_field(ctx, h, 1) as f64
        };
        write_num(regs, value);
        Ok(())
    }

    fn object_user_string(ctx: &ZvmCtx<'_>, h: ObjHandle) -> Option<String> {
        let marked = match ctx.heap.get(h) {
            Some(ObjKind::Object { fields_i, .. }) => fields_i.get(2).copied() == Some(1),
            _ => false,
        };
        if !marked {
            return None;
        }
        let sh = match ctx.heap.get(h) {
            Some(ObjKind::Object { fields_s, .. }) => fields_s.first().copied().unwrap_or_default(),
            _ => Default::default(),
        };
        Some(ctx.heap.str(sh).to_owned())
    }

    fn cvar_get_string(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let h = regs.a.first().copied().unwrap_or(ObjHandle::NULL);
        if cvar_int_field(ctx, h, 3) == 3 {
            let text = match ctx.heap.get(h) {
                Some(ObjKind::Object { fields_s, .. }) => ctx
                    .heap
                    .str(fields_s.get(1).copied().unwrap_or_default())
                    .to_owned(),
                _ => String::new(),
            };
            store_string(ctx, regs, &text);
            return Ok(());
        }
        let value = match ctx.heap.get(h) {
            Some(ObjKind::Object { fields_s, .. }) => ctx
                .heap
                .str(fields_s.get(1).copied().unwrap_or_default())
                .to_owned(),
            _ => String::new(),
        };
        if !value.is_empty() {
            let text = value;
            store_string(ctx, regs, &text);
            return Ok(());
        }
        if let Some(text) = object_user_string(ctx, h) {
            store_string(ctx, regs, &text);
            return Ok(());
        }
        let name = cvar_stored_name(ctx, h).to_ascii_lowercase();
        let text = match name.as_str() {
            "name" | "playername" => "Player",
            "language" => "en",
            "playerclass" => "DoomPlayer",
            "gender" => "male",
            _ => "",
        };
        store_string(ctx, regs, text);
        Ok(())
    }

    fn cvar_set_int(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let h = regs.a.first().copied().unwrap_or(ObjHandle::NULL);
        let v = regs.i.first().copied().unwrap_or(0);
        let name = cvar_stored_name(ctx, h);
        let ty = cvar_int_field(ctx, h, 3);
        let text = ctx.heap.intern_string(&(v as i32).to_string());
        if let Some(ObjKind::Object {
            fields_i, fields_s, ..
        }) = ctx.heap.get_mut(h)
        {
            if let Some(slot) = fields_i.first_mut() {
                *slot = v;
            }
            if let Some(slot) = fields_i.get_mut(4) {
                *slot = (v as f64).to_bits() as i64;
            }
            if let Some(slot) = fields_s.get_mut(1) {
                *slot = text;
            }
        }
        if !name.is_empty() {
            match ty {
                2 => ctx.play.set_cvar_float(&name, v as f64),
                3 => ctx.play.set_cvar_string(&name, &(v as i32).to_string()),
                _ => ctx.play.set_cvar_int(&name, v as i32),
            }
        }
        Ok(())
    }

    fn cvar_set_float(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let h = regs.a.first().copied().unwrap_or(ObjHandle::NULL);
        let value = regs.f.first().copied().unwrap_or(0.0);
        let name = cvar_stored_name(ctx, h);
        let ty = cvar_int_field(ctx, h, 3);
        let converted = match ty {
            3 => value.to_string(),
            _ => (value as i32).to_string(),
        };
        let text = ctx.heap.intern_string(&converted);
        if let Some(ObjKind::Object {
            fields_i, fields_s, ..
        }) = ctx.heap.get_mut(h)
        {
            if let Some(slot) = fields_i.first_mut() {
                *slot = value as i32 as i64;
            }
            if let Some(slot) = fields_i.get_mut(4) {
                *slot = (if ty == 2 { value } else { value as i32 as f64 }).to_bits() as i64;
            }
            if let Some(slot) = fields_s.get_mut(1) {
                *slot = text;
            }
        }
        if !name.is_empty() {
            match ty {
                2 => ctx.play.set_cvar_float(&name, value),
                3 => ctx.play.set_cvar_string(&name, &converted),
                _ => ctx.play.set_cvar_int(&name, value as i32),
            }
        }
        Ok(())
    }

    fn cvar_set_string(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let h = regs.a.first().copied().unwrap_or(ObjHandle::NULL);
        let value = ctx
            .heap
            .str(regs.s.first().copied().unwrap_or_default())
            .to_owned();
        let name = cvar_stored_name(ctx, h);
        let ty = cvar_int_field(ctx, h, 3);
        let converted = match ty {
            2 => value.trim().parse::<f64>().unwrap_or(0.0).to_string(),
            3 => value.clone(),
            _ => value.trim().parse::<i32>().unwrap_or(0).to_string(),
        };
        let string = ctx.heap.intern_string(&converted);
        if let Some(ObjKind::Object {
            fields_i, fields_s, ..
        }) = ctx.heap.get_mut(h)
        {
            if let Some(slot) = fields_i.first_mut() {
                *slot = if ty == 2 {
                    converted.parse::<f64>().unwrap_or(0.0) as i32 as i64
                } else {
                    converted.parse::<i64>().unwrap_or(0)
                };
            }
            if let Some(slot) = fields_i.get_mut(4) {
                let float_value = if ty == 2 {
                    converted.parse::<f64>().unwrap_or(0.0)
                } else {
                    converted.parse::<i64>().unwrap_or(0) as f64
                };
                *slot = float_value.to_bits() as i64;
            }
            if let Some(slot) = fields_s.get_mut(1) {
                *slot = string;
            }
        }
        if !name.is_empty() {
            match ty {
                2 => ctx
                    .play
                    .set_cvar_float(&name, converted.parse().unwrap_or(0.0)),
                3 => ctx.play.set_cvar_string(&name, &converted),
                _ => ctx.play.set_cvar_int(&name, converted.parse().unwrap_or(0)),
            }
        }
        Ok(())
    }

    fn cvar_get_default_string(
        ctx: &mut ZvmCtx<'_>,
        regs: &mut NativeRegs<'_>,
    ) -> Result<(), Trap> {
        let h = regs.a.first().copied().unwrap_or(ObjHandle::NULL);
        let text = match ctx.heap.get(h) {
            Some(ObjKind::Object { fields_s, .. }) => ctx
                .heap
                .str(fields_s.get(2).copied().unwrap_or_default())
                .to_owned(),
            _ => String::new(),
        };
        store_string(ctx, regs, &text);
        Ok(())
    }

    fn cvar_real_type(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let h = regs.a.first().copied().unwrap_or(ObjHandle::NULL);
        write_num(regs, cvar_int_field(ctx, h, 3) as f64);
        Ok(())
    }

    fn cvar_reset_to_default(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let h = regs.a.first().copied().unwrap_or(ObjHandle::NULL);
        let name = cvar_stored_name(ctx, h);
        let value = ctx.play.cvar_default(&name);
        let ty = cvar_int_field(ctx, h, 3);
        let default_string = match ctx.heap.get(h) {
            Some(ObjKind::Object { fields_s, .. }) => fields_s.get(2).copied().unwrap_or_default(),
            _ => Default::default(),
        };
        let default_float = if ty == 2 {
            cvar_int_field(ctx, h, 5) as u64
        } else {
            f64::from(value).to_bits()
        };
        let default_text = ctx.heap.str(default_string).to_owned();
        let default_int = match ty {
            2 => f64::from(value) as i64,
            3 => default_text.trim().parse::<i64>().unwrap_or(0),
            _ => i64::from(value),
        };
        if let Some(ObjKind::Object {
            fields_i, fields_s, ..
        }) = ctx.heap.get_mut(h)
        {
            if let Some(slot) = fields_i.first_mut() {
                *slot = default_int;
            }
            if let Some(slot) = fields_i.get_mut(4) {
                *slot = default_float as i64;
            }
            if let Some(slot) = fields_s.get_mut(1) {
                *slot = default_string;
            }
        }
        if !name.is_empty() {
            match ty {
                2 => ctx
                    .play
                    .set_cvar_float(&name, f64::from_bits(default_float)),
                3 => ctx.play.set_cvar_string(&name, &default_text),
                _ => ctx.play.set_cvar_int(&name, value),
            }
        }
        if let Some(slot) = regs.i.first_mut() {
            *slot = 1;
        }
        Ok(())
    }

    fn font_find_color(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let name = ctx
            .heap
            .str(regs.s.first().copied().unwrap_or_default())
            .to_ascii_lowercase();
        let name = name.strip_prefix("cr_").unwrap_or(&name);
        let color = match name {
            "brick" => 0,
            "tan" => 1,
            "gray" | "grey" => 2,
            "green" => 3,
            "brown" => 4,
            "gold" => 5,
            "red" => 6,
            "blue" => 7,
            "orange" => 8,
            "white" => 9,
            "yellow" => 10,
            "untranslated" => 11,
            "black" => 12,
            "lightblue" => 13,
            "cream" => 14,
            "olive" => 15,
            "darkgreen" => 16,
            "darkred" => 17,
            "darkbrown" => 18,
            "purple" => 19,
            "darkgray" | "darkgrey" => 20,
            "cyan" => 21,
            "ice" => 22,
            "fire" => 23,
            "sapphire" => 24,
            "teal" => 25,
            _ => -1,
        };
        write_num(regs, color as f64);
        Ok(())
    }

    fn font_cursor(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        store_string(ctx, regs, "_");
        Ok(())
    }

    fn font_displacement(_ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        write_num(regs, 8.0);
        Ok(())
    }

    /// `EventHandler.Find("Class")` → registered handler instance (a[0]).
    fn event_handler_find(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let name = ctx
            .heap
            .str(regs.s.first().copied().unwrap_or_default())
            .to_owned();
        let h = ctx.play.find_handler(&name);
        if let Some(slot) = regs.a.first_mut() {
            *slot = h;
        }
        Ok(())
    }

    /// `Object.New("Class")` — allocate a script instance using its class layout.
    fn object_new(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let name = ctx
            .heap
            .str(regs.s.first().copied().unwrap_or_default())
            .to_ascii_lowercase();
        let h = if let Some((class_id, n_i, n_a, n_s)) = ctx.play.object_layouts.get(&name).copied()
        {
            ctx.heap.new_object(class_id, n_i, n_a, n_s)
        } else {
            // Keep the prior soft behavior for DECORATE/native classes that
            // are not present in the compiled ZScript class table.
            ctx.heap.new_object(0, 8, 4, 4)
        };
        if let Some(slot) = regs.a.first_mut() {
            *slot = h;
        }
        Ok(())
    }

    fn actor_change_tid(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let recv = regs.a.first().copied().unwrap_or(ObjHandle::NULL);
        let tid = regs.i.first().copied().unwrap_or(0) as i32;
        let cookie = (i64::from(recv.generation) << 32) | i64::from(recv.index);
        if let Some(pn) = ctx.play.mo_player.get(&cookie).copied() {
            ctx.play.player_tids.insert(pn, tid);
        }
        ctx.play.tid_changes.push((cookie, tid));
        let ok = 1;
        if let Some(slot) = regs.i.first_mut() {
            *slot = i64::from(ok);
        }
        Ok(())
    }

    fn player_in_game(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let pn = regs.i.first().copied().unwrap_or(0) as i32;
        let v = ctx.play.player_in_game(pn);
        if let Some(slot) = regs.i.first_mut() {
            *slot = i64::from(v);
        }
        Ok(())
    }

    /// `PlayerInfo.GetGender()` — CVar `gender` via host (0=male default).
    fn player_get_gender(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let v = ctx.play.cvar("gender");
        if let Some(slot) = regs.i.first_mut() {
            *slot = i64::from(v);
        }
        Ok(())
    }

    fn get_player_tid(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let pn = regs.i.first().copied().unwrap_or(0) as i32;
        let v = ctx.play.player_tids.get(&pn).copied().unwrap_or(0);
        if let Some(slot) = regs.i.first_mut() {
            *slot = i64::from(v);
        }
        Ok(())
    }

    fn set_player_tid(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        let pn = regs.i.first().copied().unwrap_or(0) as i32;
        let tid = regs.i.get(1).copied().unwrap_or(0) as i32;
        ctx.play.player_tids.insert(pn, tid);
        let ok = 1;
        if let Some(slot) = regs.i.first_mut() {
            *slot = i64::from(ok);
        }
        Ok(())
    }

    fn stub_zero(_ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
        if let Some(slot) = regs.i.first_mut() {
            *slot = 0;
        }
        Ok(())
    }
}

pub use play_natives::{ZvmPlayHost, register_bd_natives};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::heap::{ObjHandle, StrHandle, ZvmHeap};

    #[test]
    fn texture_ids_intern_by_name() {
        let mut play = ZvmPlayHost::default();
        assert_eq!(play.intern_texture(""), -1);
        assert_eq!(play.intern_texture("-"), 0);
        let a = play.intern_texture("BAL1A0");
        assert!(a > 0);
        assert_eq!(play.intern_texture("bal1a0"), a);
        assert_ne!(play.intern_texture("PUFFA0"), a);
        assert_eq!(play.texture_name(a), Some("BAL1A0"));
        assert_eq!(play.texture_name(0), None);
        assert_eq!(play.texture_name(-1), None);
    }

    #[test]
    fn bind_imports_keeps_real_natives_and_traps_stubs() {
        let mut table = NativeTable::with_builtins();
        let destroy = table.id_ci("Object.Destroy").expect("destroy");
        let added = table.bind_imports(["Object.Destroy", "Actor.A_FreezeDeath"]);
        assert_eq!(added, 1);
        assert_eq!(table.id_ci("Object.Destroy"), Some(destroy));
        assert_eq!(table.bind_imports(["actor.a_freezedeath"]), 0);
        let id = table.id_ci("Actor.A_FreezeDeath").expect("stub");
        let func = table.get(id).expect("fn");
        let mut heap = ZvmHeap::new();
        let mut play = ZvmPlayHost::default();
        let mut ctx = ZvmCtx {
            heap: &mut heap,
            host: 0,
            play: &mut play,
        };
        let mut i = [0i64; 1];
        let mut f = [0.0; 1];
        let mut s = [StrHandle::EMPTY; 1];
        let mut a = [ObjHandle::NULL; 1];
        let mut regs = NativeRegs {
            i: &mut i,
            f: &mut f,
            s: &mut s,
            a: &mut a,
        };
        func(&mut ctx, &mut regs).expect("stub returns 0");
    }

    fn call_math(name: &str, args: &[f64]) -> f64 {
        let table = NativeTable::with_builtins();
        let id = table
            .id_ci(name)
            .unwrap_or_else(|| panic!("missing {name}"));
        let func = table.get(id).expect("fn");
        let mut heap = ZvmHeap::new();
        let mut play = ZvmPlayHost::default();
        play.native_name = name.to_owned();
        let mut ctx = ZvmCtx {
            heap: &mut heap,
            host: 0,
            play: &mut play,
        };
        let mut i = [0i64; 4];
        let mut f = [0.0; 4];
        for (n, v) in args.iter().enumerate() {
            f[n] = *v;
        }
        let mut s = [StrHandle::EMPTY; 1];
        let mut a = [ObjHandle::NULL; 1];
        let mut regs = NativeRegs {
            i: &mut i,
            f: &mut f,
            s: &mut s,
            a: &mut a,
        };
        func(&mut ctx, &mut regs).expect("math");
        f[0]
    }

    #[test]
    fn math_natives_use_degrees_and_are_deterministic() {
        assert_eq!(call_math("math.abs", &[-3.5]), 3.5);
        assert_eq!(call_math("math.min", &[3.0, 1.0]), 1.0);
        assert_eq!(call_math("math.max", &[3.0, 1.0]), 3.0);
        assert_eq!(call_math("math.clamp", &[5.0, 0.0, 2.0]), 2.0);
        assert_eq!(call_math("math.pow", &[2.0, 3.0]), 8.0);
        assert_eq!(call_math("math.ceil", &[1.2]), 2.0);
        assert_eq!(call_math("math.round", &[1.5]), 2.0);
        assert_eq!(call_math("math.int", &[-1.7]), -1.0);
        assert_eq!(call_math("math.frandom", &[1.0, 3.0]), 3.0);
        assert!((call_math("math.sin", &[90.0]) - 1.0).abs() < 1e-9);
        assert!((call_math("math.atan2", &[1.0, 0.0]) - 90.0).abs() < 1e-6);
        assert!((call_math("math.vectorangle", &[0.0, 1.0]) - 90.0).abs() < 1e-6);
    }

    fn call_string(name: &str, text: &str, extra: &[&str], ints: &[i64]) -> String {
        let table = NativeTable::with_builtins();
        let id = table
            .id_ci(name)
            .unwrap_or_else(|| panic!("missing {name}"));
        let func = table.get(id).expect("fn");
        let mut heap = ZvmHeap::new();
        let mut play = ZvmPlayHost::default();
        let mut s = [StrHandle::EMPTY; 4];
        s[0] = heap.intern_string(text);
        for (n, part) in extra.iter().enumerate() {
            s[n + 1] = heap.intern_string(part);
        }
        let mut ctx = ZvmCtx {
            heap: &mut heap,
            host: 0,
            play: &mut play,
        };
        let mut i = [0i64; 4];
        for (n, v) in ints.iter().enumerate() {
            i[n] = *v;
        }
        let mut f = [0.0; 1];
        let mut a = [ObjHandle::NULL; 1];
        let mut regs = NativeRegs {
            i: &mut i,
            f: &mut f,
            s: &mut s,
            a: &mut a,
        };
        func(&mut ctx, &mut regs).expect("string");
        ctx.heap.str(s[1]).to_owned()
    }

    #[test]
    fn string_left_lower_replace_and_filter() {
        assert_eq!(call_string("String.Left", "Hello", &[], &[2]), "He");
        assert_eq!(call_string("String.MakeLower", "AbC", &[], &[]), "abc");
        assert_eq!(
            call_string("String.Replace", "a-b-a", &["-", "+"], &[]),
            "a+b+a"
        );
        assert_eq!(call_string("String.Filter", "A\u{1c}cHi", &[], &[]), "AHi");
        let table = NativeTable::with_builtins();
        let id = table.id_ci("String.CharLower").expect("charlower");
        let func = table.get(id).expect("fn");
        let mut heap = ZvmHeap::new();
        let mut play = ZvmPlayHost::default();
        let mut ctx = ZvmCtx {
            heap: &mut heap,
            host: 0,
            play: &mut play,
        };
        let mut i = [65i64];
        let mut f = [0.0];
        let mut s = [StrHandle::EMPTY];
        let mut a = [ObjHandle::NULL];
        let mut regs = NativeRegs {
            i: &mut i,
            f: &mut f,
            s: &mut s,
            a: &mut a,
        };
        func(&mut ctx, &mut regs).expect("charlower");
        assert_eq!(i[0], i64::from(b'a'));
    }

    #[test]
    fn string_split_keeps_or_skips_empty_tokens() {
        let table = NativeTable::with_builtins();
        let id = table.id_ci("String.Split").expect("split");
        let func = table.get(id).expect("fn");
        let mut heap = ZvmHeap::new();
        let mut play = ZvmPlayHost::default();
        let arr = heap.new_array_s();
        let mut s = [StrHandle::EMPTY; 2];
        s[0] = heap.intern_string("a  b");
        s[1] = heap.intern_string(" ");
        let mut ctx = ZvmCtx {
            heap: &mut heap,
            host: 0,
            play: &mut play,
        };
        let mut i = [0i64; 1];
        let mut f = [0.0; 1];
        let mut a = [arr];
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            func(&mut ctx, &mut regs).expect("split");
        }
        let skipped = array_strings(&ctx.heap, arr);
        assert_eq!(skipped, ["a", "b"]);

        s[0] = ctx.heap.intern_string("a\nb\n");
        s[1] = ctx.heap.intern_string("\n");
        i[0] = 1;
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            func(&mut ctx, &mut regs).expect("split keep");
        }
        let kept = array_strings(&ctx.heap, arr);
        assert_eq!(kept, ["a", "b", ""]);
    }

    #[test]
    fn setup_crouch_sprite_fails_so_the_pawn_clears_it() {
        let table = NativeTable::with_builtins();
        let id = table.id_ci("playerpawn.SetupCrouchSprite").expect("crouch");
        let func = table.get(id).expect("fn");
        let mut heap = ZvmHeap::new();
        let mut play = ZvmPlayHost::default();
        let mut ctx = ZvmCtx {
            heap: &mut heap,
            host: 0,
            play: &mut play,
        };
        let mut i = [5i64; 1];
        let mut f = [0.0; 1];
        let mut s = [StrHandle::EMPTY; 1];
        let mut a = [ObjHandle::NULL; 1];
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            func(&mut ctx, &mut regs).expect("crouch");
        }
        assert_eq!(i[0], 0);
    }

    #[test]
    fn name_all_keys_joins_escape_and_enter() {
        let table = NativeTable::with_builtins();
        let id = table.id_ci("keybindings.nameallkeys").expect("nameallkeys");
        let func = table.get(id).expect("fn");
        let mut heap = ZvmHeap::new();
        let mut play = ZvmPlayHost::default();
        let empty = heap.new_array_i();
        let keys = heap.new_array_i();
        if let Some(crate::heap::ObjKind::ArrayI(items)) = heap.get_mut(keys) {
            items.extend([0x01, 0x1c]);
        }
        let mut ctx = ZvmCtx {
            heap: &mut heap,
            host: 0,
            play: &mut play,
        };
        let mut i = [0i64; 1];
        let mut f = [0.0; 1];
        let mut s = [StrHandle::EMPTY; 2];
        let mut a = [empty];
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            func(&mut ctx, &mut regs).expect("empty keys");
        }
        assert_eq!(ctx.heap.str(s[1]), "");
        a[0] = keys;
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            func(&mut ctx, &mut regs).expect("keys");
        }
        assert_eq!(ctx.heap.str(s[1]), "Escape or Enter");
    }

    #[test]
    fn doom_has_six_key_types() {
        let table = NativeTable::with_builtins();
        let count = table
            .get(table.id_ci("key.getkeytypecount").expect("count"))
            .expect("fn");
        let kind = table
            .get(table.id_ci("key.getkeytype").expect("type"))
            .expect("fn");
        let mut heap = ZvmHeap::new();
        let mut play = ZvmPlayHost::default();
        let mut ctx = ZvmCtx {
            heap: &mut heap,
            host: 0,
            play: &mut play,
        };
        let mut i = [9i64; 1];
        let mut f = [0.0; 1];
        let mut s = [StrHandle::EMPTY; 2];
        let mut a = [ObjHandle::NULL; 1];
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            count(&mut ctx, &mut regs).expect("count");
        }
        assert_eq!(i[0], 6);
        i[0] = 2;
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            kind(&mut ctx, &mut regs).expect("red card");
        }
        assert_eq!(ctx.heap.str(s[1]), "RedCard");
        assert_ne!(a[0], ObjHandle::NULL);
        i[0] = 9;
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            kind(&mut ctx, &mut regs).expect("missing");
        }
        assert_eq!(i[0], -1);
        assert_eq!(a[0], ObjHandle::NULL);
    }

    #[test]
    fn set_inter_music_records_the_next_map() {
        let table = NativeTable::with_builtins();
        let func = table
            .get(table.id_ci("levellocals.setintermusic").expect("music"))
            .expect("fn");
        let mut heap = ZvmHeap::new();
        let mut play = ZvmPlayHost::default();
        let mut s = [StrHandle::EMPTY; 1];
        s[0] = heap.intern_string("MAP02");
        let mut ctx = ZvmCtx {
            heap: &mut heap,
            host: 0,
            play: &mut play,
        };
        let mut i = [0i64; 1];
        let mut f = [0.0; 1];
        let mut a = [ObjHandle::NULL; 1];
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            func(&mut ctx, &mut regs).expect("music");
        }
        assert_eq!(ctx.play.inter_music, "MAP02");
    }

    #[test]
    fn set_cvar_description_stores_the_menu_label() {
        let table = NativeTable::with_builtins();
        let func = table
            .get(
                table
                    .id_ci("optionmenuitemoption.setcvardescription")
                    .expect("desc"),
            )
            .expect("fn");
        let mut heap = ZvmHeap::new();
        let mut play = ZvmPlayHost::default();
        let cv = heap.new_object(0, 2, 0, 1);
        let name = heap.intern_string("m_sensitivity");
        if let Some(crate::heap::ObjKind::Object { fields_s, .. }) = heap.get_mut(cv) {
            fields_s[0] = name;
        }
        let mut s = [StrHandle::EMPTY; 1];
        s[0] = heap.intern_string("Mouse");
        let mut ctx = ZvmCtx {
            heap: &mut heap,
            host: 0,
            play: &mut play,
        };
        let mut i = [0i64; 1];
        let mut f = [0.0; 1];
        let mut a = [ObjHandle::NULL];
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            func(&mut ctx, &mut regs).expect("null cvar");
        }
        assert!(ctx.play.cvar_desc.is_empty());
        a[0] = cv;
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            func(&mut ctx, &mut regs).expect("label");
        }
        assert_eq!(
            ctx.play.cvar_desc.get("m_sensitivity").map(String::as_str),
            Some("Mouse")
        );
    }

    #[test]
    fn player_name_changed_sets_the_user_name() {
        let table = NativeTable::with_builtins();
        let set = table
            .get(table.id_ci("playermenu.playernamechanged").expect("set"))
            .expect("fn");
        let get = table
            .get(table.id_ci("PlayerInfo.GetUserName").expect("get"))
            .expect("fn");
        let mut heap = ZvmHeap::new();
        let mut play = ZvmPlayHost::default();
        let mut ctx = ZvmCtx {
            heap: &mut heap,
            host: 0,
            play: &mut play,
        };
        let mut i = [0i64; 1];
        let mut f = [0.0; 1];
        let mut s = [StrHandle::EMPTY; 2];
        let mut a = [ObjHandle::NULL; 1];
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            get(&mut ctx, &mut regs).expect("empty name");
        }
        assert_eq!(ctx.heap.str(s[1]), "");
        s[0] = ctx.heap.intern_string("Aly");
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            set(&mut ctx, &mut regs).expect("set name");
        }
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            get(&mut ctx, &mut regs).expect("read name");
        }
        assert_eq!(ctx.heap.str(s[1]), "Aly");
    }

    #[test]
    fn gender_changed_is_what_get_gender_reads() {
        let table = NativeTable::with_builtins();
        let set = table
            .get(table.id_ci("playermenu.genderchanged").expect("set"))
            .expect("fn");
        let get = table
            .get(table.id_ci("PlayerInfo.GetGender").expect("get"))
            .expect("fn");
        let mut heap = ZvmHeap::new();
        let mut play = ZvmPlayHost::default();
        let mut ctx = ZvmCtx {
            heap: &mut heap,
            host: 0,
            play: &mut play,
        };
        let mut i = [0i64; 1];
        let mut f = [0.0; 1];
        let mut s = [StrHandle::EMPTY; 1];
        let mut a = [ObjHandle::NULL; 1];
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            get(&mut ctx, &mut regs).expect("default gender");
        }
        assert_eq!(i[0], 0);
        i[0] = 2;
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            set(&mut ctx, &mut regs).expect("set gender");
        }
        i[0] = 9;
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            get(&mut ctx, &mut regs).expect("read gender");
        }
        assert_eq!(i[0], 2);
    }

    #[test]
    fn always_run_changed_is_what_cl_run_reads() {
        let table = NativeTable::with_builtins();
        let set = table
            .get(table.id_ci("playermenu.alwaysrunchanged").expect("set"))
            .expect("fn");
        let get = table.get(table.id_ci("cl_run").expect("run")).expect("fn");
        let mut heap = ZvmHeap::new();
        let mut play = ZvmPlayHost::default();
        let mut ctx = ZvmCtx {
            heap: &mut heap,
            host: 0,
            play: &mut play,
        };
        let mut i = [0i64; 1];
        let mut f = [1.0; 1];
        let mut s = [StrHandle::EMPTY; 1];
        let mut a = [ObjHandle::NULL; 1];
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            get(&mut ctx, &mut regs).expect("default run");
        }
        assert_eq!(i[0], 1);
        i[0] = 0;
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            set(&mut ctx, &mut regs).expect("walk");
        }
        i[0] = 9;
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            get(&mut ctx, &mut regs).expect("read run");
        }
        assert_eq!(i[0], 0);
    }

    #[test]
    fn switch_on_pickup_changed_is_what_never_switch_reads() {
        let table = NativeTable::with_builtins();
        let set = table
            .get(
                table
                    .id_ci("playermenu.switchonpickupchanged")
                    .expect("set"),
            )
            .expect("fn");
        let get = table
            .get(table.id_ci("PlayerInfo.GetNeverSwitch").expect("get"))
            .expect("fn");
        let mut heap = ZvmHeap::new();
        let mut play = ZvmPlayHost::default();
        let mut ctx = ZvmCtx {
            heap: &mut heap,
            host: 0,
            play: &mut play,
        };
        let mut i = [9i64; 1];
        let mut f = [0.0; 1];
        let mut s = [StrHandle::EMPTY; 1];
        let mut a = [ObjHandle::NULL; 1];
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            get(&mut ctx, &mut regs).expect("default switch");
        }
        assert_eq!(i[0], 0);
        i[0] = 1;
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            set(&mut ctx, &mut regs).expect("never switch");
        }
        i[0] = 9;
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            get(&mut ctx, &mut regs).expect("read switch");
        }
        assert_eq!(i[0], 1);
    }

    #[test]
    fn autoaim_changed_is_what_get_autoaim_reads() {
        let table = NativeTable::with_builtins();
        let set = table
            .get(table.id_ci("playermenu.autoaimchanged").expect("set"))
            .expect("fn");
        let get = table
            .get(table.id_ci("PlayerInfo.GetAutoaim").expect("get"))
            .expect("fn");
        let mut heap = ZvmHeap::new();
        let mut play = ZvmPlayHost::default();
        let mut ctx = ZvmCtx {
            heap: &mut heap,
            host: 0,
            play: &mut play,
        };
        let mut i = [0i64; 1];
        let mut f = [0.0; 1];
        let mut s = [StrHandle::EMPTY; 1];
        let mut a = [ObjHandle::NULL; 1];
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            get(&mut ctx, &mut regs).expect("default aim");
        }
        assert_eq!(i[0], 35);
        f[0] = 20.0;
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            set(&mut ctx, &mut regs).expect("set aim");
        }
        i[0] = 9;
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            get(&mut ctx, &mut regs).expect("read aim");
        }
        assert_eq!(i[0], 20);
    }

    #[test]
    fn color_changed_is_what_get_color_reads() {
        let table = NativeTable::with_builtins();
        let set = table
            .get(table.id_ci("playermenu.colorchanged").expect("set"))
            .expect("fn");
        let get = table
            .get(table.id_ci("PlayerInfo.GetColor").expect("get"))
            .expect("fn");
        let mut heap = ZvmHeap::new();
        let mut play = ZvmPlayHost::default();
        let mut ctx = ZvmCtx {
            heap: &mut heap,
            host: 0,
            play: &mut play,
        };
        let mut i = [0i64; 3];
        let mut f = [0.0; 1];
        let mut s = [StrHandle::EMPTY; 2];
        let mut a = [ObjHandle::NULL; 1];
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            get(&mut ctx, &mut regs).expect("default color");
        }
        assert_eq!(ctx.heap.str(s[1]), "Green");
        i = [10, 20, 30];
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            set(&mut ctx, &mut regs).expect("set color");
        }
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            get(&mut ctx, &mut regs).expect("read color");
        }
        assert_eq!(ctx.heap.str(s[1]), "10 20 30");
    }

    #[test]
    fn class_changed_is_what_get_player_class_num_reads() {
        let table = NativeTable::with_builtins();
        let set = table
            .get(table.id_ci("playermenu.classchanged").expect("set"))
            .expect("fn");
        let get = table
            .get(table.id_ci("PlayerInfo.GetPlayerClassNum").expect("get"))
            .expect("fn");
        let mut heap = ZvmHeap::new();
        let mut play = ZvmPlayHost::default();
        let mut ctx = ZvmCtx {
            heap: &mut heap,
            host: 0,
            play: &mut play,
        };
        let mut i = [9i64; 1];
        let mut f = [0.0; 1];
        let mut s = [StrHandle::EMPTY; 1];
        let mut a = [ObjHandle::NULL; 1];
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            get(&mut ctx, &mut regs).expect("default class");
        }
        assert_eq!(i[0], 0);
        i[0] = 3;
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            set(&mut ctx, &mut regs).expect("set class");
        }
        i[0] = 9;
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            get(&mut ctx, &mut regs).expect("read class");
        }
        assert_eq!(i[0], 3);
    }

    #[test]
    fn team_changed_is_what_get_team_reads() {
        let table = NativeTable::with_builtins();
        let set = table
            .get(table.id_ci("playermenu.teamchanged").expect("set"))
            .expect("fn");
        let get = table
            .get(table.id_ci("playerinfo.getteam").expect("get"))
            .expect("fn");
        let mut heap = ZvmHeap::new();
        let mut play = ZvmPlayHost::default();
        let mut ctx = ZvmCtx {
            heap: &mut heap,
            host: 0,
            play: &mut play,
        };
        let mut i = [9i64; 1];
        let mut f = [0.0; 1];
        let mut s = [StrHandle::EMPTY; 1];
        let mut a = [ObjHandle::NULL; 1];
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            get(&mut ctx, &mut regs).expect("default team");
        }
        assert_eq!(i[0], 0);
        i[0] = 2;
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            set(&mut ctx, &mut regs).expect("set team");
        }
        i[0] = 9;
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            get(&mut ctx, &mut regs).expect("read team");
        }
        assert_eq!(i[0], 1);
    }

    #[test]
    fn color_set_changed_is_what_get_color_set_reads() {
        let table = NativeTable::with_builtins();
        let set = table
            .get(table.id_ci("playermenu.colorsetchanged").expect("set"))
            .expect("fn");
        let get = table
            .get(table.id_ci("playerinfo.getcolorset").expect("get"))
            .expect("fn");
        let mut heap = ZvmHeap::new();
        let mut play = ZvmPlayHost::default();
        let mut ctx = ZvmCtx {
            heap: &mut heap,
            host: 0,
            play: &mut play,
        };
        let mut i = [9i64; 1];
        let mut f = [0.0; 1];
        let mut s = [StrHandle::EMPTY; 1];
        let mut a = [ObjHandle::NULL; 1];
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            get(&mut ctx, &mut regs).expect("default set");
        }
        assert_eq!(i[0], 0);
        i[0] = -1;
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            set(&mut ctx, &mut regs).expect("set custom");
        }
        i[0] = 9;
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            get(&mut ctx, &mut regs).expect("read set");
        }
        assert_eq!(i[0], -1);
    }

    #[test]
    fn skin_changed_is_what_get_skin_reads() {
        let table = NativeTable::with_builtins();
        let set = table
            .get(table.id_ci("playermenu.skinchanged").expect("set"))
            .expect("fn");
        let get = table
            .get(table.id_ci("PlayerInfo.GetSkin").expect("get"))
            .expect("fn");
        let mut heap = ZvmHeap::new();
        let mut play = ZvmPlayHost::default();
        let mut ctx = ZvmCtx {
            heap: &mut heap,
            host: 0,
            play: &mut play,
        };
        let mut i = [9i64; 1];
        let mut f = [0.0; 1];
        let mut s = [StrHandle::EMPTY; 2];
        let mut a = [ObjHandle::NULL; 1];
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            get(&mut ctx, &mut regs).expect("default skin");
        }
        assert_eq!(ctx.heap.str(s[1]), "base");
        i[0] = 0;
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            set(&mut ctx, &mut regs).expect("set base");
        }
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            get(&mut ctx, &mut regs).expect("read base");
        }
        assert_eq!(ctx.heap.str(s[1]), "base");
        i[0] = 2;
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            set(&mut ctx, &mut regs).expect("set skin");
        }
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            get(&mut ctx, &mut regs).expect("read skin");
        }
        assert_eq!(ctx.heap.str(s[1]), "2");
    }

    fn array_strings(heap: &ZvmHeap, arr: ObjHandle) -> Vec<String> {
        match heap.get(arr) {
            Some(crate::heap::ObjKind::ArrayS(items)) => {
                items.iter().map(|h| heap.str(*h).to_owned()).collect()
            }
            _ => panic!("string array"),
        }
    }

    #[test]
    fn string_indexof_localize_and_stringwidth() {
        let table = NativeTable::with_builtins();
        let id = table.id_ci("String.IndexOf").expect("indexof");
        let func = table.get(id).expect("fn");
        let mut heap = ZvmHeap::new();
        let mut play = ZvmPlayHost::default();
        let mut s = [StrHandle::EMPTY; 4];
        s[0] = heap.intern_string("hello");
        s[1] = heap.intern_string("ll");
        let mut ctx = ZvmCtx {
            heap: &mut heap,
            host: 0,
            play: &mut play,
        };
        let mut i = [0i64; 4];
        let mut f = [0.0; 1];
        let mut a = [ObjHandle::NULL; 1];
        let mut regs = NativeRegs {
            i: &mut i,
            f: &mut f,
            s: &mut s,
            a: &mut a,
        };
        func(&mut ctx, &mut regs).expect("indexof");
        drop(regs);
        assert_eq!(i[0], 2);
        drop(ctx);

        assert_eq!(
            call_string("StringTable.Localize", "$TXT_YES", &[], &[]),
            "$TXT_YES"
        );

        let width = table.id_ci("font.StringWidth").expect("width");
        let width_fn = table.get(width).expect("fn");
        let mut heap = ZvmHeap::new();
        let mut play = ZvmPlayHost::default();
        let mut s = [StrHandle::EMPTY; 4];
        s[0] = heap.intern_string("Hi");
        let mut ctx = ZvmCtx {
            heap: &mut heap,
            host: 0,
            play: &mut play,
        };
        let mut i = [0i64; 4];
        let mut f = [0.0; 1];
        let mut a = [ObjHandle::NULL; 1];
        let mut regs = NativeRegs {
            i: &mut i,
            f: &mut f,
            s: &mut s,
            a: &mut a,
        };
        width_fn(&mut ctx, &mut regs).expect("width");
        drop(regs);
        assert_eq!(i[0], 16);
    }

    fn call_font_int(name: &str, text: &str) -> i64 {
        let table = NativeTable::with_builtins();
        let id = table
            .id_ci(name)
            .unwrap_or_else(|| panic!("missing {name}"));
        let func = table.get(id).expect("fn");
        let mut heap = ZvmHeap::new();
        let mut play = ZvmPlayHost::default();
        let mut s = [StrHandle::EMPTY; 2];
        s[0] = heap.intern_string(text);
        let mut ctx = ZvmCtx {
            heap: &mut heap,
            host: 0,
            play: &mut play,
        };
        let mut i = [0i64; 2];
        let mut f = [0.0; 1];
        let mut a = [ObjHandle::NULL; 1];
        let mut regs = NativeRegs {
            i: &mut i,
            f: &mut f,
            s: &mut s,
            a: &mut a,
        };
        func(&mut ctx, &mut regs).expect("font");
        drop(regs);
        i[0]
    }

    #[test]
    fn font_metrics_are_fixed_and_canprint_rejects_controls() {
        assert_eq!(call_font_int("font.GetCharWidth", ""), 8);
        assert_eq!(call_font_int("font.GetHeight", ""), 16);
        assert_eq!(call_font_int("font.GetMaxAscender", ""), 12);
        assert_eq!(call_font_int("font.CanPrint", "Hi"), 1);
        assert_eq!(call_font_int("font.CanPrint", "A\u{1}B"), 0);
        assert_eq!(call_font_int("font.IsValidDynamicFont", "BigFont"), 0);
    }

    #[test]
    fn font_find_returns_an_object() {
        let table = NativeTable::with_builtins();
        let id = table.id_ci("Font.FindFont").expect("find");
        let func = table.get(id).expect("fn");
        let mut heap = ZvmHeap::new();
        let mut play = ZvmPlayHost::default();
        let mut ctx = ZvmCtx {
            heap: &mut heap,
            host: 0,
            play: &mut play,
        };
        let mut i = [0i64; 1];
        let mut f = [0.0; 1];
        let mut s = [StrHandle::EMPTY; 1];
        s[0] = ctx.heap.intern_string("BigFont");
        let mut a = [ObjHandle::NULL; 1];
        let mut regs = NativeRegs {
            i: &mut i,
            f: &mut f,
            s: &mut s,
            a: &mut a,
        };
        func(&mut ctx, &mut regs).expect("find");
        assert!(!a[0].is_null(), "FindFont should return a font object");
        assert!(table.id_ci("Font.GetConsoleFont").is_some());
        assert!(table.id_ci("font.BreakLines").is_some());
    }

    #[test]
    fn screen_aspect_and_player_defaults() {
        let table = NativeTable::with_builtins();
        let aspect = table.id_ci("Screen.GetAspectRatio").expect("aspect");
        let func = table.get(aspect).expect("fn");
        let mut heap = ZvmHeap::new();
        let mut play = ZvmPlayHost::default();
        let mut ctx = ZvmCtx {
            heap: &mut heap,
            host: 0,
            play: &mut play,
        };
        let mut i = [0i64; 2];
        let mut f = [0.0; 2];
        let mut s = [StrHandle::EMPTY; 2];
        let mut a = [ObjHandle::NULL; 1];
        let mut regs = NativeRegs {
            i: &mut i,
            f: &mut f,
            s: &mut s,
            a: &mut a,
        };
        func(&mut ctx, &mut regs).expect("aspect");
        drop(regs);
        drop(ctx);
        assert!((f[0] - 4.0 / 3.0).abs() < 1e-9);

        assert_eq!(call_font_int("brokenlines.count", ""), 0);
        assert_eq!(call_font_int("PlayerInfo.GetNeverSwitch", ""), 0);
        assert_eq!(call_font_int("PlayerInfo.GetAutoaim", ""), 35);
        assert_eq!(call_string("PlayerInfo.GetSkin", "", &[], &[]), "base");
        assert_eq!(call_string("brokenlines.stringat", "", &[], &[]), "");
        assert_eq!(call_font_int("levellocals.IsJumpingAllowed", ""), 1);
        assert_eq!(call_font_int("levellocals.IsFreelookAllowed", ""), 1);
        assert_eq!(call_font_int("levellocals.IsCrouchingAllowed", ""), 1);
        assert_eq!(call_font_int("levellocals.IsPointInLevel", ""), 1);
        assert_eq!(call_font_int("levellocals.FindUniqueTid", ""), 1);
        assert_eq!(call_font_int("basestatusbar.GetTopOfStatusbar", ""), 168);
        assert_eq!(call_font_int("PlayerInfo.GetWBobSpeed", ""), 1);
        assert_eq!(call_string("PlayerInfo.GetColor", "", &[], &[]), "Green");
        assert_eq!(
            call_string("levellocals.FormatMapName", "E1M1", &[], &[]),
            "E1M1"
        );
        assert_eq!(
            call_string("levellocals.FormatMapName", "", &[], &[]),
            "MAP01"
        );
        assert_eq!(call_font_int("screen.GetWidth", ""), 320);
        assert_eq!(call_font_int("screen.GetHeight", ""), 200);
        assert_eq!(
            call_string("playerpawn.GetPrintableDisplayName", "", &[], &[]),
            "Player"
        );
    }

    #[test]
    fn next_player_walks_one_console_player() {
        let table = NativeTable::with_builtins();
        let num = table.id_ci("PlayerInfo.GetNextPlayerNumber").expect("num");
        let func = table.get(num).expect("fn");
        let mut heap = ZvmHeap::new();
        let mut play = ZvmPlayHost::default();
        let mut ctx = ZvmCtx {
            heap: &mut heap,
            host: 0,
            play: &mut play,
        };
        let mut i = [-1i64, 0, 0, 0];
        let mut f = [0.0; 4];
        let mut s = [StrHandle::EMPTY; 1];
        {
            let mut a = [ObjHandle::NULL; 1];
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            func(&mut ctx, &mut regs).expect("first");
        }
        assert_eq!(i[0], 0);
        {
            let mut a = [ObjHandle::NULL; 1];
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            func(&mut ctx, &mut regs).expect("stop");
        }
        assert_eq!(i[0], -1);

        let obj = table.id_ci("PlayerInfo.GetNextPlayer").expect("obj");
        let func = table.get(obj).expect("fn");
        let mut a = [ObjHandle::NULL; 1];
        {
            let mut i = [0i64; 1];
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            func(&mut ctx, &mut regs).expect("player");
        }
        assert!(!a[0].is_null());
        {
            let mut i = [0i64; 1];
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            func(&mut ctx, &mut regs).expect("done");
        }
        assert!(a[0].is_null());
    }

    #[test]
    fn cvar_screenblocks_and_font_color() {
        let table = NativeTable::with_builtins();
        let find = table
            .get(table.id_ci("CVar.FindCVar").expect("find"))
            .expect("fn");
        let get_int = table
            .get(table.id_ci("cvar.GetInt").expect("gi"))
            .expect("fn");
        let set_int = table
            .get(table.id_ci("cvar.SetInt").expect("si"))
            .expect("fn");
        let get_def = table
            .get(table.id_ci("cvar.GetDefaultInt").expect("gd"))
            .expect("fn");
        let get_ty = table
            .get(table.id_ci("cvar.GetRealType").expect("ty"))
            .expect("fn");
        let mut heap = ZvmHeap::new();
        let mut play = ZvmPlayHost::default();
        let mut ctx = ZvmCtx {
            heap: &mut heap,
            host: 0,
            play: &mut play,
        };
        let mut i = [0i64; 2];
        let mut f = [0.0; 2];
        let mut s = [StrHandle::EMPTY; 2];
        s[0] = ctx.heap.intern_string("screenblocks");
        let mut a = [ObjHandle::NULL; 1];
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            find(&mut ctx, &mut regs).expect("find");
        }
        assert!(!a[0].is_null());
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            get_int(&mut ctx, &mut regs).expect("get");
        }
        assert_eq!(i[0], 10);
        i[0] = 4;
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            set_int(&mut ctx, &mut regs).expect("set");
            get_int(&mut ctx, &mut regs).expect("get2");
        }
        assert_eq!(i[0], 4);
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            get_def(&mut ctx, &mut regs).expect("def");
        }
        assert_eq!(i[0], 10);
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            get_ty(&mut ctx, &mut regs).expect("type");
        }
        assert_eq!(i[0], 1);
        let get_float = table
            .get(table.id_ci("cvar.GetFloat").expect("gf"))
            .expect("fn");
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            get_float(&mut ctx, &mut regs).expect("float");
        }
        assert!((f[0] - 4.0).abs() < 1e-9);
        assert_eq!(call_font_int("font.FindFontColor", "Red"), 6);
        assert_eq!(call_font_int("font.FindFontColor", "nope"), -1);
        assert_eq!(call_font_int("font.GetDisplacement", ""), 8);
        assert_eq!(call_string("font.GetCursor", "", &[], &[]), "_");
    }

    #[test]
    fn global_ticrate_clean_size_and_fonts() {
        let table = NativeTable::with_builtins();
        let id = table.id_ci("_.gameticrate").expect("rate");
        let func = table.get(id).expect("fn");
        let mut heap = ZvmHeap::new();
        let mut play = ZvmPlayHost::default();
        play.native_name = "_.gameticrate".to_owned();
        let mut ctx = ZvmCtx {
            heap: &mut heap,
            host: 0,
            play: &mut play,
        };
        let mut i = [0i64; 2];
        let mut f = [0.0; 4];
        let mut s = [StrHandle::EMPTY; 2];
        let mut a = [ObjHandle::NULL; 2];
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            func(&mut ctx, &mut regs).expect("rate");
        }
        assert_eq!(i[0], 35);
        for name in [
            "_.paused",
            "_.menuactive",
            "_.netgame",
            "_.multiplayer",
            "_.demoplayback",
            "_.automapactive",
            "_.gametic",
        ] {
            ctx.play.native_name = name.to_owned();
            {
                let mut regs = NativeRegs {
                    i: &mut i,
                    f: &mut f,
                    s: &mut s,
                    a: &mut a,
                };
                func(&mut ctx, &mut regs).expect(name);
            }
            assert_eq!(i[0], 0, "{name}");
        }
        ctx.play.gametic = 12;
        ctx.play.native_name = "_.gametic".to_owned();
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            func(&mut ctx, &mut regs).expect("tic");
        }
        assert_eq!(i[0], 12);
        ctx.play.native_name = "_.validcount".to_owned();
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            func(&mut ctx, &mut regs).expect("valid");
        }
        assert_eq!(i[0], 1);
        ctx.play.native_name = "_.skyflatnum".to_owned();
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            func(&mut ctx, &mut regs).expect("sky");
        }
        assert_eq!(i[0], -1);
        ctx.play.native_name = "_.wp_nochange".to_owned();
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            func(&mut ctx, &mut regs).expect("wp");
        }
        let sentinel = a[0];
        assert!(!sentinel.is_null());
        a[0] = ObjHandle::NULL;
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            func(&mut ctx, &mut regs).expect("wp2");
        }
        assert_eq!(a[0], sentinel);
        let dict = table
            .get(table.id_ci("Dictionary.Create").expect("dict"))
            .expect("fn");
        let movie = table
            .get(table.id_ci("MoviePlayer.Create").expect("movie"))
            .expect("fn");
        a[0] = ObjHandle::NULL;
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            dict(&mut ctx, &mut regs).expect("dict");
        }
        assert!(!a[0].is_null());
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            movie(&mut ctx, &mut regs).expect("movie");
        }
        assert!(a[0].is_null());
        let sector = table
            .get(table.id_ci("levellocals.PointInSector").expect("sector"))
            .expect("fn");
        let health = table
            .get(table.id_ci("levellocals.FindHealthGroup").expect("health"))
            .expect("fn");
        let mid = table
            .get(table.id_ci("console.midprint").expect("mid"))
            .expect("fn");
        a[0] = ObjHandle::NULL;
        s[0] = ctx.heap.intern_string("Hi");
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            sector(&mut ctx, &mut regs).expect("sector");
        }
        assert!(!a[0].is_null());
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            health(&mut ctx, &mut regs).expect("health");
            mid(&mut ctx, &mut regs).expect("mid");
        }
        assert!(a[0].is_null());
        assert_eq!(ctx.play.printf.last().map(String::as_str), Some("Hi"));
        let sprite = table
            .get(table.id_ci("playerinfo.GetPSprite").expect("pspr"))
            .expect("fn");
        let inv = table
            .get(table.id_ci("playerpawn.FindInventory").expect("inv"))
            .expect("fn");
        a[0] = ctx.heap.new_object(0, 1, 0, 0);
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            sprite(&mut ctx, &mut regs).expect("pspr");
        }
        assert!(a[0].is_null());
        a[0] = ctx.heap.new_object(0, 1, 0, 0);
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            inv(&mut ctx, &mut regs).expect("inv");
        }
        assert!(a[0].is_null());
        let number = table
            .get(table.id_ci("playerpawn.PlayerNumber").expect("pnum"))
            .expect("fn");
        i[0] = 4;
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            number(&mut ctx, &mut regs).expect("pnum");
        }
        assert_eq!(i[0], 0);
        let seq = table
            .get(
                table
                    .id_ci("soundsequenceslot.StartSoundSequence")
                    .expect("seq"),
            )
            .expect("fn");
        let info = table
            .get(table.id_ci("soundinfo").expect("snd"))
            .expect("fn");
        s[0] = ctx.heap.intern_string("Hi");
        a[0] = ObjHandle::NULL;
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            seq(&mut ctx, &mut regs).expect("seq");
            info(&mut ctx, &mut regs).expect("snd");
        }
        match ctx.play.actor_ops.last() {
            Some(super::ZsActorOp::Sound(name)) => assert_eq!(name, "Hi"),
            other => panic!("seq op {other:?}"),
        }
        assert!(!a[0].is_null());
        let walk = table
            .get(table.id_ci("actoriterator.Next").expect("walk"))
            .expect("fn");
        let find = table
            .get(table.id_ci("array.Find").expect("find"))
            .expect("fn");
        let get = table
            .get(table.id_ci("array.Get").expect("get"))
            .expect("fn");
        a[0] = ctx.heap.new_object(0, 1, 0, 0);
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            walk(&mut ctx, &mut regs).expect("walk");
        }
        assert!(a[0].is_null());
        a[0] = ctx.heap.new_array_i();
        if let Some(crate::heap::ObjKind::ArrayI(items)) = ctx.heap.get_mut(a[0]) {
            items.push(7);
        }
        i[0] = 7;
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            find(&mut ctx, &mut regs).expect("find");
        }
        assert_eq!(i[0], 0);
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            get(&mut ctx, &mut regs).expect("get");
        }
        assert_eq!(i[0], 7);
        let tic = table.get(table.id_ci("gametic").expect("tic")).expect("fn");
        let font = table
            .get(table.id_ci("dlg_vgafont").expect("font"))
            .expect("fn");
        let state = table
            .get(table.id_ci("flamethrower.FindState").expect("state"))
            .expect("fn");
        ctx.play.gametic = 12;
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            tic(&mut ctx, &mut regs).expect("tic");
        }
        assert_eq!(i[0], 12);
        a[0] = ObjHandle::NULL;
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            font(&mut ctx, &mut regs).expect("font");
        }
        assert!(!a[0].is_null());
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            state(&mut ctx, &mut regs).expect("state");
        }
        assert!(a[0].is_null());
        let vga = table
            .get(table.id_ci("log_vgafont").expect("vga"))
            .expect("fn");
        let map_lines = table.get(table.id_ci("lines").expect("lines")).expect("fn");
        let used = table
            .get(table.id_ci("array.CountUsed").expect("used"))
            .expect("fn");
        a[0] = ObjHandle::NULL;
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            vga(&mut ctx, &mut regs).expect("vga");
        }
        assert!(!a[0].is_null());
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            map_lines(&mut ctx, &mut regs).expect("lines");
            used(&mut ctx, &mut regs).expect("used");
        }
        assert_eq!(i[0], 0);
        let nullp = table
            .get(table.id_ci("nullptr").expect("null"))
            .expect("fn");
        a[0] = ctx.heap.new_object(0, 1, 0, 0);
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            nullp(&mut ctx, &mut regs).expect("null");
        }
        assert!(a[0].is_null());
        let found = table
            .get(table.id_ci("global.FindInventory").expect("inv"))
            .expect("fn");
        a[0] = ctx.heap.new_object(0, 1, 0, 0);
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            found(&mut ctx, &mut regs).expect("inv");
        }
        assert!(a[0].is_null());
        let clock = table
            .get(table.id_ci("global.MSTimeF").expect("ms"))
            .expect("fn");
        let spawned = table
            .get(table.id_ci("global.SpawnMissile").expect("spawn"))
            .expect("fn");
        ctx.play.gametic = 35;
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            clock(&mut ctx, &mut regs).expect("ms");
        }
        assert_eq!(i[0], 1000);
        a[0] = ctx.heap.new_object(0, 1, 0, 0);
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            spawned(&mut ctx, &mut regs).expect("spawn");
        }
        assert!(a[0].is_null());
        let height = table
            .get(table.id_ci("sector.GetHeightSec").expect("hsec"))
            .expect("fn");
        a[0] = ctx.heap.new_object(0, 1, 0, 0);
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            height(&mut ctx, &mut regs).expect("hsec");
        }
        assert!(a[0].is_null());
        let icon = table
            .get(table.id_ci("icon.SetNull").expect("icon"))
            .expect("fn");
        let ambient = table
            .get(table.id_ci("ambientsound.activate").expect("amb"))
            .expect("fn");
        a[0] = ctx.heap.new_object(0, 1, 0, 0);
        s[0] = ctx.heap.intern_string("Hi");
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            icon(&mut ctx, &mut regs).expect("icon");
            ambient(&mut ctx, &mut regs).expect("amb");
        }
        assert!(a[0].is_null());
        match ctx.play.actor_ops.last() {
            Some(super::ZsActorOp::Sound(name)) => assert_eq!(name, "Hi"),
            other => panic!("ambient op {other:?}"),
        }
        let step = table.get(table.id_ci("it.Next").expect("it")).expect("fn");
        let inv = table
            .get(table.id_ci("inventory.NextInv").expect("inv"))
            .expect("fn");
        let msg = table
            .get(table.id_ci("inventory.printpickupmessage").expect("msg"))
            .expect("fn");
        a[0] = ctx.heap.new_object(0, 1, 0, 0);
        s[0] = ctx.heap.intern_string("Hi");
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            step(&mut ctx, &mut regs).expect("it");
        }
        assert!(a[0].is_null());
        a[0] = ctx.heap.new_object(0, 1, 0, 0);
        s[0] = ctx.heap.intern_string("Got");
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            inv(&mut ctx, &mut regs).expect("inv");
            msg(&mut ctx, &mut regs).expect("msg");
        }
        assert!(a[0].is_null());
        assert_eq!(ctx.play.printf.last().map(String::as_str), Some("Got"));
        let set_menu = table
            .get(table.id_ci("menu.setmenu").expect("menu"))
            .expect("fn");
        a[0] = ctx.heap.new_object(0, 1, 0, 0);
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            set_menu(&mut ctx, &mut regs).expect("menu");
        }
        assert!(a[0].is_null());
        let font = table
            .get(table.id_ci("menudelegatebase.PickFont").expect("font"))
            .expect("fn");
        let frame = table
            .get(table.id_ci("movieplayer.GetTexture").expect("frame"))
            .expect("fn");
        let note = table
            .get(table.id_ci("menu.startmessage").expect("note"))
            .expect("fn");
        a[0] = ObjHandle::NULL;
        s[0] = ctx.heap.intern_string("Hi");
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            font(&mut ctx, &mut regs).expect("font");
        }
        assert!(!a[0].is_null());
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            frame(&mut ctx, &mut regs).expect("frame");
            note(&mut ctx, &mut regs).expect("note");
        }
        assert!(a[0].is_null());
        assert_eq!(ctx.play.printf.last().map(String::as_str), Some("Hi"));
        let grab = table
            .get(table.id_ci("object.SetFocus").expect("focus"))
            .expect("fn");
        let drop_focus = table
            .get(table.id_ci("object.ReleaseFocus").expect("blur"))
            .expect("fn");
        let focused = table
            .get(table.id_ci("object.CheckFocus").expect("chk"))
            .expect("fn");
        a[0] = ctx.heap.new_object(0, 4, 0, 0);
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            grab(&mut ctx, &mut regs).expect("focus");
            focused(&mut ctx, &mut regs).expect("chk");
        }
        assert_eq!(i[0], 1);
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            drop_focus(&mut ctx, &mut regs).expect("blur");
            focused(&mut ctx, &mut regs).expect("chk2");
        }
        assert_eq!(i[0], 0);
        let state = table
            .get(table.id_ci("weapon.FindState").expect("wstate"))
            .expect("fn");
        let beep = table
            .get(table.id_ci("weapon.PlayPickupSound").expect("beep"))
            .expect("fn");
        a[0] = ctx.heap.new_object(0, 1, 0, 0);
        s[0] = ctx.heap.intern_string("pkup");
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            state(&mut ctx, &mut regs).expect("wstate");
            beep(&mut ctx, &mut regs).expect("beep");
        }
        assert!(a[0].is_null());
        match ctx.play.actor_ops.last() {
            Some(super::ZsActorOp::Sound(name)) => assert_eq!(name, "pkup"),
            other => panic!("pickup sound {other:?}"),
        }
        let spot = table
            .get(table.id_ci("spotstate.GetNextInList").expect("spot"))
            .expect("fn");
        let save = table
            .get(table.id_ci("savegamemanager.GetSavegame").expect("save"))
            .expect("fn");
        a[0] = ctx.heap.new_object(0, 1, 0, 0);
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            spot(&mut ctx, &mut regs).expect("spot");
        }
        assert!(a[0].is_null());
        a[0] = ctx.heap.new_object(0, 1, 0, 0);
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            save(&mut ctx, &mut regs).expect("save");
        }
        assert!(a[0].is_null());
        let parent = table
            .get(table.id_ci("type.GetParentClass").expect("parent"))
            .expect("fn");
        let slotw = table
            .get(table.id_ci("weaponslots.GetWeapon").expect("slotw"))
            .expect("fn");
        a[0] = ctx.heap.new_object(0, 1, 0, 0);
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            parent(&mut ctx, &mut regs).expect("parent");
        }
        assert!(a[0].is_null());
        a[0] = ctx.heap.new_object(0, 1, 0, 0);
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            slotw(&mut ctx, &mut regs).expect("slotw");
        }
        assert!(a[0].is_null());
        let query = table
            .get(table.id_ci("os_Query.fromString").expect("query"))
            .expect("fn");
        let lift = table
            .get(table.id_ci("pos.PlusZ").expect("lift"))
            .expect("fn");
        let special = table
            .get(table.id_ci("super.DoSpecialDamage").expect("special"))
            .expect("fn");
        let die = table
            .get(table.id_ci("super.Die").expect("die"))
            .expect("fn");
        let gone = table
            .get(table.id_ci("brokenlines.Destroy").expect("gone"))
            .expect("fn");
        a[0] = ObjHandle::NULL;
        f[0] = 12.0;
        i[0] = 9;
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            query(&mut ctx, &mut regs).expect("query");
        }
        assert!(!a[0].is_null());
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            lift(&mut ctx, &mut regs).expect("lift");
        }
        assert_eq!(f[2], 12.0);
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            special(&mut ctx, &mut regs).expect("special");
            die(&mut ctx, &mut regs).expect("die");
            gone(&mut ctx, &mut regs).expect("gone");
        }
        assert_eq!(i[0], 9);
        assert!(a[0].is_null());
        match ctx.play.actor_ops.last() {
            Some(super::ZsActorOp::Action { name, .. }) => assert_eq!(name, "A_Die"),
            other => panic!("super die {other:?}"),
        }
        let beep = table
            .get(table.id_ci("super.PlayPickupSound").expect("supk"))
            .expect("fn");
        let unmorph = table
            .get(table.id_ci("super.UndoMonsterMorph").expect("unmorph"))
            .expect("fn");
        a[0] = ctx.heap.new_object(0, 1, 0, 0);
        s[0] = ctx.heap.intern_string("supk");
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            beep(&mut ctx, &mut regs).expect("supk");
            unmorph(&mut ctx, &mut regs).expect("unmorph");
        }
        assert!(a[0].is_null());
        match ctx.play.actor_ops.last() {
            Some(super::ZsActorOp::Sound(name)) => assert_eq!(name, "supk"),
            other => panic!("super pickup {other:?}"),
        }
        let unit = table
            .get(table.id_ci("thrust.Unit").expect("unit"))
            .expect("fn");
        let parsed = table
            .get(table.id_ci("text.ToDouble").expect("dbl"))
            .expect("fn");
        let now = table
            .get(table.id_ci("systemtime.now").expect("now"))
            .expect("fn");
        let hush = table
            .get(table.id_ci("system.stopallsounds").expect("hush"))
            .expect("fn");
        f[0] = 3.0;
        f[1] = 4.0;
        f[2] = 0.0;
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            unit(&mut ctx, &mut regs).expect("unit");
        }
        assert!((f[0] - 0.6).abs() < 1e-9);
        assert!((f[1] - 0.8).abs() < 1e-9);
        s[0] = ctx.heap.intern_string("3.5");
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            parsed(&mut ctx, &mut regs).expect("dbl");
        }
        assert_eq!(f[0], 3.5);
        ctx.play.gametic = 35;
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            now(&mut ctx, &mut regs).expect("now");
            hush(&mut ctx, &mut regs).expect("hush");
        }
        assert_eq!(i[0], 1000);
        ctx.play.gametic = 0;
        match ctx.play.actor_ops.last() {
            Some(super::ZsActorOp::Action { name, .. }) => assert_eq!(name, "A_StopSound"),
            other => panic!("stop sounds {other:?}"),
        }
        let make = table
            .get(table.id_ci("Dictionary.Create").expect("dict"))
            .expect("fn");
        let put = table
            .get(table.id_ci("dictionary.insert").expect("put"))
            .expect("fn");
        let got = table
            .get(table.id_ci("dictionary.at").expect("got"))
            .expect("fn");
        let seq = table
            .get(table.id_ci("actor.instatesequence").expect("seq"))
            .expect("fn");
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            make(&mut ctx, &mut regs).expect("dict");
        }
        assert!(!a[0].is_null());
        s[0] = ctx.heap.intern_string("k");
        s[1] = ctx.heap.intern_string("v");
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            put(&mut ctx, &mut regs).expect("put");
            got(&mut ctx, &mut regs).expect("got");
        }
        assert_eq!(ctx.heap.str(s[1]), "v");
        assert!(!a[0].is_null());
        s[0] = ctx.heap.intern_string("no");
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            got(&mut ctx, &mut regs).expect("miss");
        }
        assert_eq!(ctx.heap.str(s[1]), "");
        i[0] = 1;
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            seq(&mut ctx, &mut regs).expect("seq");
        }
        assert_eq!(i[0], 0);
        let shove = table
            .get(table.id_ci("actor.thrust").expect("shove"))
            .expect("fn");
        let before = ctx.play.actor_ops.len();
        f[0] = 1.0e37;
        f[1] = 0.0;
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            shove(&mut ctx, &mut regs).expect("sentinel");
        }
        assert_eq!(ctx.play.actor_ops.len(), before);
        f[0] = 10.0;
        f[1] = 0.0;
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            shove(&mut ctx, &mut regs).expect("shove");
        }
        match ctx.play.actor_ops.last() {
            Some(super::ZsActorOp::Thrust { vx, vy }) => {
                assert_eq!(*vx, 10.0);
                assert_eq!(*vy, 0.0);
            }
            other => panic!("thrust {other:?}"),
        }
        let hurt = table
            .get(table.id_ci("actor.setdamage").expect("dmg"))
            .expect("fn");
        i[0] = 15;
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            hurt(&mut ctx, &mut regs).expect("dmg");
        }
        match ctx.play.actor_ops.last() {
            Some(super::ZsActorOp::Action { name, args }) => {
                assert_eq!(name, "A_SetDamage");
                assert_eq!(args[0], 15);
            }
            other => panic!("set damage {other:?}"),
        }
        let idle = table
            .get(table.id_ci("actor.setidle").expect("idle"))
            .expect("fn");
        i[0] = 0;
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            idle(&mut ctx, &mut regs).expect("idle");
        }
        match ctx.play.actor_ops.last() {
            Some(super::ZsActorOp::Idle { call_action }) => assert!(*call_action),
            other => panic!("idle {other:?}"),
        }
        i[0] = 1;
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            idle(&mut ctx, &mut regs).expect("quiet");
        }
        match ctx.play.actor_ops.last() {
            Some(super::ZsActorOp::Idle { call_action }) => assert!(!*call_action),
            other => panic!("quiet idle {other:?}"),
        }
        let active = table
            .get(table.id_ci("actor.playactivesound").expect("active"))
            .expect("fn");
        let spawned = table
            .get(table.id_ci("actor.playspawnsound").expect("spawned"))
            .expect("fn");
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            active(&mut ctx, &mut regs).expect("active");
        }
        match ctx.play.actor_ops.last() {
            Some(super::ZsActorOp::ActiveSound) => {}
            other => panic!("active sound {other:?}"),
        }
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            spawned(&mut ctx, &mut regs).expect("spawned");
        }
        match ctx.play.actor_ops.last() {
            Some(super::ZsActorOp::SpawnSound) => {}
            other => panic!("spawn sound {other:?}"),
        }
        let howl = table
            .get(table.id_ci("actor.howl").expect("howl"))
            .expect("fn");
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            howl(&mut ctx, &mut regs).expect("howl");
        }
        match ctx.play.actor_ops.last() {
            Some(super::ZsActorOp::Howl) => {}
            other => panic!("howl {other:?}"),
        }
        let bounce = table
            .get(table.id_ci("actor.clearbounce").expect("bounce"))
            .expect("fn");
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            bounce(&mut ctx, &mut regs).expect("bounce");
        }
        match ctx.play.actor_ops.last() {
            Some(super::ZsActorOp::ClearBounce) => {}
            other => panic!("clear bounce {other:?}"),
        }
        let link = table
            .get(table.id_ci("actor.linktoworld").expect("link"))
            .expect("fn");
        let unlink = table
            .get(table.id_ci("actor.unlinkfromworld").expect("unlink"))
            .expect("fn");
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            link(&mut ctx, &mut regs).expect("link");
        }
        match ctx.play.actor_ops.last() {
            Some(super::ZsActorOp::WorldLink { linked }) => assert!(*linked),
            other => panic!("link {other:?}"),
        }
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            unlink(&mut ctx, &mut regs).expect("unlink");
        }
        match ctx.play.actor_ops.last() {
            Some(super::ZsActorOp::WorldLink { linked }) => assert!(!*linked),
            other => panic!("unlink {other:?}"),
        }
        let fit = table
            .get(table.id_ci("actor.findfloorceiling").expect("fit"))
            .expect("fn");
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            fit(&mut ctx, &mut regs).expect("fit");
        }
        match ctx.play.actor_ops.last() {
            Some(super::ZsActorOp::FitSector) => {}
            other => panic!("fit sector {other:?}"),
        }
        let restore = table
            .get(table.id_ci("actor.restoredamage").expect("restore"))
            .expect("fn");
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            restore(&mut ctx, &mut regs).expect("restore");
        }
        match ctx.play.actor_ops.last() {
            Some(super::ZsActorOp::RestoreDamage) => {}
            other => panic!("restore damage {other:?}"),
        }
        let untid = table
            .get(table.id_ci("actor.removefromhash").expect("untid"))
            .expect("fn");
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            untid(&mut ctx, &mut regs).expect("untid");
        }
        match ctx.play.actor_ops.last() {
            Some(super::ZsActorOp::ClearTid) => {}
            other => panic!("clear tid {other:?}"),
        }
        let counters = table
            .get(table.id_ci("actor.clearcounters").expect("counters"))
            .expect("fn");
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            counters(&mut ctx, &mut regs).expect("counters");
        }
        match ctx.play.actor_ops.last() {
            Some(super::ZsActorOp::ClearCounters) => {}
            other => panic!("clear counters {other:?}"),
        }
        let item_counters = table
            .get(table.id_ci("inventory.ClearCounters").expect("item"))
            .expect("fn");
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            item_counters(&mut ctx, &mut regs).expect("item");
        }
        match ctx.play.actor_ops.last() {
            Some(super::ZsActorOp::ClearCounters) => {}
            other => panic!("item counters {other:?}"),
        }
        let obj_counters = table
            .get(table.id_ci("object.ClearCounters").expect("obj"))
            .expect("fn");
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            obj_counters(&mut ctx, &mut regs).expect("obj");
        }
        match ctx.play.actor_ops.last() {
            Some(super::ZsActorOp::ClearCounters) => {}
            other => panic!("object counters {other:?}"),
        }
        let hurt = table
            .get(table.id_ci("actor.damagemobj").expect("hurt"))
            .expect("fn");
        let before = ctx.play.actor_ops.len();
        i[0] = 0;
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            hurt(&mut ctx, &mut regs).expect("nodmg");
        }
        assert_eq!(ctx.play.actor_ops.len(), before);
        i[0] = 15;
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            hurt(&mut ctx, &mut regs).expect("hurt");
        }
        assert_eq!(i[0], 15);
        match ctx.play.actor_ops.last() {
            Some(super::ZsActorOp::Action { name, args }) => {
                assert_eq!(name, "A_DamageSelf");
                assert_eq!(args[0], 15);
            }
            other => panic!("damage mobj {other:?}"),
        }
        let pawn = table
            .get(table.id_ci("playerpawn.DamageMobj").expect("pawn"))
            .expect("fn");
        i[0] = 8;
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            pawn(&mut ctx, &mut regs).expect("pawn");
        }
        assert_eq!(i[0], 8);
        match ctx.play.actor_ops.last() {
            Some(super::ZsActorOp::Action { name, args }) => {
                assert_eq!(name, "A_DamageSelf");
                assert_eq!(args[0], 8);
            }
            other => panic!("pawn damage {other:?}"),
        }
        let bleed = table
            .get(table.id_ci("actor.tracebleed").expect("bleed"))
            .expect("fn");
        let before = ctx.play.actor_ops.len();
        i[0] = 0;
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            bleed(&mut ctx, &mut regs).expect("noblood");
        }
        assert_eq!(ctx.play.actor_ops.len(), before);
        i[0] = 9;
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            bleed(&mut ctx, &mut regs).expect("bleed");
        }
        match ctx.play.actor_ops.last() {
            Some(super::ZsActorOp::Bleed { damage }) => assert_eq!(*damage, 9),
            other => panic!("bleed {other:?}"),
        }
        let traced = table
            .get(
                table
                    .id_ci("ftranslatedlinetarget.tracebleed")
                    .expect("traced"),
            )
            .expect("fn");
        i[0] = 4;
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            traced(&mut ctx, &mut regs).expect("traced");
        }
        match ctx.play.actor_ops.last() {
            Some(super::ZsActorOp::Bleed { damage }) => assert_eq!(*damage, 4),
            other => panic!("trace bleed {other:?}"),
        }
        let splash = table
            .get(table.id_ci("actor.drawsplash").expect("splash"))
            .expect("fn");
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            splash(&mut ctx, &mut regs).expect("splash");
        }
        match ctx.play.actor_ops.last() {
            Some(super::ZsActorOp::Splash) => {}
            other => panic!("splash {other:?}"),
        }
        let hide = table
            .get(table.id_ci("actor.disablelocalrendering").expect("hide"))
            .expect("fn");
        i[0] = 0;
        i[1] = 1;
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            hide(&mut ctx, &mut regs).expect("hide");
        }
        match ctx.play.actor_ops.last() {
            Some(super::ZsActorOp::LocalHide { hide }) => assert!(*hide),
            other => panic!("hide {other:?}"),
        }
        i[1] = 0;
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            hide(&mut ctx, &mut regs).expect("show");
        }
        match ctx.play.actor_ops.last() {
            Some(super::ZsActorOp::LocalHide { hide }) => assert!(!*hide),
            other => panic!("show {other:?}"),
        }
        let shade = table
            .get(table.id_ci("actor.setshade").expect("shade"))
            .expect("fn");
        i[0] = 0;
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            shade(&mut ctx, &mut regs).expect("clear");
        }
        match ctx.play.actor_ops.last() {
            Some(super::ZsActorOp::Shade { rgb }) => assert!(rgb.is_none()),
            other => panic!("clear shade {other:?}"),
        }
        i[0] = 0x102030;
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            shade(&mut ctx, &mut regs).expect("shade");
        }
        match ctx.play.actor_ops.last() {
            Some(super::ZsActorOp::Shade { rgb }) => assert_eq!(*rgb, Some([16, 32, 48])),
            other => panic!("shade {other:?}"),
        }
        let style = table
            .get(table.id_ci("object.SetRenderStyle").expect("style"))
            .expect("fn");
        let before = ctx.play.actor_ops.len();
        i[0] = 99;
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            style(&mut ctx, &mut regs).expect("bad");
        }
        assert_eq!(ctx.play.actor_ops.len(), before);
        i[0] = 6;
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            style(&mut ctx, &mut regs).expect("trans");
        }
        match ctx.play.actor_ops.last() {
            Some(super::ZsActorOp::RenderMode { mode }) => assert_eq!(*mode, 6),
            other => panic!("render style {other:?}"),
        }
        let eye = table
            .get(table.id_ci("actor.setviewpos").expect("eye"))
            .expect("fn");
        f[2] = 12.0;
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            eye(&mut ctx, &mut regs).expect("eye");
        }
        match ctx.play.actor_ops.last() {
            Some(super::ZsActorOp::ViewZ { z }) => assert_eq!(*z, 12.0),
            other => panic!("view pos {other:?}"),
        }
        let copy = table
            .get(table.id_ci("actor.copyfriendliness").expect("copy"))
            .expect("fn");
        let before = ctx.play.actor_ops.len();
        a[1] = ObjHandle::NULL;
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            copy(&mut ctx, &mut regs).expect("none");
        }
        assert_eq!(ctx.play.actor_ops.len(), before);
        a[1] = ctx.heap.new_object(0, 1, 0, 0);
        i[0] = 1;
        i[1] = 1;
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            copy(&mut ctx, &mut regs).expect("copy");
        }
        match ctx.play.actor_ops.last() {
            Some(super::ZsActorOp::CopyFriend {
                change_target,
                reset_health,
            }) => {
                assert!(*change_target);
                assert!(*reset_health);
            }
            other => panic!("copy friend {other:?}"),
        }
        let friend = table
            .get(table.id_ci("actor.setfriendplayer").expect("friend"))
            .expect("fn");
        let before = ctx.play.actor_ops.len();
        a[1] = ObjHandle::NULL;
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            friend(&mut ctx, &mut regs).expect("nofriend");
        }
        assert_eq!(ctx.play.actor_ops.len(), before);
        a[1] = ctx.heap.new_object(0, 1, 0, 0);
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            friend(&mut ctx, &mut regs).expect("friend");
        }
        match ctx.play.actor_ops.last() {
            Some(super::ZsActorOp::Action { name, args }) => {
                assert_eq!(name, "A_SetFriendly");
                assert_eq!(args[0], 1);
            }
            other => panic!("friend {other:?}"),
        }
        let next = table
            .get(table.id_ci("ThinkerIterator.Next").expect("next"))
            .expect("fn");
        let menu = table
            .get(table.id_ci("Menu.GetCurrentMenu").expect("menu"))
            .expect("fn");
        a[0] = ctx.heap.new_object(0, 1, 0, 0);
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            next(&mut ctx, &mut regs).expect("next");
        }
        assert!(a[0].is_null());
        a[0] = ctx.heap.new_object(0, 1, 0, 0);
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            menu(&mut ctx, &mut regs).expect("menu");
        }
        assert!(a[0].is_null());
        ctx.play.native_name = "_.cleanwidth".to_owned();
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            func(&mut ctx, &mut regs).expect("width");
        }
        assert_eq!(i[0], 320);
        ctx.play.native_name = "_.cleanheight".to_owned();
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            func(&mut ctx, &mut regs).expect("height");
        }
        assert_eq!(i[0], 200);
        ctx.play.native_name = "_.cleanxfac".to_owned();
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            func(&mut ctx, &mut regs).expect("scale");
        }
        assert_eq!(i[0], 1);
        ctx.play.native_name = "_.bigfont".to_owned();
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            func(&mut ctx, &mut regs).expect("font");
        }
        assert!(!a[0].is_null());
        ctx.play.native_name = "_.AllClasses".to_owned();
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            func(&mut ctx, &mut regs).expect("classes");
        }
        assert!(!a[0].is_null());
        ctx.play.native_name = "_.level".to_owned();
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            func(&mut ctx, &mut regs).expect("level");
        }
        assert!(!a[0].is_null());
        ctx.play.native_name = "_.smallfont".to_owned();
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            func(&mut ctx, &mut regs).expect("small");
        }
        assert!(!a[0].is_null());
        ctx.play.native_name = "_.notifyfontscale".to_owned();
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            func(&mut ctx, &mut regs).expect("scale");
        }
        assert_eq!(i[0], 1);
        ctx.play.native_name = "_.viewactive".to_owned();
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            func(&mut ctx, &mut regs).expect("view");
        }
        assert_eq!(i[0], 1);
        ctx.play.native_name = "_.playeringame".to_owned();
        i[0] = 0;
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            func(&mut ctx, &mut regs).expect("p0");
        }
        assert_eq!(i[0], 1);
        i[0] = 2;
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            func(&mut ctx, &mut regs).expect("p2");
        }
        assert_eq!(i[0], 0);
        assert_eq!(call_math("global.Ceil", &[1.2]), 2.0);
        assert_eq!(call_math("global.Clamp", &[5.0, 0.0, 2.0]), 2.0);
        assert_eq!(call_math("global.Lerp", &[0.0, 10.0, 0.5]), 5.0);
        assert_eq!(call_math("global.Distance2D", &[0.0, 0.0, 3.0, 4.0]), 5.0);
        assert!((call_math("global.AngleTo", &[0.0, 0.0, 0.0, 1.0]) - 90.0).abs() < 1e-6);
        assert_eq!(call_math("global.AccuracyFactor", &[]), 1.0);
        assert_eq!(call_math("global.G_SkillPropertyFloat", &[]), 1.0);
        assert_eq!(call_math("global.Floor", &[1.8]), 1.0);
        assert!((call_math("global.sin", &[90.0]) - 1.0).abs() < 1e-9);
        assert_eq!(call_math("global.Splerp", &[0.0, 10.0, 0.5]), 5.0);
        assert_eq!(call_math("global.Splerp", &[0.0, 10.0, 0.0]), 0.0);
        assert_eq!(call_math("global.DistanceBySpeed", &[70.0, 10.0]), 7.0);
        assert_eq!(call_math("global.deltaangle", &[10.0, 350.0]), -20.0);
        assert_eq!(call_math("global.absangle", &[10.0, 350.0]), 20.0);
        assert_eq!(call_math("global.GetFriction", &[]), 0.90625);
        assert!((call_math("global.VectorAngle", &[0.0, 1.0]) - 90.0).abs() < 1e-6);
        assert_eq!(call_font_int("object.IsNull", ""), 1);
        assert_eq!(call_font_int("object.isValid", ""), 0);
        assert_eq!(call_font_int("object.IsNotEmpty", "Clip"), 1);
        assert_eq!(call_font_int("object.IsNotEmpty", ""), 0);
        assert_eq!(call_font_int("object.GetHeight", ""), 16);
        assert_eq!(call_font_int("object.Visible", ""), 1);
        assert!((call_math("object.VelFromAngle", &[10.0, 0.0]) - 10.0).abs() < 1e-9);
        assert!(call_math("object.VelFromAngle", &[10.0, 90.0]).abs() < 1e-9);
        assert!((call_math("object.Vel3DFromAngle", &[10.0, 0.0, 0.0]) - 10.0).abs() < 1e-9);
        assert!((call_math("object.PitchFromVel", &[0.0, 10.0, 10.0]) - 45.0).abs() < 1e-6);
        assert_eq!(call_math("object.bam", &[90.0]), 1_073_741_824.0);
        assert_eq!(call_string("object.GetPlayerColor", "", &[], &[]), "Green");
        assert_eq!(call_font_int("object.GetLightLevel", ""), 255);
        assert_eq!(call_font_int("object.GetTextColor", ""), 11);
        assert_eq!(call_math("object.Vec3Offset", &[3.0, 4.0, 5.0]), 3.0);
        assert!((call_math("object.getdeltatime", &[]) - (1.0 / 35.0)).abs() < 1e-12);
        assert_eq!(call_font_int("object.CursorSpace", ""), 8);
        assert_eq!(call_string("object.StringAt", "Hi", &[], &[1]), "i");
        assert_eq!(call_string("object.PickupMessage", "", &[], &[]), "");
        assert_eq!(call_font_int("object.CheckMissileSpawn", ""), 1);
        assert_eq!(call_font_int("object.CheckStopped", ""), 1);
        assert_eq!(call_font_int("object.CheckFocus", ""), 0);
        assert_eq!(call_math("actor.angletovector", &[0.0, 10.0]), 10.0);
        assert!(call_math("actor.angletovector", &[90.0, 10.0]).abs() < 1e-9);
        assert_eq!(call_math("actor.absangle", &[10.0, 350.0]), 20.0);
        assert_eq!(call_font_int("actor.CheckMissileSpawn", ""), 1);
        assert_eq!(call_font_int("actor.checklocalview", ""), 1);
        assert_eq!(call_math("actor.deltaangle", &[10.0, 350.0]), -20.0);
        assert_eq!(call_math("actor.distancebyspeed", &[70.0, 10.0]), 7.0);
        assert_eq!(call_font_int("actor.countsaskill", ""), 1);
        assert_eq!(call_font_int("actor.checkposition", ""), 1);
        assert_eq!(call_font_int("actor.getdefaultspeed", ""), 8);
        assert_eq!(call_font_int("actor.getspawnhealth", ""), 100);
        assert_eq!(call_string("actor.getfloorterrain", "", &[], &[]), "Rock");
        assert_eq!(call_font_int("actor.isvisible", ""), 1);
        assert_eq!(call_font_int("actor.hitfloor", ""), 1);
        assert_eq!(call_string("actor.getspecies", "", &[], &[]), "None");
        assert_eq!(call_font_int("actor.isfrozen", ""), 0);
        assert_eq!(call_font_int("actor.isfriend", ""), 0);
        assert_eq!(call_font_int("actor.iszerodamage", ""), 0);
        assert_eq!(call_font_int("actor.spawnhealth", ""), 100);
        assert_eq!(call_font_int("actor.shouldpassthroughplayer", ""), 0);
        assert_eq!(call_math("actor.velfromangle", &[10.0, 0.0]), 10.0);
        assert_eq!(call_math("actor.vec2offset", &[6.0, 7.0]), 6.0);
        assert_eq!(call_math("actor.vec3offset", &[3.0, 4.0, 5.0]), 3.0);
        assert_eq!(call_font_int("actor.testmobjlocation", ""), 1);
        assert_eq!(call_font_int("actor.trymove", ""), 0);
        assert_eq!(call_font_int("actor.teleport", ""), 0);
        assert_eq!(call_font_int("actor.checkbossdeath", ""), 0);
        assert_eq!(call_font_int("actor.canseek", ""), 0);
        assert_eq!(call_font_int("actor.checkblock", ""), 0);
        assert_eq!(call_math("actor.bulletslope", &[45.0]), 0.0);
        assert_eq!(call_math("actor.getboboffset", &[4.0]), 0.0);
        assert_eq!(call_font_int("actor.getboneindex", ""), -1);
        assert_eq!(call_string("actor.gettag", "", &[], &[]), "");
        assert_eq!(call_font_int("actor.givebody", ""), 0);
        assert_eq!(call_font_int("actor.givesecret", ""), 0);
        assert_eq!(call_font_int("actor.playerskincheck", ""), 1);
        assert_eq!(call_font_int("actor.checknodelay", ""), 0);
        assert_eq!(call_font_int("AltIcon.Exists", ""), 0);
        assert_eq!(call_font_int("AltIcon.isNull", ""), 1);
        assert_eq!(call_font_int("levellocals.givesecret", ""), 0);
        assert_eq!(call_font_int("levellocals.executespecial", ""), 0);
        assert_eq!(call_font_int("levellocals.exitlevel", ""), 0);
        assert_eq!(call_font_int("levellocals.getudmfint", ""), 0);
        assert_eq!(call_font_int("InputGridChars.ByteAt", ""), 0);
        assert_eq!(call_string("levellocals.getclustername", "", &[], &[]), "");
        assert_eq!(call_math("levellocals.vec3diff", &[9.0, 8.0, 7.0]), 0.0);
        assert_eq!(call_font_int("playerpawn.CheckKeys", ""), 0);
        assert_eq!(call_font_int("playerinfo.poisondamage", ""), 0);
        assert_eq!(call_font_int("playerinfo.getteam", ""), 0);
        assert_eq!(call_math("playerinfo.GetMoveBob", &[2.0]), 0.0);
        assert_eq!(
            call_math("playerpawn.Distance2D", &[0.0, 0.0, 3.0, 4.0]),
            5.0
        );
        assert_eq!(
            call_string("playerclass.GetColorSetName", "", &[], &[]),
            "Green"
        );
        assert_eq!(call_font_int("playerpawn.PlayerNumber", ""), 0);
        assert_eq!(call_font_int("playerpawn.isFrozen", ""), 0);
        assert_eq!(call_font_int("playerpawn.cheatsuicide", ""), 0);
        assert_eq!(call_font_int("playerpawn.checkweaponbuttons", ""), 0);
        assert_eq!(call_font_int("screen.GetTextureWidth", ""), 0);
        assert_eq!(call_font_int("screen.GetTextureHeight", ""), 0);
        assert_eq!(call_font_int("texman.checkfortexture", ""), -1);
        assert_eq!(call_font_int("texman.checkrealheight", ""), 0);
        assert_eq!(call_font_int("texture.IsValid", ""), 0);
        assert_eq!(call_font_int("translation.getid", ""), -1);
        assert_eq!(call_math("texman.getsize", &[8.0, 9.0]), 0.0);
        // `Array.Find` on no array: its size, 0 (UZDoom returns `Size()` when not found).
        assert_eq!(call_font_int("array.Find", ""), 0);
        assert_eq!(call_font_int("cleanwidth", ""), 320);
        assert_eq!(call_font_int("cleanheight", ""), 200);
        assert_eq!(call_font_int("cl_run", ""), 1);
        assert_eq!(call_font_int("althud.getlatency", ""), 0);
        assert_eq!(call_font_int("basestatusbar.isscoreboardopen", ""), 0);
        assert_eq!(call_font_int("chaintex.IsValid", ""), 0);
        assert_eq!(call_string("ReplyText.AppendFormat", "Hi", &[], &[]), "Hi");
        assert_eq!(call_font_int("deathmatch", ""), 0);
        assert_eq!(call_font_int("dmflags", ""), 0);
        assert_eq!(call_font_int("gameticrate", ""), 35);
        assert_eq!(call_font_int("gametic", ""), 0);
        assert_eq!(call_math("diff.Length", &[3.0, 4.0]), 5.0);
        assert_eq!(call_font_int("damagetypedefinition.ignorearmor", ""), 0);
        assert_eq!(call_font_int("custommessageboxmenubase.optioncount", ""), 0);
        assert_eq!(
            call_font_int("custommessageboxmenubase.optionforshortcut", ""),
            -1
        );
        assert_eq!(call_font_int("cursorTex.IsValid", ""), 0);
        assert_eq!(call_font_int("gl_texture_hqresizemult", ""), 1);
        assert_eq!(call_font_int("cleanxfac", ""), 1);
        assert_eq!(call_font_int("cleanyfac", ""), 1);
        assert_eq!(call_font_int("hud_health_red", ""), 25);
        assert_eq!(call_font_int("hud_health_yellow", ""), 50);
        assert_eq!(call_font_int("hud_health_green", ""), 100);
        assert_eq!(call_font_int("hud_berserk_health", ""), 100);
        assert_eq!(call_font_int("hud_ammo_red", ""), 25);
        assert_eq!(call_font_int("hud_showammo", ""), 0);
        assert_eq!(call_font_int("compatflags", ""), 0);
        assert_eq!(call_font_int("netgame", ""), 0);
        assert_eq!(call_font_int("sv_ammofactor", ""), 1);
        assert_eq!(call_font_int("sv_infiniteammo", ""), 0);
        assert_eq!(call_font_int("sv_noarmor", ""), 0);
        assert_eq!(call_font_int("sv_cooplosekeys", ""), 0);
        assert_eq!(call_font_int("sb_cooperative_enable", ""), 1);
        assert_eq!(call_font_int("sb_deathmatch_enable", ""), 0);
        assert_eq!(call_font_int("snd_menuvolume", ""), 1);
        assert_eq!(call_font_int("ticrate", ""), 35);
        assert_eq!(call_font_int("maxplayers", ""), 1);
        assert_eq!(call_font_int("m_cleanscale", ""), 1);
        assert_eq!(call_font_int("vid_gamma", ""), 1);
        assert_eq!(call_font_int("sv_nomonsters", ""), 0);
        assert_eq!(call_font_int("sv_nohealth", ""), 0);
        assert_eq!(call_font_int("sv_weaponstay", ""), 0);
        assert_eq!(call_font_int("multiplayer", ""), 0);
        assert_eq!(call_font_int("wi_percents", ""), 1);
        assert_eq!(call_font_int("teamplay", ""), 0);
        assert_eq!(call_font_int("global.ACS_ExecuteWithResult", ""), 0);
        assert_eq!(call_math("global.BulletSlope", &[30.0]), 0.0);
        assert_eq!(call_font_int("global.Door_Open", ""), 0);
        assert_eq!(call_font_int("global.AddAmmo", ""), 0);
        assert_eq!(
            call_string("colorpickermenu.GetColor", "", &[], &[]),
            "Green"
        );
        assert_eq!(call_font_int("global.MSTimeF", ""), 0);
        assert_eq!(call_font_int("global.TweakSpeeds", ""), 1);
        assert_eq!(call_font_int("global.GiveInventoryType", ""), 0);
        assert_eq!(call_font_int("global.Thing_Destroy", ""), 0);
        assert_eq!(
            call_math("global.StatusbarToRealCoords", &[10.0, 20.0]),
            10.0
        );
        assert_eq!(call_math("global.double", &[3.5]), 3.5);
        assert_eq!(call_string("global.name", "Hi", &[], &[]), "Hi");
        assert_eq!(call_font_int("line.index", ""), -1);
        assert_eq!(call_font_int("sector.index", ""), -1);
        assert_eq!(call_math("sector.LowestFloorAt", &[100.0, 200.0]), 0.0);
        assert_eq!(
            call_math("sector.NextHighestCeilingAt", &[0.0, 0.0, 40.0]),
            168.0
        );
        assert_eq!(call_font_int("goodbyestr.Length", "Hi"), 2);
        assert_eq!(call_string("goodbyestr.Left", "Hello", &[], &[2]), "He");
        assert_eq!(call_font_int("hotkey.GetNextCodePoint", ""), -1);
        assert_eq!(call_font_int("haptics.rumble", ""), 0);
        assert_eq!(call_font_int("destructible.projectilehitplane", ""), 0);
        assert_eq!(call_font_int("custominventory.callstatechain", ""), 0);
        assert_eq!(call_font_int("icon.isNull", ""), 1);
        assert_eq!(call_font_int("joystickconfig.getnumaxes", ""), 0);
        assert_eq!(call_font_int("joystickconfig.GetSensitivity", ""), 1);
        assert_eq!(call_string("it.GetKey", "", &[], &[]), "");
        assert_eq!(call_string("inventory.DisplayNameTag", "", &[], &[]), "");
        assert_eq!(call_string("keybindings.getbinding", "", &[], &[]), "");
        assert_eq!(call_font_int("mCurrentTooltip.IsEmpty", ""), 1);
        assert_eq!(call_font_int("mCurrentTooltip.IsEmpty", "Hi"), 0);
        assert_eq!(
            call_string("mEnterString.AppendCharacter", "Hi", &[], &[33]),
            "Hi!"
        );
        assert_eq!(
            call_string("mEnterString.DeleteLastCharacter", "Hi", &[], &[]),
            "H"
        );
        assert_eq!(call_font_int("mTexture.Exists", ""), 0);
        assert_eq!(call_font_int("m_tooltip_lines", ""), 4);
        assert_eq!(call_font_int("m_tooltip_speed", ""), 1);
        assert_eq!(call_font_int("menu.menutime", ""), 0);
        assert_eq!(call_font_int("label.GetNextCodePoint", ""), -1);
        assert_eq!(call_font_int("left.IsValid", ""), 0);
        assert_eq!(call_font_int("levellocals.createfloor", ""), 0);
        assert_eq!(call_font_int("levellocals.setfrozen", ""), 0);
        assert_eq!(call_font_int("object.CallTryPickup", ""), 0);
        assert_eq!(call_font_int("object.DepleteOrDestroy", ""), 0);
        assert_eq!(call_font_int("object.SetState", ""), 0);
        assert_eq!(call_font_int("object.MenuEvent", ""), 0);
        assert_eq!(call_font_int("object.ProcessInput", ""), 0);
        assert_eq!(call_font_int("object.Use", ""), 0);
        assert_eq!(call_font_int("object.isclass", ""), 0);
        assert_eq!(call_font_int("object.mstimef", ""), 0);
        assert_eq!(call_font_int("object.throwabortexception", ""), 0);
        assert_eq!(call_font_int("weapon.isAbstract", ""), 0);
        assert_eq!(call_string("weapon.PickupMessage", "", &[], &[]), "");
        assert_eq!(call_font_int("offtex.IsValid", ""), 0);
        assert_eq!(call_font_int("picnum.Exists", ""), 0);
        assert_eq!(call_font_int("optionvalues.getcount", ""), 0);
        assert_eq!(call_string("optionvalues.gettext", "", &[], &[]), "");
        assert_eq!(call_font_int("statusbarcore.gethudscale", ""), 1);
        assert_eq!(call_font_int("joystickconfig.GetAxisMap", ""), -1);
        assert_eq!(call_font_int("screenjobrunner.isplayerready", ""), 0);
        assert_eq!(call_math("state.distanceto", &[3.0, 4.0]), 0.0);
        assert_eq!(call_font_int("state.validatespriteframe", ""), 1);
        assert_eq!(call_font_int("tex.isNull", ""), 1);
        assert_eq!(call_font_int("tex.IsValid", ""), 0);
        assert_eq!(call_font_int("text.IsEmpty", ""), 1);
        assert_eq!(call_font_int("text.IsEmpty", "Hi"), 0);
        assert_eq!(call_font_int("team.isvalid", ""), 0);
        assert_eq!(call_font_int("joystickconfig.GetAxisResponseCurve", ""), 0);
        assert_eq!(
            call_math("joystickconfig.GetAxisDigitalThreshold", &[4.0]),
            0.0
        );
        assert_eq!(call_math("secplane.ZatPoint", &[10.0, 20.0]), 0.0);
        assert_eq!(call_font_int("psprite.setstate", ""), 0);
        assert_eq!(call_font_int("strifedialoguereply.shouldskipreply", ""), 0);
        assert_eq!(call_font_int("super.HandlePickup", ""), 0);
        assert_eq!(call_font_int("playerpawn.ClearInventory", ""), 0);
        assert_eq!(call_font_int("optionmenuitemcommand.docommand", ""), 0);
        assert_eq!(call_font_int("super.MenuEvent", ""), 0);
        assert_eq!(call_font_int("super.OnInputEvent", ""), 0);
        assert_eq!(call_font_int("super.ShouldRespawn", ""), 0);
        assert_eq!(call_font_int("super.TryPickup", ""), 0);
        assert_eq!(call_font_int("super.Use", ""), 0);
        assert_eq!(call_font_int("super.RemoveInventory", ""), 0);
        assert_eq!(call_string("super.PickupMessage", "", &[], &[]), "");
        assert_eq!(call_font_int("text.ByteAt", "Hi"), 72);
        assert_eq!(call_font_int("weaponslots.LocateWeapon", ""), 0);
        assert_eq!(call_font_int("system.stopmusic", ""), 0);
        assert_eq!(call_font_int("textentermenu.ActivateMenu", ""), 0);
        assert_eq!(call_font_int("thinker.changestatnum", ""), 0);
        assert_eq!(call_string("systemtime.format", "", &[], &[]), "");
        assert_eq!(call_math("vect.Length", &[3.0, 4.0]), 5.0);
        assert_eq!(call_font_int("actor.instatesequence", ""), 0);
        assert_eq!(
            call_string("statusbarcore.formatnumber", "", &[], &[12]),
            "12"
        );
        assert_eq!(call_font_int("object.GetIndent", ""), 0);
        assert_eq!(call_string("object.GetTooltip", "", &[], &[]), "");
        assert_eq!(call_string("object.GetAction", "", &[], &[]), "");
        assert!((call_math("negviewpos.Angle", &[0.0, 1.0]) - 90.0).abs() < 1e-6);
        assert_eq!(
            call_math("levellocals.getautomapposition", &[5.0, 6.0]),
            0.0
        );
        assert_eq!(call_math("actor.rotatevector", &[3.0, 4.0]), 3.0);
        assert_eq!(call_math("actor.normalize180", &[190.0]), -170.0);
        assert_eq!(call_math("actor.normalize180", &[-190.0]), 170.0);
        let table = NativeTable::with_builtins();
        let cmp = table
            .get(table.id_ci("object.CompareNoCase").expect("cmp"))
            .expect("fn");
        let valid = table
            .get(table.id_ci("object.isValid").expect("valid"))
            .expect("fn");
        let mut heap = ZvmHeap::new();
        let mut play = ZvmPlayHost::default();
        let mut ctx = ZvmCtx {
            heap: &mut heap,
            host: 0,
            play: &mut play,
        };
        let mut i = [0i64; 1];
        let mut f = [0.0; 4];
        let mut s = [StrHandle::EMPTY; 2];
        s[0] = ctx.heap.intern_string("Ab");
        s[1] = ctx.heap.intern_string("ab");
        let mut a = [ObjHandle::NULL; 1];
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            cmp(&mut ctx, &mut regs).expect("cmp");
        }
        assert_eq!(i[0], 0);
        s[0] = ctx.heap.intern_string("a");
        s[1] = ctx.heap.intern_string("b");
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            cmp(&mut ctx, &mut regs).expect("order");
        }
        assert_eq!(i[0], -1);
        a[0] = ctx.heap.new_object(0, 1, 0, 0);
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            valid(&mut ctx, &mut regs).expect("live");
        }
        assert_eq!(i[0], 1);
        let plus = table
            .get(table.id_ci("object.PlusZ").expect("plus"))
            .expect("fn");
        f[0] = 32.0;
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            plus(&mut ctx, &mut regs).expect("plus");
        }
        assert_eq!(f[0], 0.0);
        assert_eq!(f[2], 32.0);
        let set_light = table
            .get(table.id_ci("object.SetLightLevel").expect("set"))
            .expect("fn");
        let get_light = table
            .get(table.id_ci("object.GetLightLevel").expect("get"))
            .expect("fn");
        a[0] = ctx.heap.new_object(0, 2, 0, 0);
        i[0] = 128;
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            set_light(&mut ctx, &mut regs).expect("set");
            get_light(&mut ctx, &mut regs).expect("get");
        }
        assert_eq!(i[0], 128);
        let set_x = table
            .get(table.id_ci("object.SetX").expect("sx"))
            .expect("fn");
        let get_x = table
            .get(table.id_ci("object.GetX").expect("gx"))
            .expect("fn");
        let set_value = table
            .get(table.id_ci("object.SetValue").expect("sv"))
            .expect("fn");
        let get_value = table
            .get(table.id_ci("object.GetValue").expect("gv"))
            .expect("fn");
        a[0] = ctx.heap.new_object(0, 4, 0, 0);
        f[0] = 12.5;
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            set_x(&mut ctx, &mut regs).expect("setx");
            get_x(&mut ctx, &mut regs).expect("getx");
        }
        assert!((f[0] - 12.5).abs() < 1e-9);
        a[0] = ctx.heap.new_object(0, 4, 0, 0);
        i[0] = 4;
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            set_value(&mut ctx, &mut regs).expect("setv");
            get_value(&mut ctx, &mut regs).expect("getv");
        }
        assert_eq!(i[0], 4);
        let set_string = table
            .get(table.id_ci("object.SetString").expect("ss"))
            .expect("fn");
        let get_string = table
            .get(table.id_ci("object.GetString").expect("gs"))
            .expect("fn");
        let next = table
            .get(table.id_ci("object.Next").expect("next"))
            .expect("fn");
        let push = table
            .get(table.id_ci("object.Push").expect("push"))
            .expect("fn");
        let pop = table
            .get(table.id_ci("object.Pop").expect("pop"))
            .expect("fn");
        a[0] = ctx.heap.new_object(0, 6, 0, 1);
        s[0] = ctx.heap.intern_string("Hello");
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            set_string(&mut ctx, &mut regs).expect("sets");
            get_string(&mut ctx, &mut regs).expect("gets");
        }
        assert_eq!(ctx.heap.str(s[1]), "Hello");
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            next(&mut ctx, &mut regs).expect("next");
        }
        assert!(a[0].is_null());
        a[0] = ctx.heap.new_array_i();
        i[0] = 7;
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            push(&mut ctx, &mut regs).expect("push");
            pop(&mut ctx, &mut regs).expect("pop");
        }
        assert_eq!(i[0], 7);
        let set_focus = table
            .get(table.id_ci("object.SetFocus").expect("sf"))
            .expect("fn");
        let check_focus = table
            .get(table.id_ci("object.CheckFocus").expect("cf"))
            .expect("fn");
        let find_inv = table
            .get(table.id_ci("object.FindInventory").expect("fi"))
            .expect("fn");
        let logo = table
            .get(table.id_ci("object.GetLogo").expect("logo"))
            .expect("fn");
        let delete = table
            .get(table.id_ci("object.Delete").expect("del"))
            .expect("fn");
        a[0] = ctx.heap.new_object(0, 4, 0, 0);
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            set_focus(&mut ctx, &mut regs).expect("focus");
            check_focus(&mut ctx, &mut regs).expect("check");
        }
        assert_eq!(i[0], 1);
        a[0] = ctx.heap.new_object(0, 1, 0, 0);
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            find_inv(&mut ctx, &mut regs).expect("inv");
        }
        assert!(a[0].is_null());
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            logo(&mut ctx, &mut regs).expect("logo");
        }
        assert!(!a[0].is_null());
        a[0] = ctx.heap.new_array_i();
        i[0] = 1;
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            push(&mut ctx, &mut regs).expect("push2");
        }
        i[0] = 2;
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            push(&mut ctx, &mut regs).expect("push3");
        }
        i[0] = 0;
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            delete(&mut ctx, &mut regs).expect("del");
            pop(&mut ctx, &mut regs).expect("left");
        }
        assert_eq!(i[0], 2);
        let add_z = table
            .get(table.id_ci("actor.addz").expect("az"))
            .expect("fn");
        let get_z = table
            .get(table.id_ci("object.GetZ").expect("gz"))
            .expect("fn");
        let change = table
            .get(table.id_ci("Actor.ChangeFlag").expect("chg"))
            .expect("fn");
        let check = table
            .get(table.id_ci("Actor.CheckFlag").expect("chk"))
            .expect("fn");
        let factor = table
            .get(table.id_ci("actor.ApplyDamageFactor").expect("dmg"))
            .expect("fn");
        ctx.play.actor_slots.pos = Some(0);
        a[0] = ctx.heap.new_object_f(0, 6, 0, 1, 3);
        f[0] = 5.0;
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            add_z(&mut ctx, &mut regs).expect("add");
        }
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            get_z(&mut ctx, &mut regs).expect("z");
        }
        assert!((f[0] - 5.0).abs() < 1e-9);
        f[0] = 2.0;
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            add_z(&mut ctx, &mut regs).expect("add2");
        }
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            get_z(&mut ctx, &mut regs).expect("z2");
        }
        assert!((f[0] - 7.0).abs() < 1e-9);
        s[0] = ctx.heap.intern_string("SOLID");
        i[0] = 1;
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            change(&mut ctx, &mut regs).expect("chg");
            check(&mut ctx, &mut regs).expect("chk");
        }
        assert_eq!(i[0], 1);
        i[0] = 15;
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            factor(&mut ctx, &mut regs).expect("dmg");
        }
        assert_eq!(i[0], 15);
        assert_eq!(ZvmPlayHost::default().cvar("sv_ammofactor"), 1);
    }

    #[test]
    fn dictionary_remove_serialize_and_iterate() {
        let table = NativeTable::with_builtins();
        let get = |n: &str| table.get(table.id_ci(n).expect(n)).expect("fn");
        let (make, put, remove, to_s, from_s) = (
            get("Dictionary.Create"),
            get("Dictionary.Insert"),
            get("Dictionary.Remove"),
            get("Dictionary.ToString"),
            get("Dictionary.FromString"),
        );
        let (iter, next, key, value) = (
            get("DictionaryIterator.Create"),
            get("DictionaryIterator.Next"),
            get("DictionaryIterator.Key"),
            get("DictionaryIterator.Value"),
        );
        let mut heap = ZvmHeap::new();
        let mut play = ZvmPlayHost::default();
        let mut ctx = ZvmCtx {
            heap: &mut heap,
            host: 0,
            play: &mut play,
        };
        let mut i = [0i64; 2];
        let mut f = [0.0; 2];
        let mut s = [StrHandle::EMPTY; 2];
        let mut a = [ObjHandle::NULL; 2];
        macro_rules! call {
            ($fn:expr) => {{
                let mut regs = NativeRegs {
                    i: &mut i,
                    f: &mut f,
                    s: &mut s,
                    a: &mut a,
                };
                $fn(&mut ctx, &mut regs).expect("native");
            }};
        }
        call!(make);
        let dict = a[0];
        for (k, v) in [("a", "1"), ("b", "two \"q\""), ("c", "3")] {
            s[0] = ctx.heap.intern_string(k);
            s[1] = ctx.heap.intern_string(v);
            a[0] = dict;
            call!(put);
        }
        s[0] = ctx.heap.intern_string("c");
        a[0] = dict;
        call!(remove);
        a[0] = dict;
        call!(to_s);
        let text = ctx.heap.str(s[0]).to_owned();
        assert_eq!(text, r#"{"a":"1","b":"two \"q\""}"#);
        s[0] = ctx.heap.intern_string(&text);
        call!(from_s);
        let copy = a[0];
        assert!(!copy.is_null() && copy != dict);
        call!(iter);
        let it = a[0];
        let mut seen = Vec::new();
        loop {
            a[0] = it;
            call!(next);
            if i[0] == 0 {
                break;
            }
            a[0] = it;
            call!(key);
            let k = ctx.heap.str(s[0]).to_owned();
            a[0] = it;
            call!(value);
            seen.push((k, ctx.heap.str(s[0]).to_owned()));
        }
        assert_eq!(
            seen,
            vec![("a".into(), "1".into()), ("b".into(), "two \"q\"".into())]
        );
        s[0] = ctx.heap.intern_string("null");
        call!(from_s);
        assert!(a[0].is_null());
        s[0] = ctx.heap.intern_string("[1]");
        let mut regs = NativeRegs {
            i: &mut i,
            f: &mut f,
            s: &mut s,
            a: &mut a,
        };
        assert!(from_s(&mut ctx, &mut regs).is_err());
    }

    #[test]
    fn actor_missile_damage_keeps_the_add_and_pointers_are_null() {
        let table = NativeTable::with_builtins();
        let missile = table
            .get(table.id_ci("actor.getmissiledamage").expect("md"))
            .expect("fn");
        let modified = table
            .get(table.id_ci("actor.getmodifieddamage").expect("mod"))
            .expect("fn");
        let pointer = table
            .get(table.id_ci("actor.getpointer").expect("ptr"))
            .expect("fn");
        let mut heap = ZvmHeap::new();
        let mut play = ZvmPlayHost::default();
        let mut ctx = ZvmCtx {
            heap: &mut heap,
            host: 0,
            play: &mut play,
        };
        let mut i = [7i64, 5];
        let mut f = [0.0; 4];
        let mut s = [StrHandle::EMPTY; 1];
        let mut a = [ObjHandle::NULL; 1];
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            missile(&mut ctx, &mut regs).expect("missile");
        }
        assert_eq!(i[0], 5);
        i[0] = 15;
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            modified(&mut ctx, &mut regs).expect("mod");
        }
        assert_eq!(i[0], 15);
        a[0] = ctx.heap.new_object(0, 1, 0, 0);
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            pointer(&mut ctx, &mut regs).expect("ptr");
        }
        assert!(a[0].is_null());
        let look = table
            .get(table.id_ci("actor.lookforplayers").expect("look"))
            .expect("fn");
        let die = table
            .get(table.id_ci("actor.die").expect("die"))
            .expect("fn");
        let explode = table
            .get(table.id_ci("actor.explodemissile").expect("ex"))
            .expect("fn");
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            look(&mut ctx, &mut regs).expect("look");
        }
        assert!(matches!(
            ctx.play.actor_ops.last(),
            Some(super::ZsActorOp::Look)
        ));
        i[0] = 128;
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            explode(&mut ctx, &mut regs).expect("explode");
            die(&mut ctx, &mut regs).expect("die");
        }
        match ctx
            .play
            .actor_ops
            .get(ctx.play.actor_ops.len().wrapping_sub(2))
        {
            Some(super::ZsActorOp::Action { name, args }) => {
                assert_eq!(name, "A_Explode");
                assert_eq!(args[0], 128);
            }
            other => panic!("explode op {other:?}"),
        }
        match ctx.play.actor_ops.last() {
            Some(super::ZsActorOp::Action { name, args }) => {
                assert_eq!(name, "A_Die");
                assert_eq!(*args, [0; 4]);
            }
            other => panic!("die op {other:?}"),
        }
        let chase = table
            .get(table.id_ci("actor.monstermove").expect("move"))
            .expect("fn");
        let rail = table
            .get(table.id_ci("actor.railattack").expect("rail"))
            .expect("fn");
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            chase(&mut ctx, &mut regs).expect("chase");
        }
        assert!(matches!(
            ctx.play.actor_ops.last(),
            Some(super::ZsActorOp::Chase { .. })
        ));
        i[0] = 40;
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            rail(&mut ctx, &mut regs).expect("rail");
        }
        match ctx.play.actor_ops.last() {
            Some(super::ZsActorOp::Action { name, args }) => {
                assert_eq!(name, "A_RailAttack");
                assert_eq!(args[0], 40);
            }
            other => panic!("rail op {other:?}"),
        }
        let alert = table
            .get(table.id_ci("actor.soundalert").expect("alert"))
            .expect("fn");
        let setz = table
            .get(table.id_ci("actor.setz").expect("setz"))
            .expect("fn");
        let spawn = table
            .get(table.id_ci("actor.spawnmissile").expect("spawn"))
            .expect("fn");
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            alert(&mut ctx, &mut regs).expect("alert");
        }
        assert!(matches!(
            ctx.play.actor_ops.last(),
            Some(super::ZsActorOp::Look)
        ));
        // `SetZ` writes the mirrored `pos.z` float field.
        ctx.play.actor_slots.pos = Some(0);
        a[0] = ctx.heap.new_object_f(0, 6, 0, 0, 3);
        f[0] = 32.0;
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            setz(&mut ctx, &mut regs).expect("setz");
        }
        match ctx.heap.get(a[0]) {
            Some(crate::heap::ObjKind::Object { fields_f, .. }) => {
                assert_eq!(fields_f[2], 32.0);
            }
            other => panic!("setz object {other:?}"),
        }
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            spawn(&mut ctx, &mut regs).expect("spawn");
        }
        assert!(a[0].is_null());
        let stop = table
            .get(table.id_ci("actor.stopsoundsequence").expect("stop"))
            .expect("fn");
        let setstate = table
            .get(table.id_ci("actor.setstate").expect("state"))
            .expect("fn");
        let vel3 = table
            .get(table.id_ci("actor.vel3dfromangle").expect("vel3"))
            .expect("fn");
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            stop(&mut ctx, &mut regs).expect("stop");
        }
        match ctx.play.actor_ops.last() {
            Some(super::ZsActorOp::Action { name, .. }) => assert_eq!(name, "A_StopSound"),
            other => panic!("stop op {other:?}"),
        }
        i[0] = 1;
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            setstate(&mut ctx, &mut regs).expect("state");
        }
        assert_eq!(i[0], 0);
        f[0] = 10.0;
        f[1] = 0.0;
        f[2] = 90.0;
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            vel3(&mut ctx, &mut regs).expect("vel3");
        }
        assert_eq!(f[2], 10.0);
        let armor = table
            .get(table.id_ci("actor.GetBasicArmorClass").expect("armor"))
            .expect("fn");
        let search = table
            .get(table.id_ci("actor.RoughMonsterSearch").expect("search"))
            .expect("fn");
        let factors = table
            .get(table.id_ci("actor.applydamagefactors").expect("factors"))
            .expect("fn");
        let destroy = table
            .get(table.id_ci("actor.Destroy").expect("destroy"))
            .expect("fn");
        a[0] = ctx.heap.new_object(0, 1, 0, 0);
        i[0] = 9;
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            armor(&mut ctx, &mut regs).expect("armor");
        }
        assert!(a[0].is_null());
        a[0] = ctx.heap.new_object(0, 1, 0, 0);
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            search(&mut ctx, &mut regs).expect("search");
            factors(&mut ctx, &mut regs).expect("factors");
            destroy(&mut ctx, &mut regs).expect("destroy");
        }
        assert!(a[0].is_null());
        assert_eq!(i[0], 9);
        match ctx.play.actor_ops.last() {
            Some(super::ZsActorOp::Action { name, .. }) => assert_eq!(name, "A_Die"),
            other => panic!("destroy op {other:?}"),
        }
        let morph = table
            .get(table.id_ci("actor.morphinto").expect("morph"))
            .expect("fn");
        let number = table
            .get(table.id_ci("actor.playernumber").expect("num"))
            .expect("fn");
        let bone = table
            .get(table.id_ci("actor.getbonebaseposition").expect("bone"))
            .expect("fn");
        a[0] = ctx.heap.new_object(0, 1, 0, 0);
        i[0] = 4;
        f[0] = 1.0;
        f[1] = 2.0;
        f[2] = 3.0;
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            morph(&mut ctx, &mut regs).expect("morph");
        }
        assert!(a[0].is_null());
        {
            let mut regs = NativeRegs {
                i: &mut i,
                f: &mut f,
                s: &mut s,
                a: &mut a,
            };
            number(&mut ctx, &mut regs).expect("number");
            bone(&mut ctx, &mut regs).expect("bone");
        }
        assert_eq!(i[0], 0);
        assert_eq!(f, [0.0, 0.0, 0.0, 0.0]);
    }
}
