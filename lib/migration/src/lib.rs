#[cfg(feature = "entity-generation")]
pub use entity_database::open_entity_database;
pub use migration::Migrator;

#[cfg(feature = "entity-generation")]
mod entity_database;
mod migration;
