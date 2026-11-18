use futures_util::future::BoxFuture;
use lorebay_lib_core::{Membership, MembershipPage, OperationHandle, SpaceRole, SpaceRoles};
use sea_orm::{DatabaseTransaction, DbErr};

use crate::repository::participant::ParticipantRepository;

use super::OWNER_ID;

pub(crate) fn parse_role(value: &str) -> Result<SpaceRole, DbErr> {
    match value {
        "admin" => Ok(SpaceRole::Admin),
        "contributor" => Ok(SpaceRole::Contributor),
        "owner" => Ok(SpaceRole::Owner),
        "viewer" => Ok(SpaceRole::Viewer),
        _ => Err(DbErr::Type("Invalid participant role".into())),
    }
}

pub(crate) async fn read_role(
    transaction: &DatabaseTransaction,
    person_id: &str,
    space_id: &str,
) -> Result<Option<SpaceRole>, DbErr> {
    ParticipantRepository::builder()
        .connection(transaction)
        .build()
        .by_space_and_account(space_id, person_id)
        .await?
        .map(|row| parse_role(&row.role))
        .transpose()
}

pub(super) struct Roles;

impl SpaceRoles for Roles {
    fn role<'a>(
        &'a self,
        handle: OperationHandle<'a>,
        person_id: &'a str,
        space_id: &'a str,
    ) -> BoxFuture<'a, anyhow::Result<Option<SpaceRole>>> {
        Box::pin(async move { Ok(read_role(handle.transaction(), person_id, space_id).await?) })
    }

    fn memberships<'a>(
        &'a self,
        handle: OperationHandle<'a>,
        request: MembershipPage<'a>,
    ) -> BoxFuture<'a, anyhow::Result<Vec<Membership>>> {
        Box::pin(async move {
            if request.owner_id != OWNER_ID {
                return Ok(Vec::new());
            }
            let rows = ParticipantRepository::builder()
                .connection(handle.transaction())
                .build()
                .memberships(request.person_id, request.after_number, request.limit)
                .await?;
            rows.into_iter()
                .map(|row| {
                    Ok(Membership {
                        space_id: row.space_id,
                        space_number: row.space_number,
                        role: parse_role(&row.role)?,
                    })
                })
                .collect()
        })
    }
}
