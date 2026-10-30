//! Marks currency and kill combos (`docs/27` §6.5).

use crate::tables::TICRATE;
use crate::team::MAX_TEAM;

/// Combo window.
const COMBO_TICS: i32 = 3 * TICRATE;

/// Per-seat Marks + streak for payouts.
#[derive(Clone, Copy, Debug)]
pub struct Economy {
    pub marks: [i32; MAX_TEAM],
    streak: [i32; MAX_TEAM],
    last_kill: [i32; MAX_TEAM],
}

impl Default for Economy {
    fn default() -> Self {
        Self {
            marks: [0; MAX_TEAM],
            streak: [0; MAX_TEAM],
            last_kill: [-COMBO_TICS; MAX_TEAM],
        }
    }
}

impl Economy {
    /// Reset for a new map.
    pub fn clear(&mut self) {
        *self = Self::default();
    }

    /// Combo multiplier ×10 (10 / 15 / 20 / 30).
    #[must_use]
    pub fn combo_mul10(streak: i32) -> i32 {
        if streak >= 8 {
            30
        } else if streak >= 5 {
            20
        } else if streak >= 3 {
            15
        } else {
            10
        }
    }

    /// Award Marks for a kill by seat `slot` (0-based). Returns amount granted.
    pub fn on_kill(&mut self, slot: i32, threat_cost: i32, now: i32, elite: bool) -> i32 {
        let Ok(i) = usize::try_from(slot) else {
            return 0;
        };
        if i >= MAX_TEAM {
            return 0;
        }
        self.streak[i] = if now - self.last_kill[i] <= COMBO_TICS {
            self.streak[i] + 1
        } else {
            1
        };
        self.last_kill[i] = now;
        let mut pay = threat_cost.max(1) * Self::combo_mul10(self.streak[i]) / 10;
        if elite {
            pay = pay + pay / 2;
        }
        self.marks[i] = self.marks[i].saturating_add(pay);
        pay
    }

    /// Split round-clear bonus among alive seats.
    pub fn round_clear_bonus(&mut self, alive_slots: &[usize], wave: i32) {
        let each = (20 + wave * 4).max(10);
        for &i in alive_slots {
            if i < MAX_TEAM {
                self.marks[i] = self.marks[i].saturating_add(each);
            }
        }
    }

    #[must_use]
    pub fn marks(&self, slot: usize) -> i32 {
        self.marks.get(slot).copied().unwrap_or(0)
    }

    pub fn spend(&mut self, slot: usize, amount: i32) -> bool {
        if slot >= MAX_TEAM || amount <= 0 || self.marks[slot] < amount {
            return false;
        }
        self.marks[slot] -= amount;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn combo_scales_and_elite_bonus() {
        let mut e = Economy::default();
        assert_eq!(e.on_kill(0, 4, 0, false), 4);
        assert_eq!(e.on_kill(0, 4, 10, false), 4);
        // 3-streak → ×1.5
        assert_eq!(e.on_kill(0, 4, 20, false), 6);
        assert_eq!(e.marks(0), 14);
        let before = e.marks(0);
        let pay = e.on_kill(0, 4, 30, true);
        assert!(pay >= 9, "elite bump");
        assert_eq!(e.marks(0), before + pay);
    }
}
