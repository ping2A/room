//! Read-only world snapshot for renderer / UI (`docs/01` §6).

use crate::World;
use crate::mod_zones::ModZones;

/// Borrowed per-frame view data (extend as shells need more fields).
#[derive(Clone, Copy, Debug)]
pub struct SimView<'w> {
    /// Retained zone rings (BE1).
    pub mod_zones: &'w ModZones,
    /// Level time in tics.
    pub maptime: u32,
}

impl<'w> SimView<'w> {
    /// Snapshot from a live world.
    #[must_use]
    pub fn from_world(world: &'w World) -> Self {
        Self {
            mod_zones: &world.mod_zones,
            maptime: world.level.maptime,
        }
    }
}
