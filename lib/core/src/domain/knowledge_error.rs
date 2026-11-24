use sea_orm::DbErr;
use thiserror::Error;

use super::{CallerError, KnowledgeInputError, PageSizeError, SpaceAccessError};

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum KnowledgeError {
    #[error(transparent)]
    Access(#[from] SpaceAccessError),
    #[error(transparent)]
    Caller(#[from] CallerError),
    #[error(transparent)]
    Database(#[from] DbErr),
    #[error(transparent)]
    Input(#[from] KnowledgeInputError),
    #[error(transparent)]
    Internal(#[from] anyhow::Error),
    #[error("Invalid cursor")]
    InvalidCursor,
    #[error("Provide either a valid prefix or a keyword of 1 to 200 characters, not both.")]
    InvalidFilter,
    #[error(transparent)]
    InvalidPageSize(#[from] PageSizeError),
    #[error("Knowledge name is already in use{}", if *.deleted { " by a deleted entry; roll it back instead" } else { "" })]
    NameTaken { deleted: bool },
    #[error("Knowledge entry not found")]
    NotFound,
    #[error("Knowledge version changed; read the latest entry before writing again")]
    VersionChanged,
    #[error("Knowledge version not found")]
    VersionNotFound,
}

impl KnowledgeError {
    pub const fn code(&self) -> &'static str {
        match self {
            Self::Access(error) => error.code(),
            Self::Caller(error) => error.code(),
            Self::Database(_) | Self::Internal(_) => "server.internal",
            Self::Input(_) => "request.invalid_body",
            Self::InvalidCursor | Self::InvalidFilter | Self::InvalidPageSize(_) => {
                "request.invalid_query"
            }
            Self::NameTaken { .. } => "knowledge.name_taken",
            Self::NotFound => "knowledge.not_found",
            Self::VersionChanged => "knowledge.version_changed",
            Self::VersionNotFound => "knowledge.version_not_found",
        }
    }
}
