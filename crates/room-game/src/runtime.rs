//! Game-agnostic runtime seam (Doom today; native Duke in D5+).
//!
//! See `docs/25-duke3d-full-support.md` §4.2 and
//! `docs/decisions/0004-native-build-runtime.md`.

use crate::headless::Headless;
use room_duke::{DukeInput, DukeSession};
use room_sim::TicCmd;

/// Per-tic command for a [`GameRuntime`].
///
/// Per-tic command for a [`GameRuntime`].
#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
pub enum GameCmd {
    /// Doom / Hexen / converter-Duke path (`room-sim`).
    Doom(TicCmd),
    /// Native Duke Build runtime.
    Duke(DukeInput),
}

/// Playsim driver shared by Doom [`Headless`] and native Duke [`DukeSession`].
pub trait GameRuntime {
    /// Tics per second. Doom: 35. Native Duke: 30.
    fn tic_rate(&self) -> u32;

    /// Advance one game tic with per-seat commands.
    fn tick_runtime(&mut self, cmds: &[GameCmd]);

    /// World checksum when a level is loaded (netplay / demos / determinism).
    fn checksum(&self) -> Option<u64>;
}

impl GameRuntime for Headless {
    fn tic_rate(&self) -> u32 {
        35
    }

    fn tick_runtime(&mut self, cmds: &[GameCmd]) {
        let mut doom = Vec::with_capacity(cmds.len());
        for cmd in cmds {
            match cmd {
                GameCmd::Doom(c) => doom.push(*c),
                GameCmd::Duke(_) => {}
            }
        }
        self.tick(&doom);
    }

    fn checksum(&self) -> Option<u64> {
        Headless::checksum(self)
    }
}

impl GameRuntime for DukeSession {
    fn tic_rate(&self) -> u32 {
        DukeSession::tic_rate(self)
    }

    fn tick_runtime(&mut self, cmds: &[GameCmd]) {
        let inputs: Vec<DukeInput> = cmds
            .iter()
            .filter_map(|cmd| match cmd {
                GameCmd::Duke(input) => Some(*input),
                GameCmd::Doom(_) => None,
            })
            .collect();
        self.tick_inputs(&inputs);
    }

    fn checksum(&self) -> Option<u64> {
        DukeSession::checksum(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn doom_headless_reports_35_hz() {
        let g = Headless::new(crate::GameOptions::default());
        assert_eq!(g.tic_rate(), 35);
        assert!(g.checksum().is_none());
    }
}
