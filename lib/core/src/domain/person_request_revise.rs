use sea_orm::{ActiveModelTrait, IntoActiveModel};

use crate::repository::{KnowledgeRepository, PersonRequestRepository};
use crate::{Caller, Write, WriteOperation};

use super::key::parse_key;
use super::person_requests::person_request_result;
use super::space_access::{WRITE_ROLES, check_space};
use super::time::now_millis;
use super::{KnowledgeChanges, PersonRequest, PersonRequestError, SpaceAccessError};

pub async fn revise_person_request(
    write: &mut Write<'_>,
    caller: &Caller,
    space: &str,
    key: &str,
    changes: Option<KnowledgeChanges>,
) -> Result<PersonRequest, PersonRequestError> {
    write.begin_operation(Some(caller))?;
    let space = check_space(write.state(), write.handle(), caller, space, WRITE_ROLES).await?;
    let number = parse_key("RQ", key).ok_or(PersonRequestError::NotFound)?;
    let requests = PersonRequestRepository::builder()
        .connection(write.transaction())
        .build();
    let mut model = requests
        .locked(&space.space.id, number)
        .await?
        .ok_or(PersonRequestError::NotFound)?;
    if model.requested_by != caller.person_id {
        return Err(SpaceAccessError::PermissionRequired.into());
    }
    if model.status != "pending" {
        return Err(PersonRequestError::Decided);
    }
    if model.action == "delete" && changes.is_some() {
        return Err(super::KnowledgeInputError::NoChanges.into());
    }
    if let Some(id) = &model.knowledge_id {
        let entry = KnowledgeRepository::builder()
            .connection(write.transaction())
            .build()
            .lock_by_id(id)
            .await?
            .ok_or(super::KnowledgeError::NotFound)?;
        model.base_version = Some(entry.latest_version);
        model.base_name = Some(entry.name);
    }
    if let Some(changes) = changes {
        model.new_name = changes.name.or(model.new_name);
        model.new_summary = changes.summary.or(model.new_summary);
        model.new_content = changes.content.or(model.new_content);
    }
    model.revision = model
        .revision
        .checked_add(1)
        .ok_or_else(|| anyhow::anyhow!("Request revision exhausted"))?;
    model.revised_by = Some(caller.person_id.clone());
    model.reviser_agent_name = caller.agent_name.clone();
    model.revised_at = Some(now_millis()?);
    requests
        .update(model.into_active_model().reset_all())
        .await?;
    let result =
        person_request_result(write.state(), write.handle(), &space.space.id, number).await?;
    write.record(WriteOperation::RevisePersonRequest, Some(space.space.id));
    Ok(result)
}
