use std::collections::BTreeMap;
use std::ffi::OsString;
use std::future::Future;
use std::time::SystemTime;

use anyhow::Result;
use futures_util::Stream;
use lorebay_base::Secret;
use typed_builder::TypedBuilder;

pub(crate) use local::*;

pub(crate) trait FileStorage: Send + Sync {
    fn download_url(
        &self,
        location: &str,
        file_name: &str,
    ) -> impl Future<Output = Result<DownloadRequest>> + Send;
    fn upload_url(&self, location: &str) -> impl Future<Output = Result<UploadRequest>> + Send;
    fn size(&self, location: &str) -> impl Future<Output = Result<Option<u64>>> + Send;
    fn delete(&self, location: &str) -> impl Future<Output = Result<()>> + Send;
    fn list(&self) -> impl Stream<Item = Result<StoredFile>> + Send;
    fn delete_listed(&self, file: &StoredFile) -> impl Future<Output = Result<bool>> + Send;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum StoredFileKind {
    Artifact,
    Temporary,
}

#[derive(Clone, Debug, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub(crate) struct StoredFile {
    pub name: OsString,
    pub kind: StoredFileKind,
    pub modified: SystemTime,
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum FileOperation<'a> {
    Download(&'a str),
    Upload,
}

#[derive(Clone, Debug, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub(crate) struct DownloadRequest {
    pub url: Secret<String>,
    pub expires_at: i64,
}

#[derive(Clone, Debug, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub(crate) struct UploadRequest {
    pub method: String,
    pub url: Secret<String>,
    pub headers: BTreeMap<String, String>,
    pub expires_at: i64,
}

mod listing;
mod local;
mod signature;
