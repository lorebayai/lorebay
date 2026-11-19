use thiserror::Error;

pub const KEYWORD_MAX_CHARACTERS: usize = 200;
pub const NOTE_MAX_CHARACTERS: usize = 10_000;
pub const RESULT_MAX_CHARACTERS: usize = 100_000;
pub const TITLE_MAX_CHARACTERS: usize = 200;

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum TaskTextError {
    #[error("Must not be empty.")]
    Empty,
    #[error("Must be at most {0} characters.")]
    TooLong(usize),
}

#[derive(Clone, Debug)]
pub struct TaskTitle {
    value: String,
}

#[derive(Clone, Debug)]
pub struct TaskResult {
    value: String,
}

#[derive(Clone, Debug)]
pub struct NoteContent {
    value: String,
}

#[derive(Clone, Debug)]
pub struct TaskKeyword {
    value: String,
}

impl TryFrom<String> for TaskTitle {
    type Error = TaskTextError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        validate_text(&value, TITLE_MAX_CHARACTERS)?;
        Ok(Self { value })
    }
}

impl From<TaskTitle> for String {
    fn from(title: TaskTitle) -> Self {
        title.value
    }
}

impl TryFrom<String> for TaskResult {
    type Error = TaskTextError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        if value.chars().count() > RESULT_MAX_CHARACTERS {
            return Err(TaskTextError::TooLong(RESULT_MAX_CHARACTERS));
        }
        Ok(Self { value })
    }
}

impl From<TaskResult> for String {
    fn from(result: TaskResult) -> Self {
        result.value
    }
}

impl TryFrom<String> for NoteContent {
    type Error = TaskTextError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        validate_text(&value, NOTE_MAX_CHARACTERS)?;
        Ok(Self { value })
    }
}

impl From<NoteContent> for String {
    fn from(note: NoteContent) -> Self {
        note.value
    }
}

impl TaskKeyword {
    pub fn as_str(&self) -> &str {
        &self.value
    }
}

impl TryFrom<String> for TaskKeyword {
    type Error = TaskTextError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        validate_text(&value, KEYWORD_MAX_CHARACTERS)?;
        Ok(Self { value })
    }
}

fn validate_text(value: &str, maximum: usize) -> Result<(), TaskTextError> {
    if value.trim().is_empty() {
        return Err(TaskTextError::Empty);
    }
    if value.chars().count() > maximum {
        return Err(TaskTextError::TooLong(maximum));
    }
    Ok(())
}
