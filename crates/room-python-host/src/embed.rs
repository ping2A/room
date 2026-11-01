//! Host-side CPython play mods for desktop (`room-app --features python-mods`).
//!
//! Not netplay-safe: callbacks run in-process against a live [`World`] via a
//! thread-local bridge. Manifest modules use `language = "python"`.

#![allow(clippy::missing_errors_doc)]

use std::cell::RefCell;
use std::collections::HashMap;
use std::ffi::CString;

use pyo3::prelude::*;
use pyo3::types::{PyAnyMethods, PyDict, PyModule};
use room_game::{NativePlayMod, PythonModSource};
use room_sim::{World, FLAG_COUNTKILL};

const SDK_PY: &str = include_str!("../../../python/room/sdk.py");

thread_local! {
    static WORLD: RefCell<Option<*mut World>> = const { RefCell::new(None) };
}

fn with_world<R>(f: impl FnOnce(&mut World) -> R) -> Option<R> {
    WORLD.with(|slot| {
        let ptr = (*slot.borrow())?;
        // SAFETY: set only for NativePlayMod hooks on the owning thread.
        Some(f(unsafe { &mut *ptr }))
    })
}

fn log_py_err(log: &mut Vec<String>, ctx: &str, e: PyErr, py: Python<'_>) {
    log.push(format!("{ctx}: {e}"));
    e.print(py);
}

struct WorldGuard;

impl WorldGuard {
    fn enter(world: &mut World) -> Self {
        WORLD.with(|slot| {
            *slot.borrow_mut() = Some(std::ptr::from_mut(world));
        });
        Self
    }
}

impl Drop for WorldGuard {
    fn drop(&mut self) {
        WORLD.with(|slot| {
            *slot.borrow_mut() = None;
        });
    }
}

#[derive(Clone)]
struct ActorSnap {
    id: u32,
    class_name: String,
    x: f64,
    y: f64,
    z: f64,
    angle: f64,
    health: i32,
    is_player: bool,
    is_monster: bool,
}

fn snapshot_monsters(world: &World) -> HashMap<u32, ActorSnap> {
    let mut out = HashMap::new();
    for &id in &world.thinkers.default {
        let Some(a) = world.actors.get(id) else {
            continue;
        };
        if a.destroyed || a.health <= 0 {
            continue;
        }
        let class_flags = world.classes.get(a.class).map(|c| c.flags).unwrap_or(0);
        let is_monster = (a.flags | class_flags) & FLAG_COUNTKILL != 0;
        if !is_monster {
            continue;
        }
        let is_player = world
            .players
            .iter()
            .flatten()
            .any(|p| p.mo == Some(id));
        let class_name = world
            .classes
            .get(a.class)
            .map(|c| c.name.clone())
            .unwrap_or_else(|| "?".into());
        out.insert(
            id.index(),
            ActorSnap {
                id: id.index(),
                class_name,
                x: a.pos.x(),
                y: a.pos.y(),
                z: a.pos.z(),
                angle: a.angle.degrees(),
                health: a.health,
                is_player,
                is_monster,
            },
        );
    }
    out
}

fn player_snap(world: &World) -> Option<ActorSnap> {
    let mo = world.players.first()?.as_ref()?.mo?;
    let a = world.actors.get(mo)?;
    let class_name = world
        .classes
        .get(a.class)
        .map(|c| c.name.clone())
        .unwrap_or_else(|| "Player".into());
    Some(ActorSnap {
        id: mo.index(),
        class_name,
        x: a.pos.x(),
        y: a.pos.y(),
        z: a.pos.z(),
        angle: a.angle.degrees(),
        health: a.health,
        is_player: true,
        is_monster: false,
    })
}

/// Bridge object bound onto each Python `ModBase` (`self._session`).
#[pyclass(name = "HostSession")]
struct HostSession;

#[pymethods]
impl HostSession {
    fn maptime(&self) -> u32 {
        with_world(|w| w.level.maptime).unwrap_or(0)
    }

    fn skill(&self) -> u8 {
        with_world(|w| w.skill).unwrap_or(3)
    }

    fn player_health(&self) -> Option<i32> {
        with_world(|w| {
            let mo = w.players.first()?.as_ref()?.mo?;
            Some(w.actors.get(mo)?.health)
        })
        .flatten()
    }

    fn player_pose(&self) -> Option<(f64, f64, f64, f64)> {
        with_world(|w| {
            let mo = w.players.first()?.as_ref()?.mo?;
            let a = w.actors.get(mo)?;
            Some((a.pos.x(), a.pos.y(), a.pos.z(), a.angle.degrees()))
        })
        .flatten()
    }

    fn set_player_health(&self, health: i32) -> bool {
        with_world(|w| {
            let Some(mo) = w.players.first().and_then(|p| p.as_ref()).and_then(|p| p.mo) else {
                return false;
            };
            if let Some(a) = w.actors.get_mut(mo) {
                a.health = health.max(0);
                true
            } else {
                false
            }
        })
        .unwrap_or(false)
    }

    #[pyo3(signature = (class_name, x=None, y=None, angle=180.0))]
    fn spawn(
        &self,
        class_name: &str,
        x: Option<f64>,
        y: Option<f64>,
        angle: f64,
    ) -> PyResult<Option<u32>> {
        with_world(|w| {
            let (px, py) = match (x, y) {
                (Some(x), Some(y)) => (x, y),
                _ => {
                    let mo = w
                        .players
                        .first()
                        .and_then(|p| p.as_ref())
                        .and_then(|p| p.mo)
                        .ok_or_else(|| pyo3::exceptions::PyRuntimeError::new_err("no player"))?;
                    let a = w
                        .actors
                        .get(mo)
                        .ok_or_else(|| pyo3::exceptions::PyRuntimeError::new_err("no player"))?;
                    (a.pos.x() + 256.0, a.pos.y())
                }
            };
            let cid = w.classes.find_name(class_name).ok_or_else(|| {
                pyo3::exceptions::PyValueError::new_err(format!("unknown class {class_name}"))
            })?;
            let ang = room_core::Angle::from_degrees(angle);
            let pos = room_core::DVec3::new(px, py, 0.0);
            match w.spawn_at(cid, pos, ang) {
                Ok(id) => Ok(Some(id.index())),
                Err(e) => Err(pyo3::exceptions::PyRuntimeError::new_err(e.to_string())),
            }
        })
        .unwrap_or_else(|| Err(pyo3::exceptions::PyRuntimeError::new_err("no world")))
    }

    fn mod_print(&self, text: &str) {
        let _ = with_world(|w| {
            w.push_message(text);
        });
    }
}

fn cstr(s: &str) -> CString {
    CString::new(s.replace('\0', "")).unwrap_or_default()
}

fn ensure_sdk(py: Python<'_>) -> PyResult<()> {
    let sys = py.import("sys")?;
    let modules = sys.getattr("modules")?.downcast_into::<PyDict>()?;
    if modules.contains("room.sdk")? {
        return Ok(());
    }
    let room = PyModule::new(py, "room")?;
    let sdk = PyModule::from_code(py, &cstr(SDK_PY), &cstr("room/sdk.py"), &cstr("room.sdk"))?;
    modules.set_item("room", &room)?;
    modules.set_item("room.sdk", &sdk)?;
    room.setattr("sdk", &sdk)?;
    Ok(())
}

fn load_instance(py: Python<'_>, source: &PythonModSource) -> PyResult<Py<PyAny>> {
    ensure_sdk(py)?;
    let safe: String = source
        .name
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    let module = PyModule::from_code(
        py,
        &cstr(&source.source),
        &cstr(&format!("{}.py", source.name)),
        &cstr(&format!("room_mod_{safe}")),
    )?;
    let instance = if let Ok(create) = module.getattr("create") {
        create.call0()?
    } else {
        module.getattr("MOD")?
    };
    let session = Bound::new(py, HostSession)?;
    if instance.hasattr("bind")? {
        let _ = instance.call_method1("bind", (session.clone(),));
    } else {
        let _ = instance.setattr("_session", session);
    }
    if let Some(skill) = with_world(|w| w.skill) {
        let _ = instance.setattr("_skill", skill);
    }
    if instance.hasattr("register")? {
        let _ = instance.call_method0("register");
    }
    Ok(instance.unbind())
}

fn actor_to_py<'py>(py: Python<'py>, snap: &ActorSnap) -> PyResult<Bound<'py, PyAny>> {
    let sdk = py.import("room.sdk")?;
    let actor_cls = sdk.getattr("Actor")?;
    actor_cls.call1((
        snap.id,
        &snap.class_name,
        snap.x,
        snap.y,
        snap.z,
        snap.angle,
        snap.health,
        snap.is_player,
        snap.is_monster,
    ))
}

struct LoadedMod {
    name: String,
    instance: Py<PyAny>,
    prev_alive: HashMap<u32, ActorSnap>,
}

/// One or more Python play modules sharing the host interpreter.
pub struct PythonModBundle {
    mods: Vec<LoadedMod>,
    log: Vec<String>,
}

impl PythonModBundle {
    /// Compile / instantiate every source. Requires an embedded CPython.
    pub fn load(sources: &[PythonModSource]) -> Result<Self, String> {
        if sources.is_empty() {
            return Ok(Self {
                mods: Vec::new(),
                log: Vec::new(),
            });
        }
        Python::with_gil(|py| {
            let mut mods = Vec::new();
            let mut log = Vec::new();
            for src in sources {
                match load_instance(py, src) {
                    Ok(instance) => {
                        log.push(format!("python: loaded {}", src.name));
                        mods.push(LoadedMod {
                            name: src.name.clone(),
                            instance,
                            prev_alive: HashMap::new(),
                        });
                    }
                    Err(e) => {
                        let msg = format!("python {}: {e}", src.name);
                        e.print(py);
                        return Err(msg);
                    }
                }
            }
            Ok(Self { mods, log })
        })
    }

    /// Drain host log lines (load messages).
    pub fn drain_log(&mut self) -> Vec<String> {
        std::mem::take(&mut self.log)
    }
}

impl NativePlayMod for PythonModBundle {
    fn name(&self) -> &str {
        self.mods
            .first()
            .map(|m| m.name.as_str())
            .unwrap_or("python")
    }

    fn begin_map(&mut self, world: &mut World) {
        let _guard = WorldGuard::enter(world);
        let maptime = world.level.maptime;
        let skill = world.skill;
        let alive = snapshot_monsters(world);
        let mut log = Vec::new();
        Python::with_gil(|py| {
            for m in &mut self.mods {
                m.prev_alive = alive.clone();
                let bound = m.instance.bind(py);
                let _ = bound.setattr("_skill", skill);
                if let Err(e) = bound.call_method1("on_load", (maptime,)) {
                    log_py_err(&mut log, &format!("python {}: on_load", m.name), e, py);
                }
            }
        });
        self.log.extend(log);
    }

    fn tick(&mut self, world: &mut World) {
        let _guard = WorldGuard::enter(world);
        let t = world.level.maptime;
        let now = snapshot_monsters(world);
        let killer = player_snap(world);
        let mut log = Vec::new();
        Python::with_gil(|py| {
            for m in &mut self.mods {
                let bound = m.instance.bind(py);
                if let Err(e) = bound.call_method1("on_tick", (t,)) {
                    log_py_err(&mut log, &format!("python {}: on_tick", m.name), e, py);
                }
                let mut died = Vec::new();
                for (id, prev) in &m.prev_alive {
                    if !now.contains_key(id) {
                        died.push(prev.clone());
                    }
                }
                m.prev_alive.clone_from(&now);
                for snap in died {
                    let Ok(thing) = actor_to_py(py, &snap) else {
                        continue;
                    };
                    let killer_py: PyObject = match &killer {
                        Some(k) => match actor_to_py(py, k) {
                            Ok(o) => o.unbind(),
                            Err(_) => py.None(),
                        },
                        None => py.None(),
                    };
                    if let Err(e) = bound.call_method1("on_thing_died", (thing, killer_py)) {
                        log_py_err(
                            &mut log,
                            &format!("python {}: on_thing_died", m.name),
                            e,
                            py,
                        );
                    }
                }
            }
        });
        self.log.extend(log);
    }

    fn end_map(&mut self, world: &mut World) {
        let _guard = WorldGuard::enter(world);
        let mut log = Vec::new();
        Python::with_gil(|py| {
            for m in &mut self.mods {
                let bound = m.instance.bind(py);
                if let Err(e) = bound.call_method0("on_unload") {
                    log_py_err(&mut log, &format!("python {}: on_unload", m.name), e, py);
                }
                m.prev_alive.clear();
            }
        });
        self.log.extend(log);
    }
}

/// Build boxed native mods from discovered Python sources.
pub fn load_native_mods(
    sources: &[PythonModSource],
) -> Result<(Vec<Box<dyn NativePlayMod>>, Vec<String>), String> {
    if sources.is_empty() {
        return Ok((Vec::new(), Vec::new()));
    }
    let mut bundle = PythonModBundle::load(sources)?;
    let log = bundle.drain_log();
    Ok((vec![Box::new(bundle)], log))
}
