use std::env::args_os;
use std::path::PathBuf;

use anyhow::Result;
use lorebay_lib_migration::{Migrator as LibraryMigrator, open_entity_database};
use lorebayd_migration::Migrator as ProductMigrator;
use sea_orm_migration::MigratorTrait;

#[tokio::main]
async fn main() -> Result<()> {
    let mut arguments = args_os().skip(1);
    let (Some(path), None) = (arguments.next(), arguments.next()) else {
        anyhow::bail!("Usage: prepare-lorebay-entity-database <database.sqlite3>");
    };

    let database = open_entity_database(&PathBuf::from(path)).await?;
    LibraryMigrator::up(&database, None).await?;
    ProductMigrator::up(&database, None).await?;
    database.close().await?;
    Ok(())
}
