//! Drop selection phase: modal map, cursor, confirm, deploy resolve (`docs/28` §5 / §6.1).

#![allow(clippy::needless_range_loop)]

use core::fmt::Write;

use room_script_sdk::game;
use room_script_sdk::hud::{Hud, anchor, font, layer, map_layers};

use crate::host::Host;
use crate::tables::{DROP_PICK_SEP, DROP_RESOLVE_RADIUS, MAX_PLAYERS, TICRATE};
use crate::text::Msg;
use crate::ui::drop_id;
use crate::zone::{dist2, publish_render_zone, xy, ZoneGeom, ZoneRuntime};

/// Input capture during drop select (raw cmd still readable).
mod capture {
    pub const WEAPON_SELECT: u32 = 1 << 0;
    pub const WEAPON_CYCLE: u32 = 1 << 1;
    pub const ATTACK: u32 = 1 << 2;
    pub const USE: u32 = 1 << 3;
    pub const MOVE: u32 = 1 << 4;
    pub const TURN: u32 = 1 << 5;
    pub const JUMP: u32 = 1 << 6;
}

const CAPTURE: u32 = capture::WEAPON_SELECT
    | capture::WEAPON_CYCLE
    | capture::ATTACK
    | capture::USE
    | capture::MOVE
    | capture::TURN
    | capture::JUMP;

mod bt {
    pub const ATTACK: u32 = 1 << 0;
    pub const USE: u32 = 1 << 1;
}

/// Map units per tic at full stick (`forward` / `side` are ±127-ish).
const CURSOR_MOVE: f64 = 10.0;
const CURSOR_YAW: f64 = 3.0;

#[derive(Clone, Copy, Debug, Default)]
pub struct DropSelect {
    pub active: bool,
    pub cursor: [[f64; 2]; MAX_PLAYERS],
    pub pick: [[f64; 2]; MAX_PLAYERS],
    pub confirmed: [bool; MAX_PLAYERS],
    pub bounds: [f64; 4],
    prev_bt: [u32; MAX_PLAYERS],
}

impl DropSelect {
    pub fn begin<H: Host>(
        &mut self,
        host: &mut H,
        contender: &[bool; MAX_PLAYERS],
        center: [f64; 2],
        r0: f64,
        spots: &[[f64; 3]],
        nspots: usize,
    ) {
        self.active = true;
        self.bounds = spot_bounds(&spots[..nspots]);
        self.confirmed = [false; MAX_PLAYERS];
        self.prev_bt = [0; MAX_PLAYERS];
        for s in 0..MAX_PLAYERS {
            self.cursor[s] = center;
            self.pick[s] = center;
        }
        let geom = ZoneGeom {
            center,
            radius: r0,
            phase: 0,
            shrinking: false,
        };
        let preview = ZoneRuntime::new(center, r0).next_circle();
        publish_render_zone(host, &geom, preview);
        for s in 0..MAX_PLAYERS {
            if !contender[s] {
                continue;
            }
            game::player_capture_input(s as i32, CAPTURE);
            if let Some(body) = host.player(s as i32) {
                let z = host.floor_z(center[0], center[1]);
                let _ = host.actor_set_pos(body, [center[0], center[1], z]);
                host.actor_set_vel(body, [0.0; 3]);
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn tick<H: Host>(
        &mut self,
        host: &mut H,
        contender: &[bool; MAX_PLAYERS],
        center: [f64; 2],
        r0: f64,
        spots: &[[f64; 3]],
        nspots: usize,
        t: i32,
        ends: i32,
    ) -> bool {
        if !self.active {
            return false;
        }
        let mut all_done = true;
        for s in 0..MAX_PLAYERS {
            if !contender[s] {
                continue;
            }
            if self.confirmed[s] {
                continue;
            }
            all_done = false;
            let mut buf = [0u8; 24];
            let _ = game::player_cmd(s as i32, &mut buf);
            self.apply_cmd(s, &buf);
            let buttons = u32::from_le_bytes([buf[4], buf[5], buf[6], buf[7]]);
            let pressed = buttons & !self.prev_bt[s];
            self.prev_bt[s] = buttons;
            if pressed & (bt::USE | bt::ATTACK) != 0 {
                self.pick[s] = self.cursor[s];
                self.confirmed[s] = true;
                game::player_sound(s as i32, "menu/choose", 1.0);
            }
        }
        if all_done {
            return true;
        }
        if t + TICRATE >= ends {
            self.auto_pick(host, contender, center, r0, spots, nspots);
            return true;
        }
        false
    }

    fn apply_cmd(&mut self, seat: usize, buf: &[u8; 24]) {
        let forward = i16::from_le_bytes([buf[16], buf[17]]) as f64;
        let side = i16::from_le_bytes([buf[18], buf[19]]) as f64;
        let yaw = i16::from_le_bytes([buf[20], buf[21]]) as f64;
        let mut c = self.cursor[seat];
        c[0] += side / 127.0 * CURSOR_MOVE + yaw / 127.0 * CURSOR_YAW;
        c[1] += forward / 127.0 * CURSOR_MOVE;
        self.cursor[seat] = clamp_xy(c, self.bounds);
    }

    fn auto_pick<H: Host>(
        &mut self,
        host: &mut H,
        contender: &[bool; MAX_PLAYERS],
        center: [f64; 2],
        r0: f64,
        spots: &[[f64; 3]],
        nspots: usize,
    ) {
        for s in 0..MAX_PLAYERS {
            if !contender[s] || self.confirmed[s] {
                continue;
            }
            self.pick[s] = random_pick_outer_half(host, center, r0, spots, nspots);
            self.cursor[s] = self.pick[s];
            self.confirmed[s] = true;
        }
    }

    pub fn end_capture(&mut self) {
        if !self.active {
            return;
        }
        self.active = false;
        for s in 0..MAX_PLAYERS {
            game::player_capture_input(s as i32, 0);
        }
        clear_drop_widgets();
    }

    pub fn draw<H: Host>(
        &self,
        host: &H,
        contender: &[bool; MAX_PLAYERS],
        t: i32,
        ends: i32,
    ) {
        if !self.active {
            return;
        }
        let map_w = 300_i16;
        let map_h = 168_i16;
        let scale = map_scale_value(self.bounds, f64::from(map_w), f64::from(map_h));
        let layers = map_layers::GEOMETRY | map_layers::ZONES | map_layers::MARKERS;
        for s in 0..MAX_PLAYERS {
            if !contender[s] {
                continue;
            }
            let seat = s as i32;
            let left = ((ends - t) + TICRATE - 1) / TICRATE;
            let mut title = Msg::new();
            let _ = write!(title, "CHOOSE DROP  {}s", left.max(0));
            let _ = Hud::text(drop_id::TITLE)
                .seat(seat)
                .anchor(anchor::TOP_CENTER)
                .at(0, 8)
                .font(font::BIG)
                .layer(layer::MODAL)
                .color("Gold")
                .text(title.as_str())
                .show();
            let _ = Hud::map(drop_id::MAP)
                .seat(seat)
                .anchor(anchor::CENTER)
                .at(0, 8)
                .size(map_w, map_h)
                .layer(layer::MODAL)
                .value(scale)
                .max(layers)
                .rgba2(0x1010_18ff)
                .show();
            let cx = self.cursor[s][0] as i32;
            let cy = self.cursor[s][1] as i32;
            let cz = host.floor_z(self.cursor[s][0], self.cursor[s][1]) as i32;
            let _ = Hud::marker(drop_id::CURSOR)
                .seat(seat)
                .anchor(anchor::CENTER)
                .layer(layer::MODAL)
                .world_at(cx, cy, cz)
                .rgba(0xffff_40ff)
                .size(8, 8)
                .show();
            for j in 0..MAX_PLAYERS {
                if j == s || !contender[j] || !self.confirmed[j] {
                    continue;
                }
                let px = self.pick[j][0] as i32;
                let py = self.pick[j][1] as i32;
                let pz = host.floor_z(self.pick[j][0], self.pick[j][1]) as i32;
                let _ = Hud::marker(drop_id::TEAM0 + j as u16)
                    .seat(seat)
                    .anchor(anchor::CENTER)
                    .layer(layer::MODAL)
                    .world_at(px, py, pz)
                    .rgba(0x40ff_40ff)
                    .size(6, 6)
                    .show();
            }
            let _ = Hud::text(drop_id::FOOTER)
                .seat(seat)
                .anchor(anchor::BOTTOM_CENTER)
                .at(0, -12)
                .font(font::SMALL)
                .layer(layer::MODAL)
                .text("MOVE CURSOR   USE OR FIRE TO CONFIRM")
                .show();
        }
    }
}

pub fn clear_drop_widgets() {
    for s in 0..MAX_PLAYERS {
        let seat = s as i32;
        for id in [drop_id::TITLE, drop_id::MAP, drop_id::CURSOR, drop_id::FOOTER] {
            let _ = Hud::remove(seat, id);
        }
        for j in 0..MAX_PLAYERS {
            let _ = Hud::remove(seat, drop_id::TEAM0 + j as u16);
        }
    }
}

#[must_use]
pub fn spot_bounds(spots: &[[f64; 3]]) -> [f64; 4] {
    if spots.is_empty() {
        return [0.0, 0.0, 2048.0, 2048.0];
    }
    let mut min_x = spots[0][0];
    let mut min_y = spots[0][1];
    let mut max_x = min_x;
    let mut max_y = min_y;
    for s in spots.iter().skip(1) {
        min_x = min_x.min(s[0]);
        min_y = min_y.min(s[1]);
        max_x = max_x.max(s[0]);
        max_y = max_y.max(s[1]);
    }
    let pad = 128.0;
    [min_x - pad, min_y - pad, max_x + pad, max_y + pad]
}

fn clamp_xy(p: [f64; 2], bounds: [f64; 4]) -> [f64; 2] {
    [
        p[0].clamp(bounds[0], bounds[2]),
        p[1].clamp(bounds[1], bounds[3]),
    ]
}

pub fn mark_spot_taken(spots: &[[f64; 3]], nspots: usize, taken: &mut [bool], spot: [f64; 3]) {
    let p = xy(spot);
    for i in 0..nspots {
        if dist2(xy(spots[i]), p) < 1.0 {
            taken[i] = true;
        }
    }
}

fn map_scale_value(bounds: [f64; 4], widget_w: f64, widget_h: f64) -> i32 {
    let map_w = (bounds[2] - bounds[0]).max(64.0);
    let map_h = (bounds[3] - bounds[1]).max(64.0);
    let scale = (widget_w / map_w).min(widget_h / map_h) * 0.88;
    (scale.clamp(0.01, 4.0) * 10_000.0) as i32
}

/// Nearest spot within [`DROP_RESOLVE_RADIUS`] of `pick`, respecting pick separation.
#[must_use]
pub fn resolve_drop(
    pick: [f64; 2],
    spots: &[[f64; 3]],
    nspots: usize,
    other_picks: &[[f64; 2]],
    taken: &[bool],
) -> [f64; 3] {
    let r512 = DROP_RESOLVE_RADIUS * DROP_RESOLVE_RADIUS;
    let sep = DROP_PICK_SEP * DROP_PICK_SEP;
    let mut best: Option<(usize, f64)> = None;
    for i in 0..nspots {
        if taken.get(i).copied().unwrap_or(false) {
            continue;
        }
        let s = xy(spots[i]);
        let d = dist2(s, pick);
        if d > r512 {
            continue;
        }
        if other_picks.iter().any(|&p| dist2(s, p) < sep) {
            continue;
        }
        if best.is_none_or(|(_, bd)| d < bd) {
            best = Some((i, d));
        }
    }
    if let Some((i, _)) = best {
        return spots[i];
    }
    let mut fallback: Option<(usize, f64)> = None;
    for i in 0..nspots {
        if taken.get(i).copied().unwrap_or(false) {
            continue;
        }
        let s = xy(spots[i]);
        if other_picks.iter().any(|&p| dist2(s, p) < sep) {
            continue;
        }
        let d = dist2(s, pick);
        if fallback.is_none_or(|(_, bd)| d < bd) {
            fallback = Some((i, d));
        }
    }
    fallback.map(|(i, _)| spots[i]).unwrap_or_else(|| {
        let z = spots.first().map(|s| s[2]).unwrap_or(0.0);
        [pick[0], pick[1], z]
    })
}

#[must_use]
pub fn random_pick_outer_half<H: Host>(
    host: &mut H,
    center: [f64; 2],
    r0: f64,
    spots: &[[f64; 3]],
    nspots: usize,
) -> [f64; 2] {
    let inner = (r0 * 0.5).max(0.0);
    let inner2 = inner * inner;
    let outer2 = r0 * r0;
    let mut pool = [0usize; MAX_PLAYERS * 8];
    let mut n = 0usize;
    for i in 0..nspots {
        let p = xy(spots[i]);
        let d2 = dist2(p, center);
        if d2 >= inner2 && d2 <= outer2 && n < pool.len() {
            pool[n] = i;
            n += 1;
        }
    }
    if n == 0 {
        for i in 0..nspots {
            if dist2(xy(spots[i]), center) <= outer2 && n < pool.len() {
                pool[n] = i;
                n += 1;
            }
        }
    }
    if n == 0 {
        return center;
    }
    let pick = host.random(0, n as i32 - 1) as usize;
    xy(spots[pool[pick]])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::FakeHost;

    #[test]
    fn resolve_prefers_nearest_within_512() {
        let spots = [[100.0, 100.0, 0.0], [600.0, 100.0, 0.0]];
        let pick = [120.0, 100.0];
        let spot = resolve_drop(pick, &spots, 2, &[], &[false; 2]);
        assert!((spot[0] - 100.0).abs() < 0.1);
    }

    #[test]
    fn resolve_respects_pick_separation() {
        let spots = [[100.0, 100.0, 0.0], [200.0, 100.0, 0.0], [800.0, 100.0, 0.0]];
        let pick = [105.0, 100.0];
        let other = [[800.0, 100.0]];
        let spot = resolve_drop(pick, &spots, 3, &other, &[false; 3]);
        assert!((spot[0] - 100.0).abs() < 0.1);
        let pick2 = [195.0, 100.0];
        let spot2 = resolve_drop(pick2, &spots, 3, &other, &[false; 3]);
        assert!((spot2[0] - 200.0).abs() < 0.1);
    }

    #[test]
    fn outer_half_picks_in_annulus_when_possible() {
        let mut host = FakeHost::new(7);
        let center = [500.0, 500.0];
        let r0 = 400.0;
        let mut spots = [[0.0; 3]; 3];
        spots[0] = [500.0, 500.0, 0.0];
        spots[1] = [500.0, 850.0, 0.0];
        spots[2] = [500.0, 650.0, 0.0];
        let p = random_pick_outer_half(&mut host, center, r0, &spots, 3);
        let d = dist2(p, center).sqrt();
        assert!(d >= r0 * 0.5 - 1.0 && d <= r0 + 1.0, "pick {p:?} d={d}");
    }

    #[test]
    fn drop_select_duration_is_fifteen_seconds() {
        assert_eq!(crate::tables::DROP_SELECT_SECS, 15);
    }
}
