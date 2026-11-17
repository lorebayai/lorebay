use std::collections::HashMap;
use std::hash::Hash;
use std::net::{IpAddr, Ipv6Addr};
use std::num::NonZeroU32;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use thiserror::Error;
use typed_builder::TypedBuilder;

use crate::Clock;

pub trait RateLimitKey: Eq + Hash {
    fn normalized(self) -> Self;
}

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
    state: Arc<Mutex<LimiterState<K>>>,
}

impl RateLimitKey for IpAddr {
    fn normalized(self) -> Self {
        match self {
            Self::V4(_) => self,
            Self::V6(address) => address.to_ipv4_mapped().map_or_else(
                || Self::V6(Ipv6Addr::from(u128::from(address) & IPV6_PREFIX_MASK)),
                Self::V4,
            ),
        }
    }
}

impl<K: RateLimitKey, C: Clock> RateLimiter<K, C> {
    /// Counts attempts per normalized key; expired entries are swept at most once per window.
    pub fn new(limit: NonZeroU32, window: Duration, clock: Arc<C>) -> Self {
        let state = LimiterState::builder()
            .entries(HashMap::new())
            .swept_at(clock.now())
            .build();
        Self::builder()
            .clock(clock)
            .limit(limit)
            .window(window)
            .state(Arc::new(Mutex::new(state)))
            .build()
    }

    pub fn check(&self, key: K) -> Result<(), RateLimitError> {
        let now = self.clock.now();
        let mut state = self.state.lock().map_err(|_| RateLimitError::Unavailable)?;
        if elapsed(now, state.swept_at) >= self.window {
            state
                .entries
                .retain(|_, entry| elapsed(now, entry.started_at) < self.window);
            state.swept_at = now;
        }
        let entry = state
            .entries
            .entry(key.normalized())
            .or_insert_with(|| Window::builder().started_at(now).attempts(0_u32).build());
        if elapsed(now, entry.started_at) >= self.window {
            entry.started_at = now;
            entry.attempts = 0;
        }
        if entry.attempts >= self.limit.get() {
            return Err(RateLimitError::Exceeded {
                retry_after: self.window.saturating_sub(elapsed(now, entry.started_at)),
            });
        }
        entry.attempts += 1;
        Ok(())
    }
}

const IPV6_PREFIX_MASK: u128 = u128::MAX << 64;

#[derive(Clone, Debug, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
struct LimiterState<K> {
    entries: HashMap<K, Window>,
    swept_at: SystemTime,
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
