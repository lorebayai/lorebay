use lorebay_lib_types::sha256_hex;
use thiserror::Error;
use typed_builder::TypedBuilder;

use super::{TaskKeyword, TaskState};

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum TaskFilterError {
    #[error("Claimable tasks cannot be combined with person, agent, state or own_open.")]
    ClaimableConflict,
    #[error("Own open tasks cannot be combined with a person filter.")]
    PersonConflict,
    #[error("Own open tasks cannot be combined with a state filter.")]
    StateConflict,
}

#[derive(Clone, Debug, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub struct TaskFilters {
    pub person: Option<String>,
    pub agent: Option<String>,
    pub state: Option<TaskState>,
    pub own_open: bool,
    #[builder(default = false)]
    pub claimable: bool,
    pub keyword: Option<TaskKeyword>,
}

impl TaskFilterError {
    pub fn field(&self) -> &'static str {
        match self {
            Self::ClaimableConflict => "claimable",
            Self::PersonConflict => "person",
            Self::StateConflict => "state",
        }
    }
}

impl TaskFilters {
    pub fn validate(&self) -> Result<(), TaskFilterError> {
        if self.claimable
            && (self.own_open
                || self.person.is_some()
                || self.agent.is_some()
                || self.state.is_some())
        {
            return Err(TaskFilterError::ClaimableConflict);
        }
        if self.own_open && self.person.is_some() {
            return Err(TaskFilterError::PersonConflict);
        }
        if self.own_open && self.state.is_some() {
            return Err(TaskFilterError::StateConflict);
        }
        Ok(())
    }

    pub(super) fn cursor_version(&self, space: &str) -> Result<String, serde_json::Error> {
        let scope = serde_json::to_string(&(
            space,
            &self.person,
            &self.agent,
            self.state.map(TaskState::as_str),
            self.own_open,
            self.claimable,
            self.keyword.as_ref().map(TaskKeyword::as_str),
        ))?;
        Ok(format!("tasks-v2:{}", sha256_hex(&scope)))
    }
}
