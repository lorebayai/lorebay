use sha2::{Digest, Sha256};

/// Returns the lowercase hexadecimal SHA-256 of the string's UTF-8 bytes.
pub fn sha256_hex(value: &str) -> String {
    Sha256::digest(value.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
