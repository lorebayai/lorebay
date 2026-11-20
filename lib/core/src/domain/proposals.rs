use sea_orm::{DatabaseTransaction, IntoActiveModel};
use serde::Serialize;
use typed_builder::TypedBuilder;

use crate::entity::{space, space_proposal as proposal};
use crate::repository::{KnowledgeRepository, ProposalRepository, ProposalRow, SpaceRepository};
use crate::{Caller, LibraryState, OperationHandle, Write, WriteOperation};

use super::id::primary_key;
use super::key::parse_key;
use super::proposal_list::{ProposalListItem, named_item};
use super::space_access::{READ_ROLES, WRITE_ROLES, check_space};
use super::task_access::{TaskAction, check_task};
use super::time::now_millis;
use super::{KnowledgeError, ProposalContent, ProposalError, ProposalSource};

#[derive(Clone, Debug, Serialize, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub struct Proposal {
    #[serde(flatten)]
    pub details: ProposalListItem,
    pub new_content: Option<String>,
    pub from_task_key: Option<String>,
    pub base_version: Option<i64>,
    pub decided_by: Option<String>,
    pub decided_at: Option<i64>,
    pub reject_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub changed_since_proposed: Option<bool>,
}

#[derive(Clone, Debug, Serialize, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub struct SubmittedProposal {
    #[serde(flatten)]
    pub proposal: Proposal,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_title: Option<String>,
}

pub async fn submit_proposal(
    write: &mut Write<'_>,
    caller: &Caller,
    source: ProposalSource<'_>,
    content: ProposalContent,
) -> Result<SubmittedProposal, ProposalError> {
    write.begin_operation(Some(caller))?;
    let (space_id, task) = submission_space(write, caller, source).await?;
    let mut model = proposed_model(write.transaction(), caller, &space_id, content).await?;
    model.from_task_id = task.as_ref().map(|(id, _)| id.clone());
    model.number = SpaceRepository::builder()
        .connection(write.transaction())
        .build()
        .take_number(&space_id, space::Column::NextProposalNumber)
        .await?;
    let number = model.number;
    ProposalRepository::builder()
        .connection(write.transaction())
        .build()
        .insert(model.into_active_model())
        .await?;
    let result = proposal_result(write.state(), write.handle(), &space_id, number).await?;
    write.record(WriteOperation::SubmitProposal, Some(space_id));
    Ok(SubmittedProposal {
        proposal: result,
        task_title: task.map(|(_, title)| title),
    })
}

pub async fn read_proposal(
    state: &LibraryState,
    caller: &Caller,
    space: &str,
    key: &str,
) -> Result<Proposal, ProposalError> {
    state
        .run_read(async |handle| {
            let space = check_space(state, handle, caller, space, READ_ROLES).await?;
            let number = parse_key("PP", key).ok_or(ProposalError::NotFound)?;
            proposal_result(state, handle, &space.space.id, number).await
        })
        .await
}

pub(super) async fn proposal_result(
    state: &LibraryState,
    handle: OperationHandle<'_>,
    space: &str,
    number: i64,
) -> Result<Proposal, ProposalError> {
    let row = ProposalRepository::builder()
        .connection(handle.transaction())
        .build()
        .read(space, number)
        .await?
        .ok_or(ProposalError::NotFound)?;
    let changed = changed_since_proposed(handle.transaction(), &row).await?;
    let names = state
        .names()
        .names(
            handle,
            &[
                Some(row.model.proposed_by.clone()),
                row.model.decided_by.clone(),
            ]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>(),
        )
        .await?;
    let details = named_item(&row, &names)?;
    let decided_by = row
        .model
        .decided_by
        .as_ref()
        .map(|id| {
            names
                .get(id)
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("Person name is missing"))
        })
        .transpose()?;
    Ok(Proposal {
        details,
        new_content: row.model.new_content,
        from_task_key: row
            .task_number
            .map(|number| super::key::format_key("TK", number)),
        base_version: row.model.base_version,
        decided_by,
        decided_at: row.model.decided_at,
        reject_reason: row.model.reject_reason,
        changed_since_proposed: changed,
    })
}

async fn submission_space(
    write: &Write<'_>,
    caller: &Caller,
    source: ProposalSource<'_>,
) -> Result<(String, Option<(String, String)>), ProposalError> {
    match source {
        ProposalSource::Space(key) => {
            let space =
                check_space(write.state(), write.handle(), caller, key, WRITE_ROLES).await?;
            caller.person()?;
            Ok((space.space.id, None))
        }
        ProposalSource::Task(target) => {
            let task = check_task(
                write.state(),
                write.handle(),
                caller,
                target.check(TaskAction::SubmitProposal),
            )
            .await?;
            caller.agent()?;
            Ok((task.space_id, Some((task.id, task.title))))
        }
    }
}

async fn proposed_model(
    transaction: &DatabaseTransaction,
    caller: &Caller,
    space: &str,
    content: ProposalContent,
) -> Result<proposal::Model, ProposalError> {
    let (kind, target, name, summary, content) = match content {
        ProposalContent::Create(input) => (
            "create",
            None,
            Some(input.name),
            Some(input.summary),
            Some(input.content),
        ),
        ProposalContent::Change(target, changes) => (
            "change",
            Some(target),
            changes.name,
            changes.summary,
            changes.content,
        ),
        ProposalContent::Delete(target) => ("delete", Some(target), None, None, None),
    };
    let entry = match target {
        Some(target) => Some(
            KnowledgeRepository::builder()
                .connection(transaction)
                .build()
                .target(space, &target, false)
                .await?
                .ok_or(KnowledgeError::NotFound)?,
        ),
        None => None,
    };
    Ok(proposal::Model {
        id: primary_key(),
        space_id: space.to_owned(),
        number: 0,
        kind: kind.to_owned(),
        knowledge_id: entry.as_ref().map(|entry| entry.id.clone()),
        base_version: entry.map(|entry| entry.latest_version),
        new_name: name,
        new_summary: summary,
        new_content: content,
        proposed_by: caller.person_id.clone(),
        proposer_agent_name: caller.agent_name.clone(),
        from_task_id: None,
        status: "pending".to_owned(),
        decided_by: None,
        decided_at: None,
        reject_reason: None,
        created_at: now_millis()?,
    })
}

async fn changed_since_proposed(
    transaction: &DatabaseTransaction,
    row: &ProposalRow,
) -> Result<Option<bool>, ProposalError> {
    Ok(if row.model.status != "pending" {
        None
    } else if row.model.kind == "create" {
        let name = row
            .model
            .new_name
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Create proposal has no name"))?;
        Some(
            KnowledgeRepository::builder()
                .connection(transaction)
                .build()
                .by_name(&row.model.space_id, &name.to_lowercase())
                .await?
                .is_some(),
        )
    } else {
        Some(
            row.latest_version
                .zip(row.model.base_version)
                .is_some_and(|(latest, base)| latest > base),
        )
    })
}
