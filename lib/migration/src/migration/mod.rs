use sea_orm_migration::prelude::*;

mod m20261125_000001_create_space_tables;

pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migration_table_name() -> DynIden {
        Alias::new("space_migration").into_iden()
    }
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![Box::new(m20261125_000001_create_space_tables::Migration)]
    }
}
