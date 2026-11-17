use std::collections::HashMap;
use std::hash::Hash;
use std::num::NonZeroU32;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use thiserror::Error;
use typed_builder::TypedBuilder;

use crate::Clock;

#[derive(Debug, Error)]
pub enum RateLimitError {
    #[error("rate limit exceeded")]
    Exceeded { retry_after: Duration },
    #[error("rate limiter is unavailable")]
    Unavailable,
}

#[derive(Clone, Debug, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub struct RateLimiter<K, C> {
    clock: Arc<C>,
    limit: NonZeroU32,
    window: Duration,
    entries: Arc<Mutex<HashMap<K, Window>>>,
}

impl<K: Eq + Hash, C: Clock> RateLimiter<K, C> {
    /// Counts attempts per key in a fixed window; expired keys are removed on each check.
    pub fn new(limit: NonZeroU32, window: Duration, clock: Arc<C>) -> Self {
        Self::builder()
            .clock(clock)
            .limit(limit)
            .window(window)
            .entries(Arc::new(Mutex::new(HashMap::new())))
            .build()
    }

    pub fn check(&self, key: K) -> Result<(), RateLimitError> {
        let now = self.clock.now();
        let mut entries = self
            .entries
            .lock()
            .map_err(|_| RateLimitError::Unavailable)?;
        entries.retain(|_, entry| elapsed(now, entry.started_at) < self.window);
        let entry = entries
            .entry(key)
            .or_insert_with(|| Window::builder().started_at(now).attempts(0_u32).build());
        if entry.attempts >= self.limit.get() {
            return Err(RateLimitError::Exceeded {
                retry_after: self.window.saturating_sub(elapsed(now, entry.started_at)),
            });
        }
        entry.attempts += 1;
        Ok(())
    }
}

#[derive(Clone, Debug, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
struct Window {
    started_at: SystemTime,
    attempts: u32,
}

fn elapsed(now: SystemTime, started_at: SystemTime) -> Duration {
    now.duration_since(started_at).unwrap_or_default()
}
