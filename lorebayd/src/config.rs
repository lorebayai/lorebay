use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::PathBuf;

use lorebay_base::{ConfigError, ConfigSources, LoggingConfig, load_config};
use serde::{Deserialize, Serialize};
use typed_builder::TypedBuilder;

const CONFIG_ENV_PREFIX: &str = "LOREBAYD";
const DEFAULT_DATA_DIRECTORY: &str = "data";
const DEFAULT_LISTEN_ADDRESS: SocketAddr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 8080);

#[derive(Clone, Debug, Deserialize, Serialize, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
#[serde(deny_unknown_fields)]
pub struct Config {
    #[builder(default = DEFAULT_LISTEN_ADDRESS)]
    pub listen_address: SocketAddr,
    #[builder(default = PathBuf::from(DEFAULT_DATA_DIRECTORY))]
    pub data_directory: PathBuf,
    #[builder(default = LoggingConfig::default())]
    pub logging: LoggingConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self::builder().build()
    }
}

impl Config {
    /// Loads and validates defaults < JSON file < dotenv < supplied environment.
    pub fn load(
        config_file: Option<PathBuf>,
        dotenv_file: Option<PathBuf>,
        environment: impl IntoIterator<Item = (String, String)>,
    ) -> Result<Self, ConfigError> {
        let config: Self = load_config(
            &ConfigSources::builder()
                .config_file(config_file)
                .dotenv_file(dotenv_file)
                .env_prefix(CONFIG_ENV_PREFIX)
                .environment(environment.into_iter().collect::<HashMap<_, _>>())
                .build(),
        )?;
        config.validate()?;
        Ok(config)
    }

    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.data_directory.as_os_str().is_empty() {
            return Err(ConfigError::invalid("data_directory", "must not be empty"));
        }
        self.logging.validate()
    }
}
