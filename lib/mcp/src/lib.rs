use std::borrow::Cow;
use std::sync::Arc;

use anyhow::{Context, Result};
use axum::extract::{Request, State};
use axum::http::{StatusCode, header, request::Parts};
use axum::middleware::{Next, from_fn_with_state};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use rmcp::model::{
    CallToolRequestParams, CallToolResponse, CallToolResult, Implementation, ListToolsResult,
    PaginatedRequestParams, ProtocolVersion, ServerCapabilities, ServerConfig,
};
use rmcp::service::RequestContext;
use rmcp::transport::streamable_http_server::session::never::NeverSessionManager;
use rmcp::transport::{StreamableHttpServerConfig, StreamableHttpService};
use rmcp::{ErrorData, RoleServer, ServerHandler};
use serde::Serialize;
use typed_builder::TypedBuilder;
use url::Url;

use lorebay_lib_core::{
    Caller, KNOWLEDGE_FIELDS_MAX_JSON_BYTES, KNOWLEDGE_NAME_MAX_CHARACTERS, LibraryState,
};

use arguments::Arguments;
use catalog::catalog;

pub fn router(state: LibraryState) -> Result<Router> {
    let url = Url::parse(&state.settings().public_url)?;
    let origin = format!(
        "{}://{}:{}",
        url.scheme(),
        url.host().context("public_url must have a host")?,
        url.port_or_known_default()
            .context("public_url must have a port")?
    );
    let config = StreamableHttpServerConfig::default()
        .with_legacy_session_mode(false)
        .with_json_response(true)
        .with_max_request_body_bytes(MCP_REQUEST_BODY_MAX_BYTES)
        .disable_allowed_hosts()
        .with_allowed_origins([origin]);
    let server_state = state.clone();
    let transport = StreamableHttpService::new(
        move || {
            Ok(McpServer {
                state: server_state.clone(),
            })
        },
        Arc::new(NeverSessionManager::default()),
        config,
    );
    let path = &state.settings().mcp_path;
    let metadata = metadata_path(path);
    Ok(Router::new()
        .route_service(path, transport)
        .route_layer(from_fn_with_state(state.clone(), authenticate))
        .route(
            &metadata,
            get(protected_resource).fallback(resource_method_not_allowed),
        )
        .with_state(state))
}

const MCP_ENVELOPE_MAX_BYTES: usize = 4096;
const MCP_REQUEST_BODY_MAX_BYTES: usize =
    KNOWLEDGE_FIELDS_MAX_JSON_BYTES + KNOWLEDGE_NAME_MAX_CHARACTERS * 12 + MCP_ENVELOPE_MAX_BYTES;
const PROTOCOL_VERSION_HEADER: &str = "mcp-protocol-version";
const SUPPORTED_VERSIONS: &[ProtocolVersion] = &[
    ProtocolVersion::V_2025_03_26,
    ProtocolVersion::V_2025_06_18,
    ProtocolVersion::V_2025_11_25,
    ProtocolVersion::V_2026_07_28,
];

#[derive(Clone, Debug)]
struct McpServer {
    state: LibraryState,
}

impl ServerHandler for McpServer {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_protocol_version(ProtocolVersion::V_2026_07_28)
            .with_server_info(Implementation::new(
                self.state.settings().server_name.clone(),
                self.state.settings().server_version.clone(),
            ))
    }

    fn supported_protocol_versions(&self) -> Cow<'static, [ProtocolVersion]> {
        Cow::Borrowed(SUPPORTED_VERSIONS)
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        Ok(ListToolsResult::with_all_items(
            catalog().iter().map(|(_, tool)| tool.clone()).collect(),
        ))
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        let (name, tool) = catalog()
            .iter()
            .find(|(_, tool)| tool.name == request.name)
            .ok_or_else(|| ErrorData::invalid_params("Unknown tool", None))?;
        let caller = context
            .extensions
            .get::<Parts>()
            .and_then(|parts| parts.extensions.get::<Caller>())
            .cloned()
            .ok_or_else(|| ErrorData::internal_error("Request context unavailable", None))?;
        let arguments = match Arguments::parse(*name, tool, request.arguments) {
            Ok(arguments) => arguments,
            Err(error) => return Ok(error.into_result().into()),
        };
        let result = match tools::call(&self.state, &caller, *name, arguments).await {
            Ok(value) => CallToolResult::structured(value),
            Err(error) => error.into_result(),
        };
        Ok(result.into())
    }
}

async fn authenticate(
    State(state): State<LibraryState>,
    mut request: Request,
    next: Next,
) -> Response {
    let caller = match state.resolve_caller(request.headers()).await {
        Ok(Some(caller)) => caller,
        Ok(None) => return challenge(&state),
        Err(error) => {
            tracing::error!(%error, "caller resolution failed");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };
    request.extensions_mut().insert(caller);
    if request
        .headers()
        .get(PROTOCOL_VERSION_HEADER)
        .is_some_and(|value| {
            !SUPPORTED_VERSIONS
                .iter()
                .any(|version| value == version.as_str())
        })
    {
        return StatusCode::BAD_REQUEST.into_response();
    }
    next.run(request).await
}

fn challenge(state: &LibraryState) -> Response {
    let origin = state.settings().public_url.trim_end_matches('/');
    let metadata = metadata_path(&state.settings().mcp_path);
    (
        StatusCode::UNAUTHORIZED,
        [(
            header::WWW_AUTHENTICATE,
            format!("Bearer resource_metadata=\"{origin}{metadata}\""),
        )],
    )
        .into_response()
}

mod arguments;
mod catalog;
mod error;
mod tools;

#[derive(Clone, Debug, Serialize, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
struct ProtectedResourceResponse {
    resource: String,
    authorization_servers: [String; 1],
}

#[derive(Clone, Debug, Serialize, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
struct ResourceMethodError {
    error: &'static str,
    error_description: &'static str,
}

async fn protected_resource(State(state): State<LibraryState>) -> Json<ProtectedResourceResponse> {
    let settings = state.settings();
    Json(
        ProtectedResourceResponse::builder()
            .resource(format!(
                "{}{}",
                settings.public_url.trim_end_matches('/'),
                settings.mcp_path
            ))
            .authorization_servers([settings.authorization_server.clone()])
            .build(),
    )
}

async fn resource_method_not_allowed() -> impl IntoResponse {
    (
        StatusCode::METHOD_NOT_ALLOWED,
        [
            (header::CACHE_CONTROL, "no-store"),
            (header::PRAGMA, "no-cache"),
        ],
        Json(
            ResourceMethodError::builder()
                .error("invalid_request")
                .error_description("The request method is not supported.")
                .build(),
        ),
    )
}

fn metadata_path(mcp_path: &str) -> String {
    format!("/.well-known/oauth-protected-resource{mcp_path}")
}
