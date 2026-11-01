//! Render-only FX event stream (`docs/29` §8.K / SH10).
//!
//! Never read by the playsim, never in checksum / saves. Emitted only when
//! [`crate::World::fx_events_enabled`] is set (desktop/web shells turn it on).

use crate::actor::ActorId;
use room_core::{ClassId, DVec3};

/// RGB8 blood / effect colour.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Rgb8 {
    /// Red.
    pub r: u8,
    /// Green.
    pub g: u8,
    /// Blue.
    pub b: u8,
}

impl Rgb8 {
    /// Classic Doom blood.
    #[must_use]
    pub const fn blood_red() -> Self {
        Self {
            r: 180,
            g: 16,
            b: 16,
        }
    }
}

/// One render-side effect cue for this tic.
#[derive(Clone, Debug, PartialEq)]
pub enum FxEvent {
    /// Hitscan blood impact on an actor.
    BloodHit {
        /// Victim actor.
        victim: ActorId,
        /// World position of the hit.
        pos: DVec3,
        /// Approximate spray direction (shot → exit).
        dir: DVec3,
        /// Damage dealt this hit.
        damage: i32,
        /// Blood colour.
        color: Rgb8,
    },
    /// Player fired a weapon (console / other).
    WeaponFired {
        /// 0-based player slot.
        player: u8,
        /// Weapon class.
        weapon: ClassId,
        /// Muzzle / flash origin (approx eye).
        origin: DVec3,
        /// Aim forward.
        forward: DVec3,
    },
    /// Hitscan line finished (wall or actor).
    HitscanFired {
        /// Shooter actor.
        shooter: ActorId,
        /// Trace start.
        origin: DVec3,
        /// Trace end (impact or max range).
        end: DVec3,
        /// Actor struck, if any.
        hit_actor: Option<ActorId>,
        /// Damage (0 for puffs).
        damage: i32,
    },
    /// Explosion / radius attack.
    Explosion {
        /// Centre.
        pos: DVec3,
        /// Radius in map units.
        radius: f64,
        /// Nominal damage.
        damage: i32,
    },
    /// Actor entered a death / gib state (`docs/29` J5/J6).
    ActorDied {
        /// Dead actor.
        actor: ActorId,
        /// Death position.
        pos: DVec3,
        /// Blood colour when known.
        color: Option<Rgb8>,
        /// Approximate mass (pool size).
        mass: i32,
        /// True if gib / xdeath.
        gibbed: bool,
    },
}

impl FxEvent {
    /// Maptime / index seed for a deterministic render RNG.
    #[must_use]
    pub fn render_seed(maptime: u32, index: u32) -> u64 {
        // SplitMix64-ish mix; independent of playsim RNG.
        let mut z = u64::from(maptime)
            .wrapping_mul(0x9E37_79B9_7F4A_7C15)
            .wrapping_add(u64::from(index).wrapping_mul(0xBF58_476D_1CE4_E5B9));
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
}
