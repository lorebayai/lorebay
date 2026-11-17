use sea_orm::{ConnectionTrait, DbErr};
use thiserror::Error;
use typed_builder::TypedBuilder;

use crate::entity::space::Model;
use crate::repository::SpaceRepository;
use crate::repository::participant::ParticipantRepository;

use super::caller::VerifiedCaller;

#[derive(Debug, Error)]
pub(crate) enum SpaceAccessError {
    #[error(transparent)]
    Database(#[from] DbErr),
    #[error("Space not found")]
    NotFound,
    #[error("The space role does not allow this operation")]
    PermissionRequired,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SpaceRole {
    Admin,
    Contributor,
    Owner,
    Viewer,
}

#[derive(Clone, Debug, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub(crate) struct AuthorizedSpace {
    pub space: Model,
    pub role: SpaceRole,
}

impl TryFrom<&str> for SpaceRole {
    type Error = DbErr;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Ok(match value {
            "admin" => Self::Admin,
            "contributor" => Self::Contributor,
            "owner" => Self::Owner,
            "viewer" => Self::Viewer,
            _ => return Err(DbErr::Type("Invalid participant role".into())),
        })
    }
}

impl SpaceRole {
    pub fn for_caller(value: &str, caller: &VerifiedCaller) -> Result<Self, DbErr> {
        let role = Self::try_from(value)?;
        Ok(match caller {
            VerifiedCaller::Agent(_) => role.for_agent(),
            _ => role,
        })
    }

    pub fn for_agent(self) -> Self {
        match self {
            Self::Admin | Self::Owner => Self::Contributor,
            _ => self,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Admin => "admin",
            Self::Contributor => "contributor",
            Self::Owner => "owner",
            Self::Viewer => "viewer",
        }
    }
}

pub(crate) async fn check_space<C: ConnectionTrait>(
    connection: &C,
    caller: &VerifiedCaller,
    key: &str,
    allowed_roles: &[SpaceRole],
) -> Result<AuthorizedSpace, SpaceAccessError> {
    let number = key
        .strip_prefix("SPACE-")
        .and_then(|value| value.parse::<i64>().ok())
        .filter(|number| *number > 0 && key == format!("SPACE-{number}"))
        .ok_or(SpaceAccessError::NotFound)?;
    let space = SpaceRepository::builder()
        .connection(connection)
        .build()
        .by_number(number)
        .await?
        .ok_or(SpaceAccessError::NotFound)?;
    let participant = ParticipantRepository::builder()
        .connection(connection)
        .build()
        .by_space_and_account(&space.id, caller.account_id())
        .await?
        .ok_or(SpaceAccessError::NotFound)?;
    let role = SpaceRole::for_caller(&participant.role, caller)?;
    if !allowed_roles.contains(&role) {
        return Err(SpaceAccessError::PermissionRequired);
    }
    Ok(AuthorizedSpace::builder().space(space).role(role).build())
}
