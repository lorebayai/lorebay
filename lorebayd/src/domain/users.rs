use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{Context, Result, anyhow, bail};
use serde::Deserialize;
use serde_path_to_error::{Error as PathError, Segment};
use serde_yaml_ng::{Deserializer, Value};
use tokio::fs;
use typed_builder::TypedBuilder;

use super::password::{hash_password, validate_hash, verify_password};
use super::user_store::UserStore;

#[derive(Clone, Debug, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub(crate) struct FileUserStore {
    hashes: BTreeMap<String, String>,
    dummy_hash: String,
}

#[derive(Clone, Debug, Deserialize, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
#[serde(deny_unknown_fields)]
struct User {
    username: String,
    password_hash: String,
}

impl UserStore for FileUserStore {
    fn usernames(&self) -> Result<Vec<String>> {
        Ok(self.hashes.keys().cloned().collect())
    }

    fn verify(&self, username: &str, password: &str) -> Result<bool> {
        let hash = self.hashes.get(username).unwrap_or(&self.dummy_hash);
        let matches = verify_password(password, hash)?;
        Ok(matches && self.hashes.contains_key(username))
    }
}

impl FileUserStore {
    pub(crate) async fn load(path: &Path) -> Result<Self> {
        let content = fs::read_to_string(path).await.with_context(|| {
            format!(
                "could not read users_file {} (entry and username unavailable)",
                path.display()
            )
        })?;
        Self::parse(&content).with_context(|| format!("invalid users_file {}", path.display()))
    }

    fn parse(content: &str) -> Result<Self> {
        let users: Vec<User> = serde_path_to_error::deserialize(Deserializer::from_str(content))
            .map_err(|error| parse_error(content, error))?;
        let mut hashes = BTreeMap::new();
        let mut parameters = None;
        for (index, user) in users.into_iter().enumerate() {
            let context = format!("entry {} (username {:?})", index + 1, user.username);
            validate_username(&user.username).with_context(|| context.clone())?;
            let current = validate_hash(&user.password_hash).with_context(|| context.clone())?;
            if parameters
                .as_ref()
                .is_some_and(|expected| expected != &current)
            {
                bail!("{context}: Argon2 parameters must match every other entry");
            }
            parameters = Some(current);
            if hashes.insert(user.username, user.password_hash).is_some() {
                bail!("{context}: username repeats in users_file");
            }
        }
        // A stored hash keeps unknown-user work identical when library defaults change.
        let dummy_hash = match hashes.values().next() {
            Some(hash) => hash.clone(),
            None => hash_password("unknown account password").context(
                "entry and username unavailable: could not prepare password verification",
            )?,
        };
        Ok(Self { hashes, dummy_hash })
    }
}

fn validate_username(username: &str) -> Result<()> {
    if username.is_empty() || username.trim() != username {
        bail!("username must be nonempty without leading or trailing whitespace");
    }
    Ok(())
}

fn parse_error(content: &str, error: PathError<serde_yaml_ng::Error>) -> anyhow::Error {
    let location = error.inner().location();
    let index = error.path().iter().find_map(|segment| match segment {
        Segment::Seq { index } => Some(*index),
        _ => None,
    });
    let entry_label = if index.is_some() {
        "entry"
    } else {
        "nearest readable entry"
    };
    let document = diagnostic_document(content, location.as_ref().map(|value| value.line()));
    let entries = document.as_ref().and_then(Value::as_sequence);
    let index = index.or_else(|| entries.and_then(|entries| entries.len().checked_sub(1)));
    let username = index.and_then(|index| entries?.get(index)?.get("username")?.as_str());
    let entry = index.map_or_else(|| "unavailable".to_owned(), |index| (index + 1).to_string());
    let username = username.map_or_else(|| "unavailable".to_owned(), |name| format!("{name:?}"));
    let position = location.map_or_else(
        || "line and column unavailable".to_owned(),
        |value| format!("line {}, column {}", value.line(), value.column()),
    );
    anyhow!(
        "{entry_label} {entry} (username {username}) at {position}: expected a YAML or JSON list of username and password_hash string fields"
    )
}

fn diagnostic_document(content: &str, line: Option<usize>) -> Option<Value> {
    if let Ok(document) = serde_yaml_ng::from_str(content) {
        return Some(document);
    }
    let lines: Vec<_> = content.lines().take(line?).collect();
    (1..lines.len())
        .rev()
        .find_map(|length| serde_yaml_ng::from_str(&lines[..length].join("\n")).ok())
}
