//! Squads, downed/revive, pings — milestone **BR6** (`docs/28` §6.4).

use room_script_abi::capture;
use room_script_sdk::game::{self, actor_f64, class_flag, rule, Actor};
use room_script_sdk::hud::{self, anchor, font, layer};

use core::fmt::Write;

use crate::text::Msg;

use crate::host::Host;
use crate::tables::{MAX_PLAYERS, TICRATE};

const REVIVE_RANGE: f64 = 64.0;
const REVIVE_SECS: i32 = 4;
const BLEED_SECS: i32 = 30;
const PING_SECS: i32 = 8;
const MAX_PINGS: usize = 2;
const PING_RANGE: f64 = 8192.0;

const WIDGET_SQUAD0: u16 = 60;
const WIDGET_DOWNED: u16 = 61;
const WIDGET_REVIVE_BAR: u16 = 62;
const WIDGET_BLEED_BAR: u16 = 63;
const WIDGET_PING0: u16 = 70;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SquadMode {
    Solo,
    Duos,
}

#[derive(Clone, Copy, Debug, Default)]
struct Downed {
    active: bool,
    bleed_until: i32,
    downer: Option<i32>,
    base_speed: f64,
}

#[derive(Clone, Copy, Debug)]
struct Ping {
    until: i32,
    pos: [f64; 3],
    label: [u8; 24],
    len: u8,
}

impl Default for Ping {
    fn default() -> Self {
        Self {
            until: 0,
            pos: [0.0; 3],
            label: [0; 24],
            len: 0,
        }
    }
}

/// Squad state for the royale mod.
pub struct Squads {
    mode: SquadMode,
    downed: [Downed; MAX_PLAYERS],
    revive_tics: [i32; MAX_PLAYERS],
    pings: [[Ping; MAX_PINGS]; MAX_PLAYERS],
    prev_mod1: [bool; MAX_PLAYERS],
    revives: [i32; MAX_PLAYERS],
}

impl Default for Squads {
    fn default() -> Self {
        Self {
            mode: SquadMode::Solo,
            downed: [Downed {
                active: false,
                bleed_until: 0,
                downer: None,
                base_speed: 0.0,
            }; MAX_PLAYERS],
            revive_tics: [0; MAX_PLAYERS],
            pings: [[Ping::default(); MAX_PINGS]; MAX_PLAYERS],
            prev_mod1: [false; MAX_PLAYERS],
            revives: [0; MAX_PLAYERS],
        }
    }
}

impl Squads {
    pub fn revives(&self, seat: usize) -> i32 {
        if seat >= MAX_PLAYERS {
            0
        } else {
            self.revives[seat]
        }
    }

    /// Configure teams and damage rules when the map loads.
    pub fn on_loaded<H: Host>(&mut self, host: &mut H, contender: &[bool; MAX_PLAYERS]) {
        *self = Self::default();
        let n = contender.iter().filter(|&&c| c).count();
        // Duos need at least two squads; 2-player rooms stay solo (no DBNO).
        self.mode = if n >= 4 {
            SquadMode::Duos
        } else {
            SquadMode::Solo
        };
        if self.mode == SquadMode::Duos {
            host.set_rule(rule::TEAM_DAMAGE, 0);
            host.set_rule(rule::SQUAD_DBNO, 1);
            for s in 0..MAX_PLAYERS {
                if !contender[s] {
                    continue;
                }
                let team = (s / 2) as i32;
                host.player_set_team(s as i32, team);
            }
        }
    }

    pub fn is_downed(&self, seat: usize) -> bool {
        seat < MAX_PLAYERS && self.downed[seat].active
    }

    /// Per-tic squad logic (revive, bleed-out, pings, HUD).
    pub fn tick<H: Host>(
        &mut self,
        host: &mut H,
        contender: &[bool; MAX_PLAYERS],
        out: &[bool; MAX_PLAYERS],
        t: i32,
    ) {
        if self.mode != SquadMode::Duos {
            return;
        }
        self.tick_revive(host, contender, out, t);
        self.tick_bleed(host, contender, out, t);
        self.tick_pings(host, contender, t);
        if t % 5 == 0 {
            self.draw_squad_hud(host, contender, out, t);
        }
    }

    pub fn on_damaged<H: Host>(
        &mut self,
        host: &mut H,
        thing: Actor,
        source: Option<Actor>,
        health: i32,
        t: i32,
    ) {
        if self.mode != SquadMode::Duos || health != 1 {
            return;
        }
        let Some(seat) = host.actor_player(thing).map(|s| s as usize) else {
            return;
        };
        if seat >= MAX_PLAYERS || self.downed[seat].active {
            return;
        }
        let downer = source.and_then(|s| host.actor_player(s));
        self.enter_downed(host, seat, downer, t);
    }

    fn enter_downed<H: Host>(&mut self, host: &mut H, seat: usize, downer: Option<i32>, t: i32) {
        let Some(body) = host.player(seat as i32) else {
            return;
        };
        let speed = host.actor_get_f64(body, actor_f64::SPEED);
        self.downed[seat] = Downed {
            active: true,
            bleed_until: t + BLEED_SECS * TICRATE,
            downer,
            base_speed: if speed > 0.0 { speed } else { 1.0 },
        };
        host.player_capture_input(
            seat as i32,
            capture::ATTACK | capture::WEAPON_SELECT | capture::WEAPON_CYCLE,
        );
        host.actor_set_flag(body, class_flag::BUDDHA as i32, true);
        host.actor_set_f64(body, actor_f64::SPEED, self.downed[seat].base_speed * 0.3);
    }

    fn clear_downed<H: Host>(&mut self, host: &mut H, seat: usize) {
        if !self.downed[seat].active {
            return;
        }
        self.downed[seat].active = false;
        self.revive_tics[seat] = 0;
        host.player_capture_input(seat as i32, 0);
        if let Some(body) = host.player(seat as i32) {
            host.actor_set_flag(body, class_flag::BUDDHA as i32, false);
            host.actor_set_f64(body, actor_f64::SPEED, self.downed[seat].base_speed);
        }
    }

    fn tick_bleed<H: Host>(
        &mut self,
        host: &mut H,
        contender: &[bool; MAX_PLAYERS],
        out: &[bool; MAX_PLAYERS],
        t: i32,
    ) {
        for s in 0..MAX_PLAYERS {
            if !contender[s] || out[s] || !self.downed[s].active {
                continue;
            }
            if t < self.downed[s].bleed_until {
                continue;
            }
            let Some(body) = host.player(s as i32) else {
                continue;
            };
            host.actor_set_flag(body, class_flag::BUDDHA as i32, false);
            let downer = self.downed[s].downer;
            self.clear_downed(host, s);
            let src = downer.and_then(|d| host.player(d));
            let _ = host.actor_damage(body, 999, None, src);
        }
    }

    fn tick_revive<H: Host>(
        &mut self,
        host: &mut H,
        contender: &[bool; MAX_PLAYERS],
        out: &[bool; MAX_PLAYERS],
        _t: i32,
    ) {
        for s in 0..MAX_PLAYERS {
            if !contender[s] || out[s] || !self.downed[s].active {
                self.revive_tics[s] = 0;
                continue;
            }
            let victim_pos = host
                .player(s as i32)
                .map(|b| host.actor_pos(b))
                .unwrap_or([0.0; 3]);
            let mut helper: Option<usize> = None;
            for h in 0..MAX_PLAYERS {
                if h == s || !contender[h] || out[h] || self.downed[h].active {
                    continue;
                }
                if host.player_team(h as i32) != host.player_team(s as i32) {
                    continue;
                }
                let Some(body) = host.player(h as i32) else {
                    continue;
                };
                let p = host.actor_pos(body);
                let dx = p[0] - victim_pos[0];
                let dy = p[1] - victim_pos[1];
                if dx * dx + dy * dy <= REVIVE_RANGE * REVIVE_RANGE {
                    helper = Some(h);
                    break;
                }
            }
            let Some(helper_seat) = helper else {
                self.revive_tics[s] = 0;
                continue;
            };
            let mut cmd = [0u8; 24];
            let use_held = game::player_cmd(helper_seat as i32, &mut cmd) != 0
                && cmd[4] & (1 << 1) != 0;
            if use_held {
                self.revive_tics[s] = self.revive_tics[s].saturating_add(1);
            } else {
                self.revive_tics[s] = 0;
            }
            if self.revive_tics[s] >= REVIVE_SECS * TICRATE {
                if let Some(body) = host.player(s as i32) {
                    host.actor_set_health(body, 30);
                }
                self.revives[helper_seat] = self.revives[helper_seat].saturating_add(1);
                self.clear_downed(host, s);
            }
        }
    }

    fn tick_pings<H: Host>(&mut self, host: &mut H, contender: &[bool; MAX_PLAYERS], t: i32) {
        for s in 0..MAX_PLAYERS {
            if !contender[s] {
                continue;
            }
            let mut cmd = [0u8; 24];
            let _ = game::player_cmd(s as i32, &mut cmd);
            let buttons = u32::from_le_bytes(cmd[4..8].try_into().unwrap_or([0; 4]));
            let pressed = u32::from_le_bytes(cmd[8..12].try_into().unwrap_or([0; 4]));
            let mod1 = buttons & (1 << 16) != 0;
            let mod1_press = pressed & (1 << 16) != 0;
            if mod1_press && !self.prev_mod1[s] {
                self.place_ping(host, s, t);
            }
            self.prev_mod1[s] = mod1;
            for i in 0..MAX_PINGS {
                if self.pings[s][i].until > 0 && t >= self.pings[s][i].until {
                    self.pings[s][i] = Ping::default();
                }
            }
        }
        if t % 5 == 0 {
            self.draw_ping_markers(host, contender, t);
        }
    }

    fn place_ping<H: Host>(&mut self, host: &mut H, seat: usize, t: i32) {
        let Some(body) = host.player(seat as i32) else {
            return;
        };
        let mut out = [0u8; 56];
        let kind = game::player_trace(seat as i32, PING_RANGE, &mut out);
        let hx = f64::from_le_bytes(out[4..12].try_into().unwrap_or([0; 8]));
        let hy = f64::from_le_bytes(out[12..20].try_into().unwrap_or([0; 8]));
        let hz = f64::from_le_bytes(out[20..28].try_into().unwrap_or([0; 8]));
        let actor_bits = u64::from_le_bytes(out[28..36].try_into().unwrap_or([0; 8]));
        let mut label = [0u8; 24];
        label[..7].copy_from_slice(b"Go here");
        let mut len = 7u8;
        if kind == room_script_abi::trace_kind::ACTOR && actor_bits != 0 {
            if let Some(a) = Actor::from_bits(actor_bits as i64) {
                if host.actor_is_monster(a) {
                    label[..5].copy_from_slice(b"Enemy");
                    len = 5;
                }
            }
        }
        let slot = self.pings[seat]
            .iter()
            .position(|p| p.until <= t)
            .unwrap_or(0);
        self.pings[seat][slot] = Ping {
            until: t + PING_SECS * TICRATE,
            pos: [hx, hy, hz],
            label,
            len,
        };
        let _ = body;
    }

    fn draw_ping_markers<H: Host>(&self, host: &H, contender: &[bool; MAX_PLAYERS], t: i32) {
        for s in 0..MAX_PLAYERS {
            if !contender[s] {
                continue;
            }
            let team = host.player_team(s as i32);
            for p in 0..MAX_PLAYERS {
                if !contender[p] {
                    continue;
                }
                if host.player_team(p as i32) != team {
                    continue;
                }
                for i in 0..MAX_PINGS {
                    let ping = &self.pings[p][i];
                    if ping.until <= t {
                        continue;
                    }
                    let id = WIDGET_PING0 + (p * MAX_PINGS + i) as u16;
                    let text = core::str::from_utf8(&ping.label[..ping.len as usize])
                        .unwrap_or("Ping");
                    let _ = hud::Hud::marker(id)
                        .seat(s as i32)
                        .anchor(anchor::TOP_LEFT)
                        .layer(layer::OVER_HUD)
                        .text(text)
                        .world_at(
                            ping.pos[0] as i32,
                            ping.pos[1] as i32,
                            ping.pos[2] as i32,
                        )
                        .show();
                }
            }
        }
    }

    fn draw_squad_hud<H: Host>(
        &self,
        host: &H,
        contender: &[bool; MAX_PLAYERS],
        out: &[bool; MAX_PLAYERS],
        t: i32,
    ) {
        for viewer in 0..MAX_PLAYERS {
            if !contender[viewer] || out[viewer] {
                continue;
            }
            let my_team = host.player_team(viewer as i32);
            let mut panel = Msg::new();
            for s in 0..MAX_PLAYERS {
                if !contender[s] || out[s] {
                    continue;
                }
                if host.player_team(s as i32) != my_team {
                    continue;
                }
                if s == viewer {
                    continue;
                }
                let hp = host
                    .player(s as i32)
                    .map(|b| host.actor_health(b))
                    .unwrap_or(0);
                crate::text::write_player_name(&mut panel, s);
                let _ = write!(panel, " {hp}");
                if self.downed[s].active {
                    let _ = write!(panel, " DOWN");
                }
                let _ = write!(panel, "\n");
            }
            if !panel.as_str().is_empty() {
                let text = panel.as_str();
                let _ = hud::Hud::text(WIDGET_SQUAD0)
                    .seat(viewer as i32)
                    .anchor(anchor::BOTTOM_LEFT)
                    .at(4, -4)
                    .font(font::SMALL)
                    .layer(layer::OVER_HUD)
                    .text(text)
                    .show();
            } else {
                let _ = hud::Hud::remove(viewer as i32, WIDGET_SQUAD0);
            }
            if self.downed[viewer].active {
                let left = ((self.downed[viewer].bleed_until - t).max(0) as i32
                    + TICRATE
                    - 1)
                    / TICRATE;
                let _ = hud::Hud::text(WIDGET_DOWNED)
                    .seat(viewer as i32)
                    .anchor(anchor::TOP_CENTER)
                    .at(0, 40)
                    .font(font::BIG)
                    .layer(layer::OVER_HUD)
                    .color("Red")
                    .text("DOWNED")
                    .show();
                let _ = hud::Hud::bar(WIDGET_BLEED_BAR)
                    .seat(viewer as i32)
                    .anchor(anchor::TOP_CENTER)
                    .at(-30, 52)
                    .size(60, 4)
                    .layer(layer::OVER_HUD)
                    .value(left)
                    .max(BLEED_SECS)
                    .rgba(0xff40_40ff)
                    .show();
            } else {
                let _ = hud::Hud::remove(viewer as i32, WIDGET_DOWNED);
                let _ = hud::Hud::remove(viewer as i32, WIDGET_BLEED_BAR);
            }
            if self.downed[viewer].active && self.revive_tics[viewer] > 0 {
                let _ = hud::Hud::bar(WIDGET_REVIVE_BAR)
                    .seat(viewer as i32)
                    .anchor(anchor::CENTER)
                    .at(-40, 20)
                    .size(80, 6)
                    .layer(layer::OVER_HUD)
                    .value(self.revive_tics[viewer] / TICRATE)
                    .max(REVIVE_SECS)
                    .rgba(0x40ff_80ff)
                    .show();
            } else {
                let _ = hud::Hud::remove(viewer as i32, WIDGET_REVIVE_BAR);
            }
        }
    }

    /// Whole squad eliminated — mark remaining downed seats out (caller handles `out` flags).
    pub fn squad_eliminated<H: Host>(
        &self,
        host: &H,
        contender: &[bool; MAX_PLAYERS],
        team: i32,
    ) -> bool {
        for s in 0..MAX_PLAYERS {
            if !contender[s] {
                continue;
            }
            if host.player_team(s as i32) != team {
                continue;
            }
            if let Some(body) = host.player(s as i32) {
                if !host.actor_is_dead(body) {
                    return false;
                }
            }
        }
        true
    }
}
