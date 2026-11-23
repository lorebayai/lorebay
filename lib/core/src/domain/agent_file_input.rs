use thiserror::Error;
use typed_builder::TypedBuilder;

pub const AGENT_FILE_NAME_MAX_BYTES: usize = 255;
pub const AGENT_FILE_SUMMARY_MAX_CHARACTERS: usize = 500;

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum AgentFileTextError {
    #[error("Must not be empty.")]
    Empty,
    #[error("Must be at most {0} bytes.")]
    TooManyBytes(usize),
    #[error("Must be at most {0} characters.")]
    TooManyCharacters(usize),
}

#[derive(Clone, Debug)]
pub struct AgentFileName {
    value: String,
}

#[derive(Clone, Debug)]
pub struct AgentFileSummary {
    value: String,
}

#[derive(Clone, Debug, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub struct NewAgentFile {
    pub file_name: AgentFileName,
    pub summary: AgentFileSummary,
}

impl TryFrom<String> for AgentFileName {
    type Error = AgentFileTextError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        if value.trim().is_empty() {
            return Err(AgentFileTextError::Empty);
        }
        if value.len() > AGENT_FILE_NAME_MAX_BYTES {
            return Err(AgentFileTextError::TooManyBytes(AGENT_FILE_NAME_MAX_BYTES));
        }
        Ok(Self { value })
    }
}

impl From<AgentFileName> for String {
    fn from(value: AgentFileName) -> Self {
        value.value
    }
}

impl TryFrom<String> for AgentFileSummary {
    type Error = AgentFileTextError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        if value.trim().is_empty() {
            return Err(AgentFileTextError::Empty);
        }
        if value.chars().count() > AGENT_FILE_SUMMARY_MAX_CHARACTERS {
            return Err(AgentFileTextError::TooManyCharacters(
                AGENT_FILE_SUMMARY_MAX_CHARACTERS,
            ));
        }
        Ok(Self { value })
    }
}

impl From<AgentFileSummary> for String {
    fn from(value: AgentFileSummary) -> Self {
        value.value
    }
}
