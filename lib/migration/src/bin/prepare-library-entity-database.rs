use std::env::args_os;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use lorebay_lib_migration::Migrator as LibraryMigrator;
use sea_orm_migration::MigratorTrait;
use sea_orm_migration::sea_orm::{ConnectOptions, Database, DatabaseConnection};

#[tokio::main]
async fn main() -> Result<()> {
    let mut arguments = args_os().skip(1);
    let (Some(path), None) = (arguments.next(), arguments.next()) else {
        anyhow::bail!("Usage: prepare-library-entity-database <database.sqlite3>");
    };

    let database = open(&PathBuf::from(path)).await?;
    LibraryMigrator::up(&database, None).await?;
    database.close().await?;
    Ok(())
}

async fn open(path: &Path) -> Result<DatabaseConnection> {
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
