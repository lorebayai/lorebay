use axum::http::header;
use axum::response::{Html, IntoResponse, Response};
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use sha2::{Digest, Sha256};

use crate::oauth::no_store_headers;

const DOCUMENT: &str = include_str!("authorization.html");
const SCRIPT: &str = include_str!("authorization.js");

pub(crate) async fn authorization() -> Response {
    let hash = STANDARD.encode(Sha256::digest(SCRIPT.as_bytes()));
    let policy = format!(
        "default-src 'none'; script-src 'sha256-{hash}'; connect-src 'self'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'"
    );
    (
        no_store_headers(),
        [
            (header::CONTENT_SECURITY_POLICY, policy),
            (header::REFERRER_POLICY, "no-referrer".to_owned()),
            (header::X_CONTENT_TYPE_OPTIONS, "nosniff".to_owned()),
            (header::X_FRAME_OPTIONS, "DENY".to_owned()),
        ],
        Html(DOCUMENT.replace("{{SCRIPT}}", SCRIPT)),
    )
        .into_response()
}
