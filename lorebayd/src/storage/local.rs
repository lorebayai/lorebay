use std::collections::BTreeMap;
use std::io::{Error, ErrorKind};
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};
use futures_util::{Stream, StreamExt};
use lorebay_base::Secret;
use tempfile::{Builder, TempPath};
use thiserror::Error;
use tokio::fs::{self, File};
use tokio::io::AsyncWriteExt;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use tokio::time::timeout;
use typed_builder::TypedBuilder;
use url::Url;
use uuid::Uuid;

use crate::Config;

use super::listing::{list_files, storage_directory};
use super::signature::{load_key, sign, verify};
use super::{
    DownloadRequest, FileOperation, FileStorage, StoredFile, StoredFileKind, UploadRequest,
};

#[derive(Debug, Error)]
pub(crate) enum StorageError {
    #[error("A file already exists at this location")]
    AlreadyExists,
    #[error("The upload was interrupted")]
    Interrupted,
    #[error(transparent)]
    Io(#[from] Error),
    #[error("File not found")]
    NotFound,
    #[error("The upload exceeds the size limit")]
    TooLarge,
    #[error("Too many uploads are in progress")]
    TooMany,
    #[error("The file URL is expired")]
    UrlExpired,
    #[error("The file URL is invalid")]
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
    idle_timeout: Duration,
}

impl LocalDirectory {
    pub async fn open(config: &Config) -> Result<Self> {
        let files = config.data_directory.join("artifacts");
        let temporary = config.data_directory.join("uploads");
        fs::create_dir_all(&files).await?;
        fs::create_dir_all(&temporary).await?;
        storage_directory(&files).await?;
        storage_directory(&temporary).await?;
        File::open(&config.data_directory).await?.sync_all().await?;
        Ok(Self::builder()
            .files(files)
            .temporary(temporary)
            .key(load_key(&config.data_directory)?)
            .origin(config.public_url.trim_end_matches('/'))
            .lifetime_millis(i64::try_from(config.upload_url_lifetime_seconds)? * 1000)
            .size_limit(config.upload_size_limit_bytes)
            .idle_timeout(Duration::from_secs(config.upload_idle_timeout_seconds))
            .slots(Arc::new(Semaphore::new(
                config.upload_concurrency_limit as usize,
            )))
            .build())
    }

    pub fn authorize(
        &self,
        operation: FileOperation<'_>,
        location: &str,
        expiry: i64,
        signature: &str,
    ) -> Result<(), StorageError> {
        if !valid_location(location)
            || !verify(self.key.expose(), operation, location, expiry, signature)
        {
            return Err(StorageError::UrlInvalid);
        }
        if expiry <= now_millis().map_err(|_| StorageError::UrlInvalid)? {
            return Err(StorageError::UrlExpired);
        }
        Ok(())
    }

    pub async fn open_download(&self, location: &str) -> Result<(File, u64), StorageError> {
        let path = self
            .resolve(location)
            .map_err(|_| StorageError::UrlInvalid)?;
        let file = File::open(path).await.map_err(|error| {
            if error.kind() == ErrorKind::NotFound {
                StorageError::NotFound
            } else {
                StorageError::Io(error)
            }
        })?;
        let metadata = file.metadata().await?;
        if !metadata.is_file() {
            return Err(StorageError::NotFound);
        }
        Ok((file, metadata.len()))
    }

    pub fn admit(&self) -> Result<OwnedSemaphorePermit, StorageError> {
        self.slots
            .clone()
            .try_acquire_owned()
            .map_err(|_| StorageError::TooMany)
    }

    pub async fn receive<S, D, E>(&self, location: &str, mut stream: S) -> Result<(), StorageError>
    where
        S: Stream<Item = Result<D, E>> + Unpin,
        D: AsRef<[u8]>,
    {
        let destination = self
            .resolve(location)
            .map_err(|_| StorageError::UrlInvalid)?;
        if fs::try_exists(&destination).await? {
            return Err(StorageError::AlreadyExists);
        }
        let temporary = Builder::new()
            .prefix("upload-")
            .tempfile_in(&self.temporary)?;
        let (file, path) = temporary.into_parts();
        let mut file = File::from_std(file);
        let mut size = 0u64;
        while let Some(chunk) = timeout(self.idle_timeout, stream.next())
            .await
            .map_err(|_| StorageError::Interrupted)?
        {
            let chunk = chunk.map_err(|_| StorageError::Interrupted)?;
            let bytes = chunk.as_ref();
            size = size
                .checked_add(bytes.len() as u64)
                .ok_or(StorageError::TooLarge)?;
            if size > self.size_limit {
                return Err(StorageError::TooLarge);
            }
            file.write_all(bytes).await?;
        }
        file.flush().await?;
        file.sync_all().await?;
        drop(file);
        self.publish(path, destination).await
    }

    async fn publish(&self, temporary: TempPath, destination: PathBuf) -> Result<(), StorageError> {
        let directory = File::open(&self.files).await?;
        match fs::hard_link(&temporary, &destination).await {
            Ok(()) => {}
            Err(error) if error.kind() == ErrorKind::AlreadyExists => {
                return Err(StorageError::AlreadyExists);
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

    fn signed_url(&self, operation: FileOperation<'_>, location: &str) -> Result<(Url, i64)> {
        self.resolve(location)?;
        let expiry = now_millis()?
            .checked_add(self.lifetime_millis)
            .context("file URL expiry overflow")?;
        let signature = sign(self.key.expose(), operation, location, expiry)?;
        let mut url = Url::parse(&format!("{}/files/{location}", self.origin))?;
        url.query_pairs_mut()
            .append_pair("expires", &expiry.to_string())
            .append_pair("signature", &signature);
        Ok((url, expiry))
    }

    fn resolve(&self, location: &str) -> Result<PathBuf> {
        if !valid_location(location) {
            bail!("invalid artifact storage location");
        }
        Ok(self.files.join(location))
    }
}

impl FileStorage for LocalDirectory {
    async fn download_url(&self, location: &str, file_name: &str) -> Result<DownloadRequest> {
        let (mut url, expires_at) =
            self.signed_url(FileOperation::Download(file_name), location)?;
        url.query_pairs_mut().append_pair("file_name", file_name);
        Ok(DownloadRequest::builder()
            .url(Secret::from(String::from(url)))
            .expires_at(expires_at)
            .build())
    }

    async fn upload_url(&self, location: &str) -> Result<UploadRequest> {
        let (url, expiry) = self.signed_url(FileOperation::Upload, location)?;
        Ok(UploadRequest::builder()
            .method("PUT")
            .url(Secret::from(String::from(url)))
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

    fn list(&self) -> impl Stream<Item = Result<StoredFile>> + Send {
        list_files(self.files.clone(), self.temporary.clone())
    }

    async fn delete_listed(&self, file: &StoredFile) -> Result<bool> {
        let directory = match file.kind {
            StoredFileKind::Artifact => &self.files,
            StoredFileKind::Temporary => &self.temporary,
        };
        storage_directory(directory).await?;
        let mut components = Path::new(&file.name).components();
        if !matches!(components.next(), Some(Component::Normal(_))) || components.next().is_some() {
            bail!("invalid listed storage file name");
        }
        let path = directory.join(&file.name);
        let metadata = match fs::symlink_metadata(&path).await {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(false),
            Err(error) => return Err(error.into()),
        };
        if !metadata.is_file() || metadata.modified()? != file.modified {
            return Ok(false);
        }
        match fs::remove_file(path).await {
            Ok(()) => File::open(directory).await?.sync_all().await?,
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(false),
            Err(error) => return Err(error.into()),
        }
        Ok(true)
    }

    async fn delete(&self, location: &str) -> Result<()> {
        storage_directory(&self.files).await?;
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
