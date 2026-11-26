//! What to do when the OS hides and shows the app (docs/41 §3.2).
//!
//! Desktop windows get one `resumed` at start and never `suspended`. Phones
//! get `suspended` whenever the app leaves the screen (home button, a call,
//! the lock screen) and `resumed` when it comes back. While suspended the
//! drawing surface is gone: Android destroys the native window, so the wgpu
//! surface must be dropped and made again. Everything else (the game, the
//! GPU device, textures) stays, so coming back never reloads the map.

/// What `resumed` has to build.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ResumeStep {
    /// First start: window, surface, GPU device and render targets.
    CreateAll,
    /// Back from the background: only a new surface for the existing window.
    RecreateSurface,
    /// Nothing is missing (a repeated `resumed`).
    Nothing,
}

/// Running / suspended state of the app window.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Lifecycle {
    suspended: bool,
    /// How many times the app came back from the background.
    pub(crate) returns: u32,
}

impl Lifecycle {
    /// The OS showed the app. `has_window` / `has_surface`: what exists now.
    pub(crate) fn resumed(&mut self, has_window: bool, has_surface: bool) -> ResumeStep {
        let was_suspended = std::mem::replace(&mut self.suspended, false);
        if !has_window {
            ResumeStep::CreateAll
        } else if !has_surface {
            if was_suspended {
                self.returns = self.returns.saturating_add(1);
            }
            ResumeStep::RecreateSurface
        } else {
            ResumeStep::Nothing
        }
    }

    /// The OS hid the app. Returns `true` the first time, when the surface
    /// must be dropped and the game and sound paused.
    pub(crate) fn suspended(&mut self) -> bool {
        !std::mem::replace(&mut self.suspended, true)
    }

    /// Whether frames should run (game tics, drawing).
    pub(crate) fn is_running(&self) -> bool {
        !self.suspended
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn desktop_start_builds_everything_once() {
        let mut l = Lifecycle::default();
        assert!(l.is_running());
        assert_eq!(l.resumed(false, false), ResumeStep::CreateAll);
        assert_eq!(l.resumed(true, true), ResumeStep::Nothing);
        assert_eq!(l.returns, 0);
    }

    #[test]
    fn background_and_back_only_rebuilds_the_surface() {
        let mut l = Lifecycle::default();
        l.resumed(false, false);
        for round in 1..=20 {
            assert!(l.suspended(), "first suspend drops the surface");
            assert!(!l.suspended(), "a repeated suspend does nothing");
            assert!(!l.is_running());
            // The app dropped its surface on suspend.
            assert_eq!(l.resumed(true, false), ResumeStep::RecreateSurface);
            assert!(l.is_running());
            assert_eq!(l.returns, round);
        }
    }

    #[test]
    fn suspend_before_the_first_window_still_creates_it() {
        // Android can pause the activity before the first native window.
        let mut l = Lifecycle::default();
        assert!(l.suspended());
        assert_eq!(l.resumed(false, false), ResumeStep::CreateAll);
        assert!(l.is_running());
        assert_eq!(l.returns, 0);
    }
}
