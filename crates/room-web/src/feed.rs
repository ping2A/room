//! Kill feed lines and per-seat scores derived from sim [`KillEvent`]s.
//!
//! Every client (and spectator) sees the same deterministic event stream, so
//! scores agree everywhere without any extra network traffic.

use room_sim::KillEvent;
use std::collections::VecDeque;

/// Seats tracked on the scoreboard.
pub const MAX_SEATS: usize = 8;

/// How long a feed line stays on screen.
pub const FEED_LINE_MS: f64 = 6_000.0;

/// Max lines kept in the feed.
const MAX_FEED_LINES: usize = 6;

/// Per-seat tallies.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Score {
    /// Monsters killed.
    pub kills: u32,
    /// Times died.
    pub deaths: u32,
    /// Other players killed (minus suicides).
    pub frags: i32,
}

/// Scoreboard for every seat.
#[derive(Clone, Debug, Default)]
pub struct Scores {
    /// Indexed by seat.
    pub seats: [Score; MAX_SEATS],
}

impl Scores {
    /// Fold one death into the tallies.
    pub fn apply(&mut self, ev: &KillEvent) {
        let slot = |s: u8| usize::from(s).min(MAX_SEATS - 1);
        match (ev.victim_player, ev.killer_player) {
            (Some(v), Some(k)) if v == k => {
                self.seats[slot(v)].deaths += 1;
                self.seats[slot(v)].frags -= 1;
            }
            (Some(v), Some(k)) => {
                self.seats[slot(v)].deaths += 1;
                self.seats[slot(k)].frags += 1;
            }
            (Some(v), None) => self.seats[slot(v)].deaths += 1,
            (None, Some(k)) if ev.victim_countkill => self.seats[slot(k)].kills += 1,
            (None, _) => {}
        }
    }
}

/// Friendly name for a Doom / Doom II / Freedoom class.
#[must_use]
pub fn monster_name(class: &str) -> String {
    const TABLE: &[(&str, &str)] = &[
        ("zombieman", "zombieman"),
        ("shotgunguy", "shotgun guy"),
        ("chaingunguy", "heavy weapon dude"),
        ("doomimp", "imp"),
        ("demon", "demon"),
        ("spectre", "spectre"),
        ("lostsoul", "lost soul"),
        ("cacodemon", "cacodemon"),
        ("hellknight", "hell knight"),
        ("baronofhell", "baron of hell"),
        ("arachnotron", "arachnotron"),
        ("painelemental", "pain elemental"),
        ("revenant", "revenant"),
        ("fatso", "mancubus"),
        ("archvile", "arch-vile"),
        ("spidermastermind", "spider mastermind"),
        ("cyberdemon", "cyberdemon"),
        ("wolfensteinss", "ss guard"),
        ("commanderkeen", "commander keen"),
        ("bossbrain", "icon of sin"),
        ("explosivebarrel", "barrel"),
    ];
    let lower = class.to_ascii_lowercase();
    if let Some((_, n)) = TABLE.iter().find(|(c, _)| *c == lower) {
        return (*n).to_owned();
    }
    // `SomeCustomMonster` → "some custom monster".
    let mut out = String::new();
    for (i, ch) in class.chars().enumerate() {
        if ch.is_ascii_uppercase() && i > 0 {
            out.push(' ');
        }
        out.push(ch.to_ascii_lowercase());
    }
    if out.is_empty() {
        "something".into()
    } else {
        out
    }
}

fn with_article(noun: &str) -> String {
    let an = noun
        .chars()
        .next()
        .is_some_and(|c| matches!(c.to_ascii_lowercase(), 'a' | 'e' | 'i' | 'o' | 'u'));
    format!("{} {noun}", if an { "an" } else { "a" })
}

/// One feed line for `ev`, or `None` for noise (monster infighting).
#[cfg_attr(not(test), allow(dead_code))]
pub fn kill_line(ev: &KillEvent, name_of: impl Fn(u8) -> String) -> Option<String> {
    match (ev.victim_player, ev.killer_player) {
        (Some(v), Some(k)) if v == k => Some(format!("{} suicided", name_of(v))),
        (Some(v), Some(k)) => Some(format!("{} fragged {}", name_of(k), name_of(v))),
        (Some(v), None) if !ev.killer_class.is_empty() => Some(format!(
            "{} was killed by {}",
            name_of(v),
            with_article(&monster_name(&ev.killer_class))
        )),
        (Some(v), None) => Some(format!("{} died", name_of(v))),
        (None, Some(k)) => Some(format!(
            "{} killed {}",
            name_of(k),
            with_article(&monster_name(&ev.victim_class))
        )),
        (None, None) => None,
    }
}

/// Category + wording for `ev` from `local`'s point of view ("You killed an
/// imp", "Bot 2 fragged you"), or `None` for monster infighting.
/// Categories: `kill` (monsters), `frag` (player vs player, bots included),
/// `death` (a player killed by the world or a monster).
pub fn kill_message(
    ev: &KillEvent,
    local: u8,
    name_of: impl Fn(u8) -> String,
) -> Option<(&'static str, String)> {
    let who = |s: u8, cap: bool| {
        if s == local {
            if cap {
                "You".to_owned()
            } else {
                "you".to_owned()
            }
        } else {
            name_of(s)
        }
    };
    Some(match (ev.victim_player, ev.killer_player) {
        (Some(v), Some(k)) if v == k => (
            "frag",
            if v == local {
                "You killed yourself".to_owned()
            } else {
                format!("{} killed themself", name_of(v))
            },
        ),
        (Some(v), Some(k)) => (
            "frag",
            format!("{} fragged {}", who(k, true), who(v, false)),
        ),
        (Some(v), None) if !ev.killer_class.is_empty() => (
            "death",
            format!(
                "{} {} killed by {}",
                who(v, true),
                if v == local { "were" } else { "was" },
                with_article(&monster_name(&ev.killer_class))
            ),
        ),
        (Some(v), None) => ("death", format!("{} died", who(v, true))),
        (None, Some(k)) => (
            "kill",
            format!(
                "{} killed {}",
                who(k, true),
                with_article(&monster_name(&ev.victim_class))
            ),
        ),
        (None, None) => return None,
    })
}

/// Timed lines for the bottom-left feed (newest last)./// Timed lines for the bottom-left feed (newest last).
#[derive(Clone, Debug, Default)]
pub struct EventFeed {
    lines: VecDeque<(String, f64)>,
}

impl EventFeed {
    /// Add a line at wall-clock `now_ms`.
    pub fn push(&mut self, text: impl Into<String>, now_ms: f64) {
        self.lines.push_back((text.into(), now_ms + FEED_LINE_MS));
        while self.lines.len() > MAX_FEED_LINES {
            self.lines.pop_front();
        }
    }

    /// Drop expired lines.
    pub fn prune(&mut self, now_ms: f64) {
        while self
            .lines
            .front()
            .is_some_and(|(_, until)| *until <= now_ms)
        {
            self.lines.pop_front();
        }
    }

    /// Active lines, oldest first, with remaining lifetime in ms.
    pub fn iter(&self, now_ms: f64) -> impl Iterator<Item = (&str, f64)> {
        self.lines
            .iter()
            .map(move |(t, until)| (t.as_str(), until - now_ms))
    }
}

/// Doom's `STCFN` font covers `!`..`_` (letters are upper-cased when drawn).
#[must_use]
pub fn stcfn_safe(text: &str) -> String {
    text.chars()
        .map(|c| {
            let u = c.to_ascii_uppercase();
            if u == ' ' || ('!'..='_').contains(&u) {
                u
            } else {
                '?'
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(
        victim_player: Option<u8>,
        killer_player: Option<u8>,
        victim: &str,
        killer: &str,
    ) -> KillEvent {
        KillEvent {
            victim_player,
            victim_class: victim.into(),
            victim_countkill: victim_player.is_none(),
            killer_player,
            killer_class: killer.into(),
        }
    }

    fn name(s: u8) -> String {
        ["alice", "bob"][usize::from(s)].into()
    }

    #[test]
    fn lines_cover_each_case() {
        assert_eq!(
            kill_line(&ev(None, Some(0), "DoomImp", "DoomPlayer"), name).as_deref(),
            Some("alice killed an imp")
        );
        assert_eq!(
            kill_line(&ev(Some(1), None, "DoomPlayer", "Cacodemon"), name).as_deref(),
            Some("bob was killed by a cacodemon")
        );
        assert_eq!(
            kill_line(&ev(Some(1), Some(0), "DoomPlayer", "DoomPlayer"), name).as_deref(),
            Some("alice fragged bob")
        );
        assert_eq!(
            kill_line(&ev(Some(0), Some(0), "DoomPlayer", "DoomPlayer"), name).as_deref(),
            Some("alice suicided")
        );
        assert_eq!(
            kill_line(&ev(Some(0), None, "DoomPlayer", ""), name).as_deref(),
            Some("alice died")
        );
        assert_eq!(kill_line(&ev(None, None, "DoomImp", "Demon"), name), None);
        assert_eq!(monster_name("MyCoolBeast"), "my cool beast");
    }

    #[test]
    fn kill_messages_speak_to_the_player() {
        let k = |e: KillEvent| kill_message(&e, 0, name);
        assert_eq!(
            k(ev(None, Some(0), "DoomImp", "")),
            Some(("kill", "You killed an imp".into()))
        );
        assert_eq!(
            k(ev(None, Some(1), "Demon", "")),
            Some(("kill", "bob killed a demon".into()))
        );
        assert_eq!(
            k(ev(Some(0), None, "DoomPlayer", "Cacodemon")),
            Some(("death", "You were killed by a cacodemon".into()))
        );
        assert_eq!(
            k(ev(Some(1), Some(0), "DoomPlayer", "")),
            Some(("frag", "You fragged bob".into()))
        );
        assert_eq!(
            k(ev(Some(0), Some(1), "DoomPlayer", "")),
            Some(("frag", "bob fragged you".into()))
        );
        assert_eq!(k(ev(None, None, "DoomImp", "Demon")), None);
    }

    #[test]
    fn scores_tally() {
        let mut s = Scores::default();
        s.apply(&ev(None, Some(0), "DoomImp", ""));
        s.apply(&ev(None, Some(0), "Demon", ""));
        s.apply(&ev(Some(1), Some(0), "DoomPlayer", ""));
        s.apply(&ev(Some(0), Some(0), "DoomPlayer", ""));
        s.apply(&ev(Some(1), None, "DoomPlayer", "DoomImp"));
        assert_eq!(
            s.seats[0],
            Score {
                kills: 2,
                deaths: 1,
                frags: 0
            }
        );
        assert_eq!(s.seats[1].deaths, 2);
    }

    #[test]
    fn feed_expires_and_caps() {
        let mut f = EventFeed::default();
        for i in 0..10 {
            f.push(format!("l{i}"), 0.0);
        }
        assert_eq!(f.iter(0.0).count(), MAX_FEED_LINES);
        f.prune(FEED_LINE_MS + 1.0);
        assert_eq!(f.iter(0.0).count(), 0);
        assert_eq!(stcfn_safe("Zoë {x}"), "ZO? ?X?");
    }
}
