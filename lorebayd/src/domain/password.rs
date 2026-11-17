use anyhow::{Result, anyhow};
use argon2::Argon2;
use argon2::password_hash::{
    Error as PasswordError, PasswordHasher, PasswordVerifier, phc::PasswordHash,
};

/// Produces an Argon2id PHC string with a fresh random salt.
pub fn hash_password(password: &str) -> Result<String> {
    Argon2::default()
        .hash_password(password.as_bytes())
        .map(|hash| hash.to_string())
        .map_err(|_| anyhow!("could not hash password"))
}

pub(crate) fn verify_password(password: &str, hash: &str) -> bool {
    Argon2::default()
        .verify_password(password.as_bytes(), hash)
        .is_ok()
}

pub(crate) fn validate_hash(hash: &str) -> Result<()> {
    let parsed = PasswordHash::new(hash)
        .map_err(|_| anyhow!("password_hash must be an Argon2id PHC hash"))?;
    if parsed.algorithm.as_str() != "argon2id" || parsed.version != Some(19) {
        return Err(anyhow!("password_hash must use Argon2id version 19"));
    }
    match Argon2::default().verify_password(b"validate users file hash", &parsed) {
        Ok(()) | Err(PasswordError::PasswordInvalid) => Ok(()),
        Err(_) => Err(anyhow!(
            "password_hash is not a verifiable Argon2id PHC hash"
        )),
    }
}
