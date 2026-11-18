use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Result;

pub(super) fn now_millis() -> Result<i64> {
    Ok(i64::try_from(
        SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis(),
    )?)
}
