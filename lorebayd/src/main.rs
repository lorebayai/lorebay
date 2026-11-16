use std::collections::HashMap;
use std::env;
use std::future::Future;
use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::Parser;
use lorebay_base::{ConfigError, init_logging};
use lorebayd::{Config, run};
use tokio::net::TcpListener;
use tokio::signal::unix::{SignalKind, signal as unix_signal};
use typed_builder::TypedBuilder;

const CONFIG_ENV_PREFIX: &str = "LOREBAYD__";

#[derive(Clone, Debug, Parser, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
#[command(version, about = "Run the Lorebay server")]
struct Args {
    /// JSON configuration file.
    #[arg(long)]
    config: Option<PathBuf>,
    /// Env file loaded before process environment overrides.
    #[arg(long)]
    dotenv: Option<PathBuf>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    let config = Config::load(args.config, args.dotenv, configuration_environment()?)?;
    init_logging(&config.logging)?;
    let shutdown = shutdown_signal()?;
    let listener = TcpListener::bind(config.listen_address)
        .await
        .context("could not bind listen_address")?;
    run(config, listener, shutdown).await
}

fn shutdown_signal() -> Result<impl Future<Output = ()> + Send> {
    let mut interrupt =
        unix_signal(SignalKind::interrupt()).context("could not register SIGINT")?;
    let mut terminate =
        unix_signal(SignalKind::terminate()).context("could not register SIGTERM")?;
    Ok(async move {
        tokio::select! {
            _ = interrupt.recv() => {},
            _ = terminate.recv() => {},
        }
    })
}

fn configuration_environment() -> Result<HashMap<String, String>, ConfigError> {
    env::vars_os()
        .filter_map(|(key, value)| {
            let key = key.to_string_lossy().into_owned();
            if !key.starts_with(CONFIG_ENV_PREFIX) {
                return None;
            }
            Some(
                value
                    .into_string()
                    .map(|value| (key.clone(), value))
                    .map_err(|_| ConfigError::invalid(key, "value must be valid UTF-8")),
            )
        })
        .collect()
}
