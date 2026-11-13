//! Late AOT attachment (`docs/37` ZW4): a host that builds AOT modules out of
//! band (the browser compiles them asynchronously) registers a provider; the
//! play runtime asks it for each VM it creates, and again later.

use crate::interp::Zvm;
use crate::program::CompiledProgram;
use std::sync::Mutex;

/// Attaches an AOT tier to `vm`, built from the unlinked `program`, when one is
/// ready. Returns true when it attached.
pub type AotProvider = fn(&mut Zvm, &CompiledProgram) -> bool;

static PROVIDER: Mutex<Option<AotProvider>> = Mutex::new(None);

/// Install (or clear) the process-wide provider.
pub fn set_aot_provider(p: Option<AotProvider>) {
    if let Ok(mut slot) = PROVIDER.lock() {
        *slot = p;
    }
}

/// Ask the provider for `vm`. False without a provider or when nothing is ready.
pub fn provide_aot(vm: &mut Zvm, program: &CompiledProgram) -> bool {
    let p = PROVIDER.lock().ok().and_then(|g| *g);
    p.is_some_and(|f| f(vm, program))
}
