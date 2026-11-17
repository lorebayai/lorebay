use rmcp::model::{JsonObject, Tool};
use serde_json::Value;
use typed_builder::TypedBuilder;

use crate::api::{ApiError, ChangeTaskRequest, ListQuery, ReadTaskQuery, invalid_query};
use crate::domain::TaskChanges;
use crate::domain::pagination::PageOptions;

#[derive(Clone, Debug, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub(crate) struct Arguments {
    values: JsonObject,
}

impl Arguments {
    pub fn parse(tool: &Tool, values: Option<JsonObject>) -> Result<Self, ApiError> {
        let values = values.unwrap_or_default();
        let properties = tool.input_schema["properties"]
            .as_object()
            .ok_or(ApiError::Internal)?;
        for (name, value) in &values {
            let schema = properties.get(name).ok_or_else(ApiError::invalid_body)?;
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
                    _ => ApiError::invalid_body(),
                });
            }
        }
        if let Some(required) = tool.input_schema["required"].as_array()
            && required
                .iter()
                .filter_map(Value::as_str)
                .any(|name| !values.contains_key(name))
        {
            return Err(ApiError::invalid_body());
        }
        Ok(Self { values })
    }

    pub fn string(&self, name: &str) -> Result<String, ApiError> {
        self.values
            .get(name)
            .and_then(Value::as_str)
            .map(str::to_owned)
            .ok_or_else(ApiError::invalid_body)
    }

    pub fn page(&self) -> Result<PageOptions, ApiError> {
        ListQuery::builder()
            .page_size(self.page_size())
            .cursor(self.cursor())
            .build()
            .try_into()
    }

    pub fn scratchpad(&self) -> Result<Option<PageOptions>, ApiError> {
        ReadTaskQuery::builder()
            .include_scratchpad(
                self.values
                    .get("include_scratchpad")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
            )
            .page_size(self.page_size())
            .cursor(self.cursor())
            .build()
            .scratchpad()
    }

    pub fn changes(&self) -> Result<TaskChanges, ApiError> {
        ChangeTaskRequest::builder()
            .title(self.optional_string("title"))
            .status(self.optional_string("status"))
            .result(self.optional_string("result"))
            .build()
            .try_into()
    }

    fn optional_string(&self, name: &str) -> Option<String> {
        self.values
            .get(name)
            .and_then(Value::as_str)
            .map(str::to_owned)
    }

    fn page_size(&self) -> Option<String> {
        self.values.get("page_size").map(Value::to_string)
    }

    fn cursor(&self) -> Option<String> {
        self.values
            .get("cursor")
            .and_then(Value::as_str)
            .map(str::to_owned)
    }
}
