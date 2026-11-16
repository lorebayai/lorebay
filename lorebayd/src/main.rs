use std::env;
use std::future::Future;
use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::Parser;
use lorebay_base::init_logging;
use lorebayd::{Config, run};
use tokio::net::TcpListener;
#[cfg(not(unix))]
use tokio::signal;
#[cfg(unix)]
use tokio::signal::unix::{SignalKind, signal as unix_signal};
use typed_builder::TypedBuilder;

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
    let config = Config::load(args.config, args.dotenv, env::vars())?;
    init_logging(&config.logging)?;
    let shutdown = shutdown_signal()?;
    let listener = TcpListener::bind(config.listen_address)
        .await
        .context("could not bind listen_address")?;
    run(config, listener, shutdown).await
}

#[cfg(unix)]
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

#[cfg(not(unix))]
fn shutdown_signal() -> Result<impl Future<Output = ()> + Send> {
    Ok(async {
        if let Err(error) = signal::ctrl_c().await {
            tracing::error!(%error, "could not wait for interrupt");
        }
    })
}
