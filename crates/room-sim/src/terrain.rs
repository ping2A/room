//! TERRAIN runtime table + hit-floor splash (M7 bootstrap).

use crate::world::World;
use room_core::math::DVec3;
use std::collections::BTreeMap;

/// Splash spawn / sound recipe.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SplashSpec {
    /// Small splash actor class name.
    pub small_class: Option<String>,
    /// Base splash actor class name.
    pub base_class: Option<String>,
    /// Chunk class (optional; not spawned in bootstrap).
    pub chunk_class: Option<String>,
    /// Splash sound.
    pub sound: Option<String>,
    /// Small-splash sound.
    pub small_sound: Option<String>,
    /// UZDoom `FSplashDef::NoAlert`: do not call `P_NoiseAlert` for this splash.
    pub no_alert: bool,
    /// `|vel_z|` below this uses the small splash (map units / tic).
    pub small_threshold: i32,
}

/// One terrain entry.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TerrainSpec {
    /// Splash name in [`TerrainTable::splashes`].
    pub splash: Option<String>,
    /// Liquid flag (informational / future footclip).
    pub liquid: bool,
    /// Damage applied on land (`damageamount` when `damageonland`).
    pub damage_on_land: i32,
}

/// Flat → terrain → splash registry (filled by the game shell from LOCK/TERRAIN defs).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TerrainTable {
    /// Flat lump (upper) → terrain name.
    pub floors: BTreeMap<String, String>,
    /// Ceiling flat lump (upper) → terrain name (`SetSectorTerrain` position ≠ 0).
    pub ceilings: BTreeMap<String, String>,
    /// Terrains by name.
    pub terrains: BTreeMap<String, TerrainSpec>,
    /// Splashes by name.
    pub splashes: BTreeMap<String, SplashSpec>,
    /// Default terrain name when flat is unmapped.
    pub default_terrain: Option<String>,
}

impl TerrainTable {
    /// Resolve terrain name for a floor flat.
    #[must_use]
    pub fn terrain_for_flat(&self, flat: &str) -> Option<&TerrainSpec> {
        let key = flat.to_ascii_uppercase();
        let name = self
            .floors
            .get(&key)
            .cloned()
            .or_else(|| self.default_terrain.clone())?;
        self.terrains.get(&name)
    }

    /// Splash for a floor flat, if any.
    #[must_use]
    pub fn splash_for_flat(&self, flat: &str) -> Option<&SplashSpec> {
        let terrain = self.terrain_for_flat(flat)?;
        let splash_name = terrain.splash.as_ref()?;
        self.splashes.get(splash_name)
    }
}

/// Hit-floor splash: sound + optional splash actor when `vel_z` is downward into the floor.
pub fn hit_floor_splash(world: &mut World, actor_id: crate::actor::ActorId, vel_z: f64) {
    if vel_z >= 0.0 {
        return;
    }
    if world.terrain.splashes.is_empty() && world.terrain.floors.is_empty() {
        return;
    }
    let Some(actor) = world.actors.get(actor_id).cloned() else {
        return;
    };
    let Some(sid) = room_nodebuild::point_in_sector(&world.map, actor.pos.x(), actor.pos.y())
    else {
        return;
    };
    let idx = sid.raw() as usize;
    let flat = world
        .sector_floor_pics
        .get(&(idx as u32))
        .cloned()
        .or_else(|| world.map.sectors.get(idx).map(|s| s.floor_pic.clone()))
        .unwrap_or_default();
    if flat.is_empty() {
        return;
    }
    let Some(splash) = world.terrain.splash_for_flat(&flat).cloned() else {
        return;
    };
    let hard = vel_z < -(splash.small_threshold as f64);
    let sound = if hard {
        splash.sound.as_deref().or(splash.small_sound.as_deref())
    } else {
        splash.small_sound.as_deref().or(splash.sound.as_deref())
    };
    if let Some(name) = sound {
        world.play_sound(name, actor.pos.x(), actor.pos.y(), actor.pos.z(), 1.0);
    }
    let class_name = if hard {
        splash
            .base_class
            .as_deref()
            .or(splash.small_class.as_deref())
    } else {
        splash
            .small_class
            .as_deref()
            .or(splash.base_class.as_deref())
    };
    if let Some(cname) = class_name {
        if let Some(cid) = world.classes.find_name(cname) {
            let pos = DVec3::new(actor.pos.x(), actor.pos.y(), actor.pos.z());
            let _ = world.spawn_at(cid, pos, actor.angle);
        }
    }
    // Optional land damage from terrain.
    if let Some(terrain) = world.terrain.terrain_for_flat(&flat) {
        if terrain.damage_on_land > 0 {
            let _ = crate::damage::damage_mobj(world, actor_id, None, None, terrain.damage_on_land);
        }
    }
    // UZDoom alerts from player splashes unless the splash definition says
    // `noalert`; NOSPLASHALERT filters the monsters receiving the alert.
    let is_player = world
        .players
        .iter()
        .flatten()
        .any(|player| player.mo == Some(actor_id));
    if is_player && !splash.no_alert && !actor.extra_flags.get_name("DONTSPLASH") {
        crate::action::noise_alert_splash(world, actor_id);
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use crate::world::World;
    use room_core::math::DVec3;
    use room_nodebuild::apply_nodes;
    use room_testkit::{MapBuilder, rect};

    fn water_table() -> TerrainTable {
        let mut t = TerrainTable::default();
        t.floors.insert("FWATER1".into(), "Water".into());
        t.terrains.insert(
            "Water".into(),
            TerrainSpec {
                splash: Some("Water".into()),
                liquid: true,
                damage_on_land: 0,
            },
        );
        t.splashes.insert(
            "Water".into(),
            SplashSpec {
                small_class: None,
                base_class: None,
                chunk_class: None,
                sound: Some("world/watersplash".into()),
                small_sound: Some("world/watersplash".into()),
                no_alert: false,
                small_threshold: 12,
            },
        );
        t
    }

    #[test]
    fn splash_plays_sound_on_hard_land() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        if let Some(s) = map.sectors.get_mut(0) {
            s.floor_pic = "FWATER1".into();
        }
        apply_nodes(&mut map);
        let mut world = World::from_map(map).unwrap();
        world.terrain = water_table();
        let mo = world.players[0].as_ref().unwrap().mo.unwrap();
        {
            let a = world.actors.get_mut(mo).unwrap();
            a.pos = DVec3::new(64.0, 64.0, 16.0);
            a.vel = DVec3::new(0.0, 0.0, -20.0);
        }
        let _ = world.drain_sound_events();
        crate::physics::z_movement(&mut world, mo);
        let sounds: Vec<_> = world
            .drain_sound_events()
            .into_iter()
            .map(|e| e.name)
            .collect();
        assert!(
            sounds.iter().any(|n| n.contains("watersplash")),
            "expected watersplash, got {sounds:?}"
        );
        assert_eq!(world.sectors[0].soundtarget, Some(mo));
    }

    #[test]
    fn nosplashalert_on_player_does_not_suppress_water_noise() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        if let Some(s) = map.sectors.get_mut(0) {
            s.floor_pic = "FWATER1".into();
        }
        apply_nodes(&mut map);
        let mut world = World::from_map(map).unwrap();
        world.terrain = water_table();
        let mo = world.players[0].as_ref().unwrap().mo.unwrap();
        world
            .actors
            .get_mut(mo)
            .unwrap()
            .extra_flags
            .set_name("NOSPLASHALERT", true);
        hit_floor_splash(&mut world, mo, -20.0);
        // NOSPLASHALERT is a receiver-side filter; it does not silence the
        // player splash or prevent the sector sound target from being set.
        assert_eq!(world.sectors[0].soundtarget, Some(mo));
    }
}
