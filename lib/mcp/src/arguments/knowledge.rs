use lorebay_lib_core::{
    KnowledgeChanges, KnowledgeFilters, KnowledgeTarget, NewKnowledgeEntry, ProposalContent,
    ProposalFilters, ProposalStatus,
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

    pub fn proposal_filters(&self) -> Result<ProposalFilters, ToolError> {
        let status = self
            .optional_string("status")
            .as_deref()
            .map(ProposalStatus::try_from)
            .transpose()
            .map_err(|_| invalid_query("status"))?;
        ProposalFilters::new(status, self.optional_string("keyword")).map_err(Into::into)
    }

    pub fn proposal(&self) -> Result<ProposalContent, ToolError> {
        match self.string("kind")?.as_str() {
            "create" => {
                self.refuse_fields(&["target_name", "target_key"])?;
                Ok(ProposalContent::Create(NewKnowledgeEntry::new(
                    self.string("name")?,
                    self.string("summary")?,
                    self.string("content")?,
                )?))
            }
            "change" => Ok(ProposalContent::Change(
                self.proposal_target()?,
                KnowledgeChanges::new(
                    self.optional_string("name"),
                    self.optional_string("summary"),
                    self.optional_string("content"),
                )?,
            )),
            "delete" => {
                self.refuse_fields(&["name", "summary", "content"])?;
                Ok(ProposalContent::Delete(self.proposal_target()?))
            }
            _ => Err(ToolError::invalid_body()),
        }
    }

    fn proposal_target(&self) -> Result<KnowledgeTarget, ToolError> {
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
