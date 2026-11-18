use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;

use crate::sha256_hex;

/// Encodes a cursor bound to its version and scope.
pub fn encode_cursor(version: &str, scope_id: &str, value: &str) -> String {
    URL_SAFE_NO_PAD.encode(format!("{version}\n{}\n{value}", sha256_hex(scope_id)))
}

/// Returns a nonempty cursor payload for the requested version and scope.
pub fn decode_cursor(version: &str, scope_id: &str, cursor: &str) -> Option<String> {
    let bytes = URL_SAFE_NO_PAD.decode(cursor).ok()?;
    let decoded = String::from_utf8(bytes).ok()?;
    let mut parts = decoded.splitn(3, '\n');
    if parts.next() != Some(version) || parts.next() != Some(sha256_hex(scope_id).as_str()) {
        return None;
    }
    parts
        .next()
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}
