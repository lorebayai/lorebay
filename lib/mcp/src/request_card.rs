use rmcp::ErrorData;
use rmcp::model::{
    ListResourcesResult, MetaObject, ReadResourceResult, Resource, ResourceContents,
};
use serde_json::json;

pub(super) const URI: &str = "ui://requests/request-card.html";

pub(super) fn list() -> ListResourcesResult {
    ListResourcesResult::with_all_items(vec![
        Resource::new(URI, "request-card")
            .with_title("Request card")
            .with_mime_type(MIME_TYPE)
            .with_meta(resource_meta()),
    ])
}

pub(super) fn read(uri: &str, html: &str) -> Result<ReadResourceResult, ErrorData> {
    if uri != URI {
        return Err(ErrorData::resource_not_found("Resource not found", None));
    }
    Ok(ReadResourceResult::new(vec![
        ResourceContents::text(html, URI)
            .with_mime_type(MIME_TYPE)
            .with_meta(resource_meta()),
    ]))
}

pub(super) fn document(display_name: &str) -> String {
    let escaped_name = display_name
        .replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('\r', "&#13;");
    include_str!("request-card.html").replacen(
        "data-product-name=\"\"",
        &format!("data-product-name=\"{escaped_name}\""),
        1,
    )
}

const MIME_TYPE: &str = "text/html;profile=mcp-app";
const HOST_FONT_ORIGIN: &str = "https://assets.claude.ai";

fn resource_meta() -> MetaObject {
    let mut meta = MetaObject::new();
    meta.insert(
        "ui".to_owned(),
        json!({"prefersBorder": false, "csp": {"resourceDomains": [HOST_FONT_ORIGIN]}}),
    );
    meta
}
