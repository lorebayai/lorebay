use std::ffi::OsString;
use std::time::SystemTime;

use typed_builder::TypedBuilder;

pub(crate) use local::*;

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

mod listing;
mod local;
mod signature;
