//! Elimination / spectate flow (`§6.6` / BR5).

use core::fmt::Write;

use room_script_sdk::game::{self, Actor};
use room_script_sdk::hud::{Hud, anchor, font, layer};

use crate::combat::CombatHud;
use crate::host::Host;
use crate::tables::{MAX_PLAYERS, TICRATE};

const PANEL_SECS: i32 = 4;
const PANEL_TICS: i32 = PANEL_SECS * TICRATE;

mod bt {
    pub const ATTACK: u32 = 1 << 0;
    pub const MOD1: u32 = 1 << 16;
}

pub mod wid {
    pub const PANEL: u16 = 60;
    pub const BODY: u16 = 61;
}

/// Per-seat elimination panel + spectate (`BE9`).
#[derive(Clone, Copy, Debug, Default)]
pub struct DeathExperience {
    panel_until: [i32; MAX_PLAYERS],
    elim_tic: [i32; MAX_PLAYERS],
    place: [u8; MAX_PLAYERS],
    killer_seat: [i8; MAX_PLAYERS],
    killer_hp: [i32; MAX_PLAYERS],
    killer_dist: [i32; MAX_PLAYERS],
    spectate_idx: [u8; MAX_PLAYERS],
    spectating: [bool; MAX_PLAYERS],
    prev_cycle: [u32; MAX_PLAYERS],
}

impl DeathExperience {
    pub fn reset(&mut self) {
        *self = Self::default();
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

    pub fn on_eliminated<H: Host>(
        &mut self,
        host: &H,
        victim: usize,
        place: usize,
        killer: Option<(Actor, usize)>,
        victim_pos: [f64; 3],
        t: i32,
    ) {
        if victim >= MAX_PLAYERS {
            return;
        }
        self.panel_until[victim] = t + PANEL_TICS;
        self.elim_tic[victim] = t;
        self.place[victim] = u8::try_from(place.min(255)).unwrap_or(255);
        self.spectate_idx[victim] = 0;
        self.spectating[victim] = false;
        self.prev_cycle[victim] = 0;
        match killer {
            Some((body, k)) if k != victim => {
                self.killer_seat[victim] = i8::try_from(k).unwrap_or(-1);
                self.killer_hp[victim] = host.actor_health(body);
                let kp = host.actor_pos(body);
                let dx = kp[0] - victim_pos[0];
                let dy = kp[1] - victim_pos[1];
                self.killer_dist[victim] = host.sqrt(dx * dx + dy * dy) as i32;
            }
            _ => {
                self.killer_seat[victim] = -1;
                self.killer_hp[victim] = 0;
                self.killer_dist[victim] = 0;
            }
        }
    }

    pub fn tick<H: Host>(
        &mut self,
        host: &mut H,
        out: &[bool; MAX_PLAYERS],
        contender: &[bool; MAX_PLAYERS],
        combat: &CombatHud,
        t: i32,
    ) {
        for seat in 0..MAX_PLAYERS {
            if !out[seat] {
                continue;
            }
            if t < self.panel_until[seat] {
                self.draw_panel(host, seat, combat, t);
                continue;
            }
            let _ = Hud::remove(seat as i32, wid::PANEL);
            let _ = Hud::remove(seat as i32, wid::BODY);
            if !self.spectating[seat] {
                self.begin_spectate(host, seat, out, contender);
            }
            self.tick_cycle(host, seat, out, contender, t);
        }
    }

    fn alive_seats(out: &[bool; MAX_PLAYERS], contender: &[bool; MAX_PLAYERS]) -> [u8; MAX_PLAYERS] {
        let mut list = [0u8; MAX_PLAYERS];
        let mut n = 0u8;
        for s in 0..MAX_PLAYERS {
            if contender[s] && !out[s] {
                list[usize::from(n)] = s as u8;
                n += 1;
            }
        }
        list
    }

    fn alive_count(out: &[bool; MAX_PLAYERS], contender: &[bool; MAX_PLAYERS]) -> u8 {
        let mut n = 0u8;
        for s in 0..MAX_PLAYERS {
            if contender[s] && !out[s] {
                n += 1;
            }
        }
        n
    }

    fn begin_spectate<H: Host>(
        &mut self,
        host: &mut H,
        seat: usize,
        out: &[bool; MAX_PLAYERS],
        contender: &[bool; MAX_PLAYERS],
    ) {
        self.spectating[seat] = true;
        let alive = Self::alive_seats(out, contender);
        let n = Self::alive_count(out, contender);
        if n == 0 {
            host.player_set_camera(seat as i32, None);
            return;
        }
        let killer = self.killer_seat[seat];
        let idx = if killer >= 0 {
            alive[..usize::from(n)]
                .iter()
                .position(|&s| s as i32 == i32::from(killer as u8))
                .unwrap_or(0)
        } else {
            0
        };
        self.spectate_idx[seat] = idx as u8;
        let follow = alive[idx as usize];
        if let Some(body) = host.player(i32::from(follow)) {
            host.player_set_camera(seat as i32, Some(body));
        }
        host.player_capture_input(seat as i32, bt::ATTACK | bt::MOD1);
    }

    fn tick_cycle<H: Host>(
        &mut self,
        host: &mut H,
        seat: usize,
        out: &[bool; MAX_PLAYERS],
        contender: &[bool; MAX_PLAYERS],
        t: i32,
    ) {
        let n = Self::alive_count(out, contender);
        if n <= 1 {
            return;
        }
        let mut cmd = [0u8; 24];
        let _ = game::player_cmd(seat as i32, &mut cmd);
        let buttons = u32::from_le_bytes(cmd[4..8].try_into().unwrap_or([0; 4]));
        let cycle = buttons & (bt::ATTACK | bt::MOD1);
        let edge = cycle & !self.prev_cycle[seat];
        self.prev_cycle[seat] = cycle;
        if edge == 0 {
            return;
        }
        let alive = Self::alive_seats(out, contender);
        let next = (usize::from(self.spectate_idx[seat]) + 1) % usize::from(n);
        self.spectate_idx[seat] = next as u8;
        let follow = alive[next];
        if let Some(body) = host.player(i32::from(follow)) {
            host.player_set_camera(seat as i32, Some(body));
        }
        let _ = t;
    }

    fn draw_panel<H: Host>(&self, _host: &H, seat: usize, combat: &CombatHud, t: i32) {
        let seat_i = seat as i32;
        let mut title = crate::text::Msg::new();
        let _ = write!(title, "YOU PLACED #{}", usize::from(self.place[seat]));
        let _ = Hud::panel(wid::PANEL)
            .seat(seat_i)
            .anchor(anchor::CENTER)
            .size(280, 120)
            .layer(layer::MODAL)
            .color("Black")
            .show();
        let mut body = crate::text::Msg::new();
        let ks = self.killer_seat[seat];
        if ks >= 0 {
            let _ = write!(body, "KILLED BY ");
            crate::text::write_player_name(&mut body, usize::from(ks as u8));
            let _ = write!(
                body,
                "\n{} units  HP {}\n",
                self.killer_dist[seat],
                self.killer_hp[seat]
            );
        } else {
            let _ = write!(body, "KILLED BY THE ZONE\n");
        }
        let _ = write!(body, "RECAP (15s):\n");
        for line in combat.death_recap_lines(seat, t) {
            if let Some(s) = line.as_str() {
                let _ = write!(body, "{s}\n");
            }
        }
        let _ = Hud::text(wid::BODY)
            .seat(seat_i)
            .anchor(anchor::CENTER)
            .at(0, -8)
            .font(font::SMALL)
            .layer(layer::MODAL)
            .text(body.as_str())
            .show();
        let _ = Hud::text(wid::PANEL)
            .seat(seat_i)
            .anchor(anchor::CENTER)
            .at(0, -52)
            .font(font::BIG)
            .layer(layer::MODAL)
            .color("Gold")
            .text(title.as_str())
            .show();
    }
}
