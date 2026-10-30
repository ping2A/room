//! Draft menu Mod HUD (`docs/27` §6.5 / §6.8).

use crate::announcer::{Announcer, Priority};
use crate::host::Host;
use crate::perks::{PerkId, PerkState};
use crate::tables::{INTERMISSION_TICS, TICRATE};
use crate::team::MAX_TEAM;
use crate::text::Msg;
use core::fmt::Write;
use room_script_sdk::game;
use room_script_sdk::game::rule;
use room_script_sdk::hud::{Hud, anchor, font, layer};

/// Widget ids (per seat).
pub mod id {
    /// Draft title.
    pub const TITLE: u16 = 100;
    /// Card panels 0..2.
    pub const CARD0: u16 = 101;
    /// Card labels 0..2.
    pub const LABEL0: u16 = 104;
    /// Footer hints.
    pub const FOOTER: u16 = 107;
    /// Draft subtitle under the title.
    pub const SUBTITLE: u16 = 108;
    /// Persistent Marks counter.
    pub const MARKS: u16 = 10;
}

/// Mirror of `room_script_abi::capture` (guest crate has no abi dep).
mod capture {
    pub const WEAPON_SELECT: u32 = 1 << 0;
    pub const WEAPON_CYCLE: u32 = 1 << 1;
    pub const ATTACK: u32 = 1 << 2;
    pub const USE: u32 = 1 << 3;
}

const CAPTURE: u32 =
    capture::WEAPON_SELECT | capture::WEAPON_CYCLE | capture::USE | capture::ATTACK;

struct CmdSnap {
    weapon_select: u8,
    pressed: u32,
}

/// Playsim button bits (must match `room_sim::Buttons`).
mod bt {
    pub const ATTACK: u32 = 1 << 0;
    pub const USE: u32 = 1 << 1;
    pub const WEAPONNEXT: u32 = 1 << 5;
    pub const WEAPONPREV: u32 = 1 << 6;
}

/// One seat's draft offer.
#[derive(Clone, Copy, Debug)]
pub struct DraftState {
    pub active: bool,
    pub offer: [PerkId; 3],
    pub highlight: u8,
    pub opened_at: i32,
    /// Tics the menu has been open (advances while playsim is paused).
    pub wait_tics: i32,
    pub wave: i32,
}

impl Default for DraftState {
    fn default() -> Self {
        Self {
            active: false,
            offer: [PerkId::Vitality; 3],
            highlight: 0,
            opened_at: 0,
            wait_tics: 0,
            wave: 0,
        }
    }
}

/// All seats' draft UI.
#[derive(Clone, Copy, Debug, Default)]
pub struct ModUi {
    pub drafts: [DraftState; MAX_TEAM],
}

impl ModUi {
    /// Open a 3-card draft for `seat` (0-based).
    pub fn open_draft(
        &mut self,
        host: &mut impl Host,
        seat: usize,
        perks: &PerkState,
        now: i32,
        wave: i32,
    ) {
        if seat >= MAX_TEAM {
            return;
        }
        let owned = perks.stacks[seat];
        let mut offer = [PerkId::Vitality; 3];
        for i in 0..3 {
            let mut tries = 0;
            loop {
                let p = PerkId::pick(host, &owned);
                if !offer[..i].contains(&p) || tries > 8 {
                    offer[i] = p;
                    break;
                }
                tries += 1;
            }
        }
        self.drafts[seat] = DraftState {
            active: true,
            offer,
            highlight: 0,
            opened_at: now,
            wait_tics: 0,
            wave,
        };
        game::player_capture_input(seat as i32, CAPTURE);
        // Freeze playsim so leftover rockets / map monsters cannot kill mid-pick.
        game::set_rule(rule::PAUSED, 1);
        draw_draft(seat, &self.drafts[seat]);
        game::player_sound(seat as i32, "menu/activate", 1.0);
    }

    /// Close draft UI and release capture.
    pub fn close_draft(&mut self, seat: usize) {
        if seat >= MAX_TEAM {
            return;
        }
        self.drafts[seat].active = false;
        game::player_capture_input(seat as i32, 0);
        clear_draft_widgets(seat);
        if self.all_closed() {
            game::set_rule(rule::PAUSED, 0);
        }
    }

    /// Close every seat.
    pub fn close_all(&mut self) {
        for s in 0..MAX_TEAM {
            if self.drafts[s].active {
                self.close_draft(s);
            }
        }
        game::set_rule(rule::PAUSED, 0);
    }

    /// True when no seat still has an open draft.
    #[must_use]
    pub fn all_closed(&self) -> bool {
        self.drafts.iter().all(|d| !d.active)
    }

    /// Tick open drafts; apply on confirm/timeout.
    ///
    /// Timeout uses [`DraftState::wait_tics`] (not `maptime`) because the
    /// draft pauses the playsim — `maptime` does not advance while choosing.
    pub fn tick(
        &mut self,
        host: &mut impl Host,
        perks: &mut PerkState,
        ann: &mut Announcer,
        _now: i32,
        _intermission_ends: i32,
    ) {
        for seat in 0..MAX_TEAM {
            if !self.drafts[seat].active {
                continue;
            }
            let Some(body) = host.player(seat as i32).filter(|&a| !host.actor_is_dead(a)) else {
                self.close_draft(seat);
                continue;
            };
            self.drafts[seat].wait_tics = self.drafts[seat].wait_tics.saturating_add(1);
            let mut buf = [0u8; 24];
            let _ = game::player_cmd(seat as i32, &mut buf);
            let cmd = decode_cmd(&buf);

            let mut pick: Option<u8> = None;
            if (1..=3).contains(&cmd.weapon_select) {
                pick = Some(cmd.weapon_select - 1);
            } else if cmd.pressed & bt::WEAPONNEXT != 0 {
                self.drafts[seat].highlight = (self.drafts[seat].highlight + 1) % 3;
                game::player_sound(seat as i32, "menu/cursor", 1.0);
                draw_draft(seat, &self.drafts[seat]);
            } else if cmd.pressed & bt::WEAPONPREV != 0 {
                self.drafts[seat].highlight = (self.drafts[seat].highlight + 2) % 3;
                game::player_sound(seat as i32, "menu/cursor", 1.0);
                draw_draft(seat, &self.drafts[seat]);
            } else if cmd.pressed & (bt::USE | bt::ATTACK) != 0 {
                pick = Some(self.drafts[seat].highlight);
            }
            // Auto-pick near the end of the draft window (last 2 s).
            if pick.is_none() && self.drafts[seat].wait_tics + 2 * TICRATE >= INTERMISSION_TICS {
                pick = Some(self.drafts[seat].highlight);
            }
            if let Some(i) = pick {
                let perk = self.drafts[seat].offer[usize::from(i)];
                perks.apply(host, seat, body, perk);
                game::player_sound(seat as i32, "menu/choose", 1.0);
                let mut line = Msg::new();
                let _ = write!(line, "P{} chose {}", seat + 1, perk.name());
                ann.push(host, None, Priority::Normal, line.as_str());
                self.close_draft(seat);
            }
        }
    }

    /// Update Marks counter widgets.
    pub fn patch_marks(marks: &[i32; MAX_TEAM], in_game: &[bool; MAX_TEAM]) {
        for seat in 0..MAX_TEAM {
            if !in_game[seat] {
                continue;
            }
            let mut line = Msg::new();
            let _ = write!(line, "Marks: {}", marks[seat]);
            let _ = Hud::text(id::MARKS)
                .seat(seat as i32)
                .anchor(anchor::TOP_RIGHT)
                .at(-8, 8)
                .font(font::SMALL)
                .layer(layer::OVER_HUD)
                .text(line.as_str())
                .show();
        }
    }
}

fn decode_cmd(buf: &[u8; 24]) -> CmdSnap {
    CmdSnap {
        weapon_select: buf[2],
        pressed: u32::from_le_bytes([buf[8], buf[9], buf[10], buf[11]]),
    }
}

fn draw_draft(seat: usize, d: &DraftState) {
    let seat_i = seat as i32;
    // Two short lines — BigFont STCFN×2 cannot fit "ROUND N CLEARED - PICK A PERK".
    let mut title = Msg::new();
    let mut subtitle = Msg::new();
    if d.wave > 0 {
        let _ = write!(title, "ROUND {} CLEAR", d.wave);
    } else {
        let _ = write!(title, "DRAFT");
    }
    let _ = write!(subtitle, "PICK A PERK");
    let _ = Hud::text(id::TITLE)
        .seat(seat_i)
        .anchor(anchor::TOP_CENTER)
        .at(0, 12)
        .font(font::BIG)
        .layer(layer::MODAL)
        .color("Gold")
        .text(title.as_str())
        .show();
    let _ = Hud::text(id::SUBTITLE)
        .seat(seat_i)
        .anchor(anchor::TOP_CENTER)
        .at(0, 36)
        .font(font::SMALL)
        .layer(layer::MODAL)
        .color("Gold")
        .text(subtitle.as_str())
        .show();

    for i in 0..3u16 {
        let perk = d.offer[usize::from(i)];
        // Card centres: three 92-wide panels with gaps on 320 virtual width.
        let x = -100 + (i as i16) * 100;
        let hi = d.highlight == i as u8;
        let _ = Hud::panel(id::CARD0 + i)
            .seat(seat_i)
            .anchor(anchor::CENTER)
            .at(x, -8)
            .size(92, 100)
            .layer(layer::MODAL)
            .rgba(if hi { 0x4040_80ff } else { 0x2020_40ff })
            .rgba2(if hi { 0xffff_00ff } else { 0xffff_ffff })
            .show();
        let mut label = Msg::new();
        // Highlight with a leading '*' — brackets inflate width past the card.
        let name = perk.name();
        if hi {
            let _ = write!(label, "*{name}");
        } else {
            let _ = write!(label, "{name}");
        }
        let _ = Hud::text(id::LABEL0 + i)
            .seat(seat_i)
            .anchor(anchor::CENTER)
            .at(x, -8)
            .font(font::SMALL)
            .layer(layer::MODAL)
            .text(label.as_str())
            .show();
    }
    let _ = Hud::text(id::FOOTER)
        .seat(seat_i)
        .anchor(anchor::BOTTOM_CENTER)
        .at(0, -28)
        .font(font::SMALL)
        .layer(layer::MODAL)
        .text("1 2 3 SELECT  USE CONFIRM")
        .show();
}

fn clear_draft_widgets(seat: usize) {
    let s = seat as i32;
    for id in [id::TITLE, id::SUBTITLE, id::FOOTER] {
        let _ = Hud::remove(s, id);
    }
    for i in 0..3u16 {
        let _ = Hud::remove(s, id::CARD0 + i);
        let _ = Hud::remove(s, id::LABEL0 + i);
    }
}
