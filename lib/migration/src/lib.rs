#[cfg(feature = "sqlite")]
pub use entity_database::open_entity_database;
pub use migration::Migrator;

#[cfg(feature = "sqlite")]
mod entity_database;
mod migration;
