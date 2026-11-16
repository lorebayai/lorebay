//! The Lorebay server.

mod api;
mod config;
mod server;

pub use config::Config;
pub use server::run;
