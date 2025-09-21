// lib/rate_limiter.rs

// dependencies
use std::collections::HashMap;
use std::error::Error;
use std::fmt;
use std::sync::{Arc, Mutex};

// enum type to represent errors related to the rate limiter type
#[derive(Debug)]
pub enum RateLimiterError {
    InvalidRate,   // for rate <= 0
    InvalidBurst,  // for burst < 0
    MutexPoisoned, // if there's an issue with the Mutex
}

// implement the Display trait for the RateLimiterError type
impl fmt::Display for RateLimiterError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            RateLimiterError::InvalidRate => write!(f, "Rate must be positive"),
            RateLimiterError::InvalidBurst => write!(f, "Burst must be non-negative"),
            RateLimiterError::MutexPoisoned => write!(f, "Internal state lock was poisoned"),
        }
    }
}

// implement the Error trait for the RateLimiter type
impl Error for RateLimiterError {}

// struct type to represent a rate limiter
#[derive(Debug)]
pub struct RateLimiter {
    rate: f64,
    burst: f64,
    client_state: Arc<Mutex<HashMap<String, f64>>>,
}

// methods for the RateLimiter struct
impl RateLimiter {
    // method to create a new rate limiter given a desired rate and burst value
    pub fn new(rate: f64, burst: f64) -> Result<Self, RateLimiterError> {
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
            client_state: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    // accessor method to return the rate field
    pub fn get_rate(&self) -> f64 {
        self.rate
    }

    // accessor method to return the burst field
    pub fn get_burst(&self) -> f64 {
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
    pub fn is_allowed(&self, client_id: &str, current_time: f64) -> Result<bool, RateLimiterError> {
        // get access to the client state
        let mut state = self
            .client_state
            .lock()
            .map_err(|_| RateLimiterError::MutexPoisoned)?;

        // Get previous TAT, default to current_time for new clients
        let previous_tat = state.get(client_id).copied().unwrap_or(current_time);

        // Core GCRA conformance test
        let is_conforming = current_time >= previous_tat - self.tolerance();

        // Update TAT if request is allowed
        if is_conforming {
            let new_tat = f64::max(current_time, previous_tat) + self.increment();
            state.insert(client_id.to_string(), new_tat);
        }

        Ok(is_conforming)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constructor_rejects_zero_rate() {
        let result = RateLimiter::new(0.0, 1.0);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), RateLimiterError::InvalidRate));
    }

    #[test]
    fn constructor_rejects_negative_rate() {
        let result = RateLimiter::new(-1.0, 1.0);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), RateLimiterError::InvalidRate));
    }

    #[test]
    fn constructor_rejects_negative_burst() {
        let result = RateLimiter::new(1.0, -1.0);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            RateLimiterError::InvalidBurst
        ));
    }

    #[test]
    fn constructor_accepts_valid_parameters() {
        let result = RateLimiter::new(10.0, 5.0);
        assert!(result.is_ok());
    }

    #[test]
    fn constructor_accepts_zero_burst() {
        let result = RateLimiter::new(1.0, 0.0);
        assert!(result.is_ok());
    }

    #[test]
    fn first_request_always_allowed() {
        let limiter = RateLimiter::new(1.0, 1.0).unwrap();
        let result = limiter.is_allowed("client1", 0.0);
        assert!(result.unwrap());
    }

    #[test]
    fn rate_limiting_blocks_rapid_requests() {
        let limiter = RateLimiter::new(1.0, 0.0).unwrap(); // 1 req/sec, no burst
        let client = "client1";

        // First request at time 0.0 should be allowed
        assert!(limiter.is_allowed(client, 0.0).unwrap());

        // Second request immediately after should be blocked
        assert!(!limiter.is_allowed(client, 0.0).unwrap());

        // Request at 0.5 seconds should still be blocked
        assert!(!limiter.is_allowed(client, 0.5).unwrap());

        // Request at 1.0 seconds should be allowed (exactly 1 second later)
        assert!(limiter.is_allowed(client, 1.0).unwrap());

        // Another immediate request should be blocked again
        assert!(!limiter.is_allowed(client, 1.0).unwrap());
    }

    #[test]
    fn burst_allowance_works() {
        let limiter = RateLimiter::new(1.0, 3.0).unwrap(); // 1 req/sec, burst of 3
        let client = "client1";
        let time = 0.0;

        // First 4 requests should all be allowed (burst capacity)
        assert!(limiter.is_allowed(client, time).unwrap());
        assert!(limiter.is_allowed(client, time).unwrap());
        assert!(limiter.is_allowed(client, time).unwrap());
        assert!(limiter.is_allowed(client, time).unwrap());

        // 5th request at same time should be blocked (burst exhausted)
        assert!(!limiter.is_allowed(client, time).unwrap());

        // After 1 second, 1 more request should be allowed
        assert!(limiter.is_allowed(client, 1.0).unwrap());

        // But immediate follow-up should be blocked
        assert!(!limiter.is_allowed(client, 1.0).unwrap());
    }

    #[test]
    fn multiple_clients_independent() {
        let limiter = RateLimiter::new(1.0, 0.0).unwrap(); // 1 req/sec, no burst
        let time = 0.0;

        // Both clients' first requests should be allowed
        assert!(limiter.is_allowed("client1", time).unwrap());
        assert!(limiter.is_allowed("client2", time).unwrap());

        // Both clients' immediate second requests should be blocked
        assert!(!limiter.is_allowed("client1", time).unwrap());
        assert!(!limiter.is_allowed("client2", time).unwrap());

        // After 1 second, both should be allowed again
        assert!(limiter.is_allowed("client1", 1.0).unwrap());
        assert!(limiter.is_allowed("client2", 1.0).unwrap());

        // Client1 exhausts their allowance, but client2 should still work
        assert!(!limiter.is_allowed("client1", 1.0).unwrap());

        // Client3 (new client) should be allowed even though others are blocked
        assert!(limiter.is_allowed("client3", 1.0).unwrap());
    }

    #[test]
    fn time_progression_allows_requests() {
        let limiter = RateLimiter::new(2.0, 0.0).unwrap(); // 2 req/sec, no burst
        let client = "client1";

        // First request at t=0 should be allowed
        assert!(limiter.is_allowed(client, 0.0).unwrap());

        // Immediate second request should be blocked
        assert!(!limiter.is_allowed(client, 0.0).unwrap());

        // Request at 0.25 seconds should still be blocked (need 0.5s interval for 2 req/sec)
        assert!(!limiter.is_allowed(client, 0.25).unwrap());

        // Request at exactly 0.5 seconds should be allowed
        assert!(limiter.is_allowed(client, 0.5).unwrap());

        // Immediate follow-up should be blocked again
        assert!(!limiter.is_allowed(client, 0.5).unwrap());

        // Another 0.5 seconds later (t=1.0) should be allowed
        assert!(limiter.is_allowed(client, 1.0).unwrap());

        // Long idle period - request at t=10.0 should definitely be allowed
        assert!(limiter.is_allowed(client, 10.0).unwrap());
    }
}
