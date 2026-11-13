//! ZScript AOT tier in the browser (`docs/37` ZW4).
//!
//! The play program (or a converted mod's shipped module) is compiled to
//! WebAssembly and handed to the browser's own engine, so ZScript runs under
//! the browser JIT. Compilation is asynchronous: the game interprets until
//! the instance is ready, then [`refresh`] attaches it to the running VM.
//! Opt-in with `?zvm=aot` (`?zvm=aot-all` also enters short functions).

use room_zvm::CompiledProgram;
use room_zvm::web_backend::{WebInstance, instantiate};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;

thread_local! {
    static READY: RefCell<HashMap<(u64, u64), WebInstance>> = RefCell::new(HashMap::new());
    static STARTED: RefCell<HashMap<(u64, u64), ()>> = RefCell::new(HashMap::new());
    static PENDING: Cell<bool> = const { Cell::new(false) };
    static ALL: Cell<bool> = const { Cell::new(false) };
}

fn log(msg: &str) {
    web_sys::console::log_1(&format!("room-web: zscript aot: {msg}").into());
}

/// The `?zvm=` query parameter: `aot` / `aot-all` turn the tier on.
#[must_use]
pub fn requested(param: Option<&str>) -> Option<bool> {
    match param.map(str::to_ascii_lowercase).as_deref() {
        Some("aot") => Some(false),
        Some("aot-all") => Some(true),
        _ => None,
    }
}

fn provider(vm: &mut room_zvm::Zvm, program: &CompiledProgram) -> bool {
    let key = room_zvm_wasm::program_digest(program);
    READY.with(|r| {
        let Some(inst) = r.borrow().get(&key).cloned() else {
            return false;
        };
        match inst.attach(vm) {
            Ok(()) => {
                if ALL.with(Cell::get) {
                    vm.set_aot_policy(room_zvm::AotPolicy::All);
                }
                true
            }
            Err(e) => {
                log(&e);
                false
            }
        }
    })
}

/// Start compiling `program` for the browser engine (once per program).
pub fn prepare(program: &CompiledProgram, all: bool) {
    ALL.with(|a| a.set(all));
    room_zvm::set_aot_provider(Some(provider));
    let key = room_zvm_wasm::program_digest(program);
    if STARTED.with(|s| s.borrow_mut().insert(key, ()).is_some()) {
        return;
    }
    // A converted mod ships its module; otherwise compile the program here.
    let (bytes, compiled, hot) = match room_zvm_wasm::shipped_for(program) {
        Some(m) => (m.bytes.clone(), m.compiled.clone(), m.hot.clone()),
        None => match room_zvm_wasm::roommod::compile_room_mod(program) {
            Ok(art) => (art.bytes, art.compiled, art.hot),
            Err(e) => {
                log(&format!("codegen failed, interpreting: {e}"));
                return;
            }
        },
    };
    let size = bytes.len();
    wasm_bindgen_futures::spawn_local(async move {
        match instantiate(bytes, compiled, hot, true).await {
            Ok(inst) => {
                READY.with(|r| r.borrow_mut().insert(key, inst));
                PENDING.with(|p| p.set(true));
                log(&format!("module ready ({size} bytes); attaching"));
            }
            Err(e) => log(&format!("instantiate failed, interpreting: {e}")),
        }
    });
}

/// Attach a module that became ready since the last frame.
pub fn refresh(game: &mut room_game::Headless) {
    if !PENDING.with(|p| p.replace(false)) {
        return;
    }
    if let Some(w) = game.world_mut() {
        if w.zscript.refresh_aot() {
            log("attached to the running map");
        }
    }
}
