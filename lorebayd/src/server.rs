use std::future::Future;
use std::num::NonZeroU32;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use anyhow::{Context, Result};
use axum::extract::State;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Extension, Router};
use hyper::server::conn::http1;
use hyper_util::rt::TokioIo;
use hyper_util::service::TowerToHyperService;
use lorebay_base::{RateLimiter, SystemClock};
use lorebay_lib_core::LibraryState;
use lorebay_lib_migration::Migrator as LibraryMigrator;
use lorebayd_migration::Migrator as ProductMigrator;
use sea_orm::{
    ConnectOptions, Database, DatabaseConnection, DatabaseTransaction, EntityName, EntityTrait,
    SqliteTransactionMode, TransactionOptions, TransactionTrait,
};
use sea_orm_migration::{MigratorTrait, SchemaManager, seaql_migrations};
use tokio::fs;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::watch;
use tokio::task::JoinSet;
use tokio::time::{sleep, timeout};
use typed_builder::TypedBuilder;

use crate::Config;
use crate::api::{self, ApiError, Envelope};
use crate::domain::account;
use crate::domain::login_session::Identity;
use crate::domain::token::TokenSigner;
use crate::domain::user_store::UserStore;
use crate::domain::users::FileUserStore;
use crate::oauth::{self, OAuthState};
use crate::storage::LocalDirectory;
use crate::time::now_millis;
use crate::{files, library, worker};

#[derive(Clone, Debug, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub(crate) struct ServerState {
    pub database: DatabaseConnection,
    pub storage: Arc<LocalDirectory>,
    pub identity: Arc<Identity>,
    pub library: LibraryState,
    ready: Arc<AtomicBool>,
    oauth: OAuthState,
}

/// Applies pending database migrations before serving; the caller owns binding and shutdown signals.
pub async fn run(
    config: Config,
    listener: TcpListener,
    shutdown: impl Future<Output = ()> + Send,
) -> Result<()> {
    config.validate()?;
    let users_path = config
        .users_file
        .as_deref()
        .context("users_file path is required")?;
    let users: Arc<dyn UserStore> = Arc::new(FileUserStore::load(users_path).await?);
    fs::create_dir_all(&config.data_directory)
        .await
        .context("could not create data_directory")?;
    let database = open_database(&config.data_directory).await?;
    account::sync(&database, users.as_ref(), now_millis()?).await?;
    let identity = Arc::new(identity(config.clone(), users)?);
    let settings = library::settings(&config);
    let storage = Arc::new(LocalDirectory::open(&config, settings.upload_size_limit).await?);
    let library = library::state(settings, &database, identity.clone(), storage.clone())?;
    let state = ServerState::builder()
        .library(library)
        .storage(storage)
        .oauth(OAuthState::new(
            &config,
            database.clone(),
            identity.clone(),
        )?)
        .identity(identity)
        .database(database)
        .ready(Arc::new(AtomicBool::new(true)))
        .build();
    let result = serve_until_shutdown(listener, state.clone(), shutdown).await;
    state
        .database
        .close()
        .await
        .context("could not close database")?;
    result
}

/// Readiness remains observable for one second before connections stop being accepted.
const READINESS_PROPAGATION: Duration = Duration::from_secs(1);
/// Total shutdown budget, including readiness propagation, measured from the signal.
const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(10);
const HEALTH_PATH: &str = "/healthz";
const READINESS_PATH: &str = "/readyz";
const API_PREFIX: &str = "/api/v1";
const ACCEPT_RETRY_DELAY: Duration = Duration::from_millis(100);
const DATABASE_FILE: &str = "lorebay.sqlite3";
const EARLIER_SCHEMA_MIGRATION: &str = "m20261116_000001_create_tables";
const SQLITE_BUSY_TIMEOUT: Duration = Duration::from_secs(5);

fn identity(config: Config, users: Arc<dyn UserStore>) -> Result<Identity> {
    let limit =
        NonZeroU32::new(config.login_rate_limit).context("login_rate_limit must be positive")?;
    Ok(Identity::builder()
        .user_store(users)
        .signer(TokenSigner::load(
            &config.data_directory,
            &config.public_url,
        )?)
        .limiter(RateLimiter::new(
            limit,
            Duration::from_secs(config.login_rate_window_seconds),
            Arc::new(SystemClock),
        ))
        .device_limiter(RateLimiter::new(
            NonZeroU32::new(config.device_rate_limit)
                .context("device_rate_limit must be positive")?,
            Duration::from_secs(config.device_rate_window_seconds),
            Arc::new(SystemClock),
        ))
        .device_verification_limiter(RateLimiter::new(
            NonZeroU32::new(config.device_verification_rate_limit)
                .context("device_verification_rate_limit must be positive")?,
            Duration::from_secs(config.device_verification_rate_window_seconds),
            Arc::new(SystemClock),
        ))
        .config(config)
        .build())
}

async fn open_database(directory: &Path) -> Result<DatabaseConnection> {
    let path = directory.join(DATABASE_FILE);
    let filename = path.clone();
    let mut options = ConnectOptions::new("sqlite://");
    options
        .max_connections(1)
        .sqlx_logging(false)
        .map_sqlx_sqlite_opts(move |options| {
            options
                .filename(&filename)
                .create_if_missing(true)
                .foreign_keys(true)
                .busy_timeout(SQLITE_BUSY_TIMEOUT)
        });
    let database = Database::connect(options)
        .await
        .with_context(|| format!("could not open database {}", path.display()))?;
    let transaction = database
        .begin_with_options(TransactionOptions {
            sqlite_transaction_mode: Some(SqliteTransactionMode::Immediate),
            ..Default::default()
        })
        .await
        .context("could not begin database migration transaction")?;
    reject_earlier_data_directory(&transaction, directory).await?;
    LibraryMigrator::up(&transaction, None)
        .await
        .with_context(|| format!("could not migrate library database {}", path.display()))?;
    ProductMigrator::up(&transaction, None)
        .await
        .with_context(|| format!("could not migrate database {}", path.display()))?;
    transaction
        .commit()
        .await
        .context("could not commit database migrations")?;
    Ok(database)
}

async fn reject_earlier_data_directory(
    transaction: &DatabaseTransaction,
    directory: &Path,
) -> Result<()> {
    let schema = SchemaManager::new(transaction);
    if schema
        .has_table(seaql_migrations::Entity.table_name())
        .await?
        && seaql_migrations::Entity::find_by_id(EARLIER_SCHEMA_MIGRATION)
            .one(transaction)
            .await?
            .is_some()
    {
        anyhow::bail!(
            "Data directory {} was created by an earlier lorebayd build and must be recreated. Move it aside and restart with an empty data directory.",
            directory.display()
        );
    }
    Ok(())
}

async fn serve_until_shutdown(
    listener: TcpListener,
    state: ServerState,
    shutdown: impl Future<Output = ()> + Send,
) -> Result<()> {
    let (stop, stopped) = watch::channel(false);
    let (stop_workers, workers_stopped) = watch::channel(false);
    let mut workers = start_workers(&state, workers_stopped);
    let server = serve(listener, router(state.clone())?, stopped);
    tokio::pin!(server);
    tracing::info!("lorebayd ready");
    tokio::select! {
        biased;
        () = shutdown => {},
        result = &mut server => return result,
    }
    let _ = stop_workers.send(true);
    state.ready.store(false, Ordering::Release);
    tracing::info!("shutdown begun; readiness disabled");
    let drain = async {
        let drain_server = async {
            tokio::select! {
                result = &mut server => return result,
                () = sleep(READINESS_PROPAGATION) => {},
            }
            let _ = stop.send(true);
            server.await
        };
        let (result, ()) = tokio::join!(drain_server, finish_workers(&mut workers));
        result
    };
    match timeout(SHUTDOWN_TIMEOUT, drain).await {
        Ok(result) => result?,
        Err(_) => {
            tracing::warn!("shutdown deadline reached; closing remaining connections and workers");
            workers.abort_all();
            finish_workers(&mut workers).await;
        }
    }
    tracing::info!("lorebayd stopped");
    Ok(())
}

fn start_workers(state: &ServerState, stopped: watch::Receiver<bool>) -> JoinSet<()> {
    let config = &state.identity.config;
    let mut workers = JoinSet::new();
    workers.spawn(worker::remove_expired_credentials(
        state.database.clone(),
        Duration::from_secs(config.credential_cleanup_interval_seconds),
        Duration::from_secs(config.unused_client_lifetime_seconds),
        stopped.clone(),
    ));
    let interval = Duration::from_secs(config.artifact_cleanup_interval_seconds);
    workers.spawn(worker::remove_abandoned_artifacts(
        state.library.clone(),
        interval,
        stopped.clone(),
    ));
    workers.spawn(worker::remove_orphaned_files(
        state.library.clone(),
        interval,
        stopped.clone(),
    ));
    workers.spawn(worker::remove_local_leftovers(
        state.storage.clone(),
        interval,
        Duration::from_secs(config.orphaned_file_lifetime_seconds),
        stopped,
    ));
    workers
}

async fn finish_workers(workers: &mut JoinSet<()>) {
    while let Some(result) = workers.join_next().await {
        if let Err(error) = result
            && !error.is_cancelled()
        {
            tracing::error!(%error, "background job task failed");
        }
    }
}

fn router(state: ServerState) -> Result<Router> {
    let oauth = oauth::router(state.oauth.clone());
    let mcp = lorebay_lib_mcp::router(state.library.clone())?;
    Ok(Router::new()
        .route(HEALTH_PATH, get(health))
        .route(READINESS_PATH, get(readiness))
        .nest(API_PREFIX, api::router())
        .merge(files::router())
        .with_state(state)
        .merge(oauth)
        .merge(mcp)
        .fallback(api::not_found)
        .method_not_allowed_fallback(|| async { ApiError::MethodNotAllowed }))
}

async fn health() -> Envelope<&'static str> {
    Envelope::builder().data("ok").build()
}

async fn readiness(State(state): State<ServerState>) -> Response {
    if state.ready.load(Ordering::Acquire) {
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
                    Ok((stream, peer)) => {
                        let connection_router = router.clone().layer(Extension(peer));
                        connections.spawn(serve_connection(stream, connection_router, stopped.clone()));
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
