use std::collections::HashMap;
use std::hash::Hash;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, SystemTime};

use typed_builder::TypedBuilder;

use crate::Clock;

#[derive(Clone, Debug, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub struct RateLimiter<K, C> {
    limit: u32,
    window: Duration,
    clock: C,
    #[builder(default = Arc::new(Mutex::new(HashMap::new())))]
    entries: Arc<Mutex<HashMap<K, (SystemTime, u32)>>>,
}

impl<K: Eq + Hash, C: Clock> RateLimiter<K, C> {
    /// Counts an attempt in a fixed window, returning the wait when the limit is reached.
    pub fn check(&self, key: K) -> Result<(), Duration> {
        let now = self.clock.now();
        let mut entries = self.entries.lock().unwrap_or_else(PoisonError::into_inner);
        entries
            .retain(|_, (start, _)| now.duration_since(*start).unwrap_or_default() < self.window);
        let (start, count) = entries.entry(key).or_insert((now, 0));
        if *count >= self.limit {
            return Err(self
                .window
                .saturating_sub(now.duration_since(*start).unwrap_or_default()));
        }
        *count += 1;
        Ok(())
    }
}
