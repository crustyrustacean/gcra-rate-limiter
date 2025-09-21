// lib/rate_limiter.rs

// dependencies
use crate::clock::Clock;
use dashmap::DashMap;
use std::error::Error;
use std::fmt;
use std::hash::Hash;
use std::sync::Arc;

use crate::SystemClock;

// enum type to represent errors related to the rate limiter type
#[derive(Debug)]
pub enum RateLimiterError {
    InvalidRate,  // for rate <= 0
    InvalidBurst, // for burst < 0
}

// implement the Display trait for the RateLimiterError type
impl fmt::Display for RateLimiterError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            RateLimiterError::InvalidRate => write!(f, "Rate must be positive"),
            RateLimiterError::InvalidBurst => write!(f, "Burst must be non-negative"),
        }
    }
}

// implement the Error trait for the RateLimiter type
impl Error for RateLimiterError {}

// struct type to represent a rate limiter
#[derive(Debug)]
pub struct RateLimiter<T, C = SystemClock>
where
    T: Hash + Eq + Clone,
    C: Clock,
{
    rate: f64,
    burst: f64,
    client_state: Arc<DashMap<T, f64>>,
    clock: C,
}

// methods for the RateLimiter struct
impl<T, C> RateLimiter<T, C>
where
    T: Hash + Eq + Clone,
    C: Clock,
{
    // method to create a new rate limiter given a desired rate and burst value
    pub fn new(rate: f64, burst: f64, clock: C) -> Result<Self, RateLimiterError> {
        // rate must be non-negative and not zero
        if rate <= 0.0 {
            return Err(RateLimiterError::InvalidRate);
        }
        // burst parameter must be positive
        if burst < 0.0 {
            return Err(RateLimiterError::InvalidBurst);
        }

        Ok(Self {
            rate,
            burst,
            client_state: Arc::new(DashMap::new()),
            clock,
        })
    }

    // Convenience constructor with default system clock
    pub fn with_system_clock(rate: f64, burst: f64) -> Result<Self, RateLimiterError> 
    where 
        C: Default,
    {
        Self::new(rate, burst, C::default())
    }

    // accessor method to return the rate field
    pub fn rate(&self) -> f64 {
        self.rate
    }

    // accessor method to return the burst field
    pub fn burst(&self) -> f64 {
        self.burst
    }

    // internal method to convert a rate to the "T" value
    fn increment(&self) -> f64 {
        1.0 / self.rate
    }

    // internal method to convert a rate to the "tau" value
    fn tolerance(&self) -> f64 {
        self.burst * self.increment()
    }

    // method that implements the GCRA algorithm
    pub fn is_allowed(&self, client_id: T) -> Result<bool, RateLimiterError> {
        let current_time = self.clock.now();
        
        // Get previous TAT, default to current_time for new clients
        let previous_tat = self
            .client_state
            .get(&client_id)
            .map(|entry| *entry.value())
            .unwrap_or(current_time);

        // Core GCRA conformance test
        let is_conforming = current_time >= previous_tat - self.tolerance();

        // Update TAT if request is allowed
        if is_conforming {
            let new_tat = f64::max(current_time, previous_tat) + self.increment();
            self.client_state.insert(client_id, new_tat);
        }

        Ok(is_conforming)
    }
}

// Make SystemClock the default
impl Default for SystemClock {
    fn default() -> Self {
        Self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    // Test clock implementation
    #[derive(Debug, Clone)]
    struct TestClock {
        time: Arc<AtomicU64>, // Store as nanos for precision
    }

    impl TestClock {
        fn new(initial_time: f64) -> Self {
            Self {
                time: Arc::new(AtomicU64::new((initial_time * 1_000_000_000.0) as u64))
            }
        }
        
        fn advance(&self, seconds: f64) {
            let nanos = (seconds * 1_000_000_000.0) as u64;
            self.time.fetch_add(nanos, Ordering::Relaxed);
        }

        fn set_time(&self, seconds: f64) {
            let nanos = (seconds * 1_000_000_000.0) as u64;
            self.time.store(nanos, Ordering::Relaxed);
        }
    }

    impl Clock for TestClock {
        fn now(&self) -> f64 {
            self.time.load(Ordering::Relaxed) as f64 / 1_000_000_000.0
        }
    }

    #[test]
    fn constructor_rejects_zero_rate() {
        let clock = TestClock::new(0.0);
        let result = RateLimiter::<String, _>::new(0.0, 1.0, clock);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), RateLimiterError::InvalidRate));
    }

    #[test]
    fn constructor_rejects_negative_rate() {
        let clock = TestClock::new(0.0);
        let result = RateLimiter::<String, _>::new(-1.0, 1.0, clock);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), RateLimiterError::InvalidRate));
    }

    #[test]
    fn constructor_rejects_negative_burst() {
        let clock = TestClock::new(0.0);
        let result = RateLimiter::<String, _>::new(1.0, -1.0, clock);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            RateLimiterError::InvalidBurst
        ));
    }

    #[test]
    fn constructor_accepts_valid_parameters() {
        let clock = TestClock::new(0.0);
        let result = RateLimiter::<String, _>::new(10.0, 5.0, clock);
        assert!(result.is_ok());
    }

    #[test]
    fn constructor_accepts_zero_burst() {
        let clock = TestClock::new(0.0);
        let result = RateLimiter::<String, _>::new(1.0, 0.0, clock);
        assert!(result.is_ok());
    }

    #[test]
    fn first_request_always_allowed() {
        let clock = TestClock::new(0.0);
        let limiter = RateLimiter::new(1.0, 1.0, clock).unwrap();
        let result = limiter.is_allowed("client1");
        assert!(result.unwrap());
    }

    #[test]
    fn rate_limiting_blocks_rapid_requests() {
        let clock = TestClock::new(0.0);
        let limiter = RateLimiter::new(1.0, 0.0, clock.clone()).unwrap(); // 1 req/sec, no burst
        let client = "client1";

        // First request at time 0.0 should be allowed
        assert!(limiter.is_allowed(client).unwrap());

        // Second request immediately after should be blocked
        assert!(!limiter.is_allowed(client).unwrap());

        // Request at 0.5 seconds should still be blocked
        clock.set_time(0.5);
        assert!(!limiter.is_allowed(client).unwrap());

        // Request at 1.0 seconds should be allowed (exactly 1 second later)
        clock.set_time(1.0);
        assert!(limiter.is_allowed(client).unwrap());

        // Another immediate request should be blocked again
        assert!(!limiter.is_allowed(client).unwrap());
    }

    #[test]
    fn burst_allowance_works() {
        let clock = TestClock::new(0.0);
        let limiter = RateLimiter::new(1.0, 3.0, clock.clone()).unwrap(); // 1 req/sec, burst of 3
        let client = "client1";

        // First 4 requests should all be allowed (burst capacity)
        assert!(limiter.is_allowed(client).unwrap());
        assert!(limiter.is_allowed(client).unwrap());
        assert!(limiter.is_allowed(client).unwrap());
        assert!(limiter.is_allowed(client).unwrap());

        // 5th request at same time should be blocked (burst exhausted)
        assert!(!limiter.is_allowed(client).unwrap());

        // After 1 second, 1 more request should be allowed
        clock.set_time(1.0);
        assert!(limiter.is_allowed(client).unwrap());

        // But immediate follow-up should be blocked
        assert!(!limiter.is_allowed(client).unwrap());
    }

    #[test]
    fn multiple_clients_independent() {
        let clock = TestClock::new(0.0);
        let limiter = RateLimiter::new(1.0, 0.0, clock.clone()).unwrap(); // 1 req/sec, no burst

        // Both clients' first requests should be allowed
        assert!(limiter.is_allowed("client1").unwrap());
        assert!(limiter.is_allowed("client2").unwrap());

        // Both clients' immediate second requests should be blocked
        assert!(!limiter.is_allowed("client1").unwrap());
        assert!(!limiter.is_allowed("client2").unwrap());

        // After 1 second, both should be allowed again
        clock.set_time(1.0);
        assert!(limiter.is_allowed("client1").unwrap());
        assert!(limiter.is_allowed("client2").unwrap());

        // Client1 exhausts their allowance, but client2 should still work
        assert!(!limiter.is_allowed("client1").unwrap());

        // Client3 (new client) should be allowed even though others are blocked
        assert!(limiter.is_allowed("client3").unwrap());
    }

    #[test]
    fn time_progression_allows_requests() {
        let clock = TestClock::new(0.0);
        let limiter = RateLimiter::new(2.0, 0.0, clock.clone()).unwrap(); // 2 req/sec, no burst
        let client = "client1";

        // First request at t=0 should be allowed
        assert!(limiter.is_allowed(client).unwrap());

        // Immediate second request should be blocked
        assert!(!limiter.is_allowed(client).unwrap());

        // Request at 0.25 seconds should still be blocked (need 0.5s interval for 2 req/sec)
        clock.set_time(0.25);
        assert!(!limiter.is_allowed(client).unwrap());

        // Request at exactly 0.5 seconds should be allowed
        clock.set_time(0.5);
        assert!(limiter.is_allowed(client).unwrap());

        // Immediate follow-up should be blocked again
        assert!(!limiter.is_allowed(client).unwrap());

        // Another 0.5 seconds later (t=1.0) should be allowed
        clock.set_time(1.0);
        assert!(limiter.is_allowed(client).unwrap());

        // Long idle period - request at t=10.0 should definitely be allowed
        clock.set_time(10.0);
        assert!(limiter.is_allowed(client).unwrap());
    }

    #[test]
    fn test_clock_advances_time() {
        let clock = TestClock::new(5.0);
        assert_eq!(clock.now(), 5.0);
        
        clock.advance(2.5);
        assert_eq!(clock.now(), 7.5);
        
        clock.set_time(0.0);
        assert_eq!(clock.now(), 0.0);
    }
}