//! Retained battle-royale zone rings for rendering (`docs/28` §9 BE1).
//!
//! Zones are output-only sim state; shells draw them. Not part of
//! [`crate::World::checksum`].

use std::collections::BTreeMap;
use xxhash_rust::xxh3::xxh3_64;

/// Max simultaneous zone slots (`id` 0..=`MAX_ZONES - 1`).
pub const MAX_ZONES: usize = 4;

/// Zone flags (guest `zone_set` / [`ModZone::flags`]).
pub mod flags {
    /// Do not draw (store only).
    pub const HIDDEN: u8 = 1 << 0;
    /// Future wgpu wall visible through geometry.
    pub const THROUGH_WALLS: u8 = 1 << 1;
}

/// Error codes returned to guests (`0` = ok).
pub mod err {
    /// Parameters invalid.
    pub const BAD_PARAMS: i32 = -1;
    /// More than `MAX_ZONES` distinct ids.
    pub const CAP: i32 = -2;
    /// Unknown zone id on clear.
    pub const MISSING: i32 = -3;
}

/// One zone ring (current + next preview).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ModZone {
    /// Slot id 0..3.
    pub id: u8,
    /// Owning play module (hot-swap cleanup).
    pub owner_module: u16,
    /// Current centre (map units).
    pub cx: i32,
    /// Current centre Y.
    pub cy: i32,
    /// Current radius (map units).
    pub r: i32,
    /// Next phase centre X.
    pub next_cx: i32,
    /// Next phase centre Y.
    pub next_cy: i32,
    /// Next phase radius.
    pub next_r: i32,
    /// RGBA colour (guest little-endian u32).
    pub rgba: u32,
    /// [`flags`] bits.
    pub flags: u8,
    /// Previous centre X at last update (tic interpolation).
    pub prev_cx: i32,
    /// Previous centre Y.
    pub prev_cy: i32,
    /// Previous radius.
    pub prev_r: i32,
    /// [`crate::World::level`] `maptime` of last `set`.
    pub updated_tic: u32,
}

impl ModZone {
    /// Interpolate current circle for drawing (`ticfrac_milli` 0..=1000).
    #[must_use]
    pub fn interp_current(&self, maptime: u32, ticfrac_milli: u32) -> (i32, i32, i32) {
        let t = if maptime == self.updated_tic {
            i64::from(ticfrac_milli.min(1000))
        } else {
            1000
        };
        let lerp = |a: i32, b: i32| -> i32 { a + ((i64::from(b - a) * t / 1000) as i32) };
        (
            lerp(self.prev_cx, self.cx),
            lerp(self.prev_cy, self.cy),
            lerp(self.prev_r, self.r).max(0),
        )
    }
}

/// Ordered zone store on [`crate::World`].
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ModZones {
    zones: BTreeMap<u8, ModZone>,
}

impl ModZones {
    /// Empty store.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Number of active zones.
    #[must_use]
    pub fn len(&self) -> usize {
        self.zones.len()
    }

    /// True when empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.zones.is_empty()
    }

    /// Iterate in id order.
    pub fn iter(&self) -> impl Iterator<Item = &ModZone> {
        self.zones.values()
    }

    /// Lookup by id.
    #[must_use]
    pub fn get(&self, id: u8) -> Option<&ModZone> {
        self.zones.get(&id)
    }

    /// Create or update a zone. Returns `0` or [`err`].
    #[allow(clippy::too_many_arguments)]
    pub fn set(
        &mut self,
        owner_module: u16,
        id: u8,
        cx: i32,
        cy: i32,
        r: i32,
        next_cx: i32,
        next_cy: i32,
        next_r: i32,
        rgba: u32,
        flags: u8,
        now_tic: u32,
    ) -> i32 {
        if id as usize >= MAX_ZONES {
            return err::BAD_PARAMS;
        }
        if r < 0 || next_r < 0 {
            return err::BAD_PARAMS;
        }
        if !self.zones.contains_key(&id) && self.zones.len() >= MAX_ZONES {
            return err::CAP;
        }
        let prev = self.zones.get(&id);
        let (prev_cx, prev_cy, prev_r) = prev.map_or((cx, cy, r), |z| {
            if z.updated_tic == now_tic {
                (z.prev_cx, z.prev_cy, z.prev_r)
            } else {
                (z.cx, z.cy, z.r)
            }
        });
        self.zones.insert(
            id,
            ModZone {
                id,
                owner_module,
                cx,
                cy,
                r,
                next_cx,
                next_cy,
                next_r,
                rgba,
                flags,
                prev_cx,
                prev_cy,
                prev_r,
                updated_tic: now_tic,
            },
        );
        0
    }

    /// Remove one zone.
    pub fn clear(&mut self, id: u8) -> i32 {
        if id as usize >= MAX_ZONES {
            return err::BAD_PARAMS;
        }
        if self.zones.remove(&id).is_some() {
            0
        } else {
            err::MISSING
        }
    }

    /// Drop every zone owned by `module`.
    pub fn clear_module(&mut self, module: u16) {
        self.zones.retain(|_, z| z.owner_module != module);
    }

    /// Drop all zones (map change / hot-swap).
    pub fn clear_all(&mut self) {
        self.zones.clear();
    }

    /// Stable hash for tests (not netplay checksum).
    #[must_use]
    pub fn hash(&self) -> u64 {
        let mut bytes = Vec::new();
        for z in self.zones.values() {
            bytes.push(z.id);
            bytes.extend_from_slice(&z.owner_module.to_le_bytes());
            bytes.push(z.flags);
            for v in [
                z.cx, z.cy, z.r, z.next_cx, z.next_cy, z.next_r, z.prev_cx, z.prev_cy, z.prev_r,
            ] {
                bytes.extend_from_slice(&v.to_le_bytes());
            }
            bytes.extend_from_slice(&z.rgba.to_le_bytes());
            bytes.extend_from_slice(&z.updated_tic.to_le_bytes());
        }
        xxh3_64(&bytes)
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn set_clear_and_cap() {
        let mut z = ModZones::new();
        assert_eq!(
            z.set(0, 0, 100, 200, 5000, 110, 210, 4000, 0xff000080, 0, 1),
            0
        );
        assert_eq!(z.len(), 1);
        assert_eq!(z.get(0).unwrap().cx, 100);
        assert_eq!(z.set(0, 1, 0, 0, 100, 0, 0, 50, 0, 0, 2), 0);
        assert_eq!(z.set(0, 2, 0, 0, 100, 0, 0, 50, 0, 0, 2), 0);
        assert_eq!(z.set(0, 3, 0, 0, 100, 0, 0, 50, 0, 0, 2), 0);
        assert_eq!(z.set(0, 4, 0, 0, 100, 0, 0, 50, 0, 0, 2), err::BAD_PARAMS);
        assert_eq!(z.set(0, 0, 0, 0, 100, 0, 0, 50, 0, 0, 2), 0);
        assert_eq!(z.set(1, 0, 200, 0, 100, 0, 0, 50, 0, 0, 3), 0);
        assert_eq!(z.get(0).unwrap().cx, 200);
        assert_eq!(z.get(0).unwrap().owner_module, 1);
        assert_eq!(z.clear(1), 0);
        assert_eq!(z.clear(1), err::MISSING);
        z.clear_all();
        assert!(z.is_empty());
    }

    #[test]
    fn interp_and_checksum_policy_documented() {
        let mut z = ModZones::new();
        assert_eq!(z.set(0, 0, 0, 0, 100, 0, 0, 50, 0, 0, 10), 0);
        assert_eq!(z.set(0, 0, 0, 0, 200, 0, 0, 50, 0, 0, 11), 0);
        let zone = z.get(0).unwrap();
        let (cx, cy, r) = zone.interp_current(11, 500);
        assert_eq!((cx, cy, r), (0, 0, 150));
        let (cx, cy, r) = zone.interp_current(12, 0);
        assert_eq!((cx, cy, r), (0, 0, 200));
    }

    #[test]
    fn clear_module() {
        let mut z = ModZones::new();
        assert_eq!(z.set(1, 0, 1, 2, 3, 0, 0, 1, 0, 0, 0), 0);
        assert_eq!(z.set(2, 1, 1, 2, 3, 0, 0, 1, 0, 0, 0), 0);
        z.clear_module(1);
        assert_eq!(z.len(), 1);
        assert_eq!(z.get(1).unwrap().owner_module, 2);
    }

    #[test]
    fn abi_err_matches_script_abi() {
        use room_script_abi::zone;
        assert_eq!(err::BAD_PARAMS, zone::err::BAD_PARAMS);
        assert_eq!(flags::HIDDEN, zone::flags::HIDDEN);
    }
}
