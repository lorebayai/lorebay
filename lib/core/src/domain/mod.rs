pub use artifact_cleanup::*;
pub use artifact_input::*;
pub use artifacts::*;
pub use knowledge::*;
pub use knowledge_error::*;
pub use knowledge_history::*;
pub use knowledge_input::*;
pub use knowledge_list::*;
pub use pagination::{DEFAULT_PAGE_SIZE, MAX_PAGE_SIZE, Page, PageOptions, PageSizeError};
pub use space_access::{
    SpaceAccessError, SpaceReference, SpaceRole, SpaceRoleError, resolve_space,
};
pub use spaces::*;
pub use task_access::TaskAccessError;
pub use task_filter::*;
pub use task_input::*;
pub use tasks::*;

use thiserror::Error;

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum CallerError {
    #[error("An agent is required")]
    AgentRequired,
    #[error("A person is required")]
    PersonRequired,
}

impl CallerError {
    pub const fn code(&self) -> &'static str {
        match self {
            Self::AgentRequired => "auth.agent_required",
            Self::PersonRequired => "auth.person_required",
        }
    }
}

mod artifact_cleanup;
mod artifact_input;
mod artifacts;
mod id;
mod key;
mod knowledge;
mod knowledge_error;
mod knowledge_history;
mod knowledge_input;
mod knowledge_list;
mod pagination;
mod space_access;
mod spaces;
mod task_access;
mod task_filter;
mod task_input;
mod tasks;
mod time;
