use std::collections::BTreeMap;
use std::future::Future;

use anyhow::Result;
use lorebay_base::Secret;
use typed_builder::TypedBuilder;

pub(crate) use local::*;

pub(crate) trait FileStorage: Send + Sync {
    fn upload_url(&self, location: &str) -> impl Future<Output = Result<UploadRequest>> + Send;
    fn size(&self, location: &str) -> impl Future<Output = Result<Option<u64>>> + Send;
    #[expect(
        dead_code,
        reason = "Artifact deletion and cleanup use this storage operation in the following issues"
    )]
    fn delete(&self, location: &str) -> impl Future<Output = Result<()>> + Send;
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
