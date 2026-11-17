use thiserror::Error;

use super::access::{AgentCaller, Caller};

#[derive(Debug, Error)]
pub(crate) enum CallerError {
    #[error("A login session is required")]
    PersonRequired,
}

#[derive(Clone, Debug)]
pub(crate) enum VerifiedCaller {
    Agent(AgentCaller),
    Person(Caller),
}

impl VerifiedCaller {
    pub fn account_id(&self) -> &str {
        match self {
            Self::Agent(caller) => &caller.account_id,
            Self::Person(caller) => &caller.account_id,
        }
    }

    pub fn person(&self) -> Result<&Caller, CallerError> {
        match self {
            Self::Agent(_) => Err(CallerError::PersonRequired),
            Self::Person(caller) => Ok(caller),
        }
    }
}
