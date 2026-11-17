use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use typed_builder::TypedBuilder;

use super::token::token_hash;

#[derive(Clone, Debug, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub(crate) struct PageOptions {
    pub page_size: u64,
    pub cursor: Option<String>,
}

#[derive(Clone, Debug, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub(crate) struct Page<T> {
    pub items: Vec<T>,
    pub next_cursor: Option<String>,
}

pub(crate) fn encode_cursor(version: &str, account_id: &str, value: &str) -> String {
    URL_SAFE_NO_PAD.encode(format!("{version}\n{}\n{value}", token_hash(account_id)))
}

pub(crate) fn decode_cursor(version: &str, account_id: &str, cursor: &str) -> Option<String> {
    let bytes = URL_SAFE_NO_PAD.decode(cursor).ok()?;
    let decoded = String::from_utf8(bytes).ok()?;
    let mut parts = decoded.splitn(3, '\n');
    if parts.next() != Some(version) || parts.next() != Some(token_hash(account_id).as_str()) {
        return None;
    }
    parts
        .next()
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}
