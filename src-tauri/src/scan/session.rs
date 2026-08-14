//! Scan lifecycle: one thread pool, one live scan, monotonic generations.
//!
//! Generations exist because cancellation is not instantaneous. A scan that has
//! been told to stop may still emit a few updates while its workers wind down,
//! and those must not be mistaken for progress on the scan that replaced it. Each
//! message carries the generation it belongs to, and the frontend drops anything
//! that is not current.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use crate::model::error::{AppError, AppResult};

pub struct Scanner {
    /// A pool of our own rather than rayon's global one. Walking is blocking IO;
    /// running it on a pool shared with anything else risks starving whichever
    /// side is unlucky.
    pool: Arc<rayon::ThreadPool>,
    generation: AtomicU64,
    live: Mutex<Option<Arc<AtomicBool>>>,
}

#[derive(Clone)]
pub struct ScanHandle {
    pub generation: u64,
    pub cancel: Arc<AtomicBool>,
    pub pool: Arc<rayon::ThreadPool>,
}

impl Scanner {
    pub fn new() -> AppResult<Self> {
        let pool = rayon::ThreadPoolBuilder::new()
            .thread_name(|i| format!("scan-{i}"))
            .build()
            .map_err(|e| AppError::Scan(e.to_string()))?;
        Ok(Self {
            pool: Arc::new(pool),
            generation: AtomicU64::new(0),
            live: Mutex::new(None),
        })
    }

    /// Cancels any scan still running and returns a handle for the new one.
    pub fn begin(&self) -> ScanHandle {
        let cancel = Arc::new(AtomicBool::new(false));
        {
            let mut live = self.live.lock().expect("scanner lock");
            if let Some(previous) = live.replace(Arc::clone(&cancel)) {
                previous.store(true, Ordering::Relaxed);
            }
        }
        ScanHandle {
            generation: self.generation.fetch_add(1, Ordering::Relaxed) + 1,
            cancel,
            pool: Arc::clone(&self.pool),
        }
    }

    pub fn cancel(&self) {
        if let Some(live) = self.live.lock().expect("scanner lock").take() {
            live.store(true, Ordering::Relaxed);
        }
    }

    pub fn current_generation(&self) -> u64 {
        self.generation.load(Ordering::Relaxed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn beginning_a_scan_cancels_the_previous_one() {
        let scanner = Scanner::new().unwrap();
        let first = scanner.begin();
        assert!(!first.cancel.load(Ordering::Relaxed));

        let second = scanner.begin();
        assert!(first.cancel.load(Ordering::Relaxed));
        assert!(!second.cancel.load(Ordering::Relaxed));
        assert!(second.generation > first.generation);
    }

    #[test]
    fn generations_never_repeat() {
        let scanner = Scanner::new().unwrap();
        let seen: Vec<u64> = (0..5).map(|_| scanner.begin().generation).collect();
        let mut sorted = seen.clone();
        sorted.dedup();
        assert_eq!(seen, sorted);
        assert_eq!(scanner.current_generation(), 5);
    }

    #[test]
    fn cancel_without_a_live_scan_is_harmless() {
        let scanner = Scanner::new().unwrap();
        scanner.cancel();
        let handle = scanner.begin();
        scanner.cancel();
        assert!(handle.cancel.load(Ordering::Relaxed));
    }
}
