use lorebay_lib_core::Caller;

use super::TENANT_ID;
use thiserror::Error;

use super::access::{AgentCaller, PersonCaller};

#[derive(Debug, Error)]
pub(crate) enum PersonCallerError {
    #[error("A person's sign-in is required")]
    PersonRequired,
}

#[derive(Clone, Debug)]
pub(crate) enum VerifiedCaller {
    Agent(AgentCaller),
    Person(PersonCaller),
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
            tenant_id: TENANT_ID.to_owned(),
        }
    }

    pub fn account_id(&self) -> &str {
        match self {
            Self::Agent(caller) => &caller.account_id,
            Self::Person(caller) => &caller.account_id,
        }
    }

    pub fn person(&self) -> Result<&PersonCaller, PersonCallerError> {
        match self {
            Self::Agent(_) => Err(PersonCallerError::PersonRequired),
            Self::Person(caller) => Ok(caller),
        }
    }
}
