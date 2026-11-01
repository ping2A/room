//! Educational headless Room bindings for Python (`docs/12` API shape).
//!
//! Not for netplay / production mods — those compile to WASM (Rust or
//! AssemblyScript). This crate lets students poke the playsim from a REPL
//! and drive a Python `RoomMod` the same way a WASM module would.

use bytes::Bytes;
use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;
use room_game::{
    GameOptions, Headless, Skill, load_embedded_engine_data, mount_embedded_engine,
};
use room_map::{MapGeometry, load_map};
use room_sim::{Buttons, TicCmd, FLAG_COUNTKILL};
use room_testkit::{MapBuilder, rect};
use room_vfs::VfsBuilder;
use std::collections::HashMap;
use std::path::Path;

fn err(e: impl std::fmt::Display) -> PyErr {
    PyRuntimeError::new_err(e.to_string())
}

fn arena_map() -> MapGeometry {
    let mut map = MapBuilder::new()
        .room("a", rect(0.0, 0.0, 2048.0, 2048.0), 0.0, 256.0)
        .player_start(512.0, 512.0, 0)
        .build_udmf("MAP01")
        .expect("arena udmf");
    room_nodebuild::apply_nodes(&mut map);
    map
}

/// One living (or recently observed) actor snapshot for Python.
#[pyclass]
#[derive(Clone, Debug)]
struct ActorView {
    #[pyo3(get)]
    id: u32,
    #[pyo3(get)]
    class_name: String,
    #[pyo3(get)]
    x: f64,
    #[pyo3(get)]
    y: f64,
    #[pyo3(get)]
    z: f64,
    #[pyo3(get)]
    angle: f64,
    #[pyo3(get)]
    health: i32,
    #[pyo3(get)]
    is_player: bool,
    #[pyo3(get)]
    is_monster: bool,
}

/// Headless Room session (synthetic arena or IWAD map).
#[pyclass]
struct Session {
    game: Headless,
    /// Actor ids that were alive last poll (for death detection in the Python host).
    prev_alive: HashMap<u32, ActorView>,
}

impl Session {
    fn boot(map: MapGeometry, skill: u8) -> PyResult<Self> {
        let data = load_embedded_engine_data().map_err(err)?;
        let skill = Skill::new(skill);
        let mut game = Headless::new(GameOptions {
            skill,
            ..Default::default()
        });
        game.set_defs(data.classes, data.states);
        game.start_custom_map(map, skill).map_err(err)?;
        let mut s = Self {
            game,
            prev_alive: HashMap::new(),
        };
        s.refresh_alive();
        Ok(s)
    }

    fn refresh_alive(&mut self) {
        self.prev_alive.clear();
        for a in self.snapshot_actors(true) {
            self.prev_alive.insert(a.id, a);
        }
    }

    fn snapshot_actors(&self, monsters_only: bool) -> Vec<ActorView> {
        let Some(w) = self.game.world() else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for &id in &w.thinkers.default {
            let Some(a) = w.actors.get(id) else {
                continue;
            };
            if a.destroyed || a.health <= 0 {
                continue;
            }
            let class_flags = w.classes.get(a.class).map(|c| c.flags).unwrap_or(0);
            let is_monster = (a.flags | class_flags) & FLAG_COUNTKILL != 0;
            if monsters_only && !is_monster {
                continue;
            }
            let is_player = w
                .players
                .iter()
                .flatten()
                .any(|p| p.mo == Some(id));
            let class_name = w
                .classes
                .get(a.class)
                .map(|c| c.name.clone())
                .unwrap_or_else(|| "?".into());
            out.push(ActorView {
                id: id.index(),
                class_name,
                x: a.pos.x(),
                y: a.pos.y(),
                z: a.pos.z(),
                angle: a.angle.degrees(),
                health: a.health,
                is_player,
                is_monster,
            });
        }
        out
    }
}

#[pymethods]
impl Session {
    /// Empty 2048² arena with a player start (no IWAD required).
    #[staticmethod]
    #[pyo3(signature = (skill = 3))]
    fn arena(skill: u8) -> PyResult<Self> {
        Self::boot(arena_map(), skill)
    }

    /// Load `map_name` from an IWAD/PWAD path (e.g. `testdata/doom/doom1.wad`, `E1M1`).
    #[staticmethod]
    #[pyo3(signature = (iwad_path, map_name = "E1M1", skill = 3))]
    fn from_iwad(iwad_path: &str, map_name: &str, skill: u8) -> PyResult<Self> {
        let path = Path::new(iwad_path);
        if !path.is_file() {
            return Err(err(format!("IWAD not found: {iwad_path}")));
        }
        let bytes = std::fs::read(path).map_err(err)?;
        let vfs = mount_embedded_engine(VfsBuilder::new())
            .map_err(err)?
            .mount_memory(
                path.file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or("game.wad"),
                Bytes::from(bytes),
            )
            .map_err(err)?
            .build()
            .map_err(err)?;
        let mut map = load_map(&vfs, map_name).map_err(err)?;
        room_nodebuild::ensure_nodes(&mut map);
        Self::boot(map, skill)
    }

    /// Current map time in tics (35 Hz).
    #[getter]
    fn maptime(&self) -> u32 {
        self.game.world().map(|w| w.level.maptime).unwrap_or(0)
    }

    /// Advance one tic. Movement is `-1.0..=1.0`; `attack` holds fire.
    #[pyo3(signature = (forward = 0.0, side = 0.0, attack = false))]
    fn tick(&mut self, forward: f32, side: f32, attack: bool) {
        let mut cmd = TicCmd::default();
        #[allow(clippy::cast_possible_truncation)]
        {
            // Doom ticcmd: signed 16-bit, ≈ `value * 256` for classic run speeds.
            cmd.forward_move = (forward.clamp(-1.0, 1.0) * 50.0 * 256.0) as i16;
            cmd.side_move = (side.clamp(-1.0, 1.0) * 50.0 * 256.0) as i16;
        }
        if attack {
            cmd.buttons = Buttons(Buttons::ATTACK);
        }
        self.game.tick(&[cmd]);
    }

    /// Player 0 health, or `None` if missing.
    fn player_health(&self) -> Option<i32> {
        let w = self.game.world()?;
        let mo = w.players.first()?.as_ref()?.mo?;
        Some(w.actors.get(mo)?.health)
    }

    /// Player 0 `(x, y, z, angle_deg)`, or `None`.
    fn player_pose(&self) -> Option<(f64, f64, f64, f64)> {
        let w = self.game.world()?;
        let mo = w.players.first()?.as_ref()?.mo?;
        let a = w.actors.get(mo)?;
        Some((a.pos.x(), a.pos.y(), a.pos.z(), a.angle.degrees()))
    }

    /// Set player 0 health (educational; not a real cheat code).
    fn set_player_health(&mut self, health: i32) -> bool {
        let Some(w) = self.game.world_mut() else {
            return false;
        };
        let Some(mo) = w.players.first().and_then(|p| p.as_ref()).and_then(|p| p.mo) else {
            return false;
        };
        if let Some(a) = w.actors.get_mut(mo) {
            a.health = health.max(0);
            true
        } else {
            false
        }
    }

    /// `give all` / weapon / ammo cheat string (same as console `give`).
    fn cheat_give(&mut self, what: &str) -> u32 {
        self.game.cheat_give(0, what)
    }

    /// Spawn a class near the player (or at absolute `x`,`y` if given).
    #[pyo3(signature = (class_name, x = None, y = None, angle = 180.0))]
    fn spawn(
        &mut self,
        class_name: &str,
        x: Option<f64>,
        y: Option<f64>,
        angle: f64,
    ) -> PyResult<Option<u32>> {
        let (px, py) = match (x, y) {
            (Some(x), Some(y)) => (x, y),
            _ => self
                .player_pose()
                .map(|(x, y, _, _)| (x + 256.0, y))
                .ok_or_else(|| err("no player"))?,
        };
        let w = self.game.world_mut().ok_or_else(|| err("no world"))?;
        let cid = w
            .classes
            .find_name(class_name)
            .ok_or_else(|| err(format!("unknown class {class_name}")))?;
        let ang = room_core::Angle::from_degrees(angle);
        let pos = room_core::DVec3::new(px, py, 0.0);
        match w.spawn_at(cid, pos, ang) {
            Ok(id) => Ok(Some(id.index())),
            Err(e) => Err(err(e)),
        }
    }

    /// Living actors (optionally monsters only).
    #[pyo3(signature = (monsters_only = false))]
    fn actors(&self, monsters_only: bool) -> Vec<ActorView> {
        self.snapshot_actors(monsters_only)
    }

    /// Drain HUD / script print lines since the last call.
    fn drain_messages(&mut self) -> Vec<String> {
        let mut out = Vec::new();
        if let Some(w) = self.game.world_mut() {
            out.extend(w.drain_messages());
        }
        out.extend(self.game.drain_script_log());
        out
    }

    /// Monsters that died since the previous call (Python `RoomMod.on_thing_died`).
    fn poll_deaths(&mut self) -> Vec<ActorView> {
        let now: HashMap<u32, ActorView> = self
            .snapshot_actors(true)
            .into_iter()
            .map(|a| (a.id, a))
            .collect();
        let mut died = Vec::new();
        for (id, prev) in &self.prev_alive {
            if prev.is_monster && !now.contains_key(id) {
                died.push(prev.clone());
            }
        }
        self.prev_alive = now;
        died
    }

    /// Push a message as if the mod called `print` (shows in drain_messages).
    fn mod_print(&mut self, text: &str) {
        if let Some(w) = self.game.world_mut() {
            w.push_message(text);
        }
    }
}

/// Check whether a path looks like a readable IWAD/PWAD.
#[pyfunction]
fn iwad_exists(path: &str) -> bool {
    Path::new(path).is_file()
}

#[pymodule]
fn _native(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<Session>()?;
    m.add_class::<ActorView>()?;
    m.add_function(wrap_pyfunction!(iwad_exists, m)?)?;
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;
    Ok(())
}
