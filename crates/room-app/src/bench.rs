//! Frame-time and memory measurement for `--bench-frames` (docs/41 M6): the
//! same numbers on every platform, so phones and desktops can be compared.

/// Frame times collected during a run.
#[derive(Clone, Debug, Default)]
pub(crate) struct FrameStats {
    ms: Vec<f64>,
}

/// Summary of a run.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct BenchReport {
    pub frames: usize,
    pub avg_ms: f64,
    pub p50_ms: f64,
    pub p95_ms: f64,
    pub p99_ms: f64,
    pub max_ms: f64,
    /// Average frames per second (1000 / `avg_ms`).
    pub fps: f64,
}

impl FrameStats {
    /// Record one frame's wall time.
    pub(crate) fn push(&mut self, ms: f64) {
        if ms.is_finite() && ms >= 0.0 {
            self.ms.push(ms);
        }
    }

    pub(crate) fn len(&self) -> usize {
        self.ms.len()
    }

    /// Percentiles by nearest rank; `None` with no frames.
    pub(crate) fn report(&self) -> Option<BenchReport> {
        if self.ms.is_empty() {
            return None;
        }
        let mut sorted = self.ms.clone();
        sorted.sort_by(f64::total_cmp);
        let n = sorted.len();
        let rank = |p: f64| sorted[((p * n as f64).ceil() as usize).clamp(1, n) - 1];
        let avg = sorted.iter().sum::<f64>() / n as f64;
        Some(BenchReport {
            frames: n,
            avg_ms: avg,
            p50_ms: rank(0.50),
            p95_ms: rank(0.95),
            p99_ms: rank(0.99),
            max_ms: sorted[n - 1],
            fps: if avg > 0.0 { 1000.0 / avg } else { 0.0 },
        })
    }
}

impl std::fmt::Display for BenchReport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} frames, {:.1} fps, frame ms avg {:.2} p50 {:.2} p95 {:.2} p99 {:.2} max {:.2}",
            self.frames, self.fps, self.avg_ms, self.p50_ms, self.p95_ms, self.p99_ms, self.max_ms
        )
    }
}

/// Peak resident memory of this process in bytes, where the OS reports it
/// without native calls (Linux and Android: `VmHWM` in `/proc/self/status`).
pub(crate) fn peak_rss_bytes() -> Option<u64> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    parse_vm_hwm(&status)
}

/// With `--bench-frames` (or `ROOM_BENCH_MEM=1`), print resident memory
/// after a start-up stage, to see which stage costs what.
pub(crate) fn log_stage(enabled: bool, stage: &str) {
    crate::startlog::step(format_args!("done: {stage}"));
    if !enabled && std::env::var_os("ROOM_BENCH_MEM").is_none() {
        return;
    }
    if let Some(m) = resident() {
        eprintln!("room: bench: {m} after {stage}");
    }
}

/// One line for the launcher after a speed test: frames per second, the
/// slowest 1 % of frames and peak memory.
#[must_use]
pub(crate) fn short_summary(report: &BenchReport, peak_bytes: Option<u64>) -> String {
    let mut text = format!(
        "Speed test: {:.1} fps, slowest 1% {:.0} ms",
        report.fps, report.p99_ms
    );
    if let Some(b) = peak_bytes {
        text.push_str(&format!(", {:.0} MB", b as f64 / 1_048_576.0));
    }
    text
}

/// Resident memory now, split into private (`RssAnon`: the app's own data,
/// what phones count against an app) and file-backed (`RssFile`: mapped
/// files and code, which the OS can drop and re-read).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Resident {
    pub total: u64,
    pub private: u64,
    pub file: u64,
}

impl std::fmt::Display for Resident {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mb = |b: u64| b as f64 / 1_048_576.0;
        write!(
            f,
            "{:.1} MB resident ({:.1} MB private, {:.1} MB file-backed)",
            mb(self.total),
            mb(self.private),
            mb(self.file)
        )
    }
}

/// Current [`Resident`] memory (Linux and Android).
pub(crate) fn resident() -> Option<Resident> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    parse_resident(&status)
}

fn parse_resident(status: &str) -> Option<Resident> {
    Some(Resident {
        total: parse_kb(status, "VmRSS:")?,
        private: parse_kb(status, "RssAnon:").unwrap_or(0),
        file: parse_kb(status, "RssFile:").unwrap_or(0),
    })
}

fn parse_vm_hwm(status: &str) -> Option<u64> {
    parse_kb(status, "VmHWM:")
}

fn parse_kb(status: &str, key: &str) -> Option<u64> {
    let line = status.lines().find(|l| l.starts_with(key))?;
    let kb: u64 = line
        .trim_start_matches(key)
        .trim()
        .trim_end_matches("kB")
        .trim()
        .parse()
        .ok()?;
    Some(kb * 1024)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    #[test]
    fn report_percentiles_by_nearest_rank() {
        let mut s = FrameStats::default();
        assert!(s.report().is_none());
        for ms in 1..=100 {
            s.push(f64::from(ms));
        }
        s.push(f64::NAN);
        s.push(-1.0);
        let r = s.report().unwrap();
        assert_eq!(r.frames, 100);
        assert_eq!(
            (r.p50_ms, r.p95_ms, r.p99_ms, r.max_ms),
            (50.0, 95.0, 99.0, 100.0)
        );
        assert!((r.avg_ms - 50.5).abs() < 1e-9);
        assert!((r.fps - 1000.0 / 50.5).abs() < 1e-9);
        assert!(r.to_string().starts_with("100 frames, 19.8 fps"));
    }

    #[test]
    fn the_short_summary_fits_the_launcher() {
        let mut s = FrameStats::default();
        for _ in 0..100 {
            s.push(20.0);
        }
        let r = s.report().unwrap();
        assert_eq!(
            short_summary(&r, Some(650 * 1_048_576)),
            "Speed test: 50.0 fps, slowest 1% 20 ms, 650 MB"
        );
        assert_eq!(
            short_summary(&r, None),
            "Speed test: 50.0 fps, slowest 1% 20 ms"
        );
    }

    #[test]
    fn vm_hwm_parses() {
        let status = "Name:\troom\nVmPeak:\t  999 kB\nVmHWM:\t  123456 kB\nVmRSS:\t 1 kB\n";
        assert_eq!(parse_vm_hwm(status), Some(123_456 * 1024));
        assert_eq!(parse_vm_hwm("Name: x\n"), None);
    }

    #[test]
    fn resident_splits_private_and_file_backed() {
        let status = "VmRSS:\t 300 kB\nRssAnon:\t 200 kB\nRssFile:\t 90 kB\nRssShmem:\t 10 kB\n";
        let r = parse_resident(status).unwrap();
        assert_eq!(
            (r.total, r.private, r.file),
            (300 * 1024, 200 * 1024, 90 * 1024)
        );
        assert!(r.to_string().starts_with("0.3 MB resident (0.2 MB private"));
        assert_eq!(parse_resident("Name: x\n"), None);
    }
}
