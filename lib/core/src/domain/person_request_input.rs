use sea_orm::DbErr;
use serde::Serialize;
use thiserror::Error;

use super::{
    CallerError, KnowledgeChanges, KnowledgeError, KnowledgeInputError, KnowledgeTarget,
    NewKnowledgeEntry, PageSizeError, SpaceAccessError, TaskAccessError, TaskKeyword, TaskTarget,
};

pub const PERSON_REQUEST_REASON_MAX_CHARACTERS: usize = 2000;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum PersonRequestAction {
    Change,
    Create,
    Delete,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum PersonRequestStatus {
    Approved,
    Pending,
    Rejected,
}

impl PersonRequestStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Approved => "approved",
            Self::Pending => "pending",
            Self::Rejected => "rejected",
        }
    }
}

impl TryFrom<&str> for PersonRequestStatus {
    type Error = PersonRequestError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "approved" => Ok(Self::Approved),
            "pending" => Ok(Self::Pending),
            "rejected" => Ok(Self::Rejected),
            _ => Err(PersonRequestError::InvalidFilter),
        }
    }
}

#[derive(Clone, Debug)]
pub enum PersonRequestContent {
    Change(KnowledgeTarget, KnowledgeChanges),
    Create(NewKnowledgeEntry),
    Delete(KnowledgeTarget),
}

#[derive(Clone, Copy, Debug)]
pub enum PersonRequestSource<'a> {
    Space(&'a str),
    Task(TaskTarget<'a>),
}

#[derive(Clone, Debug)]
pub struct PersonRequestFilters {
    pub(super) status: Option<PersonRequestStatus>,
    pub(super) keyword: Option<TaskKeyword>,
}

impl PersonRequestFilters {
    pub fn new(
        status: Option<PersonRequestStatus>,
        keyword: Option<String>,
    ) -> Result<Self, PersonRequestError> {
        let keyword = keyword
            .map(|value| {
                TaskKeyword::try_from(value).map_err(|_| PersonRequestError::InvalidFilter)
            })
            .transpose()?;
        Ok(Self { status, keyword })
    }
}

#[derive(Clone, Debug)]
pub struct PersonRequestRejection {
    pub(super) reason: String,
}

impl PersonRequestRejection {
    pub fn new(reason: String) -> Result<Self, PersonRequestError> {
        if reason.trim().is_empty() || reason.chars().count() > PERSON_REQUEST_REASON_MAX_CHARACTERS
        {
            return Err(PersonRequestError::InvalidReason);
        }
        Ok(Self { reason })
    }
}

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum PersonRequestError {
    #[error(transparent)]
    Access(#[from] SpaceAccessError),
    #[error(transparent)]
    Caller(#[from] CallerError),
    #[error(transparent)]
    Database(#[from] DbErr),
    #[error("Request is already decided")]
    Decided,
    #[error(transparent)]
    Input(#[from] KnowledgeInputError),
    #[error(transparent)]
    Internal(#[from] anyhow::Error),
    #[error("Invalid cursor")]
    InvalidCursor,
    #[error("Expected a valid status and a keyword of 1 to 200 characters.")]
    InvalidFilter,
    #[error(transparent)]
    InvalidPageSize(#[from] PageSizeError),
    #[error("Provide a reason of 1 to 2000 characters.")]
    InvalidReason,
    #[error(transparent)]
    Knowledge(KnowledgeError),
    #[error("Knowledge name is already in use")]
    NameTaken,
    #[error("Request not found")]
    NotFound,
    #[error("A contributor cannot approve their own party's request")]
    OwnParty,
    #[error(transparent)]
    Task(#[from] TaskAccessError),
}

impl PersonRequestError {
    pub const fn code(&self) -> &'static str {
        match self {
            Self::Access(error) => error.code(),
            Self::Caller(error) => error.code(),
            Self::Database(_) | Self::Internal(_) => "server.internal",
            Self::Decided => "person_request.decided",
            Self::Input(_) | Self::InvalidReason => "request.invalid_body",
            Self::InvalidCursor | Self::InvalidFilter | Self::InvalidPageSize(_) => {
                "request.invalid_query"
            }
            Self::Knowledge(error) => error.code(),
            Self::NameTaken => "knowledge.name_taken",
            Self::NotFound => "person_request.not_found",
            Self::OwnParty => "person_request.own_party",
            Self::Task(error) => error.code(),
        }
    }
}

impl From<KnowledgeError> for PersonRequestError {
    fn from(error: KnowledgeError) -> Self {
        match error {
            KnowledgeError::NameTaken { .. } => Self::NameTaken,
            error => Self::Knowledge(error),
        }
    }
}
