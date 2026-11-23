use lorebay_lib_core::{
    KnowledgeChanges, KnowledgeFilters, KnowledgeTarget, NewKnowledgeEntry, PersonRequestContent,
    PersonRequestFilters, PersonRequestStatus,
};

use super::Arguments;
use crate::error::{ToolError, invalid_query};

impl Arguments {
    pub fn knowledge_filters(&self) -> Result<KnowledgeFilters, ToolError> {
        KnowledgeFilters::new(
            self.optional_string("prefix"),
            self.optional_string("keyword"),
        )
        .map_err(Into::into)
    }

    pub fn knowledge_target(&self) -> Result<KnowledgeTarget, ToolError> {
        KnowledgeTarget::new(self.optional_string("name"), self.optional_string("key"))
            .map_err(Into::into)
    }

    pub fn person_request_filters(&self) -> Result<PersonRequestFilters, ToolError> {
        let status = self
            .optional_string("status")
            .as_deref()
            .map(PersonRequestStatus::try_from)
            .transpose()
            .map_err(|_| invalid_query("status"))?;
        PersonRequestFilters::new(status, self.optional_string("keyword")).map_err(Into::into)
    }

    pub fn person_request(&self) -> Result<PersonRequestContent, ToolError> {
        match self.string("action")?.as_str() {
            "create" => {
                self.refuse_fields(&["target_name", "target_key"])?;
                Ok(PersonRequestContent::Create(NewKnowledgeEntry::new(
                    self.string("name")?,
                    self.string("summary")?,
                    self.string("content")?,
                )?))
            }
            "change" => Ok(PersonRequestContent::Change(
                self.person_request_target()?,
                KnowledgeChanges::new(
                    self.optional_string("name"),
                    self.optional_string("summary"),
                    self.optional_string("content"),
                )?,
            )),
            "delete" => {
                self.refuse_fields(&["name", "summary", "content"])?;
                Ok(PersonRequestContent::Delete(self.person_request_target()?))
            }
            _ => Err(ToolError::invalid_body()),
        }
    }

    fn person_request_target(&self) -> Result<KnowledgeTarget, ToolError> {
        KnowledgeTarget::new(
            self.optional_string("target_name"),
            self.optional_string("target_key"),
        )
        .map_err(Into::into)
    }

    fn refuse_fields(&self, names: &[&str]) -> Result<(), ToolError> {
        if names.iter().any(|name| self.values.contains_key(*name)) {
            return Err(ToolError::invalid_body());
        }
        Ok(())
    }
}
