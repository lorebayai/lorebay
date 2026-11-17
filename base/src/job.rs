use std::future::Future;
use std::time::Duration;

use tokio::time::{Instant, MissedTickBehavior, interval_at};

/// Runs serial rounds at a positive interval, starting after one interval and skipping missed ticks.
/// Shutdown finishes the active round and prevents another; dropping this future cancels the round.
pub async fn run_job<F, R>(interval: Duration, shutdown: impl Future<Output = ()>, mut round: F)
where
    F: FnMut() -> R,
    R: Future<Output = ()>,
{
    let mut ticks = interval_at(Instant::now() + interval, interval);
    ticks.set_missed_tick_behavior(MissedTickBehavior::Skip);
    tokio::pin!(shutdown);
    loop {
        tokio::select! {
            biased;
            () = &mut shutdown => break,
            _ = ticks.tick() => round().await,
        }
    }
}
