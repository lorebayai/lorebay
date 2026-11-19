use std::path::Path;

use anyhow::{Context, Result};
use sea_orm_migration::sea_orm::{ConnectOptions, Database, DatabaseConnection};

/// Creates an empty SQLite database for entity generation with foreign keys enabled.
pub async fn open_entity_database(path: &Path) -> Result<DatabaseConnection> {
    anyhow::ensure!(
        !path.exists(),
        "Schema output already exists: {}",
        path.display()
    );
    let filename = path.to_path_buf();
    let mut options = ConnectOptions::new("sqlite://");
    options
        .max_connections(1)
        .map_sqlx_sqlite_opts(move |options| {
            options
                .filename(&filename)
                .create_if_missing(true)
                .foreign_keys(true)
        });
    Database::connect(options)
        .await
        .with_context(|| format!("Could not open {}", path.display()))
}
