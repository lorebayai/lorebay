use std::collections::HashMap;
use std::path::PathBuf;

use config::{Config, Environment, File, FileFormat};
use serde::Serialize;
use serde::de::DeserializeOwned;
use thiserror::Error;
use typed_builder::TypedBuilder;

const ENV_SEPARATOR: &str = "__";

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
    #[error("invalid configuration key {key}: {reason}")]
    Invalid { key: String, reason: &'static str },
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
        builder = builder.add_source(File::from(path.as_path()).format(FileFormat::Json));
    }
    let mut environment = dotenv_environment(sources)?;
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
        .map_err(|_| ConfigError::invalid("--config", "could not load JSON configuration"))?;
    serde_path_to_error::deserialize(values).map_err(|error| {
        ConfigError::invalid(
            error.path().to_string(),
            "value has an invalid type or format",
        )
    })
}

fn dotenv_environment(sources: &ConfigSources) -> Result<HashMap<String, String>, ConfigError> {
    let Some(path) = &sources.dotenv_file else {
        return Ok(HashMap::new());
    };
    dotenvy::from_path_iter(path)
        .map_err(|_| ConfigError::invalid("--dotenv", "could not read env file"))?
        .collect::<Result<_, _>>()
        .map_err(|_| ConfigError::invalid("--dotenv", "could not parse env file"))
}
