//! Integer scale helpers and `krand` — see `docs/build-engine-primitives/00-numeric.md`.

use std::sync::OnceLock;

/// `(a * b) >> s` with wrapping 32-bit result.
#[inline]
pub fn mulscale(a: i32, b: i32, s: u32) -> i32 {
    let s = s.min(63);
    ((i64::from(a) * i64::from(b)) >> s) as i32
}

/// `(a << s) / b` with wrapping 32-bit result.
///
/// Division by zero returns `0` until goldens define vanilla behaviour.
#[inline]
pub fn divscale(a: i32, b: i32, s: u32) -> i32 {
    if b == 0 {
        return 0;
    }
    let s = s.min(63);
    ((i64::from(a) << s) / i64::from(b)) as i32
}

/// `((a*b) + (c*d)) >> s`.
#[inline]
pub fn dmulscale(a: i32, b: i32, c: i32, d: i32, s: u32) -> i32 {
    let s = s.min(63);
    let acc = i64::from(a) * i64::from(b) + i64::from(c) * i64::from(d);
    (acc >> s) as i32
}

/// `(a * b) / c`. `/0` → `0` until goldens say otherwise.
#[inline]
pub fn scale(a: i32, b: i32, c: i32) -> i32 {
    if c == 0 {
        return 0;
    }
    ((i64::from(a) * i64::from(b)) / i64::from(c)) as i32
}

/// Absolute value; `i32::MIN` stays `i32::MIN` (two's-complement wrap style).
#[inline]
pub fn klabs(x: i32) -> i32 {
    if x == i32::MIN { i32::MIN } else { x.abs() }
}

/// Integer square root of a non-negative value. Negative → `0`.
#[inline]
pub fn ksqrt(n: i32) -> i32 {
    if n <= 0 {
        return 0;
    }
    // Babylonian / integer isqrt.
    let mut x = n as u32;
    let mut y = (x + 1) >> 1;
    while y < x {
        x = y;
        y = (x + (n as u32) / x) >> 1;
    }
    x as i32
}

/// Chocolate `msqrtasm` — Ken's integer sqrt used to build `nsqrtasm` tables.
#[inline]
pub fn msqrtasm(mut input: u32) -> u32 {
    let mut a: u32 = 0x4000_0000;
    let mut b: u32 = 0x2000_0000;
    loop {
        if input >= a {
            input -= a;
            a = a.wrapping_add(b.wrapping_mul(4));
        }
        a = a.wrapping_sub(b);
        a >>= 1;
        b >>= 2;
        if b == 0 {
            break;
        }
    }
    if input >= a {
        a = a.wrapping_add(1);
    }
    a >> 1
}

struct NsqrtTables {
    sqrtable: [u16; 4096],
    shlookup: [u16; 4096 + 256],
}

fn nsqrt_tables() -> &'static NsqrtTables {
    static TABLES: OnceLock<NsqrtTables> = OnceLock::new();
    TABLES.get_or_init(|| {
        let mut sqrtable = [0u16; 4096];
        let mut shlookup = [0u16; 4096 + 256];
        let mut j: i32 = 1;
        let mut k: i32 = 0;
        for i in 0..4096i32 {
            if i >= j {
                j <<= 2;
                k += 1;
            }
            sqrtable[i as usize] = (msqrtasm(((i as u32) << 18).wrapping_add(131_072)) << 1) as u16;
            shlookup[i as usize] = ((k << 1) + ((10 - k) << 8)) as u16;
            if i < 256 {
                shlookup[(i + 4096) as usize] = (((k + 6) << 1) + ((10 - (k + 6)) << 8)) as u16;
            }
        }
        NsqrtTables { sqrtable, shlookup }
    })
}

/// Chocolate `nsqrtasm` — fixed-point sqrt used by `getzsofslope` / clip.
#[inline]
pub fn nsqrtasm(mut param: u32) -> i32 {
    let t = nsqrt_tables();
    let cx = if param & 0xff00_0000 != 0 {
        t.shlookup[((param >> 24) as usize) + 4096]
    } else {
        t.shlookup[(param >> 12) as usize]
    };
    param >>= cx & 0xff;
    let idx = (param & 0xffff) as usize;
    param = (param & 0xffff_0000) | u32::from(t.sqrtable[idx.min(4095)]);
    param >>= (cx & 0xff00) >> 8;
    param as i32
}

/// Advance Build `krand` LCG; returns high 16 bits of the new seed as `i32`.
///
/// Draft from `docs/build-engine-primitives/00-numeric.md` — verify vs DEMO traces.
#[inline]
#[track_caller]
pub fn krand_step(randomseed: &mut i32) -> i32 {
    krand_log::record(*randomseed);
    *randomseed = randomseed.wrapping_mul(2_758_4621).wrapping_add(1);
    ((*randomseed as u32) >> 16) as i32
}

/// Opt-in per-call `krand` log for sync debugging: the seed *before* each
/// call and the Rust call site (`#[track_caller]`). Compare with a reference
/// engine that logs the seed before each `krand()` (`docs/duke-trace-howto.md`).
pub mod krand_log {
    use std::cell::RefCell;
    use std::panic::Location;

    type Entry = (i32, &'static Location<'static>);
    thread_local! {
        static LOG: RefCell<Option<Vec<Entry>>> = const { RefCell::new(None) };
    }

    /// Start recording on this thread (clears an earlier log).
    pub fn start() {
        LOG.with(|l| *l.borrow_mut() = Some(Vec::new()));
    }

    /// Stop recording and return the calls since [`start`].
    #[must_use]
    pub fn take() -> Vec<(i32, String)> {
        LOG.with(|l| {
            l.borrow_mut()
                .take()
                .unwrap_or_default()
                .into_iter()
                .map(|(s, loc)| (s, format!("{}:{}", loc.file(), loc.line())))
                .collect()
        })
    }

    /// Calls recorded so far (keeps recording).
    #[must_use]
    pub fn drain() -> Vec<(i32, String)> {
        LOG.with(|l| {
            l.borrow_mut()
                .as_mut()
                .map(|v| {
                    v.drain(..)
                        .map(|(s, loc)| (s, format!("{}:{}", loc.file(), loc.line())))
                        .collect()
                })
                .unwrap_or_default()
        })
    }

    #[track_caller]
    pub(super) fn record(seed: i32) {
        let at = Location::caller();
        LOG.with(|l| {
            if let Some(v) = l.borrow_mut().as_mut() {
                v.push((seed, at));
            }
        });
    }
}

/// Inverse of [`krand_step`]'s seed update (`seed = seed*27584621+1`).
///
/// Used to rewind TEMP over-consumed TRANDs when sprite state already matches.
pub fn krand_unstep(randomseed: &mut i32) {
    // Modular inverse of 27584621 mod 2^32.
    const INV: u32 = 133_914_981;
    let s = (*randomseed as u32).wrapping_sub(1).wrapping_mul(INV);
    *randomseed = s as i32;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mulscale_basic() {
        assert_eq!(mulscale(4, 4, 2), 4);
        assert_eq!(mulscale(-4, 4, 2), -4);
    }

    #[test]
    fn divscale_by_zero_is_zero() {
        assert_eq!(divscale(100, 0, 16), 0);
    }

    #[test]
    fn klabs_min() {
        assert_eq!(klabs(i32::MIN), i32::MIN);
        assert_eq!(klabs(-5), 5);
    }

    #[test]
    fn ksqrt_squares() {
        assert_eq!(ksqrt(0), 0);
        assert_eq!(ksqrt(1), 1);
        assert_eq!(ksqrt(144), 12);
        assert_eq!(ksqrt(-3), 0);
    }

    #[test]
    fn krand_steps_and_wraps() {
        let mut s = 1_i32;
        let a = krand_step(&mut s);
        let b = krand_step(&mut s);
        assert_ne!(a, b);
        // Deterministic from seed 1:
        let mut s2 = 1_i32;
        assert_eq!(krand_step(&mut s2), a);
    }

    #[test]
    fn krand_unstep_inverts_step() {
        let mut s = 573_012_167_i32;
        let _ = krand_step(&mut s);
        let after = s;
        krand_unstep(&mut s);
        assert_eq!(s, 573_012_167);
        let _ = krand_step(&mut s);
        assert_eq!(s, after);
    }
}
