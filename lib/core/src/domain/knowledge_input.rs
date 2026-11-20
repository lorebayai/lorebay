use thiserror::Error;

use super::key::parse_key;

pub const KNOWLEDGE_NAME_MAX_CHARACTERS: usize = 200;
pub const KNOWLEDGE_SUMMARY_MAX_CHARACTERS: usize = 500;
pub const KNOWLEDGE_CONTENT_MAX_BYTES: usize = 1024 * 1024;
/// Maximum JSON string payload for content, name and summary, excluding their enclosing object.
pub const KNOWLEDGE_FIELDS_MAX_JSON_BYTES: usize = KNOWLEDGE_CONTENT_MAX_BYTES * 6
    + (KNOWLEDGE_NAME_MAX_CHARACTERS + KNOWLEDGE_SUMMARY_MAX_CHARACTERS) * 12
    + 6;

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum KnowledgeInputError {
    #[error("Must not be empty.")]
    Empty(&'static str),
    #[error("Provide exactly one of name and key.")]
    InvalidTarget,
    #[error(
        "Expected nonempty path levels without dot levels, control characters or surrounding whitespace."
    )]
    Name,
    #[error("Provide content, summary or name.")]
    NoChanges,
    #[error("Choose an existing content version to roll back to.")]
    RollbackVersion,
    #[error("Must be at most {1} UTF-8 bytes.")]
    TooManyBytes(&'static str, usize),
    #[error("Must be at most {1} characters.")]
    TooManyCharacters(&'static str, usize),
}

impl KnowledgeInputError {
    pub const fn field(&self) -> &'static str {
        match self {
            Self::Empty(field)
            | Self::TooManyBytes(field, _)
            | Self::TooManyCharacters(field, _) => field,
            Self::InvalidTarget => "target",
            Self::Name => "name",
            Self::NoChanges => "body",
            Self::RollbackVersion => "version",
        }
    }
}

#[derive(Clone, Debug)]
pub struct KnowledgeTarget {
    pub(crate) name: Option<String>,
    pub(crate) key: Option<String>,
}

impl KnowledgeTarget {
    pub fn new(name: Option<String>, key: Option<String>) -> Result<Self, KnowledgeInputError> {
        if name.is_some() == key.is_some() {
            return Err(KnowledgeInputError::InvalidTarget);
        }
        if let Some(name) = &name {
            validate_knowledge_name(name)?;
        }
        Ok(Self { name, key })
    }

    pub(crate) fn number(&self) -> Option<i64> {
        self.key.as_deref().and_then(|key| parse_key("KN", key))
    }
}

#[derive(Clone, Debug)]
pub struct NewKnowledgeEntry {
    pub(crate) name: String,
    pub(crate) summary: String,
    pub(crate) content: String,
}

impl NewKnowledgeEntry {
    pub fn new(
        name: String,
        summary: String,
        content: String,
    ) -> Result<Self, KnowledgeInputError> {
        validate_knowledge_name(&name)?;
        validate_summary(&summary)?;
        validate_content(&content)?;
        Ok(Self {
            name,
            summary,
            content,
        })
    }
}

#[derive(Clone, Debug)]
pub struct KnowledgeChanges {
    pub(crate) name: Option<String>,
    pub(crate) summary: Option<String>,
    pub(crate) content: Option<String>,
}

impl KnowledgeChanges {
    pub fn new(
        name: Option<String>,
        summary: Option<String>,
        content: Option<String>,
    ) -> Result<Self, KnowledgeInputError> {
        if name.is_none() && summary.is_none() && content.is_none() {
            return Err(KnowledgeInputError::NoChanges);
        }
        if let Some(name) = &name {
            validate_knowledge_name(name)?;
        }
        if let Some(summary) = &summary {
            validate_summary(summary)?;
        }
        if let Some(content) = &content {
            validate_content(content)?;
        }
        Ok(Self {
            name,
            summary,
            content,
        })
    }
}

pub(super) fn validate_knowledge_name(name: &str) -> Result<(), KnowledgeInputError> {
    validate_characters("name", name, KNOWLEDGE_NAME_MAX_CHARACTERS)?;
    if name.split('/').any(|level| {
        level.is_empty()
            || matches!(level, "." | "..")
            || level.trim() != level
            || level.chars().any(char::is_control)
    }) {
        return Err(KnowledgeInputError::Name);
    }
    Ok(())
}

fn validate_characters(
    field: &'static str,
    value: &str,
    maximum: usize,
) -> Result<(), KnowledgeInputError> {
    if value.is_empty() {
        return Err(KnowledgeInputError::Empty(field));
    }
    if value.chars().count() > maximum {
        return Err(KnowledgeInputError::TooManyCharacters(field, maximum));
    }
    Ok(())
}

fn validate_summary(summary: &str) -> Result<(), KnowledgeInputError> {
    validate_characters("summary", summary, KNOWLEDGE_SUMMARY_MAX_CHARACTERS)
}

fn validate_content(content: &str) -> Result<(), KnowledgeInputError> {
    if content.is_empty() {
        return Err(KnowledgeInputError::Empty("content"));
    }
    if content.len() > KNOWLEDGE_CONTENT_MAX_BYTES {
        return Err(KnowledgeInputError::TooManyBytes(
            "content",
            KNOWLEDGE_CONTENT_MAX_BYTES,
        ));
    }
    Ok(())
}
