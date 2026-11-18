use sea_orm_migration::prelude::*;

mod m20261118_000001_create_product_tables;

pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![Box::new(m20261118_000001_create_product_tables::Migration)]
    }
}
