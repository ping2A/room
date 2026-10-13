//! Scalar SFMT-607 (Saito & Matsumoto). Output matches the official C reference
//! and GZDoom's `SFMTObj` when SIMD is disabled.
//!
//! Copyright (C) 2006, 2007 Mutsuo Saito, Makoto Matsumoto and Hiroshima
//! University. All rights reserved. New BSD License; see `THIRD_PARTY.md`.

#![allow(
    clippy::unreadable_literal,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap
)]

const N: usize = 5;
pub(crate) const N32: usize = N * 4;
const IDX_N32: i32 = 20;
const POS1: usize = 2;
const SL1: u32 = 15;
const SL2: u32 = 3;
const SR1: u32 = 13;
const SR2: u32 = 3;
const MSK: [u32; 4] = [0xfdff37ff, 0xef7f3f7d, 0xff777b7d, 0x7ff7fb2f];
const PARITY: [u32; 4] = [0x00000001, 0x00000000, 0x00000000, 0x5986f054];

#[derive(Clone, Copy)]
struct W128 {
    u: [u32; 4],
}

/// SFMT-607 generator state (`SFMTObj` in GZDoom).
#[derive(Clone)]
pub struct Sfmt607 {
    idx: i32,
    state: [W128; N],
}

impl Default for Sfmt607 {
    fn default() -> Self {
        Self::new()
    }
}

impl Sfmt607 {
    /// Uninitialised (must call [`Self::init_gen_rand`] or [`Self::init_by_array`]).
    #[must_use]
    pub const fn new() -> Self {
        Self {
            idx: 0,
            state: [W128 { u: [0; 4] }; N],
        }
    }

    fn u32_at(&self, i: usize) -> u32 {
        self.state[i / 4].u[i % 4]
    }

    fn set_u32_at(&mut self, i: usize, v: u32) {
        self.state[i / 4].u[i % 4] = v;
    }

    /// `sfmt_init_gen_rand`.
    pub fn init_gen_rand(&mut self, seed: u32) {
        self.set_u32_at(0, seed);
        for i in 1..N32 {
            let prev = self.u32_at(i - 1);
            self.set_u32_at(
                i,
                1_812_433_253u32
                    .wrapping_mul(prev ^ (prev >> 30))
                    .wrapping_add(i as u32),
            );
        }
        self.idx = IDX_N32;
        self.period_certification();
    }

    /// `sfmt_init_by_array`. GZDoom `FRandom::Init` passes `[NameCRC, seed]`.
    pub fn init_by_array(&mut self, init_key: &[u32]) {
        let key_length = init_key.len();
        let size = N * 4;
        let lag = if size >= 623 {
            11
        } else if size >= 68 {
            7
        } else if size >= 39 {
            5
        } else {
            3
        };
        let mid = (size - lag) / 2;

        for i in 0..N32 {
            self.set_u32_at(i, 0x8b8b_8b8b);
        }

        let count = if key_length + 1 > N32 {
            key_length + 1
        } else {
            N32
        };

        let mut r = func1(self.u32_at(0) ^ self.u32_at(mid) ^ self.u32_at(N32 - 1));
        let mid_v = self.u32_at(mid).wrapping_add(r);
        self.set_u32_at(mid, mid_v);
        r = r.wrapping_add(key_length as u32);
        let mid_lag = self.u32_at(mid + lag).wrapping_add(r);
        self.set_u32_at(mid + lag, mid_lag);
        self.set_u32_at(0, r);

        let count = count - 1;
        let mut i = 1usize;
        let mut j = 0usize;
        while j < count && j < key_length {
            r = func1(
                self.u32_at(i) ^ self.u32_at((i + mid) % N32) ^ self.u32_at((i + N32 - 1) % N32),
            );
            let v = self.u32_at((i + mid) % N32).wrapping_add(r);
            self.set_u32_at((i + mid) % N32, v);
            r = r.wrapping_add(init_key[j].wrapping_add(i as u32));
            let v = self.u32_at((i + mid + lag) % N32).wrapping_add(r);
            self.set_u32_at((i + mid + lag) % N32, v);
            self.set_u32_at(i, r);
            i = (i + 1) % N32;
            j += 1;
        }
        while j < count {
            r = func1(
                self.u32_at(i) ^ self.u32_at((i + mid) % N32) ^ self.u32_at((i + N32 - 1) % N32),
            );
            let v = self.u32_at((i + mid) % N32).wrapping_add(r);
            self.set_u32_at((i + mid) % N32, v);
            r = r.wrapping_add(i as u32);
            let v = self.u32_at((i + mid + lag) % N32).wrapping_add(r);
            self.set_u32_at((i + mid + lag) % N32, v);
            self.set_u32_at(i, r);
            i = (i + 1) % N32;
            j += 1;
        }
        for _ in 0..N32 {
            r = func2(
                self.u32_at(i)
                    .wrapping_add(self.u32_at((i + mid) % N32))
                    .wrapping_add(self.u32_at((i + N32 - 1) % N32)),
            );
            let v = self.u32_at((i + mid) % N32) ^ r;
            self.set_u32_at((i + mid) % N32, v);
            r = r.wrapping_sub(i as u32);
            let v = self.u32_at((i + mid + lag) % N32) ^ r;
            self.set_u32_at((i + mid + lag) % N32, v);
            self.set_u32_at(i, r);
            i = (i + 1) % N32;
        }
        self.idx = IDX_N32;
        self.period_certification();
    }

    fn period_certification(&mut self) {
        let mut inner = 0u32;
        for (i, parity) in PARITY.iter().enumerate() {
            inner ^= self.u32_at(i) & *parity;
        }
        let mut shift = 16u32;
        while shift > 0 {
            inner ^= inner >> shift;
            shift >>= 1;
        }
        inner &= 1;
        if inner == 1 {
            return;
        }
        for (i, parity) in PARITY.iter().enumerate() {
            let mut work = 1u32;
            for _ in 0..32 {
                if work & *parity != 0 {
                    let v = self.u32_at(i) ^ work;
                    self.set_u32_at(i, v);
                    return;
                }
                work <<= 1;
            }
        }
    }

    fn gen_rand_all(&mut self) {
        let mut r1 = self.state[N - 2];
        let mut r2 = self.state[N - 1];
        for i in 0..(N - POS1) {
            let a = self.state[i];
            let b = self.state[i + POS1];
            self.state[i] = do_recursion(a, b, r1, r2);
            r1 = r2;
            r2 = self.state[i];
        }
        for i in (N - POS1)..N {
            let a = self.state[i];
            let b = self.state[i + POS1 - N];
            self.state[i] = do_recursion(a, b, r1, r2);
            r1 = r2;
            r2 = self.state[i];
        }
    }

    /// Next 32-bit output (`GenRand32`).
    pub fn gen_rand32(&mut self) -> u32 {
        if self.idx >= IDX_N32 {
            self.gen_rand_all();
            self.idx = 0;
        }
        let r = self.u32_at(self.idx as usize);
        self.idx += 1;
        r
    }

    /// Index into the 32-bit state (for savegames / checksums).
    #[must_use]
    pub const fn idx(&self) -> i32 {
        self.idx
    }

    /// First 32-bit word of state (GZDoom `Seed()` uses `sfmt.u[0] + idx`).
    #[must_use]
    pub fn state0(&self) -> u32 {
        self.u32_at(0)
    }

    /// Full 32-bit state words, for serialisation.
    #[must_use]
    pub fn state_words(&self) -> [u32; N32] {
        let mut out = [0u32; N32];
        for (i, slot) in out.iter_mut().enumerate() {
            *slot = self.u32_at(i);
        }
        out
    }

    /// Restore from a save snapshot.
    pub fn restore(&mut self, idx: i32, words: &[u32; N32]) {
        for (i, w) in words.iter().enumerate() {
            self.set_u32_at(i, *w);
        }
        self.idx = idx;
    }
}

fn func1(x: u32) -> u32 {
    (x ^ (x >> 27)).wrapping_mul(1_664_525)
}

fn func2(x: u32) -> u32 {
    (x ^ (x >> 27)).wrapping_mul(1_566_083_941)
}

fn lshift128(input: W128, shift: u32) -> W128 {
    let th = (u64::from(input.u[3]) << 32) | u64::from(input.u[2]);
    let tl = (u64::from(input.u[1]) << 32) | u64::from(input.u[0]);
    let ol = tl << (shift * 8);
    let oh = (th << (shift * 8)) | (tl >> (64 - shift * 8));
    W128 {
        u: [ol as u32, (ol >> 32) as u32, oh as u32, (oh >> 32) as u32],
    }
}

fn rshift128(input: W128, shift: u32) -> W128 {
    let th = (u64::from(input.u[3]) << 32) | u64::from(input.u[2]);
    let tl = (u64::from(input.u[1]) << 32) | u64::from(input.u[0]);
    let oh = th >> (shift * 8);
    let mut ol = tl >> (shift * 8);
    ol |= th << (64 - shift * 8);
    W128 {
        u: [ol as u32, (ol >> 32) as u32, oh as u32, (oh >> 32) as u32],
    }
}

fn do_recursion(a: W128, b: W128, c: W128, d: W128) -> W128 {
    let x = lshift128(a, SL2);
    let y = rshift128(c, SR2);
    W128 {
        u: [
            a.u[0] ^ x.u[0] ^ ((b.u[0] >> SR1) & MSK[0]) ^ y.u[0] ^ (d.u[0] << SL1),
            a.u[1] ^ x.u[1] ^ ((b.u[1] >> SR1) & MSK[1]) ^ y.u[1] ^ (d.u[1] << SL1),
            a.u[2] ^ x.u[2] ^ ((b.u[2] >> SR1) & MSK[2]) ^ y.u[2] ^ (d.u[2] << SL1),
            a.u[3] ^ x.u[3] ^ ((b.u[3] >> SR1) & MSK[3]) ^ y.u[3] ^ (d.u[3] << SL1),
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Official SFMT-607 `init_gen_rand(1234)` first 20 outputs
    /// (`SFMT.607.out.txt`).
    const GEN_RAND: [u32; 20] = [
        1196421539, 2865311212, 3866479472, 2692900087, 3838928621, 3188765817, 2751632982,
        3712143069, 549918413, 2026167923, 1600383423, 1164248707, 3597250909, 1646078700,
        3597866549, 738391729, 1195673494, 320418194, 3419357688, 4060638541,
    ];

    /// Official SFMT-607 `init_by_array({0x1234,0x5678,0x9abc,0xdef0})` first 20.
    const BY_ARRAY: [u32; 20] = [
        1556592192, 2713881341, 1840174392, 3468073604, 2004354026, 1304494515, 366015052,
        1011850603, 67279901, 1051755907, 1308953792, 1479381587, 1197340276, 2749666614,
        1052362076, 168556043, 2392613760, 3444358776, 1926822224, 1889847275,
    ];

    #[test]
    fn init_gen_rand_reference() {
        let mut s = Sfmt607::new();
        s.init_gen_rand(1234);
        for (i, expected) in GEN_RAND.iter().enumerate() {
            assert_eq!(s.gen_rand32(), *expected, "index {i}");
        }
    }

    #[test]
    fn init_by_array_reference() {
        let mut s = Sfmt607::new();
        s.init_by_array(&[0x1234, 0x5678, 0x9abc, 0xdef0]);
        for (i, expected) in BY_ARRAY.iter().enumerate() {
            assert_eq!(s.gen_rand32(), *expected, "index {i}");
        }
    }
}
