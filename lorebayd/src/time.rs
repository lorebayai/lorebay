use std::time::SystemTime;

use lorebay_lib_types::unix_millis;

pub(crate) fn now_millis() -> anyhow::Result<i64> {
    Ok(unix_millis(SystemTime::now())?)
}
