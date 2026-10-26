//! Game-agnostic runtime seam (Doom today; native Duke in D5+).
//!
//! See `docs/25-duke3d-full-support.md` §4.2 and
//! `docs/decisions/0004-native-build-runtime.md`.

use crate::headless::Headless;
use room_sim::TicCmd;

/// Per-tic command for a [`GameRuntime`].
///
/// Duke input lands in D10 (`room-proto`); until then only Doom `TicCmd` is used.
#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
pub enum GameCmd {
    /// Doom / Hexen / converter-Duke path (`room-sim`).
    Doom(TicCmd),
}

/// Playsim driver shared by Doom [`Headless`] and future `DukeRuntime`.
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
            }
        }
        self.tick(&doom);
    }

    fn checksum(&self) -> Option<u64> {
        Headless::checksum(self)
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
