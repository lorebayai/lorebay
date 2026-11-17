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

impl SpaceRole {
    pub fn for_caller(value: &str, caller: &VerifiedCaller) -> Result<Self, DbErr> {
        let role = match value {
            "admin" => Self::Admin,
            "contributor" => Self::Contributor,
            "owner" => Self::Owner,
            "viewer" => Self::Viewer,
            _ => return Err(DbErr::Type("Invalid participant role".into())),
        };
        Ok(match (caller, role) {
            (VerifiedCaller::Agent(_), Self::Admin | Self::Owner) => Self::Contributor,
            _ => role,
        })
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
