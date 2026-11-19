use thiserror::Error;
use typed_builder::TypedBuilder;

pub const ARTIFACT_FILE_NAME_MAX_BYTES: usize = 255;
pub const ARTIFACT_SUMMARY_MAX_CHARACTERS: usize = 500;

#[derive(Debug, Error)]
pub enum ArtifactTextError {
    #[error("Must not be empty.")]
    Empty,
    #[error("Must be at most {0} bytes.")]
    TooManyBytes(usize),
    #[error("Must be at most {0} characters.")]
    TooManyCharacters(usize),
}

#[derive(Clone, Debug)]
pub struct ArtifactFileName {
    value: String,
}

#[derive(Clone, Debug)]
pub struct ArtifactSummary {
    value: String,
}

#[derive(Clone, Debug, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub struct NewArtifact {
    pub file_name: ArtifactFileName,
    pub summary: ArtifactSummary,
}

impl TryFrom<String> for ArtifactFileName {
    type Error = ArtifactTextError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        if value.trim().is_empty() {
            return Err(ArtifactTextError::Empty);
        }
        if value.len() > ARTIFACT_FILE_NAME_MAX_BYTES {
            return Err(ArtifactTextError::TooManyBytes(
                ARTIFACT_FILE_NAME_MAX_BYTES,
            ));
        }
        Ok(Self { value })
    }
}

impl From<ArtifactFileName> for String {
    fn from(value: ArtifactFileName) -> Self {
        value.value
    }
}

impl TryFrom<String> for ArtifactSummary {
    type Error = ArtifactTextError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        if value.trim().is_empty() {
            return Err(ArtifactTextError::Empty);
        }
        if value.chars().count() > ARTIFACT_SUMMARY_MAX_CHARACTERS {
            return Err(ArtifactTextError::TooManyCharacters(
                ARTIFACT_SUMMARY_MAX_CHARACTERS,
            ));
        }
        Ok(Self { value })
    }
}

impl From<ArtifactSummary> for String {
    fn from(value: ArtifactSummary) -> Self {
        value.value
    }
}
