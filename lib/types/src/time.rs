use std::num::TryFromIntError;
use std::time::{SystemTime, SystemTimeError, UNIX_EPOCH};

use thiserror::Error;

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum UnixMillisError {
    #[error(transparent)]
    BeforeEpoch(#[from] SystemTimeError),
    #[error(transparent)]
    Overflow(#[from] TryFromIntError),
}

/// Converts a clock reading to milliseconds since the Unix epoch.
pub fn unix_millis(time: SystemTime) -> Result<i64, UnixMillisError> {
    Ok(i64::try_from(time.duration_since(UNIX_EPOCH)?.as_millis())?)
}
