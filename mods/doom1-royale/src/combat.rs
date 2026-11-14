//! Kill feed, hit markers, damage direction — milestones **BR2** / **BR7**.

use crate::host::Host;
use crate::tables::{MAX_PLAYERS, TICRATE};
use room_script_sdk::game::Actor;

const FEED_MAX: usize = 5;
const FEED_TICS: i32 = 5 * TICRATE;
const HIT_TICS: i32 = 7;
const DMG_DIR_TICS: i32 = TICRATE;
const ELIM_TICS: i32 = 2 * TICRATE;
const RECAP_WINDOW: i32 = 15 * TICRATE;
const RECAP_SLOTS: usize = 32;
const RECAP_SOURCES: usize = 8;

#[derive(Clone, Copy, Debug, Default)]
struct RecapHit {
    source_seat: u8,
    damage: i32,
    tic: i32,
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct RecapLine {
    buf: [u8; 24],
    len: u8,
    total: i32,
}

#[derive(Clone, Copy, Debug)]
struct FeedLine {
    buf: [u8; 72],
    len: u8,
    expire: i32,
}

impl Default for FeedLine {
    fn default() -> Self {
        Self {
            buf: [0; 72],
            len: 0,
            expire: 0,
        }
    }
}

impl FeedLine {
    fn set(&mut self, text: &str, expire: i32) {
        let n = text.len().min(self.buf.len());
        self.buf[..n].copy_from_slice(&text.as_bytes()[..n]);
        self.len = n as u8;
        self.expire = expire;
    }

    fn as_str(&self) -> &str {
        core::str::from_utf8(&self.buf[..usize::from(self.len)]).unwrap_or("")
    }

    fn active(&self, t: i32) -> bool {
        self.len > 0 && t < self.expire
    }
}

/// Per-seat combat feedback state (deterministic, fixed arrays).
#[derive(Clone, Copy, Debug, Default)]
pub struct CombatHud {
    feed: [FeedLine; FEED_MAX],
    hit_until: [i32; MAX_PLAYERS],
    hit_kill: [bool; MAX_PLAYERS],
    dmg_until: [i32; MAX_PLAYERS],
    dmg_actor: [u64; MAX_PLAYERS],
    elim_until: [i32; MAX_PLAYERS],
    elim_victim: [u8; MAX_PLAYERS],
    hud_cleared: bool,
    recap: [[RecapHit; RECAP_SLOTS]; MAX_PLAYERS],
    recap_i: [u8; MAX_PLAYERS],
    assists: [i32; MAX_PLAYERS],
}

impl CombatHud {
    /// Drop every widget for every seat (once).
    pub fn clear_widgets(&mut self) {
        if self.hud_cleared {
            return;
        }
        self.hud_cleared = true;
        crate::ui::clear_all();
    }

    fn push_feed_line(&mut self, line: &str, t: i32) {
        let expire = t + FEED_TICS;
        if let Some(empty) = self.feed.iter_mut().find(|f| f.len == 0) {
            empty.set(line, expire);
            return;
        }
        for i in 1..FEED_MAX {
            self.feed[i - 1] = self.feed[i];
        }
        self.feed[FEED_MAX - 1].set(line, expire);
    }

    fn format_player(buf: &mut crate::text::Msg, seat: usize) {
        crate::text::write_player_name(buf, seat);
    }

    fn format_frag_line(killer: usize, victim: usize) -> crate::text::Msg {
        use core::fmt::Write;
        let mut m = crate::text::Msg::new();
        Self::format_player(&mut m, killer);
        let _ = write!(m, " FRAGGED ");
        Self::format_player(&mut m, victim);
        m
    }

    /// Kill feed entry from [`crate::match_flow::Royale::on_player_died`].
    pub fn assists(&self, seat: usize) -> i32 {
        if seat >= MAX_PLAYERS {
            0
        } else {
            self.assists[seat]
        }
    }

    /// Credit assists when a player is eliminated (10 s damage window).
    pub fn on_victim_died(&mut self, victim: usize, killer: Option<usize>, t: i32) {
        const WINDOW: i32 = 10 * TICRATE;
        for hit in &self.recap[victim] {
            if hit.damage <= 0 || t - hit.tic > WINDOW {
                continue;
            }
            let s = usize::from(hit.source_seat);
            if s >= MAX_PLAYERS || s == victim || killer == Some(s) || hit.source_seat == 255 {
                continue;
            }
            self.assists[s] = self.assists[s].saturating_add(1);
        }
    }

    pub fn on_player_frag(&mut self, killer: usize, victim: usize, t: i32) {
        if killer >= MAX_PLAYERS || victim >= MAX_PLAYERS || killer == victim {
            return;
        }
        let line = Self::format_frag_line(killer, victim);
        self.push_feed_line(line.as_str(), t);
        self.elim_until[killer] = t + ELIM_TICS;
        self.elim_victim[killer] = victim as u8;
    }

    /// Hit marker when `source` is the attacking player seat.
    pub fn on_thing_damaged<H: Host>(
        &mut self,
        host: &H,
        thing: Actor,
        source: Option<Actor>,
        damage: i32,
        health: i32,
        t: i32,
    ) {
        if damage <= 0 {
            return;
        }
        if let Some(src) = source
            && let Some(s) = host.actor_player(src).and_then(|p| usize::try_from(p).ok())
            && s < MAX_PLAYERS
        {
            let kill = health <= 0
                && thing
                    .player()
                    .is_some_and(|v| v as usize != s);
            self.hit_until[s] = t + HIT_TICS;
            self.hit_kill[s] = kill;
        }
        if let Some(v) = thing.player().and_then(|p| usize::try_from(p).ok())
            && v < MAX_PLAYERS
            && let Some(src) = source
        {
            self.dmg_until[v] = t + DMG_DIR_TICS;
            self.dmg_actor[v] = src.0.0;
        }
        if let Some(v) = thing.player().and_then(|p| usize::try_from(p).ok())
            && v < MAX_PLAYERS
        {
            let seat = source
                .and_then(|s| host.actor_player(s))
                .and_then(|p| u8::try_from(p).ok())
                .unwrap_or(255);
            self.push_recap(v, seat, damage, t);
        }
    }

    fn push_recap(&mut self, victim: usize, source_seat: u8, damage: i32, t: i32) {
        if damage <= 0 {
            return;
        }
        let i = usize::from(self.recap_i[victim]) % RECAP_SLOTS;
        self.recap[victim][i] = RecapHit {
            source_seat,
            damage,
            tic: t,
        };
        self.recap_i[victim] = self.recap_i[victim].wrapping_add(1);
    }

    /// Top damage sources in the last 15 s for the death recap panel.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn death_recap_lines(&self, victim: usize, t: i32) -> [RecapLine; 3] {
        let mut out = [RecapLine::default(); 3];
        if victim >= MAX_PLAYERS {
            return out;
        }
        let mut totals = [(255u8, 0i32); RECAP_SOURCES];
        let mut n = 0usize;
        for hit in &self.recap[victim] {
            if hit.damage <= 0 || t - hit.tic > RECAP_WINDOW {
                continue;
            }
            if let Some(slot) = totals[..n].iter_mut().find(|(s, _)| *s == hit.source_seat) {
                slot.1 += hit.damage;
            } else if n < RECAP_SOURCES {
                totals[n] = (hit.source_seat, hit.damage);
                n += 1;
            }
        }
        for pick in 0..3.min(n) {
            let mut best = pick;
            for j in pick + 1..n {
                let (bs, bd) = totals[best];
                let (js, jd) = totals[j];
                if jd > bd || (jd == bd && js < bs) {
                    best = j;
                }
            }
            totals.swap(pick, best);
        }
        for (i, &(seat, dmg)) in totals.iter().take(3.min(n)).enumerate() {
            use core::fmt::Write;
            let mut m = crate::text::Msg::new();
            if seat == 255 {
                let _ = write!(m, "ZONE {dmg}");
            } else {
                crate::text::write_player_name(&mut m, usize::from(seat));
                let _ = write!(m, " {dmg}");
            }
            let s = m.as_str();
            let len = s.len().min(out[i].buf.len());
            out[i].buf[..len].copy_from_slice(&s.as_bytes()[..len]);
            out[i].len = len as u8;
            out[i].total = dmg;
        }
        out
    }

    pub(crate) fn feed_lines(&self, t: i32) -> [Option<&str>; FEED_MAX] {
        let mut out = [None; FEED_MAX];
        for (i, f) in self.feed.iter().enumerate() {
            if f.active(t) {
                out[i] = Some(f.as_str());
            }
        }
        out
    }

    pub(crate) fn hit_active(&self, seat: usize, t: i32) -> Option<bool> {
        if seat >= MAX_PLAYERS {
            return None;
        }
        if t < self.hit_until[seat] {
            Some(self.hit_kill[seat])
        } else {
            None
        }
    }

    pub(crate) fn dmg_dir(&self, seat: usize, t: i32) -> Option<u64> {
        if seat >= MAX_PLAYERS {
            return None;
        }
        if t < self.dmg_until[seat] && self.dmg_actor[seat] != 0 {
            Some(self.dmg_actor[seat])
        } else {
            None
        }
    }

    pub(crate) fn elim_toast(&self, seat: usize, t: i32) -> Option<usize> {
        if seat >= MAX_PLAYERS {
            return None;
        }
        if t < self.elim_until[seat] {
            Some(usize::from(self.elim_victim[seat]))
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn feed_keeps_five_lines() {
        let mut h = CombatHud::default();
        for i in 0..7 {
            h.on_player_frag(0, i % 2 + 1, (i * 10) as i32);
        }
        assert!(h.feed[0].len > 0);
        assert!(h.feed[4].len > 0);
        assert_eq!(h.feed[4].expire, 60 + FEED_TICS);
    }

    #[test]
    fn hit_marker_expires() {
        let mut hud = CombatHud::default();
        hud.hit_until[0] = 107;
        assert!(hud.hit_active(0, 100).is_some());
        assert!(hud.hit_active(0, 200).is_none());
    }

    #[test]
    fn recap_top_sources() {
        let mut hud = CombatHud::default();
        hud.push_recap(0, 0, 30, 100);
        hud.push_recap(0, 1, 20, 110);
        hud.push_recap(0, 0, 5, 120);
        let lines = hud.death_recap_lines(0, 130);
        // Names come from the engine (empty in native tests): check totals.
        assert_eq!(lines[0].total, 35, "seat 0 dealt the most");
        assert_eq!(lines[1].total, 20);
    }
}
