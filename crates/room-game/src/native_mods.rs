//! Host-side (non-WASM) play mods — e.g. desktop Python via `room-python-host`.

use room_sim::World;

/// One host-language play module (desktop / tools; not netplay-safe by default).
pub trait NativePlayMod: Send {
    /// Display / log name.
    fn name(&self) -> &str;

    /// Map start (`WorldLoaded`).
    fn begin_map(&mut self, world: &mut World);

    /// After each sim tic (`WorldTick` + death notifications).
    fn tick(&mut self, world: &mut World);

    /// Map exit (`WorldUnloaded`).
    fn end_map(&mut self, world: &mut World);
}

/// Source bytes for a host Python module (UTF-8 `.py`).
#[derive(Clone, Debug)]
pub struct PythonModSource {
    /// Manifest mod id.
    pub mod_id: String,
    /// Module name.
    pub name: String,
    /// Python source.
    pub source: String,
}
