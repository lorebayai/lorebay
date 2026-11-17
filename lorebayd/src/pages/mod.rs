use axum::http::header;
use axum::response::{Html, IntoResponse, Response};
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use sha2::{Digest, Sha256};

use crate::oauth::no_store_headers;

pub(crate) async fn authorization() -> Response {
    page(DOCUMENT, SCRIPT)
}

pub(crate) async fn device() -> Response {
    page(DEVICE_DOCUMENT, DEVICE_SCRIPT)
}

const DEVICE_DOCUMENT: &str = include_str!("../../../web/out/device.html");
const DEVICE_SCRIPT: &str = include_str!("../../../web/out/device.js");
const DOCUMENT: &str = include_str!("../../../web/out/authorization.html");
const SCRIPT: &str = include_str!("../../../web/out/authorization.js");

fn page(document: &str, script: &str) -> Response {
    let hash = STANDARD.encode(Sha256::digest(script.as_bytes()));
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
        Html(document.replace(
            "<!-- PAGE_SCRIPT -->",
            &format!("<script>{script}</script>"),
        )),
    )
        .into_response()
}
