use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::PathBuf;

use lorebay_base::{ConfigError, ConfigSources, LoggingConfig, load_config};
use serde::{Deserialize, Serialize};
use typed_builder::TypedBuilder;
use url::Url;

const DEFAULT_ACCESS_TOKEN_LIFETIME_SECONDS: u64 = 900;
const DEFAULT_REFRESH_TOKEN_LIFETIME_SECONDS: u64 = 2_592_000;
const DEFAULT_LOGIN_RATE_LIMIT: u32 = 10;
const DEFAULT_LOGIN_RATE_WINDOW_SECONDS: u64 = 60;
const MAX_DURATION_SECONDS: u64 = 315_360_000;
const DEFAULT_PUBLIC_URL: &str = "http://127.0.0.1:8080";
const DEFAULT_REGISTRATION_RATE_LIMIT: u32 = 20;
const DEFAULT_REGISTRATION_RATE_WINDOW_SECONDS: u64 = 60;
const CONFIG_ENV_PREFIX: &str = "LOREBAYD";
const DEFAULT_DATA_DIRECTORY: &str = "data";
const DEFAULT_LISTEN_ADDRESS: SocketAddr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 8080);
const DEFAULT_REGISTRATION_CLIENT_NAME_MAX_BYTES: usize = 256;
const DEFAULT_REGISTRATION_REDIRECT_URI_LIMIT: usize = 4;
const DEFAULT_REGISTRATION_REDIRECT_URI_MAX_BYTES: usize = 2048;
const DEFAULT_REGISTRATION_BODY_LIMIT_BYTES: usize = 65_536;

#[derive(Clone, Debug, Deserialize, Serialize, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
#[serde(deny_unknown_fields)]
pub struct Config {
    #[builder(default = DEFAULT_LISTEN_ADDRESS)]
    pub listen_address: SocketAddr,
    #[builder(default = PathBuf::from(DEFAULT_DATA_DIRECTORY))]
    pub data_directory: PathBuf,
    #[builder(default = DEFAULT_PUBLIC_URL.to_owned())]
    pub public_url: String,
    #[builder(default = DEFAULT_REGISTRATION_RATE_LIMIT)]
    pub registration_rate_limit: u32,
    #[builder(default = DEFAULT_REGISTRATION_RATE_WINDOW_SECONDS)]
    pub registration_rate_window_seconds: u64,
    #[builder(default = None)]
    pub users_file: Option<PathBuf>,
    #[builder(default = DEFAULT_ACCESS_TOKEN_LIFETIME_SECONDS)]
    pub access_token_lifetime_seconds: u64,
    #[builder(default = DEFAULT_REFRESH_TOKEN_LIFETIME_SECONDS)]
    pub refresh_token_lifetime_seconds: u64,
    #[builder(default = DEFAULT_LOGIN_RATE_LIMIT)]
    pub login_rate_limit: u32,
    #[builder(default = DEFAULT_LOGIN_RATE_WINDOW_SECONDS)]
    pub login_rate_window_seconds: u64,
    #[builder(default = DEFAULT_REGISTRATION_CLIENT_NAME_MAX_BYTES)]
    pub registration_client_name_max_bytes: usize,
    #[builder(default = DEFAULT_REGISTRATION_REDIRECT_URI_LIMIT)]
    pub registration_redirect_uri_limit: usize,
    #[builder(default = DEFAULT_REGISTRATION_REDIRECT_URI_MAX_BYTES)]
    pub registration_redirect_uri_max_bytes: usize,
    #[builder(default = DEFAULT_REGISTRATION_BODY_LIMIT_BYTES)]
    pub registration_body_limit_bytes: usize,
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
        if self
            .users_file
            .as_ref()
            .is_none_or(|path| path.as_os_str().is_empty())
        {
            return Err(ConfigError::invalid(
                "users_file",
                "a nonempty file path is required",
            ));
        }
        for (key, value) in [
            (
                "access_token_lifetime_seconds",
                self.access_token_lifetime_seconds,
            ),
            (
                "refresh_token_lifetime_seconds",
                self.refresh_token_lifetime_seconds,
            ),
            ("login_rate_window_seconds", self.login_rate_window_seconds),
        ] {
            if !(1..=MAX_DURATION_SECONDS).contains(&value) {
                return Err(ConfigError::invalid(
                    key,
                    "must be between 1 and 315360000 seconds",
                ));
            }
        }
        if self.login_rate_limit == 0 {
            return Err(ConfigError::invalid(
                "login_rate_limit",
                "must be greater than zero",
            ));
        }
        self.validate_registration()?;
        self.logging.validate()
    }

    fn validate_registration(&self) -> Result<(), ConfigError> {
        self.validate_public_url()?;
        self.validate_registration_limits()?;
        if self.registration_rate_limit == 0 {
            return Err(ConfigError::invalid(
                "registration_rate_limit",
                "must be positive",
            ));
        }
        if self.registration_rate_window_seconds == 0 {
            return Err(ConfigError::invalid(
                "registration_rate_window_seconds",
                "must be positive",
            ));
        }
        Ok(())
    }

    fn validate_registration_limits(&self) -> Result<(), ConfigError> {
        for (key, value) in [
            (
                "registration_client_name_max_bytes",
                self.registration_client_name_max_bytes,
            ),
            (
                "registration_redirect_uri_limit",
                self.registration_redirect_uri_limit,
            ),
            (
                "registration_redirect_uri_max_bytes",
                self.registration_redirect_uri_max_bytes,
            ),
            (
                "registration_body_limit_bytes",
                self.registration_body_limit_bytes,
            ),
        ] {
            if value == 0 {
                return Err(ConfigError::invalid(key, "must be positive"));
            }
        }
        Ok(())
    }

    fn validate_public_url(&self) -> Result<(), ConfigError> {
        let invalid = || {
            ConfigError::invalid(
                "public_url",
                "expected an HTTP(S) origin without credentials, path, query or fragment",
            )
        };
        let url = Url::parse(&self.public_url).map_err(|_| invalid())?;
        if !matches!(url.scheme(), "http" | "https")
            || !url.has_host()
            || !url.username().is_empty()
            || url.password().is_some()
            || url.path() != "/"
            || url.query().is_some()
            || url.fragment().is_some()
            || self.public_url != url.origin().ascii_serialization()
                && self.public_url != format!("{}/", url.origin().ascii_serialization())
        {
            return Err(invalid());
        }
        Ok(())
    }
}
