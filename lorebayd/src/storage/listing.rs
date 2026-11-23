use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use anyhow::{Result, bail};
use futures_util::{Stream, stream};
use tokio::fs::{self, DirEntry, ReadDir};

use super::{LocalFile, StoredFileKind};

pub(super) fn list_files(
    directory: PathBuf,
    kind: StoredFileKind,
) -> impl Stream<Item = Result<LocalFile>> + Send {
    stream::try_unfold(
        (Some(directory), None::<ReadDir>),
        move |(directory, entries)| async move {
            let mut entries = match (directory, entries) {
                (Some(directory), _) => {
                    storage_directory(&directory).await?;
                    fs::read_dir(directory).await?
                }
                (_, Some(entries)) => entries,
                _ => return Ok(None),
            };
            while let Some(entry) = entries.next_entry().await? {
                if let Some(file) = stored_file(entry, kind).await? {
                    return Ok(Some((file, (None, Some(entries)))));
                }
            }
            Ok(None)
        },
    )
}

pub(super) async fn storage_directory(path: &Path) -> Result<()> {
    if !fs::symlink_metadata(path).await?.is_dir() {
        bail!("file storage directory must be a directory, not a symbolic link");
    }
    Ok(())
}

async fn stored_file(entry: DirEntry, kind: StoredFileKind) -> Result<Option<LocalFile>> {
    let metadata = match fs::symlink_metadata(entry.path()).await {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    if !metadata.is_file() {
        return Ok(None);
    }
    Ok(Some(
        LocalFile::builder()
            .name(entry.file_name())
            .kind(kind)
            .modified(metadata.modified()?)
            .build(),
    ))
}
