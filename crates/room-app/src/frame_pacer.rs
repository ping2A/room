//! Frame-rate cap and thermal limits (docs/41 M6).
//!
//! [`FramePacer`] decides whether the next frame may start now or the loop
//! should sleep until a deadline (`vid_maxfps`). [`limits`] turns the OS's
//! thermal state into a lower resolution ceiling first and a lower frame
//! rate only at the last level. Both are pure so tests can drive them with
//! made-up clocks and states.

use std::time::{Duration, Instant};

/// How hot the device says it is. Phones report this; desktops stay
/// [`Thermal::Nominal`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum Thermal {
    /// Normal.
    #[default]
    Nominal,
    /// Warm; the OS is not slowing anything down yet.
    Fair,
    /// Hot; the OS is slowing the CPU/GPU down.
    Serious,
    /// Very hot; the OS will slow down a lot or close apps.
    Critical,
}

impl Thermal {
    /// Name for logs and `ROOM_TEST_THERMAL`.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Nominal => "nominal",
            Self::Fair => "fair",
            Self::Serious => "serious",
            Self::Critical => "critical",
        }
    }

    /// Parse a [`Self::name`].
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        [Self::Nominal, Self::Fair, Self::Serious, Self::Critical]
            .into_iter()
            .find(|t| t.name().eq_ignore_ascii_case(text.trim()))
    }
}

/// What a thermal state allows.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Limits {
    /// Highest 3D resolution scale.
    pub scale_ceiling: f32,
    /// Turn on dynamic resolution even if `r_dynres` is off.
    pub dynamic_scale: bool,
    /// Highest frame rate (`None`: no extra cap).
    pub max_fps: Option<u32>,
}

/// Resolution goes first, frame rate only at [`Thermal::Critical`].
#[must_use]
pub fn limits(thermal: Thermal) -> Limits {
    match thermal {
        Thermal::Nominal | Thermal::Fair => Limits {
            scale_ceiling: 1.0,
            dynamic_scale: false,
            max_fps: None,
        },
        Thermal::Serious => Limits {
            scale_ceiling: 0.75,
            dynamic_scale: true,
            max_fps: None,
        },
        Thermal::Critical => Limits {
            scale_ceiling: crate::render_scale::MIN_SCALE,
            dynamic_scale: true,
            max_fps: Some(30),
        },
    }
}

/// The cap in force: the lower of the setting and the thermal cap, where
/// 0 means "none".
#[must_use]
pub fn effective_cap(setting: u32, thermal: Option<u32>) -> u32 {
    match (setting, thermal) {
        (0, None) => 0,
        (0, Some(t)) => t,
        (s, None) => s,
        (s, Some(t)) => s.min(t),
    }
}

/// Whether the loop must time frames itself. With vsync the display already
/// holds frames to its refresh rate, so a cap at or above it needs no timer
/// (a timer next to vsync makes frames miss a refresh now and then). An
/// unknown refresh rate always uses the timer.
#[must_use]
pub fn timer_needed(cap: u32, refresh_millihertz: Option<u32>) -> bool {
    match (cap, refresh_millihertz) {
        (0, _) => false,
        // 1 Hz of slack: a "60 Hz" display often reports 59.94 or 60.01.
        (cap, Some(mhz)) => u64::from(cap) * 1000 + 1000 < u64::from(mhz),
        (_, None) => true,
    }
}

/// Wakes this early still counts as on time (timers may fire a little
/// before the deadline).
const EARLY: Duration = Duration::from_micros(500);

/// Spaces frames `1 / cap` seconds apart.
#[derive(Debug, Default)]
pub struct FramePacer {
    next: Option<Instant>,
}

impl FramePacer {
    /// A pacer with no frame drawn yet.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// `None`: draw now (and the next deadline is set). `Some(t)`: wait
    /// until `t`. A cap of 0 always draws.
    pub fn poll(&mut self, now: Instant, cap: u32) -> Option<Instant> {
        if cap == 0 {
            self.next = None;
            return None;
        }
        let period = Duration::from_secs_f64(1.0 / f64::from(cap));
        match self.next {
            // A higher cap set while waiting must not keep the old, longer wait.
            Some(t) if now + EARLY < t => {
                let t = t.min(now + period);
                self.next = Some(t);
                if now + EARLY < t {
                    return Some(t);
                }
                self.next = Some(now + period);
            }
            // A little early: follow the actual start, so a display that
            // runs slightly faster than the cap never drifts into a wait.
            Some(t) if now < t => self.next = Some(now + period),
            // Late: keep a steady rhythm; after a stall (more than one period
            // late) start again from now instead of drawing a burst to catch up.
            Some(t) => {
                let next = t + period;
                self.next = Some(if next <= now { now + period } else { next });
            }
            None => self.next = Some(now + period),
        }
        None
    }

    /// Forget the rhythm (after a pause or a suspend).
    pub fn reset(&mut self) {
        self.next = None;
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    /// Run the loop for `total` with frames taking `work` each, waking
    /// exactly at deadlines; return the frame start times.
    fn simulate(cap: u32, work: Duration, total: Duration) -> Vec<Duration> {
        let t0 = Instant::now();
        let mut now = t0;
        let mut p = FramePacer::new();
        let mut starts = Vec::new();
        while now - t0 < total {
            match p.poll(now, cap) {
                None => {
                    starts.push(now - t0);
                    now += work;
                }
                Some(t) => now = t,
            }
        }
        starts
    }

    #[test]
    fn no_cap_never_waits() {
        let mut p = FramePacer::new();
        let now = Instant::now();
        for i in 0..5 {
            assert_eq!(p.poll(now + ms(i), 0), None);
        }
    }

    #[test]
    fn a_fast_frame_waits_for_the_next_slot() {
        let starts = simulate(30, ms(5), ms(990));
        assert_eq!(starts.len(), 30);
        for pair in starts.windows(2) {
            let gap = pair[1] - pair[0];
            assert!(
                gap.abs_diff(Duration::from_secs_f64(1.0 / 30.0)) < ms(1),
                "{gap:?}"
            );
        }
    }

    #[test]
    fn a_slow_frame_is_not_followed_by_a_burst() {
        let t0 = Instant::now();
        let mut p = FramePacer::new();
        assert_eq!(p.poll(t0, 60), None);
        // One frame took 100 ms (six periods): the next one starts at once,
        // the one after a full period later, not straight away.
        let late = t0 + ms(100);
        assert_eq!(p.poll(late, 60), None);
        let wait = p.poll(late + ms(2), 60).expect("waits");
        assert!((wait - late).abs_diff(ms(16)) < ms(1), "{:?}", wait - late);
    }

    #[test]
    fn slightly_late_frames_keep_the_rhythm() {
        // 60 fps cap, frames finishing a little after the deadline still
        // average 60 fps (no drift to 59, 58, …).
        let starts = simulate(60, Duration::from_micros(16_900), Duration::from_secs(2));
        assert!((115..=120).contains(&starts.len()), "{}", starts.len());
    }

    #[test]
    fn vsync_slightly_faster_than_the_cap_never_waits() {
        // Frames can only start on a 60.05 Hz vsync; the cap is 60. Every
        // vsync gets a frame for 30 s, with no wait creeping in.
        let vsync = Duration::from_secs_f64(1.0 / 60.05);
        let t0 = Instant::now();
        let mut p = FramePacer::new();
        for i in 0..1800u32 {
            assert_eq!(p.poll(t0 + vsync * i, 60), None, "frame {i}");
        }
    }

    #[test]
    fn waking_a_little_early_counts_as_on_time() {
        let t0 = Instant::now();
        let mut p = FramePacer::new();
        assert_eq!(p.poll(t0, 50), None);
        let deadline = p.poll(t0 + ms(1), 50).expect("waits");
        assert_eq!(p.poll(deadline - Duration::from_micros(200), 50), None);
    }

    #[test]
    fn raising_the_cap_shortens_the_wait_at_once() {
        let t0 = Instant::now();
        let mut p = FramePacer::new();
        assert_eq!(p.poll(t0, 10), None);
        let slow = p.poll(t0 + ms(1), 10).expect("waits");
        assert!(slow - t0 >= ms(99));
        let fast = p.poll(t0 + ms(1), 100).expect("waits");
        assert!(fast - t0 <= ms(12), "{:?}", fast - t0);
        assert_eq!(p.poll(t0 + ms(30), 100), None, "past the new deadline");
    }

    #[test]
    fn the_cap_needs_a_timer_only_below_the_display_rate() {
        assert!(!timer_needed(0, None));
        assert!(!timer_needed(60, Some(60_000)));
        assert!(!timer_needed(60, Some(59_940)));
        assert!(!timer_needed(120, Some(60_000)));
        assert!(timer_needed(30, Some(60_000)));
        assert!(timer_needed(60, Some(120_000)));
        assert!(timer_needed(60, None));
    }

    #[test]
    fn heat_lowers_resolution_before_frame_rate() {
        assert_eq!(limits(Thermal::Fair), limits(Thermal::Nominal));
        let serious = limits(Thermal::Serious);
        assert!(serious.scale_ceiling < 1.0 && serious.dynamic_scale);
        assert_eq!(
            serious.max_fps, None,
            "frame rate kept while resolution can drop"
        );
        let critical = limits(Thermal::Critical);
        assert_eq!(critical.scale_ceiling, crate::render_scale::MIN_SCALE);
        assert_eq!(critical.max_fps, Some(30));
    }

    #[test]
    fn the_lower_cap_wins_and_zero_means_none() {
        assert_eq!(effective_cap(0, None), 0);
        assert_eq!(effective_cap(0, Some(30)), 30);
        assert_eq!(effective_cap(60, None), 60);
        assert_eq!(effective_cap(60, Some(30)), 30);
        assert_eq!(effective_cap(20, Some(30)), 20);
    }

    #[test]
    fn thermal_names_round_trip() {
        for t in [
            Thermal::Nominal,
            Thermal::Fair,
            Thermal::Serious,
            Thermal::Critical,
        ] {
            assert_eq!(Thermal::parse(t.name()), Some(t));
        }
        assert_eq!(Thermal::parse(" Serious "), Some(Thermal::Serious));
        assert_eq!(Thermal::parse("hot"), None);
    }
}
