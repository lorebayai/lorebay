use std::io;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing::level_filters::LevelFilter;
use tracing_subscriber::util::SubscriberInitExt;
use typed_builder::TypedBuilder;

use crate::ConfigError;

#[derive(Clone, Debug, Deserialize, Serialize, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
#[serde(deny_unknown_fields)]
pub struct LoggingConfig {
    #[builder(default = DEFAULT_LOG_LEVEL.to_owned())]
    pub level: String,
}

#[derive(Debug, Error)]
pub enum LoggingError {
    #[error(transparent)]
    Config(#[from] ConfigError),
    #[error("could not install logging subscriber")]
    Install,
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self::builder().build()
    }
}

impl LoggingConfig {
    pub fn validate(&self) -> Result<(), ConfigError> {
        self.filter().map(|_| ())
    }

    fn filter(&self) -> Result<LevelFilter, ConfigError> {
        LevelFilter::from_str(&self.level).map_err(|_| {
            ConfigError::invalid(
                "logging.level",
                "expected off, error, warn, info, debug or trace",
            )
        })
    }
}

/// Installs JSON logging on stderr; it reads no environment variables.
pub fn init_logging(config: &LoggingConfig) -> Result<(), LoggingError> {
    tracing_subscriber::fmt()
        .json()
        .with_ansi(false)
        .with_max_level(config.filter()?)
        .with_writer(io::stderr)
        .finish()
        .try_init()
        .map_err(|_| LoggingError::Install)
}

const DEFAULT_LOG_LEVEL: &str = "info";
