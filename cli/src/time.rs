use std::time::{Duration, UNIX_EPOCH};

use httpdate::{fmt_http_date, parse_http_date};
use serde_json::Value;

use crate::commands::display_value;

pub(crate) fn display(value: &Value) -> String {
    let Some(milliseconds) = value.as_u64().filter(|value| *value < MAX_MILLISECONDS) else {
        return display_value(value);
    };
    let date = fmt_http_date(UNIX_EPOCH + Duration::from_millis(milliseconds));
    date.replace(" GMT", &format!(".{:03} GMT", milliseconds % 1000))
}

pub(crate) fn parse(text: &str) -> Option<String> {
    let (seconds, fraction) = text.strip_suffix(" GMT")?.rsplit_once('.')?;
    if fraction.len() != 3 || !fraction.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let time = parse_http_date(&format!("{seconds} GMT")).ok()?;
    let milliseconds =
        time.duration_since(UNIX_EPOCH).ok()?.as_millis() + fraction.parse::<u128>().ok()?;
    Some(milliseconds.to_string())
}

const MAX_MILLISECONDS: u64 = 253_402_300_800_000;
