use rmcp::model::{JsonObject, Tool};
use serde_json::Value;
use typed_builder::TypedBuilder;

use lorebay_lib_core::{
    ArtifactFileName, ArtifactSummary, DEFAULT_PAGE_SIZE, NewArtifact, PageOptions, TaskChanges,
    TaskFilters, TaskKeyword, TaskResult, TaskStatus, TaskTitle,
};

use crate::error::{ToolError, invalid_query};

#[derive(Clone, Debug, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub(crate) struct Arguments {
    values: JsonObject,
}

impl Arguments {
    pub fn parse(tool: &Tool, values: Option<JsonObject>) -> Result<Self, ToolError> {
        let values = values.unwrap_or_default();
        let properties = tool.input_schema["properties"]
            .as_object()
            .ok_or_else(ToolError::internal)?;
        for (name, value) in &values {
            let schema = properties.get(name).ok_or_else(|| {
                if tool.name == "list_tasks" {
                    invalid_query("query")
                } else {
                    ToolError::invalid_body()
                }
            })?;
            let valid = match schema["type"].as_str() {
                Some("string") => value.is_string(),
                Some("boolean") => value.is_boolean(),
                Some("integer") => value.as_u64().is_some(),
                _ => false,
            };
            if !valid {
                return Err(match name.as_str() {
                    "page_size" | "cursor" => invalid_query(name),
                    "include_scratchpad" => invalid_query("query"),
                    _ if tool.name == "list_tasks" => invalid_query("query"),
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
        let input = PageOptions {
            page_size: self
                .values
                .get("page_size")
                .and_then(Value::as_u64)
                .unwrap_or(DEFAULT_PAGE_SIZE),
            cursor: self.cursor(),
        };
        input.validate().map_err(|_| invalid_query("page_size"))?;
        Ok(input)
    }

    pub fn scratchpad(&self) -> Result<Option<PageOptions>, ToolError> {
        if self.enabled("include_scratchpad") {
            return self.page().map(Some);
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
        let status = self
            .status()
            .map_err(|message| ToolError::body_field("status", "invalid", message))?;
        let result = self
            .optional_string("result")
            .map(TaskResult::try_from)
            .transpose()
            .map_err(|error| ToolError::task_text("result", error))?;
        if title.is_none() && status.is_none() && result.is_none() {
            return Err(ToolError::body_field(
                "body",
                "required",
                "Provide a title, status or result.",
            ));
        }
        Ok(TaskChanges {
            title,
            status,
            result,
        })
    }

    pub fn artifact(&self) -> Result<NewArtifact, ToolError> {
        let file_name = ArtifactFileName::try_from(self.string("file_name")?)
            .map_err(|error| ToolError::artifact_text("file_name", error))?;
        let summary = ArtifactSummary::try_from(self.string("summary")?)
            .map_err(|error| ToolError::artifact_text("summary", error))?;
        Ok(NewArtifact { file_name, summary })
    }

    pub fn tasks(&self) -> Result<(PageOptions, TaskFilters), ToolError> {
        let status = self
            .status()
            .map_err(|message| ToolError::query_field("status", message))?;
        let keyword = self
            .optional_string("keyword")
            .map(TaskKeyword::try_from)
            .transpose()
            .map_err(ToolError::keyword)?;
        let filters = TaskFilters {
            person: self.optional_string("person"),
            agent: self.optional_string("agent"),
            status,
            own_open: self.enabled("own_open"),
            keyword,
        };
        filters
            .validate()
            .map_err(|error| ToolError::query_field(error.field(), &error.to_string()))?;
        Ok((self.page()?, filters))
    }

    fn enabled(&self, name: &str) -> bool {
        self.values
            .get(name)
            .and_then(Value::as_bool)
            .unwrap_or(false)
    }

    fn status(&self) -> Result<Option<TaskStatus>, &'static str> {
        self.values
            .get("status")
            .and_then(Value::as_str)
            .map(TaskStatus::try_from)
            .transpose()
            .map_err(|_| "Expected in progress or ended.")
    }

    fn optional_string(&self, name: &str) -> Option<String> {
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
