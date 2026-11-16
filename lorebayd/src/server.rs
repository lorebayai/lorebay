use std::future::Future;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use anyhow::{Context, Result};
use axum::Router;
use axum::extract::State;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use hyper::server::conn::http1;
use hyper_util::rt::TokioIo;
use hyper_util::service::TowerToHyperService;
use tokio::fs;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::watch;
use tokio::task::JoinSet;
use tokio::time::{sleep, timeout};

use crate::Config;
use crate::api::{self, ApiError, Envelope};

/// Readiness remains observable for one second before connections stop being accepted.
const READINESS_PROPAGATION: Duration = Duration::from_secs(1);
/// Total shutdown budget, including readiness propagation, measured from the signal.
const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(10);
const HEALTH_PATH: &str = "/healthz";
const READINESS_PATH: &str = "/readyz";
const API_PREFIX: &str = "/api/v1";
const ACCEPT_RETRY_DELAY: Duration = Duration::from_millis(100);

/// Creates the data directory and serves HTTP; the caller owns binding and shutdown signals.
pub async fn run(
    config: Config,
    listener: TcpListener,
    shutdown: impl Future<Output = ()> + Send,
) -> Result<()> {
    config.validate()?;
    fs::create_dir_all(&config.data_directory)
        .await
        .context("could not create data_directory")?;
    let ready = Arc::new(AtomicBool::new(true));
    let (stop, stopped) = watch::channel(false);
    let server = serve(listener, router(ready.clone()), stopped);
    tokio::pin!(server);
    tracing::info!("lorebayd ready");
    tokio::select! {
        result = &mut server => return result,
        () = shutdown => {},
    }
    ready.store(false, Ordering::Release);
    tracing::info!("shutdown begun; readiness disabled");
    let drain = async {
        tokio::select! {
            result = &mut server => return result,
            () = sleep(READINESS_PROPAGATION) => {},
        }
        let _ = stop.send(true);
        server.await
    };
    match timeout(SHUTDOWN_TIMEOUT, drain).await {
        Ok(result) => result?,
        Err(_) => tracing::warn!("shutdown deadline reached; closing remaining connections"),
    }
    tracing::info!("lorebayd stopped");
    Ok(())
}

fn router(ready: Arc<AtomicBool>) -> Router {
    Router::new()
        .route(HEALTH_PATH, get(health))
        .route(READINESS_PATH, get(readiness))
        .with_state(ready)
        .nest(API_PREFIX, api::router())
        .fallback(api::not_found)
        .method_not_allowed_fallback(|| async { ApiError::MethodNotAllowed })
}

async fn health() -> Envelope<&'static str> {
    Envelope::builder().data("ok").build()
}

async fn readiness(State(ready): State<Arc<AtomicBool>>) -> Response {
    if ready.load(Ordering::Acquire) {
        health().await.into_response()
    } else {
        ApiError::Unavailable.into_response()
    }
}

async fn serve(
    listener: TcpListener,
    router: Router,
    mut stopped: watch::Receiver<bool>,
) -> Result<()> {
    let mut connections = JoinSet::new();
    tracing::info!(listen_address = %listener.local_addr()?, "lorebayd listening");
    loop {
        tokio::select! {
            biased;
            _ = stopped.changed() => break,
            Some(result) = connections.join_next() => {
                if let Err(error) = result {
                    tracing::error!(%error, "HTTP connection task failed");
                }
            },
            result = listener.accept() => {
                match result {
                    Ok((stream, _)) => {
                        connections.spawn(serve_connection(stream, router.clone(), stopped.clone()));
                    },
                    Err(error) => {
                        tracing::error!(%error, "could not accept HTTP connection");
                        sleep(ACCEPT_RETRY_DELAY).await;
                    },
                }
            },
        }
    }
    drop(listener);
    while let Some(result) = connections.join_next().await {
        if let Err(error) = result {
            tracing::error!(%error, "HTTP connection task failed during shutdown");
        }
    }
    Ok(())
}

async fn serve_connection(stream: TcpStream, router: Router, mut stopped: watch::Receiver<bool>) {
    let connection = http1::Builder::new()
        .serve_connection(TokioIo::new(stream), TowerToHyperService::new(router));
    tokio::pin!(connection);
    let result = tokio::select! {
        result = &mut connection => result,
        _ = stopped.changed() => {
            connection.as_mut().graceful_shutdown();
            connection.await
        },
    };
    if result.is_err() {
        tracing::debug!("HTTP connection closed before completing its exchange");
    }
}
