use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use lorebay_lib_core::{LibraryInterfaces, LibrarySettings, LibraryState};
use sea_orm::DatabaseConnection;

use crate::Config;
use crate::domain::MCP_PATH;
use crate::domain::login_session::Identity;
use crate::storage::LocalDirectory;

pub(crate) use space_roles::{parse_role, read_role};

pub(crate) const OWNER_ID: &str = "lorebay";

pub(crate) fn state(
    config: &Config,
    database: &DatabaseConnection,
    identity: Arc<Identity>,
    storage: Arc<LocalDirectory>,
) -> Result<LibraryState> {
    let origin = config.public_url.trim_end_matches('/').to_owned();
    let settings = LibrarySettings {
        public_url: origin.clone(),
        mcp_path: MCP_PATH.to_owned(),
        server_name: "lorebayd".to_owned(),
        server_version: env!("CARGO_PKG_VERSION").to_owned(),
        authorization_server: origin,
        upload_size_limit: config.upload_size_limit_bytes,
        upload_url_lifetime: Duration::from_secs(config.upload_url_lifetime_seconds),
        download_url_lifetime: Duration::from_secs(config.upload_url_lifetime_seconds),
        inline_content_types: [
            "image/gif",
            "image/jpeg",
            "image/png",
            "image/webp",
            "application/pdf",
        ]
        .map(str::to_owned)
        .to_vec(),
        abandoned_artifact_lifetime: Duration::from_secs(
            config.abandoned_artifact_lifetime_seconds,
        ),
        orphaned_file_lifetime: Duration::from_secs(config.orphaned_file_lifetime_seconds),
    };
    LibraryState::new(
        settings,
        LibraryInterfaces {
            caller: Arc::new(caller::AgentResolver {
                database: database.clone(),
                identity,
            }),
            database: Arc::new(database::Transactions(database.clone())),
            names: Arc::new(person_names::Names),
            roles: Arc::new(space_roles::Roles),
            storage,
        },
    )
}

mod caller;
mod database;
mod person_names;
mod space_roles;
