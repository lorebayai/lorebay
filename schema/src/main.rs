use std::env::args_os;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use sea_orm::{ConnectOptions, Database, DatabaseConnection};
use sea_orm_migration::MigratorTrait;

#[path = "../../lib/core/src/migration/mod.rs"]
mod core_migration;
#[path = "../../lorebayd/src/migration/mod.rs"]
mod product_migration;

#[tokio::main]
async fn main() -> Result<()> {
    let mut arguments = args_os().skip(1);
    let core = arguments.next().map(PathBuf::from);
    let product = arguments.next().map(PathBuf::from);
    let (Some(core), Some(product), None) = (core, product, arguments.next()) else {
        anyhow::bail!("Usage: lorebay-schema <core.sqlite3> <product.sqlite3>");
    };

    let database = open(&core).await?;
    core_migration::Migrator::up(&database, None).await?;
    database.close().await?;

    let database = open(&product).await?;
    core_migration::Migrator::up(&database, None).await?;
    product_migration::Migrator::up(&database, None).await?;
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
