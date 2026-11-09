//! Per-zone reverb environment state (ZDoom `Level->Zones`).
//!
//! The playsim only stores environment IDs (`id1 << 8 | id2`, as REVERBS);
//! the shell maps them to reverb parameters. Render-side only: nothing here
//! feeds back into gameplay, so demos are unaffected.

use crate::World;
use crate::actor::ActorId;
use room_map::sound_zones::{SOUND_ENVIRONMENT_DOOMEDNUM, flood_sound_zones};

/// Zones and their environment IDs.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SoundEnvState {
    /// Zone number of each sector.
    pub sector_zone: Vec<u16>,
    /// Environment ID per zone (`0` = "Off").
    pub zone_env: Vec<u16>,
}

impl SoundEnvState {
    /// Environment ID of `sector`'s zone.
    #[must_use]
    pub fn sector_env(&self, sector: usize) -> u16 {
        self.sector_zone
            .get(sector)
            .and_then(|&z| self.zone_env.get(usize::from(z)))
            .copied()
            .unwrap_or(0)
    }
}

impl World {
    /// Flood the sound zones and apply every non-dormant `SoundEnvironment`
    /// thing (`args[0] << 8 | args[1]`). `default_env` is MAPINFO
    /// `DefaultEnvironment`.
    pub fn init_sound_zones(&mut self, default_env: u16) {
        let (sector_zone, count) = flood_sound_zones(&self.map);
        self.sound_env = SoundEnvState {
            sector_zone,
            zone_env: vec![default_env; count],
        };
        let things: Vec<_> = self
            .map
            .things
            .iter()
            .filter(|t| t.type_id == SOUND_ENVIRONMENT_DOOMEDNUM)
            // Hexen / UDMF `dormant`: the thing waits for activation.
            .filter(|t| !(t.hexen && (t.flags & 32) != 0))
            .map(|t| (f64::from(t.x), f64::from(t.y), t.args))
            .collect();
        for (x, y, args) in things {
            self.set_environment_at(x, y, env_id(args[0], args[1]));
        }
    }

    /// `Sector.SetEnvironmentID` for the zone containing `(x, y)`.
    pub fn set_environment_at(&mut self, x: f64, y: f64, env: u16) {
        if let Some(s) = crate::physics::sector_at_xy(self, x, y) {
            self.set_sector_environment(s, env);
        }
    }

    /// Set the environment of `sector`'s zone.
    pub fn set_sector_environment(&mut self, sector: usize, env: u16) {
        if let Some(&z) = self.sound_env.sector_zone.get(sector)
            && let Some(slot) = self.sound_env.zone_env.get_mut(usize::from(z))
        {
            *slot = env;
        }
    }

    /// Reverb environment heard by `listener` and whether it is under water
    /// (`waterlevel == 3`, ZDoom's `listener.underwater`).
    #[must_use]
    pub fn listener_environment(&self, listener: ActorId) -> (u16, bool) {
        let Some(a) = self.actors.get(listener) else {
            return (0, false);
        };
        let env = crate::physics::sector_at_xy(self, a.pos.x(), a.pos.y())
            .map_or(0, |s| self.sound_env.sector_env(s));
        (env, crate::physics::actor_waterlevel(self, a) >= 3)
    }
}

/// REVERBS ID from the two thing arguments.
#[must_use]
pub fn env_id(id1: i32, id2: i32) -> u16 {
    (u16::try_from(id1.clamp(0, 255)).unwrap_or(0) << 8) | u16::try_from(id2.clamp(0, 255)).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use room_nodebuild::apply_nodes;
    use room_testkit::{MapBuilder, rect};

    #[test]
    fn sound_environment_things_set_their_zone_only() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .room("b", rect(128.0, 0.0, 256.0, 128.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        // Split the rooms with `zoneboundary`, then drop a SoundEnvironment
        // (Cave = 8 0) into room b.
        map.udmf_user.line_ext = map
            .lines
            .iter()
            .map(|l| {
                if l.back_sector.is_some() {
                    room_map::udmf::ext::LINE_ZONEBOUNDARY
                } else {
                    0
                }
            })
            .collect();
        let mut env = map.things[0].clone();
        env.type_id = SOUND_ENVIRONMENT_DOOMEDNUM;
        env.x = 192;
        env.y = 64;
        env.args = [8, 0, 0, 0, 0];
        env.hexen = true;
        env.flags = 0;
        map.things.push(env);
        apply_nodes(&mut map);
        let mut w = World::from_map(map).unwrap();
        w.init_sound_zones(0);
        assert_eq!(w.sound_env.zone_env.len(), 2);
        let a = crate::physics::sector_at_xy(&w, 64.0, 64.0).unwrap();
        let b = crate::physics::sector_at_xy(&w, 192.0, 64.0).unwrap();
        assert_eq!(w.sound_env.sector_env(a), 0);
        assert_eq!(w.sound_env.sector_env(b), 0x0800);
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        assert_eq!(w.listener_environment(mo), (0, false));
        w.actors.get_mut(mo).unwrap().pos = room_core::DVec3::new(192.0, 64.0, 0.0);
        assert_eq!(w.listener_environment(mo).0, 0x0800);
        assert_eq!(env_id(30, 1), 0x1e01);
    }
}
