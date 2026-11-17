use std::collections::BTreeMap;
use std::io::{Error, ErrorKind};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};
use futures_util::{Stream, StreamExt};
use lorebay_base::Secret;
use tempfile::{Builder, TempPath};
use thiserror::Error;
use tokio::fs::{self, File};
use tokio::io::AsyncWriteExt;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use typed_builder::TypedBuilder;
use uuid::Uuid;

use crate::Config;

use super::signature::{load_key, sign, verify};
use super::{FileStorage, UploadRequest};

#[derive(Debug, Error)]
pub(crate) enum UploadError {
    #[error("A file already exists at this location")]
    AlreadyExists,
    #[error("The upload was interrupted")]
    Interrupted,
    #[error(transparent)]
    Io(#[from] Error),
    #[error("The upload exceeds the size limit")]
    TooLarge,
    #[error("Too many uploads are in progress")]
    TooMany,
    #[error("The upload URL is expired")]
    UrlExpired,
    #[error("The upload URL is invalid")]
    UrlInvalid,
}

#[derive(Clone, Debug, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub(crate) struct LocalDirectory {
    files: PathBuf,
    temporary: PathBuf,
    key: Secret<Vec<u8>>,
    origin: String,
    lifetime_millis: i64,
    pub size_limit: u64,
    slots: Arc<Semaphore>,
}

impl LocalDirectory {
    pub async fn open(config: &Config) -> Result<Self> {
        let files = config.data_directory.join("artifacts");
        let temporary = config.data_directory.join("uploads");
        fs::create_dir_all(&files).await?;
        fs::create_dir_all(&temporary).await?;
        File::open(&config.data_directory).await?.sync_all().await?;
        Ok(Self::builder()
            .files(files)
            .temporary(temporary)
            .key(load_key(&config.data_directory)?)
            .origin(config.public_url.trim_end_matches('/'))
            .lifetime_millis(i64::try_from(config.upload_url_lifetime_seconds)? * 1000)
            .size_limit(config.upload_size_limit_bytes)
            .slots(Arc::new(Semaphore::new(
                config.upload_concurrency_limit as usize,
            )))
            .build())
    }

    pub fn authorize(
        &self,
        location: &str,
        expiry: i64,
        signature: &str,
    ) -> Result<(), UploadError> {
        if !valid_location(location) || !verify(self.key.expose(), location, expiry, signature) {
            return Err(UploadError::UrlInvalid);
        }
        if expiry <= now_millis().map_err(|_| UploadError::UrlInvalid)? {
            return Err(UploadError::UrlExpired);
        }
        Ok(())
    }

    pub fn admit(&self) -> Result<OwnedSemaphorePermit, UploadError> {
        self.slots
            .clone()
            .try_acquire_owned()
            .map_err(|_| UploadError::TooMany)
    }

    pub async fn receive<S, D, E>(&self, location: &str, mut stream: S) -> Result<(), UploadError>
    where
        S: Stream<Item = Result<D, E>> + Unpin,
        D: AsRef<[u8]>,
    {
        let destination = self
            .resolve(location)
            .map_err(|_| UploadError::UrlInvalid)?;
        if fs::try_exists(&destination).await? {
            return Err(UploadError::AlreadyExists);
        }
        let temporary = Builder::new()
            .prefix("upload-")
            .tempfile_in(&self.temporary)?;
        let (file, path) = temporary.into_parts();
        let mut file = File::from_std(file);
        let mut size = 0u64;
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|_| UploadError::Interrupted)?;
            let bytes = chunk.as_ref();
            size = size
                .checked_add(bytes.len() as u64)
                .ok_or(UploadError::TooLarge)?;
            if size > self.size_limit {
                return Err(UploadError::TooLarge);
            }
            file.write_all(bytes).await?;
        }
        file.flush().await?;
        file.sync_all().await?;
        drop(file);
        self.publish(path, destination).await
    }

    async fn publish(&self, temporary: TempPath, destination: PathBuf) -> Result<(), UploadError> {
        let directory = File::open(&self.files).await?;
        match fs::hard_link(&temporary, &destination).await {
            Ok(()) => {}
            Err(error) if error.kind() == ErrorKind::AlreadyExists => {
                return Err(UploadError::AlreadyExists);
            }
            Err(error) => return Err(error.into()),
        }
        let result = async {
            directory.sync_all().await?;
            temporary.close()?;
            Ok(())
        }
        .await;
        if result.is_err() {
            fs::remove_file(destination).await?;
            directory.sync_all().await?;
        }
        result
    }

    fn resolve(&self, location: &str) -> Result<PathBuf> {
        if !valid_location(location) {
            bail!("invalid artifact storage location");
        }
        Ok(self.files.join(location))
    }
}

impl FileStorage for LocalDirectory {
    async fn upload_url(&self, location: &str) -> Result<UploadRequest> {
        self.resolve(location)?;
        let expiry = now_millis()?
            .checked_add(self.lifetime_millis)
            .context("file URL expiry overflow")?;
        let signature = sign(self.key.expose(), location, expiry)?;
        Ok(UploadRequest::builder()
            .method("PUT")
            .url(Secret::from(format!(
                "{}/files/{location}?expires={expiry}&signature={signature}",
                self.origin
            )))
            .headers(BTreeMap::from([(
                "If-None-Match".to_owned(),
                "*".to_owned(),
            )]))
            .expires_at(expiry)
            .build())
    }

    async fn size(&self, location: &str) -> Result<Option<u64>> {
        match fs::symlink_metadata(self.resolve(location)?).await {
            Ok(metadata) if metadata.is_file() => Ok(Some(metadata.len())),
            Ok(_) => bail!("artifact storage location is not a regular file"),
            Err(error) if error.kind() == ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error.into()),
        }
    }

    async fn delete(&self, location: &str) -> Result<()> {
        match fs::remove_file(self.resolve(location)?).await {
            Ok(()) => File::open(&self.files).await?.sync_all().await?,
            Err(error) if error.kind() == ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        Ok(())
    }
}

fn valid_location(location: &str) -> bool {
    Uuid::parse_str(location)
        .is_ok_and(|id| id.get_version_num() == 7 && id.to_string() == location)
}

fn now_millis() -> Result<i64> {
    Ok(i64::try_from(
        SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis(),
    )?)
}
