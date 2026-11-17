//! The Lorebay server.

mod api;
mod config;
mod domain;
#[expect(
    dead_code,
    reason = "sea-orm-cli emits tables and relations before all consumers exist"
)]
#[expect(
    clippy::arbitrary_source_item_ordering,
    reason = "sea-orm-cli emits column enums in database column order"
)]
mod entity;
mod migration;
mod repository;
mod server;

pub use config::Config;
pub use domain::password::hash_password;
pub use server::run;
