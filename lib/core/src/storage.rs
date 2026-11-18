use std::collections::BTreeMap;
use std::time::{Duration, SystemTime};

use anyhow::Result;
use futures_util::future::BoxFuture;
use futures_util::stream::BoxStream;
use lorebay_lib_types::Secret;
use serde::Serialize;
use typed_builder::TypedBuilder;

pub trait FileStorage: Send + Sync {
    fn upload_url<'a>(&'a self, upload: UploadSpec<'a>) -> BoxFuture<'a, Result<UploadRequest>>;

    fn download_url<'a>(
        &'a self,
        download: DownloadSpec<'a>,
    ) -> BoxFuture<'a, Result<DownloadRequest>>;

    fn size<'a>(&'a self, location: &'a str) -> BoxFuture<'a, Result<Option<u64>>>;

    fn delete<'a>(&'a self, location: &'a str) -> BoxFuture<'a, Result<()>>;

    fn list(&self) -> BoxStream<'_, Result<StoredFile>>;
}

#[derive(Clone, Debug, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub struct UploadSpec<'a> {
    pub location: &'a str,
    pub lifetime: Duration,
    pub size_limit: u64,
}

#[derive(Clone, Debug, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub struct DownloadSpec<'a> {
    pub location: &'a str,
    pub lifetime: Duration,
    pub presentation: DownloadPresentation,
}

#[derive(Clone, Debug, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub struct DownloadPresentation {
    pub file_name: String,
    pub content_type: &'static str,
    pub disposition: &'static str,
}

impl DownloadPresentation {
    pub(crate) fn from_name(file_name: String, inline_content_types: &[String]) -> Self {
        let extension = file_name
            .rsplit_once('.')
            .map(|(_, extension)| extension.to_ascii_lowercase());
        let content_type = match extension.as_deref() {
            Some("gif") => "image/gif",
            Some("jpeg" | "jpg") => "image/jpeg",
            Some("pdf") => "application/pdf",
            Some("png") => "image/png",
            Some("webp") => "image/webp",
            _ => "application/octet-stream",
        };
        let disposition = if inline_content_types.iter().any(|item| item == content_type) {
            "inline"
        } else {
            "attachment"
        };
        Self {
            file_name,
            content_type,
            disposition,
        }
    }
}

#[derive(Clone, Debug, Serialize, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub struct DownloadRequest {
    pub url: Secret<String>,
    pub expires_at: i64,
}

#[derive(Clone, Debug, Serialize, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub struct UploadRequest {
    pub method: String,
    #[serde(rename = "upload_url")]
    pub url: Secret<String>,
    pub headers: BTreeMap<String, String>,
    pub expires_at: i64,
}

#[derive(Clone, Debug, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub struct StoredFile {
    pub location: String,
    pub modified: SystemTime,
}
