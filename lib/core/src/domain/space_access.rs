use sea_orm::{DatabaseTransaction, DbErr};
use serde::Serialize;
use thiserror::Error;
use typed_builder::TypedBuilder;

use crate::entity::space::Model;
use crate::repository::SpaceRepository;
use crate::{Caller, LibraryState, OperationHandle};

use super::key::parse_key;

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum SpaceAccessError {
    #[error(transparent)]
    Database(#[from] DbErr),
    #[error(transparent)]
    Internal(#[from] anyhow::Error),
    #[error("Space not found")]
    NotFound,
    #[error("The space role does not allow this operation")]
    PermissionRequired,
}

impl SpaceAccessError {
    pub const fn code(&self) -> &'static str {
        match self {
            Self::Database(_) | Self::Internal(_) => "server.internal",
            Self::NotFound => "space.not_found",
            Self::PermissionRequired => "space.permission_required",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SpaceRole {
    Admin,
    Contributor,
    Owner,
    Viewer,
}

#[derive(Debug, Error)]
#[error("Invalid space role")]
pub struct SpaceRoleError;

impl TryFrom<&str> for SpaceRole {
    type Error = SpaceRoleError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "admin" => Ok(Self::Admin),
            "contributor" => Ok(Self::Contributor),
            "owner" => Ok(Self::Owner),
            "viewer" => Ok(Self::Viewer),
            _ => Err(SpaceRoleError),
        }
    }
}

impl SpaceRole {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Admin => "admin",
            Self::Contributor => "contributor",
            Self::Owner => "owner",
            Self::Viewer => "viewer",
        }
    }

    /// Caps a person's role for an agent acting on their behalf.
    pub fn for_agent(self) -> Self {
        match self {
            Self::Admin | Self::Owner => Self::Contributor,
            role => role,
        }
    }

    pub(super) fn for_caller(self, caller: &Caller) -> Self {
        if caller.agent_name.is_some() {
            self.for_agent()
        } else {
            self
        }
    }
}

#[derive(Clone, Debug, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub struct SpaceReference {
    pub id: String,
    pub number: i64,
}

pub async fn resolve_space(
    transaction: &DatabaseTransaction,
    caller: &Caller,
    key: &str,
) -> Result<SpaceReference, SpaceAccessError> {
    let space = scoped_space(transaction, caller, key).await?;
    Ok(SpaceReference {
        id: space.id,
        number: space.number,
    })
}

pub(super) const READ_ROLES: &[SpaceRole] = &[
    SpaceRole::Admin,
    SpaceRole::Contributor,
    SpaceRole::Owner,
    SpaceRole::Viewer,
];
pub(super) const WRITE_ROLES: &[SpaceRole] =
    &[SpaceRole::Admin, SpaceRole::Contributor, SpaceRole::Owner];

#[derive(Clone, Debug, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub(super) struct AuthorizedSpace {
    pub space: Model,
    pub role: SpaceRole,
}

pub(super) async fn scoped_space(
    transaction: &DatabaseTransaction,
    caller: &Caller,
    key: &str,
) -> Result<Model, SpaceAccessError> {
    let number = parse_key("SPACE", key).ok_or(SpaceAccessError::NotFound)?;
    SpaceRepository::builder()
        .connection(transaction)
        .build()
        .by_number(number)
        .await?
        .filter(|space| space.owner_id == caller.owner_id)
        .ok_or(SpaceAccessError::NotFound)
}

pub(super) async fn check_space(
    state: &LibraryState,
    handle: OperationHandle<'_>,
    caller: &Caller,
    key: &str,
    allowed_roles: &[SpaceRole],
) -> Result<AuthorizedSpace, SpaceAccessError> {
    let space = scoped_space(handle.transaction(), caller, key).await?;
    let role = role_in_space(state, handle, caller, &space.id)
        .await?
        .ok_or(SpaceAccessError::NotFound)?;
    if !allowed_roles.contains(&role) {
        return Err(SpaceAccessError::PermissionRequired);
    }
    Ok(AuthorizedSpace { space, role })
}

pub(super) async fn role_in_space(
    state: &LibraryState,
    handle: OperationHandle<'_>,
    caller: &Caller,
    space_id: &str,
) -> anyhow::Result<Option<SpaceRole>> {
    Ok(state
        .roles()
        .role(handle, caller.person_id(), space_id)
        .await?
        .map(|role| role.for_caller(caller)))
}
