use lorebay_lib_core::{
    LibraryState, NewSpace, Space, SpaceError, SpaceRole, create_space as insert_space,
    highest_space_number,
};
use sea_orm::{DbErr, Set};

use crate::entity::participant;
use crate::library::OWNER_ID;
use crate::repository::participant::ParticipantRepository;

use super::caller::VerifiedCaller;
use super::login_session::now_millis;
use super::primary_key;

pub(crate) async fn create_space(
    state: &LibraryState,
    caller: &VerifiedCaller,
    input: NewSpace,
) -> Result<Space, SpaceError> {
    let caller = caller.library_caller();
    caller.person()?;
    state
        .run_write(Some(&caller), async |write| {
            let number = highest_space_number(write.transaction())
                .await?
                .unwrap_or(0)
                .checked_add(1)
                .ok_or_else(|| DbErr::Type("Space numbers exhausted".into()))?;
            let role = SpaceRole::Owner;
            let (space, reference) = insert_space(write, OWNER_ID, number, role, input).await?;
            let now = now_millis()?;
            ParticipantRepository::builder()
                .connection(write.transaction())
                .build()
                .insert(participant::ActiveModel {
                    id: Set(primary_key()),
                    space_id: Set(reference.id),
                    space_number: Set(reference.number),
                    account_id: Set(caller.person_id.clone()),
                    role: Set(role.as_str().to_owned()),
                    created_at: Set(now),
                    updated_at: Set(now),
                })
                .await?;
            Ok(space)
        })
        .await
}
