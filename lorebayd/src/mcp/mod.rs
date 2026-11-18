use std::borrow::Cow;
use std::sync::Arc;

use anyhow::{Context, Result};
use axum::Router;
use axum::extract::{Request, State};
use axum::http::{StatusCode, header, request::Parts};
use axum::middleware::{Next, from_fn_with_state};
use axum::response::{IntoResponse, Response};
use rmcp::model::{
    CallToolRequestParams, CallToolResponse, CallToolResult, Implementation, ListToolsResult,
    PaginatedRequestParams, ProtocolVersion, ServerCapabilities, ServerConfig,
};
use rmcp::service::RequestContext;
use rmcp::transport::streamable_http_server::session::never::NeverSessionManager;
use rmcp::transport::{StreamableHttpServerConfig, StreamableHttpService};
use rmcp::{ErrorData, RoleServer, ServerHandler};
use url::Url;

use lorebay_base::Secret;

use crate::domain::access::{AccessError, AgentCaller, verify_agent};
use crate::domain::caller::VerifiedCaller;
use crate::domain::{MCP_PATH, PROTECTED_RESOURCE_METADATA_PATH};
use crate::server::ServerState;

use arguments::Arguments;
use catalog::catalog;

pub(crate) fn router(state: ServerState) -> Result<Router> {
    let url = Url::parse(&state.identity.config.public_url)?;
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
    Ok(Router::new()
        .route_service(MCP_PATH, transport)
        .layer(from_fn_with_state(state, authenticate)))
}

const PROTOCOL_VERSION_HEADER: &str = "mcp-protocol-version";
const SUPPORTED_VERSIONS: &[ProtocolVersion] = &[
    ProtocolVersion::V_2025_03_26,
    ProtocolVersion::V_2025_06_18,
    ProtocolVersion::V_2025_11_25,
    ProtocolVersion::V_2026_07_28,
];

#[derive(Clone, Debug)]
struct McpServer {
    state: ServerState,
}

impl ServerHandler for McpServer {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_protocol_version(ProtocolVersion::V_2026_07_28)
            .with_server_info(Implementation::new("lorebayd", env!("CARGO_PKG_VERSION")))
    }

    fn supported_protocol_versions(&self) -> Cow<'static, [ProtocolVersion]> {
        Cow::Borrowed(SUPPORTED_VERSIONS)
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        Ok(ListToolsResult::with_all_items(catalog().to_vec()))
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        let tool = catalog()
            .iter()
            .find(|tool| tool.name == request.name)
            .ok_or_else(|| ErrorData::invalid_params("Unknown tool", None))?;
        let caller = context
            .extensions
            .get::<Parts>()
            .and_then(|parts| parts.extensions.get::<AgentCaller>())
            .cloned()
            .map(VerifiedCaller::Agent)
            .ok_or_else(|| ErrorData::internal_error("Request context unavailable", None))?;
        let arguments = match Arguments::parse(tool, request.arguments) {
            Ok(arguments) => arguments,
            Err(error) => return Ok(tools::error_result(error).into()),
        };
        let result = match tools::call(&self.state, &caller, &request.name, arguments).await {
            Ok(value) => CallToolResult::structured(value),
            Err(error) => tools::error_result(error),
        };
        Ok(result.into())
    }
}

async fn authenticate(
    State(state): State<ServerState>,
    mut request: Request,
    next: Next,
) -> Response {
    let token = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split_once(' '))
        .filter(|(scheme, _)| scheme.eq_ignore_ascii_case("Bearer"))
        .map(|(_, token)| Secret::from(token));
    let Some(token) = token else {
        return challenge(&state);
    };
    let Ok(claims) = state.identity.signer.verify(token.expose()) else {
        return challenge(&state);
    };
    let caller = match verify_agent(&state.database, claims).await {
        Ok(caller) => caller,
        Err(AccessError::Unauthorized) => return challenge(&state),
        Err(error) => {
            tracing::error!(%error, "agent authentication failed");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };
    tracing::debug!(account_id = %caller.account_id, agent_authorization_id = %caller.authorization_id,
        "authenticated MCP caller");
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

fn challenge(state: &ServerState) -> Response {
    let origin = state.identity.config.public_url.trim_end_matches('/');
    (
        StatusCode::UNAUTHORIZED,
        [(
            header::WWW_AUTHENTICATE,
            format!("Bearer resource_metadata=\"{origin}{PROTECTED_RESOURCE_METADATA_PATH}\""),
        )],
    )
        .into_response()
}

mod arguments;
mod catalog;
mod tools;
