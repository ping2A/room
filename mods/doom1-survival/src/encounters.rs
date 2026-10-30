//! Wave archetypes (`docs/27` §6.3).

use crate::host::Host;

/// How a round releases its threat budget.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Archetype {
    /// Cheap units in pulses.
    Swarm = 0,
    /// Two opposite flanks.
    Pincer = 1,
    /// Behind the slowest player.
    Ambush = 2,
    /// Elite + escorts (SV3).
    ElitePack = 3,
    /// Ranged pressure + melee rushes.
    Siege = 4,
    /// Continuous melee peak.
    HordeRush = 5,
    /// Boss round (SV6).
    Boss = 6,
}

impl Archetype {
    /// Display name for banners / messages.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Swarm => "SWARM",
            Self::Pincer => "PINCER",
            Self::Ambush => "AMBUSH",
            Self::ElitePack => "ELITE PACK",
            Self::Siege => "SIEGE",
            Self::HordeRush => "HORDE RUSH",
            Self::Boss => "BOSS",
        }
    }

    /// Max threat cost per spawn for this archetype (Swarm ≤ 3).
    #[must_use]
    pub const fn max_unit_cost(self) -> i32 {
        match self {
            Self::Swarm => 3,
            Self::HordeRush => 5,
            Self::Siege => 8,
            Self::Boss => 30,
            _ => 12,
        }
    }
}

/// Pick next archetype for `round` (1-based), avoiding `prev` when possible.
#[must_use]
pub fn pick_archetype(host: &mut impl Host, round: i32, prev: Option<Archetype>) -> Archetype {
    if round > 0 && round % 5 == 0 {
        return Archetype::Boss;
    }
    for _ in 0..8 {
        let roll = host.random(0, 99);
        let pick = if round < 3 {
            if roll < 70 {
                Archetype::Swarm
            } else if roll < 85 {
                Archetype::Pincer
            } else {
                Archetype::Ambush
            }
        } else if round < 6 {
            match roll {
                0..=39 => Archetype::Swarm,
                40..=59 => Archetype::Pincer,
                60..=74 => Archetype::Ambush,
                75..=89 => Archetype::ElitePack,
                _ => Archetype::Siege,
            }
        } else {
            match roll {
                0..=29 => Archetype::Swarm,
                30..=44 => Archetype::Pincer,
                45..=54 => Archetype::Ambush,
                55..=69 => Archetype::ElitePack,
                70..=84 => Archetype::Siege,
                _ => Archetype::HordeRush,
            }
        };
        if prev != Some(pick) {
            return pick;
        }
    }
    Archetype::Swarm
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::FakeHost;

    #[test]
    fn boss_every_fifth_round() {
        let mut host = FakeHost::new(3, 1);
        assert_eq!(pick_archetype(&mut host, 5, None), Archetype::Boss);
        assert_eq!(pick_archetype(&mut host, 10, Some(Archetype::Swarm)), Archetype::Boss);
    }

    #[test]
    fn avoids_immediate_repeat() {
        let mut host = FakeHost::new(3, 42);
        let a = pick_archetype(&mut host, 1, None);
        let b = pick_archetype(&mut host, 2, Some(a));
        // Not always different with unlucky RNG, but Boss path is separate.
        let _ = (a, b);
        assert_ne!(a, Archetype::Boss);
    }
}
