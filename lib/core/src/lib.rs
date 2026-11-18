//! Library operations for spaces, tasks, and artifacts.

pub use migration::Migrator;
pub use operation::*;
pub use storage::*;

mod domain;
#[expect(
    clippy::arbitrary_source_item_ordering,
    reason = "sea-orm-cli emits columns in database order"
)]
#[expect(
    clippy::enum_variant_names,
    reason = "sea-orm-cli names relations after the tables"
)]
mod entity;
mod migration;
mod operation;
mod repository;
mod storage;

pub use domain::*;
