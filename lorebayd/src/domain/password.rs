use anyhow::{Result, anyhow};
use argon2::password_hash::{
    Error as PasswordError, PasswordHasher, PasswordVerifier, phc::PasswordHash,
};
use argon2::{Argon2, Params};

/// Produces an Argon2id PHC string with a fresh random salt.
pub fn hash_password(password: &str) -> Result<String> {
    Argon2::default()
        .hash_password(password.as_bytes())
        .map(|hash| hash.to_string())
        .map_err(|_| anyhow!("could not hash password"))
}

pub(crate) fn verify_password(password: &str, hash: &str) -> Result<bool> {
    match Argon2::default().verify_password(password.as_bytes(), hash) {
        Ok(()) => Ok(true),
        Err(PasswordError::PasswordInvalid) => Ok(false),
        Err(error) => Err(error.into()),
    }
}

pub(crate) fn validate_hash(hash: &str) -> Result<Params> {
    let parsed = PasswordHash::new(hash)
        .map_err(|_| anyhow!("password_hash must be an Argon2id PHC hash"))?;
    if parsed.algorithm.as_str() != "argon2id" || parsed.version != Some(19) {
        return Err(anyhow!("password_hash must use Argon2id version 19"));
    }
    if parsed.salt.is_none() || parsed.hash.is_none() {
        return Err(anyhow!("password_hash must contain a salt and digest"));
    }
    let parameters = Params::try_from(&parsed)
        .map_err(|_| anyhow!("password_hash has invalid Argon2 parameters"))?;
    match Argon2::default().verify_password(b"validate users file hash", &parsed) {
        Ok(()) | Err(PasswordError::PasswordInvalid) => Ok(parameters),
        Err(_) => Err(anyhow!(
            "password_hash is not a verifiable Argon2id PHC hash"
        )),
    }
}
