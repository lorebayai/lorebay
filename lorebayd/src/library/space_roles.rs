use futures_util::future::BoxFuture;
use lorebay_lib_core::{Membership, MembershipPage, OperationHandle, SpaceRole, SpaceRoles};

use crate::domain::{OWNER_ID, read_role};
use crate::repository::member::MemberRepository;

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
            let rows = MemberRepository::builder()
                .connection(handle.transaction())
                .build()
                .memberships(request.person_id, request.after_number, request.limit)
                .await?;
            rows.into_iter()
                .map(|row| {
                    Ok(Membership {
                        space_id: row.space_id,
                        space_number: row.space_number,
                        role: SpaceRole::try_from(row.role.as_str())?,
                    })
                })
                .collect()
        })
    }
}
