//! Thinker stat lists (GZDoom `FThinkerCollection` / `statnum` order).

use crate::actor::ActorId;

/// Thinker list index (GZDoom `STAT_*` subset used in M5).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u16)]
pub enum StatNum {
    /// First thinking bucket (`STAT_FIRST_THINKING`).
    FirstThinking = 32,
    /// Default actor list (`STAT_DEFAULT`).
    Default = 100,
}

/// Ordered thinker lists: actors on `STAT_DEFAULT` for now.
#[derive(Clone, Debug, Default)]
pub struct ThinkerLists {
    /// Actors registered on the default list (insertion order).
    pub default: Vec<ActorId>,
}

impl ThinkerLists {
    /// Link an actor onto the default thinker list.
    pub fn link_default(&mut self, id: ActorId) {
        self.default.push(id);
    }

    /// Run all thinkers for one tic via `tick_fn`.
    pub fn run_all(&self, mut tick_fn: impl FnMut(ActorId)) {
        for &id in &self.default {
            tick_fn(id);
        }
    }
}
