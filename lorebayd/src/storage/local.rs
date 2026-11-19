use std::collections::BTreeMap;
use std::io::{Error, ErrorKind};
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use anyhow::{Context, Result, bail};
use futures_util::future::BoxFuture;
use futures_util::stream::BoxStream;
use futures_util::{Stream, StreamExt};
use lorebay_lib_core::{
    DownloadRequest, DownloadSpec, FileStorage, StoredFile as LibraryFile, UploadRequest,
    UploadSpec,
};
use lorebay_lib_types::{Secret, unix_millis};
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
use super::{FileOperation, StoredFile, StoredFileKind};

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
    pub size_limit: u64,
    slots: Arc<Semaphore>,
    idle_timeout: Duration,
}

impl LocalDirectory {
    pub async fn open(config: &Config, size_limit: u64) -> Result<Self> {
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
            .size_limit(size_limit)
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

    fn signed_url(
        &self,
        operation: FileOperation<'_>,
        location: &str,
        lifetime: Duration,
    ) -> Result<(Url, i64)> {
        self.resolve(location)?;
        let expiry = now_millis()?
            .checked_add(i64::try_from(lifetime.as_millis())?)
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
    fn download_url<'a>(
        &'a self,
        download: DownloadSpec<'a>,
    ) -> BoxFuture<'a, Result<DownloadRequest>> {
        Box::pin(async move {
            let file_name = download.presentation.file_name;
            let (mut url, expires_at) = self.signed_url(
                FileOperation::Download(&file_name),
                download.location,
                download.lifetime,
            )?;
            url.query_pairs_mut().append_pair("file_name", &file_name);
            Ok(DownloadRequest {
                url: Secret::from(String::from(url)),
                expires_at,
            })
        })
    }

    fn upload_url<'a>(&'a self, upload: UploadSpec<'a>) -> BoxFuture<'a, Result<UploadRequest>> {
        Box::pin(async move {
            let (url, expires_at) =
                self.signed_url(FileOperation::Upload, upload.location, upload.lifetime)?;
            Ok(UploadRequest {
                method: "PUT".to_owned(),
                url: Secret::from(String::from(url)),
                headers: BTreeMap::from([("If-None-Match".to_owned(), "*".to_owned())]),
                expires_at,
            })
        })
    }

    fn size<'a>(&'a self, location: &'a str) -> BoxFuture<'a, Result<Option<u64>>> {
        Box::pin(async move {
            match fs::symlink_metadata(self.resolve(location)?).await {
                Ok(metadata) if metadata.is_file() => Ok(Some(metadata.len())),
                Ok(_) => bail!("artifact storage location is not a regular file"),
                Err(error) if error.kind() == ErrorKind::NotFound => Ok(None),
                Err(error) => Err(error.into()),
            }
        })
    }

    fn delete<'a>(&'a self, location: &'a str) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            storage_directory(&self.files).await?;
            match fs::remove_file(self.resolve(location)?).await {
                Ok(()) => File::open(&self.files).await?.sync_all().await?,
                Err(error) if error.kind() == ErrorKind::NotFound => {}
                Err(error) => return Err(error.into()),
            }
            Ok(())
        })
    }

    fn list(&self) -> BoxStream<'_, Result<LibraryFile>> {
        list_files(self.files.clone(), StoredFileKind::Artifact)
            .filter_map(async |entry| match entry {
                Ok(file) => file
                    .name
                    .into_string()
                    .ok()
                    .filter(|name| valid_location(name))
                    .map(|location| {
                        Ok(LibraryFile {
                            location,
                            modified: file.modified,
                        })
                    }),
                Err(error) => Some(Err(error)),
            })
            .boxed()
    }
}

impl LocalDirectory {
    pub async fn remove_local_leftovers(&self, lifetime: Duration) -> Result<u64> {
        let cutoff = SystemTime::now()
            .checked_sub(lifetime)
            .context("Local file cleanup cutoff is out of range")?;
        let uploads = list_files(self.temporary.clone(), StoredFileKind::Temporary);
        let artifacts = list_files(self.files.clone(), StoredFileKind::Artifact);
        let mut entries = uploads.chain(artifacts).boxed();
        let mut removed = 0;
        while let Some(file) = entries.next().await {
            let file = file?;
            if file.modified < cutoff
                && (file.kind == StoredFileKind::Temporary
                    || !file.name.to_str().is_some_and(valid_location))
            {
                removed += u64::from(self.remove_listed(&file).await?);
            }
        }
        Ok(removed)
    }

    async fn remove_listed(&self, file: &StoredFile) -> Result<bool> {
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
}

fn valid_location(location: &str) -> bool {
    Uuid::parse_str(location)
        .is_ok_and(|id| id.get_version_num() == 7 && id.to_string() == location)
}

fn now_millis() -> Result<i64> {
    Ok(unix_millis(SystemTime::now())?)
}
