//! Phase banners, music, victory screen, rich standings (`docs/28` §6.7–6.8 / BR7).

use core::fmt::Write;

use room_script_abi::MatchStandingV1;
use room_script_sdk::game::{self, player_prop};
use room_script_sdk::hud::{Hud, anchor, font, layer};

use crate::announcer::Announcer;
use crate::combat::CombatHud;
use crate::death::DeathExperience;
use crate::host::Host;
use crate::tables::{MAX_PLAYERS, TICRATE, ZONE_ROWS};
use crate::text::Msg;

const VICTORY_SECS: i32 = 6;
const ANN_TOP5: u8 = 1;
const ANN_THREE: u8 = 2;
const ANN_FINAL: u8 = 4;
const ANN_DUEL: u8 = 8;

mod wid {
    pub const VICTORY_TITLE: u16 = 80;
    pub const VICTORY_BODY: u16 = 81;
    pub const STANDINGS: u16 = 83;
}

/// Banners, music, victory MODAL, standings build (BR7).
#[derive(Clone, Copy, Debug, Default)]
pub struct Moments {
    announcer: Announcer,
    announced: u8,
    music_band: i8,
    victory_until: i32,
    standings_sent: bool,
    /// Final standings shown on the victory screen.
    rows: [MatchStandingV1; MAX_PLAYERS],
    nrows: usize,
}

impl Moments {
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    pub fn tick_announcer<H: Host>(&mut self, host: &mut H, t: i32) {
        self.announcer.tick(host, t);
    }

    pub fn on_alive_count<H: Host>(
        &mut self,
        host: &mut H,
        alive: usize,
        alive_seats: &[usize],
        contenders: usize,
        t: i32,
    ) {
        if contenders < 2 {
            return;
        }
        if alive == 5 && self.announced & ANN_TOP5 == 0 {
            self.announced |= ANN_TOP5;
            self.announcer.team(host, "TOP 5", t);
        }
        if alive == 3 && self.announced & ANN_THREE == 0 {
            self.announced |= ANN_THREE;
            self.announcer.team(host, "3 LEFT", t);
        }
        if alive == 2
            && alive_seats.len() == 2
            && self.announced & ANN_DUEL == 0
        {
            self.announced |= ANN_DUEL;
            let mut m = Msg::new();
            let _ = write!(m, "LAST DUEL: ");
            crate::text::write_player_name(&mut m, alive_seats[0]);
            let _ = write!(m, " vs ");
            crate::text::write_player_name(&mut m, alive_seats[1]);
            self.announcer.team(host, m.as_str(), t);
        }
        self.update_music(alive);
    }

    pub fn on_final_circle<H: Host>(&mut self, host: &mut H, t: i32) {
        if self.announced & ANN_FINAL != 0 {
            return;
        }
        self.announced |= ANN_FINAL;
        self.announcer.team(host, "FINAL CIRCLE", t);
        game::set_music("D_E1M8");
        self.music_band = 2;
    }

    pub fn on_zone_phase_banner<H: Host>(&mut self, host: &mut H, phase: i32, shrinking: bool, t: i32) {
        if phase <= 0 {
            return;
        }
        let last = ZONE_ROWS.len() as i32;
        if phase >= last {
            self.on_final_circle(host, t);
            return;
        }
        let mut m = Msg::new();
        if shrinking {
            let _ = write!(m, "ZONE {phase} — CLOSING");
        } else {
            let _ = write!(m, "ZONE {phase}");
        }
        self.announcer.team(host, m.as_str(), t);
    }

    fn update_music(&mut self, alive: usize) {
        let band = if alive <= 2 {
            2
        } else if alive <= 5 {
            1
        } else {
            0
        };
        if band == self.music_band {
            return;
        }
        self.music_band = band;
        let track = match band {
            2 => "D_E1M8",
            1 => "D_E1M7",
            _ => "D_E1M1",
        };
        game::set_music(track);
    }

    pub fn begin_resolution<H: Host>(
        &mut self,
        host: &mut H,
        winner: Option<usize>,
        rows: &[MatchStandingV1],
        t: i32,
    ) {
        self.victory_until = t + VICTORY_SECS * TICRATE;
        self.standings_sent = false;
        self.nrows = rows.len().min(MAX_PLAYERS);
        self.rows[..self.nrows].copy_from_slice(&rows[..self.nrows]);
        self.rows[..self.nrows].sort_unstable_by_key(|r| (r.place, r.seat));
        self.draw_victory(winner);
        if winner.is_some() {
            self.announcer.team(host, "LAST ONE STANDING", t);
        }
    }

    pub fn tick_resolution(&mut self, winner: Option<usize>, t: i32) {
        if self.victory_until == 0 {
            return;
        }
        if t >= self.victory_until {
            // The screen stays up until the room closes.
            self.victory_until = 0;
            return;
        }
        self.draw_victory(winner);
    }

    pub fn finish_match_ex(winner: Option<usize>, rows: &[MatchStandingV1]) {
        if rows.is_empty() {
            game::end_match(winner.map(|w| w as i32));
        } else {
            game::end_match_ex(winner.map(|w| w as i32), rows);
        }
    }

    pub fn resolution_done(&self) -> bool {
        self.standings_sent && self.victory_until == 0
    }

    pub fn mark_standings_sent(&mut self) {
        self.standings_sent = true;
    }

    fn draw_victory(&self, winner: Option<usize>) {
        let mut title = Msg::new();
        match winner {
            Some(w) => {
                crate::text::write_player_name(&mut title, w);
                let _ = write!(title, " WINS");
            }
            None => {
                let _ = write!(title, "MATCH OVER");
            }
        }
        // Standings: place, name, kills, damage (best first).
        let mut table = Msg::new();
        for row in &self.rows[..self.nrows] {
            let _ = write!(table, "#{} ", row.place);
            let mut name = Msg::new();
            crate::text::write_player_name(&mut name, usize::from(row.seat));
            let short: &str = name.as_str().get(..14).unwrap_or(name.as_str());
            let _ = write!(table, "{short}  K {}  D {}\n", row.kills, row.damage);
        }
        let rows_h = (self.nrows as i16) * 10;
        let panel_h = 56 + rows_h;
        for seat in 0..MAX_PLAYERS {
            let s = seat as i32;
            let _ = Hud::panel(wid::VICTORY_TITLE)
                .seat(s)
                .anchor(anchor::CENTER)
                .at(0, 0)
                .size(280, panel_h)
                .layer(layer::MODAL)
                .rgba(0x0b10_17ee)
                .rgba2(0x5a64_74ff)
                .show();
            let top = -panel_h / 2;
            let _ = Hud::text(wid::VICTORY_BODY)
                .seat(s)
                .anchor(anchor::CENTER)
                .at(0, top + 8)
                .font(font::BIG)
                .layer(layer::MODAL)
                .color("Gold")
                .text(title.as_str())
                .show();
            let mut you = Msg::new();
            if let Some(r) = self.rows[..self.nrows].iter().find(|r| usize::from(r.seat) == seat) {
                let _ = write!(you, "YOU PLACED #{}", r.place);
            }
            let _ = Hud::text(wid::VICTORY_BODY + 1)
                .seat(s)
                .anchor(anchor::CENTER)
                .at(0, top + 30)
                .font(font::SMALL)
                .layer(layer::MODAL)
                .color(if Some(seat) == winner { "Gold" } else { "White" })
                .text(you.as_str())
                .show();
            let _ = Hud::text(wid::STANDINGS)
                .seat(s)
                .anchor(anchor::CENTER)
                .at(0, top + 46)
                .font(font::SMALL)
                .layer(layer::MODAL)
                .color("Gray")
                .text(table.as_str())
                .show();
        }
    }
}

/// Build BE11 rows for every contender.
pub fn build_standings<H: Host>(
    host: &H,
    contender: &[bool; MAX_PLAYERS],
    death: &DeathExperience,
    combat: &CombatHud,
    deploy_tic: i32,
    t: i32,
    winner: Option<usize>,
) -> ([MatchStandingV1; MAX_PLAYERS], usize) {
    let mut out = [MatchStandingV1::default(); MAX_PLAYERS];
    let mut n = 0usize;
    for s in 0..MAX_PLAYERS {
        if !contender[s] {
            continue;
        }
        let place = if Some(s) == winner {
            1u8
        } else {
            death.place(s).max(1)
        };
        let end_tic = if Some(s) == winner {
            t
        } else if death.place(s) > 0 {
            death.elim_tic(s)
        } else {
            t
        };
        let survival_tics = (end_tic - deploy_tic).max(0);
        out[n] = MatchStandingV1 {
            size: MatchStandingV1::SIZE,
            seat: s as u8,
            place,
            kills: host.player_stat(s as i32, player_prop::FRAGS),
            assists: combat.assists(s),
            damage: host.player_stat(s as i32, player_prop::DAMAGE_DEALT),
            revives: 0,
            survival_tics,
        };
        n += 1;
    }
    (out, n)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::FakeHost;

    #[test]
    fn final_circle_banner_once() {
        let mut host = FakeHost::new(1);
        let mut m = Moments::default();
        m.on_final_circle(&mut host, 100);
        m.on_final_circle(&mut host, 200);
        assert_eq!(
            host.prints.iter().filter(|(_, t)| t == "FINAL CIRCLE").count(),
            1
        );
    }
}
