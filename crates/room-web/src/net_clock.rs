//! Netplay clock for the web client (protocol v2): when to send input and
//! which tic to show, both paced by an estimate of the server's 35 Hz clock
//! instead of by when packets happen to arrive (`docs/38` §4).
//!
//! The server releases tic `T` at `start + T/35 s` (or as soon as every seat
//! sent it). Each received frame gives a sample `arrival - T * TIC_MS`; the
//! minimum of those samples (`origin`) is "server tic 0 + fastest one-way
//! trip" on the local clock. From it:
//!
//! * **Input horizon:** a command for tic `K` must reach the server before the
//!   server's clock gets to `K`, so the client sends every tic below
//!   `(now - origin + rtt + margin) / TIC_MS`. Sending is steady (one command
//!   per 28.6 ms of local time) instead of in clumps when frames arrive, so the
//!   predicted view moves at a constant rate. The margin grows when the server
//!   had to stand in for one of our commands (it arrived late) and shrinks
//!   slowly while everything is on time.
//! * **Playout:** the client shows tic `(now - origin - buffer) / TIC_MS`: a
//!   small jitter buffer that follows how late frames arrive, so the sim runs
//!   at an even 35 Hz instead of stalling on a late frame and then running two
//!   tics in one frame. The buffer grows after a stall and shrinks slowly.
//!
//! Pure arithmetic on millisecond timestamps — no browser APIs — so it is
//! tested natively, including a network simulation (see the tests).

/// One tic in ms (35 Hz).
pub const TIC_MS: f64 = 1000.0 / 35.0;

/// Playout buffer limits (ms).
const BUFFER_MIN_MS: f64 = 4.0;
const BUFFER_MAX_MS: f64 = 160.0;
/// Input margin limits (ms) on top of the round trip.
const MARGIN_MIN_MS: f64 = 8.0;
const MARGIN_MAX_MS: f64 = 200.0;
/// How much one late command / one playout stall widens the margin / buffer.
const LATE_BUMP_MS: f64 = 12.0;
const STALL_BUMP_MS: f64 = 10.0;
/// Late inputs / stalls closer together than this widen things only once.
const BUMP_WINDOW_MS: f64 = 250.0;
/// Decay of those bumps while things are on time (ms per second).
const BUMP_DECAY_PER_S: f64 = 3.0;
/// The origin follows a later path only slowly (fraction per sample), so one
/// slow frame does not shift the clock but a lasting route change does.
const ORIGIN_RISE: f64 = 0.002;
/// Peak-jitter memory: multiplier per frame sample (~4 s half-life at 35 Hz).
const JITTER_DECAY: f64 = 0.995;

/// Server-clock estimate plus the adaptive input margin and playout buffer.
#[derive(Clone, Debug, Default)]
pub struct NetClock {
    /// Local ms of server tic 0 plus the fastest one-way trip.
    origin: Option<f64>,
    /// Decaying peak of `sample - origin` (ms): how late frames can be.
    jitter_peak: f64,
    /// Smoothed round trip (ms).
    rtt: Option<f64>,
    /// Extra input margin after late commands (ms).
    late_bump: f64,
    /// Extra playout buffer after stalls (ms).
    stall_bump: f64,
    /// Last `decay` call (local ms).
    last_decay: Option<f64>,
    /// Last time a late input / a stall widened the margin / buffer: a burst
    /// (tab back from the background) counts once per window.
    last_late_bump: Option<f64>,
    last_stall_bump: Option<f64>,
    /// Counters for the debug overlay.
    pub late_inputs: u32,
    /// Times the playout ran out of frames.
    pub stalls: u32,
}

impl NetClock {
    /// Fresh clock (no estimate until the first frame).
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// A frame for `tic` arrived at local time `now` (ms).
    pub fn on_frame(&mut self, tic: u32, now: f64) {
        let sample = now - f64::from(tic) * TIC_MS;
        let origin = match self.origin {
            None => sample,
            Some(o) if sample < o => sample,
            Some(o) => o + (sample - o) * ORIGIN_RISE,
        };
        self.origin = Some(origin);
        let late = (sample - origin).max(0.0);
        self.jitter_peak = (self.jitter_peak * JITTER_DECAY).max(late);
    }

    /// Smoothed round-trip time from PING / PONG (ms).
    pub fn set_rtt(&mut self, rtt_ms: f64) {
        if rtt_ms.is_finite() && rtt_ms >= 0.0 {
            self.rtt = Some(rtt_ms);
        }
    }

    /// The server relayed a different command than ours for one of our tics:
    /// ours arrived after the server's grace. Send further ahead.
    pub fn on_late_input(&mut self, now: f64) {
        self.late_inputs = self.late_inputs.saturating_add(1);
        if self
            .last_late_bump
            .is_some_and(|t| now - t < BUMP_WINDOW_MS)
        {
            return;
        }
        self.last_late_bump = Some(now);
        self.late_bump = (self.late_bump + LATE_BUMP_MS).min(MARGIN_MAX_MS);
    }

    /// The playout wanted a tic that had not arrived yet. Buffer more.
    pub fn on_stall(&mut self, now: f64) {
        self.stalls = self.stalls.saturating_add(1);
        if self
            .last_stall_bump
            .is_some_and(|t| now - t < BUMP_WINDOW_MS)
        {
            return;
        }
        self.last_stall_bump = Some(now);
        self.stall_bump = (self.stall_bump + STALL_BUMP_MS).min(BUFFER_MAX_MS);
    }

    /// Shrink the bumps over time; call once per frame.
    pub fn decay(&mut self, now: f64) {
        if let Some(last) = self.last_decay {
            let secs = ((now - last) / 1000.0).clamp(0.0, 1.0);
            let d = BUMP_DECAY_PER_S * secs;
            self.late_bump = (self.late_bump - d).max(0.0);
            self.stall_bump = (self.stall_bump - d).max(0.0);
        }
        self.last_decay = Some(now);
    }

    /// The clock has seen a frame.
    #[must_use]
    pub fn ready(&self) -> bool {
        self.origin.is_some()
    }

    /// Margin (ms) added to the round trip when sending input.
    #[must_use]
    pub fn input_margin_ms(&self) -> f64 {
        (MARGIN_MIN_MS + self.jitter_peak * 0.25 + self.late_bump)
            .clamp(MARGIN_MIN_MS, MARGIN_MAX_MS)
    }

    /// Playout buffer (ms) behind the newest expected frame.
    #[must_use]
    pub fn buffer_ms(&self) -> f64 {
        (BUFFER_MIN_MS + self.jitter_peak + self.stall_bump).clamp(BUFFER_MIN_MS, BUFFER_MAX_MS)
    }

    /// Fractional tic up to which local input should already be sent: send
    /// every tic `< floor(horizon) + 1`. `None` before the first frame.
    #[must_use]
    pub fn input_horizon(&self, now: f64) -> Option<f64> {
        let origin = self.origin?;
        let rtt = self.rtt.unwrap_or(100.0);
        Some(((now - origin + rtt + self.input_margin_ms()) / TIC_MS).max(0.0))
    }

    /// Fractional tic to display: simulate every tic `<= floor(pos)`, then
    /// interpolate by `pos - floor(pos)`. `None` before the first frame.
    #[must_use]
    pub fn playout_pos(&self, now: f64) -> Option<f64> {
        let origin = self.origin?;
        Some(((now - origin - self.buffer_ms()) / TIC_MS).max(0.0))
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]
mod tests {
    use super::*;

    #[test]
    fn origin_tracks_the_fastest_frame() {
        let mut c = NetClock::new();
        c.on_frame(0, 1000.0 + 40.0);
        c.on_frame(1, 1000.0 + TIC_MS + 25.0);
        c.on_frame(2, 1000.0 + 2.0 * TIC_MS + 60.0);
        let pos = c.playout_pos(1000.0 + 25.0 + 10.0 * TIC_MS).unwrap();
        // Origin = 1025; buffer = 4 + jitter peak (35) → about 1.3 tics behind.
        assert!((pos - (10.0 - 39.0 / TIC_MS)).abs() < 0.05, "{pos}");
    }

    #[test]
    fn late_inputs_and_stalls_widen_then_decay() {
        let mut c = NetClock::new();
        c.on_frame(0, 0.0);
        let m0 = c.input_margin_ms();
        let b0 = c.buffer_ms();
        c.on_late_input(0.0);
        c.on_stall(0.0);
        assert!(c.input_margin_ms() > m0 && c.buffer_ms() > b0);
        for i in 0..=20 {
            c.decay(f64::from(i) * 1000.0);
        }
        assert!((c.input_margin_ms() - m0).abs() < 1e-9);
        assert!((c.buffer_ms() - b0).abs() < 1e-9);
    }

    // ---- network simulation: old pacing vs NetClock ---------------------

    /// Tiny deterministic RNG.
    struct Lcg(u64);
    impl Lcg {
        fn next(&mut self) -> f64 {
            self.0 = self
                .0
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            ((self.0 >> 11) as f64) / ((1u64 << 53) as f64)
        }
        /// One-way delay: base + exponential jitter + rare spikes.
        fn delay(&mut self, base: f64, jitter: f64) -> f64 {
            let mut d = base - jitter * (1.0 - self.next()).ln();
            if self.next() < 0.01 {
                d += 60.0 + 60.0 * self.next();
            }
            d
        }
    }

    #[derive(Debug, Default)]
    struct Report {
        /// Our commands that reached the server after the 30 ms grace.
        late: u32,
        /// Frames rendered with no new sim progress while the clock moved on.
        stalls: u32,
        /// RMS deviation of the per-frame displayed-time step from the ideal
        /// (tics): how jerky the world looks.
        jerk: f64,
        /// RMS deviation of the predicted local position step (tics of input).
        input_jerk: f64,
        /// Mean input-to-server lead (ms) — input latency the margin costs.
        mean_lead_ms: f64,
    }

    /// One client, 60 Hz rendering with frame-time noise, for `secs`.
    fn simulate(new: bool, base: f64, jitter: f64, secs: f64, seed: u64) -> Report {
        const GRACE: f64 = 30.0;
        let mut rng = Lcg(seed);
        let mut clock = NetClock::new();
        // Frames in flight: (arrival, tic).
        let mut arrivals: Vec<(f64, u32)> = Vec::new();
        let mut received: u32 = 0; // frames received contiguously
        let mut got = std::collections::BTreeSet::new();
        let mut next_tic: u32 = 0; // next tic to simulate
        let mut next_send: u32 = 0;
        let mut tic_accum = 0.0f64;
        let mut rtt = 2.0 * (base + jitter);
        clock.set_rtt(rtt);
        let mut report = Report::default();
        let mut steps = Vec::new();
        let mut in_steps = Vec::new();
        let mut last_disp: Option<f64> = None;
        let mut last_in: Option<f64> = None;
        let mut leads = Vec::new();
        let mut now = 0.0f64;
        // Server releases tic T at T*TIC (+ grace if our cmd is late).
        let mut cmd_arrival = std::collections::BTreeMap::new();
        let mut released: u32 = 0;
        let end = secs * 1000.0;
        while now < end {
            let dt = 16.67 * (0.85 + 0.3 * rng.next());
            now += dt;
            // Server: release every due tic.
            while f64::from(released) * TIC_MS <= now {
                let due = f64::from(released) * TIC_MS;
                let ours: Option<f64> = cmd_arrival.get(&released).copied();
                let at = match ours {
                    Some(a) if a <= due => due,
                    Some(a) if a <= due + GRACE => a,
                    _ => due + GRACE,
                };
                if at > now {
                    break;
                }
                if ours.is_none_or(|a| a > due + GRACE) {
                    report.late += 1;
                    if new {
                        // Client learns it when the frame is simulated; close enough.
                        clock.on_late_input(now);
                    }
                } else {
                    leads.push(due - (ours.unwrap() - base));
                }
                arrivals.push((at + rng.delay(base, jitter), released));
                released += 1;
            }
            // Client receives.
            arrivals.retain(|&(a, t)| {
                if a <= now {
                    got.insert(t);
                    if new {
                        clock.on_frame(t, a);
                    }
                    false
                } else {
                    true
                }
            });
            while got.contains(&received) {
                received += 1;
            }
            rtt = rtt * 0.9 + 0.1 * 2.0 * (base + jitter);
            clock.set_rtt(rtt);
            clock.decay(now);
            // Client: send input.
            let target = if new {
                clock
                    .input_horizon(now)
                    .map_or(received + 3, |h| h.floor() as u32 + 1)
            } else {
                let lead = ((rtt + 15.0) / TIC_MS).ceil().clamp(1.0, 10.0) as u32;
                received + lead
            };
            let mut sent = 0;
            while next_send < target && sent < 4 {
                cmd_arrival.insert(next_send, now + rng.delay(base, jitter));
                next_send += 1;
                sent += 1;
            }
            // Client: simulate.
            let before = next_tic;
            let disp;
            if new && clock.ready() {
                let pos = clock.playout_pos(now).unwrap();
                let want = pos.floor() as u32;
                while next_tic <= want && next_tic < received {
                    next_tic += 1;
                }
                if next_tic <= want {
                    clock.on_stall(now);
                }
                let frac = if next_tic == want + 1 {
                    pos - pos.floor()
                } else {
                    1.0
                };
                disp = f64::from(next_tic) - 1.0 + frac;
            } else {
                tic_accum = (tic_accum + dt).min(TIC_MS * 3.0);
                let backlog = received.saturating_sub(next_tic);
                if backlog > 2 {
                    tic_accum += TIC_MS * f64::from((backlog - 2).min(3));
                }
                while tic_accum >= TIC_MS && next_tic < received {
                    tic_accum -= TIC_MS;
                    next_tic += 1;
                }
                disp = f64::from(next_tic) - 1.0 + (tic_accum / TIC_MS).clamp(0.0, 1.0);
            }
            if now > 2000.0 {
                if next_tic == before && received > next_tic {
                    // had frames but did not advance: not a stall
                } else if next_tic == before && f64::from(released) > f64::from(next_tic) + 1.0 {
                    report.stalls += 1;
                }
                if let Some(l) = last_disp {
                    steps.push(disp - l);
                }
                // Predicted local position ~ newest input tic (+ input fraction).
                let inp = if new {
                    clock.input_horizon(now).unwrap_or(0.0)
                } else {
                    f64::from(next_send)
                };
                if let Some(l) = last_in {
                    in_steps.push(inp - l);
                }
                last_in = Some(inp);
            }
            last_disp = Some(disp);
        }
        let rms = |v: &[f64]| {
            let mean = v.iter().sum::<f64>() / v.len() as f64;
            (v.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / v.len() as f64).sqrt()
        };
        report.jerk = rms(&steps);
        report.input_jerk = rms(&in_steps);
        report.mean_lead_ms = leads.iter().sum::<f64>() / leads.len().max(1) as f64;
        report
    }

    #[test]
    fn netclock_beats_arrival_pacing_on_a_jittery_link() {
        for (base, jitter) in [(15.0, 3.0), (40.0, 12.0), (70.0, 25.0)] {
            let old = simulate(false, base, jitter, 120.0, 7);
            let new = simulate(true, base, jitter, 120.0, 7);
            eprintln!("link {base}±{jitter} ms\n  old {old:?}\n  new {new:?}");
            assert!(new.jerk < old.jerk, "world smoother: {new:?} vs {old:?}");
            assert!(
                new.input_jerk < old.input_jerk,
                "input steadier: {new:?} vs {old:?}"
            );
            assert!(
                new.late <= old.late,
                "no more late commands: {new:?} vs {old:?}"
            );
        }
    }
}
