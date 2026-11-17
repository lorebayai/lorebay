use std::collections::BTreeMap;
use std::future::Future;

use anyhow::Result;
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

mod local;
mod signature;
