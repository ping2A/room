//! Connection rate / size limits (docs/22 §5).

use std::time::Instant;

/// Token-bucket rate limiter (messages or bytes).
#[derive(Debug)]
pub struct TokenBucket {
    tokens: f64,
    last: Instant,
    burst: f64,
    refill_per_sec: f64,
}

impl TokenBucket {
    /// New bucket with `burst` capacity and `refill_per_sec` refill rate.
    #[must_use]
    pub fn new(burst: f64, refill_per_sec: f64) -> Self {
        Self {
            tokens: burst,
            last: Instant::now(),
            burst,
            refill_per_sec,
        }
    }

    /// Try to consume one token. Returns `false` when empty.
    pub fn try_take(&mut self) -> bool {
        self.try_take_n(1.0)
    }

    /// Try to consume `n` tokens (e.g. bytes). Returns `false` when insufficient.
    pub fn try_take_n(&mut self, n: f64) -> bool {
        if n <= 0.0 {
            return true;
        }
        self.refill();
        if self.tokens >= n {
            self.tokens -= n;
            true
        } else {
            false
        }
    }

    fn refill(&mut self) {
        let now = Instant::now();
        let dt = now.duration_since(self.last).as_secs_f64();
        self.last = now;
        self.tokens = (self.tokens + dt * self.refill_per_sec).min(self.burst);
    }
}

/// Per-connection violation counter with 60s window.
#[derive(Debug, Default)]
pub struct ViolationWindow {
    count: u32,
    window_start: Option<Instant>,
}

impl ViolationWindow {
    /// Record a violation; returns `true` if the peer should be kicked (≥50/min).
    pub fn hit(&mut self) -> bool {
        let now = Instant::now();
        match self.window_start {
            Some(start) if now.duration_since(start).as_secs() < 60 => {
                self.count = self.count.saturating_add(1);
            }
            _ => {
                self.window_start = Some(now);
                self.count = 1;
            }
        }
        self.count >= 50
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn bucket_exhausts() {
        let mut b = TokenBucket::new(3.0, 0.0);
        assert!(b.try_take());
        assert!(b.try_take());
        assert!(b.try_take());
        assert!(!b.try_take());
    }

    #[test]
    fn byte_bucket_takes_n() {
        let mut b = TokenBucket::new(100.0, 0.0);
        assert!(b.try_take_n(60.0));
        assert!(b.try_take_n(40.0));
        assert!(!b.try_take_n(1.0));
    }
}
