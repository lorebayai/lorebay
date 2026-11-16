//! The Lorebay server.

mod api;
mod config;
#[expect(dead_code, reason = "sea-orm-cli entities have no table consumers yet")]
#[expect(
    clippy::arbitrary_source_item_ordering,
    reason = "sea-orm-cli emits column enums in database column order"
)]
mod entity;
mod migration;
mod server;

pub use config::Config;
pub use server::run;
