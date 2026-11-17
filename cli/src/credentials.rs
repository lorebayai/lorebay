use std::env;
use std::fmt;
use std::fs::{self, File, OpenOptions, Permissions, TryLockError};
use std::io::{self, Write};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tempfile::NamedTempFile;
use tokio::time::{Instant, sleep};
use typed_builder::TypedBuilder;

use crate::error::{Failure, Result};

#[derive(Clone, Deserialize, Serialize, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
#[serde(transparent)]
pub(crate) struct Secret {
    pub value: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub(crate) struct Session {
    pub server_url: String,
    pub token_endpoint: String,
    pub access_token: Secret,
    pub refresh_token: Secret,
    pub expires_at: u64,
}

#[derive(Clone, Debug, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub(crate) struct Store {
    pub directory: PathBuf,
}

impl fmt::Debug for Secret {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("<redacted>")
    }
}

impl Store {
    pub fn from_environment() -> Result<Self> {
        let directory = match env::var_os("LOREBAY_HOME") {
            Some(path) if !path.is_empty() => PathBuf::from(path),
            Some(_) => {
                return Err(Failure::local(
                    "cli.credentials",
                    "LOREBAY_HOME must not be empty.",
                ));
            }
            None => env::var_os("HOME")
                .filter(|home| !home.is_empty())
                .map(PathBuf::from)
                .map(|home| home.join(".lorebay"))
                .ok_or_else(|| {
                    Failure::local(
                        "cli.credentials",
                        "Set HOME or LOREBAY_HOME to locate credentials.",
                    )
                })?,
        };
        Ok(Self::builder().directory(directory).build())
    }

    pub async fn lock(&self) -> Result<File> {
        fs::create_dir_all(&self.directory)?;
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .mode(PRIVATE_MODE)
            .open(self.directory.join("credentials.lock"))?;
        file.set_permissions(Permissions::from_mode(PRIVATE_MODE))?;
        let deadline = Instant::now() + LOCK_WAIT;
        loop {
            match file.try_lock() {
                Ok(()) => return Ok(file),
                Err(TryLockError::Error(error)) => return Err(error.into()),
                Err(TryLockError::WouldBlock) => {}
            }
            if Instant::now() >= deadline {
                return Err(Failure::local(
                    "cli.timeout",
                    "Timed out waiting for the credentials lock.",
                ));
            }
            sleep(LOCK_POLL).await;
        }
    }

    pub fn load(&self) -> Result<Option<Session>> {
        let file = match File::open(self.directory.join("credentials.json")) {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error.into()),
        };
        file.set_permissions(Permissions::from_mode(PRIVATE_MODE))?;
        serde_json::from_reader(file).map(Some).map_err(|_| {
            Failure::local(
                "cli.credentials",
                "Could not decode credentials.json; run `lorebay login` again.",
            )
        })
    }

    pub fn save(&self, session: &Session) -> Result<()> {
        let mut temporary = NamedTempFile::new_in(&self.directory)?;
        temporary
            .as_file()
            .set_permissions(Permissions::from_mode(PRIVATE_MODE))?;
        let bytes = serde_json::to_vec(session)
            .map_err(|_| Failure::local("cli.credentials", "Could not encode credentials."))?;
        temporary.write_all(&bytes)?;
        temporary.as_file().sync_all()?;
        temporary
            .persist(self.directory.join("credentials.json"))
            .map_err(|error| error.error)?;
        File::open(&self.directory)?.sync_all()?;
        Ok(())
    }

    pub fn remove(&self) -> Result<()> {
        match fs::remove_file(self.directory.join("credentials.json")) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(_) => Err(Failure::local(
                "cli.credentials",
                "The credentials could not be removed.",
            )),
        }
    }
}

pub(crate) fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

const PRIVATE_MODE: u32 = 0o600;
const LOCK_WAIT: Duration = Duration::from_secs(15);
const LOCK_POLL: Duration = Duration::from_millis(25);
