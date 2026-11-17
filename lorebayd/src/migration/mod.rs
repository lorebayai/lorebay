use sea_orm_migration::prelude::*;

mod m20261116_000001_create_tables;
mod m20261117_000001_add_previous_refresh_token_hash;

pub(crate) struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m20261116_000001_create_tables::Migration),
            Box::new(m20261117_000001_add_previous_refresh_token_hash::Migration),
        ]
    }
}
