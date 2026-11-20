use sea_orm::{DatabaseTransaction, IntoActiveModel, Set, TransactionTrait};

use crate::entity::{space_knowledge as entry, space_proposal as proposal};
use crate::repository::{KnowledgeRepository, ProposalRepository};
use crate::{Caller, Write, WriteOperation};

use super::key::parse_key;
use super::knowledge::{
    append_version, apply_changes, ensure_name_available, insert_entry, next_version,
};
use super::proposals::proposal_result;
use super::space_access::{READ_ROLES, SpaceRole, check_space};
use super::time::now_millis;
use super::{
    KnowledgeChanges, KnowledgeError, KnowledgeTarget, Proposal, ProposalError, ProposalRejection,
    SpaceAccessError,
};

pub async fn approve_proposal(
    write: &mut Write<'_>,
    caller: &Caller,
    space: &str,
    key: &str,
) -> Result<Proposal, ProposalError> {
    write.begin_operation(Some(caller))?;
    let (model, role) = decision_proposal(write, caller, space, key).await?;
    if role == SpaceRole::Contributor && model.proposed_by == caller.person_id {
        return Err(ProposalError::OwnParty);
    }
    let entry = apply_proposal(write.transaction(), caller, &model).await?;
    let number = model.number;
    let space_id = model.space_id.clone();
    let mut model = model.into_active_model();
    model.knowledge_id = Set(Some(entry.id));
    model.status = Set("approved".to_owned());
    model.decided_by = Set(Some(caller.person_id.clone()));
    model.decided_at = Set(Some(now_millis()?));
    ProposalRepository::builder()
        .connection(write.transaction())
        .build()
        .update(model)
        .await?;
    let result = proposal_result(write.state(), write.handle(), &space_id, number).await?;
    write.record(WriteOperation::ApproveProposal, Some(space_id));
    Ok(result)
}

pub async fn reject_proposal(
    write: &mut Write<'_>,
    caller: &Caller,
    space: &str,
    key: &str,
    rejection: ProposalRejection,
) -> Result<Proposal, ProposalError> {
    write.begin_operation(Some(caller))?;
    let (model, _) = decision_proposal(write, caller, space, key).await?;
    let number = model.number;
    let space_id = model.space_id.clone();
    let mut model = model.into_active_model();
    model.status = Set("rejected".to_owned());
    model.decided_by = Set(Some(caller.person_id.clone()));
    model.decided_at = Set(Some(now_millis()?));
    model.reject_reason = Set(Some(rejection.reason));
    ProposalRepository::builder()
        .connection(write.transaction())
        .build()
        .update(model)
        .await?;
    let result = proposal_result(write.state(), write.handle(), &space_id, number).await?;
    write.record(WriteOperation::RejectProposal, Some(space_id));
    Ok(result)
}

async fn decision_proposal(
    write: &Write<'_>,
    caller: &Caller,
    space: &str,
    key: &str,
) -> Result<(proposal::Model, SpaceRole), ProposalError> {
    let space = check_space(write.state(), write.handle(), caller, space, READ_ROLES).await?;
    caller.person()?;
    if space.role == SpaceRole::Viewer {
        return Err(SpaceAccessError::PermissionRequired.into());
    }
    let number = parse_key("PP", key).ok_or(ProposalError::NotFound)?;
    let model = ProposalRepository::builder()
        .connection(write.transaction())
        .build()
        .locked(&space.space.id, number)
        .await?
        .ok_or(ProposalError::NotFound)?;
    if model.status != "pending" {
        return Err(ProposalError::Decided);
    }
    Ok((model, space.role))
}

async fn apply_proposal(
    transaction: &DatabaseTransaction,
    caller: &Caller,
    proposal: &proposal::Model,
) -> Result<entry::Model, ProposalError> {
    if proposal.kind == "create" {
        return apply_create(transaction, caller, proposal).await;
    }
    let id = proposal
        .knowledge_id
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("Proposal has no target"))?;
    let entry = KnowledgeRepository::builder()
        .connection(transaction)
        .build()
        .lock_by_id(id)
        .await?
        .ok_or(KnowledgeError::NotFound)?;
    if proposal.kind == "delete" {
        return apply_delete(transaction, caller, entry, &proposal.id).await;
    }
    if proposal.kind != "change" {
        return Err(anyhow::anyhow!("Invalid proposal kind").into());
    }
    if let Some(name) = &proposal.new_name {
        ensure_name_available(transaction, &proposal.space_id, name, Some(&entry.id)).await?;
    }
    let changes = KnowledgeChanges::new(
        proposal.new_name.clone(),
        proposal.new_summary.clone(),
        proposal.new_content.clone(),
    )?;
    Ok(apply_changes(transaction, caller, entry, changes, Some(&proposal.id)).await?)
}

async fn apply_create(
    transaction: &DatabaseTransaction,
    caller: &Caller,
    proposal: &proposal::Model,
) -> Result<entry::Model, ProposalError> {
    let name = proposal
        .new_name
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("Create proposal has no name"))?;
    let target = KnowledgeTarget::new(Some(name.clone()), None)?;
    loop {
        if let Some(entry) = KnowledgeRepository::builder()
            .connection(transaction)
            .build()
            .target(&proposal.space_id, &target, true)
            .await?
        {
            let changes = KnowledgeChanges::new(
                None,
                proposal.new_summary.clone(),
                proposal.new_content.clone(),
            )?;
            return Ok(
                apply_changes(transaction, caller, entry, changes, Some(&proposal.id)).await?,
            );
        }
        // A name conflict rolls back the counter lock before locking the existing entry.
        let attempt = transaction.begin().await?;
        match insert_entry(&attempt, &proposal.space_id, name).await {
            Ok(entry) => {
                let summary = proposal
                    .new_summary
                    .clone()
                    .ok_or_else(|| anyhow::anyhow!("Create proposal has no summary"))?;
                let content = proposal
                    .new_content
                    .clone()
                    .ok_or_else(|| anyhow::anyhow!("Create proposal has no content"))?;
                append_version(
                    &attempt,
                    caller,
                    &entry,
                    Some((summary, content)),
                    Some(&proposal.id),
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
    proposal: &str,
) -> Result<entry::Model, ProposalError> {
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
    append_version(transaction, caller, &entry, None, Some(proposal)).await?;
    Ok(entry)
}
