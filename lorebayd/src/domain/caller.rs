use lorebay_lib_core::Caller;

use super::OWNER_ID;
use thiserror::Error;

use super::access::{AgentCaller, PersonSession};

#[derive(Debug, Error)]
pub(crate) enum PersonSessionError {
    #[error("A login session is required")]
    PersonRequired,
}

#[derive(Clone, Debug)]
pub(crate) enum VerifiedCaller {
    Agent(AgentCaller),
    Person(PersonSession),
}

impl VerifiedCaller {
    pub fn library_caller(&self) -> Caller {
        let agent_name = match self {
            Self::Agent(caller) => Some(caller.name.clone()),
            Self::Person(_) => None,
        };
        Caller {
            person_id: self.account_id().to_owned(),
            agent_name,
            owner_id: OWNER_ID.to_owned(),
        }
    }

    pub fn account_id(&self) -> &str {
        match self {
            Self::Agent(caller) => &caller.account_id,
            Self::Person(caller) => &caller.account_id,
        }
    }

    pub fn person(&self) -> Result<&PersonSession, PersonSessionError> {
        match self {
            Self::Agent(_) => Err(PersonSessionError::PersonRequired),
            Self::Person(caller) => Ok(caller),
        }
    }
}
