//! In-memory sliding-window rate limiting.
//!
//! Guards the unauthenticated surface — logins, registration, password reset —
//! where until now argon2's cost was the only thing slowing a brute-force
//! attempt.
//!
//! **In-memory, so each API replica counts independently.** With N replicas an
//! attacker gets N times the budget, and a restart clears every counter. That
//! is an acceptable trade at this scale: the deployment is single-node
//! (docs/DESIGN.md §2.1), and the alternative is a database write on every
//! login attempt — which hands an attacker a cheap way to generate write load.
//! If this ever runs multiple replicas, move the counters to Postgres or the
//! Redis §2.8 already contemplates.
//!
//! Comment rate limiting deliberately does *not* use this: it counts real rows
//! in the `comments` table, which is exact, shared across replicas, and
//! survives restarts. See `comments::routes`.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::error::AppError;

/// One rule: at most `max_events` within `window`.
#[derive(Debug, Clone, Copy)]
pub struct Quota {
    pub max_events: usize,
    pub window: Duration,
}

impl Quota {
    pub const fn new(max_events: usize, window_secs: u64) -> Self {
        Self {
            max_events,
            window: Duration::from_secs(window_secs),
        }
    }
}

/// Failed logins per account. Deliberately generous enough not to lock out
/// someone genuinely fumbling a password, tight enough that online guessing is
/// hopeless against argon2's cost.
pub const LOGIN_PER_ACCOUNT: Quota = Quota::new(10, 900);
/// Login attempts per client address, covering an attacker spraying one
/// password across many accounts — which the per-account limit never sees.
pub const LOGIN_PER_IP: Quota = Quota::new(30, 900);
/// Registration, resend, and password-reset requests per address. Each one
/// sends an email, so the limit protects other people's inboxes as much as
/// this service.
pub const EMAIL_TRIGGER_PER_IP: Quota = Quota::new(5, 900);

#[derive(Debug, Default)]
struct Bucket {
    /// Event timestamps inside the current window. Bounded by `max_events`,
    /// so a hot key cannot grow this without limit.
    hits: Vec<Instant>,
}

#[derive(Debug, Clone)]
pub struct RateLimiter {
    buckets: Arc<Mutex<HashMap<String, Bucket>>>,
}

impl Default for RateLimiter {
    fn default() -> Self {
        Self::new()
    }
}

impl RateLimiter {
    pub fn new() -> Self {
        Self {
            buckets: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Records an attempt against `key`, or returns `TooManyRequests`.
    ///
    /// Sliding window rather than fixed: a fixed window lets an attacker send
    /// a full budget at the end of one window and another at the start of the
    /// next, doubling the effective rate at the boundary.
    pub fn check(&self, key: &str, quota: Quota) -> Result<(), AppError> {
        let now = Instant::now();
        let mut buckets = self.buckets.lock().expect("rate limiter mutex poisoned");

        let bucket = buckets.entry(key.to_string()).or_default();
        bucket.hits.retain(|t| now.duration_since(*t) < quota.window);

        if bucket.hits.len() >= quota.max_events {
            return Err(AppError::TooManyRequests);
        }

        bucket.hits.push(now);
        Ok(())
    }

    /// Clears a key's history, called after a success.
    ///
    /// Without this, a user who mistypes a password nine times and then gets
    /// it right stays one attempt from lockout for the rest of the window.
    pub fn reset(&self, key: &str) {
        let mut buckets = self.buckets.lock().expect("rate limiter mutex poisoned");
        buckets.remove(key);
    }

    /// Drops keys with no recent activity. Without it the map grows for the
    /// life of the process, one entry per address ever seen.
    pub fn sweep(&self, max_age: Duration) {
        let now = Instant::now();
        let mut buckets = self.buckets.lock().expect("rate limiter mutex poisoned");

        buckets.retain(|_, bucket| {
            bucket.hits.retain(|t| now.duration_since(*t) < max_age);
            !bucket.hits.is_empty()
        });
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.buckets.lock().unwrap().len()
    }
}

/// Starts a background task that periodically discards stale buckets.
pub fn spawn_sweeper(limiter: RateLimiter, max_age: Duration) {
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(Duration::from_secs(300));
        loop {
            ticker.tick().await;
            limiter.sweep(max_age);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    const FAST: Quota = Quota {
        max_events: 3,
        window: Duration::from_millis(150),
    };

    #[test]
    fn allows_up_to_the_quota_then_refuses() {
        let limiter = RateLimiter::new();

        for i in 0..3 {
            assert!(limiter.check("k", FAST).is_ok(), "attempt {i} should pass");
        }
        assert!(matches!(
            limiter.check("k", FAST),
            Err(AppError::TooManyRequests)
        ));
    }

    #[test]
    fn keys_are_independent() {
        let limiter = RateLimiter::new();

        for _ in 0..3 {
            limiter.check("a", FAST).unwrap();
        }
        assert!(limiter.check("a", FAST).is_err());
        assert!(
            limiter.check("b", FAST).is_ok(),
            "one key's exhaustion must not affect another"
        );
    }

    #[test]
    fn window_slides() {
        let limiter = RateLimiter::new();

        for _ in 0..3 {
            limiter.check("k", FAST).unwrap();
        }
        assert!(limiter.check("k", FAST).is_err());

        std::thread::sleep(Duration::from_millis(200));
        assert!(
            limiter.check("k", FAST).is_ok(),
            "attempts should age out of the window"
        );
    }

    #[test]
    fn reset_clears_history() {
        let limiter = RateLimiter::new();

        for _ in 0..3 {
            limiter.check("k", FAST).unwrap();
        }
        assert!(limiter.check("k", FAST).is_err());

        limiter.reset("k");
        assert!(limiter.check("k", FAST).is_ok());
    }

    #[test]
    fn sweep_drops_stale_keys_but_keeps_active_ones() {
        let limiter = RateLimiter::new();
        limiter.check("old", FAST).unwrap();

        std::thread::sleep(Duration::from_millis(120));
        limiter.check("fresh", FAST).unwrap();
        assert_eq!(limiter.len(), 2);

        limiter.sweep(Duration::from_millis(100));
        assert_eq!(limiter.len(), 1, "only the stale key should be dropped");
        assert!(limiter.check("fresh", FAST).is_ok());
    }
}
