use rmcp::model::{JsonObject, Tool};
use serde_json::Value;
use typed_builder::TypedBuilder;

use lorebay_lib_core::{
    AgentFileName, AgentFileSummary, NewAgentFile, PageOptions, PageSizeError, TaskChanges,
    TaskFilters, TaskInclude, TaskKeyword, TaskResult, TaskState, TaskStateError, TaskTitle,
};

use crate::catalog::ToolName;
use crate::error::{ToolError, invalid_query};

mod knowledge;

#[derive(Clone, Debug, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub(crate) struct Arguments {
    values: JsonObject,
}

impl Arguments {
    pub fn parse(
        tool_name: ToolName,
        tool: &Tool,
        values: Option<JsonObject>,
    ) -> Result<Self, ToolError> {
        let values = values.unwrap_or_default();
        let properties = tool.input_schema["properties"]
            .as_object()
            .ok_or_else(ToolError::internal)?;
        for (name, value) in &values {
            let schema = properties.get(name).ok_or_else(|| {
                if tool_name == ToolName::ListTasks {
                    invalid_query("query")
                } else {
                    ToolError::invalid_body()
                }
            })?;
            let valid = match schema["type"].as_str() {
                Some("string") => value.is_string(),
                Some("boolean") => value.is_boolean(),
                Some("integer") => value.as_u64().is_some(),
                _ if name == "result_version" => {
                    value.is_null() || value.as_i64().is_some_and(|version| version > 0)
                }
                _ => false,
            };
            if !valid {
                return Err(match name.as_str() {
                    "page_size" | "cursor" => invalid_query(name),
                    "include_scratchpad" | "include_results" => invalid_query("query"),
                    _ if tool_name == ToolName::ListTasks => invalid_query("query"),
                    _ => ToolError::invalid_body(),
                });
            }
        }
        if let Some(required) = tool.input_schema["required"].as_array()
            && required
                .iter()
                .filter_map(Value::as_str)
                .any(|name| !values.contains_key(name))
        {
            return Err(ToolError::invalid_body());
        }
        Ok(Self { values })
    }

    pub fn string(&self, name: &str) -> Result<String, ToolError> {
        self.values
            .get(name)
            .and_then(Value::as_str)
            .map(str::to_owned)
            .ok_or_else(ToolError::invalid_body)
    }

    pub fn page(&self) -> Result<PageOptions, ToolError> {
        PageOptions::new(
            self.values
                .get(PageSizeError::FIELD)
                .and_then(Value::as_u64),
            self.cursor(),
        )
        .map_err(|_| invalid_query(PageSizeError::FIELD))
    }

    pub fn include(&self) -> Result<Option<TaskInclude>, ToolError> {
        if self.enabled("include_results") {
            if self.enabled("include_scratchpad") {
                return Err(invalid_query("query"));
            }
            return self.page().map(TaskInclude::Results).map(Some);
        }
        if self.enabled("include_scratchpad") {
            return self.page().map(TaskInclude::Scratchpad).map(Some);
        }
        if self.values.contains_key("page_size") {
            return Err(invalid_query("page_size"));
        }
        if self.values.contains_key("cursor") {
            return Err(invalid_query("cursor"));
        }
        Ok(None)
    }

    pub fn changes(&self) -> Result<TaskChanges, ToolError> {
        let title = self
            .optional_string("title")
            .map(TaskTitle::try_from)
            .transpose()
            .map_err(|error| ToolError::task_text("title", error))?;
        let state = self.state().map_err(|error| {
            ToolError::body_field(
                TaskStateError::FIELD,
                TaskStateError::FIELD_CODE,
                &error.to_string(),
            )
        })?;
        let result = self
            .optional_string("result")
            .map(TaskResult::try_from)
            .transpose()
            .map_err(|error| ToolError::task_text("result", error))?;
        let changes = TaskChanges {
            title,
            state,
            state_reason: self.optional_string("state_reason"),
            result,
            result_version: self.values.get("result_version").and_then(Value::as_i64),
        };
        changes.validate()?;
        Ok(changes)
    }

    pub fn agent_file(&self) -> Result<NewAgentFile, ToolError> {
        let file_name = AgentFileName::try_from(self.string("file_name")?)
            .map_err(|error| ToolError::agent_file_text("file_name", error))?;
        let summary = AgentFileSummary::try_from(self.string("summary")?)
            .map_err(|error| ToolError::agent_file_text("summary", error))?;
        Ok(NewAgentFile { file_name, summary })
    }

    pub fn tasks(&self) -> Result<(PageOptions, TaskFilters), ToolError> {
        let state = self.state().map_err(|error| {
            ToolError::query_field(
                TaskStateError::FIELD,
                TaskStateError::FIELD_CODE,
                &error.to_string(),
            )
        })?;
        let keyword = self
            .optional_string("keyword")
            .map(TaskKeyword::try_from)
            .transpose()
            .map_err(ToolError::keyword)?;
        let filters = TaskFilters {
            person: self.optional_string("person"),
            agent: self.optional_string("agent"),
            state,
            own_open: self.enabled("own_open"),
            claimable: self.enabled("claimable"),
            keyword,
        };
        filters.validate().map_err(|error| {
            ToolError::query_field(error.field(), "invalid", &error.to_string())
        })?;
        Ok((self.page()?, filters))
    }

    pub(super) fn enabled(&self, name: &str) -> bool {
        self.values
            .get(name)
            .and_then(Value::as_bool)
            .unwrap_or(false)
    }

    fn state(&self) -> Result<Option<TaskState>, TaskStateError> {
        self.values
            .get(TaskStateError::FIELD)
            .and_then(Value::as_str)
            .map(TaskState::try_from)
            .transpose()
    }

    pub(crate) fn optional_string(&self, name: &str) -> Option<String> {
        self.values
            .get(name)
            .and_then(Value::as_str)
            .map(str::to_owned)
    }

    fn cursor(&self) -> Option<String> {
        self.values
            .get("cursor")
            .and_then(Value::as_str)
            .map(str::to_owned)
    }
}
