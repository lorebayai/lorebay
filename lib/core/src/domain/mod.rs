pub use agent_file_cleanup::*;
pub use agent_file_input::*;
pub use agent_files::*;
pub use knowledge::*;
pub use knowledge_error::*;
pub use knowledge_history::*;
pub use knowledge_input::*;
pub use knowledge_list::*;
pub use pagination::{DEFAULT_PAGE_SIZE, MAX_PAGE_SIZE, Page, PageOptions, PageSizeError};
pub use person_request_apply::*;
pub use person_request_input::*;
pub use person_request_list::*;
pub use person_requests::*;
pub use space_access::{
    SpaceAccessError, SpaceReference, SpaceRole, SpaceRoleError, resolve_space,
};
pub use spaces::*;
pub use task_access::TaskAccessError;
pub use task_filter::*;
pub use task_holds::*;
pub use task_input::*;
pub use task_state::*;
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

mod agent_file_cleanup;
mod agent_file_input;
mod agent_files;
mod id;
mod key;
mod knowledge;
mod knowledge_error;
mod knowledge_history;
mod knowledge_input;
mod knowledge_list;
mod pagination;
mod person_request_apply;
mod person_request_input;
mod person_request_list;
mod person_requests;
mod space_access;
mod spaces;
mod task_access;
mod task_filter;
mod task_holds;
mod task_input;
mod task_state;
mod tasks;
mod time;
