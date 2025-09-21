// src/lib/clock.rs

// dependencies
use std::time::{SystemTime, UNIX_EPOCH};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

pub trait Clock: Send + Sync {
    fn now(&self) -> f64;
}

// Default implementation using SystemTime
#[derive(Debug, Clone)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> f64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs_f64()
    }
}

// Test clock for deterministic testing
#[derive(Debug, Clone)]
pub struct TestClock {
    time: Arc<AtomicU64>, // Store as nanos for precision
}

impl TestClock {
    pub fn new(initial_time: f64) -> Self {
        Self {
            time: Arc::new(AtomicU64::new((initial_time * 1_000_000_000.0) as u64))
        }
    }
    
    pub fn advance(&self, seconds: f64) {
        let nanos = (seconds * 1_000_000_000.0) as u64;
        self.time.fetch_add(nanos, Ordering::Relaxed);
    }
}

impl Clock for TestClock {
    fn now(&self) -> f64 {
        self.time.load(Ordering::Relaxed) as f64 / 1_000_000_000.0
    }
}