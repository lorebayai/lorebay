pub(crate) fn format_key(prefix: &str, number: i64) -> String {
    format!("{prefix}-{number}")
}

pub(crate) fn parse_key(prefix: &str, key: &str) -> Option<i64> {
    let (actual, value) = key.split_once('-')?;
    let number = value.parse::<i64>().ok()?;
    (actual == prefix && number > 0 && key == format_key(prefix, number)).then_some(number)
}
