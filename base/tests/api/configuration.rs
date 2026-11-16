use std::collections::HashMap;
use std::fs;

use lorebay_base::{ConfigSources, load_config};
use serde::{Deserialize, Serialize};
use tempfile::TempDir;
use typed_builder::TypedBuilder;

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
struct Settings {
    pub port: u16,
    pub directory: String,
}

#[test]
fn load_config_applies_layers_in_order() {
    let directory = TempDir::new().unwrap();
    let config_file = directory.path().join("config.json");
    let dotenv_file = directory.path().join("settings.env");
    fs::write(&config_file, r#"{"port":8081,"directory":"file"}"#).unwrap();
    fs::write(
        &dotenv_file,
        "LOREBAY_TEST__PORT=8082\nLOREBAY_TEST__DIRECTORY=dotenv\n",
    )
    .unwrap();
    let mut sources = ConfigSources::builder().env_prefix("LOREBAY_TEST").build();
    assert_eq!(
        load_config::<Settings>(&sources).unwrap(),
        Settings::default()
    );
    sources.config_file = Some(config_file);
    assert_eq!(
        load_config::<Settings>(&sources).unwrap(),
        Settings::builder().port(8081_u16).directory("file").build()
    );
    sources.dotenv_file = Some(dotenv_file);
    assert_eq!(
        load_config::<Settings>(&sources).unwrap(),
        Settings::builder()
            .port(8082_u16)
            .directory("dotenv")
            .build()
    );
    sources.environment = HashMap::from([("LOREBAY_TEST__PORT".into(), "8083".into())]);
    assert_eq!(
        load_config::<Settings>(&sources).unwrap(),
        Settings::builder()
            .port(8083_u16)
            .directory("dotenv")
            .build()
    );
}

#[test]
fn load_config_fails_with_key_without_disclosing_value() {
    let directory = TempDir::new().unwrap();
    let config_file = directory.path().join("invalid.json");
    fs::write(&config_file, r#"{"port":"secret-invalid-value"}"#).unwrap();
    let sources = ConfigSources::builder()
        .env_prefix("LOREBAY_TEST")
        .config_file(Some(config_file))
        .build();
    let error = load_config::<Settings>(&sources).unwrap_err().to_string();
    assert_eq!(
        error,
        "invalid configuration key port: value has an invalid type or format"
    );
}
