use std::fs::{self, File, Permissions};
use std::io::{ErrorKind, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use anyhow::{Context, Result, bail};
use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, Validation, decode, encode};
use lorebay_lib_types::Secret;
use serde::{Deserialize, Serialize};
use tempfile::Builder;
use typed_builder::TypedBuilder;

use super::{authorization_server_metadata, random_id};

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum TokenKind {
    AgentSignIn,
    PersonSignIn,
}

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

#[derive(Clone, Debug, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub(crate) struct TokenSubject<'a> {
    pub username: &'a str,
    pub random_id: &'a str,
    pub kind: TokenKind,
}

#[derive(Clone, Debug, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub(crate) struct TokenSigner {
    key: Secret<Vec<u8>>,
    issuer: String,
}

impl TokenSigner {
    pub fn load(directory: &Path, public_url: &str) -> Result<Self> {
        let path = directory.join(SIGNING_KEY_FILE);
        let key = load_key(&path)
            .with_context(|| format!("could not load signing key {}", path.display()))?;
        Ok(Self {
            key,
            issuer: authorization_server_metadata(public_url).issuer,
        })
    }

    pub fn sign(&self, subject: TokenSubject<'_>, expiry: u64, now: u64) -> Result<Secret<String>> {
        let claims = Claims::builder()
            .sub(subject.username)
            .sid(subject.random_id)
            .kind(subject.kind)
            .exp(expiry)
            .iat(now)
            .jti(random_id()?)
            .iss(self.issuer.clone())
            .aud(TOKEN_AUDIENCE)
            .build();
        Ok(Secret::from(encode(
            &Header::new(Algorithm::HS256),
            &claims,
            &EncodingKey::from_secret(self.key.expose())?,
        )?))
    }

    pub fn verify(&self, token: &str) -> Result<Claims> {
        let mut validation = Validation::new(Algorithm::HS256);
        validation.leeway = 0;
        validation.set_audience(&[TOKEN_AUDIENCE]);
        validation.set_issuer(&[&self.issuer]);
        Ok(decode::<Claims>(
            token,
            &DecodingKey::from_secret(self.key.expose())?,
            &validation,
        )?
        .claims)
    }
}

const SIGNING_KEY_FILE: &str = "signing.key";
const SIGNING_KEY_BYTES: usize = 32;
const SIGNING_KEY_MODE: u32 = 0o600;
const TOKEN_AUDIENCE: &str = "lorebay-web-api";

fn load_key(path: &Path) -> Result<Secret<Vec<u8>>> {
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
    Ok(Secret::from(key))
}

fn create_key(path: &Path) -> Result<()> {
    let directory = path
        .parent()
        .context("signing key has no parent directory")?;
    let mut temporary = Builder::new()
        .prefix(".signing-key-")
        .tempfile_in(directory)?;
    temporary
        .as_file()
        .set_permissions(Permissions::from_mode(SIGNING_KEY_MODE))?;
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
