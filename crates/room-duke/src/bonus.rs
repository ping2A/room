//! Chocolate `dobonus` (single-player): the end-of-level BONUSSCREEN with its
//! timed count-up, Duke animation frames, counter sounds and key skipping.
//!
//! The clock is Chocolate's `totalclock` (120 Hz): call [`BonusScreen::tick`]
//! with 4 per 30 Hz game tic. Sounds come out as CON label names for the host
//! to resolve and play as global sounds.

use crate::campaign::DukeLevelBonusStats;
use crate::ui_page::UiPage;

const BONUSSCREEN: i16 = 3240;
/// Chocolate skips the count-up to here on the first key.
const SKIP_TO: i64 = 60 * 13;
/// Second key: enter the 320-clock "speech" outro.
const OUTRO: i64 = 1_000_000_000;

/// Single-player end-of-level intermission state.
#[derive(Clone, Debug)]
pub struct BonusScreen {
    pub stats: DukeLevelBonusStats,
    /// `level_names[]` entry (CON `definelevelname`, uppercase).
    pub level_name: String,
    /// Zero-based episode; volume 1 uses the alternate art (`gfx_offset` 5).
    pub volume: u8,
    /// `ud.player_skill`; above 3 shows "N/A" enemies left.
    pub skill: u8,
    totalclock: i64,
    bonuscnt: u8,
    done: bool,
    rng: u32,
    sounds: Vec<&'static str>,
}

impl BonusScreen {
    /// Start the screen; queues `BONUSMUSIC` like Chocolate.
    #[must_use]
    pub fn new(stats: DukeLevelBonusStats, level_name: String, volume: u8, skill: u8) -> Self {
        Self {
            stats,
            level_name,
            volume,
            skill,
            totalclock: 0,
            bonuscnt: 0,
            done: false,
            rng: 0x2545_f491 ^ stats.elapsed_tics,
            sounds: vec!["BONUSMUSIC"],
        }
    }

    /// The intermission has ended (outro finished).
    #[must_use]
    pub fn done(&self) -> bool {
        self.done
    }

    /// Current `totalclock`.
    #[must_use]
    pub fn clock(&self) -> i64 {
        self.totalclock
    }

    /// Drain queued sound label names (`PIPEBOMB_EXPLODE`, `BONUS_SPEECH1`…).
    pub fn take_sounds(&mut self) -> Vec<&'static str> {
        std::mem::take(&mut self.sounds)
    }

    /// Any key: first skips the count-up to its end, second starts the outro.
    pub fn key(&mut self) {
        if self.totalclock <= 60 * 2 || self.done {
            return;
        }
        if self.totalclock < SKIP_TO {
            self.totalclock = SKIP_TO;
        } else if self.totalclock < OUTRO {
            self.totalclock = OUTRO;
        }
        self.stage_sounds();
    }

    /// Advance the 120 Hz clock and fire the stage sounds Chocolate's loop
    /// emits once per frame.
    pub fn tick(&mut self, clocks: i64) {
        if self.done {
            return;
        }
        self.totalclock += clocks;
        self.stage_sounds();
        if self.totalclock > OUTRO + 320 || (self.totalclock < OUTRO && self.totalclock > 10240 + 120) {
            self.done = true;
        }
        // Chocolate parks the idle screen by wrapping its clock.
        if self.totalclock > 10240 && self.totalclock < 10240 + 10240 {
            self.totalclock = 1024;
        }
    }

    fn stage_sounds(&mut self) {
        let tc = self.totalclock;
        if tc > OUTRO && tc < OUTRO + 320 {
            if (tc >> 4) % 15 == 0 && self.bonuscnt == 6 {
                self.bonuscnt += 1;
                self.sounds.push("SHOTGUN_COCK");
                self.rng = self.rng.wrapping_mul(1_103_515_245).wrapping_add(12345);
                self.sounds.push(match (self.rng >> 16) & 3 {
                    0 => "BONUS_SPEECH1",
                    1 => "BONUS_SPEECH2",
                    2 => "BONUS_SPEECH3",
                    _ => "BONUS_SPEECH4",
                });
            }
            return;
        }
        if tc > 60 * 3 {
            if self.bonuscnt == 0 {
                self.bonuscnt = 1;
            }
            if tc > 60 * 4 && self.bonuscnt == 1 {
                self.bonuscnt = 2;
                self.sounds.push("PIPEBOMB_EXPLODE");
            }
        }
        if tc > 60 * 6 {
            if self.bonuscnt == 2 {
                self.bonuscnt = 3;
                self.sounds.push("FLY_BY");
            }
            if tc > 60 * 7 && self.bonuscnt == 3 {
                self.bonuscnt = 4;
                self.sounds.push("PIPEBOMB_EXPLODE");
            }
        }
        if tc > 60 * 9 {
            if self.bonuscnt == 4 {
                self.bonuscnt = 5;
            }
            if tc > 60 * 10 && self.bonuscnt == 5 {
                self.bonuscnt = 6;
                self.sounds.push("PIPEBOMB_EXPLODE");
            }
        }
    }

    /// Draw the current frame of the intermission.
    pub fn draw(&self, page: &mut UiPage<'_>) {
        let gfx = if self.volume == 1 { 5 } else { 0 };
        let tc = self.totalclock;
        page.clear(0);
        page.sprite(0, 0, BONUSSCREEN + gfx, 0, 0, true, true);
        if tc > OUTRO && tc < OUTRO + 320 {
            match (tc >> 4) % 15 {
                0 | 1 | 4 | 5 => page.sprite(199, 31, BONUSSCREEN + 3 + gfx, 0, 0, true, true),
                2 | 3 => page.sprite(199, 31, BONUSSCREEN + 4 + gfx, 0, 0, true, true),
                _ => {}
            }
        } else {
            match (tc >> 5) & 3 {
                1 | 3 => page.sprite(199, 31, BONUSSCREEN + 1 + gfx, 0, 0, true, true),
                2 => page.sprite(199, 31, BONUSSCREEN + 2 + gfx, 0, 0, true, true),
                _ => {}
            }
        }
        page.menutext(160, 20 - 6, 0, 0, &self.level_name);
        page.menutext(160, 36 - 6, 0, 0, "COMPLETED");
        page.gametext(160, 192, "PRESS ANY KEY TO CONTINUE", 16);
        let mmss = |t: u32| format!("{:02}:{:02}", (t / (26 * 60)) % 60, (t / 26) % 60);
        let value_x = (320 >> 2) + 71;
        if tc > 60 * 3 {
            page.gametext(10, 59 + 9, "Your Time:", 0);
            page.gametext(10, 69 + 9, "Par time:", 0);
            page.gametext(10, 78 + 9, "3D Realms' Time:", 0);
            if tc > 60 * 4 {
                page.gametext(value_x, 60 + 9, &mmss(self.stats.elapsed_tics), 0);
                page.gametext(value_x, 69 + 9, &mmss(self.stats.par_tics), 0);
                page.gametext(value_x, 78 + 9, &mmss(self.stats.designer_tics), 0);
            }
        }
        if tc > 60 * 6 {
            page.gametext(10, 94 + 9, "Enemies Killed:", 0);
            page.gametext(10, 99 + 4 + 9, "Enemies Left:", 0);
            if tc > 60 * 7 {
                page.gametext(value_x - 1, 93 + 9, &format!("{:<3}", self.stats.kills), 0);
                let left = if self.skill > 3 {
                    "N/A".to_string()
                } else {
                    format!("{:<3}", self.stats.total_kills.saturating_sub(self.stats.kills))
                };
                page.gametext(value_x - 1, 99 + 4 + 9, &left, 0);
            }
        }
        if tc > 60 * 9 {
            page.gametext(10, 120 + 9, "Secrets Found:", 0);
            page.gametext(10, 130 + 9, "Secrets Missed:", 0);
            if tc > 60 * 10 {
                page.gametext(value_x - 1, 120 + 9, &format!("{:<3}", self.stats.secrets_found), 0);
                let missed = self.stats.total_secrets.saturating_sub(self.stats.secrets_found);
                page.gametext(value_x - 1, 130 + 9, &format!("{missed:<3}"), 0);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn screen() -> BonusScreen {
        BonusScreen::new(DukeLevelBonusStats { kills: 3, total_kills: 10, ..Default::default() }, "HOLLYWOOD HOLOCAUST".into(), 0, 2)
    }

    #[test]
    fn countup_sounds_fire_in_chocolate_order() {
        let mut b = screen();
        let mut all = b.take_sounds();
        for _ in 0..(60 * 11 / 4) {
            b.tick(4);
            all.extend(b.take_sounds());
        }
        assert_eq!(all, ["BONUSMUSIC", "PIPEBOMB_EXPLODE", "FLY_BY", "PIPEBOMB_EXPLODE", "PIPEBOMB_EXPLODE"]);
        assert!(!b.done());
    }

    #[test]
    fn two_keys_skip_then_finish_with_speech() {
        let mut b = screen();
        for _ in 0..40 {
            b.tick(4);
        }
        b.key();
        assert_eq!(b.clock(), SKIP_TO);
        b.take_sounds();
        b.key();
        let mut all = Vec::new();
        for _ in 0..90 {
            b.tick(4);
            all.extend(b.take_sounds());
        }
        assert!(b.done());
        assert_eq!(all[0], "SHOTGUN_COCK");
        assert!(all[1].starts_with("BONUS_SPEECH"));
    }

    #[test]
    fn idle_screen_never_ends_without_a_key() {
        let mut b = screen();
        for _ in 0..20_000 {
            b.tick(4);
        }
        assert!(!b.done());
    }
}
