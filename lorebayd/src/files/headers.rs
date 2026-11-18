use axum::http::header::{self, InvalidHeaderValue};
use axum::http::{HeaderMap, HeaderValue};

pub(super) fn download_headers(
    file_name: &str,
    size: u64,
) -> Result<HeaderMap, InvalidHeaderValue> {
    let extension = file_name
        .rsplit_once('.')
        .map(|(_, extension)| extension.to_ascii_lowercase());
    let content_type = match extension.as_deref() {
        Some("gif") => "image/gif",
        Some("jpeg" | "jpg") => "image/jpeg",
        Some("pdf") => "application/pdf",
        Some("png") => "image/png",
        Some("webp") => "image/webp",
        _ => "application/octet-stream",
    };
    let disposition = match content_type {
        "application/octet-stream" => "attachment",
        _ => "inline",
    };
    let file_name: String = file_name
        .chars()
        .filter(|character| !character.is_control())
        .collect();
    let fallback: String = file_name
        .chars()
        .map(
            |character| {
                if character.is_ascii() { character } else { '_' }
            },
        )
        .collect();
    let fallback = fallback.replace('\\', "\\\\").replace('"', "\\\"");
    let disposition = format!(
        "{disposition}; filename=\"{fallback}\"; filename*=UTF-8''{}",
        encode_file_name(&file_name)
    );
    Ok(HeaderMap::from_iter([
        (header::CONTENT_TYPE, HeaderValue::from_static(content_type)),
        (header::CONTENT_LENGTH, HeaderValue::from(size)),
        (
            header::CONTENT_DISPOSITION,
            HeaderValue::from_str(&disposition)?,
        ),
    ]))
}

const HEX: &[u8; 16] = b"0123456789ABCDEF";

fn encode_file_name(file_name: &str) -> String {
    let mut encoded = String::new();
    for byte in file_name.bytes() {
        if byte.is_ascii_alphanumeric() || b"!#$&+-.^_`|~".contains(&byte) {
            encoded.push(char::from(byte));
        } else {
            encoded.push('%');
            encoded.push(char::from(HEX[usize::from(byte >> 4)]));
            encoded.push(char::from(HEX[usize::from(byte & 15)]));
        }
    }
    encoded
}
