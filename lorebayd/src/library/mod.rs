use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use lorebay_lib_core::{LibraryInterfaces, LibrarySettings, LibraryState};
use sea_orm::DatabaseConnection;

use crate::Config;
use crate::domain::MCP_PATH;
use crate::domain::login_session::Identity;
use crate::storage::LocalDirectory;

const MCP_SERVER_NAME: &str = "lorebayd";
const INLINE_CONTENT_TYPES: [&str; 5] = [
    "image/gif",
    "image/jpeg",
    "image/png",
    "image/webp",
    "application/pdf",
];

pub(crate) fn settings(config: &Config) -> LibrarySettings {
    let origin = config.public_url.trim_end_matches('/').to_owned();
    LibrarySettings::builder()
        .public_url(origin.clone())
        .mcp_path(MCP_PATH.to_owned())
        .server_name(MCP_SERVER_NAME.to_owned())
        .server_version(env!("CARGO_PKG_VERSION").to_owned())
        .authorization_server(origin)
        .upload_size_limit(config.upload_size_limit_bytes)
        .upload_url_lifetime(Duration::from_secs(config.upload_url_lifetime_seconds))
        .download_url_lifetime(Duration::from_secs(config.upload_url_lifetime_seconds))
        .inline_content_types(INLINE_CONTENT_TYPES.map(str::to_owned).to_vec())
        .abandoned_agent_file_lifetime(Duration::from_secs(config.abandoned_file_lifetime_seconds))
        .orphaned_file_lifetime(Duration::from_secs(config.orphaned_file_lifetime_seconds))
        .build()
}

pub(crate) fn state(
    settings: LibrarySettings,
    database: &DatabaseConnection,
    identity: Arc<Identity>,
    storage: Arc<LocalDirectory>,
) -> Result<LibraryState> {
    LibraryState::new(
        settings,
        LibraryInterfaces::builder()
            .caller(Arc::new(caller::AgentResolver {
                database: database.clone(),
                identity,
            }))
            .database(Arc::new(database::Transactions(database.clone())))
            .names(Arc::new(person_names::Names))
            .roles(Arc::new(space_roles::Roles))
            .storage(storage)
            .build(),
    )
}

mod caller;
mod database;
mod person_names;
mod space_roles;
