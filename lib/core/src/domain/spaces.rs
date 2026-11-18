use std::collections::HashMap;

use sea_orm::{DatabaseTransaction, DbErr, IntoActiveModel, Set};
use serde::Serialize;
use thiserror::Error;
use typed_builder::TypedBuilder;

use crate::entity::space;
use crate::repository::SpaceRepository;
use crate::{Caller, LibraryState, MembershipPage, Write};

use super::CallerError;
use super::id::primary_key;
use super::key::format_key;
use super::pagination::{Page, PageOptions, decode_cursor, encode_cursor};
use super::space_access::{
    AuthorizedSpace, READ_ROLES, SpaceAccessError, SpaceReference, SpaceRole, check_space,
};
use super::time::now_millis;

#[derive(Debug, Error)]
pub enum SpaceError {
    #[error(transparent)]
    Access(#[from] SpaceAccessError),
    #[error(transparent)]
    Caller(#[from] CallerError),
    #[error(transparent)]
    Database(#[from] DbErr),
    #[error(transparent)]
    Input(#[from] SpaceInputError),
    #[error(transparent)]
    Internal(#[from] anyhow::Error),
    #[error("Invalid cursor")]
    InvalidCursor,
    #[error("Invalid pagination parameter.")]
    InvalidPageSize,
    #[error("Space number is already in use")]
    NumberTaken,
}

impl SpaceError {
    pub const fn code(&self) -> &'static str {
        match self {
            Self::Access(error) => error.code(),
            Self::Caller(error) => error.code(),
            Self::Database(_) | Self::Internal(_) => "server.internal",
            Self::Input(_) => "request.invalid_body",
            Self::InvalidCursor | Self::InvalidPageSize => "request.invalid_query",
            Self::NumberTaken => "space.number_taken",
        }
    }
}

#[derive(Debug, Error)]
pub enum SpaceInputError {
    #[error("Must not be empty.")]
    Empty(&'static str),
    #[error("Provide a name or summary.")]
    NoChanges,
    #[error("Must be at most {1} UTF-8 bytes.")]
    TooLong(&'static str, usize),
}

impl SpaceInputError {
    pub const fn field(&self) -> &'static str {
        match self {
            Self::Empty(field) | Self::TooLong(field, _) => field,
            Self::NoChanges => "body",
        }
    }

    pub const fn field_code(&self) -> &'static str {
        match self {
            Self::Empty(_) | Self::NoChanges => "required",
            Self::TooLong(_, _) => "too_long",
        }
    }
}

#[derive(Clone, Debug, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub struct NewSpace {
    pub name: String,
    pub summary: String,
}

impl NewSpace {
    pub fn validate(&self) -> Result<(), SpaceInputError> {
        validate_field("name", &self.name, 256)?;
        validate_field("summary", &self.summary, 4096)
    }
}

#[derive(Clone, Debug, Serialize, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub struct Space {
    pub key: String,
    pub name: String,
    pub summary: String,
    pub role: SpaceRole,
}

impl From<AuthorizedSpace> for Space {
    fn from(authorized: AuthorizedSpace) -> Self {
        Self {
            key: format_key("SPACE", authorized.space.number),
            name: authorized.space.name,
            summary: authorized.space.summary,
            role: authorized.role,
        }
    }
}

#[derive(Clone, Debug, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub struct SpaceChanges {
    pub name: Option<String>,
    pub summary: Option<String>,
}

impl SpaceChanges {
    pub fn validate(&self) -> Result<(), SpaceInputError> {
        if self.name.is_none() && self.summary.is_none() {
            return Err(SpaceInputError::NoChanges);
        }
        if let Some(name) = &self.name {
            validate_field("name", name, 256)?;
        }
        if let Some(summary) = &self.summary {
            validate_field("summary", summary, 4096)?;
        }
        Ok(())
    }
}

pub async fn highest_space_number(
    transaction: &DatabaseTransaction,
) -> Result<Option<i64>, SpaceError> {
    Ok(SpaceRepository::builder()
        .connection(transaction)
        .build()
        .highest_number()
        .await?)
}

pub async fn create_space(
    write: &mut Write<'_>,
    owner_id: &str,
    number: i64,
    input: NewSpace,
) -> Result<(Space, SpaceReference), SpaceError> {
    input.validate()?;
    let now = now_millis()?;
    let repository = SpaceRepository::builder()
        .connection(write.transaction())
        .build();
    let result = repository
        .insert(space::ActiveModel {
            id: Set(primary_key()),
            number: Set(number),
            owner_id: Set(owner_id.to_owned()),
            name: Set(input.name),
            summary: Set(input.summary),
            next_task_number: Set(1),
            next_knowledge_number: Set(1),
            next_artifact_number: Set(1),
            next_proposal_number: Set(1),
            created_at: Set(now),
            updated_at: Set(now),
        })
        .await;
    let space = match result {
        Err(error)
            if matches!(
                error.sql_err(),
                Some(sea_orm::SqlErr::UniqueConstraintViolation(_))
            ) =>
        {
            return Err(SpaceError::NumberTaken);
        }
        result => result?,
    };
    let reference = SpaceReference {
        id: space.id.clone(),
        number,
    };
    write.space_id = Some(space.id.clone());
    Ok((
        AuthorizedSpace {
            space,
            role: SpaceRole::Owner,
        }
        .into(),
        reference,
    ))
}

pub async fn list_spaces(
    state: &LibraryState,
    caller: &Caller,
    options: PageOptions,
) -> Result<Page<Space>, SpaceError> {
    options
        .validate()
        .map_err(|_| SpaceError::InvalidPageSize)?;
    let after_number = space_position(caller, &options)?;
    state
        .run_read(async |handle| {
            let request = MembershipPage {
                person_id: caller.person_id(),
                owner_id: &caller.owner_id,
                after_number,
                limit: options.page_size + 1,
            };
            let mut memberships = state.roles().memberships(handle, request).await?;
            let has_more = memberships.len() as u64 > options.page_size;
            memberships.truncate(options.page_size as usize);
            let next_cursor = memberships.last().filter(|_| has_more).map(|membership| {
                encode_cursor(
                    CURSOR_VERSION,
                    caller.person_id(),
                    &membership.space_number.to_string(),
                )
            });
            let ids = memberships
                .iter()
                .map(|record| record.space_id.clone())
                .collect();
            let mut spaces: HashMap<String, space::Model> = SpaceRepository::builder()
                .connection(handle.transaction())
                .build()
                .by_ids(ids)
                .await?
                .into_iter()
                .map(|space| (space.id.clone(), space))
                .collect();
            let items = memberships
                .into_iter()
                .filter_map(|membership| {
                    let space = spaces
                        .remove(&membership.space_id)
                        .filter(|space| space.owner_id == caller.owner_id)?;
                    Some(
                        AuthorizedSpace {
                            space,
                            role: membership.role.for_caller(caller),
                        }
                        .into(),
                    )
                })
                .collect();
            Ok(Page { items, next_cursor })
        })
        .await
}

pub async fn read_space(
    state: &LibraryState,
    caller: &Caller,
    key: &str,
) -> Result<Space, SpaceError> {
    state
        .run_read(async |handle| {
            Ok(check_space(state, handle, caller, key, READ_ROLES)
                .await?
                .into())
        })
        .await
}

pub async fn change_space(
    write: &mut Write<'_>,
    caller: &Caller,
    key: &str,
    changes: SpaceChanges,
) -> Result<Space, SpaceError> {
    changes.validate()?;
    caller.person()?;
    let authorized = check_space(write.state(), write.handle(), caller, key, CHANGE_ROLES).await?;
    let mut model = authorized.space.into_active_model();
    if let Some(name) = changes.name {
        model.name = Set(name);
    }
    if let Some(summary) = changes.summary {
        model.summary = Set(summary);
    }
    model.updated_at = Set(now_millis()?);
    let space = SpaceRepository::builder()
        .connection(write.transaction())
        .build()
        .update(model)
        .await?;
    write.space_id = Some(space.id.clone());
    Ok(AuthorizedSpace {
        space,
        role: authorized.role,
    }
    .into())
}

const CURSOR_VERSION: &str = "spaces-v1";
const CHANGE_ROLES: &[SpaceRole] = &[SpaceRole::Owner, SpaceRole::Admin];

fn space_position(caller: &Caller, options: &PageOptions) -> Result<i64, SpaceError> {
    options
        .cursor
        .as_ref()
        .map(|cursor| {
            decode_cursor(CURSOR_VERSION, caller.person_id(), cursor)
                .and_then(|number| number.parse::<i64>().ok())
                .filter(|number| *number > 0)
                .ok_or(SpaceError::InvalidCursor)
        })
        .transpose()
        .map(|number| number.unwrap_or(0))
}

fn validate_field(
    field: &'static str,
    value: &str,
    max_bytes: usize,
) -> Result<(), SpaceInputError> {
    if value.trim().is_empty() {
        return Err(SpaceInputError::Empty(field));
    }
    if value.len() > max_bytes {
        return Err(SpaceInputError::TooLong(field, max_bytes));
    }
    Ok(())
}
