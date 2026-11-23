use std::fs::{self, File, Permissions};
use std::io::{ErrorKind, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use anyhow::{Context, Result, bail};
use base64::Engine;
use base64::prelude::BASE64_URL_SAFE_NO_PAD;
use hmac::{Hmac, KeyInit, Mac};
use lorebay_lib_types::Secret;
use sha2::Sha256;
use tempfile::Builder;

use super::FileOperation;

pub(super) fn load_key(directory: &Path) -> Result<Secret<Vec<u8>>> {
    let path = directory.join("file-url-signing.key");
    if !path.try_exists()? {
        let mut temporary = Builder::new()
            .prefix(".file-url-key-")
            .tempfile_in(directory)?;
        temporary
            .as_file()
            .set_permissions(Permissions::from_mode(0o600))?;
        let mut bytes = [0; KEY_BYTES];
        getrandom::fill(&mut bytes)?;
        temporary.write_all(&bytes)?;
        temporary.as_file().sync_all()?;
        match temporary.persist_noclobber(&path) {
            Ok(_) => {}
            Err(error) if error.error.kind() == ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error.error.into()),
        }
        File::open(directory)?.sync_all()?;
    }
    if fs::metadata(&path)?.permissions().mode() & 0o077 != 0 {
        bail!("file-url-signing.key must be readable only by its owner (mode 0600)");
    }
    let key = fs::read(&path).context("could not read file-url-signing.key")?;
    if key.len() != KEY_BYTES {
        bail!("file-url-signing.key must contain 32 bytes");
    }
    Ok(Secret::from(key))
}

pub(super) fn sign(
    key: &[u8],
    operation: FileOperation<'_>,
    location: &str,
    expiry: i64,
) -> Result<String> {
    Ok(BASE64_URL_SAFE_NO_PAD.encode(
        tag(key, operation, location, expiry)?
            .finalize()
            .into_bytes(),
    ))
}

pub(super) fn verify(
    key: &[u8],
    operation: FileOperation<'_>,
    location: &str,
    expiry: i64,
    signature: &str,
) -> bool {
    let Ok(signature) = BASE64_URL_SAFE_NO_PAD.decode(signature) else {
        return false;
    };
    tag(key, operation, location, expiry).is_ok_and(|mac| mac.verify_slice(&signature).is_ok())
}

const KEY_BYTES: usize = 32;

fn tag(
    key: &[u8],
    operation: FileOperation<'_>,
    location: &str,
    expiry: i64,
) -> Result<Hmac<Sha256>> {
    let mut mac = Hmac::<Sha256>::new_from_slice(key)?;
    let (method, constraint) = match operation {
        FileOperation::Download(file_name) => ("GET", file_name),
        FileOperation::Upload => ("PUT", "If-None-Match: *"),
    };
    for field in [method, location, &expiry.to_string(), constraint] {
        mac.update(&(field.len() as u64).to_be_bytes());
        mac.update(field.as_bytes());
    }
    Ok(mac)
}
