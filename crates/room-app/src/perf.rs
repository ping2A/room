//! Frame timing for the frame-rate display (`vid_fps`) and slow-frame
//! ("hitch") logs.
//!
//! Each frame is cut into named phases ([`FrameTimer::mark`]). A frame much
//! slower than the ones before it logs one line with where its time went and
//! how many pictures were loaded during it, e.g.
//!
//! `room: hitch 84 ms (work 71 ms): sim 41.2 (3 tics), hud 12.0, draw 8.3 | 12 loads 30.1 ms`
//!
//! so a phone log (`scripts/android-logs.sh`) says why the game stuttered.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

/// Frames kept for the graph and the statistics (about 4 s at 60 fps).
pub(crate) const HISTORY: usize = 240;

/// A frame this much slower than the median is a hitch.
const HITCH_FACTOR: f32 = 2.5;
/// …and at least this slow (ms): 30 fps frames on a 60 fps median are not.
const HITCH_MIN_MS: f32 = 45.0;
/// At most this many hitch lines a second (a long stall should not flood the log).
const HITCH_LINES_PER_SEC: u32 = 4;

/// Phases of one frame, in the order they were first marked.
#[derive(Default, Clone, Debug)]
pub(crate) struct Phases {
    entries: Vec<(&'static str, f32)>,
}

impl Phases {
    fn add(&mut self, name: &'static str, ms: f32) {
        if let Some(e) = self.entries.iter_mut().find(|e| e.0 == name) {
            e.1 += ms;
        } else {
            self.entries.push((name, ms));
        }
    }

    /// `(phase, ms)` in first-marked order.
    pub(crate) fn entries(&self) -> &[(&'static str, f32)] {
        &self.entries
    }
}

/// Numbers shown by the frame-rate display.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Summary {
    /// Frames presented in the last second.
    pub fps: u32,
    /// Average frame time over the last second (ms).
    pub avg_ms: f32,
    /// Slowest frame in the history (ms).
    pub worst_ms: f32,
}

/// Frame timing state; one per app.
pub(crate) struct FrameTimer {
    frame_start: Option<Instant>,
    last_mark: Instant,
    /// Start of the previous frame (frame interval = what the player sees).
    prev_start: Option<Instant>,
    phases: Phases,
    tics: u32,
    /// Frame intervals (ms), newest last.
    history: VecDeque<f32>,
    /// Start times matching `history` (for the per-second rate).
    starts: VecDeque<Instant>,
    log_window: Instant,
    lines_in_window: u32,
    /// Hitch lines written (for tests and the speed test).
    pub(crate) hitches: u64,
    /// Lines not written because of the per-second limit.
    suppressed: u32,
    /// The next frame is the first after a load (map change, new game):
    /// it is logged as such, not as a hitch.
    after_load: bool,
    /// `ROOM_PERF_LOG=1`: per-phase average and worst every 2 s.
    report: Option<Report>,
}

/// Running per-phase totals for `ROOM_PERF_LOG`.
#[derive(Default)]
struct Report {
    since: Option<Instant>,
    frames: u32,
    interval_sum: f32,
    interval_max: f32,
    /// (phase, sum ms, max ms)
    phases: Vec<(&'static str, f32, f32)>,
    loads: u64,
}

impl Report {
    fn add(&mut self, now: Instant, interval: f32, phases: &Phases, loads: u64) {
        let since = *self.since.get_or_insert(now);
        self.frames += 1;
        self.interval_sum += interval;
        self.interval_max = self.interval_max.max(interval);
        self.loads += loads;
        for &(name, ms) in phases.entries() {
            match self.phases.iter_mut().find(|p| p.0 == name) {
                Some(p) => {
                    p.1 += ms;
                    p.2 = p.2.max(ms);
                }
                None => self.phases.push((name, ms, ms)),
            }
        }
        if now.duration_since(since) >= Duration::from_secs(2) {
            let n = self.frames as f32;
            let parts: Vec<String> = self
                .phases
                .iter()
                .map(|(name, sum, max)| format!("{name} {:.1}/{max:.0}", sum / n))
                .collect();
            eprintln!(
                "room: perf: {} frames, avg {:.1} ms, worst {:.0} ms | {} | {} load(s)  (phase avg/worst ms)",
                self.frames,
                self.interval_sum / n,
                self.interval_max,
                parts.join(", "),
                self.loads
            );
            *self = Self::default();
        }
    }
}

impl Default for FrameTimer {
    fn default() -> Self {
        let now = Instant::now();
        Self {
            frame_start: None,
            last_mark: now,
            prev_start: None,
            phases: Phases::default(),
            tics: 0,
            history: VecDeque::with_capacity(HISTORY),
            starts: VecDeque::with_capacity(HISTORY),
            log_window: now,
            lines_in_window: 0,
            hitches: 0,
            suppressed: 0,
            after_load: true,
            report: std::env::var_os("ROOM_PERF_LOG")
                .is_some_and(|v| v != "0")
                .then(Report::default),
        }
    }
}

impl FrameTimer {
    /// Start a frame.
    pub(crate) fn begin(&mut self) {
        self.begin_at(Instant::now());
    }

    fn begin_at(&mut self, now: Instant) {
        self.frame_start = Some(now);
        self.last_mark = now;
        self.phases = Phases::default();
        self.tics = 0;
        // Drop the loads counted between frames (they belong to loading).
        let _ = room_render::load_stats::take();
        let _ = room_ui::load_stats::take();
    }

    /// Time since the last mark goes to `phase`.
    pub(crate) fn mark(&mut self, phase: &'static str) {
        let now = Instant::now();
        let ms = now.duration_since(self.last_mark).as_secs_f32() * 1000.0;
        self.last_mark = now;
        self.phases.add(phase, ms);
    }

    /// Game tics run this frame.
    pub(crate) fn add_tics(&mut self, n: u32) {
        self.tics = self.tics.saturating_add(n);
    }

    /// The next frame follows a load (not a hitch).
    pub(crate) fn loaded(&mut self) {
        self.after_load = true;
    }

    /// End a frame: record its interval and log it if it was a hitch.
    pub(crate) fn end(&mut self) {
        let now = Instant::now();
        let Some(start) = self.frame_start.take() else {
            return;
        };
        let rest = now.duration_since(self.last_mark).as_secs_f32() * 1000.0;
        if rest > 0.05 {
            self.phases.add("other", rest);
        }
        let work = now.duration_since(start).as_secs_f32() * 1000.0;
        let interval = self
            .prev_start
            .map_or(work, |p| start.duration_since(p).as_secs_f32() * 1000.0)
            .max(work);
        self.prev_start = Some(start);
        let loads = room_render::load_stats::take();
        let pictures = room_ui::load_stats::take();
        let loads = (loads.0 + pictures.0, loads.1 + pictures.1);
        let median = self.median();
        self.push(start, interval);
        let after_load = std::mem::take(&mut self.after_load);
        if let Some(report) = self.report.as_mut() {
            report.add(now, interval, &self.phases, loads.0);
        }
        if let Some(line) = hitch_line(
            interval,
            work,
            median,
            &self.phases,
            self.tics,
            loads,
            after_load,
        ) {
            self.log(now, &line);
        }
    }

    fn push(&mut self, start: Instant, ms: f32) {
        if self.history.len() == HISTORY {
            self.history.pop_front();
            self.starts.pop_front();
        }
        self.history.push_back(ms);
        self.starts.push_back(start);
    }

    fn log(&mut self, now: Instant, line: &str) {
        if now.duration_since(self.log_window) >= Duration::from_secs(1) {
            if self.suppressed > 0 {
                eprintln!(
                    "room: hitch: {} more slow frame(s) not logged",
                    self.suppressed
                );
            }
            self.log_window = now;
            self.lines_in_window = 0;
            self.suppressed = 0;
        }
        if self.lines_in_window < HITCH_LINES_PER_SEC {
            self.lines_in_window += 1;
            self.hitches += 1;
            eprintln!("{line}");
        } else {
            self.suppressed += 1;
        }
    }

    /// Median frame interval of the history (ms), `None` until there are
    /// enough frames to compare with.
    fn median(&self) -> Option<f32> {
        if self.history.len() < 30 {
            return None;
        }
        let mut v: Vec<f32> = self.history.iter().copied().collect();
        v.sort_by(f32::total_cmp);
        Some(v[v.len() / 2])
    }

    /// Frame intervals (ms), oldest first.
    pub(crate) fn history(&self) -> impl ExactSizeIterator<Item = f32> + '_ {
        self.history.iter().copied()
    }

    /// FPS, average and worst frame for the display.
    pub(crate) fn summary(&self) -> Summary {
        let Some(&last) = self.starts.back() else {
            return Summary::default();
        };
        let second = Duration::from_secs(1);
        let mut fps = 0u32;
        let mut sum = 0.0f32;
        for (t, ms) in self.starts.iter().zip(self.history.iter()).rev() {
            if last.duration_since(*t) >= second {
                break;
            }
            fps += 1;
            sum += ms;
        }
        Summary {
            fps,
            avg_ms: if fps > 0 { sum / fps as f32 } else { 0.0 },
            worst_ms: self.history.iter().copied().fold(0.0, f32::max),
        }
    }
}

/// The log line for a frame, or `None` when it was not a hitch.
fn hitch_line(
    interval: f32,
    work: f32,
    median: Option<f32>,
    phases: &Phases,
    tics: u32,
    loads: (u64, Duration),
    after_load: bool,
) -> Option<String> {
    let slow = interval >= HITCH_MIN_MS && median.is_some_and(|m| interval >= m * HITCH_FACTOR)
        || after_load && interval >= HITCH_MIN_MS;
    if !slow {
        return None;
    }
    let mut line = if after_load {
        format!("room: first frame after loading {interval:.0} ms (work {work:.0} ms):")
    } else {
        format!("room: hitch {interval:.0} ms (work {work:.0} ms):")
    };
    let mut parts: Vec<String> = Vec::new();
    for &(name, ms) in phases.entries() {
        if ms < 0.5 {
            continue;
        }
        if name == "sim" && tics > 0 {
            parts.push(format!("{name} {ms:.1} ({tics} tics)"));
        } else {
            parts.push(format!("{name} {ms:.1}"));
        }
    }
    if parts.is_empty() {
        line.push_str(" waiting (GPU / screen)");
    } else {
        line.push(' ');
        line.push_str(&parts.join(", "));
    }
    if work < interval * 0.5 {
        line.push_str(" | mostly waiting on the GPU or the OS");
    }
    if loads.0 > 0 {
        line.push_str(&format!(
            " | {} load(s) {:.1} ms",
            loads.0,
            loads.1.as_secs_f32() * 1000.0
        ));
    }
    Some(line)
}

/// Colour of a graph bar for a frame of `ms` when the target is `target_ms`.
pub(crate) fn bar_color(ms: f32, target_ms: f32) -> [f32; 4] {
    if ms <= target_ms * 1.25 {
        [0.35, 0.85, 0.45, 0.9]
    } else if ms <= target_ms * 2.1 {
        [0.95, 0.78, 0.25, 0.95]
    } else {
        [0.95, 0.30, 0.25, 1.0]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn phases(list: &[(&'static str, f32)]) -> Phases {
        let mut p = Phases::default();
        for &(n, ms) in list {
            p.add(n, ms);
        }
        p
    }

    #[test]
    fn a_slow_frame_names_where_the_time_went() {
        let p = phases(&[("sim", 41.2), ("hud", 12.0), ("sim", 0.4), ("draw", 0.2)]);
        let line = hitch_line(
            84.0,
            70.0,
            Some(16.7),
            &p,
            3,
            (12, Duration::from_millis(30)),
            false,
        )
        .expect("hitch");
        assert_eq!(
            line,
            "room: hitch 84 ms (work 70 ms): sim 41.6 (3 tics), hud 12.0 | 12 load(s) 30.0 ms"
        );
    }

    #[test]
    fn ordinary_and_steady_slow_frames_are_not_hitches() {
        let p = phases(&[("sim", 10.0)]);
        // Fast frame.
        assert!(hitch_line(20.0, 18.0, Some(16.7), &p, 1, (0, Duration::ZERO), false).is_none());
        // A phone steadily at 20 fps: 50 ms frames on a 50 ms median.
        assert!(hitch_line(52.0, 50.0, Some(50.0), &p, 2, (0, Duration::ZERO), false).is_none());
        // Not enough history yet.
        assert!(hitch_line(90.0, 90.0, None, &p, 2, (0, Duration::ZERO), false).is_none());
    }

    #[test]
    fn the_first_frame_after_a_load_is_named_as_such() {
        let p = phases(&[("draw", 60.0)]);
        let line = hitch_line(200.0, 60.0, None, &p, 0, (0, Duration::ZERO), true).expect("line");
        assert!(
            line.starts_with("room: first frame after loading 200 ms"),
            "{line}"
        );
        assert!(
            line.ends_with("mostly waiting on the GPU or the OS"),
            "{line}"
        );
    }

    #[test]
    fn summary_counts_the_last_second() {
        let mut t = FrameTimer::default();
        let t0 = Instant::now();
        for i in 0..90u32 {
            // 90 frames of 16 ms, then one 100 ms frame.
            t.push(t0 + Duration::from_millis(u64::from(i) * 16), 16.0);
        }
        t.push(t0 + Duration::from_millis(90 * 16 + 100), 100.0);
        let s = t.summary();
        // Frames whose start is within 1 s of the last one.
        assert_eq!(s.fps, 57);
        assert!((s.worst_ms - 100.0).abs() < 1e-3);
        assert!(s.avg_ms > 16.0 && s.avg_ms < 18.0, "{s:?}");
    }

    #[test]
    fn bars_turn_yellow_then_red() {
        assert_eq!(bar_color(16.0, 16.7)[0], 0.35);
        assert_eq!(bar_color(30.0, 16.7)[0], 0.95);
        assert_eq!(bar_color(30.0, 16.7)[2], 0.25);
        assert_eq!(bar_color(60.0, 16.7)[1], 0.30);
    }
}
