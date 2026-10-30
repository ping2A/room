//! Draft perks (`docs/27` §6.5).

use crate::host::Host;
use crate::team::MAX_TEAM;
use room_script_sdk::game::{actor_f64, Actor};
use room_script_sdk::ClassRef;

/// Perk identifier.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum PerkId {
    Vitality = 0,
    Lifesteal = 1,
    AmmoSiphon = 2,
    Plating = 3,
    Toughness = 4,
    Fleet = 5,
    SecondWind = 6,
    Berserker = 7,
    Arsenal = 8,
    BountyHunter = 9,
    Backpack = 10,
}

impl PerkId {
    const ALL: [PerkId; 11] = [
        Self::Vitality,
        Self::Lifesteal,
        Self::AmmoSiphon,
        Self::Plating,
        Self::Toughness,
        Self::Fleet,
        Self::SecondWind,
        Self::Berserker,
        Self::Arsenal,
        Self::BountyHunter,
        Self::Backpack,
    ];

    #[must_use]
    pub const fn name(self) -> &'static str {
        // STCFN (Doom HUD font) is uppercase-only glyphs.
        // Keep ≤10 chars so labels fit a 92-wide draft card at scale 1.
        match self {
            Self::Vitality => "VITALITY",
            Self::Lifesteal => "LIFESTEAL",
            Self::AmmoSiphon => "AMMO SIPHN",
            Self::Plating => "PLATING",
            Self::Toughness => "TOUGHNESS",
            Self::Fleet => "FLEET",
            Self::SecondWind => "2ND WIND",
            Self::Berserker => "BERSERKER",
            Self::Arsenal => "ARSENAL",
            Self::BountyHunter => "BOUNTY",
            Self::Backpack => "BACKPACK",
        }
    }

    #[must_use]
    pub const fn max_stacks(self) -> u8 {
        match self {
            Self::Vitality | Self::Lifesteal | Self::Toughness => 2,
            Self::SecondWind | Self::Berserker | Self::Backpack | Self::BountyHunter => 1,
            _ => 3,
        }
    }

    #[must_use]
    pub fn pick(host: &mut impl Host, owned: &[u8; 11]) -> Self {
        for _ in 0..16 {
            let p = Self::ALL[host.random(0, 10) as usize];
            if owned[p as usize] < p.max_stacks() {
                return p;
            }
        }
        Self::Vitality
    }
}

/// Per-seat perk stacks.
#[derive(Clone, Copy, Debug)]
pub struct PerkState {
    /// Stack count per [`PerkId`].
    pub stacks: [[u8; 11]; MAX_TEAM],
    /// Second Wind used this round.
    pub second_wind_used: [bool; MAX_TEAM],
    /// Next arsenal weapon index.
    pub arsenal_step: [u8; MAX_TEAM],
}

impl Default for PerkState {
    fn default() -> Self {
        Self {
            stacks: [[0; 11]; MAX_TEAM],
            second_wind_used: [false; MAX_TEAM],
            arsenal_step: [0; MAX_TEAM],
        }
    }
}

impl PerkState {
    pub fn clear_round_flags(&mut self) {
        self.second_wind_used = [false; MAX_TEAM];
    }

    #[must_use]
    pub fn stack(&self, seat: usize, id: PerkId) -> u8 {
        self.stacks.get(seat).map(|s| s[id as usize]).unwrap_or(0)
    }

    /// Apply a picked perk to `body` for `seat`.
    pub fn apply(&mut self, host: &mut impl Host, seat: usize, body: Actor, id: PerkId) {
        if seat >= MAX_TEAM {
            return;
        }
        let n = self.stacks[seat][id as usize].saturating_add(1);
        if n > id.max_stacks() {
            return;
        }
        self.stacks[seat][id as usize] = n;
        match id {
            PerkId::Vitality => {
                let bonus = 25 * i32::from(n);
                let hp = host.actor_health(body);
                host.actor_set_health(body, (hp + 25).min(100 + bonus));
            }
            PerkId::Toughness => {
                let f = if n >= 2 { 0.8 } else { 0.9 };
                host.actor_set_f64(body, actor_f64::DAMAGE_FACTOR, f);
            }
            PerkId::Fleet => {
                let sp = host.actor_get_f64(body, actor_f64::SPEED);
                let sp = if sp <= 0.0 { 1.0 } else { sp };
                host.actor_set_f64(body, actor_f64::SPEED, sp * 1.1);
            }
            PerkId::Plating => {
                let armor = host.class_find("ArmorBonus");
                if !armor.is_null() {
                    let _ = host.actor_give(body, armor, 10);
                }
            }
            PerkId::Berserker => {
                let b = host.class_find("Berserk");
                if !b.is_null() {
                    let _ = host.actor_give(body, b, 1);
                }
            }
            PerkId::Backpack => {
                let b = host.class_find("Backpack");
                if !b.is_null() {
                    let _ = host.actor_give(body, b, 1);
                }
            }
            PerkId::Arsenal => {
                const ORDER: [&str; 4] = ["Shotgun", "Chaingun", "RocketLauncher", "Chainsaw"];
                let step = self.arsenal_step[seat] as usize;
                if let Some(name) = ORDER.get(step) {
                    let c = host.class_find(name);
                    if !c.is_null() {
                        let _ = host.actor_give(body, c, 1);
                        self.arsenal_step[seat] = self.arsenal_step[seat].saturating_add(1);
                    }
                }
            }
            PerkId::Lifesteal
            | PerkId::AmmoSiphon
            | PerkId::SecondWind
            | PerkId::BountyHunter => {}
        }
    }

    /// Heal on kill for Lifesteal.
    pub fn on_kill_heal(&self, host: &mut impl Host, seat: usize, killer: Actor) {
        let n = self.stack(seat, PerkId::Lifesteal);
        if n == 0 {
            return;
        }
        let heal = match n {
            1 => 3,
            2 => 5,
            _ => 8,
        };
        let hp = host.actor_health(killer);
        let cap = 100 + 25 * i32::from(self.stack(seat, PerkId::Vitality));
        if hp > 0 && hp < cap {
            host.actor_set_health(killer, (hp + heal).min(cap));
        }
    }

    /// Ammo siphon: give a Clip.
    pub fn on_kill_ammo(&self, host: &mut impl Host, seat: usize, killer: Actor, clip: ClassRef) {
        if self.stack(seat, PerkId::AmmoSiphon) == 0 || clip.is_null() {
            return;
        }
        let _ = host.actor_give(killer, clip, 5);
    }

    /// Second Wind when health drops below 20.
    pub fn maybe_second_wind(&mut self, host: &mut impl Host, seat: usize, body: Actor, health: i32) {
        if seat >= MAX_TEAM || self.second_wind_used[seat] {
            return;
        }
        if self.stack(seat, PerkId::SecondWind) == 0 || health <= 0 || health >= 20 {
            return;
        }
        let soul = host.class_find("Soulsphere");
        if !soul.is_null() {
            let _ = host.actor_give(body, soul, 1);
            self.second_wind_used[seat] = true;
        }
    }

    #[must_use]
    pub fn bounty_mul(&self, seat: usize) -> bool {
        self.stack(seat, PerkId::BountyHunter) > 0
    }
}
