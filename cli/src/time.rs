use serde_json::Value;

use crate::commands::display_value;

pub(crate) fn display(value: &Value) -> String {
    value
        .as_u64()
        .filter(|value| *value < MAX_MILLISECONDS)
        .map(format_time)
        .unwrap_or_else(|| display_value(value))
}

pub(crate) fn parse(text: &str) -> Option<String> {
    if text.len() != 24 {
        return None;
    }
    let year = text.get(..4)?.parse::<u64>().ok()?;
    let month = text.get(5..7)?.parse::<usize>().ok()?;
    let day = text.get(8..10)?.parse::<u64>().ok()?;
    let hour = text.get(11..13)?.parse::<u64>().ok()?;
    let minute = text.get(14..16)?.parse::<u64>().ok()?;
    let second = text.get(17..19)?.parse::<u64>().ok()?;
    let fraction = text.get(20..23)?.parse::<u64>().ok()?;
    if !(EPOCH_YEAR..=9999).contains(&year) || hour >= 24 || minute >= 60 || second >= 60 {
        return None;
    }
    let months = month_lengths(year);
    let month_index = month.checked_sub(1)?;
    if !(1..=*months.get(month_index)?).contains(&day) {
        return None;
    }
    let days = days_before_year(year) - days_before_year(EPOCH_YEAR)
        + months[..month_index].iter().sum::<u64>()
        + day
        - 1;
    let milliseconds =
        days * MILLIS_PER_DAY + ((hour * 60 + minute) * 60 + second) * MILLIS_PER_SECOND + fraction;
    (format_time(milliseconds) == text).then(|| milliseconds.to_string())
}

const EPOCH_YEAR: u64 = 1970;
const MAX_MILLISECONDS: u64 = 253_402_300_800_000;
const MILLIS_PER_DAY: u64 = 86_400_000;
const MILLIS_PER_SECOND: u64 = 1000;

fn date(days_since_epoch: u64) -> [u64; 3] {
    let absolute_day = days_before_year(EPOCH_YEAR) + days_since_epoch;
    let mut year = EPOCH_YEAR + days_since_epoch / 366;
    while days_before_year(year + 1) <= absolute_day {
        year += 1;
    }
    let mut day = absolute_day - days_before_year(year);
    let mut month = 1;
    for length in month_lengths(year) {
        if day < length {
            break;
        }
        day -= length;
        month += 1;
    }
    [year, month, day + 1]
}

fn days_before_year(year: u64) -> u64 {
    let previous = year - 1;
    previous * 365 + previous / 4 - previous / 100 + previous / 400
}

fn format_time(milliseconds: u64) -> String {
    let [year, month, day] = date(milliseconds / MILLIS_PER_DAY);
    let seconds = milliseconds / MILLIS_PER_SECOND;
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}.{:03}Z",
        seconds / 3600 % 24,
        seconds / 60 % 60,
        seconds % 60,
        milliseconds % MILLIS_PER_SECOND
    )
}

fn month_lengths(year: u64) -> [u64; 12] {
    let leap = year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400));
    [
        31,
        if leap { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ]
}
