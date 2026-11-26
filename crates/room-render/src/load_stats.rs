//! How many textures were loaded (decoded and uploaded) on the render
//! thread, and how long that took: read once a frame by the app's frame
//! timing (`vid_fps`) to tell a slow frame caused by first-use loads.

use std::sync::atomic::{AtomicU64, Ordering};

static COUNT: AtomicU64 = AtomicU64::new(0);
static NANOS: AtomicU64 = AtomicU64::new(0);

/// Count one texture load that took `took`.
pub fn record(took: std::time::Duration) {
    COUNT.fetch_add(1, Ordering::Relaxed);
    NANOS.fetch_add(
        u64::try_from(took.as_nanos()).unwrap_or(u64::MAX),
        Ordering::Relaxed,
    );
}

/// Loads and their total time since the last call, then reset.
#[must_use]
pub fn take() -> (u64, std::time::Duration) {
    let n = COUNT.swap(0, Ordering::Relaxed);
    let t = NANOS.swap(0, Ordering::Relaxed);
    (n, std::time::Duration::from_nanos(t))
}

/// A stopwatch that also builds for the web, where `Instant::now` panics
/// (there it always reads zero).
#[derive(Clone, Copy, Debug)]
pub struct Timer {
    #[cfg(not(target_arch = "wasm32"))]
    started: std::time::Instant,
}

impl Timer {
    /// Start now.
    #[must_use]
    pub fn start() -> Self {
        Self {
            #[cfg(not(target_arch = "wasm32"))]
            started: std::time::Instant::now(),
        }
    }

    /// Time since [`Self::start`] (zero on the web).
    #[must_use]
    pub fn elapsed(&self) -> std::time::Duration {
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.started.elapsed()
        }
        #[cfg(target_arch = "wasm32")]
        {
            std::time::Duration::ZERO
        }
    }
}
