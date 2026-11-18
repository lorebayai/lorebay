use lorebay_lib_types::sha256_hex;
use thiserror::Error;
use typed_builder::TypedBuilder;

use super::{TaskKeyword, TaskStatus};

#[derive(Debug, Error)]
pub(crate) enum TaskFilterError {
    #[error("Own open tasks cannot be combined with a person filter.")]
    PersonConflict,
    #[error("Own open tasks cannot be combined with a status filter.")]
    StatusConflict,
}

#[derive(Clone, Debug, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub(crate) struct TaskFilters {
    pub person: Option<String>,
    pub agent: Option<String>,
    pub status: Option<TaskStatus>,
    pub own_open: bool,
    pub keyword: Option<TaskKeyword>,
}

impl TaskFilterError {
    pub fn field(&self) -> &'static str {
        match self {
            Self::PersonConflict => "person",
            Self::StatusConflict => "status",
        }
    }
}

impl TaskFilters {
    pub fn validate(&self) -> Result<(), TaskFilterError> {
        if self.own_open && self.person.is_some() {
            return Err(TaskFilterError::PersonConflict);
        }
        if self.own_open && self.status.is_some() {
            return Err(TaskFilterError::StatusConflict);
        }
        Ok(())
    }

    pub fn cursor_version(&self, space: &str) -> Result<String, serde_json::Error> {
        let scope = serde_json::to_string(&(
            space,
            &self.person,
            &self.agent,
            self.status.map(TaskStatus::as_str),
            self.own_open,
            self.keyword.as_ref().map(TaskKeyword::as_str),
        ))?;
        Ok(format!("tasks-v2:{}", sha256_hex(&scope)))
    }
}
