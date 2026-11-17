use std::collections::HashMap;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use config::{Config, ConfigError as LayerError, Environment, File, FileFormat};
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::{Map, Value};
use thiserror::Error;
use typed_builder::TypedBuilder;

use crate::dotenv::dotenv_environment;

#[derive(Clone, Debug, Default, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub struct ConfigSources {
    #[builder(default = None)]
    pub config_file: Option<PathBuf>,
    #[builder(default = None)]
    pub dotenv_file: Option<PathBuf>,
    pub env_prefix: String,
    #[builder(default = HashMap::new())]
    pub environment: HashMap<String, String>,
}

#[derive(Clone, Debug, Error)]
pub enum ConfigError {
    #[error("could not parse env file {path} at line {line}")]
    EnvParse { path: PathBuf, line: usize },
    #[error("invalid configuration key {key}: {reason}")]
    Invalid { key: String, reason: &'static str },
    #[error("could not parse JSON file {path} at line {line}, column {column}")]
    JsonParse {
        path: PathBuf,
        line: usize,
        column: usize,
    },
    #[error("could not read file {path}: {kind:?}")]
    Read { path: PathBuf, kind: ErrorKind },
    #[error("unknown configuration key {key}")]
    Unknown { key: String },
}

impl ConfigError {
    pub fn invalid(key: impl Into<String>, reason: &'static str) -> Self {
        Self::Invalid {
            key: key.into(),
            reason,
        }
    }
}

/// Loads defaults < JSON file < dotenv < supplied environment without changing process state.
pub fn load_config<T>(sources: &ConfigSources) -> Result<T, ConfigError>
where
    T: Default + Serialize + DeserializeOwned,
{
    let defaults = serde_json::to_string(&T::default())
        .map_err(|_| ConfigError::invalid("defaults", "could not serialize defaults"))?;
    let mut builder = Config::builder().add_source(File::from_str(&defaults, FileFormat::Json));
    if let Some(path) = &sources.config_file {
        builder = builder.add_source(File::from_str(&read_json_file(path)?, FileFormat::Json));
    }
    let mut environment = dotenv_environment(sources.dotenv_file.as_deref(), &sources.environment)?;
    environment.extend(sources.environment.clone());
    let values = builder
        .add_source(
            Environment::with_prefix(&sources.env_prefix)
                .prefix_separator(ENV_SEPARATOR)
                .separator(ENV_SEPARATOR)
                .try_parsing(true)
                .source(Some(environment)),
        )
        .build()
        .map_err(|_| {
            ConfigError::invalid("configuration", "could not merge configuration layers")
        })?;
    serde_path_to_error::deserialize(values).map_err(|error| {
        let key = error.path().to_string();
        if is_unknown_key(error.inner()) {
            ConfigError::Unknown { key }
        } else {
            ConfigError::invalid(key, "value has an invalid type or format")
        }
    })
}

pub(crate) fn read_config_file(path: &Path) -> Result<String, ConfigError> {
    fs::read_to_string(path).map_err(|error| ConfigError::Read {
        path: path.to_owned(),
        kind: error.kind(),
    })
}

const ENV_SEPARATOR: &str = "__";

fn is_unknown_key(error: &LayerError) -> bool {
    match error {
        LayerError::At { error, .. } => is_unknown_key(error),
        LayerError::Message(message) => message.starts_with("unknown field "),
        _ => false,
    }
}

fn read_json_file(path: &Path) -> Result<String, ConfigError> {
    let content = read_config_file(path)?;
    serde_json::from_str::<Map<String, Value>>(&content).map_err(|error| {
        ConfigError::JsonParse {
            path: path.to_owned(),
            line: error.line(),
            column: error.column(),
        }
    })?;
    Ok(content)
}
