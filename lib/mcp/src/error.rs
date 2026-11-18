use std::fmt::Display;

use lorebay_lib_core::{ArtifactError, ArtifactTextError, SpaceError, TaskError, TaskTextError};
use rmcp::model::CallToolResult;
use serde_json::{Value, json};

#[derive(Debug)]
pub(super) struct ToolError {
    code: &'static str,
    message: String,
    errors: Vec<Value>,
}

impl ToolError {
    pub fn internal() -> Self {
        Self::operation("server.internal", "Internal Server Error".to_owned())
    }

    pub fn invalid_body() -> Self {
        Self::body_field(
            "body",
            "invalid",
            "Expected a JSON object with the required string fields.",
        )
    }

    pub fn body_field(field: &str, code: &str, message: &str) -> Self {
        Self::field("request.invalid_body", field, code, message)
    }

    pub fn query_field(field: &str, message: &str) -> Self {
        Self::field("request.invalid_query", field, "invalid", message)
    }

    pub fn task_text(field: &str, error: TaskTextError) -> Self {
        Self::body_field(field, task_text_code(&error), &error.to_string())
    }

    pub fn keyword(error: TaskTextError) -> Self {
        Self::field(
            "request.invalid_query",
            "keyword",
            task_text_code(&error),
            &error.to_string(),
        )
    }

    pub fn artifact_text(field: &str, error: ArtifactTextError) -> Self {
        let code = match error {
            ArtifactTextError::Empty => "required",
            ArtifactTextError::TooManyBytes(_) | ArtifactTextError::TooManyCharacters(_) => {
                "too_long"
            }
        };
        Self::body_field(field, code, &error.to_string())
    }

    pub fn into_result(self) -> CallToolResult {
        CallToolResult::structured_error(json!({
            "code": self.code, "message": self.message, "errors": self.errors,
        }))
    }

    fn field(problem: &'static str, field: &str, code: &str, message: &str) -> Self {
        Self {
            code: problem,
            message: "Bad Request".to_owned(),
            errors: vec![json!({"field": field, "code": code, "message": message})],
        }
    }

    fn operation(code: &'static str, message: String) -> Self {
        Self {
            code,
            message,
            errors: Vec::new(),
        }
    }

    fn from_operation(code: &'static str, error: impl Display) -> Self {
        if code == "server.internal" {
            tracing::error!(%error, "MCP operation failed");
            return Self::internal();
        }
        Self::operation(code, error.to_string())
    }
}

impl From<ArtifactError> for ToolError {
    fn from(error: ArtifactError) -> Self {
        Self::from_operation(error.code(), error)
    }
}

impl From<SpaceError> for ToolError {
    fn from(error: SpaceError) -> Self {
        match error {
            SpaceError::InvalidCursor => invalid_query("cursor"),
            SpaceError::InvalidPageSize => invalid_query("page_size"),
            error => Self::from_operation(error.code(), error),
        }
    }
}

impl From<TaskError> for ToolError {
    fn from(error: TaskError) -> Self {
        match error {
            TaskError::Filter(error) => Self::query_field(error.field(), &error.to_string()),
            TaskError::InvalidCursor => invalid_query("cursor"),
            TaskError::InvalidPageSize => invalid_query("page_size"),
            TaskError::NoChanges => Self::body_field("body", "required", &error.to_string()),
            error => Self::from_operation(error.code(), error),
        }
    }
}

pub(super) fn invalid_query(field: &str) -> ToolError {
    ToolError::query_field(field, "Invalid pagination parameter.")
}

fn task_text_code(error: &TaskTextError) -> &'static str {
    match error {
        TaskTextError::Empty => "required",
        TaskTextError::TooLong(_) => "too_long",
    }
}
