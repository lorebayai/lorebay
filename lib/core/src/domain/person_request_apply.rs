use sea_orm::{ActiveModelTrait, DatabaseTransaction, IntoActiveModel, Set, TransactionTrait};

use crate::entity::{space_knowledge as entry, space_person_request as person_request};
use crate::repository::{KnowledgeRepository, PersonRequestRepository, TaskRepository};
use crate::{Caller, Write, WriteOperation};

use super::key::parse_key;
use super::knowledge::{
    append_version, apply_changes, ensure_name_available, insert_entry, next_version,
};
use super::person_requests::person_request_result;
use super::space_access::{READ_ROLES, SpaceRole, check_space};
use super::time::now_millis;
use super::{
    KnowledgeChanges, KnowledgeError, KnowledgeTarget, PersonRequest, PersonRequestError,
    PersonRequestRejection, SpaceAccessError,
};

pub async fn approve_person_request(
    write: &mut Write<'_>,
    caller: &Caller,
    space: &str,
    key: &str,
) -> Result<PersonRequest, PersonRequestError> {
    write.begin_operation(Some(caller))?;
    let (model, role) = decision_person_request(write, caller, space, key).await?;
    if role == SpaceRole::Contributor && model.requested_by == caller.person_id {
        return Err(PersonRequestError::OwnParty);
    }
    let entry = apply_person_request(write.transaction(), caller, &model).await?;
    resume_waiting_task(write.transaction(), &model).await?;
    let number = model.number;
    let space_id = model.space_id.clone();
    let mut model = model.into_active_model();
    model.knowledge_id = Set(Some(entry.id));
    model.status = Set("approved".to_owned());
    model.decided_by = Set(Some(caller.person_id.clone()));
    model.decided_at = Set(Some(now_millis()?));
    PersonRequestRepository::builder()
        .connection(write.transaction())
        .build()
        .update(model)
        .await?;
    let result = person_request_result(write.state(), write.handle(), &space_id, number).await?;
    write.record(WriteOperation::ApprovePersonRequest, Some(space_id));
    Ok(result)
}

pub async fn reject_person_request(
    write: &mut Write<'_>,
    caller: &Caller,
    space: &str,
    key: &str,
    rejection: PersonRequestRejection,
) -> Result<PersonRequest, PersonRequestError> {
    write.begin_operation(Some(caller))?;
    let (model, _) = decision_person_request(write, caller, space, key).await?;
    resume_waiting_task(write.transaction(), &model).await?;
    let number = model.number;
    let space_id = model.space_id.clone();
    let mut model = model.into_active_model();
    model.status = Set("rejected".to_owned());
    model.decided_by = Set(Some(caller.person_id.clone()));
    model.decided_at = Set(Some(now_millis()?));
    model.reject_reason = Set(Some(rejection.reason));
    PersonRequestRepository::builder()
        .connection(write.transaction())
        .build()
        .update(model)
        .await?;
    let result = person_request_result(write.state(), write.handle(), &space_id, number).await?;
    write.record(WriteOperation::RejectPersonRequest, Some(space_id));
    Ok(result)
}

async fn decision_person_request(
    write: &Write<'_>,
    caller: &Caller,
    space: &str,
    key: &str,
) -> Result<(person_request::Model, SpaceRole), PersonRequestError> {
    let space = check_space(write.state(), write.handle(), caller, space, READ_ROLES).await?;
    caller.person()?;
    if space.role == SpaceRole::Viewer {
        return Err(SpaceAccessError::PermissionRequired.into());
    }
    let number = parse_key("RQ", key).ok_or(PersonRequestError::NotFound)?;
    let requests = PersonRequestRepository::builder()
        .connection(write.transaction())
        .build();
    let observed = requests
        .read(&space.space.id, number)
        .await?
        .ok_or(PersonRequestError::NotFound)?;
    if let Some(task_id) = &observed.model.from_task_id {
        TaskRepository::builder()
            .connection(write.transaction())
            .build()
            .lock_by_id(task_id)
            .await?;
    }
    let model = requests
        .locked(&space.space.id, number)
        .await?
        .ok_or(PersonRequestError::NotFound)?;
    if model.status != "pending" {
        return Err(PersonRequestError::Decided);
    }
    Ok((model, space.role))
}

async fn apply_person_request(
    transaction: &DatabaseTransaction,
    caller: &Caller,
    person_request: &person_request::Model,
) -> Result<entry::Model, PersonRequestError> {
    if person_request.action == "create" {
        return apply_create(transaction, caller, person_request).await;
    }
    let id = person_request
        .knowledge_id
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("Request has no target"))?;
    let entry = KnowledgeRepository::builder()
        .connection(transaction)
        .build()
        .lock_by_id(id)
        .await?
        .ok_or(KnowledgeError::NotFound)?;
    if person_request.action == "delete" {
        return apply_delete(transaction, caller, entry, &person_request.id).await;
    }
    if person_request.action != "change" {
        return Err(anyhow::anyhow!("Invalid request action").into());
    }
    if let Some(name) = &person_request.new_name {
        ensure_name_available(transaction, &person_request.space_id, name, Some(&entry.id)).await?;
    }
    let changes = KnowledgeChanges::new(
        person_request.new_name.clone(),
        person_request.new_summary.clone(),
        person_request.new_content.clone(),
    )?;
    Ok(apply_changes(
        transaction,
        caller,
        entry,
        changes,
        Some(&person_request.id),
    )
    .await?)
}

async fn apply_create(
    transaction: &DatabaseTransaction,
    caller: &Caller,
    person_request: &person_request::Model,
) -> Result<entry::Model, PersonRequestError> {
    let name = person_request
        .new_name
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("Create request has no name"))?;
    let target = KnowledgeTarget::new(Some(name.clone()), None)?;
    loop {
        if let Some(entry) = KnowledgeRepository::builder()
            .connection(transaction)
            .build()
            .target(&person_request.space_id, &target, true)
            .await?
        {
            let changes = KnowledgeChanges::new(
                None,
                person_request.new_summary.clone(),
                person_request.new_content.clone(),
            )?;
            return Ok(apply_changes(
                transaction,
                caller,
                entry,
                changes,
                Some(&person_request.id),
            )
            .await?);
        }
        // A name conflict rolls back the counter lock before locking the existing entry.
        let attempt = transaction.begin().await?;
        match insert_entry(&attempt, &person_request.space_id, name).await {
            Ok(entry) => {
                let summary = person_request
                    .new_summary
                    .clone()
                    .ok_or_else(|| anyhow::anyhow!("Create request has no summary"))?;
                let content = person_request
                    .new_content
                    .clone()
                    .ok_or_else(|| anyhow::anyhow!("Create request has no content"))?;
                append_version(
                    &attempt,
                    caller,
                    &entry,
                    Some((summary, content)),
                    Some(&person_request.id),
                )
                .await?;
                attempt.commit().await?;
                return Ok(entry);
            }
            Err(KnowledgeError::NameTaken { .. }) => attempt.rollback().await?,
            Err(error) => return Err(error.into()),
        }
    }
}

async fn apply_delete(
    transaction: &DatabaseTransaction,
    caller: &Caller,
    entry: entry::Model,
    person_request: &str,
) -> Result<entry::Model, PersonRequestError> {
    if entry.deleted_at.is_some() {
        return Ok(entry);
    }
    let now = now_millis()?;
    let version = next_version(entry.latest_version)?;
    let mut model = entry.into_active_model();
    model.latest_version = Set(version);
    model.deleted_at = Set(Some(now));
    model.updated_at = Set(now);
    let entry = KnowledgeRepository::builder()
        .connection(transaction)
        .build()
        .update(model)
        .await?;
    append_version(transaction, caller, &entry, None, Some(person_request)).await?;
    Ok(entry)
}

async fn resume_waiting_task(
    transaction: &DatabaseTransaction,
    request: &person_request::Model,
) -> Result<(), PersonRequestError> {
    let Some(id) = &request.from_task_id else {
        return Ok(());
    };
    let repository = TaskRepository::builder().connection(transaction).build();
    let Some(mut task) = repository.lock_by_id(id).await? else {
        return Ok(());
    };
    if task.state == "waiting" && task.waiting_request_id.as_deref() == Some(request.id.as_str()) {
        task.state = "working".to_owned();
        task.state_reason = None;
        task.waiting_request_id = None;
        repository
            .update(task.into_active_model().reset_all())
            .await?;
    }
    Ok(())
}
