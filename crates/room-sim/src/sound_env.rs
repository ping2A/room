//! Per-zone reverb environment state (ZDoom `Level->Zones`).
//!
//! The playsim only stores environment IDs (`id1 << 8 | id2`, as REVERBS);
//! the shell maps them to reverb parameters. Render-side only: nothing here
//! feeds back into gameplay (or the checksum), so demos are unaffected.
//!
//! After map start the zone environments change the UZDoom ways:
//! `Thing_Activate` on a (dormant) `SoundEnvironment` thing — its
//! `Activate` calls `CurSector.SetEnvironmentID` — and ZScript
//! `Sector.SetEnvironmentID` / `Sector.SetEnvironment` (by name, through
//! [`SoundEnvState::names`] that the shell fills from REVERBS).

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
    /// `SoundEnvironment` map things (activatable by tid).
    pub things: Vec<EnvThing>,
    /// Lower-case REVERBS name → ID, for `Sector.SetEnvironment(name)`.
    pub names: std::collections::BTreeMap<String, u16>,
}

/// One `SoundEnvironment` map thing (DoomEdNum 9048). Room keeps it here
/// instead of spawning an actor: it has no sprite, physics or state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EnvThing {
    /// Thing ID (`Thing_Activate` target).
    pub tid: i32,
    /// Sector the thing stands in (its `CurSector`).
    pub sector: Option<usize>,
    /// `args[0] << 8 | args[1]`.
    pub env: u16,
    /// `bDormant`: placed dormant or deactivated.
    pub dormant: bool,
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

const SAVE_MAGIC: &[u8; 4] = b"FSEN";

impl SoundEnvState {
    /// Savegame block: zone environments and thing dormancy (`soundenv.bin`).
    #[must_use]
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = SAVE_MAGIC.to_vec();
        out.extend_from_slice(&u32::try_from(self.zone_env.len()).unwrap_or(0).to_le_bytes());
        for e in &self.zone_env {
            out.extend_from_slice(&e.to_le_bytes());
        }
        out.extend_from_slice(&u32::try_from(self.things.len()).unwrap_or(0).to_le_bytes());
        out.extend(self.things.iter().map(|t| u8::from(t.dormant)));
        out
    }

    /// Restore [`Self::to_bytes`] over the zones of the same map.
    ///
    /// # Errors
    /// Malformed block or one from a map with other zones / things.
    pub fn load_bytes(&mut self, b: &[u8]) -> Result<(), String> {
        let bad = || "bad soundenv.bin".to_owned();
        if b.get(..4) != Some(SAVE_MAGIC.as_slice()) {
            return Err(bad());
        }
        let u32_at = |o: usize| -> Option<usize> {
            Some(u32::from_le_bytes(b.get(o..o + 4)?.try_into().ok()?) as usize)
        };
        let zones = u32_at(4).ok_or_else(bad)?;
        if zones != self.zone_env.len() {
            return Err(bad());
        }
        let mut o = 8;
        for e in &mut self.zone_env {
            *e = u16::from_le_bytes(b.get(o..o + 2).ok_or_else(bad)?.try_into().map_err(|_| bad())?);
            o += 2;
        }
        let things = u32_at(o).ok_or_else(bad)?;
        if things != self.things.len() {
            return Err(bad());
        }
        o += 4;
        for t in &mut self.things {
            t.dormant = *b.get(o).ok_or_else(bad)? != 0;
            o += 1;
        }
        Ok(())
    }
}

impl World {
    /// Flood the sound zones and apply every non-dormant `SoundEnvironment`
    /// thing (`args[0] << 8 | args[1]`). `default_env` is MAPINFO
    /// `DefaultEnvironment`.
    pub fn init_sound_zones(&mut self, default_env: u16) {
        let (sector_zone, count) = flood_sound_zones(&self.map);
        let names = std::mem::take(&mut self.sound_env.names);
        let mut things = Vec::new();
        for t in self.map.things.iter().filter(|t| t.type_id == SOUND_ENVIRONMENT_DOOMEDNUM) {
            things.push(EnvThing {
                tid: i32::from(t.tid),
                sector: crate::physics::sector_at_xy(self, f64::from(t.x), f64::from(t.y)),
                env: env_id(t.args[0], t.args[1]),
                // Hexen / UDMF `dormant`: the thing waits for activation.
                dormant: t.hexen && (t.flags & 32) != 0,
            });
        }
        self.sound_env = SoundEnvState {
            sector_zone,
            zone_env: vec![default_env; count],
            things,
            names,
        };
        // `PostBeginPlay`: every non-dormant thing activates, in map order.
        let live: Vec<_> = self.sound_env.things.iter().filter(|t| !t.dormant).copied().collect();
        for t in live {
            if let Some(s) = t.sector {
                self.set_sector_environment(s, t.env);
            }
        }
    }

    /// `Thing_Activate` / `Thing_Deactivate` on `SoundEnvironment` things
    /// with `tid`: activating applies the environment to its zone (even if
    /// it was not dormant, as UZDoom's `Activate`); deactivating only marks
    /// it dormant. Returns whether any thing matched.
    pub fn activate_sound_environments(&mut self, tid: i32, activate: bool) -> bool {
        if tid == 0 {
            return false;
        }
        let mut hit = Vec::new();
        for t in self.sound_env.things.iter_mut().filter(|t| t.tid == tid) {
            t.dormant = !activate;
            hit.push(*t);
        }
        if activate {
            for t in &hit {
                if let Some(s) = t.sector {
                    self.set_sector_environment(s, t.env);
                }
            }
        }
        !hit.is_empty()
    }

    /// `Sector.SetEnvironment(name)`; unknown names are ignored (UZDoom's
    /// `S_FindEnvironment` falls back to "Off"… only for IDs, not names).
    pub fn set_sector_environment_name(&mut self, sector: usize, name: &str) {
        if let Some(&id) = self.sound_env.names.get(&name.trim().to_ascii_lowercase()) {
            self.set_sector_environment(sector, id);
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

    #[test]
    fn dormant_sound_environment_activates_by_tid_after_map_start() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        let mut env = map.things[0].clone();
        env.type_id = SOUND_ENVIRONMENT_DOOMEDNUM;
        env.tid = 7;
        env.args = [10, 0, 0, 0, 0]; // Hangar
        env.hexen = true;
        env.flags = 32; // dormant
        map.things.push(env);
        apply_nodes(&mut map);
        let mut w = World::from_map(map).unwrap();
        w.init_sound_zones(0);
        let s = crate::physics::sector_at_xy(&w, 64.0, 64.0).unwrap();
        assert_eq!(w.sound_env.sector_env(s), 0, "dormant at start");
        let args = [7, 0, 0, 0, 0];
        crate::specials::execute_special(&mut w, crate::specials::num::THING_ACTIVATE, &args, None, None);
        assert_eq!(w.sound_env.sector_env(s), 0x0A00);
        w.sound_env.names.insert("cave".into(), 0x0800);
        w.set_sector_environment_name(s, "Cave");
        assert_eq!(w.sound_env.sector_env(s), 0x0800);
        // Deactivate only marks it dormant; activating again re-applies.
        crate::specials::execute_special(&mut w, crate::specials::num::THING_DEACTIVATE, &args, None, None);
        assert_eq!(w.sound_env.sector_env(s), 0x0800);
        assert!(w.sound_env.things[0].dormant);
        crate::specials::execute_special(&mut w, crate::specials::num::THING_ACTIVATE, &args, None, None);
        assert_eq!(w.sound_env.sector_env(s), 0x0A00);
        // Zone state round-trips for savegames.
        let bytes = w.sound_env.to_bytes();
        let mut fresh = w.sound_env.clone();
        fresh.zone_env.fill(0);
        fresh.things[0].dormant = true;
        fresh.load_bytes(&bytes).unwrap();
        assert_eq!(fresh, w.sound_env);
    }

    /// UZ5 acceptance: client-side (render-only) state — particle requests,
    /// FX cues and zone sound environments — never reaches the checksum.
    #[test]
    fn render_only_state_is_excluded_from_the_checksum() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map(map).unwrap();
        w.fx_events_enabled = true;
        let before = w.checksum();
        w.particle_events.push(crate::world::SimParticleEvent {
            color: 0xff0000,
            lifetime: 35,
            size: 4.0,
            x: 64.0,
            y: 64.0,
            z: 32.0,
            fullbright: true,
            render_style: Some(11),
            velocity: room_core::DVec3::new(1.0, 0.0, 0.0),
            acceleration: room_core::DVec3::ZERO,
            alpha: 1.0,
            fade_step: 0.0,
            size_step: 0.0,
            flags: 1 << 9,
            fade_out_step: 0.0,
            roll: [0.0; 3],
            texture: Some("BAL1A0".into()),
        });
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        w.push_fx(crate::fx_events::FxEvent::BloodHit {
            victim: mo,
            pos: room_core::DVec3::new(64.0, 64.0, 32.0),
            dir: room_core::DVec3::new(1.0, 0.0, 0.0),
            damage: 10,
            color: crate::fx_events::Rgb8 { r: 200, g: 0, b: 0 },
        });
        w.set_sector_environment(0, 0x0A00);
        assert!(!w.particle_events.is_empty() && !w.fx_events.is_empty());
        assert_eq!(w.checksum(), before);
    }
}
