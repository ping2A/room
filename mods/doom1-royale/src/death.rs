//! Elimination / spectate flow (`§6.6` / BR5).

use core::fmt::Write;

use room_script_sdk::game::{self, player_prop, Actor};
use room_script_sdk::hud::{Hud, anchor, font, layer};

use crate::host::Host;
use crate::tables::{MAX_PLAYERS, TICRATE};

/// Stats screen after a death, then spectating.
const PANEL_SECS: i32 = 5;
const PANEL_TICS: i32 = PANEL_SECS * TICRATE;
/// FIRE skips the stats screen after this long (not by the shot that was
/// being fired when the player died).
const SKIP_AFTER_TICS: i32 = TICRATE;

mod bt {
    pub const ATTACK: u32 = 1 << 0;
    pub const MOD1: u32 = 1 << 16;
}

pub mod wid {
    pub const PANEL: u16 = 60;
    pub const BODY: u16 = 61;
    pub const SPEC_NAME: u16 = 62;
    pub const SPEC_HINT: u16 = 63;
    pub const TITLE: u16 = 64;
    pub const HINT: u16 = 65;
}

/// Per-seat elimination stats screen + spectate (`BE9`).
#[derive(Clone, Copy, Debug, Default)]
pub struct DeathExperience {
    panel_until: [i32; MAX_PLAYERS],
    elim_tic: [i32; MAX_PLAYERS],
    place: [u8; MAX_PLAYERS],
    of: [u8; MAX_PLAYERS],
    killer_seat: [i8; MAX_PLAYERS],
    killer_hp: [i32; MAX_PLAYERS],
    kills: [i32; MAX_PLAYERS],
    damage: [i32; MAX_PLAYERS],
    survived: [i32; MAX_PLAYERS],
    /// Seat being watched (`-1` = none).
    follow: [i8; MAX_PLAYERS],
    spectating: [bool; MAX_PLAYERS],
    prev_cycle: [u32; MAX_PLAYERS],
}

impl DeathExperience {
    pub fn reset(&mut self) {
        *self = Self::default();
        self.follow = [-1; MAX_PLAYERS];
    }

    pub fn place(&self, seat: usize) -> u8 {
        if seat >= MAX_PLAYERS {
            0
        } else {
            self.place[seat]
        }
    }

    pub fn elim_tic(&self, seat: usize) -> i32 {
        if seat >= MAX_PLAYERS {
            0
        } else {
            self.elim_tic[seat]
        }
    }

    /// Player `seat` is watching (spectating after death).
    pub fn followed(&self, seat: usize) -> Option<usize> {
        if seat < MAX_PLAYERS && self.spectating[seat] && self.follow[seat] >= 0 {
            Some(self.follow[seat] as usize)
        } else {
            None
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn on_eliminated<H: Host>(
        &mut self,
        host: &H,
        victim: usize,
        place: usize,
        of: usize,
        killer: Option<(Actor, usize)>,
        survived_tics: i32,
        t: i32,
    ) {
        if victim >= MAX_PLAYERS {
            return;
        }
        let v = victim as i32;
        self.panel_until[victim] = t + PANEL_TICS;
        self.elim_tic[victim] = t;
        self.place[victim] = u8::try_from(place.min(255)).unwrap_or(255);
        self.of[victim] = u8::try_from(of.min(255)).unwrap_or(255);
        self.kills[victim] = host.player_stat(v, player_prop::FRAGS);
        self.damage[victim] = host.player_stat(v, player_prop::DAMAGE_DEALT);
        self.survived[victim] = survived_tics.max(0);
        self.spectating[victim] = false;
        self.follow[victim] = -1;
        // Buttons held at death must be released before they count.
        self.prev_cycle[victim] = bt::ATTACK | bt::MOD1;
        match killer {
            Some((body, k)) if k != victim => {
                self.killer_seat[victim] = i8::try_from(k).unwrap_or(-1);
                self.killer_hp[victim] = host.actor_health(body).max(0);
            }
            _ => {
                self.killer_seat[victim] = -1;
                self.killer_hp[victim] = 0;
            }
        }
    }

    pub fn tick<H: Host>(
        &mut self,
        host: &mut H,
        out: &[bool; MAX_PLAYERS],
        contender: &[bool; MAX_PLAYERS],
        over: bool,
        t: i32,
    ) {
        for seat in 0..MAX_PLAYERS {
            if !out[seat] {
                continue;
            }
            let pressed = self.pressed(seat);
            if over {
                Self::clear_widgets(seat);
                continue;
            }
            let skip = pressed && t >= self.panel_until[seat] - PANEL_TICS + SKIP_AFTER_TICS;
            if t < self.panel_until[seat] && !skip {
                self.draw_panel(seat, t);
                continue;
            }
            if !self.spectating[seat] {
                self.panel_until[seat] = t;
                for id in [wid::PANEL, wid::BODY, wid::TITLE, wid::HINT] {
                    let _ = Hud::remove(seat as i32, id);
                }
                self.spectating[seat] = true;
                host.player_capture_input(seat as i32, bt::ATTACK | bt::MOD1);
                let first = if self.killer_seat[seat] >= 0 {
                    Some(self.killer_seat[seat] as usize)
                } else {
                    None
                };
                self.watch(host, seat, first, out, contender);
            } else {
                let current = self.follow[seat];
                let lost = current < 0 || out[current as usize];
                if pressed || lost {
                    self.watch(host, seat, None, out, contender);
                }
            }
            self.draw_banner(seat, out, contender);
        }
    }

    /// Edge-detected FIRE / +mod1 for `seat`.
    fn pressed(&mut self, seat: usize) -> bool {
        let mut cmd = [0u8; 24];
        let _ = game::player_cmd(seat as i32, &mut cmd);
        let buttons = u32::from_le_bytes(cmd[4..8].try_into().unwrap_or([0; 4]));
        let held = buttons & (bt::ATTACK | bt::MOD1);
        let edge = held & !self.prev_cycle[seat];
        self.prev_cycle[seat] = held;
        edge != 0
    }

    /// Watch `want` if alive, else the next alive player after the current one.
    fn watch<H: Host>(
        &mut self,
        host: &mut H,
        seat: usize,
        want: Option<usize>,
        out: &[bool; MAX_PLAYERS],
        contender: &[bool; MAX_PLAYERS],
    ) {
        let alive = |s: usize| contender[s] && !out[s];
        let pick = want.filter(|&s| s < MAX_PLAYERS && alive(s)).or_else(|| {
            let from = if self.follow[seat] >= 0 { self.follow[seat] as usize } else { seat };
            (1..=MAX_PLAYERS)
                .map(|k| (from + k) % MAX_PLAYERS)
                .find(|&s| alive(s))
        });
        match pick {
            Some(s) => {
                self.follow[seat] = s as i8;
                if let Some(body) = host.player(s as i32) {
                    host.player_set_camera(seat as i32, Some(body));
                }
            }
            None => {
                self.follow[seat] = -1;
                host.player_set_camera(seat as i32, None);
            }
        }
    }

    fn clear_widgets(seat: usize) {
        for id in [
            wid::PANEL,
            wid::BODY,
            wid::TITLE,
            wid::HINT,
            wid::SPEC_NAME,
            wid::SPEC_HINT,
        ] {
            let _ = Hud::remove(seat as i32, id);
        }
    }

    fn draw_banner(&self, seat: usize, out: &[bool; MAX_PLAYERS], contender: &[bool; MAX_PLAYERS]) {
        let seat_i = seat as i32;
        let alive = (0..MAX_PLAYERS).filter(|&s| contender[s] && !out[s]).count();
        let mut name = crate::text::Msg::new();
        if self.follow[seat] >= 0 {
            let _ = write!(name, "SPECTATING ");
            crate::text::write_player_name(&mut name, self.follow[seat] as usize);
        } else {
            let _ = write!(name, "SPECTATING");
        }
        let _ = Hud::text(wid::SPEC_NAME)
            .seat(seat_i)
            .anchor(anchor::BOTTOM_CENTER)
            .at(0, -52)
            .font(font::SMALL)
            .layer(layer::OVER_HUD)
            .color("Gold")
            .text(name.as_str())
            .show();
        let hint = if alive > 1 { "FIRE: NEXT PLAYER" } else { "" };
        let _ = Hud::text(wid::SPEC_HINT)
            .seat(seat_i)
            .anchor(anchor::BOTTOM_CENTER)
            .at(0, -42)
            .font(font::SMALL)
            .layer(layer::OVER_HUD)
            .color("Gray")
            .text(hint)
            .show();
    }

    fn draw_panel(&self, seat: usize, t: i32) {
        let seat_i = seat as i32;
        let _ = Hud::panel(wid::PANEL)
            .seat(seat_i)
            .anchor(anchor::CENTER)
            .at(0, -6)
            .size(280, 112)
            .layer(layer::MODAL)
            .rgba(0x0b10_17ee)
            .rgba2(0x5a64_74ff)
            .show();
        let _ = Hud::text(wid::TITLE)
            .seat(seat_i)
            .anchor(anchor::CENTER)
            .at(0, -50)
            .font(font::BIG)
            .layer(layer::MODAL)
            .color("Red")
            .text("ELIMINATED")
            .show();
        let mut body = crate::text::Msg::new();
        let _ = write!(body, "#{} OF {}\n", self.place[seat], self.of[seat]);
        let ks = self.killer_seat[seat];
        if ks >= 0 {
            let _ = write!(body, "KILLED BY ");
            crate::text::write_player_name(&mut body, usize::from(ks as u8));
            let _ = write!(body, " ({} HP LEFT)\n", self.killer_hp[seat]);
        } else {
            let _ = write!(body, "THE ZONE GOT YOU\n");
        }
        let secs = self.survived[seat] / TICRATE;
        let _ = write!(
            body,
            "\nKILLS {}   DAMAGE {}   TIME {}:{:02}",
            self.kills[seat],
            self.damage[seat],
            secs / 60,
            secs % 60
        );
        let _ = Hud::text(wid::BODY)
            .seat(seat_i)
            .anchor(anchor::CENTER)
            .at(0, -18)
            .font(font::SMALL)
            .layer(layer::MODAL)
            .color("White")
            .text(body.as_str())
            .show();
        let left = ((self.panel_until[seat] - t) + TICRATE - 1) / TICRATE;
        let mut hint = crate::text::Msg::new();
        let _ = write!(hint, "SPECTATING IN {} - FIRE TO SKIP", left.max(0));
        let _ = Hud::text(wid::HINT)
            .seat(seat_i)
            .anchor(anchor::CENTER)
            .at(0, 38)
            .font(font::SMALL)
            .layer(layer::MODAL)
            .color("Gray")
            .text(hint.as_str())
            .show();
    }
}
