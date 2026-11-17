use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{Context, Result, bail};
use serde::Deserialize;
use tokio::fs;
use typed_builder::TypedBuilder;

use super::password::{hash_password, validate_hash};

#[derive(Clone, Debug, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub(crate) struct Users {
    pub hashes: BTreeMap<String, String>,
    pub dummy_hash: String,
}

#[derive(Clone, Debug, Deserialize, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
#[serde(deny_unknown_fields)]
struct User {
    username: String,
    password_hash: String,
}

impl Users {
    pub async fn load(path: &Path) -> Result<Self> {
        let content = fs::read_to_string(path)
            .await
            .with_context(|| format!("could not read users_file {}", path.display()))?;
        Self::parse(&content).with_context(|| format!("invalid users_file {}", path.display()))
    }

    fn parse(content: &str) -> Result<Self> {
        let users: Vec<User> = serde_yaml_ng::from_str(content).map_err(|_| {
            anyhow::anyhow!("expected a YAML or JSON list of usernames and password_hash values")
        })?;
        let mut hashes = BTreeMap::new();
        for user in users {
            if user.username.is_empty() || user.username.trim() != user.username {
                bail!("username must be nonempty without leading or trailing whitespace");
            }
            validate_hash(&user.password_hash)?;
            if hashes.insert(user.username, user.password_hash).is_some() {
                bail!("username repeats in users_file");
            }
        }
        Ok(Self {
            hashes,
            dummy_hash: hash_password("unknown account password")?,
        })
    }
}
