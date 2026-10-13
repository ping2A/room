//! SFMT-607 named generators (`FRandom`) plus vanilla `rndtable` and UI RNG.
//!
//! Seeding matches GZDoom `FRandom::Init`: `InitByArray([CRC32(name), rngseed])`.

mod sfmt;
mod vanilla;

use crate::error::CoreError;
use std::collections::BTreeMap;

pub use sfmt::Sfmt607;
pub use vanilla::{RNDTABLE, VanillaRng};

/// zlib/IEEE CRC-32 of a generator name (GZDoom `CalcCRC32` / miniz).
#[must_use]
pub fn name_crc(name: &str) -> u32 {
    crc32fast::hash(name.as_bytes())
}

/// One named SFMT stream (GZDoom `FRandom`).
#[derive(Clone)]
pub struct NamedRng {
    name: String,
    crc: u32,
    sfmt: Sfmt607,
}

impl NamedRng {
    /// `operator()` — `GenRand32() & 255`.
    #[allow(clippy::cast_possible_truncation)]
    pub fn random(&mut self) -> u8 {
        (self.sfmt.gen_rand32() & 255) as u8
    }

    /// `operator()(mod)` — `GenRand32() % mod`, or 0 if `mod == 0`.
    pub fn random_mod(&mut self, modulus: u32) -> u32 {
        if modulus == 0 {
            0
        } else {
            self.sfmt.gen_rand32() % modulus
        }
    }

    /// `(rand & mask & 255) - (rand & mask & 255)`.
    #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
    pub fn random2(&mut self, mask: u32) -> i32 {
        let t = (self.sfmt.gen_rand32() & mask & 255) as i32;
        t - ((self.sfmt.gen_rand32() & mask & 255) as i32)
    }

    /// Heretic/Hexen `HITDICE`: `(1 + (GenRand32() & 7)) * count`.
    #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
    pub fn hit_dice(&mut self, count: i32) -> i32 {
        (1 + (self.sfmt.gen_rand32() & 7) as i32) * count
    }

    /// Raw 32-bit output.
    pub fn gen_rand32(&mut self) -> u32 {
        self.sfmt.gen_rand32()
    }

    /// `[0, 1]` real (`v / (2^32 - 1)`).
    #[must_use]
    pub fn gen_rand_real1(&mut self) -> f64 {
        f64::from(self.sfmt.gen_rand32()) * (1.0 / 4_294_967_295.0)
    }

    /// `[0, 1)` real (`v / 2^32`).
    #[must_use]
    pub fn gen_rand_real2(&mut self) -> f64 {
        f64::from(self.sfmt.gen_rand32()) * (1.0 / 4_294_967_296.0)
    }

    /// Generator name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// CRC used as the first `InitByArray` seed.
    #[must_use]
    pub fn crc(&self) -> u32 {
        self.crc
    }

    /// Checksum contribution (`sfmt.u[0] + idx` in GZDoom `FRandom::Seed`).
    #[must_use]
    #[allow(clippy::cast_sign_loss)]
    pub fn seed_token(&self) -> u32 {
        self.sfmt.state0().wrapping_add(self.sfmt.idx() as u32)
    }
}

/// The full named RNG set for a world. Iteration is CRC-sorted (`BTreeMap`).
#[derive(Clone, Default)]
pub struct RngSet {
    seed: u32,
    gens: BTreeMap<u32, NamedRng>,
}

impl RngSet {
    /// Empty set with global seed 0.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Global `rngseed`.
    #[must_use]
    pub const fn seed(&self) -> u32 {
        self.seed
    }

    /// Reseed every named generator (`FRandom::StaticClearRandom`).
    pub fn set_seed(&mut self, seed: u32) {
        self.seed = seed;
        for named in self.gens.values_mut() {
            named.sfmt.init_by_array(&[named.crc, seed]);
        }
    }

    /// Get or create a named generator (`FRandom::StaticFindRNG`).
    pub fn get_or_insert(&mut self, name: &str) -> Result<&mut NamedRng, CoreError> {
        let crc = name_crc(name);
        if crc == 0 {
            return Err(CoreError::ReservedRngName(name.to_owned()));
        }
        let seed = self.seed;
        Ok(self.gens.entry(crc).or_insert_with(|| {
            let mut sfmt = Sfmt607::new();
            sfmt.init_by_array(&[crc, seed]);
            NamedRng {
                name: name.to_owned(),
                crc,
                sfmt,
            }
        }))
    }

    /// `rng.random(name) -> u8`.
    pub fn random(&mut self, name: &str) -> Result<u8, CoreError> {
        Ok(self.get_or_insert(name)?.random())
    }

    /// Iterate generators in CRC order (save / checksum).
    pub fn iter(&self) -> impl Iterator<Item = &NamedRng> {
        self.gens.values()
    }
}

/// Non-sim RNG (menus, UI, cosmetic audio). Never use inside `World::tick`.
#[derive(Clone)]
pub struct UiRng {
    sfmt: Sfmt607,
}

impl UiRng {
    /// Seed independently of the playsim.
    #[must_use]
    pub fn new(seed: u32) -> Self {
        let mut sfmt = Sfmt607::new();
        sfmt.init_gen_rand(seed);
        Self { sfmt }
    }

    /// Next 32-bit value.
    pub fn gen_rand32(&mut self) -> u32 {
        self.sfmt.gen_rand32()
    }

    /// Next byte.
    #[allow(clippy::cast_possible_truncation)]
    pub fn random(&mut self) -> u8 {
        (self.sfmt.gen_rand32() & 255) as u8
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    #[test]
    fn same_seed_and_name_same_stream() {
        let mut a = RngSet::new();
        a.set_seed(0x1234_5678);
        let mut b = RngSet::new();
        b.set_seed(0x1234_5678);
        let va: Vec<u8> = (0..16).map(|_| a.random("pr_chase").unwrap()).collect();
        let vb: Vec<u8> = (0..16).map(|_| b.random("pr_chase").unwrap()).collect();
        assert_eq!(va, vb);
    }

    #[test]
    fn different_names_diverge() {
        let mut a = RngSet::new();
        a.set_seed(1);
        let x = a.random("pr_chase").unwrap();
        let y = a.random("pr_spawnmobj").unwrap();
        // Extremely likely to differ; if they match, streams still have independent state.
        let _ = (x, y);
        assert_ne!(
            a.get_or_insert("pr_chase").unwrap().seed_token(),
            a.get_or_insert("pr_spawnmobj").unwrap().seed_token()
        );
    }

    #[test]
    fn insert_order_does_not_change_streams() {
        let mut a = RngSet::new();
        a.set_seed(99);
        a.get_or_insert("pr_acs").unwrap();
        a.get_or_insert("pr_chase").unwrap();
        let x = a.random("pr_chase").unwrap();

        let mut b = RngSet::new();
        b.set_seed(99);
        b.get_or_insert("pr_chase").unwrap();
        b.get_or_insert("pr_acs").unwrap();
        let y = b.random("pr_chase").unwrap();
        assert_eq!(x, y);
    }
}
