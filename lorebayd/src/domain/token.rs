use std::fs::{self, File};
use std::io::{ErrorKind, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use anyhow::{Context, Result, bail};
use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, Validation, decode, encode};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tempfile::Builder;
use typed_builder::TypedBuilder;

use super::random_id;

const SIGNING_KEY_FILE: &str = "signing.key";
const SIGNING_KEY_BYTES: usize = 32;
const TOKEN_ISSUER: &str = "lorebayd";
const TOKEN_AUDIENCE: &str = "lorebay-web-api";

#[derive(Clone, Debug, Deserialize, Serialize, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub(crate) struct Claims {
    pub sub: String,
    pub sid: String,
    pub kind: TokenKind,
    pub exp: u64,
    pub iat: u64,
    pub jti: String,
    pub iss: String,
    pub aud: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum TokenKind {
    AgentAuthorization,
    LoginSession,
}

#[derive(Clone, Debug, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub(crate) struct TokenSigner {
    key: Vec<u8>,
}

impl TokenSigner {
    pub fn load(directory: &Path) -> Result<Self> {
        let path = directory.join(SIGNING_KEY_FILE);
        let key = load_key(&path)
            .with_context(|| format!("could not load signing key {}", path.display()))?;
        Ok(Self { key })
    }

    pub fn sign(&self, username: &str, session: &str, expiry: u64, now: u64) -> Result<String> {
        let claims = Claims::builder()
            .sub(username)
            .sid(session)
            .kind(TokenKind::LoginSession)
            .exp(expiry)
            .iat(now)
            .jti(random_id()?)
            .iss(TOKEN_ISSUER)
            .aud(TOKEN_AUDIENCE)
            .build();
        Ok(encode(
            &Header::new(Algorithm::HS256),
            &claims,
            &EncodingKey::from_secret(&self.key)?,
        )?)
    }

    pub fn verify(&self, token: &str) -> Result<Claims> {
        let mut validation = Validation::new(Algorithm::HS256);
        validation.leeway = 0;
        validation.set_audience(&[TOKEN_AUDIENCE]);
        validation.set_issuer(&[TOKEN_ISSUER]);
        Ok(decode::<Claims>(token, &DecodingKey::from_secret(&self.key)?, &validation)?.claims)
    }
}

pub(crate) fn token_hash(token: &str) -> String {
    Sha256::digest(token.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn load_key(path: &Path) -> Result<Vec<u8>> {
    if !path.try_exists()? {
        create_key(path)?;
    }
    if fs::metadata(path)?.permissions().mode() & 0o077 != 0 {
        bail!("signing key must be readable only by its owner (mode 0600)");
    }
    let key = fs::read(path)?;
    if key.len() != SIGNING_KEY_BYTES {
        bail!("signing key must contain 32 bytes");
    }
    Ok(key)
}

fn create_key(path: &Path) -> Result<()> {
    let directory = path
        .parent()
        .context("signing key has no parent directory")?;
    let mut temporary = Builder::new()
        .prefix(".signing-key-")
        .tempfile_in(directory)?;
    let mut key = [0; SIGNING_KEY_BYTES];
    getrandom::fill(&mut key)?;
    temporary.write_all(&key)?;
    temporary.as_file().sync_all()?;
    match temporary.persist_noclobber(path) {
        Ok(_) => {}
        Err(error) if error.error.kind() == ErrorKind::AlreadyExists => {}
        Err(error) => return Err(error.error.into()),
    }
    File::open(directory)?.sync_all()?;
    Ok(())
}
