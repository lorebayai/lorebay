use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use anyhow::{Result, bail};
use futures_util::{Stream, stream};
use tokio::fs::{self, DirEntry, ReadDir};

use super::{StoredFile, StoredFileKind};

pub(super) fn list_files(
    artifacts: PathBuf,
    uploads: PathBuf,
) -> impl Stream<Item = Result<StoredFile>> + Send {
    let directories = [
        (StoredFileKind::Artifact, artifacts),
        (StoredFileKind::Temporary, uploads),
    ]
    .into_iter();
    stream::try_unfold(
        (directories, None::<(StoredFileKind, ReadDir)>),
        async |(mut directories, mut current)| {
            loop {
                if let Some((kind, entries)) = &mut current
                    && let Some(entry) = entries.next_entry().await?
                {
                    if let Some(file) = stored_file(entry, *kind).await? {
                        return Ok(Some((file, (directories, current))));
                    }
                    continue;
                }
                let Some((kind, directory)) = directories.next() else {
                    return Ok(None);
                };
                storage_directory(&directory).await?;
                current = Some((kind, fs::read_dir(directory).await?));
            }
        },
    )
}

pub(super) async fn storage_directory(path: &Path) -> Result<()> {
    if !fs::symlink_metadata(path).await?.is_dir() {
        bail!("artifact storage directory must be a directory, not a symbolic link");
    }
    Ok(())
}

async fn stored_file(entry: DirEntry, kind: StoredFileKind) -> Result<Option<StoredFile>> {
    let metadata = match fs::symlink_metadata(entry.path()).await {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    if !metadata.is_file() {
        return Ok(None);
    }
    Ok(Some(
        StoredFile::builder()
            .name(entry.file_name())
            .kind(kind)
            .modified(metadata.modified()?)
            .build(),
    ))
}
