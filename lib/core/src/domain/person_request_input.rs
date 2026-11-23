use sea_orm::DbErr;
use serde::Serialize;
use thiserror::Error;

use super::{
    CallerError, KnowledgeChanges, KnowledgeError, KnowledgeInputError, KnowledgeTarget,
    NewKnowledgeEntry, PageSizeError, SpaceAccessError, TaskAccessError, TaskKeyword, TaskTarget,
};

pub const PROPOSAL_REASON_MAX_CHARACTERS: usize = 2000;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ProposalKind {
    Change,
    Create,
    Delete,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ProposalStatus {
    Approved,
    Pending,
    Rejected,
}

impl ProposalStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Approved => "approved",
            Self::Pending => "pending",
            Self::Rejected => "rejected",
        }
    }
}

impl TryFrom<&str> for ProposalStatus {
    type Error = ProposalError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "approved" => Ok(Self::Approved),
            "pending" => Ok(Self::Pending),
            "rejected" => Ok(Self::Rejected),
            _ => Err(ProposalError::InvalidFilter),
        }
    }
}

#[derive(Clone, Debug)]
pub enum ProposalContent {
    Change(KnowledgeTarget, KnowledgeChanges),
    Create(NewKnowledgeEntry),
    Delete(KnowledgeTarget),
}

#[derive(Clone, Copy, Debug)]
pub enum ProposalSource<'a> {
    Space(&'a str),
    Task(TaskTarget<'a>),
}

#[derive(Clone, Debug)]
pub struct ProposalFilters {
    pub(super) status: Option<ProposalStatus>,
    pub(super) keyword: Option<TaskKeyword>,
}

impl ProposalFilters {
    pub fn new(
        status: Option<ProposalStatus>,
        keyword: Option<String>,
    ) -> Result<Self, ProposalError> {
        let keyword = keyword
            .map(|value| TaskKeyword::try_from(value).map_err(|_| ProposalError::InvalidFilter))
            .transpose()?;
        Ok(Self { status, keyword })
    }
}

#[derive(Clone, Debug)]
pub struct ProposalRejection {
    pub(super) reason: String,
}

impl ProposalRejection {
    pub fn new(reason: String) -> Result<Self, ProposalError> {
        if reason.trim().is_empty() || reason.chars().count() > PROPOSAL_REASON_MAX_CHARACTERS {
            return Err(ProposalError::InvalidReason);
        }
        Ok(Self { reason })
    }
}

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum ProposalError {
    #[error(transparent)]
    Access(#[from] SpaceAccessError),
    #[error(transparent)]
    Caller(#[from] CallerError),
    #[error(transparent)]
    Database(#[from] DbErr),
    #[error("Proposal is already decided")]
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
    #[error("Proposal not found")]
    NotFound,
    #[error("A contributor cannot approve their own party's proposal")]
    OwnParty,
    #[error(transparent)]
    Task(#[from] TaskAccessError),
}

impl ProposalError {
    pub const fn code(&self) -> &'static str {
        match self {
            Self::Access(error) => error.code(),
            Self::Caller(error) => error.code(),
            Self::Database(_) | Self::Internal(_) => "server.internal",
            Self::Decided => "proposal.decided",
            Self::Input(_) | Self::InvalidReason => "request.invalid_body",
            Self::InvalidCursor | Self::InvalidFilter | Self::InvalidPageSize(_) => {
                "request.invalid_query"
            }
            Self::Knowledge(error) => error.code(),
            Self::NameTaken => "knowledge.name_taken",
            Self::NotFound => "proposal.not_found",
            Self::OwnParty => "proposal.own_party",
            Self::Task(error) => error.code(),
        }
    }
}

impl From<KnowledgeError> for ProposalError {
    fn from(error: KnowledgeError) -> Self {
        match error {
            KnowledgeError::NameTaken { .. } => Self::NameTaken,
            error => Self::Knowledge(error),
        }
    }
}
