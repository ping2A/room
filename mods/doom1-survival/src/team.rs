//! Coop team model (`docs/27` §6.9).

use crate::host::Host;
use room_script_sdk::game::Actor;

/// Max seats the director tracks (Doom coop + a few extras).
pub const MAX_TEAM: usize = 8;

/// Snapshot of in-game / alive seats for one refresh.
#[derive(Clone, Copy, Debug, Default)]
pub struct Team {
    /// Body for each seat (`None` = not in game).
    pub players: [Option<Actor>; MAX_TEAM],
    /// Alive (in game and not dead) count.
    pub alive: u8,
    /// In-game seat count (alive or dead).
    pub in_game: u8,
}

impl Team {
    /// Refresh from the host: `player(slot)` + in-game + dead checks.
    pub fn refresh(&mut self, host: &impl Host) {
        self.alive = 0;
        self.in_game = 0;
        for slot in 0..MAX_TEAM {
            if !host.player_in_game(slot as i32) {
                self.players[slot] = None;
                continue;
            }
            let body = host.player(slot as i32);
            self.players[slot] = body;
            self.in_game = self.in_game.saturating_add(1);
            if body.is_some_and(|a| !host.actor_is_dead(a)) {
                self.alive = self.alive.saturating_add(1);
            }
        }
    }

    /// Any living teammate.
    #[must_use]
    pub fn any_alive(&self, host: &impl Host) -> bool {
        self.iter_alive(host).next().is_some()
    }

    /// Iterate alive player bodies in seat order.
    pub fn iter_alive<'a, H: Host>(
        &'a self,
        host: &'a H,
    ) -> impl Iterator<Item = Actor> + 'a {
        self.players.iter().filter_map(move |p| {
            let a = (*p)?;
            (!host.actor_is_dead(a)).then_some(a)
        })
    }

    /// Nearest alive player to `pos` (xy), or `None` if nobody is alive.
    #[must_use]
    pub fn nearest_alive(&self, host: &impl Host, pos: [f64; 3]) -> Option<Actor> {
        let mut best: Option<(Actor, f64)> = None;
        for a in self.iter_alive(host) {
            let p = host.actor_pos(a);
            let (dx, dy) = (p[0] - pos[0], p[1] - pos[1]);
            let d2 = dx * dx + dy * dy;
            if best.is_none_or(|(_, bd)| d2 < bd) {
                best = Some((a, d2));
            }
        }
        best.map(|(a, _)| a)
    }

    /// First alive player (seat order) — for messages / fallback.
    #[must_use]
    pub fn first_alive(&self, host: &impl Host) -> Option<Actor> {
        self.iter_alive(host).next()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::FakeHost;

    #[test]
    fn nearest_skips_dead_and_prefers_closer() {
        let mut host = FakeHost::new(3, 1);
        let p0 = host.add_player(0, [0.0, 0.0, 0.0]);
        let _p1 = host.add_player(1, [500.0, 0.0, 0.0]);
        host.actor_set_health(p0, 0);
        let mut team = Team::default();
        team.refresh(&host);
        assert_eq!(team.alive, 1);
        assert_eq!(team.in_game, 2);
        let near = team.nearest_alive(&host, [0.0, 0.0, 0.0]).unwrap();
        assert_eq!(host.actor_pos(near)[0], 500.0);
    }

    #[test]
    fn any_alive_false_when_all_dead() {
        let mut host = FakeHost::new(3, 1);
        let p0 = host.add_player(0, [0.0, 0.0, 0.0]);
        host.actor_set_health(p0, 0);
        let mut team = Team::default();
        team.refresh(&host);
        assert!(!team.any_alive(&host));
    }
}
