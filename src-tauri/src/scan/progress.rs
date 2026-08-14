//! Progress rate limiting.
//!
//! `measure` offers an update after every file. A cache directory holds hundreds
//! of thousands of them, so forwarding each one would put more load on the IPC
//! channel than on the filesystem, and the UI cannot render at that rate anyway.

use std::time::{Duration, Instant};

/// Minimum wall-clock gap between updates. Roughly 12/s: fast enough that the
/// numbers look live, slow enough to stay well inside one animation frame.
const MIN_INTERVAL: Duration = Duration::from_millis(80);

/// Emit early when this much has accumulated, regardless of the interval.
/// Without it, a scan that finds one 40 GB directory looks frozen between ticks.
const MIN_BYTES: u64 = 256 * 1024 * 1024;

#[derive(Debug)]
pub struct Throttle {
    min_interval: Duration,
    min_bytes: u64,
    last_at: Option<Instant>,
    last_bytes: u64,
}

impl Default for Throttle {
    fn default() -> Self {
        Self::new(MIN_INTERVAL, MIN_BYTES)
    }
}

impl Throttle {
    pub fn new(min_interval: Duration, min_bytes: u64) -> Self {
        Self {
            min_interval,
            min_bytes,
            last_at: None,
            last_bytes: 0,
        }
    }

    /// Whether an update carrying `bytes` should be sent now. Records the emission
    /// when it returns true, so callers must not discard the answer.
    pub fn admit(&mut self, bytes: u64) -> bool {
        let grown = bytes.saturating_sub(self.last_bytes) >= self.min_bytes;
        let due = self
            .last_at
            .is_none_or(|at| at.elapsed() >= self.min_interval);
        if grown || due {
            self.last_at = Some(Instant::now());
            self.last_bytes = bytes;
            return true;
        }
        false
    }

    /// Marks the next [`admit`] as due. Used for the final update of a scan, which
    /// carries the total and must never be dropped.
    ///
    /// [`admit`]: Self::admit
    pub fn reset(&mut self) {
        self.last_at = None;
        self.last_bytes = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_update_is_always_admitted() {
        let mut t = Throttle::default();
        assert!(t.admit(0));
    }

    #[test]
    fn a_large_jump_bypasses_the_interval() {
        // An interval long enough that only the byte threshold can admit.
        let mut t = Throttle::new(Duration::from_secs(3600), 1024);
        assert!(t.admit(0));
        assert!(!t.admit(1023));
        assert!(t.admit(1024));
        // The threshold is measured from the last emission, not from zero.
        assert!(!t.admit(2047));
        assert!(t.admit(2048));
    }

    #[test]
    fn small_updates_are_dropped_until_the_interval_elapses() {
        let mut t = Throttle::new(Duration::from_millis(50), u64::MAX);
        assert!(t.admit(0));
        assert!(!t.admit(1));
        std::thread::sleep(Duration::from_millis(60));
        assert!(t.admit(2));
    }

    #[test]
    fn reset_admits_the_final_update() {
        let mut t = Throttle::new(Duration::from_secs(3600), u64::MAX);
        assert!(t.admit(0));
        assert!(!t.admit(1));
        t.reset();
        assert!(t.admit(1));
    }
}
