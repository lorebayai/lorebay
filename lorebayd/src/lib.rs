//! The Lorebay server.

pub use config::Config;
pub use domain::password::hash_password;
pub use server::run;

mod api;
mod config;
mod domain;
#[expect(
    dead_code,
    reason = "sea-orm-cli emits relations before all consumers exist"
)]
#[expect(
    clippy::arbitrary_source_item_ordering,
    reason = "sea-orm-cli emits column enums in database column order"
)]
mod entity;
mod files;
mod library;
mod oauth;
mod pages;
mod repository;
mod server;
mod storage;
mod worker;
