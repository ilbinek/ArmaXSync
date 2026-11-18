//! Shared byte-rate limiter (token bucket).

use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Token bucket shared by all connections of a sync (or a whole process).
#[derive(Debug)]
pub struct RateLimiter {
    state: Mutex<State>,
}

#[derive(Debug)]
struct State {
    rate: f64,
    capacity: f64,
    tokens: f64,
    last: Instant,
}

impl RateLimiter {
    /// `bytes_per_sec == 0` means unlimited.
    pub fn new(bytes_per_sec: u64) -> Self {
        let rate = bytes_per_sec as f64;
        let capacity = (rate / 4.0).max(64.0 * 1024.0);
        Self {
            state: Mutex::new(State {
                rate,
                capacity,
                tokens: capacity,
                last: Instant::now(),
            }),
        }
    }

    pub fn set_rate(&self, bytes_per_sec: u64) {
        let mut s = self.state.lock().unwrap();
        s.rate = bytes_per_sec as f64;
        s.capacity = (s.rate / 4.0).max(64.0 * 1024.0);
        s.tokens = s.tokens.min(s.capacity);
    }

    pub fn rate(&self) -> u64 {
        self.state.lock().unwrap().rate as u64
    }

    /// Account for `n` bytes, sleeping as needed to respect the rate.
    pub async fn consume(&self, n: u64) {
        let wait = {
            let mut s = self.state.lock().unwrap();
            if s.rate <= 0.0 {
                return;
            }
            let now = Instant::now();
            let elapsed = now.duration_since(s.last).as_secs_f64();
            s.last = now;
            s.tokens = (s.tokens + elapsed * s.rate).min(s.capacity);
            s.tokens -= n as f64;
            if s.tokens >= 0.0 {
                return;
            }
            Duration::from_secs_f64(-s.tokens / s.rate)
        };
        tokio::time::sleep(wait).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn limits_rate() {
        let l = RateLimiter::new(1_000_000);
        let t = Instant::now();
        for _ in 0..30 {
            l.consume(100_000).await;
        }
        // 3 MB at 1 MB/s with a 256 KB burst: ~2.75 s.
        let s = t.elapsed().as_secs_f64();
        assert!(s > 2.4 && s < 3.4, "{s}");
    }
}
