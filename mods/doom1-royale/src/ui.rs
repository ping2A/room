//! Mod HUD widgets (`docs/28` §6.5 / BR2).

use core::fmt::Write;

use crate::combat::CombatHud;
use crate::host::Host;
use crate::tables::{GRACE_SECS, MAX_PLAYERS, TICRATE, ZONE_ROWS};
use crate::text::Msg;
use crate::zone::{schedule_segment, ZoneRuntime};
use room_script_sdk::game::Actor;
use room_script_sdk::hud::{Hud, anchor, font, layer, map_layers};
use room_script_sdk::ActorRef;

/// Widget ids (scoped per seat via [`HudBuilder::seat`]).
pub mod id {
    pub const ALIVE: u16 = 1;
    pub const ZONE_TEXT: u16 = 2;
    pub const ZONE_BAR: u16 = 3;
    pub const KILLS: u16 = 4;
    pub const FEED0: u16 = 10;
    pub const ELIM: u16 = 20;
    pub const HIT: u16 = 21;
    pub const DMG_DIR: u16 = 22;
    pub const INTERACT: u16 = 23;
    pub const MINIMAP: u16 = 30;
}

/// Drop-select modal widgets (`docs/28` §5 / BR3).
pub mod drop_id {
    pub const TITLE: u16 = 40;
    pub const MAP: u16 = 41;
    pub const CURSOR: u16 = 42;
    pub const FOOTER: u16 = 43;
    /// Legend entries (one per gun type).
    pub const LEGEND0: u16 = 44;
    pub const TEAM0: u16 = 50;
    /// Planned gun markers on the drop map (shared by every seat).
    pub const GUN0: u16 = 200;
}

const FEED_LINES: usize = 5;

fn format_mss(secs: i32) -> Msg {
    let mut m = Msg::new();
    let s = secs.max(0);
    let _ = write!(m, "{}:{:02}", s / 60, s % 60);
    m
}

fn zone_label(t_rel: i32) -> (Msg, Option<(i32, i32)>) {
    let grace = GRACE_SECS * TICRATE;
    if t_rel < grace {
        let left = (grace - t_rel + TICRATE - 1) / TICRATE;
        let mut line = Msg::new();
        let _ = write!(line, "ZONE IN ");
        let _ = write!(line, "{}", format_mss(left).as_str());
        return (line, Some((t_rel, grace)));
    }
    let Some((row_i, shrinking, into, _phase)) = schedule_segment(t_rel) else {
        return (Msg::new(), None);
    };
    let row = &ZONE_ROWS[row_i];
    let hold = row.hold_secs * TICRATE;
    let shrink = row.shrink_secs * TICRATE;
    let mut line = Msg::new();
    if shrinking {
        let left = ((shrink - into) + TICRATE - 1) / TICRATE;
        let _ = write!(line, "ZONE CLOSING ");
        let _ = write!(line, "{}", format_mss(left).as_str());
        (line, Some((into, shrink.max(1))))
    } else {
        let left = ((hold - into) + TICRATE - 1) / TICRATE;
        let _ = write!(line, "NEXT ZONE ");
        let _ = write!(line, "{}", format_mss(left).as_str());
        (line, Some((into, hold.max(1))))
    }
}

/// Remove all royale HUD widgets for every seat.
pub fn clear_all() {
    for s in 0..MAX_PLAYERS {
        let seat = s as i32;
        for id in [
            id::ALIVE,
            id::KILLS,
            id::ZONE_TEXT,
            id::ZONE_BAR,
            id::ELIM,
            id::HIT,
            id::DMG_DIR,
            id::INTERACT,
            id::MINIMAP,
        ] {
            let _ = Hud::remove(seat, id);
        }
        for i in 0..FEED_LINES {
            let _ = Hud::remove(seat, id::FEED0 + i as u16);
        }
    }
}

/// Inputs for one HUD patch (from match state).
pub struct HudFrame<'a> {
    pub t: i32,
    pub match_over: bool,
    pub show_zone: bool,
    pub t_rel: Option<i32>,
    pub alive_n: usize,
    pub contender: &'a [bool; MAX_PLAYERS],
    pub combat: &'a CombatHud,
    pub zone_rt: &'a mut ZoneRuntime,
    pub r0: f64,
    pub center: [f64; 2],
    pub spots: &'a [[f64; 3]],
    pub nspots: usize,
}

impl HudFrame<'_> {
    /// Draw or update widgets for every in-game contender.
    pub fn draw<H: Host>(&mut self, host: &mut H) {
        if self.match_over {
            clear_all();
            return;
        }
        if self.show_zone
            && let Some(t_rel) = self.t_rel
        {
            self.zone_rt.sync(
                host,
                self.r0,
                self.center,
                self.spots,
                self.nspots,
                t_rel,
            );
        }
        let feed = self.combat.feed_lines(self.t);
        for seat in 0..MAX_PLAYERS {
            if !self.contender[seat] {
                continue;
            }
            let seat_i = seat as i32;
            // Under the minimap: players left, then your kills.
            let stats_y: i16 = if self.show_zone { 78 } else { 8 };
            let kills = host.player_stat(seat_i, room_script_sdk::game::player_prop::FRAGS);
            for (wid, dy, label, color) in [
                (id::ALIVE, 0, self.alive_n as i32, "White"),
                (id::KILLS, 10, kills, "Gold"),
            ] {
                let mut line = Msg::new();
                let name = if wid == id::ALIVE { "ALIVE" } else { "KILLS" };
                let _ = write!(line, "{name} {label}");
                let _ = Hud::text(wid)
                    .seat(seat_i)
                    .anchor(anchor::TOP_RIGHT)
                    .at(-6, stats_y + dy)
                    .font(font::SMALL)
                    .layer(layer::OVER_HUD)
                    .align(room_script_sdk::hud::align::RIGHT)
                    .color(color)
                    .text(line.as_str())
                    .show();
            }

            if self.show_zone {
                // Square minimap under the ALIVE line: the whole level
                // around you (about 1,700 units across), zone tinted.
                let _ = Hud::map(id::MINIMAP)
                    .seat(seat_i)
                    .anchor(anchor::TOP_RIGHT)
                    .at(-74, 6)
                    .size(68, 68)
                    .layer(layer::OVER_HUD)
                    .value(400)
                    .max(
                        map_layers::GEOMETRY
                            | map_layers::ZONES
                            | map_layers::MARKERS
                            | map_layers::REVEAL,
                    )
                    .rgba2(0x0b10_17d8)
                    .show();
                if let Some(t_rel) = self.t_rel {
                    let (label, bar) = zone_label(t_rel);
                    let closing = label.as_str().starts_with("ZONE CLOSING");
                    if !label.as_str().is_empty() {
                        let _ = Hud::text(id::ZONE_TEXT)
                            .seat(seat_i)
                            .anchor(anchor::TOP_CENTER)
                            .at(0, 6)
                            .font(font::SMALL)
                            .layer(layer::OVER_HUD)
                            .color(if closing { "Red" } else { "White" })
                            .text(label.as_str())
                            .show();
                    }
                    if let Some((val, max)) = bar {
                        let _ = Hud::bar(id::ZONE_BAR)
                            .seat(seat_i)
                            .anchor(anchor::TOP_CENTER)
                            .at(0, 16)
                            .size(80, 3)
                            .layer(layer::OVER_HUD)
                            .value(val)
                            .max(max)
                            .rgba(if closing { 0xe040_40ff } else { 0xf0f0_f0ff })
                            .rgba2(0x2020_20ff)
                            .show();
                    }
                }
            } else {
                let _ = Hud::remove(seat_i, id::ZONE_TEXT);
                let _ = Hud::remove(seat_i, id::ZONE_BAR);
                let _ = Hud::remove(seat_i, id::MINIMAP);
            }

            for (i, line) in feed.iter().enumerate() {
                let wid = id::FEED0 + i as u16;
                if let Some(text) = line {
                    let _ = Hud::text(wid)
                        .seat(seat_i)
                        .anchor(anchor::MID_RIGHT)
                        .at(-8, (i as i16) * 10)
                        .font(font::SMALL)
                        .layer(layer::OVER_HUD)
                        .align(room_script_sdk::hud::align::RIGHT)
                        .color("Gray")
                        .text(text)
                        .show();
                } else {
                    let _ = Hud::remove(seat_i, wid);
                }
            }

            if let Some(victim) = self.combat.elim_toast(seat, self.t) {
                let mut toast = Msg::new();
                let _ = write!(toast, "ELIMINATED ");
                crate::text::write_player_name(&mut toast, victim);
                let _ = Hud::text(id::ELIM)
                    .seat(seat_i)
                    .anchor(anchor::BOTTOM_CENTER)
                    .at(0, -36)
                    .font(font::BIG)
                    .layer(layer::OVER_HUD)
                    .color("Gold")
                    .text(toast.as_str())
                    .show();
            } else {
                let _ = Hud::remove(seat_i, id::ELIM);
            }

            if let Some(kill) = self.combat.hit_active(seat, self.t) {
                let _ = Hud::text(id::HIT)
                    .seat(seat_i)
                    .anchor(anchor::CENTER)
                    .at(0, -4)
                    .font(font::BIG)
                    .layer(layer::OVER_HUD)
                    .color(if kill { "Red" } else { "White" })
                    .text("X")
                    .show();
            } else {
                let _ = Hud::remove(seat_i, id::HIT);
            }

            if let Some(actor_bits) = self.combat.dmg_dir(seat, self.t) {
                let actor = Actor(ActorRef(actor_bits));
                let _ = Hud::marker(id::DMG_DIR)
                    .seat(seat_i)
                    .anchor(anchor::CENTER)
                    .layer(layer::OVER_HUD)
                    .follow(actor)
                    .rgba(0xff40_40ff)
                    .size(6, 6)
                    .show();
            } else {
                let _ = Hud::remove(seat_i, id::DMG_DIR);
            }

            let _ = Hud::remove(seat_i, id::INTERACT);
        }
    }
}
